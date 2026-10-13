# Performance testing the DAW expression editor UI

This guide is for developers and coding agents working on Session's workstation UI.
The harness mounts the production `WorkstationApp` with the TCP, arrangement, MCP
and drum expression editor. It uses `dioxus-test` to dispatch events through the
Blitz DOM, including hit testing, bubbling, focus and native scroll defaults.
There is no OS mouse/keyboard driver and no second implementation of the UI.

## Start here

From the repository root:

```sh
just ee-stress set-in-stone 120 dev
```

This builds the harness, creates a fresh practice copy of Set in Stone and its
referenced audio, waits for project shape and waveform previews, checks every
navigation gesture, runs eight workloads, and writes a screenshot plus reports.
The printed temporary directories are retained for inspection and practice.

The arguments are `SONG FRAMES PROFILE ENFORCE`:

```sh
just ee-stress unbreakable 120 release
just ee-stress set-in-stone 240 release true
```

`true` makes a completed run fail if **any** measured DOM frame exceeds
`1000 / 120 = 8.333… ms`. Without it, a budget miss is recorded in the report and
the command succeeds. Crashes, missing panes and ineffective gestures fail either
way. A two-frame run is useful as a smoke test; it does not cover direction
reversals or establish sustained performance. Use at least 120 for comparisons.

The normal Rust toolchain, Python 3, `just`, fonts and the Crescendum album files
are required. No display server, desktop focus, audio output or REAPER process is
required. Dependencies come from the workspace's pinned Cargo graph; do not update
Dioxus or Blitz versions merely to run this test.

## Iterate faster

`ee-stress` is the recipe of record: a fresh copy, all eight phases, 120
frames. It is not the loop you want while trying six ideas an hour.

```sh
just ee-bench                                  # cached staging, 40 frames
just ee-bench set-in-stone 120 drum_pan        # one phase, full length
just ee-bench-compare BASELINE_DIR CANDIDATE_DIR
```

`ee-bench` reuses ONE staging per song under
`/tmp/fts-drum-practice-cache` (`EXPRESSION_EDITOR_PRACTICE_CACHE` moves
it). That is worth more than the copy it saves: peaks are cached as
REAPER-compatible `.reapeaks` sidecars beside the media, and a fresh copy
throws every one of them away, so a cold staging rescans the PCM of every
take before the window is ready. Warm, a run is about 35 seconds; cold,
it is minutes. Nothing in the benchmark writes to the staging, and the
staging is still a copy — originals remain out of every write path.

The rebuild, not the run, is what costs you now. Keep changes to leaf
crates where you can, and keep the binary from a run you may want to
compare against: `cp target/release/examples/stress /tmp/stress-<name>`
lets you interleave A and B under the same machine load, which is the
only way to compare on a box someone else is also building on.

**A short run is a signal, not a result.** 40 frames does not reverse
direction the way 120 does, and — measured, not theorised — it leaves the
drum camera clamped at the edge of the song, where the pan and vertical
phases move nothing and report sub-millisecond medians for an idle
window. Confirm anything you intend to report with a 120-frame
`ee-stress`, and read the `Moved` column before you believe a number.

## Protect source material

Use `just ee-stress` or `just ee-practice-prepare` to stage recordings. The practice
workflow copies files, remaps project references and redirects save/render paths;
it does not use hard links. The benchmark itself performs navigation gestures,
not audio rendering or editing commands.

Set `EXPRESSION_EDITOR_PRACTICE_ALBUM` if the album is elsewhere. `TMPDIR` controls
where practice copies and reports are created. Allow several GB for each practice
project. Use one song per benchmark invocation; `both` is intentionally rejected.
Keep original recordings and source `.RPP` files out of experimental write paths.

Once a practice project exists, reuse that **same scratch project** while comparing
changes. This avoids repeated multi-GB copies and keeps the project constant. Do
not clean up someone else's temporary directories or terminate unrelated builds.

## Repeat or isolate a workload

Build the current source before every measurement. The profile label in a report
is supplied by the caller; it does not change an already-built executable.

