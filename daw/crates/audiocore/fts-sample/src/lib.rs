//! Shared audio sample loading utilities for FTS plugins.
//!
//! One crate, three layers, each behind its own cargo feature so lean
//! consumers pay only for what they use:
//!
//! - [`probe`] (always available): header-only WAV inspection — rate,
//!   channels, bit depth, frame count — with no sample decode, cheap
//!   enough to sweep whole libraries.
//! - `load` — resident decodes via Symphonium (WAV, FLAC, MP3, OGG, etc.
//!   depending on enabled features), with optional resampling:
//!   [`load_audio`] (interleaved stereo `f64`), [`load_mono_f32`],
//!   [`load_planar_f32`].
//! - `write` — [`write_wav_f32`], the shared 32-bit-float WAV writer.
//! - `mapped` — [`mapped::PcmFile`], the memory-mapped uncompressed-PCM
//!   source (header parse only; samples convert to f32 per read) — the
//!   DAW's streaming WAV path. memmap2 only, none of the `engine` codecs.
//! - `budget` — the process-wide preload RAM budget (std-only; `engine`
//!   implies it).
//! - `engine-core` — the wasm-clean half of the sample-engine file layer
//!   (RAM budgeting, the decoded-sample cache, in-memory .signalpack access
//!   via [`cache::SignalPcmPack::open_bytes`], FLAC seek index, chunked
//!   decode). No mmap, no threads, no encoders.
//! - `engine-native` — mmap'd pack open, parallel preload, the background
//!   streamer/warm threads, the on-disk stream cache, and pack writing.
//! - `engine` — alias for `engine-native`; re-exported by signal-sampler as
//!   `signal_sampler::engine::*`.
//!
//! # Usage
//!
//! ```ignore
//! use fts_sample::{probe, load_mono_f32, ResampleQuality};
//!
//! // Header only — no decode.
//! let info = probe("kick.wav".as_ref())?;
//! println!("{} Hz, {} ch, {} frames", info.sample_rate, info.channels, info.num_frames);
//!
//! // Full decode, downmixed to mono at 48 kHz.
//! let (samples, sr) = load_mono_f32("kick.wav".as_ref(), Some(48_000), ResampleQuality::default())?;
//! ```

pub mod dialog;
pub mod probe;

#[cfg(feature = "load")]
mod load;
#[cfg(feature = "mapped")]
pub mod mapped;
#[cfg(feature = "write")]
mod write;

// ── Sample-engine file layer (moved from signal-sampler's `engine/`) ─────────
//
// The audio-file half of the sampler engine: RAM budgeting, the decoded-sample
// cache + .signalpack access, the FLAC seek index, chunked streaming, and the
// on-disk stream cache. Voice/playback types stay in signal-sampler; these
// modules are re-exported there as `signal_sampler::engine::{budget, cache,
// flac_index, stream, stream_cache}`.
#[cfg(feature = "budget")]
pub mod budget;
#[cfg(feature = "engine-core")]
pub mod cache;
#[cfg(feature = "engine-core")]
pub mod flac_index;
/// Which bytes of an Ogg proxy hold which second — what streaming it from
/// elsewhere fetches by.
#[cfg(feature = "engine-core")]
pub mod ogg_index;
/// An Ogg Vorbis stream decoded a window at a time — the session's
/// proxies, streamed (see the module docs).
#[cfg(feature = "engine-core")]
pub mod ogg_stream;
/// A file that arrives in pieces, readable where it has landed — the
/// bytes of a proxy streamed from elsewhere.
#[cfg(feature = "engine-core")]
pub mod sparse;
#[cfg(feature = "engine-core")]
pub mod stream;
#[cfg(feature = "engine-native")]
pub mod stream_cache;
/// The browser's streamer queue — the wasm stand-in for the native streamer
/// thread pool (see the module docs; W13 of browser-keys-rig.md). Rides the
/// same feature gate as `stream`, whose types it queues.
#[cfg(all(feature = "engine-core", target_arch = "wasm32"))]
pub mod stream_wasm;

mod error;

pub use error::SamplerError;
pub use probe::{probe, AudioInfo, WavFormat};

#[cfg(feature = "load")]
pub use load::{
    decode_bytes, decode_bytes_at, load_audio, load_audio_async, load_mono_f32, load_planar_f32,
    AudioData, LoadedAudio, ResampleQuality,
};
#[cfg(feature = "mapped")]
pub use mapped::{PcmFile, PcmFormat};
#[cfg(feature = "write")]
pub use write::write_wav_f32;
