# Keys Rig / Composition Engine — Roadmap

Continued work for the Nord-style **keys rig** (`just keys`) and the composition
engine it runs on. Companion to `docs/nord-stage-4-signal-routing.md` (the
full NS4 → Signal mapping). This file tracks **what's built** and **what's left**.

---

## Where we are

The composition engine + a playable rig exist; the routing is the contract and
it's solid. What works today:

- **Composition tree** (`signal-sampler::rig_node`) — `RigNode` = leaf `Block` |
  `Container`. A Container is an infinitely-nestable "block folder" with a `Role`
  (Preset/Engine/Layer/Module), a `Combine` (Serial chains / Parallel sums),
  `input_db`/`output_db` volume, `params`, `modulators`, `sends`, and a routing
  `Zone`. Facet-derived → round-trips through styx.
- **Two axes** — containment tree + routing graph (sends + modulators), per the
  design doc.
- **Keyboard layering** (`Zone`) — per-container key split + velocity window with
  crossfade edges (Nord splits + Omnisphere velocity crossfades). The renderer's
  `RenderNode::Zoned` filters/scales the central MIDI stream per zone; nested
  zones multiply. This is the central-MIDI-input router.
- **Tree-render runtime** (`node_render`) — compiles a Container to a `RenderNode`
  and renders audio (Serial chain / Parallel sum / Zoned filter / Leaf backend or
  pass-through).
- **First Native DSP** — a polyphonic `NativeOscillator`. `has_backend()` consults
  `native_dsp_available(block_type)` (currently `{Oscillator}`); the registry
  grows one block type at a time.
- **Preset registry** (`preset_registry`) — register presets from **code**
  (`nord_stage_preset()`, `layering_demo()`) and from **`.styx`** files; styx can
  override a builtin.
- **Live host** (`keys_rig`) — `KeysInstrument` wraps a `RenderNode` as a daw
  `PluginInstance`; `KeysRig` hosts it on an output-only daw project, routes MIDI
  via `push_live_midi`, swaps presets glitch-free.
- **`just keys`** — Nord Stage-styled TUI: red header, output meter,
  sections/splits panel, velocity-layers panel, keyboard zone strip with held
  notes, hardware MIDI (all/named/virtual, hot-switch with `i`) + computer keyboard.

The full Nord Stage 4 program is built out as a placeholder tree
(`nord_stage_preset()`): 3 engines (Organ 2L shared-FX, Keys 2L, Synth 3L), the
6-stage per-layer FX, modulators, To-Rotary sends, global Rotary. Only the synth
`Oscillator` makes sound today; everything else is a structural placeholder.

---

## What's left

Ordered roughly by leverage. None of it requires routing/type changes — the
contract is fixed; this is filling it in.

### 1. Native DSP, one block type at a time  *(biggest)*
Each implemented type flips `has_backend()` true and renders in place. The loop:
write the DSP → add the type to `native_dsp_available` → add an arm in
`node_render::build_node_backend`. Priority order for a musical synth voice:

1. **`Filter`** — LP24/LP12/LPM/LP+HP/HP/BP; Freq, Res, Drive, KbdTrack.
2. **`Amp`** — gain stage (so the voice has a real amplifier).
3. **`Envelope`** — AD-R (sustain = max decay), velocity. Needed before the voice
   sounds musical (no hard on/off). Drives Amp / Filter / Osc-Ctrl — see §2.
4. **`Sampler`** (a new `BlockImpl::Sample`) — wrap the existing `SampleEngine`
   so the **Keys/Piano** + **Organ-via-samples** layers play. Big unlock.
5. **`Tonewheel`** — the B3/Vox/Farfisa/Pipe organ model (drawbars, percussion,
   V/C). Could start as additive drawbar synthesis.
6. **FX**: `Delay`, `Reverb`, `Chorus`/`Phaser`/`Flanger`/`Trem`, `Eq`,
   `Compressor`, `Rotary`. These are effects (process input), simpler than sources.

Eventually this native-DSP core wants to be its own crate (`daw-builtin-fx`) per
the design doc; for now it lives in `signal-sampler` (`native_osc.rs` → grow a
`native/` module).

