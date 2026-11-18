//! [`SampleEngine`] as the **Sample** [`Soundsource`] — the first-class
//! generator for multisample playback (packs, zone maps, round-robins,
//! mics, legato).
//!
//! This is the MIDI analog of the rig's audio-only [`crate::rig::GuitarRig`]:
//! instead of pass-through processing an input bus, it **generates** audio
//! from MIDI. Hosted on a daw instrument track through the generic
//! [`SoundsourceLeaf`](crate::soundsource::SoundsourceLeaf) adapter, the
//! renderer runs the FX chain every block (the chain is non-empty so the
//! `dirty || !fx_chain` guard keeps it alive), feeds it the block's MIDI
//! events — including live / programmatic ones pushed via
//! `Standalone::push_note_on` — and the instrument writes the rendered
//! voices into the track bus.
//!
//! `in_l` / `in_r` are ignored: a sampler is a source, not an effect.
//!
//! ## Send-safety
//! [`Soundsource`] requires `Send`. [`SampleEngine`] contains only
//! `Cell`/`RefCell` interior mutability (metering counters + a round-robin
//! state cell) over `Send` payloads — these are `Send` (they are merely
//! not `Sync`), and there is no `Rc` anywhere in the engine — so
//! `SamplerInstrument` is `Send`.

use signal_plugin_host::{PluginDescriptor, PluginEvents, PluginFormat, PluginParamInfo};

use crate::engine::SampleEngine;
use crate::soundsource::{Soundsource, SoundsourceKind};

/// A [`SampleEngine`] presented as the **Sample** [`Soundsource`].
///
/// The engine is constructed (and its sample rate fixed) before the
/// instrument is built — `SampleEngine` has no runtime sample-rate change
/// — so [`prepare`](Soundsource::prepare) only (re)sizes the interleaved
/// render scratch.
pub struct SamplerInstrument {
    engine: SampleEngine,
    /// Interleaved stereo render scratch (`2 * block_size`), reused.
    scratch: Vec<f32>,
    /// Last frame handed to the host, so the artefact scan (see `process`)
    /// can see a discontinuity that falls ACROSS a block boundary — which is
    /// where a per-block bug puts one.
    prev: (f32, f32),
    /// Hole detection carries across blocks: a dropout does not politely
    /// begin and end inside one callback.
    holes: crate::engine::HoleDetector,
}

impl SamplerInstrument {
    /// Wrap an already-constructed [`SampleEngine`]. The engine's sample
    /// rate must match the host's; the host calls
    /// [`prepare`](Soundsource::prepare) with the same rate but the
    /// engine ignores it (its rate is immutable post-construction).
    pub fn new(engine: SampleEngine) -> Self {
        Self {
            engine,
            scratch: Vec::new(),
            prev: (0.0, 0.0),
            holes: crate::engine::HoleDetector::default(),
        }
    }

