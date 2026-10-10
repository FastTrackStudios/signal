// The menu: the app's MenuPanel, made for touch. Items are data — run one,
// name something in place, delete in two taps — and an item the rig would
// refuse is shown disabled with the reason. It opens from a ⋯ button or a
// long-press, anchored where it was asked for and kept on screen.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useStage } from "./stage";

export type MenuItem =
  | { kind: "head"; label: string }
  | { kind: "sep" }
  | { kind: "run"; id: string; label: string; detail?: string; disabled?: string; checked?: boolean }
  | { kind: "name"; id: string; label: string; initial: string; confirm: string; taken?: string[] }
  | { kind: "delete"; id: string; label: string; disabled?: string };

export interface Picked {
  id: string;
  text: string;
}

const W = 300;

/** A menu open at a point; closes on a pick, a tap outside or Escape. */
export function Menu({
  at,
  items,
  onPick,
  onClose,
  naming: startNaming,
}: {
  at: { x: number; y: number };
  items: MenuItem[];
  onPick: (p: Picked) => void;
  onClose: () => void;
  /** Open straight on this naming item (a "+ Section" that asks for a name). */
  naming?: number;
}) {
  const [naming, setNaming] = useState<number | null>(startNaming ?? null);
  const [armed, setArmed] = useState<number | null>(null);
  const first = startNaming !== undefined ? items[startNaming] : undefined;
  const [text, setText] = useState(first && first.kind === "name" ? first.initial : "");
  const ref = useRef<HTMLDivElement>(null);
  // `at` is in the window's pixels; the menu draws in the stage's.
  const stage = useStage();
  const local = (() => {
    const r = stage.el?.getBoundingClientRect();
    return r ? { x: (at.x - r.left) / stage.scale, y: (at.y - r.top) / stage.scale } : at;
  })();
  const bounds = { w: stage.el ? stage.el.offsetWidth : window.innerWidth, h: stage.el ? stage.el.offsetHeight : window.innerHeight };
  const [pos, setPos] = useState(local);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  // Keep the panel on screen: flip left of the point and above it when it
  // would run off the right or the bottom.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const h = r.height / stage.scale;
    const x = local.x + W > bounds.w - 8 ? Math.max(8, local.x - W) : local.x;
    const y = local.y + h > bounds.h - 8 ? Math.max(8, bounds.h - 8 - h) : local.y;
    setPos({ x, y });
  }, [at, naming]);

  const row = (on: boolean): React.CSSProperties => ({
    display: "flex",
    alignItems: "center",
    gap: 10,
    width: "100%",
    minHeight: 48,
    padding: "0 14px",
    borderRadius: "var(--r)",
    textAlign: "left",
    fontSize: 16,
    fontWeight: 560,
    background: on ? "var(--void)" : "transparent",
    color: on ? "#1a0505" : "var(--ink)",
  });

  let body: ReactNode;
  const item = naming !== null ? items[naming] : null;
  if (item && item.kind === "name") {
    const clash = item.taken?.some((t) => t.toLowerCase() === text.trim().toLowerCase() && t !== item.initial);
    const ok = text.trim().length > 0 && !clash;
    body = (
      <div style={{ padding: 10, display: "flex", flexDirection: "column", gap: 10 }}>
        <div className="t-label" style={{ color: "var(--ink-3)" }}>
          {item.label.replace(/…$/, "")}
        </div>
        <input
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && ok) {
              onPick({ id: item.id, text: text.trim() });
              onClose();
            }
          }}
          style={{ minHeight: 48, padding: "0 12px", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", fontSize: 17 }}
        />
        {clash && <div style={{ color: "var(--void)", fontSize: 14 }}>That name is taken.</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button className="pressable" onClick={onClose} style={{ minHeight: 44, padding: "0 16px", borderRadius: "var(--r)", border: "1px solid var(--rule-strong)", fontWeight: 600 }}>
            Cancel
          </button>
          <button
            disabled={!ok}
            onClick={() => {
              onPick({ id: item.id, text: text.trim() });
              onClose();
            }}
            style={{ minHeight: 44, padding: "0 18px", borderRadius: "var(--r)", background: ok ? "var(--primary)" : "transparent", border: `1px solid ${ok ? "var(--primary)" : "var(--rule-strong)"}`, color: ok ? "#fff" : "var(--ink-3)", fontWeight: 650 }}
          >
            {item.confirm}
          </button>
        </div>
      </div>
    );
  } else {
    body = items.map((it, i) => {
      if (it.kind === "sep") return <div key={i} style={{ height: 1, margin: "4px 6px", background: "var(--rule)" }} />;
      if (it.kind === "head")
        return (
          <div key={i} className="t-label" style={{ padding: "10px 14px 4px", color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {it.label}
          </div>
        );
      const off = "disabled" in it && !!it.disabled;
      const danger = it.kind === "delete";
      const isArmed = armed === i;
      return (
        <div key={i}>
          <button
            className={off || isArmed ? "" : "pressable"}
            disabled={off}
            onClick={() => {
              if (it.kind === "name") {
                setText(it.initial);
                setNaming(i);
              } else if (it.kind === "delete" && !isArmed) {
                setArmed(i);
              } else {
                onPick({ id: it.id, text: "" });
                onClose();
              }
            }}
            style={{ ...row(isArmed), color: isArmed ? "#1a0505" : off ? "var(--ink-3)" : danger ? "var(--void)" : "var(--ink)" }}
          >
            <span style={{ width: 14, display: "flex", justifyContent: "center" }}>
              {it.kind === "run" && it.checked && <span style={{ width: 8, height: 8, borderRadius: 999, background: "var(--live)" }} />}
            </span>
            <span style={{ flex: 1, minWidth: 0 }}>{isArmed ? "Tap again to delete" : it.label}</span>
            {it.kind === "run" && it.detail && <span className="t-meta">{it.detail}</span>}
          </button>
          {off && "disabled" in it && it.disabled && (
            <div style={{ padding: "0 14px 8px 38px", fontSize: 13, color: "var(--ink-3)", lineHeight: 1.35 }}>{it.disabled}</div>
          )}
        </div>
      );
    });
  }

  const layer = (
    <div style={{ position: stage.el ? "absolute" : "fixed", inset: 0, zIndex: 60 }} onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={ref}
        role="menu"
        style={{
          position: "absolute",
          left: pos.x,
          top: pos.y,
          width: W,
          maxHeight: bounds.h - 16,
          overflowY: "auto",
          padding: 4,
          background: "#0d0d10",
          border: "1px solid var(--rule-strong)",
          borderRadius: "var(--r-md)",
          boxShadow: "0 16px 40px rgba(0,0,0,0.7)",
        }}
      >
        {body}
      </div>
    </div>
  );
  return stage.el ? createPortal(layer, stage.el) : layer;
}

/** A ⋯ button that opens a menu under itself; a long-press on `target`
 *  (a whole row) opens the same one at the finger. */
export function useMenu() {
  const [open, setOpen] = useState<{ at: { x: number; y: number } } | null>(null);
  const fromButton = (e: React.MouseEvent) => {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setOpen({ at: { x: r.right - W, y: r.bottom + 4 } });
  };
  const longPress = () => {
    let timer = 0;
    let start: { x: number; y: number } | null = null;
    return {
      onPointerDown: (e: React.PointerEvent) => {
        start = { x: e.clientX, y: e.clientY };
        const at = { x: e.clientX, y: e.clientY };
        timer = window.setTimeout(() => setOpen({ at }), 500);
      },
      onPointerMove: (e: React.PointerEvent) => {
        if (start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > 10) window.clearTimeout(timer);
      },
      onPointerUp: () => window.clearTimeout(timer),
      onPointerCancel: () => window.clearTimeout(timer),
      onContextMenu: (e: React.MouseEvent) => {
        e.preventDefault();
        window.clearTimeout(timer);
        setOpen({ at: { x: e.clientX, y: e.clientY } });
      },
    };
  };
  return { open, setOpen, fromButton, longPress, close: () => setOpen(null) };
}

/** The ⋯ glyph, drawn. */
export function MoreButton({ onClick, label }: { onClick: (e: React.MouseEvent) => void; label: string }) {
  return (
    <button
      className="pressable"
      aria-label={label}
      onClick={onClick}
      onPointerDown={(e) => e.stopPropagation()}
      style={{ width: 48, height: 48, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--r)", color: "var(--ink-2)" }}
    >
      <svg width="20" height="4" viewBox="0 0 20 4" aria-hidden>
        <circle cx="2" cy="2" r="2" fill="currentColor" />
        <circle cx="10" cy="2" r="2" fill="currentColor" />
        <circle cx="18" cy="2" r="2" fill="currentColor" />
      </svg>
    </button>
  );
}
