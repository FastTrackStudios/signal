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
import { rig, stackOf, type ProfileEntry } from "../data/rig";
import {
  partPatch,
  addPart,
  select,
  addVariation,
  moveStackPatch,
  playPreset,
  addStackPatch,
  moveStack,
  profileStacksOf,
  removeStackPatch,
  renameStack,
  renameStackPatch,
  addSection,
  backToPart,
  profileOf,
  setSetProfile,
  setSongProfile,
  songStacks,
  keepLive,
  playing,
  tapStack,
  addSong,
  chooseSet,
  currentSet,
  currentSong,
  deleteSet,
  duplicateSet,
  goToPart,
  goToSong,
  goToSub,
  movePart,
  moveSection,
  moveSong,
  newSetlist,
  newSong,
  removePart,
  removeSection,
  renamePart,
  removeSong,
  renameSection,
  setDetails,
  sectionsOf,
  setSectionSound,
  setSongField,
  setSongColour,
  setStart,
  useStore,
  type Sound,
} from "../store";
import { SourceIcon, Strike, Tape, tapeFor } from "../ui/marks";
import { MACRO_BAR_H } from "../dock/MacroBar";
import { ProfileIcon } from "../ui/profileIcons";
import { MODULE_COLOUR } from "../ui/moduleIcons";
import { OverrideIcon } from "../ui/OverrideIcon";
import { Button, KeyBox, Tabs } from "../ui/kit";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";
import { nameColour, sectionColour, songColour, SONG_PALETTE } from "./colors";
import { borrowable, findPatch, stacksFor, type StackPatch } from "./stacks";
import { addDays, dateLabel, isoOf, MONTHS, nextDateFor, setHeading, setName, WEEKDAYS, whenLabel, type SetMeta } from "./sets";

const KEYS = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/** How much room the component has, measured: the narrow sidebar folds
 *  each row down to what fits; the sidebar and a tablet show it all. */
type Fit = "narrow" | "sidebar" | "wide";
const FitCtx = createContext<Fit>("sidebar");

/** Set by a host with a Compose area beside the setlist (the iPad, the
 *  phone's Compose page): a section's patch is then picked there — a tap
 *  on its chip selects it — instead of in a sheet over the setlist. */
export const ComposeCtx = createContext<{ onPicked?: () => void; pickRows?: boolean } | null>(null);

/** Pick what a part plays: select it for Compose when there is one, else
 *  open the patch sheet. */
function usePick(onPanel: (p: Panel) => void) {
  const compose = useContext(ComposeCtx);
  return (song: string, section: number, part?: number) => {
    if (!compose) return onPanel({ kind: "patch", song, section, part });
    select({ song, section, part: part ?? 0 });
    // The device the pick was made on opens its browser — not every remote.
    compose.onPicked?.();
  };
}

/** Whether this part is what Compose / Edit are working on. */
function isPicked(sel: { song: string; section: number; part: number } | null, song: string, section: number, part = 0) {
  return !!sel && sel.song === song && sel.section === section && sel.part === part;
}
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
  | { kind: "patch"; song: string; section: number; part?: number }
  /** A song's profile (by index in the set), or the set's default. */
  | { kind: "profile"; song?: number };

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
  // Whether the song up is scrolled out of the list, and which way.
  const [away, setAway] = useState<"above" | "below" | null>(null);
  useLayoutEffect(() => {
    const list = listRef.current;
    const row = rows.current[s.songIndex];
    if (!list || !row) return;
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) return setAway(null);
        setAway(e.boundingClientRect.top < (e.rootBounds?.top ?? 0) ? "above" : "below");
      },
      { root: list, threshold: 0 },
    );
    io.observe(row);
    return () => io.disconnect();
  }, [s.songIndex, set.songs.length]);
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
      window.removeEventListener("pointercancel", onUp);
      setDrag(null);
      if (to !== i) moveSong(i, to);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    // A touch the system takes back ends the drag where it is.
    window.addEventListener("pointercancel", onUp);
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
      <div ref={listRef} style={{ position: "relative", flex: 1, minHeight: 0, overflowY: "auto", overscrollBehavior: "contain" }}>
        {/* Scrolled away from the song up: where it is, and a way back. */}
        {away === "above" && <NowBar where="above" onBack={() => rows.current[s.songIndex]?.scrollIntoView({ block: "start", behavior: "smooth" })} />}
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
        {set.songs.length > 0 && !reordering && (
          <button
            className="pressable"
            onClick={() => setPanel({ kind: "add" })}
            style={{ display: "flex", alignItems: "center", gap: 10, width: "100%", minHeight: 52, padding: "0 18px", color: "var(--ink-3)", fontSize: 15, fontWeight: 600, borderTop: "1px solid var(--rule)" }}
          >
            <svg width="14" height="14" viewBox="0 0 12 12" aria-hidden>
              <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
            Add songs
          </button>
        )}
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
        {away === "below" && <NowBar where="below" onBack={() => rows.current[s.songIndex]?.scrollIntoView({ block: "start", behavior: "smooth" })} />}
      </div>
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
    { kind: "run", id: "add", label: "Add songs…" },
    { kind: "run", id: "edit", label: "Edit details…", detail: "event · date · title" },
    { kind: "run", id: "profile", label: "Default profile…", detail: set.profile ?? "the rig's" },
    { kind: "run", id: "new", label: "New set…" },
    { kind: "run", id: "duplicate", label: "Duplicate for next week" },
    { kind: "run", id: "sets", label: "All sets…", detail: `${s.setlists.length}` },
    { kind: "run", id: "reorder", label: "Reorder songs", disabled: set.songs.length < 2 ? "Fewer than two songs" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete set", disabled: s.setlists.length <= 1 ? "The only set — make another first" : undefined },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "add") onPanel({ kind: "add" });
    if (p.id === "edit") onPanel({ kind: "details", mode: "edit" });
    if (p.id === "profile") onPanel({ kind: "profile" });
    if (p.id === "new") onPanel({ kind: "details", mode: "new" });
    if (p.id === "duplicate") onPanel({ kind: "details", mode: "duplicate" });
    if (p.id === "sets") onPanel({ kind: "sets" });
    if (p.id === "reorder") onReorder(true);
    if (p.id === "delete") deleteSet(s.setIndex);
  };
  const when = whenLabel(set.date);
  const prof = profileOf(s, undefined);
  // Only today and tomorrow are worth a word beside the date.
  const soon = when === "Today" || when === "Tomorrow" ? when : null;
  return (
    // Exactly the macro bar's height, so the two lines run straight across.
    <header style={{ flexShrink: 0, height: MACRO_BAR_H, padding: "0 6px 0 18px", borderBottom: "1px solid var(--rule)", display: "flex", flexDirection: "column", justifyContent: "center", gap: 7 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 2, minHeight: 0 }}>
        <button
          className="pressable"
          onClick={() => onPanel({ kind: "sets" })}
          style={{ flex: 1, minWidth: 0, minHeight: 44, display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, textAlign: "left", padding: "0 8px", margin: "0 0 0 -8px", borderRadius: "var(--r-md)" }}
          title="Sets — choose another or make a new one"
          aria-haspopup="dialog"
        >
          <h1 className="t-marker" style={{ margin: 0, fontSize: fit === "narrow" ? 18 : 21, lineHeight: 1.1, display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ minWidth: 0, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{setHeading(set)}</span>
            <svg width="10" height="6" viewBox="0 0 10 6" aria-hidden style={{ flexShrink: 0, color: "var(--ink-3)" }}>
              <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </h1>
          {/* Event · date · the set's profile — words, no chips or icons. */}
          <span style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, fontWeight: 560, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden" }}>
            {set.title && set.event && (
              <>
                <span aria-hidden style={{ width: 7, height: 7, borderRadius: 999, flexShrink: 0, background: nameColour(set.event) }} />
                <span style={{ color: "var(--ink-2)", fontWeight: 650 }}>{set.event}</span>
                <span aria-hidden>·</span>
              </>
            )}
            <span>{dateLabel(set.date)}</span>
            {soon && <span style={{ color: soon === "Today" ? "var(--live)" : "var(--ink-2)", fontWeight: 650 }}>{soon}</span>}
            <span aria-hidden>·</span>
            <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{prof.name}</span>
          </span>
        </button>
        {reordering ? (
          <Button primary onClick={() => onReorder(false)} style={{ alignSelf: "center", marginRight: 6 }}>
            Done
          </Button>
        ) : null}
        {!reordering && <MoreButton label="Set actions" onClick={menu.fromButton} />}
      </div>
      {fit !== "narrow" && set.songs.length > 0 && (
        // The set as a bar: a segment per song in its colour — played ones
        // dim, the one up full and taller — and its place in the set.
        <div style={{ display: "flex", alignItems: "center", gap: 10, paddingRight: 12 }}>
          <span aria-label={`Song ${s.songIndex + 1} of ${set.songs.length}`} style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 3, height: 10 }}>
            {set.songs.map((song, i) => (
              <span
                key={`${i}-${song.name}`}
                title={song.name}
                style={{ flex: "1 1 0", height: i === s.songIndex ? 8 : 4, borderRadius: 2, background: songColour(song.name, s.songColours), opacity: i < s.songIndex ? 0.3 : i === s.songIndex ? 1 : 0.6 }}
              />
            ))}
          </span>
          <span className="num" style={{ flexShrink: 0, fontSize: 13, fontWeight: 650, color: "var(--ink-2)" }}>
            {s.songIndex + 1}
            <span style={{ color: "var(--ink-3)", fontWeight: 560 }}> / {set.songs.length}</span>
          </span>
        </div>
      )}
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </header>
  );
}

