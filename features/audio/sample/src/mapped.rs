//! Memory-mapped uncompressed PCM files — the streaming-file source.
//!
//! Real DAWs never decode whole files into RAM: uncompressed PCM is
//! random-access in the file, so playback reads the needed sample window
//! per block straight from disk (the OS page cache + read-ahead are the
//! buffer), and RAM stays flat regardless of project size. REAPER's
//! "media buffer" is exactly this plus worker-thread read-ahead.
//!
//! [`PcmFile`] is that model for WAV: opening runs the header-only
//! [`probe`](crate::probe) (no decode, no allocation proportional to the
//! file) and maps the file; samples convert to f32 on the fly per read.
//! Compressed formats have no random-access samples and take the resident
//! decode path instead (`decode_bytes` under the `load` feature).

use std::path::Path;

use crate::probe::{probe, WavFormat};
use crate::SamplerError;

/// Sample encodings we read directly from WAV `data` chunks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PcmFormat {
    I16,
    I24,
    I32,
    F32,
    F64,
}

impl PcmFormat {
    /// Bytes per sample of one channel.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            PcmFormat::I16 => 2,
            PcmFormat::I24 => 3,
            PcmFormat::I32 | PcmFormat::F32 => 4,
            PcmFormat::F64 => 8,
        }
    }
}

/// The page size the read-ahead steps by (4 KiB: the smallest the kernel
/// maps, so a step never skips a page).
const PAGE: usize = 4096;

/// A memory-mapped uncompressed PCM file (WAV). The mapping is backed by
/// the OS page cache — reading pages them in on demand and the kernel may
/// evict them under pressure; resident memory stays proportional to what's
/// actually being played, not the project's media size.
pub struct PcmFile {
    map: memmap2::Mmap,
    /// Byte offset of the `data` chunk's samples.
    data_offset: usize,
    frames: usize,
    channels: u16,
    sample_rate: u32,
    format: PcmFormat,
}

impl PcmFile {
    /// Open + parse the RIFF/WAVE header (via [`probe`]). Cheap: no sample
    /// data is read.
    // The mmap call is the one necessary unsafe here — a read-only mapping
    // whose only hazard (concurrent truncation of a media file mid-play)
    // REAPER accepts too.
    #[allow(unsafe_code)]
    pub fn open(path: &Path) -> Result<Self, SamplerError> {
        let info = probe(path)?;
        let format = match (info.format, info.bits_per_sample) {
            (WavFormat::Pcm, 16) => PcmFormat::I16,
            (WavFormat::Pcm, 24) => PcmFormat::I24,
            (WavFormat::Pcm, 32) => PcmFormat::I32,
            (WavFormat::Float, 32) => PcmFormat::F32,
            (WavFormat::Float, 64) => PcmFormat::F64,
            (fmt, bits) => {
                return Err(SamplerError::Decode(format!(
                    "{}: unsupported wav format {fmt:?}/{bits}-bit",
                    path.display()
                )));
            }
        };
        if info.channels == 0 || info.sample_rate == 0 {
            return Err(SamplerError::Probe(format!(
                "{}: degenerate wav header",
                path.display()
            )));
        }
        if info.data_offset == 0 {
            return Err(SamplerError::Probe(format!(
                "{}: no data chunk",
                path.display()
            )));
        }

        let file = std::fs::File::open(path)?;
        // SAFETY: the mapping is read-only; concurrent file truncation
        // would fault, which we accept for media files.
        let map = unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| SamplerError::Probe(format!("{}: mmap: {e}", path.display())))?;
        // Hint the kernel we'll read mostly sequentially around the playhead.
        let _ = map.advise(memmap2::Advice::Sequential);

        // Clamp to what the mapping actually holds: a truncated file (or a
        // lying data-chunk size) must never read past the map.
        let data_offset = info.data_offset as usize;
        let frame_bytes = format.bytes_per_sample() * info.channels as usize;
        let avail = map.len().saturating_sub(data_offset) / frame_bytes;
        let frames = (info.num_frames as usize).min(avail);

