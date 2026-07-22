//! The trait contracts — `docs/sampler-trait-design.md` §4, §6, §7, §9.
//!
//! [`Instrument`] is *the* business-logic seam: notes in, mics out,
//! articulation control, plus the metadata a host needs. The extension traits
//! ([`Voicer`], [`PerformanceScript`], [`MicMixer`], [`Modulator`], [`Effect`],
//! [`Loader`], [`Authorization`]) capture the exotic 10%.
//!
//! Phase A wires only [`Instrument`] (on `SampleEngine`, see
//! [`super::engine`]) and a real [`PerformanceScript`] pipeline. The rest get
//! the trait + a trivial default impl so the surface compiles and is usable;
//! the deep impls are Phase-B TODOs.

use super::model::SampleSlice;
use super::prim::{ArticulationId, Cc, Frames, MicId, Note, U7, U14, Velocity};

// ── Render buffers — mics are first-class ────────────────────────────────

/// A borrowed stereo (planar L/R) output buffer for one mic.
pub struct StereoBuf<'a> {
    pub l: &'a mut [f32],
    pub r: &'a mut [f32],
}

impl StereoBuf<'_> {
    #[inline]
    pub fn frames(&self) -> usize {
        self.l.len().min(self.r.len())
    }

    /// Zero both channels.
    pub fn clear(&mut self) {
        for s in self.l.iter_mut() {
            *s = 0.0;
        }
        for s in self.r.iter_mut() {
            *s = 0.0;
        }
    }
}

/// One block of per-mic stereo output. The host maps each mic to a track; the
/// [`MicMixer`] sums them when a single output is wanted.
pub struct MicBlock<'a> {
    pub frames: usize,
    pub mics: &'a mut [(MicId, StereoBuf<'a>)],
}

impl<'a> MicBlock<'a> {
    pub fn new(frames: usize, mics: &'a mut [(MicId, StereoBuf<'a>)]) -> Self {
        MicBlock { frames, mics }
    }

    /// Mutable access to a named mic's buffer.
    pub fn mic_mut(&mut self, id: &MicId) -> Option<&mut StereoBuf<'a>> {
        self.mics
            .iter_mut()
            .find(|(mid, _)| mid == id)
            .map(|(_, buf)| buf)
    }

    /// Zero every mic buffer (voice pools accumulate into them).
    pub fn clear(&mut self) {
        for (_, buf) in self.mics.iter_mut() {
            buf.clear();
        }
    }
}

// ── The playable trait ───────────────────────────────────────────────────

/// A live, voiced instrument. `Send` so it rides daw's audio thread inside a
/// `PluginInstance`. Realtime-safe: no alloc / lock / IO in `note_on`/`render`.
pub trait Instrument: Send {
    // ── Performance input ──
    fn note_on(&mut self, note: Note, vel: Velocity);
    fn note_off(&mut self, note: Note);
    fn note_off_vel(&mut self, note: Note, _vel: Velocity) {
        self.note_off(note)
    }
    fn control(&mut self, cc: Cc, value: U7);
    fn pitch_bend(&mut self, _bend: U14) {}
    fn aftertouch(&mut self, _note: Note, _pressure: U7) {}
    fn channel_pressure(&mut self, _pressure: U7) {}
    fn all_notes_off(&mut self);
    fn panic(&mut self) {
        self.all_notes_off()
    }

    // ── Articulation / keyswitch ──
    fn articulations(&self) -> &[ArticulationId];
    fn select_articulation(&mut self, id: ArticulationId);
    fn active_articulation(&self) -> ArticulationId;

    // ── Render (multi-mic) ──
    /// Fill one block, writing a stereo buffer per active mic.
    fn render(&mut self, block: &mut MicBlock);
    fn mics(&self) -> &[MicId];
    /// Purge / load a mic to trade RAM. Default: no-op (single-mic engines).
    fn set_mic_active(&mut self, _mic: MicId, _on: bool) {}

    // ── State the host needs ──
    fn voices_active(&self) -> usize;
    fn latency(&self) -> Frames {
        Frames(0)
    }
    fn sample_rate(&self) -> u32;
}

