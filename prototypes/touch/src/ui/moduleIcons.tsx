// Module glyphs, as the Signal app draws them (crates/signal/grid-ui/src/
// icons.rs `module_paths`): one stroke on a 24-unit grid, in the colour of
// wherever they sit. Used to say which modules a part swaps in.

const PATHS: Record<string, string[]> = {
  // An amp head.
  amp: ["M3 7h18v12H3z", "M3 11h18", "M7 15h.01", "M11 15h.01"],
  // A speaker.
  volume: ["M11 5 6 9H3v6h3l5 4z", "M15.5 8.5a5 5 0 0 1 0 7"],
  // A bolt.
  drive: ["M13 2 4 14h7l-1 8 9-12h-7z"],
  // A clock.
  time: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M12 7v5l3 2"],
  // A turn.
  motion: ["M21 12a9 9 0 1 1-3-6.7", "M21 4v5h-5"],
  // A sparkle.
  special: ["M12 3v4", "M12 17v4", "M3 12h4", "M17 12h4", "M6 6l2.5 2.5", "M15.5 15.5 18 18", "M18 6l-2.5 2.5", "M8.5 15.5 6 18"],
  // EQ bars — a sound's own edits.
  edits: ["M6 20V10", "M12 20V4", "M18 20v-6"],
};

/** Each module's colour — the macros' for its effect (Drive red, Delay
 *  blue, Reverb violet…); a part's own edits stay amber. */
export const MODULE_COLOUR: Record<string, string> = { Core: "#D6B36A", Amp: "#f97316", Drive: "#ef4444", Time: "#6366F1", Delay: "#3B82F6", Reverb: "#8B5CF6", edits: "#f59e0b" };

/** The glyph for one of the browser's module kinds (or "edits"). */
const FOR: Record<string, string> = { Core: "amp", Amp: "volume", Drive: "drive", Time: "time", Delay: "motion", Reverb: "special", edits: "edits" };

export function ModuleIcon({ kind, size = 13, colour = "currentColor", title }: { kind: string; size?: number; colour?: string; title?: string }) {
  const paths = PATHS[FOR[kind] ?? kind.toLowerCase()] ?? ["M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"];
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" role={title ? "img" : undefined} aria-label={title} aria-hidden={title ? undefined : true} style={{ flexShrink: 0, display: "block", color: colour }}>
      {title && <title>{title}</title>}
      {paths.map((d, i) => (
        <path key={i} d={d} fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
      ))}
    </svg>
  );
}
