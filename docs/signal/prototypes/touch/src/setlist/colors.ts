// Colours for songs and sections, the way Session paints them.
//
// A song's colour is its name, encoded: the same name always lands on the
// same hue (FNV-1a over the lowercased name into a fixed palette), so a song
// is recognisable across sets, sessions and devices — unless the player
// gives it one by hand.
//
// A section's colour is its type, from the music catalog's UI palettes
// (music-catalog sections::ui_palettes, what Session's keyflow sections use):
// Intro sky, Verse emerald, Chorus blue, Bridge violet, Outro amber,
// Interlude yellow, Solo rose, Vamp lime, Tag purple, the rest slate; a Pre-
// or Post- section takes its parent's light shade.

/** The hues a song can be: Tailwind 400s, distinct on the app's dark ground,
 *  none of them the live green or the danger red. */
export const SONG_PALETTE = [
  "#38bdf8", // sky
  "#60a5fa", // blue
  "#818cf8", // indigo
  "#a78bfa", // violet
  "#c084fc", // purple
  "#e879f9", // fuchsia
  "#f472b6", // pink
  "#fb7185", // rose
  "#fb923c", // orange
  "#fbbf24", // amber
  "#facc15", // yellow
  "#a3e635", // lime
  "#2dd4bf", // teal
  "#22d3ee", // cyan
] as const;

/** FNV-1a, 32-bit — small, stable, and the same in Rust for the port. */
export function fnv1a(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** A song's colour from its name alone. */
export function nameColour(name: string): string {
  return SONG_PALETTE[fnv1a(name.trim().toLowerCase()) % SONG_PALETTE.length];
}

/** A song's colour: the player's, else its name's. */
export function songColour(name: string, chosen: Record<string, string>): string {
  return chosen[name] ?? nameColour(name);
}

const SECTION: [RegExp, string][] = [
  [/^intro/, "#38bdf8"], // sky-400
  [/^verse/, "#34d399"], // emerald-400
  [/^chorus|^refrain/, "#3b82f6"], // blue-500
  [/^bridge/, "#a78bfa"], // violet-400
  [/^outro|^ending/, "#fbbf24"], // amber-400
  [/^interlude/, "#facc15"], // yellow-400
  [/^instrumental/, "#34d399"], // emerald-400
  [/^solo/, "#fb7185"], // rose-400
  [/^vamp|^turnaround/, "#a3e635"], // lime-400
  [/^tag/, "#d8b4fe"], // purple-300
];
const LIGHT: Record<string, string> = {
  "#38bdf8": "#bae6fd",
  "#34d399": "#a7f3d0",
  "#3b82f6": "#bfdbfe",
  "#a78bfa": "#ddd6fe",
  "#fbbf24": "#fde68a",
  "#facc15": "#fef08a",
  "#fb7185": "#fecdd3",
  "#a3e635": "#d9f99d",
  "#d8b4fe": "#f3e8ff",
};
const SLATE = "#94a3b8";

/** A section's colour from its type, read off its name. */
export function sectionColour(name: string): string {
  const n = name.trim().toLowerCase();
  const pre = n.match(/^(pre|post)[\s-]?(.*)$/);
  if (pre) {
    const parent = SECTION.find(([re]) => re.test(pre[2]))?.[1];
    return parent ? LIGHT[parent] ?? parent : SLATE;
  }
  return SECTION.find(([re]) => re.test(n))?.[1] ?? SLATE;
}