```sh
cargo build -p expression-editor-standalone --example stress --release

stress_project='/tmp/your-practice-directory/set in stone.practice.RPP'
stress_report=$(mktemp -d -t fts-ui-stress-XXXXXX)
printf 'Report directory: %s\n' "$stress_report"
RUST_BACKTRACE=1 FTS_STRESS_PROFILE=release FTS_STRESS_FRAMES=120 \
  target/release/examples/stress "$stress_project" \
  --drums --size 1600x900 --out "$stress_report" \
  > "$stress_report/run.log" 2>&1
python3 scripts/ui-stress/run.py "$stress_report"
```

Set `FTS_STRESS_PHASE=all_panels` on the executable invocation to isolate a phase.
Other accepted phase names:

| Phase | Input | Primary observation |
|---|---|---|
| `tcp_vertical` | Vertical wheel | Shared TCP/arrangement scroll |
| `arrange_horizontal` | Horizontal wheel | Timeline scroll, including ruler |
| `arrange_zoom` | Ctrl + wheel | Arrangement time scale |
| `mixer_horizontal` | Horizontal wheel over the strip header | MCP scroll and strip virtualization |
| `drum_pan` | Horizontal wheel | Drum time camera |
| `drum_vertical` | Vertical wheel | Drum lane scroll |
| `drum_zoom` | Focus, hold Z, pointer drag, release | Production zoom tool |
| `all_panels` | All seven inputs in one measured batch | Combined updates, layout and mount/unmount pressure |

Directions reverse every 30 frames. Each zoom batch currently includes a complete
held-Z drag, so this also stresses tool activation/restoration. The combined phase
is deliberately heavier than any individual gesture. Even an isolated run loads
the full workstation and performs all navigation checks before timing.

The editor resolves wheel gestures from the shared input configuration. Keep that
configuration constant between runs. An unbound gesture can cause readiness checks
to fail; do not silently substitute direct camera mutations to make the test pass.

## What the numbers mean

A measured frame is one input batch followed by pending Dioxus work and a Blitz
style/layout resolve. It is driven as fast as possible, without a 120 Hz sleep.
This measures processing cost, not the operating system's display cadence.
Input handlers update signals synchronously. The entire batch is dispatched against
one resolved DOM; then Dioxus mutations are drained and layout is resolved before
the next batch. Do not insert a drain between pointer events without also resolving
layout: removed/replaced nodes can leave the previous hit-test layout stale.

- `event_ms`: dispatch of the complete input batch, including the held-Z gesture.
- `update_ms`: draining pending Dioxus work after the input batch.
- `layout_ms`: Blitz resolve, including style/layout and its associated work.
- `total_ms`: wall-clock duration of all three stages, with no discarded stalls.

Loading, waveform warmup, readiness assertions, JSON output and PNG encoding are
outside the timed batch. The harness includes ordinary mounted application tasks,
so timer-driven work may also land in the measured update stage. It does not test
real audio playback, network latency, GPU rendering, compositor scheduling,
physical input latency or a live REAPER panel.

**Passing this DOM budget does not prove 120 FPS.** Paint and presentation still
need time inside the same 8.333 ms deadline. A DOM failure already shows that the
complete pipeline cannot meet that deadline for this workload. A DOM pass is a
necessary checkpoint before measuring the native GPU/presentation path. Never
label headless throughput or CPU screenshot speed as monitor FPS.

## Read the artifacts

- `report.md`: per-phase p50/p95/p99/worst durations, over-budget counts,
  and the `Moved` column — how many of a phase's frames actually changed
  its pane. A phase that moved on none of them and still cost less than
  the frame budget fails the run outright: that is an idle window, not a
  fast one. A phase that moved on none but cost real time (a clamped zoom
  still re-renders) is reported, not failed.
- `report.json`: per-stage distributions and run context.
- `samples.json`: complete run metadata and every measured frame.
- `samples.jsonl`: incremental completed-frame journal; survives a later crash.
- `workstation.png`: final full-workstation image, painted outside measurement.
- `failed-gesture.png`: diagnostic image when a preflight gesture has no effect.
- `run.log`: captured by `just ee-stress`, or by the redirection shown above.

`report.json` records source project, loaded track/item counts, viewport, profile,
OS/architecture, CPU/load, Git revision/status, and `dom_nodes` — a node
count per pane, taken after the run. The layout stage is the one that
scales with it, so a change that claims to have cut the document down
should be able to show this number falling. Dirty-worktree results need the
actual patch preserved alongside them. Keep baseline artifacts outside disposable
build directories when they are needed for review.