        Ok(Self {
            map,
            data_offset,
            frames,
            channels: info.channels,
            sample_rate: info.sample_rate,
            format,
        })
    }

    /// Number of sample frames in the `data` chunk.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Channel count.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The sample encoding of the `data` chunk.
    pub fn format(&self) -> PcmFormat {
        self.format
    }

    /// Byte offset of the first sample within [`bytes`](Self::bytes).
    pub fn data_offset(&self) -> usize {
        self.data_offset
    }

    /// The whole mapping — for bulk scans (peak building) that want one
    /// format dispatch and a bare stride walk instead of per-frame
    /// [`sample`](Self::sample) calls.
    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    /// One sample (frame, channel) as f32. Bounds-checked; out of range
    /// reads 0.
    #[inline]
    pub fn sample(&self, frame: usize, channel: usize) -> f32 {
        if frame >= self.frames {
            return 0.0;
        }
        let ch = channel.min(self.channels as usize - 1);
        let bps = self.format.bytes_per_sample();
        let off = self.data_offset + (frame * self.channels as usize + ch) * bps;
        let b = &self.map[..];
        if off + bps > b.len() {
            return 0.0;
        }
        match self.format {
            PcmFormat::I16 => i16::from_le_bytes([b[off], b[off + 1]]) as f32 / 32768.0,
            PcmFormat::I24 => {
                let v =
                    ((b[off] as i32) << 8 | (b[off + 1] as i32) << 16 | (b[off + 2] as i32) << 24)
                        >> 8;
                v as f32 / 8_388_608.0
            }
            PcmFormat::I32 => {
                i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as f32
                    / 2_147_483_648.0
            }
            PcmFormat::F32 => f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]),
            PcmFormat::F64 => f64::from_le_bytes([
                b[off],
                b[off + 1],
                b[off + 2],
                b[off + 3],
                b[off + 4],
                b[off + 5],
                b[off + 6],
                b[off + 7],
            ]) as f32,
        }
    }

    /// Bring the pages covering `[start_frame, start_frame+frames)` into
    /// memory now, on the calling thread — the read-ahead thread, ahead of
    /// the playhead — so the audio thread never waits on the disk for them.
    ///
    /// `WillNeed` alone is a hint the kernel may act on late or not at all:
    /// played cold, the audio callback still page-faulted onto the disk and
    /// stalled ~14 ms (a 2.7 ms block's deadline five times over). So after
    /// the hint, one byte of every page is read: a page not yet in memory
    /// loads here, blocking only this thread. Never call it from the audio
    /// callback.
    pub fn prefetch(&self, start_frame: usize, frames: usize) {
        let bps = self.format.bytes_per_sample() * self.channels as usize;
        let start = self.data_offset + start_frame.min(self.frames) * bps;
        let len = (frames * bps).min(self.map.len().saturating_sub(start));
        if len == 0 {
            return;
        }
        let _ = self.map.advise_range(memmap2::Advice::WillNeed, start, len);
        let mut touched = 0u8;
        for page in (start..start + len).step_by(PAGE) {
            touched ^= self.map.get(page).copied().unwrap_or(0);
        }
        // The loads are the point, not the value: `black_box` keeps the
        // optimiser from dropping them.
        std::hint::black_box(touched);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal 16-bit mono WAV with a known ramp.
    fn write_wav(path: &Path, samples: &[i16], rate: u32) {
        let data_len = samples.len() * 2;
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&((36 + data_len) as u32).to_le_bytes())
            .unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * 2).to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&(data_len as u32).to_le_bytes()).unwrap();
        for s in samples {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn pcm_file_reads_back_samples() {
        let dir = std::env::temp_dir().join("fts_sample_mapped_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ramp.wav");
        let samples: Vec<i16> = (0..1000).map(|i| (i * 16) as i16).collect();
        write_wav(&path, &samples, 48000);

        let p = PcmFile::open(&path).unwrap();
        assert_eq!(p.channels(), 1);
        assert_eq!(p.sample_rate(), 48000);
        assert_eq!(p.frames(), 1000);
        assert_eq!(p.format(), PcmFormat::I16);
        assert_eq!(p.data_offset(), 44);
        assert!((p.sample(10, 0) - (160.0 / 32768.0)).abs() < 1e-6);
        // Out-of-range frame reads silence, not garbage.
        assert_eq!(p.sample(1000, 0), 0.0);
    }

    /// Real-corpus smoke: open the PT session's WAVs without reading them
    /// into RAM (skips when the drive is absent).
    #[test]
    fn opens_real_session_wavs() {
        let dir = std::path::Path::new(
            "/run/media/cody/15B0-07EB/Transfer May 26th Done/\u{1f535} PNG Project - PT Sessions/02 LORD OF THE FIGHT/Audio Files",
        );
        let Ok(entries) = std::fs::read_dir(dir) else {
            return; // session drive absent — skip
        };
        let mut n = 0;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "wav") {
                continue;
            }
            // macOS AppleDouble droppings ("._foo.wav") aren't audio.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("._"))
            {
                continue;
            }
            let p = match PcmFile::open(&path) {
                Ok(p) => p,
                Err(err) => panic!("{path:?}: {err}"),
            };
            assert!(p.frames() > 0);
            assert!(p.sample_rate() >= 8000);
            // Read a sample from the middle — pages in one page only.
            let _ = p.sample(p.frames() / 2, 0);
            n += 1;
            if n >= 25 {
                break;
            }
        }
        assert!(n > 0, "no wavs found");
    }
}
