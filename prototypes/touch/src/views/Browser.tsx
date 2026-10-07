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
  clearVariationPick,
  currentSet,
  editParam,
  overrideOf,
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
import { SourceIcon, tapeFor } from "../ui/marks";

// ── What there is to browse, and what it's for ─────────────────────

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
  /** A heading it sits under (the stack, for patches). */
  group?: string;
}

const MODULE_COLOUR: Record<string, string> = { Core: "#D6B36A", Amp: "#f97316", Drive: "#ef4444", Time: "#8B5CF6", Delay: "#3B82F6", Reverb: "#8B5CF6" };
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
    id: "presets",
    label: "Presets",
    group: "Sounds",
    colour: "#a1a1aa",
    items: (s, bt) => {
      const now = bt?.kind === "part" ? partOf(s, bt.t).part?.sound : undefined;
      return modulesOf("Preset").map((m) => ({
        id: m.name,
        name: m.name,
        from: m.snapshots.length > 1 ? plural(m.snapshots.length, "variation") : undefined,
        colour: "#a1a1aa",
        state: now?.kind === "preset" && now.name === m.name ? ("playing" as const) : undefined,
      }));
    },
    apply: (_, bt) => (bt?.kind === "part" ? (item) => setSectionSound(bt.t.song, bt.t.section, { kind: "preset", name: item.name }, bt.t.part) : null),
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
        return [
          ...(bt?.kind === "part" || bt?.kind === "preset" ? [{ id: "", name: own, from: "no swap", colour: "var(--dim)", state: !swapped ? ("picked" as const) : undefined }] : []),
          ...modulesOf(kind).map((m) => ({
            id: m.name,
            name: m.name,
            from: m.used_by.length ? `in ${plural(m.used_by.length, "preset")}` : undefined,
            colour: MODULE_COLOUR[kind],
            state: swapped === m.name ? ("swapped" as const) : undefined,
          })),
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
          from: b.bypass ? "off" : b.used_by.length ? `in ${plural(b.used_by.length, "preset")}` : undefined,
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
  {
    id: "profiles",
    label: "Profiles",
    group: "Rig",
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
    () => (q ? KINDS.flatMap((k) => k.items(s, bt).filter((i) => i.id && i.name.toLowerCase().includes(q)).map((i) => ({ ...i, kind: k }))) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [q, s, targetKey],
  );
  const showList = wide || opened || !!q;

  return (
    <div ref={ref} style={{ height: "100%", minHeight: 0, display: "flex", flexDirection: "column", background: "#0f0f12" }}>
      {/* What it's for, and the search. */}
      <div style={{ flexShrink: 0, borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, minHeight: 48, padding: "0 4px 0 14px" }}>
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
        <div style={{ padding: "0 12px 10px" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, height: 40, padding: "0 12px", borderRadius: 10, background: "#1a1a1f" }}>
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
                  return <Row key={`${i.kind.id}/${i.id}`} item={i} sub={`${i.kind.label}${i.from ? ` · ${i.from}` : ""}`} onPick={apply ? () => apply(i) : undefined} />;
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
  // Patches sit under their stacks.
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
                <span style={{ width: 9, height: 9, borderRadius: 2, background: tapeFor(head) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(head) }} />
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

/** One thing: its mark, its name, where it comes from, its state. */
function Row({ item, sub, onPick }: { item: Item; sub?: string; onPick?: () => void }) {
  const on = item.state === "playing" || item.state === "picked" || item.state === "swapped" || item.state === "in";
  return (
    <button
      onClick={onPick}
      disabled={!onPick}
      className={onPick && !on ? "pressable" : ""}
      aria-pressed={on}
      style={{
        position: "relative",
        width: "100%",
        minHeight: 56,
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "6px 16px",
        textAlign: "left",
        background: on ? "rgba(255,255,255,0.06)" : undefined,
        cursor: onPick ? "pointer" : "default",
      }}
    >
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 10, bottom: 10, width: 3, borderRadius: 2, background: item.state === "swapped" ? "var(--modified)" : "var(--live)" }} />}
      <span style={{ width: 18, display: "flex", justifyContent: "center", flexShrink: 0 }}>
        {item.mark ?? <span style={{ width: 10, height: 10, borderRadius: 3, background: item.colour === "var(--tape-gaffer)" ? "var(--ink-3)" : item.colour }} />}
      </span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <span style={{ fontSize: 16, fontWeight: on ? 700 : 560, color: "var(--ink)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.name}</span>
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
