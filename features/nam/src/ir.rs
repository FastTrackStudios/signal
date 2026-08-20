use crate::NamError;
use std::path::Path;

/// Metadata extracted from a WAV file header.
#[derive(Debug, Clone)]
pub struct IrMetadata {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub duration_ms: f64,
}

/// Parse a WAV header to extract IR metadata — header only, no sample
/// decode, so the library scanner stays cheap over big IR banks.
pub fn parse_wav_header(path: &Path) -> Result<IrMetadata, NamError> {
    let info = fts_sample::probe(path).map_err(|e| NamError::ParseError(e.to_string()))?;
    Ok(IrMetadata {
        channels: info.channels,
        sample_rate: info.sample_rate,
        bits_per_sample: info.bits_per_sample,
        duration_ms: info.duration_secs() * 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid WAV file in memory.
    fn make_test_wav(sample_rate: u32, channels: u16, bits: u16, num_samples: u32) -> Vec<u8> {
        let bytes_per_sample = bits / 8;
        let data_size = num_samples * channels as u32 * bytes_per_sample as u32;
        let file_size = 36 + data_size;

        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = channels * bytes_per_sample;
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(44 + data_size as usize, 0); // silence
        buf
    }

    #[test]
    fn parse_wav_header_basic() {
        let wav = make_test_wav(48000, 1, 24, 48000); // 1 second mono 24-bit
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        std::fs::write(&path, &wav).unwrap();

        let meta = parse_wav_header(&path).unwrap();
        assert_eq!(meta.channels, 1);
        assert_eq!(meta.sample_rate, 48000);
        assert_eq!(meta.bits_per_sample, 24);
        assert!((meta.duration_ms - 1000.0).abs() < 1.0);
    }

    #[test]
    fn parse_wav_stereo() {
        let wav = make_test_wav(44100, 2, 16, 22050); // 0.5 second stereo 16-bit
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        std::fs::write(&path, &wav).unwrap();

        let meta = parse_wav_header(&path).unwrap();
        assert_eq!(meta.channels, 2);
        assert_eq!(meta.sample_rate, 44100);
        assert!((meta.duration_ms - 500.0).abs() < 1.0);
    }
}
