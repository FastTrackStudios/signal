//! Post-render aux hook: lets a host mix extra audio (click track,
//! count-in voices, section guide, …) into the engine's output after
//! the project block is rendered.
//!
//! Project-mode engines ([`AudioEngine::attached_to`] /
//! [`AudioEngine::with_project`]) invoke the installed [`AuxRenderer`]
//! once per audio callback with:
//!
//! - an **interleaved** f32 buffer (`clock.channels` samples per frame)
//!   already containing the rendered project block — the hook ADDS its
//!   signal into it (buffer is later converted to the device format);
//! - an [`AuxClock`] snapshot of the transport at the first frame of
//!   the block (position in seconds AND beats, tempo, time signature),
//!   derived from the project's tempo map + transport state.
//!
//! The hook is also called while the transport is stopped
//! (`clock.playing == false`, zeroed buffer) so voice tails can flush.
//!
//! Realtime rules: the engine keeps the hook behind a mutex that the
//! audio callback only `try_lock`s — installing/replacing a hook never
//! blocks the audio thread (the hook is skipped for that block under
//! contention). The hook itself must be allocation-free and must not
//! block.
//!
//! [`AudioEngine::attached_to`]: super::AudioEngine::attached_to
//! [`AudioEngine::with_project`]: super::AudioEngine::with_project

use std::sync::{Arc, Mutex};

/// Transport snapshot at the first frame of an aux-rendered block.
#[derive(Debug, Clone, Copy)]
pub struct AuxClock {
    /// Whether the transport is rolling (the project block is silent
    /// and the playhead does not advance when `false`).
    pub playing: bool,
    /// Playhead position in seconds at the first frame of the block.
    pub pos_seconds: f64,
    /// Playhead position in quarter notes at the first frame of the
    /// block, from the project's tempo map.
    pub pos_beats: f64,
    /// Tempo (quarter notes per minute) at `pos_seconds`.
    pub tempo_bpm: f64,
    /// Time signature numerator (project transport).
    pub time_sig_num: u32,
    /// Time signature denominator (project transport).
    pub time_sig_den: u32,
    /// Output sample rate.
    pub sample_rate: f64,
    /// Interleaved channel count of the buffer handed to the hook.
    pub channels: usize,
    /// Transport varispeed: timeline seconds per output second (1.0 =
    /// nominal; drift correction moves it by fractions of a percent).
    /// Frame `i` of the block is at `pos_seconds + i * playrate /
    /// sample_rate`.
    pub playrate: f64,
}

/// The post-render hook. `buf` is the INTERLEAVED f32 output block
/// (`clock.channels` samples per frame) — add into it, don't overwrite.
pub type AuxRenderer = Box<dyn FnMut(&mut [f32], &AuxClock) + Send>;

/// Shared slot the audio callback `try_lock`s each block.
pub(crate) type AuxSlot = Arc<Mutex<Option<AuxRenderer>>>;
