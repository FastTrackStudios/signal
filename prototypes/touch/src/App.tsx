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
import { StageCtx } from "./ui/stage";
import { MuteButton, PanicButton } from "./ui/Safety";
import { Indicator } from "./ui/Settings";
import { ModeButton } from "./ui/ModeButton";
import { Setlist } from "./setlist/Setlist";
import { AudioControls, Switches } from "./dock/Switches";
import { MacroBar } from "./dock/MacroBar";
import { Phone } from "./Phone";
import { PhoneShell } from "./PhoneShell";
import { redo, undo, useUndo, useStore, type PerformMode } from "./store";

const SIDEBAR = 402;
const TOP = 48;
const FOOT = 56;

export type View = "play" | "control" | "routing" | "tones";
export type Dock = "switches" | "audio" | null;

// ── The stage: the iPad at its real size ─────────────────────────────
//
// Each iPad's screen in landscape, in points, and its glass in inches
// (pixels ÷ 264 ppi; the mini 326): the 11" iPad Pro (M4) is 1210 × 834
// points, 9.17 × 6.32 in. Shown at that size, a real iPad held against the
// monitor covers it exactly — and the layout is laid out at its points. A browser
// can't know its monitor's pixels per inch, so it starts from the studio
// monitor's — 93.7 CSS px per inch, traced against a real iPad Pro 11" (M1)
// (71% here) — and a slider matches it to the iPad in your hand; the match
// is remembered.

type Size = "actual" | "fit" | "points";

/** iPads in landscape: the screen in points, its glass (pixels at its
 *  ppi), the body in mm (Apple's tech specs, long side first), the screen's
 *  corner radius in points, and where the front camera sits in landscape.
 *  The body's corners are concentric with the screen's: radius = the
 *  screen's corner + the bezel. */
const MODELS = [
  { id: "pro11", name: "iPad Pro 11″ (M1 · M2 · 2018–22)", w: 1194, h: 834, px: 2388, ppi: 264, body: [247.6, 178.5], corner: 18, camera: "short" },
  { id: "pro11-m4", name: "iPad Pro 11″ (M4)", w: 1210, h: 834, px: 2420, ppi: 264, body: [249.7, 177.5], corner: 18, camera: "long" },
  { id: "air11", name: "iPad Air (M1 10.9″ · M2 11″) · iPad 10.9″", w: 1180, h: 820, px: 2360, ppi: 264, body: [247.6, 178.5], corner: 18, camera: "short" },
  { id: "pro13-m4", name: "iPad Pro 13″ (M4)", w: 1376, h: 1032, px: 2752, ppi: 264, body: [281.6, 215.5], corner: 18, camera: "long" },
  { id: "air13", name: "iPad Air 13″ · Pro 12.9″", w: 1366, h: 1024, px: 2732, ppi: 264, body: [280.6, 214.9], corner: 18, camera: "short" },
  { id: "mini", name: "iPad mini", w: 1133, h: 744, px: 2266, ppi: 326, body: [195.4, 134.8], corner: 21.5, camera: "short" },
] as const;
type Model = (typeof MODELS)[number];

function readNumber(key: string, fallback: number): number {
  try {
    const v = Number(localStorage.getItem(key));
    return Number.isFinite(v) && v > 0 ? v : fallback;
  } catch {
    return fallback;
  }
}
function save(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* private window: not remembered */
  }
}

