// The macro bar, for fingers. The bank is the Signal app's
// (features/rigs/guitar/src/macros.rs): built from the chain by block type,
// in its order and colours — for this rig's chain, sixteen knobs (Input is
// left out: the chain has no Input block). A knob sits where the patch puts
// it, so the bar shows what the patch is doing, and it reads one of three
// ways:
//
//   level     0–100%. 0 is off (bypassed); the patch sets where it starts —
//             a drive patch's Drive sits high, a clean one's at 0.
//             Gate, Pre-Comp, Pitch, Drive, Comp, Mod, Motion, Boost, Clarity.
//   wet       0–200%, 100% in the middle: up to the middle it brings the
//             effect in to its normal level; past it the dry signal falls
//             away until, at 200%, it is only the effect. Delay, Reverb,
//             Space (which starts in the middle).
//   relative  ± around the patch, resting in the middle where it changes
//             nothing. Gain, Tone, Width (mono ← → wide), Output.
//
// Two rows of eight flush cells. Slide sideways anywhere on a cell to turn
// it (relative to where the finger lands; a full sweep is 2.5 cells);
// double-tap puts it back to rest. A knob with sub-macros (▾) opens them on
// a tap — a panel rising over the main area, its children the same cells,
// in rows when the knob spans several blocks (Delay: DLY 1, DLY 2). The bar
// sits along the top of the main area, so the panel drops down over it.

import { useEffect, useRef, useState } from "react";

interface Child {
  label: string;
  colour: string;
}
type Scale = "level" | "wet" | "relative";

interface Knob {
  id: string;
  label: string;
  colour: string;
  scale: Scale;
  /** Where the patch puts it (0–1 of the knob's travel). */
  patch: number;
  /** Sub-macros, in rows (one row per block for Delay and Reverb). */
  rows?: { head?: string; kids: Child[] }[];
}

const one = (kids: [string, string][]) => [{ kids: kids.map(([label, colour]) => ({ label, colour })) }];

const BANK: Knob[] = [
  { id: "gate", scale: "level", patch: 0.4, label: "Gate", colour: "#94A3B8", rows: one([["Threshold", "#CBD5E1"], ["Range", "#E2E8F0"], ["Attack", "#F1F5F9"], ["Release", "#CBD5E1"], ["Hold", "#CBD5E1"]]) },
  { id: "pre-comp", scale: "level", patch: 0.35, label: "Pre-Comp", colour: "#E5E7EB", rows: one([["Threshold", "#F3F4F6"], ["Ratio", "#E5E7EB"], ["Attack", "#D1D5DB"], ["Release", "#F9FAFB"]]) },
  { id: "pitch", scale: "level", patch: 0, label: "Pitch", colour: "#FACC15", rows: one([["Mix", "#FDE047"], ["Blend", "#FDE047"], ["Interval", "#FEF9C3"], ["Shimmer", "#FDE047"]]) },
  { id: "drive", scale: "level", patch: 0.62, label: "Drive", colour: "#F97316", rows: one([["King of Tone Red", "#FB923C"], ["Drive 1", "#F97316"], ["Drive 2", "#EF4444"], ["Drive 3", "#DC2626"]]) },
  { id: "gain", scale: "relative", patch: 0.5, label: "Gain", colour: "#D6B36A" },
  { id: "tone", scale: "relative", patch: 0.5, label: "Tone", colour: "#22C55E", rows: one([["Low", "#4ADE80"], ["Mid", "#86EFAC"], ["High", "#BBF7D0"]]) },
  { id: "comp", scale: "level", patch: 0.45, label: "Comp", colour: "#E5E7EB", rows: one([["Threshold", "#F3F4F6"], ["Ratio", "#E5E7EB"], ["Attack", "#D1D5DB"], ["Release", "#F9FAFB"]]) },
  { id: "mod", scale: "level", patch: 0.2, label: "Mod", colour: "#7DD3FC" },
  { id: "motion", scale: "level", patch: 0, label: "Motion", colour: "#EC4899" },
  { id: "boost", scale: "level", patch: 0, label: "Boost", colour: "#FAFAF9" },
  {
    id: "delay",
    scale: "wet",
    patch: 0.15,
    label: "Delay",
    colour: "#3B82F6",
    rows: ["DLY 1", "DLY 2"].map((head) => ({
      head,
      kids: [
        { label: "Time", colour: "#60A5FA" },
        { label: "Feedback", colour: "#A5B4FC" },
        { label: "Filter", colour: "#BFDBFE" },
        { label: "Level", colour: "#DBEAFE" },
        { label: "Mod", colour: "#93C5FD" },
      ],
    })),
  },
  {
    id: "reverb",
    scale: "wet",
    patch: 0.25,
    label: "Reverb",
    colour: "#8B5CF6",
    rows: ["VERB 1", "VERB 2"].map((head) => ({
      head,
      kids: [
        { label: "Decay", colour: "#A78BFA" },
        { label: "Character", colour: "#C4B5FD" },
        { label: "Level", colour: "#EDE9FE" },
        { label: "Mod", colour: "#DDD6FE" },
      ],
    })),
  },
  { id: "space", scale: "wet", patch: 0.5, label: "Space", colour: "#6366F1" },
  { id: "clarity", scale: "level", patch: 0.3, label: "Clarity", colour: "#2DD4BF", rows: one([["Duck", "#5EEAD4"], ["Thresh", "#2DD4BF"], ["Release", "#99F6E4"]]) },
  { id: "width", scale: "relative", patch: 0.5, label: "Width", colour: "#A3E635" },
  { id: "output", scale: "relative", patch: 0.5, label: "Output", colour: "#6B7280" },
];

