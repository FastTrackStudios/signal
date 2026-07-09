# Omnisphere 3 Compatibility — Roadmap / Checklist

Goal: **open any Omnisphere patch and have it play identically.** Companion to
`docs/nord-stage-4-signal-routing.md` and `docs/keys-rig-roadmap.md`; the
routing tree lives in `signal-sampler::omni`, the importer in
`signal-sampler::omni_import`.

Status legend: `[x]` done · `[~]` partial · `[ ]` not started.

All counts below are **empirical** — swept from the 37,037 `.prt_omn` patches
on AudioHaven (factory + voyager user library) unless marked *(manual)*, which
comes from the official Omnisphere 3 Reference Guide (v3.0.2c).

---

## 1. Patch format / import layer (`omni_import.rs`)

- [x] AmberPart XML parser (attrs-only dialect, IEEE-754 hex-bit floats,
      entity decoding) — **37,037/37,037 factory+user patches parse clean**
- [x] `ENTRYDESCR` — name, library, browser tags (Author/Genre/Mood/…)
- [x] Layer discovery — `VOICE[i]` ↔ `MULTISAMPLE[i]` pairing (2 layers in
      v2 patches, up to 4 in 2.5+/3)
- [x] Soundsource reference (`MS_IM_0 name/library`) → extraction lookup
- [x] FX rack *names* (layer `EFFRACK`, common `EFFRACK`, `AUXEFFRACK`)
- [x] `MOD_MATRIX` route strings (source/target/depth)
- [x] ARP on/off flag
- [~] `FILTER` — NameStr/act/para/freq/res imported **and applied** (Filter 1
      builds with the patch's cutoff/resonance; routes modulate around that
      base); **type1/type2 algorithm enum, act1/act2, per-filter
      freq1/res1/pan1, spread, balance, env depth, keytracking pending**
- [~] `OSC` — level only; **kind (0=synth, 4=sample), tune/tuneFine/oct/semi,
      phase, symmetry (pwidth/pdepth), hard sync, drift, sample start,
      timbre/mogrify not yet applied**
- [ ] `HARM` (Harmonia: 4 sub-osc intervals/levels/pans/detunes)
- [ ] `WAVES` / `FMWAVES` / `AMWAVES` (wavetable selection, FM/AM modulator)
- [ ] `WAVESHAPER` params (Crusher/Shaper/Reducer)
- [ ] Granular params
- [ ] `AENVPARAMS` / `FENVPARAMS` — AHDSR values (a/h/d/sust/rels, vel
      sens, sync, trigger mode)
- [~] `AENV`/`FENV` `<p>` breakpoints **DECODED + CALIBRATED** against the
      real engine: `l` = linear level, `t` = absolute time ×100 s, `s` = 18
      terminal flag, `c` = curve (0.5 linear). The importer reads the
      breakpoint list (PARAMS attrs are derived UI state the engine
      ignores); 4-point → exact ADSR, longer lists → ADSR approximation.
      Curve law measured: `c` shapes the segment STARTING at its point;
      0.5 = linear, c→0 = fast-start `(1−u)^k`, c→1 = hold-then-drop
      `u^k`, k ≈ 1 + 9.5·|1−2c|. **Full MSEG playback + curve law in our
      DSP, MODENV lists pending**
- [ ] `MOD_ENV2_2` groups (mod-env extended data)
- [ ] `LFO_SET` — per-LFO type/rate/swing/sync/trigger/random (6 LFOs in v2
      patches; sources reference **LFO1–LFO9** in the wild)