export function App() {
  const [size, setSize] = useState<Size>(() => {
    try {
      return (localStorage.getItem("stage.size") as Size) || "actual";
    } catch {
      return "actual";
    }
  });
  const [showBody, setShowBody] = useState(() => {
    try {
      return localStorage.getItem("stage.body") !== "off";
    } catch {
      return true;
    }
  });
  const [ppi, setPpi] = useState(() => readNumber("stage.ppi", 93.7));
  const [modelId, setModelId] = useState<string>(() => {
    try {
      return localStorage.getItem("stage.model") || "pro11";
    } catch {
      return "pro11";
    }
  });
  const model: Model = MODELS.find((m) => m.id === modelId) ?? MODELS[0];
  const { w: W, h: H } = model;
  // The body around the glass, in the screen's points.
  const mmPerPt = ((model.px / model.ppi) * 25.4) / W;
  const bezel = showBody ? { x: (model.body[0] / mmPerPt - W) / 2, y: (model.body[1] / mmPerPt - H) / 2 } : { x: 0, y: 0 };
  const OW = W + bezel.x * 2;
  const OH = H + bezel.y * 2;
  const [calibrating, setCalibrating] = useState(false);
  const [win, setWin] = useState({ w: window.innerWidth, h: window.innerHeight });
  useEffect(() => {
    const on = () => setWin({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  const fit = Math.min((win.w - 32) / OW, (win.h - 96) / OH);
  // Actual size: the glass's inches (pixels ÷ density) in this screen's pixels.
  const scale = size === "points" ? 1 : size === "fit" ? fit : ((model.px / model.ppi) * ppi) / W;
  const pick = (v: Size) => {
    setSize(v);
    save("stage.size", v);
  };
  return (
    <div style={{ minHeight: "100%", display: "flex", flexDirection: "column", alignItems: "center", gap: 12, padding: "12px 16px 24px", overflow: "auto" }}>
      <StageBar showBody={showBody} onBody={() => { setShowBody(!showBody); save("stage.body", showBody ? "off" : "on"); }} model={model} onModel={(id) => { setModelId(id); save("stage.model", id); }} size={size} onSize={pick} scale={scale} ppi={ppi} calibrating={calibrating} onCalibrate={() => { setCalibrating(!calibrating); pick("actual"); }} onPpi={(v) => { setPpi(v); save("stage.ppi", String(v)); }} />
      {/* The side menu, drawn out, on the iPad's left. */}
      <div style={{ display: "flex", alignItems: "flex-end", gap: 24 }}>
        <Phone pagePpi={(scale * W) / (model.px / model.ppi)}>
          <PhoneShell menuOpen />
        </Phone>
        <div style={{ width: OW * scale, height: OH * scale, flexShrink: 0 }}>
          <div style={{ width: OW, height: OH, transform: scale === 1 ? undefined : `scale(${scale})`, transformOrigin: "0 0" }}>
            <Body model={model} bezel={bezel} show={showBody}>
              <Device scale={scale} w={W} h={H} corner={model.corner} />
            </Body>
          </div>
        </div>
      </div>
      {/* The phones at the same real scale: upright on three pages — the
          setlist, the switches, the macros — and on its side, the stage.
          They wrap to the window's width. */}
      <div style={{ alignSelf: "stretch", display: "flex", flexWrap: "wrap", justifyContent: "center", alignItems: "flex-end", gap: 24, marginTop: 12 }}>
        {(["setlist", "switches", "macros"] as const).map((pg) => (
          <Phone key={pg} pagePpi={(scale * W) / (model.px / model.ppi)}>
            <PhoneShell page={pg} />
          </Phone>
        ))}
        <Phone landscape pagePpi={(scale * W) / (model.px / model.ppi)}>
          <PhoneShell />
        </Phone>
      </div>
    </div>
  );
}

function StageBar({ showBody, onBody, model, onModel, size, onSize, scale, ppi, calibrating, onCalibrate, onPpi }: { showBody: boolean; onBody: () => void; model: Model; onModel: (id: string) => void; size: Size; onSize: (s: Size) => void; scale: number; ppi: number; calibrating: boolean; onCalibrate: () => void; onPpi: (v: number) => void }) {
  const opt = (v: Size, label: string) => (
    <button
      key={v}
      onClick={() => onSize(v)}
      aria-pressed={size === v}
      className={size === v ? "" : "pressable"}
      style={{ height: 30, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: size === v ? 700 : 560, color: size === v ? "var(--ink)" : "var(--ink-3)", background: size === v ? "var(--pressed-bg)" : "transparent", boxShadow: size === v ? "var(--pressed-shadow)" : undefined }}
    >
      {label}
    </button>
  );
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 13, color: "var(--ink-3)" }}>
        <select
          value={model.id}
          onChange={(e) => onModel(e.target.value)}
          aria-label="iPad model"
          style={{ height: 30, marginRight: 6, padding: "0 8px", borderRadius: "var(--r)", border: "1px solid var(--rule-strong)", background: "var(--sheet)", color: "var(--ink-2)", font: "inherit", fontSize: 13 }}
        >
          {MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} · {m.w} × {m.h}
            </option>
          ))}
        </select>
        {opt("actual", "Actual size")}
        {opt("fit", "Fit")}
        {opt("points", "1 : 1")}
        <span style={{ width: 1, height: 18, background: "var(--rule)", margin: "0 6px" }} />
        <button className="pressable" onClick={onBody} aria-pressed={showBody} style={{ height: 30, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 600, color: showBody ? "var(--ink)" : "var(--ink-3)" }}>
          {showBody ? "Body on" : "Body off"}
        </button>
        <button className="pressable" onClick={onCalibrate} aria-pressed={calibrating} style={{ height: 30, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 600, color: calibrating ? "var(--ink)" : "var(--ink-3)" }}>
          {calibrating ? "Done matching" : "Match my iPad…"}
        </button>
        <span className="num" style={{ marginLeft: 6, fontSize: 12 }}>
          {Math.round(scale * 100)}%
        </span>
      </div>
      {calibrating && (
        <div style={{ display: "flex", alignItems: "center", gap: 12, fontSize: 13, color: "var(--ink-2)" }}>
          <span>{showBody ? "Lay your iPad over the outline and slide until its edges trace it" : "Hold your iPad against the screen and slide until the frame matches its lit screen"}</span>
          <input type="range" min={70} max={240} step={0.25} value={ppi} onChange={(e) => onPpi(Number(e.target.value))} style={{ width: 240 }} aria-label="Pixels per inch" />
          <span className="num" style={{ width: 80, color: "var(--ink-3)" }}>{ppi.toFixed(1)} px/in</span>
        </div>
      )}
    </div>
  );
}

