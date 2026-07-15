//! Native **Formant** block → the **City Wurli** physically-modeled
//! Wurlitzer 200A electric piano.
//!
//! This wraps the vendored `openwurli-dsp` `WurliEngine` (modal reed synthesis
//! → pickup → preamp → tremolo → power amp → speaker) as the **PhysicalModel**
//! [`Soundsource`] — the first physically-modeled generator on the trait.
//! openwurli is GPL-3.0, vendored for personal use; see
//! `features/rigs/wurli/openwurli-dsp/`.
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
/// `openwurli_dsp::WurliEngine`, presented as the **PhysicalModel**
/// [`Soundsource`]. Enters the render tree through the generic
/// [`SoundsourceLeaf`](crate::soundsource::SoundsourceLeaf) adapter.
pub struct NativeWurli {
    engine: WurliEngine,
    sample_rate: f64,
    /// Pre-allocated mono scratch buffer for the engine's render.
    scratch: Vec<f32>,
}

impl NativeWurli {
    pub fn new(sample_rate: u32) -> Self {
        let sr = (sample_rate.max(1)) as f64;
        Self {
            engine: WurliEngine::new(sr),
            sample_rate: sr,
            scratch: Vec::new(),
        }
    }

    pub fn active_voices(&self) -> usize {
        self.engine.active_voice_count()
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundsource::SoundsourceLeaf;
    use signal_plugin_host::{PluginInstance, PluginMidiEvent};

    fn note_on(note: u8, vel: u8) -> PluginMidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOn {
                channel: Channel::new(0),
                key: KeyNumber::new(note),
                velocity: Velocity::new(vel),
            },
        }
    }

    /// The wurli is the PhysicalModel Soundsource, and a direct
    /// `note_on` (the out-of-band voice entry point) renders nonzero audio.
    #[test]
    fn soundsource_note_on_renders_nonzero_audio() {
        let mut w = NativeWurli::new(48_000);
        assert_eq!(w.kind(), SoundsourceKind::PhysicalModel);
        assert!(!w.supports_sample_excitation(), "not a hybrid model");
        Soundsource::prepare(&mut w, 48_000.0, 512);

        w.note_on(60, 100);
        assert_eq!(w.active_voices(), 1);

        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let empty = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        // Render several tail blocks so the reed onset ramp develops.
        let mut peak = 0.0f32;
        for _ in 0..8 {
            w.render(&inl, &inr, &mut outl, &mut outr, &empty);
            for &s in &outl {
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 1e-4, "wurli voice should be audible, peak={peak}");
        // The default excitation hook is a harmless no-op.
        w.excite(60, &inl, &inr);

        // reset drops all sounding voices.
        Soundsource::reset(&mut w);
        assert_eq!(w.active_voices(), 0);
    }

    /// MIDI arriving through the generic leaf's `process_block` (the render
    /// tree boundary) still reaches the engine — the pre-inversion behavior.
    #[test]
    fn note_on_through_leaf_generates_non_silent_audio() {
        let mut leaf = SoundsourceLeaf::new(NativeWurli::new(48_000));
        leaf.prepare(48_000.0, 512).unwrap();
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let strike = [note_on(60, 100)];
        let strike_ev = PluginEvents {
            params: &[],
            midi: &strike,
            note_expressions: &[],
        };
        let silent_ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        leaf.process_block(&inl, &inr, &mut outl, &mut outr, &strike_ev)
            .unwrap();
        let mut peak = 0.0f32;
        for _ in 0..8 {
            leaf.process_block(&inl, &inr, &mut outl, &mut outr, &silent_ev)
                .unwrap();
            for &s in &outl {
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 1e-4, "wurli via leaf should be audible, peak={peak}");
        assert_eq!(leaf.descriptor().id, "signal.native.city_wurli");
    }
}
