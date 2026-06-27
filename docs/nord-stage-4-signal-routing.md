# Nord Stage 4 → Signal: Block / Module / Layer / Engine Routing

> Goal: recreate **any** Nord Stage 4 (NS4) patch inside Signal's composition model
> (`Block → Module → Layer → Engine → Preset`, see `DOMAIN.md`). This document is
> the reference for `just keys` — a faithful replication of the NS4 signal routing.
>
> Derived from the *Nord Stage 4 User Manual, OS v1.4x* (76 pp). Page numbers cited
> as `(pN)`. The NS4 synth engine is the **Nord Wave 2**; organ is the Clavia
> tonewheel/transistor/pipe model set; piano is the **Nord Piano Library** sample
> player.

---

## 0. TL;DR — the shape

```
Nord Program  ────────────────▶  Signal Rig Preset            (the playable patch)
  ├─ Organ Section  (2 layers) ─▶  Organ Engine   → 2 Layers
  ├─ Piano Section  (2 layers) ─▶  Keys  Engine   → 2 Layers
  └─ Synth Section  (3 layers) ─▶  Synth Engine   → 3 Layers
                                    └ each Layer = sound-source Module
                                      → Filter/Amp Modules
                                      → FX Modules (Mod1·Mod2·Delay·Amp/EQ·Reverb) + Comp block
                                      → (modulated by Envelope/LFO/Morph blocks)
  + 1 shared Rotary  + Global Delay/Comp/Reverb  + Morphs ×3  + Layer Scenes ×2
```

**7 sound layers total**: Organ A/B, Keys A/B, Synth A/B/C. **6 FX instances**
(Organ A+B share one). **1 global Rotary** (always the last node). (p13, p47)

The key realization: the proto `BlockType` set already contains nearly every block
NS4 needs — including the modulation blocks (`Envelope`, `Lfo`, `Arpeggiator`,
`ModMatrix`, `Unison`, `StepSeq`) and sources (`Oscillator`, `Sampler`,
`Wavetable`, `FmOperator`, `Tonewheel`, `Noise`). So this is a **composition +
native-DSP-implementation** task, not a "new domain types" task.

---

## 1. Domain mapping (NS4 ↔ Signal)

| Nord Stage 4 | Signal domain term | Notes |
|---|---|---|
| **Program** | **Rig Preset** | The whole patch; what you load & play. Stores all sections, layers, FX, morphs, scenes, splits, master clock. (p37) |
| **Section** (Organ / Piano / Synth) | **Engine** | An instrument voice. Three engines per Keys rig. |
| **Layer** (A/B, A/B/C) | **Layer** | A processing lane inside an engine. Organ 2, Keys 2, Synth 3. (p13) |
| Sound source + filter + amp + mod | **Modules** of **Blocks** | e.g. an Oscillator Module, a Filter Module. |
| Per-layer FX chain | **a sequence of FX Modules** | Mod1·Mod2·Delay·Amp/EQ·Reverb modules + Comp block (§6). |
| Drawbars / Osc Ctrl / cutoff / … | **Block parameters** | Morphable params are modulation destinations. |
| Envelopes / LFO / Vibrato / Arp | **Modulation Blocks** | Control-rate sources, routed via a ModMatrix. |
| Morph (Wheel / A.T. / Ctrl Ped) | **Macro / ModMatrix sources** | 3 performance morph sources. |
| Layer Scene I / II | **Preset Snapshots** (mute-state only) | NOT a parameter snapshot — only per-layer on/off. (p42) |
| Master Level | (host output gain) | NOT stored in the program. |

The Signal rig already calls a playable composition a **Preset** whose live audio
projection is a `RigProfile` of patches. A Nord Program becomes one **Keys Rig
Preset**; its Layer Scenes become two snapshots that differ only in which layers
are muted. The morph sources become the rig's three performance macros.

---

## 2. The `just keys` rig

`just keys` opens the **Keys Rig** the same way `just guitar` opens the Guitar Rig
(`RigManager` → `ProfileRig`). The Keys Rig's "preset" is a Nord-style Program
composed of three engines:

