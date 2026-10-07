// The Browser: where you go through what the rig has — the song's patches,
// presets, module presets, block presets, profiles. It opens in the middle
// of the main area (Perform), in the left area when the main area is busy
// (Edit), or as a page / drawer on a phone.
//
// It reads like a library, not a form: a list of kinds down the left, the
// kind's things on the right as large rows — a colour mark, the name, where
// it comes from, and its state (playing, swapped in). Narrow (a sidebar, a
// phone), the two become steps: the kinds, then a kind's things, with a
// way back. Search looks through everything.
//
// With a part picked in the setlist the browser is that part's: a row's tap
// makes it what the part plays (a patch), swaps it in for that part alone
// (a module preset), makes it the part's own edit (a block preset), or
// gives the song that profile. With nothing picked it just browses.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { blockPresetsByType, modulesOf, rig, stackOf } from "../data/rig";
import {
  addSong,
  addStackPatch,
  clearOverrides,
  clearVariationPick,
  clearVariationPicks,
  currentSet,
  editParam,
  overrideOf,
  playPreset,
  playing,
  profileOf,
  profileStacksOf,
  removeStackPatch,
  sectionsOf,
  select,
  setModuleOverride,
  setSectionSound,
  setSetProfile,
  setSongProfile,
  setVariationPick,
  songStacks,
  useStore,
  type State,
  type Target,
} from "../store";
import type { StackPatch } from "../setlist/stacks";
import { nameColour, sectionColour, songColour } from "../setlist/colors";
import { ProfileIcon } from "../ui/profileIcons";
import { MODULE_COLOUR } from "../ui/moduleIcons";
import { MACRO_BAR_H } from "../dock/MacroBar";
import { ModuleIcon } from "../ui/moduleIcons";
import nam from "../data/nam.json";
import algos from "../data/algos.json";
import { SourceIcon, tapeFor } from "../ui/marks";

// ── What there is to browse, and what it's for ─────────────────────

/** A NAM model a variation loads — exported from the rig's own config by
 *  scripts/export_nam.py (names only). */
interface NamModel {
  role: "amp" | "drive" | "cab";
  name: string;
  pedal?: string;
  option?: string | null;
}
const NAM = nam as Record<string, NamModel[]>;

/** The algorithm each block runs, exported with the models: a Delay /
 *  Reverb / Time variation -> its blocks' algorithms; a block preset ->
 *  its algorithm (delay style, reverb algorithm, modulation engine). */
interface AlgoLine {
  block: string;
  preset: string;
  algo: string;
}
const ALGOS = algos as unknown as Record<string, AlgoLine[] | string>;

/** What the browser builds into, by the footswitch mode: a section's part
 *  (Setlist — picked in the setlist), the stack open in the Profile view
 *  (Profile), the preset variation playing (Preset). Or nothing — then it
 *  just browses (and, in Setlist mode, adds songs to the set). */
type BuildTarget =
  | { kind: "part"; t: Target }
  | { kind: "stack"; profile: string; stack: string; index: number }
  | { kind: "preset"; preset: string; variation: string }
  | null;

function targetOf(s: State): BuildTarget {
  if (s.performMode === "setlist") {
    const t = s.selection;
    return t && sectionsOf(s, t.song)[t.section] ? { kind: "part", t } : null;
  }
  if (s.performMode === "profile") {
    const profile = profileOf(s, undefined).name;
    const defs = profileStacksOf(s, profile);
    const now = playing(s);
    const index = now ? defs.findIndex((d) => d.patches.includes(now)) : -1;
    return index >= 0 ? { kind: "stack", profile, stack: defs[index].name, index } : null;
  }
  return s.presetUp ? { kind: "preset", preset: s.presetUp.preset, variation: s.presetUp.variation } : null;
}

interface Kind {
  id: string;
  label: string;
  group: string;
  colour: string;
  /** The things of this kind, for the target (or none). */
  items: (s: State, bt: BuildTarget) => Item[];
  /** Apply one to the target — or null when this kind can't build into it. */
  apply: (s: State, bt: BuildTarget) => ((item: Item) => void) | null;
}

interface Item {
  id: string;
  name: string;
  /** Where it comes from, in a few words. */
  from?: string;
  colour: string;
  mark?: ReactNode;
  /** Its state for the target. */
  state?: "playing" | "swapped" | "picked" | "in";
  /** A heading it sits under (the stack, for patches; the preset, for variations). */
  group?: string;
  /** The heading's colour, when not its stack's. */
  groupColour?: string;
  /** A variation, under its preset: drawn as a child of the heading. */
  nested?: boolean;
  /** The NAM models it loads (amps, drives, cabs) — Amp and Drive. */
  models?: NamModel[];
  /** The algorithm each of its blocks runs — Delay, Reverb, Time. */
  algos?: AlgoLine[];
}

const BLOCK_COLOUR: Record<string, string> = { compressor: "#E5E7EB", gate: "#94A3B8", eq: "#22C55E", delay: "#3B82F6", reverb: "#8B5CF6", chorus: "#7DD3FC" };
const cap = (x: string) => x.charAt(0).toUpperCase() + x.slice(1);
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

function partOf(s: State, t: Target | null) {
  const sec = t ? sectionsOf(s, t.song)[t.section] : undefined;
  return { sec, part: sec && t ? sec.parts[t.part] : undefined };
}

