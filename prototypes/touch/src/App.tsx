// The stage: the setlist component alone, at the size it lives at — a
// phone's width (the sidebar, 402pt) by an 11" iPad's height held
// landscape (834pt).

import { Setlist } from "./setlist/Setlist";

export function App() {
  return (
    <div style={{ minHeight: "100%", display: "flex", alignItems: "center", justifyContent: "center", padding: 24, overflow: "auto" }}>
      <div
        style={{
          width: 402,
          height: 834,
          flexShrink: 0,
          border: "1px solid var(--rule)",
          borderRadius: "var(--r-md)",
          overflow: "hidden",
          boxShadow: "0 24px 60px rgba(0,0,0,0.5)",
        }}
      >
        <Setlist />
      </div>
    </div>
  );
}
