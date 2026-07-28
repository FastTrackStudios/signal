# Manual notes — Pro-Q 4 + SplitEQ (primary references)

Extracted from `proq4-manual.pdf` (FabFilter, v4) and `spliteq-manual.pdf`
(Eventide, Rev 2, 2022). Behavior reference only — both are commercial
products; SplitEQ's Structural Split is additionally **patented (US
10,430,154 B2)**, so our transient/tonal split must be a different,
clean-room algorithm (median-filter HPSS family per ZL-Splitter research),
matched at the *feature* level, not the method level.

## Pro-Q 4

### Filter shapes (10)
Bell, Low Shelf, Low Cut, High Shelf, High Cut, Notch, Band Pass,
Tilt Shelf, **Flat Tilt**, **All Pass** (phase-only).

### Slopes
- Universal slope support **for all shapes**: 0–96 dB/oct, any *fractional*
  value in between; Low/High Cut extend to **Brickwall**.
- Minimum slope per shape: Bell/Notch 12 dB/oct; Low Cut/High Cut/Band Pass
  0 dB/oct; all others 6 dB/oct.
- Q not adjustable at 6 dB/oct slope.

### Band parameters
- Frequency 10 Hz–30 kHz (note-entry accepted: "A4", "C#2+13").
- Gain −30…+30 dB (Bell/Shelf/Flat Tilt only).
- Q: 1.0 = default bandwidth (FF's own convention — not RBJ Q; shelf Q
  chosen for a "good range of shelf shapes").
- Gain-Q interaction toggle (Bell only): analog-console-style — Q narrows
  as gain rises, slight gain added at very narrow Q.
- Up to 24 bands; band numbers are stable across deletion (automation).
- Per-band stereo placement: Stereo / Left / Right / Mid / Side (+ split
  button duplicates band into L+R or M+S pair). Surround: per-speaker.

### Dynamic EQ
- Any Bell/Shelf (and Flat Tilt for range) band; any slope; works in
  Zero Latency, Natural Phase, and Linear Phase (≤ High resolution).
- **Dynamic range ring**: −30…+30 dB. Positive = expansion (gain rides
  up), negative = compression (gain rides down). Yellow bar = live gain
  inside red range indicator.
- **Auto mode default**: attack + release auto-set and threshold
  *continuously adapts* to the band-limited trigger level. Behavior is
  "highly program dependent: attack, release and knee all depend on the
  processed audio, the frequency range of the EQ band and the current
  dynamic range".
- Expanded panel: threshold slider (top = Auto, trigger level metered in
  the slider), soft knee (starts slightly below threshold), external
  side-chain toggle, attack/release knobs as ±% around auto (50% center =
  auto), triggering source **Band** (band-limited to the band's range,
  default) or **Free** (custom low/high-cut trigger filters + audition).
- Alt+wheel trade gain↔dynamic-range.

### Spectral dynamics (per-band "soothe")
- Variation of dynamic EQ: instead of moving the whole band's gain, only
  the frequencies **within the band** that exceed threshold get gain
  reduction (per-bin), others untouched.
- Same controls as dynamic (auto threshold/attack/release) plus:
  - **Spectral Density** slider: selectivity — low = wide triggered
    ranges, high = very narrow/specific.
  - **Spectral Tilt** (default on): +3 dB/oct tilt applied to the input
    spectrum before triggering so highs trigger slightly more (pink-noise
    normalization).
- Spectral bands force **linear-phase** processing for that band (others
  keep the global mode). Processing Resolution ≤ High only.

### Processing modes
- **Zero Latency**: analog-matched magnitude, no latency.
- **Natural Phase**: matches analog magnitude AND phase response, no
  noticeable pre-ring/long latency.
- **Linear Phase** with resolutions (latency @44.1k): Low 3072, Medium
  5120 (recommended), High 9216, Very High 17408, Maximum 66560 samples.
  L/R-specific + M/S-specific bands simultaneously → two LP stages →
  double latency. Smooth (zipper-free) frequency changes even in LP mode.
