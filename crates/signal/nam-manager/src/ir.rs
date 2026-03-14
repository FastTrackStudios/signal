use crate::NamError;
use std::io::Read;
use std::path::Path;

/// Metadata extracted from a WAV file header.
#[derive(Debug, Clone)]
pub struct IrMetadata {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub duration_ms: f64,
}

/// Parse WAV header (RIFF/PCM) to extract IR metadata.
///
/// WAV layout (first 44 bytes for standard PCM):
/// ```text
///  0..4   "RIFF"
///  4..8   file size - 8
///  8..12  "WAVE"
/// 12..16  "fmt "
/// 16..20  fmt chunk size (16 for PCM)
/// 20..22  audio format (1 = PCM)
/// 22..24  num channels
/// 24..28  sample rate
/// 28..32  byte rate
/// 32..34  block align
/// 34..36  bits per sample
/// ...     (possible extra fmt bytes)
/// data chunk: "data" + 4-byte size + samples
/// ```
pub fn parse_wav_header(path: &Path) -> Result<IrMetadata, NamError> {
    let mut file = std::fs::File::open(path)?;

    let mut header = [0u8; 44];
    file.read_exact(&mut header)?;

    // Validate RIFF header
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(NamError::ParseError(format!(
            "{}: not a valid WAV file",
            path.display()
        )));
    }

    let channels = u16::from_le_bytes([header[22], header[23]]);
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    let bits_per_sample = u16::from_le_bytes([header[34], header[35]]);

    // Find the data chunk to get its size for duration calculation.
    // For standard 44-byte headers, data starts at offset 36 with "data" marker.
    // But some WAV files have extra fmt bytes, so we search for the "data" marker.
    let data_size = find_data_chunk_size(&mut file, &header)?;

    let bytes_per_sample = bits_per_sample as u32 / 8;
    let total_samples = if bytes_per_sample > 0 && channels > 0 {
        data_size / (bytes_per_sample * channels as u32)
    } else {
        0
    };
    let duration_ms = if sample_rate > 0 {
        (total_samples as f64 / sample_rate as f64) * 1000.0
    } else {
        0.0
    };

    Ok(IrMetadata {
        channels,
        sample_rate,
        bits_per_sample,
        duration_ms,
    })
}

/// Search for the "data" chunk and return its size.
fn find_data_chunk_size(
    file: &mut std::fs::File,
    header: &[u8; 44],
) -> Result<u32, NamError> {
    // Check the standard position first (offset 36)
    if &header[36..40] == b"data" {
        return Ok(u32::from_le_bytes([
            header[40], header[41], header[42], header[43],
        ]));
    }

    // If not at standard position, scan forward from after the fmt chunk.
    // The fmt chunk size is at offset 16.
    let fmt_size = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
    let scan_start = 20 + fmt_size as usize; // after fmt chunk

    // Re-read from scan_start by seeking
    use std::io::Seek;
    file.seek(std::io::SeekFrom::Start(scan_start as u64))?;

    // Scan for "data" chunk (read 8 bytes at a time: 4 id + 4 size)
    let mut chunk_header = [0u8; 8];
    for _ in 0..100 {
        // safety limit
        if file.read_exact(&mut chunk_header).is_err() {
            break;
        }
        if &chunk_header[0..4] == b"data" {
            return Ok(u32::from_le_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]));
        }
        // Skip this chunk's data
        let chunk_size = u32::from_le_bytes([
            chunk_header[4],
            chunk_header[5],
            chunk_header[6],
            chunk_header[7],
        ]);
        file.seek(std::io::SeekFrom::Current(chunk_size as i64))?;
    }

    Err(NamError::ParseError("WAV file has no data chunk".into()))
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