/** What a song (or one of its sections) carries of its own over its
 *  preset: its module overrides, and whether Edit has unsaved changes. */
function ownChanges(s: ReturnType<typeof useStore>, song: string, section?: number): { modules: string[]; edits: boolean } {
  const prefix = section === undefined ? `${song}|` : `${song}|${section}|`;
  const modules = new Set<string>();
  let edits = false;
  for (const [k, o] of Object.entries(s.overrides)) {
    if (!k.startsWith(prefix)) continue;
    for (const m of Object.keys(o.modules)) modules.add(m);
    if (Object.keys(o.edits).length) edits = true;
  }
  return { modules: [...modules], edits };
}

/** Its own changes, in words: the override icon, then each effect it
 *  overrides by name in that effect's colour; "Unsaved" in amber for Edit
 *  changes not yet saved to the preset. A song shows a few, then a count. */
function Changes({ song, section, max = 4 }: { song: string; section?: number; max?: number }) {
  const s = useStore();
  const { modules, edits } = ownChanges(s, song, section);
  if (!modules.length && !edits) return null;
  const shown = modules.slice(0, max);
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 6, minWidth: 0, fontSize: 12, fontWeight: 650, whiteSpace: "nowrap", overflow: "hidden" }}>
      {modules.length > 0 && (
        <>
          <OverrideIcon colour="var(--ink-2)" size={11} title="Overrides" />
          {shown.map((m, k) => (
            <span key={m} style={{ color: `color-mix(in oklab, ${MODULE_COLOUR[m] ?? "var(--ink-2)"} 78%, white)` }}>
              {m}
              {k < shown.length - 1 ? <span style={{ color: "var(--ink-3)" }}> ·</span> : null}
            </span>
          ))}
          {modules.length > shown.length && <span style={{ color: "var(--ink-3)" }}>+{modules.length - shown.length}</span>}
        </>
      )}
      {edits && <span style={{ color: "var(--modified)" }}>Unsaved</span>}
    </span>
  );
}

/** The song row's height: the section playing sticks just under it. */
const SONG_ROW_H = 64;

/** Pinned over the list while the song up is scrolled out of it: the song
 *  and the section playing, and a tap back to them. */
function NowBar({ where, onBack }: { where: "above" | "below"; onBack: () => void }) {
  const s = useStore();
  const song = currentSong(s);
  if (!song) return null;
  const sec = sectionsOf(s, song.name)[s.partIndex];
  const colour = songColour(song.name, s.songColours);
  return (
    <button
      onClick={onBack}
      className="pressable"
      style={{
        position: "sticky",
        [where === "above" ? "top" : "bottom"]: 0,
        zIndex: 5,
        width: "100%",
        height: 44,
        marginBottom: where === "above" ? -44 : 0,
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "0 14px",
        textAlign: "left",
        background: tint(colour, 16),
        boxShadow: where === "above" ? "0 6px 16px rgba(0,0,0,0.45)" : "0 -6px 16px rgba(0,0,0,0.45)",
      }}
    >
      <span className="num" style={{ width: 22, height: 22, borderRadius: 5, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 12, fontWeight: 750, background: colour, color: "#0b0b0e" }}>
        {s.songIndex + 1}
      </span>
      <span style={{ fontSize: 15, fontWeight: 700, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{song.name}</span>
      {sec && <span style={{ fontSize: 14, fontWeight: 650, color: sectionColour(sec.name), whiteSpace: "nowrap" }}>{sec.name}</span>}
      <span style={{ flex: 1 }} />
      <span style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 12, fontWeight: 700, color: "var(--ink-2)", whiteSpace: "nowrap" }}>
        <svg width="10" height="12" viewBox="0 0 10 12" aria-hidden style={{ transform: where === "below" ? "rotate(180deg)" : undefined }}>
          <path d="M5 11V1.5M1 5.5 5 1.5l4 4" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        Now
      </span>
    </button>
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
  const prof = profileOf(s, song.name);
  const items: MenuItem[] = [
    { kind: "head", label: `${i + 1} · ${song.name}` },
    { kind: "run", id: "go", label: "Play from here", disabled: up ? "It's up now" : undefined },
    { kind: "sep" },
    { kind: "run", id: "key", label: "Key…", detail: song.key || "—" },
    { kind: "run", id: "tempo", label: "Tempo…", detail: song.bpm ? `${song.bpm} BPM` : "—" },
    { kind: "run", id: "start", label: "Starts on…", detail: song.start || "default" },
    { kind: "run", id: "profile", label: "Profile…", detail: prof.from === "song" ? prof.name : `${prof.name} (set's)` },
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
    if (p.id === "profile") onPanel({ kind: "profile", song: i });
    if (p.id === "colour") onPanel({ kind: "colour", song: i });
    if (p.id === "up") moveSong(i, i - 1);
    if (p.id === "down") moveSong(i, i + 1);
    if (p.id === "remove") removeSong(i);
  };
  return (
    <div
      {...(reordering ? {} : menu.longPress())}
      style={{
        // The song up stays at the top while you scroll through it.
        position: up && !reordering ? "sticky" : "relative",
        top: up && !reordering ? 0 : undefined,
        zIndex: up ? 3 : undefined,
        display: "flex",
        alignItems: "stretch",
        minHeight: SONG_ROW_H,
        background: lifted ? "var(--sheet-2)" : open ? tint(colour, up ? 7 : 4) : undefined,
        borderTop: dropAbove ? "3px solid var(--focus-fg)" : "1px solid var(--rule)",
      }}
    >
      {up && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: colour }} />}
      <button
        className="pressable"
        onClick={() => !reordering && (up ? onToggle() : goToSong(i))}
        aria-current={up ? "true" : undefined}
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
              {/* Its own profile, in words — only when it isn't the set's. */}
              {prof.from === "song" && <span style={{ flexShrink: 0, color: "var(--ink-2)", fontWeight: 650 }}>{prof.name}</span>}
              <Changes song={song.name} max={2} />
              {!open && sections.length > 0 && <span style={{ flexShrink: 0 }}>{sections.length} sections</span>}
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
  // Drag a section by its grip to move it.
  const rows = useRef<(HTMLDivElement | null)[]>([]);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const onGrip = (j: number) => (e: React.PointerEvent) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const at = (y: number) => {
      let to = 0;
      rows.current.forEach((el, k) => {
        if (el && y > el.getBoundingClientRect().top + 20) to = k;
      });
      return to;
    };
    setDrag({ from: j, to: j });
    const onMove = (ev: PointerEvent) => setDrag({ from: j, to: at(ev.clientY) });
    const onUp = (ev: PointerEvent) => {
      const to = at(ev.clientY);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      setDrag(null);
      if (to !== j) moveSection(song.name, j, to);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    // A touch the system takes back ends the drag where it is.
    window.addEventListener("pointercancel", onUp);
  };
  return (
    <div style={{ position: "relative", padding: "2px 0 10px", background: tint(colour, up ? 7 : 4) }}>
      {up && <span aria-hidden style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 3, background: colour }} />}
      {sections.map((sec, j) => (
        <div
          key={`${j}-${sec.name}`}
          ref={(el) => {
            rows.current[j] = el;
          }}
          style={{
            opacity: drag?.from === j ? 0.5 : 1,
            boxShadow: drag && drag.to === j && drag.from !== j ? `inset 0 ${drag.to < drag.from ? 2 : -2}px 0 var(--ink-2)` : undefined,
          }}
        >
          <SectionRow song={song.name} songIndex={songIndex} index={j} count={sections.length} onPanel={onPanel} onGrip={onGrip(j)} />
        </div>
      ))}
      {/* No sections: the song itself is what plays, so its stacks sit under it. */}
      {up && sections.length === 0 && <Stacks left={fit === "narrow" ? 20 : 34} />}
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