### 2. Control-rate modulation (the ModMatrix)
Today `Container.modulators` (Envelope/LFO/Arp per layer) are **listed but not
wired**. Build the control-rate layer:
- `Envelope`/`Lfo`/`Arpeggiator` as control blocks producing a control signal.
- A modulation route `(source → dest_block.param, depth, [velocity], [bipolar])`,
  evaluated per block in the render tree (a new edge kind — see `daw-audio-graph`).
- Wire the NS4 routes: AmpEnv→Amp.gain, FilterEnv→Filter.freq, LFO→Filter/Osc,
  Vibrato→pitch, Velocity→gain, plus the 3 Morph sources → param table (design
  doc §6/§7). This is the one genuinely new runtime capability beyond audio.

### 3. Renderer fidelity (cheap, high-value)
The render tree currently ignores several already-modeled fields:
- **Volume** — apply `Container.input_db` before children and `output_db` after
  (per the volume commit). Pure gain.
- **Bypass** — honor `Container.bypassed` (skip the subtree).
- **Sends** — honor `Container.sends` (To-Rotary, global Delay/Comp/Reverb buses).
  Needs a small bus/mixer stage at the preset level; the targets exist in the tree.
- **Per-layer params** — apply `octave`/transpose, `voice_mode` (poly/mono/legato),
  `unison`, `glide` from `params` (or promote them to typed fields).

### 4. RT-safety
`RenderNode::process` allocates scratch (Vec) **per block** — fine for tests/
bring-up, not for the realtime callback under polyphony. Pre-allocate scratch at
`prepare(block_size)` (each Serial/Parallel/Zoned node owns reusable buffers).

### 5. Velocity-crossfade refinement
`Zone` currently scales **velocity** by the crossfade gain (works because the
oscillator maps velocity→amp). For sample/DSP layers where velocity also changes
timbre, switch to a true per-note **amplitude** crossfade (carry the gain to the
voice as a separate amp scalar) so a crossfade blends loudness, not articulation.

### 6. keys_tui polish (toward full Nord panel fidelity)
- Re-collect colors/zones on **preset swap** (Tab) instead of only at launch.
- Show the **FX modules** + **synth params** (osc/filter/env) of the focused
  layer — the right-hand "LAYER EFFECTS" + center display of the real panel.
- Drawbar bars for Organ layers; type/model for Keys; OSC/FILTER/AMP for Synth.
- Focus + edit (select a layer/param, nudge values) — turning the viewer into a
  controller. Optionally drive it from the real panel layout in the image.

### 7. Persistence + content
- A **keys library** dir (`presets/*.styx`) + `just keys --library <dir>`, mirroring
  the guitar library. Save the current tree to styx from the TUI.
- Real **worship** content is still pending from the guitar side: the songlist
  and the patch→amp mappings (see the guitar library section). Unrelated to keys
  but on the overall list.

### 8. Performance layer (NS4 parity, later)
Master Clock (one tempo → Arp/LFO/Delay/Mod1), Layer Scenes (two mute masks),
the 3 Morph macros wired through the ModMatrix, full split/zone UI. Mostly state +
routing on top of §2.

---

## Notes / caveats

- **Parallel work:** the strings/CSS effort edits `engine/mod.rs`, `bank.rs`,
  `block.rs`, `engine/voice.rs`, `sampler_rig.rs` in the same crate. Those files
  (and binary test artifacts like `css_test.mid` / `*.wav`) are **not** part of
  the keys work — leave them to that effort; the `*.wav`/`*.mid` artifacts should
  be gitignored, not committed.
- **MIDI input** is the same `daw-midi-io` path as `just strings`
  (`attach_midi` → `push_live_midi` → renderer → instrument), now hot-switchable.
- **The incremental loop** (DSP per block type) is the whole game now — the
  routing, layering, levels, registry, host and TUI are done and won't change as
  blocks light up.

*Branch: `feat/guitar-library-stacks`. See `docs/nord-stage-4-signal-routing.md`
for the full parameter spec.*
