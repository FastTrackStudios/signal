# Drum mode — multitrack audio editing on one kit

The expression editor opened on a whole recorded kit: every mic at once,
folded into the lanes a drum editor actually reads, with the transient
quantizer (`grid-quantize.md`) and a hand slip/stretch workflow on top.
It is the first *multitrack audio* surface of the editor, and it is
built so the same machinery later serves a guitar group (DI + two amps)
or a bass (DI + amp), where "one source, several mics, one edit" is the
same problem.

The target project is `02 LORD OF THE FIGHT` (48 kHz, 84 bpm 6/8):

```text
Drums
  Kick   / SUM / In, Out, Trig, Sub
  Snare  / SUM / Top, Bottom, Alt, Trig ; Verb
  Toms   / T1 - Unused, T2, T3, T4
  Hi-Hat
  Overheads
  Room
```

Three kick mics and four snare mics are three and four *tracks*, but
while editing they are one kick and one snare. That fold is the whole
point of the mode. This spec is tracey prefix `r`, ids `drums.*`.

It builds on: `percussive-and-multitrack.md` (stack, `Track::mode`,
`UnpitchedAudio`), `grid-quantize.md` (detector, planner, WARP/SPLIT,
groups), `standalone-runner.md` (the runner) and the
`daw::service::StretchMarkers` contract. Nothing here is a second copy
of those; where they already decide something this spec only cites it.

## Opening the project in daw-standalone

The editor is a `daw` client, so before any lane is drawn the
standalone backend has to hold the project the way REAPER would.

r[drums.open.rpp]
`daw-standalone` MUST open a REAPER `.RPP` by path such that tracks,
folder nesting, items, takes, take lanes, tempo map, markers/regions
and **media** are all present: `Projects::open` installs a
project-relative media resolver for the file's directory (relative
`FILE` paths resolve against it, absolute paths as themselves) and
materializes audio, so the same call that the `daw` CLI and the
editor's runner use yields a playable, readable project. A source that
cannot be found is a per-item warning, never a failed open.

r[drums.open.stretch-markers]
Loading an `.RPP` MUST carry each take's `SM` stretch markers into
`ProjectState` under the take (REAPER keys markers per take; a
multi-take item's markers belong to the take they were read from) with
the third token as `slope`. (`PLAYRATE`'s fourth field is the pitch-shift
algorithm, not a stretch-marker mode; `StretchMode` is not in the file
and stays `ProjectDefault` on load.) `get_stretch_markers` on a freshly
opened project returns what the file said.

r[drums.open.accessor-placement]
The standalone `AudioAccessors` take accessor MUST return audio in
*take playback time* exactly as REAPER's does: honouring `start_offset`,
`play_rate`, the item's bounds, and any existing stretch markers — so
one take analysed under either backend yields the same frames. An edit
computed against standalone audio and written to REAPER, or the reverse,
lands on the same sample.

r[drums.open.peaks]
`Peaks::take_peaks` on `Standalone` MUST return real min/max peak pairs
for a take (from the mmap'd source, at the requested block size, in
take playback time), and a peak request for a take whose media is
missing returns an empty `TakePeakData` rather than an error.

r[drums.open.playback-warped]
The standalone renderer MUST honour a take's stretch markers during
playback: between two markers the source is read at the rate the pair
implies (`StretchMarker::rate_to`), so an edit written as markers is
*heard* in standalone, not only stored. Phase coherence between the
members of a group is preserved by rendering identical marker maps
identically (same rate, same boundary frames).

r[drums.open.runner]
`expression-editor-standalone --example editor -- <song>.rpp --drums`
MUST open the project as a drum workspace: every track under the kit
folder is loaded as a `Track` in `Mode::UnpitchedAudio`, folded into
lanes per `drums.lanes.*`, and shown in the stack. `--drums <folder>`
names the kit folder; with no argument the first folder whose name
classifies as a kit (`Drums`, `Drum`, `Kit`) is used.

## Lanes

r[drums.lanes.roles]
A drum workspace folds the kit's tracks into four **lane roles**, drawn
bottom-up: `Kick` at the bottom, `Snare` above it, `Toms` above that,
`Other` on top. A track is assigned a role by its place in the hierarchy
first and its name second: a track under a folder whose name classifies
as kick/snare/toms takes that folder's role; otherwise the track's own
name is classified (`DrumFamily`: kick, snare, tom → those roles;
hi-hat, cymbal, ride, overhead, room and anything unclassified → `Other`).
The classifier MUST be the one the drum map already uses
(`expression_editor_core::rows::drum_family`), extended for the folder
words, not a second list.

r[drums.lanes.summed]
A `Kick` or `Snare` lane draws the **sum** of its member tracks as one
waveform: per peak bin, the mean of the members' mono peaks (members are
phase-aligned mics of one source, so the mean is the mix a SUM bus
would render), normalised so the lane's loudest bin fills the lane.
Member tracks are not drawn separately. A lane with one member draws
that member.

