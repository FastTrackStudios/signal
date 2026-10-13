# Original Composition: the end-to-end integration test

A song written in Session from nothing: a new project, a drum kit, a bass
and a guitar, the structure placed while recording, guitar takes comped
as they go in, drum patterns drawn, a bass with a tone change in the
chorus, and the guitar grown into a doubled rhythm part, a doubled lead
and a doubled harmony, each part with its own signal rig. This document
turns that workflow into one ordered integration test. For each step it
gives:

- **Does**: what the user does.
- **Today**: the Session action (id and args), key or click that does it
  now, cited by file and line. **MISSING** marks a step that cannot be
  done yet; the entry then says what has to be built and where.
- **Asserts**: what the test checks once the step has run.

The test lives at `apps/session-daw/tests/original_composition.rs`. It
starts from an empty engine, not the golden fixture. Every step goes
through the Action List (`session_daw::actions::list().run_json`), so the
test runs the same paths that a key press, a menu or a model over MCP
would run.

Tracey prefix `compose`. Each phase is one requirement, and the test
marks each phase it covers with `r[verify compose.<phase>]`.

Builds on `workflows.md` (the `flow.guitars.*` growth vocabulary,
`flow.scenes.guide-folder`), `track-organization.md` (guide generation),
`automation.md` (`automation.by-section`), `maximal-template.md` (the
golden kit) and `../signal/profile.md` (profiles, stacks, patches).

Paths below are relative to the Session repo root unless prefixed:
`daw:` is `../daw`, `signal:` is `../signal`.

---

## Phase 0: harness

r[compose.harness]
The test drives one process-wide engine and pins down the shared facts
every later step uses: the timeline, the fake audio, how a recording is
simulated, and how the test stands in for the window.

### 0.1 Engine and process

- One test, one process, because the daw facade binds to the first
  engine opened (`apps/session-daw/tests/drum_quantize.rs:9-10`,
  `tests/default_view.rs:12-13`).
- Build with `--features native,signal`. `BASS_RIG` and `DRUMS_SAMPLES`
  exist only with `signal` (`apps/session-daw/src/actions.rs:2236-2264`,
  `:2205-2283`), and `TRACK_COMP_FROM_LANE` only with `native`
  (`actions.rs:993-994`).
- Today a test can only open a file: `session_daw::open::open_silent(path)`
  (`apps/session-daw/src/open.rs:116`). Phase 1 replaces this with
  `SESSION_NEW`.
- Engine access for assertions: `session_daw::actions::song()`
  (`actions.rs:2636`) returns `(daw, project)`. Raw engine edits go
  through `session_daw::engine::apply_now(&Edit)`
  (`apps/session-daw/src/engine.rs:1683`).

### 0.2 Being the window

Window actions such as `SESSION_COMP_TAKE`, `SESSION_TOGGLE_RECORD` and
`SESSION_MARKER` only queue an `Action` (`actions.rs:46` `take_window`).
The window applies the queue in `ArrangementWidget::paint_scene`
(`apps/session-daw/src/widget.rs:3076-3089`, which calls the private
`apply_actions` at `widget.rs:2411`). `SESSION_MODE` only stores a
request that the desktop shell takes up
(`actions.rs:960-977`, `apps/desktop/src/native/shell.rs:76`).

The harness builds an `ArrangementWidget::for_session` the way
`tests/default_view.rs:55-71` does. After any window action it calls
`paint_scene(1920, 1080, 1.0)`, which drains the queue through the real
code. When the queue holds work that needs no widget, the harness drains
it itself, as `tests/drum_quantize.rs:27-63` (`be_the_window`) does.

**MISSING (harness support):** a public `session_daw::headless::Window`
(or `ArrangementWidget::pump()`) that takes queued actions, the mode
request and pending resyncs (`crate::studio::take_resync`) without
building a Vello scene. It should also report the current mode, because
nothing reads `MODE_REQUEST` back today.

### 0.3 Timeline

4/4 at 68 bpm once step 3.5 has run. One bar is `240 / 68 = 3.529412 s`.
Bars are numbered from 1.

| what | bars | seconds |
|---|---|---|
| COUNT-IN marker | 1 | 0.0 |
| SONGSTART marker | 3 | 7.0588 |
| Intro (IN) | 3–7 | 7.0588–21.1765 |
| Verse (VS) | 7–15 | 21.1765–49.4118 |
| Chorus (CH) | 15–23 | 49.4118–77.6471 |

These lengths are what Session already produces:

- the count-in is two bars (`crates/session/session/src/keyflow/actions.rs:1059`);
- the default section lengths are Intro 4 and Verse/Chorus 8 (`keyflow/actions.rs:1053-1071`);
- after a section is inserted, the edit cursor moves to its end (`keyflow/actions.rs:446`), so the next section goes in after it.

All assertions compare times to ±1 ms.

### 0.4 Fake audio

Each recorded pass is a generated mono 48 kHz WAV, written with the
`wav(path, spans)` helper already in
`tests/automation_by_section.rs:20-49` (moved into a shared
`tests/common/audio.rs`).

- Every take gets its own frequency: take *n* of a part is a sine at
  `110 · (n + 1)` Hz at −18 dBFS, spanning exactly the recorded range.
- Files are named `<part>-<section>-t<n>.wav`, for example
  `gtr-intro-t3.wav`.
- `TRACK_ITEMS` reports each item's `source` file name
  (`actions.rs:2304-2316`). That is how the test proves which take a
  comped region plays.
- Rendering checks are optional (behind `FTS_COMPOSE_RENDER=1`). They
  render the comp lane through
  `daw::standalone::audio_engine::render::ProjectRenderer`, as
  `tests/prepare_live.rs:224` does, and check the dominant frequency per
  bar.

### 0.5 How "record N takes" is simulated

Recording does not work in the standalone engine today:

- `SESSION_TOGGLE_RECORD` queues `Action::ToggleRecord`, and the window
  only logs "record is not wired to the transport yet"
  (`apps/session-daw/src/arrange_edit.rs:1446`).
- The transport bar's record button reaches `transport.toggle_recording()`
  (`apps/session-daw/src/transport_bar.rs:185-186`, `engine.rs:934`).
  But daw-standalone's `record`, `stop_recording` and `toggle_recording`
  only change the play state
  (`daw:features/standalone/daw-standalone/src/transport.rs:119-141`).
- No take, lane or file is ever written. Live input is mixed for
  monitoring only (`daw:…/audio_engine/render/mod.rs:761`, `:986-990`).

**MISSING (M4, recording):**

1. **Capture in daw-standalone.** While recording, every armed track's
   record input is written to a WAV in the project's media folder. On
   stop, one function, `finish_recording`, puts each captured file on
   its track as a new take. That function goes in
   `daw:features/standalone/daw-standalone/src/transport.rs` next to
   `stop_recording`.
2. **The take model.** It is REAPER 7 fixed lanes, which the comp code
   already reads (`daw:crates/daw/proto/src/track/lanes.rs:32-63`):
   - The first pass on a track that has no lanes turns lanes on and
     names the take lane `1`.
   - Each later pass goes on the lowest-numbered take lane that is empty
     over the recorded range. If none is empty, it goes on a new lane
     named with the next number.
   - The comp lane (`C1`) plays if there is one. Otherwise the newest
     take plays.
3. **A test input source.** `Standalone::set_test_input(channel, wav)`
   makes an input channel play a WAV in place of a device.
   `ProjectRenderer` then renders the record span offline. The test runs
   the real transport, the real capture and the real `finish_recording`.
4. **The record action works.** `SESSION_TOGGLE_RECORD` calls the
   transport instead of logging (`arrange_edit.rs:1446`).

Until item 3 exists, the harness calls
`finish_recording(track, from, to, wav)` directly with the generated
file. That still exercises the take model. Calling `Items::add_item` and
`set_fixed_lane` the way `tests/comp_lanes.rs:40` does would test
nothing.

A pass is written below as **record(track, bars a–b, take n)**. It means:

1. set the cursor to bar `a − 2` (a two-bar pre-roll);
2. set the test input to the take's WAV;
3. run `SESSION_TOGGLE_RECORD`;
4. render to bar `b`;
5. run `SESSION_TOGGLE_RECORD` again.

The WAV is placed at bar `a`, as if the punch-in began at the first
downbeat.

---

## Phase 1: new project

r[compose.new-project]
The app opens to its start screen. New Project opens an untitled
project in Organize mode, with its structure and guide already placed.
The project gets its name the first time it is saved.

**1.1. Does:** opens the app and gets the start screen.