/** The bar's height with the rule under it: two 44pt rows and the hairline
 *  between them, plus 1. The setlist's header matches it. */
export const MACRO_BAR_H = 44 * 2 + 1 + 1;

/** Where each knob sits (0..1, rest 0.5), shared by the bar and its panels. */
type Values = Record<string, number>;

export function MacroBar({ cols = 8, cellH = 44, fill, up }: { cols?: number; cellH?: number; fill?: boolean; up?: boolean }) {
  // Each knob starts where the patch puts it.
  const [values, setValues] = useState<Values>(() => Object.fromEntries(BANK.map((k) => [k.id, k.patch])));
  const [open, setOpen] = useState<string | null>(null);
  const set = (id: string) => (v: number) => setValues((x) => ({ ...x, [id]: v }));
  const ref = useRef<HTMLDivElement>(null);
  // A tap anywhere outside the bar and its panel closes the panel.
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(null);
    };
    window.addEventListener("pointerdown", away);
    return () => window.removeEventListener("pointerdown", away);
  }, [open]);
  const knob = BANK.find((k) => k.id === open);
  return (
    // fill: the bar takes the height it is given, its rows sharing it.
    <div ref={ref} style={{ position: "relative", height: fill ? "100%" : undefined }}>
      {knob?.rows && <Panel knob={knob} values={values} set={set} up={up} onClose={() => setOpen(null)} />}
      <div style={{ display: "grid", gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, gridAutoRows: fill ? "minmax(0, 1fr)" : undefined, height: fill ? "100%" : undefined, gap: 1, background: "#000" }}>
        {BANK.map((k) => (
          <Cell
            key={k.id}
            label={k.label}
            colour={k.colour}
            value={values[k.id] ?? 0.5}
            onValue={set(k.id)}
            more={!!k.rows}
            open={open === k.id}
            onTap={k.rows ? () => setOpen(open === k.id ? null : k.id) : undefined}
            height={fill ? "100%" : cellH}
            scale={k.scale}
            rest={k.patch}
          />
        ))}
      </div>
    </div>
  );
}