// ── Voicer (Phase B) ─────────────────────────────────────────────────────

/// One note event's request to be voiced — what the [`Voicer`] turns into
/// voices. Phase A keeps this minimal; Phase B grows it (zone selection,
/// dynamics layer, mic set).
pub struct VoiceRequest {
    pub note: Note,
    pub vel: Velocity,
}

/// Shared performance state visible to the [`Voicer`] (RR counters, current
/// articulation). Phase A is a placeholder.
#[derive(Default)]
pub struct PerfState {
    pub round_robin_index: u32,
}

/// A pool the [`Voicer`] allocates voices into. Phase A placeholder — the real
/// pool lives in `engine::voice`.
#[derive(Default)]
pub struct VoicePool {
    pub requested: Vec<SampleSlice>,
}

/// Turn one note event (after performance scripting) into voices, selecting
/// zones across mics/dynamics/RR.
///
/// **Phase B**: the default impl (`DefaultVoicer`) will handle velocity layers,
/// CC-axis dynamics crossfade, round-robin cycling, and per-group
/// polyphony/choke. Today the real voicing lives inside `SampleEngine` — this
/// trait exists so the seam is named.
pub trait Voicer: Send {
    fn voices_for(&self, ev: &VoiceRequest, perf: &PerfState, alloc: &mut VoicePool);
}

// ── MicMixer ─────────────────────────────────────────────────────────────

/// A routing target for a [`MicMixer`]. Phase A models it as a single stereo
/// bus accumulator; Phase B grows it into a full `BusGraph` (mic→bus→master).
pub struct BusGraph<'a> {
    pub master: StereoBuf<'a>,
}

/// Combine an instrument's per-mic output into a routing graph.
pub trait MicMixer: Send {
    fn route(&mut self, mics: &MicBlock, out: &mut BusGraph);
}

/// Default [`MicMixer`]: sum every mic into the master bus. The daw-backed
/// impl (each mic = a track) is Phase B.
pub struct SumMixer;

impl MicMixer for SumMixer {
    fn route(&mut self, mics: &MicBlock, out: &mut BusGraph) {
        let frames = out.master.frames();
        for (_, buf) in mics.mics.iter() {
            let n = frames.min(buf.frames());
            for f in 0..n {
                out.master.l[f] += buf.l[f];
                out.master.r[f] += buf.r[f];
            }
        }
    }
}

// ── Modulator (Phase B) ──────────────────────────────────────────────────

/// Context handed to a [`Modulator`] each block (transport, sample rate, …).
/// Phase A placeholder.
#[derive(Default)]
pub struct ModContext {
    pub sample_rate: f64,
}

/// Anything that produces a control value over time (LFO / Env / CcFollower /
/// Random / Macro). Phase B ships the built-ins; Phase A provides the trait +
/// a constant default.
pub trait Modulator: Send {
    fn value(&mut self, ctx: &ModContext) -> f32;
}

/// A no-op modulator that always returns a fixed value — proves the trait is
/// usable. **Phase B**: LFO/Env/CcFollower/Random/Macro.
pub struct ConstModulator(pub f32);

impl Modulator for ConstModulator {
    fn value(&mut self, _ctx: &ModContext) -> f32 {
        self.0
    }
}

// ── Effect ───────────────────────────────────────────────────────────────

/// A named parameter for GUI binding + modulation. Phase A keeps it minimal.
pub struct Param {
    pub name: &'static str,
    pub value: f32,
}

/// A DSP block; built-ins and custom DSP impl the same trait, inserted anywhere
/// (zone/group/mic/instrument/bus). Realtime-safe.
pub trait Effect: Send {
    fn prepare(&mut self, sr: f64, max_block: usize);
    fn process(&mut self, buf: &mut StereoBuf);
    fn params(&self) -> &[Param] {
        &[]
    }
}

/// A transparent effect — proves the trait is usable as an insert.
/// **Phase B**: EQ / Comp / Convolver / Delay (signal already has
/// [`crate::Convolver`] / [`crate::NamProcessor`] to wrap here).
pub struct Passthrough;