/** A muted tint of a song's colour over the sheet: how anything that
 *  belongs to a song is picked out (oklab keeps it from going neon). */
function tint(colour: string, pct: number): string {
  return `color-mix(in oklab, ${colour} ${pct}%, var(--sheet))`;
}

/** A section's sound when it is one part; null when it has several. */
function soundOf(sec: { parts: { sound: Sound | null }[] }): Sound | null {
  return sec.parts.length === 1 ? sec.parts[0].sound : null;
}

function SectionRow({ song, songIndex, index: j, count, onPanel, onGrip }: { song: string; songIndex: number; index: number; count: number; onPanel: (p: Panel) => void; onGrip: (e: React.PointerEvent) => void }) {
  const s = useStore();
  const pick = usePick(onPanel);
  const compose = useContext(ComposeCtx);
  const sec = sectionsOf(s, song)[j];
  const menu = useMenu();
  const up = songIndex === s.songIndex;
  const state: "done" | "now" | "ahead" = !up ? (songIndex < s.songIndex ? "done" : "ahead") : j < s.partIndex ? "done" : j === s.partIndex ? "now" : "ahead";
  const names = sectionsOf(s, song).map((x) => x.name);
  const narrow = useFit() === "narrow";
  const sound = soundOf(sec);
  const several = sec.parts.length > 1;
  const items: MenuItem[] = [
    { kind: "head", label: sec.name },
    { kind: "run", id: "go", label: "Play from here", disabled: state === "now" ? "It's playing" : undefined },
    several
      ? { kind: "run", id: "parts", label: "Its parts", detail: `${sec.parts.length}` }
      : { kind: "run", id: "patch", label: "Patch…", detail: sound?.name ?? "keeps" },
    { kind: "name", id: "part", label: "Add a part…", initial: `${sec.name} · ${sec.parts.length + 1}`, confirm: "Add", taken: sec.parts.map((x) => x.name) },
    { kind: "name", id: "rename", label: "Rename…", initial: sec.name, confirm: "Rename", taken: names },
    { kind: "sep" },
    { kind: "run", id: "earlier", label: "Move earlier", disabled: j === 0 ? "Already first" : undefined },
    { kind: "run", id: "later", label: "Move later", disabled: j === count - 1 ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete section" },
  ];
  const go = () => {
    if (!up) goToSong(songIndex);
    goToPart(j);
  };
  const onPick = (p: Picked) => {
    if (p.id === "go" || p.id === "parts") go();
    if (p.id === "patch") pick(song, j);
    if (p.id === "part") {
      addPart(song, j, p.text);
      go();
    }
    if (p.id === "rename") renameSection(song, j, p.text);
    if (p.id === "earlier") moveSection(song, j, j - 1);
    if (p.id === "later") moveSection(song, j, j + 1);
    if (p.id === "delete") removeSection(song, j);
  };
  const chip = several ? (
    <span className="t-meta" style={{ fontSize: 13, padding: "3px 8px", borderRadius: 4, background: "rgba(255,255,255,0.06)", color: "var(--ink-2)", fontWeight: 600 }}>
      {sec.parts.length} parts
    </span>
  ) : sound ? (
    <PatchChip name={sound.kind === "stack" ? `${sound.name} stack` : sound.name} borrowed={sound.profile} lit={state === "now"} small={narrow} />
  ) : (
    <span className="t-meta" style={{ fontSize: 13, padding: "0 4px" }}>
      keeps
    </span>
  );
  return (
    <>
      <div
        {...menu.longPress()}
        style={{
          // The section playing stays under the song while you scroll its stacks.
          position: state === "now" ? "sticky" : "relative",
          // Only the pinned row is offset: on a relative row `top` would shift it.
          top: state === "now" ? SONG_ROW_H : undefined,
          zIndex: state === "now" ? 2 : undefined,
          display: "flex",
          alignItems: "center",
          minHeight: 50,
          background: state === "now" ? tint(songColour(song, s.songColours), several ? 10 : 18) : undefined,
          boxShadow: state === "now" ? "0 1px 0 var(--rule)" : undefined,
        }}
      >
        {/* The grip: drag to move the section; it wears the section's colour. */}
        <span
          role="button"
          aria-label={`Drag ${sec.name} to move it`}
          onPointerDown={onGrip}
          style={{ width: narrow ? 30 : 40, alignSelf: "stretch", flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "flex-end", paddingRight: 6, cursor: "grab", touchAction: "none" }}
        >
          <svg width="14" height="12" viewBox="0 0 14 12" aria-hidden>
            <path
              d="M1.5 2h11M1.5 6h11M1.5 10h11"
              stroke={sectionColour(sec.name)}
              strokeOpacity={state === "done" ? 0.45 : 1}
              strokeWidth="1.7"
              strokeLinecap="round"
            />
          </svg>
        </span>
        <button
          className="pressable"
          // Building (pickRows): a tap picks the section to build into; else it plays there.
          onClick={() => (compose?.pickRows && !several ? pick(song, j) : go())}
          // The part being built into (Build) is ringed.
          aria-pressed={!several && isPicked(s.selection, song, j)}
          style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: narrow ? 8 : 12, minHeight: 50, padding: "4px 4px 4px 6px", textAlign: "left", borderRadius: "var(--r)", boxShadow: !several && isPicked(s.selection, song, j) ? "inset 0 0 0 1.5px var(--focus-fg)" : undefined }}
        >
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
            <Changes song={song} section={j} />
            {narrow && <span style={{ alignSelf: "flex-start", maxWidth: "100%", display: "flex" }}>{chip}</span>}
          </span>
          {/* What it plays — set from a stack's ⋯ (Make default). */}
          {!narrow && <span style={{ flex: "0 1 auto", maxWidth: "64%", minWidth: 0, display: "flex", paddingRight: 10 }}>{chip}</span>}
        </button>
        <MoreButton label={`${sec.name} actions`} onClick={menu.fromButton} />
        {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
      </div>
      {/* The section playing opens into its parts, when it has more than one. */}
      {up && state === "now" && several && <Parts song={song} index={j} onPanel={onPanel} />}
      {up && state === "now" && <Stacks left={narrow ? 36 : 46} />}
    </>
  );
}

/** The parts of the section playing: where in it you are, and what each
 *  part plays — the section's own little timeline. */
