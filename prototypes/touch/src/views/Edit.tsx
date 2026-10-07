// Edit: the sound of the picked part, taken apart. Routing along the top of
// the main area — the block and module system, to be remade — and the FX
// row along the foot: the selected block's controls (Frame goes here), at
// the landscape iPhone's safe area (750 × 381 pt), so one component serves
// the iPad's main area and fills a phone on its side.
//
// Edits are the picked part's own: an override over its preset, marked as
// such, until Save to preset writes them into the preset (everywhere it
// plays) or Discard drops them.

import { useEffect, useState } from "react";
import { blockPresetsByType, chain } from "../data/rig";
import { discardEdits, editParam, focusBrowser, overrideOf, saveEditsToPreset, sectionsOf, useStore } from "../store";

const MODULE_KINDS = ["Core", "Amp", "Drive", "Time", "Delay", "Reverb"];

/** The browser kind for a block: its block presets when there are any,
 *  else its module's presets, else the patches. */
function kindFor(block?: { node: { block_type: string | null }; module: string }): string {
  const t = block?.node.block_type?.toLowerCase();
  if (t && blockPresetsByType().has(t)) return `block:${t}`;
  const m = MODULE_KINDS.find((k) => block?.module.toLowerCase().includes(k.toLowerCase()));
  return m ? `module:${m}` : "patches";
}
import { sectionColour } from "../setlist/colors";

export const FX_ROW = { w: 750, h: 381 };

/** `fill`: no routing above, the FX row takes all the height (a phone). */
export function EditView({ routing = true, fill }: { routing?: boolean; fill?: boolean }) {
  const blocks = chain();
  const [blockId, setBlockId] = useState<string>(blocks.find((b) => b.node.block_type === "Drive")?.node.id ?? blocks[0]?.node.id);
  const block = blocks.find((b) => b.node.id === blockId);
  // The browser follows the block being edited.
  const kind = kindFor(block);
  useEffect(() => {
    focusBrowser(kind);
  }, [kind]);
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
      {routing && <Routing blockId={blockId} onBlock={setBlockId} />}
      <OverrideBar />
      <FxRow name={block?.node.name ?? "—"} module={block?.module ?? ""} fill={fill} />
    </div>
  );
}

/** Routing — a stand-in until the block and module system is remade: the
 *  chain's modules in signal order, their blocks inside, one picked. */
