# BigSky MX — behavior reference & gap analysis

Target for reverb-dsp parity. Sources: BigSky MX User Manual RevB +
Secret Weapons full walkthrough. Authoritative for the parity passes.

## Platform shape

12 engines, **any two per preset** with routing `Off / Parallel / Series 1→2 /
Series 2→1 / Split L|R`. Per-slot: full param set, **Pan** (offset each
reverb in the stereo field — the "two reverbs panned ±3 either side of a
dry center" trick), Mix, copy/paste settings between slots. Kill-dry global
(mix becomes wet level — studio send workflow). Infinite footswitch:
**Freeze vs Infinite vs Off** per preset + latch/momentary. Boost ±3 dB
per preset, persist/spillover, IR-based Cab Filter (global, legacy),
line/instrument input, 300 presets, Nixie 2 editor over USB-C.

Common per-reverb params: Decay, Pre-Delay, Tone, Mod (speed+depth),
Param1/Param2 knobs **reassignable to any menu param** (press-hold a menu
entry to bind), Low End (default P1 on most), Output Level, Pan,
Infinite Mode.

## Engines (MX voices are rebuilds; Classic voices = original BigSky ports)

| Engine | Key params | Notes |
|---|---|---|
| **Room** | Size (Studio/Club/…), Diffusion, Room Style, Low End, Voice MX/Classic | Classic = slappier, punchier, more resonant harmonic buildup |
| **Hall** | Size (Concert/Arena), Low End, Mid, Swell Rise + Swell Type (wet / wet+dry), Voice | Arena = fewer ERs, deeper/cavernous. Mid control enables mid-scooped "space for dry" EQ |
| **Chamber** | Color (Neutral/…), Diffusion, Low End | NEW algorithmic engine; between room and hall; deliberately simple |
| **Plate** | Size (Small/…), Low End, Voice | MX voice smoother diffusion, tamer harmonic buildup |
| **Spring** | Dwell (Clean/…/Tube), # Springs, Voice | Rebuilt; springs count selectable |
| **Impulse** | Impulse Select, **Decay = % of IR played**, **Tail = Envelope or Gate** (how Decay<100% shortens), **Attack** (onset shaping), **Stretch** (re-samples IR: 0.25×–4×, changes decay AND pitch/tone — 2× brighter/snappier, 4× darker/grittier), **Direction** Fwd/Reverse (reverse = riser through the whole file), **Feedback** (wet → pre-delay recirculation, interacts with pre-delay), Low End | On IR load, all params reset to defaults except Mix. Factory IRs incl. spring tanks, oil can, plates, Lexicon 224 captures, halls/chapels/warehouses. USB IR import |
| **Cloud** | **Ensemble** (analyzes input, generates synthetic string layer — from Cloudburst), **Diffusion** (skittery multitap → smooth fog; retained alongside Ensemble), Low End | The flagship; Ensemble + Diffusion coexist (Cloudburst dropped Diffusion, MX has both) |
| **Shimmer** | **Shift 1 + Shift 2** (two independent voice intervals, each −oct…+oct range), **Amount** (level of both shift voices), **Feedback: Input / Regenerative / Input+Regen** (regen = shift inside the loop → runaway octave ladders), Low End, Voice | Dual-voice — our shimmer is single-shift |
| **Bloom** | Length, Feedback, **Harmonics** (analyzes input, builds harmonic overtones into the trail — dense keys-pad texture), Low End | |
| **Chorale** | Vowel (AAHHOO/…), Resonance (Mild/…), **Choir** (level), **Choir Voice (Tenor/…: two pitch ranges)**, **Mod = per-voice pitch/timbre randomization** (more mod = more distinct singers, humanizes) | |
| **Magneto** | Heads 1–4, Spacing Even/Uneven, Diffusion (multitap → reverb smear), **Ping Pong On/Off** (taps alternate hard L/R — center clarity + width), Low End. **Knob remap: Decay = delay time, Pre-Delay = feedback** | |
| **NonLinear** | Shape (Swoosh/Reverse/Gate/Bounce/Gauss…), gate speed, **Chop** (amplitude modulation/tremolo on the decay — polyrhythmic trem effects), Diffusion, **Late Speed / Late Decay / Late Level** (separate late-reverb stage), Low End. Same Decay=time/Pre-Delay=feedback remap | |

## Gap analysis vs reverb-dsp (current)

Already covered (✓ = parity or better):
- Engine coverage: room/room_studio/room_chamber ✓ (sizes ≈ variants), hall/hall_arena/hall_cathedral ✓ (+cathedral bonus), plate ×3 ✓ (+lexicon/progenitor bonus), spring ×2 ✓, chamber ✓ (room_chamber ≈ Chamber engine), cloud ✓ (diffusion), bloom ✓ (length/feedback), chorale ✓ (vowels/resonance/formants), magneto ✓ (heads/spacing/diffusion), nonlinear ✓ (shapes), shimmer ✓ (single shift), convolution ✓ — plus swell, velvet, reflections, freeverb they don't have.
- Convolution extras they DON'T have: dual-IR A/B morph, motion allpass stage, LFO/env mod of wet/predelay/damping ✓ (keep — differentiators).
- IR transforms: stretch/reverse/trim/envelope/predelay/gain exist in IrTransforms ✓ but are OFFLINE (applied at load). MX exposes them as LIVE params.
- Freeze ✓ (chain), param smoothing ✓, spillover/presets/boost = Signal rig level.

Gaps (parity passes):
1. **Dual-reverb layer**: DualReverb (two ReverbChains) with Off/Parallel/Series12/Series21/Split + per-slot pan + per-slot mix (mirror delay's dual.rs). Biggest structural item.
2. **Per-slot Pan** on ReverbChain output (equal-power, smoothed).
3. **Impulse live params**: runtime Decay%/Tail(Env|Gate)/Attack/Stretch/Direction/Feedback on the convolution algorithm — re-prepare from the cached original IR on param change (worker thread, hot-swap; NOT in tick). Stretch = resample (reuse IrTransforms), Direction = reverse, Tail/Decay = windowing, Attack = onset envelope, Feedback = wet→predelay recirculation (that one IS runtime DSP).
4. **Cloud Ensemble**: pitch-tracked synthetic string/pad layer fed into the reverb (input analysis → oscillator/grain ensemble). New DSP.
5. **Shimmer dual shift**: Shift1+Shift2 (two GrainPitchShifter/pitch-dsp voices), Amount, Feedback mode Input/Regen/Input+Regen.
6. **Bloom Harmonics**: harmonic-overtone generator on the trail (pitch-tracked or spectral-ish; pitch-dsp PolyOctave/POG is a good basis).
7. **Chorale**: Choir level + Choir Voice (Tenor/Soprano ranges), mod = per-voice randomization (we tamed the formants earlier — the vowel resonance stays a param).
8. **Magneto Ping Pong** + knob remap semantics (decay=time, predelay=fb) at the param-mapping layer.
9. **NonLinear Chop** (trem on decay) + explicit Late Speed/Decay/Level stage + gate speed param.
10. **Hall/Room**: Mid control (Hall), Swell Rise/Type as engine params (reuse swell algorithm's envelope logic), Size param mapping to existing variants (Concert/Arena = variant select, unify).
11. **Voices**: MX/Classic pairs — treat current algorithms as one voice, add the counterpart where we have both heritages (e.g. plate vs plate_lexicon already ≈ two voices); formalize a `voice` param mapping variants.
12. **Wet tremolo** chain-level (Flint-style; also covers TimeLine MX's Reverb-machine trem) — cheap, shared.
13. Cab Filter: skip in reverb-dsp (Signal has NAM/cab infrastructure at rig level) — note only.

## Suggested pass order
A. Dual layer + pan + wet trem (structural, mirrors delay work)
B. Impulse live params (runtime re-prepare pipeline)
C. Shimmer dual-shift + feedback modes; Magneto ping-pong; NonLinear chop/late stage (in-algorithm params)
D. Cloud Ensemble + Bloom Harmonics + Chorale choir (the input-analysis generators)
E. Voice pairs + Hall mid/swell + size unification + signal-fx exposure
