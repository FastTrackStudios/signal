// The switches: the footswitch grid with the macro bar above it — a port of
// the Signal app's PerformGrid (features/rigs/guitar/ui/src/perform.rs) and
// MacroBar (macro_bar.rs), look and jobs kept.
//
//   macro bar   the rig's macro knobs, turned by a drag
//   row A       switches 6–10, the hold layer (a foot's hold lives "up" from
//               the toe): Ambient · FX Toggle · Song · Boost · Tuner
//   row B       switches 1–5: the profile's first four stacks, then Tap Tempo
//
// A stack switch plays its stack; tapped again it steps through it (the
// same `tapStack` the setlist's stack rows use). Its tile is the stack's
// colour, lit when it plays and dimmed toward the grid when not.

import { useRef, useState, type ReactNode } from "react";
import { borrowedOf, currentSong, playing, profileOf, tapStack, useStore } from "../store";
import { findPatch, stacksFor, type SongStack } from "../setlist/stacks";

/** perform::folder_color: a stack's tile and its text. */
const FOLDER: Record<string, [string, string]> = {
  clean: ["#38bdf8", "#082f49"],
  crunch: ["#2563eb", "#ffffff"],
  drive: ["#f97316", "#ffffff"],
  rhythm: ["#f97316", "#ffffff"],
  lead: ["#ef4444", "#ffffff"],
  ambient: ["#06b6d4", "#04222a"],
};
const folder = (name: string): [string, string] => FOLDER[name.toLowerCase()] ?? ["#3f3f46", "#e4e4e7"];

/** switches::dim — a colour darkened toward the grid's ground. */
function dim(hex: string, amount: number): string {
  const ch = (i: number) => parseInt(hex.slice(1 + i, 3 + i), 16);
  const base = [10, 10, 12];
  return `rgb(${[0, 2, 4].map((i, k) => Math.round(base[k] + (ch(i) - base[k]) * amount)).join(",")})`;
}

export function Switches({ vertical }: { vertical?: boolean } = {}) {
  const s = useStore();
  const song = currentSong(s)?.name;
  const stacks = stacksFor(song, borrowedOf(s, song), profileOf(s, song).name);
  const now = playing(s);
  const at = now ? findPatch(stacks, now) : null;
  const [fx, setFx] = useState(true);
  const [boost, setBoost] = useState(false);
  const main = stacks.slice(0, 4);
  const ambient = stacks[4];
  const posOf = (i: number) => (at?.stack === i ? at.index : (s.stackAt[stacks[i]?.name] ?? 0) % Math.max(1, stacks[i]?.patches.length ?? 1));
  const lit = (i: number) => at?.stack === i;
  const holds = [
    ambient ? (
      <StackTile key="h6" no={6} stack={ambient} pos={posOf(4)} lit={lit(4)} compact={!vertical} onTap={() => tap(ambient, lit(4) ? posOf(4) : null)} />
    ) : (
      <Empty key="h6" no={6} />
    ),
    <FnTile key="h7" no={7} title="FX Toggle" subtitle={fx ? "Time FX on" : "Time FX off"} bg="#ec4899" text="#ffffff" lit={fx} stacked={vertical} onTap={() => setFx(!fx)} />,
    <FnTile key="h8" no={8} title="Song" subtitle={song ?? "—"} bg="#a78bfa" text="#1e1b4b" lit={s.performMode === "setlist"} stacked={vertical} />,
    <FnTile key="h9" no={9} title="Boost" subtitle={boost ? "+3 dB" : "+3 dB · off"} bg="#fafafa" text="#0a0a0a" lit={boost} stacked={vertical} onTap={() => setBoost(!boost)} />,
    <FnTile key="h10" no={10} title="Tuner" subtitle="A 440" bg="#3f3f46" text="#e4e4e7" lit={false} stacked={vertical} />,
  ];
  const mains = [
    ...[0, 1, 2, 3].map((i) =>
      main[i] ? <StackTile key={`m${i}`} no={i + 1} stack={main[i]} pos={posOf(i)} lit={lit(i)} onTap={() => tap(main[i], lit(i) ? posOf(i) : null)} /> : <Empty key={`m${i}`} no={i + 1} />,
    ),
    <TapTempo key="m5" bpm={currentSong(s)?.bpm || 120} />,
  ];
  if (vertical) {
    // The phone upright: the board turned a quarter — a row per footswitch,
    // the switch under the foot wide on the left, its hold beside it on the
    // right. Rows share the height they are given.
    return (
      <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "minmax(0, 66fr) minmax(0, 34fr)", gridTemplateRows: "repeat(5, minmax(0, 1fr))", gap: 1, background: "#000" }}>
        {mains.flatMap((m, i) => [m, holds[i]])}
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", background: "#0a0a0c" }}>
      {/* One grid, two rows of five, the same columns: the hold layer slim
          above (a foot's hold lives "up" from the toe), the switches under
          the feet tall below — Signal's minmax(44px, 1fr) / 7fr.
          Flush: no padding, no gaps — a hairline of the ground between. */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(5, minmax(0, 1fr))", gridTemplateRows: "44px 124px", gap: 1, background: "#000", borderTop: "1px solid #000" }}>
        {holds}
        {mains}
      </div>
    </div>
  );
}

function tap(stack: SongStack, playingIndex: number | null) {
  tapStack(stack.name, stack.patches.map((p) => p.name), playingIndex);
}

