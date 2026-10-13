# Unpitched audio, and every track at once

Two features that only make sense together.

**UnpitchedAudio** is the mode for audio with no pitch to speak of —
drums, percussion, noise, most FX. The pitched audio mode
(`Mode::PitchedAudio`, the Melodyne surface) is built on a pitch
contour, and on unpitched material that contour is noise: it produces
notes that flicker between octaves and a blob that jumps around the
grid. Better to admit there is no pitch and edit what is actually
there — hits in time.

The two are the whole of `ModeFamily::Audio`. There is no third audio
mode: Quantize and Align are tools that run over either of them, not
modes of their own.

**Multitrack** is showing every track at once, each rendered in *its own*
mode, on one shared timeline. That is what makes the editor worth
opening on a whole song rather than one item: a vocal in blob mode, the
reference MIDI vocal as a plain roll, the guitar as a string roll, the
kit and the bass as slices — stacked, sharing an x axis, so you can see
that the guitar is late against the kick.

## UnpitchedAudio

### The vertical axis is bands, not pitch

A slice has no pitch, but it does have a spectral centre, and putting
that on the y axis is the difference between a strip of undifferentiated
ticks and something you can read at a glance: kick low, snare middle,
hats high.

So `RowSpace::Bands` — N bands split by centroid, three by default
(`Low` / `Mid` / `High`). It is deliberately not a drum map: we are not
claiming to know a kick from a rack tom, only that one hit is darker
than another. A user who wants named lanes has `Mode::Drums`, which is
MIDI and does know.

Banding is a *view* of the analysis, not a claim about it: each slice
keeps its measured centroid, and the band is recomputed when the split
points move. Nothing is lost by re-banding.

### A slice is a Note

`Note` already carries start, length, row and velocity, and a slice
needs exactly those: when it hits, how long until the next one, which
band, how hard. Reusing it means selection, the razor, nudging, the
multitool and undo all work on slices the day the mode exists, because
none of them ask what the note *means*.

- `row` — band index.
- `velocity` — peak level of the hit, 1..127.
- `start` — the onset.
- `length` — to the next onset (or the decay floor, whichever is first),
  so a slice is a region rather than a point and can be dragged as one.

What a slice does *not* use: `fret`, `blob`, `articulation`. Pitch
drawing and the drift/vibrato handles are hidden in this mode rather
than disabled — an inert control is worse than an absent one.

### Detection

Spectral flux over the analysis frames we already compute, which is the
standard answer and needs no new analysis pass:

1. Per frame, sum the *positive* changes in band energy since the last
   frame. Positive only — a decay is not an onset, and counting it makes
   every hit fire twice, once on the way up and once on the way down.
2. Subtract a moving median of the flux. A fixed threshold cannot
   survive a track that gets louder; the median tracks it and costs one
   window.
3. Keep local maxima above the threshold, then enforce a minimum
   spacing so a single hit with a messy attack does not become four.

The minimum spacing is a real parameter, not a constant to hide: a
50 ms floor is right for a kit and wrong for a shaker.

### Editing a slice moves audio, not pitch

The write path is already built and this mode uses the cheap half of it:

- **Timing** — a moved slice is a warp marker, so it leaves as stretch
  markers. No resynthesis, nothing baked in.
- **Level** — a slice's velocity is a gain over its span, which is the
  take volume envelope, which is `lanes.rs`. A fifth lane joins gate,
  dynamics, breath and sibilance; the sum still drives the envelope.
- **Mute** — a slice at zero velocity is silence over its span, which is
  the same envelope at −inf. No item splitting.

Pitch is the only thing that needs an audio replacement, and percussive
mode does not edit pitch. So this mode never renders a take.

## Multitrack

### Stacked, sharing one x axis

Every track gets a horizontal lane; time is shared and vertical space is
divided. This is the layout the feature is *for* — "the guitar is late
against the kick" is a question about two rows and one shared time
cursor.

Overlay already exists and stays: `Track::reference` draws a track
*behind* the active one in the same grid, which is the right way to
compare two takes of the same part. The two answer different questions
and neither replaces the other:

| | overlay | stack |
|---|---|---|
| same part, two takes | yes | no |
| different parts, one song | no | yes |
| needs a shared row space | yes | no |

The stack is what allows per-track modes at all: an overlay of a string
roll on a drum map would have to reconcile two row spaces, and there is
no honest way to do that.

### Mode belongs to the track

`Track::mode`, not `Editor::mode`. The editor's mode becomes a view of
the active track's, so every existing caller keeps working, and a
workspace can hold a vocal in `PitchedAudio`, its reference in `Midi` and the
kit in `UnpitchedAudio` at the same time.

The mode is *inferred* at load — pitched audio to `PitchedAudio`, unpitched to
`UnpitchedAudio`, MIDI to `Midi` — and then it is the user's, because
inference on a whispered vocal or a melodic tom fill will be wrong
sometimes and a wrong guess must be one click to correct.

### Lane heights

A lane's natural height depends on its mode: a slice strip needs three
bands, a vocal needs two octaves, a string roll needs six rows. Dividing
the space evenly wastes it.

So each lane asks for a height in rows, and the stack distributes
proportionally, with a floor so a collapsed lane is still clickable.
The active lane may be given extra weight — you are editing it, and it
should be the one with room.

### Alignment on hits

`align.rs` aligns on energy, with pitch as a weak cue. For drums the
energy envelope *is* the signal, and the existing aligner already
handles it — but the features that make it robust for a vocal (voiced
spans pairing at zero cost) do nothing for a kit.

Aligning on the detected onsets instead is both better and cheaper: two
sorted lists of hit times, matched nearest-neighbour under the same
`max_shift_secs` promise, and the result is the same `WarpMarker` list a
timing edit produces. Same write path, no new machinery.

The cue for "which track is the reference" is the same one the editor
already has — a track marked reference — so multitrack alignment is
"align every non-reference track to the reference", one command.

## What the stack does and does not do

It is a **read-only overview**. Clicking a lane makes that track active,
and that is the only gesture it takes — which is deliberate, because it
is also the gesture that gets you back to the roll, where the editing
happens. Answering "which track needs work" and "is this in time with
that" is the job; doing the work is the roll's.

Two consequences worth stating rather than discovering:

- Lane notes draw as **bars and triangles, not blobs**. At a lane height
  of forty pixels a blob's pitch excursion is a couple of pixels and the
  waveform body is a smear, so the extra fidelity would cost the frame
  budget and buy nothing. Switch to the roll to see the contour.
- Everything is drawn from each track's **parked document**, except the
  active one which comes from the live editor. That asymmetry is already
  enforced by `Workspace::doc_of` refusing to hand out the active slot's
  stale copy, and it is why the stack shows an edit the moment it is
  made rather than one switch later.

The **toolbar wraps** now. Seven modes plus the tool, lane and view
segments do not fit the width a plugin window gets, and the two
alternatives are both worse: clipping amputates whatever is on the right
without saying so, and letting the segments shrink collides each icon
with its own label — which reads as a rendering bug rather than as a
full toolbar.
