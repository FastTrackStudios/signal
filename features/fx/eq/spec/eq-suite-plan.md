# EQ Suite plan — Pro-Q-class filters + Dynamic / Spectral / Transient EQ

Master plan for the EQ modernization. Companions:
- `manual-notes.md` — Pro-Q 4 + SplitEQ manual extraction (the behavior bar).
- `proq4-manual.pdf`, `spliteq-manual.pdf` — the manuals themselves.

## License / provenance table

| Reference | License | Use |
|---|---|---|
| ZLEqualizer / ZLSplitter (ZL-Audio) | AGPL-3 | **IDEAS ONLY** — parameter ranges, UX, structure |
| ANINA (CRQL — the "ANITA") | freeware, closed | behavior only (soothe-class spectral shaper) |
| FabFilter Pro-Q 4, Eventide SplitEQ | commercial | behavior only; SplitEQ split method **patented** (US 10,430,154 B2) — ours is Fitzgerald HPSS |
| nih-plug Spectral Compressor | GPL-3 | IDEAS ONLY |
| Vicanek papers (matched biquads 2016, reverse-IIR 2022), Fitzgerald 2010 HPSS, RBJ, Orfanidis | published math | **FAIR GAME** — implement from the papers |
| chowdsp_utils (BSD-3), Signalsmith DSP (MIT), rustfft/realfft (MIT/Apache) | permissive | **PORTABLE** |

Key clean-room position: everything interesting in ZL's core is published math
(Vicanek matched filters, Fitzgerald median HPSS). Implement from papers; use
ZL only for parameter ranges/UX.

## Current state (survey summary, 2026-07-28)

- **eq-dsp is already Pro-Q-class on static filters**: all 10 shapes (Bell,
  L/H Shelf, L/H Cut, Notch, BandPass, TiltShelf, FlatTilt, AllPass) + 3
  unreachable extras (BandShelf, BandPassVariant, ShelfAlt); MZT cascade
  design, TDF2 (DF1 for Peak), orders to 16, conformance tests at 0.005 dB.
- **Four incompatible shape orderings** across `FilterType` /
  `FilterShape::pro_q_type_id` / `EqBandShape::all()` / plugin `shape_to_int`.
- **NativeEq**: 24 bands × 6 fields (`used,on,freq,gain,q,shape`), id =
  band*6+field (0–143). No slope param (all bands stuck at order 2!), no
  output gain, no M/S, no readback. Param-id layout traps renumbering —
  new params must APPEND (ids ≥ 144), never insert fields.
- **Already in-tree and reusable**: `trigger_dsp::HpssProcessor` (Fitzgerald
  median-filter transient/steady split with latency accounting!),
  `trigger_dsp::TransientShaper`, `comp_dsp::{Detector, GainCurve,
  HermiteCubicSmoother}`, `level_dsp::DeEsser` (a one-band dynamic EQ),
  `audiocore_dsp::{EnvelopeFollower, Oversampler}`, `tune-dsp/dna.rs` STFT,
  spectrum-analyzer with ZL-style collision detection.
- Missing: dynamic bands, spectral processing, transient split in EQ, M/S
  per band, phase modes, real brickwall, per-band bypass ramps.

## Architecture decisions

1. **Dynamic model = ZL's base/target crossfade** (not GR-on-gain):
   `gain_db(t) = base + d(t)·(target − base)`, detector d(t) ∈ [0,1].
   Pro-Q's "dynamic range" maps exactly: range = target − base (bipolar).
   Symmetric boost/cut, previewable as two drawn curves.
2. **One shared detector** used by dynamic bands, spectral bins, and the
   transient splitter's realtime mode:
   level (peak/RMS-mix) → dB → soft-knee window (thr −80..0 dB, knee 0–32
   dB, then squared S-curve) → 0..1 drive → attack/release one-pole with
   punch-smooth blend (attack 0–1000 ms, release 0–5000 ms, log-mid tapers).
3. **Auto threshold** = decaying loudness histogram → percentiles: threshold
   = P50, knee = clamp(0.5·(P_hi − P_lo), ≥5 dB); user knob becomes an
   offset while learning. **Relative mode** feeds `band_dB − program_dB`
   (band loud *relative to the mix* — soothe-adjacent).
4. **Dynamic band coefficient path**: decimated control-rate redesign
   (every 32 samples) with preallocated scratch (Band redesign must be
   alloc-free after warmup) + per-sample dB smoothing of the gain.
