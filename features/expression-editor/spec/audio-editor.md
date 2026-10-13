# Audio expression editor — the Vovious interaction model

The `Mode::PitchedAudio` surface is a Melodyne competitor. This records the
interaction model it targets, sourced from the **Vovious 1.0.6 user
manual** (39 pp., `www.vovious.com/download/VoviousManual.pdf`) — a
shipping, well-reviewed pitch editor whose gesture vocabulary is more
compact than Melodyne's and maps cleanly onto what we already have.

> **Clean-room.** The manual is a *behaviour* reference, read the same
> way [`midi-editor.md`](midi-editor.md) reads the Riffer manual. No
> Vovious code, artwork, colour values or serialized format enters this
> tree. Where the manual is silent on how something works internally we
> design it ourselves rather than guessing at theirs.

## Why this model and not Melodyne's

Melodyne spreads its editing across a tool palette (main / pitch /
formant / amplitude / time / note-separation), each with its own cursor
states, so a single note edit costs a tool switch. Vovious puts **six
handles directly on the note** and switches modes only for the things
that are genuinely different gestures — timing, drawing, harmony.

That suits us: our note already carries `center + drift·amount +
modulation·amount` ([`design.md`](design.md)), which is exactly the
decomposition the handles address. The handles are a *view* of fields
that already exist.

## What the surface is made of

Reading the manual's interface page top-to-bottom, and naming each part
against what we already have:

| Vovious part | ours | state |
|---|---|---|
| Transport / playback | `daw` transport, or host via ARA | to build |
| Modes (6) | `mode.rs` — one more variant set | partial |
| Views (Note / Pitch) | canvas render flag | to build |
| Undo / Redo menu (timeline) | `doc::History` exists, no menu | partial |
| Time bar + cycle area | ruler exists; cycle does not | partial |
| Auto Pitch Correction | `tune_dsp::correct_notes` | wire up |
| Note bar (piano gutter) | landed, needs scale shading | partial |
| Overview (scroll/zoom strip) | to build | — |
| Zoom controls (3 sliders) | `zoom.rs` has the maths | wire up |
| Sidebar (settings) | to build | — |

The five things named as priorities — **pitch drawing, temporary notes,
sibilant editing, timing, MIDI reference** — plus **multitrack**, are
specified below. The rest is chrome and follows.

## The note handles (Pitch / Amplitude mode)

Six drag handles laid out on and around the note body. All are
**vertical drag**; none is a click target that opens anything.

| handle | position | edits |
|---|---|---|
| pitch tuning | note body | `center_midi`, whole note |
| fine pitch tuning | top centre | `center_midi`, cents resolution |
| left slope | top left | tilt of the entry transition |
| right slope | top right | tilt of the exit transition |
| formant shift | bottom left | `formant_shift` |
| amplitude | bottom centre | `gain_db`; **right-click mutes** |
| vibrato | bottom right | `modulation_amount` |

Two behaviours worth naming because they are not obvious:

- **Double-click the note body snaps the selection to correct pitch.**
  Configurable (sidebar) to snap to the nearest *MIDI reference* note
  instead of the nearest chromatic/scale degree.
- **Snap-to-pitch-centre while dragging the body**, with **Shift
  reversing** it. Every snap toggle in this manual works that way, so
  ours should too: one modifier, always meaning "the other behaviour".

`left slope` / `right slope` are the one part with no existing field.
They are a *tilt* applied to the drift term over the leading and
trailing portion of the note — the same shape our `shape.rs` curve
tilts already produce for MIDI. Reuse it; do not add a second model.

## Priority 1 — Pitch drawing mode

Free-draw the pitch track. The manual's design has three properties
that matter, and one of them we do not currently do at all:

1. **Anchor points, not samples.** A click adds an anchor; existing
   anchors drag. The rendered line interpolates *sinusoidally* between
   them — "the natural way a human sings" — not linearly and not with a
   spline that overshoots.
2. **The original pitch stays visible** as a thin line behind the
   drawing, for the whole session.
3. **Draft state with explicit apply.** Undo/redo works *within* the
   drawing; `Return` applies the whole drawing as **one** history step
   and discards the anchors, `Escape` dismisses it. This is the part we
   do not have — our pen commits per stroke.