/** Every patch the rig knows, once: each profile's, and the songs' own. */
function allPatches(): { name: string; stack: string; from: string }[] {
  const seen = new Map<string, { name: string; stack: string; from: string }>();
  for (const p of rig.library.profiles) for (const x of p.patch_list) if (x.stack && !seen.has(x.name)) seen.set(x.name, { name: x.name, stack: x.stack, from: p.name });
  for (const x of rig.patches) if (!seen.has(x.name)) seen.set(x.name, { name: x.name, stack: stackOf(x.name) ?? "Special", from: `${x.stack}'s own` });
  return [...seen.values()];
}

const KINDS: Kind[] = [
  {
    id: "songs",
    label: "Songs",
    group: "Library",
    colour: "#f472b6",
    items: (s) => {
      const inSet = new Set(currentSet(s).songs.map((x) => x.name));
      return [...rig.library.songs, ...s.newSongs].map((x) => ({
        id: x.name,
        name: x.name,
        from: [x.key, x.bpm ? `${x.bpm} bpm` : "", x.parts.length ? plural(x.parts.length, "section") : ""].filter(Boolean).join(" · "),
        colour: songColour(x.name, s.songColours),
        state: inSet.has(x.name) ? ("in" as const) : undefined,
      }));
    },
    // Building a set: a tap adds the song to the end of it.
    apply: (s) => (s.performMode === "setlist" ? (item) => {
      const song = [...rig.library.songs, ...s.newSongs].find((x) => x.name === item.name);
      if (song) addSong({ name: song.name, key: song.key, bpm: song.bpm, start: "" });
    } : null),
  },
  {
    id: "patches",
    label: "Patches",
    group: "Sounds",
    colour: "#38bdf8",
    items: (s, bt) => {
      // Filling a stack: every patch there is, marked where it's already in.
      if (bt?.kind === "stack") {
        const def = profileStacksOf(s, bt.profile)[bt.index];
        return allPatches()
          .sort((a, b) => Number(b.stack === bt.stack) - Number(a.stack === bt.stack))
          .map((p) => ({
            id: `${p.stack}/${p.name}`,
            name: p.name,
            group: p.stack,
            from: p.from,
            colour: tapeFor(p.stack),
            state: def?.patches.includes(p.name) ? ("in" as const) : undefined,
          }));
      }
      const t = bt?.kind === "part" ? bt.t : null;
      const song = t?.song ?? currentSet(s).songs[s.songIndex]?.name;
      const now = partOf(s, t).part?.sound?.name;
      const colour = song ? songColour(song, s.songColours) : "var(--ink-3)";
      return songStacks(s, song).flatMap((st) =>
        st.patches.map((p: StackPatch) => ({
          id: `${st.name}/${p.name}`,
          name: p.name,
          group: st.name,
          from: p.from === "song" ? `${song}'s own` : p.from === "other" ? `from ${p.profile}` : p.profile,
          colour: tapeFor(st.name),
          mark: <SourceIcon from={p.from} profile={p.profile} colour={p.from === "song" ? colour : p.from === "other" ? nameColour(p.profile!) : "var(--ink-3)"} size={15} />,
          state: now === p.name ? ("playing" as const) : undefined,
        })),
      );
    },
    apply: (s, bt) => {
      if (bt?.kind === "part") return (item) => setSectionSound(bt.t.song, bt.t.section, { kind: "patch", name: item.name }, bt.t.part);
      if (bt?.kind === "stack") {
        const def = profileStacksOf(s, bt.profile)[bt.index];
        // In the stack already: a tap takes it out; else in it goes.
        return (item) => {
          const at = def?.patches.indexOf(item.name) ?? -1;
          if (at >= 0) removeStackPatch(bt.profile, bt.index, at);
          else addStackPatch(bt.profile, bt.index, item.name);
        };
      }
      return null;
    },
  },
  {
    id: "profiles",
    label: "Profiles",
    group: "Sounds",
    colour: "#a78bfa",
    items: (s, bt) => {
      const on = profileOf(s, bt?.kind === "part" ? bt.t.song : undefined).name;
      return rig.library.profiles.map((p) => ({
        id: p.name,
        name: p.name,
        from: p.stacks.filter((st) => p.patch_list.some((x) => x.stack === st)).join(" · "),
        colour: nameColour(p.name),
        mark: <ProfileIcon name={p.name} colour={nameColour(p.name)} size={16} />,
        state: on === p.name ? ("playing" as const) : undefined,
      }));
    },
    apply: (s, bt) => (bt?.kind === "part" ? (item) => setSongProfile(bt.t.song, item.name) : s.performMode !== "preset" ? (item) => setSetProfile(item.name) : null),
  },
  {
    id: "presets",
    label: "Presets",
    group: "Sounds",
    colour: "#a1a1aa",
    // Every preset with all its variations, each under its preset.
    items: (s, bt) => {
      const now = bt?.kind === "part" ? partOf(s, bt.t).part?.sound : undefined;
      return s.presets
        .filter((p) => !p.cancelled)
        .flatMap((p) => {
          const colour = tapeFor(stackOf(p.name)) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(stackOf(p.name));
          return p.variations.map((v) => {
            const name = `${p.name} · ${v}`;
            const playing = (now?.kind === "preset" && now.name === name) || (s.presetUp?.preset === p.name && s.presetUp.variation === v && s.performMode === "preset");
            return { id: name, name: v, group: p.name, groupColour: colour, nested: true, colour, state: playing ? ("playing" as const) : undefined };
          });
        });
    },
    apply: (s, bt) => {
      // A section plays a preset's variation; in Preset mode a tap plays it.
      if (bt?.kind === "part") return (item) => setSectionSound(bt.t.song, bt.t.section, { kind: "preset", name: item.id }, bt.t.part);
      if (s.performMode === "preset") return (item) => playPreset(item.group!, item.name);
      return null;
    },
  },
  ...(["Core", "Amp", "Drive", "Time", "Delay", "Reverb"] as const).map(
    (kind): Kind => ({
      id: `module:${kind}`,
      label: kind,
      group: "Modules",
      colour: MODULE_COLOUR[kind],
      items: (s, bt) => {
        const swapped = bt?.kind === "part" ? overrideOf(s, bt.t).modules[kind] : bt?.kind === "preset" ? s.presetPicks[`${bt.preset}/${bt.variation}/${kind}`] : undefined;
        const own = bt?.kind === "preset" ? "The preset's own" : "The patch's own";
        // Each module preset with all its variations (its snapshots), each
        // under its preset: what's swapped in is a preset's variation.
        return [
          ...(bt?.kind === "part" || bt?.kind === "preset" ? [{ id: "", name: own, from: "no swap", colour: "var(--dim)", state: !swapped ? ("picked" as const) : undefined }] : []),
          ...modulesOf(kind).flatMap((m) =>
            (m.snapshots.length ? m.snapshots : [m.name]).map((v, n) => {
              const id = `${m.name} · ${v}`;
              return {
                id,
                name: v,
                group: m.name,
                groupColour: MODULE_COLOUR[kind],
                nested: true,
                // An amp or drive says what it loads; a delay or reverb what it runs.
                models: kind === "Amp" || kind === "Drive" ? NAM[`${kind}/${m.name}/${v}`] : undefined,
                algos: kind === "Delay" || kind === "Reverb" || kind === "Time" ? (ALGOS[`${kind}/${m.name}/${v}`] as AlgoLine[] | undefined) : undefined,
                from: n === 0 && m.used_by.length ? `in ${plural(m.used_by.length, "preset")}` : undefined,
                colour: MODULE_COLOUR[kind],
                state: swapped === id ? ("swapped" as const) : undefined,
              };
            }),
          ),
        ];
      },
      apply: (_, bt) => {
        if (bt?.kind === "part") return (item) => setModuleOverride(bt.t, kind, item.id || null);
        if (bt?.kind === "preset") return (item) => (item.id ? setVariationPick(bt.preset, bt.variation, kind, item.id) : clearVariationPick(bt.preset, bt.variation, kind));
        return null;
      },
    }),
  ),
  ...[...blockPresetsByType()].map(
    ([type, list]): Kind => ({
      id: `block:${type}`,
      label: type === "eq" ? "EQ" : cap(type),
      group: "Blocks",
      colour: BLOCK_COLOUR[type] ?? "#a1a1aa",
      items: (s, bt) => {
        const pickedPart = bt?.kind === "part" ? overrideOf(s, bt.t).edits[`block:${type}`] : undefined;
        const pickedPreset = bt?.kind === "preset" ? s.presetPicks[`${bt.preset}/${bt.variation}/block:${type}`] : undefined;
        return list.map((b, i) => ({
          id: b.name,
          name: b.name,
          // A delay, reverb or modulation block says its algorithm first.
          from: b.bypass ? "off" : [ALGOS[`block:${type}/${b.name}`] as string | undefined, b.used_by.length ? `in ${plural(b.used_by.length, "preset")}` : ""].filter(Boolean).join(" · ") || undefined,
          colour: BLOCK_COLOUR[type] ?? "#a1a1aa",
          state: pickedPart === i || pickedPreset === b.name ? ("swapped" as const) : undefined,
        }));
      },
      apply: (_, bt) => {
        if (bt?.kind === "part") return (item) => editParam(bt.t, `block:${type}`, list.findIndex((b) => b.name === item.name));
        if (bt?.kind === "preset") return (item) => setVariationPick(bt.preset, bt.variation, `block:${type}`, item.name);
        return null;
      },
    }),
  ),
];

