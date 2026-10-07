// The chain as one strip, scrolled left and right: every block in signal
// order under its module, what it plays, and whether it is in. A tap opens
// the block's presets; the corner switch takes it in or out. A bypassed block
// is not hidden — it is struck, so the chain always reads whole.

import { useState } from "react";
import { blockPresetsByType, type ChainNode } from "../data/rig";
import { orderedChain, pickBlockPreset, toggleBypass, useStore } from "../store";
import { Strike } from "./marks";
import { SideSheet } from "./kit";

const TYPES = blockPresetsByType();

export function ChainStrip() {
  const s = useStore();
  const [open, setOpen] = useState<ChainNode | null>(null);
  const blocks = orderedChain(s);
  let lastModule = "";
  return (
    <footer
      style={{
        height: "var(--strip-h)",
        flexShrink: 0,
        background: "var(--sheet)",
        borderTop: "1px solid var(--rule-strong)",
        display: "flex",
        alignItems: "stretch",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          padding: "0 14px 0 18px",
          borderRight: "1px solid var(--rule)",
          flexShrink: 0,
        }}
      >
        <span className="t-label" style={{ color: "var(--ink-3)", writingMode: "vertical-rl", transform: "rotate(180deg)" }}>
          Chain
        </span>
      </div>
      <div style={{ flex: 1, minWidth: 0, overflowX: "auto", overflowY: "hidden", display: "flex", alignItems: "stretch" }}>
        {blocks.map(({ node, module }) => {
          const off = s.bypass[node.id];
          const firstOfModule = module !== lastModule;
          lastModule = module;
          const pick = s.blockPick[node.id] ?? node.presets[0]?.name ?? "";
          return (
            <div
              key={node.id}
              style={{
                display: "flex",
                flexDirection: "column",
                borderLeft: firstOfModule ? "1px solid var(--rule-strong)" : "1px solid var(--rule)",
                minWidth: 138,
                flexShrink: 0,
              }}
            >
              <div className="t-label" style={{ height: 24, padding: "5px 10px 0", fontSize: 13, color: "var(--ink-3)" }}>
                {firstOfModule ? module : ""}
              </div>
              <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
                <button
                  className="pressable"
                  onClick={() => setOpen(node)}
                  style={{ flex: 1, minWidth: 0, textAlign: "left", padding: "4px 6px 8px 10px", display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, opacity: off ? 0.55 : 1 }}
                >
                  <span style={{ position: "relative", fontWeight: 780, fontSize: 15, whiteSpace: "nowrap", alignSelf: "flex-start" }}>
                    {node.name.replace(/ · .*$/, "")}
                    {off && <Strike width={2.5} />}
                  </span>
                  <span className="t-meta" style={{ fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 150 }}>
                    {off ? "out" : pick || node.block_type}
                  </span>
                </button>
                <button
                  aria-label={off ? `Engage ${node.name}` : `Bypass ${node.name}`}
                  onClick={() => toggleBypass(node.id, node.name)}
                  style={{ width: 40, display: "flex", alignItems: "center", justifyContent: "center" }}
                >
                  <span
                    style={{
                      width: 14,
                      height: 14,
                      borderRadius: 999,
                      border: `2px solid ${off ? "var(--dim)" : "var(--live)"}`,
                      background: off ? "transparent" : "var(--live)",
                    }}
                  />
                </button>
              </div>
            </div>
          );
        })}
      </div>
      {open && (
        <BlockSheet node={open} pick={s.blockPick[open.id] ?? open.presets[0]?.name ?? ""} onClose={() => setOpen(null)} />
      )}
    </footer>
  );
}

export function BlockSheet({ node, pick, onClose }: { node: ChainNode; pick: string; onClose: () => void }) {
  const type = (node.block_type ?? "").toLowerCase();
  const fromLibrary = TYPES.get(type) ?? [];
  const names = fromLibrary.length > 0 ? fromLibrary.map((b) => b.name) : node.presets.map((p) => p.name);
  return (
    <SideSheet title={node.name.replace(/ · .*$/, "")} sub={`${node.block_type ?? "Block"} · ${names.length} presets`} onClose={onClose}>
      <div className="ruled">
        {names.map((n) => (
          <button
            key={n}
            className="pressable"
            onClick={() => pickBlockPreset(node.id, node.name, n)}
            style={{ width: "100%", minHeight: 56, padding: "0 22px", display: "flex", alignItems: "center", textAlign: "left", fontSize: 17, fontWeight: n === pick ? 820 : 560 }}
          >
            <span style={{ flex: 1 }}>{n}</span>
            {n === pick && <span className="t-label">Playing</span>}
          </button>
        ))}
      </div>
    </SideSheet>
  );
}
