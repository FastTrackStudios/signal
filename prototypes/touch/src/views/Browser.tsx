// The Browser: where you go through what the rig has — the song's patches,
// presets, module presets, block presets, profiles. It opens in the middle
// of the main area (Perform), in the left area when the main area is busy
// (Edit), or as a page / drawer on a phone.
//
// It reads like a library, not a form: a list of kinds down the left, the
// kind's things on the right as large rows — a colour mark, the name, where
// it comes from, and its state (playing, an override). Narrow (a sidebar, a
// phone), the two become steps: the kinds, then a kind's things, with a
// way back. Search looks through everything.
//
// With a part picked in the setlist the browser is that part's: a row's tap
// makes it what the part plays (a patch), swaps it in for that part alone
// (a module preset), makes it the part's own edit (a block preset), or
// gives the song that profile. With nothing picked it just browses.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { blockPresetsByType, chain, modulesOf, rig, stackOf } from "../data/rig";
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
  GENRES,
  newCollection,
  removeCollection,
  renameCollection,
  setSongInfo,
  songInfoOf,
  toggleInCollection,
  type State,
  type Target,
} from "../store";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import type { StackPatch } from "../setlist/stacks";
import { nameColour, sectionColour, songColour } from "../setlist/colors";
import { ProfileIcon } from "../ui/profileIcons";
import { MODULE_COLOUR } from "../ui/moduleIcons";
import { MACRO_BAR_H } from "../dock/MacroBar";
import { ModuleIcon } from "../ui/moduleIcons";
import { OverrideIcon } from "../ui/OverrideIcon";
import { InheritIcon } from "../ui/InheritIcon";
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
  /** Chosen by a preset higher up (the patch, the Core…): who chose it. */
  inherited?: string;
  /** More it can be found by in a search (a song's artist, genre…). */
  search?: string;
}

const BLOCK_COLOUR: Record<string, string> = { compressor: "#E5E7EB", gate: "#94A3B8", eq: "#22C55E", delay: "#3B82F6", reverb: "#8B5CF6", chorus: "#7DD3FC" };
const cap = (x: string) => x.charAt(0).toUpperCase() + x.slice(1);
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

function partOf(s: State, t: Target | null) {
  const sec = t ? sectionsOf(s, t.song)[t.section] : undefined;
  return { sec, part: sec && t ? sec.parts[t.part] : undefined };
}

/** What a part (or a preset variation) inherits for each module kind: the
 *  patch is a preset that picks its Core, Amp and Time; the Core picks its
 *  Amp and Drive; the Time its Delay and Reverb. Followed down, first
 *  choice wins, each tagged with who chose it. */
function inheritedPicks(s: State, bt: BuildTarget): Record<string, { preset: string; variation: string; from: string }> {
  return inherited(s, bt).modules;
}

/** The block a chain block name is (its type, lower-case), from the rig's
 *  chain: "Pre Comp" → compressor, "Amp EQ" → eq, "DLY 1" → delay. */
function blockTypeOf(block: string): string | undefined {
  return chain().find((b) => b.node.name === block)?.node.block_type?.toLowerCase() ?? undefined;
}

/** What a part inherits: module variations, and block presets — each
 *  picked by a preset higher up (the patch, its Core, its Time…), with the
 *  block it sits in. */
