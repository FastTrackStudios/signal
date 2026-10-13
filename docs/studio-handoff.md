# Session studio on macOS — handoff

The Blitz studio window (`session-daw`) opening a real multitrack on
`daw-standalone` on macOS, preparing it — organized into folders, the song
built from its keyflow chart, click / count / guide generated and played —
and playing it through CoreAudio. Everything here was done on the Mac mini
`airlock` (macOS 27, Apple Silicon) on 2026-09-21.

**Nothing is pushed.** All of this is local commits on three repos, held until
there is more to push together (§6).

**2026-09-22 follow-on:** this studio is now also embedded as a panel in the
Session app's own Blitz shell (`session-desktop`, `apps/desktop/src/native/`)
— a top bar, a DAW view and a Performance view with a live paginated chart
panel, on the same prepared session this doc describes. See
`docs/app-on-blitz.md` for that work (currently uncommitted, further along
than this file).

## 1. Try it

```bash
cd /Volumes/build-disk/development/session
just studio-song "/Volumes/build-disk/development/sessions/Always On Time/Always On Time.RPP" \
                 "/Volumes/build-disk/development/sessions/Always On Time/Always_on_Time.kf"
```

- `studio-song PROJECT [CHART]` builds `blitz_shot` (release), opens the
  window on PROJECT and — when CHART is given — prepares it first
  (`FTS_BLITZ_ORGANIZE=1 FTS_BLITZ_CHART=… FTS_BLITZ_GUIDE=1`).
- Space plays / stops. The window logs to `/tmp/fts-studio.log`, including
  `session_daw::audio` every 2 s: block size, mean/peak render against the
  block's budget, and a WARN line when blocks overran or the device xrun'd.
