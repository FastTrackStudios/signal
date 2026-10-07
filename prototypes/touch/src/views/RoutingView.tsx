// Routing: the rig as a map you work by hand. The signal runs from IN to OUT
// along one lane per row, left to right like a line of text, each row
// handing on to the next through the gutter. Every block that is in sits on
// the lane in the one colour kept for the live path; the amp pair splits
// into two lanes — left on the path, right just under it — and joins again.
// What is out of the path folds into one struck chip under its module, so
// the path never crosses it; a tap lists them to put back.
// Drag a block along the lane to move it in its module; hold it to put it on
// the amp pair's other side.

import { useLayoutEffect, useRef, useState } from "react";
import type { ChainNode } from "../data/rig";
import { moveBlock, orderedChain, setSide, sideOf, toggleBypass, useStore, type State } from "../store";
import { BlockSheet } from "../ui/ChainStrip";
import { Strike } from "../ui/marks";
import { Button, SideSheet } from "../ui/kit";

const CARD_W = 128;
const CARD_H = 60;
const CHIP_H = 48;
const OUT_LINE = 21;
const SLOT_GAP = 14;
const MOD_GAP = 20;
const HEAD = 26;
const LANE_GAP = 12;
const PAD = 20;
const TERM_W = 64;
const GUTTER = 20;

interface Module {
  name: string;
  /** In the path, in order; `side` R plays on the lower lane. */
  live: { node: ChainNode; side: "L" | "R" | null }[];
  out: ChainNode[];
  /** Slots along the lane (the two sides of the pair share their slots). */
  slots: number;
  w: number;
  split: boolean;
}

function modules(s: State): Module[] {
  const out: Module[] = [];
  for (const { node, module } of orderedChain(s)) {
    let m = out[out.length - 1];
    if (!m || m.name !== module) {
      m = { name: module, live: [], out: [], slots: 0, w: 0, split: false };
      out.push(m);
    }
    if (s.bypass[node.id]) m.out.push(node);
    else m.live.push({ node, side: sideOf(s, node.id, node.name) });
  }
  for (const m of out) {
    const right = m.live.filter((b) => b.side === "R").length;
    const main = m.live.length - right;
    m.split = right > 0;
    m.slots = Math.max(main, right);
    m.w = m.slots > 0 ? m.slots * CARD_W + (m.slots - 1) * SLOT_GAP : 120;
  }
  return out;
}

interface Placed {
  key: string;
  kind: "card" | "chip" | "in" | "out" | "head";
  node?: ChainNode;
  module: string;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  lane: "main" | "R";
  side?: "L" | "R" | null;
  out?: ChainNode[];
  order: number;
  slot?: number;
}

