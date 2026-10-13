# The drum corpus

Build-time tooling for scenario 2 of [#149](https://github.com/FastTrackStudios/FastTrackStudio/issues/149) —
"multitracked drums quantized to the grid, tuned against a real open
multitrack drum dataset". Resolves [#176](https://github.com/FastTrackStudios/FastTrackStudio/issues/176),
built on the research in [#158](https://github.com/FastTrackStudios/FastTrackStudio/issues/158)
(`../spec/research/drum-datasets.md`).

**Nothing this produces is committed.** The corpus lands in
`$CORPUS_DIR` (default `~/.cache/fts/drum-corpus`). The repository holds
the script, the tool, and two small fixtures the tool itself generates.

```bash
./fetch-corpus.sh kits     # CrocellKit + DRSKit, md5-checked, headers read
./fetch-corpus.sh egmd     # E-GMD MIDI (103 MB; not its 96 GB of audio)
./fetch-corpus.sh sweep    # the flam grid: MIDI, ground truth, synthetic render
./fetch-corpus.sh render   # render the sweep through a kit, measure per mic
./fetch-corpus.sh enst     # gated behind --i-accept-nc-nd
```

## Licence, which is the hard part

| Material | Licence | What that permits |
|---|---|---|
| DrumGizmo CrocellKit, DRSKit | **CC BY 4.0** | Commercial use. Anything derived that ships must carry `Drum samples provided by DrumGizmo.org`. |
| E-GMD MIDI | CC BY 4.0 | Same. |
| ENST-Drums | **CC BY-NC-ND 4.0** | Internal evaluation only. Never vendored, never shipped, nothing derived from it in a release asset. The site states plainly: "No commercial use is possible." |
| `drumgizmo` renderer CLI | **GPL** | A build-time tool that is *invoked*. Never in this tree, nothing links it. |
| The flam sweep, its MIDI, its renders | ours | Free of all of the above. |

`fetch-corpus.sh enst` refuses to run without `--i-accept-nc-nd`, writes
into a directory called `internal-eval-only/`, and drops a
`LICENCE-NOTICE.txt` beside the download. `Licence::shippable()` in
`lib.rs` carries the same rule as code, so it can be asserted on rather
than remembered.

## Why a corpus has to be assembled rather than downloaded

#158's finding, and it shapes everything: **there is no permissive
dataset of real multitracked drums.** Every corpus with true close mics
is non-commercial (ENST, MDB, IDMT, MedleyDB); every CC BY corpus
(Slakh, StemGMD, GMD, E-GMD) is bleed-free by construction — and bleed
between mics is exactly the difficulty the quantize path has to survive.

DrumGizmo's kits are the way out. Every hit was captured on the whole
mic array at once, so each sample already carries that hit's leakage
into every other channel: the bleed is measured, not modelled. Ground
truth stays exact because we author the MIDI.

The honest limitation, worth repeating because it is why ENST stays in
the loop: DrumGizmo's bleed is per-hit captured and then *summed*, so
overlapping-hit interaction — cymbal wash, snare buzz between
strikes — is under-represented.

## What the headers turned out to be

#158 had to leave this open: no DrumGizmo page states a sample rate or a
bit depth. Read off the files (by range-requesting the zips' central
directories rather than downloading 8 GB):

| Kit | Rate | Format | Channels per file |
|---|---|---|---|
| CrocellKit 1.1 | 48 000 Hz | 32-bit IEEE float | 15 |
| DRSKit 2.1 | 44 100 Hz | 32-bit IEEE float | 13 |

Three things follow. The two recommended kits are at **different sample
rates**, so a harness that assumes one rate per corpus breaks on its
second kit. The samples are **float, not integer** — a reader that
assumes PCM produces noise. And **one file per hit holds the whole
array**, interleaved, so a mic is a channel to deinterleave rather than
a file to open. `drum-corpus probe` reports all three on every fetch,
and `render` re-probes what came back.

## The flam sweep, and what it measured

Flams are the case `onsets.rs` documents itself as getting wrong, and
there is essentially no public data on them: **eleven annotated flams
exist across the entire published corpus**, all in MDB-Drums, whose QC
collapses labels inside a 50 ms window — so a flam is *one* label and
even those eleven cannot say where the second strike was.

So the grid is authored: spacing 5→60 ms × grace velocity 0.15→0.6 ×
both orderings (grace-before-accent, the flam a drummer plays; and
ghost-after-accent, the decay-masking case `onsets.rs` describes).
`synth.rs` renders it deterministically for a test that needs no
download; `smf.rs` writes the identical grid as MIDI for the real kit.

The measured curve, on the synthetic sweep, with the detector's spacing
floor lowered to 3 ms so the question is answerable at all:

| Spacing | Grace before accent | Ghost after accent |
|---|---|---|
| 5–10 ms | 0% | 0% |
| 15 ms | 25% | 0% |
| 20 ms | 50% | 0% |
| 25–35 ms | 75% | 0% |
| 40–60 ms | **100%** | 0% |

Read as: a flam played the normal way is reliably resolved into two hits
from about **40 ms** apart. A ghost note landing *inside* the previous
strike's decay is **never** resolved anywhere in the 5–60 ms range —
widening the axis by hand puts that knee near **200 ms**:

```bash
cargo run -p expression-editor-corpus --bin drum-corpus -- \
  recall --spacings-ms 60,80,100,140,200,300
```

That is not a threshold to hunt. It is a statement that log-compressed
spectral flux cannot see a quiet strike inside a loud one's decay, and
that the engine which has to is `gate.rs` — two envelope followers
racing, sample by sample.

Two further findings fell out:

- **Detections lag their strike by a median 5.7 ms** (worst 11 ms) — an
  STFT reports the frame a change was observed in, not the sample it
  happened at. Quantizing straight off `onsets.rs` moves every note
  late by about a hop, which is the concrete reason `gate.rs` exists.
- **In the grace-first ordering the strike that goes missing is the
  accent, not the grace.** Log compression values a rise out of silence
  far above a louder rise out of a decay, so the detector reports the
  flam as a single onset at the *grace's* position — several
  milliseconds before where a musician would say the note is. That is a
  quantize hazard rather than a detection one, and it is invisible if
  you only count onsets.

`OnsetConfig::default()` sets `min_spacing_secs` to 50 ms, which decides
the answer for every flam before the audio is examined. That is correct
for segmenting a take and fatal for measuring flam sensitivity; there is
a test asserting it, so nobody rediscovers it the hard way.

## ENST, and verifying the inference before relying on it

ENST has no flam label. The inference is that a flam must appear as two
`sd` onsets tens of milliseconds apart, since annotation is per onset
per track and ghost notes were deliberately annotated — but that is
read off the annotation *method*, not claimed by the authors. #158 said
verify it on first fetch. `drum-corpus enst` histograms inter-`sd`
intervals and returns a named verdict rather than a boolean, because a
count alone cannot settle it: a 32nd note at 200 bpm is 37.5 ms and
lands inside the same window, and the annotations carry no velocity to
tell a ghost from an even stroke.

The other trap, from #158 and worth building into any harness on top of
this: ENST deliberately **did not** annotate quiet time-keeping strokes,
so a detector that finds them scores false positives against a reference
that simply declined to mark them.

## What is committed

- `fetch-corpus.sh`, and the `drum-corpus` tool it drives.
- `fixtures/flam-recall-baseline.csv` — the measured curve, so a change
  to `onsets.rs` shows as a diff against a recorded shape. Regenerate
  deliberately with `drum-corpus recall --csv …/flam-recall-baseline.csv`.
  It is tied to the default grid: changing the spacing or velocity axes
  changes the global maximum the detection function normalises against.
- `fixtures/annotation-format-sample.txt` — two bars of a backbeat with
  one flam in it, **written by hand**, containing no ENST data, so the
  parser and the verdict can be tested with no copy of the dataset
  present.

No audio, no kits, no MIDI corpus, nothing under NC.