- **Today:** the start screen is `Start` in the desktop app
  (`apps/desktop/src/native/start.rs:93-97`). Its home shelves are the
  demo, Reconnect to REAPER, setlists, Recent and On this device
  (`start.rs:712-834`). It is a dioxus desktop view, so the test does not
  drive it.
- **Asserts:** nothing. This step is covered by the action in 1.2.

**1.2. Does:** clicks New Project.

- **Today: MISSING.** There is no new-project path:
  - The only "New song" pill makes a Task library song record, not a
    DAW project (`start.rs:987`, `apps/desktop/src/native/library.rs:811`,
    `:932-937`).
  - The app can only open a file (`open.rs:116`, `SESSION_OPEN` at
    `actions.rs:1738`).
- **Build (M1):** an action `SESSION_NEW {template?: String}` in
  `apps/session-daw/src/actions.rs`, beside `SESSION_OPEN`, plus a
  "New Project" button on the Home shelf of `start.rs` that runs it.
  The action:
  1. creates an untitled project in the app's scratch folder;
  2. opens it the way `open_silent` / `switch_to` do (`open.rs:116-140`);
  3. stamps COUNT-IN at bar 1 and SONGSTART at bar 3, the two-bar
     count-in that `keyflow/actions.rs:1059` uses, with a SONGEND placed
     after SONGSTART. A chart import already stamps these markers
     (`crates/session/session/src/keyflow/from_chart.rs:104-131`). An
     empty project needs the same markers with no chart.
  4. generates the guide (step 3.4);
  5. requests Organize mode.

  The project's name is `Untitled`.
- **Asserts:**
  - `SESSION_TRACKS` (`actions.rs:635`) lists only the Guide folder and
    its tracks: `Guide/Click`, `Guide/Count`, `Guide/Guide`, in the order
    `crates/session/session/src/guide.rs:65-81` gives them.
  - The markers are `COUNT-IN` at 0.0 s and `SONGSTART` at bar 3.
  - The mode reads back as Organize (needs the 0.2 readback).
  - The project name is `Untitled`, and it has no file path.

**1.3. Does:** sees that the project is unnamed and will be named at the
first save.

- **Today: partly.** `SESSION_SAVE {to?}` exists (`actions.rs:1629`,
  handler `:2121-2147`). It saves to the open file's path with the
  extension changed to `.session`, or to `to`. With no file open there is
  nothing to save to, and no name is asked for.
- **Build (M1):** saving an untitled project needs a `to` (the window
  asks for a name), and the stem of `to` becomes the project's name.
- **Asserts:** deferred to step 14.1, which is the first save.

---

## Phase 2: Organize — the starting tracks

r[compose.organize]
Organize mode's left toolbar adds instruments. Drums adds the whole
golden kit, folded. Bass adds one Bass track. Guitar adds one Guitar
track.

**2.1. Does:** sees the instrument toolbar on the left in Organize mode.

- **Today: partly.**
  - Organize mode mounts `OrganizeToolbar` across the top
    (`apps/desktop/src/native/shell.rs:613-614`). It has the 12 structure
    buttons and the time signatures (`apps/session-daw/src/organize.rs:42-55`).
  - The left rail lists scenes (`apps/session-daw/src/rails.rs:467-497`).
  - Session has no instrument buttons.
  - In REAPER, `_FTS_SESSION_CREATE_NEW_*` toolbar buttons exist
    (`nix/reaper-config/reaper-menu.ini:22-43`). They map to
    `FTS_DYNAMIC_TEMPLATE_CREATE_NEW_*`
    (`features/dynamic-template/dynamic-template/src/daw_module.rs:126-145`),
    which are not in Session's Action List.
- **Build (M2):** an instrument column on the left rail in Organize mode.
  Each button runs `TRACK_ADD_INSTRUMENT {instrument}`, described in
  2.2–2.4.

**2.2. Does:** clicks Drums.

- **Today: MISSING.**
  - REAPER's `CREATE_NEW_DRUM_KIT` builds
    `Drums/Drum Kit/{Kick, Snare, Toms, Hi-Hat, Overheads, Room}`
    (`daw_module.rs:1178-1181`). That is not the golden kit: it has no
    Sum, Trig or Sample.
  - The golden kit is `dynamic_template::golden_session::shape::drums()`
    (`features/dynamic-template/dynamic-template/src/golden_session/shape.rs:518`).
- **Build (M2):** `TRACK_ADD_INSTRUMENT {instrument: "Drums"}` in a new
  `apps/session-daw/src/organize_tracks.rs`. It appends the golden kit
  from `shape::drums()` and a `Drums/MIDI` track (see Q6), then folds
  `Drums` with `SESSION_FOLD {track: "Drums", open: false}`
  (`actions.rs:1664`, handler `:1917-1941`). The whole action is one
  undo step.
- **Asserts:**
  - `SESSION_TRACKS {filter: "Drums"}` contains `Drums/Kick/Sum/In`,
    `Drums/Kick/Trig`, `Drums/Kick/Sample`, `Drums/Snare/Sum/Top`,
    `Drums/Toms`, `Drums/Cymbals/OH` and `Drums/Rooms`, the shape that
    `maximal-template.md` ticks under Drums. It also contains
    `Drums/MIDI`.
  - `Drums` is folded (needs the fold readback from M2).
  - The kit's DRUM BUS send exists.

**2.3. Does:** clicks Bass.

- **Today: MISSING in Session.** REAPER's `CREATE_NEW_BASS` makes one
  top-level "Bass" track (`daw_module.rs:757-778`, `:1193-1196`).
- **Build (M2):** `TRACK_ADD_INSTRUMENT {instrument: "Bass"}` adds one
  top-level track named `Bass` that goes to BASS BUS. This is the
  smallest bass, not the golden `Bass/{Guitar/{DI,Amp}, Synth/…}` from
  `shape.rs:635-651`. A level is a folder only when it has more than
  one member (`r[flow.guitars.dimensions]`).
- **Asserts:** a track whose path is exactly `Bass`, not a folder, with
  0 items.

**2.4. Does:** clicks Guitar.

- **Today: MISSING in Session.** REAPER's `CREATE_NEW_ELECTRIC_GUITAR`
  makes one track named "Guitars" (`daw_module.rs:1218-1221`).
- **Build (M2):** `TRACK_ADD_INSTRUMENT {instrument: "Guitar"}` adds one
  top-level track named `Guitar` to ELECTRIC BUS. The dynamic template
  already reads "guitar" as the Electric group
  (`features/dynamic-template/dynamic-template/src/groups/guitars/electric_guitar.rs:116-120`),
  so the grow actions in phases 10–13 work on it.
- **Asserts, the state the narrative names:**
  - The top-level rows are exactly `Guide` (folded), `Drums` (folded),
    `Bass` and `Guitar`, plus the buses.
  - `Bass` and `Guitar` are plain tracks.
  - No track has items, apart from the Guide tracks.

---

## Phase 3: Record mode, guitar set up

r[compose.record-setup]
Record mode, with the guitar armed. The browser loads the worship
guitar profile, and the track is named for the loaded stack. The input
comes from the routing preset. The song start and the guide are already
in place, and setting 68 bpm turns on the eighth-note click.

**3.1. Does:** switches to Record mode.

- **Today:** `SESSION_MODE {mode: "Record"}` (`actions.rs:758`,
  `:960-977`). The modes are listed in
  `crates/session/session/src/modes.rs:15-29`. The request is applied
  by the shell (`apps/desktop/src/native/shell.rs:76`) and, in the test,
  by the 0.2 harness.
- **Asserts:** the mode reads back as Record. Record mode shows the
  record arm in the track panel (`apps/session-daw/src/tcp.rs:193`).

**3.2. Does:** arms the guitar.

- **Today: UI only.** The arm button sends `Edit::ToggleArm`
  (`widget.rs:2200`, `engine.rs:642`, `:1881`). No action arms a track,
  and `SESSION_TRACKS` does not report whether a track is armed
  (`TrackRow`, `actions.rs:672-687`).
- **Build (M4):** `TRACK_ARM {track, armed?: bool}` in `actions.rs`,
  using `Tracks::set_armed`
  (`daw:crates/daw/proto/src/track/service.rs:68`). Add `armed` and
  `record_input` to `TrackRow`. The track proto already has both fields
  (`daw:crates/daw/proto/src/track/track.rs:90`, `:113`).
- **Asserts:**
  - `Guitar.armed == true`.
  - No other track is armed.

**3.3. Does:** presses B to open the browser and clicks the worship
guitar profile. The track is then named for the stack that loads:
`Guitar Clean`.

