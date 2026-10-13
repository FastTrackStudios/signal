//! Audio sources — REAPER's streaming model.
//!
//! Real DAWs never decode whole files into RAM: uncompressed PCM is
//! random-access in the file, so playback reads the needed sample window
//! per block straight from disk (the OS page cache + read-ahead are the
//! buffer), and RAM stays flat regardless of project size. REAPER's
//! "media buffer" is exactly this plus worker-thread read-ahead.
//!
//! [`AudioSource`] is that abstraction:
//! - [`AudioSource::PcmFile`] — a memory-mapped WAV
//!   ([`fts_sample::mapped::PcmFile`]; opening parses the header only —
//!   instant, no decode, no allocation proportional to the file); samples
//!   convert to f32 on the fly per block.
//! - [`AudioSource::Memory`] — fully decoded PCM for compressed formats
//!   (MP3/FLAC/OGG/AAC), the eager fallback.

use super::decoder::DecodedAudio;

/// The memory-mapped PCM file source — fts-sample's mapper.
#[cfg(not(target_arch = "wasm32"))]
pub use fts_sample::mapped::{PcmFile, PcmFormat};

/// A playable audio source: streamed PCM or decoded memory.
pub enum AudioSource {
    /// Fully decoded interleaved f32 (compressed formats).
    Memory(DecodedAudio),
    /// Memory-mapped WAV, converted per block (uncompressed formats).
    #[cfg(not(target_arch = "wasm32"))]
    PcmFile(PcmFile),
    /// Decoded a window at a time around the playhead, from a compressed
    /// proxy held in memory ([`super::streamed`]) — the browser's source.
    Streamed(super::streamed::Streamed),
}

