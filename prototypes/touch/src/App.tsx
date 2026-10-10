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

import { useEffect, useState, type ReactNode } from "react";
import { StageCtx } from "./ui/stage";
import { setSignalTempo, useSignal } from "./ui/signal";
import { MuteButton, PanicButton } from "./ui/Safety";
import { Indicator } from "./ui/Settings";
import { ModeButton } from "./ui/ModeButton";
import { ComposeCtx, SidebarContent } from "./setlist/Setlist";
import { Switches } from "./dock/Switches";
import { Browser } from "./views/Browser";
import { EditView } from "./views/Edit";
import { OpenSetup, SetupView, type SetupTab } from "./views/Setup";
import { MacroBar } from "./dock/MacroBar";
import { Phone } from "./Phone";
import { PhoneShell } from "./PhoneShell";
import { currentSong, useStore, type PerformMode } from "./store";

const SIDEBAR = 402;
const TOP = 48;
const FOOT = 56;

/** What the screen is for: playing (Perform), putting setlists, profiles
 *  and presets together from the browser (Build), or taking a sound apart
 *  (Edit). */
export type View = "perform" | "build" | "edit" | "setup";
export type Dock = "switches" | null;

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

type Size = "actual" | "fit";

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
      // Actual size is 100%: an old "1 : 1" choice comes back as it.
      return localStorage.getItem("stage.size") === "fit" ? "fit" : "actual";
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
  const actual = ((model.px / model.ppi) * ppi) / W;
  const scale = size === "fit" ? fit : actual;
  const pick = (v: Size) => {
    setSize(v);
    save("stage.size", v);
  };
  return (
    <div style={{ minHeight: "100%", display: "flex", flexDirection: "column", alignItems: "center", gap: 12, padding: "12px 16px 24px", overflow: "auto" }}>
      <StageBar showBody={showBody} onBody={() => { setShowBody(!showBody); save("stage.body", showBody ? "off" : "on"); }} model={model} onModel={(id) => { setModelId(id); save("stage.model", id); }} size={size} onSize={pick} scale={scale} actual={actual} ppi={ppi} calibrating={calibrating} onCalibrate={() => { setCalibrating(!calibrating); pick("actual"); }} onPpi={(v) => { setPpi(v); save("stage.ppi", String(v)); }} />
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

function StageBar({ showBody, onBody, model, onModel, size, onSize, scale, actual, ppi, calibrating, onCalibrate, onPpi }: { showBody: boolean; onBody: () => void; model: Model; onModel: (id: string) => void; size: Size; onSize: (s: Size) => void; scale: number; actual: number; ppi: number; calibrating: boolean; onCalibrate: () => void; onPpi: (v: number) => void }) {
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
        <span style={{ width: 1, height: 18, background: "var(--rule)", margin: "0 6px" }} />
        <button className="pressable" onClick={onBody} aria-pressed={showBody} style={{ height: 30, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 600, color: showBody ? "var(--ink)" : "var(--ink-3)" }}>
          {showBody ? "Body on" : "Body off"}
        </button>
        <button className="pressable" onClick={onCalibrate} aria-pressed={calibrating} style={{ height: 30, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 600, color: calibrating ? "var(--ink)" : "var(--ink-3)" }}>
          {calibrating ? "Done matching" : "Match my iPad…"}
        </button>
        <span className="num" style={{ marginLeft: 6, fontSize: 12 }}>
          {/* Against actual size — the real iPad is 100%. */}
          {Math.round((scale / actual) * 100)}%
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
  const [view, setView] = useState<View>("perform");
  const [setupTab, setSetupTab] = useState<SetupTab>("guitar");
  const openSetup = (t: SetupTab) => {
    setSetupTab(t);
    setView("setup");
  };
  const [dock, setDock] = useState<Dock>("switches");
  const [macros, setMacros] = useState(true);
  const [sidebar, setSidebar] = useState(true);
  const [browser, setBrowser] = useState(false);
  const [left, setLeft] = useState<"browser" | "sidebar">("browser");
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  return (
    <StageCtx.Provider value={{ el, scale }}>
      <OpenSetup.Provider value={openSetup}>
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
          {/* The left area: the sidebar for the mode — or, in Edit, where
              routing and the FX row hold the main area, the browser (with
              the sidebar a tap away, to pick a section). */}
          {/* Setup takes the whole body: its own list and its page. */}
          {view === "setup" && <SetupView tab={setupTab} onTab={setSetupTab} />}
          {view !== "setup" && sidebar && (
            <aside style={{ width: SIDEBAR, flexShrink: 0, borderRight: "1px solid var(--rule)", minHeight: 0, display: "flex", flexDirection: "column" }}>
              <ComposeCtx.Provider value={{ onPicked: () => view === "perform" && setBrowser(true), pickRows: view === "build" }}>
                {view === "edit" ? (
                  <>
                    <LeftSwitch value={left} onChange={setLeft} />
                    <div style={{ flex: 1, minHeight: 0 }}>{left === "browser" ? <Browser /> : <Sidebar />}</div>
                  </>
                ) : (
                  <Sidebar />
                )}
              </ComposeCtx.Provider>
            </aside>
          )}
          {view !== "setup" && <main style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--desk)" }}>
            {view === "perform" && (
              <>
                {/* The macros along the top: what you turn while playing; their panels drop down. */}
                {macros && (
                  <div style={{ flexShrink: 0, position: "relative", zIndex: 4, borderBottom: "1px solid #000" }}>
                    <MacroBar />
                  </div>
                )}
                {/* The middle: the browser when it's asked for, else nothing. */}
                <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>{browser ? <Browser onClose={() => setBrowser(false)} /> : null}</div>
                {dock && <div style={{ flexShrink: 0 }}>{dock === "switches" && <Switches />}</div>}
              </>
            )}
            {/* Build: the browser takes the main area. */}
            {view === "build" && <Browser />}
            {view === "edit" && <EditView />}
          </main>}
        </div>
        <BottomBar
          view={view}
          onView={setView}
          dock={dock}
          onDock={(d) => setDock(dock === d ? null : d)}
          macros={macros}
          onMacros={() => setMacros(!macros)}
          browser={browser}
          onBrowser={() => setBrowser(!browser)}
        />
      </div>
      </OpenSetup.Provider>
    </StageCtx.Provider>
  );
}