function layout(mods: Module[], width: number) {
  const placed: Placed[] = [];
  const rows: Module[][] = [[]];
  let used = PAD + TERM_W + MOD_GAP;
  for (const m of mods) {
    if (used + m.w > width - PAD - TERM_W - MOD_GAP && rows[rows.length - 1].length) {
      rows.push([]);
      used = PAD;
    }
    rows[rows.length - 1].push(m);
    used += m.w + MOD_GAP;
  }
  let top = 10;
  let order = 0;
  const rowLanes: { lane: number; bottom: number; right: number }[] = [];
  rows.forEach((row, r) => {
    const split = row.some((m) => m.split);
    const lane = top + HEAD;
    const chipsTop = lane + CARD_H + LANE_GAP + (split ? CARD_H + LANE_GAP : 0);
    let x = PAD;
    if (r === 0) {
      placed.push({ key: "in", kind: "in", module: "", label: "IN", x, y: lane, w: TERM_W, h: CARD_H, lane: "main", order: order++ });
      x += TERM_W + MOD_GAP;
    }
    for (const m of row) {
      placed.push({ key: `head-${m.name}-${x}`, kind: "head", module: m.name, label: m.name, x, y: top, w: m.w, h: HEAD, lane: "main", order: -1 });
      let main = 0;
      let right = 0;
      for (const b of m.live) {
        const isR = b.side === "R";
        const slot = isR ? right++ : main++;
        placed.push({
          key: b.node.id,
          kind: "card",
          node: b.node,
          module: m.name,
          label: b.node.name.replace(/ · .*$/, ""),
          x: x + slot * (CARD_W + SLOT_GAP),
          y: isR ? lane + CARD_H + LANE_GAP : lane,
          w: CARD_W,
          h: CARD_H,
          lane: isR ? "R" : "main",
          side: b.side,
          order: order++,
          slot,
        });
      }
      if (m.out.length) {
        placed.push({
          key: `out-${m.name}-${x}`,
          kind: "chip",
          module: m.name,
          label: m.out.map((n) => n.name.replace(/ · .*$/, "")).join(", "),
          x,
          // Under the lane, always: the path runs along the lane and never
          // through a block that is out of it.
          y: chipsTop,
          w: Math.max(m.w, 120),
          // A struck line per block: what is out stays readable on the map.
          h: Math.max(CHIP_H, 10 + m.out.length * OUT_LINE),
          lane: "main",
          out: m.out,
          order: -1,
        });
      }
      x += m.w + MOD_GAP;
    }
    const bottom = Math.max(chipsTop, ...placed.filter((p) => p.kind === "chip" && p.y >= top).map((p) => p.y + p.h));
    if (r === rows.length - 1) placed.push({ key: "out", kind: "out", module: "", label: "OUT", x, y: lane, w: TERM_W, h: CARD_H, lane: "main", order: order++ });
    rowLanes.push({ lane, bottom, right: x });
    top = bottom + GUTTER;
  });
  return { placed, height: top, rowLanes };
}

