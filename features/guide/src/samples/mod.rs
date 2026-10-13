//! Preloaded PCM samples and wav loading.
//!
//! The guide engine mixes preloaded PCM at scheduled sample positions
//! (same approach as the legacy fts-guide plugin). Decoding goes through
//! the shared `fts-sample` loader (its `load` feature only, keeping this
//! crate iOS-lean); the small linear resampler below stays for in-memory
//! rate changes — no platform audio I/O.

mod bank;
pub mod library;
mod loader;

pub use bank::{
    get_guide_key, section_to_guide_filename, ClickSamplePaths, ClickSound, SampleBank,
};
pub use loader::load_wav;

/// Decoded, planar (per-channel) f32 PCM.
///
/// Mirrors the shape of symphonium's `DecodedAudioF32` that the legacy
/// players consumed: `data[channel][frame]`.
#[derive(Debug, Clone, Default)]
pub struct AudioSample {
    /// Planar channel data: `data[channel][frame]`.
    pub data: Vec<Vec<f32>>,
    /// Sample rate this PCM is at.
    pub sample_rate: u32,
}

impl AudioSample {
    /// Create a mono sample from raw PCM.
    #[must_use]
    pub fn mono(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            data: vec![samples],
            sample_rate,
        }
    }

    /// Number of frames (samples per channel).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.first().map_or(0, std::vec::Vec::len)
    }

    /// Number of channels.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.data.len()
    }

    /// Linear-resample to `target_rate` (no-op if already there).
    ///
    /// Linear interpolation is plenty for click/voice cues; heavy resamplers
    /// stay out of the processing core.
    #[must_use]
    pub fn resampled_to(&self, target_rate: u32) -> Self {
        if self.sample_rate == target_rate || self.sample_rate == 0 || self.frames() == 0 {
            let mut out = self.clone();
            out.sample_rate = target_rate.max(out.sample_rate);
            return out;
        }
        let ratio = f64::from(self.sample_rate) / f64::from(target_rate);
        // Calculate output frames using integer arithmetic to avoid precision loss from f64 conversions.
        // This avoids casting usize to f64 and back: instead we compute the scaled frame count directly.
        let target_rate_usize = usize::try_from(target_rate).unwrap_or(usize::MAX);
        let sample_rate_usize = usize::try_from(self.sample_rate).unwrap_or(usize::MAX);
        let out_frames = self
            .frames()
            .saturating_mul(target_rate_usize)
            .saturating_add(sample_rate_usize / 2)
            .checked_div(sample_rate_usize)
            .unwrap_or(0);
        let data = self
            .data
            .iter()
            .map(|ch| {
                (0..out_frames)
                    .map(|i| {
                        let src = crate::cast::f64_from_usize(i) * ratio;
                        let i0: usize = crate::cast::usize_from_f64_floor_nonneg(src);
                        let i1 = i0.saturating_add(1).min(ch.len().saturating_sub(1));
                        let frac: f32 = crate::cast::f32_from_f64_saturating(src.fract());
                        let a = ch.get(i0).copied().unwrap_or(0.0);
                        let b = ch.get(i1).copied().unwrap_or(0.0);
                        (b - a).mul_add(frac, a)
                    })
                    .collect()
            })
            .collect();
        Self {
            data,
            sample_rate: target_rate,
        }
    }
}
