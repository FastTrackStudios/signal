//! Realtime-safe feed producers every engine shares, so every plugin gets
//! the same analyser, trace and meters.
//!
//! Each tap is a pair made once, off the audio thread: the **tap** (audio
//! thread) only pushes — wait-free, no locks, no allocation — and the
//! **reader** (control thread) turns what was pushed into a frame's values
//! at the rate a surface asked for.
//!
//! | tap | reader | feed kind |
//! |---|---|---|
//! | [`SpectrumTap`] | [`SpectrumAnalyzer`] (Hann → FFT → log bins → smoothing → ballistics) | `Spectrum` |
//! | [`HistoryTap`] | [`HistoryReader`] (last `len` points at `rate_hz`) | `History` |
//! | [`LevelTap`] | [`LevelReader`] (peak/RMS dBFS per channel) | `Level` |
//! | [`ScalarTap`] | [`ScalarReader`] (min/max/last since the last read) | `GainReduction`, `Scalar` |
//! | [`ScopeTap`] | [`ScopeReader`] (triggered window) | `Waveform` |
//! | [`EnvelopeTap`] | [`EnvelopeReader`] (stage/position/level per voice) | `Envelope` |
//!
//! Every reader is a [`FeedSource`]; a [`Publisher`] holds one per catalog
//! feed, applies `request_feeds` options, and emits due frames on
//! [`Publisher::poll`].
//!
//! ```
//! use crate::feed::dsp::{Aggregate, Publisher, history, scalar, spectrum};
//! use crate::feed::proto::{FeedInfo, FeedKind, FeedRequest};
//!
//! let sr = 48_000.0;
//! // Made once, when the block is built:
//! let (mut spec_tap, spec) = spectrum(4096, sr);
//! let (gr_tap, gr) = scalar(Aggregate::Min, 0.0);
//! let (mut trace_tap, trace) = history(1024, sr, 30.0, Aggregate::Min, 0.0);
//! let mut feeds = Publisher::new();
//! feeds.add(FeedInfo::new("comp/spectrum", "Spectrum", FeedKind::spectrum(64)), spec);
//! feeds.add(FeedInfo::new("comp/gr", "GR", FeedKind::GainReduction), gr);
//! feeds.add(FeedInfo::new("comp/gr_trace", "GR history", FeedKind::gr_history(256)), trace);
//!
//! // Audio thread, per block:
//! let block = [0.0f32; 256];
//! spec_tap.push(&block);
//! gr_tap.push(-3.5);
//! trace_tap.push(-3.5, block.len() as u32);
//!
//! // Control thread: a surface asks, a timer publishes.
//! feeds.request(&[FeedRequest::new("comp/spectrum").bins(128).smoothing(1.0 / 6.0)]).unwrap();
//! let mut frames = Vec::new();
//! feeds.poll(0.0, |f| frames.push(f));
//! assert_eq!(frames.len(), 3);
//! assert_eq!(frames[0].values.len(), 128);
//! ```

pub mod envelope;
pub mod fft;
pub mod history;
pub mod level;
pub mod publish;
pub mod ring;
pub mod scope;
pub mod spectrum;

pub use envelope::{EnvelopeReader, EnvelopeTap, envelope};
pub use fft::Fft;
pub use history::{Aggregate, HistoryReader, HistoryTap, history};
pub use level::{LevelReader, LevelTap, ScalarReader, ScalarTap, level, scalar};
pub use publish::{FeedSource, Publisher};
pub use ring::{RingReader, RingWriter, ring};
pub use scope::{ScopeReader, ScopeTap, scope};
pub use spectrum::{SpectrumAnalyzer, SpectrumConfig, SpectrumTap, spectrum};

/// What silence reads, dBFS.
pub const FLOOR_DB: f64 = -120.0;

/// Linear amplitude → dBFS, never below [`FLOOR_DB`].
#[must_use]
#[inline]
pub fn amp_to_db(amp: f64) -> f64 {
    if amp > 1e-6 {
        (20.0 * amp.log10()).max(FLOOR_DB)
    } else {
        FLOOR_DB
    }
}
