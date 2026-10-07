// Setup: the rig setups — the hardware and the hands the rig plays through,
// each saved as one thing, chosen down the left, set up on the right:
//
//   Guitar   which guitar, its pickups, and its own input EQ
//   Audio    the interface (its input, rate, buffer, outputs) and the input
//            trim — with a level match that measures your playing and
//            trims it to the level the presets expect
//   MIDI     the controller
//   Gates    five gate presets — Off, Subtle, Default, Tight, Ultra — whose
//            thresholds are set here, for this guitar into this interface,
//            so a preset's gate means the same on any rig; a noise-floor
//            measure sets them, and Noisy input lifts Off to Subtle and
//            Subtle to Default
//
// Levels are simulated (ui/signal.ts); the rig measures them.

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { chooseSetup, currentSetup, editSetup, gateFor, GATE_LEVELS, newSetup, removeSetup, tuneSetup, useStore, type GateLevel, type RigSetup } from "../store";
import { useSignal } from "../ui/signal";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import { MACRO_BAR_H } from "../dock/MacroBar";

const GATE_NAME: Record<GateLevel, string> = { off: "Off", subtle: "Subtle", default: "Default", tight: "Tight", ultra: "Ultra" };

/** The guitar's level in dBFS, after the trim (the simulation's 0–1 onto
 *  a −90…0 dB scale: a quiet string's hiss near −86, a strum near −18). */
function levelDb(input: number, trimDb: number) {
  return Math.min(0, -90 + 90 * input + trimDb);
}

export function SetupView() {
  const ref = useRef<HTMLDivElement>(null);
  const [wide, setWide] = useState(true);
  const [showList, setShowList] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWide(el.clientWidth >= 640));
    ro.observe(el);
    setWide(el.clientWidth >= 640);
    return () => ro.disconnect();
  }, []);
  return (
    <div ref={ref} style={{ height: "100%", minHeight: 0, display: "flex", background: "var(--desk)" }}>
      {(wide || showList) && <SetupList wide={wide} onPicked={() => setShowList(false)} />}
      {(wide || !showList) && <SetupDetail onList={wide ? undefined : () => setShowList(true)} />}
    </div>
  );
}

// ── The setups ─────────────────────────────────────────────────────

function SetupList({ wide, onPicked }: { wide: boolean; onPicked: () => void }) {
  const s = useStore();
  const add = useMenu();
  return (
    <aside style={{ width: wide ? 280 : "100%", flexShrink: 0, display: "flex", flexDirection: "column", minHeight: 0, borderRight: wide ? "1px solid var(--rule)" : undefined, background: "var(--sheet)" }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "0 16px", borderBottom: "1px solid var(--rule)" }}>
        <span className="t-marker" style={{ fontSize: 22 }}>
          Rig setups
        </span>
        <span className="t-meta" style={{ fontSize: 13 }}>
          What the rig plays through
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {s.setups.map((x, i) => (
          <SetupRow key={`${i}-${x.name}`} setup={x} index={i} on={i === s.setupIndex} onPick={() => {
            chooseSetup(i);
            onPicked();
          }} />
        ))}
        <button className="pressable" onClick={add.fromButton} style={{ display: "flex", alignItems: "center", gap: 10, width: "100%", minHeight: 48, padding: "0 16px", color: "var(--ink-3)", fontSize: 14, fontWeight: 600, textAlign: "left" }}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
            <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
          New setup
        </button>
        {add.open && (
          <Menu
            at={add.open.at}
            naming={0}
            items={[{ kind: "name", id: "add", label: "New setup — a copy of this one…", initial: "New rig", confirm: "Add", taken: s.setups.map((x) => x.name) }]}
            onPick={(p) => newSetup(p.text)}
            onClose={add.close}
          />
        )}
      </div>
    </aside>
  );
}

