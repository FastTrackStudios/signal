//! Native **Formant** block → the **City Wurli** physically-modeled
//! Wurlitzer 200A electric piano.
//!
//! This wraps the vendored `openwurli-dsp` `WurliEngine` (modal reed synthesis
//! → pickup → preamp → tremolo → power amp → speaker) as the **`PhysicalModel`**
//! [`Soundsource`] — the first physically-modeled generator on the trait.
//! openwurli is GPL-3.0, vendored for personal use; see
//! `features/rigs/keys/wurli/`.
//!
//! `WurliEngine` owns all voice management (up to 64 voices, allocation,
//! stealing, sustain) and the full mono signal chain internally. This shim just
//! translates MIDI note on/off into `engine.note_on/note_off` and copies the
//! engine's mono render into both output channels.
//!
//! The engine's default params (volume 0.5, tremolo depth 0.5, speaker
//! character 0.0, MLP on) already produce sound, so no setter wiring is needed
//! for the basic keys-rig voice. MIDI `offset` is ignored — events are applied
//! at the start of the block (block-quantized), matching `NativeModal`.
//!
//! The reed model is excited by its own hammer simulation, so the trait's
//! sample-excitation hook keeps its default (not a hybrid model —
//! `supports_sample_excitation()` = `false`).

use openwurli_dsp::WurliEngine;
use signal_plugin_host::{PluginDescriptor, PluginEvents, PluginFormat};

use crate::soundsource::{Soundsource, SoundsourceKind};

/// The City Wurli voice — a polyphonic physically-modeled generator backed by
/// `openwurli_dsp::WurliEngine`, presented as the **`PhysicalModel`**
/// [`Soundsource`].
///
/// Enters the render tree through the generic [`SoundsourceLeaf`](crate::soundsource::SoundsourceLeaf) adapter.
pub struct NativeWurli {
    engine: WurliEngine,
    sample_rate: f64,
    /// Pre-allocated mono scratch buffer for the engine's render.
    scratch: Vec<f32>,
}

impl NativeWurli {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let sr = (sample_rate.max(1)) as f64;
        Self {
            engine: WurliEngine::new(sr),
            sample_rate: sr,
            scratch: Vec::new(),
        }
    }

    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.engine.active_voice_count()
    }

    fn apply_midi(&mut self, message: &midi::MidiEvent) {
        use midi::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => {
                let vel = velocity.get();
                if vel == 0 {
                    self.engine.note_off(key.get());
                } else {
                    self.engine.note_on(key.get(), vel as f32 / 127.0);
                }
            }
            MidiEvent::NoteOff { key, .. } => self.engine.note_off(key.get()),
            _ => {}
        }
    }
}

// r[impl signal.soundsource.physical]
impl Soundsource for NativeWurli {
    fn kind(&self) -> SoundsourceKind {
        SoundsourceKind::PhysicalModel
    }

    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.city_wurli".into(),
            name: "City Wurli".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn prepare(&mut self, sample_rate: f32, block_size: usize) {
        let new_sr = (sample_rate.max(1.0)) as f64;
        if (new_sr - self.sample_rate).abs() > f64::EPSILON {
            self.sample_rate = new_sr;
            self.engine.set_sample_rate(new_sr);
        }
        let bs = block_size.max(1);
        self.engine.ensure_buffer_capacity(bs);
        if self.scratch.len() < bs {
            self.scratch.resize(bs, 0.0);
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.engine.note_off(note);
        } else {
            self.engine.note_on(note, velocity as f32 / 127.0);
        }
    }

    fn note_off(&mut self, note: u8) {
        self.engine.note_off(note);
    }

    fn render(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) {
        // A physical model is a generator: audio input is ignored, notes
        // arrive through `events.midi`.
        for ev in events.midi {
            self.apply_midi(&ev.message);
        }
        let frames = out_l.len().min(out_r.len());
        if self.scratch.len() < frames {
            self.scratch.resize(frames, 0.0);
        }
        // WurliEngine::render overwrites the whole slice with its mono output.
        self.engine.render(&mut self.scratch[..frames]);
        out_l[..frames].copy_from_slice(&self.scratch[..frames]);
        out_r[..frames].copy_from_slice(&self.scratch[..frames]);
    }

    fn reset(&mut self) {
        self.engine.reset();
    }
}