function inherited(s: State, bt: BuildTarget): {
  modules: Record<string, { preset: string; variation: string; from: string }>;
  blocks: Record<string, { preset: string; block: string; from: string }[]>;
} {
  let preset: string | undefined;
  let variation: string | undefined;
  if (bt?.kind === "part") {
    const sound = partOf(s, bt.t).part?.sound;
    if (sound?.kind === "preset") [preset, variation] = sound.name.split(" · ");
    else if (sound?.kind === "stack") preset = profileStacksOf(s, sound.profile ?? profileOf(s, bt.t.song).name).find((d) => d.name === sound.name)?.patches[0];
    else preset = sound?.name;
  } else if (bt?.kind === "preset") {
    preset = bt.preset;
    variation = bt.variation;
  }
  const out: Record<string, { preset: string; variation: string; from: string }> = {};
  const blocks: Record<string, { preset: string; block: string; from: string }[]> = {};
  const addBlocks = (list: { block: string; preset: string }[], from: string) => {
    for (const b of list) {
      const type = blockTypeOf(b.block);
      if (!type) continue;
      const have = (blocks[type] ??= []);
      if (!have.some((x) => x.block === b.block)) have.push({ preset: b.preset, block: b.block, from });
    }
  };
  const root = preset ? modulesOf("Preset").find((m) => m.name === preset) : undefined;
  if (!root) return { modules: out, blocks };
  const at = Math.max(0, variation ? root.snapshots.indexOf(variation) : 0);
  addBlocks(root.snapshot_info[at]?.blocks ?? [], root.name);
  const queue = (root.snapshot_info[at]?.modules ?? []).map((m) => ({ ...m, from: root.name }));
  while (queue.length) {
    const pick = queue.shift()!;
    if (out[pick.module]) continue;
    out[pick.module] = { preset: pick.preset, variation: pick.snapshot, from: pick.from };
    const comp = modulesOf(pick.module).find((m) => m.name === pick.preset);
    const k = comp ? comp.snapshots.indexOf(pick.snapshot) : -1;
    if (comp && k >= 0) {
      // The module's own block picks (Core: its gate, its comps, its EQ).
      addBlocks(comp.snapshot_info[k]?.blocks ?? [], pick.module);
      for (const sub of comp.snapshot_info[k]?.modules ?? []) queue.push({ ...sub, from: pick.module });
    }
  }
  return { modules: out, blocks };
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
        from: [songInfoOf(s, x.name).artist, x.key, x.bpm ? `${x.bpm} bpm` : ""].filter(Boolean).join(" · "),
        search: [songInfoOf(s, x.name).artist, songInfoOf(s, x.name).genre, ...s.collections.filter((c) => c.songs.includes(x.name)).map((c) => c.name)].join(" ").toLowerCase(),
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
        // What a preset higher up chose here — shown even with no override,
        // so clearing one shows what comes back.
        const from = inheritedPicks(s, bt)[kind];
        // Each module preset with all its variations (its snapshots), each
        // under its preset: an override is a preset's variation.
        return [
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
                inherited: from && from.preset === m.name && from.variation === v ? from.from : undefined,
              };
            }),
          ),
        ];
      },
      apply: (_, bt) => {
        // Picking what's inherited anyway clears the override instead.
        if (bt?.kind === "part") return (item) => setModuleOverride(bt.t, kind, item.inherited ? null : item.id || null);
        if (bt?.kind === "preset") return (item) => (item.id && !item.inherited ? setVariationPick(bt.preset, bt.variation, kind, item.id) : clearVariationPick(bt.preset, bt.variation, kind));
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
        // What a preset higher up picked for this kind of block (each block
        // of it: a Pre Comp and a Post Comp are both compressors).
        const from = inherited(s, bt).blocks[type] ?? [];
        const pickedPart = bt?.kind === "part" ? overrideOf(s, bt.t).edits[`block:${type}`] : undefined;
        const pickedPreset = bt?.kind === "preset" ? s.presetPicks[`${bt.preset}/${bt.variation}/block:${type}`] : undefined;
        return list.map((b, i) => ({
          id: b.name,
          name: b.name,
          // A delay, reverb or modulation block says its algorithm first.
          from: b.bypass ? "off" : [ALGOS[`block:${type}/${b.name}`] as string | undefined, b.used_by.length ? `in ${plural(b.used_by.length, "preset")}` : ""].filter(Boolean).join(" · ") || undefined,
          colour: BLOCK_COLOUR[type] ?? "#a1a1aa",
          state: pickedPart === i || pickedPreset === b.name ? ("swapped" as const) : undefined,
          inherited: from.filter((f) => f.preset === b.name).map((f) => `${f.from} · ${f.block}`).join(", ") || undefined,
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
    () => (q ? KINDS.flatMap((k) => k.items(s, bt).filter((i) => i.id && (i.name.toLowerCase().includes(q) || i.group?.toLowerCase().includes(q) || i.search?.includes(q))).map((i) => ({ ...i, kind: k }))) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [q, s, targetKey],
  );
  const showList = wide || opened || !!q;
  const listRef = useScrollToCurrent(`${kind.id}|${targetKey}|${q ? "q" : ""}|${opened}`);

  return (
    <div ref={ref} style={{ height: "100%", minHeight: 0, display: "flex", flexDirection: "column", background: "#0f0f12" }}>
      {/* What it's for, and the search. */}
      {/* The macro bar's / set header's height, so the lines run straight across. */}
      <div style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", flexDirection: "column", justifyContent: "center", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 42, padding: "0 4px 0 14px" }}>
          {!wide && opened && !q && (
            <button className="pressable" onClick={() => setOpened(false)} aria-label="All kinds" style={{ width: 44, height: 44, marginLeft: -12, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", borderRadius: "var(--r)" }}>
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
                <span style={{ fontSize: 15, fontWeight: 750, color: tapeFor(bt.stack) === "var(--tape-gaffer)" ? "var(--ink)" : `color-mix(in oklab, ${tapeFor(bt.stack)} 78%, white)` }}>{bt.stack}</span>
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
                  // What the picked part (or stack, or variation) has here now.
                  const cur = items.find((i) => i.state && i.state !== "picked" && i.state !== "in") ?? items.find((i) => i.inherited);
                  const swapped = cur?.state === "swapped";
                  const inheritedOnly = !!cur && !cur.state && !!cur.inherited;
                  const label = cur ? (cur.nested ? cur.id : cur.name) : null;
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
                      title={label ? `${swapped ? "Override" : "Now"}: ${label}` : k.label}
                      style={{
                        position: "relative",
                        width: "100%",
                        minHeight: label ? 52 : 44,
                        display: "flex",
                        alignItems: "center",
                        gap: 10,
                        padding: "4px 14px 4px 16px",
                        marginBottom: 1,
                        textAlign: "left",
                        // The kind's colour, muted, behind the whole row — stronger when open.
                        background: `color-mix(in oklab, ${k.colour} ${on ? 30 : 11}%, #0f0f12)`,
                      }}
                    >
                      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: k.colour }} />}
                      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
                        <span style={{ fontSize: 15, fontWeight: on ? 700 : 600, color: on ? "var(--ink)" : "var(--ink-2)" }}>{k.label}</span>
                        {label && (
                          <span style={{ display: "flex", alignItems: "center", gap: 5, minWidth: 0, fontSize: 12, fontWeight: 600, color: swapped ? `color-mix(in oklab, ${k.colour} 45%, var(--ink-3))` : "var(--ink-3)" }}>
                            {swapped && <OverrideMark colour={k.colour} />}
                            {inheritedOnly && <InheritIcon colour="var(--ink-3)" size={11} />}
                            <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{label}</span>
                          </span>
                        )}
                      </span>
                      {!wide && (
                        <svg width="7" height="12" viewBox="0 0 7 12" aria-hidden style={{ color: "var(--ink-3)", flexShrink: 0 }}>
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
          <div ref={listRef} style={{ position: "relative", flex: 1, minWidth: 0, overflowY: "auto" }}>
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

/** Scroll the current one (marked data-current) into the middle of a
 *  scrolling list, whenever `key` changes — open a kind or a preset and
 *  what's in use is in view, no hunting. */
function useScrollToCurrent(key: string) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>('[data-current="true"]');
    const box = ref.current;
    if (!el || !box) return;
    // In the list's own pixels (the stage may be scaled).
    const k = box.getBoundingClientRect().height / box.offsetHeight || 1;
    const within = (el.getBoundingClientRect().top - box.getBoundingClientRect().top) / k + box.scrollTop;
    box.scrollTop = Math.max(0, within - box.clientHeight / 2 + el.offsetHeight / 2);
  }, [key]);
  return ref;
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

/** No instructions in the list: with nothing to build into, its rows sit
 *  disabled and the header says what they would fill. */
function hintFor(_s: State, _bt: BuildTarget, _kind: Kind): string | null {
  return null;
}

function KindList({ kind, items, apply, hint }: { kind: Kind; items: Item[]; apply: ((item: Item) => void) | null; hint: string | null }) {
  // Profiles open into their stacks and patches.
  if (kind.id === "profiles") return <ProfileColumns />;
  // Songs: by collection, artist, key and genre.
  if (kind.id === "songs") return <SongList items={items} apply={apply} hint={hint} />;
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
  const inUse = presets.find((p) => p.items.some((i) => i.state)) ?? presets.find((p) => p.items.some((i) => i.inherited));
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
  const presetsRef = useScrollToCurrent(`p|${picked}|${step}`);
  const variationsRef = useScrollToCurrent(`v|${picked}|${step}`);
  const showPresets = !narrow || step === "presets";
  const showVariations = !narrow || step === "variations";
  return (
    <div ref={ref} style={{ flex: 1, minHeight: 0, display: "flex", borderTop: "1px solid var(--rule)" }}>
      {showPresets && (
        <div ref={presetsRef} style={{ position: "relative", width: narrow ? "100%" : "38%", maxWidth: narrow ? undefined : 240, flexShrink: 0, overflowY: "auto", borderRight: narrow ? undefined : "1px solid var(--rule)" }}>
          {presets.map((p) => {
            const on = p.name === preset?.name;
            const over = p.items.some((i) => i.state === "swapped");
            const plays = p.items.some((i) => i.state === "playing");
            const from = p.items.some((i) => i.inherited);
            return (
              <button
                key={p.name}
                onClick={() => {
                  setPicked(p.name);
                  setStep("variations");
                }}
                className={on && !narrow ? "" : "pressable"}
                aria-current={on ? "true" : undefined}
                data-current={on ? "true" : undefined}
                style={{
                  position: "relative",
                  width: "100%",
                  minHeight: 46,
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "0 12px 0 16px",
                  textAlign: "left",
                  // The picked preset in a muted wash of its colour.
                  background: on && !narrow ? `color-mix(in oklab, ${p.colour} 18%, #0f0f12)` : undefined,
                }}
              >
                {on && !narrow && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: p.colour }} />}
                <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
                {/* Where something of it is in use: icons, not words. */}
                {over && <OverrideIcon colour={p.colour} size={12} title="An override is from here" />}
                {plays && <span title="Playing" style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />}
                {from && !over && <span title="Inherited from here" style={{ display: "flex" }}><InheritIcon colour="var(--ink-3)" size={12} /></span>}
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
        <div ref={variationsRef} style={{ position: "relative", flex: 1, minWidth: 0, overflowY: "auto" }}>
          <div style={{ position: "sticky", top: 0, zIndex: 1, display: "flex", alignItems: "center", gap: 10, minHeight: 48, padding: "8px 16px 6px", background: "#0f0f12", borderBottom: "1px solid var(--rule)" }}>
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

/** One variation: a state glyph (the override icon, the inherit icon, a
 *  live dot — or a quiet ring), its name, and what it loads or runs as
 *  compact chips — an amp capture, a drive pedal · setting, a cab IR, a
 *  delay or reverb algorithm. The one in use sits in a muted wash of its
 *  effect's colour. */
function Variation({ item, colour, onPick }: { item: Item; colour: string; onPick?: () => void }) {
  const on = !!item.state;
  const underneath = !!item.inherited;
  const chips: { icon: string; tint: string; text: ReactNode }[] = [
    ...(item.algos ?? []).map((a) => ({
      icon: a.block.startsWith("VERB") ? "Reverb" : "Delay",
      tint: a.block.startsWith("VERB") ? "#8B5CF6" : "#3B82F6",
      text: (
        <>
          <b style={{ color: "var(--ink-2)", fontWeight: 650 }}>{a.algo}</b> {a.preset}
        </>
      ),
    })),
    ...(item.models ?? []).map((m) => ({
      icon: m.role === "amp" ? "Core" : m.role === "drive" ? "Drive" : "Amp",
      tint: m.role === "amp" ? "#D6B36A" : m.role === "drive" ? "#ef4444" : "#a1a1aa",
      text: m.role === "drive" && m.pedal ? (
        <>
          <b style={{ color: "var(--ink-2)", fontWeight: 650 }}>{m.pedal}</b>
          {m.option ? ` ${m.option}` : ""}
        </>
      ) : (
        m.name
      ),
    })),
  ];
  return (
    <button
      onClick={onPick}
      disabled={!onPick}
      aria-pressed={on}
      data-current={on || underneath ? "true" : undefined}
      className={onPick && !on ? "pressable" : ""}
      style={{
        position: "relative",
        width: "100%",
        display: "flex",
        alignItems: "flex-start",
        gap: 12,
        minHeight: 52,
        padding: "12px 16px",
        textAlign: "left",
        borderBottom: "1px solid var(--rule)",
        background: on ? `color-mix(in oklab, ${colour} 16%, #0f0f12)` : underneath ? "rgba(255,255,255,0.025)" : undefined,
        cursor: onPick ? "pointer" : "default",
      }}
    >
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: item.state === "swapped" ? colour : "var(--live)" }} />}
      {/* Its state, as a glyph. */}
      <span style={{ width: 16, height: 20, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {item.state === "swapped" ? (
          <OverrideIcon colour={colour} size={14} />
        ) : item.state ? (
          <span style={{ width: 9, height: 9, borderRadius: 999, background: "var(--live)" }} />
        ) : underneath ? (
          <InheritIcon colour="var(--ink-2)" size={13} />
        ) : (
          <span style={{ width: 9, height: 9, borderRadius: 999, boxShadow: "inset 0 0 0 1.5px #3f3f46" }} />
        )}
      </span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 7 }}>
        <span style={{ display: "flex", alignItems: "baseline", gap: 10, minWidth: 0 }}>
          <span style={{ flex: 1, minWidth: 0, fontSize: 16, fontWeight: on ? 700 : 600, color: "var(--ink)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.name}</span>
          {item.state === "swapped" && <span style={{ fontSize: 12, fontWeight: 700, color: `color-mix(in oklab, ${colour} 45%, var(--ink-2))`, whiteSpace: "nowrap" }}>Override</span>}
          {item.state && item.state !== "swapped" && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)", whiteSpace: "nowrap" }}>Playing</span>}
          {!item.state && underneath && <span title={`Chosen by ${item.inherited}`} style={{ fontSize: 12, fontWeight: 650, color: "var(--ink-3)", whiteSpace: "nowrap" }}>From {item.inherited}</span>}
        </span>
        {chips.length > 0 && (
          <span style={{ display: "flex", flexWrap: "wrap", gap: 5 }}>
            {chips.map((c, k) => (
              <span key={k} style={{ display: "inline-flex", alignItems: "center", gap: 6, maxWidth: "100%", height: 24, padding: "0 8px 0 6px", borderRadius: 5, background: `color-mix(in oklab, ${c.tint} 10%, #17171b)`, fontSize: 12, color: "var(--ink-3)" }}>
                <ModuleIcon kind={c.icon} size={11} colour={c.tint} />
                <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{c.text}</span>
              </span>
            ))}
          </span>
        )}
      </span>
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
      data-current={on || !!item.inherited ? "true" : undefined}
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
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 10, bottom: 10, width: 3, borderRadius: 2, background: item.state === "swapped" ? item.colour : "var(--live)" }} />}
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
      {!item.state && item.inherited && (
        <span title={`Chosen by ${item.inherited}`} style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 5, fontSize: 12, fontWeight: 650, color: "var(--ink-2)", maxWidth: "45%", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          <InheritIcon colour="var(--ink-2)" size={11} />
          From {item.inherited}
        </span>
      )}
      {item.state && (
        <span style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 5, fontSize: 12, fontWeight: 700, color: item.state === "swapped" ? `color-mix(in oklab, ${item.colour} 70%, white)` : "var(--live)" }}>
          {item.state === "swapped" && <OverrideMark colour={item.colour} />}
          {item.state === "playing" ? "Playing" : item.state === "swapped" ? "Override" : item.state === "in" ? "In" : "In use"}
        </span>
      )}
    </button>
  );
}

// ── Songs ──────────────────────────────────────────────────────────

const KEY_ORDER = ["C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"];

/** The library's songs, found fast: a collection along the top (All, or
 *  one of the user's — Church…), then artist, key and genre
 *  to narrow it; each song's ⋯ puts it in collections and sets its
 *  artist and genre. */
function SongList({ items, apply, hint }: { items: Item[]; apply: ((item: Item) => void) | null; hint: string | null }) {
  const s = useStore();
  const [collection, setCollection] = useState<string | null>(null);
  const [artist, setArtist] = useState<string | null>(null);
  const [key, setKey] = useState<string | null>(null);
  const [genre, setGenre] = useState<string | null>(null);
  const addMenu = useMenu();
  const colMenu = useMenu();
  const songs = [...rig.library.songs, ...s.newSongs];
  const keyOf = (name: string) => songs.find((x) => x.name === name)?.key ?? "";
  // The facets offer only what the library has.
  const artists = [...new Set(items.map((i) => songInfoOf(s, i.name).artist).filter(Boolean))].sort();
  const keys = [...new Set(items.map((i) => keyOf(i.name)).filter(Boolean))].sort((a, b) => KEY_ORDER.indexOf(a) - KEY_ORDER.indexOf(b));
  const genres = [...new Set(items.map((i) => songInfoOf(s, i.name).genre).filter(Boolean))].sort();
  const col = s.collections.find((c) => c.name === collection);
  const shown = items.filter((i) => {
    const info = songInfoOf(s, i.name);
    return (!col || col.songs.includes(i.name)) && (!artist || info.artist === artist) && (!key || keyOf(i.name) === key) && (!genre || info.genre === genre);
  });
  const filtered = !!(col || artist || key || genre);
  const clear = () => {
    setCollection(null);
    setArtist(null);
    setKey(null);
    setGenre(null);
  };
  return (
    <div style={{ paddingBottom: 16 }}>
      <div style={{ position: "sticky", top: 0, zIndex: 2, background: "#0f0f12", borderBottom: "1px solid var(--rule)" }}>
        {/* Collections. */}
        <div role="tablist" aria-label="Collections" style={{ display: "flex", alignItems: "stretch", height: 46, overflowX: "auto", scrollbarWidth: "none", padding: "0 6px" }}>
          {[{ name: null as string | null, colour: "var(--ink-2)" }, ...s.collections.map((c) => ({ name: c.name as string | null, colour: c.colour }))].map((c) => {
            const on = c.name === collection;
            return (
              <button
                key={c.name ?? "all"}
                role="tab"
                aria-selected={on}
                onClick={() => setCollection(c.name)}
                className="pressable"
                style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 7, padding: "0 12px", fontSize: 14.5, fontWeight: on ? 750 : 600, whiteSpace: "nowrap", color: on ? "var(--ink)" : "var(--ink-3)", boxShadow: on ? `inset 0 -2px 0 ${c.colour}` : undefined }}
              >
                {c.name && <span aria-hidden style={{ width: 8, height: 8, borderRadius: 999, background: c.colour }} />}
                {c.name ?? "All songs"}
              </button>
            );
          })}
          <button className="pressable" onClick={addMenu.fromButton} aria-label="New collection" style={{ flexShrink: 0, width: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)" }}>
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
              <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
          </button>
          {addMenu.open && (
            <Menu
              at={addMenu.open.at}
              naming={0}
              items={[{ kind: "name", id: "add", label: "New collection…", initial: "", confirm: "Add", taken: s.collections.map((c) => c.name) }]}
              onPick={(p) => {
                newCollection(p.text);
                setCollection(p.text);
              }}
              onClose={addMenu.close}
            />
          )}
        </div>
        {/* Narrow it: artist, key, genre. */}
        <div style={{ display: "flex", alignItems: "center", gap: 6, minHeight: 52, padding: "4px 8px 6px 10px", flexWrap: "wrap" }}>
          <Facet label="Artist" value={artist} options={artists} onPick={setArtist} />
          <Facet label="Key" value={key} options={keys} onPick={setKey} />
          <Facet label="Genre" value={genre} options={genres} onPick={setGenre} />
          <span style={{ flex: 1 }} />
          {col && (
            <>
              <MoreButton label={`${col.name} actions`} onClick={colMenu.fromButton} />
              {colMenu.open && (
                <Menu
                  at={colMenu.open.at}
                  items={[
                    { kind: "head", label: col.name },
                    { kind: "name", id: "rename", label: "Rename…", initial: col.name, confirm: "Rename", taken: s.collections.map((c) => c.name) },
                    { kind: "sep" },
                    { kind: "delete", id: "delete", label: "Delete collection" },
                  ]}
                  onPick={(p) => {
                    if (p.id === "rename") {
                      renameCollection(col.name, p.text);
                      setCollection(p.text);
                    }
                    if (p.id === "delete") {
                      removeCollection(col.name);
                      setCollection(null);
                    }
                  }}
                  onClose={colMenu.close}
                />
              )}
            </>
          )}
        </div>
      </div>
      {hint && <Quiet small>{hint}</Quiet>}
      {shown.map((i) => (
        <SongRow key={i.id} item={i} onPick={apply ? () => apply(i) : undefined} />
      ))}
      {shown.length === 0 && (
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12, padding: "32px 16px" }}>
          <span style={{ fontSize: 14, color: "var(--ink-3)" }}>{col && !artist && !key && !genre ? `${col.name} is empty` : "No songs match"}</span>
          {filtered && (
            <button className="pressable" onClick={clear} style={{ height: 44, padding: "0 16px", borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, color: "var(--ink)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
              Clear filters
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** A filter: its name when open to anything, its value (and a clear)
 *  when set; the choices open as a menu. */
function Facet({ label, value, options, onPick }: { label: string; value: string | null; options: string[]; onPick: (v: string | null) => void }) {
  const menu = useMenu();
  const on = value !== null;
  return (
    <span style={{ display: "flex", alignItems: "center", borderRadius: "var(--r)", background: on ? "var(--fill-on)" : "var(--fill)" }}>
      <button
        className="pressable"
        onClick={menu.fromButton}
        aria-haspopup="menu"
        aria-label={on ? `${label}: ${value}` : `Filter by ${label.toLowerCase()}`}
        style={{ height: 44, display: "flex", alignItems: "center", gap: 7, padding: on ? "0 6px 0 12px" : "0 10px 0 12px", fontSize: 14, fontWeight: on ? 700 : 600, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap" }}
      >
        {on ? value : label}
        {!on && (
          <svg width="10" height="6" viewBox="0 0 11 7" aria-hidden style={{ color: "var(--ink-3)" }}>
            <path d="M1 1l4.5 4.5L10 1" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </button>
      {on && (
        <button className="pressable" onClick={() => onPick(null)} aria-label={`Clear ${label.toLowerCase()}`} style={{ width: 36, height: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)" }}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
      )}
      {menu.open && (
        <Menu
          at={menu.open.at}
          items={[{ kind: "head", label }, { kind: "run", id: "__any", label: `Any ${label.toLowerCase()}`, checked: !on }, ...options.map((o): MenuItem => ({ kind: "run", id: o, label: o, checked: o === value }))]}
          onPick={(p) => onPick(p.id === "__any" ? null : p.id)}
          onClose={menu.close}
        />
      )}
    </span>
  );
}

/** A song: its colour, name, who it's by with key and tempo, the
 *  collections it's in as dots; ⋯ puts it in collections, sets its artist
 *  and genre. */
function SongRow({ item, onPick }: { item: Item; onPick?: () => void }) {
  const s = useStore();
  const menu = useMenu();
  const info = songInfoOf(s, item.name);
  const ins = s.collections.filter((c) => c.songs.includes(item.name));
  const on = item.state === "in";
  const items: MenuItem[] = [
    { kind: "head", label: item.name },
    ...s.collections.map((c): MenuItem => ({ kind: "run", id: `col:${c.name}`, label: c.name, checked: c.songs.includes(item.name) })),
    { kind: "name", id: "newcol", label: "New collection…", initial: "", confirm: "Add", taken: s.collections.map((c) => c.name) },
    { kind: "sep" },
    { kind: "name", id: "artist", label: info.artist ? `Artist · ${info.artist}` : "Artist…", initial: info.artist, confirm: "Set" },
    { kind: "head", label: "Genre" },
    ...GENRES.map((g): MenuItem => ({ kind: "run", id: `genre:${g}`, label: g, checked: info.genre === g })),
  ];
  const onMenu = (p: Picked) => {
    if (p.id.startsWith("col:")) toggleInCollection(p.id.slice(4), item.name);
    else if (p.id === "newcol") newCollection(p.text, [item.name]);
    else if (p.id === "artist") setSongInfo(item.name, { artist: p.text });
    else if (p.id.startsWith("genre:")) setSongInfo(item.name, { genre: p.id.slice(6) });
  };
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", background: on ? "rgba(255,255,255,0.06)" : undefined }}>
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 10, bottom: 10, width: 3, borderRadius: 2, background: "var(--live)" }} />}
      <button
        onClick={onPick}
        disabled={!onPick}
        className={onPick ? "pressable" : ""}
        aria-pressed={on}
        data-current={on ? "true" : undefined}
        style={{ flex: 1, minWidth: 0, minHeight: 56, display: "flex", alignItems: "center", gap: 12, padding: "6px 4px 6px 16px", textAlign: "left", cursor: onPick ? "pointer" : "default" }}
      >
        <span aria-hidden style={{ width: 10, height: 10, borderRadius: 3, flexShrink: 0, background: item.colour }} />
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
          <span style={{ fontSize: 16, fontWeight: on ? 700 : 560, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.name}</span>
          {item.from && <span style={{ fontSize: 13, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.from}</span>}
        </span>
        {ins.length > 0 && (
          <span aria-label={`In ${ins.map((c) => c.name).join(", ")}`} style={{ display: "flex", gap: 4, flexShrink: 0 }}>
            {ins.map((c) => (
              <span key={c.name} title={c.name} style={{ width: 7, height: 7, borderRadius: 999, background: c.colour }} />
            ))}
          </span>
        )}
        {on && <span style={{ flexShrink: 0, fontSize: 12, fontWeight: 700, color: "var(--live)" }}>In</span>}
      </button>
      <MoreButton label={`${item.name} actions`} onClick={menu.fromButton} />
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onMenu} onClose={menu.close} />}
    </div>
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

/** "Clear n overrides": every override (and, for a part, Edit's unsaved
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
      title={bt?.kind === "part" ? "Clear every override and unsaved change — back to its preset" : "Clear every override in this variation"}
      style={{ height: 32, padding: "0 10px", borderRadius: "var(--r)", display: "flex", alignItems: "center", gap: 6, fontSize: 12.5, fontWeight: 700, whiteSpace: "nowrap", color: "var(--ink-2)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}
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

/** The override icon, at the size of the browser's state labels. */
function OverrideMark({ colour }: { colour: string }) {
  return <OverrideIcon colour={colour} size={11} />;
}
