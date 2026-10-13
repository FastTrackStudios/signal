# Drum editing: hit markers, zones and the Quantize tool

The goal is drum editing an agent can run end to end, over MCP, with
results a mixer signs off on. Two tools split the work:

- **HitDetector** finds the hits once and writes them down as **hit
  markers**: take markers expanded with an identity, an origin and
  properties. Detection is never re-run to edit; a wrong marker is
  moved, deleted or added by hand.
- **Quantize** never detects. It reads hit markers (audio) or notes
  (MIDI) inside a selection and moves them, honouring **zones** — the
  fills and exceptions it must leave alone.

Session is its own DAW: none of this has to round-trip through REAPER.

## Why this shape — what was measured

Scored on Unbreakable (three full takes) against the same takes'
contact triggers, ±10 ms:

- The Perfect Timing! envelope detector at Session's shipped settings
  (−60 dBFS / 5 dB) on a close mic is mostly bleed: kick F 0.75,
  snare 0.4, toms under 0.1.
- The same detector with its threshold fitted to the mic reaches
  F 0.92–1.0 on kick, snare and T1–T3, and the fitted threshold
  (−28…−34 dBFS) holds from take to take.
- Fully automatic thresholds plus a learned bleed model reach about
  0.97 on kick and rack toms but fail on the floor-tom mics (kick bleed),
  with or without gate-dsp's key filter in front.
- Unbreakable's T4 trigger is dead: about 5,000 "hits" per take.

So the detector is not the gap. Calibration is, and the triggers are
what calibrate it. And because no detector is right everywhere, its
output has to be data that can be inspected and corrected, not a
transient decision made inside a slip.

## Hit markers

r[hits.marker.take-marker]
A hit marker is a **take marker**, expanded. It is anchored to a take
by **source position**, so it survives splits, comps, slips and moves
of the item without re-detection.

r[hits.marker.identity]
Every take marker has a **stable id** that does not change when markers
are inserted, deleted or reordered. The enumeration index stays for
callers that walk a take's markers in order, and means nothing else.

r[hits.marker.persisted]
Take markers, with every field below, are saved with the project and
restored on open, as stretch markers are. A marker that only lives in
memory is a detection that has to be run again.

r[hits.marker.kind]
A take marker has a **kind**: `Hit` for drum hits, alongside the plain
named marker and the take-rating marker that exist today. Kinds other
than `Hit` ignore the hit properties.

r[hits.marker.origin]
A hit marker records its **origin**:

- `Auto`, with the detector run that placed it (an id, the settings
  used and when);
- `Manual`, placed by a person or an agent;
- `Approved`, an `Auto` marker someone has checked.

r[hits.marker.rerun]
A detector run replaces only the `Auto` markers inside its range and on
its tracks. `Manual` and `Approved` markers are never moved, deleted or
overwritten by a run.

r[hits.marker.properties]
A hit marker carries:

- the **piece** it belongs to (`Kick`, `Snare`, `Tom 1`…);
- **velocity**: the attack's peak on its close mic, in dBFS, plus the
  same relative to the piece's loudest;
- **confidence**, `0..=1`;
- **sources**: which inputs heard it — its trigger, its close mic, or
  both;
- flags: **ghost**, **flam** (with the id of its partner), **bleed
  suspect**;
- a free-text **note**, for whoever looks next.

r[hits.marker.stretch]
A hit marker can carry a **target time**. With none it is identity: it
documents where a hit is and changes nothing that is heard. With one,
and stretch enabled on the take, it acts as a stretch marker from its
source position to its target. Quantize in slip mode never sets one.

r[hits.marker.phase-group]
A hit is marked on **every mic of its piece** (Kick In, Out and Trig;
Snare Top, Bottom and Trig) at the same source time, and those markers
share one hit id. A piece's mics are one source and are cut as one.

## Zones

r[zones.kinds]
A zone is a range of project time with a **kind**:

- `Fill`: the drummer leaves the groove; Quantize leaves it as played
  unless asked;
- `Exception`: anything else Quantize must not touch;
- `Settings`: Quantize uses this zone's own settings inside it (a
  looser strength through a half-time bridge, say).

r[zones.origin]
A zone records its origin, `Auto` (from the fill finder) or `Manual`,
and `Auto` zones can be approved or rejected, as markers can. A
rejected zone stays recorded as rejected, so a re-run of the fill
finder does not bring it back.

## HitDetector

r[hits.detect.parameters]
The detector's parameters can each be given per call and are otherwise
sticky for the song: time selection, tracks or pieces, threshold,
sensitivity, crest, retrigger, key filter (gate-dsp's Kick, Snare Top,
Snare Bottom and Toms modes, or none), time offset, which source to
trust (`Trigger`, `Mic`, `Auto`), and whether to calibrate from the
trigger.

r[hits.detect.trigger-health]
Before a trigger is trusted, it is checked against its close mic: a
trigger that fires far more often than its mic — a dead or noisy one —
is reported and not used.

r[hits.detect.calibrate]
With a working trigger, each close mic's threshold and crest are fitted
so that the mic's hits agree with the trigger's, and the fitted values
are kept for the song.

r[hits.detect.fusion]
With both a trigger and a calibrated mic:

