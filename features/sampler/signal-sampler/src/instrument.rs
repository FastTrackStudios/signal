//! [`SampleEngine`] wrapped as an instrument [`PluginInstance`].
//!
//! This is the MIDI analog of the rig's audio-only [`crate::rig::GuitarRig`]:
//! instead of pass-through processing an input bus, it **generates** audio
//! from MIDI. Hosted on a daw instrument track, the renderer runs the FX
//! chain every block (the chain is non-empty so the `dirty || !fx_chain`
//! guard keeps it alive), feeds it the block's MIDI events — including
//! live / programmatic ones pushed via `Standalone::push_note_on` — and
//! the instrument writes the rendered voices into the track bus.
//!
//! `in_l` / `in_r` are ignored: a sampler is a source, not an effect.
//!
//! ## Send-safety
//! [`PluginInstance`] requires `Send`. [`SampleEngine`] contains only
//! `Cell`/`RefCell` interior mutability (metering counters + a round-robin
//! state cell) over `Send` payloads — these are `Send` (they are merely
//! not `Sync`), and there is no `Rc` anywhere in the engine — so
//! `SamplerInstrument` is `Send`.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::engine::SampleEngine;

/// A [`SampleEngine`] presented to a daw host as an instrument plugin.
///
/// The engine is constructed (and its sample rate fixed) before the
/// instrument is built — `SampleEngine` has no runtime sample-rate change
/// — so [`prepare`](PluginInstance::prepare) only validates the block size
/// and (re)sizes the interleaved render scratch.
pub struct SamplerInstrument {
    engine: SampleEngine,
    /// Interleaved stereo render scratch (`2 * block_size`), reused.
    scratch: Vec<f32>,
    prepared: bool,
}

impl SamplerInstrument {
    /// Wrap an already-constructed [`SampleEngine`]. The engine's sample
    /// rate must match the host's; the host calls
    /// [`prepare`](PluginInstance::prepare) with the same rate but the
    /// engine ignores it (its rate is immutable post-construction).
    pub fn new(engine: SampleEngine) -> Self {
        Self {
            engine,
            scratch: Vec::new(),
            prepared: false,
        }
    }

    /// Immutable access to the wrapped engine (e.g. for metering / stats).
    pub fn engine(&self) -> &SampleEngine {
        &self.engine
    }

    /// Mutable access to the wrapped engine.
    pub fn engine_mut(&mut self) -> &mut SampleEngine {
        &mut self.engine
    }

    /// Apply one MIDI message to the engine. Channel is ignored — the
    /// engine is monotimbral. Split out so it can be unit-tested without
    /// driving a full [`process_block`](PluginInstance::process_block).
    pub(crate) fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => {
                // velocity 0 is a Note-Off by convention; `note_on` already
                // routes vel-0 to `note_off`, so just forward.
                self.engine.note_on(key.get(), velocity.get());
            }
            MidiEvent::NoteOff { key, velocity, .. } => {
                self.engine.note_off_with_velocity(key.get(), velocity.get());
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                self.engine.cc(controller.get(), value.get());
            }
            MidiEvent::ChannelPressure { pressure, .. } => {
                self.engine.channel_aftertouch(pressure.get());
            }
            MidiEvent::PolyAftertouch { key, pressure, .. } => {
                self.engine.poly_aftertouch(key.get(), pressure.get());
            }
            // ProgramChange / PitchBend / SysEx: not handled by the
            // sampler — ignored.
            _ => {}
        }
    }
}

