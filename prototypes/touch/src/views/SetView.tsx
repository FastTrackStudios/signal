// Set: tonight's sheet on the left — songs struck as they're played, the one
// up circled — and the song up on the right: its sections as taped tiles, and
// under them the sound the tapped section plays, picked from big taped lists.

import { useRef, useState } from "react";
import { rig, stackOf, modulesOf, recipe } from "../data/rig";
import {
  addSection,
  addSong,
  chooseSet,
  currentSet,
  goToPart,
  goToSong,
  moveSong,
  newSetlist,
  removeSong,
  sectionsOf,
  setSectionSound,
  setSongField,
  useStore,
  type Sound,
} from "../store";
import { Circle, Strike, Tape, tapeFor } from "../ui/marks";
import { Button, KeyBox, SideSheet, Tabs } from "../ui/kit";

const ROW_H = 76;

export function SetView() {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(320px, 38%) 1fr", gap: 14, padding: 14, height: "100%", minHeight: 0 }}>
      <SetSheet />
      <SongPage />
    </div>
  );
}

// ── The set ──────────────────────────────────────────────────────────

function SetSheet() {
  const s = useStore();
  const set = currentSet(s);
  const [sheet, setSheet] = useState<null | "sets" | "add">(null);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const onHandleDown = (i: number) => (e: React.PointerEvent) => {
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

  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
      <header style={{ padding: "18px 14px 14px 20px", borderBottom: "2px solid var(--ink)", display: "flex", gap: 10, alignItems: "flex-start" }}>
        <button onClick={() => setSheet("sets")} style={{ flex: 1, minWidth: 0, textAlign: "left" }} title="Choose a set">
          <h1 className="t-marker" style={{ margin: 0, fontSize: 30 }}>
            {set.name}
          </h1>
          <div className="t-meta" style={{ marginTop: 6 }}>
            {set.songs.length} songs · on {rig.perf.profile_name} · tap for other sets
          </div>
        </button>
      </header>
      <div ref={listRef} className="ruled" style={{ flex: 1, minHeight: 0, overflowY: "auto", position: "relative" }}>
        {set.songs.map((song, i) => {
          const played = i < s.songIndex;
          const up = i === s.songIndex;
          const lifted = drag?.from === i;
          const startTape = song.start ? tapeFor(stackOf(song.start)) : null;
          return (
            <div
              key={`${i}-${song.name}`}
              style={{
                height: ROW_H,
                display: "flex",
                alignItems: "center",
                background: lifted ? "var(--sheet-2)" : up ? "#fffbe0" : undefined,
                boxShadow: drag && drag.to === i && drag.from !== i ? "inset 0 3px 0 var(--ink)" : undefined,
              }}
            >
              <button
                onClick={() => goToSong(i)}
                className="pressable"
                style={{ flex: 1, minWidth: 0, height: "100%", display: "flex", alignItems: "center", gap: 12, padding: "0 6px 0 16px", textAlign: "left" }}
              >
                <span className="num" style={{ position: "relative", width: 28, textAlign: "center", fontSize: 18, fontWeight: 760, color: played ? "var(--ink-3)" : "var(--ink)" }}>
                  {i + 1}
                  {up && <Circle inset={-9} />}
                </span>
                <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 5 }}>
                  <span
                    style={{
                      position: "relative",
                      alignSelf: "flex-start",
                      maxWidth: "100%",
                      fontSize: 20,
                      fontWeight: up ? 860 : 720,
                      fontStretch: "104%",
                      color: played ? "var(--ink-3)" : "var(--ink)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {song.name}
                    {played && <Strike />}
                  </span>
                  {startTape && !played && (
                    <Tape colour={startTape} style={{ alignSelf: "flex-start", fontSize: 11, padding: "2px 7px" }}>
                      starts {song.start}
                    </Tape>
                  )}
                </span>
                {i === s.songIndex + 1 && <span className="t-label" style={{ color: "var(--ink-2)" }}>Next</span>}
                <KeyBox k={song.key} />
                <span className="num" style={{ width: 38, textAlign: "right", fontSize: 17, fontWeight: 650, color: "var(--ink-2)" }}>
                  {song.bpm || ""}
                </span>
              </button>
              <span
                onPointerDown={onHandleDown(i)}
                role="button"
                aria-label={`Move ${song.name}`}
                style={{ width: 44, height: "100%", display: "flex", alignItems: "center", justifyContent: "center", cursor: "grab", touchAction: "none", color: "var(--ink-3)" }}
              >
                <svg width="18" height="14" viewBox="0 0 18 14" aria-hidden>
                  <path d="M1 2h16M1 7h16M1 12h16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                </svg>
              </span>
            </div>
          );
        })}
        {set.songs.length === 0 && (
          <div style={{ padding: 24 }}>
            <div className="t-marker" style={{ fontSize: 22 }}>An empty set</div>
            <p className="t-meta" style={{ margin: "8px 0 16px" }}>Add the songs you're playing, in order. Each keeps its own key, tempo and sounds.</p>
            <Button primary onClick={() => setSheet("add")}>Add songs</Button>
          </div>
        )}
      </div>
      <footer style={{ display: "flex", gap: 8, padding: 12, borderTop: "1px solid var(--rule)" }}>
        <Button primary onClick={() => setSheet("add")} style={{ flex: 1 }}>
          Add songs
        </Button>
        <Button onClick={() => removeSong(s.songIndex)} disabled={set.songs.length === 0} title="Take the song up out of the set (Undo brings it back)">
          Remove
        </Button>
      </footer>
      {sheet === "sets" && <SetsSheet onClose={() => setSheet(null)} />}
      {sheet === "add" && <AddSongsSheet onClose={() => setSheet(null)} />}
    </section>
  );
}