function Parts({ song, index: j, onPanel }: { song: string; index: number; onPanel: (p: Panel) => void }) {
  const s = useStore();
  const sec = sectionsOf(s, song)[j];
  const narrow = useFit() === "narrow";
  const add = useMenu();
  const left = narrow ? 36 : 46;
  return (
    <div style={{ position: "relative", paddingBottom: 4 }}>
      <span aria-hidden style={{ position: "absolute", left: left + 3, top: 0, bottom: 26, width: 1, background: "color-mix(in srgb, var(--live) 40%, var(--rule-strong))" }} />
      {sec.parts.map((part, k) => (
        <PartRow key={`${k}-${part.name}`} song={song} section={j} index={k} count={sec.parts.length} left={left} onPanel={onPanel} />
      ))}
      <button
        className="pressable"
        onClick={add.fromButton}
        style={{ display: "flex", alignItems: "center", gap: 8, width: "100%", minHeight: 36, padding: `0 14px 0 ${left - 2}px`, color: "var(--ink-3)", fontSize: 13, fontWeight: 600 }}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
        Add a part
      </button>
      {add.open && (
        <Menu
          at={add.open.at}
          naming={0}
          items={[{ kind: "name", id: "add", label: "New part…", initial: `${sec.name} · ${sec.parts.length + 1}`, confirm: "Add", taken: sec.parts.map((x) => x.name) }]}
          onPick={(p) => addPart(song, j, p.text)}
          onClose={add.close}
        />
      )}
    </div>
  );
}

function PartRow({ song, section: j, index: k, count, left, onPanel }: { song: string; section: number; index: number; count: number; left: number; onPanel: (p: Panel) => void }) {
  const s = useStore();
  const pick = usePick(onPanel);
  const compose = useContext(ComposeCtx);
  const part = sectionsOf(s, song)[j].parts[k];
  const menu = useMenu();
  const narrow = useFit() === "narrow";
  const state = k < s.subIndex ? "done" : k === s.subIndex ? "now" : "ahead";
  const items: MenuItem[] = [
    { kind: "head", label: part.name },
    { kind: "run", id: "go", label: "Play from here", disabled: state === "now" ? "It's playing" : undefined },
    { kind: "run", id: "patch", label: "Patch…", detail: part.sound?.name ?? "keeps" },
    { kind: "name", id: "rename", label: "Rename…", initial: part.name, confirm: "Rename" },
    { kind: "sep" },
    { kind: "run", id: "earlier", label: "Move earlier", disabled: k === 0 ? "Already first" : undefined },
    { kind: "run", id: "later", label: "Move later", disabled: k === count - 1 ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete part", disabled: count <= 1 ? "A section keeps at least one part" : undefined },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "go") goToSub(j, k);
    if (p.id === "patch") pick(song, j, k);
    if (p.id === "rename") renamePart(song, j, k, p.text);
    if (p.id === "earlier") movePart(song, j, k, k - 1);
    if (p.id === "later") movePart(song, j, k, k + 1);
    if (p.id === "delete") removePart(song, j, k);
  };
  const chip = part.sound ? <PatchChip name={part.sound.kind === "stack" ? `${part.sound.name} stack` : part.sound.name} borrowed={part.sound.profile} lit={state === "now"} small /> : <span className="t-meta" style={{ fontSize: 12, padding: "0 4px" }}>keeps</span>;
  return (
    <div {...menu.longPress()} style={{ position: "relative", display: "flex", alignItems: "center", minHeight: 44, background: state === "now" ? tint(songColour(song, s.songColours), 18) : undefined }}>
      <button className="pressable" onClick={() => (compose?.pickRows ? pick(song, j, k) : goToSub(j, k))} aria-pressed={isPicked(s.selection, song, j, k)} style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 10, minHeight: 44, padding: `2px 4px 2px ${left}px`, textAlign: "left", borderRadius: "var(--r)", boxShadow: isPicked(s.selection, song, j, k) ? "inset 0 0 0 1.5px var(--focus-fg)" : undefined }}>
        <span
          style={{
            position: "relative",
            zIndex: 1,
            width: 8,
            height: 8,
            borderRadius: 999,
            flexShrink: 0,
            background: state === "now" ? "var(--live)" : "var(--sheet)",
            border: `1.5px solid ${state === "now" ? "var(--live)" : state === "done" ? "var(--ink-3)" : "var(--dim)"}`,
          }}
        />
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
          <span style={{ fontSize: 14, fontWeight: state === "now" ? 650 : 500, color: state === "done" ? "var(--ink-3)" : state === "now" ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {part.name}
          </span>
          {narrow && <span style={{ alignSelf: "flex-start", display: "flex", maxWidth: "100%" }}>{chip}</span>}
        </span>
        {!narrow && <span style={{ flex: "0 1 auto", maxWidth: "64%", minWidth: 0, display: "flex", paddingRight: 10 }}>{chip}</span>}
      </button>

      <MoreButton label={`${part.name} actions`} onClick={menu.fromButton} />
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}


/** A patch, with the stack it lands on: the stack's name in its colour,
 *  then the patch; a patch borrowed from another profile says from where. */
function PatchChip({ name, lit, small, borrowed }: { name: string; lit?: boolean; small?: boolean; borrowed?: string }) {
  const isStack = name.endsWith(" stack");
  const stack = isStack ? name.slice(0, -6) : stackOf(name);
  const colour = tapeFor(stack);
  const gaffer = colour === "var(--tape-gaffer)";
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        minWidth: 0,
        maxWidth: "100%",
        padding: small ? "2px 8px 2px 7px" : "5px 9px 5px 8px",
        borderRadius: 4,
        background: lit ? "var(--fill-on)" : "var(--fill)",
        color: lit ? "var(--ink)" : "var(--ink-2)",
        fontSize: small ? 12 : 13,
        fontWeight: 600,
        whiteSpace: "nowrap",
      }}
    >
      {stack && (
        <span style={{ flexShrink: 0, fontSize: small ? 10.5 : 11, fontWeight: 750, letterSpacing: "0.07em", textTransform: "uppercase", color: gaffer ? "var(--ink-3)" : `color-mix(in oklab, ${colour} 80%, white)` }}>{stack}</span>
      )}
      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{isStack ? "the stack" : name}</span>
      {borrowed && <span style={{ flexShrink: 0, color: nameColour(borrowed), fontWeight: 650 }}>{borrowed}</span>}
    </span>
  );
}