/** Edit's left area: the browser, or the sidebar to pick a section. */
function LeftSwitch({ value, onChange }: { value: "browser" | "sidebar"; onChange: (v: "browser" | "sidebar") => void }) {
  const s = useStore();
  const name = s.performMode === "setlist" ? "Setlist" : s.performMode === "profile" ? "Profile" : "Presets";
  return (
    <div role="tablist" style={{ flexShrink: 0, display: "flex", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
      {(
        [
          ["browser", "Browser"],
          ["sidebar", name],
        ] as const
      ).map(([id, label]) => (
        <button
          key={id}
          role="tab"
          aria-selected={value === id}
          className="pressable"
          onClick={() => onChange(id)}
          style={{ position: "relative", flex: 1, height: 44, fontSize: 14, fontWeight: value === id ? 700 : 560, color: value === id ? "var(--ink)" : "var(--ink-3)" }}
        >
          {label}
          {value === id && <span aria-hidden style={{ position: "absolute", left: 16, right: 16, bottom: 0, height: 2, borderRadius: 1, background: "var(--ink-2)" }} />}
        </button>
      ))}
    </div>
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
      <Meters />
      <Rule />
      <MuteButton />
    </header>
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
    <span title={`DSP load on the rig: ${cpu}%`} style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 1, padding: "0 6px", fontSize: 11, fontWeight: 750, lineHeight: 1, color: tone }}>
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden>
        <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
        <rect x="6" y="6" width="4" height="4" rx="0.5" fill="currentColor" />
        <path d="M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      </svg>
      <span className="num">{cpu}%</span>
    </span>
  );
}