impl PluginInstance for SamplerInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.sampler.instrument".into(),
            name: "Signal Sampler".into(),
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

    fn prepare(&mut self, _sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        // `SampleEngine`'s rate is fixed at construction; only size the
        // interleaved (stereo) scratch so `process_block` never allocates.
        self.scratch.resize(block_size as usize * 2, 0.0);
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
        let frames = out_l.len().min(out_r.len());

        // Apply this block's MIDI in offset order (the host already sorts).
        for ev in events.midi {
            self.apply_midi(&ev.message);
        }

        // Interleaved stereo scratch, zeroed (the voice pool accumulates).
        let want = frames * 2;
        if self.scratch.len() < want {
            self.scratch.resize(want, 0.0);
        }
        for s in &mut self.scratch[..want] {
            *s = 0.0;
        }
        self.engine.render(&mut self.scratch[..want]);

        // De-interleave into the host's planar L/R output buffers.
        for f in 0..frames {
            out_l[f] = self.scratch[f * 2];
            out_r[f] = self.scratch[f * 2 + 1];
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::{PluginEvents, PluginMidiEvent};

    /// `PluginInstance: Send` is a hard requirement (the renderer hands
    /// `Box<dyn PluginInstance>` across threads). This is a compile-time
    /// proof that the wrapped `SampleEngine` keeps `SamplerInstrument`
    /// `Send` — no `Rc`, only `Send`-but-not-`Sync` interior mutability.
    #[test]
    fn sampler_instrument_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SamplerInstrument>();
        // And it really is usable as the trait object the host stores.
        fn assert_boxed_send(_: Box<dyn PluginInstance>) {}
        // (type-checks the Box<dyn PluginInstance>: Send coercion path)
        let _ = assert_boxed_send;
    }

    /// Write a tiny loud-sine WAV (f32) the sampler can decode + play.
    fn write_sine_wav(path: &std::path::Path, frames: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("create wav");
        for i in 0..frames {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.8;
            w.write_sample(s).expect("write sample");
        }
        w.finalize().expect("finalize wav");
    }

    /// Build a minimal zone-mode `PlayerPatch` from a generated WAV +
    /// inline styx spec, so the engine renders real audio headlessly
    /// (no external sample-pack mount required).
    fn minimal_engine(dir: &std::path::Path) -> SampleEngine {
        let wav = dir.join("note.wav");
        write_sine_wav(&wav, 48_000); // 1s of sine

        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");

        let patch = crate::PlayerPatch::load(&spec_path, dir).expect("load patch");
        assert!(patch.is_zoned(), "patch must be in zone mode");
        let mut engine = SampleEngine::new(patch, 48_000, "", "");
        engine.preload_samples();
        engine
    }

    fn note_on_event(note: u8, velocity: u8) -> PluginMidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOn {
                channel: Channel::new(0),
                key: KeyNumber::new(note),
                velocity: Velocity::new(velocity),
            },
        }
    }

    /// End-to-end seam: a `NoteOn` arriving through `process_block` reaches
    /// the `SampleEngine`, allocates a voice, and produces non-silent output.
    #[test]
    fn note_on_through_process_block_produces_audio() {
        let dir = std::env::temp_dir().join(format!("signal-sampler-it-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let mut inst = SamplerInstrument::new(minimal_engine(&dir));
        inst.prepare(48_000.0, 512).expect("prepare");

        let frames = 512usize;
        let in_l = vec![0.0f32; frames];
        let in_r = vec![0.0f32; frames];
        let mut out_l = vec![0.0f32; frames];
        let mut out_r = vec![0.0f32; frames];

        // Drive a Note-On; the engine must allocate a voice.
        let midi = [note_on_event(60, 100)];
        let events = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        inst.process_block(&in_l, &in_r, &mut out_l, &mut out_r, &events)
            .expect("process");

        assert!(
            inst.engine().active_voices() > 0,
            "NoteOn should allocate at least one voice"
        );

        // RMS over a few blocks: a 220 Hz sine at 0.8 must be audibly non-silent.
        let mut energy = 0.0f64;
        let mut count = 0usize;
        for _ in 0..8 {
            for v in out_l.iter_mut().chain(out_r.iter_mut()) {
                *v = 0.0;
            }
            let empty = PluginEvents {
                params: &[],
                midi: &[],
                note_expressions: &[],
            };
            inst.process_block(&in_l, &in_r, &mut out_l, &mut out_r, &empty)
                .expect("process");
            for s in out_l.iter().chain(out_r.iter()) {
                energy += (*s as f64) * (*s as f64);
                count += 1;
            }
        }
        let rms = (energy / count.max(1) as f64).sqrt();
        assert!(
            rms > 1e-4,
            "rendered output should be non-silent, rms={rms}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The MIDI→engine mapping seam (no audio needed): NoteOff is delivered
    /// with its release velocity, CC reaches `engine.cc`, and a NoteOn with
    /// velocity 0 is treated as a NoteOff.
    #[test]
    fn apply_midi_maps_every_handled_variant() {
        use midicore::{
            Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, PitchBend,
            Pressure, ProgramNumber, Velocity,
        };
        let ch = Channel::new(0);
        let dir = std::env::temp_dir().join(format!("signal-sampler-map-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let mut inst = SamplerInstrument::new(minimal_engine(&dir));

        // NoteOn allocates a voice…
        inst.apply_midi(&MidiEvent::NoteOn {
            channel: ch,
            key: KeyNumber::new(64),
            velocity: Velocity::new(110),
        });
        assert!(inst.engine().active_voices() > 0);

        // CC / aftertouch must not panic and are accepted by the engine.
        inst.apply_midi(&MidiEvent::ControlChange {
            channel: ch,
            controller: ControllerNumber::new(1),
            value: ControllerValue::new(90),
        });
        inst.apply_midi(&MidiEvent::ChannelPressure {
            channel: ch,
            pressure: Pressure::new(50),
        });
        inst.apply_midi(&MidiEvent::PolyAftertouch {
            channel: ch,
            key: KeyNumber::new(64),
            pressure: Pressure::new(70),
        });

        // Ignored variants are no-ops (must not panic).
        inst.apply_midi(&MidiEvent::ProgramChange {
            channel: ch,
            program: ProgramNumber::new(3),
        });
        inst.apply_midi(&MidiEvent::PitchBend {
            channel: ch,
            bend: PitchBend::new((1000i32 + 8192).clamp(0, 16383) as u16),
        });

        let _ = std::fs::remove_dir_all(&dir);
    }
}
