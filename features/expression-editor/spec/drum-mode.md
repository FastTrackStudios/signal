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

r[drums.lanes.trigger-overlay]
A **trigger** track — one whose name carries a `Trig` or `Trigger` token
— is not a lane or a sub-lane of its own. It is the same drum sensed a
second way, so it is drawn *over* the drum it triggers, in that lane's
space, outlined rather than filled: a trigger is near-silent between
hits, and a filled one would read as a hole punched in the mics'
waveform instead of a second view of it. A trigger is excluded from
`drums.lanes.summed`, since averaging its silence in would only pull the
mean down.

In a split lane a trigger pairs with a tom by number, `T3 Trig` over
`T3`. It keeps a sub-lane of its own only when nothing claims it — no
tom number (a bare `Trig`), or a number with no matching tom (`T4 Trig`
where `T4` was never recorded) — because a track that is really there
must not silently vanish. Given a sub-lane each, four toms with triggers
would read as an eight-piece kit at half the row height.

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

r[drums.lanes.hit-density]
Hit markers thin as they crowd. Marker stroke width is a function of the
mean spacing between visible markers, clamped to a hairline floor, and
the onset flag is dropped once the flags would overlap into a band. A
fixed weight cannot serve both ends: it is right for a handful of hits
and useless for a song's worth, where sixteenth kicks land a couple of
pixels apart and the markers merge into a solid bar hiding the waveform
they annotate. Markers stay thick when there is room for them, which is
the case a thick marker is good at.

r[drums.lanes.hits-per-sub-row]
In a split lane a hit marker is confined to the sub-row of the drum it
was detected on, so the picture answers *which* tom was hit rather than
"a tom". Detection is already per tom (`drums.group.detection-source`);
drawing every hit across the full lane height threw that answer away at
the last step. A trigger's hits land in the row of the tom it triggers,
the same row its waveform overlays. A member with no sub-row of its own
still draws full height.

r[drums.chrome.markers]
The ruler shows the host's timeline chrome — the song's structure — and
must read **both** kinds, because sessions use both. A region is a named
*span* and draws as a coloured band; a marker is a named *point* and
draws as a coloured tick with its label, over the bands rather than
under them. Both carry a thin line down through every lane at low
opacity, so a boundary is visible where the user is looking and not only
in the ruler.

Markers stay points. Not every marker is a section boundary — `tempo
change`, `back to 4/4` — so stretching each one to the next would draw a
structure nobody wrote. Labels are clipped to the room before the next
marker *in the same lane*, so a dense passage reads as ticks with the
names that fit rather than overlapping words.

The ruler is a stack of **shelves**, one per (REAPER ruler lane, kind)
pair actually in use, ordered by lane with regions above markers within
a lane, and labelled by the lane's name; a lane with no name is labelled
by its index. Keyed by kind as well as lane because REAPER allows both
on one lane and these projects do exactly that — `The ballad` files 15
regions *and* 2 markers on `SONG` — where a marker tick lands inside a
region band and the two fight for the same pixels. A span and a point
are different things and get different rows.

The ruler grows a shelf at a time rather than dividing a fixed band: one
shelf is sized to match the band the ruler used before shelves existed,
so a single-lane project renders exactly as it always did, and extra
shelves make the ruler taller instead of shrinking each other into
illegibility. Everything below the ruler — the lanes, the playhead, and
the pointer maths that maps a click to a lane — takes its offset from
this computed height, never from a constant.

The grouping shows the project as it is, not as it should be: these
sessions declare `SONG`, `SECTIONS` and `MARKS` and then file every
marker under `SONG`. Redistributing them would hide exactly the thing
the ruler exists to show.

r[drums.detect.hybrid]
A second detector is available that finds hits with **spectral flux
against a moving median** and then places them with the **envelope**.
Each stage covers the other's weakness: the envelope gate is
sample-accurate but its thresholds are absolute, so no one setting suits
a whole album; spectral flux scores every frame against its own
neighbourhood and so needs no setting, but an STFT answers only to the
nearest hop. Flux decides *that* a hit happened, the envelope decides
*when*, hunting the steepest rise in a window deliberately narrower than
the gap between two hits so refinement can sharpen a hit but never move
it onto its neighbour.

