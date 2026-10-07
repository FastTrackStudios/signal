// Setup: what the rig plays through, as two things chosen apart —
//
//   Guitars  each a profile of its own: its photo, its pickups, its own
//            input EQ, and how it sits on each rig it has been set up on —
//            the input trim (a level match measures your playing and trims
//            it to the level presets expect) and the five gate presets
//            (Off, Subtle, Default, Tight, Ultra) set above the noise this
//            guitar makes into that interface, plus Noisy input (lifts Off
//            to Subtle and Subtle to Default)
//   Rigs     the audio interface (input, rate, buffer, outputs) and the
//            MIDI controller
//
// Choose a guitar, choose a rig, and it is set. Levels are simulated
// (ui/signal.ts); the rig measures them.

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import {
  chooseGuitar,
  chooseRig,
  currentGuitar,
  currentRig,
  editFit,
  editGuitar,
  editRig,
  gateFor,
  gatesAbove,
  GATE_LEVELS,
  newGuitar,
  newRig,
  removeGuitar,
  removeRig,
  tuneFit,
  tuneGuitar,
  useStore,
  UNFIT,
  type Fit,
  type GateLevel,
  type Guitar,
  type Rig,
} from "../store";
import { useSignal } from "../ui/signal";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import { MACRO_BAR_H } from "../dock/MacroBar";

const GATE_NAME: Record<GateLevel, string> = { off: "Off", subtle: "Subtle", default: "Default", tight: "Tight", ultra: "Ultra" };

/** The guitar's level in dBFS, after the trim (the simulation's 0–1 onto
 *  a −90…0 dB scale: a quiet string's hiss near −86, a strum near −18). */
function levelDb(input: number, trimDb: number) {
  return Math.min(0, -90 + 90 * input + trimDb);
}

type Focus = "guitar" | "rig";

export function SetupView() {
  const ref = useRef<HTMLDivElement>(null);
  const [wide, setWide] = useState(true);
  const [showList, setShowList] = useState(false);
  const [focus, setFocus] = useState<Focus>("guitar");
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWide(el.clientWidth >= 640));
    ro.observe(el);
    setWide(el.clientWidth >= 640);
    return () => ro.disconnect();
  }, []);
  const onList = wide ? undefined : () => setShowList(true);
  return (
    <div ref={ref} style={{ height: "100%", minHeight: 0, display: "flex", background: "var(--desk)" }}>
      {(wide || showList) && (
        <SetupList
          wide={wide}
          focus={focus}
          onPicked={(f) => {
            setFocus(f);
            setShowList(false);
          }}
        />
      )}
      {(wide || !showList) && (focus === "guitar" ? <GuitarDetail onList={onList} onRig={() => setFocus("rig")} /> : <RigDetail onList={onList} onGuitar={() => setFocus("guitar")} />)}
    </div>
  );
}

// ── The photo ──────────────────────────────────────────────────────

/** A guitar's photo, cropped to its body; its finish colour if the photo
 *  is missing (the photos are local, not committed). */
function GuitarPhoto({ g, size, radius = 8, style }: { g: Guitar; size?: number; radius?: number; style?: React.CSSProperties }) {
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [g.image]);
  const box: React.CSSProperties = { width: size, height: size, flexShrink: 0, borderRadius: radius, overflow: "hidden", background: g.colour, ...style };
  if (!g.image || broken)
    return (
      <span aria-hidden style={{ ...box, display: "flex", alignItems: "center", justifyContent: "center", boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.12)" }}>
        <svg width="45%" height="45%" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.55)" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M19 2l3 3-6.5 6.5M15.5 11.5 12.5 8.5" />
          <path d="M12.5 8.5c-2-1.2-4.6-.6-5.4 1.4-.3.8-1 1.3-1.9 1.4-2.3.3-3.6 3.2-1.9 5.3l2.9 2.9c2.1 1.7 5 .4 5.3-1.9.1-.9.6-1.6 1.4-1.9 2-.8 2.6-3.4 1.4-5.4" />
        </svg>
      </span>
    );
  return <img src={g.image} alt="" onError={() => setBroken(true)} draggable={false} style={{ ...box, display: "block", objectFit: "cover", objectPosition: "50% 62%" }} />;
}

// ── The list ───────────────────────────────────────────────────────

