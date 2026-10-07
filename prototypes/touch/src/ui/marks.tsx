// The marks the app uses for state: a strike through what is out or done (a
// bypassed block, a played song, a deleted preset), a bar under what is up,
// and the stack colours as tinted chips.

import type { CSSProperties, ReactNode } from "react";

/** A marker line through its parent's content, drawn once when it appears. */
export function Strike({ width = 3, tone = "var(--marker)" }: { width?: number; tone?: string }) {
  return (
    <svg
      className="mark"
      viewBox="0 0 100 10"
      preserveAspectRatio="none"
      aria-hidden
      style={{ position: "absolute", left: -4, right: -4, top: "50%", height: 12, marginTop: -6, width: "calc(100% + 8px)", pointerEvents: "none" }}
    >
      <path d="M1 6 C 20 4.5, 40 6.8, 60 5.2 S 90 4.6, 99 5.6" pathLength={1} style={{ strokeWidth: width, stroke: tone }} />
    </svg>
  );
}

/** What is up: a bar under its parent's content — green when it is what
 *  plays (the song up, the section playing), blue when it is only picked. */
export function Circle({ tone = "live" }: { width?: number; inset?: number; tone?: "live" | "focus" }) {
  return (
    <span
      aria-hidden
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        bottom: -5,
        height: 3,
        borderRadius: 2,
        background: tone === "live" ? "var(--live)" : "var(--focus-fg)",
        pointerEvents: "none",
      }}
    />
  );
}

export const TAPE: Record<string, string> = {
  clean: "var(--tape-clean)",
  crunch: "var(--tape-crunch)",
  drive: "var(--tape-drive)",
  lead: "var(--tape-lead)",
  ambient: "var(--tape-ambient)",
  special: "var(--tape-special)",
  // Rhythm (Metal, Rock): the drive orange's heavier neighbour.
  rhythm: "var(--tape-drive)",
};

/** A stack's tape colour; anything unknown is gaffer black. */
export function tapeFor(stack?: string): string {
  if (!stack) return "var(--tape-gaffer)";
  return TAPE[stack.toLowerCase()] ?? "var(--tape-gaffer)";
}

/** A stack's chip: its name on a tint of its colour, with a swatch — the
 *  app's patch chip. Gaffer means "this one" (pressed in); sheet is a plain
 *  outlined chip. */
export function Tape({
  colour,
  children,
  style,
}: {
  colour: string;
  children: ReactNode;
  tilt?: number;
  style?: CSSProperties;
}) {
  const pressed = colour === "var(--tape-gaffer)";
  const plain = colour === "var(--sheet)";
  return (
    <span
      className="t-label"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 9px",
        borderRadius: 4,
        letterSpacing: "0.06em",
        whiteSpace: "nowrap",
        color: pressed ? "#fafafa" : "var(--ink)",
        background: pressed
          ? "var(--pressed-bg)"
          : plain
            ? "transparent"
            : `color-mix(in srgb, ${colour} 20%, transparent)`,
        boxShadow: pressed ? "var(--pressed-shadow)" : undefined,
        border: plain ? "1px solid var(--rule-strong)" : undefined,
        ...style,
      }}
    >
      {!pressed && !plain && (
        <span style={{ width: 8, height: 8, borderRadius: 2, flexShrink: 0, background: colour }} />
      )}
      {children}
    </span>
  );
}

/** Where a patch comes from, as a mark the size of a letter: a note for the
 *  song's own, a stack of layers for a profile's (quiet for the profile the
 *  song plays on, in its own colour for one it borrows from). */
export function SourceIcon({ from, colour, size = 12 }: { from: "song" | "profile" | "other"; colour: string; size?: number }) {
  if (from === "song")
    return (
      <svg width={size} height={size} viewBox="0 0 12 12" aria-hidden style={{ flexShrink: 0, display: "block" }}>
        <path d="M4.6 2.2 10 1v6.6" fill="none" stroke={colour} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M4.6 2.2v6.6" fill="none" stroke={colour} strokeWidth="1.4" strokeLinecap="round" />
        <ellipse cx="3.1" cy="9.2" rx="1.9" ry="1.5" fill={colour} />
        <ellipse cx="8.5" cy="8" rx="1.9" ry="1.5" fill={colour} />
      </svg>
    );
  return (
    <svg width={size} height={size} viewBox="0 0 12 12" aria-hidden style={{ flexShrink: 0, display: "block" }}>
      <path d="M6 1.2 11 3.8 6 6.4 1 3.8Z" fill={from === "other" ? colour : "none"} stroke={colour} strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M1 6.2 6 8.8l5-2.6" fill="none" stroke={colour} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M1 8.4 6 11l5-2.6" fill="none" stroke={colour} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" opacity="0.6" />
    </svg>
  );
}
