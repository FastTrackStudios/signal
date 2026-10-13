# Session app on Blitz — plan and handoff

The Session app (`apps/desktop`, `session-desktop`) moves onto Blitz
(`dioxus-native`) on desktop and iOS, and becomes a docking app built from
panels. The web build stays on dioxus-web (custom widgets work there too).
Started 2026-09-21, most recently worked 2026-09-22.

## Where things stand (read this first)

Everything below is **uncommitted**, on purpose — the user wants a lot of
local progress before anything gets pushed (see the memory note
`local-progress-no-push` if you have access to it). Three sibling repos are
dirty:

```
daw       — crates/keyflow/keyflow-proto (chord renotation, unrelated to
            this doc's work but sits alongside it), features/standalone
            (a note-length fix, also unrelated) — see git status, these
            predate the Blitz work and are their own thing.
keyflow   — features/engraver/{engraver,proto}, crates/keyflow/keyflow:
            the new `paint` feature and `ChartView` (see below). All new
            code in this session's work is here.
session   — apps/desktop/src/native/ (the Blitz shell), apps/session-daw
            (the studio-as-panel, the toolbar, the transport bar, the
            chart panel), docs/app-on-blitz.md (this file).
```

Run `git status --short` in each of `daw`, `keyflow`, `session` before
touching anything — don't assume this doc is exhaustive, it's a map, not
a diff.

**What works right now**, confirmed by eye in a running window (not just
compiling): the DAW view (arrangement, main toolbar, transport-in-top-bar)
and the Performance view (progress bar with section-click-to-seek, the
live chart panel, the big transport buttons) both run cleanly on
`sessions/Always On Time`. All 474 `session-daw` lib tests pass. The chart
panel's rendering was checked element by element against keyflow-ui's own
output — title, header (artist/composer/tempo/version/footer), section-
label capsules, and section comments all confirmed in the correct font as
of this doc (see "The chart panel" below for the four-round story of how
each one was found).

**What's still rough**, in priority order:
1. **Docking (phase 3) hasn't started.** Panels are hand-composed in
   `apps/desktop/src/native/shell.rs`'s `DawView`/`PerformanceView`, not
   through `dock-proto`'s tree. This is the next big piece — see Phases.
2. **Multi-window (phase 4, spike 0a) needs a dioxus-native patch** before
   it can start — see "Spike results" below, unchanged since 0a was
   written.
3. **Auto-follow on the chart panel is deliberately not implemented** —
   see "The chart panel" below for why (fights manual pan without more
   state).
4. **Memory growth over a long-running session was observed** (measured,
   not fixed — the user said explicitly to defer this and land features
   first; it has not been looked at since).
5. `ChartView::paint` still takes `width_px`/`scale` parameters it no
   longer uses for layout (the Page preset is a fixed size) — harmless,
   but worth trimming in a pass that isn't mid-feature-work.

## What the app is

**Panels** — the modules everything is built from:

| panel | from | today |
|---|---|---|
| ProgressBar | `session-ui` `SongProgressBar` / `SegmentedProgressBar` | done — `apps/desktop/src/native/progress.rs`, click-to-seek |
| Arrangement | `session-daw` `ArrangementWidget` + ruler + scrollbars + main toolbar | done — `session_daw::studio`, `session_daw::toolbar` |
| Mixer | `session-daw` (`mcp::Mixer`, `tone.rs`) | not yet wired into a panel |
| Editor | expression editor (`features/expression-editor`) | not yet wired into a panel |
| Chart | keyflow engraver, painted live (`ChartView`), paginated, pannable | done — `session_daw::chart_panel` |
| Lyrics | lyric sync + an editor mode (`lyric_sync_view.rs`) | not yet wired into a panel |
| Settings | the app's settings panel | button only, no content yet |

**Views** are docking layouts of panels: a main **Performance** view and a
main **DAW** view ship today (hand-composed, not yet through the dock —
see above), and users will eventually make their own (progress bar on
top, DAW left, chart right, …). Views will span **several OS windows**
once phase 4 lands — full docking, tear-off included.

**Chrome**: one top bar across every view, macOS-style — the traffic
lights sit inside it (transparent, full-size-content titlebar), then the
view switcher, then the transport (right-aligned, with its position/tempo/
key pills — `session_daw::transport_bar`), then the **mode**
(Organize … Scoring — `session::modes::Mode`) and Settings. No side rails;
the DAW toolbar no longer carries the modes — it carries REAPER-style
editing toggles instead (`session_daw::toolbar`: metronome, auto
crossfade, grouping, ripple, grid, snap, lock).

