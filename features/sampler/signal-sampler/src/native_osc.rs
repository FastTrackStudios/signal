//! Native **Oscillator** — the first built-in DSP block (the `Native`
//! implementation of an `Oscillator`-typed [`RigBlock`](crate::rig::RigBlock)).
//!
//! A simple polyphonic generator that proves the node-render pipeline end to end:
//! a Nord-style synth `Oscillator` block now makes sound. The full waveform
//! taxonomy (Pure / Sub / Sync / Shape / Multi / Super / FM …) and `osc_ctrl`
//! shaping come later — this is the seed.
//!
//! Implemented as a [`PluginInstance`] generator (like
//! [`SamplerInstrument`](crate::SamplerInstrument)): it ignores its audio input
//! and writes summed voices into the output from the MIDI it receives.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::native::{Adsr, AdsrParams};

/// Oscillator waveform. (A stand-in for the eventual Nord category/Osc-Ctrl set.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OscWave {
    Sine,
    Saw,
    Square,
    Triangle,
}

impl OscWave {
    /// One cycle, `phase` in `[0, 1)`, output in `[-1, 1]`.
    #[inline]
    fn sample(self, phase: f32) -> f32 {
        match self {
            OscWave::Sine => (core::f32::consts::TAU * phase).sin(),
            OscWave::Saw => 2.0 * phase - 1.0,
            OscWave::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            OscWave::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
        }
    }
}

/// One sounding note.
struct Voice {
    note: u8,
    phase: f32,
    /// Per-frame phase increment (= freq / sample_rate).
    inc: f32,
    amp: f32,
    /// Per-voice amplitude envelope; the voice is dropped once it goes idle.
    env: Adsr,
}

/// A polyphonic native oscillator.
pub struct NativeOscillator {
    sample_rate: f32,
    wave: OscWave,
    voices: Vec<Voice>,
    prepared: bool,
}

impl NativeOscillator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            wave: OscWave::Sine,
            voices: Vec::new(),
            prepared: false,
        }
    }

    #[must_use]
    pub fn with_wave(mut self, wave: OscWave) -> Self {
        self.wave = wave;
        self
    }

    /// Number of sounding voices (for tests / metering).
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        // Keep total level sane under polyphony.
        let amp = (velocity as f32 / 127.0) * 0.15;
        // Retrigger if the note is already held (envelope continues from its
        // current level, so no click).
        if let Some(v) = self.voices.iter_mut().find(|v| v.note == note) {
            v.inc = freq / self.sample_rate;
            v.amp = amp;
            v.env.note_on();
        } else {
            let mut env = Adsr::new(self.sample_rate, AdsrParams::default());
            env.note_on();
            self.voices.push(Voice {
                note,
                phase: 0.0,
                inc: freq / self.sample_rate,
                amp,
                env,
            });
        }
    }

    fn note_off(&mut self, note: u8) {
        // Enter the release tail; the render loop drops the voice at idle.
        for v in self.voices.iter_mut().filter(|v| v.note == note) {
            v.env.note_off();
        }
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => self.note_on(key.get(), velocity.get()),
            MidiEvent::NoteOff { key, .. } => self.note_off(key.get()),
            _ => {}
        }
    }
}

impl PluginInstance for NativeOscillator {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.oscillator".into(),
            name: "Oscillator".into(),
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

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        // Re-pitch live voices if the rate changed.
        let new_sr = sample_rate.max(1.0) as f32;
        if (new_sr - self.sample_rate).abs() > f32::EPSILON {
            let ratio = self.sample_rate / new_sr;
            for v in &mut self.voices {
                v.inc *= ratio;
                v.env.set_sample_rate(new_sr);
            }
            self.sample_rate = new_sr;
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
        for f in 0..frames {
            let mut s = 0.0f32;
            for v in &mut self.voices {
                s += self.wave.sample(v.phase) * v.amp * v.env.tick();
                v.phase += v.inc;
                if v.phase >= 1.0 {
                    v.phase -= 1.0;
                }
            }
            out_l[f] = s;
            out_r[f] = s;
        }
        // Reap voices whose release finished.
        self.voices.retain(|v| !v.env.is_idle());
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
        self.voices.clear();
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
        let mut osc = NativeOscillator::new(48_000);
        osc.prepare(48_000.0, 512).unwrap();
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(69, 100)]; // A4 = 440 Hz
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        osc.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
            .unwrap();
        assert_eq!(osc.active_voices(), 1);
        let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
        assert!(rms > 1e-3, "oscillator should be audible, rms={rms}");
    }

    #[test]
    fn note_off_releases_then_silences() {
        let mut osc = NativeOscillator::new(48_000);
        osc.prepare(48_000.0, 512).unwrap();
        osc.note_on(60, 100);
        assert_eq!(osc.active_voices(), 1);
        osc.note_off(60);
        // The voice rides its release tail, then is reaped.
        assert_eq!(osc.active_voices(), 1, "release tail keeps the voice alive");
        let (inl, inr) = (vec![0.0; 512], vec![0.0; 512]);
        let (mut outl, mut outr) = (vec![0.0; 512], vec![0.0; 512]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        // 1 s of blocks ≫ the 150 ms release.
        for _ in 0..(48_000 / 512 + 1) * 1 {
            osc.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
                .unwrap();
        }
        assert_eq!(osc.active_voices(), 0, "voice reaped after release");
        let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
        assert!(rms < 1e-5, "output silent after release, rms={rms}");
    }
}
