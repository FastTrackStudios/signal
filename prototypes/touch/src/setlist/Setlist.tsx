// The setlist component: a set, its songs in order, each song's sections and
// the patch each section plays. One component for the sidebar (a phone's
// width), the narrow sidebar and a whole tablet screen.
//
// - The set: its name, ‹ › to step sets, ⋯ to manage it.
// - A song: number, name, key, tempo, the patch it starts on; tap to open
//   its sections (the song up is open), ⋯ or a long-press for the rest.
// - A section: where the song is (played, now, ahead), its name and the
//   patch it plays — tap the patch to change it; ⋯ to rename, move, delete.
// Every change goes through the store, so the bar-less Undo undoes it.

import { createContext, useContext, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { rig, stackOf } from "../data/rig";
import {
  addSection,
  addSong,
  chooseSet,
  currentSet,
  deleteSet,
  duplicateSet,
  goToPart,
  goToSong,
  moveSection,
  moveSong,
  newSetlist,
  newSong,
  removeSection,
  removeSong,
  renameSection,
  setDetails,
  sectionsOf,
  setSectionSound,
  setSongField,
  setSongColour,
  setStart,
  undo,
  useStore,
  useUndo,
} from "../store";
import { Strike, Tape, tapeFor } from "../ui/marks";
import { Button, KeyBox, Tabs } from "../ui/kit";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import { nameColour, sectionColour, songColour, SONG_PALETTE } from "./colors";
import { addDays, dateLabel, isoOf, MONTHS, nextDateFor, setHeading, setName, WEEKDAYS, whenLabel, type SetMeta } from "./sets";

const KEYS = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/** How much room the component has, measured: the narrow sidebar folds
 *  each row down to what fits; the sidebar and a tablet show it all. */
type Fit = "narrow" | "sidebar" | "wide";
const FitCtx = createContext<Fit>("sidebar");
const useFit = () => useContext(FitCtx);

type Panel =
  | null
  | { kind: "sets" }
  | { kind: "add" }
  | { kind: "key"; song: number }
  | { kind: "tempo"; song: number }
  | { kind: "start"; song: number }
  | { kind: "colour"; song: number }
  | { kind: "details"; mode: "new" | "edit" | "duplicate" }
  | { kind: "patch"; song: string; section: number };

export function Setlist() {
  const s = useStore();
  const set = currentSet(s);
  const [panel, setPanel] = useState<Panel>(null);
  // Songs opened to show their sections, beside the one up (always open).
  const [open, setOpen] = useState<Set<number>>(new Set());
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  // Reorder mode: the rows grow handles, everything else waits.
  const [reordering, setReordering] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const rows = useRef<(HTMLDivElement | null)[]>([]);
  const rootRef = useRef<HTMLElement>(null);
  const [fit, setFit] = useState<Fit>("sidebar");
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const measure = () => setFit(el.clientWidth < 300 ? "narrow" : el.clientWidth > 700 ? "wide" : "sidebar");
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    measure();
    return () => ro.disconnect();
  }, []);

  const toggle = (i: number) => {
    const next = new Set(open);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    setOpen(next);
  };

  const onHandleDown = (i: number) => (e: React.PointerEvent) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const at = (y: number) => {
      let to = 0;
      rows.current.forEach((el, j) => {
        if (el && y > el.getBoundingClientRect().top + 24) to = j;
      });
      return to;
    };
    setDrag({ from: i, to: i });
    const onMove = (ev: PointerEvent) => setDrag({ from: i, to: at(ev.clientY) });
    const onUp = (ev: PointerEvent) => {
      const to = at(ev.clientY);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setDrag(null);
      if (to !== i) moveSong(i, to);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <FitCtx.Provider value={fit}>
    <section
      ref={rootRef}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        background: "var(--sheet)",
        color: "var(--ink)",
        overflow: "hidden",
      }}
    >
      <SetHeader onPanel={setPanel} reordering={reordering} onReorder={setReordering} />
      <div ref={listRef} style={{ flex: 1, minHeight: 0, overflowY: "auto", overscrollBehavior: "contain" }}>
        {set.songs.map((song, i) => (
          <div key={`${i}-${song.name}`} ref={(el) => { rows.current[i] = el; }}>
            <SongRow
              index={i}
              reordering={reordering}
              open={!reordering && (i === s.songIndex || open.has(i))}
              onToggle={() => toggle(i)}
              onPanel={setPanel}
              onHandleDown={onHandleDown(i)}
              dropAbove={!!drag && drag.to === i && drag.from !== i}
              lifted={drag?.from === i}
            />
            {!reordering && (i === s.songIndex || open.has(i)) && <Sections songIndex={i} onPanel={setPanel} />}
          </div>
        ))}
        {set.songs.length === 0 && (
          <div style={{ padding: "28px 20px", display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 10 }}>
            <div style={{ fontSize: 18, fontWeight: 700 }}>No songs yet</div>
            <p className="t-meta" style={{ margin: 0, lineHeight: 1.45 }}>
              Add the songs you're playing, in order. Each keeps its own key, tempo and the patch every section plays.
            </p>
            <Button primary onClick={() => setPanel({ kind: "add" })}>
              Add songs
            </Button>
          </div>
        )}
      </div>
      <Footer onPanel={setPanel} />
      {panel && <PanelView panel={panel} onClose={() => setPanel(null)} />}
    </section>
    </FitCtx.Provider>
  );
}

// ── The set's heading ────────────────────────────────────────────────