/** The iPad's body around the glass: black bezels, corners concentric with
 *  the screen's, the aluminium edge as a traceable line, the camera. */
function Body({ model, bezel, show, children }: { model: Model; bezel: { x: number; y: number }; show: boolean; children: ReactNode }) {
  if (!show) return <div style={{ borderRadius: model.corner, boxShadow: "0 24px 60px rgba(0,0,0,0.5)", outline: "1px solid var(--rule-strong)" }}>{children}</div>;
  const outer = model.corner + Math.min(bezel.x, bezel.y);
  const long = model.camera === "long";
  return (
    <div
      style={{
        position: "relative",
        padding: `${bezel.y}px ${bezel.x}px`,
        borderRadius: outer,
        background: "#050506",
        // The edge to trace: a bright hairline exactly on the body's outline.
        boxShadow: "inset 0 0 0 1.5px #a1a1aa, inset 0 0 0 4px #2a2a2e, 0 24px 60px rgba(0,0,0,0.5)",
      }}
    >
      <span
        aria-hidden
        style={{
          position: "absolute",
          width: 9,
          height: 9,
          borderRadius: 999,
          background: "#16161a",
          boxShadow: "inset 0 0 0 2px #0b0b0d",
          ...(long ? { top: bezel.y / 2 - 4.5, left: "50%", marginLeft: -4.5 } : { left: bezel.x / 2 - 4.5, top: "50%", marginTop: -4.5 }),
        }}
      />
      {children}
    </div>
  );
}

function Device({ scale, w: W, h: H, corner }: { scale: number; w: number; h: number; corner: number }) {
  const s = useStore();
  const [chosen, setView] = useState<View>("play");
  // Routing and Tones edit a preset: outside Preset mode, Play shows instead.
  const view: View = s.performMode !== "preset" && (chosen === "routing" || chosen === "tones") ? "play" : chosen;
  const [dock, setDock] = useState<Dock>("switches");
  const [macros, setMacros] = useState(true);
  const [sidebar, setSidebar] = useState(true);
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  return (
    <StageCtx.Provider value={{ el, scale }}>
      <div
        ref={setEl}
        style={{
          position: "relative",
          width: W,
          height: H,
          display: "flex",
          flexDirection: "column",
          background: "var(--desk)",
          borderRadius: corner,
          overflow: "hidden",
        }}
      >
        <TopBar sidebar={sidebar} onSidebar={() => setSidebar(!sidebar)} />
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          {sidebar && <Sidebar />}
          <Main view={view} dock={dock} macros={macros} />
        </div>
        <BottomBar view={view} onView={setView} dock={dock} onDock={(d) => setDock(dock === d ? null : d)} macros={macros} onMacros={() => setMacros(!macros)} />
      </div>
    </StageCtx.Provider>
  );
}

// ── Top: the rig ─────────────────────────────────────────────────────

export const MODES: { id: PerformMode; label: string; hint: string }[] = [
  { id: "preset", label: "Preset", hint: "Footswitches pick presets and their variations" },
  { id: "profile", label: "Profile", hint: "Footswitches play the profile's stacks" },
  { id: "setlist", label: "Setlist", hint: "Footswitches step through the set: songs, sections, parts" },
];

