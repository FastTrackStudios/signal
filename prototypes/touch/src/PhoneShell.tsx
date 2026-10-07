// The app on a phone. The bars run edge to edge — under the status bar,
// around the Dynamic Island, under the home indicator — with their content
// kept inside the safe area, as iOS apps do.
//
//   top bar     status only: the menu, the mode the footswitches are in,
//               health, the meters, the house mute's state
//   side menu   swipe in from the left edge (or tap ☰): the footswitch
//               mode, Undo / Redo, the house mute, the rig's health, the
//               tuner and the library — what the iPad's top bar holds
//   foot        the views and the docks, a tab bar
//
//   portrait    the setlist is the page; macros four to a row above it,
//               the switches docked below
//   landscape   the stage: macros and switches docked; the setlist slides
//               in from its tab
//
// The side menu opens from ☰ only — no edge swipe, which iOS keeps for
// going back. Both read the same store as the iPad — linked remotes.

import { useRef, useState, type ReactNode } from "react";
import { ComposeCtx, SidebarContent } from "./setlist/Setlist";
import { Browser } from "./views/Browser";
import { EditView } from "./views/Edit";
import { MacroBar } from "./dock/MacroBar";
import { Switches } from "./dock/Switches";
import { redo, setPerformMode, undo, useStore, useUndo } from "./store";
import { MuteButton, PanicButton } from "./ui/Safety";
import { Indicator, SettingsSheet } from "./ui/Settings";
import { ModeButton } from "./ui/ModeButton";
import { SetlistStrip } from "./setlist/SetlistStrip";
import { Cpu, FootButton, Meters, MODES, Rule, type Dock, type View } from "./App";
import { useSafe } from "./Phone";

const MENU_W = 300;

/** What fills an upright phone: one page at a time, chosen from the foot. */
export type Page = "setlist" | "browser" | "switches" | "macros" | "edit";

const ICON = {
  setlist: (
    <>
      <rect x="2.5" y="3.5" width="13" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M5.5 7h7M5.5 9.5h7M5.5 12h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </>
  ),
  perform: <path d="M5 3.5v11l9-5.5Z" fill="currentColor" />,
  browser: <path d="M3 3.5h3v11H3ZM7.5 3.5h3v11h-3ZM12 4l2.8-.8 2 10.6-2.8.8Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />,
  switches: (
    <>
      <rect x="2" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <rect x="7" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <rect x="12" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </>
  ),
  macros: (
    <>
      <circle cx="4.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="13.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M4.5 9 6 7.4M13.5 9l1.5-1.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </>
  ),
  build: (
    <>
      <rect x="2.5" y="2.5" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <rect x="10" y="2.5" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <rect x="2.5" y="10" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M12.75 10v5.5M10 12.75h5.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </>
  ),
  edit: (
    <>
      <path d="M4 2.5v13M9 2.5v13M14 2.5v13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
      <rect x="2.3" y="10" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
      <rect x="7.3" y="5" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
      <rect x="12.3" y="8" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
    </>
  ),
};

