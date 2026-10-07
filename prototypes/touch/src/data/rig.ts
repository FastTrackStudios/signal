// The rig as the prototype reads it: the real fixture exported from the
// engine (`cargo run -p signal-guitar --example export_fixture`), typed to
// the shapes the Dioxus remote reads over the wire, plus the few joins every
// screen needs. Nothing here invents data except the example profiles
// marked below; edits live in the store.

import raw from "./fixture.json";

export interface ModulePick {
  module: string;
  preset: string;
  snapshot: string;
  blocks: string[];
}

export interface SnapshotInfo {
  modules: ModulePick[];
  blocks: { block: string; preset: string }[];
  captures: string[];
}

/** A module preset: a Preset (a whole sound), a Core, a Time, a Delay… */
export interface ModulePreset {
  module: string;
  name: string;
  snapshots: string[];
  used_by: string[];
  snapshot_info: SnapshotInfo[];
}

export interface BlockPreset {
  block_type: string;
  name: string;
  bypass: boolean;
  used_by: string[];
}

export interface PerfPart {
  name: string;
  patch: string;
  section: string;
  profile: string;
  repeat_of: string;
  switch_count: number;
}

export interface SongSlot {
  name: string;
  key: string;
  bpm: number;
  start: string;
}

export interface Setlist {
  name: string;
  active: boolean;
  songs: SongSlot[];
}

export interface SongEntry {
  name: string;
  key: string;
  bpm: number;
  parts: string[];
  setlists: string[];
  profile: string;
  start_part: string;
}

export interface ProfileEntry {
  name: string;
  active: boolean;
  stacks: string[];
  patches: number;
  patch_list: { name: string; stack: string }[];
}

export interface PerfStack {
  name: string;
  current_patch: string;
  preset: string;
  patches: string[];
  is_active: boolean;
}

export interface Patch {
  name: string;
  stack: string;
  active: boolean;
  rig_preset: string;
  variation: string;
  default_in_stack: boolean;
  override_modules: string[];
}

export interface ChainNode {
  id: string;
  name: string;
  role: string;
  depth: number;
  is_block: boolean;
  block_type: string | null;
  bypassed: boolean;
  presets: { id: string; name: string }[];
}

interface Fixture {
  perf: {
    profile_name: string;
    stacks: PerfStack[];
    tempo_bpm: number;
    songs: SongSlot[];
    song_index: number;
    setlists: string[];
    setlist_index: number;
    parts: PerfPart[];
    part_index: number;
  };
  library: {
    profiles: ProfileEntry[];
    songs: SongEntry[];
    setlists: Setlist[];
  };
  compositions: {
    modules: ModulePreset[];
    block_presets: BlockPreset[];
    active_modules: ModulePick[];
  };
  patches: Patch[];
  nodes: ChainNode[];
}

export const rig = raw as unknown as Fixture;

// Example profiles, for the prototype only — not in the rig's fixture. They
// give the profile picker a realistic spread: few stacks or many, thin
// stacks and full ones.
const EXAMPLE_PROFILES: Record<string, Record<string, string[]>> = {
  Funk: {
    Clean: ["Spank Clean", "Quack Clean", "Chicken Scratch"],
    Crunch: ["Edge Crunch", "Fat Crunch"],
    Lead: ["Funk Lead", "Octave Lead"],
    Special: ["Envelope Filter", "Talk Box"],
  },
  Jazz: {
    Clean: ["Archtop", "Warm Neck", "Bright Bridge"],
    Lead: ["Horn Lead", "Octave Lead"],
    Ambient: ["Room", "Hall"],
  },
  MkGee: {
    Clean: ["Tape Clean", "Chorus Crush", "Wobble"],
    Crunch: ["Blown Combo", "Cassette Crunch"],
    Drive: ["Fuzz Smear"],
    Ambient: ["Warble Wash", "Reverse Bloom"],
    Special: ["Bit Crush", "Ring Mod"],
  },
  Indie: {
    Clean: ["Jangle", "Tremolo Clean"],
    Crunch: ["Jazzmaster Crunch", "Edge of Breakup"],
    Drive: ["Big Muff", "Shoegaze Wall"],
    Lead: ["Fuzz Lead"],
    Ambient: ["Shimmer"],
  },
  Experimental: {
    Clean: ["Prepared"],
    Drive: ["Gated Fuzz", "Octave Fuzz", "Bit Fuzz"],
    Ambient: ["Granular", "Freeze", "Reverse"],
    Special: ["Glitch", "Ring Mod", "Pitch Chaos", "Feedback Loop"],
  },
  Soundscape: {
    Clean: ["Volume Swell", "Ebow Clean"],
    Ambient: ["Swell Pad", "Shimmer Pad", "Infinite Hold", "Cloud", "Tape Drift"],
    Special: ["Freeze", "Drone"],
  },
};

for (const [name, stacks] of Object.entries(EXAMPLE_PROFILES)) {
  if (rig.library.profiles.some((p) => p.name === name)) continue;
  const patch_list = Object.entries(stacks).flatMap(([stack, patches]) => patches.map((p) => ({ name: p, stack })));
  rig.library.profiles.push({ name, active: false, stacks: Object.keys(stacks), patches: patch_list.length, patch_list });
}

/** The modules a sound is made of, in the order a player thinks of them. */
export const MODULE_KINDS = ["Preset", "Core", "Amp", "Drive", "Time", "Delay", "Reverb"] as const;
export type ModuleKind = (typeof MODULE_KINDS)[number];

export function modulesOf(kind: string): ModulePreset[] {
  return rig.compositions.modules.filter((m) => m.module === kind);
}

/** A preset's recipe: the Core, Time and block picks it plays. */
export function recipe(p: ModulePreset): SnapshotInfo | undefined {
  return p.snapshot_info[0];
}

export function blockPresetsByType(): Map<string, BlockPreset[]> {
  const out = new Map<string, BlockPreset[]>();
  for (const b of rig.compositions.block_presets) {
    const list = out.get(b.block_type) ?? [];
    list.push(b);
    out.set(b.block_type, list);
  }
  return out;
}

/** The stack a patch sits in: the playing profile's stacks, then its
 *  patch list (a song's own patches carry their stack there), then any
 *  profile that has it. */
export function stackOf(patch: string): string | undefined {
  const k = patch.toLowerCase();
  // A song's own patch lists the song as its "stack": not a stack.
  const real = (st?: string) => (st && rig.perf.stacks.some((s) => s.name === st) ? st : undefined);
  return (
    rig.perf.stacks.find((s) => s.patches.some((p) => p.toLowerCase() === k))?.name ??
    real(rig.patches.find((p) => p.name.toLowerCase() === k)?.stack) ??
    real(rig.library.profiles.flatMap((p) => p.patch_list).find((p) => p.name.toLowerCase() === k)?.stack) ??
    guessStack(k)
  );
}

/** A song's own patch, named after its sound ("Dry Chorus Clean L",
 *  "Ambient Delay Flute"): the stack its name says it is. A guess, for the
 *  tape colour only. */
function guessStack(name: string): string | undefined {
  const stacks = rig.perf.stacks.map((s) => s.name);
  return stacks.find((s) => name.split(/\s+/).includes(s.toLowerCase()));
}

/** The chain's blocks in signal order, with the module each sits under. */
export function chain(): { node: ChainNode; module: string }[] {
  const out: { node: ChainNode; module: string }[] = [];
  let module = "";
  for (const n of rig.nodes) {
    if (!n.is_block && n.depth === 1) module = n.name;
    if (n.is_block) out.push({ node: n, module });
  }
  return out;
}
