//! Native **Formant** block → the **City Wurli** physically-modeled
//! Wurlitzer 200A electric piano.
//!
//! This wraps the vendored `openwurli-dsp` `WurliEngine` (modal reed synthesis
//! → pickup → preamp → tremolo → power amp → speaker) as a `PluginInstance`
//! generator. openwurli is GPL-3.0, vendored for personal use; see
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

use openwurli_dsp::WurliEngine;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

/// The City Wurli voice — a polyphonic physically-modeled `PluginInstance`
/// backed by `openwurli_dsp::WurliEngine`.
pub struct NativeWurli {
    engine: WurliEngine,
    sample_rate: f64,
    prepared: bool,
    /// Pre-allocated mono scratch buffer for the engine's render.
    scratch: Vec<f32>,
}

impl NativeWurli {
    pub fn new(sample_rate: u32) -> Self {
        let sr = (sample_rate.max(1)) as f64;
        Self {
            engine: WurliEngine::new(sr),
            sample_rate: sr,
            prepared: false,
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

impl PluginInstance for NativeWurli {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.city_wurli".into(),
            name: "City Wurli".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
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
        let new_sr = sample_rate.max(1.0);
        if (new_sr - self.sample_rate).abs() > f64::EPSILON {
            self.sample_rate = new_sr;
            self.engine.set_sample_rate(new_sr);
        }
        self.engine.ensure_buffer_capacity(block_size.max(1) as usize);
        if (self.scratch.len() as u32) < block_size {
            self.scratch.resize(block_size as usize, 0.0);
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
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
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
        self.engine.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

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

    #[test]
    fn note_on_generates_non_silent_audio() {
        let mut w = NativeWurli::new(48_000);
        w.prepare(48_000.0, 512).unwrap();
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
        // Strike once, then render several tail blocks so the reed onset ramp
        // develops before measuring energy.
        w.process_block(&inl, &inr, &mut outl, &mut outr, &strike_ev)
            .unwrap();
        assert_eq!(w.active_voices(), 1);
        let mut peak = 0.0f32;
        for _ in 0..8 {
            w.process_block(&inl, &inr, &mut outl, &mut outr, &silent_ev)
                .unwrap();
            for &s in &outl {
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 1e-4, "wurli voice should be audible, peak={peak}");
    }
}