r[drums.lanes.toms-split]
The `Toms` lane has the same height as the `Kick` and `Snare` lanes and
is divided evenly into one sub-lane per tom track, each drawing its own
waveform with its track name. A tom track whose name contains `Unused`
(case-insensitive) or is muted still gets a sub-lane but is drawn at
half opacity and excluded from detection.

r[drums.lanes.other]
The `Other` lane holds the remaining members (hi-hat, overheads, rooms,
reverb returns) as one summed waveform at `Kick`'s height. It is not
editable on its own and not a detection source; it exists so the user
sees the whole kit move together. Opening it into per-member sub-lanes
is a later feature and the layout MUST leave room for it (a role lane is
a `Lane` with a `role` and a `members` list, not a flattened track).

r[drums.lanes.heights]
The four role lanes share the stack height: `Kick`, `Snare`, `Toms` and
`Other` get equal weight, with the `percussive-and-multitrack.md` floor,
and the active lane's boost. Horizontal time is shared with the ruler
and every other lane, so a hit at x in `Kick` is the same instant at x
in `Snare`.

r[drums.lanes.hits]
Each `Kick`/`Snare`/tom sub-lane draws its detected transients as
vertical hit lines over its waveform (`NoteShape::Triangle` at the
onset, length to the next hit), coloured by deviation from the grid the
way the timing separators are (`audio-editor.md` Timing mode). A
selected hit is the unit of manual editing.

## Detection and the kit group

r[drums.group.kit]
The kit is **one edit group**: every cut, slip and marker written by the
editor is applied identically to every member track of every role lane
(including `Other`), at the same project time. Editing one lane and not
the rest is not offered — that is how a kit smears. The trigger tracks
are chosen per `grid-quantize.md` (one detection drives many tracks) and
the shared-start rule there applies: members whose items do not share a
start are reported, and SPLIT is refused for the group until they do
(WARP remains available).

r[drums.group.detection-source]
Transients are detected on the `Kick` and `Snare` lanes' **summed**
signal (their member mean) — not per mic — and the two hit lists are
merged (union, nearest-duplicate within the retrigger window collapses
to the louder). Tom sub-lanes detect on their own signal and join the
merged list only when the user arms them. The detector is the envelope
gate of `grid-quantize.md`, with its `DetectConfig` exposed per lane.

r[drums.group.tempo]
Grid targets are the project tempo map (here 84 bpm 6/8), taken from the
`daw` backend, so a tempo change mid-song moves the grid with it. The
division is the editor's sub-beat setting.

## The quantize panel

The maths exists (`expression_editor_tools::quantize`,
`expression_editor_audio::{detect,quantize,apply_quantize}`,
`quantize_panel.rs` state). What is specified here is the surface.

r[drums.quantize.panel]
A **Quantize** tool on the toolbar (visible in `UnpitchedAudio` mode)
opens a drawer panel with, top to bottom: *Detect* (per-lane threshold
and sensitivity sliders with a live hit histogram; crest, filters,
retrigger and offset under an *advanced* disclosure), *Target* (grid
division, tolerance, grid-scan toggle, strength 0–100 %), *Write* (mode
SPLIT / WARP — SPLIT default for a kit — with pad and crossfade for
SPLIT), and *Apply*. Every control change re-runs detection and
re-plans immediately; nothing is written until Apply.

