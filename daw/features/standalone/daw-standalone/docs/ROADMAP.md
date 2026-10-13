# daw-standalone → the workstation: where this is going

*2026-08-19. This document is the forward plan for the standalone DAW
engine and the workstation window built on it. The requirements that
are already targets live in
`features/expression-editor/spec/drum-mode.md` (`r[drums.*]`, tracey-
tracked); this file is the part that comes after them.*

## What exists today

One `Standalone` backend opens a real REAPER project — tracks, folder
nesting, fixed item lanes, takes, media (mmap-streamed), stretch
markers, tempo, regions/markers — and serves it over the in-process
vox link to three faces at once:

- the **arrangement + TCP** (`daw_ui::components`, the native vector
  panels the REAPER theme's art is exported from),
- the **mixer** (native strips, live meters, fader write-back),
- the **expression editor** (drum mode: role lanes, quantize engine +
  panel, slip/stretch/nudge gestures, per-lane mic selection).

Playback renders the full project graph — items, fades, take
envelopes, channel modes, play rates, stretch markers — to the default
output, sample-accurate. Edits land through the group rule as one undo
step, lanes re-read the daw after every write, and Save produces a new
`.rpp` beside the original via the byte-honest patched export (proven
byte-identical on an unedited 3.9 MB session). Startup on that session:
window in under a second, waveforms streaming in behind it.

`just workstation` runs the whole thing.

## The plan, in order of intent

### 1. Input as config, not code (the reaper-input port)

Space and Home are hand-wired today. The real design is
`crates/input`'s profile/keymap layer — the same `keymap.styx` families
that drive REAPER — bound to daw-standalone actions so the workstation,
the plain editor and REAPER share one scheme. This includes the
quick-edit gestures (the armed-action + slip-drag workflow
`features/reaper/reaper-input` implements in REAPER) speaking to the
standalone backend through the same facade calls the editor's gestures
already use.

### 2. Editing in the arrangement

The arrange view is read-only apart from seek. Next: item selection,
move/trim with snap, split at cursor — through `daw::service` so REAPER
gets them for free — and selection shared with the expression editor
(click an item above, edit it below). The alignment contract already
gives the two panes one time axis; a shared zoom/scroll model is the
first concrete step.

### 3. The ruler pinned, the panes resizable

The region/marker/bar ruler scrolls away with the tracks (Blitz has no
`position: sticky`); pinning it needs either scroll-event sync or an
`ArrangePreview` split into ruler + lanes. Pane splits (arrange/editor
height, mixer width) are launch-time constants; they should be drags.
Window resize should re-flow the layout live.

### 4. Persistent peaks

The peaks cache is in-memory, so the first open of a session still
pays one scan per take. Persist it beside the media (`.reapeaks`-style,
or our own sidecar keyed by content hash) and the second open of any
session costs nothing. The cache key already carries everything needed.

### 5. FX in the graph

The loader records FX chains but the renderer plays dry, and the mixer
shows unresolved plugin paths as raw strings. Plan: CLAP hosting in the
render graph (the `clap-host` feature and plugin registry exist),
built-in FTS FX (`features/fx`) as first-class citizens, honest
"unavailable" badges for plugins we cannot load, and the mixer's FX
slots wired to insert/bypass/reorder through the facade.

### 6. Re-editing after a SPLIT

The drum host's group model assumes one item per member per span; after
an Apply the track holds pieces, and a second manual gesture should cut
the piece under the hit rather than the original bounds. This is the
model change that makes drum editing iterative instead of one-shot —
REAPER's quick-edit semantics ("the item under the mouse") are the
reference.

### 7. Recording

The duplex engine (`pipewire` feature) already captures input for the
live rigs. The workstation needs: record-arm honoring the TCP's arm
buttons, input monitoring, takes landing as new items on fixed lanes —
the comping model the loader already understands.

### 8. The same window, other instruments

Drum mode's machinery was built deliberately generic: `LaneRole` is a
label + two rules, groups are "trigger lanes + members", the fold takes
`(guid, role)` pairs (`r[drums.scope.generic-groups]`). Guitar mode
(DI + amps, DI-triggered) and bass are the next folds; the pitched
editor (Melodyne surface) already runs against the same backend.

### 9. Write-back graduation

Save-as stays the rule until the round trip has survived real sessions
for a while (`r[drums.save.new-file]`). Graduation criteria: byte-
identical no-op round trips across the whole project corpus (not one
session), REAPER opening every edited file cleanly, and a `fts daw
diff` readout of every patched token. Then in-place save with an
automatic backup becomes an option, not a default. In REAPER itself the
panel edits live through the API instead — write-back is the
standalone path only.

### 10. Remote faces

Everything reaches the window over the same vox link that
`architect::axum_ws` can serve to a browser. The web build of the
arrange/mixer (daw-ui is wasm-clean) plus the existing browser-setlist
worklet renderer point at the same end state as the rest of FTS: one
engine, every screen a remote.

## Non-goals, stated so they stay non-goals

- **A second UI family.** The WALTER panels were deleted (77c3374b5);
  the native components are the one UI. Theme-image rendering returns
  only inside a theme *editor*, revived from git history.
- **A REAPER replacement.** REAPER remains the tracking/production
  DAW; the workstation is FTS's own face on FTS project data, and the
  bridge between them is the `.rpp` format and the shared `daw` facade.
