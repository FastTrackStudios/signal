// The two safety buttons, side by side on every device:
//
//   Mute   a tap mutes the house (the main outs) — the phones keep playing,
//          so you can rehearse, tune or fix something mid-set. Hold it for
//          the rest: mute the phones too, mute everything, unmute all.
//          Red and crossed while anything is muted, saying what.
//   Panic  for when something is stuck: every note off on every MIDI
//          channel, the audio engine stopped, cleared and started again.
//          One tap — it's for emergencies — and it shows it's working.

import { useRef } from "react";
import { panic, setMutes, toggleHouseMute, useStore } from "../store";
import { Menu, useMenu, type MenuItem, type Picked } from "./Menu";

const HOLD_MS = 500;

export function MuteButton({ size = 36, label }: { size?: number; label?: boolean }) {
  const s = useStore();
  const menu = useMenu();
  const held = useRef<{ t: number; fired: boolean } | null>(null);
  const any = s.houseMute || s.phonesMute;
  const what = s.houseMute && s.phonesMute ? "All muted" : s.houseMute ? "House muted" : s.phonesMute ? "Phones muted" : "Mute house";
  const items: MenuItem[] = [
    { kind: "head", label: "Mute" },
    { kind: "run", id: "house", label: "House", detail: "main outs", checked: s.houseMute },
    { kind: "run", id: "phones", label: "Phones", detail: "in-ears", checked: s.phonesMute },
    { kind: "sep" },
    { kind: "run", id: "all", label: "Mute everything", disabled: s.houseMute && s.phonesMute ? "Everything is muted" : undefined },
    { kind: "run", id: "none", label: "Unmute all", disabled: any ? undefined : "Nothing is muted" },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "house") setMutes(!s.houseMute, s.phonesMute);
    if (p.id === "phones") setMutes(s.houseMute, !s.phonesMute);
    if (p.id === "all") setMutes(true, true);
    if (p.id === "none") setMutes(false, false);
  };
  return (
    <>
      <button
        aria-label={`${what} — tap to ${s.houseMute ? "unmute" : "mute"} the house, hold for more`}
        title={`${what} — tap: house · hold: phones, everything`}
        aria-pressed={any}
        onPointerDown={(e) => {
          const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
          held.current = { t: window.setTimeout(() => {
            if (held.current) held.current.fired = true;
            menu.setOpen({ at: { x: rect.left, y: rect.bottom + 4 } });
          }, HOLD_MS), fired: false };
        }}
        onPointerUp={() => {
          const h = held.current;
          held.current = null;
          if (!h) return;
          window.clearTimeout(h.t);
          if (!h.fired) toggleHouseMute();
        }}
        onPointerLeave={() => {
          if (held.current && !held.current.fired) window.clearTimeout(held.current.t);
          held.current = null;
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
          menu.setOpen({ at: { x: rect.left, y: rect.bottom + 4 } });
        }}
        style={{
          flex: label ? 1 : undefined,
          height: size,
          minWidth: size,
          padding: label ? "0 12px" : 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          borderRadius: "var(--r)",
          fontSize: 12,
          fontWeight: 750,
          letterSpacing: "0.05em",
          whiteSpace: "nowrap",
          color: any ? "#1a0505" : "var(--ink-2)",
          background: any ? "var(--void)" : "transparent",
          boxShadow: any ? undefined : "inset 0 0 0 1px var(--rule-strong)",
          animation: any ? "mute-pulse 1.6s ease-in-out infinite" : undefined,
          touchAction: "none",
        }}
      >
        <SpeakerIcon muted={any} phones={s.phonesMute && !s.houseMute} />
        {label && what.toUpperCase()}
      </button>
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
      <style>{`@keyframes mute-pulse { 50% { filter: brightness(0.82) } } @media (prefers-reduced-motion: reduce) { [aria-pressed="true"] { animation: none !important } }`}</style>
    </>
  );
}

/** A speaker; crossed when muted. With only the phones muted, headphones. */
function SpeakerIcon({ muted, phones }: { muted: boolean; phones: boolean }) {
  if (phones)
    return (
      <svg width="17" height="17" viewBox="0 0 16 16" aria-hidden>
        <path d="M2.5 10V8a5.5 5.5 0 0 1 11 0v2" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <rect x="1.8" y="9.5" width="3" height="4.5" rx="1" fill="currentColor" />
        <rect x="11.2" y="9.5" width="3" height="4.5" rx="1" fill="currentColor" />
        <path d="M1.5 1.5l13 13" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    );
  return (
    <svg width="17" height="17" viewBox="0 0 16 16" aria-hidden>
      <path d="M2 6h2.5L8 3v10L4.5 10H2Z" fill="currentColor" />
      {muted ? (
        <path d="M10.5 6l4 4M14.5 6l-4 4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      ) : (
        <>
          <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          <path d="M12.5 3.8a6 6 0 0 1 0 8.4" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" opacity="0.7" />
        </>
      )}
    </svg>
  );
}

export function PanicButton({ size = 36, label }: { size?: number; label?: boolean }) {
  const s = useStore();
  const busy = s.panicAt !== null;
  return (
    <button
      onClick={() => !busy && panic()}
      aria-label="Panic — all notes off, reset audio and MIDI"
      title="Panic — all notes off on every MIDI channel, the audio engine stopped, cleared and restarted"
      aria-busy={busy}
      className={busy ? "" : "pressable"}
      style={{
        height: size,
        minWidth: size,
        padding: label ? "0 12px" : 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        borderRadius: "var(--r)",
        fontSize: 12,
        fontWeight: 750,
        letterSpacing: "0.05em",
        whiteSpace: "nowrap",
        color: busy ? "#1a1205" : "#fbbf24",
        background: busy ? "#fbbf24" : "transparent",
        boxShadow: busy ? undefined : "inset 0 0 0 1px color-mix(in srgb, #fbbf24 45%, var(--rule-strong))",
      }}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden style={{ animation: busy ? "panic-spin 0.8s linear infinite" : undefined }}>
        {busy ? (
          <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2.5v3.2h-3.2" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
        ) : (
          <>
            <path d="M5.2 1.5h5.6l3.7 3.7v5.6l-3.7 3.7H5.2l-3.7-3.7V5.2Z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
            <path d="M8 4.8v4" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
            <circle cx="8" cy="11.2" r="1" fill="currentColor" />
          </>
        )}
      </svg>
      {label && (busy ? "RESETTING…" : "PANIC")}
      <style>{`@keyframes panic-spin { to { transform: rotate(360deg) } }`}</style>
    </button>
  );
}