function SetupRow({ setup, index, on, onPick }: { setup: RigSetup; index: number; on: boolean; onPick: () => void }) {
  const s = useStore();
  const menu = useMenu();
  const items: MenuItem[] = [
    { kind: "head", label: setup.name },
    { kind: "name", id: "rename", label: "Rename…", initial: setup.name, confirm: "Rename", taken: s.setups.map((x) => x.name) },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete setup", disabled: s.setups.length <= 1 ? "The only setup" : undefined },
  ];
  const onMenu = (p: Picked) => {
    if (p.id === "rename") {
      chooseSetup(index);
      editSetup(`renamed ${p.text}`, (x) => ({ ...x, name: p.text }));
    }
    if (p.id === "delete") removeSetup(index);
  };
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", borderBottom: "1px solid var(--rule)", background: on ? "rgba(255,255,255,0.06)" : undefined }}>
      {on && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: "var(--live)" }} />}
      <button className="pressable" onClick={onPick} style={{ flex: 1, minWidth: 0, minHeight: 60, display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "6px 4px 6px 16px", textAlign: "left" }}>
        <span style={{ fontSize: 15, fontWeight: on ? 700 : 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{setup.name}</span>
        <span className="t-meta" style={{ fontSize: 12.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {setup.guitar.name} · {setup.audio.device}
        </span>
      </button>
      {on && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)", whiteSpace: "nowrap" }}>In use</span>}
      <MoreButton label={`${setup.name} actions`} onClick={menu.fromButton} />
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onMenu} onClose={menu.close} />}
    </div>
  );
}

// ── The setup in use ────────────────────────────────────────────────

