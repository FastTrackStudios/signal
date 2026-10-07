// Set: managing a setlist by touch. On the left the set — its name with ‹ ›
// to step sets and ⋯ to manage it, the songs in order (the one up marked
// green, the played ones struck), each with ⋯ (or a long-press) for its key,
// tempo, what it starts on, its place in the set. On the right the song up:
// its key and tempo, its sections (each with ⋯ to rename, move, delete), and
// the sound the tapped section plays.

import { useRef, useState } from "react";
import { rig, stackOf, recipe } from "../data/rig";
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
  renameSet,
  sectionsOf,
  setSectionSound,
  setSongField,
  setStart,
  useStore,
  type Sound,
} from "../store";
import { Circle, Strike, Tape, tapeFor } from "../ui/marks";
import { Button, KeyBox, SideSheet, Tabs } from "../ui/kit";
import { Menu, MoreButton, useMenu, type MenuItem, type Picked } from "../ui/Menu";

const ROW_H = 72;
const KEYS = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

export function SetView() {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(340px, 40%) 1fr", gap: 12, padding: 12, height: "100%", minHeight: 0 }}>
      <SetSheet />
      <SongPage />
    </div>
  );
}

type Sheet =
  | null
  | { kind: "sets" }
  | { kind: "add" }
  | { kind: "key"; index: number }
  | { kind: "tempo"; index: number }
  | { kind: "start"; index: number };

// ── The set ──────────────────────────────────────────────────────────

function SetSheet() {
  const s = useStore();
  const set = currentSet(s);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const setMenu = useMenu();
  const names = s.setlists.map((l) => l.name);

  const setItems: MenuItem[] = [
    { kind: "head", label: `Set · ${set.name}` },
    { kind: "name", id: "new", label: "New set…", initial: "", confirm: "Create", taken: names },
    { kind: "name", id: "rename", label: "Rename…", initial: set.name, confirm: "Rename", taken: names },
    { kind: "name", id: "duplicate", label: "Duplicate…", initial: `${set.name} copy`, confirm: "Duplicate", taken: names },
    { kind: "run", id: "sets", label: "All sets…", detail: `${s.setlists.length}` },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete set", disabled: s.setlists.length <= 1 ? "The only set — make another first" : undefined },
  ];
  const onSet = (p: Picked) => {
    if (p.id === "new") newSetlist(p.text);
    if (p.id === "rename") renameSet(s.setIndex, p.text);
    if (p.id === "duplicate") duplicateSet(s.setIndex, p.text);
    if (p.id === "sets") setSheet({ kind: "sets" });
    if (p.id === "delete") deleteSet(s.setIndex);
  };

  const onHandleDown = (i: number) => (e: React.PointerEvent) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const top = listRef.current?.getBoundingClientRect().top ?? 0;
    const scroll = listRef.current?.scrollTop ?? 0;
    const at = (y: number) => Math.max(0, Math.min(set.songs.length - 1, Math.floor((y - top + scroll) / ROW_H)));
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

  const step = (d: number) => chooseSet((s.setIndex + d + s.setlists.length) % s.setlists.length);

  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
      <header style={{ display: "flex", alignItems: "center", gap: 4, padding: "12px 8px 12px 18px", borderBottom: "1px solid var(--rule)" }}>
        <button onClick={() => setSheet({ kind: "sets" })} style={{ flex: 1, minWidth: 0, textAlign: "left" }} title="All sets">
          <h1 className="t-marker" style={{ margin: 0, fontSize: 24 }}>
            {set.name}
          </h1>
          <div className="t-meta" style={{ marginTop: 4 }}>
            {set.songs.length} songs · on {rig.perf.profile_name}
          </div>
        </button>
        <IconBtn label="Previous set" onClick={() => step(-1)}>
          <Chevron dir="left" />
        </IconBtn>
        <IconBtn label="Next set" onClick={() => step(1)}>
          <Chevron dir="right" />
        </IconBtn>
        <MoreButton label="Set actions" onClick={setMenu.fromButton} />
      </header>
      <div ref={listRef} className="ruled" style={{ flex: 1, minHeight: 0, overflowY: "auto", position: "relative" }}>
        {set.songs.map((song, i) => (
          <SongRow
            key={`${i}-${song.name}`}
            index={i}
            onSheet={setSheet}
            onHandleDown={onHandleDown(i)}
            dropAbove={!!drag && drag.to === i && drag.from !== i}
            lifted={drag?.from === i}
          />
        ))}
        {set.songs.length === 0 && (
          <div style={{ padding: 24, display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 10 }}>
            <div style={{ fontSize: 18, fontWeight: 700 }}>No songs yet</div>
            <p className="t-meta" style={{ margin: 0 }}>
              Add the songs you're playing, in order. Each keeps its own key, tempo and sounds for this set.
            </p>
            <Button primary onClick={() => setSheet({ kind: "add" })}>
              Add songs
            </Button>
          </div>
        )}
      </div>
      <footer style={{ display: "flex", gap: 8, padding: 10, borderTop: "1px solid var(--rule)" }}>
        <Button primary onClick={() => setSheet({ kind: "add" })} style={{ flex: 1 }}>
          Add songs
        </Button>
        <Button onClick={() => setSheet({ kind: "sets" })}>All sets</Button>
      </footer>
      {setMenu.open && <Menu at={setMenu.open.at} items={setItems} onPick={onSet} onClose={setMenu.close} />}
      {sheet?.kind === "sets" && <SetsSheet onClose={() => setSheet(null)} />}
      {sheet?.kind === "add" && <AddSongsSheet onClose={() => setSheet(null)} />}
      {sheet?.kind === "key" && <KeySheet index={sheet.index} onClose={() => setSheet(null)} />}
      {sheet?.kind === "tempo" && <TempoSheet index={sheet.index} onClose={() => setSheet(null)} />}
      {sheet?.kind === "start" && <StartSheet index={sheet.index} onClose={() => setSheet(null)} />}
    </section>
  );
}

