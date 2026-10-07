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
import { songColour } from "../setlist/colors";

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

export function Switches() {
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
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: "8px 10px 10px", background: "#0a0a0c" }}>
      <MacroBar colour={song ? songColour(song, s.songColours) : "#94a3b8"} />
      {/* Row A: the hold layer. */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: 8 }}>
        {ambient ? (
          <HoldTile no={6} label={ambient.name} detail={ambient.patches[posOf(4)]?.name} colour={folder(ambient.name)[0]} lit={at?.stack === 4} onTap={() => tap(ambient, at?.stack === 4 ? posOf(4) : null)} />
        ) : (
          <HoldTile no={6} label="—" />
        )}
        <HoldTile no={7} label="FX Toggle" detail={fx ? "Time FX on" : "Time FX off"} colour="#a78bfa" lit={fx} onTap={() => setFx(!fx)} />
        <HoldTile no={8} label="Song" detail={song ?? "—"} colour={song ? songColour(song, s.songColours) : undefined} />
        <HoldTile no={9} label="Boost" detail={boost ? "+3 dB" : "off"} colour="#facc15" lit={boost} onTap={() => setBoost(!boost)} />
        <HoldTile no={10} label="Tuner" detail="A 440" colour="#e4e4e7" />
      </div>
      {/* Row B: the switches under the feet. */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: 8 }}>
        {[0, 1, 2, 3].map((i) =>
          main[i] ? (
            <StackTile key={main[i].name} no={i + 1} stack={main[i]} pos={posOf(i)} lit={at?.stack === i} onTap={() => tap(main[i], at?.stack === i ? posOf(i) : null)} />
          ) : (
            <div key={i} style={{ borderRadius: 12, border: "2px dashed var(--rule)", minHeight: 88 }} />
          ),
        )}
        <TapTempo bpm={currentSong(s)?.bpm || 120} />
      </div>
    </div>
  );
}

function tap(stack: SongStack, playingIndex: number | null) {
  tapStack(stack.name, stack.patches.map((p) => p.name), playingIndex);
}

function StackTile({ no, stack, pos, lit, onTap }: { no: number; stack: SongStack; pos: number; lit: boolean; onTap: () => void }) {
  const [bg, fg] = folder(stack.name);
  const patch = stack.patches[pos];
  return (
    <button
      onClick={onTap}
      aria-pressed={lit}
      style={{
        position: "relative",
        minHeight: 88,
        borderRadius: 12,
        padding: "18px 10px 10px",
        display: "flex",
        flexDirection: "column",
        justifyContent: "flex-end",
        gap: 4,
        textAlign: "left",
        background: lit ? bg : dim(bg, 0.22),
        color: lit ? fg : dim(bg, 0.95),
        boxShadow: lit ? "0 0 0 2px rgba(255,255,255,0.8), 0 10px 24px rgba(0,0,0,0.5)" : undefined,
        transition: "background 120ms var(--ease)",
      }}
    >
      <SwitchNo no={no} />
      <span className="t-label" style={{ fontSize: 11, opacity: 0.8 }}>
        {stack.name}
      </span>
      <span style={{ fontSize: 17, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{patch?.name ?? "—"}</span>
      <span style={{ display: "flex", gap: 4 }}>
        {stack.patches.map((p, k) => (
          <span key={p.name} style={{ width: k === pos ? 12 : 5, height: 5, borderRadius: 999, background: "currentColor", opacity: k === pos ? 0.95 : 0.35 }} />
        ))}
      </span>
    </button>
  );
}

function HoldTile({ no, label, detail, colour, lit, onTap }: { no: number; label: string; detail?: string; colour?: string; lit?: boolean; onTap?: () => void }) {
  const c = colour && colour.startsWith("#") ? colour : "#71717a";
  return (
    <button
      onClick={onTap}
      disabled={!onTap}
      aria-pressed={lit}
      style={{
        position: "relative",
        minHeight: 40,
        borderRadius: 9,
        padding: "0 10px 0 26px",
        display: "flex",
        alignItems: "center",
        gap: 8,
        textAlign: "left",
        background: lit ? dim(c, 0.4) : "#141418",
        boxShadow: lit ? `inset 0 0 0 1.5px ${c}` : "inset 0 0 0 1px #222228",
        color: "var(--ink)",
        cursor: onTap ? "pointer" : "default",
      }}
    >
      <SwitchNo no={no} top={13} />
      <span style={{ fontSize: 13, fontWeight: 700, whiteSpace: "nowrap" }}>{label}</span>
      {detail && <span style={{ fontSize: 12, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{detail}</span>}
    </button>
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
        minHeight: 88,
        borderRadius: 12,
        padding: "18px 10px 10px",
        display: "flex",
        flexDirection: "column",
        justifyContent: "flex-end",
        gap: 4,
        textAlign: "left",
        background: flash ? "#3f3f46" : "#1d1d22",
        color: "var(--ink)",
      }}
    >
      <SwitchNo no={5} />
      <span className="t-label" style={{ fontSize: 11, color: "var(--ink-3)" }}>
        Tap Tempo
      </span>
      <span className="num" style={{ fontSize: 22, fontWeight: 750 }}>
        {tempo}
        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--ink-3)", marginLeft: 4 }}>BPM</span>
      </span>
    </button>
  );
}

function SwitchNo({ no, top = 6 }: { no: number; top?: number }) {
  return <span style={{ position: "absolute", top, left: 9, fontSize: 10, fontFamily: "ui-monospace, monospace", opacity: 0.45 }}>{no}</span>;
}

// ── The macro bar ────────────────────────────────────────────────────

const MACROS: { label: string; colour?: string; value: number; unit?: string }[] = [
  { label: "Gain", colour: "#f97316", value: 0.55 },
  { label: "Bass", value: 0.5 },
  { label: "Mid", value: 0.6 },
  { label: "Treble", value: 0.45 },
  { label: "Delay", colour: "#a78bfa", value: 0.3 },
  { label: "Reverb", colour: "#22d3ee", value: 0.35 },
  { label: "Mod", colour: "#e879f9", value: 0.2 },
  { label: "Volume", colour: "#e4e4e7", value: 0.72 },
];

function MacroBar({ colour }: { colour: string }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: `repeat(${MACROS.length}, 1fr)`, borderRadius: 10, background: "#111114", boxShadow: "inset 0 0 0 1px #1d1d22" }}>
      {MACROS.map((m, i) => (
        <MacroCell key={m.label} {...m} colour={m.colour ?? colour} last={i === MACROS.length - 1} />
      ))}
    </div>
  );
}