function ListHead({ label, onAdd, addLabel }: { label: string; onAdd: (e: React.MouseEvent<HTMLButtonElement>) => void; addLabel: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", minHeight: 44, padding: "8px 4px 0 16px" }}>
      <span className="t-label" style={{ flex: 1, fontSize: 11, letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--ink-3)", fontWeight: 700 }}>
        {label}
      </span>
      <button className="pressable" onClick={onAdd} aria-label={addLabel} style={{ width: 44, height: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)", borderRadius: "var(--r)" }}>
        <svg width="13" height="13" viewBox="0 0 12 12" aria-hidden>
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}

function SetupList({ wide, focus, onPicked }: { wide: boolean; focus: Focus; onPicked: (f: Focus) => void }) {
  const s = useStore();
  const addGuitar = useMenu();
  const addRig = useMenu();
  const rig = currentRig(s);
  return (
    <aside style={{ width: wide ? 300 : "100%", flexShrink: 0, display: "flex", flexDirection: "column", minHeight: 0, borderRight: wide ? "1px solid var(--rule)" : undefined, background: "var(--sheet)" }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "0 16px", borderBottom: "1px solid var(--rule)" }}>
        <span className="t-marker" style={{ fontSize: 22 }}>
          Setup
        </span>
        <span className="t-meta" style={{ fontSize: 13 }}>
          A guitar, into a rig
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <ListHead label="Guitars" addLabel="New guitar" onAdd={addGuitar.fromButton} />
        {s.guitars.map((g, i) => (
          <GuitarRow
            key={g.id}
            g={g}
            index={i}
            inUse={i === s.guitarIndex}
            shown={focus === "guitar" && i === s.guitarIndex}
            fitted={!!g.fits[rig.id]}
            rigName={rig.name}
            onPick={() => {
              if (i !== s.guitarIndex) chooseGuitar(i);
              onPicked("guitar");
            }}
          />
        ))}
        {addGuitar.open && (
          <Menu at={addGuitar.open.at} naming={0} items={[{ kind: "name", id: "add", label: "New guitar…", initial: "New guitar", confirm: "Add", taken: s.guitars.map((x) => x.name) }]} onPick={(p) => newGuitar(p.text)} onClose={addGuitar.close} />
        )}
        <div style={{ height: 12 }} />
        <ListHead label="Rigs · audio and MIDI" addLabel="New rig" onAdd={addRig.fromButton} />
        {s.rigs.map((r, i) => (
          <RigRow
            key={r.id}
            r={r}
            index={i}
            inUse={i === s.rigIndex}
            shown={focus === "rig" && i === s.rigIndex}
            onPick={() => {
              if (i !== s.rigIndex) chooseRig(i);
              onPicked("rig");
            }}
          />
        ))}
        {addRig.open && (
          <Menu at={addRig.open.at} naming={0} items={[{ kind: "name", id: "add", label: "New rig — a copy of this one…", initial: "New rig", confirm: "Add", taken: s.rigs.map((x) => x.name) }]} onPick={(p) => newRig(p.text)} onClose={addRig.close} />
        )}
      </div>
    </aside>
  );
}

/** A row in the list: the one in use marked green; the one open on the
 *  right in a light wash. */
function ListRow({ inUse, shown, onPick, lead, title, sub, menu }: { inUse: boolean; shown: boolean; onPick: () => void; lead: ReactNode; title: string; sub: ReactNode; menu: ReactNode }) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", borderTop: "1px solid var(--rule)", background: shown ? "rgba(255,255,255,0.06)" : undefined }}>
      {inUse && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: "var(--live)" }} />}
      <button className="pressable" onClick={onPick} aria-current={inUse ? "true" : undefined} style={{ flex: 1, minWidth: 0, minHeight: 68, display: "flex", alignItems: "center", gap: 12, padding: "8px 4px 8px 14px", textAlign: "left" }}>
        {lead}
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
          <span style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
            <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: inUse ? 700 : 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</span>
            {inUse && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)", whiteSpace: "nowrap" }}>In use</span>}
          </span>
          <span className="t-meta" style={{ fontSize: 12.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {sub}
          </span>
        </span>
      </button>
      {menu}
    </div>
  );
}

/** Pickups in a line: "Pearly Gates · Lawler Blonde ×2". */
function pickupLine(g: Guitar) {
  const counts = new Map<string, number>();
  for (const p of g.pickups) if (p.model) counts.set(p.model, (counts.get(p.model) ?? 0) + 1);
  return [...counts].map(([m, n]) => (n > 1 ? `${m} ×${n}` : m)).join(" · ") || "No pickups yet";
}

