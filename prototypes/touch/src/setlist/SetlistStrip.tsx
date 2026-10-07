// Where you are in the set, for the landscape phone's stage: the room above
// the macros and switches. Read at a glance from a mic stand — not the
// setlist (no parts list, no reordering, no editing):
//
//   NOW    the song (its number in its colour, key, tempo) and, large, the
//          section playing in its section colour — its part when it has
//          several — with the patch it plays
//   NEXT   what the next step lands on — the next part, the next section,
//          or the next song once this one ends — and the patch it brings;
//          a tap goes there, ‹ goes back
//   MAP    the song's sections as one bar across the width, each in its
//          colour and sized alike, played ones dimmed, the one playing lit;
//          a tap on one goes there

import { currentSet, goToPart, goToSong, goToSub, playing, sectionsOf, useStore, type Section } from "../store";
import { sectionColour, songColour } from "./colors";
import { tapeFor } from "../ui/marks";
import { stackOf } from "../data/rig";

type Step = { label: string; detail: string; patch: string | null; colour: string; go: () => void } | null;

export function SetlistStrip(_: { fill?: boolean } = {}) {
  const s = useStore();
  const set = currentSet(s);
  const song = set.songs[s.songIndex];
  if (!song) return null;
  const colour = songColour(song.name, s.songColours);
  const sections = sectionsOf(s, song.name);
  const sec = sections[s.partIndex];
  const now = playing(s);
  const several = !!sec && sec.parts.length > 1;
  const part = several ? sec.parts[s.subIndex] : null;

  const next = nextStep(s.songIndex, s.partIndex, s.subIndex, sections, set.songs, (name) => sectionsOf(s, name), (name) => songColour(name, s.songColours));
  const back = () => {
    if (several && s.subIndex > 0) goToSub(s.partIndex, s.subIndex - 1);
    else if (s.partIndex > 0) goToPart(s.partIndex - 1);
    else if (s.songIndex > 0) goToSong(s.songIndex - 1);
  };
  const atStart = s.songIndex === 0 && s.partIndex === 0 && (!several || s.subIndex === 0);

  return (
    <div style={{ flex: 1, minHeight: 0, minWidth: 0, display: "flex", flexDirection: "column", background: `color-mix(in oklab, ${colour} 6%, var(--sheet))` }}>
      <div style={{ flex: 1, minHeight: 0, display: "flex", alignItems: "stretch" }}>
        {/* Back a step. */}
        <button
          onClick={back}
          disabled={atStart}
          aria-label="Back a step"
          className={atStart ? "" : "pressable"}
          style={{ width: 44, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", color: atStart ? "var(--dim)" : "var(--ink-2)" }}
        >
          <Chevron dir={-1} />
        </button>

        {/* NOW */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", justifyContent: "center", gap: 4, padding: "6px 12px 6px 4px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
            <span className="num" style={{ width: 22, height: 22, borderRadius: 5, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 12, fontWeight: 750, background: colour, color: "#0b0b0e" }}>
              {s.songIndex + 1}
            </span>
            <span style={{ fontSize: 15, fontWeight: 700, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{song.name}</span>
            <span className="t-meta num" style={{ fontSize: 12.5, whiteSpace: "nowrap" }}>
              {[song.key, song.bpm ? `${song.bpm}` : ""].filter(Boolean).join(" · ")}
            </span>
            <span className="t-meta num" style={{ fontSize: 12, marginLeft: "auto", whiteSpace: "nowrap" }}>
              {s.songIndex + 1}/{set.songs.length}
            </span>
          </div>
          <div style={{ display: "flex", alignItems: "baseline", gap: 10, minWidth: 0 }}>
            <span className="t-marker" style={{ fontSize: 30, color: sec ? sectionColour(sec.name) : "var(--ink)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {sec?.name ?? song.name}
            </span>
            {part && <span style={{ fontSize: 17, fontWeight: 650, color: "var(--ink-2)", whiteSpace: "nowrap" }}>{part.name}</span>}
          </div>
          {now && <Patch name={now} lit />}
        </div>

        {/* NEXT — a tap goes there. */}
        <button
          onClick={next?.go}
          disabled={!next}
          className={next ? "pressable" : ""}
          aria-label={next ? `Next: ${next.label}` : "End of the set"}
          style={{
            width: 220,
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "6px 6px 6px 14px",
            textAlign: "left",
            borderLeft: "1px solid var(--rule)",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="t-label" style={{ fontSize: 11, color: "var(--ink-3)" }}>
              {next ? next.detail : "End of the set"}
            </span>
            {next && (
              <>
                <span style={{ fontSize: 19, fontWeight: 750, color: next.colour, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{next.label}</span>
                {next.patch ? <Patch name={next.patch} /> : <span className="t-meta" style={{ fontSize: 12 }}>keeps the patch</span>}
              </>
            )}
          </span>
          {next && <Chevron dir={1} />}
        </button>
      </div>

      {/* MAP — the song's sections across the width. */}
      {sections.length > 0 && (
        <div style={{ display: "flex", gap: 2, height: 44, padding: "0 6px" }}>
          {sections.map((x, j) => {
            const state = j < s.partIndex ? "done" : j === s.partIndex ? "now" : "ahead";
            const c = sectionColour(x.name);
            return (
              <button
                key={`${j}-${x.name}`}
                onClick={() => goToPart(j)}
                title={x.name}
                aria-current={state === "now" ? "step" : undefined}
                style={{
                  flex: "1 1 0",
                  minWidth: 0,
                  display: "flex",
                  alignItems: "center",
                  padding: "6px 0 10px",
                  background: "transparent",
                }}
              >
                <span
                  style={{
                  flex: 1,
                  minWidth: 0,
                  height: "100%",
                  display: "flex",
                  alignItems: "center",
                  borderRadius: 4,
                  padding: "0 6px",
                  fontSize: 11,
                  fontWeight: state === "now" ? 800 : 650,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  color: state === "now" ? "#0b0b0e" : state === "done" ? "var(--ink-3)" : "var(--ink-2)",
                  background: state === "now" ? c : `color-mix(in oklab, ${c} ${state === "done" ? 12 : 26}%, #0b0b0e)`,
                  boxShadow: state === "now" ? "0 0 0 1.5px rgba(255,255,255,0.85)" : undefined,
                  }}
                >
                  {x.name}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** What the next step lands on: the next part, section, or song. */
function nextStep(
  songIndex: number,
  partIndex: number,
  subIndex: number,
  sections: Section[],
  songs: { name: string }[],
  sectionsFor: (name: string) => Section[],
  colourOf: (name: string) => string,
): Step {
  const sec = sections[partIndex];
  const soundOf = (x: Section | undefined, k = 0) => x?.parts[k]?.sound?.name ?? null;
  if (sec && subIndex + 1 < sec.parts.length) {
    const p = sec.parts[subIndex + 1];
    return { label: p.name, detail: `Next part · ${sec.name}`, patch: p.sound?.name ?? null, colour: sectionColour(sec.name), go: () => goToSub(partIndex, subIndex + 1) };
  }
  if (partIndex + 1 < sections.length) {
    const x = sections[partIndex + 1];
    return { label: x.name, detail: "Next section", patch: soundOf(x), colour: sectionColour(x.name), go: () => goToPart(partIndex + 1) };
  }
  const n = songs[songIndex + 1];
  if (!n) return null;
  const first = sectionsFor(n.name)[0];
  return { label: n.name, detail: `Next song · ${songIndex + 2}`, patch: soundOf(first), colour: colourOf(n.name), go: () => goToSong(songIndex + 1) };
}

function Patch({ name, lit }: { name: string; lit?: boolean }) {
  const t = tapeFor(stackOf(name));
  return (
    <span style={{ alignSelf: "flex-start", maxWidth: "100%", display: "inline-flex", alignItems: "center", gap: 6, padding: "2px 8px", borderRadius: 4, background: lit ? "rgba(255,255,255,0.08)" : "rgba(255,255,255,0.05)", fontSize: 12.5, fontWeight: 650, color: lit ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden" }}>
      <span style={{ width: 7, height: 7, borderRadius: 2, flexShrink: 0, background: t === "var(--tape-gaffer)" ? "var(--ink-3)" : t }} />
      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{name}</span>
    </span>
  );
}

function Chevron({ dir }: { dir: 1 | -1 }) {
  return (
    <svg width="10" height="16" viewBox="0 0 10 16" aria-hidden style={{ flexShrink: 0, transform: dir < 0 ? "scaleX(-1)" : undefined, color: "var(--ink-3)" }}>
      <path d="M2 2l6 6-6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
