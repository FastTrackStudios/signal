// What the player changes, seeded from the rig's real data, with an undo for
// every change — the prototype of the engine's `undo_sound`. Nothing vanishes,
// it cancels: a deleted preset stays on the sheet, struck, until Undo or a
// purge; a played song is struck, not removed.

import { useSyncExternalStore } from "react";
import { chain, rig, modulesOf, type ModulePreset, type Setlist as RigSetlist, type SongEntry, type SongSlot } from "./data/rig";
import { parseSetName, setName, type SetMeta } from "./setlist/sets";

/** A set as the player keeps it: its songs, and what it is — an event on a
 *  date, with a title only when the night has one. `name` is built from
 *  those (the house style the rig's files use). */
export type Setlist = RigSetlist & SetMeta;

/** What a section plays: a patch of the profile, or a preset. */
export interface Sound {
  kind: "patch" | "preset";
  name: string;
}

/** A part: the smallest step through a song, and the patch it plays. */
export interface Part {
  name: string;
  sound: Sound | null;
}

/** A section — Verse 1, Chorus 2 — and the parts it runs through, in order.
 *  Most sections are one part; a section that changes sound partway (a
 *  build, a tag) has several. */
export interface Section {
  name: string;
  parts: Part[];
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
  /** The part playing within that section. */
  subIndex: number;
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
  /** Colours set by hand, by song name (the rest are their names'). */
  songColours: Record<string, string>;
  /** What the player switched to by hand, over what the part plays; null
   *  when the part's own patch is playing. */
  live: string | null;
  /** Where each stack's rotation is: the patch a tap on it plays. */
  stackAt: Record<string, number>;
}

let seedAt = { section: 0, part: 0 };

