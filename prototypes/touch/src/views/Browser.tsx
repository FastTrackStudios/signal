// The Browser: where you go through what the rig has — patches, module
// presets, block presets, presets, profiles. It opens in the middle of the
// main area (Perform), or in the left area when the main area is busy
// (Edit). With a part picked in the setlist it is that part's: a patch
// becomes what it plays, a module preset is swapped in for it alone (a Time
// module just for the bridge), a block preset is its own edit. With nothing
// picked it just browses.

import { useState } from "react";
import { blockPresetsByType, MODULE_KINDS, modulesOf, rig } from "../data/rig";
import { editParam, overrideOf, sectionsOf, select, setModuleOverride, setSectionSound, setSongProfile, useStore, type Target } from "../store";
import { ProfileIcon } from "../ui/profileIcons";
import { nameColour } from "../setlist/colors";
import { PatchList } from "../setlist/Setlist";
import { sectionColour, songColour } from "../setlist/colors";

/** The modules a part can swap, in the order a player thinks of them. */
const SWAPPABLE = MODULE_KINDS.filter((k) => k !== "Preset" && modulesOf(k).length > 0);

type Tab = "patches" | "modules" | "blocks" | "presets" | "profiles";
const TABS: { id: Tab; label: string }[] = [
  { id: "patches", label: "Patches" },
  { id: "modules", label: "Modules" },
  { id: "blocks", label: "Blocks" },
  { id: "presets", label: "Presets" },
  { id: "profiles", label: "Profiles" },
];

