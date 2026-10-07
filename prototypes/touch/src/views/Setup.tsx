// Setup: choose a guitar, choose a rig — and it is set.
//
//   Guitar  a profile of its own: its photo, its pickups, and its tone — the
//           input trim (a level match measures your playing and trims it to
//           the level presets expect), the five gate presets (Off, Subtle,
//           Default, Tight, Ultra; Noisy input lifts Off to Subtle and Subtle
//           to Default) and its input EQ. The tone is the guitar's default on
//           every rig; any rig can override any part of it, marked with the
//           override icon and saved back to the guitar or discarded — the
//           same model as a section over its preset.
//   Audio   the rig's interface: device, sample rate and buffer (with the
//           round trip they make), the guitar's input, the house and phones
//           outputs with their levels and a left/right check.
//   MIDI    the rig's controller: its switches (press one to see what it
//           sends), the channel, program changes, clock.
//
// Levels and MIDI are simulated (ui/signal.ts); the rig measures them.

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import {
  chooseGuitar,
  chooseRig,
  currentGuitar,
  currentRig,
  discardToneOverrides,
  editGuitar,
  editRig,
  editTone,
  gateFor,
  gatesAbove,
  interfaceOf,
  INTERFACES,
  latencyMs,
  MIDI_DEVICES,
  newGuitar,
  newRig,
  overriddenOn,
  removeGuitar,
  removeRig,
  saveToneOverrides,
  toneOn,
  tuneRig,
  tuneTone,
  useStore,
  type GateLevel,
  type Guitar,
  type Rig,
  type Tone,
  type ToneKey,
  type ToneScope,
} from "../store";
import { useSignal } from "../ui/signal";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import { OverrideIcon } from "../ui/OverrideIcon";
import { MACRO_BAR_H } from "../dock/MacroBar";

const GATE_NAME: Record<GateLevel, string> = { off: "Off", subtle: "Subtle", default: "Default", tight: "Tight", ultra: "Ultra" };
/** A rig's override of the guitar, in the rig's colour. */
const RIG_COLOUR = "#38BDF8";
const TONE_NAME: Record<ToneKey, string> = { trimDb: "Trim", gates: "Gate", noisy: "Noisy input", eq: "EQ" };

/** The guitar's level in dBFS, after the trim (the simulation's 0–1 onto
 *  a −90…0 dB scale: a quiet string's hiss near −86, a strum near −18). */
function levelDb(input: number, trimDb: number) {
  return Math.min(0, -90 + 90 * input + trimDb);
}

type Tab = "guitar" | "audio" | "midi";

export function SetupView() {
  const ref = useRef<HTMLDivElement>(null);
  const [wide, setWide] = useState(true);
  const [showList, setShowList] = useState(false);
  const [tab, setTab] = useState<Tab>("guitar");
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWide(el.clientWidth >= 640));
    ro.observe(el);
    setWide(el.clientWidth >= 640);
    return () => ro.disconnect();
  }, []);
  return (
    <div ref={ref} style={{ flex: 1, minWidth: 0, height: "100%", minHeight: 0, display: "flex", background: "var(--desk)" }}>
      {(wide || showList) && (
        <SetupList
          wide={wide}
          tab={tab}
          onPicked={(t) => {
            setTab(t);
            setShowList(false);
          }}
        />
      )}
      {(wide || !showList) && <SetupDetail tab={tab} onTab={setTab} onList={wide ? undefined : () => setShowList(true)} />}
    </div>
  );
}

// ── Choosing: a guitar, a rig ──────────────────────────────────────

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

