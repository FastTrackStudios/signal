# Full MIDI editor — sources and plan

The expression editor has to become a complete MIDI editor, serving six
products off one surface. This records where the behaviour specs come
from, so nothing is invented where a reference exists.

## The six products, and what actually differs

| product | row means | note label | note extras |
|---|---|---|---|
| Melodyne-style audio | MIDI pitch | note name | drift / vibrato blend |
| MPE MIDI | MIDI pitch | note name | channel, 3 expression lanes |
| plain MIDI | MIDI pitch | note name | velocity |
| drum MIDI | named drum lane | drum name | diamonds, no length |
| guitar / bass | **string** | **fret** | articulation, legato, bend |
| vocal lyrics | MIDI pitch | **syllable** | — |

Almost the only structural difference is *what a row means*, so that is
abstracted as [`RowSpace`](../expression-editor-core/src/rows.rs)
(`Pitch` / `Drums(DrumMap)` / `Strings(StringTuning)`). Everything else
— gestures, zones, curves, camera, undo — already works in row space and
needed no change.

## Source 1 — REAPER's MIDI mouse modifiers (in-tree)

`features/reaper/reaper-input/src/input/mouse_modifiers/behaviors/midi/`
already carries REAPER's decoded MIDI-editor contexts and behaviour
names: piano roll, note, note edge, CC lane, CC event, CC segment,
ruler, marker lanes. That is the authoritative list of what a MIDI
editor's mouse must do — 37 piano-roll drag behaviours, 33 note drag
behaviours, 18 note click behaviours.

`mouse.rs` mirrors that shape (context × gesture × modifiers → action)
so a REAPER mouse map can be loaded without translation, and so the
per-product defaults can differ: a drum editor wants paint-on-drag where
a Melodyne editor wants marquee.

Razor-edit behaviours are decoded in-tree too
(`behaviors/razor_edit.rs`) — 17 drag, 9 click, plus edge behaviours.
Modelled in `razor.rs`.

### Source 1b — MIDI Razor Edits (sockmonkey72)

