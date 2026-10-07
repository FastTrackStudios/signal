// Signal Touch: the shell. One bar (the modes, the profile, Undo, the rig's
// state), the mode's page, and the chain strip along the bottom of every page
// — the sound is always in view.

import { useState } from "react";
import { rig } from "./data/rig";
import { undo, useUndo } from "./store";
import { ChainStrip } from "./ui/ChainStrip";
import { Tabs } from "./ui/kit";
import { Tape } from "./ui/marks";
import { LibraryView } from "./views/LibraryView";
import { RoutingView } from "./views/RoutingView";
import { SetView } from "./views/SetView";
import { SoundsView } from "./views/SoundsView";

const MODES = ["Set", "Sounds", "Library", "Routing"] as const;
type Mode = (typeof MODES)[number];

export function App() {
  const [mode, setMode] = useState<Mode>(() => (new URLSearchParams(location.search).get("mode") as Mode) || "Set");
  const { depth, label } = useUndo();
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column" }}>
      <header
        style={{
          height: "var(--bar-h)",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 16,
          padding: "0 14px 0 20px",
          background: "var(--sheet)",
          borderBottom: "1px solid var(--rule-strong)",
        }}
      >
        <span className="t-marker" style={{ fontSize: 24, letterSpacing: "-0.03em" }}>
          signal
        </span>
        <span style={{ width: 1, height: 28, background: "var(--rule)" }} />
        <Tabs options={MODES} value={mode} onChange={setMode} size="lg" />
        <span style={{ flex: 1 }} />
        <Tape colour="var(--tape-gaffer)" tilt={-1} style={{ fontSize: 14, padding: "7px 11px" }}>
          {rig.perf.profile_name}
        </Tape>
        <button
          className="pressable"
          onClick={undo}
          disabled={depth === 0}
          title={label ? `Undo: ${label}` : "Nothing to undo"}
          style={{
            minHeight: 48,
            padding: "0 16px",
            display: "inline-flex",
            alignItems: "center",
            gap: 8,
            border: "1px solid var(--rule-strong)",
            borderRadius: "var(--r)",
            color: depth ? "var(--ink)" : "var(--ink-3)",
            fontWeight: 780,
            maxWidth: 300,
          }}
        >
          <svg width="18" height="16" viewBox="0 0 18 16" aria-hidden>
            <path d="M6 1 L2 5 L6 9 M2 5 H11 a5 5 0 0 1 0 10 H8" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{label ? `Undo ${label}` : "Undo"}</span>
        </button>
        {/* The prototype runs on an exported copy of the rig: say so. */}
        <span className="t-meta" style={{ display: "inline-flex", alignItems: "center", gap: 8, minHeight: 48, padding: "0 6px", fontWeight: 700 }} title="A copy of the rig's data, exported from the engine — nothing here plays or saves">
          <span style={{ width: 10, height: 10, borderRadius: 999, border: "2px solid var(--ink-3)" }} />
          Demo rig · no audio
        </span>
      </header>
      <main style={{ flex: 1, minHeight: 0, position: "relative" }}>
        {mode === "Set" && <SetView />}
        {mode === "Sounds" && <SoundsView />}
        {mode === "Library" && <LibraryView />}
        {mode === "Routing" && <RoutingView />}
      </main>
      <ChainStrip />
    </div>
  );
}
