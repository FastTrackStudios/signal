// The stacks a song plays through: the profile's top-level stacks, every
// one of them always there, with the song's own patches put into them.
//
// A stack is a footswitch: tap it to play it, tap it again to step to its
// next patch. What a stack holds is the song's patches first (dialled in for
// this song) and then the profile's (passed through unchanged), so nothing
// the profile has is ever out of reach in a song.

import { rig, type ProfileEntry } from "../data/rig";

/** Where a patch in a song's stack comes from: the song put it there, the
 *  song's profile passes it through, or it is borrowed from another
 *  profile (a part was given a patch the playing profile doesn't have). */
export type From = "song" | "profile" | "other";

export interface StackPatch {
  name: string;
  from: From;
  /** The profile it comes from (for "profile" and "other"). */
  profile?: string;
}

/** A patch a song's part borrows from another profile. */
export interface Borrowed {
  name: string;
  profile: string;
}

export interface SongStack {
  name: string;
  patches: StackPatch[];
  /** Holds at least one of the song's own patches. */
  song: boolean;
}

/** A profile by name, else the one the rig has active. */
export function profileNamed(name?: string): ProfileEntry {
  const profiles = rig.library.profiles;
  return profiles.find((p) => p.name === name) ?? profiles.find((p) => p.active) ?? profiles[0];
}

/** The profile a song plays on, when nothing in the set says otherwise:
 *  its own, else the one that is active. (The store's `profileOf` knows
 *  the set's default and the player's picks.) */
export function profileFor(song?: string, name?: string): ProfileEntry {
  return profileNamed(name ?? (rig.library.songs.find((x) => x.name === song)?.profile || undefined));
}

/** A song's own patches: the patches filed under the song's name. */
export function songPatches(song?: string): string[] {
  if (!song) return [];
  const k = song.toLowerCase();
  return rig.patches.filter((p) => p.stack.toLowerCase() === k).map((p) => p.name);
}

/** The stack a song patch goes into: the first stack its name says
 *  ("Ambient Delay Flute" → Ambient, "Verbed Lead" → Lead), else the last
 *  stack (a name that says no stack is most often a texture). */
export function placeOf(patch: string, stacks: string[]): string {
  const words = patch.toLowerCase().split(/\s+/);
  for (const w of words) {
    const hit = stacks.find((s) => s.toLowerCase() === w);
    if (hit) return hit;
  }
  return stacks[stacks.length - 1];
}

/** The profile's top-level stacks: the ones it fills (an empty slot, like
 *  Worship's Special, is not a stack you can play). */
export function topStacks(profile: ProfileEntry): string[] {
  return profile.stacks.filter((name) => profile.patch_list.some((p) => p.stack === name));
}

/** Every top-level stack of the song's profile, in the profile's order,
 *  holding the song's patches, then the profile's, then any its parts
 *  borrow from other profiles (in the stack they sit in there). */
export function stacksFor(song?: string, borrowed: Borrowed[] = [], profileName?: string): SongStack[] {
  const profile = profileFor(song, profileName);
  const names = topStacks(profile);
  const own = songPatches(song);
  const taken = new Set(own.map((p) => p.toLowerCase()));
  const mine = profile.patch_list.filter((p) => p.stack && !taken.has(p.name.toLowerCase()));
  for (const p of mine) taken.add(p.name.toLowerCase());
  const extra = borrowed.filter((b, i) => !taken.has(b.name.toLowerCase()) && borrowed.findIndex((x) => x.name === b.name) === i);
  const homeOf = (b: Borrowed) => {
    const st = rig.library.profiles.find((x) => x.name === b.profile)?.patch_list.find((x) => x.name === b.name)?.stack;
    return st && names.includes(st) ? st : placeOf(b.name, names);
  };
  return names.map((name) => {
    const fromSong = own.filter((p) => placeOf(p, names) === name).map((p) => ({ name: p, from: "song" as const }));
    const fromProfile = mine.filter((p) => p.stack === name).map((p) => ({ name: p.name, from: "profile" as const, profile: profile.name }));
    const fromOther = extra.filter((b) => homeOf(b) === name).map((b) => ({ name: b.name, from: "other" as const, profile: b.profile }));
    return { name, patches: [...fromSong, ...fromProfile, ...fromOther], song: fromSong.length > 0 };
  });
}

/** The other profiles' patches a song could borrow: everything they have
 *  in a stack that the song's own profile doesn't, by profile. */
export function borrowable(song?: string, profileName?: string): { profile: string; patches: { name: string; stack: string }[] }[] {
  const own = profileFor(song, profileName);
  const have = new Set(own.patch_list.map((p) => p.name.toLowerCase()));
  return rig.library.profiles
    .filter((p) => p.name !== own.name)
    .map((p) => ({ profile: p.name, patches: p.patch_list.filter((x) => x.stack && !have.has(x.name.toLowerCase())) }))
    .filter((g) => g.patches.length > 0);
}

/** Where a patch sits among a song's stacks. */
export function findPatch(stacks: SongStack[], patch: string): { stack: number; index: number } | null {
  const k = patch.toLowerCase();
  for (let i = 0; i < stacks.length; i++) {
    const j = stacks[i].patches.findIndex((p) => p.name.toLowerCase() === k);
    if (j >= 0) return { stack: i, index: j };
  }
  return null;
}