function GuitarRow({ g, index, inUse, shown, fitted, rigName, onPick }: { g: Guitar; index: number; inUse: boolean; shown: boolean; fitted: boolean; rigName: string; onPick: () => void }) {
  const s = useStore();
  const menu = useMenu();
  const items: MenuItem[] = [
    { kind: "head", label: g.name },
    { kind: "name", id: "rename", label: "Rename…", initial: g.name, confirm: "Rename", taken: s.guitars.map((x) => x.name) },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete guitar", disabled: s.guitars.length <= 1 ? "The only guitar" : undefined },
  ];
  const onMenu = (p: Picked) => {
    if (p.id === "rename") {
      chooseGuitar(index);
      editGuitar(`renamed ${p.text}`, (x) => ({ ...x, name: p.text }));
    }
    if (p.id === "delete") removeGuitar(index);
  };
  return (
    <ListRow
      inUse={inUse}
      shown={shown}
      onPick={onPick}
      lead={<GuitarPhoto g={g} size={52} />}
      title={g.name}
      sub={
        fitted ? (
          pickupLine(g)
        ) : (
          <span style={{ color: "#d4a24a" }}>Not set up on {rigName}</span>
        )
      }
      menu={
        <>
          <MoreButton label={`${g.name} actions`} onClick={menu.fromButton} />
          {menu.open && <Menu at={menu.open.at} items={items} onPick={onMenu} onClose={menu.close} />}
        </>
      }
    />
  );
}

function RigGlyph({ on }: { on: boolean }) {
  // An interface, front on: two inputs and a knob.
  return (
    <span aria-hidden style={{ width: 52, height: 52, flexShrink: 0, borderRadius: 8, background: "#17171b", boxShadow: "inset 0 0 0 1px var(--rule-strong)", display: "flex", alignItems: "center", justifyContent: "center", color: on ? "var(--ink-2)" : "var(--ink-3)" }}>
      <svg width="30" height="18" viewBox="0 0 30 18" fill="none" stroke="currentColor" strokeWidth="1.5">
        <rect x="1" y="1" width="28" height="16" rx="3" />
        <circle cx="8" cy="9" r="3" />
        <circle cx="16" cy="9" r="3" />
        <circle cx="24" cy="9" r="2" fill="currentColor" stroke="none" />
      </svg>
    </span>
  );
}

function RigRow({ r, index, inUse, shown, onPick }: { r: Rig; index: number; inUse: boolean; shown: boolean; onPick: () => void }) {
  const s = useStore();
  const menu = useMenu();
  const items: MenuItem[] = [
    { kind: "head", label: r.name },
    { kind: "name", id: "rename", label: "Rename…", initial: r.name, confirm: "Rename", taken: s.rigs.map((x) => x.name) },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete rig", disabled: s.rigs.length <= 1 ? "The only rig" : undefined },
  ];
  const onMenu = (p: Picked) => {
    if (p.id === "rename") {
      chooseRig(index);
      editRig(`renamed ${p.text}`, (x) => ({ ...x, name: p.text }));
    }
    if (p.id === "delete") removeRig(index);
  };
  return (
    <ListRow
      inUse={inUse}
      shown={shown}
      onPick={onPick}
      lead={<RigGlyph on={inUse} />}
      title={r.name}
      sub={`${r.audio.device} · ${r.midi.device}`}
      menu={
        <>
          <MoreButton label={`${r.name} actions`} onClick={menu.fromButton} />
          {menu.open && <Menu at={menu.open.at} items={items} onPick={onMenu} onClose={menu.close} />}
        </>
      }
    />
  );
}

// ── A guitar ───────────────────────────────────────────────────────

