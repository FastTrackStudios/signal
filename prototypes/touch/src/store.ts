// What the player changes, seeded from the rig's real data, with an undo for
// every change — the prototype of the engine's `undo_sound`. Nothing vanishes,
// it cancels: a deleted preset stays on the sheet, struck, until Undo or a
// purge; a played song is struck, not removed.

import { useSyncExternalStore } from "react";
import { defaultStacks, profileNamed, stacksFor, type Borrowed, type StackDef } from "./setlist/stacks";
import { chain, rig, modulesOf, type ModulePreset, type Setlist as RigSetlist, type SongEntry, type SongSlot } from "./data/rig";
import { parseSetName, setName, type SetMeta } from "./setlist/sets";

/** A set as the player keeps it: its songs, and what it is — an event on a
 *  date, with a title only when the night has one. `name` is built from
 *  those (the house style the rig's files use). */
export type Setlist = RigSetlist & SetMeta & {
  /** The profile every song in the set plays on, unless the song has its own. */
  profile?: string;
};

/** What a section plays: a patch of the profile, or a preset. */
export interface Sound {
  /** A patch, a preset's variation, or a whole stack (it plays the stack —
   *  its first patch, then wherever the switch steps it). */
  kind: "patch" | "preset" | "stack";
  name: string;
  /** Borrowed from this profile, not the one the song plays on. */
  profile?: string;
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
  /** A song's own profile, picked by the player (over the set's default). */
  songProfiles: Record<string, string>;
  /** What the player switched to by hand, over what the part plays; null
   *  when the part's own patch is playing. */
  live: string | null;
  /** Where each stack's rotation is: the patch a tap on it plays. */
  stackAt: Record<string, number>;
  /** What the footswitches step through — rig state, shared by every remote. */
  performMode: PerformMode;
  /** The house (main outputs) muted; the phones keep playing. */
  houseMute: boolean;
  /** Fully muted: your own guitar out of the phones too (the rest of the
   *  in-ear mix plays on). Only ever with the house muted. */
  phonesMute: boolean;
  /** When Panic last reset the audio and MIDI (ms), while it runs. */
  panicAt: number | null;
  /** What Compose and Edit work on: a section's part, picked in the
   *  sidebar. Nothing until something is picked. */
  selection: Target | null;
  /** Per section/part: module presets swapped in for it (Compose), and
   *  edits made to its sound in Edit — both its own, over the preset,
   *  until saved to the preset. Keyed by `targetKey`. */
  overrides: Record<string, Override>;
  /** Profiles' stacks as the player has edited them (names, order,
   *  patches); a profile not here plays as the library has it. */
  profileStacks: Record<string, StackDef[]>;
  /** The kind the browser should show for what's being worked on (a block
   *  picked in Edit's routing → its presets). */
  browserFocus: string | null;
  /** The guitars — each its own profile, carrying how it sits on every
   *  rig it has been set up on. */
  guitars: Guitar[];
  guitarIndex: number;
  /** The audio + MIDI rigs — the interface and controller. */
  rigs: Rig[];
  rigIndex: number;
  /** In Preset mode, the preset and variation playing. */
  presetUp: { preset: string; variation: string } | null;
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
    songProfiles: {},
    live: null,
    stackAt: {},
    performMode: "setlist",
    houseMute: false,
    phonesMute: false,
    panicAt: null,
    selection: null,
    overrides: {},
    profileStacks: {},
    browserFocus: null,
    presetUp: null,
    guitars: seedGuitars(),
    guitarIndex: 0,
    rigs: seedRigs(),
    rigIndex: 0,
  };
}

let state: State = seed();
const past: { label: string; state: State }[] = [];
/** What Undo took back, for Redo; a new change forgets it. */
const future: { label: string; state: State }[] = [];
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

/** Change the state, remembering what it was for Undo. */
export function change(label: string, edit: (s: State) => State) {
  past.push({ label, state });
  if (past.length > 64) past.shift();
  future.length = 0;
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
  future.push({ label: last.label, state });
  state = last.state;
  emit();
}

