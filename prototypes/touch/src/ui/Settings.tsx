// The MIDI and Audio indicators: a light that says the link is up, and a
// press away from its settings — Setup, on the MIDI or Audio tab.

import { useOpenSetup } from "../views/Setup";
import { currentController, currentRig, useStore, type State } from "../store";

type Kind = "midi" | "audio";

/** What the link is: the chosen controller, or the audio rig's numbers. */
export function linkDetail(s: State, kind: Kind): string {
  if (kind === "midi") return currentController(s).device;
  const r = currentRig(s);
  return `${r.name} · ${r.audio.rate / 1000} kHz · ${r.audio.buffer}`;
}

export type Health = "ok" | "warn" | "down";
const TONE: Record<Health, string> = { ok: "var(--live)", warn: "#eab308", down: "var(--void)" };
const SAYS: Record<Health, string> = { ok: "connected", warn: "struggling", down: "down" };

/** Its icon, coloured by how the link is — green, amber, red — and a
 *  press away from its settings. */
export function Indicator({ kind, health = "ok" }: { kind: Kind; health?: Health; compact?: boolean }) {
  const openSetup = useOpenSetup();
  const s = useStore();
  const label = kind === "midi" ? "MIDI" : "Audio";
  return (
    <button
        className="pressable"
        onClick={() => openSetup(kind)}
        title={`${label} ${SAYS[health]}: ${linkDetail(s, kind)}`}
        aria-label={`${label} ${SAYS[health]} — open ${label} in Setup`}
        style={{ alignSelf: "stretch", padding: "0 7px", display: "flex", alignItems: "center", color: TONE[health] }}
      >
        <KindIcon kind={kind} />
    </button>
  );
}

function KindIcon({ kind }: { kind: Kind }) {
  return kind === "midi" ? (
    <svg width="17" height="17" viewBox="0 0 16 16" aria-hidden>
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
    <svg width="17" height="17" viewBox="0 0 16 16" aria-hidden>
      <path d="M1.5 8h2l1.5-4 2.5 8 2-6 1.5 4 1-2h2.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
