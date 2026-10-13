# Signal Synth Engine — Runtime TODO List

Tracks the work needed to make the synth Block scaffold (see
`crates/signal-proto/src/synth_blocks.rs`) actually produce audio.

The scaffold lands the type system, parameter schemas, and `todo!()`
runtime stubs. This doc tracks **what each `todo!()` actually needs** so
the work can be picked up incrementally — likely by spawning subagents
per Block once the design intent is solid.

Status legend: 🔲 not started · ⬜ partial · ☑️ usable · ✅ shippable

## Tier 1 — Minimum viable Spectrasonics playback

These six Blocks together let us play any sample-based Keyscape patch.
Implement first; everything else is gravy.

### 🔲 `SamplerBlock` — bridges into existing `signal-sampler` engine

- `crates/signal-proto/src/synth_blocks.rs::process_sampler`
- **What it does**: looks up a sample by `(library_ref, soundsource, note,
  velocity)` and starts a voice. Bridge from Block-graph world into the
  existing zone-mode `SampleEngine`.
- **References**:
  - Existing: `crates/signal-sampler/src/engine/mod.rs::note_on_zoned`
    (zone-mode trigger logic — wraps PlayerPatch + cache + voice pool).
  - SFZ `sfizz` ([github.com/sfztools/sfizz](https://github.com/sfztools/sfizz))
    — clean reference for opcodes → playback voice mapping.
  - DecentSampler — JSON-defined sampler with similar zone semantics
    ([decentsamples.com](https://www.decentsamples.com/decent-sampler/)).
- **Notes**: The hardest part is the cross-crate handoff. signal-sampler's
  engine owns its own voice pool; the Block-graph runtime needs a way to
  ask it to spawn a voice tied to this Block's modulation outputs.

### 🔲 `EnvelopeBlock` — sample-accurate ADSR with curve shaping

- `crates/signal-proto/src/synth_blocks.rs::process_envelope`
- **What it does**: per-voice envelope state machine; outputs a control-rate
  signal per audio block, sample-accurate at note-on/off boundaries.
- **References**:
  - Surge XT ([surge-synthesizer.github.io](https://surge-synthesizer.github.io/))
    — `Tunings/SurgeStorage.h` AHDLR envelope is a strong reference for the
    same shape with curves per segment.
  - Vital ([vital.audio](https://vital.audio/)) — multi-segment env editor
    demonstrates loop regions / curve shaping UX.
- **Notes**: Spectrasonics uses curve shape `[-1..1]` per segment (negative
  = exponential, positive = log). Make sure the curve math matches at the
  boundaries (zero curve = linear, ±1 = strong exp/log).

### 🔲 `LfoBlock` — phase accumulator + waveform LUT + sync clock

- `crates/signal-proto/src/synth_blocks.rs::process_lfo`
- **What it does**: free-running or tempo-synced phase accumulator;
  selectable waveform; reset on note-on; outputs uni- or bipolar.
- **References**:
  - Surge XT modulators (LFO1/2 + scenes).
  - Vital LFO editor (custom point-based shapes — likely Tier 2).

### 🔲 `OscillatorBlock` — basic VA (saw/square/tri/sine/noise)

- `crates/signal-proto/src/synth_blocks.rs::process_oscillator`
- **What it does**: anti-aliased classic-shape oscillator. PolyBLEP for
  saw/square; sine direct; noise via PCG/xorshift + filter.
- **References**:
  - Surge XT classic oscillators (`src/common/dsp/oscillators/`).
  - Mutable Instruments Plaits (open-source modular DSP, well documented):
    [github.com/pichenettes/eurorack/tree/master/plaits](https://github.com/pichenettes/eurorack/tree/master/plaits).
  - DPW / PolyBLEP techniques: see [martin-finke.de](https://martin-finke.de/articles/audio-plugins-018-polyblep-oscillator/).
- **Tier 2 work on this same Block**: wavetable mode, FM, AM, hard-sync,
  granular, mogrify. Tier 1 is just the classic shapes.

### 🔲 `FilterBlock` — multi-mode (LP/HP/BP/notch) with envelope mod

- Currently lands under existing `BlockType::Filter` (`Special` category).
- **References**:
  - Surge XT filter algorithms (`src/common/dsp/filters/`) — covers SVF,
    Moog ladder, comb, vocal, etc. Spectrasonics has ~30 algos; Surge has
    a similar count.
  - Vital filter pages.
  - Will Pirkle's *Designing Software Synthesizer Plug-Ins in C++* — chapter
    on TPT (topology-preserving transform) filters.
- **Notes**: Spectrasonics filters store an algo `name` string; we need a
  string → enum table.

### 🔲 `ModMatrixBlock` — wires sources to targets

- `crates/signal-proto/src/synth_blocks.rs` (no `process_*` — mod matrix
  is wiring, not a DSP step).
- **What it does**: at audio start, resolve each `ModMatrixRow.target`
  string to a parameter pointer; at audio time, sum source values × amount
  into target.
- **References**:
  - `crates/macromod/` — Signal already has a similar concept at the Layer
    / Engine level; this Block extends that to per-Module / per-Voice.

## Tier 2 — Omnisphere parity

After Tier 1: ~80% of Omnisphere patches playable.

- 🔲 `WavetableBlock::process_wavetable` — read `.stmwf` / Serum WAV +
  morph between frames. Vital is the canonical reference.
- 🔲 `HarmonicBlock::process_harmonic` — additive bin generators. Look at
  Synfig / [HarmonicAudioSynthesis](https://github.com/sevagh/Real-Time-HPSS).
- 🔲 `WaveshaperBlock::process_waveshaper` — selectable nonlinearity +
  tone EQ + mix. JUCE's `WaveShaper` class is a basic reference.
- 🔲 `UnisonBlock::process_unison` — multi-voice spread/detune/drift.
  Surge XT's Vintage Oscillator demonstrates analog-style drift.
- 🔲 `MultisegEnvelopeBlock::process_multiseg_envelope` — n-stage with
  loop region. See Vital's mod-source envelope editor.
- 🔲 `StepSeqBlock::process_step_seq` — clock-driven step advance,
  emits note + slice events. Reference: Renoise pattern editor model.
- 🔲 `ArpBlock::process_arpeggiator` — held-note tracker + groove
  application. MIDIPolyphonicExpression spec covers note state; groove
  patterns are MIDI clock + swing math.
- 🔲 `Eq12Block` (extension to existing `Eq` block) — 12-band parametric
  with selectable filter type per band. Reference: Equalizer APO source.

## Tier 3 — Full feature parity (exotic)

- 🔲 `GranularBlock::process_granular` — overlap-add scheduler + grain
  envelope (Hann / Tukey window). References: Mutable Instruments Beads,
  PaulStretch.
- 🔲 `FmOperatorBlock::process_fm_operator` — sine + envelope + ratio +
  feedback. Reference: Dexed (open-source DX7) source code.
  ([github.com/asb2m10/dexed](https://github.com/asb2m10/dexed)).
- 🔲 `TonewheelBlock::process_tonewheel` — 91-oscillator additive bank +
  drawbar mixing + leakage + scanner vibrato. References: setBfree
  ([github.com/pantherb/setBfree](https://github.com/pantherb/setBfree)),
  Bristol's Hammond model.
- 🔲 `FormantBlock::process_formant` — bandpass resonator bank with vowel
  interpolation. References: VocaTone, Soundblaster's vox emulation.
- 🔲 `DfsBlock::process_dfs` — Spectrasonics-specific Dynamic FM Synthesis.
  **Format unknown — needs RE on `<DFS>` tag** by inspecting individual
  patches that use it.
- 🔲 `NoiseBlock::process_noise` — colored noise filter chain (white →
  pink via 1-pole IIR, brown via integration, blue/violet via
  differentiation).

## Cross-cutting infrastructure

These show up in every Block runtime; build once, reuse everywhere.

- 🔲 **Voice pool / polyphony manager** — shared across all Blocks in a
  Module. Existing `signal-sampler::engine::voice::VoicePool` is the
  starting point; needs to become Block-graph-aware (Voice = a
  through-running graph instance, not just a single sample player).
- 🔲 **Block-graph audio scheduler** — given a Module's `SignalChain`
  topology, schedule Block `process_*` calls in dependency order with the
  right buffer plumbing. JUCE's `AudioProcessorGraph` is the reference;
  CMajor's stream graph is another.
- 🔲 **Mod-source resolution** — at preset load, each `ModMatrixRow` and
  each `MidiLearnBinding` resolves a string path to a typed
  parameter handle. URL-style paths recommended: `module/<id>/block/<id>/param/<name>`.
- 🔲 **Patch importer** — `.prt_omn` / `.prt_key` / `.prt_trl` SynthMaster
  XML → `Module` instance. Mechanical mapping driven by the corpus
  parameter inventory.

## How to pick up a task

1. Find the Block in `synth_blocks.rs`.
2. Look at its `default_params()` and the corpus reference in the doc
   comment.
3. Pick a reference implementation from the list above.
4. Implement the runtime in `signal-synth-blocks` (new crate) or wherever
   the Block-graph scheduler lives.
5. Replace the `todo!()` with the call into your runtime.
6. Add a unit test that produces a known buffer for a known param set.

## Open architectural questions to resolve before implementing

These shape every Block runtime — answer first to avoid rework.

1. **Block buffers**: stereo interleaved or split L/R? Audio rate vs
   control rate?
2. **Voice management granularity**: per-Block voice (Surge XT model) or
   per-Module voice (Falcon / Kontakt model)? The latter is simpler;
   the former lets exotic blocks have their own voice management.
3. **Modulation rate**: control-rate (one value per audio block) or
   sample-rate (one value per sample)? Sample-rate is more flexible but
   more expensive; most synths run mod sources at control rate.
4. **Block hot-reload**: can you swap a Block instance mid-playback
   without note dropouts? If yes, all Block runtimes need state-transfer
   semantics.
