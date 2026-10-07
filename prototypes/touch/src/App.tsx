// The stage: an 11" iPad in landscape (1194 × 834 points), framed the way
// the app will be.
//
// The two bars split by what they answer:
//   top    — what the rig IS: which mode the footswitches step through
//            (Preset / Profile / Setlist, rig state shared by every remote),
//            and whether it is healthy and safe: MIDI, audio, CPU, input and
//            output levels, and the house mute. Not what plays — the
//            sidebar and the switches already say it.
//   bottom — what you are LOOKING at: the views (Play, Control, Routing,
//            Tones), and what is docked along the foot of the main area —
//            the Switches (footswitches + macro bar) or the Audio controls
//            (the block being played; where Frame goes), or nothing.
// Bar items sit flat and full height, hairlines between groups; the pick is
// pressed into the bar (theme::PRESSED), green when it is live rig state.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Setlist } from "./setlist/Setlist";
import { AudioControls, Switches } from "./dock/Switches";
import { setPerformMode, toggleHouseMute, useStore, type PerformMode } from "./store";

const W = 1194;
const H = 834;
const SIDEBAR = 402;
const TOP = 48;
const FOOT = 56;

type View = "play" | "control" | "routing" | "tones";
type Dock = "switches" | "audio" | null;

export function App() {
  const s = useStore();
  const [chosen, setView] = useState<View>("play");
  // Routing and Tones edit a preset: outside Preset mode, Play shows instead.
  const view: View = s.performMode !== "preset" && (chosen === "routing" || chosen === "tones") ? "play" : chosen;
  const [dock, setDock] = useState<Dock>("switches");
  const [sidebar, setSidebar] = useState(true);
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
        <TopBar sidebar={sidebar} onSidebar={() => setSidebar(!sidebar)} />
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          {sidebar && <Sidebar />}
          <Main view={view} dock={dock} />
        </div>
        <BottomBar view={view} onView={setView} dock={dock} onDock={(d) => setDock(dock === d ? null : d)} />
      </div>
    </div>
  );
}

// ── Top: the rig ─────────────────────────────────────────────────────

const MODES: { id: PerformMode; label: string; hint: string }[] = [
  { id: "preset", label: "Preset", hint: "Footswitches pick presets and their variations" },
  { id: "profile", label: "Profile", hint: "Footswitches play the profile's stacks" },
  { id: "setlist", label: "Setlist", hint: "Footswitches step through the set: songs, sections, parts" },
];

function TopBar({ sidebar, onSidebar }: { sidebar: boolean; onSidebar: () => void }) {
  const s = useStore();
  return (
    <header style={{ height: TOP, flexShrink: 0, display: "flex", alignItems: "stretch", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
      {/* Over the sidebar: its toggle and the mode the footswitches are in. */}
      <div style={{ width: SIDEBAR, flexShrink: 0, display: "flex", alignItems: "stretch", borderRight: "1px solid var(--rule)" }}>
        <BarButton label={sidebar ? "Hide the setlist" : "Show the setlist"} onClick={onSidebar} on={sidebar}>
          <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden>
            <rect x="2.5" y="3.5" width="15" height="13" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
            <path d="M8 4v12" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </BarButton>
        <Rule />
        <div role="radiogroup" aria-label="Footswitch mode" style={{ flex: 1, display: "flex", alignItems: "center", gap: 4, padding: "0 8px" }}>
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
                  flex: 1,
                  height: 36,
                  borderRadius: "var(--r)",
                  fontSize: 14,
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
      </div>

      <span style={{ flex: 1 }} />

      {/* Health and safety: always in view. */}
      <Rule />
      <Status label="MIDI" ok detail="Morningstar MC8 · in" />
      <Status label="Audio" ok detail="voyager · 48 kHz · 128 samples" />
      <Cpu />
      <Rule />
      <Meters muted={s.houseMute} />
      <Rule />
      <HouseMute on={s.houseMute} />
    </header>
  );
}

function Status({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  return (
    <button className="pressable" title={detail} style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 10px", fontSize: 12, fontWeight: 650, color: "var(--ink-2)" }}>
      <span style={{ width: 7, height: 7, borderRadius: 999, background: ok ? "var(--live)" : "var(--void)" }} />
      {label}
    </button>
  );
}

/** CPU, as a number that jitters the way a real one does. */
function Cpu() {
  const [cpu, setCpu] = useState(14);
  useEffect(() => {
    const t = window.setInterval(() => setCpu((c) => Math.max(9, Math.min(24, Math.round(c + (Math.random() - 0.5) * 4)))), 900);
    return () => window.clearInterval(t);
  }, []);
  return (
    <span title="DSP load on the rig" style={{ display: "flex", alignItems: "center", gap: 5, padding: "0 10px 0 4px", fontSize: 12, fontWeight: 650, color: "var(--ink-3)" }}>
      CPU <span className="num" style={{ color: cpu > 70 ? "var(--modified)" : "var(--ink-2)", width: 26 }}>{cpu}%</span>
    </span>
  );
}

/** IN and OUT, small: enough to see signal and clipping at a glance (the
 *  views carry the big ones). Simulated here; the rig streams peaks. */
function Meters({ muted }: { muted: boolean }) {
  const [lv, setLv] = useState({ i: 0.4, o: 0.5, ih: 0.4, oh: 0.5 });
  const raf = useRef(0);
  useEffect(() => {
    let t = 0;
    let hold = { i: 0, o: 0, at: 0 };
    const tick = () => {
      t += 1;
      const strum = Math.max(0, Math.sin(t / 22)) ** 3;
      const i = Math.min(1, 0.25 + 0.55 * strum + Math.random() * 0.08);
      const o = Math.min(1, 0.3 + 0.5 * strum + Math.random() * 0.06);
      if (i > hold.i || o > hold.o || t - hold.at > 60) hold = { i: Math.max(i, t - hold.at > 60 ? 0 : hold.i), o: Math.max(o, t - hold.at > 60 ? 0 : hold.o), at: t };
      setLv({ i, o, ih: hold.i, oh: hold.o });
      raf.current = requestAnimationFrame(tick);
    };
    if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) raf.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf.current);
  }, []);
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", gap: 5, padding: "0 12px" }}>
      <MiniMeter label="IN" level={lv.i} hold={lv.ih} />
      <MiniMeter label="OUT" level={muted ? 0 : lv.o} hold={muted ? 0 : lv.oh} dim={muted} />
    </div>
  );
}