- The test song: `sessions/Always On Time/` (copied from `thebattleship:~/Downloads`),
  its chart `Always_on_Time.kf` (68 bpm 4/4 #F — the tempo/key line and the
  title's capital "On" were added here). The `.RPP` was edited once: BGVS and
  the three Choir tracks unmuted.
- Guide samples: `~/.config/fts/guide-samples/{Click,Counts,Guide}` — the
  FTS-GUIDE library, copied from `thebattleship:~/.config/fts/guide-samples`
  (20 MB). `FTS_GUIDE_SAMPLES` overrides the path.

Confirmed by eye/ear at the end of the session: chord items draw their MIDI
notes, the KEY item shows, the MIX BUS tree is hidden, "Verse, 2, 3, 4"
sounds right.

## 2. What preparing a session does (`session_daw::prepare`)

Runs on the engine the window opened, before the view reads it, in order:

1. **Arrange into groups** — `DawTarget::arrange_into_groups`
   (`dynamic-template/src/apply/live.rs`). Takes apart a folder that only
   wrapped the multitrack (no media, not a bus or a known group), and files
   every content track into the group the grouping engine
   (`organize_into_tracks`) assigns: Drums, Bass, Guitars/Electric, Keys,
   Choir, Guide… Tracks keep their own names (the engine would rename).
2. **Organize** — `dynamic_template::apply::organize`, the whole bus pass
   (classify, repair folders, colour, buses, routing, DI nesting, UNSORTED)
   moved out of the `--apply-buses` CLI so the CLI (byte-identical output)
   and the live target share it. Bus tree gained **PERC BUS**.
3. **Build from the chart** — `session::keyflow::from_chart::build_from_chart`:
   core ruler lanes (SONG/SECTIONS/MARKS), project tempo and meter, the SONG
   region (title, count-in through `=END`), COUNT-IN / SONGSTART / SONGEND /
   =END markers, section regions on SECTIONS with keyflow colours, the
   Keyflow folder (KEY / CHORD / LINES / HITS), a KEY item at 0 (label = key,
   `crate::key::set_key_at`), and one CHORD MIDI item per chord
   (`keyflow::generate::voicings`, named as written). Refuses a project that
   already has a Keyflow folder.
4. **Generate click / count / guide** — `session::guide::Guide` with
   `.with_instrument("fts.guide")`. MIDI tracks Click / Count / Guide, the
   multitrack's own stems renamed **Click Audio / Guide Audio** and muted,
   an empty **Shaker**; the Guide folder ordered
   Click, Shaker, Count, Guide, Click Audio, Guide Audio.
5. **Top-level order and hiding** — Guide, Keyflow, the groups, then
   CLICK + GUIDE BUS / MIX BUS / UNSORTED; MIX BUS and CLICK + GUIDE BUS
   hidden from the track panel (only the folder track — the panel hides a
   hidden folder's contents; the mixer shows all).

## 3. The guide instrument (`session-daw/src/guide_instrument.rs`)

`session_guide::GuideEngine` in MIDI mode — the DSP the FTS Guide CLAP/VST3
(`signal/apps/plugins/guide`) wraps — as a native `PluginInstance`,
installed as the `fts.guide` FX factory when a session opens. Named per role
(`fts.guide:click|count|guide`).

- **Prepared at creation, never on the audio thread.** The renderer prepares
  an unprepared plugin inside the callback; loading 20 MB of samples there
  was a 40–60 ms first block. Measured after: worst block 0.6 ms of 10.7.
- **Click**: `ClickSubdivision::Auto` (default) — eighths below 75 bpm,
  quarters from it, per tempo segment. Voicing: on-beats take the kit's
  higher tick, off-beat eighths the lower, the bar's one = an on-beat
  (`SampleBank::beats_high_offbeats_low`; Cowbell measured).
- **Cues** land one measure before the section (`speak_lead_measures`, was 8
  beats). Both the cue and the count note under it are in the MIDI; the
  generator tags that count note (`COUNT_UNDER_CUE_VELOCITY`), the Guide
  track's instrument reports every block it plays unmuted, and the Count's
  skips tagged notes while it does → "Verse, 2, 3, 4"; mute Guide and the
  "1" is back. Order-independent.
- Cue sample keys fixed: "Pre Chorus.wav" was parsed as type "Pre"; End is
  the library's "Ending".

## 4. Fixes along the way worth knowing

- **Lanes are 0-based everywhere** now (REAPER's API numbering): the
  standalone loader converts the file's 1-based rows; the song builder
  walked lanes from 1 and never found SONG (on REAPER too); `CoreLane::flags`
  had SONG as the default region lane (REAPER ignores flag writes, standalone
  honours them); the studio ruler assumed 1-based.
- **Colours**: `daw` pinned color-palette v0.1.0, which wrote RGB instead of
  REAPER-native BGR on macOS/Linux — section colours were byte-swapped in
  REAPER too. Bumped to v0.1.3; standalone stores RGB; the view masks the
  custom-colour flag.
- **Guide generation on standalone**: `SongBuilder::build_on(daw, project)`
  replaces the REAPER-only builder; a single song takes its name from the
  SONG-lane region.
- **Tag** is a `SectionType` (daw keyflow-proto) / `SectionKind` (session);
  the guide says "Tag".
- Cursors: edit cursor blue, play cursor yellow, fainter trail/glow.

## 5. Tests that cover it (session-daw)

| test | what |
|---|---|
| `organize_live` | live organize on daw-standalone == the file organize (golden session) |
| `chart_live` | chart → tempo, markers, regions, Keyflow folder, KEY item, CHORD MIDI |
| `prepare_live` | multitrack with click/guide stems: organize + chart + guide; folder order; audible render; cue-over-count in rendered audio |
| `guide_instrument` | every click/count/cue note sounds; click voicing by pitch |
| `diagnose_playback` | (env-gated) render cost per block + click onsets vs the grid |

Real-song variants run with `FTS_CHART_PROJECT=… FTS_CHART=…`. Tests that open
a project hold a lock (the window's engine is process-wide).

## 6. Repos, branches, what is unpushed

Sibling checkouts under `/Volumes/build-disk/development/`. Session builds
`daw` crates from `../daw` and `engraver-proto` from `../keyflow` (`[patch]`).

| repo | branch | local-only |
|---|---|---|
| `daw` | `tag-section` (on top of pushed `main` = `macos-compat`) | Tag, standalone lanes, colours, xrun handling, render context |
| `session` | `macos-compat` | everything in §2–§5 |
| `keyflow` | `tag-section` (off `v0.2.1`) | engraver Tag arm |

**Before pushing session**, the `[patch."…keyflow"] engraver-proto = { path = "../keyflow/…" }`
needs keyflow's Tag change released (tag) and the pin moved — or everyone
building session needs the keyflow sibling on that branch. `daw`'s `macos-compat`
work (first half of this session) is already on `origin/main`; CI on Linux ran
for it but was not checked to completion.

## 7. Open threads

- **Saving**: nothing is written back — deferred until our own session file
  format (which REAPER should also open). `save_project_as` only patches items
  on existing tracks.
- **The window reads the session once**, at startup; edits inside it do not
  re-read, and prepare only runs at open.
- **REAPER side of the guide**: the generator adds the instrument by name;
  REAPER needs the FTS Guide plugin under a findable name, set to MIDI mode.
- **Click kit** is fixed to Cowbell; per-kit pitch order was only measured
  for Cowbell.
- **Seek into unread media** page-faults on the audio thread (daw-standalone
  mmap) — a playback prefetch issue.
- **The CI runner `airlock` is stopped** (`~/actions-runner`): restart with
  `./svc.sh start`; task's TestFlight job that was cancelled needs a re-run.
- `blitz_shot` still carries the window mode; folding it into the real
  `session-daw` binary (and retiring the WRY `main.rs`) is still to do.
