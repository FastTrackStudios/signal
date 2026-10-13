//! Header-only audio probing — reads the RIFF chunk list, never decodes
//! samples, so scanning a whole library (IR banks, DrumGizmo kits) costs a
//! few short reads per file rather than gigabytes of I/O.
//!
//! Walks the chunk list rather than assuming `fmt ` sits at byte 12. It
//! usually does, but a file with a `JUNK` or `bext` chunk in front is
//! perfectly legal and a fixed offset reads garbage from it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::SamplerError;

/// How samples are encoded, as the `fmt ` chunk's format tag reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WavFormat {
    /// Tag 1 — integer PCM.
    Pcm,
    /// Tag 3 — IEEE float.
    Float,
    /// Tag 0xFFFE — the real format hides in the extension's sub-format
    /// GUID, whose first two bytes are one of the tags above.
    Extensible(u16),
    /// Anything else, reported rather than rejected: the point of a
    /// probe is to say what is there.
    Other(u16),
}

impl WavFormat {
    fn from_tag(tag: u16) -> Self {
        match tag {
            1 => Self::Pcm,
            3 => Self::Float,
            0xFFFE => Self::Extensible(0),
            other => Self::Other(other),
        }
    }

    /// The effective encoding, resolving an extensible header to the
    /// sub-format it actually carries.
    pub fn effective(self) -> Self {
        match self {
            Self::Extensible(sub) => Self::from_tag(sub),
            other => other,
        }
    }
}

/// What a file's header says, without decoding any audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    /// Frames in the `data` chunk — 0 when the chunk is absent or the
    /// header gives no usable frame size.
    pub num_frames: u64,
    /// Byte offset of the `data` chunk's samples within the file — 0 when
    /// the chunk is absent (sample data can never start at byte 0). This
    /// is what a memory-mapper needs to address samples directly.
    pub data_offset: u64,
    /// The effective encoding (extensible headers resolved to their
    /// sub-format).
    pub format: WavFormat,
}

impl AudioInfo {
    /// Duration in seconds (0.0 when the header gives no rate or length).
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.num_frames as f64 / self.sample_rate as f64
        }
    }
}

/// Read one WAV file's header. Header-only: no sample decode.
pub fn probe(path: &Path) -> Result<AudioInfo, SamplerError> {
    let mut file = File::open(path)?;
    let mut riff = [0u8; 12];
    file.read_exact(&mut riff)?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(SamplerError::Probe(format!(
            "{}: not a RIFF/WAVE file",
            path.display()
        )));
    }

    let mut fmt: Option<Vec<u8>> = None;
    let mut data_size: Option<u64> = None;
    let mut data_offset: u64 = 0;
    let mut header = [0u8; 8];
    loop {
        if file.read_exact(&mut header).is_err() {
            break;
        }
        let id = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        match &id {
            b"fmt " => {
                let mut buf = vec![0u8; size as usize];
                file.read_exact(&mut buf)?;
                // Chunks are word-aligned: an odd size is followed by a
                // pad byte that is not counted in the size.
                if size & 1 == 1 {
                    file.seek(SeekFrom::Current(1))?;
                }
                fmt = Some(buf);
            }
            b"data" => {
                data_size = Some(u64::from(size));
                data_offset = file.stream_position()?;
                file.seek(SeekFrom::Current(size as i64 + (size as i64 & 1)))?;
            }
            _ => {
                file.seek(SeekFrom::Current(size as i64 + (size as i64 & 1)))?;
            }
        }
        if fmt.is_some() && data_size.is_some() {
            break;
        }
    }

    let Some(fmt) = fmt else {
        return Err(SamplerError::Probe(format!(
            "{}: no fmt chunk",
            path.display()
        )));
    };
    parse_fmt(&fmt, data_size.unwrap_or(0), data_offset, path)
}

