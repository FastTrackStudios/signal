// The app on a phone: the iPad's two bars, fitted.
//
//   portrait   the setlist is the page. Top: the footswitch mode, Undo /
//              Redo, health in one readout, the house mute as an icon.
//              Foot: the views and the docks as a tab bar. Macros (four to
//              a row) drop in above the setlist, the switches dock below.
//   landscape  the stage. The iPad's bars, tighter; the setlist slides in
//              from the left by the sidebar button; macros and switches
//              docked by default, the view between them.
//
// Both read the same store as the iPad — linked remotes.

import { useState, type ReactNode } from "react";
import { Setlist } from "./setlist/Setlist";
import { MacroBar } from "./dock/MacroBar";
import { AudioControls, Switches } from "./dock/Switches";
import { setPerformMode, useStore } from "./store";
import { BarButton, Cpu, FootButton, HouseMute, Meters, MODES, Placeholder, Rule, Status, UndoRedo, type Dock, type View } from "./App";

export function PhoneShell({ landscape }: { landscape?: boolean }) {
  const s = useStore();
  const [chosen, setView] = useState<View>("play");
  const view: View = s.performMode !== "preset" && (chosen === "routing" || chosen === "tones") ? "play" : chosen;
  const [dock, setDock] = useState<Dock>(landscape ? "switches" : null);
  const [macros, setMacros] = useState(!!landscape);
  const [drawer, setDrawer] = useState(false);
  return (
    <div style={{ position: "relative", flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
      <header style={{ height: landscape ? 44 : 48, flexShrink: 0, display: "flex", alignItems: "stretch", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
        {landscape && (
          <>
            <BarButton label={drawer ? "Hide the setlist" : "Show the setlist"} on={drawer} onClick={() => setDrawer(!drawer)}>
              <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
                <rect x="2.5" y="3.5" width="15" height="13" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
                <path d="M8 4v12" stroke="currentColor" strokeWidth="1.5" />
              </svg>
            </BarButton>
            <Rule />
          </>
        )}
        <ModeSwitch compact={!landscape} />
        <UndoRedo compact />
        <span style={{ flex: 1 }} />
        {landscape ? (
          <>
            <Rule />
            <Status label="MIDI" ok detail="Morningstar MC8 · in" />
            <Status label="Audio" ok detail="voyager · 48 kHz · 128 samples" />
            <Cpu />
            <Rule />
            <Meters muted={s.houseMute} width={40} />
          </>
        ) : (
          // Health in one: green while MIDI and audio are up; the meters beside it.
          <span title="MIDI · Audio · CPU" style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 4px" }}>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
            <Meters muted={s.houseMute} width={28} />
          </span>
        )}
        <Rule />
        <HouseMute on={s.houseMute} compact={!landscape} />
      </header>

      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", position: "relative" }}>
        {macros && (
          <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderBottom: "1px solid #000" }}>
            <MacroBar cols={landscape ? 8 : 4} />
          </div>
        )}
        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
          {landscape ? <Placeholder title={view} note="Main area" /> : <Setlist />}
        </div>
        {dock && <div style={{ flexShrink: 0, borderTop: "1px solid var(--rule)" }}>{dock === "switches" ? <Switches /> : <AudioControls />}</div>}
        {/* Landscape: the setlist slides in over the stage. */}
        {landscape && drawer && (
          <>
            <button aria-label="Close the setlist" onClick={() => setDrawer(false)} style={{ position: "absolute", inset: 0, zIndex: 9, background: "rgba(0,0,0,0.5)" }} />
            <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: 402, zIndex: 10, display: "flex", flexDirection: "column", borderRight: "1px solid var(--rule-strong)", boxShadow: "16px 0 40px rgba(0,0,0,0.5)", animation: "drawer-in 200ms var(--ease) both" }}>
              <Setlist />
            </div>
            <style>{`@keyframes drawer-in { from { transform: translateX(-24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
          </>
        )}
      </div>

      <footer style={{ height: landscape ? 48 : 54, flexShrink: 0, display: "flex", alignItems: "stretch", borderTop: "1px solid var(--rule)", background: "var(--sheet)", padding: "0 2px" }}>
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
        {landscape && <span style={{ flex: 1 }} />}
        <Tab landscape={landscape} label="Tuner" onClick={() => {}}>
          <path d="M3 13a6 6 0 0 1 12 0M9 13l3-5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </Tab>
      </footer>
    </div>
  );
}

/** A foot button that shares the bar evenly in portrait, sized to itself in landscape. */
function Tab({ landscape, children, ...rest }: { landscape?: boolean; label: string; on?: boolean; pin?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <span style={{ display: "flex", flex: landscape ? undefined : "1 1 0", minWidth: 0 }}>
      <FootButton {...rest}>{children}</FootButton>
    </span>
  );
}

/** Preset · Profile · Setlist, sized for the phone's bar. */
function ModeSwitch({ compact }: { compact?: boolean }) {
  const s = useStore();
  return (
    <div role="radiogroup" aria-label="Footswitch mode" style={{ display: "flex", alignItems: "center", gap: 2, padding: compact ? "0 4px 0 6px" : "0 6px" }}>
      {MODES.map((m) => {
        const on = s.performMode === m.id;
        return (
          <button
            key={m.id}
            role="radio"
            aria-checked={on}
            title={m.hint}
            className={on ? "" : "pressable"}
            onClick={() => setPerformMode(m.id)}
            style={{
              height: compact ? 34 : 32,
              padding: compact ? "0 9px" : "0 12px",
              borderRadius: "var(--r)",
              fontSize: 13,
              fontWeight: on ? 700 : 560,
              color: on ? "var(--ink)" : "var(--ink-3)",
              background: on ? "var(--pressed-bg)" : "transparent",
              boxShadow: on ? "var(--pressed-shadow), inset 0 -2px 0 var(--live)" : undefined,
            }}
          >
            {m.label}
          </button>
        );
      })}
    </div>
  );
}