function Badge({ children, tone }: { children: ReactNode; tone?: "live" }) {
  return (
    <span
      className="t-label"
      style={{
        flexShrink: 0,
        fontSize: 11,
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

// ── The stacks: every one of the profile's, under what is playing ──

/** The song's stacks, indented under the section playing (under the song
 *  when it has no sections). Every top-level stack of the profile is here,
 *  so any patch is always in reach: a tap plays a stack, a tap on the one
 *  playing steps through it. Each patch shows where it came from — the
 *  song put it there, or the profile passes it through. */
/** The stacks, managed where they are: under the section playing (the
 *  song's stacks) or as the Profile view (the profile's own, `reorder` on:
 *  grips to drag them into order). A profile has five stacks — they are
 *  the five switches — so none are added or deleted; long-press a stack
 *  (or its ⋯) to add a patch to it, rename or remove the one showing,
 *  rename the stack, or move it. */
export function Stacks({ left, profile: only, reorder }: { left: number; profile?: string; reorder?: boolean }) {
  const s = useStore();
  const song = only ? undefined : currentSong(s)?.name;
  const profile = only ?? profileOf(s, song).name;
  const stacks = only ? songStacksForProfile(s, only) : songStacks(s, song);
  const now = playing(s);
  const at = now ? findPatch(stacks, now) : null;
  const own = partPatch(s);
  const saved = own ? findPatch(stacks, own) : null;
  const colour = song ? songColour(song, s.songColours) : nameColour(profile);
  const where = (() => {
    const sec = song ? sectionsOf(s, song)[s.partIndex] : undefined;
    if (!sec) return song ?? "this song";
    return sec.parts.length > 1 ? (sec.parts[s.subIndex]?.name ?? sec.name) : sec.name;
  })();
  // Drag a stack by its grip to move it (Profile view).
  const rows = useRef<(HTMLDivElement | null)[]>([]);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const onGrip = (i: number) => (e: React.PointerEvent) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const atY = (y: number) => {
      let to = 0;
      rows.current.forEach((el, k) => {
        if (el && y > el.getBoundingClientRect().top + 16) to = k;
      });
      return to;
    };
    setDrag({ from: i, to: i });
    const onMove = (ev: PointerEvent) => setDrag({ from: i, to: atY(ev.clientY) });
    const onUp = (ev: PointerEvent) => {
      const to = atY(ev.clientY);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      setDrag(null);
      if (to !== i) moveStack(profile, i, to);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    // A touch the system takes back ends the drag where it is.
    window.addEventListener("pointercancel", onUp);
  };
  return (
    <div style={{ position: "relative", padding: `2px 0 8px ${left}px` }}>
      <div style={{ background: "rgba(0,0,0,0.18)", display: "flex", flexDirection: "column", borderBottom: "1px solid var(--rule)" }}>
        {stacks.map((st, i) => {
          const on = at?.stack === i;
          // The part's own patch, while something else plays by hand.
          const home = s.live && saved && saved.stack === i ? saved.index : null;
          const pos = on ? at!.index : home ?? (s.stackAt[st.name] ?? 0) % Math.max(1, st.patches.length);
          return (
            <div
              key={st.name}
              ref={(el) => {
                rows.current[i] = el;
              }}
              style={{ opacity: drag?.from === i ? 0.5 : 1, boxShadow: drag && drag.to === i && drag.from !== i ? `inset 0 ${drag.to < drag.from ? 2 : -2}px 0 var(--ink-2)` : undefined }}
            >
              <StackRow stack={st} index={i} count={stacks.length} profile={profile} on={on} home={home !== null} where={where} pos={pos} songColour={colour} onGrip={reorder ? onGrip(i) : undefined} song={song} isDefault={!!saved && saved.stack === i && saved.index === pos} />
              {/* The Profile view opens the stack playing into all its patches. */}
              {reorder && on && <StackPatches profile={profile} stack={st} index={i} pos={pos} />}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Every patch in a stack, under it: tap one to play it; ⋯ (or a
 *  long-press) to rename it, move it, or remove it; "Add a patch" at the
 *  foot. The Profile view's open stack. */
function StackPatches({ profile, stack, pos }: { profile: string; stack: ReturnType<typeof stacksFor>[number]; index: number; pos: number }) {
  const s = useStore();
  const add = useMenu();
  const defs = profileStacksOf(s, profile);
  const def = defs.findIndex((d) => d.name === stack.name);
  const tape = tapeFor(stack.name);
  return (
    <div style={{ background: "rgba(0,0,0,0.25)", paddingBottom: 2 }}>
      {stack.patches.map((p, k) => (
        <PatchRow key={p.name} profile={profile} stack={stack} def={def} index={k} count={stack.patches.length} on={k === pos} tape={tape} />
      ))}
      <button className="pressable" onClick={add.fromButton} style={{ display: "flex", alignItems: "center", gap: 10, width: "100%", minHeight: 44, padding: "0 12px 0 52px", color: "var(--ink-3)", fontSize: 14, fontWeight: 600, textAlign: "left" }}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
        Add a patch to {stack.name}
      </button>
      {add.open && (
        <Menu
          at={add.open.at}
          naming={0}
          items={[{ kind: "name", id: "add", label: `New patch in ${stack.name}…`, initial: `${stack.name} ${stack.patches.length + 1}`, confirm: "Add", taken: stack.patches.map((x) => x.name) }]}
          onPick={(x) => def >= 0 && addStackPatch(profile, def, x.text)}
          onClose={add.close}
        />
      )}
          </div>
  );
}

function PatchRow({ profile, stack, def, index: k, count, on, tape }: { profile: string; stack: ReturnType<typeof stacksFor>[number]; def: number; index: number; count: number; on: boolean; tape: string }) {
  const menu = useMenu();
  const p = stack.patches[k];
  const items: MenuItem[] = [
    { kind: "head", label: p.name },
    { kind: "name", id: "rename", label: "Rename…", initial: p.name, confirm: "Rename", taken: stack.patches.map((x) => x.name) },
    { kind: "run", id: "up", label: "Move up", disabled: k === 0 ? "Already first" : undefined },
    { kind: "run", id: "down", label: "Move down", disabled: k === count - 1 ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "remove", label: "Remove from the stack", disabled: count <= 1 ? "A stack keeps at least one patch" : undefined },
  ];
  const onPick = (x: Picked) => {
    if (def < 0) return;
    if (x.id === "rename") renameStackPatch(profile, def, k, x.text);
    if (x.id === "up") moveStackPatch(profile, def, k, k - 1);
    if (x.id === "down") moveStackPatch(profile, def, k, k + 1);
    if (x.id === "remove") removeStackPatch(profile, def, k);
  };
  return (
    <div {...menu.longPress()} style={{ position: "relative", display: "flex", alignItems: "center", minHeight: 44, background: on ? "rgba(255,255,255,0.06)" : undefined }}>
      <button
        className="pressable"
        // Play this one: step the stack so a tap lands on it.
        onClick={() => tapStack(stack.name, stack.patches.map((x) => x.name), (k - 1 + count) % count)}
        style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 10, minHeight: 44, padding: "0 4px 0 52px", textAlign: "left" }}
      >
        <span style={{ width: 8, height: 8, borderRadius: 999, flexShrink: 0, background: on ? "var(--live)" : "transparent", boxShadow: on ? undefined : `inset 0 0 0 1.5px ${tape === "var(--tape-gaffer)" ? "var(--ink-3)" : tape}` }} />
        <span style={{ flex: 1, minWidth: 0, fontSize: 15, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
      </button>
      <MoreButton label={`${p.name} actions`} onClick={menu.fromButton} />
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}

/** The Preset view: the presets, and the one playing (or picked) open into
 *  all its variations — tap a variation to play it; add one at the foot. */
export function PresetView() {
  const s = useStore();
  const [open, setOpen] = useState<string | null>(s.presetUp?.preset ?? null);
  const presets = s.presets.filter((p) => !p.cancelled);
  return (
    <section style={{ height: "100%", display: "flex", flexDirection: "column", background: "var(--sheet)", minHeight: 0 }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, padding: "0 16px", borderBottom: "1px solid var(--rule)" }}>
        <span className="t-marker" style={{ fontSize: 22 }}>
          Presets
        </span>
        <span className="t-meta" style={{ fontSize: 13 }}>
          {presets.length} presets · {presets.reduce((n, p) => n + p.variations.length, 0)} variations
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {presets.map((p) => {
          const playing = s.presetUp?.preset === p.name;
          const isOpen = open === p.name;
          return (
            <div key={p.name} style={{ borderBottom: "1px solid var(--rule)" }}>
              <button
                className="pressable"
                onClick={() => {
                  setOpen(isOpen ? null : p.name);
                  if (!playing) playPreset(p.name, p.variations[0]);
                }}
                aria-expanded={isOpen}
                style={{ position: "relative", width: "100%", display: "flex", alignItems: "center", gap: 10, minHeight: 52, padding: "0 14px 0 16px", textAlign: "left", background: playing ? "rgba(255,255,255,0.05)" : undefined }}
              >
                {playing && <span aria-hidden style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 3, borderRadius: 2, background: "var(--live)" }} />}
                <span style={{ width: 9, height: 9, borderRadius: 2, flexShrink: 0, background: tapeFor(stackOf(p.name)) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(stackOf(p.name)) }} />
                <span style={{ flex: 1, minWidth: 0, fontSize: 16, fontWeight: playing ? 700 : 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
                <span className="t-meta" style={{ fontSize: 13 }}>
                  {p.variations.length} {p.variations.length === 1 ? "variation" : "variations"}
                </span>
                <svg width="10" height="6" viewBox="0 0 10 6" aria-hidden style={{ color: "var(--ink-3)", transform: isOpen ? "rotate(180deg)" : undefined }}>
                  <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>
              {isOpen && <Variations preset={p.name} variations={p.variations} />}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function Variations({ preset, variations }: { preset: string; variations: string[] }) {
  const s = useStore();
  const add = useMenu();
  return (
    <div style={{ background: "rgba(0,0,0,0.25)", paddingBottom: 2 }}>
      {variations.map((v) => {
        const on = s.presetUp?.preset === preset && s.presetUp.variation === v;
        return (
          <button key={v} className="pressable" onClick={() => playPreset(preset, v)} style={{ width: "100%", display: "flex", alignItems: "center", gap: 10, minHeight: 44, padding: "0 16px 0 36px", textAlign: "left", background: on ? "rgba(255,255,255,0.06)" : undefined }}>
            <span style={{ width: 8, height: 8, borderRadius: 999, flexShrink: 0, background: on ? "var(--live)" : "transparent", boxShadow: on ? undefined : "inset 0 0 0 1.5px var(--ink-3)" }} />
            <span style={{ flex: 1, fontSize: 15, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-2)" }}>{v}</span>
            {on && <span style={{ fontSize: 12, fontWeight: 700, color: "var(--live)" }}>Playing</span>}
          </button>
        );
      })}
      <button className="pressable" onClick={add.fromButton} style={{ display: "flex", alignItems: "center", gap: 10, width: "100%", minHeight: 44, padding: "0 16px 0 36px", color: "var(--ink-3)", fontSize: 14, fontWeight: 600, textAlign: "left" }}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
          <path d="M6 1v10M1 6h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
        Add a variation
      </button>
      {add.open && (
        <Menu
          at={add.open.at}
          naming={0}
          items={[{ kind: "name", id: "add", label: `New variation of ${preset}…`, initial: `Variation ${variations.length + 1}`, confirm: "Add", taken: variations }]}
          onPick={(x) => addVariation(preset, x.text)}
          onClose={add.close}
        />
      )}
    </div>
  );
}

/** A profile's stacks on their own — no song's patches in them. */
function songStacksForProfile(s: ReturnType<typeof useStore>, profile: string) {
  return stacksFor(undefined, [], profile, profileStacksOf(s, profile));
}

/** Where a patch comes from, in words, for its tooltip. */
function fromLabel(p: StackPatch): string {
  return p.from === "song" ? "the song's own" : p.from === "other" ? `borrowed from ${p.profile}` : `from ${p.profile}, passed through`;
}

/** The colour of a patch's source mark. */
function markColour(p: StackPatch, songColour: string): string {
  return p.from === "song" ? songColour : p.from === "other" ? nameColour(p.profile!) : "var(--ink-3)";
}

/** One stack: its name, the patch a tap plays, its rotation as dots (the
 *  song's filled in the song's colour, the profile's open), and whose that
 *  patch is. */
function StackRow({ stack, index: i, count, profile, on, home, where, pos, songColour: colour, onGrip, song, isDefault }: { stack: ReturnType<typeof stacksFor>[number]; index: number; count: number; profile: string; on: boolean; home: boolean; where: string; pos: number; songColour: string; onGrip?: (e: React.PointerEvent) => void; song?: string; isDefault?: boolean }) {
  const s = useStore();
  const menu = useMenu();
  const byHand = on && !!s.live;
  const showing = stack.patches[pos];
  // Only the profile's own patches are the profile's to rename or remove;
  // a song's own, or one borrowed, belongs elsewhere.
  const defs = profileStacksOf(s, profile);
  const def = defs.findIndex((d) => d.name === stack.name);
  const mine = showing && showing.from === "profile" && def >= 0 ? defs[def].patches.indexOf(showing.name) : -1;
  const items: MenuItem[] = [
    { kind: "head", label: `${stack.name} · ${showing?.name ?? "empty"}` },
    ...stack.patches.map((p, k) => ({ kind: "run" as const, id: `p${k}`, label: p.name, detail: fromLabel(p), checked: k === pos && on })),
    ...(byHand
      ? [{ kind: "sep" as const }, { kind: "run" as const, id: "keep", label: `Keep for ${where}` }, { kind: "run" as const, id: "back", label: `Back to ${where}'s patch` }]
      : []),
    // The section (or part) playing gets this patch as its own.
    ...(song && showing && sectionsOf(s, song).length
      ? [{ kind: "sep" as const }, { kind: "run" as const, id: "default", label: `Make “${showing.name}” default for ${where}`, disabled: isDefault ? `It's ${where}'s already` : undefined }]
      : []),
    { kind: "sep" },
    { kind: "name", id: "add", label: "Add a patch…", initial: `${stack.name} ${stack.patches.length + 1}`, confirm: "Add", taken: stack.patches.map((p) => p.name) },
    ...(mine >= 0 ? [{ kind: "name" as const, id: "renamePatch", label: `Rename “${showing.name}”…`, initial: showing.name, confirm: "Rename", taken: stack.patches.map((p) => p.name) }] : []),
    { kind: "name", id: "rename", label: "Rename stack…", initial: stack.name, confirm: "Rename", taken: defs.map((d) => d.name) },
    { kind: "run", id: "up", label: "Move up", disabled: i === 0 ? "Already first" : undefined },
    { kind: "run", id: "down", label: "Move down", disabled: i === count - 1 ? "Already last" : undefined },
    ...(mine >= 0 ? [{ kind: "sep" as const }, { kind: "delete" as const, id: "removePatch", label: `Remove “${showing.name}”`, disabled: stack.patches.length <= 1 ? "A stack keeps at least one patch" : undefined }] : []),
  ];
  const onPick = (p: Picked) => {
    if (p.id === "default" && song && showing) setSectionSound(song, s.partIndex, { kind: "patch", name: showing.name, profile: showing.from === "other" ? showing.profile : undefined }, s.subIndex);
    else if (p.id === "keep") keepLive();
    else if (p.id === "back") backToPart();
    else if (p.id === "add") addStackPatch(profile, def, p.text);
    else if (p.id === "renamePatch") renameStackPatch(profile, def, mine, p.text);
    else if (p.id === "rename") renameStack(profile, def, p.text);
    else if (p.id === "up") moveStack(profile, def, def - 1);
    else if (p.id === "down") moveStack(profile, def, def + 1);
    else if (p.id === "removePatch") removeStackPatch(profile, def, mine);
    else if (p.id.startsWith("p")) {
      const k = Number(p.id.slice(1));
      tapStack(stack.name, stack.patches.map((x) => x.name), (k - 1 + stack.patches.length) % stack.patches.length);
    }
  };
  const tape = tapeFor(stack.name);
  const patch = stack.patches[pos] ?? { name: "Empty", from: "profile" as const, profile };
  const many = stack.patches.length > 1;
  const next = many ? stack.patches[(pos + 1) % stack.patches.length] : undefined;
  const narrow = useFit() === "narrow";
  return (
    <div style={{ display: "flex", alignItems: "stretch", borderTop: "1px solid var(--rule)", background: on ? tint(colour, 18) : undefined }}>
    {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    {onGrip && (
      <span
        role="button"
        aria-label={`Drag ${stack.name} to move it`}
        onPointerDown={onGrip}
        style={{ width: 36, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center", cursor: "grab", touchAction: "none" }}
      >
        <svg width="14" height="12" viewBox="0 0 14 12" aria-hidden>
          <path d="M1.5 2h11M1.5 6h11M1.5 10h11" stroke={tape === "var(--tape-gaffer)" ? "var(--ink-3)" : tape} strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      </span>
    )}
    <button
      className="pressable"
      disabled={stack.patches.length === 0}
      onClick={() => (home ? backToPart() : tapStack(stack.name, stack.patches.map((p) => p.name), on ? pos : null))}
      title={on && next ? `Tap again for ${next.name}` : home ? `${where}'s patch — tap to go back to it` : `Play ${patch.name}`}
      aria-pressed={on}
      {...menu.longPress()}
      style={{
        position: "relative",
        flex: 1,
        minWidth: 0,
        display: "flex",
        alignItems: "center",
        gap: 10,
        minHeight: 44,
        padding: onGrip ? "0 4px 0 2px" : "0 4px 0 10px",
        textAlign: "left",
      }}
    >
      {/* Green bar: what plays. Hollow: the part's own patch, while
          something else plays by hand — tap it to go back. */}
      {(on || home) && (
        <span
          aria-hidden
          style={{ position: "absolute", left: 0, top: 6, bottom: 6, width: 3, borderRadius: 2, background: on ? (byHand ? "var(--modified)" : "var(--live)") : "transparent", boxShadow: home ? "inset 0 0 0 1.25px var(--live)" : undefined }}
        />
      )}

      <span style={{ width: narrow ? 58 : 70, flexShrink: 0, display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ width: 8, height: 8, borderRadius: 2, flexShrink: 0, background: tape }} />
        <span className="t-label" style={{ fontSize: 11, letterSpacing: "0.08em", color: on ? "var(--ink)" : "var(--ink-3)" }}>
          {stack.name}
        </span>
      </span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
        <span style={{ fontSize: 14, fontWeight: on ? 700 : 560, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {patch.name}
        </span>
        {many && (
          <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
            {stack.patches.map((p, k) => {
              const lit = k === pos;
              const song = p.from === "song";
              return (
                <span
                  key={p.name}
                  title={`${p.name} — ${fromLabel(p)}`}
                  style={{
                    width: lit ? 12 : 5,
                    height: 5,
                    borderRadius: 999,
                    flexShrink: 0,
                    
                    background: song ? colour : p.from === "other" ? nameColour(p.profile!) : lit ? "var(--ink-2)" : "transparent",
                    boxShadow: song || p.from === "other" ? undefined : `inset 0 0 0 1.25px ${lit ? "var(--ink-2)" : "var(--ink-3)"}`,
                    opacity: song && !lit ? 0.55 : 1,
                  }}
                />
              );
            })}
            {/* What the next tap plays, on the stack that's playing. */}
            {on && next && (
              <span style={{ marginLeft: 6, minWidth: 0, fontSize: 12, fontWeight: 600, color: "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                next › <span style={{ color: "var(--ink-2)" }}>{next.name}</span>
              </span>
            )}
          </span>
        )}
      </span>
      {isDefault && <span style={{ flexShrink: 0, fontSize: 12, fontWeight: 650, color: "var(--ink-3)" }}>Default</span>}
      {/* Borrowed from another profile: that profile, by name. */}
      {patch.from === "other" && <span style={{ flexShrink: 0, fontSize: 12, fontWeight: 650, color: nameColour(patch.profile!) }}>{patch.profile}</span>}
    </button>
    <MoreButton label={`${stack.name} actions`} onClick={menu.fromButton} />
    </div>
  );
}

/** The Profile view: the profile's stacks on their own — the same rows the
 *  setlist shows under a section, with grips to drag them into order. */
export function ProfileView() {
  const s = useStore();
  const profile = profileOf(s, undefined).name;
  const stacks = profileStacksOf(s, profile);
  return (
    <section style={{ height: "100%", display: "flex", flexDirection: "column", background: "var(--sheet)", minHeight: 0 }}>
      <header style={{ flexShrink: 0, height: MACRO_BAR_H, display: "flex", alignItems: "center", gap: 12, padding: "0 16px", borderBottom: "1px solid var(--rule)" }}>
        <ProfileIcon name={profile} colour={nameColour(profile)} size={22} />
        <span style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          <span className="t-marker" style={{ fontSize: 22 }}>
            {profile}
          </span>
          <span className="t-meta" style={{ fontSize: 13 }}>
            {stacks.length} stacks · {stacks.reduce((n, d) => n + d.patches.length, 0)} patches
          </span>
        </span>
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto", paddingTop: 8 }}>
        <Stacks left={8} profile={profile} reorder />
      </div>
    </section>
  );
}

/** The sidebar for the footswitch mode: the setlist, the profile's stacks,
 *  or (next) the presets. */
export function SidebarContent() {
  const s = useStore();
  if (s.performMode === "profile") return <ProfileView />;
  if (s.performMode === "preset") return <PresetView />;
  return <Setlist />;
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
          <KeyGrid value={song.key || "C"} onPick={(k) => { setSongField(panel.song, "key", k); onClose(); }} />
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
    case "profile": {
      const song = panel.song !== undefined ? set.songs[panel.song] : undefined;
      const current = song ? (s.songProfiles[song.name] ?? null) : (set.profile ?? null);
      const fallback = song ? profileOf({ ...s, songProfiles: {} }, song.name) : profileOf({ ...s, setlists: s.setlists.map((l) => ({ ...l, profile: undefined })) }, undefined);
      title = song ? song.name : "Default profile";
      sub = "";
      body = (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <ProfileCell
            name={song ? "Setlist default" : "Rig default"}
            of={fallback.name}
            inherit
            profile={rig.library.profiles.find((x) => x.name === fallback.name)}
            on={current === null}
            onClick={() => { if (song) setSongProfile(song.name, null); else setSetProfile(null); onClose(); }}
          />
          {/* The default stands apart: a band of the desk between it and the profiles. */}
          <div aria-hidden style={{ height: 8, background: "var(--desk)", borderBottom: "1px solid var(--rule)" }} />
          {/* The one picked leads, right under the default; the rest follow. */}
          {[...rig.library.profiles].sort((a, b) => Number(b.name === current) - Number(a.name === current)).map((p) => (
            <ProfileCell
              key={p.name}
              name={p.name}
              colour={nameColour(p.name)}
              profile={p}
              on={current === p.name}
              onClick={() => { if (song) setSongProfile(song.name, p.name); else setSetProfile(p.name); onClose(); }}
            />
          ))}
        </div>
      );
      break;
    }
    case "start": {
      const song = set.songs[panel.song];
      title = song.name;
      sub = "The patch it starts on";
      body = <PatchList song={song.name} current={song.start || null} defaultLabel="The profile's default" onPick={(p) => { setStart(panel.song, p?.name ?? ""); onClose(); }} />;
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
            onClick={() => { setSongColour(song.name, null); onClose(); }}
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
                  onClick={() => { setSongColour(song.name, c); onClose(); }}
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
      const k = panel.part ?? 0;
      const part = sec?.parts[k];
      title = sec && sec.parts.length > 1 ? `${sec.name} · ${part?.name ?? ""}` : `${panel.song} · ${sec?.name ?? ""}`;
      sub = sec && sec.parts.length > 1 ? `The patch this part plays — ${panel.song}` : "The patch this section plays";
      body = (
        <PatchList
          song={panel.song}
          current={part?.sound?.name ?? null}
          defaultLabel="Keep what plays"
          onPick={(p) => { setSectionSound(panel.song, panel.section, p ? { kind: "patch", ...p } : null, k); onClose(); }}
        />
      );
      break;
    }
  }
  // The sets switcher drops from the title it opened from; the pickers
  // rise from the bottom, under the thumb.
  const top = panel.kind === "sets";
  const [pull, setPull] = useState(0);
  const from = useRef<number | null>(null);
  const swipe = {
    onPointerDown: (e: React.PointerEvent) => {
      from.current = e.clientY;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    },
    onPointerMove: (e: React.PointerEvent) => {
      if (from.current !== null) setPull(Math.max(0, e.clientY - from.current));
    },
    onPointerUp: () => {
      from.current = null;
      if (pull > 80) onClose();
      else setPull(0);
    },
    onPointerCancel: () => {
      from.current = null;
      setPull(0);
    },
  };
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 20, display: "flex", flexDirection: "column", justifyContent: top ? "flex-start" : "flex-end" }}>
      <button aria-label="Close" onClick={onClose} style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.66)", cursor: "default" }} />
      <div
        style={{
          position: "relative",
          maxHeight: top ? "92%" : "95%",
          display: "flex",
          flexDirection: "column",
          background: "#18181c",
          borderTop: top ? undefined : "1px solid #3a3a42",
          borderBottom: top ? "1px solid var(--rule-strong)" : undefined,
          borderRadius: top ? "0 0 14px 14px" : "12px 12px 0 0",
          boxShadow: top ? "0 16px 40px rgba(0,0,0,0.55)" : "0 -16px 40px rgba(0,0,0,0.5)",
          animation: `${top ? "panel-down" : "panel-up"} 220ms var(--ease) both`,
          transform: pull ? `translateY(${pull}px)` : undefined,
          transition: pull ? "none" : "transform 180ms var(--ease)",
        }}
      >
        {/* Pull it down by its grabber or its title to close it. */}
        <div {...(top ? {} : swipe)} style={{ touchAction: "none", cursor: top ? undefined : "grab", display: "flex", flexDirection: "column" }}>
        {!top && <span aria-hidden style={{ alignSelf: "center", width: 36, height: 4, borderRadius: 2, background: "var(--dim)", marginTop: 8 }} />}
        <header style={{ display: "flex", alignItems: "center", gap: 10, minHeight: 52, padding: "6px 18px 10px", borderBottom: "1px solid var(--rule)" }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 18, fontWeight: 750, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</div>
            {sub && (
              <div className="t-meta" style={{ fontSize: 13 }}>
                {sub}
              </div>
            )}
          </div>
        </header>
        </div>
        <div style={{ overflowY: "auto", minHeight: 0 }}>{body}</div>
      </div>
      <style>{`@keyframes panel-up { from { transform: translateY(24px); opacity: 0 } to { transform: none; opacity: 1 } } @keyframes panel-down { from { transform: translateY(-24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

export type Pick = { name: string; profile?: string } | null;

export function PatchList({ song, current, defaultLabel, onPick }: { song?: string; current: string | null; defaultLabel: string; onPick: (p: Pick) => void }) {
  // Every stack of the song's profile, the song's own patches first in
  // each: what its sections were dialled in with, then what passes through,
  // then what its parts borrow. Other profiles' patches wait below.
  const s = useStore();
  const colour = song ? songColour(song, s.songColours) : "var(--ink-3)";
  const profile = profileOf(s, song);
  const others = borrowable(song, profile.name);
  const [showOthers, setShowOthers] = useState(false);
  return (
    <div style={{ padding: "10px 14px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
      <Cell title={defaultLabel} on={current === null} onClick={() => onPick(null)} />
      {songStacks(s, song).map((st) => (
        <div key={st.name}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
            <Tape colour={tapeFor(st.name)}>{st.name}</Tape>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))", gap: 6 }}>
            {st.patches.map((p) => (
              <Cell
                key={p.name}
                title={p.name}
                mark={<SourceIcon from={p.from} profile={p.profile} colour={markColour(p, colour)} size={13} />}
                hint={fromLabel(p)}
                on={current === p.name}
                onClick={() => onPick(p.from === "other" ? { name: p.name, profile: p.profile } : { name: p.name })}
              />
            ))}
          </div>
        </div>
      ))}
      {others.length > 0 && (
        <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 10 }}>
          <button
            className="pressable"
            onClick={() => setShowOthers(!showOthers)}
            aria-expanded={showOthers}
            style={{ width: "100%", minHeight: 44, display: "flex", alignItems: "center", gap: 10, padding: "0 4px", borderRadius: "var(--r)", textAlign: "left" }}
          >
            <SourceIcon from="profile" colour="var(--ink-2)" size={14} />
            <span style={{ flex: 1, fontSize: 15, fontWeight: 600 }}>From another profile</span>
            <span className="t-meta" style={{ fontSize: 13 }}>
              {others.map((o) => o.profile).join(" · ")}
            </span>
            <svg width="10" height="6" viewBox="0 0 10 6" aria-hidden style={{ transform: showOthers ? "rotate(180deg)" : undefined, color: "var(--ink-3)" }}>
              <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
          {showOthers && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: 8 }}>
              <p className="t-meta" style={{ margin: 0, fontSize: 13, lineHeight: 1.4 }}>
                {song ?? "The song"} plays on {profile.name}. A patch from another profile is borrowed: it joins the stack it sits in there.
              </p>
              {others.map((g) => (
                <div key={g.profile}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6, color: nameColour(g.profile) }}>
                    <SourceIcon from="other" profile={g.profile} colour={nameColour(g.profile)} size={12} />
                    <span className="t-label">{g.profile}</span>
                  </div>
                  <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))", gap: 6 }}>
                    {g.patches.map((p) => (
                      <Cell
                        key={p.name}
                        title={p.name}
                        mark={<span style={{ width: 7, height: 7, borderRadius: 2, background: tapeFor(p.stack), flexShrink: 0 }} />}
                        hint={`${p.stack} in ${g.profile}`}
                        on={current === p.name}
                        onClick={() => onPick({ name: p.name, profile: g.profile })}
                      />
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** A profile in the list: its name and what it holds, with its stacks
 *  drawn small at the right — one thin bar each, in the stack's colour, as
 *  tall as the stack is full. Rows run flush, hairlines between; the one
 *  picked is lifted. */
function ProfileCell({ name, of, inherit, profile, colour, on, onClick }: { name: string; of?: string; inherit?: boolean; profile?: ProfileEntry; colour?: string; on: boolean; onClick: () => void }) {
  const stacks = profile ? profile.stacks.map((st) => ({ name: st, count: profile.patch_list.filter((x) => x.stack === st).length })).filter((x) => x.count > 0) : [];
  const first = profile?.patch_list.find((x) => x.stack === stacks[0]?.name)?.name;
  const most = Math.max(1, ...rig.library.profiles.flatMap((p) => p.stacks.map((st) => p.patch_list.filter((x) => x.stack === st).length)));
  return (
    <button
      className={on ? "" : "pressable"}
      onClick={onClick}
      aria-pressed={on}
      style={{
        width: "100%",
        minHeight: 48,
        padding: "4px 16px",
        display: "flex",
        alignItems: "center",
        gap: 12,
        textAlign: "left",
        background: on ? "var(--focus-bg)" : "transparent",
        borderBottom: inherit ? undefined : "1px solid var(--rule)",
      }}
    >
      <span style={{ width: 22, display: "flex", justifyContent: "center", flexShrink: 0 }}>
        {inherit ? (
          // Follows the set: an arrow coming down from above.
          <svg width="18" height="18" viewBox="0 0 16 16" aria-hidden style={{ color: "var(--ink-3)" }}>
            <path d="M4 2v5.5a2.5 2.5 0 0 0 2.5 2.5H13" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            <path d="M10 7l3 3-3 3" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : (
          <ProfileIcon name={profile?.name} colour={colour ?? "var(--ink-3)"} size={18} />
        )}
      </span>
      <span style={{ flexShrink: 0, fontSize: 15, fontWeight: on ? 700 : 600, fontStyle: inherit ? "italic" : undefined, color: on ? "var(--ink)" : "var(--ink-2)", whiteSpace: "nowrap" }}>{name}</span>
      {/* What it opens on: the first patch of its first stack. */}
      <span className="t-meta" style={{ flex: 1, minWidth: 0, textAlign: "right", fontSize: 11.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
        {[of, first].filter(Boolean).join(" · ")}
      </span>
      {on && (
        <svg width="16" height="16" viewBox="0 0 14 14" aria-label="picked" style={{ color: "var(--ink)", flexShrink: 0 }}>
          <path d="M2.5 7.5 5.5 10.5 11.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
      {/* One column per stack, one block per patch: count them. */}
      <span aria-label={stacks.map((x) => `${x.name} ${x.count}`).join(", ")} style={{ display: "flex", alignItems: "flex-end", justifyContent: "flex-end", gap: 3, width: 45, height: most * 5, flexShrink: 0 }}>
        {stacks.map((x) => {
          const tape = tapeFor(x.name) === "var(--tape-gaffer)" ? "var(--ink-3)" : tapeFor(x.name);
          return (
            <span key={x.name} title={`${x.name} · ${x.count}`} style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              {Array.from({ length: x.count }, (_, k) => (
                <span key={k} style={{ width: 6, height: 4, borderRadius: 1, background: tape, opacity: on ? 1 : 0.75 }} />
              ))}
            </span>
          );
        })}
      </span>
    </button>
  );
}

function Cell({ title, on, onClick, mark, hint }: { title: string; on: boolean; onClick: () => void; mark?: ReactNode; hint?: string }) {
  return (
    <button
      title={hint}
      className={on ? "" : "pressable"}
      onClick={onClick}
      style={{
        minHeight: 48,
        padding: "6px 12px",
        textAlign: "left",
        borderRadius: "var(--r)",
        background: on ? "var(--focus-bg)" : "var(--sheet)",
        border: on ? "1.5px solid var(--focus-fg)" : "1px solid var(--rule-strong)",
        fontSize: 15,
        fontWeight: on ? 700 : 560,
        display: "flex",
        alignItems: "center",
        gap: 8,
      }}
    >
      <span style={{ flex: 1, minWidth: 0 }}>{title}</span>
      {mark}
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
                <span className="t-label" style={{ display: "block", fontSize: 11, color: "var(--ink-3)" }}>
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