function SetupDetail({ onList }: { onList?: () => void }) {
  const s = useStore();
  const x = currentSetup(s);
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", alignItems: "center", gap: 10, padding: "0 16px", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
        {onList && (
          <button className="pressable" onClick={onList} aria-label="All setups" style={{ width: 36, height: 44, marginLeft: -8, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", borderRadius: "var(--r)" }}>
            <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
              <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        )}
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
          <span className="t-marker" style={{ fontSize: 22, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {x.name}
          </span>
          <span className="t-meta" style={{ fontSize: 13 }}>
            Guitar · Audio · MIDI · Gates
          </span>
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <GuitarSection x={x} />
        <AudioSection x={x} />
        <MidiSection x={x} />
        <GatesSection x={x} />
      </div>
    </div>
  );
}

function Section({ title, note, children }: { title: string; note: string; children: ReactNode }) {
  return (
    <section style={{ padding: "18px 20px 22px", borderBottom: "1px solid var(--rule)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 12, marginBottom: 14 }}>
        <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>{title}</h2>
        <span style={{ fontSize: 13, color: "var(--ink-3)" }}>{note}</span>
      </div>
      {children}
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label style={{ display: "flex", alignItems: "center", gap: 14, minHeight: 48, borderTop: "1px solid var(--rule)" }}>
      <span style={{ width: 120, flexShrink: 0, fontSize: 14, color: "var(--ink-2)", fontWeight: 600 }}>{label}</span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 8 }}>{children}</span>
    </label>
  );
}

function Text({ value, onCommit, placeholder }: { value: string; onCommit: (v: string) => void; placeholder?: string }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  return (
    <input
      value={v}
      placeholder={placeholder}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => v.trim() && v !== value && onCommit(v.trim())}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      style={{ flex: 1, minWidth: 0, height: 40, padding: "0 12px", borderRadius: "var(--r)", border: "1px solid var(--rule-strong)", fontSize: 15 }}
    />
  );
}

function Choice<T extends string | number>({ options, value, onPick }: { options: T[]; value: T; onPick: (v: T) => void }) {
  return (
    <span style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
      {options.map((o) => (
        <button
          key={String(o)}
          onClick={() => onPick(o)}
          className={o === value ? "" : "pressable"}
          style={{ height: 36, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13.5, fontWeight: o === value ? 700 : 560, color: o === value ? "var(--ink)" : "var(--ink-3)", background: o === value ? "var(--pressed-bg)" : "transparent", boxShadow: o === value ? "var(--pressed-shadow)" : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {String(o)}
        </button>
      ))}
    </span>
  );
}

// ── Guitar ─────────────────────────────────────────────────────────

function GuitarSection({ x }: { x: RigSetup }) {
  const g = x.guitar;
  const add = useMenu();
  return (
    <Section title="Guitar" note="Which guitar, and how it sounds coming in">
      <Field label="Guitar">
        <Text value={g.name} onCommit={(v) => editSetup("guitar", (y) => ({ ...y, guitar: { ...y.guitar, name: v } }))} />
      </Field>
      {g.pickups.map((p, i) => (
        <Field key={i} label={`${p.position} pickup`}>
          <Text value={p.model} onCommit={(v) => editSetup("pickup", (y) => ({ ...y, guitar: { ...y.guitar, pickups: y.guitar.pickups.map((q, k) => (k === i ? { ...q, model: v } : q)) } }))} />
        </Field>
      ))}
      <button className="pressable" onClick={add.fromButton} style={{ display: "flex", alignItems: "center", gap: 8, minHeight: 44, color: "var(--ink-3)", fontSize: 14, fontWeight: 600, borderTop: "1px solid var(--rule)", width: "100%" }}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
        Add a pickup
      </button>
      {add.open && (
        <Menu
          at={add.open.at}
          naming={0}
          items={[{ kind: "name", id: "add", label: "Pickup position…", initial: "Neck", confirm: "Add" }]}
          onPick={(p) => editSetup("pickup added", (y) => ({ ...y, guitar: { ...y.guitar, pickups: [...y.guitar.pickups, { position: p.text, model: "" }] } }))}
          onClose={add.close}
        />
      )}
      <div style={{ marginTop: 16 }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: 10, marginBottom: 10 }}>
          <span style={{ fontSize: 15, fontWeight: 700 }}>Input EQ</span>
          <span style={{ fontSize: 12.5, color: "var(--ink-3)" }}>This guitar's own — before anything else, on every preset</span>
        </div>
        <GuitarEq x={x} />
      </div>
    </Section>
  );
}

function GuitarEq({ x }: { x: RigSetup }) {
  const eq = x.guitar.eq;
  const set = (k: keyof typeof eq, v: number) => tuneSetup((y) => ({ ...y, guitar: { ...y.guitar, eq: { ...y.guitar.eq, [k]: v } } }));
  const bands: { k: keyof typeof eq; label: string; min: number; max: number; unit: string }[] = [
    { k: "lowCut", label: "Low cut", min: 20, max: 200, unit: "Hz" },
    { k: "bass", label: "Bass", min: -12, max: 12, unit: "dB" },
    { k: "mid", label: "Mid", min: -12, max: 12, unit: "dB" },
    { k: "treble", label: "Treble", min: -12, max: 12, unit: "dB" },
  ];
  // The curve: a low cut rising to flat, then the three bands' tilt.
  const W = 400;
  const H = 90;
  const y = (db: number) => H / 2 - (db / 12) * (H / 2 - 8);
  const cut = ((eq.lowCut - 20) / 180) * 70;
  const path = `M0 ${H - 4} C ${cut * 0.6} ${H - 4}, ${cut} ${y(eq.bass)}, ${cut + 30} ${y(eq.bass)} S ${W * 0.45} ${y(eq.mid)}, ${W * 0.55} ${y(eq.mid)} S ${W * 0.85} ${y(eq.treble)}, ${W} ${y(eq.treble)}`;
  return (
    <div style={{ display: "flex", gap: 16, alignItems: "stretch", flexWrap: "wrap" }}>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{ flex: "1 1 260px", height: 110, borderRadius: "var(--r-md)", background: "#0b0b0e", boxShadow: "inset 0 0 0 1px var(--rule)" }} aria-label="The input EQ's curve">
        <line x1="0" x2={W} y1={H / 2} y2={H / 2} stroke="#2b2b31" strokeDasharray="3 4" />
        <path d={path} fill="none" stroke="#22C55E" strokeWidth="2.2" vectorEffect="non-scaling-stroke" />
      </svg>
      <div style={{ flex: "1 1 260px", display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 8 }}>
        {bands.map((b) => (
          <label key={b.k} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, padding: "8px 0", borderRadius: "var(--r)", background: "#141418" }}>
            <span style={{ fontSize: 12, fontWeight: 700, color: "var(--ink-3)" }}>{b.label}</span>
            <input type="range" min={b.min} max={b.max} step={b.k === "lowCut" ? 5 : 0.5} value={eq[b.k]} onChange={(e) => set(b.k, Number(e.target.value))} style={{ writingMode: "vertical-lr", direction: "rtl", height: 70, accentColor: "#22C55E" }} />
            <span className="num" style={{ fontSize: 13, fontWeight: 700 }}>
              {b.k !== "lowCut" && eq[b.k] > 0 ? "+" : ""}
              {eq[b.k]} {b.unit}
            </span>
          </label>
        ))}
      </div>
    </div>
  );
}

