//! Load an IR from disk into deinterleaved f64 buffers at a target SR.
//!
//! Uses `symphonium` for decoding + resampling. WAV / AIFF / FLAC / OGG
//! / MP3 all work depending on which features the workspace enables on
//! the `symphonium` dep (default workspace config: wav/pcm + fft
//! resampler).

use std::path::{Path, PathBuf};

use symphonium::{ResampleQuality, SymphoniumLoader};
use thiserror::Error;

/// Decoded impulse response, resampled to `sample_rate`. Channels are
/// stored deinterleaved (`channels[c][i]`).
#[derive(Debug, Clone)]
pub struct IrAsset {
    pub channels: Vec<Vec<f64>>,
    pub sample_rate: f64,
    pub source_path: Option<PathBuf>,
    pub original_sample_rate: f64,
}

impl IrAsset {
    /// Decode an audio file and resample to `target_sample_rate`. Mono
    /// IRs come back with 1 channel; multichannel IRs preserve their
    /// channel count (transforms layer collapses to stereo later).
    pub fn load<P: AsRef<Path>>(path: P, target_sample_rate: f64) -> Result<Self, IrLoadError> {
        let path = path.as_ref();
        let mut loader = SymphoniumLoader::new();
        let target_sr = target_sample_rate as u32;
        let decoded = loader
            .load_f32(
                path,
                Some(target_sr),
                ResampleQuality::High,
                Some(256 * 1024 * 1024), // 256 MB cap
            )
            .map_err(|e| IrLoadError::Decode(format!("{e:?}")))?;

        let channels: Vec<Vec<f64>> = decoded
            .data
            .iter()
            .map(|ch| ch.iter().map(|&s| s as f64).collect())
            .collect();

        if channels.is_empty() || channels.iter().any(|c| c.is_empty()) {
            return Err(IrLoadError::Empty);
        }

        Ok(Self {
            channels,
            sample_rate: target_sample_rate,
            source_path: Some(path.to_path_buf()),
            original_sample_rate: decoded.sample_rate as f64,
        })
    }

    /// Construct directly from f64 mono samples (e.g. synthetic IRs).
    pub fn from_mono(samples: Vec<f64>, sample_rate: f64) -> Self {
        Self {
            channels: vec![samples],
            sample_rate,
            source_path: None,
            original_sample_rate: sample_rate,
        }
    }

    /// Construct from a stereo pair already at the target SR.
    pub fn from_stereo(left: Vec<f64>, right: Vec<f64>, sample_rate: f64) -> Self {
        Self {
            channels: vec![left, right],
            sample_rate,
            source_path: None,
            original_sample_rate: sample_rate,
        }
    }

    pub fn frames(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / self.sample_rate.max(1.0)
    }
}

#[derive(Debug, Error)]
pub enum IrLoadError {
    #[error("audio decode failed: {0}")]
    Decode(String),
    #[error("decoded audio was empty")]
    Empty,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