/** The lit switch's ring, inside the tile: flush tiles have no room outside. */
const LIT_RING = "inset 0 0 0 2px rgba(255,255,255,0.85)";

/** A stack's switch: lit in its folder colour when it plays, dark when
 *  not. The stack name is the main sound; a variation gets a sub-label;
 *  dots say where the next press lands. */
function StackTile({ no, stack, pos, lit, compact, onTap }: { no: number; stack: SongStack; pos: number; lit: boolean; compact?: boolean; onTap: () => void }) {
  const [bg, fg] = folder(stack.name);
  const patch = stack.patches[pos];
  const variation = patch && patch.name.toLowerCase() !== stack.name.toLowerCase() ? patch.name : null;
  const dots = stack.patches.length > 1 && (
    <span style={{ display: "flex", alignItems: "center", gap: compact ? 4 : 6, marginTop: compact ? 0 : 4 }}>
      {stack.patches.map((p, k) => (
        <span key={p.name} style={{ width: compact ? 5 : 6, height: compact ? 5 : 6, borderRadius: 999, background: "currentColor", opacity: k === pos ? 0.95 : 0.35 }} />
      ))}
    </span>
  );
  return (
    <button
      onClick={onTap}
      aria-pressed={lit}
      style={{
        position: "relative",
        minWidth: 0,
        overflow: "hidden",
        borderRadius: 0,
        padding: "0 8px",
        display: "flex",
        flexDirection: compact ? "row" : "column",
        alignItems: "center",
        justifyContent: "center",
        gap: compact ? 8 : 4,
        background: lit ? bg : dim(bg, 0.24),
        color: lit ? fg : dim(fg, 0.35),
        boxShadow: lit ? LIT_RING : undefined,
      }}
    >
      <SwitchNo no={no} />
      <span style={{ fontSize: compact ? 14 : 24, fontWeight: 700, letterSpacing: "0.02em", whiteSpace: "nowrap" }}>{stack.name}</span>
      {variation && <span style={{ fontSize: compact ? 10 : 14, fontWeight: 600, opacity: compact ? 0.8 : 0.9, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: "100%" }}>{variation}</span>}
      {dots}
    </button>
  );
}

/** A function switch: a title and what it is doing, lit in its colour. */
function FnTile({ no, title, subtitle, bg, text, lit, stacked, onTap }: { no: number; title: string; subtitle: string; bg: string; text: string; lit: boolean; stacked?: boolean; onTap?: () => void }) {
  return (
    <button
      onClick={onTap}
      aria-pressed={lit}
      style={{
        position: "relative",
        minWidth: 0,
        overflow: "hidden",
        borderRadius: 0,
        padding: "0 8px",
        display: "flex",
        flexDirection: stacked ? "column" : "row",
        alignItems: "center",
        justifyContent: "center",
        gap: stacked ? 3 : 8,
        background: lit ? bg : dim(bg, 0.3),
        color: lit ? text : dim(text, 0.45),
        boxShadow: lit ? LIT_RING : undefined,
        cursor: onTap ? "pointer" : "default",
      }}
    >
      <SwitchNo no={no} />
      <span style={{ fontSize: 14, fontWeight: 700, letterSpacing: "0.02em", whiteSpace: "nowrap" }}>{title}</span>
      <span style={{ fontSize: 10, opacity: 0.8, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{subtitle}</span>
    </button>
  );
}

function Empty({ no }: { no: number }) {
  return (
    <div style={{ position: "relative", background: "#0e0e11" }}>
      <SwitchNo no={no} />
    </div>
  );
}

function TapTempo({ bpm }: { bpm: number }) {
  const [tempo, setTempo] = useState(bpm);
  const [flash, setFlash] = useState(false);
  const taps = useRef<number[]>([]);
  const onTap = () => {
    const t = performance.now();
    taps.current = [...taps.current.filter((x) => t - x < 2500), t].slice(-4);
    if (taps.current.length >= 2) {
      const gaps = taps.current.slice(1).map((x, i) => x - taps.current[i]);
      setTempo(Math.round(60000 / (gaps.reduce((a, b) => a + b, 0) / gaps.length)));
    }
    setFlash(true);
    window.setTimeout(() => setFlash(false), 90);
  };
  return (
    <button
      onClick={onTap}
      style={{
        position: "relative",
        borderRadius: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 2,
        background: flash ? "#52525b" : "#3f3f46",
        color: "#e4e4e7",
      }}
    >
      <SwitchNo no={5} />
      <span style={{ fontSize: 20, fontWeight: 700 }}>Tap Tempo</span>
      <span className="num" style={{ fontSize: 14, fontWeight: 600, opacity: 0.8 }}>{tempo} BPM</span>
    </button>
  );
}

function SwitchNo({ no }: { no: number }) {
  return <span style={{ position: "absolute", top: 5, left: 9, fontSize: 10, fontFamily: "ui-monospace, monospace", opacity: 0.45 }}>{no}</span>;
}

export function AudioControls(): ReactNode {
  return (
    <div style={{ height: 200, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 6, background: "#0a0a0c" }}>
      <span className="t-label" style={{ color: "var(--dim)" }}>
        Audio controls
      </span>
      <span style={{ fontSize: 13, color: "var(--dim)" }}>The block being played — where Frame goes</span>
    </div>
  );
}