- **Today: MISSING, three separate gaps.**
  1. **The browser.** Session has no B binding and no browser panel. The
     keys come from the FTS keybind profile (`apps/session-daw/src/keys.rs:1-16`).
     Signal has a headless browser crate (`signal:crates/signal/browser/src/lib.rs`)
     and a tablet `Browser` component
     (`signal:features/rigs/guitar/ui/src/tablet/browser.rs`); the spec
     is `docs/spec/signal/browser.md`.
  2. **A guitar rig as track FX.** signal-fx hosts only `FTS Bass Rig`
     and `FTS Kit Mic` (`features/signal-fx/src/lib.rs:30-32`).
     `apps/session-daw/src/rig.rs` only draws (`rig.rs:153-167`). On the
     signal side:
     - `RigHome` lets a guitar rig and a bass rig coexist
       (`signal:features/rigs/guitar/src/library.rs:83-126`, commit
       2e4d3e2e; test `tests/rig_homes.rs:30`).
     - `GuitarRigBackend::with_home` is at
       `signal:features/rigs/guitar/src/session.rs:623`.
     - The hosted rig (`GuitarRigBackend::hosted`, `RigHost`,
       `session.rs:711`) is uncommitted work in signal's tree.
  3. **Naming the track after the stack.** Nothing does this. `rename.rs`
     is manual in-place renaming only.

  The data exists. The worship guitar profile has the stacks Clean,
  Crunch, Drive, Lead, Ambient and Special
  (`signal:features/rigs/guitar/default-config/profiles/worship.styx:3-9`),
  and Worship is the default profile (`library.rs:556-558`).
- **Build:**
  - **(M7)** An `FTS Signal Rig` `PluginInstance` in
    `features/signal-fx/src/`, with state
    `{instrument, profile, stack, patch}`. It owns the hosted rig's audio
    half and has a stepped Scene parameter, as `FTS Bass Rig` does
    (`features/signal-fx/src/rig.rs:18`).
  - **(M7)** `RIG_LOAD {track, instrument?, profile, stack?}` puts the rig
    first in the track's chain, or replaces the rig already there. The
    instrument comes from the track's group when it is not given.
    `BASS_RIG` becomes the bass case of this action.
  - **(M8)** The browser panel, toggled with B, in a new
    `apps/session-daw/src/browser.rs`. It follows the selected track
    (`SESSION_SELECT_TRACK`, `actions.rs:1827`). Clicking an entry runs
    `RIG_LOAD`, or `TONE_SET` for a stack within the loaded profile.
  - **(M7)** When a rig loads onto a track whose name has no
    Arrangement value yet, the stack name is added as the Arrangement:
    `Guitar` becomes `Guitar Clean`. The dynamic template already reads
    "Clean" as an electric Arrangement (`electric_guitar.rs:8-27`).
- **Asserts:**
  - The track is now named `Guitar Clean`, and its GUID is unchanged.
  - `SESSION_TRACKS.fx` for it is `["FTS Signal Rig"]`.
  - `FX_PARAMS {track: "Guitar Clean", fx: "FTS Signal Rig"}`
    (`apps/session-daw/src/fx_actions.rs:32`) reads Scene = `Clean`, and
    the profile reads `Worship`.
  - Negative control: the `Bass` track has no FX.

**3.4. Does:** relies on the routing manager for the input. It does not
pick one by hand.

- **Today: MISSING.**
  - Signal keeps per-rig audio settings: `RigManager`
    (`signal:features/sampler/signal-sampler/src/rig_manager.rs:1-16`,
    saved at `$XDG_CONFIG_HOME/signal/rigs/<slug>.styx`) and
    `RigAudioPrefs` (`rig_prefs.rs:13-40`).
  - Session can only draw the record input (`tcp.rs:709-716`).
  - No action maps an input to a track.
- **Build (M6):** routing presets, one per audio device, in
  `~/.config/fts/routing/<device>.styx`. The folder can be redirected
  with `FTS_ROUTING_DIR` so the test can use a temp folder. A preset
  maps instrument kinds to inputs (`guitar DI → in 1`,
  `bass DI → in 2`, …). It is read from signal's `RigManager` when that
  names the device.
  - Session applies the preset when `TRACK_ADD_INSTRUMENT` creates a
    track and again when `TRACK_ARM` arms one, using
    `Tracks::set_record_input` (`daw:…/track/service.rs:226`).
  - A `ROUTING_PRESET {device?}` action reads the preset and applies it.
  - This lives in a new `apps/session-daw/src/routing_presets.rs`.
- **Asserts:**
  - With a preset in the temp folder, `Guitar Clean.record_input` is
    mono input 1, and `Bass.record_input` is mono input 2.
  - Negative control: with no preset, both inputs are the engine's
    default and nothing fails.

**3.5. Does:** sees the SONGSTART marker, the count-in and the guide
folder, already in place.