5. **Spectral EQ** = STFT (512–2048 block, 4× overlap, Hann; latency ≈
   block+hop ≈ 11 ms @1024/48k): per-bin gain from (a) drawn curve sampled
   at bins, (b) per-bin dynamics vs absolute or **relative** (spectrally
   smoothed program spectrum) threshold — relative = ANINA/soothe; band-
   masked relative = Pro-Q Spectral. Controls: Density (mask sharpening),
   trigger Tilt +3 dB/oct, Freeze, Gate, **Delta morph** (suppress ↔
   isolate), sidechain spectrum.
6. **Transient EQ** = complementary splitter → EQ_A(transient) + EQ_B
   (steady) → sum (null when both flat). Two splitter qualities:
   - realtime: dual-RMS peak/steady relative-threshold mask (Attack,
     Balance ±50, Hold, Smooth; zero latency);
   - HQ: Fitzgerald median HPSS via `trigger_dsp::HpssProcessor` extended
     with Balance (exponential mask weight), Strength (mask sharpening
     around 0.5), Hold (per-bin max-decay), Smoothness (blend toward mean
     mask = broadband); latency = fft + 2 hops.
   Per-stream master gain/pan/scale + solos, SplitEQ-style.
7. **Phase modes** (later phase): chowdsp-style prototype rendering —
   sample the minimum-phase response → linear-phase FIR → background swap.
   Zero-latency IIR stays the default.
8. **Tapers**: adopt ZL's log-mid taper helper (min/mid/max, midpoint at
   50% travel) for freq/Q/knee/attack/release params.

## Phases

- **Phase 0 — hygiene (prereq)**: one canonical shape enum ordering
  (append-only, documented), one slope→order table; expose slope/order in
  NativeEq via APPENDED params; make Brickwall reachable; reach
  BandShelf/BandPassVariant/ShelfAlt from the param layer; per-band bypass
  click ramp.
- **Phase 1 — shared detector** (`eq-dsp/src/dynamics/`): Detector,
  soft-knee, punch-smooth follower, loudness histogram auto-threshold,
  relative mode, side-filter (BP/LP/HP band-linked or free). Unit tests.
- **Phase 2 — Dynamic EQ**: `DynBand` wrapper (base/target/d(t) crossfade,
  decimated redesign), chain wiring, NativeEq appended params
  (per-band: dyn_range, dyn_on, threshold(+auto), attack%, release%,
  side free lo/hi, relative, external-SC), tests (sine-burst gain rides,
  auto-threshold convergence, alloc-free processing).
- **Phase 3 — Spectral EQ**: new `eq-spectral` module/crate: STFT core,
  per-bin detector array, relative threshold spectrum, density/tilt,
  freeze/gate/delta, drawn-curve static mode. Latency reporting. Tests
  (resonance suppression on synthetic resonant noise, null at Amount 0,
  delta complementarity).
- **Phase 4 — Transient EQ**: peak/steady realtime splitter (new, in
  trigger-dsp or eq-dsp), HPSS param extension, dual-EqChain engine +
  per-stream masters, null-sum test, splitter solo/delta.
- **Phase 5 — UI + integration**: eq-ui dynamic range ribbons + threshold
  handles, spectral band shading, transient dual-curve editing (SplitEQ
  linked-edit model), signal-fx registration of the new engines, PatternEditor
  crossover (drawn curves as spectral EQ static curves).

## Parameter sketches (ranges/tapers)

Dynamic band (per band, appended ids): dyn_on 0/1; dyn_range −30..+30 dB;
threshold −80..0 dB (top = auto) + learn 0/1 + relative 0/1; attack 0–100%
(50 = auto); release 0–100% (50 = auto); side_mode 0=band,1=free;
side_lo/side_hi 10–30k (free mode); ext_sc 0/1.

Spectral engine: amount 0–100%; density 0–100%; tilt on/off; attack ms
(per-bin), release ms; lo/hi work range; gate dB; freeze; delta −100..100
(suppress↔isolate); block 512/1024/2048; sidechain on/off.

Transient engine: split quality (rt/hq); separation/strength 0–100;
balance −50..+50; hold 0–100; smooth 0–100; per-stream gain ±30 dB,
scale −100..200%, solo; then 2 × 24-band EQ params.
