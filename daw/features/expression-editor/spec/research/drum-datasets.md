# Open multitrack drum datasets — what exists, and what we can actually use

Research for [#158](https://github.com/FastTrackStudios/FastTrackStudio/issues/158),
under the map [#149](https://github.com/FastTrackStudios/FastTrackStudio/issues/149).
Scenario 2 quantizes multitracked drums, and the two engines that decide
where a hit is — `gate.rs` (envelope race, the timing detector) and
`onsets.rs` (spectral flux, the segmenter) — have never been run against
real close mics. Bleed between them is the entire difficulty, and a
synthesized fixture has none of it.

Nothing was downloaded. Every size and every URL below was checked with a
`HEAD` request or read off the owner's own page; where a fact could not
be confirmed from a first-party source it says so.

## The short answer

There is **no permissive dataset of real multitracked drums**. That is
the finding, and it shapes everything else. Every corpus with true close
mics is non-commercial or educational-only; every corpus that is CC BY is
bleed-free by construction, which is precisely the property we needed.

So the recommendation is a pair:

- **Primary — DrumGizmo's CrocellKit and DRSKit, rendered from E-GMD
  MIDI.** CC BY 4.0 on both halves. Real simultaneous mic arrays, so the
  bleed in the samples is physical. Ground truth is exact, because we
  author the MIDI.
- **Reality check — ENST-Drums.** Real drummers, eight real mics,
  hand-verified per-instrument onsets, and ghost notes deliberately
  annotated. CC BY-NC-ND: an internal evaluation corpus, never shipped
  and never redistributed.

The primary is what we tune against; the secondary is what tells us the
tuning transferred to a real room.

## Comparison

| Dataset | Licence | True close mics | Onset GT | Size | Fetch | Flams / ghosts |
|---|---|---|---|---|---|---|
| DrumGizmo kits | **CC BY 4.0** | **yes** — 13–16 ch, real array | exact (you author the MIDI) | 2.5–5.6 GB/kit | plain HTTPS zip | you synthesize them |
| ENST-Drums | CC BY-NC-ND 4.0 | **yes** — 8 mono mics | hand-verified, per instrument | 4.2 GB audio | Zenodo, anonymous | ghosts annotated; no flam label |
| MDB-Drums | CC BY-NC-SA 4.0 | **no** — one file per instrument, `has_bleed: no` | 21 subclasses, hand-refined | ~1.4 GB | `git clone` | 790 ghosts, **11 flams**, 2 drags |
| MedleyDB | NC, unstated on Zenodo | submix only for drums | not drum onsets | large | **access-request wall** | — |
| IDMT-SMT-Drums | CC BY-NC-ND 4.0 | **no** — mono, 3 instruments | XML + SVL | ~2h10m | Zenodo | not described |
| Slakh2100 | CC BY 4.0 | **no** — synthesized, drums are one stem | MIDI-exact | 104 GB | Zenodo | no technique labels |
| StemGMD | CC BY 4.0 | **no** — isolated stems, zero bleed | MIDI-exact | ~217 GB | Zenodo | no technique labels |
| GMD / E-GMD | CC BY 4.0 | **no** — Roland module output | MIDI, ±2 ms | 4.8 / 96.4 GB | direct GCS | human-played, unlabelled |
| ADTOF | CC BY-NC-SA 4.0 | no — mel-spectrograms only | chart-derived | 359 h | "upon request" | — |
| STAR Drums | BSD-3-Clause | **no** — stereo | machine-labelled, ±50 ms | ~124 h | Zenodo | none |
| Cambridge "Mixing Secrets" | educational only | yes | none | large | 403 to `curl` | real, unlabelled |
| Open Multitrack Testbed | CC (mixed) | yes | none | — | **502, dead** | — |

## The recommendation, in detail

### DrumGizmo kits — real bleed under CC BY

The one candidate that was not on the ticket's list, and the only
commercially-safe source of real per-mic drum audio found.

[CrocellKit](https://drumgizmo.org/wiki/doku.php?id=kits:crocellkit) is
CC BY 4.0 and ships 15 channels: `AmbL`, `AmbR`, `OHLeft`, `OHRight`,
`OHCenter`, `Hihat`, `Ride`, `SnareTop`, `SnareBottom`, `Tom1`, `Tom2`,
`FTom1`, `FTom2`, `KDrumInside`, `KDrumOutside`. That is a superset of
the mic set the editor will meet in the field — snare top *and* bottom,
kick in *and* out, which is exactly where bleed rejection gets decided.
Verified 5 646 502 341 B, HTTP 200.

[DRSKit](https://drumgizmo.org/wiki/doku.php?id=kits:drskit) is CC BY 4.0
with 13 mics — 7 close, 4 overhead/ambience, 2 kick front and back — and
its page states all 13 were "connected to individual channels
simultaneously".
[MuldjordKit](https://drumgizmo.org/wiki/doku.php?id=kits:muldjordkit)
is CC BY 4.0, 16 channels, verified 2 536 576 789 B — but its wiki page
notes it has notably *less* bleed than the others, so Crocell and DRS are
the ones that stress the gate.

Because every hit was captured on the whole array at once, each sample
already carries that hit's leakage into every other mic. The snare's
bleed into the hi-hat channel is measured, not modelled.

Attribution is required: "Drum samples provided by DrumGizmo.org".

**~~Unconfirmed~~ — answered by [#176](https://github.com/FastTrackStudios/FastTrackStudio/issues/176).**
No wiki page states a sample rate or a bit depth, so the headers were
read directly out of the zips, by range-requesting each one's central
directory and inflating the front of one small entry — no download:

| Kit | Rate | Format | Channels per file |
|---|---|---|---|
| CrocellKit 1.1 | 48 000 Hz | 32-bit IEEE float (`fmt ` tag 3) | 15 |
| DRSKit 2.1 | 44 100 Hz | 32-bit IEEE float | 13 |

The two recommended kits are at **different sample rates** — the same
trap #159 found in the demo material, and one a harness assuming one
rate per corpus walks into on its second kit. The samples are float, not
integer PCM. And one file per hit carries the whole array interleaved,
so a mic is a channel to deinterleave rather than a file to open.

Published MD5s, for the fetch: CrocellKit 1.1
`fa2be0f847bcd8ddef3830c1523690d3`, DRSKit 2.1
`8c4d4b61ad9d354b3b845edd5da9c133`. There are no `.md5` sibling URLs;
both come off the wiki pages.

### Why the ground truth is the real argument

The stated bug is that `onsets.rs` misses the second strike of a flam —
it "rises out of the first one's decay rather than out of silence, so its
spectral change is small and it falls below the threshold"
(`expression-editor-audio/src/onsets.rs`). Tuning that needs flams at
known spacings, at known grace-note velocities, in known quantity.

The entire public corpus contains **eleven annotated flams and two
drags**, all in MDB-Drums, and MDB is not multitrack. Worse, MDB's QC
"check for duplicate labels within a 50 ms window" means a flam is *one*
label — so it cannot tell us where the second strike was even for those
eleven.

Rendering from MIDI removes the problem rather than working around it.
Sweep flam spacing 5→60 ms, grace velocity across the ghost range, drag
pairs and buzz density, and gate recall becomes a *function* of spacing
that a test can assert on, instead of a judgement call about a threshold.

Fetch shape:

```bash
curl -fL -o CrocellKit1_1.zip https://drumgizmo.org/kits/CrocellKit/CrocellKit1_1.zip
curl -fL -o DRSKit2_1.zip     https://drumgizmo.org/kits/DRSKit/DRSKit2_1.zip
curl -fL -o e-gmd-midi.zip \
  https://storage.googleapis.com/magentadata/datasets/e-gmd/v1.0.0/e-gmd-v1.0.0-midi.zip
```

The kit pages publish an MD5 to check against. The E-GMD MIDI-only zip is
103 MB — the MIDI is the reusable asset there, not the 96.4 GB of audio,
which is Roland module output off mesh pads and has no acoustic transient
physics worth tuning against.

The kits ship as multichannel WAVs plus a DrumGizmo XML kit description,
so turning MIDI + kit into per-mic stems needs a small renderer of our
own or the `drumgizmo` CLI. That CLI is GPL — fine as a build-time tool
that never enters the tree, consistent with the map's clean-room rules.

**Honest limitation**: DrumGizmo bleed is per-hit captured then summed,
so overlapping-hit bleed is a linear superposition rather than true
simultaneous capture. Cymbal wash and snare-buzz interaction *between*
adjacent strikes will be under-represented. That is the reason ENST stays
in the loop.

### ENST-Drums — the reality check

Sources: the [dataset site](https://perso.telecom-paristech.fr/grichard/ENST-drums/),
the [ISMIR 2006 paper](https://perso.telecom-paristech.fr/grichard/Publications/ISMIR2006_Gillet.pdf),
[Zenodo 7432188](https://zenodo.org/records/7432188), and a v1.1 record
[Zenodo 21506051](https://zenodo.org/records/21506051) posted 2026-07-23.

Eight mono mics per the paper §3.1 — Beyerdynamic M-88 on kick, SM57 on
snare, Schoeps CMC cardioid on hi-hat, two SM58s on mid and low-mid tom,
Sennheiser 441 on low tom, two AT4040 overheads — into a Tascam MX2424 at
**16 bit / 44.1 kHz**. Each sequence ships those eight plus a dry stereo
mix, a wet stereo mix and the accompaniment.

The bleed is real and the authors say so: "the hi-hat track… required
many more manual corrections, as the snare drum was also present in this
track." Annotation was semi-automatic — an onset detector, hand-corrected
in Wavelab against video, double-verified. About 79 600 strokes across
three drummers, ~60 phrases each at three tempi and two complexity levels
("straight without ornaments, and complex with fill-ins and ornaments"),
plus soli and minus-one sequences.

On ghost notes, §3.5.2: "Attenuated 'Ghost notes' played off-beat and
used to create a feeling of 'groove', especially in styles such as Funk
or Shuffle-Blues. **These events were annotated.**" It also warns that
quiet *time-keeping* strokes were **not** annotated — an evaluation trap
worth writing into whatever harness consumes this, since a gate firing on
those scores as a false positive against a reference that simply declined
to mark them.

There is no flam label in the 20-label set. But annotation is per onset
per track, so a flam should surface as two `sd` events tens of
milliseconds apart — which is exactly the per-strike timing MDB cannot
give. **That is an inference from the annotation method, not an author
claim**; verify it on first fetch by histogramming inter-`sd` intervals
on the snare channel.

**Licence gotcha, and it is the important one.** CC BY-NC-ND 4.0, and the
site states plainly "No commercial use is possible." NC bars commercial
use; ND nominally bars distributing derivatives. So: internal evaluation
only, never vendored, never shipped, nothing derived from it baked into a
release asset. Fetch verified at 4 201 272 613 B, HTTP 200:

```bash
curl -fL -o enst-audio.tar.bz2 \
  https://zenodo.org/api/records/7432188/files/ENST-drums-audio.tar.bz2/content
```

### MDB-Drums — labels only, and it is the ticket's trap

The ticket warned one obvious candidate was stereo-only. MDB-Drums is the
subtler trap: its "multi-track" means one file *per instrument*. Walking
all 23 `*_METADATA.yaml` files, every track has exactly one raw and one
stem labelled `drum set`, with `has_bleed: 'no'`. There are no close
mics. Useless for the bleed problem.

Its labels are still the most detailed published. Aggregated across all
23 subclass files: CHH 1847, KD 1539, SD 1510, RDC 835, **SDG (ghost)
790**, PHH 523, SDB 332, OHH 269, CRC 126, LFT 46, SST 38, TMB 32, MHT
26, RDB 16, CHC 15, HFT 14, **SDF (flam) 11**, SPC 10, SDNS 9, HIT 4,
**SDD (drag) 2**.

The [paper](https://musicinformatics.gatech.edu/wp-content_nondefault/uploads/2017/10/Wu-et-al_2017_MDB-Drums-An-Annotated-Subset-of-MedleyDB-for-Automatic-Drum-Transcription.pdf)
is also the citation for our own framing: existing datasets "only contain
annotations of basic techniques… more detailed annotations on techniques
such as **flam, drag, and buzz rolls are missing**."

Worth ~1.4 GB of `git clone https://github.com/CarlSouthall/MDBDrums`
purely to calibrate what *we* mean by a ghost versus a flam against what
human annotators meant. CC BY-NC-SA.

## Ruled out, with the reason

- **MedleyDB** — Zenodo records 1649325 and 1715175 are
  `access_right: restricted`, a request-and-approve wall, so not
  scriptable on a fresh clone. Neither record carries a licence field.
  Audio was "modified from its original version", so even approved raws
  are not guaranteed untouched close mics.
- **IDMT-SMT-Drums** — 44.1 kHz **mono**, kick/snare/hihat only, no mic
  array, no bleed. CC BY-NC-ND.
  ([Fraunhofer](https://www.idmt.fraunhofer.de/en/publications/datasets/drums.html))
- **Slakh2100** (104 GB, CC BY 4.0) and **StemGMD** (~217 GB, CC BY 4.0)
  — permissive, but sampler renders with drums as a single whole-kit stem
  or nine isolated stems. Zero bleed by construction.
- **GMD / E-GMD** — CC BY 4.0, human-performed with velocity, MIDI
  aligned within 2 ms. The MIDI is genuinely valuable and is part of the
  recommendation; the audio is a Roland module's full-kit output, not a
  mic array. E-GMD's zip verified at 96 422 999 145 B (the page claims
  90 GB).
- **ADTOF** — mel-spectrograms only, no audio to gate; NC; the Zenodo
  copy is "available upon request".
- **STAR Drums** — BSD-3-Clause per Zenodo 15690078, but stereo, and its
  labels come from running source separation then an ADT algorithm,
  evaluated at ±50 ms. A flam is 15–40 ms; the tolerance is wider than
  the phenomenon.
- **Cambridge "Mixing Secrets"** — real sessions with real arrays, but
  the stated terms are "educational purposes only… should not be used for
  any commercial purpose", research terms are agreed per contributor,
  there are no annotations, and `curl -sI` on the library returns **403**.
- **Open Multitrack Testbed** — `http://multitrack.eecs.qmul.ac.uk/`
  returns **HTTP 502**. Dead.
- **Weathervane / Telefunken** — registration walls, no public licence.
- **DrummerNet** is a method, not a dataset. **"A-DRUM"** could not be
  found from any first-party source; the intended reference is probably
  ADTOF or A2MD, unconfirmed either way.
- **The Spheres Dataset** ([Zenodo 17347681](https://zenodo.org/records/17347681),
  CC BY-SA 4.0, 46.6 GB, 23 mics with "controlled bleeding" plus isolated
  stems and RIRs) turned up in the Zenodo sweep and is a good precedent
  for how such a corpus should be packaged — but it is orchestral, not
  drums.

## What this means for the engines

1. Tune against rendered DrumGizmo material with exact MIDI ground truth;
   the gate's absolute threshold (the bleed rejector) and the crest test
   both become measurable against a known answer per mic.
2. Assert flam recall as a curve over grace spacing rather than picking a
   threshold. `onsets.rs` documents its conservatism as deliberate — the
   test should record where the knee is, not demand it move.
3. Validate on ENST's annotated ghost notes, and remember that its
   unannotated quiet time-keeping strokes will read as false positives.
4. If real flam timing is ever needed at volume, the cheapest honest path
   is an hour recorded on our own kit through the array we actually ship
   against — we own that licence outright, and a deliberate flam-spacing
   ladder against a click is something no public corpus offers.

## What was built on this, and what it measured

[#176](https://github.com/FastTrackStudios/FastTrackStudio/issues/176)
turned the recommendation into `features/expression-editor/expression-editor-corpus`
— `fetch-corpus.sh` plus a `drum-corpus` tool. Its README carries the
detail; the findings that change how the engines should be approached:

1. **The flam curve exists now.** On the synthetic sweep, a flam played
   the ordinary way (grace, then accent) is resolved into two hits from
   about **40 ms** apart, graded down through 75% at 25–35 ms to nothing
   below 15 ms. A ghost landing *inside* the previous strike's decay is
   **never** resolved anywhere in 5–60 ms; widening the axis puts that
   knee near **200 ms**. Point 2 below is therefore settled: the knee is
   recorded, not moved.
2. **The missing strike in a normal flam is the accent, not the grace.**
   Log compression values a rise out of silence far above a louder rise
   out of a decay, so the detector collapses the flam onto the *grace's*
   position — earlier than where the note is. A quantize error, not a
   detection one, and invisible if you only count onsets.
3. **Onsets lag by a median 5.7 ms**, worst 11 ms, which is the hop plus
   flux build-up. That is the concrete size of the gap `gate.rs` exists
   to close.
4. **`OnsetConfig::default()`'s 50 ms `min_spacing_secs` decides every
   flam before the audio is examined.** Correct for segmenting, fatal
   for measuring; any future flam work that leaves it alone measures
   nothing.

## Not asserted

The ENST licence PDF itself was not opened (terms come from the site text
and the Zenodo record); no archive interior was inspected beyond the
DrumGizmo `fmt ` chunks read over HTTP range requests, because nothing
else was downloaded; the flam numbers above are measured on synthetic
snares with no bleed, so they are an optimistic bound on what a real kit
will show; and the Cambridge-MT terms page returned 403 to a direct
request, so its terms come from its indexed library and FAQ text.