    /// Look at what we just produced, and count the artefacts a deadline
    /// counter cannot see.
    ///
    /// `xruns` and `over_budget` answer "was the callback late". They are
    /// both silent about audio that arrived exactly on time and was WRONG,
    /// which is the failure this catches:
    ///
    /// - **gaps** — digital silence while voices are sounding. `fts_sample`
    ///   returns 0.0 for a chunk that has not streamed in yet (it asks for
    ///   the chunk and moves on, rather than blocking the audio thread), so
    ///   a starved stream scatters micro-gaps through otherwise correct
    ///   audio. That is heard as vinyl crackle, not as a dropout.
    /// - **clicks** — a step no audio signal can make, e.g. the edge where
    ///   such a gap begins or ends.
    /// - **non-finite** — a NaN reaching the device.
    ///
    /// Cheap enough to leave on: two compares and a subtract per frame, on
    /// data already in cache because we just wrote it.
    fn scan_output(&mut self, out_l: &[f32], out_r: &[f32], frames: usize) {
        // Silence with nothing playing is not a gap, it is silence.
        // With nothing playing there is no stream to have a hole in, and the
        // silence that follows the last note must not be held open as a
        // candidate: drop the run instead of letting the next note close it.
        if self.engine.active_voices() == 0 {
            self.holes.reset();
        }
        let mut g = crate::engine::OutputGlitches::default();
        let (mut pl, mut pr) = self.prev;
        for f in 0..frames {
            let (l, r) = (out_l[f], out_r[f]);
            if !l.is_finite() || !r.is_finite() {
                g.nonfinite_frames += 1;
                pl = 0.0;
                pr = 0.0;
                continue;
            }
            // A hole is silence that signal RESUMES after — not silence
            // before a note has started, which every voice legitimately has
            // (`start_hold`, `attack_delay`) and which the first version of
            // this counted by the thousand.
            let magnitude = l.abs().max(r.abs());
            if let Some(run) = self.holes.feed(magnitude) {
                g.gap_frames += run;
                g.gap_runs += 1;
                g.longest_gap = g.longest_gap.max(run);
            }
            let slew = (l - pl).abs().max((r - pr).abs());
            if slew > crate::engine::CLICK_SLEW {
                g.click_frames += 1;
            }
            // Millionths, so the worst case survives in an integer atomic.
            let ppm = (slew * 1.0e6) as usize;
            if ppm > g.peak_slew_ppm {
                g.peak_slew_ppm = ppm;
            }
            pl = l;
            pr = r;
        }
        self.prev = (pl, pr);
        crate::engine::record_output_glitches(g);
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
    /// driving a full [`render`](Soundsource::render).
    pub(crate) fn apply_midi(&mut self, message: &midi::MidiEvent) {
        use midi::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => {
                // velocity 0 is a Note-Off by convention; `note_on` already
                // routes vel-0 to `note_off`, so just forward.
                self.engine.note_on(key.get(), velocity.get());
            }
            MidiEvent::NoteOff { key, velocity, .. } => {
                self.engine
                    .note_off_with_velocity(key.get(), velocity.get());
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
            // Pitch wheel: live playback-rate bend on every pitched voice
            // (Omnisphere-style synth presets want this; percussion engines
            // ignore it).
            MidiEvent::PitchBend { bend, .. } => {
                self.engine.pitch_bend(bend.get());
            }
            // ProgramChange / SysEx: not handled by the sampler — ignored.
            _ => {}
        }
    }
}

// r[impl signal.soundsource.sample]
impl Soundsource for SamplerInstrument {
    fn kind(&self) -> SoundsourceKind {
        SoundsourceKind::Sample
    }

    fn shed_voices(&mut self, level: u8) {
        self.engine_mut().shed_voices(level);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.sampler.instrument".into(),
            name: "Signal Sampler".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn prepare(&mut self, _sample_rate: f32, block_size: usize) {
        // `SampleEngine`'s rate is fixed at construction; only size the
        // interleaved (stereo) scratch so `render` never allocates.
        self.scratch.resize(block_size * 2, 0.0);
    }

    /// Modulation inputs a route can drive (0.5 = none): `pitch_mod`
    /// ±12 semitones on every voice, `cutoff_mod` ±4 octaves on the voices'
    /// own filter.
    fn params(&self) -> Vec<PluginParamInfo> {
        let mk = |id, name: &str| PluginParamInfo {
            id,
            name: name.into(),
            min: 0.0,
            max: 1.0,
            default: 0.5,
        };
        vec![mk(0, "pitch_mod"), mk(1, "cutoff_mod")]
    }

    fn set_param(&mut self, id: u32, value: f64) {
        let v = (value as f32).clamp(0.0, 1.0) - 0.5;
        match id {
            0 => self.engine.set_pitch_mod(v * 24.0),
            1 => self.engine.set_cutoff_mod(v * 8.0),
            _ => {}
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        // Drive the wrapped engine directly (the same call `apply_midi` makes
        // for an incoming NoteOn).
        self.engine.note_on(note, velocity);
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
        // The sampler is a source: audio input is ignored, notes arrive
        // through `events.midi`, and the engine's interleaved render is
        // de-interleaved into the planar output.
        let frames = out_l.len().min(out_r.len());

        // Modulation writes (routes onto `pitch_mod` / `cutoff_mod`).
        for &(id, value) in events.params {
            self.set_param(id, value);
        }
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

        self.scan_output(out_l, out_r, frames);
    }
}