```
Keys Rig Preset  "Nord Stage"
├─ Engine "Organ"   (shared FX)   ── Layer A ─┐
│                                  ── Layer B ─┴─▶ shared Organ FX → Rotary
├─ Engine "Keys"    (piano)        ── Layer A ───▶ Keys A FX → (Rotary?)
│                                  ── Layer B ───▶ Keys B FX
└─ Engine "Synth"                  ── Layer A ───▶ Synth A FX
                                   ── Layer B ───▶ Synth B FX
                                   ── Layer C ───▶ Synth C FX
                          ╰─ all → bus → master ; Rotary is the single shared tail
```

Unlike the guitar rig (one input → one chain), the Keys rig is **MIDI-driven**
(it generates audio from notes, like `signal-sampler`'s `SamplerInstrument`) and
**polyphonic across 7 simultaneous layers** summed to the output. This is the first
rig that exercises the full `Engine → Layer → Module` composition rather than a
flat block chain.

---

## 3. Block taxonomy — what each NS4 element is

All of these `BlockType`s **already exist** in `signal_proto::block::BlockType`
unless marked **(NEW)**. The implementation axis (`BlockImpl`, see the guitar rig)
is `Native` (our DSP), `Nam`, `Ir`, `Plugin`, plus **`Sample` (NEW impl)** for
sample-playback blocks.

### 3.1 Sound sources

| NS4 element | BlockType | BlockImpl | Notes |
|---|---|---|---|
| Synth Analog osc (Pure/Sub/Sync/Shape/Multi/Super/Misc) | `Oscillator` | Native | `Osc Ctrl` = one shaping param; meaning varies by category (PWM/detune/sync/mix). |
| Synth FM-H / FM-I | `FmOperator` | Native | 2–4 operator algorithms; Osc Ctrl = FM amount; Partial (0.5–24) / Pitch (−12…48). |
| Synth Wave (wavetable) | `Wavetable` | Native | Osc Ctrl unused. |
| Synth Samples mode | `Sampler` | **Sample** | Nord Sample Library; Bright filter; sample-baked amp-env/velocity presets. |
| Noise (White/Pink/Red) | `Noise` | Native | A noise category of the analog osc; modelled as a `Noise` source. |
| Organ B3 / B3 Bass | `Tonewheel` | Native | Drawbars + percussion + scanner V/C + tonewheel mode + key click. |
| Organ Vox / Farfisa | `Tonewheel` (model param) | Native | Same source block, `model` param switches voicing + drawbar semantics. |
| Organ Pipe 1 / Pipe 2 | `Tonewheel` (model param) | Native | No electromechanical artifacts; chorus = detune model. |
| Piano / EP / Clav | `Sampler` | **Sample** | Nord Piano Library; type+model+size; timbre/string-res/etc. as params. |

> **Organ source design:** one `Tonewheel`-typed source block with a `model` enum
> {B3, B3Bass, Vox, Farfisa, Pipe1, Pipe2} is cleaner than six block types. The 9
> drawbars are 9 params; their semantics (continuous level vs Vox mix-drawbar vs
> Farfisa on/off switch vs B3Bass 16'/8'-only) are interpreted per `model`.

### 3.2 Processors (audio chain)

| NS4 element | BlockType | Impl | Key params |
|---|---|---|---|
| Synth Filter | `Filter` | Native | type {LP24,LP12,LPM,LP+HP,HP,BP}, Freq, Res, EnvAmt, Drive {Off,1,2,3}, KbdTrack {Off,⅓,⅔,1}. |
| Amp Sim / EQ overdrive | `Amp` + `Eq` | Native | amp models {Twin,JC,Small}; Drive; 3-band EQ (Bass 100Hz, Mid 200Hz–8kHz, Treble 4kHz, ±15dB). |
| Amp Sim resonant filters | `Filter` (LP24/HP24) | Native | within the Amp/EQ unit; morphable cutoff (pedal-wah). |
| Compressor (FX section) | `Compressor` | Native | Amount, Fast mode; per-layer or Global. |

### 3.3 Effects

| NS4 effect | BlockType(s) | Modes / params |
|---|---|---|
| **Mod 1** | `Panner` / `Trem` / `RingModulator` / `Wah` | A-Pan, Trem, RM, A-Wah, Wah, Pump — Rate+Amount, mst-clk sync, Variation/Ped. |
| **Mod 2** | `Phaser` / `Flanger` / `Vibrato` / `Chorus` | Phaser, Flanger, Vibe, Chorus, Ensemble, Spin — Rate+Amount, Variation. |
| **Delay** | `Delay` | Tempo, Feedback, Dry/Wet, Ping-Pong, feedback Effects {Ens,Vibe,Chor,Flam,Space}, Filter {LP,HP,BP}, Analog mode, mst-clk (½–1/32 +S/T/D). |
| **Reverb** | `Reverb` | 6 types {Spring,Booth,Room,Stage,Hall,Cath}, Variation/Chorale, Bright/Dark, Dry/Wet. |
| **Rotary** | `Rotary` | Slow/Fast/Stop, Drive, Angle, Close-Mic; bass/horn balance + speeds (menu). **One global instance, always last.** |

### 3.4 Modulation / control blocks (already in proto)

| NS4 element | BlockType | Routes to | Params |
|---|---|---|---|
| Amp / Filter / Osc Envelope | `Envelope` | Amp / Filter Freq / Osc Ctrl or Pitch | Attack, Decay, Release (AD-R; Sustain = max Decay). Velocity scaling. Osc-env amount is bipolar. |
| LFO | `Lfo` | Osc Ctrl / Pitch / Filter (one dest) | Rate, ModAmt, waveform {Tri,Saw↓,Saw↑,Square,S&H}, mst-clk sync, Group. |
| Vibrato | `Lfo` (pitch, fixed) | Pitch | Rate 2–8Hz, Amount 0–10, mode {On,Dly,Whl,A.T.,Ped}, Dly time {0.2,0.7,1.2,2.0}. |
| Arpeggiator / Gate | `Arpeggiator` | note trigger / amp gate | modes {Arp,Poly,Gate}, Rate (BPM, mst-clk), Range 0–4oct, Direction, Pattern (len 2–16, accent, pan). |
| Unison | `Unison` | source voices | {Off,1,2,3} detune/width; extra osc count. |
| Morph routing | `ModMatrix` | any morphable param | 3 sources (Wheel, A.T., Ctrl Ped) → dest table (§6). |

**(NEW) work is therefore not in the type system** — it is (a) writing the
`Native` DSP for these block types in a `daw-builtin-fx`-style core, (b) a
`Sample` `BlockImpl`, and (c) wiring control-rate modulation (next section).

---

## 4. Modulation: the missing runtime piece

The guitar rig's chain is **audio-only** — block N's audio out feeds block N+1's
audio in. NS4 needs **control-rate modulation**: a block's parameter is driven by
another block's output (an Envelope drives the Amp's gain; an LFO drives the
Filter's cutoff; a Morph drives many params at once).

Signal models this with the **`ModMatrix` block** plus per-connection routes:

```
ModMatrix routes (per Synth layer, typical):
  AmpEnv     ──▶ Amp.gain            (always, full)
  FilterEnv  ──▶ Filter.freq         (× Filter.EnvAmt, velocity-scaled)
  OscEnv     ──▶ Osc.ctrl | Osc.pitch (× bipolar amount, velocity-scaled)
  LFO        ──▶ Osc.ctrl | Filter.freq | Osc.pitch   (× LFO.amount)
  Vibrato    ──▶ Osc.pitch           (× Vibrato.amount)
  Velocity   ──▶ Amp.gain / Filter.freq   (per Velocity setting)
  KbdTrack   ──▶ Filter.freq          (Off/⅓/⅔/1)
  Morph[W]   ──▶ {Level, OscCtrl, Filter.freq, …}   (start→end, can invert)
  Morph[AT]  ──▶ {…}
  Morph[Ped] ──▶ {…}
```

Each route = `(source, dest_block, dest_param, depth, [velocity], [bipolar])`.
A **Morph** is a route whose source is one of the three performance controllers and
whose depth describes the start→end span (one source may move one param up while
moving another down — the NS4 inverse-range crossfade, p15/37).

This control-rate layer is the one genuinely new runtime capability `just keys`
needs beyond what the guitar rig has. It belongs in the processing core (the
node graph already exists — see `daw-audio-graph`); modulation is a control-rate
edge type alongside the audio edges.

---

## 5. Per-engine composition

### 5.1 Organ Engine (2 layers, shared FX) — pp18–22

```
Organ Layer A ─┐
               ├─▶  [Tonewheel source]            (model, 9 drawbars, V/C, percussion*)
Organ Layer B ─┘        *percussion: B3 only
   (summed)  ─▶  shared Organ FX modules (§6)  ─▶  Rotary  ─▶ out
```

- **Source block** `Tonewheel`: params `model {B3,B3Bass,Vox,Farfisa,Pipe1,Pipe2}`,
  `drawbar[1..9]`, `vc_type {V1,V2,V3,C1,C2,C3}`, `vc_on`, percussion `{on, harmonic
  2nd/3rd, level Normal/Soft, decay Slow/Fast, poly}`, `preset_mode` (preset vs
  drawbar-live), `swell`. Engine-level menu: `tonewheel_mode {Clean,V1,V2}`,
  `click_level {Normal,High}`, `trigger_point {High,Low}`.
- **Both layers share ONE FX chain** — the §6 FX modules live at the **engine**
  level here (above both layers), fed by the A+B sum. This is the case the flexible
  tree exists for: an engine holding Layers *and* the Modules above them. (Only
  engine where layers don't have private FX.)
- Organ routes to **Rotary** by default (organ's signature pairing).

### 5.2 Keys / Piano Engine (2 layers, private FX) — pp23–26

```
Keys Layer A ─▶ [Sampler source] → Piano params → FX modules (§6) → out
Keys Layer B ─▶ [Sampler source] → Piano params → FX modules (§6) → out
```

- **Source block** `Sampler` (impl `Sample`, Nord Piano Library): `type
  {Grand,Upright,Electric,Clav,Digital,Misc}`, `model`, `size {Sml,Med,Lrg,XL}`.
- **Piano params** (block params, no separate audio block needed for most):
  `kb_touch {Heavy,Med,Light}`, `dyn_comp {Off,1,2,3}`, `timbre` (per category:
  acoustic {Soft,Mid,Bright}; EP {Soft,Mid,Bright,Dyno1,Dyno2}; Clav {7 EQ combos}),
  `clav_pickup {A,B,C,D}`, `soft_release {on}`, `string_res {on}`, `ped_noise {on}`,
  `unison {Off,1,2,3}`. Global trims (engine menu): pedal-noise & string-res level
  ±6 dB.
- **String Resonance / Pedal Noise** are sample-engine features (extra sample
  layers + dynamic level), realized inside the `Sample` impl, gated by `size`.
- Each Keys layer has its **own** sequence of FX modules (§6).

### 5.3 Synth Engine (3 layers, private FX) — pp27–36

A Synth layer is the fullest case — a chain of voice Modules, then the FX Modules
of §6:

```
Synth Layer X = [ Osc Module → Filter Module → Amp Module ]      (voice)
              → [ Mod1 Mod → Mod2 Mod → Delay Mod → Amp/EQ Mod → Comp blk → Reverb Mod ]  (§6)
              → fader → bus
   voice control edges (ModMatrix):
     OscEnv,Vibrato → Osc.ctrl/pitch | FilterEnv,LFO,KbdTrack → Filter.freq | AmpEnv,Velocity → Amp.gain
     Arp/Gate → note events into the voice
```

- **Osc Module** = one source block (`Oscillator` | `FmOperator` | `Wavetable` |
  `Sampler`) + `Unison` + the Osc `Envelope` + the `Lfo`(Vibrato). Param `osc_ctrl`
  (the single morphable shaper), plus type-specific (FM Partial/Pitch, sample
  Bright/dynamics).
- **Filter Module** = `Filter` block {LP24,LP12,LPM,LP+HP,HP,BP; Freq,Res,EnvAmt,
  Drive,KbdTrack} + the Filter `Envelope` (+ the shared `Lfo` when its dest = filter).
  (LP+HP: Res knob = HP cutoff.)
- **Amp Module** = `Amp` block (final gain) + the Amp `Envelope` + Velocity.
- **Voice mode** (layer param): Poly | Mono | Legato; note priority Lo/Hi; `glide`
  (Legato/Mono only). The `Arpeggiator` block gates note events into the layer.
- Each Synth layer then has its **own** sequence of FX modules (§6).

---

## 6. Layer FX — a sequence of Modules (not one module)

The per-layer effects are **not** one "FX module" — each effect section is its own
**Module**, because each is a *family* of swappable blocks (and several contain
multiple blocks). A Module that holds a single fixed block collapses to a Block;
only `Compressor` does. So a layer's FX tail is an ordered sequence of modules +
one block (p51/p52):

```
[in] → Mod1 Module → Mod2 Module → Delay Module → Amp/EQ Module → Comp (Block) → Reverb Module
     → (TO ROTARY send) ──────────────────────────────────────────────────────────────────┐
[layer audio also continues to the layer fader → bus]                                      │
                                                            Rotary (1 global Block, LAST) ◀─┘
```

| FX stage | Module / Block | Why | Holds |
|---|---|---|---|
| **Mod 1** | Module | a family — one mode active at a time | `Panner` \| `Trem` \| `RingModulator` \| `Wah`(A-Wah) \| `Wah` \| `Pump`, + Rate/Amount/Variation |
| **Mod 2** | Module | a family | `Phaser` \| `Flanger` \| `Vibrato`(Vibe) \| `Chorus`(+Ensemble/Spin), + Variation |
| **Delay** | Module | several blocks at once | `Delay` block **+** feedback-FX block {Ens,Vibe,Chor,Flam,Space} **+** feedback `Filter` {LP,HP,BP} |
| **Amp/EQ** | Module | several blocks at once | `Amp`(model) **+** `Eq`(3-band) **+** `Filter`(LP24\|HP24) **+** Drive **+** To-Rotary send |
| **Comp** | **Block** | one fixed block | `Compressor` {amount, fast} |
| **Reverb** | Module | swappable algorithm + modes | reverb-algo block {Spring,Booth,Room,Stage,Hall,Cath} + Variation/Chorale + Bright/Dark |

- **6 instances**: Keys A, Keys B, Synth A, Synth B, Synth C, Organ(A+B shared). (p47)
- **Global mode** (Delay, Compressor, Reverb only): collapses the 6 per-layer
  instances into ONE shared instance for that stage. Model as a per-stage
  `scope {PerLayer, Global}` flag. (p47)
- **Group mode** (Piano & Synth): all layers in a section share the *same settings*
  (still separate instances). Model as a `group {on}` flag per section/stage. (p47)
- **Rotary** is a single global **Block**, always last; fed by per-layer `to_rotary`
  sends and the Organ routing; its Drive scales with the feeding layer's level. (p52)

### Composition is a flexible tree (important)

The hierarchy `Block → Module → Layer → Engine → Preset` is the *vocabulary of
levels*, **not** a rigid "each level only holds the one below it." Any container
holds **any lower-level item plus leaf blocks**:

- a **Module** holds Blocks *and* sub-Modules (e.g. the Delay module holds a Delay
  block + a feedback-FX sub-module + a filter block);
- a **Layer** holds Modules *and* standalone Blocks (its Synth voice modules + the
  bare `Compressor` block + the FX modules);
- an **Engine** holds Layers *and* Modules *and* Blocks (e.g. the Organ engine holds
  its 2 Layers **plus** the one shared Organ-FX modules that sit above both layers);
- a **Preset** holds Engines *and* the global Blocks (the single `Rotary`, any
  global Delay/Comp/Reverb instances) that live above all engines.

So the runtime node model is a heterogeneous tree: every node is either a leaf
**Block** or a container holding an ordered mix of child containers and blocks.
`Module`/`Layer`/`Engine` are *roles* a container plays, not separate rigid types —
they label intent (an FX family, a voice lane, an instrument part) and where shared
vs per-child processing sits. The shared Organ FX and the global Rotary/Delay are
exactly the case the strict hierarchy couldn't express: processing that lives at the
**engine** or **preset** level, above the layers.

### Master Clock (one tempo → 4 sync targets) — pp38–39

`Arp/Gate rate`, `Synth LFO rate`, `Delay time`, `Mod1 rate` each sync to one
program tempo with an independent subdivision. External MIDI-clock auto-lock. Model
as a rig-level `master_clock {bpm, ext_sync, kbs}` + per-target `sync {off | div}`.

---

## 7. Morphs & Layer Scenes (performance layer)

- **Morphs** = 3 sources (`Wheel`, `A.T.`/channel-pressure, `CtrlPed`) wired through
  the `ModMatrix`. Destination table (p38), per source:

  | Organ | Keys | Synth | Effects |
  |---|---|---|---|
  | Layer Level | Layer Level | Layer Level, LFO Rate/Amount, Osc Ctrl, Filter Freq/Res, Arp/Gate Rate | Mod1 Rate/Amt, Mod2 Amt, Delay Tempo/Feedback/DryWet, EQ Mid / Filter Freq, Drive Amt, Reverb DryWet |
  | Drawbars | | | |
  | Rotary Speed | | | |

  A morph route stores `(start_value, end_value)` so the controller sweeps the span;
  ranges may invert for crossfades.

- **Layer Scenes I/II** = two snapshots that differ **only** in per-layer/section
  on/off state — *not* a parameter snapshot (p42). Model as two boolean mute masks
  over the 7 layers + 3 sections; everything else is shared. This is exactly a
  Signal **Preset Snapshot** restricted to the "which members are enabled" axis.

- **Splits / Zones**: up to 4 keyboard zones via 3 split points (Low/Mid/High,
  positions C2–C7), per-split `xFade {–, ±6, ±12}`. Each layer carries a 4-bit zone
  mask + octave shift. (p38)

---

## 8. What Signal must build (gap list)

Ordered by dependency:

1. **Engine/Layer/Module composition in the rig.** `signal-sampler` today plays a
   flat `RigBlock` chain. `just keys` needs the real `Engine → Layer → Module →
   Block` tree, summed polyphonically (7 layers). The proto types for this exist
   (`engine`, `layer`, `module`); the *audio-side runtime* must honor them.
2. **`Sample` `BlockImpl`** — a sample-player backend (Nord Piano/Sample Library
   analog) for `Sampler`-typed blocks. `signal-sampler`'s `SampleEngine` is most of
   this already; wrap it as a block implementation.
3. **Native DSP core (`daw-builtin-fx`)** — the `BlockImpl::Native` backends for:
   `Oscillator`, `FmOperator`, `Wavetable`, `Noise`, `Filter`, `Amp`, `Eq`,
   `Compressor`, `Delay`, `Reverb`, `Rotary`, `Chorus`/`Phaser`/`Flanger`/`Trem`/
   `Panner`/`RingModulator`/`Wah`, and the `Tonewheel` organ model. (Until written,
   these blocks report `has_backend() == false` — same gate the guitar rig uses.)
4. **Control-rate modulation** — `Envelope`, `Lfo`, `Arpeggiator`, `Unison` as
   control blocks + a `ModMatrix` of `(source → block.param)` routes, evaluated per
   block in the processing core (a new edge type in `daw-audio-graph`).
5. **Performance layer** — 3 morph macros, 2 layer-scene mute masks, 4-zone splits,
   master clock. Mostly state + routing on top of (4).

None of this requires new `BlockType`s. The two-axis block model (`block_type ×
BlockImpl`, with per-type `BlockImpl::allowed_for`) already accommodates every NS4
element; e.g. a `Filter` block may be `Native` (our DSP) or `Plugin` (an external
filter), an `Amp` may be `Native`/`Nam`/`Plugin`.

---

## 9. Appendix — full parameter checklists

### A. Organ (per layer A/B unless noted)
model · 9 drawbars (continuous B3/Vox/Pipe; on-off Farfisa; 16'/8' only B3Bass; Vox
drawbar-8 = filter mix) · level (morph) · octave shift · KB zone · pitch-stick on ·
sustain on · V/C type + on (per-layer B3; shared Vox/Farf) · percussion {on, 2nd/3rd,
Normal/Soft, Slow/Fast, poly} (B3) · preset/drawbar-live · swell.
Engine menu: tonewheel mode {Clean,V1,V2} · click {Normal,High} · trigger {High,Low}.

### B. Keys/Piano (per layer A/B)
type · model · size · level (morph) · octave · KB zone · pitch-stick · sustain ·
kb_touch {Heavy,Med,Light} · dyn_comp {Off,1,2,3} · timbre (per category) ·
clav_pickup {A,B,C,D} · soft_release · string_res · ped_noise · unison {Off,1,2,3}.
Engine menu: ped-noise level ±6dB · string-res level ±6dB.

### C. Synth (per layer A/B/C)
mode {Samples,Analog} · osc type {Analog{Pure,SubOsc,Sync,Shape,ShapeSine,Multi,
Super,Misc}, FM-H{A–E}, FM-I{A–E}, Wave} or sample · osc_ctrl (morph) · FM
Partial(0.5–24)/Pitch(−12…48) · sample {bright, dynamics Natural/NoDyn/FastAtk} ·
filter {type, freq(morph), res(morph), env_amt(morph), drive{Off,1,2,3},
kbd_track{Off,⅓,⅔,1}} · OscEnv/FilterEnv/AmpEnv {A,D,R, velocity} · osc_env amount
(bipolar) + dest {OscCtrl,Pitch} · LFO {rate(morph), amt(morph), wave{Tri,Saw↓,Saw↑,
Sq,S&H}, dest{OscCtrl,Pitch,Filter}, mst-clk} · vibrato {mode,rate 2–8,amt 0–10,dly} ·
voice {Poly,Mono,Legato} + priority{Lo,Hi} · glide · unison {Off,1,2,3} · arp {mode
{Arp,Poly,Gate}, rate(BPM,mst-clk), range 0–4oct, direction{Up,Down,UpDown,Rnd},
pattern{len 2–16, accent, pan, zigzag}} · kb hold/sync · level (morph).

### D. Per-layer FX (×6 instances; Organ shared)
Mod1 {mode{A-Pan,Trem,RM,A-Wah,Wah,Pump}, rate(morph), amount(morph), variation/ped,
mst-clk} · Mod2 {mode{Phaser,Flanger,Vibe,Chorus,Ens,Spin}, rate(morph),
amount(morph), variation} · Delay {tempo(morph), feedback(morph), drywet(morph),
pingpong, effects{Ens,Vibe,Chor,Flam,Space}, filter{LP,HP,BP}, analog, global,
mst-clk} · AmpSim/EQ {drive(morph), amp{Twin,JC,Small}+var, eq bass/mid(freq
morph)/treble ±15dB, lp24{freq morph,res}, hp24{freq morph,res}, to_rotary} ·
Compressor {amount, fast, global} · Reverb {type{Spring,Booth,Room,Stage,Hall,Cath},
variation/chorale, bright/dark, drywet(morph), global}.

### E. Global / program
Rotary {speed Slow/Fast/Stop, drive, angle, close-mic, bass/horn 70/30…30/70, rotor/
horn speed+acc Low/Normal/High} · Master Clock {bpm, ext-sync, kbs} · Morphs ×3
{Wheel, A.T., CtrlPed} → dest table · Layer Scenes I/II (mute masks) · Splits {3
points C2–C7, xFade –/±6/±12} + per-layer 4-zone mask · per-program transpose ±6 ·
output bus routing (Main + Sub I/II/III).

---

*Sources: NS4 User Manual v1.4x — Program/architecture pp5–16,37–46,56–60; Organ
pp18–22,57; Piano pp9–10,23–26,57; Synth pp10,27–36; Effects pp47–52; Morphs
pp37–38.*