function seed(): State {
  const perf = rig.perf;
  const setlists: Setlist[] = rig.library.setlists.map((s) => ({ ...s, ...parseSetName(s.name), songs: s.songs.map((x) => ({ ...x })) }));
  const setIndex = Math.max(0, setlists.findIndex((s) => s.active));
  // The rig's own view of the set up carries what each song starts on.
  for (const song of setlists[setIndex]?.songs ?? []) {
    const live = perf.songs.find((x) => x.name === song.name);
    if (live?.start) song.start = live.start;
  }
  const sections: Record<string, Section[]> = {};
  for (const song of rig.library.songs) {
    sections[song.name] = song.parts.map((p) => ({ name: p, parts: [{ name: p, sound: null }] }));
  }
  const up = perf.songs[perf.song_index]?.name;
  if (up) {
    // The rig's parts, grouped into their sections (runs of one section
    // name), each keeping its own patch.
    const grouped: Section[] = [];
    let at = { section: 0, part: 0 };
    perf.parts.forEach((p, i) => {
      const name = p.section || p.name;
      const last = grouped[grouped.length - 1];
      const part: Part = { name: p.name, sound: p.patch ? { kind: "patch", name: p.patch } : null };
      if (last && last.name === name) last.parts.push(part);
      else grouped.push({ name, parts: [part] });
      if (i === perf.part_index) at = { section: grouped.length - 1, part: grouped[grouped.length - 1].parts.length - 1 };
    });
    sections[up] = grouped;
    seedAt = at;
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
    partIndex: seedAt.section,
    subIndex: seedAt.part,
    presets,
    bypass,
    blockPick: {},
    presetPicks: {},
    order: {},
    side: {},
    newSongs: [],
    songColours: {},
    live: null,
    stackAt: {},
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

/** What part `part` of section `index` plays (a one-part section: the
 *  section's sound). */
export function setSectionSound(song: string, index: number, sound: Sound | null, part = 0) {
  const sec = state.sections[song]?.[index];
  const where = sec && sec.parts.length > 1 ? `${sec.name} · ${sec.parts[part]?.name}` : sectionsOfName(song, index);
  change(`${song} · ${where} → ${sound ? sound.name : "keeps what plays"}`, (s) => ({
    ...s,
    sections: {
      ...s.sections,
      [song]: sectionsOf(s, song).map((x, i) =>
        i === index ? { ...x, parts: x.parts.map((p, j) => (j === part ? { ...p, sound } : p)) } : x,
      ),
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
  move((s) => ({ ...s, songIndex: index, partIndex: 0, subIndex: 0, live: null, stackAt: {} }));
}

export function goToPart(index: number) {
  move((s) => ({ ...s, partIndex: index, subIndex: 0, live: null }));
}

export function chooseSet(index: number) {
  move((s) => ({ ...s, setIndex: index, songIndex: 0, partIndex: 0, subIndex: 0, live: null, stackAt: {} }));
}

export function newSetlist(meta: SetMeta, songs: SongSlot[] = []) {
  const name = setName(meta);
  change(`New set ${name}`, (s) => ({
    ...s,
    setlists: [...s.setlists, { name, active: false, songs: songs.map((x) => ({ ...x })), ...meta }],
    setIndex: s.setlists.length,
    songIndex: 0,
    partIndex: 0,
  }));
}

/** A set's event, date and title (its stored name follows). */
export function setDetails(index: number, meta: SetMeta) {
  const name = setName(meta);
  change(`Set → ${name}`, (s) => ({
    ...s,
    setlists: s.setlists.map((l, i) => (i === index ? { ...l, ...meta, name } : l)),
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
    sections: { ...s.sections, [song]: [...sectionsOf(s, song), { name, parts: [{ name, sound: null }] }] },
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


export function duplicateSet(index: number, meta: SetMeta) {
  const src = state.setlists[index];
  newSetlist(meta, src?.songs ?? []);
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
    sections: {
      ...s.sections,
      [song]: sectionsOf(s, song).map((x, i) =>
        i === index ? { ...x, name, parts: x.parts.length === 1 && x.parts[0].name === x.name ? [{ ...x.parts[0], name }] : x.parts } : x,
      ),
    },
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

/** A song's colour by hand (null: back to its name's). */
export function setSongColour(name: string, colour: string | null) {
  change(`${name} colour → ${colour ?? "from its name"}`, (s) => {
    const songColours = { ...s.songColours };
    if (colour) songColours[name] = colour;
    else delete songColours[name];
    return { ...s, songColours };
  });
}

// ── Parts within a section ───────────────────────────────────────────

/** Play part `part` of section `section` (of the song up). */
export function goToSub(section: number, part: number) {
  move((s) => ({ ...s, partIndex: section, subIndex: part, live: null }));
}

function editParts(s: State, song: string, index: number, f: (parts: Part[]) => Part[]): State {
  return {
    ...s,
    sections: { ...s.sections, [song]: sectionsOf(s, song).map((x, i) => (i === index ? { ...x, parts: f(x.parts) } : x)) },
  };
}

/** A new part at the end of a section — it plays what the last one did,
 *  until it's given its own. */
export function addPart(song: string, index: number, name: string) {
  change(`${song}: add part ${name}`, (s) =>
    editParts(s, song, index, (parts) => [...parts, { name, sound: parts[parts.length - 1]?.sound ?? null }]),
  );
}

export function renamePart(song: string, index: number, part: number, name: string) {
  change(`${song}: rename part → ${name}`, (s) => editParts(s, song, index, (parts) => parts.map((p, j) => (j === part ? { ...p, name } : p))));
}

export function removePart(song: string, index: number, part: number) {
  change(`${song}: delete a part`, (s) => {
    const next = editParts(s, song, index, (parts) => parts.filter((_, j) => j !== part));
    const sub = index === s.partIndex && part <= s.subIndex ? Math.max(0, s.subIndex - 1) : s.subIndex;
    return { ...next, subIndex: sub };
  });
}

export function movePart(song: string, index: number, from: number, to: number) {
  change(`${song}: move a part`, (s) =>
    editParts(s, song, index, (parts) => {
      if (to < 0 || to >= parts.length) return parts;
      const list = [...parts];
      const [x] = list.splice(from, 1);
      list.splice(to, 0, x);
      return list;
    }),
  );
}

// ── Playing a stack by hand ──────────────────────────────────────────

/** The patch the part up plays: its own, else the last one before it in
 *  the song that sets one (a part that "keeps" plays on). */
export function partPatch(s: State): string | null {
  const song = currentSong(s)?.name;
  if (!song) return null;
  const secs = sectionsOf(s, song);
  for (let i = s.partIndex; i >= 0; i--) {
    const parts = secs[i]?.parts ?? [];
    const from = i === s.partIndex ? Math.min(s.subIndex, parts.length - 1) : parts.length - 1;
    for (let k = from; k >= 0; k--) if (parts[k].sound) return parts[k].sound!.name;
  }
  return currentSong(s)?.start || null;
}

/** What is playing: the hand-picked patch, else the part's. */
export function playing(s: State): string | null {
  return s.live ?? partPatch(s);
}

/** Tap a stack: play the patch its rotation is on — or, when it is the
 *  stack already playing, step to its next one. */
export function tapStack(stack: string, patches: string[], playingIndex: number | null) {
  if (patches.length === 0) return;
  move((s) => {
    const at = playingIndex !== null ? (playingIndex + 1) % patches.length : (s.stackAt[stack] ?? 0) % patches.length;
    const name = patches[at];
    return { ...s, live: name === partPatch(s) ? null : name, stackAt: { ...s.stackAt, [stack]: at } };
  });
}

/** Back to what the part plays. */
export function backToPart() {
  move((s) => ({ ...s, live: null }));
}

/** Keep the hand-picked patch: the part up plays it from now on. */
export function keepLive() {
  const song = currentSong(state)?.name;
  const name = state.live;
  if (!song || !name) return;
  setSectionSound(song, state.partIndex, { kind: "patch", name }, state.subIndex);
  move((s) => ({ ...s, live: null }));
}