function SongRow({
  index: i,
  onSheet,
  onHandleDown,
  dropAbove,
  lifted,
}: {
  index: number;
  onSheet: (s: Sheet) => void;
  onHandleDown: (e: React.PointerEvent) => void;
  dropAbove: boolean;
  lifted: boolean;
}) {
  const s = useStore();
  const set = currentSet(s);
  const song = set.songs[i];
  const menu = useMenu();
  const played = i < s.songIndex;
  const up = i === s.songIndex;
  const last = set.songs.length - 1;
  const items: MenuItem[] = [
    { kind: "head", label: `Song ${i + 1} · ${song.name}` },
    { kind: "run", id: "open", label: "Go to this song", disabled: up ? "It's up now" : undefined },
    { kind: "sep" },
    { kind: "run", id: "key", label: "Key…", detail: song.key || "—" },
    { kind: "run", id: "tempo", label: "Tempo…", detail: song.bpm ? `${song.bpm} BPM` : "—" },
    { kind: "run", id: "start", label: "Starts on…", detail: song.start || "profile default" },
    { kind: "sep" },
    { kind: "run", id: "up", label: "Move up", disabled: i === 0 ? "Already first" : undefined },
    { kind: "run", id: "down", label: "Move down", disabled: i === last ? "Already last" : undefined },
    { kind: "run", id: "top", label: "Move to the top", disabled: i === 0 ? "Already first" : undefined },
    { kind: "run", id: "bottom", label: "Move to the end", disabled: i === last ? "Already last" : undefined },
    { kind: "sep" },
    { kind: "delete", id: "remove", label: "Remove from this set" },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "open") goToSong(i);
    if (p.id === "key") onSheet({ kind: "key", index: i });
    if (p.id === "tempo") onSheet({ kind: "tempo", index: i });
    if (p.id === "start") onSheet({ kind: "start", index: i });
    if (p.id === "up") moveSong(i, i - 1);
    if (p.id === "down") moveSong(i, i + 1);
    if (p.id === "top") moveSong(i, 0);
    if (p.id === "bottom") moveSong(i, last);
    if (p.id === "remove") removeSong(i);
  };
  const startStack = song.start ? stackOf(song.start) : undefined;
  return (
    <div
      {...menu.longPress()}
      style={{
        height: ROW_H,
        display: "flex",
        alignItems: "center",
        background: lifted ? "var(--sheet-2)" : up ? "var(--up)" : undefined,
        boxShadow: dropAbove ? "inset 0 3px 0 var(--focus-fg)" : undefined,
      }}
    >
      <button
        onClick={() => goToSong(i)}
        className="pressable"
        style={{ flex: 1, minWidth: 0, height: "100%", display: "flex", alignItems: "center", gap: 12, padding: "0 4px 0 16px", textAlign: "left" }}
      >
        <span className="num" style={{ width: 24, textAlign: "center", fontSize: 16, fontWeight: 650, color: up ? "var(--live)" : "var(--ink-3)" }}>
          {i + 1}
        </span>
        <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 5 }}>
          <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
            <span
              style={{
                position: "relative",
                minWidth: 0,
                fontSize: 18,
                fontWeight: up ? 750 : 600,
                color: played ? "var(--ink-3)" : "var(--ink)",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {song.name}
              {played && <Strike width={2} />}
              {up && <Circle />}
            </span>
            {i === s.songIndex + 1 && (
              <span className="t-label" style={{ flexShrink: 0, fontSize: 11, color: "var(--ink-2)", padding: "2px 6px", borderRadius: 4, background: "rgba(255,255,255,0.07)" }}>
                Next
              </span>
            )}
          </span>
          {song.start && !played && (
            <span>
              <Tape colour={tapeFor(startStack)} style={{ fontSize: 12, padding: "2px 7px", letterSpacing: "0.04em", textTransform: "none", fontWeight: 600 }}>
                starts {song.start}
              </Tape>
            </span>
          )}
        </span>
        <KeyBox k={song.key} />
        <span className="num" style={{ width: 34, textAlign: "right", fontSize: 15, fontWeight: 600, color: "var(--ink-2)" }}>
          {song.bpm || ""}
        </span>
      </button>
      <MoreButton label={`${song.name} actions`} onClick={menu.fromButton} />
      <span
        onPointerDown={onHandleDown}
        role="button"
        aria-label={`Drag ${song.name}`}
        style={{ width: 40, height: "100%", display: "flex", alignItems: "center", justifyContent: "center", cursor: "grab", touchAction: "none", color: "var(--ink-3)" }}
      >
        <svg width="16" height="12" viewBox="0 0 16 12" aria-hidden>
          <path d="M1 1.5h14M1 6h14M1 10.5h14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      </span>
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}

function IconBtn({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button className="pressable" aria-label={label} title={label} onClick={onClick} style={{ width: 44, height: 48, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--r)", color: "var(--ink-2)" }}>
      {children}
    </button>
  );
}

function Chevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg width="10" height="16" viewBox="0 0 10 16" aria-hidden>
      <path d={dir === "left" ? "M8 2 L2 8 L8 14" : "M2 2 L8 8 L2 14"} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ── Sheets ───────────────────────────────────────────────────────────

function SetsSheet({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const [naming, setNaming] = useState("");
  const taken = s.setlists.some((l) => l.name.toLowerCase() === naming.trim().toLowerCase());
  return (
    <SideSheet title="Sets" sub={`${s.setlists.length} sets · tap one to open it`} onClose={onClose}>
      <div className="ruled">
        {s.setlists.map((l, i) => (
          <button
            key={`${i}-${l.name}`}
            className="pressable"
            onClick={() => {
              chooseSet(i);
              onClose();
            }}
            style={{ width: "100%", minHeight: 64, padding: "10px 22px", display: "flex", alignItems: "center", gap: 12, textAlign: "left", background: i === s.setIndex ? "var(--up)" : undefined }}
          >
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: "block", fontSize: 17, fontWeight: i === s.setIndex ? 700 : 560 }}>{l.name}</span>
              <span className="t-meta">
                {l.songs.length} songs{l.songs.length ? ` · ${l.songs.slice(0, 3).map((x) => x.name).join(", ")}${l.songs.length > 3 ? "…" : ""}` : ""}
              </span>
            </span>
            {i === s.setIndex && <span className="t-label" style={{ color: "var(--focus-fg)" }}>Open</span>}
          </button>
        ))}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: 18, borderTop: "1px solid var(--rule)" }}>
        <div style={{ display: "flex", gap: 8 }}>
          <input
            value={naming}
            onChange={(e) => setNaming(e.target.value)}
            placeholder="New set — e.g. Sunday 10-12"
            style={{ flex: 1, minWidth: 0, minHeight: 48, padding: "0 12px", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", fontSize: 16 }}
          />
          <Button
            primary
            disabled={!naming.trim() || taken}
            onClick={() => {
              newSetlist(naming.trim());
              setNaming("");
              onClose();
            }}
          >
            Create
          </Button>
        </div>
        {taken && <span style={{ color: "var(--void)", fontSize: 14 }}>There's a set called that already.</span>}
      </div>
    </SideSheet>
  );
}

function AddSongsSheet({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const set = currentSet(s);
  const [q, setQ] = useState("");
  const [making, setMaking] = useState(false);
  const [key, setKey] = useState("G");
  const [bpm, setBpm] = useState(72);
  const inSet = new Set(set.songs.map((x) => x.name));
  const all = [...rig.library.songs, ...s.newSongs];
  const songs = all.filter((x) => x.name.toLowerCase().includes(q.trim().toLowerCase()));
  const exists = all.some((x) => x.name.toLowerCase() === q.trim().toLowerCase());
  return (
    <SideSheet title="Add songs" sub={`To ${set.name} — tap to add, in order`} onClose={onClose} width={500}>
      <div style={{ padding: "14px 18px 10px", display: "flex", flexDirection: "column", gap: 10, borderBottom: "1px solid var(--rule)" }}>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Find a song, or type a new one"
          style={{ width: "100%", minHeight: 48, padding: "0 14px", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", fontSize: 17 }}
        />
        {q.trim() && !exists && !making && (
          <Button primary onClick={() => setMaking(true)}>
            New song “{q.trim()}”…
          </Button>
        )}
        {making && (
          <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: 12, border: "1px solid var(--rule-strong)", borderRadius: "var(--r-md)" }}>
            <div style={{ fontSize: 17, fontWeight: 700 }}>{q.trim()}</div>
            <KeyGrid value={key} onPick={setKey} />
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span className="t-meta" style={{ flex: 1 }}>
                Tempo
              </span>
              <Stepper value={bpm} unit="BPM" onChange={setBpm} />
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
      <div className="ruled">
        {songs.map((song) => {
          const added = inSet.has(song.name);
          return (
            <button
              key={song.name}
              className="pressable"
              disabled={added}
              onClick={() => addSong({ name: song.name, key: song.key, bpm: song.bpm, start: "" })}
              style={{ width: "100%", minHeight: 60, padding: "8px 18px 8px 22px", display: "flex", alignItems: "center", gap: 12, textAlign: "left", opacity: added ? 0.5 : 1 }}
            >
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: "block", fontSize: 17, fontWeight: 650 }}>{song.name}</span>
                {song.setlists.length > 0 && (
                  <span className="t-meta" style={{ fontSize: 13 }}>
                    In {song.setlists.length === 1 ? song.setlists[0] : `${song.setlists.length} sets`}
                  </span>
                )}
              </span>
              <KeyBox k={song.key} />
              <span className="num" style={{ width: 34, textAlign: "right", color: "var(--ink-2)" }}>
                {song.bpm || ""}
              </span>
              <span className="t-label" style={{ width: 54, textAlign: "right", color: added ? "var(--ink-3)" : "var(--focus-fg)" }}>
                {added ? "In set" : "Add"}
              </span>
            </button>
          );
        })}
      </div>
    </SideSheet>
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
                background: on ? "var(--focus-bg)" : "transparent",
                fontSize: 17,
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

function KeySheet({ index, onClose }: { index: number; onClose: () => void }) {
  const s = useStore();
  const song = currentSet(s).songs[index];
  return (
    <SideSheet title={`${song.name}: key`} sub="For this set — the song's own key stays as it is" onClose={onClose}>
      <div style={{ padding: 18 }}>
        <KeyGrid value={song.key || "C"} onPick={(k) => setSongField(index, "key", k)} />
      </div>
    </SideSheet>
  );
}

function TempoSheet({ index, onClose }: { index: number; onClose: () => void }) {
  const s = useStore();
  const song = currentSet(s).songs[index];
  return (
    <SideSheet title={`${song.name}: tempo`} sub="For this set" onClose={onClose}>
      <div style={{ padding: 22, display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
        <div className="num" style={{ fontSize: 64, fontWeight: 750, lineHeight: 1 }}>
          {song.bpm || "—"}
          <span className="t-meta" style={{ fontSize: 16, marginLeft: 8 }}>
            BPM
          </span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 8, width: "100%" }}>
          {[-5, -1, 1, 5].map((d) => (
            <Button key={d} onClick={() => setSongField(index, "bpm", Math.max(30, (song.bpm || 100) + d))} style={{ minHeight: 56, fontSize: 18 }}>
              {d > 0 ? `+${d}` : `−${-d}`}
            </Button>
          ))}
        </div>
      </div>
    </SideSheet>
  );
}

function StartSheet({ index, onClose }: { index: number; onClose: () => void }) {
  const s = useStore();
  const song = currentSet(s).songs[index];
  return (
    <SideSheet title={`${song.name}: starts on`} sub="The patch it opens on when the set reaches it" onClose={onClose} width={520}>
      <div style={{ padding: "12px 18px 18px", display: "flex", flexDirection: "column", gap: 14 }}>
        <PickCell title="The profile's default" sub="Whatever the profile lands on" on={!song.start} onClick={() => setStart(index, "")} />
        {rig.perf.stacks.map((st) => (
          <div key={st.name}>
            <Tape colour={tapeFor(st.name)} style={{ marginBottom: 8 }}>
              {st.name}
            </Tape>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 8 }}>
              {st.patches.map((p) => (
                <PickCell key={p} title={p} on={song.start === p} onClick={() => setStart(index, p)} />
              ))}
            </div>
          </div>
        ))}
      </div>
    </SideSheet>
  );
}

// ── The song up ──────────────────────────────────────────────────────

function SongPage() {
  const s = useStore();
  const set = currentSet(s);
  const song = set.songs[s.songIndex];
  const [editing, setEditing] = useState<number | null>(null);
  const [keyOpen, setKeyOpen] = useState(false);
  const addMenu = useMenu();
  if (!song) {
    return (
      <section className="sheet" style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <span className="t-meta">Pick a song on the left.</span>
      </section>
    );
  }
  const sections = sectionsOf(s, song.name);
  const sel = editing !== null && editing < sections.length ? editing : s.partIndex;
  const selected = sections[sel];
  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
      <header style={{ display: "flex", alignItems: "center", gap: 12, padding: "14px 18px", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h2 className="t-marker" style={{ margin: 0, fontSize: 32, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {song.name}
          </h2>
          <div className="t-meta" style={{ marginTop: 4 }}>
            Song {s.songIndex + 1} of {set.songs.length}
            {song.start ? ` · starts ${song.start}` : ""}
          </div>
        </div>
        <button className="pressable" onClick={() => setKeyOpen(true)} title="Key for this set" style={{ borderRadius: "var(--r)" }}>
          <KeyBox k={song.key || "—"} big />
        </button>
        <Stepper value={song.bpm} unit="BPM" onChange={(v) => setSongField(s.songIndex, "bpm", v)} />
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <div style={{ padding: "14px 18px 6px", display: "flex", alignItems: "baseline", gap: 12 }}>
          <h3 style={{ margin: 0, fontSize: 17, fontWeight: 700 }}>Sections</h3>
          <span className="t-meta">Tap one to go there and give it a sound</span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(168px, 1fr))", gap: 8, padding: "6px 18px 16px" }}>
          {sections.map((sec, i) => (
            <SectionTile
              key={`${i}-${sec.name}`}
              song={song.name}
              index={i}
              count={sections.length}
              now={i === s.partIndex}
              on={i === sel}
              onTap={() => {
                goToPart(i);
                setEditing(i);
              }}
            />
          ))}
          <button
            className="pressable"
            onClick={addMenu.fromButton}
            style={{ minHeight: 76, border: "1px dashed var(--rule-strong)", borderRadius: "var(--r-md)", fontWeight: 600, color: "var(--ink-2)" }}
          >
            + Section
          </button>
        </div>
        {addMenu.open && (
          <Menu
            at={addMenu.open.at}
            naming={0}
            items={[{ kind: "name", id: "add", label: "New section…", initial: suggestSection(sections.map((x) => x.name)), confirm: "Add", taken: sections.map((x) => x.name) }]}
            onPick={(p) => addSection(song.name, p.text)}
            onClose={addMenu.close}
          />
        )}
        {selected ? (
          <SoundPicker key={`${song.name}-${sel}`} sectionName={selected.name} current={selected.sound} onPick={(sound) => setSectionSound(song.name, sel, sound)} />
        ) : (
          <p className="t-meta" style={{ padding: "0 18px" }}>
            No sections yet. Add the song's form — Verse, Chorus, Bridge — and give each its sound.
          </p>
        )}
      </div>
      {keyOpen && <KeySheet index={s.songIndex} onClose={() => setKeyOpen(false)} />}
    </section>
  );
}

/** The next section name a song's form suggests (Verse 1 → Chorus 1 → …). */
function suggestSection(names: string[]): string {
  const count = (base: string) => names.filter((n) => n.toLowerCase().startsWith(base.toLowerCase())).length;
  if (names.length === 0) return "Intro";
  const verses = count("Verse");
  const choruses = count("Chorus");
  return verses <= choruses ? `Verse ${verses + 1}` : `Chorus ${choruses + 1}`;
}

function SectionTile({ song, index: i, count, now, on, onTap }: { song: string; index: number; count: number; now: boolean; on: boolean; onTap: () => void }) {
  const s = useStore();
  const sec = sectionsOf(s, song)[i];
  const menu = useMenu();
  const stack = sec.sound?.kind === "patch" ? stackOf(sec.sound.name) : undefined;
  const band = sec.sound ? tapeFor(stack) : "var(--rule-strong)";
  const names = sectionsOf(s, song).map((x) => x.name);
  const items: MenuItem[] = [
    { kind: "head", label: sec.name },
    { kind: "name", id: "rename", label: "Rename…", initial: sec.name, confirm: "Rename", taken: names },
    { kind: "run", id: "earlier", label: "Move earlier", disabled: i === 0 ? "Already first" : undefined },
    { kind: "run", id: "later", label: "Move later", disabled: i === count - 1 ? "Already last" : undefined },
    { kind: "run", id: "clear", label: "Clear its sound", disabled: sec.sound ? undefined : "It keeps what plays already" },
    { kind: "sep" },
    { kind: "delete", id: "delete", label: "Delete section" },
  ];
  const onPick = (p: Picked) => {
    if (p.id === "rename") renameSection(song, i, p.text);
    if (p.id === "earlier") moveSection(song, i, i - 1);
    if (p.id === "later") moveSection(song, i, i + 1);
    if (p.id === "clear") setSectionSound(song, i, null);
    if (p.id === "delete") removeSection(song, i);
  };
  return (
    <div
      {...menu.longPress()}
      style={{
        position: "relative",
        display: "flex",
        minHeight: 76,
        background: now ? "var(--live-bg)" : on ? "var(--focus-bg)" : "var(--sheet-2)",
        border: on ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)",
        borderRadius: "var(--r-md)",
        overflow: "hidden",
      }}
    >
      <span style={{ position: "absolute", left: 0, right: 0, top: 0, height: 4, background: band }} />
      <button onClick={onTap} className="pressable" style={{ flex: 1, minWidth: 0, textAlign: "left", padding: "12px 4px 10px 12px", display: "flex", flexDirection: "column", gap: 4 }}>
        <span style={{ position: "relative", alignSelf: "flex-start", maxWidth: "100%", fontSize: 16, fontWeight: 700, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {sec.name}
          {now && <Circle />}
        </span>
        <span className="t-meta" style={{ fontSize: 13, color: sec.sound ? "var(--ink-2)" : "var(--ink-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {sec.sound ? sec.sound.name : "keeps what plays"}
        </span>
      </button>
      <MoreButton label={`${sec.name} actions`} onClick={menu.fromButton} />
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </div>
  );
}

function Stepper({ value, unit, onChange }: { value: number; unit: string; onChange: (v: number) => void }) {
  return (
    <div style={{ display: "flex", alignItems: "center", border: "1px solid var(--rule-strong)", borderRadius: "var(--r)", height: 48 }}>
      <button className="pressable" aria-label={`Lower ${unit}`} onClick={() => onChange(Math.max(30, value - 1))} style={{ width: 44, height: "100%", display: "flex", alignItems: "center", justifyContent: "center" }}>
        <svg width="14" height="2" viewBox="0 0 14 2" aria-hidden>
          <path d="M1 1h12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      </button>
      <span style={{ minWidth: 60, textAlign: "center", lineHeight: 1 }}>
        <span className="num" style={{ display: "block", fontSize: 20, fontWeight: 700 }}>
          {value || "—"}
        </span>
        <span className="t-label" style={{ fontSize: 11, color: "var(--ink-3)" }}>
          {unit}
        </span>
      </span>
      <button className="pressable" aria-label={`Raise ${unit}`} onClick={() => onChange(value + 1)} style={{ width: 44, height: "100%", display: "flex", alignItems: "center", justifyContent: "center" }}>
        <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
          <path d="M1 7h12M7 1v12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}

// ── Picking a section's sound ────────────────────────────────────────

function SoundPicker({ sectionName, current, onPick }: { sectionName: string; current: Sound | null; onPick: (s: Sound | null) => void }) {
  const [tab, setTab] = useState<"patches" | "presets">(current?.kind === "preset" ? "presets" : "patches");
  const s = useStore();
  const presets = s.presets.filter((p) => !p.cancelled);
  return (
    <div style={{ borderTop: "1px solid var(--rule)", background: "rgba(0,0,0,0.18)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "12px 18px 8px", flexWrap: "wrap" }}>
        <h3 style={{ margin: 0, fontSize: 17, fontWeight: 700 }}>{sectionName} plays</h3>
        <Tabs
          options={[
            { id: "patches", label: "Patches", count: rig.patches.length },
            { id: "presets", label: "Presets", count: presets.length },
          ]}
          value={tab}
          onChange={setTab}
        />
        <span style={{ flex: 1 }} />
        <Button onClick={() => onPick(null)} disabled={!current} title="This section stops changing the sound: it keeps whatever is playing">
          Clear its sound
        </Button>
      </div>
      {tab === "patches" ? (
        <div style={{ padding: "4px 18px 18px", display: "flex", flexDirection: "column", gap: 12 }}>
          {rig.perf.stacks.map((st) => (
            <div key={st.name}>
              <Tape colour={tapeFor(st.name)} style={{ marginBottom: 8 }}>
                {st.name}
              </Tape>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))", gap: 8 }}>
                {st.patches.map((p) => {
                  const on = current?.kind === "patch" && current.name === p;
                  return <PickCell key={p} on={on} title={p} sub={on ? "Plays here" : undefined} onClick={() => onPick({ kind: "patch", name: p })} />;
                })}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div style={{ padding: "4px 18px 18px", display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(190px, 1fr))", gap: 8 }}>
          {presets.map((p) => {
            const r = recipe(p.source);
            const core = r?.modules.find((m) => m.module === "Core");
            const time = r?.modules.find((m) => m.module === "Time");
            const on = current?.kind === "preset" && current.name === p.name;
            return (
              <PickCell
                key={p.name}
                on={on}
                title={p.name}
                sub={[core && `${core.snapshot}`, time && `${time.preset} ${time.snapshot}`].filter(Boolean).join(" · ")}
                onClick={() => onPick({ kind: "preset", name: p.name })}
              />
            );
          })}
          {presets.length === 0 && <span className="t-meta">No presets yet — make them in Sounds.</span>}
        </div>
      )}
    </div>
  );
}

function PickCell({ title, sub, on, onClick }: { title: string; sub?: string; on: boolean; onClick: () => void }) {
  return (
    <button
      className={on ? "" : "pressable"}
      onClick={onClick}
      style={{
        minHeight: 56,
        padding: "8px 12px",
        textAlign: "left",
        background: on ? "var(--focus-bg)" : "var(--sheet-2)",
        border: on ? "2px solid var(--focus-fg)" : "1px solid var(--rule-strong)",
        borderRadius: "var(--r)",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap: 3,
      }}
    >
      <span style={{ fontSize: 15, fontWeight: on ? 700 : 600 }}>{title}</span>
      {sub && (
        <span className="t-meta" style={{ fontSize: 13 }}>
          {sub}
        </span>
      )}
    </button>
  );
}
