# Open-Source Delay Design Research

**Generated:** 2026-07-27. Companion to `timeline-mx-parity.md` (and the reverb
parity doc) — reference algorithms, portable code, and papers for dialing in
the delay machines and for growing new algorithm types (issue #69).

**License triage convention** used throughout:
- **PORTABLE** — MIT/BSD/CC0/permissive: code may be translated into this tree
  (keep provenance notes at the port site).
- **IDEAS ONLY** — GPL/LGPL/AGPL/unlicensed: study concepts, clean-room only,
  never copy code (repo rule: this codebase is not GPL).
- **FAIR GAME** — papers/blogs: algorithms freely implementable.

## Executive summary — the ten highest-value moves

1. **dBucket core**: adopt the Holters–Parker DAFx-18 combined BBD model
   (fixed-length sample queue at the varying BBD clock; input/output filter
   states evaluated at exact clock instants — no interpolation, real aliasing
   emerges from physics). Jatin Chowdhury's standalone **BBDDelay repo is
   BSD-3** — a portable reference implementation of that structure.
2. **dBucket parasitics**: Raffel–Smith DAFx-10 kit — NE570 compander
   (2:1, one-pole rectifier averagers, τ ≈ 2–10 ms; the attack/release
   mismatch against the delayed envelope IS "bucket loss" breathing),
   level-independent cubic THD (a=1/8, b=1/18), clock-tracking insertion-gain
   droop, in-loop noise (→ authentic self-oscillation at fb > 1).
3. **dTape repitch**: Zavalishin–Parker tape equation (speed-type: pitch =
   write/read speed ratio; difference-form cumulative-distance solver) so
   time changes and wow/flutter repitch like tape, not like a chorus.
   Airwindows **TapeDelay2 (MIT)** is a portable speed-type reference, plus
   its amplitude-keyed flutter and undersampled-regen tricks.
4. **dTape wow/flutter**: layered generator per the Echoplex AES paper —
   capstan periodic + pinch-wheel component (incommensurate rate) + random-walk
   drift; crinkle = stochastic dropout segments (Chew-style), tape age =
   spacing/gap/thickness playback-loss filter terms (Chowdhury DAFx-19).
5. **Spectral (granular)**: port Mutable Clouds' (MIT) morphing grain window
   (boxcar↔triangle↔Hann via one LUT), block pre-delay + fractional grain
   starts, deterministic↔stochastic density crossfade, and the 4-allpass
   post-diffuser (L={126,180,269,444}, R={151,205,245,405} @32 kHz, k=0.625).
6. **Spectral v2 (true FFT)**: per-bin frame-delay lines + spectral-domain
   feedback with per-recirculation ops (phase randomization, magnitude
   quantize/warp) — Clouds' `pvoc/frame_transformation.cc` is MIT and is a
   ready menu of those ops.
7. **Digital ADM voice**: real CVSD constants (N=3/4 bit-coincidence rule,
   syllabic τ ≈ 5 ms, step ratio ~16:1; embARC BSD reference); 12-bit voice =
   companding around the quantizer (envelope-modulated noise floor is the
   sound); bonus µ-law @16 kHz voice from Clouds.
8. **Ice / shimmer quality**: signalsmith-stretch (MIT, Rust bindings exist)
   as the high-tier shifter (tonality limit kills chipmunk shimmer); Valhalla
   guidance: diffusion AFTER the shifter inside the loop + in-loop LP ~4–6 kHz
   so octave ladders bloom then die.
9. **Oil Can ghost, properly**: no-erase head = partial overwrite
   `disc[w] = α·in + (1−α)·disc[w]` — a fixed tap at the rotation period
   decaying (1−α) per rev, self-consistent with feedback; spring-arm wobble =
   2nd-order resonator + Parker-style dispersive allpasses; Greyhole (now MIT
   in faustlibraries) is the diffuser-in-feedback topology for the
   reverb-echo blend.
10. **New machine candidates**: Dispersion/Chirp delay (Välimäki–Abel–Smith
    spectral delay filters: hundreds of identical first-order allpasses +
    feedback — cheap, no-FFT, no_std-safe, nothing like it at pedal tier);
    Stretch delay (PaulStretch-per-recirculation); Sympathetic/Resonant delay
    (Rings' MIT comb-network in the feedback path).

Cross-cutting: **Signalsmith `dsp` (MIT)** fractional-delay interpolators
(Lagrange-N, windowed-sinc) upgrade every modulated read; Echorec feedback is
a 4×1 head-select matrix into ONE record head ("swell" modes); RE-201-style
**motor control ballistics** (inertia-slewed speed) for knob-twist feel.

---

# Part 1 — Analog-modeled delays (tape / BBD / drum / oil-can)


# Analog-Modeled Delay Research Report
**For: features/fx/delay/delay-dsp — dialing in dTape / dBucket / Drum / Oil Can realism**

License legend: **PORT** = MIT/BSD/permissive, code can be translated into our non-GPL Rust tree. **IDEAS** = GPL/unlicensed, clean-room concepts only. **PAPER** = academic paper, algorithms fair game.

---

## 1. Jatin Chowdhury / ChowDSP

### 1.1 ChowTapeModel / AnalogTapeModel — **IDEAS (GPLv3) + PAPER**
- Repo: https://github.com/jatinchowdhury18/AnalogTapeModel (GPLv3)
- Paper: ["Real-time Physical Modelling for Analog Tape Machines", DAFx-19](https://www.dafx.de/paper-archive/2019/DAFx2019_paper_3.pdf) — fair game

What it models and the key techniques (verified from the paper text):
- **Hysteresis**: Jiles–Atherton magnetisation ODE `M = F_loop(H)` solved per-sample with RK4 / Newton–Raphson (plugin later added an STN neural solver). Langevin function `L(x) = coth(x) − 1/x`, with the branch `L(x) ≈ x/3` for `|x| < 1e-4` to kill rounding blowups, and `coth` approximated via a Gaussian continued fraction of `tanh` for speed.
- **Bias**: the record signal is `I + B·sin(2π f_bias t)` — bias gain `B` is a user parameter. Lowering bias produces the **"deadzone" crossover distortion** exactly as in under-biased tape, and the hysteresis acting on the bias alone produces realistic tape hiss on silence. *This is precisely the Strymon "record level vs. bias drive" axis*: drive-into-hysteresis vs. bias amount are two independent, physically-meaningful distortion controls.
- **Playback loss filter**: `V(ω) = V0(ω) · e^{−kd} · (1−e^{−kδ})/(kδ) · sinc(kg/2)` with `k = ω/v` — spacing loss (tape-to-head distance d), thickness loss (δ), gap loss (head gap g), all **tape-speed dependent**. Implemented as an FIR (order ~100) from the inverse DFT of that response; head bump emerges/changes correctly with speed. Their reference numbers: spacing 20 µm, gap 5 µm, thickness 35 µm at 15 ips.
- **Wow/flutter**: measured — they recorded a 1000-pulse train through a Sony TC-260 at 7.5 and 3.75 ips, extracted the timing error, and fit a **periodic function** that drives a modulated delay line. (I.e., not synthetic LFOs; a measured quasi-periodic profile per speed.)
- Plugin-only extras (GPL, ideas only): **Chew** (dropout segments with variance control) and **Degrade** (wear/aging) — direct conceptual matches for Strymon's **crinkle** and **tape age**.

**Improves**: **dTape** — (a) the loss-filter equation set gives you a *parametric* "tape age"/head-alignment filter: age = increase spacing d and reduce azimuth alignment; (b) bias-vs-level as two axes of the hysteresis input is the physically correct way to get the record-level/bias-drive interaction; (c) crinkle = stochastic dropout segments (short gain+HF-loss dips with variance), not just noise.

### 1.2 chowdsp_utils BBD module — **IDEAS (GPLv3)**; BBDDelay repo — **PORT (BSD-3)**
- `chowdsp::BBD` (`BBDDelayLine`, `BBDFilterBank`) lives in `chowdsp_dsp_utils`, which is **GPLv3** ([chowdsp_utils README](https://github.com/Chowdhury-DSP/chowdsp_utils) — per-module licensing; only `chowdsp_buffers`/`chowdsp_math` etc. are BSD). API docs: [chowdsp::BBD namespace](https://ccrma.stanford.edu/~jatin/chowdsp/chowdsp_utils/namespacechowdsp_1_1BBD.html).
- **However**, his standalone experiment repo [jatinchowdhury18/BBDDelay](https://github.com/jatinchowdhury18/BBDDelay) is **BSD-3-Clause** and implements the same Holters/Parker-derived filter-bank BBD. That is a legitimate permissive port source for the multirate filter-bank structure (verify the LICENSE file at port time and keep provenance notes).
- His article [Faster Non-Integer Sample Rate Conversion](https://jatinchowdhury18.medium.com/faster-non-integer-sample-rate-conversion-8034c87d7fa4) explains the trick in plain language: the BBD's input/output anti-alias filters are implemented as **multi-rate filters** — audio rate on one side, BBD clock rate on the other — so the SRC comes for free.

**Improves**: **dBucket** — see §2.1 for the algorithm itself.

ChowKick (GPLv3) is a drum synth — not relevant; skipped.

---

## 2. Julian D. Parker's academic work

### 2.1 Holters & Parker, "A Combined Model for a Bucket Brigade Device and its Input and Output Filters", DAFx-18 — **PAPER** (read in full)
- PDF: https://www.hsu-hh.de/ant/wp-content/uploads/sites/699/2018/09/Holters-Parker-2018-A-Combined-Model-for-a-Bucket-Brigade-Device-and-its-Input-and-Output-Filters.pdf

This is *the* reference algorithm for a variable-clock BBD, and it's directly actionable:
- BBD = **fixed-length queue of N samples running at the (varying) BBD clock rate**, `delay = N/(2·f_clk)`. No interpolation anywhere.
- The circuit's input anti-aliasing filter `H_in(s)` is decomposed into partial fractions (first-order complex sections, conjugate pairs combined into real biquad-like sections). Each section runs its recursive update **at audio rate** (`x_m ← p̄_m·x_m + u(k)`, `p̄_m = e^{p_m T_s}`), and whenever the BBD clock ticks (input edge, fractional position `d_n` within the audio sample), the BBD input sample is read as `Σ_m g_m(d_n)·x_m` with `g_m(d_n) = T_s·r_m·p̄_m^{d_n}` — i.e. the filter state is *evaluated at the exact clock instant* via a modified impulse-invariant transform.
- The output filter runs mirrored: each BBD output tick accumulates `g_out,m(d_n)·Δ(n)` (Δ = difference between consecutive BBD outputs, modeling the zero-order-hold) into the output filter states, which are decayed and summed at audio rate, plus a direct `H0·y_BBD` term.
- Coefficients need one `exp` + two `cos` per section per tick — or **polynomial approx / LUT over d∈[0,1)**, which is what you'd do in Rust.
- Reproduces the *real* aliasing behavior: input reflections around the BBD Nyquist plus sample-and-hold images leaking through the reconstruction filter — this is exactly the Strymon dBucket "audible aliasing at long delay times" behavior, emerging for free from physics rather than being faked.
- Pitch behavior: on a clock-rate step, output pitch changes smoothly because pitch depends on the ratio of clock rate at write vs. read time — the essential BBD-vs-digital-delay difference.
- Paper gives the **Juno-60 chorus 5th-order input/output filter poles/residues numerically** (Table 1) plus measured +2.3 dB BBD insertion gain — a ready-made filter set to sanity-check the structure before fitting Memory-Man-style echo filters.
- Costs scale with BBD clock rate, so long delays (slow clocks) are *cheap*; oversampling the audio side is noted as cheap too since BBD-rate work is independent of fs.

**Improves**: **dBucket** — this should be the core loop if it isn't already. Combined with Raffel/Smith's compander + nonlinearity (§6.2) you have the complete machine.

### 2.2 Zavalishin & Parker, "Efficient Emulation of Tape-like Delay Modulation Behavior", DAFx-18 — **PAPER** (read in full)
- PDF: https://www.dafx.de/paper-archive/2018/papers/DAFx2018_paper_9.pdf; supplementary: https://github.com/julian-parker/DAFX-Tape

- Formalizes **length-type** (Echoplex: move the head → pitch from rate-of-change of length, erratic under feedback) vs. **speed-type** (Space Echo: change motor speed → pitch = ratio of speed at write vs. read, musically clean repitching under feedback). Strymon's dTape repitch behavior is speed-type.
- The **tape equation**: `∫_{t−T(t)}^{t} v(τ)dτ = L`, best solved in difference form `V(t) − V(t−T(t)) = L` where `V` is cumulative distance — the differential form drifts, and the moving-sum form accumulates float error (they explicitly warn; use the difference form or fixed-point).
- Implementation: keep an ordinary ring buffer + a stored monotonic `V[n]` history; each output sample, find the write-time whose `V` equals `V(now) − L` (search over the stored history — O(1) steady-state, O(log K) on speedups via binary search) and read the ring buffer there. Speedups need antialiasing (they present an O(K) scheme reading multiple samples).
- Key detail for feedback: split read-then-write processing within the sample tick to avoid the implicit unit delay, otherwise the exact-repetition property of tape feedback under modulation breaks.
- Analytic result worth encoding as a test: after an instantaneous speed jump, `T(t)` ramps **linearly** from `L/v0` to `L/v1` over a duration of exactly `L/v1` (the new steady-state delay time), so a pitch-shifted echo occupies exactly one new echo period.

**Improves**: **dTape** (and **Drum**): if your tape machine currently modulates a fractional delay length directly, this is the upgrade that makes delay-time changes and wow/flutter *repitch like tape* instead of like a chorus. For Drum, the same equation applies with per-head `L_i`.

### 2.3 Spring reverb work — **PAPER**
- [Parker & Bilbao, "Spring Reverberation: A Physical Perspective", DAFx-09](https://www.dafx.de/paper-archive/2009/papers/paper_84.pdf) and Parker's "Efficient Dispersion Generation Structures for Spring Reverb Emulation" (EURASIP): spring responses = repeated dispersive chirps; emulated efficiently with **cascades of stretched/dispersive allpass filters inside a delay loop**.

**Improves**: **Oil Can** — the Tel-Ray pickup arm is literally spring-suspended in oil. A short cascade of dispersive allpasses (low count, e.g. 4–8 stretched APs) inside the feedback path gives the "rubbery"/dispersive smear that separates oil-can from plain dark BBD repeats; a damped 2nd-order resonator (the arm) excited by the rotation LFO + noise makes a physically-plausible "spring-loaded wobble" modulator rather than a plain sine.

### 2.4 Greyhole / JPverb — **PORT (MIT)**
- Both now MIT-licensed in the Faust libraries ([reverbs.lib](https://github.com/grame-cncm/faustlibraries/blob/master/reverbs.lib); relicensing confirmed on the [HISE forum by Stéphane Letz](https://forum.hise.audio/user/sletz/posts)).
- **Greyhole** = a diffuser (mini-reverb allpass lattice) inside a feedback loop with a long *modulated* delay line — an "echo that melts".

**Improves**: **Oil Can** — Greyhole's topology (diffuser-in-feedback-loop with modulated delay) is exactly the structural recipe for oil-can's reverb-echo hybrid character; scale the diffusion way down and cap bandwidth hard. Portable code.

---

## 3. Airwindows (Chris Johnson) — **PORT (MIT)**
All plugins are MIT ([github.com/airwindows/airwindows](https://github.com/airwindows/airwindows); the plugin pages state "free and open source under the MIT license"). Delay/tape-relevant set and their techniques:

- **[TapeDelay2](https://www.airwindows.com/tapedelay2/)** — the gem for dTape. (a) *True speed-type delay*: "takes a full-length tape loop and literally speeds it up" — delay-time changes and warble repitch like a physical machine with fixed head spacing (poor-man's Zavalishin/Parker, already in MIT C++). (b) **Undersampled delay line + regeneration path** with Airwindows bandpasses applied in the undersampled domain → identical tone at 44.1k/96k/192k and big CPU savings at high rates. (c) **Amplitude-keyed flutter**: flutter depth keys off the underlying signal amplitude → irregular, "wavery" modulation instead of metronomic LFO wobble. (d) Two bandpasses: one global, one regen-only (repeats darken progressively). 
- **[TapeDelay](https://www.airwindows.com/tapedelay/)** (original) — Iron-Oxide-style tone shaping inside a delay; superseded but readable.
- **[Iron Oxide series](https://www.airwindows.com/iron-oxide-5/)** ([Classic 2](https://www.airwindows.com/iron-oxide-classic-2/), [3](https://www.airwindows.com/iron-oxide-3/), [4](https://www.airwindows.com/iron-oxide-4/)) — tape saturation built from **averaging taps over a time window in a delay buffer** (bandpassy, ips-controlled low/high cutoffs), undersampled for rate-independence; v3+ adds flutter as a 1–2 sample "fuzzy smear" of the read position. Cheap grit that sounds like tape rather than waveshaping.
- **[ToTape8](https://www.airwindows.com/totape8/)** (and [ToTape7](https://www.airwindows.com/totape7/)) — **Dubly encode/decode** (Dolby-ish single-band compander: brightened, compressed boost pre-tape; complementary decode after) — harmonics generated by the tape stage get *removed again* by the decode ("treated as noise"), which is why tape reads as a "clean level compressor". Plus **head bump that prefers a nonzero DC offset** (asymmetry!), bias control, and "3D tape" flutter (lateral bend + stretch). All zero-latency, no brickwall oversampling.
- Also potentially useful: **TapeDust** (slew-based HF grit noise), **TapeFat** (tap-averaging tone).

**Improves**: **dTape** directly (flutter keying, undersampling strategy, Dubly-style compander as "tape age ≤ mid" tonality); **Drum** (Iron Oxide-style window-averaging saturation is a great cheap "soft-clip grit" for the Echorec tube stage). Because it's MIT, these can be ported line-by-line into Rust as reference implementations and then re-idiomized.

---

## 4. Faust standard libraries — **mostly PORT-friendly, check per function**
- Overall: LGPL v2.1+ **with the Faust exception** (compiled output freely licensable), but **licenses are per-function** — check each function's `license:` metadata ([misceffects.lib](https://github.com/grame-cncm/faustlibraries/blob/master/misceffects.lib), [faustlibraries docs](https://faustlibraries.grame.fr/libs/misceffects/)).
- Delay/tape-relevant, with per-function licenses (verified from the lib source):
  - `ef.echo` — STK-4.3 (MIT-like): trivial feedback echo.
  - `ef.reverseEchoN` / `ef.reverseDelayRamped` — STK-4.3: ramped reverse-delay with click suppression (nice for a "reverse" delay mode someday).
  - `ef.tapeStop` — "MIT-style STK-4.3" (David Braun): **read-velocity animated from 1→0 with a power curve**, dual delay circuit crossfade — directly reusable for dTape transport-stop/start behaviors (Strymon-style tape-stop on bypass).
  - `re.greyhole` / `re.jpverb` — MIT (see §2.4).
- Faust's physical-modeling (`pm.lib`) has nothing delay-machine-specific worth taking.

**Improves**: **dTape** transport gestures (tape stop curve), **Oil Can** (greyhole). Faust code is compact enough to transcribe by hand.

---

## 5. Open-source Echorec / Space Echo emulations
Findings: **no true open-source Echorec exists**; Space Echo has two student/hobby JUCE projects.

- **[je3928/RE201models](https://github.com/je3928/RE201models)** — **IDEAS (GPL-3.0)**. BSc thesis project, full RE-201: tone stack two ways (WDF shelving + virtual-analog), tape saturation two ways (Chowdhury-2019 hysteresis + sigmoid), wow/flutter, **tape-speed-dependent EQ**, **control ballistics** (motor inertia on the repeat-rate knob!), spring reverb (FFT convolution + waveguide). Best value: its *architecture checklist* — control ballistics (slewed motor speed with inertia/lag) is something our dTape/Drum should have for knob-twist repitch feel.
- **[cyrusasfa/TapeDelay](https://github.com/cyrusasfa/TapeDelay)** — RE-201/Echoplex JUCE, WIP, crashes, **no visible license → treat as all-rights-reserved; do not port**.
- **GuitarML** ([Proteus](https://github.com/GuitarML/Proteus) etc.) — LSTM captures of amps/pedals; no tape echo captures in the tone library, and LSTM capture can't represent time-variant delay anyway. At most: capture a real Space Echo *preamp/saturation stage* as a static nonlinearity via their training pipeline (RTNeural is BSD-3; models you train yourself are yours). Marginal.
- Commercial references for behavior only (no code): [Pulsar Echorec](https://pulsar.audio/echorec/), [Audiority Echoes T7E](https://www.audiority.com/shop/echoes-t7e/), [Anatomy of Tone Echorec writeup](https://www.anatomyoftone.com/home/the-echorec-delay-sound) — confirm: 1 record + 4 replay heads on a **spinning magnetic drum** (wire on early units), max ~300 ms at head 4, head spacings 75/150/225/300 ms, drum transport is far more stable than tape (less wow; character comes from head switching, tube stages, and the "swell" recirculation matrix).

**Improves**: **Drum** — head layout/behavioral ground truth + the ballistics idea; all algorithmic substance for Drum comes from the papers (§6) instead.

---

## 6. DAFx / AES papers (all PAPER = fair game)

### 6.1 Arnardóttir, Abel & Smith, "A Digital Model of the Echoplex Tape Delay" (AES 125, 2008)
- https://secure.aes.org/forum/pubs/conventions/?elib=14800
- Circular buffer with **read, write, and erase pointers**; movable record head (length-type). Two things to steal:
  1. **Flutter decomposition**: measured delay-time fluctuation = **quasiperiodic capstan component + pinch-wheel component + low-frequency drift** (random walk). Synthesizing wow/flutter as (a) capstan-rate sinusoid + harmonics, (b) pinch-wheel-rate component at a different (incommensurate) rate, (c) integrated noise drift is the standard recipe — matches Chowdhury's measured-periodic-function approach and gives dTape a physically-layered wow generator (Strymon's wow/flutter knob = crossfading these layers).
  2. **Variable-cutoff anti-aliasing filter tracking the moving record head** — needed whenever write-side speed varies; same requirement appears in Z&P speedup antialiasing.

### 6.2 Raffel & Smith, "Practical Modeling of Bucket-Brigade Device Circuits" (DAFx-10) — read in full
- https://www.dafx.de/paper-archive/2010/DAFx10/RaffelSmith_DAFx10_P42.pdf
- The **complete parasitic-effects kit** dBucket needs on top of Holters/Parker:
  - **Filters**: typical echo circuit = 3rd-order Sallen-Key anti-alias + (3rd-order + 2nd-order "corner correction") reconstruction, cutoffs ⅓–½ of clock rate, as low as **1.5 kHz** in dark echo units. Typical values: R=10 kΩ everywhere; AA caps 6.8n/82n/330p; recon 39n/330p and 2.2n/33n/1n. Fit the series 8th-order response with an IIR (equation-error / invfreqz-style).
  - **Compander (NE570/571)**: ratio 2; averager = full-wave rectifier → one-pole RC with `τ = 10kΩ·C_rect`, C_rect 0.22–1 µF (τ ≈ 2.2–10 ms). Expander (feedforward): `f(x) = avg(|x|)·x`; compressor (feedback): `f(x) = x / avg(|f(x)|)`. One-pole averager: `y[n] = x[n]·T/(RC+T) + y[n−1]·RC/(RC+T)`. **The compander's attack/release mismatch against the delayed envelope is the source of BBD "bucket loss" breathing on repeats** — put compress before the delay, expand after, and the envelope error compounds per feedback pass.
  - **Nonlinearity**: THD ≈ `1.01^{N/1024} − 1` (~1% per 1024 stages), *nearly level-independent* (not clipping-like). Modeled with `f(x) = x − a·x² − b·x³ + a` on (−1,1) (clamped smoothly outside), with `a = 1/8, b = 1/18` fitting a 4096-stage unit. Level-independence is the give-away BBD flavor — don't use tanh here.
  - **Insertion gain**: 0–2 dB LF, drooping to −4..−6 dB at BBD Nyquist → clock-tracking one-pole/shelf.
  - **Noise**: ~60 dB down, injected at the delay line → enables the classic **self-oscillation** when feedback >1. (Add noise inside the loop, not at the output.)

### 6.3 Holters & Parker (§2.1), Zavalishin & Parker (§2.2) — covered above.

### 6.4 Tape saturation / wow-flutter
- Chowdhury DAFx-19 (§1.1).
- ["Neural modeling of magnetic tape recorders" (arXiv 2305.16862, Aalto)](https://arxiv.org/pdf/2305.16862) — beyond the GRU saturation model, its **measurement method for wow/flutter trajectories** (pulse trains → extracted time-varying delay, decomposed into periodic + stochastic parts) is a practical calibration recipe if you ever profile a real RE-201/Echorec.
- Välimäki et al., "Digital Audio Antiquing" (JAES 2008) — colored-noise + dropout + wow recipes for aged-media simulation (cited by Raffel; good for tape-age/crinkle parameter mapping).

### 6.5 Oil can / Tel-Ray
- **No academic paper exists** on electrostatic oil-can delays (searched multiple phrasings; nothing at DAFx/AES). Best available sources:
  - [Strymon's own oil-can description](https://www.strymon.net/this-weeks-preset-timeline-oil-can-delay/) (behavioral).
  - [Scientific Guitarist "Don't Tell Ray"](https://scientificguitarist.wixsite.com/home/don-t-tell-ray) — DIY analysis: **wobble is sinusoidal** (disc rotation), rotation periods ≈ **300 ms (guitar units) / 150 ms (organ units)**; input **soft-clip diodes**; heavy HF rolloff; "aged" = darker still.
  - [Effectrode's History of Delay](https://www.effectrode.com/knowledge-base/history-of-delay/) + [Equipboard oil-can guide](https://equipboard.com/posts/oil-can-delays): rotating disc, oil as dielectric storing charge **electrostatically**, pickup sloshing through oil, sound = "blend of reverb and warbling vibrato".
  - Physical model of the ghost: **there is no erase head** — the write head only *partially* re-charges the disc each revolution, so at the write point: `disc[w] = α·input + (1−α)·disc[w]` with α < 1. The un-erased residue re-emerges every rotation period → a second echo tap at exactly `T_rot` (fixed, independent of the read-head delay), decaying by `(1−α)` per revolution. This one-line recurrence *is* the no-erase ghost, and it self-consistently interacts with feedback.
- Adjacent but useful: [Simionato/Liski/Välimäki/Avanzini, "A Virtual Tube Delay Effect" (DAFx-18)](https://dafx.de/paper-archive/2018/papers/DAFx2018_paper_21.pdf) — loss modeling of an acoustic delay medium with a parametric cascade (two high-shelves + lowpass) continuously tunable by physical parameters; a good template for a physically-parameterized "murk" filter (oil viscosity/age → shelf/LP params).

### 6.6 Multi-head feedback topologies
- No dedicated paper found; the Echoplex paper (§6.1) + Echorec behavioral docs (§5) cover it. Design note: Echorec's "swell" modes recirculate *combinations* of heads (head-select matrix feeding the record amp), so feedback should be a 4×1 gain matrix into the single record head, not per-head feedback loops.

---

## 7. Linux plugin suites (checked; little to take)
- **[zam-plugins / ZamDelay](https://github.com/zamaudio/zam-plugins)** (mixed GPL2+/LGPL/ISC): plain feedback delay + filter — nothing analog-modeled.
- **Calf Vintage Delay** (LGPL): filter-in-feedback-loop "tape sim" with BPM sync — simple, nothing we don't have.
- **LSP Plugins** (LGPL-3): pristine digital delays/comp — deliberately clean, not vintage-modeled.
- **x42**: no analog-modeled delay. 
- Conclusion: skip; Airwindows + the papers dominate this space.

---

## Per-machine action summary

| Machine | Highest-value upgrades | Source & status |
|---|---|---|
| **dTape** | Speed-type tape equation (repitch = speed ratio; difference-form V(t) solver, split read/write in feedback); amplitude-keyed flutter; layered wow = capstan periodic + pinch-wheel + drift random-walk; bias-vs-level via J-A hysteresis deadzone; speed-dependent loss FIR (spacing/gap/thickness) as the tape-age/head filter; Dubly-style compander for the "clean compressor" character; tape-stop power curve for transport gestures; crinkle = dropout segments with variance | Z&P paper; Airwindows TapeDelay2/ToTape8 (MIT, portable); Echoplex + Chowdhury papers; Faust `ef.tapeStop` (MIT-style) |
| **dBucket** | Holters/Parker multirate filter-bank BBD (exact clock-instant filter evaluation, no interpolation; real aliasing emerges at long times); NE570 compander with τ=10k·C averagers (breathing bucket loss); level-independent cubic THD `1.01^{N/1024}−1` (a=1/8, b=1/18); clock-tracking insertion-gain droop; in-loop noise → self-oscillation | Holters/Parker + Raffel/Smith papers; BBDDelay repo BSD-3 (portable reference impl) |
| **Drum** | 4-head gain-matrix-into-record-head feedback ("swell" recirculation); drum = *low* wow, character from head-switching + tube grit (Iron Oxide window-averaging saturation, MIT); head-alignment filter = gap/azimuth terms of the loss-filter equations; motor **control ballistics** (inertia-slewed speed) for knob feel | Echorec behavioral docs; Chowdhury loss equations (paper); Airwindows Iron Oxide (MIT); RE201models architecture (GPL, ideas) |
| **Oil Can** | No-erase ghost as partial-overwrite recurrence `disc[w]=α·in+(1−α)·disc[w]` → fixed tap at rotation period (300/150 ms); sinusoidal once-per-rev wobble + spring-arm 2nd-order resonator (dispersive-allpass smear from Parker's spring work); murky physically-parameterized bandpass (tube-delay-style shelving cascade); Greyhole diffuser-in-feedback topology for the reverb-echo blend | Own derivation from Tel-Ray physics (sources above); Parker spring papers; Greyhole MIT (portable) |

**License bottom line**: Portable code = **Airwindows (MIT)**, **BBDDelay (BSD-3)**, **Greyhole/JPverb + tapeStop/echo functions in faustlibraries (MIT/STK)**. Ideas-only = ChowTapeModel, chowdsp_dsp_utils BBD, RE201models (all GPL-3), cyrusasfa/TapeDelay (unlicensed). All DAFx/AES papers above are fair game and contain everything needed with implementable detail (I extracted full text of Holters–Parker, Raffel–Smith, Zavalishin–Parker, and Chowdhury's tape paper during this research).

Sources: [ChowTapeModel repo](https://github.com/jatinchowdhury18/AnalogTapeModel) · [Chowdhury DAFx-19 PDF](https://www.dafx.de/paper-archive/2019/DAFx2019_paper_3.pdf) · [chowdsp_utils](https://github.com/Chowdhury-DSP/chowdsp_utils) · [chowdsp::BBD docs](https://ccrma.stanford.edu/~jatin/chowdsp/chowdsp_utils/namespacechowdsp_1_1BBD.html) · [BBDDelay (BSD-3)](https://github.com/jatinchowdhury18/BBDDelay) · [SRC article](https://jatinchowdhury18.medium.com/faster-non-integer-sample-rate-conversion-8034c87d7fa4) · [Holters–Parker DAFx-18 PDF](https://www.hsu-hh.de/ant/wp-content/uploads/sites/699/2018/09/Holters-Parker-2018-A-Combined-Model-for-a-Bucket-Brigade-Device-and-its-Input-and-Output-Filters.pdf) · [Zavalishin–Parker DAFx-18 PDF](https://www.dafx.de/paper-archive/2018/papers/DAFx2018_paper_9.pdf) · [DAFX-Tape supplementary](https://github.com/julian-parker/DAFX-Tape) · [Raffel–Smith DAFx-10 PDF](https://www.dafx.de/paper-archive/2010/DAFx10/RaffelSmith_DAFx10_P42.pdf) · [Parker–Bilbao spring DAFx-09](https://www.dafx.de/paper-archive/2009/papers/paper_84.pdf) · [faustlibraries reverbs.lib](https://github.com/grame-cncm/faustlibraries/blob/master/reverbs.lib) · [misceffects.lib](https://github.com/grame-cncm/faustlibraries/blob/master/misceffects.lib) · [Airwindows TapeDelay2](https://www.airwindows.com/tapedelay2/) · [Iron Oxide 5](https://www.airwindows.com/iron-oxide-5/) · [ToTape8](https://www.airwindows.com/totape8/) · [airwindows GitHub](https://github.com/airwindows/airwindows) · [Echoplex AES paper](https://secure.aes.org/forum/pubs/conventions/?elib=14800) · [RE201models](https://github.com/je3928/RE201models) · [cyrusasfa/TapeDelay](https://github.com/cyrusasfa/TapeDelay) · [GuitarML Proteus](https://github.com/GuitarML/Proteus) · [neural tape arXiv](https://arxiv.org/pdf/2305.16862) · [Virtual Tube Delay DAFx-18](https://dafx.de/paper-archive/2018/papers/DAFx2018_paper_21.pdf) · [Don't Tell Ray](https://scientificguitarist.wixsite.com/home/don-t-tell-ray) · [Effectrode History of Delay](https://www.effectrode.com/knowledge-base/history-of-delay/) · [Strymon oil-can preset](https://www.strymon.net/this-weeks-preset-timeline-oil-can-delay/) · [Anatomy of Tone Echorec](https://www.anatomyoftone.com/home/the-echorec-delay-sound) · [Pulsar Echorec](https://pulsar.audio/echorec/) · [zam-plugins](https://github.com/zamaudio/zam-plugins) · [Calf](https://en.wikipedia.org/wiki/Calf_Studio_Gear)

---

# Part 2 — Granular / spectral / pitch / digital delays

# Open-Source Delay/Granular/Spectral DSP — Research Report

Research for `features/fx/delay/delay-dsp` (TimeLine MX parity: Spectral, Ice, Reverse, Digital, MultiTap, Filter + Flint-style Reverb). License legend: **PORTABLE** = permissive code you can port/translate; **IDEAS ONLY** = GPL/proprietary, study concepts, clean-room only; **FAIR GAME** = papers/blogs, no code license issue.

---

## 1. Mutable Instruments — Clouds (verified in source) and Beads

**Repo**: [pichenettes/eurorack](https://github.com/pichenettes/eurorack), `clouds/` directory. **License: MIT** — [confirmed](https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/open_source/). **PORTABLE** (best single source in this whole report for your Spectral machine).

### Grain engine (`clouds/dsp/grain.h`, `granular_processor.*`) — verified specifics

- **Window**: one continuously-morphable envelope parameter, not a table of shapes. Envelope phase runs 0→2.0 with triangle peak at 1.0 (`gain = phase >= 1 ? 2 - phase : phase`). Then:
  - `window_shape < 0.5` → sharpen toward **boxcar** via `envelope_slope_ = 0.5 / (window_shape + 0.01)` (linear slope steepening, clipped — trapezoid→rectangle);
  - `window_shape >= 0.5` → smooth toward **Hann** by crossfading the triangle value through a 4096-entry raised-cosine LUT (`stmlib::Interpolate(lut_window, gain, 4096.0f)`), with `envelope_smoothness_ = (window_shape - 0.5) * 2`.
  - This boxcar↔triangle↔Hann morph is exactly the "grain shape" control TimeLine's Spectral machine wants; it's one multiply + one LUT lookup per sample.
- **Pitch per grain**: 16.16 fixed-point phase accumulator; top 16 bits = sample index, low 16 = fractional interpolation. Beads later found a **21:10 fixed-point format** to be a big win over 47:16 (avoids int64↔float conversion) — relevant if you're doing fixed-point anywhere.
- **Block-accurate onset**: grains scheduled inside a 32-sample block use a `pre_delay_` counter that skips output samples before rendering — grains can start on any sample without per-sample scheduling overhead. Beads went further: **fractional (inter-sample) grain start indices**.
- **Interpolation quality is a template parameter** of the buffer read (LOW/MEDIUM/HIGH → different interpolators) — a clean pattern for your quality tiers.
- **Buffer formats / quality modes**: 16-bit linear stereo ~1s, 16-bit mono ~2s, **8-bit µ-law** (`mu_law.cc`) stereo/mono 4–8s at 16 kHz (`kDownsamplingFactor = 2` off a 32 kHz base). µ-law + downsampling is a *deliberate lo-fi character mode* — a cheap, great-sounding "vintage" voice for your Digital machine alongside ADM/12-bit.
- **Density/overlap**: grain seeding rate derived from DENSITY×SIZE with deterministic vs randomized spacing on either side of the center detent (constant-rate at one extreme, Poisson-ish at the other) — matches your "density sync" ambitions; MI's trick is that the *same knob* crossfades periodic↔stochastic scheduling.

### The post-granular diffuser (`clouds/dsp/fx/diffuser.h`) — verified

4 series allpasses per channel, **delays L = {126, 180, 269, 444}, R = {151, 205, 245, 405} samples (at 32 kHz), coefficient kap = 0.625**, wet mixed as `out += amount * (wet - in)`. Putting a small 4-allpass diffuser *after* the granular cloud (and optionally *inside* the feedback path) is a large part of why Clouds sounds "expensive" — directly applicable to Spectral and Ice repeats smearing.

### Clouds' hidden Spectral mode (`clouds/dsp/pvoc/*`) — verified, this is a real FFT granular-over-STFT reference

`stft.*` + `phase_vocoder.*` + `frame_transformation.cc`. Frame transform does, per STFT frame (polar domain):
- **Phase randomization**: `synthesis_phase[i] += Random * amount >> 14`, amount squared for control taper (this is PaulStretch-style smearing, dialable).
- **Spectral quantization/warping** on a single "texture" knob: below 0.48 → magnitude quantization to N levels (robotization/degradation); above 0.52 → polynomial magnitude warp `4x(1-x)^3` (dynamic compression of the spectrum); middle = clean.
- **Spectral warping** of bin positions via cubic polynomial LUT (`kWarpPolynomials`) — reads source bins at warped positions with interpolation = spectral "pitch bend" without resampling.
- **Glitch modes**: frozen magnitudes that self-grow (`held *= 1.01`), spectrum up-shift with wraparound aliasing, loudest-bin suppression + 8× second-bin boost, random bin attenuation.
- Pitch ratio applied by **scaling per-bin phase deltas** (classic PV) + `kHighFrequencyTruncation` to save cycles.

**Concrete wins for your machines**: (a) the window morph LUT scheme; (b) pre-delay block scheduling + fractional starts; (c) diffuser constants above; (d) frame_transformation.cc is a ready-made menu of *per-frame spectral ops* for a true-FFT Spectral machine v2; (e) µ-law buffer mode for Digital.

### Beads

**Still closed-source** — Émilie Gillet said (2023) she'll release it only after officially ending support ([MOD WIGGLER thread](https://www.modwiggler.com/forum/viewtopic.php?t=281283)). What's documented in [Beads history](https://pichenettes.github.io/mutable-instruments-documentation/trivia_and_history/beads_history/): full stereo rewrite for STM32H7, fractional grain triggering, non-contiguous buffer chaining, 21:10 delay addressing, 4 quality modes. **IDEAS ONLY** (and few implementation details public). Don't plan around it.

Also note **Kammerl Beat-Repeat** ([kammerl.de/audio/clouds](https://www.kammerl.de/audio/clouds/)) — an MIT Clouds alternate firmware doing sliced beat-repeat with per-slice pitch/speed — very close to your **Ice** "slice + shift" architecture, in MIT code.

### Rings / Elements (future algorithm ideas)

Same repo, MIT. Three resonator models: **modal** (64 zero-delay SVF bandpass bank from Elements), **sympathetic strings** (network of comb filters tuned in intervals), **string with dispersion** (comb + multimode filter + nonlinearity + allpass dispersion in the loop). NEW machine idea: **"Resonant" delay** — replace the feedback filter with a tuned comb network or small modal bank so repeats ring at chord intervals (a "sympathetic delay"); Rings' interval tables (Structure knob → string tuning ratios) are directly liftable. [Rings docs](https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/).

---

## 2. monome norns softcut

**Repo**: [monome/softcut-lib](https://github.com/monome/softcut-lib). **License: GPL-3.0 — IDEAS ONLY.**

Architecture (from docs/discussions): 6 voices over 2 shared buffers; each voice = a `ReadWriteHead` pair with **two subheads** that alternate. The key trick: any discontinuity (loop point, position jump, rate change through zero) is handled by **handing off to the other subhead and equal-power crossfading over a configurable fade time**, with subsample-accurate resampled read/write (interpolated write for overdub, plus a per-voice "preserve" level scaling existing material under the write head). Loop mode = crossfaded loop; one-shot = fade out at loop end.

**Lessons for Reverse machine**: softcut proves the robust primitive is *"never glitch — always dual-head + crossfade any discontinuity, including write-head collisions."* Your reverse windows should (a) run read rate −1 with the same fractional-resampler as forward, (b) crossfade window boundaries with equal-power fades of fixed *time* (5–100 ms) not fixed samples, (c) handle read-head-crosses-write-head by early handoff. The KVR consensus matches: reverse needs 2× buffer (read moves −1 while write moves +1, closing at 2 samples/sample), and the collision point must be crossfaded ([KVR thread](https://www.kvraudio.com/forum/viewtopic.php?t=599376)).

---

## 3. DaisySP and Soundpipe / sndkit

### DaisySP — [electro-smith/DaisySP](https://github.com/electro-smith/DaisySP), **MIT, PORTABLE**

- `Source/Effects/pitchshifter.h` — the classic **dual-head delay-line shifter** (based on the UCSD "Pitch Shifting" doc): two taps 180° apart on a modulated delay, triangular crossfade, optional transposition quantization (`SetFun`). Simple, known-latency; a decent baseline for Ice but *inferior* to Clouds' version (same idea, Clouds adds cubic window-size mapping).
- `Source/Sampling/granularplayer.*` — independent pitch/time granular player (two-phasor design, MIT).
- Also has `DelayLine` (templated, fractional), `chorus`, `reverbsc` (see Soundpipe below).

### Soundpipe / sndkit — Paul Batchelor

- [sndkit](https://paulbatchelor.github.io/sndkit/): literate ANSI C algorithms; **text CC0, tangled code dual MIT/Unlicense — maximally PORTABLE.** Directly relevant algorithms: **`vardelay`** (variable delay w/ third-order interpolation — good reference for your modulated Filter machine delay), **`bigverb`** (the famous Sean Costello `reverbsc` FDN — 8 delay lines with jittered lengths + one-pole damping; **this is a legitimate, permissively-licensed core for your Flint-style reverb bonus machine**), `modalres`, `smoother`, `peakeq`.
- [Soundpipe](https://github.com/paulbatchelor/soundpipe) (MIT) has pre-tangled versions.
- No granular/spectral/pitch modules in sndkit proper — use it for delay/reverb infrastructure.

---

## 4. FFT / spectral delay

### What a true FFT spectral delay looks like

Consensus architecture from [Max/MSP gen~ discussions](https://cycling74.com/forums/spectral-delay-gen), the [JUCE spectral-delay example](https://github.com/Aayushchou/spectral-delay), and the Sydney phase-vocoder design report:

1. STFT with 50–75% overlap (1024–4096 FFT @ 48 kHz; bigger = lusher/smearier).
2. **Per-bin delay line of *frames*, not samples**: for each bin k, a circular buffer of past (mag, phase) values; delay[k] in frames (so time resolution = hop size). Delay mag and phase together.
3. **Per-bin feedback**: `frame[k] = input[k] + fb[k] * delayed[k]` — feedback in the *spectral* domain is the magic: each bin recirculates independently, so decays become frequency-dependent washes. Add per-bin damping (multiply fb by a spectral tilt) for "tonal evolution."
4. Delay-time-vs-frequency *curves* (linear ramp, random-per-bin, quantized stairsteps) are the user-facing parameter — this is what NI Spektral Delay did.
5. On resynthesis: overlap-add with synthesis window, normalize by window overlap sum.
- **Feedback pitfall**: feedback must be applied per-frame inside the FFT process (sample-accurate outer feedback loops can't reach into bins).
- **Tonal evolution ideas** to bolt on per recirculation pass: phase randomization amount (PaulStretch-ify each repeat), magnitude quantization (Clouds), per-bin ±1 bin drift (spectral "wow"), freeze (fb=1, input=0).

### SuperCollider — [FFT Overview](https://doc.sccode.org/Guides/FFT-Overview.html)

**GPL — IDEAS ONLY**, but the PV UGen *catalog* is the best idea menu: `PV_MagFreeze`, `PV_BinScramble` (scramble bins = spectral granular), `PV_Diffuser` (random constant phases), `PV_MagSmear`, `PV_BinShift` (linear bin shift = inharmonic "shift" distinct from pitch shift — great Ice variant), `PV_RandComb`. Each of these is a one-liner per-bin op you can reimplement freely in your spectral machine's feedback path.

### PaulStretch / PaulXStretch

Algorithm: window → FFT → **keep magnitudes, fully randomize phases** → IFFT → overlap-add, with windows advancing slower on input than output. Destroys transients, yields the smooth wash. **Code is GPL-2/GPL-3** ([paulnasca/paulstretch_cpp](https://github.com/paulnasca/paulstretch_cpp), [essej/paulxstretch](https://github.com/essej/paulxstretch)) — **IDEAS ONLY**, but the algorithm is fully described publicly ([explainer](https://polarity.me/posts/articles/2026-07-07-paulxstretch-paulstretch-explained/)) and trivially clean-room (Clouds' MIT phase-randomization is already 90% of it). NEW machine idea: **"Stretch" delay** — each repeat is progressively PaulStretched (frame reads slow down per recirculation) → repeats melt into pads.

### Spectral freeze — Jean-François Charles, *A Tutorial on Spectral Sound Processing Using Max/MSP and Jitter*, CMJ 32(3), 2008 ([MIT Press](https://direct.mit.edu/comj/article/32/3/87/94223/A-Tutorial-on-Spectral-Sound-Processing-Using-Max), [patches](https://www.jeanfrancoischarles.com/2019/11/max-patches-for-spectral-audio-freeze.html))

**FAIR GAME.** Key technique: *stochastic* freeze — instead of looping one frame (metallic "frame effect"), draw each synthesis frame's bins from a small neighborhood of stored frames + per-frame phase randomization. Use this for your Spectral machine's freeze/infinite-hold and for feedback≥1 stability character.

### Spectral Delay Filters — Välimäki, Abel, Smith, JAES 57(7/8) 2009 (+ [Pekonen follow-up with feedback/time-varying coefficients](https://www.semanticscholar.org/paper/2524359675bff02b2fc33e8f8f3da399f87d3595))

**FAIR GAME**, and a sleeper hit: a cascade of **M identical first-order allpasses (M in the hundreds–thousands) + magnitude EQ filter** gives a *continuous* frequency-dependent group delay — chirpy, dispersive "laser" echoes — **no FFT, no latency, sample-rate processing, trivially cheap per stage**. With feedback around the cascade you get self-arpeggiating dispersive echoes. This is a NEW machine candidate ("Chirp"/"Dispersion" delay) that no Strymon box has, and it's realtime/no_std-friendly (fits your processing-core rules — no FFT allocation).

---

## 5. Pitch shifting inside delays

### signalsmith-stretch — [GitHub](https://github.com/Signalsmith-Audio/signalsmith-stretch), **MIT, PORTABLE** (C++; Rust bindings exist: [`signalsmith-stretch`](https://crates.io/crates/signalsmith-stretch) cxx-based and [`ssstretch`](https://lib.rs/crates/ssstretch))

The state of the art in permissive polyphonic shifting. From the [design writeup](https://signalsmith-audio.co.uk/writing/2023/stretch-design/):
- STFT, default ~120 ms block / 4× overlap (configurable; `configure(channels, blockSamples, intervalSamples)`).
- Instead of classic phase-vocoder unwrapping, phases are **predicted from time-frequency neighbors** using conjugate products `X[p2]·conj(X[p1])` — amplitude-weighted so strong partials/transients dominate; two passes (horizontal in time, then vertical in frequency downward) so low bins lock to strong harmonics.
- **Non-linear frequency map** keeps 1:1 scaling near strong peaks (avoids inharmonic smearing), optional **tonality limit** = shift below limit, preserve formants/timbre above — this is *the* fix for the "chipmunk shimmer" problem.
- Vertical phase-scaling clamped at 2×; mild randomization for extreme stretches.
- Watch the talk: [Four Ways To Write A Pitch-Shifter — ADC22](https://www.youtube.com/watch?v=fJUmmcGKZMI) (1: dual-head delay crossfade, 2: granular/overlap-add, 3: phase vocoder, 4: the stretch design).
- **For your machines**: Ice's interval ladder and regen re-shift would jump in quality using stretch as the shifter (it stays clean under repeated re-shifting far longer than dual-head). Latency (~1–2 blocks) is fine inside a delay's feedback loop — it just adds to the delay time; *compensate by shortening the nominal delay*.

### Delay-line (Whammy-style) shifters

Clouds `fx/pitch_shifter.h` (MIT, verified above) is the canonical cheap version: window 128→2047 samples via cubic size mapping, dual taps a half-window apart, triangular crossfade `tri = 2*(phase>=0.5 ? 1-phase : phase)`, head advance `phase += (1-ratio)/size`. DaisySP's is equivalent. Upgrades worth implementing: equal-power (sin/cos) instead of triangular fade; **transient-aware splice** (only jump heads near low-energy or zero-crossing points — the "de-glitch" idea Valhalla mentions autocorrelation shifters used); slight per-head detune randomization to decorrelate the warble.

### smbPitchShift — Stephan Bernsee ([code](http://blogs.zynaptiq.com/bernsee/repo/smbPitchShift.cpp), [article](http://blogs.zynaptiq.com/bernsee/pitch-shifting-using-the-ft/))

Classic STFT bin-shifting phase vocoder, **Wide Open License (permissive, BSD-like) — PORTABLE**. Sounds phasier than signalsmith-stretch; useful mainly as a simple reference/AB baseline.

### Shimmer technique — Valhalla blog (FAIR GAME)

[How it's made](https://valhalladsp.com/2010/05/11/enolanois-shimmer-sound-how-it-is-made/), [history](https://valhalladsp.com/2010/11/23/valhallashimmer-a-bit-of-history/): the canonical topology is **pitch shifter (+12 semi) *inside* a feedback loop with a long diffuse reverb/delay**; the reverb's diffusion masks grain artifacts, and each recirculation re-shifts (octave ladder). Costello deliberately used a *noisy/textured* granular shifter rather than a clean deglitched one — artifacts become the aesthetic. Direct guidance for your Ice regen ladder and a Shimmer mode on the Flint-style reverb: put diffusion *after* the shifter in the loop, low-pass the loop (~4–6 kHz) so the ladder brightens then dies rather than screeching, and keep loop gain < ~0.7 with the shifter's energy gain accounted for.

---

## 6. ADM / CVSD and companding (Digital machine)

Your two voices map to real hardware lineages: 1-bit ADM = DeltaLab Effectron/MXR era; 12-bit companded = PCM-42/Prime Time era.

### CVSD algorithm (verified constants)

From [Wikipedia](https://en.wikipedia.org/wiki/Continuously_variable_slope_delta_modulation), [Motorola AN1544](https://perso.esiee.fr/~francaio/enseignement/I4_Etude_de_K_save/an1544.pdf), [IRIG 106 Appendix F](https://irig106.org/docs/106-15/appendixF.pdf):
- 1 bit/sample: emit sign of (input − integrator); integrator slews by ±step.
- **Slope adaptation**: keep last N output bits (N=3 for ≤16 kHz clocks, N=4 for ≥32 kHz). All-same → step += large increment toward `step_max`; else step decays exponentially toward `step_min` (the "syllabic" filter). **Syllabic time constant 4–10 ms (Bluetooth/mil spec: 5 ms ±1); integrator ("principal") time constant ~1 ms; companding ratio ≤30%.** For a 32 kHz-clock delay voice: syllabic τ ≈ 160 samples, step_max/step_min ratio ~16:1 is a good start; the *character* (slope-overload "fizz" on transients, granular noise on quiet sustain) comes from step_min floor and the N-bit coincidence rule.
- Open implementations: **[embARC audio codecs BT-CVSD](https://github.com/foss-for-synopsys-dwc-arc-processors/embarc_audio) — BSD-style, PORTABLE** (the one to crib bit-exact Bluetooth constants from); [GNU Radio gr-vocoder CVSD](https://github.com/gnuradio/gnuradio/blob/master/gr-vocoder/include/gnuradio/vocoder/cvsd_encode_sb.h) — **GPL, IDEAS ONLY**; [liquidsdr CVSD doc](https://liquidsdr.org/doc/cvsd/) (liquid-dsp is MIT) — **PORTABLE**.
- For the delay voice: encode → store bitstream → decode at read tap; run feedback through the codec again each pass so repeats progressively gain ADM grit (that's the authentic Effectron regen behavior).

### NE570/571 compander + 12-bit era — *Practical Modeling of Bucket-Brigade Device Circuits*, Raffel & Smith, DAFx-10 ([PDF](https://www.dafx.de/paper-archive/2010/DAFx10/RaffelSmith_DAFx10_P42.pdf)) — **FAIR GAME**, read in full

Verified actionable model:
- 570/571 = fixed **2:1 ratio**; gain from average level via **full-wave rectifier → one-pole RC**, decay τ = 10 kΩ × C_rect (typ. a few ms–tens of ms in delays).
- **Feedforward expander: `f(x) = avg(|x|) · x`; feedback compressor: `f(x) = x / avg(|x|)`** (feedback topology matters — the compressor measures its own *output*). Model avg(|x|) as `y[n] = y[n-1] + (|x[n]| − y[n-1])/(τ·fs)`.
- The 12-bit character = compressor → (12-bit quantize + delay + noise) → expander: the expander **modulates the noise floor with the signal envelope** (breathing/pumping on decays) — that envelope-modulated noise is the sound, not the bit depth per se. Inject a small fixed noise+quantization error inside the companded region.
- Also from the paper: anti-alias/reconstruction = 3rd-order Sallen-Key in, 3rd-order + 2nd-order corner-correction out, cutoffs below Nyquist of the *lowest* clock rate — for your Digital voice, fixed ~6–8 kHz filters regardless of delay time reproduce the era's darkening.

---

## 7. Valhalla blog, Signalsmith library, Bela

- **Valhalla / Sean Costello** (FAIR GAME, no code): [Reverbs: diffusion, allpass delays, metallic artifacts](https://valhalladsp.com/2011/01/21/reverbs-diffusion-allpass-delays-and-metallic-artifacts/) — series allpasses ring metallic on impulses; mutually-prime, non-harmonically-related delays + modulation fix it; diffusion control = allpass coefficient g (0→~0.7). [ValhallaPlate on diffusion](https://valhalladsp.com/2015/11/18/valhallaplate-diffusion-and-presets/), [ValhallaDelay diffusion section](https://valhalladsp.com/2019/06/13/valhalladelay-the-diffusion-section/) (site blocks fetch bots; readable in a browser): cascaded short allpasses on the delay *taps and feedback path*, diffusion amount + optional modulated allpasses turn discrete repeats into smeared "tape-ish" repeats. Concrete for MultiTap/Filter machines: 2–4 allpasses of 5–40 ms, coefficients ~0.5–0.7, ~0.5 Hz LFO on the allpass delays. His reverbsc/bigverb core is already permissively available via sndkit (see §3).
- **Signalsmith DSP library** — [Signalsmith-Audio/dsp](https://github.com/Signalsmith-Audio/dsp), **MIT, PORTABLE**: `delay.h` has production-quality fractional-delay interpolators (LagrangeN per Franck 2009, and windowed-sinc/Kaiser polyphase with measured aliasing performance) — a direct upgrade path for every modulated read in your suite (Filter machine sweeps, Reverse variable-rate reads, MultiTap tap movement). Also [Signalsmith-Audio/basics](https://github.com/Signalsmith-Audio/basics) (MIT effect classes) and the blog's interactive articles (e.g., crossfade curves, stretch design) — FAIR GAME + MIT demos.
- **Bela** — [examples/tutorials](https://github.com/BelaPlatform/Bela/wiki/Example-projects-and-tutorials); core is **LGPL-3**, C++ examples are teaching-grade (delay, [pitch-aware granular](https://github.com/maxgraf96/pitch-aware-granular-synth/)). Nothing here beats MI/Signalsmith; treat as tutorial material only.

---

## 8. Rust crates

| Crate | License | Relevance |
|---|---|---|
| [fundsp](https://github.com/SamiPerttu/fundsp) | MIT/Apache-2.0 dual | Graph DSL; good for prototyping/AB rigs, not for your no_std hot path |
| [dasp](https://lib.rs/crates/dasp) | MIT/Apache-2.0 | no-alloc sample/frame/signal traits, ring buffers, interpolation — fits processing-core rules (avoid the GPLv3 **dasp-rs** — different project!) |
| [freeverb-rs](https://github.com/irh/freeverb-rs) (irh) | MIT | Clean Freeverb port (orig. public domain); baseline reverb reference |
| [signalsmith-stretch](https://crates.io/crates/signalsmith-stretch) / [ssstretch](https://lib.rs/crates/ssstretch) | MIT (cxx bindings) | C++ dep — fine native, awkward for wasm/no_std; consider a pure-Rust port of the algorithm instead (MIT allows it) |
| synfx-dsp / HexoDSP (WeirdConstructor) | AGPL/GPL family | IDEAS ONLY — verify per-crate before touching |

No mature pure-Rust granular/spectral-delay crate exists — your suite is genuinely filling a gap; nothing worth depending on beyond dasp + maybe a stretch port.

---

## Machine-by-machine action list

- **Spectral (granular)**: adopt Clouds' morphing window LUT (boxcar↔tri↔Hann), 32-block pre-delay + fractional grain starts, quality-templated interpolated reads, deterministic↔stochastic density crossfade, and the 4-allpass diffuser (constants in §1) post-cloud. **v2**: true-FFT mode lifted from Clouds `pvoc/frame_transformation.cc` (MIT!) — per-bin frame delay + spectral feedback + phase-randomize/quantize/warp per recirculation (§4 architecture).
- **Ice**: Kammerl beat-repeat (MIT) for slice logic; upgrade shifter from dual-head to signalsmith-stretch (tonality limit on!) for the regen octave ladder; diffusion + loop LPF per shimmer guidance.
- **Reverse**: softcut's dogma — dual subheads, equal-power time-based crossfades on every discontinuity, early handoff at head collisions, 2× buffer margin.
- **Digital**: ADM voice = CVSD with N=3/4 coincidence rule, syllabic τ 5 ms, re-encode in feedback (embARC BSD as reference); 12-bit voice = 2:1 feedback-compressor/feedforward-expander around quantize+noise (`avg(|x|)` one-pole rectifier model), fixed ~7 kHz Sallen-Key-style band-limiting; bonus lo-fi voice = Clouds µ-law@16 kHz.
- **MultiTap / Filter**: Signalsmith `delay.h` interpolators for all modulated taps; Valhalla-style modulated allpass diffusion (2–4 stages, g≈0.6) switchable per tap/feedback.
- **Reverb (Flint-style)**: sndkit **bigverb** (MIT/Unlicense, literate writeup) as the core; add shimmer mode via in-loop shifter.
- **NEW machine candidates**: (1) **Dispersion/Chirp delay** — Välimäki spectral-delay-filter allpass cascade w/ feedback, cheap + no_std-safe, nothing on the market at pedal tier; (2) **Stretch delay** — PaulStretch-per-recirculation; (3) **Sympathetic/Resonant delay** — Rings comb-network in the feedback path; (4) **Freeze** — Charles-style stochastic spectral freeze as an infinite-hold on Spectral.

**License hygiene summary**: MI eurorack (MIT), DaisySP (MIT), sndkit/Soundpipe (CC0/MIT/Unlicense), signalsmith-stretch + dsp + basics (MIT), smbPitchShift (WOL), embARC codecs (BSD), liquid-dsp (MIT), freeverb (PD/MIT), dasp/fundsp (MIT/Apache) → all portable. softcut (GPL-3), SuperCollider (GPL), PaulStretch/PaulXStretch (GPL-2/3), GNU Radio (GPL-3), Bela core (LGPL-3), dasp-rs (GPLv3) → concepts only, no code. Papers/blogs (DAFx BBD, JAES spectral delay filters, CMJ freeze, Valhalla, Signalsmith writing) → fair game.

Sources: [Clouds open-source page](https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/open_source/) · [pichenettes/eurorack](https://github.com/pichenettes/eurorack) · [Beads history](https://pichenettes.github.io/mutable-instruments-documentation/trivia_and_history/beads_history/) · [Kammerl Beat-Repeat](https://www.kammerl.de/audio/clouds/) · [Rings docs](https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/) · [monome/softcut-lib](https://github.com/monome/softcut-lib) · [softcut studies](https://monome.org/docs/norns/softcut/) · [electro-smith/DaisySP](https://github.com/electro-smith/DaisySP) · [DaisySP pitchshifter.h](https://github.com/electro-smith/DaisySP/blob/master/Source/Effects/pitchshifter.h) · [sndkit](https://paulbatchelor.github.io/sndkit/) · [PaulBatchelor/Soundpipe](https://github.com/paulbatchelor/soundpipe) · [SC FFT Overview](https://doc.sccode.org/Guides/FFT-Overview.html) · [paulnasca/paulstretch_cpp](https://github.com/paulnasca/paulstretch_cpp) · [essej/paulxstretch](https://github.com/essej/paulxstretch) · [Paulstretch explained](https://polarity.me/posts/articles/2026-07-07-paulxstretch-paulstretch-explained/) · [Charles CMJ tutorial](https://direct.mit.edu/comj/article/32/3/87/94223/A-Tutorial-on-Spectral-Sound-Processing-Using-Max) · [Charles freeze patches](https://www.jeanfrancoischarles.com/2019/11/max-patches-for-spectral-audio-freeze.html) · [Spectral Delay Filters follow-up](https://www.semanticscholar.org/paper/2524359675bff02b2fc33e8f8f3da399f87d3595) · [Aayushchou/spectral-delay](https://github.com/Aayushchou/spectral-delay) · [gen~ spectral delay thread](https://cycling74.com/forums/spectral-delay-gen) · [Signalsmith-Audio/signalsmith-stretch](https://github.com/Signalsmith-Audio/signalsmith-stretch) · [Stretch design blog](https://signalsmith-audio.co.uk/writing/2023/stretch-design/) · [Four Ways To Write A Pitch-Shifter (ADC22)](https://www.youtube.com/watch?v=fJUmmcGKZMI) · [Signalsmith-Audio/dsp](https://github.com/Signalsmith-Audio/dsp) · [Signalsmith-Audio/basics](https://github.com/Signalsmith-Audio/basics) · [smbPitchShift](http://blogs.zynaptiq.com/bernsee/pitch-shifting-using-the-ft/) · [Shimmer: how it's made](https://valhalladsp.com/2010/05/11/enolanois-shimmer-sound-how-it-is-made/) · [ValhallaShimmer history](https://valhalladsp.com/2010/11/23/valhallashimmer-a-bit-of-history/) · [Valhalla diffusion/allpass](https://valhalladsp.com/2011/01/21/reverbs-diffusion-allpass-delays-and-metallic-artifacts/) · [ValhallaDelay diffusion](https://valhalladsp.com/2019/06/13/valhalladelay-the-diffusion-section/) · [CVSD (Wikipedia)](https://en.wikipedia.org/wiki/Continuously_variable_slope_delta_modulation) · [Motorola AN1544](https://perso.esiee.fr/~francaio/enseignement/I4_Etude_de_K_save/an1544.pdf) · [IRIG 106 App. F](https://irig106.org/docs/106-15/appendixF.pdf) · [embARC audio codecs](https://github.com/foss-for-synopsys-dwc-arc-processors/embarc_audio) · [GNU Radio CVSD](https://github.com/gnuradio/gnuradio/blob/master/gr-vocoder/include/gnuradio/vocoder/cvsd_encode_sb.h) · [liquidsdr CVSD](https://liquidsdr.org/doc/cvsd/) · [Raffel & Smith DAFx-10 BBD paper](https://www.dafx.de/paper-archive/2010/DAFx10/RaffelSmith_DAFx10_P42.pdf) · [Bela examples wiki](https://github.com/BelaPlatform/Bela/wiki/Example-projects-and-tutorials) · [SamiPerttu/fundsp](https://github.com/SamiPerttu/fundsp) · [dasp](https://lib.rs/crates/dasp) · [irh/freeverb-rs](https://github.com/irh/freeverb-rs) · [signalsmith-stretch crate](https://crates.io/crates/signalsmith-stretch) · [ssstretch](https://lib.rs/crates/ssstretch) · [KVR reverse delay thread](https://www.kvraudio.com/forum/viewtopic.php?t=599376) · [Strymon TimeLine](https://www.strymon.net/product/timeline/) · [Beads source thread](https://www.modwiggler.com/forum/viewtopic.php?t=281283)