It is **not the default**, because measurement does not support making
it one. Against the album's drum MIDI the two are a tie — F1 0.342 for
the best hybrid settings against 0.345 for the best gate settings, at
precision around 0.4 where the reference (a different take) cannot
discriminate further. Its advantage is that it reaches that without a
dial: the gate needed a calibration pass to get there. It is kept for
the jobs where robustness matters more than milliseconds, and so that
the choice can be re-measured whenever either detector changes.

r[drums.view.page-bars]
`]` and `[` move the view forward and back one **page of four bars**,
keeping the zoom; `\` frames the page without moving off it. Four
because that is the phrase drummers play in and the unit a take is
edited in — fixing a bar of a fill wants the three around it for
context, not a screen of the whole song.

A page lands on a **bar line**, and the bar lines come from the host's
tempo map (`ExpressionDoc::bars`), never from a bar length multiplied
out. Paging by a fixed number of seconds would drift out of phase within
a few pages and put the downbeat somewhere different each time, making
the thing being navigated by the thing that moves. `set in stone` has
198 bars in three different lengths — 1.33s, 1.68s and 2.95s — so this
is the common case, not an edge one.

The view snaps to the nearest bar line before counting, so a view nudged
off the grid re-aligns instead of carrying its error into every
subsequent page; and paging stops at the last full page rather than
scrolling past the take, where an empty screen reads as the editor
having lost the project.

r[drums.host.daw-agnostic]
Everything the workspace asks of a daw is named as one bound,
`expression_editor_audio::daw_bound::DrumDaw`, so "will this work in
REAPER" is a question the compiler answers rather than one someone
argues about. Both `Standalone` and `daw::reaper::Reaper` are asserted
against it at compile time; a service added to the bound that either
cannot serve is a build error, not a discovery.

The edit vocabulary is deliberately the one REAPER's item model already
offers, and the same one its own audio quantizers use: **split** with a
leading pad and a crossfade, or **warp** by writing stretch markers on
transients, either way applied identically to every mic of a group so
phase coherence survives. A split needs no facade call of its own — it
is a duplicate with its position, length and start offset set. Detection
reads samples through the audio-accessor service, which is REAPER's own
API for exactly that, rather than going to the files on disk.

`DrumHost` is generic over that bound, and `DrumHost<Reaper>` is
asserted to build at compile time — a stronger claim than the bound
alone, since a backend can satisfy `DrumDaw` while the host still fails
to instantiate over it if some method reaches past the bound. Saving a
copy is the one thing the generic host does not offer: it writes a new
`.rpp`, which is a window-with-no-host-application's answer to saving,
and REAPER saves through REAPER.

The workspace loader is split accordingly: opening an `.rpp` from disk
was the only standalone-specific part, and `drum_workspace` — find the
kit folder, score the candidates, read each mic, fold the role lanes —
now takes any backend.

It asks for `Send + Sync` as well, and that bound is a real limit rather
than a formality. The mics are read in parallel because twenty tracks of
a five-minute take is the slow part of opening a kit; REAPER's API is
main-thread only, the same constraint that makes every service touching
it dispatch through `main_thread::query`. A REAPER panel therefore wants
this logic with a sequential read. Writing that is a smaller job than
pretending a main-thread API is thread-safe, which would compile and
then fail inside REAPER at a distance from the cause.

r[drums.mouse.contexts]
The stacked view resolves its gestures through the same `MouseMap` the
piano roll does, in three contexts of its own — `Lane` (a role lane's
waveform), `Hit` (a detected marker) and `LaneGutter` (the name and mic
chip). Its own contexts rather than the roll's, because there are no
notes here: a "note" is a transient in audio the user does not own, the
canvas is a waveform rather than a pitch grid, and every gesture is
about *when* a hit is, never what pitch it is.

The bindings live in the base map every preset is built from, not in the
Drums preset alone: the profile is the user's choice and bindings that
existed only in one would leave the surface inert for anyone on another.

Dragging a hit is one action, `MoveHit`, whether it slips or warps — the
quantize panel's write mode decides which. Binding them separately would
let a drag and the Apply button mean different edits. Adding a hit and
cutting the kit are deliberate modifier gestures and must not be
reachable by a plain click: one invents a hit and the other rewrites
every item in the kit, and a plain binding puts both one slip of the
hand away at all times. A click on the lane behind a hit selects the
lane rather than falling through to nothing, so a click two pixels off a
marker does not feel broken.

r[drums.manual.split]
The **razor** cuts the kit. Arming it and clicking in a role lane puts
an item boundary at the click on *every* mic at once — one cut time and
one undo step, because mics cut at different places are no longer
phase-coherent and cannot be repaired by hand afterwards.

The cut lands a leading pad *before* the click, like every other cut
here: anyone splitting a drum take aims at a hit, and a cut on the
attack clips it. A split moves nothing; it only creates the boundary, so
the piece either side can then be dragged, deleted or replaced. It is
the one edit with no other gesture available, since every other one
starts by grabbing a hit and a cut is precisely for where there is not
one.

A click at or beyond either end is refused, and so is one that would
leave a piece shorter than 20 ms: the first is not a split, and the
second is a sliver the user then has to find and delete.

r[drums.manual.undo]
Undo rewinds the **daw's** last write, not the document's, whenever a
host is attached. Every gesture in drum mode — slip, stretch, quantize
Apply — writes through the host inside its own undo block and none of
them touch the document's history, so undoing there rewinds a document
nobody edited while the edit stays on disk. That is the worst shape the
bug can take: nothing appears to happen, and the user believes the take
is back the way it was. Surfaces with no host (the piano roll, demo
scenes) keep the document stack, where their edits really are.

## Fills

r[drums.fills.detect]
A **fill** is where the drummer stops keeping time and plays something,
and it is the part of a take that must not be quantized like the rest.
Groove wants the grid; a fill is often played across it deliberately — a
triplet run, a drag into the downbeat — and flattening it onto
sixteenths takes the performance out. The editor therefore finds the
fills before quantizing anything, so they can be left alone or given
settings of their own.

Every bar is scored on two signals. **Tom activity** leads: a rock
groove is kick, snare and hats, and the toms sit unused until the fill.
**Density** corroborates, because not every fill reaches for the toms —
a snare roll or a run of kick sixteenths is a fill too. Only bars busier
than usual score; a bar with *fewer* hits than the median is a break,
not a fill.

Both are measured against the **median bar of that same take**, never a
fixed count. "More than six toms in a bar" works on one song: it marks a
tom-driven groove as one continuous fill and misses the single fill in a
sparse ballad. The median and the median absolute deviation are used
rather than the mean and standard deviation, because fills are precisely
the outliers being looked for and an average is dragged toward whatever
it is meant to detect — on a song with four fills in sixty bars the mean
tom count is inflated by the very bars that should stand out.

r[drums.fills.sensitivity]
Fill detection runs the transient detector at its own sensitivity, not
the quantize panel's. The two jobs want opposite things: the panel
*moves* every hit it reports, so a false one damages the take and it is
tuned for precision, while fill detection only counts how busy a bar
was, where a missed hit is the costly error and a spurious one is noise
the median absorbs.

The gap is large enough to matter. On `unbreakable` — 160bpm, the
drummer playing around ten hits a second — the panel's default finds
1.6 a second, about a fifth of what was played, confirmed against the
project's own drum MIDI. Scoring bars against a fifth of the evidence is
what made fill counts swing between three and twenty-four across the
album.

r[drums.fills.protect]
A quantize **leaves detected fills alone** by default. The hits inside a
fill are still detected and still drawn — the user can see what was not
moved rather than wondering where it went — they are simply excluded
from the plan the Apply builds. The default is on because the damage is
asymmetric: a fill quantized like groove has its phrasing flattened and
undoing that means finding the fills by hand afterwards, while a fill
wrongly left alone is merely un-quantized, which is visible and one
gesture to fix.

r[drums.fills.draw]
Fills draw as a wash behind the lanes, not an outline: a fill is a
*region* of the take, and the hits inside it still have to read as hits.
The bands are on screen from load, because what a quantize will leave
alone is worth knowing before it runs rather than after. They are
recomputed when an edit lands, after the host has dropped its cached
fills — asking earlier returns the fills of the audio as it used to be.

r[drums.fills.calibration]
Detector defaults are **computed, not chosen**:
`cargo run -p expression-editor-standalone --example calibrate` sweeps
them against the album and prints the score for each. Every number in
the detect chain was previously somebody's guess, which is how a
sensitivity that found a fifth of what the drummer played survived
unnoticed — nothing measured it, so nothing contradicted it.

Two references, because the two questions have different ground truth.
Detection is scored against the projects' **drum MIDI**: not an exact
transcription of the audio, so its absolute F1 is pessimistic and must
not be read as an accuracy figure, but the same bias applies to every
setting on the sweep, and a *ranking* survives a biased reference where
an absolute score does not. The fill threshold, which has no ground
truth at all, is scored on how much more often its fills land at a
section boundary than a bar picked at random does — the "than random"
half being the whole metric, since a third of each song is already near
some marker and a detector firing blindly scores 30%.

Matching is **one-to-one** at a ±25 ms window, with reference onsets
closer than 30 ms combined into one event — the standard onset-detection
evaluation convention (Böck & Widmer, DAFx-13), so the numbers can be
read against published results rather than only against each other. The
one-to-one part is not a detail: counting every detection that merely
sits near *some* reference onset lets one onset absolve a whole burst of
false positives, which is what made a detector firing thirty times a
second tie for the best F1 on the sweep. Under the correct rule F1 peaks
at sensitivity 0.90 and falls either side, and the rate constraint
becomes a cross-check rather than a crutch.

Neither metric may be taken at its argmax, and the reasons differ.
**F1 cannot police over-detection here**: the reference is a different
take, so precision is capped near 0.55 however good the detector is, and
spurious hits cost almost nothing in F1 while recall keeps climbing —
sensitivity 1.0 scores the best F1 on the sweep while firing thirty
times a second against a drummer playing six. It is therefore
also constrained to settings whose hit rate stays near the drummer's,
which under one-to-one matching agrees with the F1 optimum.
**Section lift has no recall term**, so it rewards reporting fewer and
safer fills until barely any remain; it is read next to the per-song
fill count, and the tie is broken on cost, which is asymmetric — a
missed fill gets quantized and flattened, a bar wrongly called a fill is
merely left alone.

The two optima are not the same setting. Fill detection's sensitivity
sits *below* the detector's F1 optimum, because it wants contrast
between bars rather than maximum recall: pushing to the F1 optimum adds
marginal hits evenly across groove and fill alike, lifting the median as
much as the outliers and flattening the difference being measured.

r[drums.fills.bars]
Bar boundaries come from the host's tempo map, one query per measure —
never a bar length multiplied out. A real take does not have one bar
length: `set in stone` is 6/8, changes tempo partway, and has a 7/4
section, which is three bar durations in one song. A grid derived from a
single bpm drifts out of phase within a few bars and puts every fill in
the wrong place. Where the map cannot place bars the detector returns
nothing rather than guessing a grid.

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
Transients are detected per **detection unit**, and the units' hit lists
are merged (union, nearest-duplicate within the retrigger window
collapses to the louder). A unit is one blended signal, not one mic:
`Kick` and `Snare` are a unit each, and `Toms` is **one unit per tom**,
so a hit can be attributed to the tom that made it rather than to "some
tom". `Other` never detects. `Unused` members are excluded everywhere.
The detector is the envelope gate of `grid-quantize.md`, with its
`DetectConfig` exposed per lane.

Within a unit, a **trigger is weighted over the acoustic mics it shares
a drum with** — 4:1. A trigger is a contact mic: almost no bleed, almost
no decay, a near-vertical attack, and so better evidence of *when* the
drum was hit. It is not infallible — it can drop out, double-fire on a
rim shot, or sit slightly out of alignment — so the mics keep a vote
rather than sitting detection out; the trigger merely outweighs them.
At 4:1 one trigger outweighs any realistic number of mics on one drum
while still being pulled by them where they agree, and where the trigger
misses a hit entirely the mics can still put one there. Weights within a
unit sum to 1, so every unit's signal arrives at a comparable level and a
threshold means the same thing across the kit.

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
