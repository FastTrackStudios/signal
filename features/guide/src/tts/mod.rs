//! TTS cues: pre-rendered spoken announcements for the guide bus.
//!
//! TTS is NOT realtime-safe, so nothing here runs on the render path.
//! At setlist-build time a [`CueBank`] renders each cue text ("Chorus",
//! "Bridge in 2", ...) to mono PCM via a [`TtsRenderer`], writes it to a
//! wav cache keyed by SHA-256 of `(text, voice)`, and hands the engine's
//! `SampleBank` preloaded samples under [`tts_cue_key`] keys.
//!
//! Reading previously cached wavs needs only `hound` — the `tts` cargo
//! feature (chatterbox-rs / ONNX Runtime) is required only to render
//! texts that are not cached yet.

#[cfg(feature = "tts")]
mod chatterbox;

#[cfg(feature = "tts")]
pub use chatterbox::{ChatterboxTts, ChatterboxTtsConfig};

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::samples::{load_wav, AudioSample, SampleBank};
use crate::GuideError;

/// The guide-sample key a spoken cue is stored under in the `SampleBank`.
#[must_use]
pub fn tts_cue_key(text: &str) -> String {
    format!("tts:{text}")
}

/// Mono PCM produced by a TTS backend.
#[derive(Debug, Clone)]
pub struct TtsAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// A text-to-speech backend. Implemented by [`ChatterboxTts`] behind the
/// `tts` feature; hosts can supply their own (e.g. a remote service or a
/// test stub). Never called on the audio thread.
pub trait TtsRenderer {
    /// Stable identifier of the voice/model — part of the cache key, so
    /// changing voices re-renders cues.
    fn voice_id(&self) -> &str;

    /// Synthesize `text` to mono PCM.
    ///
    /// # Errors
    ///
    /// Returns an error if synthesis fails.
    fn render(&mut self, text: &str) -> Result<TtsAudio, GuideError>;
}

/// Disk-cached bank of spoken cues.
pub struct CueBank {
    cache_dir: PathBuf,
    voice_id: String,
}

impl CueBank {
    /// `cache_dir` is caller-supplied (e.g. `~/.config/fts/tts-cache/`);
    /// it is created on first write. `voice_id` must match the renderer's.
    pub fn new(cache_dir: impl Into<PathBuf>, voice_id: impl Into<String>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            voice_id: voice_id.into(),
        }
    }

    /// The wav path a cue text caches to: `<dir>/<sha256(text \0 voice)>.wav`.
    #[must_use]
    pub fn cache_path(&self, text: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        hasher.update([0u8]);
        hasher.update(self.voice_id.as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        });
        self.cache_dir.join(format!("{hex}.wav"))
    }

    /// Load a cached cue (does not require the `tts` feature), resampled
    /// to `target_sample_rate`. Returns `None` when not cached.
    pub fn load_cached(&self, text: &str, target_sample_rate: u32) -> Option<AudioSample> {
        let path = self.cache_path(text);
        if !path.exists() {
            return None;
        }
        match load_wav(&path, target_sample_rate) {
            Ok(sample) => Some(sample),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to load cached TTS cue");
                None
            }
        }
    }

    /// Get a cue's PCM, rendering + caching it via `tts` on a cache miss.
    ///
    /// # Errors
    ///
    /// Returns `GuideError` if the text cannot be synthesized.
    pub fn get_or_render(
        &self,
        text: &str,
        target_sample_rate: u32,
        tts: &mut dyn TtsRenderer,
    ) -> Result<AudioSample, GuideError> {
        debug_assert_eq!(
            tts.voice_id(),
            self.voice_id,
            "CueBank voice_id must match the renderer's"
        );
        if let Some(sample) = self.load_cached(text, target_sample_rate) {
            return Ok(sample);
        }

        let audio = tts.render(text)?;
        let path = self.cache_path(text);
        if let Err(e) = write_wav_mono(&path, &audio) {
            // Cache write failure is non-fatal — still return the PCM.
            warn!(path = %path.display(), error = %e, "Failed to write TTS cue cache");
        } else {
            info!(text, path = %path.display(), "Rendered + cached TTS cue");
        }
        Ok(AudioSample::mono(audio.samples, audio.sample_rate).resampled_to(target_sample_rate))
    }

    /// Preload cues for `texts` into `bank` under [`tts_cue_key`] keys.
    ///
    /// With `tts: None` only already-cached cues load (feature-free path);
    /// uncached texts are skipped with a warning. Returns the texts that
    /// were loaded.
    pub fn preload_into(
        &self,
        bank: &mut SampleBank,
        texts: &[String],
        target_sample_rate: u32,
        mut tts: Option<&mut (dyn TtsRenderer + '_)>,
    ) -> Vec<String> {
        let mut loaded = Vec::new();
        for text in texts {
            let sample = tts.as_deref_mut().map_or_else(
                || self.load_cached(text, target_sample_rate),
                |renderer| match self.get_or_render(text, target_sample_rate, renderer) {
                    Ok(sample) => Some(sample),
                    Err(e) => {
                        warn!(text, error = %e, "TTS render failed for cue");
                        None
                    }
                },
            );
            match sample {
                Some(sample) => {
                    bank.insert_guide(tts_cue_key(text), sample);
                    loaded.push(text.clone());
                }
                None => {
                    warn!(text, "No TTS cue available (not cached, no renderer)");
                }
            }
        }
        loaded
    }
}

/// Write mono f32 PCM as 16-bit wav (the cache format).
fn write_wav_mono(path: &Path, audio: &TtsAudio) -> Result<(), GuideError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GuideError::CueCache(format!("{}: {e}", parent.display())))?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| GuideError::CueCache(format!("{}: {e}", path.display())))?;
    for s in &audio.samples {
        // Safe to use to_int_unchecked: value is clamped to i16 range
        let v = unsafe {
            (s.clamp(-1.0, 1.0) * f32::from(i16::MAX))
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                .to_int_unchecked::<i16>()
        };
        writer
            .write_sample(v)
            .map_err(|e| GuideError::CueCache(format!("{}: {e}", path.display())))?;
    }
    writer
        .finalize()
        .map_err(|e| GuideError::CueCache(format!("{}: {e}", path.display())))?;
    Ok(())
}