/** IN, OUT and PHONES as three slim upright bars: not a meter to read,
 *  just signal moving and how loud — clipping turns the top red. A muted
 *  output's bar goes red and still; all go flat while Panic resets. Labels
 *  underneath at 11 pt: I(n), O(ut), and the phones as a headphone icon
 *  (the names in full on hover and to assistive tech). Simulated here; the rig streams peaks. */
export function Meters() {
  const s = useStore();
  const sig = useSignal();
  const song = currentSong(s);
  setSignalTempo(song?.bpm || 120);
  const reset = s.panicAt !== null;
  return (
    <div title="IN · OUT · PHONES" style={{ alignSelf: "stretch", flexShrink: 0, display: "flex", gap: 5, padding: "6px 8px 4px" }}>
      <MiniMeter label="In" level={reset ? 0 : sig.input} />
      <MiniMeter label="Out" level={reset ? 0 : sig.output} muted={s.houseMute} />
      <MiniMeter label="Phones" icon={<Headphones />} level={reset ? 0 : sig.phones} muted={s.phonesMute} />
    </div>
  );
}

export function MiniMeter({ label, icon, level, muted }: { label: string; icon?: ReactNode; level: number; muted?: boolean }) {
  return (
    <div title={muted ? `${label} muted` : label} aria-label={muted ? `${label} muted` : `${label} level`} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2, minHeight: 0, minWidth: 10 }}>
      <span style={{ position: "relative", flex: 1, width: 6, minHeight: 0, borderRadius: 3, overflow: "hidden", background: muted ? "color-mix(in oklab, var(--void) 55%, #000)" : "#08080a" }}>
        {!muted && (
          <span
            style={{
              position: "absolute",
              inset: 0,
              background: "linear-gradient(0deg, #15803d 0%, var(--live) 60%, #eab308 82%, var(--void) 100%)",
              clipPath: `inset(${(1 - level) * 100}% 0 0 0)`,
            }}
          />
        )}
      </span>
      <span style={{ height: 12, display: "flex", alignItems: "center", fontSize: 11, fontWeight: 800, lineHeight: 1, color: muted ? "var(--void)" : "var(--ink-3)" }}>{icon ?? label.charAt(0)}</span>
    </div>
  );
}

/** The phones, drawn: a headband and two cups, sized to a meter's label. */
function Headphones() {
  return (
    <svg width="12" height="11" viewBox="0 0 16 14" aria-hidden>
      <path d="M2.5 9V7.5a5.5 5.5 0 0 1 11 0V9" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="1.2" y="8.2" width="3.6" height="5" rx="1.2" fill="currentColor" />
      <rect x="11.2" y="8.2" width="3.6" height="5" rx="1.2" fill="currentColor" />
    </svg>
  );
}

// ── The body ─────────────────────────────────────────────────────────

function Sidebar() {
  return <SidebarContent />;
}

export function Placeholder({ title, note }: { title: string; note: string }) {
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 6 }}>
      <span className="t-label" style={{ color: "var(--ink-3)" }}>
        {title}
      </span>
      <span style={{ fontSize: 13, color: "var(--ink-3)" }}>{note}</span>
    </div>
  );
}

// ── Bottom: what you look at ─────────────────────────────────────────

