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
- [~] `FILTER` — NameStr/para/freq/res imported; **type1/type2 algorithm
      enum, act1/act2, per-filter freq1/res1/pan1, spread, balance, env
      depth, keytracking not yet applied**
- [~] `OSC` — level only; **kind (0=synth, 4=sample), tune/tuneFine/oct/semi,
      phase, symmetry (pwidth/pdepth), hard sync, drift, sample start,
      timbre/mogrify not yet applied**
- [ ] `HARM` (Harmonia: 4 sub-osc intervals/levels/pans/detunes)
- [ ] `WAVES` / `FMWAVES` / `AMWAVES` (wavetable selection, FM/AM modulator)
- [ ] `WAVESHAPER` params (Crusher/Shaper/Reducer)
- [ ] Granular params
- [ ] `AENVPARAMS` / `FENVPARAMS` — AHDSR values (a/h/d/sust/rels, vel
      sens, sync, trigger mode)
- [ ] `AENV`/`FENV`/`MODENV` `<p>` children — **MSEG breakpoints** (the
      Complex envelope: "hundreds of stages" *(manual)*)
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
- [ ] `.mlt_omn` **Multi** import (8 Parts + mixer + 4 aux + master)
- [ ] `.prs_omn` / Presets folder variants (FX-rack presets etc.)
- [ ] User Tags index (voyager `Settings Library/User Tags`) → browser
- [ ] Patch browser over the Settings Library (37k patches; lazy, indexed
      by ENTRYDESCR tags — don't preload the registry)

## 2. Routing / structure (`omni.rs` + `node_render`)

- [x] Part tree: Quadzone → 4 Layers → osc stack → dual filters → amp →
      layer rack; Common/Aux/Master racks; sends modeled
- [x] Placeholder-safe render (structure plays today; placeholders = thru)
- [ ] **Aux send summing** — `Container.sends` are modeled but the renderer
      doesn't mix send buses yet (blocks Aux racks + `irsendaux` targets)
- [ ] Filter series/**parallel** routing at runtime (para flag imported)
- [ ] Quadzone **Fader scan** (modulatable layer crossfade — needs mod
      matrix); Notes/Velo modes already map to `Zone`
- [ ] Layer level/pan applied (imported as params, not applied)
- [ ] Multi level: 8 Parts, part mixer (level/pan/output/4 sends), Live
      Mode + Stack Mode grids *(maps to Profile/Stack layer — see
      stacks design)*

## 3. Sound sources (per-layer, pre-filter — the Oscillator stack)

- [x] **Sample mode** — Soundsource via `BlockImpl::Sample` against the
      extraction (name-matched, ~90% of user-patch refs resolve; 4,036
      extracted sources indexed)
- [ ] Sample mode fidelity: start offset (≤90 s), Timbre (crush/shift),
      Mogrify, reverse, layer thinning rules, release samples, pedal noise
- [ ] **Synth mode** — wavetable oscillator, 638 morphing wavetables
      *(manual)*; SHAPE sweep, Symmetry/PWM, Hard Sync, Phase, Analog,
      **Drift** (v3) — *blocks every synth-mode patch (e.g. "1975
      Attempt"); highest-leverage source gap*
- [ ] Which wavetables ship where — extract/recreate the 638 tables or map
      by name to fundsp/mi-plaits wavetables as approximations first
- [ ] **Unison** — ≤8 voices, depth/detune/spread/octave/analog/scatter/drift
- [ ] **Harmonia** — 4 extra oscillators (interval/level/pan/detune/wave)
- [ ] **FM** — dedicated per-layer modulator osc (any wavetable)
- [ ] **Ring Mod** (polyphonic, key-tracked)
- [ ] **Dual Frequency Shifter** (v3 — serial/parallel pair, per-note)
- [ ] **Waveshaper** (Crusher/Shaper/Reducer, polyphonic in-osc)
- [ ] **Granular** — 8 grain voices/layer; Speed/Position/Intensity/WILD/
      Legacy modes *(manual)*

## 4. Filters — 70 types *(manual v3)*, **45 algorithm indices observed**

The patch stores the algorithm as `type1`/`type2` (normalized index,
0.02 steps → ~50-slot enum; 45 distinct values in the wild). `NameStr` is
just the filter-section preset label.

- [~] Native SVF exists (LP/HP/BP 12 dB) — covers "Basic 12db Lowpass",
      "State Variable 12dB"-class types
- [ ] **Decode the type enum** — map each `type1` value to its algorithm
      (play a sweep per type in Omnisphere on voyager, or match NameStr
      factory presets → type values across the 37k corpus)
- [ ] Pole-cascade ladder family: Classic LPF 1/2/3/4/6/8-pole
      ("Classic LPF 4-pole" = 39k uses — #2 most-used filter)
- [ ] Character models: Juicy, UVI 1–3, Power, Warm, Beefy, Sauce, OB
      (Oberheim), Jupiter, French, Brit, FATBOY, Metal Pipe±, Rich-and-
      Moogie 1–3 — each is LP/HP/BP/notch variant with its own saturation
- [ ] Formant, Allpass, Notch, dual/stereo combos (Series Throaty LP12s,
      Parallel Widened LP12s, Dual Stereo Bandpass, Bandpass+Allpass…)
- [ ] Component-modeled **filter saturation** (v3)
- [ ] Dual-filter plumbing: per-filter freq/res/pan offsets, spread,
      balance, series/parallel, per-filter env depth, keytracking (+invert)
- [ ] Embedded per-layer `DIST` + `EQ12`/`EQ2` stages (post/pre flags)

## 5. Envelopes — 12 per Part (4 Amp + 4 Filter + 4 Mod) *(manual)*

- [~] Native ADSR exists (drives oscillator voices) — not yet wired to
      imported AHDSR values
- [ ] AHDSR from `AENVPARAMS`/`FENVPARAMS` (attack/hold/decay/sustain/
      release + velocity sensitivity, tempo sync, trigger modes)
- [ ] **MSEG Complex envelopes** — breakpoint lists from `<p>` children;
      curves (9 presets), looping, Chaos *(proto `MultisegEnvelopeParams`
      exists, DSP doesn't)*
- [ ] Filter-env → cutoff with per-filter depth (mod-matrix route)

## 6. LFOs — 8 *(manual v3)*; sources up to LFO9 observed

- [ ] LFO engine: waveform set, rate (free + tempo-sync), swing, phase,
      random/sample-hold, retrigger modes, delay/fade — *(proto
      `LfoParams` exists; fts-modulation has the base engine)*

## 7. Mod Matrix — 48 slots *(manual v3)*; **60 sources / 591 targets observed**

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
      LFO (sine/tri/saw/square, free-rate), note-gated ADSR envelope —
      **missing: Key, Alt, Bias, Random, Constant, MPE per-note; imported
      LFO/env parameter values (defaults used today)**
- [x] Target resolution: block display name + backend param name within the
      route's subtree ("LPF UVI 3.cutoff")
- [ ] lo/hi range, mute, damp (smoothing) per route; per-route base from
      imported block params (base = param default today)

## 8. Arpeggiator — per Part, 32 steps

- [ ] Step engine from `ARPSEQ2`/`SLICESEQSTEP` (begin/end ticks, slice,
      velocity), clock/swing/latch, note patterns (19), step modifiers
      (transpose/slide/chord/dividers/strums) *(manual)*, Groove Lock
- [ ] MIDI-domain placement (before the engine, like the Nord Master Clock)

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