- **Today: partly.** `Guide::generate` builds the Guide folder, its
  tracks and their notes from the sections and the tempo map
  (`crates/session/session/src/guide.rs:209-260`). Only these call it:
  - an Organize toolbar press (`organize.rs:156-162`);
  - a chart edit (`apps/session-daw/src/prepare.rs:569-583`);
  - prepare (`prepare.rs:714-718`).

  Generation fails on a project with no length ("song has no length — is
  SONGSTART/SONGEND stamped?", `guide.rs:552-571`). Nothing places
  SONGSTART or COUNT-IN in an empty project.
- **Build (M1):** `SESSION_NEW` places the markers (1.2). The guide's
  length comes from SONGEND, which grows when sections are inserted
  past it.
- **Asserts:**
  - `Guide/Click` has one MIDI item from COUNT-IN to SONGEND.
  - `Guide/Count` has count notes in the two bars before SONGSTART
    (pitches 72–79, `features/guide/src/midi.rs:7-14`).

**3.6. Does:** sets the tempo to 68 bpm. 68 is below the click
threshold, so the guide's click plays soft eighths.

- **Today, the tempo: MISSING as an action.**
  - `engine::Edit::SetTempo(guid, at, bpm)` exists
    (`engine.rs:199`, applied by `set_tempo` at `engine.rs:2027-2045`).
    The only thing that produces it is a test (`tests/reaper_panel.rs:342`).
  - The ruler's tempo lane only draws (`apps/session-daw/src/ruler/tempo.rs`).
  - The only user path is a chart's `68bpm` line through
    `rebuild_from_chart`.
- **Today, the eighths: EXISTS.**
  - `ClickSubdivision::Auto` is the default
    (`features/guide/src/midi.rs:181-193`).
  - `AUTO_EIGHTHS_BELOW_BPM = 75.0` (`midi.rs:196`), applied per tempo
    segment (`midi.rs:202-208`).
  - The offbeat clicks have pitch 62 (`midi.rs:9`, `:214`). The guide
    instrument plays them as the kit's lower tick
    (`apps/session-daw/src/guide_instrument.rs:68-70`).
  - They are "soft" by sample only. The velocity is the same 96 as the
    beats (`midi.rs:224-225`).
  - There is a unit test at 68 bpm (`midi.rs:412-440`).
- **Build (M3):**
  - `TEMPO_SET {bpm, at?: String}` in `actions.rs`. `at` is a bar or a
    time, and the default is the project's start. It produces
    `Edit::SetTempo` and is one undo step.
  - After a tempo change, the guide is regenerated and the markers and
    sections stay on their bars (Q4). Nothing regenerates the guide
    when only the tempo map changes today.
- **Asserts:**
  - The tempo at 0 s is 68 (`TempoMap::get_tempo_at`, as
    `tests/chart_live.rs:360` reads it).
  - SONGSTART is at 7.0588 s.
  - `Guide/Click` has a note on every eighth from bar 1, with pitch 62 on
    the offbeats.
  - Negative control: after `TEMPO_SET {bpm: 80}`, the offbeat notes are
    gone. Then set it back to 68.

---

## Phase 4: the intro guitar, comped as it goes in

r[compose.intro]
Five four-bar takes. Bars 1–2 are comped from take 3 and bars 3–4 from
take 5, with a swipe. A time selection over the four bars, then Insert
Intro: the region goes over them and the guide picks up the intro cue.

**4.1. Does:** records a four-bar part five times.

- **Today: MISSING** (see 0.5, M4).
- **Test:** for n in 1 to 5, record(`Guitar Clean`, bars 3–7, take n),
  with file `gtr-intro-t<n>.wav`.
- **Asserts:**
  - `SESSION_TRACKS.lanes` for `Guitar Clean` lists lanes `1` to `5`,
    each with 1 item (`LaneRow`, `actions.rs:690-696`).
  - `TRACK_ITEMS {track: "Guitar Clean"}` (`actions.rs:2286`) returns 5
    items, each at 7.0588 s with length 14.1176 s, whose sources are
    `gtr-intro-t1.wav` to `gtr-intro-t5.wav`, on lanes 0 to 4.
  - The newest take, lane `5`, plays.

**4.2. Does:** swipe-comps take 3 over the first two bars and take 5 over
the last two, in Record mode.

- **Today: EXISTS as an action. Comping in Record mode does not.**
  - `TRACK_COMP_FROM_LANE {track, lane, start, end}`
    (`apps/session-daw/src/comp.rs:347-354`, args `:367-375`) copies the
    source lane's items onto the comp lane with 5 ms fades and records a
    `CompArea`. It is one undo step (`comp.rs:107`, `:124`, `:130-170`).
  - A second area cuts into the first (`comp.rs:298-334`).
  - The window's swipe gesture is `session_daw::comp::swipe`
    (`tests/comp_lanes.rs:122-182`).
  - `record_view.rs` has no comp control (its parts are listed at
    `record_view.rs:1-15`).
- **Build (M5):**
  - Let `start` and `end` also be given as bars (`"3"`, `"5:1"`), the
    way `FX_AUTOMATE` reads `at` (`fx_actions.rs:77-80`).
  - The comp lane is created when the first comp is made (`C1`), as
    `comp.rs:32-42` already finds it.
  - The swipe works on take lanes in Record mode, not only in Edit.
- **Test:**
  1. `TRACK_COMP_FROM_LANE {track: "Guitar Clean", lane: "3", start: 7.0588, end: 14.1176}`
  2. `… lane: "5", start: 14.1176, end: 21.1765`
- **Asserts:**
  - The lanes are `1`–`5` and `C1`, and only `C1` plays.
  - `C1` has two items: `[7.0588, 14.1176)` from `gtr-intro-t3.wav`
    with source offset 0, and `[14.1176, 21.1765)` from
    `gtr-intro-t5.wav` with source offset 7.0588.
  - The take lanes are unchanged: 5 items.
  - One `SESSION_UNDO` removes the second area only.
  - Optional render check: bars 3–4 are 440 Hz and bars 5–6 are 660 Hz.

**4.3. Does:** makes a time selection over the four bars and runs Insert
Intro. The region goes over them and the guide updates.

- **Today: EXISTS behind a UI click. MISSING on the Action List.**
  - The Organize toolbar's INTRO button runs `press_local`
    (`organize.rs:125-163`). That hands the arrangement's selection and
    cursor to the engine, then calls
    `session::keyflow::actions::dispatch(InsertSection(Intro))`
    (`crates/session/session/src/keyflow/actions.rs:38-77`), then
    regenerates the guide.
  - `insert_section_region` fills a time selection, carves away any
    overlapping section, moves the cursor to the section's end and
    clears the selection (`keyflow/actions.rs:399-468`, `:446-451`,
    `:493-496`).
  - The time selection is only ever set by a mouse drag
    (`apps/session-daw/src/cursor.rs:157-185`).
  - The toolbar exists only in Organize mode. The narrative does this
    from Record mode.
- **Build (M3):**
  - `TIME_SELECTION_SET {from, to}` (bars or seconds; `null` clears it).
    It sets both the arrangement's `cursor::Edit.selection` and the
    engine's.
  - `SECTION_INSERT {kind, from?, to?}` and `MARKER_INSERT {kind, at?}`.
    They do the same as `press_local`, guide regeneration included, so
    the toolbar and a model take one path. Move `press_local` into
    `apps/session-daw/src/organize.rs` as a `pub fn insert`.
  - Bind them to keys that work in every mode.
- **Test:** `TIME_SELECTION_SET {from: "3", to: "7"}`, then
  `SECTION_INSERT {kind: "Intro"}`.
- **Asserts:**
  - `SESSION_SECTIONS` (`actions.rs:789`) lists one section whose name
    starts with `IN`, the abbreviation that `keyflow/actions.rs:431`
    writes. It runs from 7.0588 s to 21.1765 s, `first_bar` 3,
    `last_bar` 6.
  - The edit cursor is at 21.1765 s, and there is no time selection.
  - `Guide/Guide` has an intro cue (pitch 84–102, `midi.rs:7-14`) at
    the intro, and a count into it.

---

## Phase 5: the verse, three rated takes, a two-take comp

r[compose.verse]
Insert Verse lands after the intro. Three takes of the whole verse. Each
take is rated right after it is recorded (one to three stars, or an X);
the unused take gets the X. The comp uses two of the takes.

**5.1. Does:** runs Insert Verse.

- **Today:** as 4.3. With no selection, the verse starts at the cursor,
  which is the intro's end, and runs its default 8 bars, capped at the
  next section (`keyflow/actions.rs:511-520`, `:1053-1071`).
- **Test:** `SECTION_INSERT {kind: "Verse"}`.
- **Asserts:**
  - The sections are `IN` (bars 3–6) and `VS` (bars 7–14,
    21.1765–49.4118 s).
  - The guide has a verse cue.
  - SONGEND has moved to 49.4118 s or later (1.2).

**5.2. Does:** records three takes of the verse and rates each right
after recording it: take 1 two stars, take 2 three stars, take 3 X.

- **Today, recording: MISSING** (0.5).
- **Today, rating: half exists.**
  - `take_rating::star(Scope)` and `miss(Scope)` drop REAPER's rank
    markers (`:)`, `:))`, `:)))`, `:(`) on the active take
    (`apps/session-daw/src/take_rating.rs:89-143`, `:146-165`;
    `TakeRating` is at `daw:crates/daw/proto/src/item/take.rs:273-300`).
  - They are only reached from Record mode's rating pad
    (`record_view.rs:346-352`, `:414-421`).
  - They only work on vocal tracks (`take_rating.rs:207-231`, which looks
    for "vocal" or "vox" in the name).
  - No action rates a take, no key does, and `TRACK_ITEMS` returns no
    ratings.
- **Build (M5):**
  - `TAKE_RATE {track, lane?: String, at?: String, rating: "*"|"**"|"***"|"x"}`.
    With no `lane`, it rates the take just recorded on the armed track.
    It writes the same take markers.
  - Make `take_rating` choose the selected or armed track, not the vocal
    track.
  - Add `rating` to `TRACK_ITEMS`' `ItemRow`, read through
    `TakeHandle::rating()` (`daw:crates/daw/control/src/items.rs:1147-1152`).
  - Give the rating pad keys `1`, `2`, `3` and `x` in Record mode.
- **Test:** for each n from 1 to 3: record(`Guitar Clean`, bars 7–15,
  take n) with `gtr-verse-t<n>.wav`, then `TAKE_RATE` with `**`, `***`
  or `x` in turn.
- **Asserts:**
  - The verse takes are on lanes `1` to `3`, the lowest lanes free over
    the verse (0.5).
  - The intro takes are untouched.
  - The ratings in `TRACK_ITEMS` over the verse are `**`, `***` and
    `x`.
  - Negative control: the intro takes have no rating.

**5.3. Does:** comps the verse from two takes: bars 1–4 of the verse
from take 2 and bars 5–8 from take 1.

- **Test:**
  1. `TRACK_COMP_FROM_LANE {track: "Guitar Clean", lane: "2", start: "7", end: "11"}`
  2. `… lane: "1", start: "11", end: "15"`
- **Asserts:**
  - `C1` now has four items: the two intro pieces and
    `[21.1765, 35.2941)` from `gtr-verse-t2.wav` and
    `[35.2941, 49.4118)` from `gtr-verse-t1.wav`.
  - No `C1` item comes from `gtr-verse-t3.wav`, the take rated X.

---

## Phase 6: drums, a worship kit, drawn patterns

r[compose.drums]
The browser on the drums offers signal's kits. The worship kit loads
onto the golden kit's tracks, and each section's pattern is drawn as
MIDI on the kit.

**6.0. Does:** inserts the chorus. The narrative does not say when, but
the chorus pattern in 6.3 needs it to exist.

- **Test:** with the cursor at the verse's end,
  `SECTION_INSERT {kind: "Chorus"}`.
- **Asserts:** `CH` covers bars 15–22 (49.4118–77.6471 s).

**6.1. Does:** selects Drums, opens the browser (B) and clicks a worship
kit.

- **Today: MISSING.**
  - No worship kit exists. Signal's kit library is the GGD Modern and
    Massive 2 presets: Heavy Thwacky, Stadium Filler, Organic Oomph, …
    (`signal:features/rigs/drums/src/backend.rs:25-27`, `:1517-1540`).
    The library is overridden by `SIGNAL_DRUM_LIBRARY`.
  - `FTS Kit Mic` plays a `.signalpreset` from a track's own MIDI
    (`features/signal-fx/src/kit.rs:1-58`).
  - The only action that puts it on the kit is `DRUMS_SAMPLES
    {preset?, dry_run?}` (`actions.rs:2205-2283`, default Heavy Thwacky
    at `:2233`). That action writes notes from recorded hit markers,
    which a composed song does not have.
- **Build (M9):**
  - A `Worship.signalpreset` in the kit library. It is a preset over an
    existing pack, so it is data, not code. The test uses a tiny fixture
    pack under `features/signal-fx/fixtures/kit/` via
    `SIGNAL_DRUM_LIBRARY`.
  - `KIT_LOAD {preset}`: `FTS Kit Mic` on each piece's Sample track,
    with the overheads and rooms in `split` mode
    (`features/signal-fx/src/kit.rs:21-24`). Each Kit Mic is fed by MIDI
    sends from `Drums/MIDI`.
  - The browser's drum section lists the kits and runs `KIT_LOAD`.
- **Asserts:**
  - Every `Drums/*/Sample` track has `FTS Kit Mic` with `preset` ending
    in `Worship.signalpreset`.
  - `Drums/MIDI` has a MIDI send to each of them.
  - Negative control: the close-mic tracks (`Kick/Sum/In`, …) have no
    Kit Mic.

**6.2. Does:** draws the intro pattern: a four-on-the-floor tom groove
with snare and hi-hat on beat 4.

- **Today: MISSING as an action.**
  - The expression editor has a pencil (`Tool::NoteDraw`,
    `daw:features/expression-editor/expression-editor-core/src/tools.rs:15-27`).
    Its notes reach the engine only when the editor saves
    (`apps/session-daw/src/editor_view.rs:772-791`).
  - The engine API exists: `create_midi_item`
    (`daw:crates/daw/proto/src/midi/service.rs:316`) and `add_notes_ppq`
    (`:339`). `DRUMS_SAMPLES` uses both
    (`apps/session-daw/src/drum_samples.rs:281-319`).
- **Build (M9):** `MIDI_NOTES_ADD {track, at: String, notes: [{bar_beat, pitch|piece, velocity, length}]}`.
  - It creates or extends the MIDI item under `at` (a section name or a
    bar range).
  - `piece` (`kick`, `snare`, `hihat`, `floor-tom`, `crash`, …) is
    resolved through the loaded kit's note map.
  - One undo step.
- **Test:** over `IN`, in each bar: floor tom on beats 1–4, and snare
  plus closed hi-hat on beat 4.
- **Asserts:** `Drums/MIDI` has an item covering bars 3–6 with 4 × 4
  floor-tom notes, 4 snare notes and 4 hi-hat notes, each snare and
  hi-hat at beat 4 of its bar (±1 tick).

**6.3. Does:** draws the verse and chorus patterns.

- Verse: a light tom groove (rack-tom eighths at velocity ≤ 60), a kick
  on beat 1 of every bar, and a cymbal swell into bars 4 and 8 of the
  verse (a ride roll of 16ths over the bar's last beat with rising
  velocity, see Q7).
- Chorus: a big open beat. Kick on 1 and 3 and the "and" of 2, snare on
  2 and 4, open hi-hat eighths, and a crash on beat 1 of bar 15.
- **Asserts:**
  - Per section, the note counts and positions match.
  - Every kick in the verse is on beat 1.
  - Every snare in the chorus is on beat 2 or 4.
  - Optional render check: `Drums/Kick/Sample` is above −40 dBFS within
    20 ms after each kick note and silent between them.

---

## Phase 7: the chorus guitar

r[compose.chorus-guitar]
Five chorus takes, comped from takes 2 and 3.

**7.1. Does:** records the chorus part five times.

- **Test:** for n in 1 to 5, record(`Guitar Clean`, bars 15–23, take n)
  with `gtr-chorus-t<n>.wav`.
- **Asserts:** the chorus takes are on lanes `1` to `5`, and nothing
  outside the chorus has changed.

**7.2. Does:** comps the first half from take 2 and the second from
take 3.

- **Test:** `TRACK_COMP_FROM_LANE` with lane `2` over bars 15–19, then
  lane `3` over bars 19–23.
- **Asserts:** `C1` covers bars 3–23 without gaps, as six items whose
  sources, in order, are intro t3, intro t5, verse t2, verse t1, chorus
  t2 and chorus t3.

---

## Phase 8: bass, worship profile, Drive in the chorus

r[compose.bass]
The bass gets the worship bass profile, on its default clean stack. A
Shift-click on the Drive stack for the chorus puts a scene change on the
chorus. Then the bass is recorded: intro and verse in one take, the
chorus in four, using the fourth.

The narrative records the bass before the Shift-click (step 10) and
then says the Shift-click came first (step 11). The test does it in the
order it happened.

**8.1. Does:** selects Bass, opens the browser and clicks the worship
bass profile.

- **Today: EXISTS without the browser.**
  - `BASS_RIG {profile: "Worship"}` (`actions.rs:2236-2264`;
    `apps/session-daw/src/bass_rig.rs:129-207`) puts `FTS Bass Rig`
    first on every non-folder track whose top folder is `Bass`. A plain
    top-level `Bass` track matches, because `gain_stage::path_of`
    includes the track itself (`apps/session-daw/src/gain_stage.rs:199-217`).
  - The starting scene is the patch whose name the track's name contains.
    Otherwise it is the profile's default patch (`bass_rig.rs:107-121`).
  - The profile is read from Session's own copy at
    `~/.config/fts/bass-rig`, redirected by `FTS_BASS_RIG_DIR`
    (`features/signal-fx/src/profile.rs:53-85`).
  - The worship bass profile has the stacks Clean (Amp, DI), Crunch,
    Drive (Drive), Synth, Fuzz and Ambient
    (`signal:features/rigs/guitar/default-config-bass/profiles/worship.styx:3-9`).
  - The Scene parameter switches patches, not stacks
    (`bass_rig.rs:113-118`).
- **Build (M7, M8):** the browser runs `RIG_LOAD {track: "Bass", profile: "Worship"}`.
  Until then the test runs `BASS_RIG`. The test copies the profile into
  a temp folder and sets `FTS_BASS_RIG_DIR` to it.
- **Asserts:**
  - `Bass.fx == ["FTS Bass Rig"]`.
  - `FX_PARAMS` reads Scene = `Amp`, the first patch of the Clean stack.
  - The track name is unchanged (`Bass`) until the M7 naming rule
    exists, then `Bass Clean` (Q2).

**8.2. Does:** Shift-clicks the Drive stack while the cursor is in the
chorus.

- **Today: the data path EXISTS. The gesture is MISSING.**
  - `FX_AUTOMATE {track, fx, param, points: [{at, value, shape}]}`
    (`fx_actions.rs:77-80`, args `:152-159`) writes points at section
    names (`"CH 1"`, `"CH 1 end"`) with the value in the plugin's own
    words.
  - `AUTOMATION_BY_SECTION` (`apps/session-daw/src/automation_actions.rs:425-431`,
    impl `:709`) sets a whole envelope by section.
    `tests/automation_by_section.rs:265-360` already automates
    `FTS Bass Rig/Scene` with Drive in the choruses.
- **Build (M8):** `TONE_INSERT {track, tone, at?: String}`.
  - `at` is a section name, a bar or the cursor. The default is the
    section under the cursor.
  - It writes two step points on the rig's Scene envelope: `tone` at the
    section's start and the tone that was playing at its end.
  - Shift-click in the browser or the switches runs it.
  - `TONE_SET {track, tone}` (a plain click) changes the rig's base tone
    where no automation overrides it.
- **Test:** `FX_AUTOMATE {track: "Bass", fx: "FTS Bass Rig", param: "Scene", points: [{at: "CH 1", value: "Drive", shape: "step"}, {at: "CH 1 end", value: "Amp", shape: "step"}]}`.
  Once M8 lands, use `TONE_INSERT {track: "Bass", tone: "Drive", at: "CH 1"}`.
- **Asserts:**
  - `FX_AUTOMATION {track: "Bass"}` (`fx_actions.rs:67-74`) lists two
    Scene points: `Drive` at 49.4118 s and `Amp` at 77.6471 s, both
    step-shaped.
  - The Scene reads `Amp` at 30 s and `Drive` at 60 s.

**8.3. Does:** records the intro and verse in one take, then the chorus
four times, and uses the fourth chorus take.

- **Test:**
  1. record(`Bass`, bars 3–15, take 1) with `bass-iv-t1.wav`.
  2. For n in 1 to 4, record(`Bass`, bars 15–23, take n) with
     `bass-ch-t<n>.wav`.
  3. `TRACK_COMP_FROM_LANE {track: "Bass", lane: "1", start: "3", end: "15"}`.
  4. `… lane: "4", start: "15", end: "23"`.
- **Asserts:**
  - The bass lanes are `1`–`4` and `C1`.
  - `C1` has `bass-iv-t1.wav` over bars 3–14 and `bass-ch-t4.wav` over
    bars 15–22.
  - The Scene automation from 8.2 is unchanged.

---

## Phase 9: Drive on the guitar's chorus

r[compose.guitar-drive]
A Drive tone change on the rhythm guitar's chorus, put in after the
guitar was recorded.

**9.1. Does:** in the browser, with `Guitar Clean` selected and the
cursor in the chorus, Shift-clicks the Drive stack.

- **Today:** the same data path as 8.2, but on `FTS Signal Rig`, which
  is MISSING (M7).
- **Test:** `TONE_INSERT {track: "Guitar Clean", tone: "Drive", at: "CH 1"}`.
- **Asserts:**
  - The guitar rig's Scene envelope has `Drive` at 49.4118 s and `Clean`
    at 77.6471 s.
  - The bass envelope is unchanged.
  - The track name is still `Guitar Clean` (Q2).

---

## Phase 10: double the rhythm guitar

r[compose.double]
Doubling the guitar makes it a folder over two performances. The rig
stays on the folder, and the two sides feed it in stereo. Then the
double is recorded.

**10.1. Does:** selects the guitar and runs the action the narrative
calls "add layer", which makes `Main` and `DBL`.

- **Today: EXISTS in the session crate. Not on Session's Action List.**
  - `GUITAR_DOUBLE` (`crates/session/proto/src/guitar_grow.rs:35-55`;
    impl `crates/session/session/src/guitar_grow.rs:66-72`) acts on the
    selected track. The track carrying the items becomes the folder, and
    a new first member takes the items
    (`guitar_grow.rs:257-264`, `grow_level_to` at `:205-233`).
  - The new members are named `L` and `R`, the Channel dimension
    (`crates/session/session/tests/guitar_grow_actions.rs:188-207`).
  - It is registered for REAPER only, as `FTS_SESSION_GUITAR_DOUBLE`
    (`crates/session/session/src/lib.rs:166-171`).
  - It moves items. Whether take lanes, lane names, the play mask and
    comp areas move too is not tested.
- **Build (M10):**
  - Register `GuitarGrow<Standalone>` on Session's Action List as
    `TRACK_DOUBLE`, `TRACK_ADD_ARRANGEMENT`, `TRACK_ADD_LAYER` and
    `TRACK_ADD_SOURCE`, in a new `apps/session-daw/src/grow.rs`.
  - Moving the content to the first member moves its lanes and its comp
    with it: `lane_names`, `lane_play_mask` and the `LaneComping` areas.
  - Settle the naming (Q1).
  - The two channels are panned hard left and right.
  - The folder's rig takes the folder's stereo sum. The profile rig
    already processes the input in stereo
    (`features/signal-fx/src/profile.rs:220-260`).
- **Test:** `SESSION_SELECT_TRACK {track: "Guitar Clean"}` (`actions.rs:1827`),
  then `TRACK_DOUBLE {}`.
- **Asserts (with Q1 resolved as L/R; swap the names if Q1 picks
  Main/DBL):**
  - The tree is `Guitar Clean/{L, R}`. The folder keeps the original
    GUID and `FTS Signal Rig`. `L` and `R` have no FX.
  - `L` has every item, lane and comp area the track had: lanes `1`–`5`
    and `C1`, with `C1` still covering bars 3–23 as in 7.2.
  - `R` is empty.
  - Pans: `L` −100 %, `R` +100 %.
  - The Scene envelope is still on the folder's rig.
  - One `SESSION_UNDO` restores the single track with its lanes.

**10.2. Does:** records into the double.

- **Test:** arm `R` and disarm `L` (`TRACK_ARM`). Then
  record(`Guitar Clean/R`, bars 3–23, take 1) with `gtr-dbl-t1.wav`.
- **Asserts:**
  - `R` has one take over bars 3–23.
  - `R`'s record input is the guitar input from the routing preset
    (3.4). The double inherits it.

---

## Phase 11: add a lead

r[compose.lead]
Add Arrangement on the guitar adds a second part, which is renamed
Lead. It gets a lead sound and is tracked in the intro and chorus only.

**11.1. Does:** runs Add Arrangement on the guitar and renames the new
track Lead.

- **Today: EXISTS in the session crate. Not in Session.**
  - `GUITAR_ADD_ARRANGEMENT` (`crates/session/proto/src/guitar_grow.rs:57-69`)
    reads the container's own Arrangement value off its name, moves it
    down onto a new folder, and adds the template's next unused
    arrangement beside it
    (`crates/session/session/tests/guitar_grow_actions.rs:251-266`).
  - The electric arrangement order is Clean, Crunch, Drive, Lead, …
    (`electric_guitar.rs:8-27`). So `Guitar Clean` would become
    `Guitar/{Clean/{L,R}, Crunch}`.
  - Rename is UI only: double-click the name (`widget.rs:2177-2193`),
    committed as `Edit::Rename` (`engine.rs:46`, `:1908`). It has no
    action.
  - When the container is already a folder (it is, after 10.1), the new
    level wraps its children in place, and the container keeps its GUID
    and its FX (`guitar_grow.rs:257-264`). The rig would then sit on the
    whole Guitar group and process both arrangements.
- **Build (M10):**
  - `TRACK_ADD_ARRANGEMENT {name?}`. With `name`, it adds that
    arrangement and not the template's next one.
  - `TRACK_RENAME {track, name}`, running `Edit::Rename`.
  - When the Arrangement level is inserted, the part's chain (the rig
    and its envelopes) moves down onto the arrangement it belongs to.
    "A Channel shares a chain, a Layer has its own"
    (`crates/session/proto/src/guitar_grow.rs:42-45`). The same applies
    to an Arrangement.
  - Q2 decides whether the first arrangement is named `Clean` or
    `Rhythm`.