/** `up`: the bar sits low (over the switches), so the panel rises above it. */
function Panel({ knob, values, set, up, onClose }: { knob: Knob; values: Values; set: (id: string) => (v: number) => void; up?: boolean; onClose: () => void }) {
  const cols = Math.max(...knob.rows!.map((r) => r.kids.length));
  const heads = knob.rows!.some((r) => r.head);
  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        ...(up ? { bottom: "100%", borderTop: `2px solid ${knob.colour}`, boxShadow: "0 -16px 32px rgba(0,0,0,0.55)" } : { top: "100%", borderBottom: `2px solid ${knob.colour}`, boxShadow: "0 16px 32px rgba(0,0,0,0.55)" }),
        zIndex: 5,
        background: "#0d0d10",
        animation: "macro-rise 160ms var(--ease) both",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, height: 36, padding: "0 6px 0 12px" }}>
        <span className="t-label" style={{ color: knob.colour }}>
          {knob.label}
        </span>
        <span style={{ flex: 1 }} />
        <button className="pressable" aria-label={`Close ${knob.label}`} onClick={onClose} style={{ width: 36, height: 36, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)", borderRadius: "var(--r)" }}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
            <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: `${heads ? "64px " : ""}repeat(${cols}, minmax(0, 1fr))`, gap: 1, background: "#000", borderTop: "1px solid #000" }}>
        {knob.rows!.map((row, r) => [
          heads && (
            <span key={`h${r}`} className="t-label" style={{ display: "flex", alignItems: "center", padding: "0 10px", fontSize: 10.5, whiteSpace: "nowrap", color: "var(--ink-3)", background: "#111114" }}>
              {row.head}
            </span>
          ),
          ...row.kids.map((c) => {
            const id = `${knob.id}/${row.head ?? ""}/${c.label}`;
            return <Cell key={id} label={c.label} colour={c.colour} value={values[id] ?? 0.5} onValue={set(id)} />;
          }),
          ...Array.from({ length: cols - row.kids.length }, (_, k) => <span key={`pad${r}-${k}`} style={{ background: "#111114" }} />),
        ])}
      </div>
      <style>{`@keyframes macro-rise { from { transform: translateY(-8px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

/** One macro: the whole cell is the control, drawn by its scale — a level
 *  fills from the left (0 is off); a wet knob fills from the left with its
 *  normal level marked in the middle and the stretch past it hatched (dry
 *  falling away); a relative knob fills from the centre (its rest). A
 *  double-tap goes back to where the patch put it. A tap that doesn't move
 *  opens its sub-macros, when it has them. */
function Cell({ label, colour, value: v, onValue, more, open, onTap, height = 44, scale = "relative", rest = 0.5 }: { label: string; colour: string; value: number; onValue: (v: number) => void; more?: boolean; open?: boolean; onTap?: () => void; height?: number | string; scale?: Scale; rest?: number }) {
  const [active, setActive] = useState(false);
  const from = useRef<{ x: number; v: number; w: number; moved: boolean } | null>(null);
  const lastTap = useRef(0);
  const offset = Math.round((v - 0.5) * 200);
  // The readout, by scale.
  const pct = Math.round(v * (scale === "wet" ? 200 : 100));
  const readout = scale === "relative" ? (offset > 0 ? `+${offset}` : `${offset}`) : pct === 0 ? "off" : scale === "wet" && pct === 200 ? "wet" : `${pct}%`;
  const quiet = scale === "relative" ? offset === 0 : pct === 0;
  const from0 = scale !== "relative";
  const lo = from0 ? 0 : Math.min(v, 0.5);
  const hi = from0 ? v : Math.max(v, 0.5);
  const grey = /^#(?:6B7280|94A3B8|E5E7EB|FAFAF9|F3F4F6|D1D5DB|F9FAFB|E2E8F0|F1F5F9|CBD5E1)$/i.test(colour);
  return (
    <div
      role="slider"
      aria-label={label}
      aria-valuemin={scale === "relative" ? -100 : 0}
      aria-valuemax={scale === "wet" ? 200 : 100}
      aria-valuenow={scale === "relative" ? offset : pct}
      aria-valuetext={readout}
      aria-expanded={more ? open : undefined}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight") onValue(Math.min(1, v + 0.01));
        if (e.key === "ArrowLeft") onValue(Math.max(0, v - 0.01));
        if (e.key === "Enter" && onTap) onTap();
      }}
      onPointerDown={(e) => {
        const now = performance.now();
        if (now - lastTap.current < 300) {
          onValue(rest);
          lastTap.current = 0;
          from.current = null;
          return;
        }
        lastTap.current = now;
        from.current = { x: e.clientX, v, w: (e.currentTarget as HTMLElement).clientWidth * 2.5, moved: false };
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const f = from.current;
        if (!f) return;
        if (!f.moved && Math.abs(e.clientX - f.x) < 4) return;
        if (!f.moved) setActive(true);
        f.moved = true;
        onValue(Math.max(0, Math.min(1, f.v + (e.clientX - f.x) / f.w)));
      }}
      onPointerUp={() => {
        const f = from.current;
        from.current = null;
        setActive(false);
        if (f && !f.moved && onTap) window.setTimeout(() => lastTap.current && onTap(), 0);
      }}
      onPointerCancel={() => {
        from.current = null;
        setActive(false);
      }}
      title={`${label} — slide sideways; double-tap for the patch's setting${more ? "; tap for its sub-macros" : ""}`}
      style={{
        position: "relative",
        height,
        overflow: "hidden",
        background: open ? "#1c1c22" : active ? "#18181d" : "#111114",
        boxShadow: open ? `inset 0 -2px 0 ${colour}` : undefined,
        touchAction: "none",
        userSelect: "none",
        cursor: "ew-resize",
        outline: "none",
      }}
    >
      {/* The middle: rest for a relative knob, the normal level for a wet one. */}
      {scale !== "level" && <span aria-hidden style={{ position: "absolute", top: scale === "wet" ? 4 : 8, bottom: scale === "wet" ? 4 : 8, left: "50%", width: 1, background: scale === "wet" ? "#45454d" : "#2b2b31" }} />}
      <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: `${lo * 100}%`, width: `${(Math.min(hi, scale === "wet" ? 0.5 : 1) - lo) * 100}%`, background: `color-mix(in oklab, ${colour} ${active ? 32 : 22}%, transparent)` }} />
      {/* A wet knob past its normal level: hatched, the dry falling away. */}
      {scale === "wet" && v > 0.5 && (
        <span
          aria-hidden
          style={{
            position: "absolute",
            top: 0,
            bottom: 0,
            left: "50%",
            width: `${(v - 0.5) * 100}%`,
            background: `repeating-linear-gradient(135deg, color-mix(in oklab, ${colour} ${active ? 46 : 36}%, transparent) 0 4px, color-mix(in oklab, ${colour} ${active ? 24 : 16}%, transparent) 4px 8px)`,
          }}
        />
      )}
      {!quiet && <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${v * 100}% - 1px)`, width: 2, background: colour }} />}
      <span style={{ position: "relative", height: "100%", display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "0 8px" }}>
        <span className="t-label" style={{ minWidth: 0, display: "flex", alignItems: "center", gap: 3, fontSize: 10.5, letterSpacing: "0.06em", color: grey ? "var(--ink-2)" : colour, whiteSpace: "nowrap", overflow: "hidden" }}>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{label}</span>
          {more && (
            <svg width="8" height="5" viewBox="0 0 8 5" aria-hidden style={{ flexShrink: 0, transform: open ? "rotate(180deg)" : undefined, opacity: 0.8 }}>
              <path d="M1 1l3 3 3-3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
        </span>
        <span className="num" style={{ fontSize: active ? 16 : 13, lineHeight: 1, fontWeight: 700, color: quiet ? "var(--ink-3)" : scale === "wet" && v > 0.5 ? colour : "var(--ink)", transition: "font-size 120ms var(--ease)" }}>
          {readout}
        </span>
      </span>
    </div>
  );
}