r[drums.quantize.preview]
While the panel is open the lanes MUST show the plan: each hit draws
its current position and a ghost at its planned position, joined by a
short arrow, coloured by how far it moves; hits the plan leaves alone
(outside tolerance, lost the window, below strength) are dimmed. The
summed waveform is redrawn at the planned positions on hover of the
Apply button, so the user sees the result before committing.

r[drums.quantize.apply]
Apply MUST write the plan through the group rule in one undo step: SPLIT
cuts every member at each planned transient (pad before, crossfade at
the join) and moves the pieces; WARP writes one stretch-marker map
(`set_stretch_markers`) to every member take. The daw-side write is the
same `apply_split` / `Plan::alignment` path the engine already has —
the panel adds no second write path. After Apply the lanes re-detect
and the hit histogram reflects the new state.

r[drums.quantize.grid-options]
The grid control MUST offer straight, triplet and dotted divisions from
1/4 to 1/64 and a swing amount (0–100 %, applied to the off-beat
divisions), reading the project's grid setting as its initial value
when the backend exposes one. Targets are computed from the chosen grid
over the tempo map, so swing and triplets are target placement only —
the planner is unchanged.

r[drums.quantize.filter-presets]
The detector's filter block (high-pass, low-pass, a transient-attack
emphasis and a gain compensation) MUST be savable as named presets
(`Kick`, `Snare`, `Toms`, `Full kit` ship as defaults, user presets are
stored with the editor's settings), chosen per lane from the *Detect*
section, so dialling a kick in is one pick rather than four sliders.

r[drums.quantize.slider-defaults]
Every slider in the panel supports *right-click → store as my default*
and *Alt-click → reset to default*, and the panel remembers its last
settings across sessions — the same affordance Perfect Timing ships,
because a drum editor re-dials the same kit for every song.

r[drums.quantize.undo]
Apply is undoable as one step (`Undo` returns every member to its
pre-apply items/markers), and re-opening the panel after an undo shows
the plan again, unchanged.

### What was taken from Perfect Timing, and what was not

The script (80icio, ReaPack `Items Editing/80icio_Perfect Timing! -
Audio Quantizer.lua`, v0.41, unlicensed — read for the method, nothing
copied; see `grid-quantize.md`) settled these choices: the three-page
settings layout (*Main / Filters / Advanced*) becomes our *Detect /
Target / Write* drawer; the histogram and the trigger lines drawn in
the editing window itself are kept; the "Edit Tracks" indented member
list becomes lane roles with a member tree; sliders store defaults on
right-click. Its v0.41 simplified grid scan from *closest and loudest*
to *closest*; ours keeps loudest-wins in the window (`grid-quantize.md`
explains why the ghost note must not win), and that difference is
deliberate. Its constraints — one item per track, items sharing start
and length — are our group rule, stated in `drums.group.kit`.

## Manual editing

The fast-edit gesture from `reaper-input`'s `quick-edit` workflow,
expressed DAW-agnostically so it works against standalone and REAPER
through `daw::service`.

r[drums.manual.slip]
In SPLIT mode, **drag a hit** left/right: the group is cut at the hit
(pad before it, `grid-quantize.md`) and everything from that cut to the
next hit slides with the mouse, on every member. The slide is a
take-start-offset change of the right piece
(`Takes::set_start_offset`, the math of `slip_drag.rs`:
`Δoffset = −Δx / px_per_sec × play_rate`), snapping to the grid when
snap is on; releasing leaves crossfades at both joins. One drag, one
undo step.

r[drums.manual.stretch]
In WARP mode, **drag a hit** moves a stretch marker at that transient:
the hit moves, its neighbours stay, and the audio between is stretched
(rate between marker pairs), clamped to ⅛×–4× as the timing separators
are. The same marker map is written to every member take. Holding the
stretch modifier drags *both* sides (the `StretchLaw::BothStretch` law).

r[drums.manual.nudge]
Selected hits nudge by the grid division with the arrow keys, and
`Shift`+arrows by one sample-accurate millisecond; **double-click a hit
snaps it to the nearest division** — the same gestures as the timing
separators, reused not re-bound.

r[drums.manual.add-remove]
The user MUST be able to add a hit the detector missed (click in a lane
with the quantize tool + modifier, placed at the nearest local energy
maximum within the retrigger window) and remove a false hit (select +
Delete). Both edit the hit list only; nothing is written until a drag or
Apply.

r[drums.manual.daw-split]
The gesture's split primitive is the one the quantizer already has:
`apply_quantize` splits by duplicate + set position/length/start-offset
+ fades, over `Items`/`Takes` calls both backends implement — a facade
`split_item` RPC was considered and declined there, and this spec keeps
that decision. The requirement is sharing: the manual slip MUST use the
same split helper `apply_split` uses (hoisted, not copied), so a
quantize cut and a hand cut cannot come apart in pad or fade semantics.

## Scope and portability

r[drums.scope.not-midi]
Drum mode edits audio only. A MIDI drum track in the same folder loads
as a `Mode::Drums` lane (the drum map) in the stack, is shown aligned,
and is not a member of the kit group.

r[drums.scope.generic-groups]
Lane roles and the group rule MUST NOT be drum-specific in type: a role
is a label + classifier, and a group is "trigger lanes + members". A
guitar workspace later defines roles `DI` / `Amps` over the same `Lane`
and the same `apply` path, with a DI-triggered group — no new edit
machinery.

## Saving

Standalone edits must survive as a REAPER project. The write path is
`dawfile-standalone`'s patched export — the original text is re-parsed
and only what changed is rewritten, so every construct the model never
learned (FX chunks, ruler lanes, extensions state) goes out verbatim.

r[drums.save.new-file]
Saving a standalone project MUST write a **new** file beside the
original — `<stem>.fts-edit.rpp`, numbered up (`.fts-edit-2.rpp`, …)
when taken — and never modify the original in place. This is the rule
until the round trip is trusted at 100 %: the source session stays
exactly as REAPER left it, and the experiment is always inspectable
next to it. `Projects::save` returns after writing and the project
remembers the path it wrote.

r[drums.save.patched]
The written file is the original **patched**, not regenerated: item
blocks are reconciled by `IGUID` (edited items rewritten in place,
split pieces added, deleted ones dropped), take offsets, fades and `SM`
lines carry the edits, and every byte the edit did not touch is the
original's. Saving an unedited project writes a byte-identical copy.

r[drums.save.reopens]
A saved file MUST reopen: loading it back through `Projects::open`
yields the edited item layout (same piece positions, lengths, offsets
and markers on every mic), with media resolving exactly as the original
did — relative paths stay relative.

## Verification

The runner and `dioxus-test` are the harness (`standalone-runner.md`,
`reaper-testing.md`).

r[drums.verify.open]
A test opens `02 LORD OF THE FIGHT.RPP` (skipped when the path is not
present) through `Projects::open` on `Standalone` and asserts: the
`Drums` folder resolves to four role lanes, `Kick` has members
`In, Out, Trig, Sub`, `Snare` has `Top, Bottom, Alt, Trig, Verb`, `Toms`
has four sub-lanes in track order, and every audio member has non-empty
peaks.

r[drums.verify.quantize-roundtrip]
A synthetic three-track kit (kick, snare, overhead with known off-grid
hits) quantized at 100 % through the engine's apply path lands every
hit on its division on all three tracks, in SPLIT and in WARP. The
*write* is sample-exact (piece placements / the marker map, checked
directly); re-detection on the rendered audio confirms within 0.2 ms —
the gate's own trigger jitter, an order of magnitude under a flam — and
a second run plans no move above that.

r[drums.verify.slip-gesture]
A `dioxus-test` drive of one slip drag on the `Kick` lane asserts the
same cut times and the same offset delta on every member track, and one
undo step restores all of them.
