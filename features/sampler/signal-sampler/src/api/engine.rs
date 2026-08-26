//! The working default [`Instrument`] impl — `docs/sampler-trait-design.md` §4.
//!
//! This is the *proof* the new API is playable end-to-end: it maps the
//! [`Instrument`] trait onto the existing [`SampleEngine`], reusing its full
//! voicing / loading / round-robin / legato machinery. No second engine.
//!
//! A thin [`EngineInstrument`] wrapper owns two pieces of cached state the bare
//! engine doesn't expose as `&[ArticulationId]`/`ArticulationId`:
//! - the interned articulation-id list (`articulations()` returns `&[…]`), and
//! - the active articulation id (kept in sync with `SampleEngine::articulation`).
//!
//! Everything else delegates straight through:
//! - `note_on`  → [`SampleEngine::note_on`]
//! - `note_off` → [`SampleEngine::note_off`] (`note_off_vel` →
//!   `note_off_with_velocity`)
//! - `control`  → [`SampleEngine::cc`]
//! - `aftertouch`/`channel_pressure` → poly/channel aftertouch
//! - `render`   → fills the [`MicBlock`] from
//!   [`SampleEngine::render`] / [`render_multi`](SampleEngine::render_multi)
//! - `select_articulation` → [`SampleEngine::set_articulation`]
//! - `voices_active`/`sample_rate` → engine getters.

use crate::engine::SampleEngine;

use super::prim::{ArticulationId, Cc, Frames, MicId, Note, Velocity, U14, U7};
use super::traits::{Instrument, MicBlock};

/// [`SampleEngine`] presented as an [`Instrument`].
///
/// `Send` (the engine is `Send`); the cached `Vec`s are plain owned data.
pub struct EngineInstrument {
    engine: SampleEngine,
    /// Interned articulation ids, cached from the patch spec so
    /// `articulations()` can hand out a `&[ArticulationId]`.
    articulation_ids: Vec<ArticulationId>,
    /// The currently active articulation, mirrored from the engine.
    active: ArticulationId,
    /// Interned mic ids, cached from the engine's mic list.
    mic_ids: Vec<MicId>,
    /// Per-mic interleaved scratch (`2 * frames` each), grown lazily, never
    /// allocated on the hot path once sized.
    mic_scratch: Vec<Vec<f32>>,
}

impl EngineInstrument {
    /// Wrap an already-constructed [`SampleEngine`].
    pub fn new(engine: SampleEngine) -> Self {
        let articulation_ids = engine
            .patch()
            .spec
            .articulations
            .iter()
            .map(|a| ArticulationId::new(&a.id))
            .collect::<Vec<_>>();
        let active = ArticulationId::new(engine.articulation());
        let mic_ids = engine.mic_ids().iter().map(MicId::new).collect::<Vec<_>>();
        Self {
            engine,
            articulation_ids,
            active,
            mic_ids,
            mic_scratch: Vec::new(),
        }
    }

    /// Borrow the wrapped engine (metering / stats).
    pub fn engine(&self) -> &SampleEngine {
        &self.engine
    }
    /// Mutable engine access.
    pub fn engine_mut(&mut self) -> &mut SampleEngine {
        &mut self.engine
    }

    /// Render into a single interleaved stereo scratch buffer, then split it
    /// into the first mic of the block. Used when the block carries one mic.
    fn render_single(&mut self, block: &mut MicBlock) {
        let frames = block.frames;
        let want = frames * 2;
        if self.mic_scratch.is_empty() {
            self.mic_scratch.push(Vec::new());
        }
        let scratch = &mut self.mic_scratch[0];
        if scratch.len() < want {
            scratch.resize(want, 0.0);
        }
        for s in &mut scratch[..want] {
            *s = 0.0;
        }
        self.engine.render(&mut scratch[..want]);

        if let Some((_, buf)) = block.mics.first_mut() {
            let n = frames.min(buf.frames());
            for f in 0..n {
                buf.l[f] += scratch[f * 2];
                buf.r[f] += scratch[f * 2 + 1];
            }
        }
    }

    /// Render every engine mic into its matching block mic via `render_multi`.
    fn render_multi(&mut self, block: &mut MicBlock) {
        let frames = block.frames;
        let want = frames * 2;
        let n_mics = self.mic_ids.len();

        // Size the per-mic interleaved scratch (no alloc once warmed).
        if self.mic_scratch.len() < n_mics {
            self.mic_scratch.resize_with(n_mics, Vec::new);
        }
        for s in self.mic_scratch.iter_mut().take(n_mics) {
            if s.len() < want {
                s.resize(want, 0.0);
            }
            for v in &mut s[..want] {
                *v = 0.0;
            }
        }
        self.engine.render_multi(&mut self.mic_scratch[..n_mics]);

        // Fan each engine mic's interleaved scratch into the block's matching
        // planar mic buffer (by mic id).
        for (i, mic_id) in self.mic_ids.iter().enumerate() {
            let scratch = &self.mic_scratch[i];
            if let Some(buf) = block.mic_mut(mic_id) {
                let n = frames.min(buf.frames());
                for f in 0..n {
                    buf.l[f] += scratch[f * 2];
                    buf.r[f] += scratch[f * 2 + 1];
                }
            }
        }
    }
}

