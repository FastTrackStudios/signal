// The stage: an 11" iPad in landscape (1194 × 834 points), framed the way
// the app will be — a top bar, the setlist as the left sidebar, the main
// area (empty for now), a status bar along the foot.

import { Setlist } from "./setlist/Setlist";
import { currentSong, playing, sectionsOf, useStore } from "./store";
import { profileFor } from "./setlist/stacks";
import { songColour } from "./setlist/colors";
import { tapeFor } from "./ui/marks";
import { stackOf } from "./data/rig";

const W = 1194;
const H = 834;

export function App() {
  return (
    <div style={{ minHeight: "100%", display: "flex", alignItems: "center", justifyContent: "center", padding: 24, overflow: "auto" }}>
      <div
        style={{
          width: W,
          height: H,
          flexShrink: 0,
          display: "flex",
          flexDirection: "column",
          background: "var(--desk)",
          border: "1px solid var(--rule-strong)",
          borderRadius: 18,
          overflow: "hidden",
          boxShadow: "0 24px 60px rgba(0,0,0,0.5)",
        }}
      >
        <TopBar />
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          <aside style={{ width: 402, flexShrink: 0, borderRight: "1px solid var(--rule)", minHeight: 0 }}>
            <Setlist />
          </aside>
          <Main />
        </div>
        <StatusBar />
      </div>
    </div>
  );
}

function TopBar() {
  const s = useStore();
  const song = currentSong(s);
  const sec = song ? sectionsOf(s, song.name)[s.partIndex] : undefined;
  const now = playing(s);
  const profile = profileFor(song?.name);
  return (
    <header style={{ height: "var(--bar-h)", flexShrink: 0, display: "flex", alignItems: "center", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
      <div style={{ width: 402, flexShrink: 0, display: "flex", alignItems: "center", gap: 10, padding: "0 16px", height: "100%", borderRight: "1px solid var(--rule)" }}>
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
          <rect x="2.5" y="3.5" width="15" height="13" rx="2.5" fill="none" stroke="var(--ink-2)" strokeWidth="1.5" />
          <path d="M8 4v12" stroke="var(--ink-2)" strokeWidth="1.5" />
        </svg>
        <span style={{ fontWeight: 750, fontSize: 16, letterSpacing: "-0.01em" }}>Signal</span>
        <span className="t-meta" style={{ fontSize: 13 }}>Guitar rig</span>
      </div>
      <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 14, padding: "0 18px" }}>
        {song && <span style={{ width: 10, height: 10, borderRadius: 3, background: songColour(song.name, s.songColours), flexShrink: 0 }} />}
        <span style={{ fontWeight: 700, fontSize: 16, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{song?.name ?? "No song"}</span>
        {sec && <span style={{ color: "var(--ink-2)", fontSize: 15, whiteSpace: "nowrap" }}>{sec.parts.length > 1 ? `${sec.name} · ${sec.parts[s.subIndex]?.name}` : sec.name}</span>}
        <span style={{ flex: 1 }} />
        {now && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 7, padding: "5px 10px", borderRadius: 4, background: "rgba(255,255,255,0.06)", color: "var(--ink)", fontSize: 14, fontWeight: 650, whiteSpace: "nowrap" }}>
            <span style={{ width: 7, height: 7, borderRadius: 2, background: tapeFor(stackOf(now)) === "var(--tape-gaffer)" ? "var(--live)" : tapeFor(stackOf(now)) }} />
            {now}
          </span>
        )}
        <Divider />
        <Readout label="Profile" value={profile.name} />
        <Readout label="Tempo" value={song ? `${song.bpm}` : "—"} />
        <Readout label="Key" value={song?.key || "—"} />
      </div>
    </header>
  );
}

function Main() {
  return (
    <main style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", justifyContent: "center", background: "var(--desk)" }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, color: "var(--ink-3)" }}>
        <span className="t-label" style={{ color: "var(--dim)" }}>
          Main area
        </span>
        <span style={{ fontSize: 13, color: "var(--dim)" }}>792 × 742</span>
      </div>
    </main>
  );
}

function StatusBar() {
  return (
    <footer
      style={{ height: 36, flexShrink: 0, display: "flex", alignItems: "center", gap: 18, padding: "0 16px", borderTop: "1px solid var(--rule)", background: "var(--sheet)", fontSize: 12.5, color: "var(--ink-3)" }}
    >
      <span style={{ display: "inline-flex", alignItems: "center", gap: 7, color: "var(--ink-2)", fontWeight: 600 }}>
        <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
        voyager
      </span>
      <span>48 kHz · 128</span>
      <span>DSP 14%</span>
      <span style={{ flex: 1 }} />
      <span>In −12 dB</span>
      <span>Out −9 dB</span>
      <span>Tuner A 440</span>
    </footer>
  );
}

function Readout({ label, value }: { label: string; value: string }) {
  return (
    <span style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}>
      <span className="t-label" style={{ fontSize: 10, color: "var(--ink-3)" }}>
        {label}
      </span>
      <span style={{ fontSize: 14, fontWeight: 650, whiteSpace: "nowrap" }}>{value}</span>
    </span>
  );
}

function Divider() {
  return <span aria-hidden style={{ width: 1, height: 28, background: "var(--rule)" }} />;
}