function MacroCell({ label, colour, value: initial, unit, last }: { label: string; colour: string; value: number; unit?: string; last: boolean }) {
  const [v, setV] = useState(initial);
  const from = useRef<{ y: number; v: number } | null>(null);
  return (
    <div
      onPointerDown={(e) => {
        from.current = { y: e.clientY, v };
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (from.current) setV(Math.max(0, Math.min(1, from.current.v + (from.current.y - e.clientY) / 160)));
      }}
      onPointerUp={() => (from.current = null)}
      onDoubleClick={() => setV(initial)}
      title={`${label} — drag up or down; double-tap to reset`}
      style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 10px", minHeight: 48, borderRight: last ? undefined : "1px solid #1d1d22", touchAction: "none", cursor: "ns-resize", userSelect: "none" }}
    >
      <Knob value={v} colour={colour} />
      <span style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        <span className="t-label" style={{ fontSize: 10, letterSpacing: "0.08em", color: colour === "#e4e4e7" ? "var(--ink-2)" : colour }}>
          {label}
        </span>
        <span className="num" style={{ fontSize: 12, fontFamily: "ui-monospace, monospace", color: "var(--ink-2)", whiteSpace: "nowrap" }}>
          {Math.round(v * 100)}
          {unit ? ` ${unit}` : "%"}
        </span>
      </span>
    </div>
  );
}

/** A mini knob: a 270° arc track, the value arc in its colour, a pointer. */
function Knob({ value, colour }: { value: number; colour: string }) {
  const r = 12;
  const c = 16;
  const a0 = 135;
  const pt = (deg: number) => {
    const rad = (deg * Math.PI) / 180;
    return [c + r * Math.cos(rad), c + r * Math.sin(rad)];
  };
  const arc = (from: number, to: number) => {
    const [x0, y0] = pt(from);
    const [x1, y1] = pt(to);
    return `M${x0} ${y0} A${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${x1} ${y1}`;
  };
  const end = a0 + 270 * value;
  const [px, py] = pt(end);
  return (
    <svg width="32" height="32" viewBox="0 0 32 32" aria-hidden style={{ flexShrink: 0 }}>
      <path d={arc(a0, a0 + 270)} fill="none" stroke="#26262b" strokeWidth="3" strokeLinecap="round" />
      {value > 0.005 && <path d={arc(a0, end)} fill="none" stroke={colour} strokeWidth="3" strokeLinecap="round" />}
      <line x1={c} y1={c} x2={px} y2={py} stroke="var(--ink)" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
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