Anchors may sit in unvoiced regions. That is deliberate: forbidding it
would break dragging *through* a sibilant, and an anchor there still
shapes the voiced line either side of it.

Implementation: a `Draft` layer over `ExpressionDoc` holding anchors +
the captured original curve, with its own `History`. The drawer's
restore-then-reapply preview (`modulation.rs`) is the same shape and is
the thing to generalize.

## Priority 2 — Temporary note mode

A range selection *inside* a note that gets the full handle set. Drag
horizontally to define the range; the same seven handles then act on
only that span.

The rules:

- Dragging again **outside the current temporary note's bounds** starts
  a new one and discards the previous.
- It is a view, not a document object — nothing is persisted but the
  edits it makes.

This is a scoped-edit primitive, so build it as one: a `TimeScope`
(`t0..t1`, or the whole note) threaded through the handle edits. The
handle code should not know which it is operating on.

## Priority 3 — Sibilant editing

Sibilants are the unvoiced spans the tracker already identifies (no f0).
Amplitude edits then have two scopes:

- **note scope** — the whole note's `gain_db`
- **sibilant scope** — only the unvoiced spans *within* the note's range

Selected by a sidebar toggle, with **Shift reversing** it per-gesture,
same as every other snap in this manual. The amplitude handle draws as
a **hollow circle** in sibilant scope, and sibilant spans shade dark in
the waveform whenever sibilant editing is armed.

We need an explicit unvoiced-span set on the audio doc. `TrackedNote`
has the frames; what is missing is promoting "frames with no f0 between
note bounds" into a first-class span list the UI can hit-test and shade.

## Priority 4 — Timing mode

Drag the **vertical separator lines** between note segments. Line
colour encodes deviation from the beat (using the sidebar's sub-beat
setting).

The dual behaviour is the good idea here — one line, two meanings by
where on it you grab, split at a small white horizontal marker:

| grab | effect |
|---|---|
| **above** the marker | stretch the note(s) *left* of the line; notes right of it only **move** |
| **below** the marker | stretch the notes on **both** sides |

Plus: **double-click a line snaps it to the beat**, and stretch is
clamped to **⅛× … 4×** — beyond that the gesture simply refuses rather
than degrading.

Backing store exists: `PitchDoc::markers` (`WarpMarker { sample, d_time,
pitch_bend }`) and `render_world_warped`. What is missing is the
separator-line UI and the two drag laws expressed against markers.

## Priority 5 — MIDI reference tuning

Load a MIDI file (sidebar or drag-and-drop); it is stored in our project
file, not referenced by path.

| control | does |
|---|---|
| visibility | show / hide reference notes |
| track select | pick a track from the file |
| transpose | shift the whole reference |
| apply beat + tempo | adopt the MIDI file's tempo map |
| hold `M` | bring reference notes to the front |

The reference feeds two consumers: **Snap To Midi Reference** in auto
pitch correction, and the double-click snap target. Both are already
"snap to a set of target pitches over time" — so model the reference as
a target-pitch function, and the scale, the chromatic grid and the MIDI
reference all become the same interface with three implementations.

## Priority 6 — Multitrack

Vovious calls it **TrackSwitcher** (`T`), and it is keyboard-first by
design.

- Mouse-over a track in the switcher **previews** it; commit to edit.
- Per-track keyboard shortcuts, assignable, shown top-right of each
  track.
- Track ordering in the switcher **is** the shortcut ordering.
- Any subset of other tracks can be shown as **reference tracks** —
  drawn but not editable, coloured by default / DAW colour / shadow.
  **Double-clicking a reference track's note makes it editable**, which
  is the fastest possible track switch and costs nothing to support.
- Hold `R` brings references to the front (`M` does it for MIDI).
- One **undo history per track**, so a track switch can never eat
  another track's edits.

The structural consequence: `ExpressionDoc` becomes one of many in a
`Workspace { tracks: Vec<TrackDoc>, active: usize }`, each `TrackDoc`
owning its own `History`. Everything currently taking `&mut
ExpressionDoc` keeps working on `workspace.active_mut()`.

That refactor should land **before** the audio UI, not after — retrofitting
per-track history onto a single-doc editor is the expensive order.

## The rest, briefly

- **Views** — *Note view* (colour = deviation from correct pitch) and
  *Pitch view* (focus on the pitch track; long-press the button to
  choose whether colour is relative to note centre or pitch centre).