- **Test:** select `Guitar Clean`, then `TRACK_ADD_ARRANGEMENT {}`, then
  `TRACK_RENAME {track: <the new arrangement>, name: "Lead"}`.
  The narrative adds and then renames. `{name: "Lead"}` must give the
  same result.
- **Asserts (Q2 resolved as Rhythm):**
  - The tree is `Guitar/{Rhythm/{L, R}, Lead}`.
  - `Guitar` has no FX.
  - `Guitar/Rhythm` has `FTS Signal Rig` with its Scene envelope.
  - `Guitar/Lead` is a plain track with no items and no FX.
  - All items are still on `Rhythm/L` and `Rhythm/R`.

**11.2. Does:** selects Lead and picks a lead sound in the browser.

- **Test:** `RIG_LOAD {track: "Guitar/Lead", profile: "Worship", stack: "Lead"}`.
- **Asserts:**
  - `Guitar/Lead.fx == ["FTS Signal Rig"]`, Scene `Lead`.
  - The name stays `Lead`, because it already has an Arrangement value
    (3.3 build).

**11.3. Does:** tracks the lead in the intro and the chorus only.

- **Test:** record(`Guitar/Lead`, bars 3–7, take 1) with
  `lead-in-t1.wav`, and record(`Guitar/Lead`, bars 15–23, take 1) with
  `lead-ch-t1.wav`.