- [ ] `EFFMODULE` **parameters** (each effect's `<p>`/attr set), Active
      level (it's a float — wet/dry?), PRE/POST aux switch
- [ ] `ARPSEQ2` — 32× `SLICESEQSTEP` (begin/end/slice/vel), groove/swing
- [ ] `EQ12`/`EQ2`/`DIST` sub-elements inside `FILTER` (per-layer EQ +
      distortion stage embedded in the filter block)
- [ ] `MULTISAMPLE` playback params — Layer0–3 vol/active, release vol,
      pedal level/FX, sample-round-robin xfade, timbre, reverse, thinning
- [ ] `CustomData2` (opaque — identify)
- [~] `.mlt_omn` **Multi** import — SynthMaster/SynthSubEngine×8 parsed
      via the part parser; parts sum in parallel with mixer level (0.75 ≈
      unity, CALIBRATE) + mute→bypass; empty parts skipped. **Per-part
      pan/output/aux sends, Live/Stack modes, master rack pending**
- [ ] `.prs_omn` / Presets folder variants (FX-rack presets etc.)
- [ ] User Tags index (voyager `Settings Library/User Tags`) → browser
- [ ] Patch browser over the Settings Library (37k patches; lazy, indexed
      by ENTRYDESCR tags — don't preload the registry)

## 2. Routing / structure (`omni.rs` + `node_render`)

- [x] Part tree: Quadzone → 4 Layers → osc stack → dual filters → amp →
      layer rack; Common/Aux/Master racks; sends modeled
- [x] Placeholder-safe render (structure plays today; placeholders = thru)
- [x] **Aux send summing** — `SendTap`/`BusInject` render nodes: a node's
      sends tap its output onto named buses; the target container/block
      becomes a send/return (processes bus content, output sums onto the
      pass-through main). Unity gain; per-send level + PRE/POST pending
- [x] Filter series/**parallel** routing at runtime (module combine)
- [ ] Quadzone **Fader scan** (modulatable layer crossfade — needs mod
      matrix); Notes/Velo modes already map to `Zone`
- [~] Container volumes (input_db/output_db) + bypass render (Gain node);
      **imported layer level/pan → volume wiring pending**
- [ ] Multi level: 8 Parts, part mixer (level/pan/output/4 sends), Live
      Mode + Stack Mode grids *(maps to Profile/Stack layer — see
      stacks design)*

## 3. Sound sources (per-layer, pre-filter — the Oscillator stack)

- [x] **Sample mode** — Soundsource via `BlockImpl::Sample` against the
      extraction (name-matched, ~90% of user-patch refs resolve; 4,036
      extracted sources indexed)
- [ ] Sample mode fidelity: start offset (≤90 s), Timbre (crush/shift),
      Mogrify, reverse, layer thinning rules, release samples, pedal noise
- [~] **Synth mode** — native morphing oscillator (sine→tri→saw→square,
      PolyBLEP band-limited, `shape` param modulatable + build param);
      synth-mode patches sound ("1975 Attempt" verified). **Real 638
      wavetable spectra, Symmetry/PWM, Hard Sync, Phase, Analog, Drift
      pending**
- [ ] Which wavetables ship where — extract/recreate the 638 tables or map
      by name to fundsp/mi-plaits wavetables as approximations first
- [x] **Unison** — ≤8 voices, symmetric cent detune, stereo width, 1/√n
      comp, octave / analog-jitter / drift modes (UNI uoct/uanalg/udrft);
      works in BOTH synth mode and sample mode (SampleEngine::set_unison
      spawns detuned/panned voice copies at every zone trigger). Scatter
      pending; normalized→cents scaling to calibrate
- [~] **Harmonia** — 4 sub-oscillators (interval/level/pan/waveform) from
      the HARM element (smi/lvl/pan/wfm, hrmOn/hrmLv gates). **Per-voice
      symmetry/sync + sample-mode pending**
- [x] **FM** — per-note modulator osc, ratio + depth (OSC `fm`), morphing
      modulator waveform from `fmwf`
- [~] **Ring Mod** — key-tracked carrier, mix from OSC `am`; **carrier
      wave/ratio import (amwf/amscl) pending**
- [x] **Dual Frequency Shifter** — true SSB shifter (Hilbert allpass
      pair), two shifters serial/parallel, ±Hz + mix each, runtime params;
      imported from the DFS element (freq scale ±2 kHz CALIBRATE)
- [x] **Waveshaper** — native Crusher/Shaper/Reducer block (drive/crush/
      reduce/mix, runtime + build params) from the WAVESHAPER element
- [ ] **Granular** — 8 grain voices/layer; Speed/Position/Intensity/WILD/
      Legacy modes *(manual)*

## 4. Filters — 70 types *(manual v3)*, **45 algorithm indices observed**

The patch stores the algorithm as `type1`/`type2` — a 50-slot enum at
0.02 steps, now MEASURED per slot (see `TYPE1_TABLE`). Cutoff is
calibrated: **freq → 15 Hz × 2^(9.55·v)** (knee sweep, keytracking off).
`NameStr` is just the filter-section preset label.

- [~] Native SVF cascade: LP/HP/BP/Notch at 1..8 poles (12 dB TPT
      sections, resonance on the first) — covers the Classic/Basic pole
      families; coarse mode+poles classified from the factory `NameStr`
      ("Classic LPF 4-pole" → LP 24 dB) until the type enum is decoded
- [x] **Type enum DECODED** — all 50 `type1` slots fingerprinted through
      the real engine (8-band Goertzel per slot): measured mode + pole
      table drives the importer (`TYPE1_TABLE`); NameStr now only selects
      the ladder character. Pole counts are lower bounds — a low-cutoff
      refinement pass can sharpen them
- [x] Pole-cascade family: 1..8-pole LP/HP/BP/Notch via SVF cascade
      (true ladder character models still pending below)
- [~] Character models: a saturating 4-stage ladder engine (tanh input,
      resonance feedback → self-osc) now backs the Juicy/Moogie/OB/Jupiter/
      Sauce/Beefy/Warm/Power/FATBOY/French/Brit lowpass families (name-
      classified). **Per-family voicing differences, UVI/Metal Pipe
      colors, HP/BP character variants pending — needs A/B calibration**
- [ ] Formant, Allpass, Notch, dual/stereo combos (Series Throaty LP12s,
      Parallel Widened LP12s, Dual Stereo Bandpass, Bandpass+Allpass…)
- [ ] Component-modeled **filter saturation** (v3)
- [~] Dual-filter plumbing: Filter 2 builds from act2/freq2/res2; the
      Filters module compiles SERIES or PARALLEL from `para`. **Pan/spread/
      balance, per-filter env depth, keytracking pending**
- [ ] Embedded per-layer `DIST` + `EQ12`/`EQ2` stages (post/pre flags)

## 5. Envelopes — 12 per Part (4 Amp + 4 Filter + 4 Mod) *(manual)*

- [~] Native ADSR exists (drives oscillator voices) — not yet wired to
      imported AHDSR values
- [~] AHDSR from `AENVPARAMS`/`FENVPARAMS` — attack/decay/sustain/release
      import + apply: synth voices get the full ADSR, sample voices get
      attack/release (SampleEngine set_attack/release_frames; decay/sustain
      need per-voice ADSR in the engine), the Filter Env modulator gets the
      FENVPARAMS shape and the FILTER envdpth(+inv) becomes a live cutoff
      route. **Hold, velocity sens, sync, trigger modes pending; the
      normalized→seconds curve (cubic × 10 s) needs A/B calibration**
- [ ] **MSEG Complex envelopes** — breakpoint lists from `<p>` children;
      curves (9 presets), looping, Chaos *(proto `MultisegEnvelopeParams`
      exists, DSP doesn't)*
- [ ] Filter-env → cutoff with per-filter depth (mod-matrix route)

## 6. LFOs — 8 *(manual v3)*; sources up to LFO9 observed

- [~] LFO engine: sine/tri/saw/square, free rate; LFO_SET rate+type
      import onto the part's 8 LFO modulators. **Rate curve partially
      probed on a live factory route (0.4642→3.4 Hz, 0.516→6.6 Hz,
      0.55→8.0 Hz — entangled with 5 other routes; needs single-route
      isolation to fit). Tempo-sync, swing, phase, S&H, retrigger,
      delay/fade pending**

## 7. Mod Matrix — 48 slots *(manual v3)*; **60 sources / 591 targets observed**

Harness findings (2026-07-02): factory routes RUN in our host (proved by
mute A/B on a live route); LFO rate edits on a live route change the
audio; but **flipping a row's source from "off" to a live source via
state injection does NOT create a running route** — route activation
involves state beyond the row attrs (candidates: the `lLFOP` 0→999
registration seen in round-trips, or graph build at patch-load). Sweep
tooling gotchas that cost a day: unanchored `rate` regex corrupts
`pulserate`; source-muting/voice-soloing edits can silence the audible
target entirely — always A/B against an untouched control.

The single biggest unlock: nearly every imported patch's character lives
here. Top observed sources: Wheel, LFO1–9, Velo, Random(±uni), ModEnv1–4,
Aftertouch, **MPE (MPEv/MPE3)**, Layer A/B FENV, Key, Alt, Bias1/2, Bender,
Constant. Top targets: `A/B/C/D freq` (cutoff), `tune/tuneFine`, `atrm`
(amp trem), `Harmmix`, `pdepth` (PWM), `mogrify`, `timbre`, `hrdsnc`, LFO
rate/swing, envelope points (`A E1P0`), aux send (`irsendaux`), pan.

- [x] Routes **imported** (source/target/depth strings preserved as params)
- [x] **Control-rate runtime** — `ModEngine` in `node_render`: sources tick
      once per block, writes `base + depth×value` to `(leaf, param)` targets
      as parameter events; wired into Nord synth layers AND translated
      Omnisphere routes ("A freq"/"A res" → layer filter cutoff/resonance)
- [~] Source implementations: MIDI wheel/velocity/aftertouch/bender/CCn,
      Key (keytrack), Random (per-note S&H), Alt, Constant/Bias, LFOs
      (4 waves, imported rate/type), envelopes (imported ADSR) —
      **missing: MPE per-note, LFO tempo-sync**
- [x] Target resolution: block display name + backend param name within the
      route's subtree ("LPF UVI 3.cutoff")
- [ ] lo/hi range, mute, damp (smoothing) per route; per-route base from
      imported block params (base = param default today)

## 8. Arpeggiator — per Part, 32 steps

- [~] Step engine (`ArpEngine`): up-pattern over held notes, per-step
      velocity + gate from `ARPSEQ2`/`SLICESEQSTEP` (tick spacing → step
      beats), tempo-clocked, phrase-start reset, rests. **Other patterns,
      swing, latch, step modifiers, Groove Lock, per-part arps inside a
      Multi pending**
- [x] MIDI-domain placement — the root ModEngine rewrites the note stream
      before rendering (CC/bend pass through)

## 9. FX units — **98 types observed** (93 internal *(manual v3)*)

Every rack is 4 slots; implement by observed usage frequency. The FTS
plugin suite covers a large share as `*-dsp` crate reuse (framework-free,
already `.clap`-built):

| Priority (uses) | Omnisphere FX | FTS building block |
|---|---|---|
| 12,146 | Chorus Echo | chorus-dsp + delay-dsp |
| 10,124 | PRO-Verb | reverb-dsp |
| 7,756 / 6,190 / 3,524 / 1,785 / 1,384 / 1,139 / 374 | Studio EQ, Vintage 2/3-Band, Graphic 12/7-Band, Parametric 2/3-Band | eq-dsp |
| 6,044 | Tape Slammer | tape-dsp |
| 5,417 / 2,341 / 1,264 / 1,082 | Super Verb, Velvet Verb, EZ-Verb, Spring Verb | reverb-dsp (12 algos) |
| 5,362 / 650 / 462 / 45 | Analog Chorus, Chameleon Chorus, Solina Ensemble, Ultra Chorus | chorus-dsp (5 engines) |
| 5,019 / 378 | Vintage Tremolo, Analog Vibrato | trem-dsp |
| 4,308 / 4,030 / 3,077 / 2,474 / 1,539 / 805 / 658 / 408 | Multiband/Vintage/Precision/Modern Compressors, Tube Limiter, 1176, 1950s Tube, Stomp Comp | comp-dsp + limiter-dsp |
| 3,472 / 3,046 / 2,740 / 2,000 / 1,479 / 910 / 394 / 233 / 222 | Retroplex, BPM Delay (X2/X3), Magnetic Echo, Radio Delay, Swiss Army Delay, Refraction, Backward Echo | delay-dsp |
| 3,372 / 2,352 / 963 / 805 / 797 | Optical Leveling Amp, Super Channel, 1970s Console, Vintage British Desk, Solid State Buss | comp-dsp + saturate-dsp (console) |
| 2,452 / 919 / 336 / 210 / 51 … | Brit-Vox, Smoke Amp, Classic Twin, Bassman, Hiwattage, Rock Stack… | **FTS-NAM** (amp sims → nam-dsp models) |
| 2,327 / 1,366 / 893 / 844 / 620 / 395 / 288 | Valve Radio, Flame Distortion, Tube Saturator, Toxic Smasher, Mean Machine, Multiband Distortion, Tube Overdrive | saturate-dsp |
| 895 / 396 / 222 / 160 / 158 / 112 / 328 / 179 | Analog/Retro/EZ/Barberpole/PRO-Phaser, Flanger, Analog/Retro-Flanger | chorus-dsp engines + fts-modulation |
| 390 / 264 / 85 / 9 | Envelope Filter, Formant Filter, Wah-Wah, Crying Wah | wah-dsp |
| 3,209 | Imager | build native (stereo width) |
| 2,323 / 2,030 | Pump-O-Matic, Pulsar Split | comp-dsp sidechain + build |
| 1,583 / 1,127 / 1,047 / 1,038 / 479 / 342 / 252 / 33 | Unstable Drifter, Noisemaker, Half Speeder, Solar Shimmer, Flip Backward, Warp Shifter, Quad Resonators, Ring of Fire | build native ("Creative" family) |
| 1,213 | Innerspace | build native (convolution-ish — convolver exists) |
| 1,527 / 1,827 / 1,178 / 176 | Power Filter, Classic Tube Filters, Classic Tube 2-Band/Midband | filter DSP (§4) |

- [ ] FX **unit registry**: Omnisphere type-name → our block ctor + param map
- [ ] Per-unit parameter import (each `EFFMODULE`'s param set)
- [ ] `Active` float semantics (on/off vs mix?)
- [ ] Aux rack send/return with PRE/POST (needs §2 send summing)

## 10. Verification — "identical" needs a harness

- [ ] **A/B render pipeline vs real Omnisphere on voyager** (same approach
      as the CSS engine-matching work): render a MIDI test file through
      Omnisphere (headless via its plugin in a host on voyager, ssh),
      render the imported patch here, diff (null test / spectral)
- [ ] Corpus regression: per-subsystem coverage stats over all 37k patches
      (how many patches use only implemented features → "% identical")
- [ ] Tag the importer's warnings (unmatched soundsource, unsupported FX)
      so a patch reports its own fidelity

---

## Suggested order of attack (leverage-ranked)

1. **Mod-matrix control-rate runtime** (§7) — unlocks Nord + Omnisphere
   both; imported routes already carry the data
2. **AHDSR + filter/amp param application** (§5, §4-plumbing) — imported
   patches get their dynamics + tone; native ADSR/SVF already exist
3. **Aux/send summing in the renderer** (§2) — completes the routing axes
4. **Synth-mode wavetable oscillator** (§3) — un-silences synth patches
   (fundsp/mi-plaits first, exact 638 tables later)
5. **FX registry over FTS `*-dsp` crates** (§9, by the usage table) —
   top ~15 units cover the overwhelming majority of racks
6. **Filter type enum decode + character families** (§4)
7. Unison → Harmonia → FM/Ring/Waveshaper → Granular (§3)
8. Arp engine (§8), MSEG envelopes (§5), Multi/Live/Stack (§2), browser (§1)
9. A/B verification harness (§10) throughout