- the trigger says **that** a hit happened;
- the mic says **when** — the attack start, sample-accurate, on the
  audio that is cut — and how hard;
- a hit both agree on is high confidence;
- a mic-only hit above the mic's calibrated level is a **ghost**
  candidate;
- a trigger-only hit is flagged: a choke, or crosstalk from a
  neighbouring drum.

r[hits.detect.no-trigger]
A piece with no working trigger is detected from its close mic, with
bleed rejected against every other close mic and the drum MIDI
(`DRUMS_REFERENCE`) as a cross-check, at lower confidence.

r[hits.detect.attack-start]
A hit's position is where its attack starts on the close mic, found by
refining the detector's trigger point back to the rise of a fine
envelope, not the sample at which a gate fired.

## Quantize

r[quantize.selection]
Quantize acts on a selection: a time range, a section by name, items,
tracks, or MIDI notes. Verse and chorus can each be quantized with
their own settings.

r[quantize.options]
Options: grid (with triplets and swing), strength, tolerance window,
target (`Grid`, `Reference`: the drum MIDI, `Guided`: the MIDI says
which line, the grid says when), whether zones are honoured (on by
default), mode (`Slip`: cut, move, crossfade; `Stretch`: set targets on
the hit markers), lead before the cut, crossfade time, and gap fill.

r[quantize.snap-on-marker]
What lands on the grid is the **hit marker** — the attack — never the
item's edge. A slice starts a lead before its marker (a few ms, so a
kick's soft onset and a stick's first contact are never cut off), and
that lead is set as the item's snap offset, so a hand drag snaps on the
hit too. An item therefore never has to start on the beat for its hit
to be on it.

r[quantize.groove]
Target `Pattern` places each hit by its section's own groove: each
piece's hits laid over the bar (or a two-bar cycle when the figure takes
two), bunched where they recur, and the bunches played in most bars kept
as slots — each with its line, the drummer's feel there and how much it
wanders. A hit nearer where its slot is usually played than the nearest
line goes to the slot's line, however far it drifted; otherwise as
`Guided`. A slot is on a 16th unless the grid is a triplet grid or the
drum MIDI writes it otherwise; the fill zones are left out of the
learning. The quantize names every hit the groove sent to a line other
than the nearest, and `DRUMS_GROOVE` shows each section's groove and the
bars that depart from it.

r[quantize.stays-in-span]
Quantizing a span moves nothing outside it: the first hit after it is
cut where it is, so the slice holding the span's last moved hit does not
carry the rest of the take with it, and the check fails on any hit
outside the span that plays somewhere else after the edit.

r[quantize.written]
With the drum MIDI to guide by, each piece's hits are matched one to one,
in order, with that piece's own written notes, after the drummer's feel
read from the hits around each one (a section can be pushed in one half
and dragged in the other); a hit and a note that are each other's nearest
are a match. A hit the chart has goes to its note — even past the line
before it — and a fill the chart writes is edited like the rest of its
section. A hit the chart does not have is placed by the section's groove
or the nearest line. No hit is sent more than a sixteenth and a little
from where it was played.

r[quantize.seams]
An edit leaves no silence: every seam between two slices overlaps by the
crossfade, a slice reaching back into its own source but never over any
hit's attack (a cut hit's, or one riding uncut in a slice, like a flam's
grace), the slice before running on into the next hit's lead-in, and what
neither can close drawn over by the slice before. A quantize of several
spans is one edit, its seams closed together; the check counts any gap.

r[hits.place]
A hit is put where it should play by hand — dragging its flag, or
`HITS_PLACE` — and the whole kit is cut a lead before it and moved with
it, the hits either side left where they play, the seams closed as a
quantize closes them. Dragging with Ctrl moves the marker alone: where
the hit was found, not where it plays.

r[hits.revert]
A stretch of the edit is undone by itself — a selection, the folded kit
rows, a section — every hit there put back where it was played, the
whole kit together, the seams closed as a quantize closes them; the hit
markers stay, to drag into place by hand or quantize again. A
no-quantize zone goes over it (unless asked not to), so a later
quantize leaves the hand work alone.

r[quantize.reads-markers]
Audio quantize reads hit markers and never runs detection. With no hit
markers in its selection it says so and does nothing.

r[quantize.one-core]
MIDI notes and audio hits are quantized through the same core
(`expression-editor-tools::quantize`), so the two never disagree about
which grid line an event belongs to.

r[quantize.dry-run]
Every quantize can be a dry run, returning each move it would make.

r[quantize.check]
After applying, Quantize reports against pass/fail limits:

- each hit's remaining distance from its target;
- any double attack created at a seam;
- any gap left, and any level dip across a crossfade;
- that every mic of a piece was cut at the same sample;
- that no hit was lost or duplicated.

This is what lets an agent that cannot listen know an edit worked.

## Inspecting

r[hits.explain]
For any moment, the agent can see what the detector saw: aligned peaks
of the piece's mic, its trigger and its neighbours' mics, with the
detector's envelope, its threshold and the bleed predicted there.
`AUDIO_PEAKS` already gives one track; this lines them up.

## Measuring

r[hits.bench]
Detector changes are scored against recorded triggers on real songs
(Unbreakable, Chained Expectations), excluding dead triggers, so that
a change is a number, not an impression.