function SetHeader({ onPanel, reordering, onReorder }: { onPanel: (p: Panel) => void; reordering: boolean; onReorder: (on: boolean) => void }) {
  const s = useStore();
  const set = currentSet(s);
  const menu = useMenu();
  const fit = useFit();
  const items: MenuItem[] = [
    { kind: "head", label: set.name },
    { kind: "run", id: "edit", label: "Edit details…", detail: "event · date · title" },
    { kind: "run", id: "new", label: "New set…" },
    { kind: "run", id: "duplicate", label: "Duplicate for next week" },
    { kind: "run", id: "sets", label: "All sets…", detail: `${s.setlists.length}` },
    { kind: "run", id: "reorder", label: "Reorder songs", disabled: set.songs.length < 2 ? "Fewer than two songs" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete set", disabled: s.setlists.length <= 1 ? "The only set — make another first" : undefined },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "edit") onPanel({ kind: "details", mode: "edit" });
    if (p.id === "new") onPanel({ kind: "details", mode: "new" });
    if (p.id === "duplicate") onPanel({ kind: "details", mode: "duplicate" });
    if (p.id === "sets") onPanel({ kind: "sets" });
    if (p.id === "reorder") onReorder(true);
    if (p.id === "delete") deleteSet(s.setIndex);
  };
  const minutes = Math.round(set.songs.length * 4.5);
  const when = whenLabel(set.date);
  return (
    <header style={{ flexShrink: 0, padding: "14px 6px 12px 18px", borderBottom: "1px solid var(--rule)", display: "flex", alignItems: "flex-start", gap: 2 }}>
      <button
        className="pressable"
        onClick={() => onPanel({ kind: "sets" })}
        style={{ flex: 1, minWidth: 0, textAlign: "left", padding: "2px 8px 4px", margin: "-2px 0 0 -8px", borderRadius: "var(--r-md)" }}
        title="Sets — choose another or make a new one"
        aria-haspopup="dialog"
      >
        <h1 className="t-marker" style={{ margin: 0, fontSize: fit === "narrow" ? 18 : 24, lineHeight: 1.12, display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ minWidth: 0 }}>{setHeading(set)}</span>
          <span aria-hidden style={{ flexShrink: 0, display: "flex", width: 22, height: 22, borderRadius: 999, alignItems: "center", justifyContent: "center", background: "rgba(255,255,255,0.07)", color: "var(--ink-2)" }}>
            <svg width="10" height="6" viewBox="0 0 10 6">
              <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </span>
        </h1>
        <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: "4px 8px", marginTop: 6 }}>
          {set.title && set.event && <EventChip event={set.event} />}
          <span style={{ fontSize: 14, fontWeight: 600, color: "var(--ink)" }}>{dateLabel(set.date)}</span>
          {when && <span style={{ fontSize: 13, fontWeight: 600, color: when === "Today" ? "var(--live)" : "var(--ink-3)" }}>{when}</span>}
        </div>
        {fit !== "narrow" && (
          <div className="t-meta" style={{ marginTop: 3, fontSize: 13 }}>
            {set.songs.length} songs · about {minutes} min
          </div>
        )}
      </button>
      {reordering ? (
        <Button primary onClick={() => onReorder(false)} style={{ alignSelf: "center", marginRight: 6 }}>
          Done
        </Button>
      ) : null}
      {!reordering && <MoreButton label="Set actions" onClick={menu.fromButton} />}
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </header>
  );
}

/** A recurring event, as a chip in its colour (its name's, like a song). */
function EventChip({ event, big }: { event: string; big?: boolean }) {
  const c = nameColour(event);
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: big ? "5px 10px" : "2px 8px",
        borderRadius: 999,
        background: `color-mix(in srgb, ${c} 18%, transparent)`,
        color: "var(--ink)",
        fontSize: big ? 14 : 12,
        fontWeight: 650,
        whiteSpace: "nowrap",
      }}
    >
      <span style={{ width: 7, height: 7, borderRadius: 999, background: c }} />
      {event}
    </span>
  );
}

// ── A song ───────────────────────────────────────────────────────────