function SetupList({ wide, tab, onPicked }: { wide: boolean; tab: Tab; onPicked: (t: Tab) => void }) {
  const s = useStore();
  const addGuitar = useMenu();
  const addRig = useMenu();
  const rig = currentRig(s);
  return (
    <aside style={{ width: wide ? 290 : "100%", flexShrink: 0, display: "flex", flexDirection: "column", minHeight: 0, borderRight: wide ? "1px solid var(--rule)" : undefined, background: "var(--sheet)" }}>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <ListHead label="Guitar" addLabel="New guitar" onAdd={addGuitar.fromButton} />
        {s.guitars.map((g, i) => (
          <GuitarRow
            key={g.id}
            g={g}
            index={i}
            inUse={i === s.guitarIndex}
            shown={tab === "guitar" && i === s.guitarIndex}
            rig={rig}
            onPick={() => {
              if (i !== s.guitarIndex) chooseGuitar(i);
              onPicked("guitar");
            }}
          />
        ))}
        {addGuitar.open && (
          <Menu at={addGuitar.open.at} naming={0} items={[{ kind: "name", id: "add", label: "New guitar…", initial: "New guitar", confirm: "Add", taken: s.guitars.map((x) => x.name) }]} onPick={(p) => newGuitar(p.text)} onClose={addGuitar.close} />
        )}
        <div style={{ height: 14 }} />
        <ListHead label="Rig" addLabel="New rig" onAdd={addRig.fromButton} />
        {s.rigs.map((r, i) => (
          <RigRow
            key={r.id}
            r={r}
            index={i}
            inUse={i === s.rigIndex}
            shown={tab !== "guitar" && i === s.rigIndex}
            onPick={() => {
              if (i !== s.rigIndex) chooseRig(i);
              onPicked(tab === "guitar" ? "audio" : tab);
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

function GuitarRow({ g, index, inUse, shown, rig, onPick }: { g: Guitar; index: number; inUse: boolean; shown: boolean; rig: Rig; onPick: () => void }) {
  const s = useStore();
  const menu = useMenu();
  const over = overriddenOn(g, rig.id);
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
        over.length ? (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <OverrideIcon colour={RIG_COLOUR} size={11} />
            {over.map((k) => TONE_NAME[k]).join(", ")} on {rig.name}
          </span>
        ) : undefined
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
      sub={`${khz(r.audio.rate)} · ${r.audio.buffer} · ${latencyMs(r).toFixed(1)} ms`}
      menu={
        <>
          <MoreButton label={`${r.name} actions`} onClick={menu.fromButton} />
          {menu.open && <Menu at={menu.open.at} items={items} onPick={onMenu} onClose={menu.close} />}
        </>
      }
    />
  );
}

const khz = (rate: number) => `${(rate / 1000).toLocaleString("en", { maximumFractionDigits: 1 })} kHz`;

// ── The detail: guitar into rig, three tabs ────────────────────────

function SetupDetail({ tab, onTab, onList }: { tab: Tab; onTab: (t: Tab) => void; onList?: () => void }) {
  const s = useStore();
  const g = currentGuitar(s);
  const r = currentRig(s);
  const tabs: { id: Tab; label: string }[] = [
    { id: "guitar", label: "Guitar" },
    { id: "audio", label: "Audio" },
    { id: "midi", label: "MIDI" },
  ];
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <header style={{ flexShrink: 0, minHeight: MACRO_BAR_H, display: "flex", alignItems: "center", gap: 12, padding: "8px 16px", flexWrap: "wrap", borderBottom: "1px solid var(--rule)", background: "var(--sheet)" }}>
        {onList && <BackButton onList={onList} />}
        {/* What plays: this guitar, into this rig. */}
        <span style={{ flex: "1 1 260px", minWidth: 0, display: "flex", alignItems: "center", gap: 10 }}>
          <GuitarPhoto g={g} size={44} />
          <span style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
            <span style={{ fontSize: 17, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{g.name}</span>
            <span style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden" }}>
              <svg width="14" height="8" viewBox="0 0 14 8" aria-hidden style={{ flexShrink: 0 }}>
                <path d="M0 4h12M9 1l3 3-3 3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
              <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.name}</span>
            </span>
          </span>
        </span>
        {/* Guitar · Audio · MIDI */}
        <span role="tablist" style={{ display: "flex", padding: 3, gap: 2, borderRadius: 10, background: "#0a0a0c", boxShadow: "inset 0 0 0 1px var(--rule)" }}>
          {tabs.map((t) => {
            const on = t.id === tab;
            return (
              <button
                key={t.id}
                role="tab"
                aria-selected={on}
                onClick={() => onTab(t.id)}
                className={on ? "" : "pressable"}
                style={{ height: 38, minWidth: 84, padding: "0 16px", borderRadius: 8, fontSize: 14, fontWeight: on ? 750 : 600, color: on ? "var(--ink)" : "var(--ink-3)", background: on ? "#26262c" : "transparent", boxShadow: on ? "0 1px 0 rgba(255,255,255,0.06) inset, 0 1px 3px rgba(0,0,0,0.5)" : undefined }}
              >
                {t.label}
              </button>
            );
          })}
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {tab === "guitar" && <GuitarTab g={g} rig={r} />}
        {tab === "audio" && <AudioTab r={r} />}
        {tab === "midi" && <MidiTab r={r} />}
      </div>
    </div>
  );
}

// ── Guitar ─────────────────────────────────────────────────────────

function GuitarTab({ g, rig }: { g: Guitar; rig: Rig }) {
  const over = overriddenOn(g, rig.id);
  // Where changes land: the guitar's default (every rig), or only this rig.
  const [scope, setScope] = useState<ToneScope>("guitar");
  useEffect(() => setScope("guitar"), [g.id]);
  const tone = scope === "guitar" ? g.tone : toneOn(g, rig.id);
  const add = useMenu();
  return (
    <div style={{ display: "flex", alignItems: "flex-start", minHeight: "100%" }}>
      {/* The guitar: its photo, upright, and its pickups. */}
      <aside style={{ position: "sticky", top: 0, width: 232, flexShrink: 0, padding: 16, display: "flex", flexDirection: "column", gap: 14, borderRight: "1px solid var(--rule)", alignSelf: "stretch" }}>
        <GuitarPhoto g={g} radius={10} style={{ width: "100%", height: 260 }} />
        <span style={{ fontSize: 20, fontWeight: 800, letterSpacing: "-0.01em", lineHeight: 1.15 }}>{g.name}</span>
        <div style={{ display: "flex", flexDirection: "column" }}>
          {g.pickups.map((p, i) => (
            <label key={i} style={{ display: "flex", flexDirection: "column", gap: 4, padding: "8px 0", borderTop: "1px solid var(--rule)" }}>
              <span style={{ fontSize: 11.5, fontWeight: 750, letterSpacing: "0.07em", textTransform: "uppercase", color: "var(--ink-3)" }}>{p.position}</span>
              <Text value={p.model} placeholder="Pickup model" onCommit={(v) => editGuitar("pickup", (y) => ({ ...y, pickups: y.pickups.map((q, k) => (k === i ? { ...q, model: v } : q)) }))} />
            </label>
          ))}
          <button className="pressable" onClick={add.fromButton} style={{ display: "flex", alignItems: "center", gap: 8, minHeight: 44, color: "var(--ink-3)", fontSize: 14, fontWeight: 600, borderTop: "1px solid var(--rule)", width: "100%" }}>
            <Plus />
            Pickup
          </button>
          {add.open && (
            <Menu at={add.open.at} naming={0} items={[{ kind: "name", id: "add", label: "Pickup position…", initial: "Middle", confirm: "Add" }]} onPick={(p) => editGuitar("pickup added", (y) => ({ ...y, pickups: [...y.pickups, { position: p.text, model: "" }] }))} onClose={add.close} />
          )}
        </div>
      </aside>

      <div style={{ flex: 1, minWidth: 0 }}>
        {/* Where a change lands. */}
        <div style={{ position: "sticky", top: 0, zIndex: 2, display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap", minHeight: 60, padding: "8px 20px", background: "rgba(14,14,17,0.97)", borderBottom: "1px solid var(--rule)" }}>
          <span style={{ display: "flex", padding: 3, gap: 2, borderRadius: 10, background: "#0a0a0c", boxShadow: "inset 0 0 0 1px var(--rule)" }}>
            {(["guitar", "rig"] as ToneScope[]).map((sc) => {
              const on = sc === scope;
              return (
                <button
                  key={sc}
                  onClick={() => setScope(sc)}
                  aria-pressed={on}
                  className={on ? "" : "pressable"}
                  style={{ height: 38, padding: "0 14px", display: "flex", alignItems: "center", gap: 8, borderRadius: 8, fontSize: 13.5, fontWeight: on ? 750 : 600, whiteSpace: "nowrap", color: on ? "var(--ink)" : "var(--ink-3)", background: on ? (sc === "rig" ? `color-mix(in oklab, ${RIG_COLOUR} 22%, #1c1c21)` : "#26262c") : "transparent" }}
                >
                  {sc === "rig" && <OverrideIcon colour={on ? RIG_COLOUR : "currentColor"} size={12} />}
                  {sc === "guitar" ? "Guitar default" : `Only on ${rig.name}`}
                </button>
              );
            })}
          </span>
          {over.length > 0 && (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 6, fontSize: 13, fontWeight: 650, color: `color-mix(in oklab, ${RIG_COLOUR} 70%, var(--ink))` }}>
              <OverrideIcon colour={RIG_COLOUR} size={12} />
              {over.map((k) => TONE_NAME[k]).join(", ")}
            </span>
          )}
        </div>
        <ToneBlock title="Trim" keys={["trimDb"]} g={g} rig={rig} scope={scope} was={`${signed(g.tone.trimDb)} dB`}>
          <LevelMatch tone={tone} scope={scope} rig={rig} />
        </ToneBlock>
        <ToneBlock title="Gate" keys={["gates", "noisy"]} g={g} rig={rig} scope={scope} was={`Default ${g.tone.gates.default} dB${g.tone.noisy ? " · noisy" : ""}`}>
          <Gates tone={tone} scope={scope} />
        </ToneBlock>
        <ToneBlock title="Input EQ" keys={["eq"]} g={g} rig={rig} scope={scope} was={`Low cut ${g.tone.eq.lowCut} Hz`}>
          <GuitarEq tone={tone} scope={scope} />
        </ToneBlock>
      </div>
    </div>
  );
}

const signed = (v: number) => `${v > 0 ? "+" : ""}${v}`;

function Plus() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
      <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

/** A part of the guitar's tone: its title, and whether the rig overrides
 *  it — with Save to the guitar and Discard, as a section's override. */
function ToneBlock({ title, keys, g, rig, scope, was, children }: { title: string; keys: ToneKey[]; g: Guitar; rig: Rig; scope: ToneScope; was: string; children: ReactNode }) {
  const over = overriddenOn(g, rig.id).filter((k) => keys.includes(k));
  const showing = scope === "rig" && over.length > 0;
  return (
    <section style={{ position: "relative", padding: "18px 20px 22px", borderBottom: "1px solid var(--rule)", background: showing ? `color-mix(in oklab, ${RIG_COLOUR} 5%, transparent)` : undefined }}>
      {showing && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: RIG_COLOUR }} />}
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 14, flexWrap: "wrap" }}>
        <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>{title}</h2>
        {over.length > 0 ? (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 7, fontSize: 13, fontWeight: 650, color: `color-mix(in oklab, ${RIG_COLOUR} 70%, var(--ink))` }}>
            <OverrideIcon colour={RIG_COLOUR} size={13} />
            {scope === "rig" ? `Override on ${rig.name} · guitar: ${was}` : `Overridden on ${rig.name} — showing the guitar's`}
          </span>
        ) : null}
        <span style={{ flex: 1 }} />
        {over.length > 0 && (
          <span style={{ display: "flex", gap: 6 }}>
            <button className="pressable" onClick={() => discardToneOverrides(over)} style={{ height: 36, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 650, color: "var(--ink-2)" }}>
              Discard
            </button>
            <button className="pressable" onClick={() => saveToneOverrides(over)} title={`Make ${rig.name}'s ${title.toLowerCase()} the guitar's, on every rig`} style={{ height: 36, padding: "0 14px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, color: "var(--ink)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
              Save to guitar
            </button>
          </span>
        )}
      </div>
      {children}
    </section>
  );
}

// ── Audio ──────────────────────────────────────────────────────────

/** A dropdown: the value, and a chevron; it opens the choices as a menu. */
function Select<T extends string | number>({ label, value, options, show = String, detail, onPick, width }: { label: string; value: T; options: T[]; show?: (v: T) => string; detail?: (v: T) => string | undefined; onPick: (v: T) => void; width?: number | string }) {
  const menu = useMenu();
  return (
    <>
      <button
        className="pressable"
        onClick={menu.fromButton}
        aria-label={`${label}: ${show(value)}`}
        aria-haspopup="menu"
        style={{ width, minWidth: 0, height: 44, padding: "0 12px 0 14px", display: "flex", alignItems: "center", gap: 10, borderRadius: "var(--r)", background: "#17171b", boxShadow: "inset 0 0 0 1px var(--rule-strong)", textAlign: "left" }}
      >
        <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: 650, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{show(value)}</span>
        <svg width="11" height="7" viewBox="0 0 11 7" aria-hidden style={{ flexShrink: 0, color: "var(--ink-3)" }}>
          <path d="M1 1l4.5 4.5L10 1" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {menu.open && (
        <Menu
          at={menu.open.at}
          items={[{ kind: "head", label }, ...options.map((o): MenuItem => ({ kind: "run", id: String(o), label: show(o), detail: detail?.(o), checked: o === value }))]}
          onPick={(p) => {
            const o = options.find((x) => String(x) === p.id);
            if (o !== undefined) onPick(o);
          }}
          onClose={menu.close}
        />
      )}
    </>
  );
}

/** A labelled cell of the settings grid: a small label over its control. */
function Cell({ label, children, foot }: { label: string; children: ReactNode; foot?: ReactNode }) {
  return (
    <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 8, padding: "14px 18px 16px" }}>
      <span style={{ fontSize: 11.5, fontWeight: 750, letterSpacing: "0.07em", textTransform: "uppercase", color: "var(--ink-3)" }}>{label}</span>
      {children}
      {foot && <span style={{ fontSize: 12, color: "var(--ink-3)" }}>{foot}</span>}
    </div>
  );
}

