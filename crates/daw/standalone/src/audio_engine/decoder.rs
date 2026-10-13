//! Audio file decoding — a thin wrapper over `fts_sample::decode_bytes`.
//!
//! Decodes audio files into interleaved f32 PCM buffers that the mixer
//! can play back. Supports WAV, MP3, OGG/Vorbis, FLAC, and AAC (the
//! fts-sample `decode-compressed` set).
//!
//! Every decoded buffer is charged against the process-wide preload RAM
//! budget ([`fts_sample::budget`]) so the DAW's resident decodes share
//! one ceiling with the sampler engine.

use tracing::debug;

/// Decoded audio data: interleaved f32 PCM samples.
#[derive(Clone)]
pub struct DecodedAudio {
    /// Interleaved f32 samples (e.g., [L0, R0, L1, R1, ...] for stereo)
    pub samples: Vec<f32>,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Sample rate in Hz (e.g., 44100, 48000)
    pub sample_rate: u32,
    /// RAM-budget charge for `samples`, released when the last clone
    /// drops. `None` for synthesized buffers (test tones, click tracks).
    charge: Option<fts_sample::budget::Charge>,
}

impl DecodedAudio {
    /// A buffer that does NOT count against the preload budget —
    /// synthesized audio (test tones, clicks) whose size the caller
    /// already controls.
    pub fn new(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Self {
        Self {
            samples,
            channels,
            sample_rate,
            charge: None,
        }
    }

    /// A buffer charged against the process-wide preload RAM budget
    /// (released when the last clone drops) — every decode of project
    /// media goes through here. `src` labels the origin for the
    /// over-budget warning (an extension or a short tag, never a full
    /// path).
    ///
    /// When the budget would be exceeded the buffer still loads —
    /// playback must not silently fail; the DAW has no streaming
    /// fallback for compressed media yet (the future butler/streaming
    /// seam) — but the overrun gets one alertable warning.
    pub fn charged(samples: Vec<f32>, channels: u16, sample_rate: u32, src: &str) -> Self {
        let bytes = samples.len() * core::mem::size_of::<f32>();
        if (bytes as u64) > fts_sample::budget::remaining_bytes() {
            tracing::warn!(
                bytes,
                src,
                used_mb = fts_sample::budget::used_bytes() / (1024 * 1024),
                budget_mb = fts_sample::budget::limit_bytes() / (1024 * 1024),
                "daw: decoded audio exceeds the preload budget — loading anyway \
                 (no compressed streaming fallback yet)"
            );
        }
        Self {
            samples,
            channels,
            sample_rate,
            charge: Some(fts_sample::budget::Charge::now(bytes)),
        }
    }

    /// Total number of sample frames (samples per channel).
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    /// Duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frame_count() as f64 / self.sample_rate as f64
    }

    /// Convert a time in seconds to a sample frame index.
    pub fn seconds_to_frame(&self, seconds: f64) -> usize {
        (seconds * self.sample_rate as f64) as usize
    }
}

/// Decode audio from raw bytes.
///
/// The format is auto-detected from the data (no file extension needed).
/// Returns `None` if the data cannot be decoded.
pub fn decode_audio(data: &[u8]) -> Option<DecodedAudio> {
    from_loaded(fts_sample::decode_bytes(data, None).ok()?, "?")
}

/// Decode audio from raw bytes with a format hint.
///
/// Use this when you know the file extension (e.g., "mp3", "wav"). The
/// extension is a hint only: the content is probed, and a file named
/// `.wav` that is really a FLAC still decodes.
pub fn decode_audio_with_extension(data: &[u8], extension: &str) -> Option<DecodedAudio> {
    from_loaded(
        fts_sample::decode_bytes(data, Some(extension)).ok()?,
        extension,
    )
}

/// Interleave fts-sample's planar channels into the mixer's layout and
/// attach the budget charge.
fn from_loaded(loaded: fts_sample::LoadedAudio, src: &str) -> Option<DecodedAudio> {
    let channels = loaded.num_channels();
    if channels == 0 {
        return None;
    }
    let frames = loaded.num_frames();
    let mut samples = vec![0.0f32; frames * channels];
    for (c, ch) in loaded.channels.iter().enumerate() {
        for (frame, &s) in ch.iter().enumerate() {
            samples[frame * channels + c] = s;
        }
    }

    debug!(
        frames,
        channels,
        sample_rate = loaded.sample_rate,
        "decoded audio"
    );

    Some(DecodedAudio::charged(
        samples,
        channels as u16,
        loaded.sample_rate,
        src,
    ))
}