fn parse_fmt(
    fmt: &[u8],
    data_size: u64,
    data_offset: u64,
    path: &Path,
) -> Result<AudioInfo, SamplerError> {
    if fmt.len() < 16 {
        return Err(SamplerError::Probe(format!(
            "{}: fmt chunk is {} bytes, want 16",
            path.display(),
            fmt.len()
        )));
    }
    let tag = u16::from_le_bytes([fmt[0], fmt[1]]);
    let mut format = WavFormat::from_tag(tag);
    // WAVE_FORMAT_EXTENSIBLE: the sub-format GUID starts at offset 24
    // of the chunk and its first two bytes are the real format tag.
    if let WavFormat::Extensible(_) = format {
        if fmt.len() >= 26 {
            format = WavFormat::Extensible(u16::from_le_bytes([fmt[24], fmt[25]]));
        }
    }
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
    // Frame size: trust `block_align` when it is sane, else compute it
    // from channels × sample width.
    let block_align = u16::from_le_bytes([fmt[12], fmt[13]]) as u64;
    let frame_bytes = if block_align > 0 {
        block_align
    } else {
        u64::from(channels) * u64::from(bits_per_sample / 8)
    };
    let num_frames = if frame_bytes > 0 {
        data_size / frame_bytes
    } else {
        0
    };
    Ok(AudioInfo {
        channels,
        sample_rate: u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]),
        bits_per_sample,
        num_frames,
        data_offset,
        format: format.effective(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A unique temp path (std-only; no tempfile dep).
    fn temp_wav(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fts-sample-probe-{}-{name}.wav",
            std::process::id()
        ))
    }

    /// Build a minimal WAV: optional leading junk chunk, a `fmt ` chunk of
    /// `fmt_body` bytes, and a `data` chunk of `data_size` zero bytes.
    fn make_wav(junk: Option<usize>, fmt_body: &[u8], data_size: u32) -> Vec<u8> {
        let mut chunks = Vec::new();
        if let Some(n) = junk {
            chunks.extend_from_slice(b"JUNK");
            chunks.extend_from_slice(&(n as u32).to_le_bytes());
            chunks.resize(chunks.len() + n + (n & 1), 0);
        }
        chunks.extend_from_slice(b"fmt ");
        chunks.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
        chunks.extend_from_slice(fmt_body);
        chunks.extend_from_slice(b"data");
        chunks.extend_from_slice(&data_size.to_le_bytes());
        chunks.resize(chunks.len() + data_size as usize, 0);

        let mut buf = Vec::with_capacity(12 + chunks.len());
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(4 + chunks.len() as u32).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(&chunks);
        buf
    }

    fn fmt_body(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let mut b = Vec::new();
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
        b.extend_from_slice(&block_align.to_le_bytes());
        b.extend_from_slice(&bits.to_le_bytes());
        b
    }

    #[test]
    fn probe_pcm_basic() {
        let path = temp_wav("pcm");
        // 1 s mono 24-bit at 48 kHz.
        let body = fmt_body(1, 1, 48_000, 24);
        std::fs::write(&path, make_wav(None, &body, 48_000 * 3)).unwrap();
        let info = probe(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(info.channels, 1);
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.bits_per_sample, 24);
        assert_eq!(info.num_frames, 48_000);
        assert_eq!(info.data_offset, 44); // 12 RIFF + 8+16 fmt + 8 data header
        assert_eq!(info.format, WavFormat::Pcm);
        assert!((info.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn probe_float_stereo_with_junk_chunk() {
        let path = temp_wav("float-junk");
        // 0.5 s stereo 32-bit float at 44.1 kHz, JUNK chunk before fmt.
        let body = fmt_body(3, 2, 44_100, 32);
        std::fs::write(&path, make_wav(Some(37), &body, 22_050 * 8)).unwrap();
        let info = probe(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 44_100);
        assert_eq!(info.format, WavFormat::Float);
        assert_eq!(info.num_frames, 22_050);
        // 12 RIFF + (8 + 37 + 1 pad) JUNK + (8 + 16) fmt + 8 data header.
        assert_eq!(info.data_offset, 90);
    }

    #[test]
    fn probe_extensible_resolves_sub_format() {
        let path = temp_wav("extensible");
        // WAVE_FORMAT_EXTENSIBLE (0xFFFE) wrapping IEEE float: 40-byte fmt
        // chunk, sub-format GUID at offset 24 starting with the real tag.
        let mut body = fmt_body(0xFFFE, 15, 48_000, 32);
        body.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        body.extend_from_slice(&32u16.to_le_bytes()); // valid bits
        body.extend_from_slice(&0u32.to_le_bytes()); // channel mask
        body.extend_from_slice(&3u16.to_le_bytes()); // sub-format tag: float
        body.resize(40, 0); // rest of the GUID
        std::fs::write(&path, make_wav(None, &body, 15 * 4 * 100)).unwrap();
        let info = probe(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(info.channels, 15);
        assert_eq!(info.format, WavFormat::Float);
        assert_eq!(info.num_frames, 100);
    }

    #[test]
    fn probe_rejects_non_wav() {
        let path = temp_wav("not-wav");
        std::fs::write(&path, b"this is not a riff file at all").unwrap();
        let err = probe(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(matches!(err, SamplerError::Probe(_)));
    }
}