function Toggle({ on, onFlip, label }: { on: boolean; onFlip: () => void; label: string }) {
  return (
    <button role="switch" aria-checked={on} aria-label={label} onClick={onFlip} className="pressable" style={{ width: 52, height: 44, display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0 }}>
      <span aria-hidden style={{ width: 46, height: 28, borderRadius: 999, padding: 3, background: on ? "var(--live)" : "#2b2b31", display: "flex", justifyContent: on ? "flex-end" : "flex-start" }}>
        <span style={{ width: 22, height: 22, borderRadius: 999, background: "#f4f4f5" }} />
      </span>
    </button>
  );
}

function AudioTab({ r }: { r: Rig }) {
  const a = r.audio;
  const dev = interfaceOf(r);
  const set = <K extends keyof Rig["audio"]>(label: string, k: K, v: Rig["audio"][K]) => editRig(label, (y) => ({ ...y, audio: { ...y.audio, [k]: v } }));
  const lat = latencyMs(r);
  const latColour = lat < 8 ? "var(--live)" : lat < 14 ? "#eab308" : "#f87171";
  return (
    <>
      {/* The interface: the device, and the three numbers that decide feel. */}
      <section style={{ borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap", padding: "18px 20px 6px" }}>
          <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>Interface</h2>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 7, fontSize: 13, color: "var(--ink-3)" }}>
            <span style={{ width: 8, height: 8, borderRadius: 999, background: "var(--live)" }} />
            Connected · {dev.inputs.length} in · {dev.outputs.length * 2 + dev.phones.length * 2} out
          </span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1.6fr) repeat(3, minmax(0, 1fr))" }}>
          <Cell label="Device">
            <Select
              label="Interface"
              value={a.device}
              options={INTERFACES.map((i) => i.name)}
              onPick={(v) => {
                const d = INTERFACES.find((i) => i.name === v)!;
                // A new device: keep what it can do, else its nearest.
                editRig(`interface ${v}`, (y) => ({
                  ...y,
                  audio: {
                    ...y.audio,
                    device: v,
                    input: d.inputs.find((i) => i.name === y.audio.input)?.name ?? d.inputs[0].name,
                    rate: d.rates.includes(y.audio.rate) ? y.audio.rate : d.rates.includes(48000) ? 48000 : d.rates[0],
                    buffer: d.buffers.includes(y.audio.buffer) ? y.audio.buffer : d.buffers.includes(128) ? 128 : d.buffers[0],
                    house: d.outputs.includes(y.audio.house) ? y.audio.house : d.outputs[0],
                    phones: d.phones.includes(y.audio.phones) ? y.audio.phones : d.phones[0],
                  },
                }));
              }}
            />
          </Cell>
          <Cell label="Sample rate">
            <Select label="Sample rate" value={a.rate} options={dev.rates} show={khz} onPick={(v) => set("sample rate", "rate", v)} />
          </Cell>
          <Cell label="Buffer">
            <Select label="Buffer size" value={a.buffer} options={dev.buffers} show={(b) => `${b} samples`} detail={(b) => `${((b * 1000) / a.rate).toFixed(1)} ms`} onPick={(v) => set("buffer", "buffer", v)} />
          </Cell>
          <Cell label="Round trip">
            <span style={{ height: 44, display: "flex", alignItems: "baseline", gap: 5 }}>
              <span className="num" style={{ fontSize: 30, fontWeight: 800, color: latColour, lineHeight: "44px" }}>
                {lat.toFixed(1)}
              </span>
              <span style={{ fontSize: 14, fontWeight: 650, color: "var(--ink-3)" }}>ms</span>
            </span>
          </Cell>
        </div>
      </section>

      {/* The guitar's input: the interface's inputs, each with its level. */}
      <section style={{ padding: "18px 20px 20px", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: 12, marginBottom: 14 }}>
          <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>Guitar input</h2>
        </div>
        <Inputs r={r} />
        <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 14, minHeight: 52, borderTop: "1px solid var(--rule)" }}>
          <span style={{ flex: 1, fontSize: 15, fontWeight: 700 }}>Direct monitor</span>
          <Toggle on={a.directMonitor} label="Direct monitor" onFlip={() => set(a.directMonitor ? "direct monitor off" : "direct monitor on", "directMonitor", !a.directMonitor)} />
        </div>
      </section>

      {/* Outputs: the house and the phones, each with its level and a check. */}
      <section style={{ borderBottom: "1px solid var(--rule)" }}>
        <div style={{ padding: "18px 20px 4px" }}>
          <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>Outputs</h2>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))" }}>
          <OutputStrip kind="house" r={r} />
          <OutputStrip kind="phones" r={r} />
        </div>
      </section>
    </>
  );
}