function BottomBar({ view, onView, dock, onDock, macros, onMacros, browser, onBrowser }: { view: View; onView: (v: View) => void; dock: Dock; onDock: (d: Exclude<Dock, null>) => void; macros: boolean; onMacros: () => void; browser: boolean; onBrowser: () => void }) {
  return (
    <footer style={{ height: FOOT, flexShrink: 0, display: "flex", alignItems: "stretch", borderTop: "1px solid var(--rule)", background: "var(--sheet)", padding: "0 6px" }}>
      {/* The two views. */}
      <FootButton label="Perform" on={view === "perform"} onClick={() => onView("perform")}>
        <path d="M5 3.5v11l9-5.5Z" fill="currentColor" />
      </FootButton>
      <FootButton label="Build" on={view === "build"} onClick={() => onView("build")}>
        <>
          <rect x="2.5" y="2.5" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="10" y="2.5" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <rect x="2.5" y="10" width="5.5" height="5.5" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M12.75 10v5.5M10 12.75h5.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      </FootButton>
      <FootButton label="Edit" on={view === "edit"} onClick={() => onView("edit")}>
        <>
          <path d="M4 2.5v13M9 2.5v13M14 2.5v13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <rect x="2.3" y="10" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="7.3" y="5" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
          <rect x="12.3" y="8" width="3.4" height="2.6" rx="0.8" fill="currentColor" />
        </>
      </FootButton>
      {/* Perform: the browser on call, and the docks — the macros along
          the top, the switches along the foot. (Build and Edit have the
          browser always.) */}
      {view === "perform" && (
        <>
          <Rule />
          <FootButton label="Browser" on={browser} pin onClick={onBrowser}>
            <path d="M3 3.5h3v11H3ZM7.5 3.5h3v11h-3ZM12 4l2.8-.8 2 10.6-2.8.8Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
          </FootButton>
          <Rule />
          <FootButton label="Macros" on={macros} pin onClick={onMacros}>
            <>
              <circle cx="4.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <circle cx="13.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <path d="M4.5 9 6 7.4M13.5 9l1.5-1.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
              <path d="M8 4.5h2M8 13.5h2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </>
          </FootButton>
          <FootButton label="Switches" on={dock === "switches"} pin onClick={() => onDock("switches")}>
            <>
              <rect x="2" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <rect x="7" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <rect x="12" y="5" width="4" height="8" rx="1" fill="none" stroke="currentColor" strokeWidth="1.4" />
            </>
          </FootButton>
        </>
      )}
      <span style={{ flex: 1 }} />
      <FootButton label="Tuner" pin onClick={() => {}}>
        <path d="M3 13a6 6 0 0 1 12 0M9 13l3-5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </FootButton>
      <Rule />
      <FootButton label="Setup" on={view === "setup"} onClick={() => onView("setup")}>
        <GearIcon />
      </FootButton>
    </footer>
  );
}

/** A bar button in the foot: an icon, its word under it (Session's foot). */
/** A foot-bar button. A view (Perform, Build, Edit, Setup) is a tab: the
 *  one open is bright with a bar along its top edge. A dock (`pin`:
 *  Browser, Macros, Switches) is a toggle: on, it sits in a fill. */
export function FootButton({ label, on, pin, off, onClick, children }: { label: string; on?: boolean; pin?: boolean; off?: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      disabled={!!off}
      title={off ?? label}
      aria-pressed={on}
      className={off ? "" : "pressable"}
      style={{
        position: "relative",
        minWidth: 64,
        margin: pin ? "6px 2px" : "0 2px",
        padding: "0 10px",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 4,
        borderRadius: pin ? 8 : 0,
        color: off ? "var(--dim)" : on ? "var(--ink)" : "var(--ink-3)",
        background: pin && on ? "var(--fill-on)" : "transparent",
      }}
    >
      {!pin && on && <span aria-hidden style={{ position: "absolute", top: 0, left: 12, right: 12, height: 3, borderRadius: "0 0 3px 3px", background: "var(--ink)" }} />}
      <svg width="20" height="20" viewBox="0 0 18 18" aria-hidden>
        {children}
      </svg>
      <span style={{ fontSize: 11, fontWeight: on ? 700 : 600, letterSpacing: "0.01em" }}>{label}</span>
    </button>
  );
}

/** A gear: teeth around a hub. */
export function GearIcon() {
  const teeth = 8;
  const pts: string[] = [];
  for (let i = 0; i < teeth * 4; i++) {
    const a = (i / (teeth * 4)) * Math.PI * 2 - Math.PI / 2;
    const r = i % 4 === 1 || i % 4 === 2 ? 7.6 : 5.9;
    pts.push(`${(9 + r * Math.cos(a)).toFixed(2)},${(9 + r * Math.sin(a)).toFixed(2)}`);
  }
  return (
    <>
      <polygon points={pts.join(" ")} fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
      <circle cx="9" cy="9" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </>
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