function SetsSheet({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const [naming, setNaming] = useState("");
  return (
    <SideSheet title="Sets" sub={`${s.setlists.length} sets`} onClose={onClose}>
      <div className="ruled">
        {s.setlists.map((l, i) => (
          <button
            key={l.name}
            className="pressable"
            onClick={() => {
              chooseSet(i);
              onClose();
            }}
            style={{ width: "100%", minHeight: 64, padding: "10px 22px", display: "flex", alignItems: "center", gap: 12, textAlign: "left" }}
          >
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: "block", fontSize: 18, fontWeight: i === s.setIndex ? 860 : 650 }}>{l.name}</span>
              <span className="t-meta">{l.songs.length} songs</span>
            </span>
            {i === s.setIndex && <Tape colour="var(--tape-gaffer)">Open</Tape>}
          </button>
        ))}
      </div>
      <div style={{ display: "flex", gap: 8, padding: 18, borderTop: "1px solid var(--rule)" }}>
        <input
          value={naming}
          onChange={(e) => setNaming(e.target.value)}
          placeholder="New set — e.g. Sunday 10-12"
          style={{ flex: 1, minWidth: 0, minHeight: 48, padding: "0 12px", border: "2px solid var(--ink)", borderRadius: "var(--r)", fontSize: 16 }}
        />
        <Button
          primary
          disabled={!naming.trim()}
          onClick={() => {
            newSetlist(naming.trim());
            setNaming("");
            onClose();
          }}
        >
          Create
        </Button>
      </div>
    </SideSheet>
  );
}