function Inputs({ r }: { r: Rig }) {
  const sig = useSignal();
  const dev = interfaceOf(r);
  return (
    <div style={{ display: "grid", gridTemplateColumns: `repeat(${Math.min(dev.inputs.length, 4)}, minmax(0, 1fr))`, gap: 8 }}>
      {dev.inputs.map((inp, i) => {
        const on = inp.name === r.audio.input;
        // Only the guitar's input has signal; the rest sit at the floor.
        const lvl = on ? sig.input : 0.02 + 0.01 * ((i * 7) % 3);
        return (
          <button
            key={inp.name}
            onClick={() => !on && editRig(`input ${inp.name}`, (y) => ({ ...y, audio: { ...y.audio, input: inp.name } }))}
            className={on ? "" : "pressable"}
            aria-pressed={on}
            style={{ position: "relative", display: "flex", alignItems: "center", gap: 12, minHeight: 72, padding: "10px 12px", borderRadius: "var(--r-md)", textAlign: "left", background: on ? "rgba(34,197,94,0.08)" : "#141418", boxShadow: on ? "inset 0 0 0 2px var(--live)" : "inset 0 0 0 1px var(--rule)" }}
          >
            {/* Its level, upright. */}
            <span aria-hidden style={{ position: "relative", width: 8, height: 48, borderRadius: 3, background: "#08080a", overflow: "hidden", flexShrink: 0 }}>
              <span style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: `${Math.min(100, lvl * 100)}%`, background: lvl > 0.9 ? "#f87171" : "var(--live)" }} />
            </span>
            <span style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
              <span style={{ fontSize: 15, fontWeight: on ? 750 : 600 }}>{inp.name}</span>
              <span style={{ fontSize: 12, color: on ? "var(--live)" : "var(--ink-3)", fontWeight: on ? 650 : 500 }}>{on ? "Guitar" : inp.kind}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** One output: where it goes, its level over its meter, and a check that
 *  plays left, right, then both — so a swapped cable or a dead side shows. */
function OutputStrip({ kind, r }: { kind: "house" | "phones"; r: Rig }) {
  const sig = useSignal();
  const dev = interfaceOf(r);
  const a = r.audio;
  const db = kind === "house" ? a.houseDb : a.phonesDb;
  const [check, setCheck] = useState<{ start: number } | null>(null);
  const [, tick] = useState(0);
  useEffect(() => {
    if (!check) return;
    const id = setInterval(() => {
      if (performance.now() - check.start > 3600) setCheck(null);
      tick((n) => n + 1);
    }, 100);
    return () => clearInterval(id);
  }, [check]);
  const phase = check ? Math.floor((performance.now() - check.start) / 1200) : -1;
  const side: "L" | "R" | "LR" | null = phase === 0 ? "L" : phase === 1 ? "R" : phase === 2 ? "LR" : null;
  const level = (kind === "house" ? sig.output : sig.phones) * Math.pow(10, db / 40);
  const fill = (db + 60) / 66;
  const setDb = (v: number) => tuneRig((y) => ({ ...y, audio: { ...y.audio, [kind === "house" ? "houseDb" : "phonesDb"]: v } }));
  return (
    <div style={{ padding: "12px 20px 20px", display: "flex", flexDirection: "column", gap: 12, minWidth: 0 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {kind === "house" ? <SpeakersGlyph side={side} /> : <PhonesGlyph side={side} />}
        <span style={{ fontSize: 16, fontWeight: 750 }}>{kind === "house" ? "House" : "Phones"}</span>
        <span style={{ flex: 1 }} />
        <Select
          label={kind === "house" ? "House output" : "Phones output"}
          value={kind === "house" ? a.house : a.phones}
          options={kind === "house" ? dev.outputs : dev.phones}
          width={170}
          onPick={(v) => editRig(`${kind} output`, (y) => ({ ...y, audio: { ...y.audio, [kind]: v } }))}
        />
      </div>
      <Fader value={db} min={-60} max={6} step={0.5} label={`${kind === "house" ? "House" : "Phones"} level`} level={Math.min(1, level) * fill} hot={level > 0.95} show={(v) => (v <= -60 ? "−∞" : `${signed(v)} dB`)} onChange={setDb} />
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <button
          className={check ? "" : "pressable"}
          disabled={!!check}
          onClick={() => setCheck({ start: performance.now() })}
          style={{ flexShrink: 0, whiteSpace: "nowrap", height: 44, padding: "0 16px", display: "flex", alignItems: "center", gap: 8, borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, color: check ? "#04210f" : "var(--ink)", background: check ? "var(--live)" : "transparent", boxShadow: check ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {check ? (side === "L" ? "Left…" : side === "R" ? "Right…" : "Both…") : kind === "house" ? "Check speakers" : "Check phones"}
        </button>
      </div>
    </div>
  );
}

/** A wide touch fader: its value written in it, the signal moving along
 *  its foot; the whole strip is the target. */
function Fader({ value, min, max, step, label, level, hot, show, onChange }: { value: number; min: number; max: number; step: number; label: string; level?: number; hot?: boolean; show: (v: number) => string; onChange: (v: number) => void }) {
  const fill = (value - min) / (max - min);
  return (
    <div style={{ position: "relative", flex: 1, minWidth: 0, height: 52, borderRadius: 10, background: "#0b0b0e", boxShadow: "inset 0 0 0 1px var(--rule)", overflow: "hidden" }}>
      <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: `${fill * 100}%`, background: "linear-gradient(90deg, #1e1e24, #2a2a31)" }} />
      {level !== undefined && <span aria-hidden style={{ position: "absolute", left: 0, bottom: 0, height: 4, width: `${Math.max(0, Math.min(1, level)) * 100}%`, background: hot ? "#f87171" : "var(--live)" }} />}
      <span aria-hidden style={{ position: "absolute", top: 8, bottom: 8, left: `calc(${fill * 100}% - 2px)`, width: 4, borderRadius: 2, background: "#f4f4f5" }} />
      <span className="num" style={{ position: "absolute", left: 14, top: 0, bottom: 0, display: "flex", alignItems: "center", fontSize: 18, fontWeight: 800, pointerEvents: "none" }}>
        {show(value)}
      </span>
      <input type="range" min={min} max={max} step={step} value={value} aria-label={label} onChange={(e) => onChange(Number(e.target.value))} style={{ position: "absolute", inset: 0, width: "100%", height: "100%", opacity: 0, cursor: "ew-resize", margin: 0 }} />
    </div>
  );
}

function PhonesGlyph({ side }: { side: "L" | "R" | "LR" | null }) {
  const lit = (s: "L" | "R") => (side === s || side === "LR" ? "var(--live)" : "#3f3f46");
  return (
    <svg width="30" height="26" viewBox="0 0 30 26" aria-hidden style={{ flexShrink: 0 }}>
      <path d="M4 16v-3a11 11 0 0 1 22 0v3" fill="none" stroke="#71717a" strokeWidth="2" strokeLinecap="round" />
      <rect x="2" y="15" width="7" height="10" rx="2.5" fill={lit("L")} />
      <rect x="21" y="15" width="7" height="10" rx="2.5" fill={lit("R")} />
    </svg>
  );
}

function SpeakersGlyph({ side }: { side: "L" | "R" | "LR" | null }) {
  const lit = (s: "L" | "R") => (side === s || side === "LR" ? "var(--live)" : "#3f3f46");
  return (
    <svg width="30" height="26" viewBox="0 0 30 26" aria-hidden style={{ flexShrink: 0 }}>
      {(["L", "R"] as const).map((s, i) => (
        <g key={s} transform={`translate(${i * 16} 0)`}>
          <rect x="1" y="2" width="12" height="22" rx="2.5" fill="none" stroke={lit(s)} strokeWidth="1.8" />
          <circle cx="7" cy="16" r="3.6" fill={lit(s)} />
          <circle cx="7" cy="7.5" r="1.8" fill={lit(s)} />
        </g>
      ))}
    </svg>
  );
}

// ── MIDI ───────────────────────────────────────────────────────────

interface Heard {
  at: number;
  text: string;
}

function MidiTab({ r }: { r: Rig }) {
  const m = r.midi;
  const ctl = MIDI_DEVICES.find((d) => d.name === m.device);
  const [pressed, setPressed] = useState<number | null>(null);
  const [heard, setHeard] = useState<Heard[]>([]);
  const set = <K extends keyof Rig["midi"]>(label: string, k: K, v: Rig["midi"][K]) => editRig(label, (y) => ({ ...y, midi: { ...y.midi, [k]: v } }));
  const press = (i: number) => {
    setPressed(i);
    setTimeout(() => setPressed((p) => (p === i ? null : p)), 220);
    const ch = m.channel === "Omni" ? 1 : m.channel;
    setHeard((h) => [{ at: Date.now(), text: `Switch ${i + 1} · CC ${20 + i} · 127 · ch ${ch}` }, ...h].slice(0, 6));
  };
  return (
    <>
      <section style={{ borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap", padding: "18px 20px 6px" }}>
          <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>Controller</h2>
          {ctl && (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 7, fontSize: 13, color: "var(--ink-3)" }}>
              <span style={{ width: 8, height: 8, borderRadius: 999, background: "var(--live)" }} />
              {ctl.link === "Bluetooth" ? "Bluetooth MIDI · connected" : "USB · connected"}
            </span>
          )}
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1.6fr) repeat(2, minmax(0, 1fr))" }}>
          <Cell label="Device">
            <Select label="Controller" value={m.device} options={[...MIDI_DEVICES.map((d) => d.name), "None"]} detail={(v) => MIDI_DEVICES.find((d) => d.name === v)?.link} onPick={(v) => set(`controller ${v}`, "device", v)} />
          </Cell>
          <Cell label="Listens on">
            <Select label="MIDI channel" value={m.channel} options={["Omni", ...Array.from({ length: 16 }, (_, i) => i + 1)] as (number | "Omni")[]} show={(c) => (c === "Omni" ? "Omni · all" : `Channel ${c}`)} onPick={(v) => set("channel", "channel", v)} />
          </Cell>
          <Cell label="Clock">
            <Select label="MIDI clock" value={m.clock} options={["off", "send", "receive"] as Rig["midi"]["clock"][]} show={(c) => (c === "off" ? "Off" : c === "send" ? "Send tempo" : "Follow it")} onPick={(v) => set("clock", "clock", v)} />
          </Cell>
        </div>
      </section>
      {ctl && (
        <section style={{ padding: "18px 20px 20px", borderBottom: "1px solid var(--rule)" }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 12, marginBottom: 14 }}>
            <h2 style={{ margin: 0, fontSize: 19, fontWeight: 750, letterSpacing: "-0.01em" }}>Test it</h2>
          </div>
          {/* The controller, top down: its switches in a row. */}
          <div style={{ display: "flex", gap: 10, padding: 14, borderRadius: 12, background: "linear-gradient(180deg, #1c1c21, #121215)", boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
            {Array.from({ length: ctl.switches }, (_, i) => {
              const on = pressed === i;
              return (
                <button key={i} onClick={() => press(i)} aria-label={`Switch ${i + 1}`} className="pressable" style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", alignItems: "center", gap: 8, padding: "8px 0" }}>
                  <span aria-hidden style={{ width: 10, height: 10, borderRadius: 999, background: on ? "var(--live)" : "#2b2b31", boxShadow: on ? "0 0 10px var(--live)" : undefined }} />
                  <span aria-hidden style={{ width: 44, height: 44, borderRadius: 999, background: on ? "#3a3a42" : "#2a2a30", boxShadow: "inset 0 -3px 0 rgba(0,0,0,0.45), inset 0 0 0 1px rgba(255,255,255,0.08)", transform: on ? "translateY(1px)" : undefined }} />
                  <span className="num" style={{ fontSize: 12, fontWeight: 700, color: "var(--ink-3)" }}>{i + 1}</span>
                </button>
              );
            })}
          </div>
          <div style={{ marginTop: 12, minHeight: 44, display: "flex", flexDirection: "column" }}>
            {heard.length === 0 ? (
              <span style={{ fontSize: 13, color: "var(--ink-3)", padding: "12px 0" }}>—</span>
            ) : (
              heard.map((h, i) => (
                <span key={h.at + h.text} className="num" style={{ display: "flex", gap: 12, minHeight: 32, alignItems: "center", fontSize: 13.5, color: i === 0 ? "var(--ink)" : "var(--ink-3)", borderTop: i ? "1px solid var(--rule)" : undefined }}>
                  {h.text}
                </span>
              ))
            )}
          </div>
        </section>
      )}
      <section style={{ padding: "6px 20px 10px", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 64 }}>
          <span style={{ flex: 1, fontSize: 15, fontWeight: 700 }}>Program changes</span>
          <Toggle on={m.programChange} label="Program changes" onFlip={() => set(m.programChange ? "program changes off" : "program changes on", "programChange", !m.programChange)} />
        </div>
      </section>
    </>
  );
}

// ── Pieces ─────────────────────────────────────────────────────────

/** A row in the list: the one in use marked green; the one open on the
 *  right in a light wash. */
function ListRow({ inUse, shown, onPick, lead, title, sub, menu }: { inUse: boolean; shown: boolean; onPick: () => void; lead: ReactNode; title: string; sub?: ReactNode; menu: ReactNode }) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", borderTop: "1px solid var(--rule)", background: shown ? "rgba(255,255,255,0.06)" : undefined }}>
      {inUse && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: "0 2px 2px 0", background: "var(--live)" }} />}
      <button className="pressable" onClick={onPick} aria-current={inUse ? "true" : undefined} style={{ flex: 1, minWidth: 0, minHeight: 68, display: "flex", alignItems: "center", gap: 12, padding: "8px 4px 8px 14px", textAlign: "left" }}>
        {lead}
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
          <span style={{ fontSize: 15, fontWeight: inUse ? 700 : 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</span>
          {sub && (
            <span className="t-meta" style={{ fontSize: 12.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {sub}
            </span>
          )}
        </span>
      </button>
      {menu}
    </div>
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

function BackButton({ onList }: { onList: () => void }) {
  return (
    <button className="pressable" onClick={onList} aria-label="Guitars and rigs" style={{ width: 44, height: 44, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink)", borderRadius: "var(--r)", background: "rgba(0,0,0,0.45)" }}>
      <svg width="9" height="15" viewBox="0 0 9 15" aria-hidden>
        <path d="M7.5 1.5 1.5 7.5l6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
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


// ── Tone: EQ, level, gates ─────────────────────────────────────────

function GuitarEq({ tone, scope }: { tone: Tone; scope: ToneScope }) {
  const eq = tone.eq;
  const set = (k: keyof typeof eq, v: number) => tuneTone(scope, "eq", (t) => ({ ...t, eq: { ...t.eq, [k]: v } }));
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

/** The trim, against the level: the guitar's level on a dBFS scale with
 *  the band presets expect and its peak; a wide fader for the trim, and
 *  Match level, which listens and trims the peaks into the band. */
function LevelMatch({ tone, scope, rig }: { tone: Tone; scope: ToneScope; rig: Rig }) {
  const sig = useSignal();
  const target = rig.audio.targetDb;
  const now = levelDb(sig.input, tone.trimDb);
  const [peak, setPeak] = useState(-90);
  const [matching, setMatching] = useState<{ until: number; max: number } | null>(null);
  useEffect(() => {
    setPeak((p) => Math.max(now, p - 0.35));
    if (matching) {
      const max = Math.max(matching.max, now - tone.trimDb);
      if (performance.now() > matching.until) {
        // Trim the measured peaks onto the target.
        editTone("level matched", scope, "trimDb", (y) => ({ ...y, trimDb: Math.max(-24, Math.min(24, Math.round((target - max) * 2) / 2)) }));
        setMatching(null);
      } else if (max !== matching.max) setMatching({ ...matching, max });
    }
  });
  const pos = (db: number) => `${((db + 60) / 60) * 100}%`;
  const inBand = Math.abs(peak - target) <= 3;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
        {/* −60…0 dBFS: the band presets expect, the level, its peak. */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ position: "relative", height: 22, borderRadius: 5, background: "#08080a", overflow: "hidden" }}>
            <span aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: pos(target - 3), width: `${(6 / 60) * 100}%`, background: "rgba(34,197,94,0.14)", boxShadow: "inset 0 0 0 1px rgba(34,197,94,0.45)" }} />
            <span aria-hidden style={{ position: "absolute", top: 5, bottom: 5, left: 0, width: pos(Math.max(-60, now)), borderRadius: 3, background: "linear-gradient(90deg, #15803d, #22c55e 70%, #eab308 90%, #f87171)" }} />
            <span aria-hidden style={{ position: "absolute", top: 2, bottom: 2, left: `calc(${pos(Math.max(-60, peak))} - 1px)`, width: 2, background: inBand ? "var(--live)" : "#f4f4f5" }} />
          </div>
          <div className="num" style={{ position: "relative", height: 14, marginTop: 3, fontSize: 11, color: "var(--ink-3)" }}>
            {[-60, -45, -30, -15, 0].map((d) => (
              <span key={d} style={{ position: "absolute", left: pos(d), transform: d === -60 ? undefined : d === 0 ? "translateX(-100%)" : "translateX(-50%)" }}>
                {d === 0 ? "0" : `−${-d}`}
              </span>
            ))}
          </div>
        </div>
        <span className="num" style={{ width: 92, flexShrink: 0, display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 1 }}>
          <span style={{ fontSize: 17, fontWeight: 800, color: inBand ? "var(--live)" : "var(--ink)" }}>{Math.round(peak)}</span>
          <span style={{ fontSize: 11, color: "var(--ink-3)" }}>peak · {target}</span>
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <Fader value={tone.trimDb} min={-24} max={24} step={0.5} label="Trim" show={(v) => `${signed(v)} dB`} onChange={(v) => tuneTone(scope, "trimDb", (y) => ({ ...y, trimDb: v }))} />
        <button
          className={matching ? "" : "pressable"}
          disabled={!!matching}
          onClick={() => setMatching({ until: performance.now() + 4000, max: -90 })}
          style={{ flexShrink: 0, height: 52, minWidth: 128, padding: "0 16px", borderRadius: 10, fontSize: 14, fontWeight: 700, whiteSpace: "nowrap", color: matching ? "#04210f" : "var(--ink)", background: matching ? "var(--live)" : "transparent", boxShadow: matching ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {matching ? "Play…" : "Match level"}
        </button>
      </div>
    </div>
  );
}

type Threshold = Exclude<GateLevel, "off">;
const THRESHOLDS: Threshold[] = ["subtle", "default", "tight", "ultra"];
const GATE_MIN = -96;
const GATE_MAX = -24;

/** The four gate thresholds on one strip, over the live level: drag the
 *  nearest marker, or step a value; Measure sets all four above the noise
 *  floor. Noisy input lifts Off to Subtle and Subtle to Default. */
function Gates({ tone, scope }: { tone: Tone; scope: ToneScope }) {
  const sig = useSignal();
  const now = levelDb(sig.input, tone.trimDb);
  const [measuring, setMeasuring] = useState<{ until: number; floor: number } | null>(null);
  const drag = useRef<{ which: Threshold; moved: boolean } | null>(null);
  useEffect(() => {
    if (!measuring) return;
    // The floor: the quietest the input gets while nothing is played.
    const floor = Math.min(measuring.floor, now);
    if (performance.now() > measuring.until) {
      const f = Math.round(floor);
      editTone("gates measured", scope, "gates", (y) => ({ ...y, gates: gatesAbove(f) }));
      setMeasuring(null);
    } else if (floor !== measuring.floor) setMeasuring({ ...measuring, floor });
  });
  const frac = (db: number) => (db - GATE_MIN) / (GATE_MAX - GATE_MIN);
  const pct = (db: number) => `${Math.max(0, Math.min(1, frac(db))) * 100}%`;
  // Keep the four in order, a dB apart.
  const place = (which: Threshold, db: number, gates: Tone["gates"]) => {
    const i = THRESHOLDS.indexOf(which);
    const lo = i > 0 ? gates[THRESHOLDS[i - 1]] + 1 : GATE_MIN;
    const hi = i < 3 ? gates[THRESHOLDS[i + 1]] - 1 : GATE_MAX;
    return { ...gates, [which]: Math.round(Math.max(lo, Math.min(hi, db))) };
  };
  const dbAt = (e: React.PointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return GATE_MIN + ((e.clientX - r.left) / r.width) * (GATE_MAX - GATE_MIN);
  };
  const set = (which: Threshold, db: number, undoable: boolean) =>
    (undoable ? (f: (t: Tone) => Tone) => editTone(`${GATE_NAME[which]} gate`, scope, "gates", f) : (f: (t: Tone) => Tone) => tuneTone(scope, "gates", f))((y) => ({ ...y, gates: place(which, db, y.gates) }));
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* The strip: the level moving, the four thresholds over it. */}
      <div
        onPointerDown={(e) => {
          const db = dbAt(e);
          const which = THRESHOLDS.reduce((a, b) => (Math.abs(tone.gates[b] - db) < Math.abs(tone.gates[a] - db) ? b : a));
          drag.current = { which, moved: false };
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          // The first move is the undo step; the rest follow the finger.
          set(d.which, dbAt(e), !d.moved);
          d.moved = true;
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
        style={{ position: "relative", height: 64, borderRadius: 10, background: "#08080a", boxShadow: "inset 0 0 0 1px var(--rule)", touchAction: "none", cursor: "ew-resize", overflow: "hidden" }}
      >
        <span aria-hidden style={{ position: "absolute", left: 0, top: 26, bottom: 10, width: pct(now), borderRadius: "0 3px 3px 0", background: now >= tone.gates.default ? "rgba(34,197,94,0.55)" : "#3f3f46" }} />
        {THRESHOLDS.map((t) => {
          const open = now >= tone.gates[t];
          return (
            <span key={t} aria-hidden style={{ position: "absolute", top: 0, bottom: 0, left: pct(tone.gates[t]), transform: "translateX(-50%)", display: "flex", flexDirection: "column", alignItems: "center", pointerEvents: "none" }}>
              <span style={{ marginTop: 5, width: 18, height: 16, borderRadius: 4, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 10.5, fontWeight: 800, color: open ? "#04210f" : "var(--ink)", background: open ? "var(--live)" : "#3a3a42" }}>{GATE_NAME[t][0]}</span>
              <span style={{ flex: 1, width: 2, marginTop: 2, background: "#f4f4f5", opacity: 0.85 }} />
            </span>
          );
        })}
      </div>
      {/* Each threshold, to step. */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 8 }}>
        {THRESHOLDS.map((t) => (
          <div key={t} style={{ display: "flex", alignItems: "center", gap: 4, padding: "6px 4px 6px 12px", borderRadius: 8, background: "#141418" }}>
            <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 1 }}>
              <span style={{ fontSize: 11.5, fontWeight: 750, letterSpacing: "0.06em", textTransform: "uppercase", color: "var(--ink-3)" }}>{GATE_NAME[t]}</span>
              <span className="num" style={{ fontSize: 16, fontWeight: 800 }}>{tone.gates[t]}</span>
            </span>
            {[-1, 1].map((d) => (
              <button key={d} className="pressable" aria-label={`${GATE_NAME[t]} ${d < 0 ? "lower" : "higher"}`} onClick={() => set(t, tone.gates[t] + d, true)} style={{ width: 36, height: 44, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-2)", fontSize: 20, fontWeight: 600 }}>
                {d < 0 ? "−" : "+"}
              </button>
            ))}
          </div>
        ))}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap", minHeight: 52, borderTop: "1px solid var(--rule)", paddingTop: 8 }}>
        <button
          className={measuring ? "" : "pressable"}
          disabled={!!measuring}
          onClick={() => setMeasuring({ until: performance.now() + 3000, floor: 0 })}
          style={{ height: 44, padding: "0 16px", borderRadius: "var(--r)", fontSize: 14, fontWeight: 700, whiteSpace: "nowrap", color: measuring ? "#04210f" : "var(--ink)", background: measuring ? "var(--live)" : "transparent", boxShadow: measuring ? undefined : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {measuring ? "Hands off the strings…" : "Measure noise floor"}
        </button>
        <span style={{ flex: 1 }} />
        <span style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 2 }}>
          <span style={{ fontSize: 15, fontWeight: 700 }}>Noisy input</span>
          {tone.noisy && <span style={{ fontSize: 12, color: "var(--ink-3)" }}>{(["off", "subtle"] as GateLevel[]).map((l) => `${GATE_NAME[l]} → ${GATE_NAME[gateFor(tone, l)]}`).join(" · ")}</span>}
        </span>
        <Toggle on={tone.noisy} label="Noisy input" onFlip={() => editTone(tone.noisy ? "noisy input off" : "noisy input on", scope, "noisy", (y) => ({ ...y, noisy: !y.noisy }))} />
      </div>
    </div>
  );
}
