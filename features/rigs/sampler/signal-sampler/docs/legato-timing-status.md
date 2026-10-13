# Legato timing — status, findings, and open problems (2026-07-15)

Eight rounds of owner ear-passes + measurement on CSS 1st Violins (Violin 1
parity deep-dive, PR #25–#28). This documents what is PROVEN, what is fixed,
and what remains unresolved — so future work starts from knowledge.

## Proven facts (do not re-derive these)

1. **Engine scheduling is sample-exact.** Playback-emitted markers (a voice
   emits its arrival marker only when its playhead actually crosses the
   zone's marker position — `EmittedMarker`) land +0.0 ms vs grid on every
   join in every velocity lane, every scale, every chromatic. All remaining
   timing error is *data* (marker positions) or *perception* (what counts
   as "arrival"), never arithmetic.
2. **CSS ships no per-zone arrival data.** The decoded NKI (AudioHaven
   `_decode/1st-violins/`) shows transition zones with `loop_start = -1` —
   no loop points; only sustains are looped. The KSP applies ONE
   velocity-range start offset per lane (EX 0/83/177 ms, LL 100/148/177 —
   `$1fvjk`, plus an IOI boost) to *every* transition and lets arrival land
   late-of-note-on wherever the recording puts it. Kontakt players absorb
   per-note slop by nudging MIDI. Arrival markers are OUR invention (needed
   for arrive-at-tick); they can only come from measurement.
3. **The mode-delay cap was a systematic bug** (fixed here): capping the
   prefire lead at the velocity-zone delay forced start-offset skips PAST
   `lt_offset`, chopping recorded glides mid-bend — the chopped excess is
   destination-pitched content played BEFORE the beat. Deterministically
   "early" to the ear while every emitted marker read +0.0. Worst in the
   fast lane (cap 100 ms). Removed: full-glide prefire, clamped only by
   leaving the previous note max(150 ms, 35 % of IOI).
4. **Detector-vs-perception is the core unresolved problem.** The acoustic
   settle detector has been wrong in at least four distinct harmonic-collision
   families (octave, fifth, semitone, vibrato-wobble). The owner's ear
   consistently applies: *arrival = first strong crossing; vibrato wobble
   after it is "the note", not "not arrived"*. Estimator v3 encodes this
   (single-hop ≥ 0.60 or weak ≥ 0.55 with 120 ms mean guard; validation
   mean ≥ 0.50, ≤ 150 ms below-0.5; octaves stricter) — 29 183 markers
   rewritten — and the owner STILL hears inaccuracy across keys. The
   estimator does not yet match perception.
5. **Join loudness differs from Kontakt.** Measured on identical A/B units:
   Kontakt's arrived note enters +2…+5 dB above the held-note plateau within
   150 ms; ours enters ≈ −4.5 dB and swells over ~1 s (`sustain_trim_db`
   −6 dB + `sustain_bloom_ms` 1000). A hot glide over a quiet arrival biases
   perception toward "the note came early". Uncalibrated.

## Fixed and verified in PR #28's stack

- Per-zone measured arrival markers (`arrival_ms`), idempotent measurement
  tool (`measure_arrivals`), playback-emitted marker architecture,
  full-glide prefire, KSP-authentic velocity-range LT offsets (the old IOI
  curve was CSS's *marcato* table), `start_at_tick` attack placement policy,
  re-bow flux-peak markers (0.0–7.5 ms), shorts ≤ 0.7 ms, underlay held to
  the tick, slow-bloom carrier.

## Open problems (in priority order)

1. **Transition arrival markers still don't match perception** on enough
   zones to be audible in scale runs (owner round-8: v3 still "not
   accurate"). Known bad case study: `Mix … NVLeg/ff_up_C#3_2.wav`
   (D→E borrows it repitched; whole-tone sampling grid means HALF of all
   joins are ±1 borrows). Its share curve: brief 0.61 crossing at 425 ms,
   wobble to 0.39, stable ≥ 0.7 only at ~655. Tool measures 486; ear says
   ~425; hand-override written. Candidate next approaches: (a) measure
   arrival through the ACTUAL render pipeline per zone (render an isolated
   two-note join per zone, owner-style click A/B, cross-correlate against
   the isolated destination sustain); (b) human-in-the-loop calibration UI
   (waveform + draggable marker per flagged zone — the marker-timeline GUI
   feature 65f95fac is the natural host); (c) fit a perceptual model
   (loudness-weighted pitch dominance) against the owner's per-join verdicts
   collected so far.
2. **Fresh-attack leading silence** (owner: Eb/E scale first notes ~1/16
   late, "almost no audio initially"): `start_at_tick` plays raw sample
   head; `ff_D#3.wav` reaches −20 dB only at ~97 ms (G: 55 ms — reads
   accurate). Needs a per-zone `audio_start_ms` (first energy above ~−40 dB
   of peak) written by the tool and skipped by the attack spawn (few-ms
   declick fade). Schema + tool + engine, small.
3. **Join loudness calibration** (finding 5): fit `sustain_trim_db` /
   `sustain_bloom_ms` (and possibly a transition gain envelope) to the
   Kontakt A/B envelope (target: arrived note ≥ plateau within ~150 ms).
   Pack data only.
4. **Room-clamp tuning**: full-glide prefire clamps at 35 % IOI; long
   transitions at fast tempi get partially chopped (graceful but audible —
   same failure class as the old cap, just rarer). Perceptual guard
   (bend-start marker: never skip past the point where destination content
   begins) would eliminate the class.
5. **Missing "run" sample set**: CSS auto run mode (shipped ON) switches to
   `run sim` groups at IOI ≤ 175 ms; our pack never extracted them — fast
   runs currently use normal transitions.

## Where the data lives

- Pack (arrival markers, interpretation layer): AudioHaven
  `…/Cinematic Studio Strings/_patches/1st Violins/library.styx` — NOT in
  git; `measure_arrivals --write` regenerates deterministically (the C#3_2
  ear-override at 425.0 will be clobbered by a re-run — encode it in the
  tool or accept re-override).
- KSP ground truth: AudioHaven `…/Cinematic Studio Strings/_decode/1st-violins/`.
- Diagnostic renders + reports: the acf02955 worktree `listen/` trees
  (scale matrices, sweep lanes, A/B corpus, arrival reports).
- Ear-pass history: Task issue acf02955 park notes + PR #28 body.