/** Where the browser opens for a mode: songs to build a set, patches to
 *  fill a stack, modules to shape a preset. */
function homeKind(s: State, bt: BuildTarget): string {
  if (bt?.kind === "part" || bt?.kind === "stack") return "patches";
  if (s.performMode === "preset") return "module:Core";
  return s.performMode === "setlist" ? "songs" : "patches";
}

// ── The browser ─────────────────────────────────────────────────────

export function Browser({ onClose }: { onClose?: () => void }) {
  const s = useStore();
  const bt = targetOf(s);
  const t = bt?.kind === "part" ? bt.t : null;
  const { sec, part } = partOf(s, t);
  const [kindId, setKindId] = useState(() => homeKind(s, bt));
  // A new kind of target (a part picked, a stack opened, a preset played)
  // takes the browser to where it builds.
  const targetKey = bt ? `${bt.kind}:${bt.kind === "part" ? `${bt.t.song}|${bt.t.section}|${bt.t.part}` : bt.kind === "stack" ? `${bt.profile}|${bt.stack}` : `${bt.preset}|${bt.variation}`}` : `none:${s.performMode}`;
  useEffect(() => {
    setKindId(homeKind(s, bt));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey.split(":")[0], s.performMode]);
  const [opened, setOpened] = useState(false); // narrow: inside a kind
  const [query, setQuery] = useState("");
  // It filters itself to what's being worked on (a block picked in Edit).
  useEffect(() => {
    if (s.browserFocus && KINDS.some((k) => k.id === s.browserFocus)) {
      setKindId(s.browserFocus);
      setOpened(true);
      setQuery("");
    }
  }, [s.browserFocus]);
  const ref = useRef<HTMLDivElement>(null);
  const [wide, setWide] = useState(true);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWide(el.clientWidth >= 560));
    ro.observe(el);
    setWide(el.clientWidth >= 560);
    return () => ro.disconnect();
  }, []);
  const kind = KINDS.find((k) => k.id === kindId) ?? KINDS[0];
  const q = query.trim().toLowerCase();
  // Searching looks through every kind at once.
  const results = useMemo(
    () => (q ? KINDS.flatMap((k) => k.items(s, bt).filter((i) => i.id && (i.name.toLowerCase().includes(q) || i.group?.toLowerCase().includes(q))).map((i) => ({ ...i, kind: k }))) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [q, s, targetKey],
  );
  const showList = wide || opened || !!q;

  return (
    <div ref={ref} style={{ height: "100%", minHeight: 0, display: "flex", flexDirection: "column", background: "#0f0f12" }}>
      {/* What it's for, and the search. */}
      {/* The macro bar's / set header's height, so the lines run straight across. */}
      <div style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", flexDirection: "column", justifyContent: "center", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 42, padding: "0 4px 0 14px" }}>
          {!wide && opened && !q && (
            <button className="pressable" onClick={() => setOpened(false)} aria-label="All kinds" style={{ width: 36, height: 44, marginLeft: -8, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", borderRadius: "var(--r)" }}>
              <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
                <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          )}
          <span style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "baseline", gap: 7, whiteSpace: "nowrap", overflow: "hidden" }}>
            {t && sec ? (
              <>
                <span style={{ fontSize: 13, color: "var(--ink-3)" }}>For</span>
                <span style={{ fontSize: 15, fontWeight: 650, color: songColour(t.song, s.songColours) }}>{t.song}</span>
                <span style={{ fontSize: 15, fontWeight: 750, color: sectionColour(sec.name) }}>{sec.name}</span>
                {sec.parts.length > 1 && <span style={{ fontSize: 14, fontWeight: 650 }}>{part?.name}</span>}
              </>
            ) : bt?.kind === "stack" ? (
              <>
                <span style={{ fontSize: 13, color: "var(--ink-3)" }}>Filling</span>
                <span style={{ fontSize: 15, fontWeight: 650, color: nameColour(bt.profile) }}>{bt.profile}</span>
                <span style={{ fontSize: 15, fontWeight: 750, color: tapeFor(bt.stack) === "var(--tape-gaffer)" ? "var(--ink)" : tapeFor(bt.stack) }}>{bt.stack}</span>
              </>
            ) : bt?.kind === "preset" ? (
              <>
                <span style={{ fontSize: 13, color: "var(--ink-3)" }}>Shaping</span>
                <span style={{ fontSize: 15, fontWeight: 750 }}>{bt.preset}</span>
                <span style={{ fontSize: 14, fontWeight: 650, color: "var(--ink-2)" }}>{bt.variation}</span>
              </>
            ) : (
              <span style={{ fontSize: 15, fontWeight: 700 }}>{!wide && opened ? kind.label : "Browser"}</span>
            )}
          </span>
          {/* Clear what the target carries of its own, in one go. */}
          <ClearOwn bt={bt} />
          {/* Step the target through the song's sections and parts — set a
              whole song without going back to the setlist. */}
          {t && sec && <StepTarget t={t} />}
          {t && (
            <button className="pressable" onClick={() => select(null)} title="Stop choosing for this part" style={{ height: 44, padding: "0 10px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 650, color: "var(--ink-3)" }}>
              Unpick
            </button>
          )}
          {onClose && (
            <button className="pressable" aria-label="Close the browser" onClick={onClose} style={{ width: 44, height: 44, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)" }}>
              <svg width="13" height="13" viewBox="0 0 12 12" aria-hidden>
                <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
              </svg>
            </button>
          )}
        </div>
        <div style={{ padding: "0 12px 8px" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, height: 36, padding: "0 12px", borderRadius: 10, background: "#1a1a1f" }}>
            <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden style={{ color: "var(--ink-3)", flexShrink: 0 }}>
              <circle cx="7" cy="7" r="4.8" fill="none" stroke="currentColor" strokeWidth="1.6" />
              <path d="M10.6 10.6 14 14" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search everything" aria-label="Search the browser" style={{ flex: 1, minWidth: 0, height: "100%", border: "none", outline: "none", background: "transparent", fontSize: 15 }} />
            {query && (
              <button onClick={() => setQuery("")} style={{ color: "var(--ink-3)", fontSize: 13, fontWeight: 650 }}>
                Clear
              </button>
            )}
          </label>
        </div>
      </div>

      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* The kinds. */}
        {(wide || (!opened && !q)) && (
          <nav aria-label="Kinds" style={{ width: wide ? 188 : "100%", flexShrink: 0, overflowY: "auto", borderRight: wide ? "1px solid var(--rule)" : undefined, padding: "0 0 12px" }}>
            {groupsOf(KINDS).map(([group, kinds]) => (
              <div key={group}>
                <div className="t-label" style={{ padding: "14px 16px 4px", fontSize: 11, color: "var(--ink-3)" }}>
                  {group}
                </div>
                {kinds.map((k) => {
                  const on = wide && k.id === kindId && !q;
                  const items = k.items(s, bt);
                  const count = items.filter((i) => i.id).length;
                  const swapped = items.some((i) => i.state === "swapped");
                  return (
                    <button
                      key={k.id}
                      onClick={() => {
                        setKindId(k.id);
                        setOpened(true);
                        setQuery("");
                      }}
                      className={on ? "" : "pressable"}
                      aria-current={on ? "true" : undefined}
                      style={{ position: "relative", width: "100%", minHeight: 44, display: "flex", alignItems: "center", gap: 10, padding: "0 14px 0 16px", textAlign: "left", background: on ? "rgba(255,255,255,0.07)" : undefined }}
                    >
                      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: 2, background: k.colour }} />}
                      <span style={{ width: 9, height: 9, borderRadius: 3, flexShrink: 0, background: k.colour }} />
                      <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-2)" }}>{k.label}</span>
                      {swapped && <span title="Swapped in for this part" style={{ width: 7, height: 7, borderRadius: 999, background: "var(--modified)" }} />}
                      <span className="num" style={{ fontSize: 12, color: "var(--ink-3)" }}>
                        {count}
                      </span>
                      {!wide && (
                        <svg width="7" height="12" viewBox="0 0 7 12" aria-hidden style={{ color: "var(--ink-3)" }}>
                          <path d="M1 1l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                      )}
                    </button>
                  );
                })}
              </div>
            ))}
          </nav>
        )}

        {/* The things. */}
        {showList && (
          <div style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
            {q ? (
              results.length ? (
                results.map((i) => {
                  const apply = i.kind.apply(s, bt);
                  // A variation found by search says whose it is.
                  const shown = i.group && (i.kind.id === "presets" || i.kind.id.startsWith("module:")) ? { ...i, name: i.id } : i;
                  return <Row key={`${i.kind.id}/${i.id}`} item={shown} sub={`${i.kind.label}${i.from ? ` · ${i.from}` : ""}`} onPick={apply ? () => apply(i) : undefined} />;
                })
              ) : (
                <Quiet>Nothing called “{query}”.</Quiet>
              )
            ) : (
              <KindList kind={kind} items={kind.items(s, bt)} apply={kind.apply(s, bt)} hint={hintFor(s, bt, kind)} />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function groupsOf(kinds: Kind[]): [string, Kind[]][] {
  const out: [string, Kind[]][] = [];
  for (const k of kinds) {
    const last = out[out.length - 1];
    if (last && last[0] === k.group) last[1].push(k);
    else out.push([k.group, [k]]);
  }
  return out;
}

/** What to do to build with this kind, when nothing is being built into. */
function hintFor(s: State, bt: BuildTarget, kind: Kind): string | null {
  if (kind.apply(s, bt)) return null;
  if (s.performMode === "setlist") return "Pick a section's patch in the setlist to choose for it.";
  if (s.performMode === "profile") return kind.id === "patches" ? "Open a stack in the profile to fill it." : "Stacks hold patches — open one, then pick from Patches.";
  return "Play a preset's variation to shape it.";
}

function KindList({ kind, items, apply, hint }: { kind: Kind; items: Item[]; apply: ((item: Item) => void) | null; hint: string | null }) {
  // Profiles open into their stacks and patches.
  if (kind.id === "profiles") return <ProfileColumns />;
  const nested = items.filter((i) => i.nested);
  const plain = items.filter((i) => !i.nested);
  // Presets with variations: a column browser — presets, then the picked
  // preset's variations with what they load.
  if (nested.length)
    return (
      <div style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
        {hint && <Quiet small>{hint}</Quiet>}
        {plain.map((i) => (
          <Row key={i.id || "none"} item={i} sub={i.from} onPick={apply ? () => apply(i) : undefined} />
        ))}
        <PresetColumns key={kind.id} items={nested} onPick={apply ?? undefined} />
      </div>
    );
  let last = "";
  return (
    <div style={{ paddingBottom: 16 }}>
      {hint && <Quiet small>{hint}</Quiet>}
      {items.map((i) => {
        const head = i.group && i.group !== last ? i.group : null;
        if (i.group) last = i.group;
        return (
          <div key={i.id || "none"}>
            {head && (
              <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "14px 16px 6px" }}>
                <span style={{ width: 9, height: 9, borderRadius: 2, background: i.groupColour ?? (tapeFor(head) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(head)) }} />
                <span className="t-label" style={{ fontSize: 11, color: "var(--ink-2)" }}>
                  {head}
                </span>
              </div>
            )}
            <Row item={i} sub={i.from} onPick={apply ? () => apply(i) : undefined} />
          </div>
        );
      })}
      {kind.id === "songs" && apply && <Quiet small>A tap adds the song to the end of the set.</Quiet>}
    </div>
  );
}

/** Presets on the left, the picked preset's variations on the right — each
 *  variation with the NAM models it loads. Narrow, the two become steps. */
function PresetColumns({ items, onPick }: { items: Item[]; onPick?: (item: Item) => void }) {
  const presets: { name: string; colour: string; items: Item[] }[] = [];
  for (const i of items) {
    const p = presets[presets.length - 1];
    if (p && p.name === i.group) p.items.push(i);
    else presets.push({ name: i.group!, colour: i.groupColour ?? i.colour, items: [i] });
  }
  const inUse = presets.find((p) => p.items.some((i) => i.state));
  const [picked, setPicked] = useState<string>(inUse?.name ?? presets[0]?.name);
  const [step, setStep] = useState<"presets" | "variations">("presets");
  const ref = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setNarrow(el.clientWidth < 480));
    ro.observe(el);
    setNarrow(el.clientWidth < 480);
    return () => ro.disconnect();
  }, []);
  const preset = presets.find((p) => p.name === picked) ?? presets[0];
  const showPresets = !narrow || step === "presets";
  const showVariations = !narrow || step === "variations";
  return (
    <div ref={ref} style={{ flex: 1, minHeight: 0, display: "flex", borderTop: "1px solid var(--rule)" }}>
      {showPresets && (
        <div style={{ width: narrow ? "100%" : "38%", maxWidth: narrow ? undefined : 240, flexShrink: 0, overflowY: "auto", borderRight: narrow ? undefined : "1px solid var(--rule)" }}>
          {presets.map((p) => {
            const on = p.name === preset?.name;
            const used = p.items.find((i) => i.state);
            return (
              <button
                key={p.name}
                onClick={() => {
                  setPicked(p.name);
                  setStep("variations");
                }}
                className={on && !narrow ? "" : "pressable"}
                aria-current={on ? "true" : undefined}
                style={{ position: "relative", width: "100%", minHeight: 52, display: "flex", alignItems: "center", gap: 10, padding: "6px 12px 6px 14px", textAlign: "left", background: on && !narrow ? "rgba(255,255,255,0.07)" : undefined }}
              >
                <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: p.colour, opacity: on || used ? 1 : 0.35 }} />
                <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
                  <span style={{ fontSize: 15, fontWeight: on ? 700 : 600, color: "var(--ink)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
                  {used && <span style={{ fontSize: 12, color: used ? (used.state === "swapped" ? "var(--modified)" : "var(--live)") : "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {used ? `${used.state === "swapped" ? "Swapped in" : "Playing"} · ${used.name}` : ""}
                  </span>}
                </span>
                {narrow && (
                  <svg width="7" height="12" viewBox="0 0 7 12" aria-hidden style={{ color: "var(--ink-3)", flexShrink: 0 }}>
                    <path d="M1 1l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                )}
              </button>
            );
          })}
        </div>
      )}
      {showVariations && preset && (
        <div style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10, minHeight: 48, padding: "8px 16px 6px", borderBottom: "1px solid var(--rule)" }}>
            {narrow && (
              <button className="pressable" onClick={() => setStep("presets")} aria-label="All presets" style={{ width: 32, height: 44, marginLeft: -8, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", borderRadius: "var(--r)" }}>
                <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
                  <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>
            )}
            <span style={{ width: 10, height: 10, borderRadius: 3, background: preset.colour, flexShrink: 0 }} />
            <span style={{ flex: 1, minWidth: 0, fontSize: 17, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{preset.name}</span>
          </div>
          {preset.items.map((i) => (
            <Variation key={i.id} item={i} colour={preset.colour} onPick={onPick ? () => onPick(i) : undefined} />
          ))}
        </div>
      )}
    </div>
  );
}

/** One variation: its name, and the NAM models it loads — an amp head for
 *  an amp capture, a bolt for a drive (pedal · setting), a speaker for a
 *  cab IR. */
function Variation({ item, colour, onPick }: { item: Item; colour: string; onPick?: () => void }) {
  const on = !!item.state;
  return (
    <button
      onClick={onPick}
      disabled={!onPick}
      aria-pressed={on}
      className={onPick && !on ? "pressable" : ""}
      style={{ position: "relative", width: "100%", display: "flex", alignItems: "flex-start", gap: 12, minHeight: 52, padding: "10px 16px", textAlign: "left", borderBottom: "1px solid var(--rule)", background: on ? `color-mix(in oklab, ${colour} 12%, transparent)` : undefined, cursor: onPick ? "pointer" : "default" }}
    >
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: item.state === "swapped" ? "var(--modified)" : "var(--live)" }} />}
      <span style={{ width: 16, height: 16, marginTop: 2, borderRadius: 999, flexShrink: 0, boxShadow: `inset 0 0 0 1.5px ${on ? colour : "var(--ink-3)"}`, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {on && <span style={{ width: 8, height: 8, borderRadius: 999, background: colour }} />}
      </span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
        <span style={{ fontSize: 16, fontWeight: on ? 700 : 600, color: "var(--ink)" }}>{item.name}</span>
        {item.algos?.map((a, k) => (
          <span key={`a${k}`} style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0, fontSize: 12.5, color: "var(--ink-3)" }}>
            <ModuleIcon kind={a.block.startsWith("VERB") ? "Reverb" : "Delay"} size={12} colour={a.block.startsWith("VERB") ? "#8B5CF6" : "#3B82F6"} />
            <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              <b style={{ color: "var(--ink-2)", fontWeight: 650 }}>{a.algo}</b> · {a.preset}
            </span>
          </span>
        ))}
        {item.models?.map((m, k) => (
          <span key={k} style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0, fontSize: 12.5, color: "var(--ink-3)" }}>
            <ModuleIcon kind={m.role === "amp" ? "Core" : m.role === "drive" ? "Drive" : "Amp"} size={12} colour={m.role === "amp" ? "#D6B36A" : m.role === "drive" ? "#ef4444" : "#a1a1aa"} />
            <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{m.role === "drive" && m.pedal ? `${m.pedal}${m.option ? ` · ${m.option}` : ""}` : m.name}</span>
          </span>
        ))}
      </span>
      {item.state && (
        <span style={{ flexShrink: 0, fontSize: 12, fontWeight: 700, marginTop: 2, color: item.state === "swapped" ? "var(--modified)" : "var(--live)" }}>
          {item.state === "swapped" ? "Swapped in" : item.state === "playing" ? "Playing" : "In use"}
        </span>
      )}
    </button>
  );
}

/** One thing: its mark, its name, where it comes from, its state. */
function Row({ item, sub, onPick }: { item: Item; sub?: string; onPick?: () => void }) {
  const on = item.state === "playing" || item.state === "picked" || item.state === "swapped" || item.state === "in";
  // A variation: indented on its preset's guide line, a lighter row.
  const nested = !!item.nested;
  return (
    <button
      onClick={onPick}
      disabled={!onPick}
      className={onPick && !on ? "pressable" : ""}
      aria-pressed={on}
      style={{
        position: "relative",
        width: "100%",
        minHeight: nested ? 46 : 56,
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: nested ? "4px 16px 4px 38px" : "6px 16px",
        textAlign: "left",
        background: on ? "rgba(255,255,255,0.06)" : undefined,
        cursor: onPick ? "pointer" : "default",
      }}
    >
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 10, bottom: 10, width: 3, borderRadius: 2, background: item.state === "swapped" ? "var(--modified)" : "var(--live)" }} />}
      {nested && <span aria-hidden style={{ position: "absolute", left: 21, top: 0, bottom: 0, width: 1.5, background: `color-mix(in oklab, ${item.colour} 45%, transparent)` }} />}
      <span style={{ width: nested ? 10 : 18, display: "flex", justifyContent: "center", flexShrink: 0 }}>
        {nested ? (
          <span style={{ width: 8, height: 8, borderRadius: 999, background: on ? item.colour : "transparent", boxShadow: on ? undefined : `inset 0 0 0 1.5px ${item.colour}` }} />
        ) : (
          item.mark ?? <span style={{ width: 10, height: 10, borderRadius: 3, background: item.colour === "var(--tape-gaffer)" ? "var(--ink-3)" : item.colour }} />
        )}
      </span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <span style={{ fontSize: nested ? 15 : 16, fontWeight: on ? 700 : nested ? 520 : 560, color: nested && !on ? "var(--ink-2)" : "var(--ink)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.name}</span>
        {sub && <span style={{ fontSize: 13, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{sub}</span>}
      </span>
      {item.state && (
        <span style={{ flexShrink: 0, fontSize: 12, fontWeight: 700, color: item.state === "swapped" ? "var(--modified)" : "var(--live)" }}>
          {item.state === "playing" ? "Playing" : item.state === "swapped" ? "Swapped in" : item.state === "in" ? "In" : "In use"}
        </span>
      )}
    </button>
  );
}

function Quiet({ children, small }: { children: ReactNode; small?: boolean }) {
  return <div style={{ padding: small ? "12px 16px 4px" : "32px 16px", fontSize: 13.5, color: "var(--ink-3)", lineHeight: 1.4, textAlign: small ? "left" : "center" }}>{children}</div>;
}

/** ‹ › through a song's parts, section by section. */
function StepTarget({ t }: { t: Target }) {
  const s = useStore();
  const steps = sectionsOf(s, t.song).flatMap((sec, i) => sec.parts.map((_, k) => ({ section: i, part: k })));
  const at = steps.findIndex((x) => x.section === t.section && x.part === t.part);
  const go = (d: number) => {
    const x = steps[at + d];
    if (x) select({ song: t.song, section: x.section, part: x.part });
  };
  const btn = (d: number, label: string) => (
    <button
      className={steps[at + d] ? "pressable" : ""}
      disabled={!steps[at + d]}
      onClick={() => go(d)}
      aria-label={label}
      style={{ width: 40, height: 44, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: steps[at + d] ? "var(--ink-2)" : "var(--dim)" }}
    >
      <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden style={{ transform: d > 0 ? "scaleX(-1)" : undefined }}>
        <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
  return (
    <span style={{ display: "flex", alignItems: "center" }}>
      {btn(-1, "Previous section")}
      <span className="num" style={{ fontSize: 12, color: "var(--ink-3)", minWidth: 34, textAlign: "center" }}>
        {at + 1}/{steps.length}
      </span>
      {btn(1, "Next section")}
    </span>
  );
}

/** "Clear n": every module swapped in (and, for a part, Edit's unsaved
 *  changes) dropped at once — back to the preset as it is. Shown only when
 *  there is something to clear. */
function ClearOwn({ bt }: { bt: BuildTarget }) {
  const s = useStore();
  let n = 0;
  let clear: (() => void) | null = null;
  if (bt?.kind === "part") {
    const o = overrideOf(s, bt.t);
    n = Object.keys(o.modules).length + Object.keys(o.edits).length;
    clear = () => clearOverrides(bt.t);
  } else if (bt?.kind === "preset") {
    n = Object.keys(s.presetPicks).filter((k) => k.startsWith(`${bt.preset}/${bt.variation}/`)).length;
    clear = () => clearVariationPicks(bt.preset, bt.variation);
  }
  if (!n || !clear) return null;
  return (
    <button
      className="pressable"
      onClick={clear}
      title={bt?.kind === "part" ? "Drop every module swapped in and every unsaved change — back to its preset" : "Drop every module swapped into this variation"}
      style={{ height: 32, padding: "0 10px", borderRadius: "var(--r)", display: "flex", alignItems: "center", gap: 6, fontSize: 12.5, fontWeight: 700, whiteSpace: "nowrap", color: "var(--modified)", boxShadow: "inset 0 0 0 1px color-mix(in oklab, var(--modified) 45%, transparent)" }}
    >
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
        <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
      Clear {n} override{n === 1 ? "" : "s"}
    </button>
  );
}

/** Profiles on the left; the picked one's stacks and their patches on the
 *  right. With a part picked, a stack row makes the part play that stack
 *  and a patch row that patch — borrowed when the profile isn't the
 *  song's; a button gives the whole song the profile. Filling a stack
 *  (Profile mode), a patch row adds the patch to it or takes it out. */
function ProfileColumns() {
  const s = useStore();
  const bt = targetOf(s);
  const t = bt?.kind === "part" ? bt.t : null;
  const songProfile = profileOf(s, t?.song ?? currentSet(s).songs[s.songIndex]?.name).name;
  const [picked, setPicked] = useState(songProfile);
  const [step, setStep] = useState<"profiles" | "stacks">("profiles");
  const ref = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setNarrow(el.clientWidth < 480));
    ro.observe(el);
    setNarrow(el.clientWidth < 480);
    return () => ro.disconnect();
  }, []);
  const part = t ? partOf(s, t).part : undefined;
  const sound = part?.sound;
  const defs = profileStacksOf(s, picked);
  const borrowed = picked !== songProfile ? picked : undefined;
  const fill = bt?.kind === "stack" ? profileStacksOf(s, bt.profile)[bt.index] : null;
  return (
    <div ref={ref} style={{ height: "100%", display: "flex", minHeight: 0 }}>
      {(!narrow || step === "profiles") && (
        <div style={{ width: narrow ? "100%" : "38%", maxWidth: narrow ? undefined : 240, flexShrink: 0, overflowY: "auto", borderRight: narrow ? undefined : "1px solid var(--rule)" }}>
          {rig.library.profiles.map((p) => {
            const on = p.name === picked;
            return (
              <button
                key={p.name}
                onClick={() => {
                  setPicked(p.name);
                  setStep("stacks");
                }}
                className={on && !narrow ? "" : "pressable"}
                style={{ position: "relative", width: "100%", minHeight: 52, display: "flex", alignItems: "center", gap: 10, padding: "6px 12px 6px 14px", textAlign: "left", background: on && !narrow ? "rgba(255,255,255,0.07)" : undefined }}
              >
                {on && !narrow && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: nameColour(p.name) }} />}
                <ProfileIcon name={p.name} colour={nameColour(p.name)} size={16} />
                <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: on ? 700 : 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
                {p.name === songProfile && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)" }}>{t ? "The song's" : "Playing"}</span>}
              </button>
            );
          })}
        </div>
      )}
      {(!narrow || step === "stacks") && (
        <div style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10, minHeight: 52, padding: "8px 12px 8px 16px", borderBottom: "1px solid var(--rule)" }}>
            {narrow && (
              <button className="pressable" onClick={() => setStep("profiles")} aria-label="All profiles" style={{ width: 32, height: 44, marginLeft: -8, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", borderRadius: "var(--r)" }}>
                <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
                  <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>
            )}
            <ProfileIcon name={picked} colour={nameColour(picked)} size={18} />
            <span style={{ flex: 1, minWidth: 0, fontSize: 17, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{picked}</span>
            {t && picked !== songProfile && (
              <button className="pressable" onClick={() => setSongProfile(t.song, picked)} style={{ height: 36, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, boxShadow: "inset 0 0 0 1px var(--rule-strong)", whiteSpace: "nowrap" }}>
                Play {t.song} on it
              </button>
            )}
          </div>
          {defs.map((d) => {
            const tape = tapeFor(d.name) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(d.name);
            const stackOn = sound?.kind === "stack" && sound.name === d.name && (sound.profile ?? songProfile) === picked;
            return (
              <section key={d.name} style={{ borderBottom: "1px solid var(--rule)" }}>
                {/* The stack itself: a part can play the whole stack. */}
                <button
                  onClick={t ? () => setSectionSound(t.song, t.section, { kind: "stack", name: d.name, ...(borrowed ? { profile: borrowed } : {}) }, t.part) : undefined}
                  disabled={!t}
                  className={t && !stackOn ? "pressable" : ""}
                  style={{ position: "relative", width: "100%", minHeight: 48, display: "flex", alignItems: "center", gap: 10, padding: "6px 16px", textAlign: "left", background: stackOn ? `color-mix(in oklab, ${tape} 14%, transparent)` : undefined, cursor: t ? "pointer" : "default" }}
                >
                  {stackOn && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: "var(--live)" }} />}
                  <span style={{ width: 12, height: 12, borderRadius: 3, background: tape, flexShrink: 0 }} />
                  <span style={{ flex: 1, minWidth: 0, fontSize: 16, fontWeight: 750 }}>{d.name}</span>
                  {t && <span style={{ fontSize: 12, fontWeight: 700, color: stackOn ? "var(--live)" : "var(--ink-3)" }}>{stackOn ? "Plays the stack" : "Whole stack"}</span>}
                </button>
                {/* Its patches. */}
                {d.patches.map((name) => {
                  const on = (sound?.kind === "patch" && sound.name === name) || !!fill?.patches.includes(name);
                  const pick = t
                    ? () => setSectionSound(t.song, t.section, { kind: "patch", name, ...(borrowed ? { profile: borrowed } : {}) }, t.part)
                    : bt?.kind === "stack"
                      ? () => {
                          const at = fill?.patches.indexOf(name) ?? -1;
                          if (at >= 0) removeStackPatch(bt.profile, bt.index, at);
                          else addStackPatch(bt.profile, bt.index, name);
                        }
                      : undefined;
                  return (
                    <button
                      key={name}
                      onClick={pick}
                      disabled={!pick}
                      aria-pressed={on}
                      className={pick && !on ? "pressable" : ""}
                      style={{ position: "relative", width: "100%", minHeight: 44, display: "flex", alignItems: "center", gap: 10, padding: "4px 16px 4px 38px", textAlign: "left", background: on ? "rgba(255,255,255,0.06)" : undefined, cursor: pick ? "pointer" : "default" }}
                    >
                      <span aria-hidden style={{ position: "absolute", left: 21, top: 0, bottom: 0, width: 1.5, background: `color-mix(in oklab, ${tape} 45%, transparent)` }} />
                      <span style={{ width: 8, height: 8, borderRadius: 999, flexShrink: 0, background: on ? tape : "transparent", boxShadow: on ? undefined : `inset 0 0 0 1.5px ${tape}` }} />
                      <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: on ? 700 : 540, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{name}</span>
                      {on && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)" }}>{fill ? "In" : "Playing"}</span>}
                    </button>
                  );
                })}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}
