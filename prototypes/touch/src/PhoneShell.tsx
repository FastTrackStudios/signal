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
// Both read the same store as the iPad — linked remotes.

import { useRef, useState, type ReactNode } from "react";
import { Setlist } from "./setlist/Setlist";
import { MacroBar } from "./dock/MacroBar";
import { AudioControls, Switches } from "./dock/Switches";
import { redo, setPerformMode, toggleHouseMute, undo, useStore, useUndo } from "./store";
import { Cpu, FootButton, Meters, MODES, Placeholder, Rule, type Dock, type View } from "./App";
import { useSafe } from "./Phone";
import { useStage } from "./ui/stage";

const MENU_W = 300;

export function PhoneShell() {
  const s = useStore();
  const safe = useSafe();
  const { landscape } = safe;
  const stage = useStage();
  const [chosen, setView] = useState<View>("play");
  const view: View = s.performMode !== "preset" && (chosen === "routing" || chosen === "tones") ? "play" : chosen;
  const [dock, setDock] = useState<Dock>(landscape ? "switches" : null);
  const [macros, setMacros] = useState(landscape);
  const [setlist, setSetlist] = useState(false);
  // The side menu: open, or following a finger in from the left edge.
  const [menu, setMenu] = useState(false);
  const [pull, setPull] = useState<number | null>(null);
  const edge = useRef<{ x: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const localX = (clientX: number) => (clientX - (root.current?.getBoundingClientRect().left ?? 0)) / stage.scale;
  return (
    <div
      ref={root}
      style={{ position: "relative", flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}
      // An edge swipe: a press within 20 pt of the left edge, dragged right.
      onPointerDownCapture={(e) => {
        if (menu || localX(e.clientX) > 20) return;
        edge.current = { x: e.clientX };
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        e.stopPropagation();
      }}
      onPointerMove={(e) => {
        if (!edge.current) return;
        setPull(Math.max(0, Math.min(MENU_W, (e.clientX - edge.current.x) / stage.scale)));
      }}
      onPointerUp={() => {
        if (!edge.current) return;
        edge.current = null;
        if ((pull ?? 0) > 60) setMenu(true);
        setPull(null);
      }}
    >
      {landscape ? (
        // Landscape: the bar is a rail down the left (as Signal's and
        // Session's), carrying the status too, so the stage keeps its height.
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          <Rail onMenu={() => setMenu(true)}>
        {landscape && (
          <Tab landscape label="Setlist" on={setlist} onClick={() => setSetlist(!setlist)}>
            <rect x="2.5" y="3.5" width="13" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
            <path d="M7 4v10" stroke="currentColor" strokeWidth="1.4" />
          </Tab>
        )}
        <Tab landscape={landscape} label="Play" on={view === "play"} onClick={() => setView("play")}>
          <path d="M5 3.5v11l9-5.5Z" fill="currentColor" />
        </Tab>
        <Tab landscape={landscape} label="Control" on={view === "control"} onClick={() => setView("control")}>
          <path d="M4 2.5v13M9 2.5v13M14 2.5v13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <rect x="2.3" y="10" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="7.3" y="5" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="12.3" y="8" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
        </Tab>
        <Rule />
        <Tab landscape={landscape} label="Macros" on={macros} pin onClick={() => setMacros(!macros)}>
          <circle cx="4.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="13.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M4.5 9 6 7.4M13.5 9l1.5-1.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </Tab>
        <Tab landscape={landscape} label="Switches" on={dock === "switches"} pin onClick={() => setDock(dock === "switches" ? null : "switches")}>
          <rect x="2" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="7" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="12" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </Tab>
        <Tab landscape={landscape} label="Audio" on={dock === "audio"} pin onClick={() => setDock(dock === "audio" ? null : "audio")}>
          <circle cx="5" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="13" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </Tab>
          </Rail>
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", position: "relative", paddingRight: safe.right }}>
        {macros && (
          <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderBottom: "1px solid #000" }}>
            <MacroBar cols={landscape ? 8 : 4} />
          </div>
        )}
        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>{landscape ? <Placeholder title={view} note="Main area" /> : <Setlist />}</div>
        {dock && <div style={{ flexShrink: 0, borderTop: "1px solid var(--rule)" }}>{dock === "switches" ? <Switches /> : <AudioControls />}</div>}
        {/* Landscape: the setlist slides in over the stage. */}
        {landscape && setlist && (
          <>
            <button aria-label="Close the setlist" onClick={() => setSetlist(false)} style={{ position: "absolute", inset: 0, zIndex: 9, background: "rgba(0,0,0,0.5)" }} />
            <div style={{ position: "absolute", top: 0, bottom: 0, left: safe.left, width: 402, zIndex: 10, display: "flex", flexDirection: "column", borderRight: "1px solid var(--rule-strong)", boxShadow: "16px 0 40px rgba(0,0,0,0.5)", animation: "drawer-in 200ms var(--ease) both" }}>
              <Setlist />
            </div>
          </>
        )}
      </div>

        </div>
      ) : (
        <>
          <TopBar onMenu={() => setMenu(true)} />
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", position: "relative", paddingRight: safe.right }}>
        {macros && (
          <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderBottom: "1px solid #000" }}>
            <MacroBar cols={landscape ? 8 : 4} />
          </div>
        )}
        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>{landscape ? <Placeholder title={view} note="Main area" /> : <Setlist />}</div>
        {dock && <div style={{ flexShrink: 0, borderTop: "1px solid var(--rule)" }}>{dock === "switches" ? <Switches /> : <AudioControls />}</div>}
        {/* Landscape: the setlist slides in over the stage. */}
        {landscape && setlist && (
          <>
            <button aria-label="Close the setlist" onClick={() => setSetlist(false)} style={{ position: "absolute", inset: 0, zIndex: 9, background: "rgba(0,0,0,0.5)" }} />
            <div style={{ position: "absolute", top: 0, bottom: 0, left: safe.left, width: 402, zIndex: 10, display: "flex", flexDirection: "column", borderRight: "1px solid var(--rule-strong)", boxShadow: "16px 0 40px rgba(0,0,0,0.5)", animation: "drawer-in 200ms var(--ease) both" }}>
              <Setlist />
            </div>
          </>
        )}
      </div>

          <footer style={{ flexShrink: 0, display: "flex", alignItems: "stretch", height: 54 + safe.bottom, padding: `0 2px ${safe.bottom}px`, borderTop: "1px solid var(--rule)", background: "var(--sheet)" }}>
        {landscape && (
          <Tab landscape label="Setlist" on={setlist} onClick={() => setSetlist(!setlist)}>
            <rect x="2.5" y="3.5" width="13" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
            <path d="M7 4v10" stroke="currentColor" strokeWidth="1.4" />
          </Tab>
        )}
        <Tab landscape={landscape} label="Play" on={view === "play"} onClick={() => setView("play")}>
          <path d="M5 3.5v11l9-5.5Z" fill="currentColor" />
        </Tab>
        <Tab landscape={landscape} label="Control" on={view === "control"} onClick={() => setView("control")}>
          <path d="M4 2.5v13M9 2.5v13M14 2.5v13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <rect x="2.3" y="10" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="7.3" y="5" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="12.3" y="8" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
        </Tab>
        <Rule />
        <Tab landscape={landscape} label="Macros" on={macros} pin onClick={() => setMacros(!macros)}>
          <circle cx="4.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="13.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M4.5 9 6 7.4M13.5 9l1.5-1.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </Tab>
        <Tab landscape={landscape} label="Switches" on={dock === "switches"} pin onClick={() => setDock(dock === "switches" ? null : "switches")}>
          <rect x="2" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="7" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="12" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </Tab>
        <Tab landscape={landscape} label="Audio" on={dock === "audio"} pin onClick={() => setDock(dock === "audio" ? null : "audio")}>
          <circle cx="5" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="13" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </Tab>
          </footer>
        </>
      )}

      {(menu || pull !== null) && <SideMenu offset={menu ? 0 : MENU_W - (pull ?? 0)} view={view} onView={setView} onClose={() => setMenu(false)} />}
      <style>{`@keyframes drawer-in { from { transform: translateX(-24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

/** Status only, edge to edge: its background runs up under the status bar
 *  (portrait) or across the side insets (landscape). */
function TopBar({ onMenu }: { onMenu: () => void }) {
  const s = useStore();
  const safe = useSafe();
  const mode = MODES.find((m) => m.id === s.performMode)!;
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
      {/* The mode, as a fact: green-edged like the iPad's pick; a tap opens the menu. */}
      <button onClick={onMenu} style={{ alignSelf: "center", height: 28, padding: "0 10px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, background: "var(--pressed-bg)", boxShadow: "var(--pressed-shadow), inset 0 -2px 0 var(--live)" }}>
        {mode.label}
      </button>
      <span style={{ flex: 1 }} />
      <span title="MIDI · Audio" style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 12, fontWeight: 650, color: "var(--ink-2)" }}>
        <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
        {safe.landscape && "MIDI · Audio"}
      </span>
      {safe.landscape && <Cpu />}
      <Meters muted={s.houseMute} width={safe.landscape ? 56 : 36} />
      {/* The house mute's state; it is switched from the menu. Red when muted. */}
      <button
        onClick={toggleHouseMute}
        aria-pressed={s.houseMute}
        aria-label={s.houseMute ? "House muted — tap to unmute" : "Mute the house"}
        style={{ alignSelf: "center", width: 36, height: 30, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: s.houseMute ? "#1a0505" : "var(--ink-3)", background: s.houseMute ? "var(--void)" : "transparent", boxShadow: s.houseMute ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden>
          <path d="M2 6h2.5L8 3v10L4.5 10H2Z" fill="currentColor" />
          {s.houseMute ? <path d="M10.5 6l4 4M14.5 6l-4 4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /> : <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />}
        </svg>
      </button>
    </header>
  );
}

/** Landscape's rail: the menu at the top, the views and docks, then the
 *  status at the foot — health, meters, the house mute. Its background runs
 *  to the screen's edge; its buttons sit inside the safe area. */
function Rail({ onMenu, children }: { onMenu: () => void; children: ReactNode }) {
  const s = useStore();
  const safe = useSafe();
  return (
    <nav style={{ flexShrink: 0, width: safe.left + 66, paddingLeft: safe.left, paddingBottom: safe.bottom, display: "flex", flexDirection: "column", alignItems: "stretch", borderRight: "1px solid var(--rule)", background: "var(--sheet)" }}>
      <button aria-label="Menu" onClick={onMenu} className="pressable" style={{ height: 44, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)" }}>
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
          <path d="M3.5 5.5h13M3.5 10h13M3.5 14.5h13" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      </button>
      <div className="rail" style={{ display: "flex", flexDirection: "column" }}>{children}</div>
      <span style={{ flex: 1 }} />
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, padding: "6px 0 8px" }}>
        <Meters muted={s.houseMute} width={28} />
        <button
          onClick={toggleHouseMute}
          aria-pressed={s.houseMute}
          aria-label={s.houseMute ? "House muted — tap to unmute" : "Mute the house"}
          style={{ width: 44, height: 32, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: s.houseMute ? "#1a0505" : "var(--ink-3)", background: s.houseMute ? "var(--void)" : "transparent", boxShadow: s.houseMute ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden>
            <path d="M2 6h2.5L8 3v10L4.5 10H2Z" fill="currentColor" />
            {s.houseMute ? <path d="M10.5 6l4 4M14.5 6l-4 4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /> : <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />}
          </svg>
        </button>
      </div>
      <style>{`.rail > span > button { margin: 1px 4px !important; min-width: 0 !important; flex: 1; padding: 0 !important } .rail > span[aria-hidden] { width: 36px !important; height: 1px !important; align-self: center; margin: 4px 0 }`}</style>
    </nav>
  );
}

/** The side menu: what the iPad's top bar holds, for a thumb. Slides in
 *  from the left, under the finger while it pulls. */
function SideMenu({ offset, view, onView, onClose }: { offset: number; view: View; onView: (v: View) => void; onClose: () => void }) {
  const s = useStore();
  const safe = useSafe();
  const { depth, label, redoDepth, redoLabel } = useUndo();
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
          width: MENU_W + safe.left,
          paddingTop: safe.top,
          paddingLeft: safe.left,
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
        {/* Every view, so the phone reaches all the iPad does; Routing and
            Tones edit a preset, so they wait for Preset mode. */}
        <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
          View
        </div>
        {(["play", "control", "routing", "tones"] as View[]).map((v) => {
          const off = s.performMode !== "preset" && (v === "routing" || v === "tones");
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
          <button
            onClick={toggleHouseMute}
            aria-pressed={s.houseMute}
            style={{ width: "100%", height: 52, borderRadius: "var(--r-md)", fontSize: 15, fontWeight: 750, letterSpacing: "0.06em", color: s.houseMute ? "#1a0505" : "var(--ink)", background: s.houseMute ? "var(--void)" : "transparent", boxShadow: s.houseMute ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
          >
            {s.houseMute ? "HOUSE MUTED — UNMUTE" : "MUTE HOUSE"}
          </button>
        </div>
        <div style={{ height: 1, background: "var(--rule)", margin: "8px 0" }} />
        <div className="t-label" style={{ padding: "8px 16px 6px", color: "var(--ink-3)" }}>
          Rig
        </div>
        {[
          ["MIDI", "Morningstar MC8 · in"],
          ["Audio", "48 kHz · 128 samples"],
        ].map(([k, v]) => (
          <div key={k} style={{ ...row, minHeight: 40, fontSize: 14 }}>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
            <span style={{ flex: 1 }}>{k}</span>
            <span className="t-meta" style={{ fontSize: 13 }}>
              {v}
            </span>
          </div>
        ))}
        <div style={{ ...row, minHeight: 40, fontSize: 14 }}>
          <Cpu />
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
    <span style={{ display: "flex", flex: landscape ? undefined : "1 1 0", minWidth: 0, height: landscape ? 42 : undefined }}>
      <FootButton {...rest}>{children}</FootButton>
    </span>
  );
}