function TopBar({ sidebar, onSidebar }: { sidebar: boolean; onSidebar: () => void }) {
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
        {/* The footswitch mode: one button, its menu picks. */}
        <ModeButton wide />
        <Rule />
        <span style={{ flex: 1 }} />
        {/* Undo and Redo, for the whole app: every edit anywhere is one step. */}
        <UndoRedo />
      </div>

      <span style={{ flex: 1 }} />

      {/* Health and safety: always in view. */}
      <Rule />
      {/* Panic sits with the health it fixes — far from Mute. */}
      <PanicButton />
      <Rule />
      <Indicator kind="midi" />
      <Indicator kind="audio" />
      <Cpu />
      <Rule />
      <Meters width={128} />
      <Rule />
      <MuteButton />
    </header>
  );
}

export function UndoRedo({ compact }: { compact?: boolean } = {}) {
  const { depth, label, redoDepth, redoLabel } = useUndo();
  const btn = (on: boolean, title: string, onClick: () => void, flip: boolean) => (
    <button
      onClick={onClick}
      disabled={!on}
      aria-label={title}
      title={title}
      className={on ? "pressable" : ""}
      style={{ width: compact ? 34 : 48, display: "flex", alignItems: "center", justifyContent: "center", color: on ? "var(--ink-2)" : "var(--dim)" }}
    >
      <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden style={{ transform: flip ? "scaleX(-1)" : undefined }}>
        <path d="M7 4.5 3.5 8 7 11.5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M4 8h7.5a4.5 4.5 0 0 1 0 9H9" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
      </svg>
    </button>
  );
  return (
    <div style={{ display: "flex", alignItems: "stretch", paddingLeft: compact ? 0 : 4 }}>
      {btn(depth > 0, label ? `Undo ${label}` : "Nothing to undo", undo, false)}
      {btn(redoDepth > 0, redoLabel ? `Redo ${redoLabel}` : "Nothing to redo", redo, true)}
    </div>
  );
}

/** CPU, as a number that jitters the way a real one does. */
export function Cpu() {
  const [cpu, setCpu] = useState(14);
  useEffect(() => {
    const t = window.setInterval(() => setCpu((c) => Math.max(9, Math.min(24, Math.round(c + (Math.random() - 0.5) * 4)))), 900);
    return () => window.clearInterval(t);
  }, []);
  // Green while easy, amber from 60%, red from 85% (where xruns start).
  const tone = cpu >= 85 ? "var(--void)" : cpu >= 60 ? "var(--modified)" : "var(--ink-3)";
  return (
    <span title={`DSP load on the rig: ${cpu}%`} style={{ display: "flex", alignItems: "center", gap: 3, padding: "0 6px", fontSize: 11.5, fontWeight: 700, color: tone }}>
      <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden>
        <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
        <rect x="6" y="6" width="4" height="4" rx="0.5" fill="currentColor" />
        <path d="M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      </svg>
      <span className="num" style={{ minWidth: 22 }}>{cpu}%</span>
    </span>
  );
}

/** IN, OUT and PHONES, as big as their spot allows: no padding, three
 *  bars filling the bar's height, as wide as given. OUT is covered while
 *  the house is muted (MUTE HOUSE; MUTED when fully muted), PH while your
 *  guitar is out of the phones; all go flat while Panic resets. Simulated
 *  here; the rig streams peaks. */
export function Meters({ width = 96 }: { width?: number }) {
  const s = useStore();
  const [lv, setLv] = useState({ i: 0.4, o: 0.5, p: 0.45, ih: 0.4, oh: 0.5, ph: 0.45 });
  const raf = useRef(0);
  useEffect(() => {
    let t = 0;
    let hold = { i: 0, o: 0, p: 0, at: 0 };
    const tick = () => {
      t += 1;
      const strum = Math.max(0, Math.sin(t / 22)) ** 3;
      const i = Math.min(1, 0.25 + 0.55 * strum + Math.random() * 0.08);
      const o = Math.min(1, 0.3 + 0.5 * strum + Math.random() * 0.06);
      const p = Math.min(1, 0.28 + 0.45 * strum + Math.random() * 0.06);
      const stale = t - hold.at > 60;
      if (i > hold.i || o > hold.o || p > hold.p || stale) hold = { i: Math.max(i, stale ? 0 : hold.i), o: Math.max(o, stale ? 0 : hold.o), p: Math.max(p, stale ? 0 : hold.p), at: t };
      setLv({ i, o, p, ih: hold.i, oh: hold.o, ph: hold.p });
      raf.current = requestAnimationFrame(tick);
    };
    if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) raf.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf.current);
  }, []);
  const reset = s.panicAt !== null;
  const full = s.houseMute && s.phonesMute;
  return (
    <div style={{ alignSelf: "stretch", width, flexShrink: 0, display: "flex", flexDirection: "column", gap: 2, padding: "4px 0" }}>
      <MiniMeter label="IN" level={reset ? 0 : lv.i} hold={reset ? 0 : lv.ih} />
      <MiniMeter label="OUT" level={reset ? 0 : lv.o} hold={reset ? 0 : lv.oh} cover={s.houseMute ? (full ? "MUTED" : "MUTE HOUSE") : undefined} />
      <MiniMeter label="PH" level={reset ? 0 : lv.p} hold={reset ? 0 : lv.ph} cover={s.phonesMute ? "MUTED" : undefined} />
    </div>
  );
}

