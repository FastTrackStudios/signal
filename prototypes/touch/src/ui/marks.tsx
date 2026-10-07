// The only hand-drawn marks in the world: a marker strike through what is
// done (a played song, a cancelled preset) and a marker circle round what is
// up now. Everything else is set type on a ruled sheet.

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

/** A marker loop round its parent's content: what is up now. */
export function Circle({ width = 2.5, inset = -6 }: { width?: number; inset?: number }) {
  return (
    <svg
      className="mark"
      viewBox="0 0 100 40"
      preserveAspectRatio="none"
      aria-hidden
      style={{ position: "absolute", inset, width: `calc(100% - ${inset * 2}px)`, height: `calc(100% - ${inset * 2}px)`, pointerEvents: "none", overflow: "visible" }}
    >
      <path
        d="M58 3 C 82 2, 98 9, 98 20 C 98 32, 76 38, 50 38 C 22 38, 2 32, 2 20 C 2 9, 22 3, 46 2.5 C 56 2, 66 3.5, 72 5"
        pathLength={1}
        style={{ strokeWidth: width }}
      />
    </svg>
  );
}

export const TAPE: Record<string, string> = {
  clean: "var(--tape-clean)",
  crunch: "var(--tape-crunch)",
  drive: "var(--tape-drive)",
  lead: "var(--tape-lead)",
  ambient: "var(--tape-ambient)",
  special: "var(--tape-special)",
};

/** A stack's tape colour; anything unknown is gaffer black. */
export function tapeFor(stack?: string): string {
  if (!stack) return "var(--tape-gaffer)";
  return TAPE[stack.toLowerCase()] ?? "var(--tape-gaffer)";
}

/** A strip of gaffer tape with a marker label on it. */
export function Tape({
  colour,
  children,
  tilt = 0,
  style,
}: {
  colour: string;
  children: ReactNode;
  tilt?: number;
  style?: CSSProperties;
}) {
  const dark = colour === "var(--tape-gaffer)";
  return (
    <span
      className="t-label"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 9px",
        background: colour,
        color: dark ? "#fff" : "var(--ink)",
        transform: tilt ? `rotate(${tilt}deg)` : undefined,
        whiteSpace: "nowrap",
        boxShadow: "inset 0 -1px 0 rgba(0,0,0,0.12)",
        ...style,
      }}
    >
      {children}
    </span>
  );
}
