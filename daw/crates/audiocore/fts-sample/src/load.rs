//! Resident (fully-decoded) loads over symphonium: any format the enabled
//! symphonium features can decode, with optional resampling to a target
//! rate at a caller-chosen [`ResampleQuality`].

use std::path::Path;

pub use symphonium::ResampleQuality;
use symphonium::SymphoniumLoader;
use tracing::error;

use crate::SamplerError;

/// Decoded planar audio: `channels[ch][frame]` `f32`, at [`sample_rate`].
///
/// [`sample_rate`]: Self::sample_rate
#[derive(Clone, Debug)]
pub struct LoadedAudio {
    /// Planar channel data: `channels[ch][frame]`.
    pub channels: Vec<Vec<f32>>,
    /// Sample rate of the decoded audio (after any resampling).
    pub sample_rate: u32,
}

impl LoadedAudio {
    /// Number of sample frames.
    pub fn num_frames(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    /// Number of channels.
    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }
}

/// Load an audio file as planar `f32` channels.
///
/// `target_sr = Some(rate)` resamples to `rate` at `quality`; `None` keeps
/// the file's native rate (`quality` is then unused).
pub fn load_planar_f32(
    path: &Path,
    target_sr: Option<u32>,
    quality: ResampleQuality,
) -> Result<LoadedAudio, SamplerError> {
    let mut loader = SymphoniumLoader::new();
    let decoded = loader
        .load_f32(path, target_sr, quality, None)
        .map_err(|e| SamplerError::Decode(format!("{}: {e}", path.display())))?;
    Ok(LoadedAudio {
        channels: decoded.data,
        sample_rate: decoded.sample_rate,
    })
}

/// Decode a whole in-memory audio file (any container/codec the enabled
/// symphonium features cover — `decode-compressed` adds mp3/ogg/flac/aac)
/// to planar `f32` at its native rate.
///
/// `ext_hint` is a file-extension hint only: the content is probed, and a
/// file named `.wav` that is really a FLAC still decodes. This is the
/// resident path for bytes that never had a stable on-disk home (project
/// containers, browser uploads); on-disk files use [`load_planar_f32`].
pub fn decode_bytes(data: &[u8], ext_hint: Option<&str>) -> Result<LoadedAudio, SamplerError> {
    decode_bytes_at(data, ext_hint, None, ResampleQuality::default())
}

/// [`decode_bytes`], resampled to `target_sr` at `quality` when one is
/// given — the in-memory twin of [`load_planar_f32`], for bytes a browser
/// fetched (a guide library's samples, say).
pub fn decode_bytes_at(
    data: &[u8],
    ext_hint: Option<&str>,
    target_sr: Option<u32>,
    quality: ResampleQuality,
) -> Result<LoadedAudio, SamplerError> {
    let mut hint = symphonium::symphonia::core::probe::Hint::new();
    if let Some(ext) = ext_hint {
        hint.with_extension(ext);
    }
    let cursor = std::io::Cursor::new(data.to_vec());
    let mut loader = SymphoniumLoader::new();
    let decoded = loader
        .load_f32_from_source(
            Box::new(cursor),
            Some(hint),
            target_sr,
            quality,
            // No RAM cap here: the caller owns the budget accounting, and a
            // long take must decode rather than error.
            Some(usize::MAX),
        )
        .map_err(|e| SamplerError::Decode(format!("{} bytes: {e}", data.len())))?;
    Ok(LoadedAudio {
        channels: decoded.data,
        sample_rate: decoded.sample_rate,
    })
}

/// Load an audio file as mono `f32` (downmix by averaging channels),
/// returning the samples and the rate they are at.
///
/// `target_sr` / `quality` as for [`load_planar_f32`].
pub fn load_mono_f32(
    path: &Path,
    target_sr: Option<u32>,
    quality: ResampleQuality,
) -> Result<(Vec<f32>, u32), SamplerError> {
    let audio = load_planar_f32(path, target_sr, quality)?;
    match audio.channels.len() {
        0 => Ok((Vec::new(), audio.sample_rate)),
        1 => {
            let sample_rate = audio.sample_rate;
            let mut channels = audio.channels;
            Ok((channels.remove(0), sample_rate))
        }
        n => {
            let frames = audio.num_frames();
            let scale = 1.0 / n as f32;
            let mut mono = vec![0.0f32; frames];
            for ch in &audio.channels {
                for (out, &s) in mono.iter_mut().zip(ch) {
                    *out += s;
                }
            }
            for s in &mut mono {
                *s *= scale;
            }
            Ok((mono, audio.sample_rate))
        }
    }
}

/// Decoded audio data in a format suitable for DSP processing.
///
/// Stores interleaved stereo `f64` sample pairs. Mono sources are
/// duplicated to both channels during loading.
#[derive(Clone)]
pub struct AudioData {
    /// Stereo sample data: `[left, right]` pairs.
    pub data: Vec<[f64; 2]>,
    /// Sample rate of the decoded audio (after resampling).
    pub sample_rate: f64,
    /// Original file name (stem only, no extension).
    pub name: String,
}

impl AudioData {
    /// Number of sample frames.
    pub fn num_frames(&self) -> usize {
        self.data.len()
    }

    /// Duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.data.len() as f64 / self.sample_rate
    }

    /// Number of channels (always 2 — mono sources are duplicated).
    pub fn num_channels(&self) -> usize {
        2
    }
}

/// Load an audio file synchronously, resampling to `target_sr`.
///
/// Supports any format Symphonium can decode (WAV, FLAC, MP3, OGG, etc.
/// depending on enabled features).
pub fn load_audio(
    path: &Path,
    target_sr: f64,
) -> Result<AudioData, Box<dyn std::error::Error + Send + Sync>> {
    let decoded = load_planar_f32(path, Some(target_sr as u32), ResampleQuality::default())
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(format!("{e}")))
        })?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sample")
        .to_string();

    let num_channels = decoded.num_channels();
    if num_channels == 0 {
        return Err("Empty audio file".into());
    }

    let num_frames = decoded.channels[0].len();
    let mut data = Vec::with_capacity(num_frames);
    for frame in 0..num_frames {
        let l = decoded.channels[0][frame] as f64;
        let r = if num_channels > 1 {
            decoded.channels[1][frame] as f64
        } else {
            l
        };
        data.push([l, r]);
    }

    Ok(AudioData {
        data,
        sample_rate: target_sr,
        name,
    })
}

/// Spawn a background thread to load an audio file and send the result
/// through a crossbeam channel.
///
/// `id` is an opaque identifier passed through to the message (e.g. slot index).
/// `make_msg` converts the loaded `AudioData` + `id` into the channel's message type.
pub fn load_audio_async<M: Send + 'static>(
    path: String,
    target_sr: f64,
    tx: crossbeam_channel::Sender<M>,
    id: usize,
    make_msg: fn(AudioData, usize) -> M,
) {
    std::thread::spawn(move || match load_audio(Path::new(&path), target_sr) {
        Ok(audio) => {
            let _ = tx.send(make_msg(audio, id));
        }
        Err(e) => {
            error!("Failed to load audio (id={id}): {e}");
        }
    });
}