function SongRow({
  index: i,
  reordering,
  open,
  onToggle,
  onPanel,
  onHandleDown,
  dropAbove,
  lifted,
}: {
  index: number;
  reordering: boolean;
  open: boolean;
  onToggle: () => void;
  onPanel: (p: Panel) => void;
  onHandleDown: (e: React.PointerEvent) => void;
  dropAbove: boolean;
  lifted: boolean;
}) {
  const s = useStore();
  const set = currentSet(s);
  const song = set.songs[i];
  const menu = useMenu();
  const up = i === s.songIndex;
  const played = i < s.songIndex;
  const next = i === s.songIndex + 1;
  const last = set.songs.length - 1;
  const sections = sectionsOf(s, song.name);
  const fit = useFit();
  const narrow = fit === "narrow";
  const colour = songColour(song.name, s.songColours);
  const items: MenuItem[] = [
    { kind: "head", label: `${i + 1} · ${song.name}` },
    { kind: "run", id: "go", label: "Play from here", disabled: up ? "It's up now" : undefined },
    { kind: "sep" },
    { kind: "run", id: "key", label: "Key…", detail: song.key || "—" },
    { kind: "run", id: "tempo", label: "Tempo…", detail: song.bpm ? `${song.bpm} BPM` : "—" },
    { kind: "run", id: "start", label: "Starts on…", detail: song.start || "default" },
    { kind: "run", id: "colour", label: "Colour…", detail: s.songColours[song.name] ? "set" : "from its name" },
    { kind: "sep" },
    { kind: "run", id: "up", label: "Move up", disabled: i === 0 ? "Already first" : undefined },
    { kind: "run", id: "down", label: "Move down", disabled: i === last ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "remove", label: "Remove from this set" },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "go") goToSong(i);
    if (p.id === "key") onPanel({ kind: "key", song: i });
    if (p.id === "tempo") onPanel({ kind: "tempo", song: i });
    if (p.id === "start") onPanel({ kind: "start", song: i });
    if (p.id === "colour") onPanel({ kind: "colour", song: i });
    if (p.id === "up") moveSong(i, i - 1);
    if (p.id === "down") moveSong(i, i + 1);
    if (p.id === "remove") removeSong(i);
  };
  return (
    <div
      {...(reordering ? {} : menu.longPress())}
      style={{
        position: "relative",
        display: "flex",
        alignItems: "stretch",
        minHeight: 64,
        background: lifted ? "var(--sheet-2)" : open ? `color-mix(in srgb, ${colour} ${up ? 13 : 8}%, var(--sheet))` : undefined,
        borderTop: dropAbove ? "3px solid var(--focus-fg)" : "1px solid var(--rule)",
      }}
    >
      {up && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: "var(--live)" }} />}
      <button
        className="pressable"
        onClick={() => !reordering && onToggle()}
        aria-expanded={open}
        style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 10, padding: "10px 2px 10px 14px", textAlign: "left" }}
      >
        {/* The song's number on its colour: the song, at a glance. */}
        <span
          className="num"
          style={{
            width: 28,
            height: 28,
            borderRadius: 7,
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 14,
            fontWeight: 750,
            background: played ? `color-mix(in srgb, ${colour} 35%, var(--sheet))` : colour,
            color: "#0b0b0e",
          }}
        >
          {i + 1}
        </span>
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
            <span
              style={{
                position: "relative",
                minWidth: 0,
                fontSize: 17,
                fontWeight: up ? 750 : 600,
                color: played ? "var(--ink-3)" : "var(--ink)",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {song.name}
              {played && <Strike width={1.6} />}
            </span>
            {!narrow && up && <Badge tone="live">Now</Badge>}
            {!narrow && next && <Badge>Next</Badge>}
          </span>
          {narrow ? (
            <span className="t-meta num" style={{ fontSize: 13 }}>
              {[song.key, song.bpm ? `${song.bpm}` : ""].filter(Boolean).join(" · ")}
            </span>
          ) : (
            <span className="t-meta" style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, minWidth: 0, whiteSpace: "nowrap", overflow: "hidden" }}>
              {song.start && <PatchChip name={song.start} small />}
              {!open && sections.length > 0 && <span style={{ flexShrink: 0 }}>{song.start ? "· " : ""}{sections.length} sections</span>}
            </span>
          )}
        </span>
        {!narrow && <KeyBox k={song.key} />}
        {!narrow && (
          <span className="num" style={{ width: 30, textAlign: "right", fontSize: 14, fontWeight: 600, color: "var(--ink-2)", flexShrink: 0 }}>
            {song.bpm || ""}
          </span>
        )}

      </button>
      <div style={{ display: "flex", alignItems: "center" }}>
        {!reordering && <MoreButton label={`${song.name} actions`} onClick={menu.fromButton} />}
        {reordering && <span
          onPointerDown={onHandleDown}
          role="button"
          aria-label={`Drag ${song.name}`}
          style={{ width: 52, height: 52, display: "flex", alignItems: "center", justifyContent: "center", cursor: "grab", touchAction: "none", color: "var(--ink-2)" }}
        >
          <svg width="14" height="10" viewBox="0 0 14 10" aria-hidden>
            <path d="M1 1h12M1 5h12M1 9h12" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
          </svg>
        </span>}
      </div>
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}

// ── Its sections ─────────────────────────────────────────────────────

function Sections({ songIndex, onPanel }: { songIndex: number; onPanel: (p: Panel) => void }) {
  const s = useStore();
  const song = currentSet(s).songs[songIndex];
  const sections = sectionsOf(s, song.name);
  const up = songIndex === s.songIndex;
  const add = useMenu();
  const fit = useFit();
  const colour = songColour(song.name, s.songColours);
  return (
    <div style={{ position: "relative", padding: "2px 0 10px", background: `color-mix(in srgb, ${colour} ${up ? 13 : 8}%, var(--sheet))` }}>
      {up && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: "var(--live)" }} />}
      {/* The song's timeline: one line its sections sit on. */}
      {sections.length > 0 && (
        <span aria-hidden style={{ position: "absolute", left: fit === "narrow" ? 21 : 35, top: 18, bottom: 40, width: 1, background: "var(--rule-strong)" }} />
      )}
      {sections.map((sec, j) => (
        <SectionRow key={`${j}-${sec.name}`} song={song.name} songIndex={songIndex} index={j} count={sections.length} onPanel={onPanel} />
      ))}
      <button
        className="pressable"
        onClick={add.fromButton}
        style={{ display: "flex", alignItems: "center", gap: 10, width: "100%", minHeight: 44, padding: "0 14px 0 28px", color: "var(--ink-2)", fontSize: 14, fontWeight: 600 }}
      >
        <span style={{ width: 16, height: 16, borderRadius: 999, border: "1px dashed var(--ink-3)", display: "flex", alignItems: "center", justifyContent: "center" }}>
          <svg width="8" height="8" viewBox="0 0 8 8" aria-hidden>
            <path d="M4 0.5v7M0.5 4h7" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </span>
        Add a section
      </button>
      {add.open && (
        <Menu
          at={add.open.at}
          naming={0}
          items={[{ kind: "name", id: "add", label: "New section…", initial: suggestSection(sections.map((x) => x.name)), confirm: "Add", taken: sections.map((x) => x.name) }]}
          onPick={(p) => addSection(song.name, p.text)}
          onClose={add.close}
        />
      )}
    </div>
  );
}

