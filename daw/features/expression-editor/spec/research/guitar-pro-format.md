# What a Guitar Pro file carries

Research for #160 (wayfinder map #149, scenario 4: *import a Guitar Pro
file and show it as a six-string roll with bend flow*). This is a
data-model summary, not a code sketch: the point is the **enumeration**,
because that enumeration is the spec for `Mode::Guitar`'s note extras.

**Clean-room note.** Every source below was read, none was copied. The
one the ticket names — `tommythecat/reaper-gp-import/gpimport.lua` — has
no licence and is therefore all rights reserved; it was read to see what
a shipping importer chose to map, and nothing from it enters this tree.
Same stance as Perfect Timing and MPElodyne (`references.md`).

Sources, with what each is allowed to be:

| source | licence | status here |
|---|---|---|
| `gpimport.lua` (tommythecat) | **none** | read-only clean-room reference |
| `guitarpro` crate 0.4.2 / *scorelib* (slundi) | **MIT** | **usable as a dependency** |
| alphaTab `GpifParser.ts` / `BendPoint.ts` (CoderLine) | LICENSE not SPDX-detected | read-only corroboration |
| PyGuitarPro (Perlence) | LGPL-3.0 | read-only corroboration |

The binary layout facts below are quoted from `FILE-STRUCTURE-NOTE.md`,
which ships **inside the MIT crate** — it is the community-documented
GP3/4/5 layout, not a leak from anything proprietary.

---

## 1. The formats, and which matter

There are two entirely different families behind the name.

| ext | GP version | container | what it is |
|---|---|---|---|
| `.gp3` | 3 | plain binary | flat, little-endian, length-prefixed strings |
| `.gp4` | 4 | plain binary | GP3 + note effects (harmonics, trills, tremolo picking, whammy) |
| `.gp5` | 5 | plain binary | GP4 + second voice, per-note duration, RSE, richer slides |
| `.gpx` | 6 | **BCFZ/BCFS** compressed archive | proprietary container wrapping the same GPIF XML |
| `.gp` | 7 / 8 | **ZIP** | `Content/score.gpif`, an XML document |

The binary family and the XML family share a *musical* model but share
no bytes. Any importer either handles both or picks one.

**What the reference does.** `gpimport.lua` handles **`.gp` (GP7/GP8)
only** — it checks the `PK` ZIP magic, shells out to PowerShell to
extract `Content/score.gpif`, and refuses anything else with "Older GP5
binary files are not supported." It is also Windows-only for that
reason. Worth knowing before treating it as a model: **it is a far
thinner importer than the ticket assumed.** From the GPIF it reads only
track name, MIDI program, primary channel (channel 9 ⇒ drums), note MIDI
pitch, muted flag, ties, rhythm/tuplet/dot, dynamics, time signature and
tempo. It reads **no string, no fret, no bend, and no articulation of
any kind**, and emits plain MIDI note-on/off. So it answers "how does
one map GP onto a DAW timeline" and answers nothing at all about the
expression vocabulary — which had to come from the format itself.

**What we should do.** Both families, and we get both free (§5).
`.gp` is what current Guitar Pro writes and what most new tabs are; the
`.gp3/4/5` corpus is enormous and is most of what exists in the wild
(Ultimate-Guitar-era archives). `.gpx` (GP6) is the awkward middle — same
XML, hostile container — and is worth having but is not scenario-critical.

---

## 2. What a note carries

The union across formats. Names are given in our terms; the parenthetical
is where it lives.

### Position
- **string** — 1-based index *from the top string* (string 1 = highest
  pitch). Not a MIDI channel, not an inference.
- **fret** — absolute fret number. Sounding pitch is
  `fret + tuning[string]`, plus capo, plus track transposition.
- **note type** — `Rest | Normal | Tie | Dead`. `Tie` means "continuation
  of the previous note on this string" (no re-attack); `Dead` is the
  muted X-notehead. Both are structural, not decorative: a tie chain is
  *one* played note in our model.

### Articulations — the vocabulary the ticket asked to enumerate
Every one of these is present in the binary format's note-effect header
bits and again in GPIF as a named `<Property>`:

| articulation | shape | notes |
|---|---|---|
| bend | **point list** (§3) | the crux; also used for the whammy bar |
| slide | typed, and *multiple can coexist* | see below |
| hammer-on / pull-off | one flag, `hammer` | GP does **not** distinguish h from p — direction is implied by the pitch of the next note. GPIF splits it into `HopoOrigin`/`HopoDestination`; alphaTab computes the destination rather than trusting it |
| vibrato | bool (note-level) | plus a *separate* beat-level vibrato, and a per-bend-point vibrato flag |
| palm mute | bool | |
| harmonic | typed + optional fret | `Natural(1) / Artificial(2) / Tapped(3) / Pinch(4) / Semi(5)`. GP5 also encodes artificial-at-interval as raw `15 = +5th, 17 = +7th, 22 = +12th`. GPIF carries `HarmonicFret` as a *float* (7.0, 5.0, 3.2 …) |
| ghost note | bool | in GP5 it's a header bit; in GPIF it arrives as `AntiAccent` |
| let ring | bool | note-level, but tracks also carry an `auto_let_ring` setting |
| tapping | **beat-level**, not note-level | `SlapEffect: None / Tapping / Slapping / Popping` on the beat. GPIF additionally has a note-level `LeftHandTapped` |
| tremolo picking | typed by *rate* | `1 = 8th, 2 = 16th, 3 = 32nd` |
| staccato | bool | |
| accent / heavy accent | two bools | GPIF packs accent, heavy accent and staccato into one bitmask |
| trill | fret + period | `period: 1 = 16th, 2 = 32nd, 3 = 64th`; fret is the *other* note |
| grace note | its own object | fret, dynamic, duration (`1 = 32nd, 2 = 24th, 3 = 16th`), on-beat flag, and a **transition** `None / Slide / Bend / Hammer`. `fret == -1` means a dead grace note |
| fingering | left + right hand | `-1 nothing, 0 thumb, 1 index, 2 middle, 3 ring, 4 little` |

**Slides are a set, not a scalar.** The binary format stores one signed
byte — `-2 into-from-above, -1 into-from-below, 1 shift, 2 legato,
3 out-downwards, 4 out-upwards` — but GPIF stores a **bitmask**, and an
in-slide and an out-slide legitimately coexist on one note (bits: `0x01`
shift, `0x02` legato, `0x04` out-down, `0x08` out-up, `0x10` in-from-below,
`0x20` in-from-above; alphaTab also reads `0x40`/`0x80` as pick-slide
down/up). Model slides as **`slide_in: Option<…>` + `slide_out: Option<…>`**,
not one enum.

### Loudness
Dynamics are symbolic (`ppp…fff`), not a velocity byte. The mapping
used by the MIT crate is `velocity = 15 + 16·d − 16`, i.e. `ppp = 15`
through `fff = 143` clamped — with `f` (d = 6) as the default. The
reference script instead hard-codes `PPP=16 … FFF=127`. Neither is
authoritative; **pick one and own it**, because "the tab said mf" is the
real datum and the velocity is our rendering of it.

### Beat level (shared by every note in the beat)
duration + dotted + tuplet, chord diagram, free text, `fade_in`,
`has_rasgueado`, strum `stroke` (direction + speed + swap), `pick_stroke`
direction, `slap_effect` (the tap/slap/pop above), beat `vibrato`, a
**tremolo bar** (whammy) — *which is the same `BendEffect` structure as a
string bend* — and a mix-table change (tempo/volume/pan/instrument
automation mid-song).

### Measure level
time signature (with beaming), key signature, tempo, markers/sections,
`repeat_open` / `repeat_close` / `repeat_alternative`, double bar,
triplet feel (`None / Eighth / Sixteenth`), navigation signs (Coda, Segno,
D.C./D.S. family), fermatas, and free time.

---

## 3. Bends — the crux

**A GP bend is exactly a point list over normalised note time.** This is
the good news the ticket was hoping for, and it holds in both format
families.

A bend (and identically a whammy dive) is:

```
kind:   a preset label
value:  the headline height
points: [ { position, value, vibrato } ]   // 2..n points, ordered
```

### Units

*Binary GP3/4/5*, per `FILE-STRUCTURE-NOTE.md`:

- **position**: integer `0..60` — "sixties of the note duration". So
  position is already **normalised time over the note**, 60 = the note's
  end. It is not seconds and not ticks.
- **value**: integer, **100 per whole tone, quantised to quarter tones** —
  `0 normal, 25 quarter tone, 50 half tone, 75 three-quarter, 100 whole
  tone, … up to 300 = three tones`. So **50 raw = 1 semitone**, and the
  ceiling is 6 semitones.