/** One meter: its label inside the bar's left end, the bar filling the row. */
export function MiniMeter({ label, level, hold, cover }: { label: string; level: number; hold: number; cover?: string }) {
  return (
    <div title={cover ? `${label}: ${cover}` : label} style={{ position: "relative", flex: 1, minHeight: 0, borderRadius: 2, overflow: "hidden", background: cover ? "var(--void)" : "#08080a" }}>
      {cover ? (
        <span style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 7.5, fontWeight: 800, letterSpacing: "0.1em", color: "#1a0505", whiteSpace: "nowrap" }}>{cover}</span>
      ) : (
        <>
          {/* The full scale, revealed up to the level: green, then amber, red at the top. */}
          <span
            style={{
              position: "absolute",
              inset: 0,
              background: "linear-gradient(90deg, #15803d 0%, var(--live) 65%, #eab308 85%, var(--void) 100%)",
              clipPath: `inset(0 ${(1 - level) * 100}% 0 0)`,
              opacity: 0.9,
            }}
          />
          {hold > 0.02 && <span style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${hold * 100}% - 1px)`, width: 2, background: hold > 0.92 ? "var(--void)" : "var(--ink)" }} />}
          <span style={{ position: "absolute", left: 4, top: 0, bottom: 0, display: "flex", alignItems: "center", fontSize: 7.5, fontWeight: 800, letterSpacing: "0.08em", color: "rgba(255,255,255,0.85)", textShadow: "0 0 2px #000" }}>{label}</span>
        </>
      )}
    </div>
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

function Main({ view, dock, macros }: { view: View; dock: Dock; macros: boolean }) {
  const label = { play: "Play", control: "Control", routing: "Routing", tones: "Tones" }[view];
  return (
    <main style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--desk)" }}>
      {/* The macros, along the top of the main area: what you turn while
          playing, out of the feet's way; their panels drop down. */}
      {macros && (
        <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderBottom: "1px solid #000" }}>
          <MacroBar />
        </div>
      )}
      <div style={{ flex: 1, minHeight: 0 }}>
        <Placeholder title={label} note="Main area" />
      </div>
      {dock && <div style={{ flexShrink: 0, borderTop: "1px solid var(--rule)" }}>{dock === "switches" ? <Switches /> : <AudioControls />}</div>}
    </main>
  );
}

export function Placeholder({ title, note }: { title: string; note: string }) {
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

function BottomBar({ view, onView, dock, onDock, macros, onMacros }: { view: View; onView: (v: View) => void; dock: Dock; onDock: (d: Exclude<Dock, null>) => void; macros: boolean; onMacros: () => void }) {
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
      <FootButton label="Macros" on={macros} pin onClick={onMacros}>
        <>
          <circle cx="4.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="13.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M4.5 9 6 7.4M13.5 9l1.5-1.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <path d="M8 4.5h2M8 13.5h2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      </FootButton>
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
export function FootButton({ label, on, pin, off, onClick, children }: { label: string; on?: boolean; pin?: boolean; off?: string; onClick: () => void; children: ReactNode }) {
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

export function BarButton({ label, on, onClick, children }: { label: string; on?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button aria-label={label} title={label} aria-pressed={on} className="pressable" onClick={onClick} style={{ width: 52, display: "flex", alignItems: "center", justifyContent: "center", color: on ? "var(--ink)" : "var(--ink-3)" }}>
      {children}
    </button>
  );
}

export function Rule() {
  return <span aria-hidden style={{ width: 1, alignSelf: "center", height: 22, background: "var(--rule)", flexShrink: 0 }} />;
}
