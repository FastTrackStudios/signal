// What the player changes, seeded from the rig's real data, with an undo for
// every change — the prototype of the engine's `undo_sound`. Nothing vanishes,
// it cancels: a deleted preset stays on the sheet, struck, until Undo or a
// purge; a played song is struck, not removed.

import { useSyncExternalStore } from "react";
import { chain, rig, modulesOf, type ModulePreset, type Setlist, type SongEntry, type SongSlot } from "./data/rig";

/** What a section plays: a patch of the profile, or a preset. */
export interface Sound {
  kind: "patch" | "preset";
  name: string;
}

export interface Section {
  name: string;
  sound: Sound | null;
}

export interface PresetState {
  name: string;
  variations: string[];
  /** Cancelled: struck on the sheet, gone on the next purge. */
  cancelled: boolean;
  source: ModulePreset;
}

export interface State {
  setlists: Setlist[];
  setIndex: number;
  /** The song up in the set; those before it are played (struck). */
  songIndex: number;
  /** Section lists by song name. */
  sections: Record<string, Section[]>;
  /** The section playing in the song up. */
  partIndex: number;
  presets: PresetState[];
  /** Bypass by chain block id. */
  bypass: Record<string, boolean>;
  /** Block preset picks by chain block id. */
  blockPick: Record<string, string>;
  /** A preset's module picks changed here, by "preset/variation/module". */
  presetPicks: Record<string, string>;
  /** The chain's order, by module: block ids as the player arranged them. */
  order: Record<string, string[]>;
  /** A block moved to the amp pair's other side (or out of it). */
  side: Record<string, "L" | "R" | null>;
  /** Songs made in this session, beside the rig's library. */
  newSongs: SongEntry[];
}

function seed(): State {
  const perf = rig.perf;
  const setlists = rig.library.setlists.map((s) => ({ ...s, songs: s.songs.map((x) => ({ ...x })) }));
  const setIndex = Math.max(0, setlists.findIndex((s) => s.active));
  // The rig's own view of the set up carries what each song starts on.
  for (const song of setlists[setIndex]?.songs ?? []) {
    const live = perf.songs.find((x) => x.name === song.name);
    if (live?.start) song.start = live.start;
  }
  const sections: Record<string, Section[]> = {};
  for (const song of rig.library.songs) {
    sections[song.name] = song.parts.map((p) => ({ name: p, sound: null }));
  }
  const up = perf.songs[perf.song_index]?.name;
  if (up) {
    sections[up] = perf.parts.map((p) => ({
      name: p.section || p.name,
      sound: p.patch ? { kind: "patch", name: p.patch } : null,
    }));
  }
  const presets = modulesOf("Preset").map((m) => ({
    name: m.name,
    variations: [...m.snapshots],
    cancelled: false,
    source: m,
  }));
  const bypass: Record<string, boolean> = {};
  for (const n of rig.nodes) if (n.is_block) bypass[n.id] = n.bypassed;
  return {
    setlists,
    setIndex,
    songIndex: perf.song_index,
    sections,
    partIndex: perf.part_index,
    presets,
    bypass,
    blockPick: {},
    presetPicks: {},
    order: {},
    side: {},
    newSongs: [],
  };
}

let state: State = seed();
const past: { label: string; state: State }[] = [];
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

/** Change the state, remembering what it was for Undo. */
export function change(label: string, edit: (s: State) => State) {
  past.push({ label, state });
  if (past.length > 64) past.shift();
  state = edit(state);
  emit();
}

/** Change without an undo step (moving through the set). */
export function move(edit: (s: State) => State) {
  state = edit(state);
  emit();
}

export function undo() {
  const last = past.pop();
  if (!last) return;
  state = last.state;
  emit();
}

