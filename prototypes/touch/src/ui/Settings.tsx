// The MIDI and Audio indicators: a light that says the link is up, and a
// press away from its settings. Settings open as a sheet over the device
// they were asked from (a centred card on an iPad, a bottom sheet on a
// phone). The rig's real settings live on the engine; these are the
// prototype's stand-ins, laid out as the remote will show them.

import { useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useStage } from "./stage";

type Kind = "midi" | "audio";

const DETAIL: Record<Kind, string> = { midi: "Morningstar MC8 · in", audio: "voyager · 48 kHz · 128" };

/** A status light that opens its settings. `compact`: the light and an
 *  icon only, for a phone's bar. */
export function Indicator({ kind, ok = true, compact }: { kind: Kind; ok?: boolean; compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const label = kind === "midi" ? "MIDI" : "Audio";
  return (
    <>
      <button
        className="pressable"
        onClick={() => setOpen(true)}
        title={`${label}: ${DETAIL[kind]} — settings`}
        aria-label={`${label} ${ok ? "connected" : "down"} — open ${label} settings`}
        style={{ display: "flex", alignItems: "center", gap: 6, padding: compact ? "0 6px" : "0 10px", fontSize: 12, fontWeight: 650, color: "var(--ink-2)", borderRadius: "var(--r)" }}
      >
        <span style={{ width: 7, height: 7, borderRadius: 999, background: ok ? "var(--live)" : "var(--void)", flexShrink: 0 }} />
        {compact ? <KindIcon kind={kind} /> : label}
      </button>
      {open && <SettingsSheet kind={kind} onClose={() => setOpen(false)} />}
    </>
  );
}

function KindIcon({ kind }: { kind: Kind }) {
  return kind === "midi" ? (
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden>
      <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      {[
        [4.6, 8],
        [5.6, 5.4],
        [8, 4.4],
        [10.4, 5.4],
        [11.4, 8],
      ].map(([x, y]) => (
        <circle key={`${x}`} cx={x} cy={y} r="0.9" fill="currentColor" />
      ))}
    </svg>
  ) : (
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden>
      <path d="M1.5 8h2l1.5-4 2.5 8 2-6 1.5 4 1-2h2.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function SettingsSheet({ kind, onClose }: { kind: Kind; onClose: () => void }) {
  const stage = useStage();
  const narrow = (stage.el?.offsetWidth ?? 1000) < 700;
  const body = (
    <div style={{ position: "absolute", inset: 0, zIndex: 70, display: "flex", alignItems: narrow ? "flex-end" : "center", justifyContent: "center" }}>
      <button aria-label="Close" onClick={onClose} style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.6)" }} />
      <div
        role="dialog"
        aria-label={kind === "midi" ? "MIDI settings" : "Audio settings"}
        style={{
          position: "relative",
          width: narrow ? "100%" : 520,
          maxHeight: "90%",
          overflowY: "auto",
          background: "#141418",
          border: "1px solid var(--rule-strong)",
          borderRadius: narrow ? "14px 14px 0 0" : 14,
          boxShadow: "0 24px 60px rgba(0,0,0,0.6)",
          paddingBottom: narrow ? 30 : 8,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "14px 10px 10px 18px", borderBottom: "1px solid var(--rule)" }}>
          <span style={{ flex: 1, fontSize: 18, fontWeight: 750 }}>{kind === "midi" ? "MIDI" : "Audio"}</span>
          <button className="pressable" onClick={onClose} aria-label="Close" style={{ width: 40, height: 40, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--ink-3)" }}>
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
              <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
          </button>
        </div>
        {kind === "midi" ? <MidiBody /> : <AudioBody />}
      </div>
    </div>
  );
  return stage.el ? createPortal(body, stage.el) : body;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div style={{ padding: "12px 0 4px" }}>
      <div className="t-label" style={{ padding: "0 18px 6px", color: "var(--ink-3)" }}>
        {title}
      </div>
      {children}
    </div>
  );
}

function Row({ children }: { children: ReactNode }) {
  return <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 48, padding: "0 18px", borderTop: "1px solid var(--rule)" }}>{children}</div>;
}

