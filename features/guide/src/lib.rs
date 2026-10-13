// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! session-guide — portable guide engine for the session domain.
//!
//! Port of the legacy FTS-Guide REAPER plugin (click track, count-in,
//! section guide) with the REAPER cdylib shell dropped. The engine is a
//! synchronous processing core: no audio I/O, no threads, no DAW deps —
//! the host drives it with a per-block [`BlockClock`] (sample position +
//! tempo/time-signature) and it mixes preloaded PCM into caller buffers.
//! It runs inside daw-standalone's render callback or standalone.
//!
//! ```no_run
//! use session_guide::{BlockClock, GuideConfig, GuideEngine, GuideSongTiming, sections_from_song};
//! # fn demo(song: &session_proto::Song, click_dir: &std::path::Path) {
//! let mut engine = GuideEngine::new(GuideConfig::default());
//! engine.bank_mut().load_click(click_dir, session_guide::ClickSound::Blip, 48_000);
//! engine.set_sections(&sections_from_song(song), &GuideSongTiming::from_song(song));
//!
//! // In the render callback:
//! let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
//! let clock = BlockClock {
//!     playing: true,
//!     pos_seconds: 0.0,
//!     pos_beats: 0.0,
//!     tempo_bpm: 120.0,
//!     time_sig_num: 4,
//!     time_sig_den: 4,
//!     sample_rate: 48_000.0,
//! };
//! engine.render_stereo(&mut l, &mut r, &clock);
//! # }
//! ```
//!
//! TTS cues ("Chorus", "Bridge in 2") are pre-rendered at setlist-build
//! time by [`CueBank`] (see the [`tts`] module); the optional `tts` cargo
//! feature adds a local Chatterbox backend.

pub mod audio;
mod cast;
pub mod count_in;
mod engine;
pub mod midi;
pub mod samples;
mod schedule;
pub mod tts;

pub use engine::{
    BlockClock, ClickSubdivisions, GuideBuses, GuideConfig, GuideEngine, GuideTrigger,
    TriggerSource,
};
pub use samples::{get_guide_key, section_to_guide_filename, AudioSample, ClickSound, SampleBank};
pub use schedule::{
    sections_from_song, CountInOptions, CueEvent, CueSchedule, GuideSection, GuideSongTiming,
    ScheduleOptions, ScheduledCue,
};
pub use tts::{tts_cue_key, CueBank, TtsAudio, TtsRenderer};

#[cfg(feature = "tts")]
pub use tts::{ChatterboxTts, ChatterboxTtsConfig};

/// Errors from sample loading, cue caching, and TTS rendering.
/// Nothing on the render path returns errors.
#[derive(Debug, thiserror::Error)]
pub enum GuideError {
    #[error("failed to load sample: {0}")]
    SampleLoad(String),
    #[error("TTS cue cache error: {0}")]
    CueCache(String),
    #[error("TTS error: {0}")]
    Tts(String),
}