- **vibrato**: per point — `0 none, 1 fast, 2 average, 3 slow` in the
  format; both parsers reduce it to a bool.

*GPIF (GP6/7/8)* stores the same curve as up to **seven float
properties** rather than a list:
`BendOriginValue`, `BendOriginOffset`, `BendMiddleValue`,
`BendMiddleOffset1`, `BendMiddleOffset2`, `BendDestinationValue`,
`BendDestinationOffset` (plus a `Bended` flag), and the whammy equivalents
`WhammyBar*` / a `<Whammy>` element with `originValue`/`middleValue`/… 
attributes. Here **offsets are `0..100`** (percent of the note) and
**values are in 1/100 tone**, the same quarter-tone-scaled unit as the
binary format. alphaTab normalises with exactly two constants —
`positionFactor = 60/100`, `valueFactor = 1/25` — landing on
`BendPoint.MaxPosition = 60`, `BendPoint.MaxValue = 12` with `value`
documented as *"the 1/4 note value offsets for the bend"*. The two
middle offsets exist so a bend can **hold**: `middleOffset1` and
`middleOffset2` carry the *same* middle value, giving a flat plateau
between them.

So the canonical normalised form, which both families reduce to:

> **position ∈ 0..60 (fraction of the note), value in quarter-tones
> (÷2 = semitones), max 12 = 6 semitones, optional vibrato per point.**

### Presets
`kind` is a label over the same points, and is worth keeping because it
is what a tab *means*, not just what it does:
`None, Bend, BendRelease, BendReleaseBend, Prebend, PrebendRelease`, and
for the bar `Dip, Dive, ReleaseUp, InvertedDip, Return, ReleaseDown`.
**Prebend is the one that bites**: the curve starts at a non-zero value,
i.e. the string is already bent at the attack, so the note's *sounding*
pitch at onset is not `fret + tuning`.

### How this lands on our note model

Our model is `center + drift·amount + modulation·amount`. The mapping is
direct and needs no new primitive:

- **center** = `tuning[string] + fret + capo + transpose`, in semitones —
  *plus* `points[0].value / 2` when the bend is a prebend.
- **drift** = the bend curve itself: the point list resampled onto our
  per-note curve, x from `position/60` × note length, y in semitones
  = `value / 2`. `amount` is the curve's own scale — a bend is not a
  modulator with a depth knob, so `amount = 1` and the curve carries the
  magnitude. Slides, hammer-ons and legato slides are *also* drift: they
  are pitch motion over the note's duration and belong on the same curve,
  which is what makes "bend flow, Ample-style" a single rendering
  problem rather than several.
- **modulation** = vibrato — note-level, beat-level, or the per-point
  flag — with `amount` from the vibrato *rate* (`fast/average/slow`),
  which is the one place GP's data is coarser than our model and where we
  choose depth ourselves.

Two consequences worth committing to now:

1. **A tie chain is one note.** GP's `Tie` notes carry their own effects,
   and a bend can start on the tied continuation. If we import ties as
   separate notes the curve fragments; if we merge them the curve is
   continuous and `position/60` must be taken over the *merged* length.
2. **Whammy is a beat-level curve of the same type.** It is not a note
   extra. It either becomes a track-level pitch lane or gets distributed
   onto every note in the beat — an open question, but the data model
   should not pretend it is per-note.

---

## 4. Track-level data

- **tuning** — an explicit list of MIDI pitch numbers, one per string,
  **stored highest string first**. Default standard tuning is
  `[64, 59, 55, 50, 45, 40]` (E4 B3 G3 D3 A2 E2). In the binary format
  it is a fixed **7-integer table** of which the first *n* are used.
- **string count** — `n`, and it is genuinely not always six: 4/5/6-string
  bass, 7- and 8-string guitar, banjo. Everything downstream (the roll's
  lane count, the string→lane map) must be driven by `tuning.len()`.
- **capo** — one integer, the fret. Binary GP calls it the track
  `offset`; GPIF calls it `CapoFret` and hangs it on the **staff**, not
  the track. alphaTab additionally models it per-staff so a multi-staff
  track can differ. Partial capo is not in the format.
- **fret count** — default 24, per track.
- **instrument** — GM program + MIDI port and primary/secondary channel;
  GPIF adds an explicit `Sounds`/`InstrumentSet` and a `GeneralMidi`
  block. Percussion is identified by channel 9 in the binary format, and
  GPIF carries a real percussion articulation table (element/variation).