export function redo() {
  const next = future.pop();
  if (!next) return;
  past.push({ label: next.label, state });
  state = next.state;
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

export function useUndo(): { depth: number; label: string | null; redoDepth: number; redoLabel: string | null } {
  useStore();
  return {
    depth: past.length,
    label: past[past.length - 1]?.label ?? null,
    redoDepth: future.length,
    redoLabel: future[future.length - 1]?.label ?? null,
  };
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

/** Back to the preset's own pick for a module (or block) in a variation. */
export function clearVariationPick(preset: string, variation: string, module: string) {
  change(`${preset} · ${variation}: ${module} → its own`, (s) => {
    const presetPicks = { ...s.presetPicks };
    delete presetPicks[`${preset}/${variation}/${module}`];
    return { ...s, presetPicks };
  });
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
    // The section playing keeps playing wherever it moves to.
    let partIndex = s.partIndex;
    if (currentSong(s)?.name === song) {
      if (partIndex === from) partIndex = to;
      else if (from < partIndex && to >= partIndex) partIndex--;
      else if (from > partIndex && to <= partIndex) partIndex++;
    }
    return { ...s, partIndex, sections: { ...s.sections, [song]: list } };
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
    for (let k = from; k >= 0; k--) {
      const sound = parts[k].sound;
      if (!sound) continue;
      // A stack plays its first patch.
      if (sound.kind === "stack") return profileStacksOf(s, sound.profile ?? profileOf(s, song).name).find((d) => d.name === sound.name)?.patches[0] ?? sound.name;
      return sound.name;
    }
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

/** The patches a song's parts borrow from other profiles. */
export function borrowedOf(s: State, song?: string): Borrowed[] {
  if (!song) return [];
  const out: Borrowed[] = [];
  for (const sec of sectionsOf(s, song))
    for (const p of sec.parts) {
      const sound = p.sound;
      if (!sound?.profile) continue;
      // A stack from another profile lends all its patches.
      if (sound.kind === "stack") for (const name of profileStacksOf(s, sound.profile).find((d) => d.name === sound.name)?.patches ?? []) out.push({ name, profile: sound.profile });
      else out.push({ name: sound.name, profile: sound.profile });
    }
  return out;
}

/** Keep the hand-picked patch: the part up plays it from now on. */
export function keepLive() {
  const song = currentSong(state)?.name;
  const name = state.live;
  if (!song || !name) return;
  const hit = songStacks(state, song)
    .flatMap((st) => st.patches)
    .find((p) => p.name === name);
  const profile = hit?.from === "other" ? hit.profile : undefined;
  setSectionSound(song, state.partIndex, { kind: "patch", name, ...(profile ? { profile } : {}) }, state.subIndex);
  move((s) => ({ ...s, live: null }));
}

// ── Profiles: the set's default, a song's own ────────────────────────

export type ProfileFrom = "song" | "set" | "rig";

/** The profile a song plays on, and why: the song's own (the player's
 *  pick, else the library's), else the set's default, else the rig's
 *  active profile. */
export function profileOf(s: State, song?: string): { name: string; from: ProfileFrom } {
  const own = (song && s.songProfiles[song]) || rig.library.songs.find((x) => x.name === song)?.profile;
  if (own) return { name: own, from: "song" };
  const set = currentSet(s)?.profile;
  if (set) return { name: set, from: "set" };
  return { name: rig.library.profiles.find((p) => p.active)?.name ?? rig.library.profiles[0].name, from: "rig" };
}

/** The set's default profile; null: the rig's active one. */
export function setSetProfile(name: string | null) {
  change(`${currentSet(state).name} · default profile → ${name ?? "the rig's"}`, (s) => ({
    ...s,
    setlists: s.setlists.map((l, i) => (i === s.setIndex ? { ...l, profile: name ?? undefined } : l)),
    live: null,
    stackAt: {},
  }));
}

/** A song's own profile; null: it inherits the set's. */
export function setSongProfile(song: string, name: string | null) {
  change(`${song} · profile → ${name ?? "the set's"}`, (s) => {
    const songProfiles = { ...s.songProfiles };
    if (name) songProfiles[song] = name;
    else delete songProfiles[song];
    return { ...s, songProfiles, live: null, stackAt: {} };
  });
}

// ── Rig state the bars show ──────────────────────────────────────────

export type PerformMode = "preset" | "profile" | "setlist";

export function setPerformMode(mode: PerformMode) {
  move((s) => ({ ...s, performMode: mode }));
}

/** Mute the house, or bring it back. Not an edit: no undo. */
export function toggleHouseMute() {
  move((s) => ({ ...s, houseMute: !s.houseMute }));
}

/** Mute the house, mute fully (house + your guitar in the phones), or
 *  unmute. Not edits: no undo. */
export function setMutes(house: boolean, phones: boolean) {
  move((s) => ({ ...s, houseMute: house, phonesMute: phones }));
}

/** Panic: stop everything stuck — every note off on every MIDI channel,
 *  the audio engine stopped, its buffers and tails cleared, and started
 *  again. Mutes are left as they were. (The rig does this; here it is a
 *  second of "resetting".) */
export function panic() {
  const at = Date.now();
  move((s) => ({ ...s, panicAt: at, live: null }));
  window.setTimeout(() => move((s) => (s.panicAt === at ? { ...s, panicAt: null } : s)), 1200);
}

// ── Compose and Edit: what a section plays, and its own changes ──────

/** A part of a section of a song: what Compose and Edit work on. */
export interface Target {
  song: string;
  section: number;
  part: number;
}

export interface Override {
  /** Module → the module preset swapped in for this part. */
  modules: Record<string, string>;
  /** Edits made in Edit (param → value), over the patch, not yet saved. */
  edits: Record<string, number>;
}

export const targetKey = (t: Target) => `${t.song}|${t.section}|${t.part}`;

export function select(t: Target | null) {
  move((s) => ({ ...s, selection: t }));
}

export function overrideOf(s: State, t: Target | null): Override {
  return (t && s.overrides[targetKey(t)]) || { modules: {}, edits: {} };
}

function withOverride(s: State, t: Target, f: (o: Override) => Override): State {
  const k = targetKey(t);
  const next = f(s.overrides[k] ?? { modules: {}, edits: {} });
  const overrides = { ...s.overrides };
  if (Object.keys(next.modules).length || Object.keys(next.edits).length) overrides[k] = next;
  else delete overrides[k];
  return { ...s, overrides };
}

/** Swap a module preset in for this part (null: back to the patch's). */
export function setModuleOverride(t: Target, module: string, preset: string | null) {
  change(`${t.song} · ${module} → ${preset ?? "the patch's"}`, (s) =>
    withOverride(s, t, (o) => {
      const modules = { ...o.modules };
      if (preset) modules[module] = preset;
      else delete modules[module];
      return { ...o, modules };
    }),
  );
}

/** An edit to this part's sound, in Edit: kept as its override. */
export function editParam(t: Target, param: string, value: number) {
  move((s) => withOverride(s, t, (o) => ({ ...o, edits: { ...o.edits, [param]: value } })));
}

/** Write this part's edits back into the preset itself — everywhere it
 *  plays — and clear them from the part. */
export function saveEditsToPreset(t: Target, preset: string) {
  change(`${preset} ← ${t.song}'s edits`, (s) => withOverride(s, t, (o) => ({ ...o, edits: {} })));
}

/** Drop this part's edits: back to the preset. */
export function discardEdits(t: Target) {
  change(`${t.song} · edits dropped`, (s) => withOverride(s, t, (o) => ({ ...o, edits: {} })));
}

// ── Stacks: the profile's, as the player keeps them ─────────────────
// Five stacks — the five switches — named and ordered by the player; what
// changes is their names, their order, and the patches in each.

/** A profile's stacks: the player's edit, else the library's. */
export function profileStacksOf(s: State, profile: string): StackDef[] {
  return s.profileStacks[profile] ?? defaultStacks(profileNamed(profile));
}

/** The stacks a song plays through: its profile's (as edited), with the
 *  song's own patches and any its parts borrow. */
export function songStacks(s: State, song?: string) {
  const profile = profileOf(s, song).name;
  return stacksFor(song, borrowedOf(s, song), profile, profileStacksOf(s, profile));
}

function editStacks(profile: string, label: string, f: (d: StackDef[]) => StackDef[]) {
  change(`${profile} · ${label}`, (s) => ({ ...s, profileStacks: { ...s.profileStacks, [profile]: f(profileStacksOf(s, profile).map((d) => ({ ...d, patches: [...d.patches] }))) } }));
}

const moved = <T,>(list: T[], from: number, to: number) => {
  const out = [...list];
  if (to < 0 || to >= out.length) return out;
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
};

export function renameStack(profile: string, i: number, name: string) {
  editStacks(profile, `stack → ${name}`, (d) => d.map((x, k) => (k === i ? { ...x, name } : x)));
}
export function moveStack(profile: string, from: number, to: number) {
  editStacks(profile, "stack moved", (d) => moved(d, from, to));
}
export function addStackPatch(profile: string, i: number, name: string) {
  editStacks(profile, `${name} added`, (d) => d.map((x, k) => (k === i ? { ...x, patches: [...x.patches, name] } : x)));
}
export function renameStackPatch(profile: string, i: number, j: number, name: string) {
  editStacks(profile, `patch → ${name}`, (d) => d.map((x, k) => (k === i ? { ...x, patches: x.patches.map((p, n) => (n === j ? name : p)) } : x)));
}
export function removeStackPatch(profile: string, i: number, j: number) {
  editStacks(profile, "patch removed", (d) => d.map((x, k) => (k === i ? { ...x, patches: x.patches.filter((_, n) => n !== j) } : x)));
}
export function moveStackPatch(profile: string, i: number, from: number, to: number) {
  editStacks(profile, "patch moved", (d) => d.map((x, k) => (k === i ? { ...x, patches: moved(x.patches, from, to) } : x)));
}

/** Point the browser at a kind (its id), for what's being worked on. */
export function focusBrowser(kind: string | null) {
  if (state.browserFocus === kind) return;
  move((s) => ({ ...s, browserFocus: kind }));
}

/** Play a preset's variation (Preset mode): not an edit, no undo. */
export function playPreset(preset: string, variation: string) {
  move((s) => ({ ...s, presetUp: { preset, variation }, live: null }));
}

/** Drop everything a part carries of its own: the modules swapped in and
 *  Edit's unsaved changes — back to its preset as it is. */
export function clearOverrides(t: Target) {
  change(`${t.song} · own changes cleared`, (s) => {
    const overrides = { ...s.overrides };
    delete overrides[targetKey(t)];
    return { ...s, overrides };
  });
}

/** Drop every module or block swapped into a preset's variation. */
export function clearVariationPicks(preset: string, variation: string) {
  change(`${preset} · ${variation}: swaps cleared`, (s) => ({
    ...s,
    presetPicks: Object.fromEntries(Object.entries(s.presetPicks).filter(([k]) => !k.startsWith(`${preset}/${variation}/`))),
  }));
}

// ── Setup: guitars and rigs ────────────────────────────────────────────
// What the rig plays through is two things chosen apart: a guitar and a rig.
//
//   Guitar  a profile of its own: its photo, its pickups, its own input EQ,
//           and — per rig it has been set up on — the input trim that brings
//           it to the level presets expect, the five gate thresholds, and
//           whether it is noisy there. A gate's level depends on both the
//           guitar and the interface, so the guitar carries one per rig.
//   Rig     the audio interface (input, rate, buffer, outputs) and the MIDI
//           controller.
//
// Choose a guitar and a rig and it is set. (A song will later be able to
// ask for a different guitar.)

export type GateLevel = "off" | "subtle" | "default" | "tight" | "ultra";
export const GATE_LEVELS: GateLevel[] = ["off", "subtle", "default", "tight", "ultra"];

export interface Pickup {
  position: string;
  model: string;
}

export type Gates = Record<Exclude<GateLevel, "off">, number>;

/** How a guitar sits on one rig: measured there, kept with the guitar. */
export interface Fit {
  /** Input trim, dB: brings this guitar's peaks to the rig's target. */
  trimDb: number;
  /** Each gate preset's threshold, dBFS (off: none). */
  gates: Gates;
  /** Noisy input: Off plays Subtle and Subtle plays Default. */
  noisy: boolean;
}

export interface Guitar {
  id: string;
  name: string;
  /** A photo, served locally (public/guitars/, not committed). */
  image?: string;
  /** Its finish, for when the photo is missing. */
  colour: string;
  pickups: Pickup[];
  /** The guitar's own input EQ, before anything: a low cut (Hz) and three
   *  bands (dB). */
  eq: { lowCut: number; bass: number; mid: number; treble: number };
  /** Its fit on each rig it has been set up on, by rig id. */
  fits: Record<string, Fit>;
}

export interface Rig {
  id: string;
  name: string;
  audio: { device: string; input: string; rate: string; buffer: number; house: string; phones: string; targetDb: number };
  midi: { device: string; channel: number | "Omni" };
}

/** Gates set a step above a noise floor. */
export function gatesAbove(floor: number): Gates {
  return { subtle: floor + 5, default: floor + 10, tight: floor + 16, ultra: floor + 24 };
}

// Functions, hoisted: the store seeds itself before this point in the file.
function seedRigs(): Rig[] {
  return [
    {
      id: "minifuse",
      name: "MiniFuse 4 · Home",
      audio: { device: "Arturia MiniFuse 4", input: "Input 1 · Inst (Hi-Z)", rate: "48 kHz", buffer: 128, house: "Outputs 1–2", phones: "Phones 1", targetDb: -15 },
      midi: { device: "Morningstar MC8", channel: "Omni" },
    },
    {
      id: "stage",
      name: "Stage · Scarlett 2i2",
      audio: { device: "Focusrite Scarlett 2i2", input: "Input 1 · Inst (Hi-Z)", rate: "48 kHz", buffer: 64, house: "Outputs 1–2", phones: "Phones 1", targetDb: -15 },
      midi: { device: "Morningstar MC6", channel: 1 },
    },
  ];
}

function seedGuitars(): Guitar[] {
  const flat = { lowCut: 70, bass: 0, mid: 0, treble: 0 };
  return [
    {
      id: "strat",
      name: "TMG Strat",
      image: "/guitars/strat-white-gold.webp",
      colour: "#e9e4d8",
      pickups: [
        { position: "Bridge", model: "Seymour Duncan SH-PG1b Pearly Gates" },
        { position: "Middle", model: "Lawler Blonde" },
        { position: "Neck", model: "Lawler Blonde" },
      ],
      eq: flat,
      fits: {
        minifuse: { trimDb: 0, gates: gatesAbove(-77), noisy: false },
        stage: { trimDb: 2.5, gates: gatesAbove(-72), noisy: false },
      },
    },
    {
      id: "tele",
      name: "Blacked Out Tele",
      image: "/guitars/tele-black.webp",
      colour: "#18181b",
      pickups: [
        { position: "Bridge", model: "Tele single-coil" },
        { position: "Neck", model: "Tele single-coil" },
      ],
      eq: { lowCut: 80, bass: 1, mid: 0, treble: -1.5 },
      fits: { minifuse: { trimDb: 3, gates: gatesAbove(-70), noisy: true } },
    },
    {
      id: "es339",
      name: "Epiphone ES-339",
      image: "/guitars/es339-cherry.webp",
      colour: "#9f1d2b",
      pickups: [
        { position: "Bridge", model: "P-90" },
        { position: "Neck", model: "P-90" },
      ],
      eq: { lowCut: 90, bass: -1, mid: 0, treble: 0.5 },
      fits: { minifuse: { trimDb: 1.5, gates: gatesAbove(-68), noisy: true } },
    },
    {
      id: "goldtop",
      name: "Goldtop Les Paul",
      image: "/guitars/lp-goldtop.webp",
      colour: "#c9a24a",
      pickups: [
        { position: "Bridge", model: "P-90" },
        { position: "Neck", model: "P-90" },
      ],
      eq: { lowCut: 80, bass: -1.5, mid: 0, treble: 1 },
      fits: {},
    },
  ];
}

/** The fit to start a guitar on a rig it has not been set up on. */
export const UNFIT: Fit = { trimDb: 0, gates: gatesAbove(-74), noisy: false };

/** The gate a preset's level plays at. A noisy input lifts the light end
 *  only: Off plays Subtle, Subtle plays Default; the rest stay. */
export function gateFor(fit: Fit, level: GateLevel): GateLevel {
  if (!fit.noisy) return level;
  return level === "off" ? "subtle" : level === "subtle" ? "default" : level;
}

export function currentGuitar(s: State): Guitar {
  return s.guitars[s.guitarIndex] ?? s.guitars[0];
}
export function currentRig(s: State): Rig {
  return s.rigs[s.rigIndex] ?? s.rigs[0];
}
/** The guitar in use on the rig in use: its fit, if it has one there. */
export function currentFit(s: State): Fit | undefined {
  return currentGuitar(s).fits[currentRig(s).id];
}

function mapAt<T>(xs: T[], i: number, f: (x: T) => T) {
  return xs.map((x, k) => (k === i ? f(x) : x));
}

/** Edit the guitar in use (an undoable change). */
export function editGuitar(label: string, f: (g: Guitar) => Guitar) {
  change(`Guitar · ${label}`, (s) => ({ ...s, guitars: mapAt(s.guitars, s.guitarIndex, f) }));
}
/** Turning a value continuously (a slider): no undo step per move. */
export function tuneGuitar(f: (g: Guitar) => Guitar) {
  move((s) => ({ ...s, guitars: mapAt(s.guitars, s.guitarIndex, f) }));
}
/** Edit the guitar in use's fit on the rig in use (made if it has none). */
export function editFit(label: string, f: (x: Fit) => Fit) {
  change(`Fit · ${label}`, (s) => fitting(s, f));
}
export function tuneFit(f: (x: Fit) => Fit) {
  move((s) => fitting(s, f));
}
function fitting(s: State, f: (x: Fit) => Fit): State {
  const rig = currentRig(s).id;
  return { ...s, guitars: mapAt(s.guitars, s.guitarIndex, (g) => ({ ...g, fits: { ...g.fits, [rig]: f(g.fits[rig] ?? UNFIT) } })) };
}
export function editRig(label: string, f: (r: Rig) => Rig) {
  change(`Rig · ${label}`, (s) => ({ ...s, rigs: mapAt(s.rigs, s.rigIndex, f) }));
}

export function chooseGuitar(index: number) {
  change("Guitar chosen", (s) => ({ ...s, guitarIndex: index }));
}
export function chooseRig(index: number) {
  change("Rig chosen", (s) => ({ ...s, rigIndex: index }));
}

export function newGuitar(name: string) {
  change(`New guitar ${name}`, (s) => ({
    ...s,
    guitars: [...s.guitars, { id: `g${Date.now()}`, name, colour: "#71717a", pickups: [{ position: "Bridge", model: "" }], eq: { lowCut: 70, bass: 0, mid: 0, treble: 0 }, fits: {} }],
    guitarIndex: s.guitars.length,
  }));
}
export function newRig(name: string) {
  change(`New rig ${name}`, (s) => ({ ...s, rigs: [...s.rigs, { ...structuredClone(currentRig(s)), id: `r${Date.now()}`, name }], rigIndex: s.rigs.length }));
}
export function removeGuitar(index: number) {
  change("Guitar removed", (s) => {
    if (s.guitars.length <= 1) return s;
    const guitars = s.guitars.filter((_, i) => i !== index);
    return { ...s, guitars, guitarIndex: Math.min(s.guitarIndex > index ? s.guitarIndex - 1 : s.guitarIndex, guitars.length - 1) };
  });
}
export function removeRig(index: number) {
  change("Rig removed", (s) => {
    if (s.rigs.length <= 1) return s;
    const gone = s.rigs[index].id;
    const rigs = s.rigs.filter((_, i) => i !== index);
    // Its fits go with it.
    const guitars = s.guitars.map((g) => ({ ...g, fits: Object.fromEntries(Object.entries(g.fits).filter(([k]) => k !== gone)) }));
    return { ...s, rigs, guitars, rigIndex: Math.min(s.rigIndex > index ? s.rigIndex - 1 : s.rigIndex, rigs.length - 1) };
  });
}