// ── Audio ──────────────────────────────────────────────────────────

function AudioSection({ x }: { x: RigSetup }) {
  const a = x.audio;
  const set = <K extends keyof RigSetup["audio"]>(k: K, v: RigSetup["audio"][K]) => editSetup(String(k), (y) => ({ ...y, audio: { ...y.audio, [k]: v } }));
  return (
    <Section title="Audio" note="The interface, and the level the guitar comes in at">
      <Field label="Interface">
        <Choice options={["Arturia MiniFuse 4", "Built-in", "Focusrite Scarlett 2i2"]} value={a.device} onPick={(v) => set("device", v)} />
      </Field>
      <Field label="Guitar input">
        <Choice options={["Input 1 · Inst (Hi-Z)", "Input 2 · Inst (Hi-Z)", "Input 3 · Line"]} value={a.input} onPick={(v) => set("input", v)} />
      </Field>
      <Field label="Sample rate">
        <Choice options={["44.1 kHz", "48 kHz", "96 kHz"]} value={a.rate} onPick={(v) => set("rate", v)} />
      </Field>
      <Field label="Buffer">
        <Choice options={[64, 128, 256, 512]} value={a.buffer} onPick={(v) => set("buffer", v)} />
      </Field>
      <Field label="House">
        <Choice options={["Outputs 1–2", "Outputs 3–4"]} value={a.house} onPick={(v) => set("house", v)} />
      </Field>
      <Field label="Phones">
        <Choice options={["Phones 1", "Phones 2", "Outputs 3–4"]} value={a.phones} onPick={(v) => set("phones", v)} />
      </Field>
      <LevelMatch x={x} />
    </Section>
  );
}

/** The input trim, and the level match: the guitar's level on a dBFS scale
 *  with the band the presets expect; play as you would and Match trims the
 *  peaks into it. */
