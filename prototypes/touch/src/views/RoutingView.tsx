// Routing: the rig as a map. Modules are columns in signal order, their
// blocks stacked in them; the live signal path is drawn over everything in
// the one colour reserved for it, through the blocks that are in — a bypassed
// block stays on the map, struck, and the path goes round it. The amp pair
// splits the path into its two sides and joins them again.

import { useState } from "react";
import { chain, type ChainNode } from "../data/rig";
import { toggleBypass, useStore } from "../store";
import { Strike } from "../ui/marks";
import { Button, SideSheet } from "../ui/kit";

const COL_W = 176;
const COL_GAP = 46;
const NODE_H = 62;
const NODE_GAP = 12;
const TOP = 54;

interface Placed {
  node: ChainNode;
  module: string;
  x: number;
  y: number;
  side: "L" | "R" | null;
}

function layout(): { placed: Placed[]; columns: { module: string; x: number }[]; height: number } {
  const blocks = chain();
  const columns: { module: string; x: number }[] = [];
  const placed: Placed[] = [];
  const rowsIn = new Map<string, number>();
  for (const { node, module } of blocks) {
    if (columns[columns.length - 1]?.module !== module) columns.push({ module, x: 24 + columns.length * (COL_W + COL_GAP) });
    const col = columns[columns.length - 1];
    const row = rowsIn.get(module) ?? 0;
    rowsIn.set(module, row + 1);
    const side = /\bR\b|\[R\]|R$/.test(node.name) ? "R" : /\bL\b|\[L\]|L$/.test(node.name) ? "L" : null;
    placed.push({ node, module, x: col.x, y: TOP + row * (NODE_H + NODE_GAP), side });
  }
  const height = Math.max(...placed.map((p) => p.y + NODE_H)) + 40;
  return { placed, columns, height };
}

export function RoutingView() {
  const s = useStore();
  const { placed, columns, height } = layout();
  const [open, setOpen] = useState<ChainNode | null>(null);
  const width = columns.length * (COL_W + COL_GAP) + 24;

  // The live path: through every block that is in, splitting at the amp pair.
  const live = placed.filter((p) => !s.bypass[p.node.id]);
  const segments: string[] = [];
  const mid = (p: Placed) => ({ x: p.x + COL_W / 2, y: p.y + NODE_H / 2 });
  const left = live.filter((p) => p.side !== "R");
  const right = live.filter((p) => p.side === "R");
  const pathThrough = (pts: Placed[]) => {
    for (let i = 1; i < pts.length; i++) {
      const a = mid(pts[i - 1]);
      const b = mid(pts[i]);
      if (a.x === b.x) segments.push(`M${a.x} ${a.y} L${b.x} ${b.y}`);
      else {
        const cx = (a.x + b.x) / 2;
        segments.push(`M${a.x + COL_W / 2 - 8} ${a.y} C ${cx} ${a.y}, ${cx} ${b.y}, ${b.x - COL_W / 2 + 8} ${b.y}`);
      }
    }
  };
  pathThrough(left);
  if (right.length > 0) {
    // The right side leaves the block before it and rejoins the one after.
    const firstR = placed.indexOf(right[0]);
    const before = [...left].reverse().find((p) => placed.indexOf(p) < firstR);
    const lastR = placed.indexOf(right[right.length - 1]);
    const after = left.find((p) => placed.indexOf(p) > lastR);
    pathThrough([...(before ? [before] : []), ...right, ...(after ? [after] : [])]);
  }

  return (
    <div style={{ padding: 14, height: "100%", minHeight: 0 }}>
      <section className="sheet" style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
        <header style={{ display: "flex", alignItems: "center", gap: 14, padding: "14px 20px", borderBottom: "2px solid var(--ink)" }}>
          <h2 className="t-marker" style={{ margin: 0, fontSize: 30, flex: 1 }}>
            Routing
          </h2>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }} className="t-meta">
            <span style={{ width: 28, height: 4, background: "var(--path)", borderRadius: 2 }} /> the signal, through what's in
          </span>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }} className="t-meta">
            <span style={{ position: "relative", width: 28, height: 12 }}>
              <Strike width={2} />
            </span>
            bypassed
          </span>
        </header>
        <div style={{ flex: 1, minHeight: 0, overflow: "auto", backgroundImage: "linear-gradient(var(--rule) 1px, transparent 1px), linear-gradient(90deg, var(--rule) 1px, transparent 1px)", backgroundSize: "24px 24px", backgroundColor: "var(--sheet)" }}>
          <div style={{ position: "relative", width, height }}>
            {columns.map((c) => (
              <div key={`${c.module}-${c.x}`} className="t-label" style={{ position: "absolute", left: c.x, top: 18, width: COL_W, color: "var(--ink-2)" }}>
                {c.module}
              </div>
            ))}
            <svg width={width} height={height} style={{ position: "absolute", inset: 0, pointerEvents: "none" }} aria-hidden>
              {segments.map((d, i) => (
                <path key={i} d={d} fill="none" stroke="var(--path)" strokeWidth={4} strokeLinecap="round" />
              ))}
            </svg>
            {placed.map((p) => {
              const off = s.bypass[p.node.id];
              return (
                <button
                  key={p.node.id}
                  className="pressable"
                  onClick={() => setOpen(p.node)}
                  style={{
                    position: "absolute",
                    left: p.x,
                    top: p.y,
                    width: COL_W,
                    height: NODE_H,
                    background: "var(--sheet)",
                    border: off ? "1px dashed var(--rule-strong)" : "2px solid var(--ink)",
                    borderRadius: "var(--r)",
                    padding: "6px 12px",
                    textAlign: "left",
                    display: "flex",
                    flexDirection: "column",
                    justifyContent: "center",
                    gap: 2,
                    color: off ? "var(--ink-3)" : "var(--ink)",
                  }}
                >
                  <span style={{ position: "relative", alignSelf: "flex-start", fontSize: 15, fontWeight: 800, whiteSpace: "nowrap", maxWidth: COL_W - 24, overflow: "hidden", textOverflow: "ellipsis" }}>
                    {p.node.name.replace(/ · .*$/, "")}
                    {off && <Strike width={2} />}
                  </span>
                  <span className="t-meta" style={{ fontSize: 12 }}>
                    {p.side ? `${p.node.block_type} · ${p.side === "L" ? "left" : "right"}` : p.node.block_type}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </section>
      {open && (
        <SideSheet title={open.name.replace(/ · .*$/, "")} sub={`${open.block_type} · ${s.bypass[open.id] ? "bypassed" : "in the path"}`} onClose={() => setOpen(null)}>
          <div style={{ padding: 22, display: "flex", flexDirection: "column", gap: 14 }}>
            <Button primary onClick={() => toggleBypass(open.id, open.name)}>
              {s.bypass[open.id] ? "Put it in the path" : "Take it out of the path"}
            </Button>
            <p className="t-meta" style={{ margin: 0 }}>
              Its presets are in the chain strip below — tap the block there.
            </p>
          </div>
        </SideSheet>
      )}
    </div>
  );
}