function BackButton({ onList }: { onList: () => void }) {
  return (
    <button className="pressable" onClick={onList} aria-label="Guitars and rigs" style={{ width: 44, height: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink)", borderRadius: "var(--r)", background: "rgba(0,0,0,0.45)" }}>
      <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
        <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
}

function GuitarDetail({ onList, onRig }: { onList?: () => void; onRig: () => void }) {
  const s = useStore();
  const g = currentGuitar(s);
  const rig = currentRig(s);
  const fit = g.fits[rig.id];
  const add = useMenu();
  return (
    <div style={{ flex: 1, minWidth: 0, minHeight: 0, overflowY: "auto" }}>
      {/* The guitar, large: its photo across the top, its name over it. */}
      <div style={{ position: "relative", height: 230, overflow: "hidden", background: g.colour }}>
        <GuitarPhoto g={g} radius={0} style={{ width: "100%", height: "100%" }} />
        <span aria-hidden style={{ position: "absolute", inset: 0, background: "linear-gradient(180deg, rgba(0,0,0,0) 35%, rgba(10,10,12,0.92) 100%)" }} />
        {onList && (
          <span style={{ position: "absolute", left: 10, top: 10 }}>
            <BackButton onList={onList} />
          </span>
        )}
        <div style={{ position: "absolute", left: 20, right: 20, bottom: 16, display: "flex", alignItems: "flex-end", gap: 12 }}>
          <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 28, fontWeight: 800, letterSpacing: "-0.02em", color: "#fafafa", textShadow: "0 1px 12px rgba(0,0,0,0.6)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{g.name}</span>
            <span style={{ fontSize: 13.5, color: "rgba(250,250,250,0.78)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{pickupLine(g)}</span>
          </span>
          <span style={{ flexShrink: 0, height: 28, padding: "0 10px", borderRadius: 999, display: "flex", alignItems: "center", fontSize: 12.5, fontWeight: 700, color: "#04210f", background: "var(--live)" }}>In use</span>
        </div>
      </div>

      <Section title="Pickups" note="What this guitar carries">
        {g.pickups.map((p, i) => (
          <Field key={i} label={p.position}>
            <Text value={p.model} placeholder="Pickup model" onCommit={(v) => editGuitar("pickup", (y) => ({ ...y, pickups: y.pickups.map((q, k) => (k === i ? { ...q, model: v } : q)) }))} />
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
            items={[{ kind: "name", id: "add", label: "Pickup position…", initial: "Middle", confirm: "Add" }]}
            onPick={(p) => editGuitar("pickup added", (y) => ({ ...y, pickups: [...y.pickups, { position: p.text, model: "" }] }))}
            onClose={add.close}
          />
        )}
      </Section>

      <Section title="Input EQ" note="This guitar's own — before anything else, on every preset and every rig">
        <GuitarEq g={g} />
      </Section>

      <FitSection g={g} rig={rig} fit={fit} onRig={onRig} />
    </div>
  );
}

/** The guitar on the rig in use: its level and its gates there — and the
 *  other rigs it has been set up on, a tap away. */
function FitSection({ g, rig, fit, onRig }: { g: Guitar; rig: Rig; fit: Fit | undefined; onRig: () => void }) {
  const s = useStore();
  return (
    <section style={{ padding: "18px 20px 22px", borderBottom: "1px solid var(--rule)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 12, marginBottom: 6, flexWrap: "wrap" }}>
        <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>
          On{" "}
          <button className="pressable" onClick={onRig} style={{ font: "inherit", color: "inherit", textDecoration: "underline", textDecorationColor: "var(--rule-strong)", textUnderlineOffset: 4 }}>
            {rig.name}
          </button>
        </h2>
        <span style={{ fontSize: 13, color: "var(--ink-3)" }}>This guitar's level and gates into this interface</span>
      </div>
      {/* Every rig: set up there or not; a tap makes it the rig in use. */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6, margin: "10px 0 14px" }}>
        {s.rigs.map((r, i) => {
          const on = r.id === rig.id;
          const has = !!g.fits[r.id];
          return (
            <button
              key={r.id}
              onClick={() => !on && chooseRig(i)}
              className={on ? "" : "pressable"}
              aria-pressed={on}
              style={{ height: 36, padding: "0 12px", display: "flex", alignItems: "center", gap: 7, borderRadius: "var(--r)", fontSize: 13.5, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-3)", background: on ? "var(--pressed-bg)" : "transparent", boxShadow: on ? "var(--pressed-shadow)" : "inset 0 0 0 1px var(--rule-strong)" }}
            >
              <span aria-hidden style={{ width: 7, height: 7, borderRadius: 999, background: has ? "var(--live)" : "transparent", boxShadow: has ? undefined : "inset 0 0 0 1.5px #d4a24a" }} />
              {r.name}
            </button>
          );
        })}
      </div>
      {fit ? (
        <>
          <LevelMatch fit={fit} rig={rig} />
          <Gates fit={fit} />
        </>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap", padding: 16, borderRadius: "var(--r-md)", background: "rgba(212,162,74,0.08)", boxShadow: "inset 0 0 0 1px rgba(212,162,74,0.35)" }}>
          <span style={{ flex: "1 1 240px", display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 15, fontWeight: 700 }}>
              {g.name} isn't set up on {rig.name} yet
            </span>
            <span style={{ fontSize: 12.5, color: "var(--ink-3)", lineHeight: 1.4 }}>Its level and gates are measured per rig. Until then it plays at no trim with general gates.</span>
          </span>
          <button className="pressable" onClick={() => editFit(`${g.name} on ${rig.name}`, () => UNFIT)} style={{ height: 44, padding: "0 16px", borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, color: "var(--ink)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
            Set up on this rig
          </button>
        </div>
      )}
    </section>
  );
}

// ── A rig ──────────────────────────────────────────────────────────

function RigDetail({ onList, onGuitar }: { onList?: () => void; onGuitar: () => void }) {
  const s = useStore();
  const r = currentRig(s);
  const a = r.audio;
  const set = <K extends keyof Rig["audio"]>(k: K, v: Rig["audio"][K]) => editRig(String(k), (y) => ({ ...y, audio: { ...y.audio, [k]: v } }));
  const here = s.guitars.map((g, i) => ({ g, i })).filter(({ g }) => g.fits[r.id]);
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", alignItems: "center", gap: 12, padding: "0 16px", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
        {onList && <BackButton onList={onList} />}
        <RigGlyph on />
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
          <span className="t-marker" style={{ fontSize: 22, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {r.name}
          </span>
          <span className="t-meta" style={{ fontSize: 13 }}>
            Audio · MIDI
          </span>
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <Section title="Audio" note="The interface the guitar comes in through">
          <Field label="Interface">
            <Choice options={["Arturia MiniFuse 4", "Focusrite Scarlett 2i2", "Built-in"]} value={a.device} onPick={(v) => set("device", v)} />
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
        </Section>
        <Section title="MIDI" note="The controller the switches come from">
          <Field label="Controller">
            <Choice options={["Morningstar MC8", "Morningstar MC6", "None"]} value={r.midi.device} onPick={(v) => editRig("MIDI device", (y) => ({ ...y, midi: { ...y.midi, device: v } }))} />
          </Field>
          <Field label="Listens on">
            <Choice options={["Omni", 1, 2, 3, 4] as (number | "Omni")[]} value={r.midi.channel} onPick={(v) => editRig("MIDI channel", (y) => ({ ...y, midi: { ...y.midi, channel: v } }))} />
          </Field>
        </Section>
        <Section title="Guitars set up here" note="Each carries its own level and gates for this rig">
          {here.length === 0 && <span style={{ fontSize: 13, color: "var(--ink-3)" }}>None yet — open a guitar and set it up on this rig.</span>}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 10 }}>
            {here.map(({ g, i }) => {
              const fit = g.fits[r.id];
              return (
                <button
                  key={g.id}
                  className="pressable"
                  onClick={() => {
                    if (i !== s.guitarIndex) chooseGuitar(i);
                    onGuitar();
                  }}
                  style={{ display: "flex", flexDirection: "column", borderRadius: "var(--r-md)", overflow: "hidden", textAlign: "left", background: "#141418", boxShadow: i === s.guitarIndex ? "inset 0 0 0 2px var(--live)" : "inset 0 0 0 1px var(--rule)" }}
                >
                  <GuitarPhoto g={g} radius={0} style={{ width: "100%", height: 96 }} />
                  <span style={{ padding: "8px 10px 10px", display: "flex", flexDirection: "column", gap: 2 }}>
                    <span style={{ fontSize: 14, fontWeight: 700, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{g.name}</span>
                    <span className="num" style={{ fontSize: 12, color: "var(--ink-3)" }}>
                      Trim {fit.trimDb > 0 ? "+" : ""}
                      {fit.trimDb} dB · Gate {fit.gates.default} dB{fit.noisy ? " · Noisy" : ""}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </Section>
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

// ── Input EQ ───────────────────────────────────────────────────────

function GuitarEq({ g }: { g: Guitar }) {
  const eq = g.eq;
  const set = (k: keyof typeof eq, v: number) => tuneGuitar((y) => ({ ...y, eq: { ...y.eq, [k]: v } }));
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

// ── Level and gates (per guitar, per rig) ─────────────────────────

/** The input trim, and the level match: the guitar's level on a dBFS scale
 *  with the band the presets expect; play as you would and Match trims the
 *  peaks into it. */
function LevelMatch({ fit, rig }: { fit: Fit; rig: Rig }) {
  const sig = useSignal();
  const a = { targetDb: rig.audio.targetDb, trimDb: fit.trimDb };
  const now = levelDb(sig.input, a.trimDb);
  const [peak, setPeak] = useState(-90);
  const [matching, setMatching] = useState<{ until: number; max: number } | null>(null);
  useEffect(() => {
    setPeak((p) => Math.max(now, p - 0.35));
    if (matching) {
      const max = Math.max(matching.max, now - a.trimDb);
      if (performance.now() > matching.until) {
        // Trim the measured peaks onto the target.
        editFit("level matched", (y) => ({ ...y, trimDb: Math.round((a.targetDb - max) * 2) / 2 }));
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
        <input type="range" min={-24} max={24} step={0.5} value={a.trimDb} onChange={(e) => tuneFit((y) => ({ ...y, trimDb: Number(e.target.value) }))} style={{ flex: "1 1 160px", accentColor: "#22C55E" }} aria-label="Input trim" />
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

/** The five gate presets for this rig: a measure of the noise floor sets
 *  them; each can be fine-tuned against the live level; Noisy input lifts
 *  the light end (Off → Subtle, Subtle → Default). */
function Gates({ fit }: { fit: Fit }) {
  const sig = useSignal();
  const now = levelDb(sig.input, fit.trimDb);
  const [measuring, setMeasuring] = useState<{ until: number; floor: number } | null>(null);
  useEffect(() => {
    if (!measuring) return;
    // The floor: the quietest the input gets while nothing is played.
    const floor = Math.min(measuring.floor, now);
    if (performance.now() > measuring.until) {
      const f = Math.round(floor);
      editFit("gates measured", (y) => ({ ...y, gates: gatesAbove(f) }));
      setMeasuring(null);
    } else if (floor !== measuring.floor) setMeasuring({ ...measuring, floor });
  });
  const pos = (db: number) => `${((db + 96) / 96) * 100}%`;
  return (
    <div style={{ marginTop: 22 }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, marginBottom: 12 }}>
        <span style={{ fontSize: 15, fontWeight: 700 }}>Gates</span>
        <span style={{ fontSize: 12.5, color: "var(--ink-3)" }}>Every preset's gate means the same with this guitar here</span>
      </div>
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
        const db = level === "off" ? null : fit.gates[level];
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
                  onChange={(e) => tuneFit((y) => ({ ...y, gates: { ...y.gates, [level]: Number(e.target.value) } }))}
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
        aria-checked={fit.noisy}
        className="pressable"
        onClick={() => editFit(fit.noisy ? "noisy input off" : "noisy input on", (y) => ({ ...y, noisy: !y.noisy }))}
        style={{ display: "flex", alignItems: "center", gap: 14, width: "100%", minHeight: 64, marginTop: 6, padding: "8px 0", borderTop: "1px solid var(--rule)", textAlign: "left" }}
      >
        <span style={{ flex: 1, display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={{ fontSize: 15, fontWeight: 700 }}>Noisy input</span>
          <span style={{ fontSize: 12.5, color: "var(--ink-3)", lineHeight: 1.4 }}>
            {fit.noisy
              ? (["off", "subtle"] as GateLevel[]).map((l) => `${GATE_NAME[l]} → ${GATE_NAME[gateFor(fit, l)]}`).join(" · ") + " · the rest as they are"
              : "For a noisy room or pickups: presets with no gate play Subtle, and Subtle plays Default."}
          </span>
        </span>
        <span aria-hidden style={{ width: 46, height: 28, borderRadius: 999, padding: 3, flexShrink: 0, background: fit.noisy ? "var(--live)" : "#2b2b31", display: "flex", justifyContent: fit.noisy ? "flex-end" : "flex-start" }}>
          <span style={{ width: 22, height: 22, borderRadius: 999, background: "#f4f4f5" }} />
        </span>
      </button>
    </div>
  );
}