export function PhoneShell({ page: firstPage = "setlist", menuOpen = false }: { page?: Page; menuOpen?: boolean } = {}) {
  const safe = useSafe();
  const { landscape } = safe;
  const [view, setView] = useState<View>("perform");
  const [dock, setDock] = useState<Dock>(landscape ? "switches" : null);
  const [macros, setMacros] = useState(landscape);
  // Landscape drawers over the stage: the setlist, the browser.
  const [drawer, setDrawer] = useState<"setlist" | "browser" | null>(null);
  // The side menu: open, or following a finger in from the left edge.
  const [menu, setMenu] = useState(menuOpen);
  const [page, setPage] = useState<Page>(firstPage);
  const root = useRef<HTMLDivElement>(null);
  const drawerTab = (id: "setlist" | "browser", label: string) => (
    <Tab landscape label={label} on={drawer === id} onClick={() => setDrawer(drawer === id ? null : id)}>
      {ICON[id]}
    </Tab>
  );
  return (
    <div
      ref={root}
      style={{ position: "relative", flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}
    >
      {/* Picking a part on this phone takes it to the browser for it. */}
      <ComposeCtx.Provider value={{ onPicked: () => (landscape ? setDrawer("browser") : setPage("browser")) }}>
        {landscape ? (
          // Landscape: the bar is a rail down the left (as Signal's and
          // Session's), carrying the status too, so the stage keeps its height.
          <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
            <Rail onMenu={() => setMenu(true)}>
              <Tab landscape label="Perform" on={view === "perform"} onClick={() => setView("perform")}>
                {ICON.perform}
              </Tab>
              <Tab landscape label="Build" on={view === "build"} onClick={() => setView("build")}>
                {ICON.build}
              </Tab>
              <Tab landscape label="Edit" on={view === "edit"} onClick={() => setView("edit")}>
                {ICON.edit}
              </Tab>
              <Rule />
              {drawerTab("setlist", "Setlist")}
              {drawerTab("browser", "Browser")}
            </Rail>
            <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", position: "relative", paddingRight: safe.right }}>
              {view === "perform" ? (
                <>
                  {/* The stage: the macros sitting on the switches at the foot,
                      at the iPad's sizes, and all the room above them for where
                      you are in the set. */}
                  <SetlistStrip fill />
                  {macros && (
                    <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderTop: "1px solid #000" }}>
                      <MacroBar cols={8} up />
                    </div>
                  )}
                  {/* Flush: the switches bring their own hairline. */}
                  {dock && <div style={{ flexShrink: 0 }}>{dock === "switches" && <Switches />}</div>}
                </>
              ) : view === "build" ? (
                // Build on its side: the browser fills the screen.
                <Browser />
              ) : (
                // Edit on its side: the FX row fills the screen — it was made
                // this size; routing waits for the iPad.
                <EditView routing={false} fill />
              )}
              {/* The setlist or the browser slides in over the stage. */}
              {drawer && (
                <>
                  <button aria-label="Close" onClick={() => setDrawer(null)} style={{ position: "absolute", inset: 0, zIndex: 9, background: "rgba(0,0,0,0.5)" }} />
                  <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: drawer === "browser" ? 520 : 402, zIndex: 10, display: "flex", flexDirection: "column", borderRight: "1px solid var(--rule-strong)", boxShadow: "16px 0 40px rgba(0,0,0,0.5)", animation: "drawer-in 200ms var(--ease) both" }}>
                    {drawer === "setlist" ? <SidebarContent /> : <Browser onClose={() => setDrawer(null)} />}
                  </div>
                </>
              )}
            </div>
          </div>
        ) : (
          <>
            <TopBar onMenu={() => setMenu(true)} />
            {/* Upright, a page at a time: the setlist, the browser, the
                switches (the board turned a quarter), the macros two to a
                row, Edit. */}
            <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", position: "relative" }}>
              {page === "setlist" && <SidebarContent />}
              {page === "browser" && <Browser />}
              {page === "switches" && <Switches vertical />}
              {page === "macros" && (
                <div style={{ flex: 1, minHeight: 0, overflowY: "auto", background: "#000" }}>
                  <MacroBar cols={2} cellH={64} />
                </div>
              )}
              {page === "edit" && <EditView routing={false} fill />}
            </div>
            <footer style={{ flexShrink: 0, display: "flex", alignItems: "stretch", height: 54 + safe.bottom, padding: `0 2px ${safe.bottom}px`, borderTop: "1px solid var(--rule)", background: "var(--sheet)" }}>
              {(
                [
                  ["setlist", "Setlist"],
                  ["browser", "Browser"],
                  ["switches", "Switches"],
                  ["macros", "Macros"],
                  ["edit", "Edit"],
                ] as const
              ).map(([id, label]) => (
                <Tab key={id} label={label} on={page === id} onClick={() => setPage(id)}>
                  {ICON[id]}
                </Tab>
              ))}
            </footer>
          </>
        )}
      </ComposeCtx.Provider>

      {menu && <SideMenu offset={0} view={view} onView={setView} onClose={() => setMenu(false)} docks={landscape && view === "perform" ? { macros, dock, onMacros: () => setMacros(!macros), onSwitches: () => setDock(dock === "switches" ? null : "switches") } : undefined} />}
      <style>{`@keyframes drawer-in { from { transform: translateX(-24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

/** Status only, edge to edge: its background runs up under the status bar
 *  (portrait) or across the side insets (landscape). */
function TopBar({ onMenu }: { onMenu: () => void }) {
  const safe = useSafe();
  return (
    <header
      style={{
        flexShrink: 0,
        height: 44 + safe.top,
        padding: `${safe.top}px ${8 + safe.right}px 0 ${4 + safe.left}px`,
        display: "flex",
        alignItems: "stretch",
        gap: 4,
        borderBottom: "1px solid var(--rule)",
        background: "var(--sheet)",
      }}
    >
      <button aria-label="Menu" onClick={onMenu} className="pressable" style={{ width: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)" }}>
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
          <path d="M3.5 5.5h13M3.5 10h13M3.5 14.5h13" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      </button>
      <ModeButton />
      <span style={{ flex: 1 }} />
      <PanicButton size={30} />
      <Indicator kind="midi" compact />
      <Indicator kind="audio" compact />
      <Cpu />
      <Meters />
      <MuteButton size={32} />
    </header>
  );
}

/** Landscape's rail: the menu at the top, the views and docks, then the
 *  status at the foot — meters and the mute; Panic up top, away from it. It hugs the left
 *  edge (the island is on the right; this side's inset is only the
 *  corners'), its ends kept in from the rounded corners. */
function Rail({ onMenu, children }: { onMenu: () => void; children: ReactNode }) {
  const safe = useSafe();
  return (
    <nav style={{ flexShrink: 0, width: 74, paddingLeft: 8, paddingTop: 14, paddingBottom: safe.bottom, display: "flex", flexDirection: "column", alignItems: "stretch", borderRight: "1px solid var(--rule)", background: "var(--sheet)" }}>
      <button aria-label="Menu" onClick={onMenu} className="pressable" style={{ height: 38, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)" }}>
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
          <path d="M3.5 5.5h13M3.5 10h13M3.5 14.5h13" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      </button>
      <div className="rail" style={{ display: "flex", flexDirection: "column" }}>{children}</div>
      <span style={{ flex: 1 }} />
      <div style={{ height: 52, display: "flex", justifyContent: "center" }}>
        <Meters />
      </div>
      <MuteButton size={40} rail />
      <style>{`.rail > span > button { margin: 1px 4px !important; min-width: 0 !important; flex: 1; padding: 0 !important } .rail > span[aria-hidden] { width: 36px !important; height: 1px !important; align-self: center; margin: 4px 0 }`}</style>
    </nav>
  );
}

/** The side menu: what the iPad's top bar holds, for a thumb. Slides in
 *  from the left, under the finger while it pulls. */
function SideMenu({ offset, view, onView, onClose, docks }: { offset: number; view: View; onView: (v: View) => void; onClose: () => void; docks?: { macros: boolean; dock: Dock; onMacros: () => void; onSwitches: () => void } }) {
  const s = useStore();
  const safe = useSafe();
  const { depth, label, redoDepth, redoLabel } = useUndo();
  const [settings, setSettings] = useState<"midi" | "audio" | null>(null);
  const row: React.CSSProperties = { display: "flex", alignItems: "center", gap: 12, width: "100%", minHeight: 52, padding: "0 16px", textAlign: "left", fontSize: 16, fontWeight: 600 };
  return (
    <>
      <button aria-label="Close the menu" onClick={onClose} style={{ position: "absolute", inset: 0, zIndex: 50, background: `rgba(0,0,0,${0.55 * (1 - offset / MENU_W)})` }} />
      <nav
        style={{
          position: "absolute",
          zIndex: 51,
          top: 0,
          bottom: 0,
          left: 0,
          // Landscape: the island is on the right, so this edge needs only a
          // little room for the corner.
          width: MENU_W + (safe.landscape ? 8 : safe.left),
          paddingTop: safe.top,
          paddingLeft: safe.landscape ? 8 : safe.left,
          paddingBottom: safe.bottom,
          display: "flex",
          flexDirection: "column",
          background: "#141418",
          borderRight: "1px solid var(--rule-strong)",
          boxShadow: "16px 0 40px rgba(0,0,0,0.5)",
          transform: `translateX(${-offset}px)`,
          transition: offset === 0 ? "transform 200ms var(--ease)" : "none",
          overflowY: "auto",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "14px 16px 10px" }}>
          <span style={{ fontWeight: 750, fontSize: 18 }}>Signal</span>
          <span className="t-meta" style={{ fontSize: 13 }}>
            voyager
          </span>
        </div>
        <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
          Footswitches
        </div>
        {MODES.map((m) => {
          const on = s.performMode === m.id;
          return (
            <button
              key={m.id}
              role="radio"
              aria-checked={on}
              className={on ? "" : "pressable"}
              onClick={() => {
                setPerformMode(m.id);
                onClose();
              }}
              style={{ ...row, minHeight: 60, alignItems: "center", background: on ? "var(--pressed-bg)" : undefined, boxShadow: on ? "var(--pressed-shadow), inset 3px 0 0 var(--live)" : undefined }}
            >
              <span style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2 }}>
                <span style={{ fontWeight: on ? 750 : 600 }}>{m.label}</span>
                <span className="t-meta" style={{ fontSize: 12.5, lineHeight: 1.3 }}>
                  {m.hint}
                </span>
              </span>
            </button>
          );
        })}
        <div style={{ height: 1, background: "var(--rule)", margin: "8px 0" }} />
        {/* Every view, so the phone reaches all the iPad does. */}
        <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
          View
        </div>
        {(["perform", "build", "edit"] as View[]).map((v) => {
          const off = false;
          const on = view === v;
          return (
            <button
              key={v}
              disabled={off}
              className={on || off ? "" : "pressable"}
              onClick={() => {
                onView(v);
                onClose();
              }}
              style={{ ...row, minHeight: 46, textTransform: "capitalize", color: off ? "var(--dim)" : "var(--ink)", background: on ? "var(--pressed-bg)" : undefined, boxShadow: on ? "var(--pressed-shadow)" : undefined }}
            >
              <span style={{ flex: 1 }}>{v}</span>
              {off && <span style={{ fontSize: 12, color: "var(--dim)", textTransform: "none" }}>Preset mode</span>}
            </button>
          );
        })}
        {/* On its side the rail has no room for the docks: they're here. */}
        {docks && (
          <>
            <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
              Show
            </div>
            {(
              [
                ["Macros", docks.macros, docks.onMacros],
                ["Switches", docks.dock === "switches", docks.onSwitches],
              ] as const
            ).map(([label, on, toggle]) => (
              <button key={label} className="pressable" role="switch" aria-checked={on} onClick={toggle} style={{ ...row, minHeight: 46 }}>
                <span style={{ flex: 1 }}>{label}</span>
                <span aria-hidden style={{ width: 40, height: 24, borderRadius: 999, padding: 2, background: on ? "var(--live)" : "#2b2b31", display: "flex", justifyContent: on ? "flex-end" : "flex-start" }}>
                  <span style={{ width: 20, height: 20, borderRadius: 999, background: "#f4f4f5" }} />
                </span>
              </button>
            ))}
          </>
        )}
        <div style={{ height: 1, background: "var(--rule)", margin: "8px 0" }} />
        <div style={{ display: "flex", padding: "0 8px", gap: 4 }}>
          <button className={depth ? "pressable" : ""} disabled={!depth} onClick={undo} title={label ? `Undo ${label}` : undefined} style={{ ...row, flex: 1, padding: "0 10px", justifyContent: "center", borderRadius: "var(--r)", color: depth ? "var(--ink)" : "var(--dim)" }}>
            Undo
          </button>
          <button className={redoDepth ? "pressable" : ""} disabled={!redoDepth} onClick={redo} title={redoLabel ? `Redo ${redoLabel}` : undefined} style={{ ...row, flex: 1, padding: "0 10px", justifyContent: "center", borderRadius: "var(--r)", color: redoDepth ? "var(--ink)" : "var(--dim)" }}>
            Redo
          </button>
        </div>
        <div style={{ padding: "8px 12px" }}>
          <div style={{ display: "flex" }}>
            <MuteButton size={48} label />
          </div>
        </div>
        <div style={{ height: 1, background: "var(--rule)", margin: "8px 0" }} />
        <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
          Rig
        </div>
        {(
          [
            ["midi", "MIDI", "Morningstar MC8 · in"],
            ["audio", "Audio", "48 kHz · 128 samples"],
          ] as const
        ).map(([kind, k, v]) => (
          <button key={k} className="pressable" onClick={() => setSettings(kind)} style={{ ...row, minHeight: 44, fontSize: 14 }}>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
            <span style={{ flex: 1 }}>{k}</span>
            <span className="t-meta" style={{ fontSize: 13 }}>
              {v}
            </span>
            <svg width="7" height="12" viewBox="0 0 7 12" aria-hidden style={{ color: "var(--ink-3)" }}>
              <path d="M1 1l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        ))}
        {settings && <SettingsSheet kind={settings} onClose={() => setSettings(null)} />}
        <div style={{ ...row, minHeight: 40, fontSize: 14 }}>
          <Cpu />
        </div>
        <div style={{ display: "flex", padding: "4px 12px 8px" }}>
          <PanicButton size={44} label />
        </div>
        <div style={{ height: 1, background: "var(--rule)", margin: "8px 0" }} />
        <button className="pressable" style={row} onClick={onClose}>
          Tuner
        </button>
        <button className="pressable" style={row} onClick={onClose}>
          Library
        </button>
      </nav>
    </>
  );
}

/** A foot button that shares the bar evenly in portrait, sized to itself in landscape. */
function Tab({ landscape, children, ...rest }: { landscape?: boolean; label: string; on?: boolean; pin?: boolean; onClick: () => void; children: ReactNode }) {
  // In the rail: a stack, each button the rail's width; in the foot: shares
  // the bar evenly.
  return (
    <span style={{ display: "flex", flex: landscape ? undefined : "1 1 0", minWidth: 0, height: landscape ? 44 : undefined }}>
      <FootButton {...rest}>{children}</FootButton>
    </span>
  );
}
