# Soundsource — the generator abstraction

One instrument engine for **Synths, Orchestral, Percussion, Cinematic** — they
differ only in *what generates the sound*, not how it's routed, layered,
filtered, enveloped, or modulated. That generator is a **`Soundsource`**.

## The idea

A layer (A/B/C/D…) is a self-contained sub-instance: `Soundsource → Filter →
Amp → FX`, with its own envelopes/LFOs/modulation. The **only** pluggable part
that changes between a synth, an orchestra, a drum kit, and a cinematic texture
is the layer's `Soundsource`. Everything downstream is shared.

```
Layer
├─ Soundsource  ← Oscillator | Sample | Audio   (this trait)
├─ Filter(s)
├─ Amp
├─ FX
└─ modulators (Env, LFO, ModMatrix)
```

## The trait

The generator role, distinct from a **processor** (which transforms an incoming
signal). A `Soundsource` turns note/param events into audio; it ignores audio
input. It is a refinement of today's `PluginInstance` (the general leaf), so a
`Soundsource` can be adapted into the render tree with no new leaf machinery.

```rust
pub enum SoundsourceKind { Oscillator, Sample, Audio }

pub trait Soundsource: Send {
    fn kind(&self) -> SoundsourceKind;
    fn prepare(&mut self, sample_rate: f32, block_size: usize);
    /// Note lifecycle (poly voices are the impl's concern).
    fn note_on(&mut self, note: u8, velocity: u8);
    fn note_off(&mut self, note: u8);
    /// Generate one block. `events` carries param writes (mods), pitch bend, etc.
    fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32], events: &PluginEvents<'_>);
    /// Params for the Control/Edit UI + the mod engine.
    fn params(&self) -> Vec<PluginParamInfo>;
    fn set_param(&mut self, id: u32, value: f64);
}
```

`Send` stays (matches the existing `AudioNode: Send` rule). No heap on the hot
path — allocate in `prepare`.

## The three implementations

| Kind | Wraps today | Notes |
|---|---|---|
| **Oscillator** | `NativeOscillator` (`native_osc.rs`), `NativeWavetable` (`native/wavetable.rs`) | Analog/wavetable synthesis. Unison, FM, ring, harmonia live here. |
| **Sample** | `SampleEngine` (packs / `library.styx`, zone maps, RR, mics, loops) | Keyscape pianos, **Omnisphere soundsources**, drum kits, orchestral multisamples. Multi-mic is a Sample-soundsource feature. |
| **Audio** | *(new)* | Straight audio file / input streaming — cinematic beds, one-shots, granular fodder, live audio into the layer. |

**Naming note:** today "soundsource" means an Omnisphere extracted multisample.
That is simply an instance of the **Sample** `Soundsource` — the terminology now
generalizes cleanly (a soundsource is any generator; a Sample soundsource is a
sampled one).

## Relationship to the existing axes

- `BlockType` (semantic) / `BlockImpl` (Native/Sample/Nam/Ir/Plugin) stay for the
  render tree. `Soundsource` is the **generator view** over the source block:
  the layer's source leaf is-a `Soundsource`. `SoundsourceKind::Oscillator`
  corresponds to `BlockType::{Oscillator,Wavetable} + BlockImpl::Native`;
  `Sample` to `BlockType::Sampler + BlockImpl::Sample`; `Audio` is new.
- Processors (Filter, Amp, FX) are **not** Soundsources — they keep the plain
  `PluginInstance` leaf role. Only the *source* slot is a `Soundsource`.

## Migration (additive, safe on the live engine)

The shared engine drives every rig on the production machine, so this lands
incrementally, never as a big-bang rewrite:

1. ~~**Define** `Soundsource` + `SoundsourceKind` (this doc).~~ DONE.
2. ~~**Adapt** the existing generators onto the trait.~~ DONE — and then
   **inverted**: `NativeOscillator`, `NativeWavetable`, and the `SampleEngine`
   (via `SamplerInstrument`) now implement `Soundsource` **natively** as their
   primary trait; none of them implements `PluginInstance` directly anymore.
   The single generic `SoundsourceLeaf` adapter is the only bridge into the
   render tree's `PluginInstance` leaf (generators keep their established
   descriptor ids — `signal.native.oscillator`, `signal.native.wavetable`,
   `signal.sampler.instrument` — via `Soundsource::descriptor`).
3. ~~**Audio soundsource**~~ DONE — `AudioSoundsource` (the guitar-DI /
   input-passthrough case) was the first native `Soundsource`.
4. ~~**Expose** `SoundsourceKind` over the proto~~ DONE — `SoundsourceKind`
   now lives in `signal-proto` (`block_kind.rs`, beside `BlockKind`; this
   crate re-exports it), with `BlockType::soundsource_kind()` classifying
   generator block types for remotes and `SynthLayer.source_kind` carrying
   the tag on the synth wire. Per-kind param editing remains UI work.
5. ~~`node_render` source leaves hold `Box<dyn Soundsource>` directly~~
   DONE — a compiled leaf is a `LeafBackend`: `Source(Box<dyn Soundsource>)`
   for generators (no adapter round-trip inside the tree; see
   `RenderNode::source_kinds`) or `Plugin(Box<dyn PluginInstance>)` for
   processors/hosted plugins. `SoundsourceLeaf` remains only at graph
   boundaries that need a true `PluginInstance` (`build_block`'s FX-chain
   callers, `build_node_backend`).

PhysicalModel is a real kind: **City Wurli** (`NativeWurli` over the vendored
`openwurli-dsp` engine) implements `Soundsource` natively with
`SoundsourceKind::PhysicalModel`; the trait carries the sample-excited /
hybrid hook (`supports_sample_excitation` / `excite`,
r[signal.soundsource.physical.hybrid]) with a no-op default. The City Grand
waveguide/modal engines still enter the tree as plain `PluginInstance`
blocks.

Deferred: r[signal.soundsource.module.*] gaps are tracked as Task issues
(source-module framework + Wave, Analog/Drift, Unison, Harmony, FM/Ring,
Waveshaper-as-module, Granular) under the sampler pillar.

Every step keeps the current `PluginInstance` path working — the live keys /
drums / guitar / synth rigs never break mid-migration.