**One engine**: every panel reads the app's in-process daw-standalone
session engine (`session_daw::studio::StudioSession`, opened once at
launch), so the transport, the song and the edits are the same
everywhere. The example session is `sessions/Always On Time` (+ its
`.kf`), prepared (organize, chart, guide) the same way `just studio-song`
prepares it — see `native::launch` in `apps/desktop/src/native/mod.rs`.

## Architecture

- **Layout model**: `dock-proto` (daw repo, `libs/dock`) — already has the
  split/tab tree, drop zones, presets, persistence, history and a
  multi-window `DockWorkspace`. Add `PanelId`s for ProgressBar / Editor /
  Chart / Lyrics — **not started**.
- **Dock UI**: a Blitz renderer for it, inline styles only (Blitz and CSS
  files — see the repo's root CLAUDE.md). `dock-dioxus` stays for the
  WRY/web consumers until they move. **Not started.**
- **Panels are components** with no props (context in via `use_context`,
  provided at the root from `StudioSession`), so one panel renders in any
  tile of any window once the dock exists. This part of the design is
  already true of every panel that's been built.
- **Windows**: dioxus-native, one `DockWindow` per OS window (once phase 4
  lands); winit title-bar attributes for the chrome
  (`native::window_attributes`), `drag_window` from the top bar (already
  working).

## Phases

0. **Spikes** — done, see below.
1. **Studio as a component** — done: `session_daw::studio`
   (`StudioSession`, the `Arrangement` panel). No side rails, no modes in
   the toolbar; `blitz_shot` still has its own older copy of a studio
   window (not yet switched over to the panel — low priority, it's a
   benchmark/test harness, not user-facing).
2. **Shell on Blitz** — done: `session-desktop`'s `native` feature
   (default on) launches dioxus-native — top bar (traffic lights, views,
   transport, mode, settings), window drag; DAW and Performance views on
   Always On Time.
3. **Docking** — the Blitz dock renderer on `dock-proto`; Performance and
   DAW views as presets; dividers, tabs, drag-to-dock. **NOT started** —
   panels are still hand-composed in `shell.rs`, not yet through a real
   dock tree. This is the next thing to pick up.
4. **Multi-window** — tear a panel off into its own window; persistence of
   the whole workspace. Blocked on the dioxus-native patch from spike 0a.
5. **The rest of the panels** — Mixer, Editor, Lyrics (with its editor
   mode), Settings.
6. **iOS on Blitz.**
7. **Retire the WRY paths** (`dioxus::desktop` launch, `fts-chrome`'s WRY
   window calls, `session-daw/src/main.rs`).

## Running it

```bash
cargo run -p session-desktop          # feature `native` is on by default
```

Opens `sessions/Always On Time` prepared (organize, chart, guide);
`FTS_SESSION_PROJECT` / `FTS_SESSION_CHART` point elsewhere;
`FTS_SESSION_VIEW=performance` opens on the Performance view (default
DAW); `FTS_SESSION_AUTOPLAY=1` starts playback on open — useful for
watching the chart's cursor and the transport bar move without a hand on
the mouse. The WRY app still builds as
`--no-default-features --features session,charts`.

```bash
cargo test -p session-daw --lib       # 474 tests, all passing as of this doc
```

## Guide levels

The guide instrument (`guide_instrument.rs`) plays the Count and the Guide
cues at `VOICE_GAIN` (−8 dB); the click stays at unity. `Guide::generate`
(session crate) starts the Guide folder at −6 dB (`GUIDE_FOLDER_DB`) for
headroom, but only while its fader is still at unity, so a level someone
has set is kept.

## Mixer, visibility manager (DAW view)

**Mixer** (`session_daw::mixer_panel`): `x` (the profile's "Toggle mixer",
40078) docks it under the arrangement, `mixer_panel::HEIGHT` tall.
`DawPanels` composes the two tiles, and the shell's DAW view renders it.
The widget reuses the painted window's mixer: `mcp::Mixer` records,
`overlay::controls` draws live values, `engine::click`/`drag` produce edits.
`Links` is what the two panels share:
- the arrangement's rows (with a generation counter), so a hidden group
  leaves both;
- edits both ways: each widget predicts its own, and the panel hands them
  to the OTHER widget (never back, or a toggle flips twice);
- the open signal and a toggle request;
- keys: the keymap lives in the arrangement, and a press in the mixer hands
  focus back to it on the next redraw.

Focusing from inside an event used to panic ("RefCell already borrowed")
in the vendored `dioxus-native-dom` `set_focus`. It now waits for the
document to be free. The arrangement also takes focus when it mounts;
before that, no shortcut worked until the first click. Not in v1: the
routing panel, rename, folder fold, the Tone rack.

**Live mode strips** (`Settings::live_strips`, `strip::shape`/`fx_section`):
in Live mode (the top bar's mode, provided as context by the shell, and
the default the app opens in for now) the strips keep their FX row but
have no input section. The coloured band is the pan knob's 33px with the
arm beside it, and the freed height goes to the fader. (A version without
the FX row was tried and looked cut off.) `FTS_BENCH_LIVE_STRIPS=1` with `FTS_BENCH_MIXER=out.png` renders
them headless.

**Visibility manager**: `v` opens its tree (the profile's
`FTS_VISIBILITY_MANAGER_*`). The groups come from
`dynamic_template::visibility::groups`, the same name classification the
REAPER actions use (moved out of `daw_module`). The widget flips
visibility on its copy of the raw project, re-plans the rows in-process
(`studio::Planner`, split out of `read_back`), restructures, and sends
`Edit::SetVisibility`. The panel's scroll range follows via `content_h`.

## Which-key and the zoom keys (DAW view)

`z` opens the profile's "Zoom" tree in a which-key popup
(`session_daw::which_key::Panel`, bottom-right, inline styles, one DOM
shape, hidden with `display:none`). The keymap and its labels come from
the FTS profile, **embedded** by `input-keybinds::embedded` in the daw repo,
so an installed app has the same keys wherever it starts.
`FTS_INPUT_PROFILE=<dir>` overrides it for a profile being edited.
`input-keybinds::which_key_labels` keeps the names the flattened keymap
drops. `keys::Keys` mirrors the pending chords, reports key-ups (so a held
`z` is sticky: hold it and tap `t`, `v`, both fire) and lists continuations.
Tap `z` and the popup waits for the next key. Hold `z` and drag or wheel,
and it's the zoom tool: the popup hides and the prefix is dropped on
release (`tool::Pointing::tool_used`). Escape cancels.

Zooms (`zoom::Command`, mapped from the profile's REAPER/SWS ids): `z t`
selected tracks (+ time selection), toggle; `z v` all tracks; `z x` whole
project; `z s` time selection or selected items; `z z` the same as a toggle;
`z f` selected items; `z u` / `z r` back and forward; `+`/`-` (Shift: rows)
steps. The widget resolves what to frame in session units
(`zoom::Request`); the panel turns it into zoom and scroll
(`zoom::frame`, `zoom::History`). Unimplemented bindings (`z h`, `z i`,
`z m`, `z p`) are listed dimmed.

## Tools and the mouse pointer (DAW view)

`session_daw::tool` holds the tool the Arrangement panel has up (`Zoom` on
a held `z`, `Pan` on a held middle button) and the pointer's shape. The
panel (`studio.rs`) sets the tool; the widget (`widget.rs`) stands down
while one is up. Its `stands_down` turns away presses, moves and any
non-left button, but always lets the left button's release through, so a
gesture begun before the tool still finishes. Otherwise the shape comes
from the mouse map: `resolve(context, Drag, mods)` → an icon (trim
arrows on edges, crosshair for the Ctrl razor, copy on Alt, `NsResize` on
knobs). The shape is set on the winit window directly, not with CSS:
Blitz only re-reads `cursor` when the pointer crosses into another node,
and the whole arrangement is one node. The widget applies it on every
pointer event; the panel applies it when a key alone changes things
(a modifier, `z`). The zoom tool: drag down = in; `z`+wheel zooms time, Shift+`z`+wheel the rows, both about the pointer; a zoom-drag zooms about where it started (`studio::zoom_about`). `z` is matched by PHYSICAL key, since Shift makes the character "Z".

**Row control band** (`tcp::band`): 24px, 6px from the row top, on every
row whatever its height. Only rows too short for it shrink it. Painting
(`draw_row`) and hit-testing (`row::Row`) both call it. Routing and FX show
on any row whose band is at least `KNOB_LEGIBLE` tall, not only on full
rows. The edit cursor and time selection clamp to the lanes' left edge,
as the play cursor already did.

## Installing it (macOS)

```bash
just macos-install    # build Session.app + the signed .pkg, install to ~/Applications
just macos-pkg        # just the .pkg (target/Session-<ver>-macos.pkg)
```

`apps/desktop/ios/package-session-macos.sh` builds with the HOST toolchain
(`cargo build --release`, no nix shell, no dx). It assembles `Session.app`
by hand: Info.plist, bundle id `app.fasttrackstudio.session`, and
`icon.icns` made from the iOS icon master by `macos-icon.swift`, which puts
it on the macOS icon grid because macOS doesn't mask icons. It signs with
the Developer ID Application identity, then wraps a Developer-ID-Installer-
signed `.pkg` using `installer-resources/session/`. `NOTARIZE=1` notarizes
and staples it, needed before anyone else downloads it. `MAC_TARGETS=
"aarch64-apple-darwin x86_64-apple-darwin"` builds universal.
`ADHOC_SIGN=1` builds with no certificates. The binary links no non-system
dylibs today; the script bundles any that appear.

**First launch from Finder asks "Session.app would like to access files
on a removable volume"**, because the example session is still hard-coded
to `/Volumes/build-disk/development/sessions/Always On Time`
(`native/mod.rs`). Until you click Allow the app blocks with no window.
Launching from a terminal borrows the terminal's permission, which is why
`cargo run` never showed it. That path is the next thing to fix for an
install that works on another machine: open a setlist, or ship an example
session inside the bundle.

## Blitz notes

- **`inset:0` does nothing** — write `top:0; left:0; right:0; bottom:0`. A
  panel that fills its tile with `inset` (or with `height:100%` inside a
  flex item) comes out zero tall.
- A style sheet goes in as a plain `style { }` element, not
  `document::Style`.
- Wheel events never reach the DOM; panels read them at the winit level
  (`use_window_event`) and filter by their own rect (`get_client_rect`).
- A `RefCell`'s `borrow_mut()` LHS and a `borrow()` on the RHS of the same
  assignment statement are not ordered — split them into two statements,
  or it panics ("already borrowed") on a live wheel/drag handler. Bit us
  once already in `chart_panel.rs`'s wheel-zoom handler.
- On macOS, winit tells a trackpad's two-finger scroll apart from an
  actual mouse wheel by the `MouseScrollDelta` variant: `PixelDelta`
  (smooth, both axes at once) is the trackpad; `LineDelta` (discrete,
  single axis) is a real wheel. Useful for matching a browser's own
  split (trackpad scroll pans, wheel/ctrl+scroll zooms) — see the chart
  panel's interaction below.

## The chart panel — matching keyflow's own renderer

`session_daw::chart_panel::Chart` paints a live, paginated chart (glyphs,
section labels, the playback cursor) with
`keyflow::engraver::renderer::ChartView` — a new type added to keyflow
(gated behind a new `paint` feature on `engraver-proto`/`engraver`/
`keyflow`: layout + cursor + paint, no wgpu window).

Getting the rendering to actually MATCH keyflow's own output (not just
compile) took three rounds, each one found by comparing against
keyflow-ui's own reference renderer, `crates/keyflow/keyflow-ui/src/
chart_renderer.rs` — read that file first if something still looks off,
before guessing:

1. **`spatium` mismatch.** `SceneRenderConfig::default()`'s render-time
   `spatium` is 10pt; the layout config's is whatever the preset says
   (Page/`master_rhythm()`: 5.0; the Responsive preset this used to use
   before step 2 below: 7–12pt per breakpoint). Every SMuFL glyph
   (noteheads, slashes, clefs) rendered at the wrong size relative to the
   barlines and text around it. Fixed by reading `config.spatium` off the
   SAME `ChartLayoutConfig` used for layout and building the renderer with
   `SceneRenderBuilder::new().spatium(spatium).build()`. Also: the
   cursor's SMuFL font was `None`, should be `Some(bundle.smufl_font())`
   (keyflow-ui passes the same) — fixed alongside.
2. **Wrong preset entirely.** `ChartView` used to call
   `Preset::Responsive` (a phone/tablet single-column reading view).
   keyflow-ui's own default is `Preset::Page` with `Paper::Letter`
   (`ChartLayoutManager::new()`'s `paper: Paper::Letter,
   last_preview_mode: PreviewMode::Page`) — a paginated document, which is
   also what "it needs to be in pages" was asking for. Switched over;
   `ChartView` now goes through `ChartPipeline::resolve_preset` (the SAME
   facade keyflow-ui and the CLI call) instead of hand-building the
   `(LayoutMode, ChartLayoutConfig)` pair, so it can't drift out of step
   with keyflow's own table again. Page mode has a FIXED physical page
   size independent of viewport/zoom — `ChartView::paint`'s `width_px`/
   `scale` params are now unused for layout, only for the pan/zoom
   transform (see the note in "What's still rough" above).
   The white background is now painted PER PAGE (`layout.pages`, each at
   its own `x_offset`/`y_offset`/`width`/`height`), with the gap between
   pages left as the panel's own dark background — reads as separate
   sheets, not one long roll.
3. **The "sans-serif" font family doesn't resolve.** This was the actual
   cause of "section labels/artist use the wrong font", found AFTER
   switching to Page mode made it obvious it wasn't a preset problem. The
   header (artist/composer, version, tempo, subtitle, footer) and every
   section-label capsule (`layout_margin_label`, in keyflow's own layout
   code) are written with the literal, CSS-style family name
   `"sans-serif"` — meant to pair with the title/part-name's bold
   `"FreeSans"` as its regular weight, which is exactly what a BROWSER
   resolves it to (this is why the SVG/wasm chart panes never showed the
   bug — a browser has real generic-name resolution). `VelloSceneRenderer`
   has none: an unregistered family silently falls back to the default
   TEXT font, Chicago — so the header and every section label rendered in
   Chicago while the title stayed FreeSans. Fixed in
   `ChartFontBundle::configure_renderer` (`keyflow/features/engraver/
   proto/src/engraver/fonts/bundle.rs`) — one more
   `.with_named_font_arc("sans-serif", self.freesans_font_data.clone())`.
   This fixes it for EVERY consumer of `configure_renderer`, not just
   `ChartView` — keyflow-ui's own WGPU canvas preview mode had the same
   latent bug if it ever hit an unregistered family, though nobody had
   noticed because the SVG path is what people normally look at.
4. **`"section-comment"` had the identical bug, one step removed.** The
   small italic note under a section-label capsule (`layout_margin_label`'s
   comment text — "BRIDGE" under "INST", "DOWN" under "CH 4" in the test
   song) goes through `PaintCommand::section_comment`, which hardcodes its
   OWN family name, `"section-comment"` — a real registry entry (unlike
   `"sans-serif"`, this one WAS registered), but registered to
   `aux_font_data` (Chicago), not FreeSans. It read as "wrong font" for the
   same underlying reason as step 3 — Chicago next to a FreeSans capsule —
   just one level of indirection further away from the obvious fix, which
   is why it wasn't caught in the same pass. Fixed the same way: re-pointed
   `"section-comment"` (and `"section-note"`, which has no current caller
   but is the same conceptual family — aligned pre-emptively) at
   `freesans_font_data` in the same `configure_renderer`. **Two more
   registered-but-never-called family names remain in `configure_renderer`
   pointed at Chicago: `"title-bold"` and `"part-name-bold"`.** Nothing in
   the current codebase emits a `PaintCommand` with either family, so they
   could not have been exercised — if a future keyflow change starts using
   them and something reads "wrong font" again, check these two first
   before re-deriving this whole chain from scratch.

Confirmed identical to keyflow-ui otherwise: `MStyle::new()`/`default()`
(not `lead_sheet()` — keyflow-ui's own doc comment says keyflow-ui and the
CLI use `new()`, `editor-keyflow` alone uses the lead-sheet preset),
`DPI_SCALE = 96/72`, `ChartFontBundle::configure_renderer` for the rest of
the font wiring.

**Interaction** matches a browser (Safari/Chrome's own split, which is
what "the keyflow site" actually behaves like): a trackpad's two-finger
scroll pans (both axes), a real mouse wheel or ctrl+scroll zooms. A
middle-mouse-drag also pans, for anyone with a real mouse. See the Blitz
note above on how winit tells a trackpad from a wheel. Pan is stored in
chart POINTS (`scroll_pt`), not pixels, so it stays physically anchored to
the content as zoom changes — the content's own size in points
(`live.content_pt`) and the device-pixels-per-point factor
(`live.px_per_pt`) are written by the widget after each paint and read by
the window-event handler to convert a screen-pixel delta, one frame
behind, which self-corrects (same trade-off as `ChartView::cursor_y_pt`).

**The cursor** is `CursorConfig::playback()` (keyflow, `layout/chart/
cursor.rs`), a soft blue highlight over the current measure with a thin
1.5pt line at the playhead (`CursorStyle::MeasureWithLine`), and no
notehead glow. It is the look session's web SVG pane already had, now
moved into keyflow as a named preset. `ChartView::new` uses it, and so does
`apps/desktop/src/session_chart_pane.rs`, which dropped its hand-drawn
`<line>`, so the two panes can't drift apart. keyflow's own default
(a thick red line with a notehead glow) is still what keyflow-ui uses.
Confirmed by eye on native with `FTS_SESSION_AUTOPLAY=1`. The web build
could NOT be checked: `cargo check -p session-desktop --target
wasm32-unknown-unknown --no-default-features --features session` fails
before and after this change, because architect's tokio pulls `mio` into
wasm. That's a separate break.

**The count-in offset.** The DAW's transport and the chart's own layout do
not share a zero: the chart's timeline starts at its first REAL measure
(the count-in is a header snippet at negative chart time), while the
transport's seconds run from the project's own start, before any
count-in. `chart_panel.rs` reads the SONGSTART marker once at construction
and subtracts it from every transport read (`songstart_secs`), so a
playhead over the count-in lands on the count-in header rather than
nowhere. Confirmed working by eye (the red cursor line sits over the
count-in's own beat numbers at rest, since the transport starts at 0 and
SONGSTART is a few seconds in).

**v1**, still scoped down from task's reference pane
(`task`'s `crates/player-ui/src/session_chart_pane.rs`, which this and
session's own older SVG pane were compared against for the interaction
model): no auto-follow of the playhead's page — manual pan takes
priority, and the two would fight without a "the user just touched it"
timeout, which is the thing to add before bringing auto-follow back.

## Spike results (2026-09-21, unchanged since)

- **0a multi-window** — possible, needs a patch. `blitz-shell`'s
  `BlitzApplication` holds many windows, but new ones are only created in
  `can_create_surfaces`, and dioxus-native offers no way to open one from
  a running app (`DioxusNativeDocument::new`, the event handlers and the
  net provider are crate-private). Plan: vendor dioxus-native into
  `libs/vendor/dioxus-native` (as `dioxus-native-dom` already is) and add
  an `OpenWindow` embedder event that builds a document + `View` on the
  live event loop with the same context injection as the first window.
- **0b Tailwind under Blitz** — passes. `SongProgressBar` renders
  correctly (segments, played/unplayed shading, labels, rounded ends)
  with the compiled sheet in a plain `style { }` element
  (`bin/blitz_panels.rs`). NOT via `document::Style`, which goes through a
  window's head and does nothing in a headless document. Hovers/
  transitions still to check in a live window.
- **0c iOS** — blitz-shell carries iOS paths (window, soft keyboard,
  simulator deps in dioxus-native). Verify on the simulator in phase 6.
- **0d Chart** — done, see "The chart panel" above for the full story
  (it took three rounds to match keyflow's actual output, not just to
  compile against its API).

## Open questions / next cleanup

- Trim `ChartView::paint`'s now-unused `width_px`/`scale` layout role —
  they still convert the pan/zoom transform, so keep the params, just
  update the doc comment's framing (already partly done) and consider
  whether `PresetOptions::for_screen(612.0, 1.0)`'s placeholder numbers
  (ignored by `Preset::Page`) deserve a clearer constructor upstream in
  keyflow (`PresetOptions::for_page()`?) instead of reusing screen
  defaults that don't apply.
- Auto-follow for the chart panel (see above) — needs a pan-recency timeout
  before it can coexist with manual pan.
- Docking (phase 3) is the actual next milestone; everything else in this
  doc is either done or blocked on it or on phase 4's patch.
- Memory growth over a long session — observed, not investigated. Deferred
  by explicit user instruction; pick up when asked, not proactively.