- **transposition** — GPIF `Transpose` carries `Chromatic` and `Octave`
  separately; a guitar track is written an octave above sounding pitch.
- **flags** — `percussion_track`, `twelve_stringed_guitar_track`,
  `banjo_track`, solo, mute, visible, colour, short name.
- **auto settings that change playback** — `auto_let_ring`, `auto_brush`,
  and per-track RSE (the modelled-amp stack). RSE is out of scope: we
  import notes, not tone.

---

## 5. Is there a Rust crate? Yes — MIT

**`guitarpro` 0.4.2** on crates.io (project name *scorelib*,
codeberg.org/slundi/scorelib), **MIT**, published 2026-07-11, edition
2024, 100% safe Rust. Its own support table:

| format | read | write | notes |
|---|---|---|---|
| `.gp3` / `.gp4` / `.gp5` | ✅ | ✅ | "high-fidelity legacy binary" |
| `.gpx` (GP6) | ✅ | ✅ | BCFZ/BCFS container decoded in-crate |
| `.gp` (GP7+) | ✅ | ✅ | ZIP + GPIF XML |
| `.mscz` (MuseScore 4) | ✅ | ✅ | |
| MusicXML | ✅ | ✅ | partwise and timewise |

Its model is the full vocabulary of §2 — `Note { value, velocity, string,
effect, duration_percent, kind }`, `NoteEffect` with all sixteen fields,
`BendEffect`/`BendPoint`, `HarmonicEffect`, `GraceEffect`, `TrillEffect`,
`TremoloPickingEffect`, `BeatEffects` with the whammy as a `BendEffect`,
`Track` with `strings`, `offset` (capo), `fret_count` and the instrument
flags. Dependencies are five and all ordinary: `serde`, `quick-xml`,
`zip`, `encoding_rs`, `fraction`, `thiserror`.

**So the parser is a dependency, not a build.** That is the headline
answer to the ticket.

Three caveats to go in with eyes open:

1. **The GPIF path is thinner than the binary path.** Its own README
   calls GP7 support "experimental". Concretely, `gpif_import.rs` reads
   `BendOriginValue` and `BendDestinationValue` only and *synthesises* a
   three-point curve at positions 0/6/12 with a linearly interpolated
   midpoint — **it discards `BendMiddleValue`, both middle offsets, and
   both origin/destination offsets**. For a project whose scenario is
   "bend flow, Ample-style" that is precisely the data we care about.
   It also does not read `CapoFret` on that path. Both are small,
   well-isolated upstream fixes (or a local pre-pass over the GPIF).
2. **It is a one-maintainer crate**, ~3.8k downloads, 33% documented,
   0.x. Fine to depend on, worth vendoring-friendly caution: pin it, and
   keep our own domain types rather than exposing theirs through the
   facade.
3. **Its bend positions are pre-normalised to 0..12**, not 0..60 —
   `BEND_EFFECT_MAX_POSITION = 12`, with raw values scaled by `12/60` on
   read. Point values are divided by 25, i.e. **quarter-tones**, capped
   at `max_value = 12`. Convert once at the boundary and keep semitones
   internally.

Also on crates.io: `gpx_reader`, which despite the name is for GPS
tracks — as are `gpx`, `gpx-rs`, `alltrailsgpx`. Ignore all of them.
PyGuitarPro (LGPL-3.0) and alphaTab remain useful for *reading* when a
field is ambiguous, and for nothing else.

---

## 6. What this settles for `Mode::Guitar`

- The note extras are: **string, fret, note-type (normal/tie/dead)**, and
  the flags/typed-effects of §2. That list is now closed — it is the
  format's own vocabulary, not a wishlist.
- **Bends need no new primitive.** They are a point list over normalised
  note time in quarter-tones; they land on `drift` with `amount = 1`.
  Slides and legato land on the same curve. Vibrato lands on `modulation`
  and is the one axis where we supply depth.
- **Lane count comes from the tuning**, which is a list, not a six.
- **Ties must be merged at import**, or every long bend fragments.
- **Whammy is beat-level** and needs its own decision (#161 / #168).
- The parser is `guitarpro` (MIT), with a known GPIF bend-fidelity gap to
  close before scenario 4 can look right on a `.gp` file.