function Choice<T extends string | number>({ options, value, onPick }: { options: T[]; value: T; onPick: (v: T) => void }) {
  return (
    <span style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
      {options.map((o) => (
        <button
          key={String(o)}
          onClick={() => onPick(o)}
          className={o === value ? "" : "pressable"}
          style={{ height: 34, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: o === value ? 700 : 560, color: o === value ? "var(--ink)" : "var(--ink-3)", background: o === value ? "var(--pressed-bg)" : "transparent", boxShadow: o === value ? "var(--pressed-shadow)" : "inset 0 0 0 1px var(--rule-strong)" }}
        >
          {String(o)}
        </button>
      ))}
    </span>
  );
}

function MidiBody() {
  const [ins, setIns] = useState({ "Morningstar MC8": true, "IAC Driver Bus 1": false });
  const [channel, setChannel] = useState<number | "Omni">("Omni");
  return (
    <>
      <Section title="Inputs">
        {Object.entries(ins).map(([name, on]) => (
          <Row key={name}>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: on ? "var(--live)" : "var(--dim)" }} />
            <span style={{ flex: 1, fontSize: 15, fontWeight: 600 }}>{name}</span>
            <button className="pressable" onClick={() => setIns({ ...ins, [name]: !on })} style={{ height: 34, padding: "0 12px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 650, boxShadow: "inset 0 0 0 1px var(--rule-strong)", color: on ? "var(--ink)" : "var(--ink-3)" }}>
              {on ? "On" : "Off"}
            </button>
          </Row>
        ))}
      </Section>
      <Section title="Listens on">
        <Row>
          <Choice options={["Omni", 1, 2, 3, 4] as (number | "Omni")[]} value={channel} onPick={setChannel} />
        </Row>
      </Section>
      <Section title="Footswitches">
        <Row>
          <span style={{ flex: 1, fontSize: 14, color: "var(--ink-2)" }}>Teach a switch: press Learn, then the switch</span>
          <button className="pressable" style={{ height: 36, padding: "0 14px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
            Learn…
          </button>
        </Row>
      </Section>
    </>
  );
}

function AudioBody() {
  const [rate, setRate] = useState<string>("48 kHz");
  const [buffer, setBuffer] = useState<number>(128);
  return (
    <>
      <Section title="Rig">
        <Row>
          <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--live)" }} />
          <span style={{ flex: 1, fontSize: 15, fontWeight: 600 }}>voyager</span>
          <span className="t-meta" style={{ fontSize: 13 }}>running · 2 in · 4 out</span>
        </Row>
      </Section>
      <Section title="Sample rate">
        <Row>
          <Choice options={["44.1 kHz", "48 kHz", "96 kHz"]} value={rate} onPick={setRate} />
        </Row>
      </Section>
      <Section title="Buffer">
        <Row>
          <Choice options={[64, 128, 256, 512]} value={buffer} onPick={setBuffer} />
          <span className="t-meta num" style={{ fontSize: 13, marginLeft: "auto" }}>
            {((buffer / (rate === "96 kHz" ? 96 : rate === "44.1 kHz" ? 44.1 : 48)) * 2).toFixed(1)} ms round trip
          </span>
        </Row>
      </Section>
      <Section title="Engine">
        <Row>
          <span style={{ flex: 1, fontSize: 14, color: "var(--ink-2)" }}>Stop and start the audio engine</span>
          <button className="pressable" style={{ height: 36, padding: "0 14px", borderRadius: "var(--r)", fontSize: 13, fontWeight: 700, boxShadow: "inset 0 0 0 1px var(--rule-strong)" }}>
            Restart audio
          </button>
        </Row>
      </Section>
    </>
  );
}
