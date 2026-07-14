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

1. **Define** `Soundsource` + `SoundsourceKind` (this doc). No behavior change.
2. **Adapt**: blanket-impl / newtype so the existing generators
   (`NativeOscillator`, `NativeWavetable`, `SampleEngine`) satisfy `Soundsource`
   — they already have `note_on`/`process_block`/`params`. A `SoundsourceLeaf`
   adapter bridges `Soundsource` ↔ the render tree's `PluginInstance` so nothing
   in `node_render` changes.
3. **Audio soundsource**: implement the new one (file/stream player) as the first
   *native* `Soundsource` (proves the trait carries a non-sampler, non-osc gen).
4. **Expose** `SoundsourceKind` + params over the synth proto so the Control/Edit
   UI shows/edits the layer's source generically (source picker, per-kind params).
5. Only once (2)–(4) are proven, consider making `node_render` source leaves hold
   `Box<dyn Soundsource>` directly.

Every step keeps the current `PluginInstance` path working — the live keys /
drums / guitar / synth rigs never break mid-migration.