function Routing({ blockId, onBlock }: { blockId?: string; onBlock: (id: string) => void }) {
  const groups: { module: string; blocks: ReturnType<typeof chain> }[] = [];
  for (const b of chain()) {
    const g = groups[groups.length - 1];
    if (g && g.module === b.module) g.blocks.push(b);
    else groups.push({ module: b.module, blocks: [b] });
  }
  return (
    <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: 12, display: "flex", flexDirection: "column", gap: 8 }}>
      <span className="t-label" style={{ color: "var(--ink-3)" }}>
        Routing <span style={{ fontWeight: 500, textTransform: "none", letterSpacing: 0 }}>— the block and module system, to be remade</span>
      </span>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6, alignContent: "flex-start" }}>
        {groups.map((g, i) => (
          <div key={`${i}-${g.module}`} style={{ display: "flex", flexDirection: "column", gap: 4, padding: 6, borderRadius: "var(--r-md)", background: "#141418", boxShadow: "inset 0 0 0 1px var(--rule)" }}>
            <span className="t-label" style={{ fontSize: 11, color: "var(--ink-3)", padding: "0 4px" }}>
              {g.module}
            </span>
            <div style={{ display: "flex", gap: 4 }}>
              {g.blocks.map((b) => {
                const on = b.node.id === blockId;
                return (
                  <button
                    key={b.node.id}
                    onClick={() => onBlock(b.node.id)}
                    className={on ? "" : "pressable"}
                    style={{ height: 44, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: on ? 700 : 560, whiteSpace: "nowrap", color: b.node.bypassed ? "var(--ink-3)" : "var(--ink)", background: on ? "var(--pressed-bg)" : "#1b1b20", boxShadow: on ? "var(--pressed-shadow), inset 0 -2px 0 var(--ink-2)" : undefined }}
                  >
                    {b.node.name}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Whose edits these are, and the way to keep them for good. */
function OverrideBar() {
  const s = useStore();
  const t = s.selection;
  const sec = t ? sectionsOf(s, t.song)[t.section] : undefined;
  const part = sec && t ? sec.parts[t.part] : undefined;
  const edits = Object.keys(overrideOf(s, t).edits).length;
  const preset = part?.sound?.name ?? null;
  return (
    <div style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 10, minHeight: 48, padding: "0 6px 0 14px", borderTop: "1px solid var(--rule)", background: edits ? "rgba(245,158,11,0.08)" : "var(--sheet)" }}>
      {!t || !sec ? (
        <span style={{ fontSize: 13.5, color: "var(--ink-3)" }}>Pick a section in the setlist — what you change here becomes its own.</span>
      ) : (
        <>
          <span style={{ fontSize: 14, fontWeight: 750, color: sectionColour(sec.name), whiteSpace: "nowrap" }}>{sec.name}</span>
          {sec.parts.length > 1 && <span style={{ fontSize: 13.5, fontWeight: 650, whiteSpace: "nowrap" }}>{part?.name}</span>}
          <span style={{ fontSize: 13, color: edits ? "var(--modified)" : "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {edits ? `${edits} change${edits > 1 ? "s" : ""} — this section's own` : `plays ${preset ?? "what came before"}`}
          </span>
          <span style={{ flex: 1 }} />
          {edits > 0 && (
            <>
              <button className="pressable" onClick={() => discardEdits(t)} style={{ height: 36, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 650, color: "var(--ink-2)" }}>
                Discard
              </button>
              <button
                className="pressable"
                disabled={!preset}
                onClick={() => preset && saveEditsToPreset(t, preset)}
                title={preset ? `Write these changes into ${preset}, everywhere it plays` : "This part has no patch of its own"}
                style={{ height: 36, padding: "0 14px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, color: "var(--ink)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}
              >
                Save to {preset ?? "preset"}
              </button>
            </>
          )}
        </>
      )}
    </div>
  );
}

const PARAMS = ["Drive", "Tone", "Level", "Bass", "Mid", "Treble"];

/** The FX row: the picked block's controls — Frame's place. Here, a row of
 *  stand-in faders that make edits, so the override flow can be felt. */
function FxRow({ name, module, fill }: { name: string; module: string; fill?: boolean }) {
  const s = useStore();
  const t = s.selection;
  const edits = overrideOf(s, t).edits;
  return (
    <div style={{ flex: fill ? 1 : undefined, flexShrink: 0, minHeight: 0, height: fill ? undefined : FX_ROW.h, display: "flex", flexDirection: "column", borderTop: "1px solid #000", background: "#0d0d10" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, padding: "12px 16px 6px" }}>
        <span style={{ fontSize: 17, fontWeight: 750 }}>{name}</span>
        <span className="t-meta" style={{ fontSize: 13 }}>
          {module}
        </span>
        <span style={{ flex: 1 }} />
        <span className="t-label" style={{ color: "var(--dim)" }}>
          FX row · Frame goes here
        </span>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: `repeat(${PARAMS.length}, minmax(0, 1fr))`, gap: 1, background: "#000", margin: "0 0 0 0" }}>
        {PARAMS.map((p) => {
          const key = `${name}:${p}`;
          const v = edits[key] ?? 0.5;
          const changed = edits[key] !== undefined;
          return (
            <label key={p} style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 10, background: "#111114", opacity: t ? 1 : 0.5 }}>
              <span className="t-label" style={{ color: changed ? "var(--modified)" : "var(--ink-3)" }}>
                {p}
              </span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={v}
                disabled={!t}
                onChange={(e) => t && editParam(t, key, Number(e.target.value))}
                style={{ writingMode: "vertical-lr", direction: "rtl", height: fill ? 120 : 180, accentColor: changed ? "#f59e0b" : "#a1a1aa" }}
              />
              <span className="num" style={{ fontSize: 14, fontWeight: 700, color: changed ? "var(--ink)" : "var(--ink-3)" }}>
                {Math.round(v * 100)}
              </span>
            </label>
          );
        })}
      </div>
    </div>
  );
}