- **Asserts:**
  - Lead's items cover bars 3–6 and 15–22, and nothing in bars 7–14.
  - Both are on lane `1`, which plays. No comp lane exists yet.

---

## Phase 12: double the lead

r[compose.lead-double]

**12.1. Does:** runs the double on Lead and records the double.

- **Test:** select `Guitar/Lead`, `TRACK_DOUBLE {}`, arm `Lead/R`, then
  record the intro and chorus as in 11.3 with `lead-dbl-*.wav`.
- **Asserts:**
  - The tree is `Guitar/Lead/{L, R}`. The rig stays on `Lead`, with none
    on `L` or `R`.
  - Pans are ±100 %.
  - `L` holds the two takes from 11.3, and `R` holds the new pair.

---

## Phase 13: the harmony

r[compose.harmony]
The narrative's "add part" makes Lead a folder over Main and a new
track named Harmony. The harmony is tracked, doubled, and has its own
rig.

**13.1. Does:** runs Add Part on Lead and names the new track Harmony.

- **Today: EXISTS as a Layer grow, with two differences.**
  - `GUITAR_ADD_LAYER` (`crates/session/proto/src/guitar_grow.rs:71-82`)
    wraps the part's channels under a `Main` layer and adds the next
    layer.
  - The next layer is `OCT` for an electric
    (`guitar_grow_actions.rs:244-249`). The layer patterns are DBL, TPL,
    Double, Triple, Main, OCT, … and do not include Harmony
    (`features/dynamic-template/dynamic-template/src/metadata_patterns.rs:90-107`).
    The growth order prefers main, harmony, oct
    (`track_schema.rs:235`).
  - The new layer mirrors the first layer's whole subtree, so it would
    arrive already doubled as `Harmony/{L,R}`.
