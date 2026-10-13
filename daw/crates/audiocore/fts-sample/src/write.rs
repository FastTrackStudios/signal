//! The shared WAV writer: planar `f32` channels out as 32-bit IEEE float.
//!
//! Float rather than 16- or 24-bit int because these files are typically
//! intermediates a later edit reads back: there is no headroom decision to
//! make, nothing to dither, and a round trip costs nothing.

use std::path::Path;

use crate::SamplerError;

/// Write planar channels (`channels[ch][frame]`) as a 32-bit float WAV.
///
/// All channels must be the same length.
pub fn write_wav_f32(
    path: &Path,
    sample_rate: u32,
    channels: &[Vec<f32>],
) -> Result<(), SamplerError> {
    let Some(first) = channels.first() else {
        return Err(SamplerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "write_wav_f32: no channels",
        )));
    };
    let frames = first.len();
    if channels.iter().any(|c| c.len() != frames) {
        return Err(SamplerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "write_wav_f32: channel lengths differ",
        )));
    }

    let spec = hound::WavSpec {
        channels: channels.len() as u16,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(to_err)?;
    for frame in 0..frames {
        for ch in channels {
            writer.write_sample(ch[frame]).map_err(to_err)?;
        }
    }
    writer.finalize().map_err(to_err)
}

fn to_err(e: hound::Error) -> SamplerError {
    match e {
        hound::Error::IoError(e) => SamplerError::Io(e),
        other => SamplerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            other.to_string(),
        )),
    }
}