REAPER has no razor edits *in the MIDI editor*; the reference for what
they should mean there is Jeremy Bernstein's `sockmonkey72_MIDIRazorEdits`
([ReaPack index](https://raw.githubusercontent.com/jeremybernstein/ReaScripts/main/index.xml),
`MIDI Editor/RazorEdits/`). Its `MRE_CMD.pdf` is the command reference,
and this is that table — read as a **specification**, not vendored. The
script is a modal ReaScript that you enter and leave; here the mode is
simply having the razor tool armed.

| key | MRE | here |
|---|---|---|
| `R` | retrograde contents | ✅ |
| `Ctrl+R` | retrograde values only (keep the rhythm) | ✅ |
| `S` | select contents | ✅ |
| `U` | unselect contents | ✅ |
| `V` | invert contents | ✅ (about the area's own pitch centre) |
| `X` | delete contents | ✅ |
| `D` | duplicate contents (move area) | ✅ |
| `F` | make area → full-lane area | ✅ (full pitch range) |
| `I` | insert mode — do not delete the target area | ✅ |
| `H` / `L` | horizontal / vertical lock, mutually exclusive | ✅ |
| `Delete` | delete areas + contents | ✅ (keeps the areas — see below) |
| `Escape` | exit the script | ✅ (drop areas, back to Select) |
| `C` | toggle note-area ↔ CC-area span | ✗ — our lanes are a view of the same notes, so an area already spans both |
| `W` | "widget" mode | ✗ — CC-lane-specific UI with no counterpart here |

Two deliberate departures. `Delete` keeps the areas rather than removing
them, because the commonest follow-up is putting something else in the
same region and clearing them makes that two gestures. And MRE's
"preserving leading/trailing notes" drag variants are not modifier
combinations here: `I` is a sticky mode instead, because a four-modifier
chord is not a thing anyone reaches for mid-edit.

The property that makes a razor different from a marquee: an area
**slices** notes at its edges rather than selecting whole ones.
Dragging a razor over the middle of a held note takes the middle of that
note with it. So every razor operation carves first — slice both edges,
then act — which makes "the notes in the area" a well-defined set with
no partial overlaps left to reason about.

Two cases worth naming, both tested:

- A destination is cleared before contents land on it, so comping
  replaces rather than piling up — **except** when the destination
  overlaps the source, where clearing would delete the material out from
  under a nudge.
- Clearing a lane across an area splices the default in at both edges
  rather than only deleting points inside. A held note whose curve is
  defined by endpoints *outside* the razor has no points to delete, but
  the region still has to read as cleared.

## Source 2 — Ample Sound Riffer (guitar/bass)

[Ample Guitar manual](https://www.amplesound.net/en/Ample_Guitar_Manual.pdf) §6, and the
[Guitar Tab manual](https://www.amplesound.net/en/Guitar_Tab.pdf).

Riffer's panel is a **string roll**, not a pitch roll: Note Properties
line, Expression line, String Roll, FX Noise line, per-string tuner,
Strum line.

Its key commands (§6.2.1), which the `Riffer (Ample)` mouse preset
matches so nobody coming from it is retrained:

| gesture | action |
|---|---|
| left click | insert note |
| left click a note | select it |
| left click elsewhere | deselect |
| double click a note | delete |
| right click / Alt+click | context menu |
| drag vertically | change pitch |
| drag border horizontally | change length |
| Ctrl + drag vertically | change velocity |
| Ctrl + drag border | change duration |
| Shift + drag | move |

Ten per-note properties (§6.3.1): Pitch, Velocity, Duration,
Articulation, Legato, Vibrato Range, Vibrato Rate, Bend Type, Bend Rate,
Note-Off Velocity — plus a bend editor with draggable points.

Articulations (§6.4.2): Natural Harmonic, Palm Mute, Slap, Pop, Tap,
Staccato, Slide In/Out, Hammer On, Pull Off, Legato Slide, Bender,
Vibrato, Slide Guitar.

Rules worth keeping (§6.4.3): legato is only available between adjacent
notes **on the same string** and is marked on the *first* note; long
legato slide speed comes from the destination note's velocity; natural
harmonics only speak at frets 5, 7, 9, 12.

Two behaviours fall out of the model rather than being special-cased:
moving a note to another string **re-fingers it at the same sounding
pitch** rather than transposing it, and importing MIDI picks the
position nearest the hand rather than the lowest fret.

## Source 3 — juliansader's Multi Tool

`js_Mouse editing - Multi Tool.lua` v6.61, 5581 lines
([forum thread](http://forum.cockos.com/showthread.php?t=176878)),
cloned from ReaTeam/ReaScripts.

The idea: on a modifier press, **colored zones light up over the
selection**, each with a left-drag function and a mousewheel function.
Where the drag *starts* picks the tool. Its zones:

| zone | left drag | mousewheel |
|---|---|---|
| compress from top | compress lane from top | flip values absolute |
| compress from bottom | compress lane from bottom | flip values absolute |
| scale from top | scale values from top | flip values relative |
| scale from bottom | scale values from bottom | flip values relative |
| warp | warp left/right or up/down (whichever the drag commits to) | reset and evenly space |
| stretch left | stretch from left | reverse positions |
| stretch right | stretch from right | reverse positions |
| tilt left | tilt left side | snap to chased values on left |
| tilt right | tilt right side | snap to chased values on right |
| move | move up/down and left/right | flip values absolute |
| undo / redo | — | — |

Tweaks *while the gesture runs* — this is most of why it feels powerful:

- **middle-click** switches curve shape (sine ↔ power for compress/tilt;
  slow-start ↔ slow-end for warp)
- **mousewheel** tweaks curve steepness, with a deliberate pause at the
  centre so neutral is easy to return to
- **right-click** toggles one-sided vs symmetrical
- **Shift** ignores snapping while stretching

Two design points to carry over:

- Float positions and values are **remembered between steps**, so
  repeated edits do not accumulate rounding error from snapping to
  ticks or to 128-step value ranges. Our `Curve` is already `f64`, and
  the drawer's restore-then-reapply preview follows the same principle.
- Mouse position at start selects scope: inside a lane edits that lane's
  values *and* positions; over a lane divider or the ruler edits all
  selected events' positions only.

## Source 4 — FeedTheCat's MIDI Editor Magic

`FTC_MeMagic.lua` v1.4.0, **MIT** (Ilias-Timon Poulakis),
[forum thread](https://forum.cockos.com/showthread.php?t=238851),
cloned from
[iliaspoulakis/Reaper-Tools](https://github.com/iliaspoulakis/Reaper-Tools).
MIT means this one we can port properly rather than only read.

Two ideas, both now in `zoom.rs`:

**1. Zoom to a note count, not a bar count.** Four bars is the wrong
amount of screen for a whole note and for a 32nd-note run, so a fixed
zoom is always wrong somewhere in the part. The span comes from a
locally-weighted average note length using **Shepard's inverse distance
weighting** — each note contributes its length plus the following gap,
weighted `1 / distance^smoothing`, so the view adapts to the density of
the passage under the pointer rather than the item average. Gaps are
folded in (a sparse passage should zoom out) but capped, or one long
rest blows the view open. An empty stretch widens until the nearest note
is visible rather than showing nothing.

**2. One key, contextual behaviour.** Horizontal and vertical modes are
chosen independently by *where the pointer is*: note area → density-aware
+ fit notes in view; piano keys → whole item + all notes; ruler → time
only, pitch untouched; CC lane → time only. One shortcut covers what
would otherwise be a dozen.

Also carried over: `cursor_alignment` (where the anchor lands, 0..1),
row-count floor and row-height ceiling, and — importantly — clamping to
the item **slides** the view rather than narrowing it, so the zoom level
does not change as you approach an item edge.

## Status

Landed (122 core tests):

- `RowSpace` — pitch / drums / strings, with GM drum map, guitar and
  bass tunings, capo, nearest-hand fingering
- `Articulation` — the full Riffer set, with legato and
  valid-fret rules
- note properties — velocity, off-velocity, mute, lyric text,
  articulation, legato, fret
- `MouseMap` — context × gesture × modifiers → action, with fallback to
  the unmodified binding, plus four presets (REAPER-like, Drums, Riffer,
  Lyrics)
- edits — velocity set/nudge, off-velocity, mute/toggle, channel nudge,
  scale length, stretch positions (arpeggiate), copy notes, partial
  quantize, legato, set text, set articulation, set string (re-fingers),
  set fret
- **`MouseMap` is wired**: `pointer_down` resolves a context and a
  gesture through the map and executes the resulting `Action`. Contexts
  are detected including razor areas and edges (which outrank notes, so
  a drawn rectangle owns its region). The tool-driven path survives
  only for the expression tools the map explicitly defers to
  (`ActiveTool`, `PenOverride`), which is what keeps pen/curve/eraser
  behaviour intact.
- **Chord box**, backed by `keyflow_proto::chord` rather than a second
  detector. The adapter takes the *last* reading from
  `detect_chords_from_midi_notes`, not the first: it emits progressive
  interpretations as it accumulates pitches, so the first entry for a
  Cmaj7 is just "C". Reads the selection, or the notes under the
  playhead when nothing is selected.
- **Velocity / CC lane strip** with per-note stems and continuous-lane
  curves, sharing the roll's horizontal camera exactly.
- **Bar layout reworked**: the top bar carries only modes (tool, lane,
  shape, history, view) and now fits one row at plugin width; settings
  (grid, key, temperament, bend, strip) moved to the status bar; the
  selection row carries the chord plus the drift/vibrato blend
  controls, which act on the selection and belong with it.
- `zoom` — the MeMagic port: density-aware smart zoom, seven horizontal
  and ten vertical modes, contextual mode pairs, bound to `F` /
  `Shift+F` in the UI

**Row-space rendering is done.** Drums draw right-pointing triangle
heads with the flat edge on the onset — a triangle beats a diamond
because the attack is a straight vertical line you can align to the grid
by eye, where a diamond's widest point is its middle and the onset has
to be inferred. String rolls print fret numbers on the body and colour
by *string*, not pitch class, because the string is what a player
tracks and pitch-class colour would scatter one string's part across six
hues. Drums colour by kit section for the same reason. Articulations
draw as badges above the note, legato as a tie arc leaving its right
edge, and a lyric replaces the note name wherever one is set.

**Multi Tool zones are done** (`multitool.rs` + `multitool_ui.rs`).
Armed on `A` over the selection; zones light up over its bounding box
and where the drag starts picks the transform. Mid-gesture: wheel bends
the curve (with a detent at neutral so linear is findable), `M` switches
sine/power, `S` toggles symmetric. Transforms always run from a captured
snapshot, never the live state — the Lua original calls this out as why
it keeps float positions between steps.

**Modes** (`mode.rs`) — MIDI / MPE / Vocals / Drums / Guitar, then
PitchedAudio / UnpitchedAudio. Each is a *preset*, not a lock: it sets
the row space, mouse map, visible lanes and strip, all of which stay
editable. The top bar is conditional on it, so MPE channel controls and
the per-note expression lanes only appear where the format can carry
them. PitchedAudio is the Melodyne surface — same components, notes
sourced from pitch tracking. The switcher groups them by
`Mode::family()`: the first five are `ModeFamily::Midi`, the last two
`ModeFamily::Audio`. Quantize and Align are *tools*, not modes.

**Controller lanes are done.** `EditCcEvents` was bound in the preset
and handled nowhere, and `ShapeCc` / `ScaleCc` / `EraseCc` were
implemented in core but unreachable. CC now routes through the mouse map
like everything else: `context_at` returns `CcLane` / `CcEvent` by
proximity to the drawn curve, plain drag freehands, alt draws a shaped
ramp that survives release, ctrl rides the fader from a captured
snapshot, and right-drag erases. Shift stays unbound as the snap-reverse
key.

**The note context menu is done** (`menu.rs` + `menu_ui.rs`), with the
clipboard it needs. Riffer §6.2.2 supplies the core set and each mode
adds what its note actually carries. A right-click on an unselected note
acts on *that* note, and items grey out rather than disappearing — a
menu whose shape moves with the selection is one you cannot learn.

Next: the audio surface, specified in [`audio-editor.md`](audio-editor.md).
