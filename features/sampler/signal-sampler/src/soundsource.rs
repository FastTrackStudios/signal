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

use signal_plugin_host::{PluginEvents, PluginParamInfo};

/// Which kind of generator a [`Soundsource`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundsourceKind {
    /// Analog / wavetable synthesis (unison, FM, ring, harmonia).
    Oscillator,
    /// Sampled multisample playback (zone maps, round-robins, mics, loops) —
    /// Keyscape, **Omnisphere soundsources**, drum kits, orchestral libraries.
    Sample,
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
