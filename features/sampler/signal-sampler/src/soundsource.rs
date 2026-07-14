//! The **`Soundsource`** — the pluggable *generator* inside an instrument layer.
//!
//! One engine drives Synths, Orchestral, Percussion, and Cinematic sounds; they
//! differ only in what generates the audio, not how it's filtered, enveloped,
//! layered, or modulated. That generator is a `Soundsource`. See
//! `docs/SOUNDSOURCE.md` for the full design + migration plan.
//!
//! This module defines the abstraction; the existing generators
//! ([`NativeOscillator`](crate::native_osc::NativeOscillator),
//! [`NativeWavetable`], and the [`SampleEngine`](crate::SampleEngine)) are
//! adapted onto it incrementally without disturbing the render tree's
//! [`PluginInstance`](signal_plugin_host::PluginInstance) leaves.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

/// Which kind of generator a [`Soundsource`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundsourceKind {
    /// Analog / wavetable synthesis (unison, FM, ring, harmonia).
    Oscillator,
    /// Sampled multisample playback (zone maps, round-robins, mics, loops) —
    /// Keyscape, **Omnisphere soundsources**, drum kits, orchestral libraries.
    Sample,
    /// Physically-modeled instrument — an excitation (hammer/bow/pluck/breath)
    /// driving a resonant model (strings, body/soundboard). A physically-modeled
    /// piano; may be sample-excited/hybrid. See `docs/spec/signal/soundsource.md`.
    PhysicalModel,
    /// Live audio / file input as the layer's source — the **guitar rig** case
    /// (the DI feeds straight into the layer's filter/amp/FX), plus cinematic
    /// beds, one-shots, and granular fodder.
    Audio,
}

impl SoundsourceKind {
    pub const fn tag(self) -> &'static str {
        match self {
            SoundsourceKind::Oscillator => "oscillator",
            SoundsourceKind::Sample => "sample",
            SoundsourceKind::PhysicalModel => "physical-model",
            SoundsourceKind::Audio => "audio",
        }
    }
}

/// A sound **generator** — turns note/param events into audio, ignoring audio
/// input (the distinction from a *processor*). A refinement of the render
/// tree's general leaf, so a `Soundsource` adapts into the graph with no new
/// leaf machinery.
///
/// Contract (shared with the processing-core rules): `Send`; allocate in
/// [`prepare`](Soundsource::prepare); no heap on the hot path; `render` never
/// blocks or spawns.
pub trait Soundsource: Send {
    /// Which generator this is (for the source picker / per-kind UI).
    fn kind(&self) -> SoundsourceKind;

    /// (Re)allocate for `sample_rate` / `block_size`.
    fn prepare(&mut self, sample_rate: f32, block_size: usize);

    /// Start a voice. Polyphony is the implementation's concern.
    fn note_on(&mut self, note: u8, velocity: u8);
    /// Release a voice.
    fn note_off(&mut self, note: u8);

    /// Generate one block into `out_l`/`out_r`. `events` carries parameter
    /// writes (from the mod engine), pitch bend, and note expressions.
    ///
    /// `in_l`/`in_r` are the layer's input audio: **synthesis** generators
    /// (Oscillator, Sample) ignore it; the **Audio** generator emits it (the
    /// live guitar DI / a file) — matching the render-tree leaf signature so a
    /// `Soundsource` drops in without new plumbing.
    fn render(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    );

    /// Parameters exposed to the Control/Edit UI and the mod engine.
    fn params(&self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    /// Set a parameter by id (matches [`params`](Soundsource::params)).
    fn set_param(&mut self, _id: u32, _value: f64) {}
}

/// Adapts any [`Soundsource`] into the render tree's
/// [`PluginInstance`] leaf, so a generator drops into `node_render` with no
/// new leaf machinery (migration step 2). `process_block` → `render`,
/// `prepare` → `prepare`, `params`/`param_value` → `params`, the param
/// setters → `set_param`.
///
/// Notes still flow through `events.midi`: `render` forwards the block's
/// events, and the wrapped source (oscillator / wavetable / sampler) reads
/// them exactly as it does today. The direct
/// [`note_on`](Soundsource::note_on) / [`note_off`](Soundsource::note_off)
/// entry points are for callers that drive voices out-of-band.
pub struct SoundsourceLeaf {
    source: Box<dyn Soundsource>,
    prepared: bool,
}

impl SoundsourceLeaf {
    /// Wrap a generator as a render-tree leaf.
    pub fn new(source: impl Soundsource + 'static) -> Self {
        Self {
            source: Box::new(source),
            prepared: false,
        }
    }

    /// Wrap an already-boxed generator.
    pub fn from_boxed(source: Box<dyn Soundsource>) -> Self {
        Self {
            source,
            prepared: false,
        }
    }

    /// Which kind of generator this leaf carries.
    pub fn kind(&self) -> SoundsourceKind {
        self.source.kind()
    }
}

impl PluginInstance for SoundsourceLeaf {
    fn descriptor(&self) -> PluginDescriptor {
        let tag = self.source.kind().tag();
        PluginDescriptor {
            id: format!("signal.soundsource.{tag}"),
            name: format!("Soundsource ({tag})"),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        self.source.params()
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.source.params().into_iter().find(|p| p.id == id).map(|p| p.default)
    }

    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }

    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }

    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.source
            .prepare(sample_rate.max(1.0) as f32, block_size as usize);
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        self.source.render(in_l, in_r, out_l, out_r, events);
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_soundsource::AudioSoundsource;

    #[test]
    fn leaf_wraps_audio_soundsource_and_passes_input_through() {
        let mut leaf = SoundsourceLeaf::new(AudioSoundsource::new());
        assert_eq!(leaf.kind(), SoundsourceKind::Audio);
        leaf.prepare(48_000.0, 512).unwrap();
        assert!(leaf.is_prepared());

        let n = 512;
        let in_l = vec![0.5f32; n];
        let in_r = vec![-0.25f32; n];
        let mut out_l = vec![0.0f32; n];
        let mut out_r = vec![0.0f32; n];
        leaf.process_block(&in_l, &in_r, &mut out_l, &mut out_r, &PluginEvents::default())
            .unwrap();
        assert!(out_l.iter().all(|&s| (s - 0.5).abs() < 1e-6));
        assert!(out_r.iter().all(|&s| (s + 0.25).abs() < 1e-6));
    }
}