- Latency-in-samples scales with sample rate to keep ms/resolution.

### Character modes
Clean (default) / Subtle (vintage saturation, program + frequency
dependent, affected by EQ bands) / Warm (tube-like).

### Workflow features (UI-level, later)
EQ Sketch (draw a curve → bands inferred), Spectrum Grab (grab peaks in
the analyzer), EQ Match (match spectrum to reference/audio file),
Instance List with cross-instance **collision detection**, external
side-chain.

## SplitEQ (Eventide)

### Structure
- Two full EQs in parallel: **Transient** (green) and **Tonal** (blue);
  white = linked/both. Split is complementary: Transient + Tonal
  reconstructs the input exactly (null-sum guarantee).
- 8 bands: dedicated **Highpass + Lowpass** (independent Transient and
  Tonal cutoffs) + 6 assignable bands with types: Low Shelf, Peak,
  Notch, High Shelf, Tilt Shelf, Bandpass. Slope selector per band
  (dB/oct). Bands 1–6 share ONE frequency across the two streams; gains
  (and Q) are per-stream, adjustable linked (offset-preserving; Q ratio-
  preserving) or split. Notch replaces gain with a **Split Source**
  selector (Transient/Both/Tonal).
- Per-band **Pan** per stream (L/R or M/S width mode). Master per-stream:
  output gain ±30 dB, pan/width, **EQ Scale** (curve-amount macro),
  solo Transient/Tonal, phase invert, EQ bypass.

### Split engine (feature-level; method is patented — do NOT copy)
- Finds time-frequency regions of relative stability → Tonal;
  input − Tonal = Transient. NOT level-dependent (unlike compressors).
- Controls:
  - **Source**: coarse algorithm tunings (full drums, electronic beat,
    piano, guitar, vocal) — scales internal parameters; higher-polyphony
    tunings for complex sources. Source Lock across presets.
  - **Transient Separation** 0–100%: main decision point; higher = smaller,
    sharper transient regions.
  - **Transient Decay** 0–100%: limits Transient→Tonal transition rate =
    longer transient tails / slower tonal swells.
  - **Smoothing** 0–50 ms: decision speed; trims transient chirps,
    softens attacks, reduces curve differences.
  - Split solo (Transient/Tonal) for tuning.
- Latency: 3592 samples @44.1/48k, 7176 @88.2/96k, 14344 @176.4/192k
  (STFT history — ~75 ms). "Not fit for real-time use."

### Analyzer
Sources: All / Split (T+T separately) / Transient / Tonal; Pre and/or
Post routing; resolution trade (freq vs time); decay; freeze.

## Feature-level targets for our engines

1. **eq-dsp filter set**: add whatever is missing of: Bell, Low/High
   Shelf, Low/High Cut (0–96 dB/oct + brickwall), Notch, Band Pass,
   Tilt Shelf, Flat Tilt, All Pass; fractional slopes; Gain-Q interaction
   option; per-band stereo placement (Stereo/L/R/M/S).
2. **Dynamic EQ**: per-band detector on band-limited (or Free-filtered,
   or external) trigger; auto threshold (continuously adaptive) with
   manual override; program-dependent attack/release with ±% trim around
   auto; soft knee; dynamic range −30…+30 dB (bipolar = compress or
   expand); works minimum-phase.
3. **Spectral EQ**: per-band per-bin dynamics (STFT): bins in-band
   exceeding an (auto) threshold get pulled toward the band's dynamic
   target; Density (selectivity) + 3 dB/oct trigger tilt; linear-phase
   band path.
4. **Transient EQ**: clean-room transient/steady split (ZL-Splitter
   median-filter family), then two parallel EQ stacks + per-stream
   master gain/pan/scale, linked-edit parameter model, null-sum
   reconstruction guarantee, split solos.