A missing `samples.json` or an exception from the report tool is **not a pass**.
The JSONL journal can explain the last completed phase/frame, but must not be
summarized as a completed full run. Also check whether a result used phase isolation
before comparing it to a full-workstation run.

## An optimization loop for agents

1. Read applicable repository instructions and this guide. Identify the exact
   pane/gesture and an observable correctness constraint, such as ruler alignment,
   restored mixer strips, preserved zoom anchor or unchanged audio sources.
2. Capture a baseline with the intended profile, fixed scratch project, viewport,
   frame count and input configuration. Record machine load. Prefer an otherwise
   idle machine; if that is unavailable, label the measurements as contended.
3. Read the stage distributions. High update cost suggests excessive rerenders,
   signal subscriptions, allocations or rebuilt item trees. High resolve cost
   suggests DOM size, style invalidation or layout work. These are hypotheses to
   profile, not proof of a specific cause. One of them has already been tested
   and did NOT hold: cutting the TCP column's mounted nodes by 44% did not make
   the combined phase faster, because the frames that cost anything are the ones
   that re-rendered, and mounting fewer rows meant re-cutting the window more
   often. What the resolve stage tracks is *dirty* nodes, not the document's
   size — so a change that shrinks `dom_nodes` still has to show a time.
4. Watch the machine. On a shared build box this benchmark has been seen to
   swing 50% run to run on identical code (9.6x at a load of 24, 14.4x at 48).
   Differences smaller than that cannot be resolved here at all: interleave A
   and B binaries, repeat, and read `load` in every report before believing a
   delta.
5. Make one bounded change. Keep navigation handlers shared with production. Favor
   stable keys, scoped signal subscriptions, parent-owned scroll containers, cached
   derived data and viewport-bounded rendering where measurement supports them.
6. Run the focused correctness regression, then repeat the same benchmark. Compare
   tail latency, worst cases and over-budget counts as well as the median. Never
   remove stalls, reduce the project, disable visible panels or change the profile
   to manufacture an improvement.
7. Repeat measurements when evaluating a performance claim; compare multiple runs
   under similar load. Recheck the full combined phase after isolated improvements.
   Test both songs before calling a change broadly scalable.
8. Report the exact command, project/viewport/profile, baseline and candidate paths,
   correctness checks, affected percentiles, budget failures and remaining limits.
   Separate measured findings from assumptions. Do not claim 120 FPS without a
   native rendering/presentation measurement.

Keep an optimization only when it preserves behavior and has repeatable evidence.
A faster blank/offscreen view or an ignored gesture is a test failure, not progress.

## Reading the surface's own meter

The toolbar's right-hand readout is three numbers, and a stutter is
invisible in any one of them alone:

    118 fps · roll 1.20ms · 240/s
    ^^^^^^   ^^^^^^^^^^^^   ^^^^^
    frames   how much of a  renders of the surface
    actually frame this     per second
    presented surface costs

- **fps** is presented frames, never renders or DOM mutations. On native
  it is counted in `RollWidget::paint`, which the renderer calls; on a
  WebView it comes from `requestAnimationFrame` in the page
  (`frame_meter.rs`), averaged over a window and reported a few times a
  second — a bridge crossing per frame would be a measurable share of
  what it is measuring. On the WebView the second number is the WORST
  frame in that window rather than a mean, because a drag that stutters
  averages well and feels terrible.
- **renders/s far above fps is the diagnosis you want.** It means the
  surface is rebuilding for events no frame ever showed, which is what a
  high-polling-rate mouse does to a drag. The fix is upstream of the
  renderer — coalesce the events or make the render cheaper — and no
  amount of drawing faster will help.

There is no dedicated Dioxus profiler. `dioxus-core` instruments
`VirtualDom::run_scope` with `tracing` at `trace` level, so
`RUST_LOG=dioxus_core=trace` gives per-scope render spans; on a WebView
the engine's own devtools timeline is better than anything available from
Rust, and is a reason to do performance work there.

## Getting the window's own words back

The `dioxus-mcp-probe` crate is installed in both launchers, so renders,
signal writes and **panics** land in `target/dioxus-mcp/events.jsonl` and
`dioxus-mcp`'s `runtime_events` tool reads them back — no pasting
scrollback, and an agent can look for itself. A no-op in release builds
unless its `force` feature is on.