- **Note assignment** (`9`, or hold `N` for temporary) — click a
  boundary line to **merge**, click mid-note to **split**. Purely
  visual; does not change the sound. A live translucent preview shows
  the result before commit.
- **Harmony** (`5`) — added voices with mute/solo/delete, over a
  **harmony timeline** whose ranges are painted by dragging: one drag
  direction enables, the other disables, plus expand-to-arrangement and
  clear-all.
- **Auto pitch correction** (`X`) — `Notes` (how many of the worst to
  touch), `Amount`, `Min note length`, `Snap to scale`, `Snap to MIDI
  reference`. Applies to the selection, or to everything when nothing is
  selected. `tune_dsp::correct_notes` already has the maths.
- **Scale detection** — rank major/minor scales by fit, and hovering a
  candidate previews its degrees in the note bar before committing.
- **Undo/redo menu** (`E`) — a *vertical timeline* of steps, each
  labelled with the **arrangement position it edited** (bar/beat,
  averaged when a step spans several). Click below the current-state
  line to undo N, above to redo N. Single click keeps it open for
  comparison, double click closes.
- **Overview** — a full-length strip at the bottom: drag the middle to
  scroll, drag its borders to zoom.
- **Preview / auto-cycle** (`S` / `A`) — audition the edited notes
  looped, without the backing track, over a configurable range: only the
  notes / notes ± N / surrounding beats / surrounding bars.

## Shortcuts

Worth adopting wholesale; they are dense and unclaimed by our existing
map.

`1`–`5` modes (pitch/amp, timing, pitch draw, temp note, harmony) ·
`9` note assign, `N` temporarily · `Space` play, `Backspace` stop ·
`C` cycle · `X` auto-correct · `S` preview, `A` auto-cycle, `P` drag
preview · `Z` vertical auto-zoom · `F` follow cursor · `E` undo menu ·
`M` MIDI to front, `R` reference to front · `T` trackswitcher ·
`Q` quick help, `O` overview, `0` sidebar · `←`/`→` scroll by page,
`↑`/`↓` scroll vertically · `Ctrl ±` horizontal zoom, `Alt ±` vertical.

**`R` is mode-dependent**, and deliberately so. Channel reassignment is
meaningless outside MPE — an audio or vocal note has no member channel —
so in every other mode bare `R` takes Vovious's meaning and brings
reference tracks forward. `Shift+R` does that from any mode, so the
gesture stays reachable in MPE too. This is the general rule for key
conflicts here: a binding belongs to whichever meaning the current mode
can actually use, rather than being globally reserved by the mode that
claimed it first.

## Status

**Landed:**

1. **`Workspace` + per-track `History`** — `tracks.rs`, with reference
   tracks and the switcher bar.
2. **`PitchDoc` ↔ `ExpressionDoc` adapter** — `expression-editor-audio`,
   including the take waveform, per-note envelopes and unvoiced spans.
3. **The seven note handles + `TimeScope`** — `handles.rs`, and the
   temporary note came nearly free from it.
4. **Sibilant scope** — armed with `I`, Shift-reversed per gesture,
   spans shaded and the amplitude handle hollow.
5. **Pitch drawing** — `draft.rs`: anchors, raised-cosine interpolation,
   the original underneath, its own undo, one history step on apply.
6. **Timing separators** — `timing.rs`: the two drag laws split by grab
   height, clamped to ⅛×–4×, double-click to the beat, coloured by beat
   deviation. Expressed as note moves and resizes so the same gesture
   works on MIDI; the audio domain derives warp markers from the result.
7. **MIDI reference** — `reference.rs`, sharing one `SnapSource`
   interface with the chromatic grid and the scale, since all three
   answer *what pitch should this note be?* Only the reference depends
   on time, which is the sole real difference between them.

**Sung notes draw as the waveform they are** — the body is the recorded
amplitude mirrored about the note's own pitch, carried up and down to
wherever the audio actually sits, with the white pitch track wandering
through it and breaking across unvoiced spans.

**Next — the chrome**, none of it structural: views (note/pitch),
note assignment (split/merge), harmony voices, the auto-correct panel
(the maths is `tune_dsp::correct_notes` plus `plan_corrections`), scale
detection, the undo timeline, the overview strip, and
preview/auto-cycle.