function SectionRow({ song, songIndex, index: j, count, onPanel }: { song: string; songIndex: number; index: number; count: number; onPanel: (p: Panel) => void }) {
  const s = useStore();
  const sec = sectionsOf(s, song)[j];
  const menu = useMenu();
  const up = songIndex === s.songIndex;
  const state: "done" | "now" | "ahead" = !up ? (songIndex < s.songIndex ? "done" : "ahead") : j < s.partIndex ? "done" : j === s.partIndex ? "now" : "ahead";
  const names = sectionsOf(s, song).map((x) => x.name);
  const narrow = useFit() === "narrow";
  const items: MenuItem[] = [
    { kind: "head", label: sec.name },
    { kind: "run", id: "go", label: "Play from here", disabled: state === "now" ? "It's playing" : undefined },
    { kind: "run", id: "patch", label: "Patch…", detail: sec.sound?.name ?? "keeps" },
    { kind: "name", id: "rename", label: "Rename…", initial: sec.name, confirm: "Rename", taken: names },
    { kind: "sep" },
    { kind: "run", id: "earlier", label: "Move earlier", disabled: j === 0 ? "Already first" : undefined },
    { kind: "run", id: "later", label: "Move later", disabled: j === count - 1 ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete section" },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "go") {
      if (!up) goToSong(songIndex);
      goToPart(j);
    }
    if (p.id === "patch") onPanel({ kind: "patch", song, section: j });
    if (p.id === "rename") renameSection(song, j, p.text);
    if (p.id === "earlier") moveSection(song, j, j - 1);
    if (p.id === "later") moveSection(song, j, j + 1);
    if (p.id === "delete") removeSection(song, j);
  };
  return (
    <div {...menu.longPress()} style={{ position: "relative", display: "flex", alignItems: "center", minHeight: 50, background: state === "now" ? "var(--live-bg)" : undefined }}>
      <button
        className="pressable"
        onClick={() => {
          if (!up) goToSong(songIndex);
          goToPart(j);
        }}
        style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: narrow ? 8 : 12, minHeight: 50, padding: narrow ? "4px 2px 4px 14px" : "4px 4px 4px 28px", textAlign: "left" }}
      >
        <Node state={state} colour={sectionColour(sec.name)} />
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
        <span
          style={{
            minWidth: 0,
            fontSize: 15,
            fontWeight: state === "now" ? 700 : 520,
            color: state === "done" ? "var(--ink-3)" : state === "now" ? "var(--ink)" : "var(--ink-2)",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {sec.name}
        </span>
        {narrow && (sec.sound ? <span style={{ alignSelf: "flex-start", maxWidth: "100%", display: "flex" }}><PatchChip name={sec.sound.name} lit={state === "now"} small /></span> : <span className="t-meta" style={{ fontSize: 12 }}>keeps</span>)}
        </span>
      </button>
      {!narrow && <button
        className="pressable"
        onClick={() => onPanel({ kind: "patch", song, section: j })}
        title="Change the patch"
        style={{ width: 178, flexShrink: 0, minHeight: 44, padding: "0 10px 0 4px", marginRight: 6, display: "flex", alignItems: "center", justifyContent: "flex-start", borderRadius: "var(--r)" }}
      >
        {sec.sound ? <PatchChip name={sec.sound.name} lit={state === "now"} /> : <span className="t-meta" style={{ fontSize: 13, padding: "0 4px" }}>keeps</span>}
      </button>}
      {narrow && <MoreButton label={`${sec.name} actions`} onClick={menu.fromButton} />}
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}

/** Where a section sits in the song: played (a check), now (green), ahead. */
function Node({ state, colour }: { state: "done" | "now" | "ahead"; colour: string }) {
  const size = state === "now" ? 14 : 12;
  return (
    <span style={{ position: "relative", zIndex: 1, width: 16, display: "flex", justifyContent: "center", flexShrink: 0 }}>
      <span
        style={{
          width: size,
          height: size,
          borderRadius: 999,
          // The section's type colour: filled while it plays, a ring ahead,
          // faded once played.
          background: state === "now" ? colour : "var(--sheet)",
          border: `2px solid ${state === "done" ? `color-mix(in srgb, ${colour} 40%, var(--sheet))` : colour}`,
          boxShadow: state === "now" ? `0 0 0 4px color-mix(in srgb, ${colour} 30%, transparent)` : undefined,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {state === "done" && (
          <svg width="7" height="6" viewBox="0 0 7 6" aria-hidden>
            <path d="M1 3 L3 5 L6 1" fill="none" stroke={`color-mix(in srgb, ${colour} 60%, var(--ink-3))`} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </span>
    </span>
  );
}

/** A patch: its name on a tint of its stack's colour, with a swatch. */
function PatchChip({ name, lit, small }: { name: string; lit?: boolean; small?: boolean }) {
  const colour = tapeFor(stackOf(name));
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        minWidth: 0,
        maxWidth: "100%",
        padding: small ? "1px 7px 1px 6px" : "4px 9px 4px 8px",
        borderRadius: 4,
        background: `color-mix(in srgb, ${colour} ${lit ? 30 : 18}%, transparent)`,
        color: lit ? "var(--ink)" : "var(--ink-2)",
        fontSize: small ? 12 : 13,
        fontWeight: 600,
        whiteSpace: "nowrap",
      }}
    >
      <span style={{ width: 7, height: 7, borderRadius: 2, background: colour, flexShrink: 0 }} />
      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{name}</span>
    </span>
  );
}

function Badge({ children, tone }: { children: ReactNode; tone?: "live" }) {
  return (
    <span
      className="t-label"
      style={{
        flexShrink: 0,
        fontSize: 10,
        letterSpacing: "0.1em",
        padding: "2px 6px",
        borderRadius: 4,
        background: tone === "live" ? "var(--live-bg)" : "rgba(255,255,255,0.07)",
        color: tone === "live" ? "var(--live)" : "var(--ink-2)",
      }}
    >
      {children}
    </span>
  );
}

// ── The foot ─────────────────────────────────────────────────────────

function Footer({ onPanel }: { onPanel: (p: Panel) => void }) {
  const { depth, label } = useUndo();
  return (
    <footer style={{ flexShrink: 0, display: "flex", gap: 8, padding: 10, borderTop: "1px solid var(--rule)" }}>
      <Button primary onClick={() => onPanel({ kind: "add" })} style={{ flex: 1 }}>
        Add songs
      </Button>
      <Button onClick={undo} disabled={depth === 0} title={label ? `Undo ${label}` : "Nothing to undo"}>
        Undo
      </Button>
    </footer>
  );
}

// ── Panels: the pickers, sliding over the list, inside the component ─

function PanelView({ panel: initial, onClose }: { panel: NonNullable<Panel>; onClose: () => void }) {
  const s = useStore();
  const set = currentSet(s);
  // The sets list can hand on to "New set" without closing.
  const [panel, setPanel] = useState<NonNullable<Panel>>(initial);
  const setPanelMode = (mode: "new") => setPanel({ kind: "details", mode });
  let title = "";
  let sub = "";
  let body: ReactNode = null;
  switch (panel.kind) {
    case "sets": {
      title = "Sets";
      sub = `${s.setlists.length} sets`;
      body = <SetsBody onClose={onClose} onNew={() => setPanelMode("new")} />;
      break;
    }
    case "details": {
      title = panel.mode === "new" ? "New set" : panel.mode === "duplicate" ? "Duplicate set" : "Set details";
      sub = panel.mode === "duplicate" ? `${set.songs.length} songs from ${setHeading(set)}` : panel.mode === "new" ? "An event on a date — a title only if the night has one" : set.name;
      body = <DetailsBody mode={panel.mode} onDone={onClose} />;
      break;
    }
    case "add": {
      title = "Add songs";
      sub = `To ${set.name}`;
      body = <AddBody />;
      break;
    }
    case "key": {
      const song = set.songs[panel.song];
      title = song.name;
      sub = "Key in this set";
      body = (
        <div style={{ padding: 16 }}>
          <KeyGrid value={song.key || "C"} onPick={(k) => setSongField(panel.song, "key", k)} />
        </div>
      );
      break;
    }
    case "tempo": {
      const song = set.songs[panel.song];
      title = song.name;
      sub = "Tempo in this set";
      body = (
        <div style={{ padding: 20, display: "flex", flexDirection: "column", alignItems: "center", gap: 16 }}>
          <div className="num" style={{ fontSize: 56, fontWeight: 750, lineHeight: 1 }}>
            {song.bpm || "—"}
            <span className="t-meta" style={{ fontSize: 15, marginLeft: 6 }}>
              BPM
            </span>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 8, width: "100%" }}>
            {[-5, -1, 1, 5].map((d) => (
              <Button key={d} onClick={() => setSongField(panel.song, "bpm", Math.max(30, (song.bpm || 100) + d))} style={{ minHeight: 56, fontSize: 18, padding: 0 }}>
                {d > 0 ? `+${d}` : `−${-d}`}
              </Button>
            ))}
          </div>
        </div>
      );
      break;
    }
    case "start": {
      const song = set.songs[panel.song];
      title = song.name;
      sub = "The patch it starts on";
      body = <PatchList song={song.name} current={song.start || null} defaultLabel="The profile's default" onPick={(p) => setStart(panel.song, p ?? "")} />;
      break;
    }
    case "colour": {
      const song = set.songs[panel.song];
      const chosen = s.songColours[song.name];
      const auto = nameColour(song.name);
      title = song.name;
      sub = "Its colour — every set and device shows it";
      body = (
        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
          <button
            className="pressable"
            onClick={() => setSongColour(song.name, null)}
            style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 56, padding: "0 14px", borderRadius: "var(--r)", border: !chosen ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)", background: !chosen ? "var(--focus-bg)" : "var(--sheet)", textAlign: "left" }}
          >
            <span style={{ width: 28, height: 28, borderRadius: 7, background: auto, flexShrink: 0 }} />
            <span style={{ flex: 1 }}>
              <span style={{ display: "block", fontSize: 15, fontWeight: 650 }}>From its name</span>
              <span className="t-meta" style={{ fontSize: 13 }}>Always the same for “{song.name}”</span>
            </span>
          </button>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 8 }}>
            {SONG_PALETTE.map((c) => {
              const on = chosen === c;
              return (
                <button
                  key={c}
                  aria-label={`Colour ${c}`}
                  onClick={() => setSongColour(song.name, c)}
                  style={{ aspectRatio: "1", minHeight: 44, borderRadius: 10, background: c, boxShadow: on ? "0 0 0 2px var(--sheet-2), 0 0 0 4px var(--focus-fg)" : undefined }}
                />
              );
            })}
          </div>
        </div>
      );
      break;
    }
    case "patch": {
      const sec = sectionsOf(s, panel.song)[panel.section];
      title = `${panel.song} · ${sec?.name ?? ""}`;
      sub = "The patch this section plays";
      body = (
        <PatchList
          song={panel.song}
          current={sec?.sound?.name ?? null}
          defaultLabel="Keep what plays"
          onPick={(p) => setSectionSound(panel.song, panel.section, p ? { kind: "patch", name: p } : null)}
        />
      );
      break;
    }
  }
  // The sets switcher drops from the title it opened from; the pickers
  // rise from the bottom, under the thumb.
  const top = panel.kind === "sets";
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 20, display: "flex", flexDirection: "column", justifyContent: top ? "flex-start" : "flex-end" }}>
      <button aria-label="Close" onClick={onClose} style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.5)", cursor: "default" }} />
      <div
        style={{
          position: "relative",
          maxHeight: top ? "92%" : "86%",
          display: "flex",
          flexDirection: "column",
          background: "var(--sheet-2)",
          borderTop: top ? undefined : "1px solid var(--rule-strong)",
          borderBottom: top ? "1px solid var(--rule-strong)" : undefined,
          borderRadius: top ? "0 0 14px 14px" : "12px 12px 0 0",
          boxShadow: top ? "0 16px 40px rgba(0,0,0,0.55)" : "0 -16px 40px rgba(0,0,0,0.5)",
          animation: `${top ? "panel-down" : "panel-up"} 220ms var(--ease) both`,
        }}
      >
        {!top && <span aria-hidden style={{ alignSelf: "center", width: 36, height: 4, borderRadius: 2, background: "var(--dim)", marginTop: 8 }} />}
        <header style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 10px 10px 18px", borderBottom: "1px solid var(--rule)" }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 18, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</div>
            <div className="t-meta" style={{ fontSize: 13 }}>
              {sub}
            </div>
          </div>
          <Button onClick={onClose}>Done</Button>
        </header>
        <div style={{ overflowY: "auto", minHeight: 0 }}>{body}</div>
      </div>
      <style>{`@keyframes panel-up { from { transform: translateY(24px); opacity: 0 } to { transform: none; opacity: 1 } } @keyframes panel-down { from { transform: translateY(-24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

function PatchList({ song, current, defaultLabel, onPick }: { song?: string; current: string | null; defaultLabel: string; onPick: (p: string | null) => void }) {
  // The song's own versions of patches come first: they are what its
  // sections were dialled in with.
  const own = song ? rig.patches.filter((p) => p.stack.toLowerCase() === song.toLowerCase()).map((p) => p.name) : [];
  return (
    <div style={{ padding: "10px 14px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
      <Cell title={defaultLabel} on={current === null} onClick={() => onPick(null)} />
      {own.length > 0 && (
        <div>
          <div className="t-label" style={{ marginBottom: 6, color: "var(--ink-2)" }}>
            {song}'s own
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))", gap: 6 }}>
            {own.map((p) => (
              <Cell key={p} title={p} swatch={tapeFor(stackOf(p))} on={current === p} onClick={() => onPick(p)} />
            ))}
          </div>
        </div>
      )}
      {rig.perf.stacks.filter((st) => st.patches.length > 0).map((st) => (
        <div key={st.name}>
          <Tape colour={tapeFor(st.name)} style={{ marginBottom: 6 }}>
            {st.name}
          </Tape>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))", gap: 6 }}>
            {st.patches.map((p) => (
              <Cell key={p} title={p} on={current === p} onClick={() => onPick(p)} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function Cell({ title, on, onClick, swatch }: { title: string; on: boolean; onClick: () => void; swatch?: string }) {
  return (
    <button
      className={on ? "" : "pressable"}
      onClick={onClick}
      style={{
        minHeight: 48,
        padding: "6px 12px",
        textAlign: "left",
        borderRadius: "var(--r)",
        background: on ? "var(--focus-bg)" : "var(--sheet)",
        border: on ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)",
        fontSize: 15,
        fontWeight: on ? 700 : 560,
        display: "flex",
        alignItems: "center",
        gap: 8,
      }}
    >
      {swatch && <span style={{ width: 7, height: 7, borderRadius: 2, background: swatch, flexShrink: 0 }} />}
      {title}
    </button>
  );
}

function SetsBody({ onClose, onNew }: { onClose: () => void; onNew: () => void }) {
  const s = useStore();
  const todayIso = isoOf(new Date());
  const rows = s.setlists.map((l, i) => ({ l, i }));
  const upcoming = rows.filter((r) => r.l.date >= todayIso).sort((a, b) => a.l.date.localeCompare(b.l.date));
  const past = rows.filter((r) => r.l.date && r.l.date < todayIso).sort((a, b) => b.l.date.localeCompare(a.l.date));
  const undated = rows.filter((r) => !r.l.date);
  const group = (label: string, list: typeof rows) =>
    list.length > 0 && (
      <div key={label}>
        <div className="t-label" style={{ padding: "14px 18px 6px", color: "var(--ink-3)" }}>
          {label}
        </div>
        {list.map(({ l, i }) => {
          const d = l.date ? new Date(l.date + "T00:00") : null;
          const on = i === s.setIndex;
          return (
            <button
              key={`${i}-${l.name}`}
              className="pressable"
              onClick={() => {
                chooseSet(i);
                onClose();
              }}
              style={{ width: "100%", minHeight: 64, padding: "8px 14px 8px 18px", display: "flex", alignItems: "center", gap: 14, textAlign: "left", borderTop: "1px solid var(--rule)", background: on ? "var(--focus-bg)" : undefined }}
            >
              {/* The date as a block: the day large, the month and weekday small. */}
              <span style={{ width: 44, flexShrink: 0, textAlign: "center", lineHeight: 1 }}>
                <span className="t-label" style={{ display: "block", fontSize: 10, color: "var(--ink-3)" }}>
                  {d ? MONTHS[d.getMonth()] : "—"}
                </span>
                <span className="num" style={{ display: "block", fontSize: 22, fontWeight: 750, margin: "2px 0" }}>
                  {d ? d.getDate() : ""}
                </span>
                <span style={{ display: "block", fontSize: 11, color: "var(--ink-3)" }}>{d ? WEEKDAYS[d.getDay()] : ""}</span>
              </span>
              <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
                <span style={{ fontSize: 16, fontWeight: on ? 750 : 620, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{setHeading(l)}</span>
                <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  {l.title && l.event && <EventChip event={l.event} />}
                  <span className="t-meta" style={{ fontSize: 13 }}>
                    {l.songs.length} songs{l.date ? ` · ${whenLabel(l.date)}` : ""}
                  </span>
                </span>
              </span>
              {on && <Badge tone="live">Open</Badge>}
            </button>
          );
        })}
      </div>
    );
  return (
    <div style={{ paddingBottom: 14 }}>
      <div style={{ padding: "12px 14px 0" }}>
        <Button primary onClick={onNew} style={{ width: "100%" }}>
          New set
        </Button>
      </div>
      {group("Upcoming", upcoming)}
      {group("Past", past)}
      {group("No date", undated)}
    </div>
  );
}

function DetailsBody({ mode, onDone }: { mode: "new" | "edit" | "duplicate"; onDone: () => void }) {
  const s = useStore();
  const set = currentSet(s);
  const metas: SetMeta[] = s.setlists;
  // Events, most recently used first.
  const events = [...new Set([...metas].filter((m) => m.event).sort((a, b) => b.date.localeCompare(a.date)).map((m) => m.event))];
  const start: SetMeta =
    mode === "edit"
      ? { event: set.event, date: set.date, title: set.title }
      : mode === "duplicate"
        ? { event: set.event, date: set.date ? addDays(set.date, 7) : isoOf(new Date()), title: "" }
        : { event: set.event || events[0] || "", date: nextDateFor(set.event || events[0] || "", metas), title: "" };
  const [m, setM] = useState<SetMeta>(start);
  const [newEvent, setNewEvent] = useState<string | null>(null);
  const name = setName(m);
  const clash = s.setlists.some((l, i) => l.name.toLowerCase() === name.toLowerCase() && !(mode === "edit" && i === s.setIndex));
  const ok = m.event.trim().length > 0 && !clash;
  const today = isoOf(new Date());
  const quick: { label: string; date: string }[] = [
    { label: "Today", date: today },
    { label: "Tomorrow", date: addDays(today, 1) },
  ];
  const nextUsual = m.event ? nextDateFor(m.event, metas.filter((x) => mode !== "edit" || x !== set)) : "";
  if (nextUsual && nextUsual !== today && nextUsual !== addDays(today, 1)) quick.push({ label: `Next ${m.event} · ${dateLabel(nextUsual)}`, date: nextUsual });
  const field = { width: "100%", minHeight: 48, padding: "0 12px", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", fontSize: 16 } as const;
  return (
    <div style={{ padding: "14px 16px 18px", display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <div className="t-label" style={{ color: "var(--ink-3)", marginBottom: 8 }}>
          Event
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
          {events.map((e) => {
            const on = e === m.event && newEvent === null;
            return (
              <button
                key={e}
                onClick={() => {
                  setNewEvent(null);
                  setM({ ...m, event: e, date: mode === "edit" ? m.date : nextDateFor(e, metas) });
                }}
                style={{ borderRadius: 999, boxShadow: on ? "0 0 0 2px var(--focus-fg)" : undefined }}
              >
                <EventChip event={e} big />
              </button>
            );
          })}
          <button
            className="pressable"
            onClick={() => setNewEvent("")}
            style={{ minHeight: 34, padding: "0 12px", borderRadius: 999, border: "1px dashed var(--rule-strong)", color: "var(--ink-2)", fontSize: 14, fontWeight: 600 }}
          >
            + New event
          </button>
        </div>
        {newEvent !== null && (
          <input
            autoFocus
            value={newEvent}
            onChange={(e) => {
              setNewEvent(e.target.value);
              setM({ ...m, event: e.target.value });
            }}
            placeholder="e.g. Sunday AM, Youth, Easter"
            style={{ ...field, marginTop: 10 }}
          />
        )}
      </div>
      <div>
        <div className="t-label" style={{ color: "var(--ink-3)", marginBottom: 8 }}>
          Date
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 8 }}>
          {quick.map((q) => {
            const on = q.date === m.date;
            return (
              <button
                key={q.label}
                className={on ? "" : "pressable"}
                onClick={() => setM({ ...m, date: q.date })}
                style={{ minHeight: 40, padding: "0 12px", borderRadius: "var(--r)", border: on ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)", background: on ? "var(--focus-bg)" : "transparent", fontSize: 14, fontWeight: on ? 700 : 560 }}
              >
                {q.label}
              </button>
            );
          })}
        </div>
        <input type="date" value={m.date} onChange={(e) => setM({ ...m, date: e.target.value })} style={{ ...field, colorScheme: "dark" }} />
      </div>
      <div>
        <div className="t-label" style={{ color: "var(--ink-3)", marginBottom: 8 }}>
          Title <span style={{ textTransform: "none", letterSpacing: 0, fontWeight: 500 }}>— optional</span>
        </div>
        <input value={m.title} onChange={(e) => setM({ ...m, title: e.target.value })} placeholder="Only for a special night — e.g. Worship Night" style={field} />
      </div>
      <div style={{ padding: "12px 14px", borderRadius: "var(--r-md)", background: "var(--sheet)", border: "1px solid var(--rule)" }}>
        <div style={{ fontSize: 18, fontWeight: 750 }}>{setHeading(m)}</div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 6 }}>
          {m.title && m.event && <EventChip event={m.event} />}
          <span style={{ fontSize: 14, fontWeight: 600 }}>{dateLabel(m.date)}</span>
          <span className="t-meta" style={{ fontSize: 13 }}>{whenLabel(m.date)}</span>
        </div>
        <div className="t-meta" style={{ marginTop: 8, fontSize: 12 }}>
          Saved as “{name}”
        </div>
        {clash && <div style={{ marginTop: 6, color: "var(--void)", fontSize: 13 }}>There's already a set for this event on this date.</div>}
      </div>
      <Button
        primary
        disabled={!ok}
        onClick={() => {
          if (mode === "edit") setDetails(s.setIndex, m);
          else if (mode === "duplicate") duplicateSet(s.setIndex, m);
          else newSetlist(m);
          onDone();
        }}
      >
        {mode === "edit" ? "Save" : mode === "duplicate" ? `Duplicate ${set.songs.length} songs` : "Create set"}
      </Button>
    </div>
  );
}

function AddBody() {
  const s = useStore();
  const set = currentSet(s);
  const [q, setQ] = useState("");
  const [making, setMaking] = useState(false);
  const [key, setKey] = useState("G");
  const [bpm, setBpm] = useState(72);
  const inSet = new Set(set.songs.map((x) => x.name));
  const all = [...rig.library.songs, ...s.newSongs];
  const shown = all.filter((x) => x.name.toLowerCase().includes(q.trim().toLowerCase()));
  const exists = all.some((x) => x.name.toLowerCase() === q.trim().toLowerCase());
  return (
    <div>
      <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8, borderBottom: "1px solid var(--rule)" }}>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Find a song, or type a new one"
          style={{ width: "100%", minHeight: 48, padding: "0 12px", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", fontSize: 16 }}
        />
        {q.trim() && !exists && !making && (
          <Button primary onClick={() => setMaking(true)}>
            New song “{q.trim()}”…
          </Button>
        )}
        {making && (
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <KeyGrid value={key} onPick={setKey} />
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span className="t-meta" style={{ flex: 1 }}>
                Tempo
              </span>
              <Button onClick={() => setBpm(Math.max(30, bpm - 1))} style={{ padding: 0, width: 48 }}>
                −
              </Button>
              <span className="num" style={{ minWidth: 48, textAlign: "center", fontSize: 18, fontWeight: 700 }}>
                {bpm}
              </span>
              <Button onClick={() => setBpm(bpm + 1)} style={{ padding: 0, width: 48 }}>
                +
              </Button>
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <Button onClick={() => setMaking(false)}>Cancel</Button>
              <Button
                primary
                onClick={() => {
                  newSong(q.trim(), key, bpm);
                  setMaking(false);
                  setQ("");
                }}
              >
                Create and add
              </Button>
            </div>
          </div>
        )}
      </div>
      {shown.map((song) => {
        const added = inSet.has(song.name);
        return (
          <button
            key={song.name}
            className="pressable"
            disabled={added}
            onClick={() => addSong({ name: song.name, key: song.key, bpm: song.bpm, start: "" })}
            style={{ width: "100%", minHeight: 56, padding: "6px 14px 6px 18px", display: "flex", alignItems: "center", gap: 10, textAlign: "left", borderBottom: "1px solid var(--rule)", opacity: added ? 0.5 : 1 }}
          >
            <span style={{ width: 10, height: 10, borderRadius: 3, flexShrink: 0, background: songColour(song.name, s.songColours) }} />
            <span style={{ flex: 1, minWidth: 0, fontSize: 16, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{song.name}</span>
            <KeyBox k={song.key} />
            <span className="num" style={{ width: 30, textAlign: "right", color: "var(--ink-2)", fontSize: 14 }}>
              {song.bpm || ""}
            </span>
            <span className="t-label" style={{ width: 46, textAlign: "right", fontSize: 11, color: added ? "var(--ink-3)" : "var(--focus-fg)" }}>
              {added ? "In set" : "Add"}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function KeyGrid({ value, onPick }: { value: string; onPick: (k: string) => void }) {
  const minor = value.endsWith("m");
  const root = minor ? value.slice(0, -1) : value;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(6, 1fr)", gap: 6 }}>
        {KEYS.map((k) => {
          const on = k === root;
          return (
            <button
              key={k}
              className={on ? "" : "pressable"}
              onClick={() => onPick(minor ? `${k}m` : k)}
              style={{
                minHeight: 48,
                borderRadius: "var(--r)",
                border: on ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)",
                background: on ? "var(--focus-bg)" : "var(--sheet)",
                fontSize: 16,
                fontWeight: on ? 750 : 560,
              }}
            >
              {k}
            </button>
          );
        })}
      </div>
      <Tabs
        options={[
          { id: "major", label: "Major" },
          { id: "minor", label: "Minor" },
        ]}
        value={minor ? "minor" : "major"}
        onChange={(v) => onPick(v === "minor" ? `${root}m` : root)}
      />
    </div>
  );
}

// ── Small things ─────────────────────────────────────────────────────

/** The next section a song's form suggests (Intro → Verse 1 → Chorus 1 …). */
function suggestSection(names: string[]): string {
  if (names.length === 0) return "Intro";
  const count = (base: string) => names.filter((n) => n.toLowerCase().startsWith(base.toLowerCase())).length;
  const verses = count("Verse");
  const choruses = count("Chorus");
  return verses <= choruses ? `Verse ${verses + 1}` : `Chorus ${choruses + 1}`;
}