That matters because `dx serve` captures the child process's console into
its own TUI: piping the recipe catches `dx`'s output, not the app's, so a
panic in the window otherwise reaches nobody.

Its static lints are worth running before hunting a crash by hand.
`check_rsx`'s `missing_key_in_for_loop` found fifteen unkeyed loops in
`stack/markup.rs` in seconds — and an unkeyed loop is not a style nit
here: zooming changes how many ticks, hits and bands exist, so without
keys dioxus diffs them positionally and pairs a node with whichever one
now sits at its index.


Both `just ee-practice` and `just ee-webview` tee everything to
`target/ee-<which>.log`, and `just ee-log [which] [lines]` reads it back
with repeated messages collapsed to a count. A thousand copies of one
warning then read as one fact rather than a wall of scrollback, and it
gives an agent something to look at that is not a screenshot of a
terminal.

`WARN Changing the props of `Style {}` is not supported` is worth knowing
by sight. `document::Style` cannot restyle the head in place, so it warns
whenever its props differ from the ones it first saw — and because it
warns *per render*, a flood of it is really a report that whatever holds
it is re-rendering constantly. The fix is never to quiet the warning: put
the sheets in a component with no props, which nothing can invalidate, so
they mount once. `velocity_panel.rs`'s `PanelStyles` is the pattern.

## Profiling tools worth reaching for

In rough order of value for this UI:

**The WebView's own inspector — already on, and free.** dioxus-desktop
sets `with_devtools(true)` whenever `disable_context_menu` is false,
which defaults to `!debug_assertions` — so any debug build (everything
`dx serve` produces) has it. Right-click → Inspect Element, or
Ctrl+Shift+I. Its Timelines tab has a **Rendering Frames** mode that
plots each frame's height as the time it took, broken into script,
layout, paint and composite. Nothing reachable from Rust comes close for
the WebView target, and it is sitting there unopened.

**samply** for the native build. A sampling profiler that needs no
instrumentation and opens the result in the Firefox Profiler UI:
`samply record target/release/examples/workstation …`. The right first
question for "where do the 5 ms go".

**tracing-tracy** when frame-level detail is wanted. Tracy is a
real-time, nanosecond-resolution frame profiler, and `tracing-tracy`
feeds it from `tracing` spans — which matters here twice over: this repo
already mandates tracing, and `dioxus-core` instruments
`VirtualDom::run_scope`, so per-component render times arrive without
adding a single macro. Note Tracy's model does not represent spans that
enter and exit on different threads, so async work needs care.

The `profiling` crate is already in the lockfile transitively; it is a
thin abstraction over tracy/puffin/optick if a backend-agnostic
instrumentation ever seems worth it.

### Linux WebKitGTK environment variables

There are three that circulate for WebKitGTK trouble, and they are
crash workarounds rather than optimisations — reach for them only for
the symptom each names:

- `__NV_DISABLE_EXPLICIT_SYNC=1` — Wayland protocol errors, no
  performance cost.
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` — the DMABUF framebuffer error and
  the "Error 71" crash, at the cost of the faster rendering path.
- `WEBKIT_DISABLE_COMPOSITING_MODE=1` — **disables accelerated
  compositing entirely.** A last resort for silent crashes on resize,
  and the opposite of an optimisation: it takes the GPU out of the
  picture. Worth knowing because it is the one most often copied off a
  forum, and because a headless X server needs it — which means any
  screenshot taken under Xvfb was of a deliberately handicapped
  renderer and says nothing about speed.

## Ask the renderer where the time went

The `layout_ms` stage is not layout. Blitz's `resolve()` is seven phases,
and it will print them itself: add `log-phase-times` to the `blitz-dom`
features in `expression-editor-ui/Cargo.toml`, rebuild, and every resolve
writes a line to stdout. Measured on the combined phase, a frame that
re-renders looks like this:

```
Resolve(1): 53ms (style: 18ms, damage: 1.2ms, construct: 23ms,
                  pconstruct: 6.8ms, flush: 2.0ms, layout: 2.0ms,
                  transform: 85us, c_damage: 168us)