impl Instrument for EngineInstrument {
    fn note_on(&mut self, note: Note, vel: Velocity) {
        self.engine.note_on(note.get(), vel.get());
    }

    fn note_off(&mut self, note: Note) {
        self.engine.note_off(note.get());
    }

    fn note_off_vel(&mut self, note: Note, vel: Velocity) {
        self.engine.note_off_with_velocity(note.get(), vel.get());
    }

    fn control(&mut self, cc: Cc, value: U7) {
        self.engine.cc(cc.get(), value.get());
    }

    fn pitch_bend(&mut self, _bend: U14) {
        // SampleEngine does not implement pitch bend (Phase B): no-op.
    }

    fn aftertouch(&mut self, note: Note, pressure: U7) {
        self.engine.poly_aftertouch(note.get(), pressure.get());
    }

    fn channel_pressure(&mut self, pressure: U7) {
        self.engine.channel_aftertouch(pressure.get());
    }

    fn all_notes_off(&mut self) {
        self.engine.all_notes_off();
    }

    fn articulations(&self) -> &[ArticulationId] {
        &self.articulation_ids
    }

    fn select_articulation(&mut self, id: ArticulationId) {
        self.engine.set_articulation(id.as_str());
        self.active = id;
    }

    fn active_articulation(&self) -> ArticulationId {
        // Keep in sync with the engine in case a CC/keyswitch changed it.
        ArticulationId::new(self.engine.articulation())
    }

    fn render(&mut self, block: &mut MicBlock) {
        if self.mic_ids.len() > 1 {
            self.render_multi(block);
        } else {
            self.render_single(block);
        }
    }

    fn mics(&self) -> &[MicId] {
        &self.mic_ids
    }

    fn voices_active(&self) -> usize {
        self.engine.active_voices()
    }

    fn latency(&self) -> Frames {
        Frames(0)
    }

    fn sample_rate(&self) -> u32 {
        self.engine.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::traits::{MicBlock, StereoBuf};

    /// Write a 1s loud sine WAV the engine can decode + play (mirrors the
    /// `instrument.rs` fixture so the proof reuses the same approach).
    fn write_sine_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("create wav");
        for i in 0..48_000 {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.8;
            w.write_sample(s).expect("write sample");
        }
        w.finalize().expect("finalize wav");
    }

    fn minimal_engine(dir: &std::path::Path) -> SampleEngine {
        let wav = dir.join("note.wav");
        write_sine_wav(&wav);
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");
        let patch = crate::PlayerPatch::load(&spec_path, dir).expect("load patch");
        let mut engine = SampleEngine::new(patch, 48_000, "", "");
        engine.preload_samples();
        engine
    }

    /// The proof: `Instrument::note_on` → `render(&mut MicBlock)` produces
    /// audio via the existing `SampleEngine`.
    #[test]
    fn note_on_then_render_produces_audio() {
        let dir = std::env::temp_dir().join(format!("signal-api-engine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let mut inst = EngineInstrument::new(minimal_engine(&dir));
        assert_eq!(inst.sample_rate(), 48_000);

        inst.note_on(Note(60), Velocity::new(100));
        assert!(inst.voices_active() > 0, "note_on should allocate a voice");

        // Render several blocks and accumulate RMS.
        let frames = 512usize;
        let mut energy = 0.0f64;
        let mut count = 0usize;
        for _ in 0..8 {
            let mut l = vec![0.0f32; frames];
            let mut r = vec![0.0f32; frames];
            {
                let mut mics = [(
                    MicId::new(""),
                    StereoBuf {
                        l: &mut l,
                        r: &mut r,
                    },
                )];
                let mut block = MicBlock::new(frames, &mut mics);
                inst.render(&mut block);
            }
            for s in l.iter().chain(r.iter()) {
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

    /// Articulation list + active selection round-trips through the engine.
    #[test]
    fn articulation_state_reflects_engine() {
        let dir = std::env::temp_dir().join(format!("signal-api-artic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let mut inst = EngineInstrument::new(minimal_engine(&dir));

        // The minimal zone lib has no articulation spec rows, but selecting one
        // must round-trip through the engine's active articulation.
        inst.select_articulation(ArticulationId::new("Sustain"));
        assert_eq!(inst.active_articulation().as_str(), "Sustain");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_instrument_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<EngineInstrument>();
    }
}