export function useStore(): State {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

export function useUndo(): { depth: number; label: string | null } {
  useStore();
  return { depth: past.length, label: past[past.length - 1]?.label ?? null };
}

// ── Edits ─────────────────────────────────────────────────────────────

export function currentSet(s: State): Setlist {
  return s.setlists[s.setIndex];
}

export function currentSong(s: State): SongSlot | undefined {
  return currentSet(s)?.songs[s.songIndex];
}

export function sectionsOf(s: State, song: string): Section[] {
  return s.sections[song] ?? [];
}

export function setSectionSound(song: string, index: number, sound: Sound | null) {
  const label = `${song} · ${sectionsOfName(song, index)} → ${sound ? sound.name : "keeps what plays"}`;
  change(label, (s) => ({
    ...s,
    sections: {
      ...s.sections,
      [song]: sectionsOf(s, song).map((x, i) => (i === index ? { ...x, sound } : x)),
    },
  }));
}

function sectionsOfName(song: string, index: number): string {
  return state.sections[song]?.[index]?.name ?? "section";
}

export function moveSong(from: number, to: number) {
  change("Reorder the set", (s) => {
    const set = currentSet(s);
    const songs = [...set.songs];
    const [x] = songs.splice(from, 1);
    songs.splice(to, 0, x);
    const setlists = s.setlists.map((l, i) => (i === s.setIndex ? { ...l, songs } : l));
    // The song up stays the song up.
    let songIndex = s.songIndex;
    if (from === s.songIndex) songIndex = to;
    else if (from < s.songIndex && to >= s.songIndex) songIndex -= 1;
    else if (from > s.songIndex && to <= s.songIndex) songIndex += 1;
    return { ...s, setlists, songIndex };
  });
}

export function addSong(slot: SongSlot) {
  change(`Add ${slot.name}`, (s) => {
    const setlists = s.setlists.map((l, i) => (i === s.setIndex ? { ...l, songs: [...l.songs, { ...slot }] } : l));
    return { ...s, setlists };
  });
}

export function removeSong(index: number) {
  const name = currentSet(state).songs[index]?.name ?? "song";
  change(`Remove ${name}`, (s) => {
    const setlists = s.setlists.map((l, i) =>
      i === s.setIndex ? { ...l, songs: l.songs.filter((_, j) => j !== index) } : l,
    );
    const songIndex = index < s.songIndex ? s.songIndex - 1 : Math.min(s.songIndex, setlists[s.setIndex].songs.length - 1);
    return { ...s, setlists, songIndex };
  });
}

export function setSongField(index: number, field: "key" | "bpm", value: string | number) {
  const name = currentSet(state).songs[index]?.name ?? "song";
  change(`${name} ${field} → ${value}`, (s) => {
    const setlists = s.setlists.map((l, i) =>
      i === s.setIndex ? { ...l, songs: l.songs.map((x, j) => (j === index ? { ...x, [field]: value } : x)) } : l,
    );
    return { ...s, setlists };
  });
}

export function goToSong(index: number) {
  move((s) => ({ ...s, songIndex: index, partIndex: 0 }));
}

export function goToPart(index: number) {
  move((s) => ({ ...s, partIndex: index }));
}

export function chooseSet(index: number) {
  move((s) => ({ ...s, setIndex: index, songIndex: 0, partIndex: 0 }));
}

export function newSetlist(name: string) {
  change(`New set ${name}`, (s) => ({
    ...s,
    setlists: [...s.setlists, { name, active: false, songs: [] }],
    setIndex: s.setlists.length,
    songIndex: 0,
  }));
}

export function newPreset(name: string, from?: PresetState) {
  change(`New preset ${name}`, (s) => ({
    ...s,
    presets: [
      ...s.presets,
      {
        name,
        variations: ["Main"],
        cancelled: false,
        source: from?.source ?? s.presets[0].source,
      },
    ],
  }));
}

export function cancelPreset(name: string, cancelled: boolean) {
  change(`${cancelled ? "Delete" : "Restore"} ${name}`, (s) => ({
    ...s,
    presets: s.presets.map((p) => (p.name === name ? { ...p, cancelled } : p)),
  }));
}

export function renamePreset(name: string, to: string) {
  change(`Rename ${name} → ${to}`, (s) => ({
    ...s,
    presets: s.presets.map((p) => (p.name === name ? { ...p, name: to } : p)),
  }));
}

export function addVariation(preset: string, variation: string) {
  change(`${preset}: new variation ${variation}`, (s) => ({
    ...s,
    presets: s.presets.map((p) => (p.name === preset ? { ...p, variations: [...p.variations, variation] } : p)),
  }));
}

export function toggleBypass(id: string, name: string) {
  change(`${state.bypass[id] ? "Engage" : "Bypass"} ${name}`, (s) => ({
    ...s,
    bypass: { ...s.bypass, [id]: !s.bypass[id] },
  }));
}

export function pickBlockPreset(id: string, block: string, preset: string) {
  change(`${block} → ${preset}`, (s) => ({ ...s, blockPick: { ...s.blockPick, [id]: preset } }));
}

export function addSection(song: string, name: string) {
  change(`${song}: add ${name}`, (s) => ({
    ...s,
    sections: { ...s.sections, [song]: [...sectionsOf(s, song), { name, sound: null }] },
  }));
}

export function setVariationPick(preset: string, variation: string, module: string, pick: string) {
  change(`${preset} · ${variation}: ${module} → ${pick}`, (s) => ({
    ...s,
    presetPicks: { ...s.presetPicks, [`${preset}/${variation}/${module}`]: pick },
  }));
}

/** A block moved within its module (a drag in Routing). */
export function moveBlock(module: string, ids: string[], from: number, to: number) {
  change(`Move a block in ${module}`, (s) => {
    const list = [...(s.order[module] ?? ids)];
    const [x] = list.splice(from, 1);
    list.splice(to, 0, x);
    return { ...s, order: { ...s.order, [module]: list } };
  });
}

/** Split a block onto the amp pair's other side, or join it back. */
export function setSide(id: string, name: string, side: "L" | "R" | null) {
  change(`${name} → ${side === "R" ? "right side" : side === "L" ? "left side" : "the main path"}`, (s) => ({
    ...s,
    side: { ...s.side, [id]: side },
  }));
}

/** The chain in signal order as the player arranged it (Routing's drags). */
export function orderedChain(s: State): ReturnType<typeof chain> {
  const base = chain();
  const out: ReturnType<typeof chain> = [];
  let i = 0;
  while (i < base.length) {
    const module = base[i].module;
    const group: ReturnType<typeof chain> = [];
    while (i < base.length && base[i].module === module) group.push(base[i++]);
    const order = s.order[module];
    if (order) group.sort((a, b) => order.indexOf(a.node.id) - order.indexOf(b.node.id));
    out.push(...group);
  }
  return out;
}

/** Which side of the amp pair a block plays on: the player's choice, else
 *  what its name says (Amp R, Cab R). */
export function sideOf(s: State, id: string, name: string): "L" | "R" | null {
  if (id in s.side) return s.side[id];
  return /(\s|\[)R\]?$|\bR$/.test(name) ? "R" : /(\s|\[)L\]?$|\bL$/.test(name) ? "L" : null;
}

// ── Setlist management ───────────────────────────────────────────────

export function renameSet(index: number, name: string) {
  change(`Rename set → ${name}`, (s) => ({
    ...s,
    setlists: s.setlists.map((l, i) => (i === index ? { ...l, name } : l)),
  }));
}

export function duplicateSet(index: number, name: string) {
  change(`Duplicate set as ${name}`, (s) => {
    const src = s.setlists[index];
    const copy = { ...src, name, active: false, songs: src.songs.map((x) => ({ ...x })) };
    const setlists = [...s.setlists.slice(0, index + 1), copy, ...s.setlists.slice(index + 1)];
    return { ...s, setlists, setIndex: index + 1, songIndex: 0, partIndex: 0 };
  });
}

export function deleteSet(index: number) {
  const name = state.setlists[index]?.name ?? "set";
  change(`Delete ${name}`, (s) => {
    const setlists = s.setlists.filter((_, i) => i !== index);
    const setIndex = Math.max(0, Math.min(s.setIndex - (index < s.setIndex ? 1 : 0), setlists.length - 1));
    return { ...s, setlists, setIndex, songIndex: 0, partIndex: 0 };
  });
}

/** What a song starts on in this set (empty: the profile's default). */
export function setStart(index: number, patch: string) {
  const name = currentSet(state).songs[index]?.name ?? "song";
  change(`${name} starts on ${patch || "the default"}`, (s) => {
    const setlists = s.setlists.map((l, i) =>
      i === s.setIndex ? { ...l, songs: l.songs.map((x, j) => (j === index ? { ...x, start: patch } : x)) } : l,
    );
    return { ...s, setlists };
  });
}

/** A song made here: into the library (this session's) and onto the set. */
export function newSong(name: string, key: string, bpm: number) {
  change(`New song ${name}`, (s) => ({
    ...s,
    newSongs: [...s.newSongs, { name, key, bpm, parts: [], setlists: [currentSet(s).name], profile: "", start_part: "" }],
    sections: { ...s.sections, [name]: [] },
    setlists: s.setlists.map((l, i) => (i === s.setIndex ? { ...l, songs: [...l.songs, { name, key, bpm, start: "" }] } : l)),
  }));
}

export function renameSection(song: string, index: number, name: string) {
  change(`${song}: rename section → ${name}`, (s) => ({
    ...s,
    sections: { ...s.sections, [song]: sectionsOf(s, song).map((x, i) => (i === index ? { ...x, name } : x)) },
  }));
}

export function removeSection(song: string, index: number) {
  const name = sectionsOf(state, song)[index]?.name ?? "section";
  change(`${song}: delete ${name}`, (s) => ({
    ...s,
    sections: { ...s.sections, [song]: sectionsOf(s, song).filter((_, i) => i !== index) },
    partIndex: Math.max(0, s.partIndex - (index <= s.partIndex ? 1 : 0)),
  }));
}

export function moveSection(song: string, from: number, to: number) {
  change(`${song}: move a section`, (s) => {
    const list = [...sectionsOf(s, song)];
    if (to < 0 || to >= list.length) return s;
    const [x] = list.splice(from, 1);
    list.splice(to, 0, x);
    return { ...s, sections: { ...s.sections, [song]: list } };
  });
}