```

Taffy layout is **two milliseconds**. The cost is `construct` (box and
inline-layout construction, which includes parley text shaping) and
`style` (selector matching, plus a full stylo parse of every `style`
attribute that changed — `flush_style_attribute` re-parses the whole
declaration string on every write). A resolve with no damage at all costs
**2.6 ms** for the same document.

So the lever is not the size of the document, it is **how many nodes a
frame damages**, and how much re-parsing and re-shaping each damaged node
costs. Three consequences worth knowing before optimizing:

- Cull the chrome as well as the content. The arrangement's lanes had
  been culled to the viewport since they were first hosted, but the
  ruler over them still drew a numbered node per bar of the WHOLE song —
  ninety-odd of them, all but a few off screen, each carrying text that
  was shaped again every time a zoom moved it. Culling the ruler took
  `arrange_zoom` from 3.6x over budget to 2.1x. When a phase spends
  milliseconds in `pconstruct`, it is building inline layout: look for
  text you are shaping and cannot see.
- Watch what a value change does to an ATTRIBUTE. `Waveform` sized its
  point count from the item's pixel width, so every zoom notch rewrote a
  several-hundred-coordinate `d` string on every visible item — dioxus
  diffs it, blitz re-parses it, the node restyles. Quantising the count
  to a power of two keeps the same detail and writes nothing until the
  item's width doubles.
- Mounting is dear, so mount rarely. A channel strip is ~186 nodes of
  traced art with its own gradients and text; a frame that mounts one
  costs 15-17 ms of `construct` against 2 ms of everything else. Both the
  mixer and the shared TCP/timeline viewport therefore re-cut their
  mounted window in coarse steps with generous overscan, rather than on
  every pixel of travel. Widening the mixer's step from one strip to
  three took that phase from 2.4x over budget to about 1.25x, at the cost
  of ~1200 more resident nodes — a trade worth making, and worth
  re-measuring if the strip ever gets cheaper to build.
- Moving content by changing `left`/`x` on many children damages all of
  them. Moving the same content by changing `transform` on ONE parent
  costs a single small style write — the whole `transform` phase is
  ~100 µs for the entire document. The arrange ruler already pans this
  way; the panes that do not are the ones that cost.
- A long inline `style` string is re-parsed in full whenever any part of
  it changes (`flush_style_attribute` → stylo's `parse_style_attribute`).
  Splitting the constant half into a class is therefore tempting — but
  measure before doing it: `style` also covers selector matching, and
  adding classes gives the matcher more work, so the two effects pull in
  opposite directions. Nobody has measured which wins here yet.
- Dense traced vector art is cheapest as no DOM at all. The roll already
  paints into a Vello scene through `roll_widget::SceneSlot` and costs
  one node. The drum stack has now had the same treatment
  (`stack/paint.rs`): 1885 nodes became 6, and its pan went from 154 ms
  to 6.6 ms a frame — the first phase to fit inside the 120 Hz budget.
  The TCP rows and mixer strips are the same kind of art still expressed
  as thousands of nodes — but see the next point before porting them.

- **Native scrolling is nearly free; our re-render is not.** Suppress
  the signal write so a vertical scroll changes no state at all, and the
  frame costs **0.47 ms with 23 µs of style** — blitz scrolls the pane
  itself and asks nothing of us. Every millisecond above that is a
  component of ours choosing to re-render, so that is where to look.
  (An earlier version of this guide said the opposite, on the strength
  of a freeze test that froze the track panel while leaving the timeline
  subscribed to the very signal being changed. If an experiment says the
  cost is not yours, check that you actually stopped ALL the readers.)

- **Subscribe to the axis you use, not to the viewport.** The shared
  scroll was one `(left, top)` signal, so a VERTICAL scroll re-rendered
  the timeline — every item, grid line and lane — to arrive at exactly
  the horizontal layout it already had: 10-12 ms a frame, for nothing.
  One signal per axis, each read only by the pane that needs it, took
  `tcp_vertical` from 1.3x over budget to 0.9x. A dioxus component
  subscribes to the signals it reads, so the shape of your state is the
  shape of your invalidation.

- **Two readers of the same axis can want different step sizes.** The
  track panel mounts one row per step and must keep up with the scroll;
  the timeline re-renders everything it holds and must not. They now
  take the same vertical scroll through separate signals committed at
  three rows and at a screenful respectively, with the canvas' vertical
  overscan sized to cover the coarser step.

### Porting a pane to a painted scene

`stack/paint.rs` is the worked example and `lane_strip.rs` the smallest
one. The shape is: `use_hook` a `SceneSlot`, a `Labeller` and a
write-once `CustomWidgetAttr`; build the scene during render where
reading signals is safe; emit an `object` sized in explicit pixels
carrying the same gestures the old `svg` had. Four things cost real time
to find, all of them invisible in a code review:

- **Verify with pixels, not with reading.** Render the same fixture
  before and after (`just ee-shots shoot_stack`) and diff them with
  `scripts/ui-stress/imagediff.py`. That found two defects here that
  looked perfectly correct in the source.
- **SVG's `text y` is a baseline; `crate::text::draw` positions the
  run's top.** Parley's positioned glyphs already carry the ascent, so
  an SVG coordinate passed straight through drops the label by exactly
  one ascent — nine pixels on a nine-pixel font. Subtract
  `Shaped::ascent`.
- **`push_clip_layer` is not usable in a replayed widget scene.** Used
  to clip lane content, it discarded everything drawn before it — the
  ruler's section band vanished entirely. Do not reach for a clip layer;
  find an ordering that does not need one.
- **Paint order is the clip.** An SVG that draws `background, content`
  per row gets row clipping free, because the next row's opaque
  background covers the previous row's overflow. Any restructuring that
  hoists all the backgrounds out — which is tempting, because it lets
  shared layers like a beat grid be drawn once — silently removes that,
  and content bleeds a full row down. In a scene the hoist buys nothing
  anyway (a grid is path segments, not nodes), so keep the per-row
  order.

- **A painted pane stops advertising its state in the DOM.** The stress
  harness watches markup to prove a gesture moved something; paint it,
  and a pane that never moves looks identical to one that does. The
  stack now carries a `data-view` attribute with its time origin, scale
  and scroll for exactly this reason. Give any pane you port the same.

`incremental` is load-bearing and easy to lose: it is NOT one of
blitz-dom's default features. `dioxus-native` enables it, the vendored
`dioxus-native-dom` did not, and the two resolve to separate units — so
whether a given binary got incremental layout depended on which crates
happened to share a build. Both now ask for it explicitly. If a resolve
line ever comes back with no `damage:` entry, it has been lost again.

## Debug failures before tuning

Run with `RUST_BACKTRACE=1`, preserve the journal/log, and isolate the last failing
phase. Check the screenshot and pane assertions before adjusting timing logic.
The harness has already exposed two classes of real failure:

- Nested arrangement scroll areas consumed input inside the preview instead of
  moving the host's shared timeline. The workstation now uses parent-owned scrolling;
  standalone previews retain their own scroll area.
- Culling the arrangement to its viewport cut the zoom phase by an order
  of magnitude and made *scrolling* slower, because the signal that told
  the panes where to look was written on every scrolled pixel — so every
  frame of a wheel gesture paid for a re-render and a style/layout
  resolve that the native scroll had not asked for. Blitz scrolls whether
  or not dioxus hears about it; the only reason to hear about it is to
  mount what came into range. The window is now committed in steps
  (`COMMIT_X`/`COMMIT_Y` in `workstation.rs`), covered by the overscan the
  panes already mount. If you add a viewport-driven pane, keep that
  shape: coarse commits, generous overscan, and a test that the overscan
  outruns the commit step.
- Sustained combined input exposed recycled native DOM node IDs during subtree
  replacement. The focused local renderer patch and its provenance are documented
  in [the vendor notes](../../libs/vendor/dioxus-native-dom/UPSTREAM.md).

Do not catch a renderer panic and count the frame as successful. Do not disable
virtualization, omit the crashing phase or bypass event handlers merely to obtain
a green report. Reduce the reproduction, fix the underlying behavior, and rerun it.

## Code map and regression commands

- [Harness entry point](../../features/expression-editor/expression-editor-standalone/examples/stress.rs):
  load/stage, readiness, validation, measurement and raw output.
- [Input workload](../../features/expression-editor/expression-editor-standalone/examples/stress_support/input.rs):
  pane coordinates, DOM input and phase assertions.
- [Report tool](run.py): distributions and strict budget gate; never sends input.
- [Workstation](../../features/expression-editor/expression-editor-standalone/src/workstation.rs):
  the production root shared by the window and headless test.
- [Mixer](../../features/expression-editor/expression-editor-standalone/src/workstation/mixer.rs):
  viewport-based strip mounting and scroll subscription.
- [Practice workflow](../../features/expression-editor/expression-editor-standalone/PRACTICE.md):
  safe copies and the opt-in audio/project preservation regression.

```sh
python3 -m unittest discover -s scripts/ui-stress -p 'test_*.py'
cargo test -p expression-editor-ui --test stack --test stack_slip
cargo test -p expression-editor-standalone --lib workstation::tests
cargo test -p expression-editor-standalone --test arrange_scroll
cargo test -p expression-editor-standalone --lib workstation::mixer::tests
cargo test -p expression-editor-ui --test stack_slip
cargo test -p daw-ui --features web --test folders
cargo test -p dioxus-native-dom --lib lifetime_tests
```

When extending the workload, add a pane/gesture assertion first, use DOM events,
keep setup and output outside the measured interval, retain raw samples and record
any changed workload semantics. A changed workload establishes a new baseline;
its numbers are not directly comparable to the old workload.

## Measuring the WebView window

`ee-stress` and `ee-bench` above drive the **Blitz** DOM through
`dioxus-test`. The WebView build has no such seam — the engine owns
layout and paint — so it is measured by driving a real window on a
display of its own:

```sh
scripts/ui-stress/webview-display.sh run off        # launch, scroll, report
scripts/ui-stress/webview-display.sh measure off    # scroll an already-open window
scripts/ui-stress/webview-display.sh shot           # one screenshot
scripts/ui-stress/webview-display.sh stop           # tear the display down
```

It is the same shape as `daw::test::VirtualDisplay`, which is how the
REAPER panels are already tested: a private Xvfb, a window manager on it,
`xdotool` for input and `import` for capture. The developer's own window
cannot be used — it is a native Wayland surface, so `grim` is refused by
the compositor and `xdotool` cannot see it at all.

### The numbers, and which to trust

| | what it is |
|---|---|
| `stack ready in Ns` | launch to a drum stack with lanes in it. The kit decode is the slowest thing in the process, so a tax on every thread shows up here first. |
| `travelled` | pixels the picture changed over the gesture. **Read this before anything else.** |
| `backlog` | pixels that kept changing for two seconds after the last notch — input queued and the view still going where the hand no longer is. |
| `fps` | the page's own `requestAnimationFrame` rate. Reported last and trusted least. |

**A high frame rate next to a large backlog means the UI is
unresponsive, not fast.** The rate is the *engine's* presentation loop,
and it stays cheerful while the Rust side is too backed up to send it any
mutations — the tell is the reading count (`n`), which is how many of the
meter's twice-a-second reports the Rust side managed to drain. `n=8` over
a twenty-second gesture is a starved event loop wearing a good number.

### Traps that have each cost an hour

- **`xdotool click --window <id>` does nothing.** It sends `XSendEvent`,
  and GTK/WebKit drop synthetic events. The run completes, the picture
  never moves. Drive the real pointer (`mousemove` then `click`, no
  `--window`) so the events go through XTEST.
- **The camera clamps at both ends.** A gesture from a limit moves
  nothing and reports beautifully. Scroll both ways and take the larger
  leg — which is what `measure` does.
- **A symmetric gesture ends where it started.** Measure travel at the
  midpoint, not from start to end.
- **`tracing`'s fmt layer colours the field name**, so the bytes on disk
  are `ui.fps<esc>[0m<esc>[2m=` and a pattern written against what the
  terminal shows matches nothing — which reads as "the gesture did
  nothing".
- **The probe rotates its log every 10 MB and writes ~17 MB/s**, so a
  mark taken as a line offset points into a file that has since been
  truncated. Select by timestamp (`webview-fps.py` does).
- **Xvfb is not a GPU.** Absolute numbers here are not the numbers on a
  developer's screen. What this rig is for is an A/B under a fixed
  environment; say which of the two any number is before quoting it.
- **Check `uptime` first.** This box runs other people's work; an A/B
  under a moving load is not one.