impl Effect for Passthrough {
    fn prepare(&mut self, _sr: f64, _max_block: usize) {}
    fn process(&mut self, _buf: &mut StereoBuf) {}
}

// ── Loader (Phase B) ─────────────────────────────────────────────────────

/// Which samples to warm into RAM ahead of playback. Phase A placeholder.
#[derive(Default, Clone, Copy)]
pub struct PreloadProfile {
    /// Bytes per voice to prefetch from the head of each sample.
    pub head_bytes: usize,
}

/// Result of a preload pass. Phase A placeholder.
#[derive(Default, Clone, Copy)]
pub struct PreloadStats {
    pub samples: usize,
    pub bytes: usize,
}

/// A library-scoped reference to one sample. Phase A placeholder.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SampleRef(pub u32);

/// Streams samples so `note_on` never touches disk.
///
/// **Phase B**: back this with daw's `AudioSource` (mmap WAV / decoded memory)
/// plus prefetch. Today `SampleEngine` owns its own cache (`engine::cache`);
/// this trait names the seam.
pub trait Loader: Send + Sync {
    fn preload(&self, profile: PreloadProfile) -> PreloadStats;
    fn slice(&self, id: SampleRef) -> SampleSlice;
}

// ── Authorization ────────────────────────────────────────────────────────

/// An account identity. Phase A placeholder.
#[derive(Clone, Debug)]
pub struct Account(pub String);

/// A library identity. Phase A placeholder.
#[derive(Clone, Debug)]
pub struct LibraryId(pub String);

/// A granted license token. Phase A placeholder.
#[derive(Clone, Debug)]
pub struct License {
    pub library: String,
}

/// Account-based unlock + per-library license, checked at load (never render).
pub trait Authorization: Send + Sync {
    fn authorize(&self, library: &LibraryId, account: &Account) -> Result<License, AuthError>;
}

/// Authorization failure. Phase A placeholder.
#[derive(Clone, Debug)]
pub struct AuthError(pub String);

/// A no-op authorization that grants every request — proves the trait is
/// usable in dev/test. **Phase B**: account-based unlock + license packaging.
pub struct OpenAuth;

impl Authorization for OpenAuth {
    fn authorize(&self, library: &LibraryId, _account: &Account) -> Result<License, AuthError> {
        Ok(License {
            library: library.0.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_mixer_sums_mics() {
        let mut a_l = [1.0f32, 1.0];
        let mut a_r = [2.0f32, 2.0];
        let mut b_l = [0.5f32, 0.5];
        let mut b_r = [0.5f32, 0.5];
        let mut mics = [
            (
                MicId::new("A"),
                StereoBuf {
                    l: &mut a_l,
                    r: &mut a_r,
                },
            ),
            (
                MicId::new("B"),
                StereoBuf {
                    l: &mut b_l,
                    r: &mut b_r,
                },
            ),
        ];
        let block = MicBlock::new(2, &mut mics);

        let mut m_l = [0.0f32, 0.0];
        let mut m_r = [0.0f32, 0.0];
        let mut graph = BusGraph {
            master: StereoBuf {
                l: &mut m_l,
                r: &mut m_r,
            },
        };
        SumMixer.route(&block, &mut graph);
        assert_eq!(m_l, [1.5, 1.5]);
        assert_eq!(m_r, [2.5, 2.5]);
    }

    #[test]
    fn trivial_defaults_are_usable() {
        // Effect, Modulator, Authorization defaults exist and behave.
        let mut fx = Passthrough;
        fx.prepare(48_000.0, 512);
        let mut l = [1.0f32];
        let mut r = [1.0f32];
        fx.process(&mut StereoBuf {
            l: &mut l,
            r: &mut r,
        });
        assert_eq!(l, [1.0]);

        let mut m = ConstModulator(0.7);
        assert_eq!(m.value(&ModContext::default()), 0.7);

        let lic = OpenAuth
            .authorize(&LibraryId("CSS".into()), &Account("me".into()))
            .unwrap();
        assert_eq!(lic.library, "CSS");
    }
}