- **Build (M10):**
  - `TRACK_ADD_LAYER {name?, mirror?: bool}`. The narrative's
    "Add Part" is this action with `mirror: false`, which adds one plain
    track.
  - Add `Harmony` to the layer patterns, so the classifier reads it back
    as a Layer.
  - The new layer gets its own rig, a copy of the first layer's chain,
    because a Layer has its own chain.
- **Test:** select `Guitar/Lead`, then
  `TRACK_ADD_LAYER {name: "Harmony", mirror: false}`.
- **Asserts:**
  - The tree is `Guitar/Lead/{Main/{L, R}, Harmony}`.
  - `Lead` has no FX. `Lead/Main` has the rig that was on `Lead`, with
    the same GUID as the plugin instance and its state.
  - `Lead/Harmony` is a plain track carrying a copy of that rig, with
    no items.
  - The 11.3 and 12.1 items are on `Lead/Main/L` and `Lead/Main/R`.

**13.2. Does:** tracks the harmony, then doubles it.

- **Test:**
  1. record(`Lead/Harmony`, bars 15–23, take 1) with
     `harm-ch-t1.wav` (Q8 covers which sections).
  2. Select `Lead/Harmony`, then `TRACK_DOUBLE {}`.
  3. Arm `Harmony/R`, then record the same range with
     `harm-dbl-t1.wav`.
- **Asserts:** the tree is `Guitar/Lead/{Main/{L,R}, Harmony/{L,R}}`.
  The rig is on `Harmony`, none on its `L` or `R`, and the pans are
  ±100 %.

---

## Phase 14: the finished song

r[compose.final]
The whole song as the narrative ends it, saved for the first time under
its name, and the same after reopening.

**14.1. Does:** saves, which names the project.

- **Test:** `SESSION_SAVE {to: "<tmp>/Original Composition.session"}`
  (`actions.rs:1629`), then `SESSION_CHECK_SAVED` (`actions.rs:1649`),
  then `SESSION_OPEN {path}` (`actions.rs:1738`).
- **Asserts (checked before the save and again after reopening):**
  1. **Tree.** Every top-level row and the full guitar subtree:

     ```text
     Guide/{Click, Count, Guide}
     Drums/…            (golden kit + MIDI)
     Bass
     Guitar/
       Rhythm/{L, R}
       Lead/
         Main/{L, R}
         Harmony/{L, R}
     ```

  2. **Rigs.** `FTS Signal Rig` is on exactly `Guitar/Rhythm`,
     `Guitar/Lead/Main` and `Guitar/Lead/Harmony`. `FTS Bass Rig` is on
     exactly `Bass`. `FTS Kit Mic` is on exactly the Sample tracks.
     There is no rig on any `L` or `R`.
  3. **Pans.** Every `L` is −100 % and every `R` is +100 %.
  4. **Tone automation.**
     - `Rhythm` Scene: Clean, Drive at `CH 1`, Clean at its end.
     - `Bass` Scene: Amp, Drive at `CH 1`, Amp at its end.
     - Lead and Harmony have no Scene automation.
  5. **Comps.** `Rhythm/L` `C1` is as 7.2. `Bass` `C1` is as 8.3.
  6. **Ratings.** The verse takes on `Rhythm/L` are `**`, `***` and `x`.
  7. **Sections and guide.**
     - The sections are `IN` (3–6), `VS` (7–14) and `CH` (15–22).
     - The tempo is 68 throughout.
     - The click has eighths, and the guide has three cues.
  8. **Name.** The project's name reads `Original Composition`.
  9. **Undo boundary.** Undo history is not kept across reopening. It is
     not asserted.

---

## Phase 15 (follow-on): split by tone

r[compose.split-by-tone]
For mixing, a part with tone automation is split into one track per
tone, each holding what was recorded under that tone. This is not in
the narrative. The lead engineer has asked for it, and it is the
remaining step of the demo test in the memory notes.

- **Today: MISSING.** The only split is `SESSION_SPLIT`, which splits at
  the cursor (`actions.rs:284-291`).
- **Build (M11):** `TRACK_SPLIT_BY_TONE {track}`. For each distinct
  Scene value the rig's envelope takes, it:
  1. adds a sibling Arrangement named for the tone (`Rhythm Clean`,
     `Rhythm Drive`), with a rig fixed on that tone and no automation;
  2. moves each comp segment, or the playing item, to the track of the
     tone that was playing under it, cutting at the tone changes with
     the comp's 5 ms fades.
- **Asserts on `Guitar/Rhythm`:**
  - It becomes `Rhythm Clean` and `Rhythm Drive`.
  - Drive holds exactly bars 15–22.
  - Clean holds bars 3–14.
  - Both rigs have no Scene envelope.
  - Rendered, the sum is the same as before the split within −60 dB.

---

## MISSING features, in dependency order

| # | feature | needs | where | steps |
|---|---|---|---|---|
| M1 | `SESSION_NEW` (untitled project, COUNT-IN/SONGSTART/SONGEND, guide, Organize), a New Project button, naming on first save | — | `apps/session-daw/src/actions.rs`, `open.rs`, `apps/desktop/src/native/start.rs` | 1.2, 1.3, 3.5, 14.1 |
| M2 | `TRACK_ADD_INSTRUMENT` (golden kit + `Drums/MIDI`, `Bass`, `Guitar`), the Organize instrument rail, `TRACK_RENAME`, `SESSION_TRACKS` readback (`armed`, `record_input`, `open`) | M1 | new `organize_tracks.rs`, `rails.rs`, `actions.rs` | 2.1–2.4, 11.1 |
| M3 | `TEMPO_SET` (markers keep their bars, guide regenerates), `TIME_SELECTION_SET`, `SECTION_INSERT`, `MARKER_INSERT` on the Action List | M1 | `actions.rs`, `organize.rs` (`press_local` → `pub fn insert`) | 3.6, 4.3, 5.1, 6.0 |
| M4 | Recording: capture + `finish_recording` into take lanes in daw-standalone, a test input source, `TRACK_ARM`, the record action working (`arrange_edit.rs:1446`) | M2 | `daw:features/standalone/daw-standalone/src/transport.rs`, `audio_engine/render`, `actions.rs` | 3.2, 4.1, 5.2, 7.1, 8.3, 10.2, 11.3, 12.1, 13.2 |
| M5 | `TAKE_RATE` on any track + ratings in `TRACK_ITEMS`; comp ranges in bars; swipe comping in Record mode | M4 | `take_rating.rs`, `comp.rs`, `record_view.rs`, `actions.rs` | 4.2, 5.2, 5.3 |
| M6 | Routing presets per audio device, applied on create and arm; `ROUTING_PRESET` | M2, M4 | new `routing_presets.rs`, signal's `RigManager` | 3.4, 10.2 |
| M7 | `FTS Signal Rig` (the hosted guitar rig as track FX, signal S1–S3), `RIG_LOAD {instrument, profile, stack}`, naming a track after the loaded stack | M2 | `features/signal-fx`, `signal:features/rigs/guitar/src/session.rs` | 3.3, 8.1, 9.1, 11.2 |
| M8 | The browser panel on B (follows the selection; click = `TONE_SET`/`RIG_LOAD`, Shift-click = `TONE_INSERT`) | M7 | new `browser.rs`, `keys.rs`, signal-guitar-ui exports | 3.3, 8.1, 8.2, 9.1 |
| M9 | A worship kit preset + `KIT_LOAD`, MIDI sends from `Drums/MIDI`, `MIDI_NOTES_ADD` | M2, M8 | kit library data, `features/signal-fx/src/kit.rs`, `actions.rs` | 6.1–6.3 |
| M10 | Grow actions on Session's list (`TRACK_DOUBLE`, `TRACK_ADD_ARRANGEMENT {name}`, `TRACK_ADD_LAYER {name, mirror}`), lanes and comps moving with their items, the chain moving to its level, L/R panning into the folder's rig, `Harmony` as a layer | M2, M7 | new `apps/session-daw/src/grow.rs`, `crates/session/session/src/guitar_grow.rs`, `metadata_patterns.rs` | 10.1, 11.1, 12.1, 13.1, 13.2 |
| M11 | `TRACK_SPLIT_BY_TONE` | M7, M10 | new action | 15 |
| H | Test harness: a headless window pump and a mode readback | — | `apps/session-daw/src/` | 0.2 |

