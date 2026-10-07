// Library: everything the rig holds as one grid of whole cells — sets, songs,
// profiles, patches, presets, Core and Time, block presets — found by kind or
// by typing, the picked one circled and opened beside the grid.

import { Fragment, useMemo, useState } from "react";
import { blockPresetsByType, modulesOf, rig, stackOf } from "../data/rig";
import { useStore } from "../store";
import { Circle, Tape, tapeFor } from "../ui/marks";

const KIND_ORDER = ["Sets", "Songs", "Profiles", "Patches", "Presets", "Core", "Time", "Blocks"] as const;
import { Tabs } from "../ui/kit";

type Kind = "All" | "Sets" | "Songs" | "Profiles" | "Patches" | "Presets" | "Core" | "Time" | "Blocks";

interface Item {
  kind: Exclude<Kind, "All">;
  name: string;
  meta: string;
  tape: string;
  lines: string[];
}

function useItems(): Item[] {
  const s = useStore();
  return useMemo(() => {
    const out: Item[] = [];
    for (const l of s.setlists)
      out.push({ kind: "Sets", name: l.name, meta: `${l.songs.length} songs`, tape: "", lines: l.songs.map((x, i) => `${i + 1}. ${x.name}  ${x.key} ${x.bpm || ""}`) });
    for (const song of rig.library.songs)
      out.push({
        kind: "Songs",
        name: song.name,
        meta: [song.key, song.bpm ? `${song.bpm} BPM` : ""].filter(Boolean).join(" · "),
        tape: "",
        lines: [song.parts.length ? `Sections: ${song.parts.join(", ")}` : "No sections yet", song.setlists.length ? `In: ${song.setlists.join(", ")}` : "In no set"],
      });
    for (const p of rig.library.profiles)
      out.push({ kind: "Profiles", name: p.name, meta: `${p.stacks.length} stacks · ${p.patches} patches`, tape: "", lines: p.stacks.map((st) => `${st}: ${p.patch_list.filter((x) => x.stack === st).map((x) => x.name).join(", ")}`) });
    for (const p of rig.patches)
      out.push({ kind: "Patches", name: p.name, meta: `${p.rig_preset} ${p.variation}`, tape: stackOf(p.name) ? tapeFor(stackOf(p.name)) : "", lines: [`Stack: ${p.stack}`, `Core: ${p.rig_preset} · ${p.variation}`, p.override_modules.length ? `Its own: ${p.override_modules.join(", ")}` : "Nothing of its own"] });
    for (const p of s.presets.filter((x) => !x.cancelled)) {
      const info = p.source.snapshot_info[0];
      out.push({ kind: "Presets", name: p.name, meta: `${p.variations.length} variation${p.variations.length === 1 ? "" : "s"}`, tape: "", lines: (info?.modules ?? []).filter((m) => m.module === "Core" || m.module === "Time").map((m) => `${m.module}: ${m.preset} · ${m.snapshot}`) });
    }
    for (const m of modulesOf("Core"))
      out.push({ kind: "Core", name: m.name, meta: `${m.snapshots.length} variations`, tape: "", lines: [m.snapshots.join(", "), `${m.used_by.length} patches play it`] });
    for (const m of modulesOf("Time"))
      out.push({ kind: "Time", name: m.name, meta: `${m.snapshots.length} variations`, tape: "", lines: [m.snapshots.join(", "), `${m.used_by.length} patches play it`] });
    for (const [type, list] of blockPresetsByType())
      for (const b of list) out.push({ kind: "Blocks", name: b.name, meta: type, tape: "", lines: [`${type} preset`, `${b.used_by.length} play it`] });
    return out.sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind));
  }, [s.setlists, s.presets]);
}