function LevelMatch({ x }: { x: RigSetup }) {
  const sig = useSignal();
  const a = x.audio;
  const now = levelDb(sig.input, a.trimDb);
  const [peak, setPeak] = useState(-90);
  const [matching, setMatching] = useState<{ until: number; max: number } | null>(null);
  useEffect(() => {
    setPeak((p) => Math.max(now, p - 0.35));
    if (matching) {
      const max = Math.max(matching.max, now - a.trimDb);
      if (performance.now() > matching.until) {
        // Trim the measured peaks onto the target.
        tuneSetup((y) => ({ ...y, audio: { ...y.audio, trimDb: Math.round((a.targetDb - max) * 2) / 2 } }));
        setMatching(null);
      } else if (max !== matching.max) setMatching({ ...matching, max });
    }
  });
  const pos = (db: number) => `${((db + 60) / 60) * 100}%`;
  const inBand = Math.abs(peak - a.targetDb) <= 3;
  return (
    <div style={{ marginTop: 16, padding: 14, borderRadius: "var(--r-md)", background: "#111114", boxShadow: "inset 0 0 0 1px var(--rule)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, marginBottom: 10 }}>
        <span style={{ fontSize: 15, fontWeight: 700 }}>Input level</span>
        <span style={{ fontSize: 12.5, color: "var(--ink-3)" }}>Presets expect the guitar's peaks around {a.targetDb} dBFS</span>
      </div>
      {/* The meter: −60…0 dBFS, the target band, the level and its peak. */}
      <div style={{ position: "relative", height: 26, borderRadius: 6, background: "#08080a", overflow: "hidden" }}>
        <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: pos(a.targetDb - 3), width: `${(6 / 60) * 100}%`, background: "rgba(34,197,94,0.16)", boxShadow: "inset 0 0 0 1px rgba(34,197,94,0.5)" }} />
        <span aria-hidden style={{ position: "absolute", top: 6, bottom: 6, left: 0, width: pos(now), borderRadius: 3, background: "linear-gradient(90deg, #15803d, #22c55e 70%, #eab308 90%, #f87171)" }} />
        <span aria-hidden style={{ position: "absolute", top: 3, bottom: 3, left: `calc(${pos(peak)} - 1px)`, width: 2, background: inBand ? "var(--live)" : "#f4f4f5" }} />
      </div>
      <div className="num" style={{ display: "flex", justifyContent: "space-between", marginTop: 4, fontSize: 11, color: "var(--ink-3)" }}>
        <span>−60</span>
        <span>−30</span>
        <span>−15</span>
        <span>0 dBFS</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 12, flexWrap: "wrap" }}>
        <span style={{ fontSize: 14, fontWeight: 600, color: "var(--ink-2)" }}>Trim</span>
        <input type="range" min={-24} max={24} step={0.5} value={a.trimDb} onChange={(e) => tuneSetup((y) => ({ ...y, audio: { ...y.audio, trimDb: Number(e.target.value) } }))} style={{ flex: "1 1 160px", accentColor: "#22C55E" }} aria-label="Input trim" />
        <span className="num" style={{ width: 64, fontSize: 15, fontWeight: 700 }}>
          {a.trimDb > 0 ? "+" : ""}
          {a.trimDb} dB
        </span>
        <button
          className={matching ? "" : "pressable"}
          disabled={!!matching}
          onClick={() => setMatching({ until: performance.now() + 4000, max: -90 })}
          style={{ height: 40, padding: "0 16px", borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, color: matching ? "#04210f" : "var(--ink)", background: matching ? "var(--live)" : "transparent", boxShadow: matching ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {matching ? "Play as you would…" : "Match level"}
        </button>
      </div>
      <div style={{ marginTop: 8, fontSize: 12.5, color: inBand ? "var(--live)" : "var(--ink-3)" }}>
        {matching ? "Listening for 4 seconds — strum as you would in the set." : inBand ? "In the band — presets will sound as they were dialled." : `Peaks at ${Math.round(peak)} dBFS — Match level trims them to ${a.targetDb}.`}
      </div>
    </div>
  );
}

// ── MIDI ───────────────────────────────────────────────────────────

function MidiSection({ x }: { x: RigSetup }) {
  return (
    <Section title="MIDI" note="The controller the switches come from">
      <Field label="Controller">
        <Choice options={["Morningstar MC8", "Morningstar MC6", "None"]} value={x.midi.device} onPick={(v) => editSetup("MIDI device", (y) => ({ ...y, midi: { ...y.midi, device: v } }))} />
      </Field>
      <Field label="Listens on">
        <Choice options={["Omni", 1, 2, 3, 4] as (number | "Omni")[]} value={x.midi.channel} onPick={(v) => editSetup("MIDI channel", (y) => ({ ...y, midi: { ...y.midi, channel: v } }))} />
      </Field>
    </Section>
  );
}

// ── Gates ──────────────────────────────────────────────────────────

/** The five gate presets for this rig: a measure of the noise floor sets
 *  them; each can be fine-tuned against the live level; Noisy input lifts
 *  the light end (Off → Subtle, Subtle → Default). */