function AddSongsSheet({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const set = currentSet(s);
  const [q, setQ] = useState("");
  const inSet = new Set(set.songs.map((x) => x.name));
  const songs = rig.library.songs.filter((x) => x.name.toLowerCase().includes(q.toLowerCase()));
  return (
    <SideSheet title="Add songs" sub={`To ${set.name}`} onClose={onClose}>
      <div style={{ padding: "14px 18px 6px" }}>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Find a song"
          style={{ width: "100%", minHeight: 48, padding: "0 14px", border: "2px solid var(--ink)", borderRadius: "var(--r)", fontSize: 17 }}
        />
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
              style={{ width: "100%", minHeight: 60, padding: "8px 22px", display: "flex", alignItems: "center", gap: 12, textAlign: "left", opacity: added ? 0.5 : 1 }}
            >
              <span style={{ flex: 1, fontSize: 18, fontWeight: 700 }}>{song.name}</span>
              <KeyBox k={song.key} />
              <span className="num" style={{ width: 36, textAlign: "right", color: "var(--ink-2)" }}>
                {song.bpm || ""}
              </span>
              <span className="t-label" style={{ width: 52, textAlign: "right" }}>
                {added ? "In set" : "Add"}
              </span>
            </button>
          );
        })}
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
  if (!song) {
    return (
      <section className="sheet" style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <span className="t-meta">Pick a song on the left.</span>
      </section>
    );
  }
  const sections = sectionsOf(s, song.name);
  const sel = editing ?? s.partIndex;
  const selected = sections[sel];
  return (
    <section className="sheet" style={{ display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
      <header style={{ display: "flex", alignItems: "flex-end", gap: 18, padding: "18px 22px 16px", borderBottom: "1px solid var(--rule)" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h2 className="t-marker" style={{ margin: 0, fontSize: 44 }}>
            {song.name}
          </h2>
          <div className="t-meta" style={{ marginTop: 6 }}>
            Song {s.songIndex + 1} of {set.songs.length} · on {rig.perf.profile_name}
          </div>
        </div>
        <KeyBox k={song.key} big />
        <Stepper
          value={song.bpm}
          unit="BPM"
          onChange={(v) => setSongField(s.songIndex, "bpm", v)}
        />
      </header>
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <div style={{ padding: "16px 22px 8px", display: "flex", alignItems: "baseline", gap: 12 }}>
          <h3 style={{ margin: 0, fontSize: 18, fontWeight: 820 }}>Sections</h3>
          <span className="t-meta">Tap one to go there and give it a sound</span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 10, padding: "6px 22px 18px" }}>
          {sections.map((sec, i) => {
            const now = i === s.partIndex;
            const on = i === sel;
            const tape = sec.sound ? tapeFor(sec.sound.kind === "patch" ? stackOf(sec.sound.name) : undefined) : "var(--rule-strong)";
            return (
              <button
                key={`${i}-${sec.name}`}
                className="pressable"
                onClick={() => {
                  goToPart(i);
                  setEditing(i);
                }}
                style={{
                  position: "relative",
                  minHeight: 84,
                  textAlign: "left",
                  background: "var(--sheet)",
                  border: on ? "3px solid var(--ink)" : "1px solid var(--rule-strong)",
                  borderRadius: "var(--r)",
                  padding: on ? "0 10px 10px" : "2px 12px 12px",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <span style={{ height: 10, margin: on ? "0 -10px 4px" : "-2px -12px 4px", background: tape }} />
                <span style={{ position: "relative", alignSelf: "flex-start", fontSize: 18, fontWeight: 840 }}>
                  {sec.name}
                  {now && <Circle inset={-7} />}
                </span>
                <span className="t-meta" style={{ fontSize: 14, color: sec.sound ? "var(--ink-2)" : "var(--ink-3)" }}>
                  {sec.sound ? sec.sound.name : "keeps what plays"}
                </span>
              </button>
            );
          })}
          <button
            className="pressable"
            onClick={() => addSection(song.name, `Section ${sections.length + 1}`)}
            style={{ minHeight: 84, border: "2px dashed var(--rule-strong)", borderRadius: "var(--r)", fontWeight: 760, color: "var(--ink-2)" }}
          >
            + Section
          </button>
        </div>
        {selected && (
          <SoundPicker
            key={`${song.name}-${sel}`}
            sectionName={selected.name}
            current={selected.sound}
            onPick={(sound) => setSectionSound(song.name, sel, sound)}
          />
        )}
        {sections.length === 0 && (
          <p className="t-meta" style={{ padding: "0 22px" }}>
            No sections yet. Add the song's form (Verse, Chorus, Bridge…) and give each its sound.
          </p>
        )}
      </div>
    </section>
  );
}

function Stepper({ value, unit, onChange }: { value: number; unit: string; onChange: (v: number) => void }) {
  return (
    <div style={{ display: "flex", alignItems: "center", border: "2px solid var(--ink)", borderRadius: 2, height: 48 }}>
      <button aria-label={`Lower ${unit}`} onClick={() => onChange(Math.max(30, value - 1))} style={{ width: 44, height: "100%", fontSize: 22, fontWeight: 700 }}>
        −
      </button>
      <span style={{ minWidth: 64, textAlign: "center", lineHeight: 1 }}>
        <span className="num" style={{ display: "block", fontSize: 22, fontWeight: 840 }}>
          {value || "—"}
        </span>
        <span className="t-label" style={{ fontSize: 10, color: "var(--ink-3)" }}>
          {unit}
        </span>
      </span>
      <button aria-label={`Raise ${unit}`} onClick={() => onChange(value + 1)} style={{ width: 44, height: "100%", fontSize: 22, fontWeight: 700 }}>
        +
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
    <div style={{ borderTop: "2px solid var(--ink)", background: "var(--sheet-2)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 14, padding: "14px 22px 10px", flexWrap: "wrap" }}>
        <h3 style={{ margin: 0, fontSize: 20, fontWeight: 840 }}>{sectionName} plays</h3>
        <Tabs
          options={[
            { id: "patches", label: "Patches", count: rig.patches.length },
            { id: "presets", label: "Presets", count: presets.length },
          ]}
          value={tab}
          onChange={setTab}
        />
        <span style={{ flex: 1 }} />
        <Button onClick={() => onPick(null)} disabled={!current}>
          Keep what plays
        </Button>
      </div>
      {tab === "patches" ? (
        <div style={{ padding: "4px 22px 22px", display: "flex", flexDirection: "column", gap: 14 }}>
          {rig.perf.stacks.map((st) => (
            <div key={st.name}>
              <Tape colour={tapeFor(st.name)} tilt={-0.8} style={{ marginBottom: 8 }}>
                {st.name}
              </Tape>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(170px, 1fr))", gap: 8 }}>
                {st.patches.map((p) => {
                  const on = current?.kind === "patch" && current.name === p;
                  return (
                    <PickCell key={p} on={on} title={p} sub={on ? "Playing here" : undefined} onClick={() => onPick({ kind: "patch", name: p })} />
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div style={{ padding: "4px 22px 22px", display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(200px, 1fr))", gap: 8 }}>
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
          {modulesOf("Preset").length === 0 && null}
        </div>
      )}
    </div>
  );
}

function PickCell({ title, sub, on, onClick }: { title: string; sub?: string; on: boolean; onClick: () => void }) {
  return (
    <button
      className="pressable"
      onClick={onClick}
      style={{
        position: "relative",
        minHeight: 58,
        padding: "8px 12px",
        textAlign: "left",
        background: "var(--sheet)",
        border: on ? "3px solid var(--ink)" : "1px solid var(--rule-strong)",
        borderRadius: "var(--r)",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap: 3,
      }}
    >
      <span style={{ fontSize: 16, fontWeight: on ? 860 : 700 }}>{title}</span>
      {sub && <span className="t-meta" style={{ fontSize: 13 }}>{sub}</span>}
    </button>
  );
}