Already there, and used as-is:

| action or code | location |
|---|---|
| `SESSION_MODE` | `actions.rs:758` |
| `SESSION_SELECT_TRACK` | `actions.rs:1827` |
| `SESSION_FOLD` | `actions.rs:1664` |
| `SESSION_TRACKS` | `actions.rs:635` |
| `TRACK_ITEMS` | `actions.rs:2286` |
| `SESSION_SECTIONS` | `actions.rs:789` |
| `SESSION_SAVE` / `SESSION_CHECK_SAVED` / `SESSION_OPEN` / `SESSION_UNDO` | `actions.rs` |
| `TRACK_COMP_FROM_LANE` | `comp.rs:347` |
| `BASS_RIG` | `actions.rs:2236` |
| `FX_AUTOMATE` / `FX_AUTOMATION` / `FX_PARAMS` | `fx_actions.rs` |
| the auto-eighths click | `features/guide/src/midi.rs:196` |
| section insertion and guide regeneration | `keyflow/actions.rs:38`, `guide.rs:209` (behind a UI click) |
| the guitar grow actions | `crates/session/session/src/guitar_grow.rs` (REAPER only) |

## Step count

The test has 38 numbered steps across phases 1–14 (1.1–14.1). Step 1.1
asserts nothing, and phase 15 is a follow-on.

- **4 run on today's Action List as they are:**
  - 3.1 (`SESSION_MODE`);
  - 4.2 (`TRACK_COMP_FROM_LANE`, in seconds);
  - 8.1 (`BASS_RIG`, without the browser);
  - 8.2 (`FX_AUTOMATE`, without the Shift-click).
- **6 exist only behind a UI click or in another crate, and need
  wiring:**
  - section insertion with guide regeneration: 4.3, 5.1, 6.0;
  - the grow actions: 10.1, 11.1, 13.1.
- **The other 28 need a MISSING feature.** Recording (M4) blocks the most
  of them: ten steps record or comp recorded takes.

## Open questions

**Decided (user, 2026-10-12):**
- **Q1:** the double's tracks are **Main** and **DBL**, under one folder that carries the rig, panned left and right into it (the Channel behaviour, with the user's names). "Add layer" and "double track" are that one action.
- **Q2:** a part is named for its stack while it is the only arrangement; adding an arrangement renames it to its **role** (Rhythm), the new one takes the given name (Lead).
- **Q6:** drawn drum MIDI lives on **one `Drums/MIDI` track**, feeding each piece's Sample track.
- **Q11:** the Drums button adds the **full golden kit**, folded, even with nothing recorded.


**Q1. The double: "Main/DBL" or "L/R"?**
- The narrative says "add layer" makes `Main` and `DBL`, and "double
  track" on Harmony also makes `Main` and `DBL`.
- Session's guitar vocabulary calls the double a **Channel** (`L`/`R`),
  one that "shares a chain". A **Layer** (Main, Harmony, OCT) "has its
  own" (`workflows.md` `r[flow.guitars.dimensions]`;
  `crates/session/proto/src/guitar_grow.rs:42-45`).
- The organizer also parses `DBL` as a Layer
  (`metadata_patterns.rs:93`; `electric_guitar.rs:323-360` sorts
  `Guitar Clean DBL` into a `DBL` folder). Vocals use Main and DBL as
  layers (`workflows.md:637-638`).
- The narrative's rig placement, one rig on the part fed by both
  performances panned left and right, is the Channel behaviour.
- **Proposal:** the narrative's "add layer" and "double track" are one
  action, `TRACK_DOUBLE` (Channel). Its members are named `L` and `R`
  by default, with `Main` and `DBL` as a per-project naming option that
  the classifier reads as a Channel inside a guitar group. Decide which
  one the test asserts.

**Q2. Is a track named after its stack or its role?**
- 3.3 names the guitar `Guitar Clean` after the stack it loaded. The
  narrative's final state names the same part `Guitar Rhythm`, and from
  phase 9 it plays Drive in the chorus, so "Clean" no longer describes
  it.
- **Proposal:** the stack name goes into the name only while the part
  is the only arrangement. When `TRACK_ADD_ARRANGEMENT` runs, the
  existing part takes its role name (`Rhythm`), and the new one gets
  `name` or the user's rename.
- The alternative: keep `Clean` (what `GUITAR_ADD_ARRANGEMENT` does
  today) and expect the user to rename it.

**Q3. "Add Part" vs "Add Layer".**
- The narrative's "add part" (a Lead folder with Main and Harmony) is
  the template's **Layer** grow, and its "add layer" is the **Channel**
  grow.
- **Proposal:** name the actions by what they do and give the menus the
  user's words:
  - "Add Part" runs `TRACK_ADD_LAYER {mirror: false}` (a Harmony beside
    the Main);
  - "Double" and "Add Layer" run `TRACK_DOUBLE`;
  - "Add Arrangement" runs `TRACK_ADD_ARRANGEMENT`.
- "Part" is already a `Kind` in the golden session
  (`golden_session/kind.rs:55-57`), and BGVs are "arranged by part"
  (`workflows.md:645`). Check that these meanings do not collide.

**Q4. What happens to markers when the tempo changes?**
- SONGSTART is placed before the tempo is set to 68. If markers keep
  their time, SONGSTART lands at bar 2.13 after the change.
- **Proposal:** while the project has no audio, a tempo change keeps
  markers and sections on their bars. Once audio exists, `TEMPO_SET`
  refuses to move anything without `follow: true`.

**Q5. Where do new takes go?** The spec says the lowest lane that is
free over the range (0.5), so verse takes 1–3 sit beside intro takes
1–3. REAPER can also be set to always add a lane. Which should Session
do?

**Q6. Where does the drawn drum MIDI live?**
- The golden kit has `Trig` tracks, described as "what says the drum
  was hit — a recorded trigger or the kit's notes".
- **Proposal:** one `Drums/MIDI` track that is drawn on and sends to
  each piece's Sample track. Writing the notes onto each piece's `Trig`
  is the alternative. Drawing one pattern argues for one track.
  `drum_reference.rs:187-195` already treats a track whose name has
  "drum" and "midi" in it as the written part.

**Q7. "Cymbal swells" in MIDI.** Is a swell a ride or crash roll with
rising velocity, as proposed, or a dedicated swell articulation in the
kit's note map?

**Q8. Harmony range.** The narrative does not say where the harmony
plays. The test assumes the chorus only.

**Q9. "Comp as you go" in Record mode.** Is the comp lane audible
while the next take records (as proposed), and does a new take go
straight onto `C1` when nothing is comped over its range yet?

**Q10. The routing preset's key.**
- A preset can be keyed by the device name alone, or by device plus
  instrument rig (signal's `RigManager` keys by rig).
- How does the preset meet signal's audio-device settings: is it one
  file the two apps share, or does Session read signal's?

**Q11. Is the drum stack always the full golden kit?**
- A composed song has no recorded drums. Is the full recording kit
  (close mics, Sum, Trig) right for a composition, or does Drums add a
  "virtual kit" (MIDI + Sample tracks + OH/Room sample returns) that
  grows into the recording kit when drums are tracked?

**Q12. CI data.**
- The real rigs need NAM captures and IRs. The worship bass profile
  uses Tone Hammer captures, and the kit needs a sample pack. Neither is
  on CI.
- **Proposal:** small fixture profiles and packs under
  `features/signal-fx/fixtures/`, selected by `FTS_BASS_RIG_DIR`,
  `SIGNAL_RIG_DIR` and `SIGNAL_DRUM_LIBRARY`.
- Only state, structure and automation are asserted on CI. Audio
  checks run under `FTS_COMPOSE_RENDER=1`.