export function LibraryView() {
  const items = useItems();
  const [kind, setKind] = useState<Kind>("All");
  const [q, setQ] = useState("");
  const [picked, setPicked] = useState<Item | null>(null);
  const kinds: Kind[] = ["All", "Sets", "Songs", "Profiles", "Patches", "Presets", "Core", "Time", "Blocks"];
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = items.filter(
    (it) => (kind === "All" || it.kind === kind) && words.every((w) => `${it.name} ${it.meta} ${it.kind}`.toLowerCase().includes(w)),
  );
  const count = (k: Kind) => (k === "All" ? items.length : items.filter((i) => i.kind === k).length);
  return (
    <div style={{ display: "grid", gridTemplateColumns: picked ? "1fr 360px" : "1fr", gap: 14, padding: 14, height: "100%", minHeight: 0 }}>
      <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "12px 14px", borderBottom: "2px solid var(--ink)" }}>
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Find anything — a song, a sound, a pedal"
            style={{ width: 340, minHeight: 48, padding: "0 14px", border: "2px solid var(--ink)", borderRadius: "var(--r)", fontSize: 17 }}
          />
          <div style={{ flex: 1, minWidth: 0 }}>
            <Tabs options={kinds.map((k) => ({ id: k, label: k, count: count(k) }))} value={kind} onChange={setKind} />
          </div>
        </div>
        <div
          style={{
            flex: 1,
            minHeight: 0,
            overflowY: "auto",
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(176px, 1fr))",
            gap: 0,
            alignContent: "start",
            background: "var(--sheet)",
          }}
        >
          {shown.map((it, i) => {
            const on = picked?.kind === it.kind && picked.name === it.name;
            const first = kind === "All" && (i === 0 || shown[i - 1].kind !== it.kind);
            return (
              <Fragment key={`${it.kind}/${it.name}`}>
              {first && (
                <div style={{ gridColumn: "1 / -1", background: "var(--sheet-2)", padding: "16px 16px 10px", display: "flex", alignItems: "baseline", gap: 10, boxShadow: "inset 0 -2px 0 var(--ink)" }}>
                  <h3 style={{ margin: 0, fontSize: 20, fontWeight: 860 }}>{it.kind}</h3>
                  <span className="t-meta">{shown.filter((x) => x.kind === it.kind).length}</span>
                </div>
              )}
              <button
                className="pressable"
                onClick={() => setPicked(on ? null : it)}
                style={{ position: "relative", minHeight: 96, background: on ? "var(--up)" : "var(--sheet)", boxShadow: "inset -1px -1px 0 var(--rule)", padding: "12px 14px", textAlign: "left", display: "flex", flexDirection: "column", gap: 6, overflow: "hidden" }}
              >

                <span style={{ position: "relative", alignSelf: "flex-start", maxWidth: "100%", fontSize: 17, fontWeight: 800, lineHeight: 1.15, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: on ? "visible" : "hidden" }}>
                  {it.name}
                  {on && <Circle inset={-7} />}
                </span>
                <span className="t-meta" style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, whiteSpace: "nowrap", overflow: "hidden" }}>
                  {it.tape && (
                    <Tape colour={it.tape} style={{ fontSize: 13, padding: "1px 6px" }}>
                      {stackOf(it.name)}
                    </Tape>
                  )}
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{it.meta}</span>
                </span>
              </button>
              </Fragment>
            );
          })}
          {shown.length === 0 && (
            <div style={{ gridColumn: "1 / -1", background: "var(--sheet)", padding: 24 }}>
              <div className="t-marker" style={{ fontSize: 22 }}>Nothing called “{q}”</div>
              <p className="t-meta">Try fewer words, or look under All.</p>
            </div>
          )}
        </div>
      </section>
      {picked && (
        <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
          <header style={{ padding: "18px 20px 14px", borderBottom: "1px solid var(--rule)" }}>

            <h2 className="t-marker" style={{ margin: 0, fontSize: 32 }}>
              {picked.name}
            </h2>
            <div className="t-meta" style={{ marginTop: 6 }}>
              {singular(picked.kind)} · {picked.meta}
            </div>
          </header>
          <div className="ruled" style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
            {picked.lines.map((l, i) => (
              <div key={i} style={{ padding: "12px 20px", fontSize: 15, lineHeight: 1.4 }}>
                {l}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function singular(kind: string): string {
  return kind === "Sets" ? "Set" : kind === "Core" || kind === "Time" ? `${kind} preset` : kind === "Blocks" ? "Block preset" : kind.replace(/s$/, "");
}
