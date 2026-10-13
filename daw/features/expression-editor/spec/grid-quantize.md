# Quantizing audio to the grid

Beat Detective / Audio Bend / Perfect Timing: detect transients, decide
where each one *should* be, and move it there — either by splitting the
item and sliding the pieces, or by warping.

This is a different problem from the take-to-take alignment already in
`align.rs` and `align_hits.rs`, and the difference is only in where the
targets come from:

| | target |
|---|---|
| `align.rs` | another take's frames |
| `align_hits.rs` | another take's hits |
| here | the project grid |

Everything downstream — the time map, the warp markers, the write path —
is shared. What this adds is target selection, a much stricter detector,
and the split write path.

## Sources, and what we take from them

[Perfect Timing](https://forum.cockos.com/showthread.php?t=288964) by
80icio, and MK Slicer by Cool, which it shares a transient engine with.
Both are ReaScript on ReaPack (ReaTeam/ReaScripts). **Neither the
repository nor the script carries a licence**, so authors retain all
rights and none of it can be copied.

Same treatment as mpl's scripts and MPElodyne, and the same as
`references.md` already states: read the algorithm, describe it here,
write our own. A method is not copyrightable; its expression is. Nothing
is ported.

What reading it settled — and it changed the design, so it was worth
reading rather than working only from the forum post:

**The detector is an envelope gate, not spectral flux.** Two peak
followers on the rectified signal with different time constants, and a
hit fires when the fast one is both above an absolute threshold and far
enough ahead of the slow one. That is much better suited to this job
than the flux detector already in `onsets.rs`, for a reason that is
obvious in hindsight: an STFT reports the *frame* a change happened in,
so its answer is quantised to a hop — 5.8 ms at our settings. Quantizing
with a 5.8 ms detector replaces one jitter with another of the same
size. A sample-by-sample gate has no such floor, and it is far cheaper
besides.

**The crest test happens at trigger time.** Our first pass measured
peak-to-RMS over the attack window afterwards, which is a reasonable
idea and strictly worse: the fast/slow envelope ratio *is* the crest,
it is available at the instant the hit arrives, and using it as a
trigger condition means a rejected swell costs nothing to reject.

Flux keeps its own job — segmenting a take for the percussive editor,
where a hop is fine and the spectral centroid it produces is what sorts
a hit into a band.

## Detection

- **Filters** ahead of detection: a steep high-pass and a gentler
  low-pass. The point is not tone. A kick's transient is buried under a
  ringing snare in the full-band signal, and band-limiting to where the
  hit lives makes it the loudest thing again. Detection sees the
  filtered signal; everything written applies to the original.
- **Threshold** — absolute, in dB, on the fast envelope. Rejects bleed.
- **Crest** — how far the fast envelope must lead the slow one. This is
  the control that separates a struck hit from a swell at the same
  level, and no amount of thresholding does that.
- **Sensitivity**, on a **peak** or **RMS** scale, optionally
  normalised, applied *after* detection. Which loudness measure ranks
  hits: peak favours sharp attacks, RMS favours weight, and a kit wants
  one where a bass wants the other.
- **Time offset** — a fixed shift on every hit. Filters have latency and
  any detector fires once an attack is underway.
- **Retrigger** — shortest gap between hits. Clamped to at least the
  measurement window, below which it would silently do nothing.

Two measurement details are not obvious and both matter. The **peak is
taken after a short holdoff**, because the first instants of an attack
are the stick and not the drum. **RMS is DC-corrected**, because a
close-miked kick has real offset and an RMS that counts it reports a
quiet hit as a loud one — and the correction has to be done on the
*signed* signal, since rectifying folds the negative half up and no
mean-subtraction afterwards recovers the offset.

### The threshold is not optional

The crest test only means anything above the threshold. Out of digital
silence the slow envelope is zero, so it gets floored at the threshold,
and the ratio then measures how far above the *threshold* the signal has
climbed rather than how sharply it arrived — so a slow swell fading up
from nothing does trip a high crest setting if the threshold is far
enough below it. Set the threshold near the material's noise floor,
which is what it is for.

## Where the planner lives

None of the matching below is audio-specific, and it is not written
twice. The planner lives in `expression-editor-tools`, generic over two
traits (`Timed`, `Sustained`) that a MIDI note, an audio transient and a
pitch-detected note all satisfy — so a drum take and a programmed kit are
put on the grid by the same code and cannot drift into disagreeing about
which division a hit belongs to.

`expression_editor_audio::quantize` keeps the part that is genuinely
audio: a config in seconds (a musical grid under a tempo map is evenly
spaced in *ticks*, not seconds, which is why the seam itself carries no
unit), and the two ways a plan becomes sound — WARP and SPLIT below.

The one rule with a domain in it is the sensitivity filter. Audio gets it
upstream, from the detector's gate; MIDI has no detector, so the seam
carries a `min_strength` that a caller with velocities can use instead.

## Grid scan

The expensive part of quantizing is deciding which transient a grid
division refers to, and the trick that makes it cheap is inverting the
question: **each division gets at most one transient**, taken from a
window of `tolerance` either side.

That single rule does most of the work:

- A buzz roll no longer produces eight hits fighting for one division.
- A ghost note between divisions is ignored rather than dragged onto a
  beat it was never near.
- A division with nothing in its window stays empty — silence is not
  quantized onto.

With grid scan **off**, every detected transient is a target and each
snaps to its own nearest division. That is the right behaviour for
material that is not on a grid at all, and the wrong behaviour for a kit,
which is why it is a switch rather than a default.

When two transients fall in one window the louder wins, on the same
reasoning as the spacing contest in `onsets.rs`: the ghost note must not
suppress the hit.

## Strength

`0%` leaves the take alone, `100%` puts every hit exactly on its
division, and in between each hit moves proportionally. Applied to the
*offset*, not the position, so a hit already on the grid never moves
regardless of strength.

## The two modes

### WARP

Each quantized transient becomes a warp marker; the map between them is
linear. This is what the editor already does for timing edits, so it
needs nothing new, and it is correct for one track.

It is **wrong across several mics on one source**, because REAPER's
stretching is not phase-coherent between items: two mics warped
independently drift apart by a sample or two and the source smears. The
mode is still offered for multitrack — a user who has decided the smear
is acceptable is entitled to — but the default for a group is SPLIT.

### SPLIT

Cut the item at each transient, move each piece so its transient lands
on the grid, crossfade the joins.

Phase-coherent by construction: every piece moves as a rigid block, so
the relationship between two mics inside one piece is untouched. This is
why drum editing is done this way and not with warping.

Two parameters, both about the cut rather than the move:

- **Leading pad** — cut a few milliseconds *before* the transient while
  the piece still lands by its transient. A cut exactly on an attack
  clips the front of it; a cut 5–10 ms early leaves the attack whole and
  puts the join in the decay of the previous hit, where a crossfade is
  inaudible.
- **Crossfade** — a pre-splice fade at every join. Moving pieces leaves
  a gap or an overlap at each cut, and a hard edge there is a click.

The pad shifts the cut and not the snap point. Confusing the two is the
classic way to get this wrong: the audio arrives 5 ms early and every
hit is flam'd against the rest of the kit.

## Groups

Multi-mic editing is one detection driving many tracks.

Transients are detected on **one** track — the trigger track, normally
the closest mic — and the resulting edit is applied identically to every
track in the group. Detecting per track and editing per track is the
thing that must *not* happen: two mics would get slightly different cut
points and the source would smear at every join.

The constraint that follows is real and worth stating: the tracks in a
group must share a start time, or "the same cut" means different audio
on each. Perfect Timing requires items to share start and length, and so
do we.