impl AudioSource {
    pub fn channels(&self) -> u16 {
        match self {
            AudioSource::Memory(d) => d.channels,
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => p.channels(),
            AudioSource::Streamed(s) => s.channels(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioSource::Memory(d) => d.sample_rate,
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => p.sample_rate(),
            AudioSource::Streamed(s) => s.sample_rate(),
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            AudioSource::Memory(d) => d.frame_count(),
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => p.frames(),
            AudioSource::Streamed(s) => usize::try_from(s.frames()).unwrap_or(usize::MAX),
        }
    }

    /// Duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate() == 0 {
            return 0.0;
        }
        self.frame_count() as f64 / self.sample_rate() as f64
    }

    /// Linearly interpolated stereo read between frames `i0` and `i1`
    /// (mono duplicates; >2ch reads the first pair) — the mixer's inner
    /// sampling primitive.
    #[inline]
    pub fn stereo_interp(&self, i0: usize, i1: usize, frac: f32) -> (f32, f32) {
        match self {
            AudioSource::Memory(d) => {
                let ch = d.channels.max(1) as usize;
                let s = &d.samples;
                let read = |i: usize, c: usize| -> f32 {
                    s.get(i * ch + c.min(ch - 1)).copied().unwrap_or(0.0)
                };
                let l = read(i0, 0) + (read(i1, 0) - read(i0, 0)) * frac;
                let r = read(i0, 1) + (read(i1, 1) - read(i0, 1)) * frac;
                (l, r)
            }
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => {
                let l0 = p.sample(i0, 0);
                let l1 = p.sample(i1, 0);
                let l = l0 + (l1 - l0) * frac;
                // Mono (most stems): both channels read the same data —
                // skip the second decode entirely.
                if p.channels() <= 1 {
                    return (l, l);
                }
                let r0 = p.sample(i0, 1);
                let r1 = p.sample(i1, 1);
                (l, r0 + (r1 - r0) * frac)
            }
            AudioSource::Streamed(s) => {
                let l0 = s.sample(i0, 0);
                let l = l0 + (s.sample(i1, 0) - l0) * frac;
                if s.channels() <= 1 {
                    return (l, l);
                }
                let r0 = s.sample(i0, 1);
                (l, r0 + (s.sample(i1, 1) - r0) * frac)
            }
        }
    }

    /// Linearly interpolated read of one source channel (clamped to
    /// the last channel) — backs REAPER channel modes (mono-of-N,
    /// stereo pair N).
    #[inline]
    pub fn channel_interp(&self, i0: usize, i1: usize, frac: f32, ch: usize) -> f32 {
        match self {
            AudioSource::Memory(d) => {
                let nch = d.channels.max(1) as usize;
                let c = ch.min(nch - 1);
                let s0 = d.samples.get(i0 * nch + c).copied().unwrap_or(0.0);
                let s1 = d.samples.get(i1 * nch + c).copied().unwrap_or(0.0);
                s0 + (s1 - s0) * frac
            }
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => {
                let s0 = p.sample(i0, ch);
                let s1 = p.sample(i1, ch);
                s0 + (s1 - s0) * frac
            }
            AudioSource::Streamed(s) => {
                let s0 = s.sample(i0, ch);
                s0 + (s.sample(i1, ch) - s0) * frac
            }
        }
    }

    /// Prefetch a window (no-op for in-memory sources). A streamed source
    /// takes it as where playback is reading, and decodes there next.
    #[allow(unused_variables)]
    pub fn prefetch(&self, start_frame: usize, frames: usize) {
        if let AudioSource::Streamed(s) = self {
            s.want(start_frame as u64);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let AudioSource::PcmFile(p) = self {
            p.prefetch(start_frame, frames);
        }
    }

    /// Min/max of one channel over `[lo, hi)` frames, in one linear
    /// pass over the raw storage.
    ///
    /// This is what makes peak building cheap: `sample()` per frame
    /// costs a bounds check, an offset multiply and a format dispatch
    /// *each*, and a peaks pass touches every frame of every take.
    /// Here the dispatch happens once and the inner loop is a bare
    /// stride walk — an order of magnitude on a whole session.
    pub fn min_max_block(&self, lo: usize, hi: usize, channel: usize) -> (f32, f32) {
        let (mut mn, mut mx) = (f32::MAX, f32::MIN);
        match self {
            AudioSource::Memory(d) => {
                let ch = d.channels.max(1) as usize;
                let c = channel.min(ch - 1);
                let hi = hi.min(d.frame_count());
                if lo >= hi {
                    return (0.0, 0.0);
                }
                let mut i = lo * ch + c;
                let end = hi * ch;
                while i < end {
                    let v = d.samples[i];
                    mn = mn.min(v);
                    mx = mx.max(v);
                    i += ch;
                }
            }
            // Read below, one sample at a time: only the resident window.
            AudioSource::Streamed(_) => {}
            #[cfg(not(target_arch = "wasm32"))]
            AudioSource::PcmFile(p) => {
                let ch = p.channels().max(1) as usize;
                let c = channel.min(ch - 1);
                let hi = hi.min(p.frames());
                if lo >= hi {
                    return (0.0, 0.0);
                }
                let bps = p.format().bytes_per_sample();
                let stride = ch * bps;
                let start = p.data_offset() + (lo * ch + c) * bps;
                let end = p.data_offset() + (hi * ch + c) * bps;
                let b: &[u8] = p.bytes();
                if end > b.len() {
                    // Truncated file: fall back to the checked reader.
                    for i in lo..hi {
                        let v = p.sample(i, c);
                        mn = mn.min(v);
                        mx = mx.max(v);
                    }
                } else {
                    match p.format() {
                        PcmFormat::I16 => {
                            let mut off = start;
                            let (mut imn, mut imx) = (i16::MAX, i16::MIN);
                            while off < end {
                                let v = i16::from_le_bytes([b[off], b[off + 1]]);
                                imn = imn.min(v);
                                imx = imx.max(v);
                                off += stride;
                            }
                            mn = imn as f32 / 32768.0;
                            mx = imx as f32 / 32768.0;
                        }
                        PcmFormat::I24 => {
                            let mut off = start;
                            while off < end {
                                let v = ((b[off] as i32) << 8
                                    | (b[off + 1] as i32) << 16
                                    | (b[off + 2] as i32) << 24)
                                    >> 8;
                                let v = v as f32 / 8_388_608.0;
                                mn = mn.min(v);
                                mx = mx.max(v);
                                off += stride;
                            }
                        }
                        _ => {
                            for i in lo..hi {
                                let v = p.sample(i, c);
                                mn = mn.min(v);
                                mx = mx.max(v);
                            }
                        }
                    }
                }
            }
        }
        if let AudioSource::Streamed(s) = self {
            let hi = hi.min(usize::try_from(s.frames()).unwrap_or(usize::MAX));
            for i in lo..hi {
                let v = s.sample(i, channel);
                mn = mn.min(v);
                mx = mx.max(v);
            }
        }
        if mn > mx { (0.0, 0.0) } else { (mn, mx) }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

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
        let dir = std::env::temp_dir().join("fts_pcm_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ramp.wav");
        let samples: Vec<i16> = (0..1000).map(|i| (i * 16) as i16).collect();
        write_wav(&path, &samples, 48000);

        let p = PcmFile::open(&path).unwrap();
        assert_eq!(p.channels(), 1);
        assert_eq!(p.sample_rate(), 48000);
        assert_eq!(p.frames(), 1000);
        assert!((p.sample(10, 0) - (160.0 / 32768.0)).abs() < 1e-6);
        // Mono duplicates to both channels through the source API.
        let src = AudioSource::PcmFile(p);
        let (l, r) = src.stereo_interp(10, 11, 0.0);
        assert_eq!(l, r);
        assert!((l - (160.0 / 32768.0)).abs() < 1e-6);
        // Interpolation midpoint.
        let (l, _) = src.stereo_interp(10, 11, 0.5);
        assert!((l - (168.0 / 32768.0)).abs() < 1e-6);
    }
}