export function RoutingView() {
  const s = useStore();
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(1100);
  const [open, setOpen] = useState<ChainNode | null>(null);
  const [outOf, setOutOf] = useState<Placed | null>(null);
  const [drag, setDrag] = useState<{ id: string; dx: number } | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const mods = modules(s);
  const { placed, height, rowLanes } = layout(mods, width);

  // The live path, along the lanes.
  const path = placed.filter((p) => p.kind === "card" || p.kind === "in" || p.kind === "out");
  const main = path.filter((p) => p.lane === "main").sort((a, b) => a.order - b.order);
  const right = path.filter((p) => p.lane === "R").sort((a, b) => a.order - b.order);
  const segs: string[] = [];
  const midY = (p: Placed) => p.y + p.h / 2;
  const rowOf = (p: Placed) => rowLanes.findIndex((r) => p.y >= r.lane - 1 && p.y <= r.bottom);
  const link = (a: Placed, b: Placed) => {
    const ra = rowOf(a);
    const rb = rowOf(b);
    if (ra === rb || ra < 0 || rb < 0) {
      // Along a lane, or between the lanes of the pair.
      const x1 = a.x + a.w;
      const x2 = b.x;
      const y1 = midY(a);
      const y2 = midY(b);
      if (y1 === y2) segs.push(`M${x1} ${y1} H${x2}`);
      else {
        const mx = (x1 + x2) / 2;
        segs.push(`M${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`);
      }
      return;
    }
    // On to the next row: out the right end, down the margin to the gutter,
    // back along the gutter, down into the next lane's start.
    const g = rowLanes[ra].bottom + GUTTER / 2;
    const xr = Math.min(width - 8, a.x + a.w + 16);
    const y1 = midY(a);
    const y2 = midY(b);
    segs.push(`M${a.x + a.w} ${y1} H${xr - 8} Q ${xr} ${y1}, ${xr} ${y1 + 8} V${g - 8} Q ${xr} ${g}, ${xr - 8} ${g} H${8 + 8} Q 8 ${g}, 8 ${g + 8} V${y2 - 8} Q 8 ${y2}, 16 ${y2} H${b.x}`);
  };
  for (let i = 1; i < main.length; i++) link(main[i - 1], main[i]);
  if (right.length) {
    // The pair branches from the last block before it on the main path and
    // joins the first after it — Gate, in the same module.
    const before = [...main].reverse().find((p) => p.order < right[0].order && p.side !== "L");
    const after = main.find((p) => p.order > right[right.length - 1].order && p.side !== "L");
    const seq = [...(before ? [before] : []), ...right, ...(after ? [after] : [])];
    for (let i = 1; i < seq.length; i++) link(seq[i - 1], seq[i]);
  }

  const onDown = (p: Placed) => (e: React.PointerEvent) => {
    if (!p.node) return;
    const node = p.node;
    const x0 = e.clientX;
    let moved = false;
    const hold = window.setTimeout(() => {
      if (moved) return;
      const side = sideOf(s, node.id, node.name);
      setSide(node.id, node.name, side === "R" ? null : "R");
      cleanup();
    }, 550);
    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      if (Math.abs(dx) > 8) moved = true;
      if (moved) setDrag({ id: node.id, dx });
    };
    const onUp = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      cleanup();
      if (!moved) {
        setOpen(node);
        return;
      }
      const m = mods.find((x) => x.name === p.module);
      if (!m) return;
      const ids = orderedChain(s).filter((c) => c.module === p.module).map((c) => c.node.id);
      const from = ids.indexOf(node.id);
      const to = Math.max(0, Math.min(ids.length - 1, from + Math.round(dx / (CARD_W + SLOT_GAP))));
      if (to !== from) moveBlock(p.module, ids, from, to);
    };
    const cleanup = () => {
      window.clearTimeout(hold);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setDrag(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <div style={{ padding: 14, height: "100%", minHeight: 0 }}>
      <section className="sheet" style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
        <header style={{ display: "flex", alignItems: "center", gap: 18, padding: "10px 20px", borderBottom: "1px solid var(--rule-strong)", flexWrap: "wrap" }}>
          <h2 className="t-marker" style={{ margin: 0, fontSize: 28 }}>
            Routing
          </h2>
          <span className="t-meta" style={{ flex: 1, minWidth: 220 }}>
            Drag a block along the line to move it · hold it to put it on the amp pair's other side · tap for its presets
          </span>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }} className="t-meta">
            <span style={{ width: 28, height: 4, background: "var(--path)", borderRadius: 2 }} /> the signal
          </span>
        </header>
        <div
          ref={ref}
          style={{
            flex: 1,
            minHeight: 0,
            overflowY: "auto",
            overflowX: "hidden",
            backgroundImage: "linear-gradient(var(--rule) 1px, transparent 1px), linear-gradient(90deg, var(--rule) 1px, transparent 1px)",
            backgroundSize: "24px 24px",
            backgroundColor: "var(--sheet)",
          }}
        >
          <div style={{ position: "relative", width: "100%", height }}>
            <svg width={width} height={height} style={{ position: "absolute", inset: 0, pointerEvents: "none" }} aria-hidden>
              {segs.map((d, i) => (
                <path key={i} d={d} fill="none" stroke="var(--path)" strokeWidth={4} strokeLinecap="round" strokeLinejoin="round" />
              ))}
            </svg>
            {placed.map((p) => {
              if (p.kind === "head") {
                return (
                  <div key={p.key} className="t-label" style={{ position: "absolute", left: p.x, top: p.y + 4, width: Math.max(p.w, 90), color: "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {p.label}
                  </div>
                );
              }
              if (p.kind === "in" || p.kind === "out") {
                return (
                  <div
                    key={p.key}
                    className="t-label"
                    style={{ position: "absolute", left: p.x, top: p.y, width: p.w, height: p.h, display: "flex", alignItems: "center", justifyContent: "center", background: "var(--path)", color: "#052e16", borderRadius: 999, fontSize: 15 }}
                  >
                    {p.label}
                  </div>
                );
              }
              if (p.kind === "chip") {
                return (
                  <button
                    key={p.key}
                    className="pressable"
                    onClick={() => setOutOf(p)}
                    title={`Out of the path: ${p.label}`}
                    style={{ position: "absolute", left: p.x, top: p.y, width: p.w, height: p.h, padding: "5px 10px", border: "1px dashed var(--rule-strong)", borderRadius: "var(--r)", background: "var(--sheet)", textAlign: "left", color: "var(--ink-3)", display: "flex", flexDirection: "column", justifyContent: "center", gap: 0 }}
                  >
                    {p.out!.map((n) => (
                      <span key={n.id} style={{ position: "relative", alignSelf: "flex-start", maxWidth: "100%", fontSize: 13, fontWeight: 650, lineHeight: `${OUT_LINE}px`, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                        {n.name.replace(/ · .*$/, "")}
                        <Strike width={1.6} />
                      </span>
                    ))}
                  </button>
                );
              }
              const lifted = drag?.id === p.key;
              return (
                <button
                  key={p.key}
                  onPointerDown={onDown(p)}
                  style={{
                    position: "absolute",
                    left: p.x + (lifted ? drag!.dx : 0),
                    top: p.y,
                    width: p.w,
                    height: p.h,
                    zIndex: lifted ? 5 : 1,
                    touchAction: "none",
                    background: "var(--sheet)",
                    border: "1px solid var(--rule-strong)",
                    boxShadow: lifted ? "0 10px 24px -8px rgba(0,0,0,0.35)" : undefined,
                    borderRadius: "var(--r)",
                    padding: "6px 10px",
                    textAlign: "left",
                    display: "flex",
                    flexDirection: "column",
                    justifyContent: "center",
                    gap: 2,
                  }}
                >
                  <span style={{ maxWidth: "100%", fontSize: 14, fontWeight: 800, lineHeight: 1.12, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{p.label}</span>
                  <span className="t-meta" style={{ fontSize: 13 }}>
                    {p.node?.block_type}
                    {p.side ? ` · ${p.side === "R" ? "right" : "left"}` : ""}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </section>
      {open && <RoutingSheet node={open} onClose={() => setOpen(null)} />}
      {outOf && <OutSheet placed={outOf} onClose={() => setOutOf(null)} />}
    </div>
  );
}

function OutSheet({ placed, onClose }: { placed: Placed; onClose: () => void }) {
  const s = useStore();
  return (
    <SideSheet title={`${placed.module}: out of the path`} sub="Struck, not gone — put any back in" onClose={onClose}>
      <div className="ruled">
        {(placed.out ?? []).map((n) => {
          const off = s.bypass[n.id];
          return (
            <div key={n.id} style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 64, padding: "8px 22px" }}>
              <span style={{ position: "relative", flex: 1, fontSize: 17, fontWeight: 760, color: off ? "var(--ink-3)" : "var(--ink)" }}>
                <span style={{ position: "relative" }}>
                  {n.name.replace(/ · .*$/, "")}
                  {off && <Strike width={2} />}
                </span>
              </span>
              <Button primary={off} onClick={() => toggleBypass(n.id, n.name)}>
                {off ? "Put it in" : "Take it out"}
              </Button>
            </div>
          );
        })}
      </div>
    </SideSheet>
  );
}

function RoutingSheet({ node, onClose }: { node: ChainNode; onClose: () => void }) {
  const s = useStore();
  const [presets, setPresets] = useState(false);
  const side = sideOf(s, node.id, node.name);
  if (presets) return <BlockSheet node={node} pick={s.blockPick[node.id] ?? node.presets[0]?.name ?? ""} onClose={onClose} />;
  return (
    <SideSheet title={node.name.replace(/ · .*$/, "")} sub={`${node.block_type} · ${s.bypass[node.id] ? "out of the path" : "in the path"}`} onClose={onClose}>
      <div style={{ padding: 22, display: "flex", flexDirection: "column", gap: 12 }}>
        <Button primary onClick={() => toggleBypass(node.id, node.name)}>
          {s.bypass[node.id] ? "Put it in the path" : "Take it out of the path"}
        </Button>
        <h3 style={{ margin: "10px 0 0", fontSize: 17, fontWeight: 820 }}>Where it plays</h3>
        <div style={{ display: "flex", gap: 8 }}>
          {([null, "L", "R"] as const).map((v) => (
            <Button key={String(v)} primary={side === v} onClick={() => setSide(node.id, node.name, v)} style={{ flex: 1 }}>
              {v === null ? "Main path" : v === "L" ? "Left side" : "Right side"}
            </Button>
          ))}
        </div>
        <Button onClick={() => setPresets(true)} style={{ marginTop: 10 }}>
          Its presets
        </Button>
      </div>
    </SideSheet>
  );
}
