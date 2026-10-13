# Frame themes: the arrangement, TCP and mixer as runtime-swappable frame documents

Status: the mixer strip is themed (2026-10-02). Spans this repo and
[frame](https://github.com/FastTrackStudios/frame).

## Where it stands

- **Frame** (branch `theme-runtime`) has tokens with modes, host binding
  (`bind:*` over `theme-expr`), compiled per-size plans with record and live
  passes, **components** (`<Component arg:…>`, placed by
  `<Instance of="…" arg:…>`, expanded when the theme compiles) and
  **layers** (`Theme::from_sources`: pages, components and tokens each
  replaced by name, so a theme on another is only the files it changes).
- **The mixer strip** draws from a theme in the Mix and Record mixer views.
  `FTS_THEME` is a `:`-list of files and folders, base first, hot-reloaded.
  - `themes/console/` is the console layout as hand-edited files:
    `tokens.frame`, `controls.frame` (each control a component) and
    `strip.frame` (the strip placing them by name).
  - `themes/fts-dark.frame` is REAPER's layout, still generated from the
    painters (`skin::export`) because it is the reference the native strip
    is held to, pixel for pixel.
- **Still native:** the rails, the Tone rack, the strip's edge, selection and
  folder steps, the Live-mode strips, the TCP and the arrangement.

## Goal

The arrangement, the track control panel (TCP) and the mixer (MCP) are drawn
from a **theme** — frame documents loaded at runtime — instead of from
hand-written painters. Concretely:

- **Swap at runtime.** Picking another theme repaints every view on the next
  frame. No relaunch, no rebuild.
- **Edit at runtime.** Saving a theme file hot-reloads it into the running
  app, the way frame's player already reloads a face.
- **Any look.** One theme reproduces today's look exactly; another looks like
  REAPER; users make their own.
- **No performance regression.** This is a gate, not an aspiration: see
  [The gate](#the-gate).

## What exists

**Session's painters** (`apps/session-daw`) have three parts for every view,
and a theme has to supply all three:

1. **Geometry.** `strip.rs` (mixer), `tcp.rs` / `row.rs` (TCP) and
   `arrangement.rs` compute where every control sits, once per size. Drawing,
   hit-testing and the overlay all read the same geometry, so they cannot
   drift apart.
2. **A recorded scene.** `mcp.rs` and `widget.rs` record a static
   `anyrender::Scene` in content space and replay it under a transform, with
   an index that culls off-screen strips and rows.
3. **A live pass.** `overlay.rs` draws whatever moves (fader caps, meters,
   lit buttons) over the replay every frame, so a mute or a fader move never
   re-records.

Colours come from `daw_ui::theming::Theme::dark()`, converted once to
`arrangement::Palette`. Sizes come from `daw_theme_art::geometry`. Neither can
change at runtime.

**Frame** already has these parts for one surface at a time:

- `frame-embed` paints into an anyrender 0.11 scene in two passes, `record`
  and `live`. It uses session's width tiers and has a frame version of the
  Tone rack.
- `surface-runtime` routes pointer gestures and runs meter ballistics.

**What frame lacks** (none of this exists in `document-proto` today):

| Missing | Why the arrangement/TCP/MCP need it |
|---|---|
| **Variables with modes** (tokens) | Switching colour mode without rebuilding a document. Listed as "Next" in frame's roadmap. |
| **Components, instances and props** | A mixer is one strip component drawn 200 times with different data, not 200 copies of nodes. |
| **Host data binding** | Bind to a track's name, colour, volume, mute or meter, not only to a surface catalog's params. |
| **A compiled form** | `frame-embed` builds a view with taffy and paints the node tree. That is fine for one rack; 200 strips × every zoom step needs the layout solved once and replayed. |
| **Change classification** | Today a value change either re-records the whole embed or is drawn live. The roadmap's `ChangeKind {Value, Paint, Layout, Full}` is unbuilt. |

## Design

### A theme

A theme is a `.frame` bundle holding:

- **Tokens.** Variables with modes (`dark`, `light`, `stage`, …). Colours,
  radii, font sizes and spacing reference tokens, never raw hex.
- **One component per role.** For example `mcp.strip`, `mcp.master`,
  `tcp.row`, `tcp.folder`, `arrange.lane`, `arrange.item`, `arrange.ruler`,
  `arrange.marker`. The vocabulary comes from daw-ui's `ElementRole` plan
  (`features/ui/daw-ui/THEMING_PLAN.md`) and REAPER's WALTER layout names.
- **Variants** by size tier and state (narrow/wide strip, row density,
  selected, armed, folder depth), the way WALTER switches layouts on height
  and width.
- **Bound slots.** Nodes bind to host fields (`track.name`, `track.color`,
  `track.volume`, `track.mute`, `meter.peak_l`) and declare whether they are
  *static* (part of the recording) or *live* (drawn in the live pass).

### The host contract

For each role, session hands frame a typed record of the instance's data
(`StripData`, `RowData`, `ItemData`) and gets back:

- the **resolved geometry**, with named control rects. This replaces
  `strip.rs` / `tcp.rs` constants for hit-testing; `mcp::Control` and
  `row::Control` map to the theme's control names.
- a **record** into the host's scene.
- a **live** paint for the bound live slots.

The data-heavy content stays host-painted: waveforms and peaks, MIDI notes,
EQ curves, text shaping. The theme positions and styles these as **plot
slots**; the host fills them. This is the same seam frame already uses for
`register_curve`.

### Compiling a theme

Loading a theme compiles it to a `CompiledTheme`. For each component, variant
and size bucket it holds:

- the **solved layout**: no taffy at record time;
- a **paint plan**: a flat list of primitives whose fills, text and visibility
  have holes for tokens and bound fields.

Recording an instance replays its plan into the host scene with the
instance's data substituted in. That is the same amount of work today's
painters do, which is why a plan, rather than the node tree, is what the gate
is measured against.

Change classes:

| Change | Cost |
|---|---|
| live slot (fader, meter, lit button) | live pass only, as today |
| static field (name, colour) | re-record that one instance |
| mode switch | re-resolve tokens, re-record |
| size crossing a bucket | re-solve that bucket's layout (cached) |
| theme swap / hot reload | compile off-thread, swap one `Arc`, re-record everything |

### Swapping

`ThemeHandle` is an atomically swapped `Arc<CompiledTheme>` with a version
number. Every widget compares the version in `needs_redraw`/record and
re-records when it changed. A compile error leaves the previous theme in
place and shows frame's error banner.

## The gate

Each phase that moves a view onto frame runs the existing headless benches
against the native painter and **must not be worse**:

- `just daw-bench`: the arrangement sweep, 5120×1440
- `just daw-animate`: every mixer control moving, 2560×1440
- `just daw-studio`: arrangement + editor + mixer against 240 Hz

The threshold is p50 and p99 within noise of native (two runs each,
interleaved, with machine load noted; this box runs other agents). A theme
swap must compile off the UI thread and cost the UI thread no more than one
full re-record.

A second gate covers correctness: the `fts-dark` frame theme must match the
native painter pixel for pixel. `blitz_shot` / the bench's image renderer
provides the native image, and frame's visual-diff harness compares the two.

## Phases

0. **Baseline and plumbing.**
   - Record bench numbers on `main`.
   - Bring frame onto session's dependency set (architect v0.9.2, facet/vox
     rc.7) and tag a release containing `frame-embed`.
   - Add a `Skin` seam in session-daw that the current painters implement as
     `NativeSkin`, with no change in output or bench numbers.
1. **Frame core** (in frame): variables and modes, components and instances
   with props, host binding, compile-to-plan, and plan replay into anyrender
   0.11. Frame-side micro-benches.
2. **The mixer strip.**
   - `fts-dark` `mcp.strip` and `mcp.master` as frame components, behind a
     feature flag.
   - Hit-testing from the theme's geometry.
   - Must pass the golden diff and the `daw-animate` gate.
3. **TCP rows and folder bands.** Gate: `daw-bench`.
4. **Arrangement chrome:** lanes, items, ruler and markers, with host plot
   slots for peaks and notes. Gate: `daw-bench` and `daw-studio`.
5. **Runtime switching:** a theme picker, a themes directory, hot reload,
   and mode switching.
6. **A REAPER theme.** Built from `daw-theme-art`'s vector art and WALTER
   layouts, then the Tone rack through frame-embed in the same theme.

Wiring the Tone rack to the engine (fx-blocks' `NativeFxFactory` in
daw-standalone, issue #105) does not depend on any of this and can proceed
in parallel.

## Open questions

- **Authoring format.** Settled: frame markup files on disk are the source
  of truth (the console theme). Generated files are a starting point or a
  reference, not where a theme is edited.
- **Text.** frame-embed shapes text with its own font path; session has
  `text::Font`. One of them should own glyph caching for both.
- **Web build.** The web build replays the same scene through vello_hybrid,
  so plans must stay free of GPU-only effects, or degrade the way
  frame-embed's paint already does.
