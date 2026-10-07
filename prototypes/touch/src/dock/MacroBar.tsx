// The macro bar, for fingers. The bank is the Signal app's
// (features/rigs/guitar/src/macros.rs): built from the chain by block type,
// in its order and colours — for this rig's chain, sixteen knobs (Input is
// left out: the chain has no Input block). Each knob is RELATIVE to the
// patch: it rests in the middle, where it changes nothing; right gives
// more of what the patch does, left less. So the fill grows from the
// centre and the number is the offset (+12, −8, 0 at rest).
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
interface Knob {
  id: string;
  label: string;
  colour: string;
  /** Sub-macros, in rows (one row per block for Delay and Reverb). */
  rows?: { head?: string; kids: Child[] }[];
}

const one = (kids: [string, string][]) => [{ kids: kids.map(([label, colour]) => ({ label, colour })) }];

const BANK: Knob[] = [
  { id: "gate", label: "Gate", colour: "#94A3B8", rows: one([["Threshold", "#CBD5E1"], ["Range", "#E2E8F0"], ["Attack", "#F1F5F9"], ["Release", "#CBD5E1"], ["Hold", "#CBD5E1"]]) },
  { id: "pre-comp", label: "Pre-Comp", colour: "#E5E7EB", rows: one([["Threshold", "#F3F4F6"], ["Ratio", "#E5E7EB"], ["Attack", "#D1D5DB"], ["Release", "#F9FAFB"]]) },
  { id: "pitch", label: "Pitch", colour: "#FACC15", rows: one([["Mix", "#FDE047"], ["Blend", "#FDE047"], ["Interval", "#FEF9C3"], ["Shimmer", "#FDE047"]]) },
  { id: "drive", label: "Drive", colour: "#F97316", rows: one([["King of Tone Red", "#FB923C"], ["Drive 1", "#F97316"], ["Drive 2", "#EF4444"], ["Drive 3", "#DC2626"]]) },
  { id: "gain", label: "Gain", colour: "#D6B36A" },
  { id: "tone", label: "Tone", colour: "#22C55E", rows: one([["Low", "#4ADE80"], ["Mid", "#86EFAC"], ["High", "#BBF7D0"]]) },
  { id: "comp", label: "Comp", colour: "#E5E7EB", rows: one([["Threshold", "#F3F4F6"], ["Ratio", "#E5E7EB"], ["Attack", "#D1D5DB"], ["Release", "#F9FAFB"]]) },
  { id: "mod", label: "Mod", colour: "#7DD3FC" },
  { id: "motion", label: "Motion", colour: "#EC4899" },
  { id: "boost", label: "Boost", colour: "#FAFAF9" },
  {
    id: "delay",
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
  { id: "space", label: "Space", colour: "#6366F1" },
  { id: "clarity", label: "Clarity", colour: "#2DD4BF", rows: one([["Duck", "#5EEAD4"], ["Thresh", "#2DD4BF"], ["Release", "#99F6E4"]]) },
  { id: "width", label: "Width", colour: "#A3E635" },
  { id: "output", label: "Output", colour: "#6B7280" },
];

/** Where each knob sits (0..1, rest 0.5), shared by the bar and its panels. */
type Values = Record<string, number>;

export function MacroBar() {
  const [values, setValues] = useState<Values>({ drive: 0.62, delay: 0.42, reverb: 0.58, width: 0.66 });
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
    <div ref={ref} style={{ position: "relative" }}>
      {knob?.rows && <Panel knob={knob} values={values} set={set} onClose={() => setOpen(null)} />}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(8, minmax(0, 1fr))", gap: 1, background: "#000" }}>
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
          />
        ))}
      </div>
    </div>
  );
}

function Panel({ knob, values, set, onClose }: { knob: Knob; values: Values; set: (id: string) => (v: number) => void; onClose: () => void }) {
  const cols = Math.max(...knob.rows!.map((r) => r.kids.length));
  const heads = knob.rows!.some((r) => r.head);
  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top: "100%",
        zIndex: 5,
        background: "#0d0d10",
        borderBottom: `2px solid ${knob.colour}`,
        boxShadow: "0 16px 32px rgba(0,0,0,0.55)",
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

/** One macro: the whole cell is the control. The fill grows from the
 *  centre (rest) toward the value; the number is the offset. A tap that
 *  doesn't move opens its sub-macros, when it has them. */
function Cell({ label, colour, value: v, onValue, more, open, onTap }: { label: string; colour: string; value: number; onValue: (v: number) => void; more?: boolean; open?: boolean; onTap?: () => void }) {
  const [active, setActive] = useState(false);
  const from = useRef<{ x: number; v: number; w: number; moved: boolean } | null>(null);
  const lastTap = useRef(0);
  const offset = Math.round((v - 0.5) * 200);
  const lo = Math.min(v, 0.5);
  const hi = Math.max(v, 0.5);
  const grey = /^#(?:6B7280|94A3B8|E5E7EB|FAFAF9|F3F4F6|D1D5DB|F9FAFB|E2E8F0|F1F5F9|CBD5E1)$/i.test(colour);
  return (
    <div
      role="slider"
      aria-label={label}
      aria-valuemin={-100}
      aria-valuemax={100}
      aria-valuenow={offset}
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
          onValue(0.5);
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
      title={`${label} — slide sideways; double-tap for rest${more ? "; tap for its sub-macros" : ""}`}
      style={{
        position: "relative",
        height: 44,
        overflow: "hidden",
        background: open ? "#1c1c22" : active ? "#18181d" : "#111114",
        boxShadow: open ? `inset 0 -2px 0 ${colour}` : undefined,
        touchAction: "none",
        userSelect: "none",
        cursor: "ew-resize",
        outline: "none",
      }}
    >
      {/* Rest, and the offset from it. */}
      <span aria-hidden style={{ position: "absolute", top: 8, bottom: 8, left: "50%", width: 1, background: "#2b2b31" }} />
      <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: `${lo * 100}%`, width: `${(hi - lo) * 100}%`, background: `color-mix(in oklab, ${colour} ${active ? 32 : 22}%, transparent)` }} />
      {offset !== 0 && <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${v * 100}% - 1px)`, width: 2, background: colour }} />}
      <span style={{ position: "relative", height: "100%", display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "0 8px" }}>
        <span className="t-label" style={{ minWidth: 0, display: "flex", alignItems: "center", gap: 3, fontSize: 10.5, letterSpacing: "0.06em", color: grey ? "var(--ink-2)" : colour, whiteSpace: "nowrap", overflow: "hidden" }}>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{label}</span>
          {more && (
            <svg width="8" height="5" viewBox="0 0 8 5" aria-hidden style={{ flexShrink: 0, transform: open ? "rotate(180deg)" : undefined, opacity: 0.8 }}>
              <path d="M1 1l3 3 3-3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
        </span>
        <span className="num" style={{ fontSize: active ? 16 : 13, lineHeight: 1, fontWeight: 700, color: offset === 0 ? "var(--ink-3)" : "var(--ink)", transition: "font-size 120ms var(--ease)" }}>
          {offset > 0 ? `+${offset}` : offset}
        </span>
      </span>
    </div>
  );
}
