# Expression editor architecture

The editor has one editing model and multiple application shells. Drum editing is the next production workflow; MIDI, MPE, guitar, and pitched audio remain clients of the same core.

This refactor follows [PR #12](https://github.com/FastTrackStudios/session/pull/12). That PR made drum loading and editing useful in the standalone runner, but left the REAPER panel without the loader and callbacks. It also exposed a dependency inversion: shared drum services lived in the standalone application and imported settings from the renderer.

## Module ownership

| Layer | Owns | Must not own |
| --- | --- | --- |
| `expression-editor-core` | Documents, editor state, gestures, instrument semantics, local history | DAW access, DSP, Dioxus, RPC runtime |
| `expression-editor-tools` | Portable tool algorithms, quantize settings and preview geometry | Window state, DAW implementations |
| `expression-editor-audio` | Analysis and item-local audio edit algorithms | Application lifecycle or UI callbacks |
| `expression-editor-daw` | MIDI take conversion and write-back | Window creation |
| `expression-editor-host` | Kit discovery, capture, analysis orchestration, detection, edit scope, host undo, reconciliation | Renderer, desktop shell, global UI state |
| `expression-editor-ui` | Components, layout, painting, pointer/keyboard translation | File loading or backend-specific save policy |
| `expression-editor-ui::host` | Optional Dioxus adapter for shared host services | Its own edit algorithms |
| `expression-editor-standalone` | Files, command line, examples, standalone save and application shells | A second copy of shared drum logic |
| `expression-editor-reaper` | Dock/action registration, REAPER session lifecycle, live take sync | A separate drum editor |

The host crate builds without a renderer. UI's `host` feature is opt-in. Standalone save support is an explicit host feature; REAPER does not depend on the standalone application crate. Existing standalone and UI import paths re-export the moved APIs so downstream consumers can migrate separately.

### Core editor

`core/src/lib.rs` is the public facade. `editor/mod.rs` owns state and initialization, with behavior in private modules:

- `workspace`: track identity, switching, folded lanes, lane cameras, document reconciliation.
- `history`: document edits and gesture undo/redo.
- `gestures`: handles, temporary notes, pitch drafts.
- `instruments`: drum hands/flams, guitar frets, lyrics, modes, controller editing, chords.
- `commands`: command targeting and execution.
- `razor_ops`: razor selection and edits.
- `navigation`: cursor, grid, camera, phrase paging and zoom.

These are inherent implementations of the same `Editor`; callers retain `Editor::…`. Private child modules can access editor invariants without making helpers public to the workspace. This preserves behavior while making future state extraction reviewable by responsibility.

### Stacked UI

`ui::stack` exposes its existing API through three private modules:

- `geometry`: lane models, row layout and conversion through seconds.
- `waveform`: waveform reduction and polygon construction.
- `view`: the Dioxus component and transient interaction state.

`HitGesture` lives in core. `QuantizePanel` and its settings live in tools; their UI paths are compatibility exports. A service can name commands and settings without linking Dioxus.

## Host workflow and invariants

1. Discover the kit and its playing microphone tracks.
2. Capture audio through the facade on its owner thread.
3. Analyze owned audio buffers on a bounded number of workers. A backend, accessor, or UI signal never enters a worker. Results preserve input order; a worker panic propagates instead of silently dropping a mic.
4. Attach tempo/ruler metadata on the owner thread and build the folded workspace.
5. Preview without writing audio. Commit a group edit inside one host undo block.
6. Re-read the current audio and reconcile every affected track, then update previews and fills.

The capture/analysis helper deliberately gives only the analysis callback `Send`/`Sync` constraints. This is enforced by the Rust type boundary and a regression test whose capture closure owns an `Rc<Cell<_>>` and checks the calling thread.

The host itself has private `detection`, `write`, and `refresh` modules. `controller` translates complete gestures into these services and reconciles the editor; it has no UI state.

### Edit coordinates

The stacked editor emits **project seconds**. The low-level audio split writer expects **seconds relative to an item**. Conversion belongs at the host boundary.

An anchor item identifies a microphone track; it is not the entire performance. Before each split/slip/quantize write, the host resolves the current playing items on every member track. It intersects the shared project-time plan with each item's span and converts those intersections to item-local cuts. This supports late-starting and already-sliced takes without extending an anchor to the length of the song or editing only its first piece.

The workspace duration is the end of its shared timeline. Refresh uses that same extent. Muted items and nonplaying fixed lanes are excluded by one shared predicate. Locked items, non-unit take playback rates, and existing stretch maps are rejected before writes begin. Warp currently requires one full-length, unwarped item per mic; it cannot yet compose maps over comped takes. The UI displays these refusals.

Host APIs can still fail after an earlier mic was written. The undo block makes that operation recoverable; it is not a transaction or a guarantee of rollback. Do not describe it as atomic until failure injection and rollback are implemented.

### Host lifecycle

The REAPER module registers **FTS: Open Expression Editor on drum kit**. It uses the shared loader and callback adapter. A loaded kit is pinned to its project GUID and does not follow item selection into the single-take pitch editor. Drum operations write immediately through host undo; the single-take document debounce does not write drum documents back as MIDI or resynthesized audio.

Standalone adds its own Save callback. The shared adapter owns preview/refresh/undo/redo/gesture wiring and operation feedback. Applications should mount that adapter instead of copying callbacks.

## Session as a remote host

The intended live architecture is **REAPER authoritative, Session a client**. The renderer-free host layer is implemented; a remote expression-editor service and Session workspace are **not yet implemented**. A synchronous `DrumDaw` is not a network client, and generic compatibility does not prove remote support.

The remote layer should expose editor operations rather than individual sample reads or facade calls:

- Open a workspace by project GUID and kit identity.
- Receive a snapshot containing document/track identity, waveform summaries, detected hits, fills, and a revision.
- Send a typed gesture or quantize request with the expected revision and request ID.
- Receive a committed revision or typed refusal; reject stale revisions before writing.
- Subscribe to document changes, undo state, progress and host disconnection.

Keep camera, selection, hover, and in-progress pointer drags local to each view. Keep committed audio, manual hit overrides, detection settings, and undo authority in the host session. Send summaries and incremental changes, not entire multitrack PCM or cloned `Editor` values on every pointer frame. Reconnect through a fresh snapshot; never replay an unacknowledged destructive request without idempotency.

Use the repository's existing architect/vox transport when this service is implemented. Do not turn synchronous facade methods into blocking per-sample RPC calls. The capture boundary is the place to introduce cancellable background jobs and progress reporting.

## Quality gates and next steps

Refactoring rules:

- Keep dependencies pointing toward portable models and algorithms.
- Prefer concrete types and private modules; introduce traits for a real alternative implementation.
- Use owned capture data across threads, not `unsafe impl Send` or a `Send + Sync` bound on REAPER.
- Make time units explicit at adapter boundaries and keep stable track/project identities.
- Preserve existing imports through small re-exports during migrations.
- Test observable edits and refusal behavior. A compile-only backend assertion complements, but does not replace, a REAPER integration test.
- Add regression coverage before changing a calibrated detector's defaults.

Remaining work, in order:

1. Run the dock workflow inside REAPER: open, resize, kit selection, split/slip, undo, project switching and reload. Compilation does not prove native panel behavior.
2. Test timeline split behavior on the real multitrack corpus, including mixed comp boundaries and crossfades. Compose existing stretch maps and take playback rates before removing the current refusals.
3. Add write-failure injection and rollback so a partial host failure cannot leave a partially edited kit.
4. Introduce a host session with revisions, request idempotency and cancellation, then implement the remote service and Session workspace described above.
5. Make capture/analysis incremental and cancellable. Current capture and the join remain synchronous, and whole-kit PCM is retained during loading. Bound memory by sample budget and cache waveform levels for long sessions.
6. Replace remaining string lane labels in manual-hit commands with stable detection-unit identity. Persist per-unit overrides and define their own undo behavior.
7. Extract state from `Editor` only where ownership warrants it (document/workspace/view/gesture state). The modular inherent implementations are a migration boundary, not a claim that the state model is finished.

Useful verification commands:

```sh
cargo check -p expression-editor-host --no-default-features
cargo check -p expression-editor-reaper -p expression-editor-standalone --lib
cargo test -p expression-editor-core -p expression-editor-tools -p expression-editor-audio \
  -p expression-editor-host -p expression-editor-ui -p expression-editor-standalone --lib --tests
cargo clippy --no-deps -p expression-editor-host -p expression-editor-core -p expression-editor-tools \
  -p expression-editor-ui -p expression-editor-standalone -p expression-editor-reaper --lib
```

## Verification of this refactor

Verified locally on 2026-09-08:

- 57 test binaries: **1,041 passed, 0 failed, 28 ignored** across core, tools, audio, host, UI and standalone.
- Regression coverage includes owner-thread capture with a non-`Send` capture closure, worker failure propagation, repeated splits, late project-time cuts and refresh, locked-mic preflight, zero item gain, and preservation of an existing warp on refusal.
- A rendered Dioxus surface exercises the shared adapter's commit, Undo, Redo and visible refusal behavior against the standalone backend.
- REAPER and standalone library builds pass; their downstream test/example targets were also checked.
- The host builds with no default features. Its normal dependency tree contains no Dioxus, Blitz or expression-editor UI.
- Formatting and `git diff --check` pass. Focused Clippy (`--no-deps`) passes with existing findings in unchanged code; unrestricted Clippy encounters the existing `daw-theme` lint failures noted in PR #12.

No live REAPER panel or Session network integration was exercised. The ignored tests were not counted as passes.

## Practice with real recordings

See [Crescendum drum practice](expression-editor-standalone/PRACTICE.md) for
`just ee-practice` and the opt-in real-song regression test. The staging module
owns filesystem preparation independently of the editor and host: every project
and media reference is copied into a retained temporary workspace before loading.

## Workstation performance checks

`just ee-stress` mounts `WorkstationApp` through `dioxus-test` on a temporary
practice copy. The input workload is separate from loading and report generation;
DOM events exercise the production handlers, including held-Z zoom. Each gesture
must visibly change its pane before timing starts. Raw event/update/layout samples
and an 8.333 ms budget gate make regressions reviewable. These headless timings do
not include paint or display presentation and cannot establish 120 FPS on their own.

The workstation owns arrangement scrolling. Its `ArrangePreview` disables its own
scroll container, keeping TCP rows, ruler and items in the same coordinate system.
The mixer owns its scroll subscription and mounts only visible strips plus overscan;
track state lives in the shared store, independently of strip component lifetime.

For repeatable measurements and agent handoffs, read the [performance harness guide](../../scripts/ui-stress/GUIDE.md).