function GatesSection({ x }: { x: RigSetup }) {
  const sig = useSignal();
  const now = levelDb(sig.input, x.audio.trimDb);
  const [measuring, setMeasuring] = useState<{ until: number; floor: number } | null>(null);
  useEffect(() => {
    if (!measuring) return;
    // The floor: the quietest the input gets while nothing is played.
    const floor = Math.min(measuring.floor, now);
    if (performance.now() > measuring.until) {
      const f = Math.round(floor);
      editSetup("gates measured", (y) => ({ ...y, gates: { subtle: f + 5, default: f + 10, tight: f + 16, ultra: f + 24 } }));
      setMeasuring(null);
    } else if (floor !== measuring.floor) setMeasuring({ ...measuring, floor });
  });
  const pos = (db: number) => `${((db + 96) / 96) * 100}%`;
  return (
    <Section title="Gates" note="Set once for this guitar into this interface — every preset's gate then means the same">
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 14, flexWrap: "wrap" }}>
        <button
          className={measuring ? "" : "pressable"}
          disabled={!!measuring}
          onClick={() => setMeasuring({ until: performance.now() + 3000, floor: 0 })}
          style={{ height: 44, padding: "0 16px", borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, color: measuring ? "#04210f" : "var(--ink)", background: measuring ? "var(--live)" : "transparent", boxShadow: measuring ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {measuring ? "Hands off the strings…" : "Measure the noise floor"}
        </button>
        <span style={{ flex: "1 1 200px", fontSize: 12.5, color: "var(--ink-3)", lineHeight: 1.4 }}>
          {measuring ? "Listening for 3 seconds to what comes in with nothing played." : "Turn up as you play, mute the strings, and measure: the four gates are set above the noise this rig makes."}
        </span>
      </div>
      {GATE_LEVELS.map((level) => {
        const db = level === "off" ? null : x.gates[level];
        const open = db === null || now >= db;
        return (
          <div key={level} style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 56, borderTop: "1px solid var(--rule)" }}>
            <span style={{ width: 84, flexShrink: 0, fontSize: 15, fontWeight: 700 }}>{GATE_NAME[level]}</span>
            {db === null ? (
              <span style={{ flex: 1, fontSize: 13, color: "var(--ink-3)" }}>No gate</span>
            ) : (
              <>
                {/* The live level against this threshold: green while open. */}
                <span style={{ position: "relative", flex: 1, minWidth: 120, height: 22, borderRadius: 5, background: "#08080a", overflow: "hidden" }}>
                  <span aria-hidden style={{ position: "absolute", top: 5, bottom: 5, left: 0, width: pos(now), borderRadius: 3, background: open ? "var(--live)" : "#52525b" }} />
                  <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${pos(db)} - 1px)`, width: 2, background: "#f4f4f5" }} />
                </span>
                <input
                  type="range"
                  min={-96}
                  max={-24}
                  step={1}
                  value={db}
                  aria-label={`${GATE_NAME[level]} threshold`}
                  onChange={(e) => tuneSetup((y) => ({ ...y, gates: { ...y.gates, [level]: Number(e.target.value) } }))}
                  style={{ width: 140, accentColor: "#94A3B8" }}
                />
                <span className="num" style={{ width: 66, fontSize: 14, fontWeight: 700 }}>
                  {db} dB
                </span>
              </>
            )}
            <span style={{ width: 52, textAlign: "right", fontSize: 12, fontWeight: 700, color: open ? "var(--live)" : "var(--ink-3)" }}>{open ? "Open" : "Closed"}</span>
          </div>
        );
      })}
      {/* Noisy input: every preset one step tighter. */}
      <button
        role="switch"
        aria-checked={x.noisy}
        className="pressable"
        onClick={() => editSetup(x.noisy ? "noisy input off" : "noisy input on", (y) => ({ ...y, noisy: !y.noisy }))}
        style={{ display: "flex", alignItems: "center", gap: 14, width: "100%", minHeight: 64, marginTop: 6, padding: "8px 0", borderTop: "1px solid var(--rule)", textAlign: "left" }}
      >
        <span style={{ flex: 1, display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={{ fontSize: 15, fontWeight: 700 }}>Noisy input</span>
          <span style={{ fontSize: 12.5, color: "var(--ink-3)", lineHeight: 1.4 }}>
            {x.noisy
              ? (["off", "subtle"] as GateLevel[]).map((l) => `${GATE_NAME[l]} → ${GATE_NAME[gateFor(x, l)]}`).join(" · ") + " · the rest as they are"
              : "For a noisy room or pickups: presets with no gate play Subtle, and Subtle plays Default."}
          </span>
        </span>
        <span aria-hidden style={{ width: 46, height: 28, borderRadius: 999, padding: 3, flexShrink: 0, background: x.noisy ? "var(--live)" : "#2b2b31", display: "flex", justifyContent: x.noisy ? "flex-end" : "flex-start" }}>
          <span style={{ width: 22, height: 22, borderRadius: 999, background: "#f4f4f5" }} />
        </span>
      </button>
    </Section>
  );
}