function MiniMeter({ label, level, hold, dim }: { label: string; level: number; hold: number; dim?: boolean }) {
  const w = 72;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <span className="t-label" style={{ width: 24, fontSize: 9, letterSpacing: "0.08em", color: dim ? "var(--void)" : "var(--ink-3)" }}>
        {label}
      </span>
      <span style={{ position: "relative", width: w, height: 5, borderRadius: 2, background: "var(--field)", overflow: "hidden" }}>
        <span
          style={{
            position: "absolute",
            inset: 0,
            width: `${level * 100}%`,
            background: "linear-gradient(90deg, var(--live) 0%, var(--live) 70%, #eab308 85%, var(--void) 100%)",
            backgroundSize: `${w}px 100%`,
          }}
        />
        {hold > 0.02 && <span style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${hold * 100}% - 1px)`, width: 2, background: hold > 0.92 ? "var(--void)" : "var(--ink-2)" }} />}
      </span>
    </div>
  );
}

/** The house mute: the main outputs go silent, the phones keep playing —
 *  rehearse, tune, or fix something mid-set. Unmissable when on. */
function HouseMute({ on }: { on: boolean }) {
  return (
    <button
      onClick={toggleHouseMute}
      aria-pressed={on}
      title={on ? "The house is muted — phones still play. Tap to unmute." : "Mute the house; the phones keep playing"}
      className={on ? "" : "pressable"}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        margin: 6,
        padding: "0 14px",
        borderRadius: "var(--r)",
        fontSize: 12,
        fontWeight: 750,
        letterSpacing: "0.06em",
        color: on ? "#1a0505" : "var(--ink-2)",
        background: on ? "var(--void)" : "transparent",
        boxShadow: on ? undefined : "inset 0 0 0 1px var(--rule-strong)",
        animation: on ? "house-pulse 1.6s ease-in-out infinite" : undefined,
      }}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden>
        <path d="M2 6h2.5L8 3v10L4.5 10H2Z" fill="currentColor" />
        {on ? (
          <path d="M10.5 6l4 4M14.5 6l-4 4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        ) : (
          <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12.5 3.8a6 6 0 0 1 0 8.4" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        )}
      </svg>
      {on ? "HOUSE MUTED" : "MUTE HOUSE"}
      <style>{`@keyframes house-pulse { 50% { filter: brightness(0.82) } } @media (prefers-reduced-motion: reduce) { [aria-pressed="true"] { animation: none !important } }`}</style>
    </button>
  );
}

// ── The body ─────────────────────────────────────────────────────────

function Sidebar() {
  const s = useStore();
  return (
    <aside style={{ width: SIDEBAR, flexShrink: 0, borderRight: "1px solid var(--rule)", minHeight: 0 }}>
      {s.performMode === "setlist" ? (
        <Setlist />
      ) : (
        <Placeholder title={s.performMode === "preset" ? "Presets" : "Profile stacks"} note="The sidebar for this mode — next." />
      )}
    </aside>
  );
}

function Main({ view, dock }: { view: View; dock: Dock }) {
  const label = { play: "Play", control: "Control", routing: "Routing", tones: "Tones" }[view];
  return (
    <main style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--desk)" }}>
      <div style={{ flex: 1, minHeight: 0 }}>
        <Placeholder title={label} note="Main area" />
      </div>
      {dock && <div style={{ flexShrink: 0, borderTop: "1px solid var(--rule)" }}>{dock === "switches" ? <Switches /> : <AudioControls />}</div>}
    </main>
  );
}

function Placeholder({ title, note }: { title: string; note: string }) {
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 6 }}>
      <span className="t-label" style={{ color: "var(--dim)" }}>
        {title}
      </span>
      <span style={{ fontSize: 13, color: "var(--dim)" }}>{note}</span>
    </div>
  );
}

// ── Bottom: what you look at ─────────────────────────────────────────

function BottomBar({ view, onView, dock, onDock }: { view: View; onView: (v: View) => void; dock: Dock; onDock: (d: Exclude<Dock, null>) => void }) {
  const s = useStore();
  const presetOnly = s.performMode !== "preset";
  const views: { id: View; label: string; icon: ReactNode; off?: string }[] = [
    { id: "play", label: "Play", icon: <path d="M5 3.5v11l9-5.5Z" fill="currentColor" /> },
    {
      id: "control",
      label: "Control",
      icon: (
        <>
          <path d="M4 2.5v13M9 2.5v13M14 2.5v13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <rect x="2.3" y="10" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="7.3" y="5" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="12.3" y="8" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
        </>
      ),
    },
    {
      id: "routing",
      label: "Routing",
      off: presetOnly ? "Routing edits a preset — switch to Preset mode" : undefined,
      icon: <path d="M2.5 5h4l5 8h4M2.5 13h4l1.6-2.5M11.5 5h4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />,
    },
    {
      id: "tones",
      label: "Tones",
      off: presetOnly ? "Tones edits a preset — switch to Preset mode" : undefined,
      icon: <path d="M2 9c1.5-4 3-4 4.5 0s3 4 4.5 0 3-4 4.5 0" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />,
    },
  ];
  return (
    <footer style={{ height: FOOT, flexShrink: 0, display: "flex", alignItems: "stretch", borderTop: "1px solid var(--rule)", background: "var(--sheet)", padding: "0 6px" }}>
      {views.map((v) => (
        <FootButton key={v.id} label={v.label} on={view === v.id} off={v.off} onClick={() => onView(v.id)}>
          {v.icon}
        </FootButton>
      ))}
      <Rule />
      {/* The dock along the foot of the main area: one or the other, or none. */}
      <FootButton label="Switches" on={dock === "switches"} pin onClick={() => onDock("switches")}>
        <>
          <rect x="2" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="7" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="12" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </>
      </FootButton>
      <FootButton label="Audio" on={dock === "audio"} pin onClick={() => onDock("audio")}>
        <>
          <circle cx="5" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M5 9 6.6 6.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <circle cx="13" cy="9" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M13 9 11.2 7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      </FootButton>
      <span style={{ flex: 1 }} />
      <FootButton label="Tuner" onClick={() => {}}>
        <path d="M3 13a6 6 0 0 1 12 0M9 13l3-5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </FootButton>
      <FootButton label="Library" onClick={() => {}}>
        <path d="M3 3.5h3v11H3ZM7.5 3.5h3v11h-3ZM12 4l2.8-.8 2 10.6-2.8.8Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
      </FootButton>
    </footer>
  );
}

/** A bar button in the foot: an icon, its word under it (Session's foot). */
function FootButton({ label, on, pin, off, onClick, children }: { label: string; on?: boolean; pin?: boolean; off?: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      disabled={!!off}
      title={off ?? label}
      aria-pressed={on}
      className={on || off ? "" : "pressable"}
      style={{
        minWidth: 64,
        margin: "5px 2px",
        padding: "0 10px",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 3,
        borderRadius: "var(--r)",
        color: off ? "var(--dim)" : on ? "var(--ink)" : "var(--ink-3)",
        background: on ? "var(--pressed-bg)" : "transparent",
        boxShadow: on ? `var(--pressed-shadow)${pin ? ", inset 0 -2px 0 var(--ink-2)" : ""}` : undefined,
      }}
    >
      <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden>
        {children}
      </svg>
      <span style={{ fontSize: 10.5, fontWeight: on ? 700 : 600, letterSpacing: "0.02em" }}>{label}</span>
    </button>
  );
}

function BarButton({ label, on, onClick, children }: { label: string; on?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button aria-label={label} title={label} aria-pressed={on} className="pressable" onClick={onClick} style={{ width: 52, display: "flex", alignItems: "center", justifyContent: "center", color: on ? "var(--ink)" : "var(--ink-3)" }}>
      {children}
    </button>
  );
}

function Rule() {
  return <span aria-hidden style={{ width: 1, alignSelf: "center", height: 22, background: "var(--rule)", flexShrink: 0 }} />;
}
