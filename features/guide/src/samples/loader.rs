//! wav file loading, via the shared `fts-sample` loader (`load` feature
//! only — the heavy pack engine stays out of this iOS-clean crate).

use std::path::Path;

use crate::GuideError;

use super::AudioSample;

/// Load an audio file as planar f32 PCM, resampled to `target_sample_rate`.
///
/// Guide assets are wav, but anything symphonium decodes works.
///
/// # Errors
///
/// Returns `GuideError::SampleLoad` if the audio file cannot be loaded or decoded.
pub fn load_wav(path: &Path, target_sample_rate: u32) -> Result<AudioSample, GuideError> {
    // Low quality = linear interpolation — the same fidelity the old local
    // resampler had, which is plenty for click/voice cues.
    let loaded = fts_sample::load_planar_f32(
        path,
        Some(target_sample_rate),
        fts_sample::ResampleQuality::Low,
    )
    .map_err(|e| GuideError::SampleLoad(e.to_string()))?;

    Ok(AudioSample {
        data: loaded.channels,
        sample_rate: loaded.sample_rate,
    })
}
