// The stacks a song plays through: the profile's top-level stacks, every
// one of them always there, with the song's own patches put into them.
//
// A stack is a footswitch: tap it to play it, tap it again to step to its
// next patch. What a stack holds is the song's patches first (dialled in for
// this song) and then the profile's (passed through unchanged), so nothing
// the profile has is ever out of reach in a song.

import { rig, type ProfileEntry } from "../data/rig";

export type From = "song" | "profile";

export interface StackPatch {
  name: string;
  from: From;
}

export interface SongStack {
  name: string;
  patches: StackPatch[];
  /** Holds at least one of the song's own patches. */
  song: boolean;
}

/** The profile a song plays on: its own, else the one that is active. */
export function profileFor(song?: string): ProfileEntry {
  const own = rig.library.songs.find((x) => x.name === song)?.profile;
  const profiles = rig.library.profiles;
  return profiles.find((p) => p.name === own) ?? profiles.find((p) => p.active) ?? profiles[0];
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
 *  holding the song's patches then the profile's. */
export function stacksFor(song?: string): SongStack[] {
  const profile = profileFor(song);
  const names = topStacks(profile);
  const own = songPatches(song);
  const ownSet = new Set(own.map((p) => p.toLowerCase()));
  return names.map((name) => {
    const fromSong = own.filter((p) => placeOf(p, names) === name).map((p) => ({ name: p, from: "song" as const }));
    const fromProfile = profile.patch_list
      .filter((p) => p.stack === name && !ownSet.has(p.name.toLowerCase()))
      .map((p) => ({ name: p.name, from: "profile" as const }));
    return { name, patches: [...fromSong, ...fromProfile], song: fromSong.length > 0 };
  });
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