export function Browser({ onClose }: { onClose?: () => void }) {
  const s = useStore();
  const [tab, setTab] = useState<Tab>("patches");
  const t = s.selection;
  const sec = t ? sectionsOf(s, t.song)[t.section] : undefined;
  const target = t && sec ? t : null;
  const part = sec && t ? sec.parts[t.part] : undefined;
  const over = overrideOf(s, target);
  const colour = t ? songColour(t.song, s.songColours) : "var(--ink-3)";
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0, background: "var(--sheet)" }}>
      {/* What it's for: the picked part, or just browsing. */}
      <header style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 10, height: 48, padding: "0 4px 0 16px", background: target ? `color-mix(in oklab, ${colour} 6%, var(--sheet))` : undefined }}>
        <span className="t-label" style={{ color: "var(--ink-3)" }}>
          Browser
        </span>
        {target && sec ? (
          <>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: colour, flexShrink: 0 }} />
            <span style={{ fontSize: 14, fontWeight: 650, color: "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{target.song}</span>
            <span style={{ fontSize: 15, fontWeight: 750, color: sectionColour(sec.name), whiteSpace: "nowrap" }}>{sec.name}</span>
            {sec.parts.length > 1 && <span style={{ fontSize: 14, fontWeight: 650, whiteSpace: "nowrap" }}>{part?.name}</span>}
            <button className="pressable" onClick={() => select(null)} style={{ height: 30, padding: "0 10px", borderRadius: "var(--r)", fontSize: 12.5, fontWeight: 650, color: "var(--ink-3)" }}>
              Unpick
            </button>
          </>
        ) : (
          <span style={{ fontSize: 13, color: "var(--ink-3)" }}>Pick a section's patch in the setlist to choose for it</span>
        )}
        <span style={{ flex: 1 }} />
        {onClose && (
          <button className="pressable" aria-label="Close the browser" onClick={onClose} style={{ width: 44, height: 44, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)" }}>
            <svg width="13" height="13" viewBox="0 0 12 12" aria-hidden>
              <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
          </button>
        )}
      </header>
      {/* The kinds of thing, as tabs along a hairline. */}
      <nav role="tablist" style={{ flexShrink: 0, display: "flex", borderBottom: "1px solid var(--rule)", overflowX: "auto", scrollbarWidth: "none" }}>
        {TABS.map((x) => (
          <button
            key={x.id}
            role="tab"
            aria-selected={tab === x.id}
            className="pressable"
            onClick={() => setTab(x.id)}
            style={{ position: "relative", flexShrink: 0, height: 44, padding: "0 16px", fontSize: 14, fontWeight: tab === x.id ? 700 : 560, color: tab === x.id ? "var(--ink)" : "var(--ink-3)" }}
          >
            {x.label}
            {tab === x.id && <span aria-hidden style={{ position: "absolute", left: 10, right: 10, bottom: 0, height: 2, borderRadius: 1, background: "var(--ink-2)" }} />}
          </button>
        ))}
      </nav>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {tab === "patches" && (
          <PatchList
            song={target?.song}
            current={part?.sound?.name ?? null}
            defaultLabel={target ? "Keep what plays" : "—"}
            onPick={(p) => target && setSectionSound(target.song, target.section, p ? { kind: "patch", ...p } : null, target.part)}
          />
        )}
        {tab === "modules" &&
          SWAPPABLE.map((kind) =>
            target ? (
              <ModuleRow key={kind} t={target} kind={kind} picked={over.modules[kind] ?? null} />
            ) : (
              <ListRow key={kind} title={kind} items={modulesOf(kind).map((m) => m.name)} />
            ),
          )}
        {tab === "blocks" &&
          [...blockPresetsByType()].map(([type, list]) => (
            <ListRow
              key={type}
              title={type}
              items={list.map((b) => b.name)}
              picked={target ? (over.edits[`block:${type}`] !== undefined ? list[over.edits[`block:${type}`]]?.name : undefined) : undefined}
              onPick={target ? (name) => editParam(target, `block:${type}`, list.findIndex((b) => b.name === name)) : undefined}
            />
          ))}
        {tab === "presets" && <ListRow title="Presets" items={modulesOf("Preset").map((m) => m.name)} wrap />}
        {tab === "profiles" && (
          <div>
            {rig.library.profiles.map((p) => (
              <button
                key={p.name}
                className="pressable"
                onClick={() => target && setSongProfile(target.song, p.name)}
                style={{ width: "100%", display: "flex", alignItems: "center", gap: 12, minHeight: 52, padding: "0 16px", borderBottom: "1px solid var(--rule)", textAlign: "left" }}
              >
                <ProfileIcon name={p.name} colour={nameColour(p.name)} size={18} />
                <span style={{ flex: 1, fontSize: 15, fontWeight: 600 }}>{p.name}</span>
                <span className="t-meta" style={{ fontSize: 12 }}>
                  {p.stacks.filter((st) => p.patch_list.some((x) => x.stack === st)).join(" · ")}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** A kind of thing and its items, as chips; picked when one is the
 *  target's; tappable when there is a target. */
function ListRow({ title, items, picked, onPick, wrap }: { title: string; items: string[]; picked?: string; onPick?: (name: string) => void; wrap?: boolean }) {
  return (
    <div style={{ display: "flex", alignItems: wrap ? "flex-start" : "center", gap: 12, minHeight: 60, padding: wrap ? "10px 16px" : "0 16px", borderBottom: "1px solid var(--rule)" }}>
      <span className="t-label" style={{ width: 84, flexShrink: 0, color: picked ? "var(--modified)" : "var(--ink-3)", paddingTop: wrap ? 14 : 0 }}>
        {title}
      </span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexWrap: wrap ? "wrap" : "nowrap", gap: 6, overflowX: wrap ? undefined : "auto", scrollbarWidth: "none", padding: "8px 0" }}>
        {items.map((name) => {
          const on = picked === name;
          return (
            <button
              key={name}
              disabled={!onPick}
              onClick={() => onPick?.(name)}
              className={onPick && !on ? "pressable" : ""}
              style={{
                flexShrink: 0,
                height: 44,
                padding: "0 14px",
                borderRadius: "var(--r)",
                fontSize: 14,
                fontWeight: on ? 700 : 560,
                whiteSpace: "nowrap",
                color: on ? "var(--ink)" : "var(--ink-2)",
                background: on ? "var(--pressed-bg)" : "transparent",
                boxShadow: on ? "var(--pressed-shadow), inset 0 -2px 0 var(--modified)" : "inset 0 0 0 1px var(--rule)",
                cursor: onPick ? "pointer" : "default",
              }}
            >
              {name}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function ModuleRow({ t, kind, picked }: { t: Target; kind: string; picked: string | null }) {
  const presets = modulesOf(kind).map((m) => m.name);
  const chip = (label: string, on: boolean, onClick: () => void, swapped?: boolean) => (
    <button
      key={label}
      onClick={onClick}
      className={on ? "" : "pressable"}
      aria-pressed={on}
      style={{
        flexShrink: 0,
        height: 44,
        padding: "0 14px",
        borderRadius: "var(--r)",
        display: "flex",
        alignItems: "center",
        gap: 7,
        fontSize: 14,
        fontWeight: on ? 700 : 560,
        whiteSpace: "nowrap",
        color: on ? "var(--ink)" : "var(--ink-2)",
        background: on ? "var(--pressed-bg)" : "transparent",
        boxShadow: on ? `var(--pressed-shadow)${swapped ? ", inset 0 -2px 0 var(--modified)" : ""}` : "inset 0 0 0 1px var(--rule)",
      }}
    >
      {swapped && on && <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--modified)" }} />}
      {label}
    </button>
  );
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 60, padding: "0 16px", borderBottom: "1px solid var(--rule)" }}>
      <span className="t-label" style={{ width: 84, flexShrink: 0, color: picked ? "var(--modified)" : "var(--ink-3)" }}>
        {kind}
      </span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", gap: 6, overflowX: "auto", scrollbarWidth: "none", padding: "8px 0" }}>
        {chip("The patch's", picked === null, () => setModuleOverride(t, kind, null))}
        {presets.map((p) => chip(p, picked === p, () => setModuleOverride(t, kind, p), true))}
      </div>
    </div>
  );
}

