// Where you are in the set, in one horizontal strip — for the landscape
// phone, between the macros and the switches. Not the setlist: no parts
// list, no reordering, no editing. The song up (its number in its colour,
// key and tempo, what comes next), and its sections as a row of chips in
// their colours, the one playing filled, with the part playing named on it.
// A tap on a section goes there; ‹ › step through the songs.

import { currentSet, goToPart, goToSong, playing, sectionsOf, useStore } from "../store";
import { sectionColour, songColour } from "./colors";
import { tapeFor } from "../ui/marks";
import { stackOf } from "../data/rig";

export function SetlistStrip() {
  const s = useStore();
  const set = currentSet(s);
  const song = set.songs[s.songIndex];
  const next = set.songs[s.songIndex + 1];
  if (!song) return null;
  const colour = songColour(song.name, s.songColours);
  const sections = sectionsOf(s, song.name);
  const now = playing(s);
  const tape = now ? tapeFor(stackOf(now)) : undefined;
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", gap: 8, padding: "8px 0 8px", background: `color-mix(in oklab, ${colour} 7%, var(--sheet))`, borderBottom: "1px solid #000", minWidth: 0 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "0 6px 0 10px", minWidth: 0 }}>
        <Step dir={-1} disabled={s.songIndex === 0} onClick={() => goToSong(s.songIndex - 1)} />
        <span className="num" style={{ width: 26, height: 26, borderRadius: 6, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 13, fontWeight: 750, background: colour, color: "#0b0b0e" }}>
          {s.songIndex + 1}
        </span>
        <span style={{ fontSize: 18, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{song.name}</span>
        <span className="t-meta num" style={{ fontSize: 13, whiteSpace: "nowrap" }}>
          {[song.key, song.bpm ? `${song.bpm} bpm` : ""].filter(Boolean).join(" · ")}
        </span>
        {now && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 6, padding: "2px 8px", borderRadius: 4, background: "rgba(255,255,255,0.07)", fontSize: 13, fontWeight: 650, whiteSpace: "nowrap" }}>
            <span style={{ width: 7, height: 7, borderRadius: 2, background: tape === "var(--tape-gaffer)" ? "var(--ink-3)" : tape }} />
            {now}
          </span>
        )}
        <span style={{ flex: 1 }} />
        {next && (
          <span className="t-meta" style={{ fontSize: 12.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 180 }}>
            Next <b style={{ color: "var(--ink-2)", fontWeight: 650 }}>{next.name}</b>
          </span>
        )}
        <Step dir={1} disabled={!next} onClick={() => goToSong(s.songIndex + 1)} />
      </div>
      {sections.length > 0 && (
        <div style={{ display: "flex", gap: 4, padding: "0 10px", overflowX: "auto", scrollbarWidth: "none" }}>
          {sections.map((sec, j) => {
            const state = j < s.partIndex ? "done" : j === s.partIndex ? "now" : "ahead";
            const c = sectionColour(sec.name);
            const part = state === "now" && sec.parts.length > 1 ? sec.parts[s.subIndex]?.name : null;
            return (
              <button
                key={`${j}-${sec.name}`}
                className={state === "now" ? "" : "pressable"}
                onClick={() => goToPart(j)}
                style={{
                  flexShrink: 0,
                  height: 34,
                  padding: "0 12px",
                  borderRadius: 6,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 13,
                  fontWeight: state === "now" ? 750 : 600,
                  whiteSpace: "nowrap",
                  color: state === "now" ? "#0b0b0e" : state === "done" ? "var(--ink-3)" : "var(--ink)",
                  background: state === "now" ? c : `color-mix(in oklab, ${c} ${state === "done" ? 8 : 16}%, transparent)`,
                  boxShadow: state === "now" ? undefined : `inset 0 -2px 0 color-mix(in oklab, ${c} ${state === "done" ? 30 : 70}%, transparent)`,
                }}
              >
                {sec.name}
                {part && <span style={{ fontWeight: 600, opacity: 0.75 }}>· {part}</span>}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Step({ dir, disabled, onClick }: { dir: 1 | -1; disabled: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      aria-label={dir < 0 ? "Previous song" : "Next song"}
      className={disabled ? "" : "pressable"}
      style={{ width: 32, height: 32, flexShrink: 0, borderRadius: "var(--r)", display: "flex", alignItems: "center", justifyContent: "center", color: disabled ? "var(--dim)" : "var(--ink-2)" }}
    >
      <svg width="8" height="12" viewBox="0 0 8 12" aria-hidden style={{ transform: dir < 0 ? "scaleX(-1)" : undefined }}>
        <path d="M1.5 1.5 6 6l-4.5 4.5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
}
