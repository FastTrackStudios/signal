// Sounds: making and tweaking what plays. On the left every preset (and the
// Core, Time and block presets they're built from); on the right the one
// picked — its variations as taped tabs, its recipe as three big rows (Core,
// Time, blocks) each changed with one tap, and who plays it.

import { useState } from "react";
import { blockPresetsByType, modulesOf, recipe, type ModulePreset } from "../data/rig";
import {
  addVariation,
  cancelPreset,
  newPreset,
  renamePreset,
  setVariationPick,
  useStore,
  type PresetState,
} from "../store";
import { Circle, Strike, Tape } from "../ui/marks";
import { Button, SideSheet, Tabs } from "../ui/kit";

const KINDS = ["Presets", "Core", "Time", "Drive", "Delay", "Reverb", "Blocks"] as const;
type Kind = (typeof KINDS)[number];
const MODULE_OF: Record<Kind, string> = {
  Presets: "Preset",
  Core: "Core",
  Time: "Time",
  Drive: "Drive",
  Delay: "Delay",
  Reverb: "Reverb",
  Blocks: "",
};
const BLOCKS = blockPresetsByType();

export function SoundsView() {
  const s = useStore();
  const [kind, setKind] = useState<Kind>("Presets");
  const [pick, setPick] = useState<string>(s.presets[0]?.name ?? "");
  const [naming, setNaming] = useState<string | null>(null);

  const counts: Record<Kind, number> = {
    Presets: s.presets.filter((p) => !p.cancelled).length,
    Core: modulesOf("Core").length,
    Time: modulesOf("Time").length,
    Drive: modulesOf("Drive").length,
    Delay: modulesOf("Delay").length,
    Reverb: modulesOf("Reverb").length,
    Blocks: [...BLOCKS.values()].reduce((n, l) => n + l.length, 0),
  };

  const rows: { name: string; sub: string; cancelled?: boolean }[] =
    kind === "Presets"
      ? s.presets.map((p) => ({ name: p.name, sub: subOfPreset(p), cancelled: p.cancelled }))
      : kind === "Blocks"
        ? [...BLOCKS.entries()].map(([type, list]) => ({ name: cap(type), sub: `${list.length} presets` }))
        : modulesOf(MODULE_OF[kind]).map((m) => ({ name: m.name, sub: `${m.snapshots.length} variations · ${m.used_by.length} play it` }));

  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(320px, 34%) 1fr", gap: 14, padding: 14, height: "100%", minHeight: 0 }}>
      <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ padding: "12px 10px 10px", borderBottom: "2px solid var(--ink)" }}>
          <Tabs
            options={KINDS.map((k) => ({ id: k, label: k, count: counts[k] }))}
            value={kind}
            wrap
            onChange={(k) => {
              setKind(k);
              const first =
                k === "Presets" ? s.presets[0]?.name : k === "Blocks" ? cap([...BLOCKS.keys()][0] ?? "") : modulesOf(MODULE_OF[k])[0]?.name;
              setPick(first ?? "");
            }}
          />
        </div>
        <div className="ruled" style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
          {rows.map((r) => {
            const on = r.name === pick;
            return (
              <button
                key={r.name}
                className="pressable"
                onClick={() => setPick(r.name)}
                style={{
                  width: "100%",
                  minHeight: 64,
                  padding: "8px 16px 8px 18px",
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  textAlign: "left",
                  background: on ? "#fffbe0" : undefined,
                }}
              >
                <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
                  <span style={{ position: "relative", alignSelf: "flex-start", fontSize: 18, fontWeight: on ? 860 : 720, color: r.cancelled ? "var(--ink-3)" : undefined }}>
                    {r.name}
                    {r.cancelled && <Strike tone="var(--void)" />}
                    {on && !r.cancelled && <Circle inset={-7} />}
                  </span>
                  <span className="t-meta" style={{ fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {r.cancelled ? "Deleted — Undo or Restore brings it back" : r.sub}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
        {kind === "Presets" && (
          <footer style={{ display: "flex", gap: 8, padding: 12, borderTop: "1px solid var(--rule)" }}>
            {naming === null ? (
              <Button primary style={{ flex: 1 }} onClick={() => setNaming("")}>
                New preset from what plays
              </Button>
            ) : (
              <>
                <input
                  autoFocus
                  value={naming}
                  onChange={(e) => setNaming(e.target.value)}
                  placeholder="Name it — e.g. Washed Pad"
                  style={{ flex: 1, minWidth: 0, minHeight: 48, padding: "0 12px", border: "2px solid var(--ink)", borderRadius: "var(--r)", fontSize: 16 }}
                />
                <Button
                  primary
                  disabled={!naming.trim() || s.presets.some((p) => p.name === naming.trim())}
                  onClick={() => {
                    newPreset(naming.trim());
                    setPick(naming.trim());
                    setNaming(null);
                  }}
                >
                  Save
                </Button>
              </>
            )}
          </footer>
        )}
      </section>
      {kind === "Presets" ? (
        <PresetPage preset={s.presets.find((p) => p.name === pick)} onRenamed={setPick} />
      ) : kind === "Blocks" ? (
        <BlockTypePage type={pick.toLowerCase()} />
      ) : (
        <ModulePage module={modulesOf(MODULE_OF[kind]).find((m) => m.name === pick)} />
      )}
    </div>
  );
}

function subOfPreset(p: PresetState): string {
  const r = recipe(p.source);
  const core = r?.modules.find((m) => m.module === "Core");
  const time = r?.modules.find((m) => m.module === "Time");
  return [core && `${core.preset} ${core.snapshot}`, time && `${time.preset} ${time.snapshot}`].filter(Boolean).join(" · ");
}

function cap(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// ── A preset ─────────────────────────────────────────────────────────

/** What a preset's variation plays in `module`: its own pick, else Main's
 *  pick, else the recipe the rig holds for it. */
function usePickOf(preset: PresetState) {
  const s = useStore();
  const main = preset.variations[0] ?? "Main";
  const fromRecipe = (variation: number, module: string) => {
    const info = preset.source.snapshot_info[variation] ?? preset.source.snapshot_info[0];
    if (module.startsWith("block:")) return info?.blocks.find((b) => b.block === module.slice(6))?.preset ?? "—";
    const m = info?.modules.find((x) => x.module === module);
    return m ? `${m.preset} · ${m.snapshot}` : "—";
  };
  return (variation: number, module: string): string => {
    const v = preset.variations[variation] ?? main;
    return (
      s.presetPicks[`${preset.name}/${v}/${module}`] ??
      (variation > 0 ? s.presetPicks[`${preset.name}/${main}/${module}`] : undefined) ??
      fromRecipe(variation, module)
    );
  };
}

function PresetPage({ preset, onRenamed }: { preset?: PresetState; onRenamed: (n: string) => void }) {
  const [variation, setVariation] = useState(0);
  const [changing, setChanging] = useState<null | "Core" | "Time">(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [block, setBlock] = useState<string | null>(null);
  const pickOf = usePickOf(preset ?? ({ name: "", variations: [], cancelled: false, source: modulesOf("Preset")[0] } as PresetState));
  if (!preset) return <section className="sheet" />;
  const v = preset.variations[variation] ?? preset.variations[0];
  const ownBlocks = (preset.source.snapshot_info[0]?.blocks ?? []).map((b) => b.block);
  const differs = (module: string) => variation > 0 && pickOf(variation, module) !== pickOf(0, module);
  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
      <header style={{ padding: "18px 22px 14px", borderBottom: "1px solid var(--rule)", display: "flex", alignItems: "flex-end", gap: 12 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          {renaming === null ? (
            <h2 className="t-marker" style={{ margin: 0, fontSize: 44, position: "relative", display: "inline-block", color: preset.cancelled ? "var(--ink-3)" : undefined }}>
              {preset.name}
              {preset.cancelled && <Strike width={4} tone="var(--void)" />}
            </h2>
          ) : (
            <input
              autoFocus
              value={renaming}
              onChange={(e) => setRenaming(e.target.value)}
              className="t-marker"
              style={{ fontSize: 40, width: "100%", border: "none", borderBottom: "3px solid var(--ink)", outline: "none", background: "transparent" }}
            />
          )}
          <div className="t-meta" style={{ marginTop: 6 }}>
            A whole sound — any patch in any profile can play it
          </div>
        </div>
        {renaming === null ? (
          <>
            <Button onClick={() => setRenaming(preset.name)}>Rename</Button>
            {preset.cancelled ? (
              <Button primary onClick={() => cancelPreset(preset.name, false)}>
                Restore
              </Button>
            ) : (
              <Button onClick={() => cancelPreset(preset.name, true)} style={{ borderColor: "var(--void)", color: "var(--void)" }}>
                Delete
              </Button>
            )}
          </>
        ) : (
          <>
            <Button onClick={() => setRenaming(null)}>Cancel</Button>
            <Button
              primary
              disabled={!renaming.trim()}
              onClick={() => {
                renamePreset(preset.name, renaming.trim());
                onRenamed(renaming.trim());
                setRenaming(null);
              }}
            >
              Save
            </Button>
          </>
        )}
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto", background: "var(--sheet-2)" }}>
        {/* The variations: the one shown taped down in black. */}
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "16px 22px 6px", flexWrap: "wrap" }}>
          {preset.variations.map((name, i) => (
            <button key={name} onClick={() => setVariation(i)} style={{ padding: 2 }} aria-pressed={i === variation}>
              <Tape
                colour={i === variation ? "var(--tape-gaffer)" : "var(--sheet)"}
                tilt={i % 2 ? 0.7 : -0.7}
                style={{ fontSize: 15, padding: "10px 16px", border: i === variation ? undefined : "1px solid var(--rule-strong)" }}
              >
                {name}
              </Tape>
            </button>
          ))}
          <Button onClick={() => addVariation(preset.name, `Variation ${preset.variations.length + 1}`)} style={{ minHeight: 44 }}>
            + Variation
          </Button>
        </div>
        {/* The composition: Core and Time, each its own taped panel. */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14, padding: "14px 22px" }}>
          {(["Core", "Time"] as const).map((module) => {
            const pick = pickOf(variation, module);
            const [name, snap] = pick.split(" · ");
            const source = modulesOf(module).find((m) => m.name === name);
            return (
              <div key={module} className="sheet" style={{ position: "relative", padding: "18px 18px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
                <button onClick={() => setChanging(module)} style={{ textAlign: "left" }} title={`Choose another ${module}`}>
                  <h3 className="t-marker" style={{ margin: 0, fontSize: 28 }}>
                    {module}
                  </h3>
                  <div style={{ fontSize: 22, fontWeight: 800, marginTop: 8 }}>{name}</div>
                  <div style={{ fontSize: 17, fontWeight: 650, marginTop: 2, color: "var(--ink-2)" }}>
                    {snap ?? ""}
                    {differs(module) && (
                      <span className="t-label" style={{ marginLeft: 10, color: "var(--void)" }}>
                        differs from {preset.variations[0]}
                      </span>
                    )}
                  </div>
                </button>
                {source && (
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                    {source.snapshots.map((x) => {
                      const on = x === snap;
                      return (
                        <button
                          key={x}
                          className="pressable"
                          onClick={() => setVariationPick(preset.name, v, module, `${name} · ${x}`)}
                          style={{
                            minHeight: 44,
                            padding: "0 12px",
                            border: on ? "3px solid var(--ink)" : "1px solid var(--rule-strong)",
                            borderRadius: "var(--r)",
                            fontWeight: on ? 840 : 620,
                            fontSize: 15,
                            background: "var(--sheet)",
                          }}
                        >
                          {x}
                        </button>
                      );
                    })}
                  </div>
                )}
                <Button onClick={() => setChanging(module)} style={{ alignSelf: "flex-start", minHeight: 44 }}>
                  Another {module}…
                </Button>
              </div>
            );
          })}
        </div>
        {/* Its own blocks, as a little chain. */}
        <div style={{ padding: "6px 22px 18px" }}>
          <h3 style={{ margin: "0 0 10px", fontSize: 18, fontWeight: 820 }}>Its own blocks</h3>
          {ownBlocks.length === 0 ? (
            <p className="t-meta" style={{ margin: 0 }}>
              None — it plays the Core's and the Time's.
            </p>
          ) : (
            <div style={{ display: "flex", gap: 0, overflowX: "auto", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", background: "var(--sheet)" }}>
              {ownBlocks.map((bname, i) => (
                <button
                  key={bname}
                  className="pressable"
                  onClick={() => setBlock(bname)}
                  style={{ minWidth: 180, minHeight: 72, padding: "10px 14px", textAlign: "left", borderLeft: i ? "1px solid var(--rule)" : undefined }}
                >
                  <div style={{ fontSize: 16, fontWeight: 800 }}>{bname}</div>
                  <div className="t-meta" style={{ fontSize: 14 }}>
                    {pickOf(variation, `block:${bname}`)}
                    {differs(`block:${bname}`) && <span style={{ color: "var(--void)" }}> · differs</span>}
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
        <div style={{ padding: "4px 22px 22px" }}>
          <h3 style={{ margin: "0 0 10px", fontSize: 18, fontWeight: 820 }}>Played by</h3>
          {preset.source.used_by.length === 0 ? (
            <p className="t-meta" style={{ margin: 0 }}>
              No patch plays it yet. Give it to a section in Set.
            </p>
          ) : (
            <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
              {preset.source.used_by.map((u) => (
                <Tape key={u} colour="var(--sheet)" style={{ border: "1px solid var(--rule-strong)", fontSize: 13 }}>
                  {u}
                </Tape>
              ))}
            </div>
          )}
        </div>
      </div>
      {changing && (
        <ModulePickSheet
          module={changing}
          current={pickOf(variation, changing)}
          onPick={(x) => {
            setVariationPick(preset.name, v, changing, x);
            setChanging(null);
          }}
          onClose={() => setChanging(null)}
        />
      )}
      {block && (
        <BlockPickSheet
          block={block}
          current={pickOf(variation, `block:${block}`)}
          onPick={(x) => {
            setVariationPick(preset.name, v, `block:${block}`, x);
            setBlock(null);
          }}
          onClose={() => setBlock(null)}
        />
      )}
    </section>
  );
}

function BlockPickSheet({ block, current, onPick, onClose }: { block: string; current: string; onPick: (v: string) => void; onClose: () => void }) {
  // A block's type from its presets' names in the library (Pre Verb → reverb).
  const type = [...BLOCKS.entries()].find(([, list]) => list.some((b) => b.name === current))?.[0] ?? guessType(block);
  const list = BLOCKS.get(type) ?? [];
  return (
    <SideSheet title={block} sub={`${cap(type)} presets · ${list.length}`} onClose={onClose}>
      <div className="ruled">
        {list.map((b) => (
          <button
            key={b.name}
            className="pressable"
            onClick={() => onPick(b.name)}
            style={{ width: "100%", minHeight: 56, padding: "0 22px", display: "flex", alignItems: "center", textAlign: "left", fontSize: 17, fontWeight: b.name === current ? 840 : 580 }}
          >
            <span style={{ flex: 1 }}>{b.name}</span>
            {b.name === current && <span className="t-label">In this variation</span>}
          </button>
        ))}
      </div>
    </SideSheet>
  );
}

function guessType(block: string): string {
  const b = block.toLowerCase();
  if (b.includes("verb")) return "reverb";
  if (b.includes("delay") || b.includes("dly")) return "delay";
  if (b.includes("comp")) return "compressor";
  if (b.includes("eq")) return "eq";
  return [...BLOCKS.keys()][0] ?? "";
}

function RecipeRow({ label, detail, value, onChange }: { label: string; detail: string; value: string; onChange?: () => void }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 14, minHeight: 76, padding: "8px 22px" }}>
      <div style={{ width: 150, flexShrink: 0 }}>
        <div style={{ fontSize: 19, fontWeight: 860 }}>{label}</div>
        <div className="t-meta" style={{ fontSize: 13 }}>
          {detail}
        </div>
      </div>
      <div style={{ flex: 1, minWidth: 0, fontSize: 20, fontWeight: 650, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{value}</div>
      {onChange && <Button onClick={onChange}>Change</Button>}
    </div>
  );
}

function ModulePickSheet({ module, current, onPick, onClose }: { module: string; current: string; onPick: (v: string) => void; onClose: () => void }) {
  const list = modulesOf(module);
  return (
    <SideSheet title={`Choose the ${module}`} sub={`${list.length} ${module} presets — each with its variations`} onClose={onClose} width={520}>
      {list.map((m) => (
        <div key={m.name} style={{ borderBottom: "1px solid var(--rule)", padding: "12px 22px 14px" }}>
          <div style={{ fontSize: 18, fontWeight: 840, marginBottom: 8 }}>{m.name}</div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
            {m.snapshots.map((snap) => {
              const v = `${m.name} · ${snap}`;
              const on = v === current;
              return (
                <button
                  key={snap}
                  className="pressable"
                  onClick={() => onPick(v)}
                  style={{
                    minHeight: 44,
                    padding: "0 14px",
                    border: on ? "3px solid var(--ink)" : "1px solid var(--rule-strong)",
                    borderRadius: "var(--r)",
                    fontWeight: on ? 840 : 620,
                    fontSize: 15,
                  }}
                >
                  {snap}
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </SideSheet>
  );
}

// ── A Core / Time / Drive / Delay / Reverb preset ────────────────────

function ModulePage({ module }: { module?: ModulePreset }) {
  const [snap, setSnap] = useState(0);
  if (!module) return <section className="sheet" />;
  const info = module.snapshot_info[snap] ?? module.snapshot_info[0];
  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
      <header style={{ padding: "18px 22px 14px", borderBottom: "1px solid var(--rule)" }}>
        <h2 className="t-marker" style={{ margin: 0, fontSize: 44 }}>
          {module.name}
        </h2>
        <div className="t-meta" style={{ marginTop: 6 }}>
          {module.module} preset · {module.snapshots.length} variations · {module.used_by.length} patches play it
        </div>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "14px 22px", flexWrap: "wrap" }}>
          <h3 style={{ margin: "0 6px 0 0", fontSize: 18, fontWeight: 820 }}>Variations</h3>
          {module.snapshots.map((v, i) => (
            <button key={v} onClick={() => setSnap(i)} style={{ padding: 2 }}>
              <Tape
                colour={i === snap ? "var(--tape-gaffer)" : "var(--sheet)"}
                tilt={i % 2 ? 0.7 : -0.7}
                style={{ fontSize: 15, padding: "10px 16px", border: i === snap ? undefined : "1px solid var(--rule-strong)" }}
              >
                {v}
              </Tape>
            </button>
          ))}
        </div>
        <div className="ruled" style={{ borderTop: "2px solid var(--ink)", borderBottom: "1px solid var(--rule)" }}>
          {(info?.modules ?? []).map((m) => (
            <RecipeRow key={m.module} label={m.module} detail="Module" value={`${m.preset} · ${m.snapshot}`} />
          ))}
          {(info?.blocks ?? []).map((b) => (
            <RecipeRow key={b.block} label={b.block} detail="Block" value={b.preset} />
          ))}
          {(info?.captures ?? []).length > 0 && (
            <RecipeRow label="Captures" detail="NAM models" value={(info?.captures ?? []).join(" · ")} />
          )}
        </div>
        <div style={{ padding: "18px 22px" }}>
          <h3 style={{ margin: "0 0 10px", fontSize: 18, fontWeight: 820 }}>Played by</h3>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
            {module.used_by.map((u) => (
              <Tape key={u} colour="var(--sheet)" style={{ border: "1px solid var(--rule-strong)", fontSize: 13 }}>
                {u.replace(/^Patch /, "")}
              </Tape>
            ))}
            {module.used_by.length === 0 && <span className="t-meta">Nothing plays it yet.</span>}
          </div>
        </div>
      </div>
    </section>
  );
}

function BlockTypePage({ type }: { type: string }) {
  const list = BLOCKS.get(type) ?? [];
  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
      <header style={{ padding: "18px 22px 14px", borderBottom: "2px solid var(--ink)" }}>
        <h2 className="t-marker" style={{ margin: 0, fontSize: 44 }}>
          {cap(type)} presets
        </h2>
        <div className="t-meta" style={{ marginTop: 6 }}>
          {list.length} presets — any {type} block can play them
        </div>
      </header>
      <div className="ruled" style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {list.map((b) => (
          <div key={b.name} style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 64, padding: "8px 22px" }}>
            <span style={{ flex: 1, fontSize: 18, fontWeight: 760 }}>{b.name}</span>
            <span className="t-meta">{b.bypass ? "Off" : `${b.used_by.length} play it`}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
