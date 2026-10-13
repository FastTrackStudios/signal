//! REAPER `.reapeaks` peak-cache files.
//!
//! REAPER builds a peak cache next to each media file (or in the project's
//! `peaks/` folder) and draws items from it instead of re-reading the audio.
//! Reading these lets us draw waveforms **exactly the way REAPER does** —
//! same mipmap levels, same min/max columns.
//!
//! Format (reverse-engineered, validated byte-exact against a 70-file corpus
//! written by REAPER 7.x; little-endian throughout):
//!
//! ```text
//! magic       4 bytes  "RPKN" (older writers: "RPKL", same layout)
//! nch         u8       channel count
//! nlevels     u8       mipmap level count (REAPER writes 3)
//! samplerate  u32      source sample rate
//! src_stamp   u64      source size (high 32) | mtime in unix secs (low 32)
//! levels      nlevels × { samples_per_peak: u32, peak_count: u32 }
//! data        per level, peak_count × nch × { max: i16, min: i16 }
//! ```
//!
//! REAPER's ladder is **relative to the sample rate**: the finest level is
//! `samplerate / 300` samples per peak (160 at 48 kHz, 147 at 44.1 kHz),
//! then ×15 and ×20 — `147 / 2205 / 44100` for a 44.1 kHz stem, as the
//! sessions in `sessions/Always On Time/Media/peaks` are written. A
//! hard-coded 160 reads fine (the level table is explicit) but *writes*
//! a file REAPER will not recognize as its own, so [`fine_spp`] is the
//! one place that number comes from.
//!
//! Each peak is the (max, min) sample pair over its window — waveforms are
//! asymmetric, so both bounds matter. To render at a given zoom, pick the
//! finest level whose `samples_per_peak` ≤ samples-per-pixel (mip-mapping),
//! then aggregate the covered peaks per pixel column.

use std::io::Read;
use std::path::Path;

/// REAPER's finest level covers `samplerate / 300` samples — 160 at
/// 48 kHz, 147 at 44.1 kHz, 320 at 96 kHz.
///
/// A rate of 0 (a source that would not say) falls back to 160 so the
/// ladder is still a ladder.
#[must_use]
pub fn fine_spp(samplerate: u32) -> usize {
    if samplerate == 0 {
        160
    } else {
        (samplerate as usize / 300).max(1)
    }
}

/// REAPER's 8-byte source stamp: size in the high 32 bits, mtime in
/// unix seconds in the low 32. Both are truncated to 32 bits, exactly as
/// REAPER truncates them — a file over 4 GiB wraps, and so does its
/// cache, which is REAPER's behaviour and not a bug to fix here.
#[must_use]
pub fn stamp(size_bytes: u64, mtime_secs: u64) -> u64 {
    ((size_bytes & 0xFFFF_FFFF) << 32) | (mtime_secs & 0xFFFF_FFFF)
}

/// A sample as the file stores it: signed 16-bit, full scale at ±1.
fn quantize(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// One coarser level, folded from `src` by `factor`.
fn fold(src: &PeakLevel, nch: usize, factor: usize) -> PeakLevel {
    let count = src.count.div_ceil(factor);
    let mut data: Vec<i16> = Vec::with_capacity(count * nch * 2);
    for p in 0..count {
        let a = p * factor;
        let b = (a + factor).min(src.count);
        for ch in 0..nch {
            let (mut max, mut min) = (i16::MIN, i16::MAX);
            for s in a..b {
                let i = (s * nch + ch) * 2;
                max = max.max(src.data[i]);
                min = min.min(src.data[i + 1]);
            }
            if max < min {
                max = 0;
                min = 0;
            }
            data.push(max);
            data.push(min);
        }
    }
    PeakLevel {
        samples_per_peak: src.samples_per_peak * factor as u32,
        count,
        data,
    }
}

/// The finest mipmap level, built a block at a time.
///
/// [`ReaPeaks::compute`] wants random access into the source, which a
/// memory-mapped WAV gives and a compressed stream does not: an Ogg
/// proxy is decoded forward, window by window, and seeking backwards to
/// re-read a frame costs a page re-decode. This accumulates the same
/// `(max, min)` per [`fine_spp`] frames from whatever arrives next, so
/// both paths write the same file.
///
/// Allocation-free per block after the first: the running window is two
/// floats a channel.
#[derive(Clone, Debug)]
pub struct PeaksBuilder {
    channels: usize,
    samplerate: u32,
    /// Source frames behind one peak — [`fine_spp`] of the rate.
    per: usize,
    /// The window in progress, per channel.
    max: Vec<f32>,
    min: Vec<f32>,
    /// Frames into the window in progress.
    filled: usize,
    /// Channel the next sample belongs to — carried across calls, so a
    /// block may end mid-frame.
    ch: usize,
    /// Closed windows, in file order.
    data: Vec<i16>,
    count: usize,
}

impl PeaksBuilder {
    /// An empty builder for a source of `channels` at `samplerate`.
    pub fn new(channels: usize, samplerate: u32) -> Self {
        let nch = channels.max(1);
        Self {
            channels: nch,
            samplerate,
            per: fine_spp(samplerate),
            max: vec![f32::MIN; nch],
            min: vec![f32::MAX; nch],
            filled: 0,
            ch: 0,
            data: Vec::new(),
            count: 0,
        }
    }

    /// Frames summarized so far — closed windows plus the one in
    /// progress.
    pub fn frames(&self) -> u64 {
        self.count as u64 * self.per as u64 + self.filled as u64
    }

    /// Fold in the next interleaved block. A block need not be a whole
    /// number of frames or windows; whatever is left over stays in the
    /// running window for the next call.
    pub fn push_interleaved(&mut self, samples: &[f32]) {
        for &v in samples {
            self.max[self.ch] = self.max[self.ch].max(v);
            self.min[self.ch] = self.min[self.ch].min(v);
            self.ch += 1;
            if self.ch == self.channels {
                self.ch = 0;
                self.filled += 1;
                if self.filled == self.per {
                    self.close();
                }
            }
        }
    }

    /// Close the window in progress into the level.
    fn close(&mut self) {
        for ch in 0..self.channels {
            let (max, min) = if self.max[ch] < self.min[ch] {
                (0.0, 0.0)
            } else {
                (self.max[ch], self.min[ch])
            };
            self.data.push(quantize(max));
            self.data.push(quantize(min));
            self.max[ch] = f32::MIN;
            self.min[ch] = f32::MAX;
        }
        self.filled = 0;
        self.count += 1;
    }

    /// The finished three-level mipmap. A partly filled last window is
    /// closed as it stands — the same rounding [`ReaPeaks::compute`]
    /// does with `ceil(frames / fine_spp)`.
    pub fn finish(mut self) -> ReaPeaks {
        if self.filled > 0 {
            self.close();
        }
        ReaPeaks::from_fine(
            self.channels,
            self.samplerate,
            PeakLevel {
                samples_per_peak: self.per as u32,
                count: self.count,
                data: self.data,
            },
        )
    }
}

/// One mipmap level: `(max, min)` peak pairs per channel, channel-interleaved
/// per peak (`ch0 max, ch0 min, ch1 max, ch1 min, …`).
#[derive(Clone, Debug)]
pub struct PeakLevel {
    /// Source samples summarized per peak (REAPER: `sr/300`, ×15, ×20 —
    /// 160 / 2400 / 48000 at 48 kHz, 147 / 2205 / 44100 at 44.1 kHz).
    pub samples_per_peak: u32,
    /// Peak count (per channel).
    pub count: usize,
    /// Interleaved `(max, min)` i16 pairs: `count × nch × 2` values.
    pub data: Vec<i16>,
}

impl PeakLevel {
    /// The `(max, min)` pair for `peak` on `channel`, normalized to −1…1.
    pub fn pair(&self, nch: usize, channel: usize, peak: usize) -> (f32, f32) {
        let i = (peak * nch + channel) * 2;
        let max = self.data.get(i).copied().unwrap_or(0) as f32 / 32767.0;
        let min = self.data.get(i + 1).copied().unwrap_or(0) as f32 / 32767.0;
        (max, min)
    }
}

/// A parsed `.reapeaks` file.
#[derive(Clone, Debug)]
pub struct ReaPeaks {
    pub channels: usize,
    pub samplerate: u32,
    /// REAPER's source stamp, raw: the media file's **size in bytes** in
    /// the high 32 bits, its **mtime in unix seconds** in the low 32.
    ///
    /// Decoded with [`Self::source_size`] / [`Self::source_mtime_secs`],
    /// built with [`stamp`]. It was read as a bare timestamp until the
    /// real sessions were checked: every file in
    /// `sessions/Always On Time/Media/peaks` carries
    /// `0x06A039AA_67AA831E`, whose halves are exactly the stem's 111 MB
    /// size and its mtime. A cache written with a bare timestamp is one
    /// REAPER rebuilds on sight, and a REAPER cache read as one looks
    /// permanently stale — which is why both halves matter.
    pub source_stamp: u64,
    /// Mipmap levels, finest first (as stored).
    pub levels: Vec<PeakLevel>,
}

/// Parse / IO errors.
#[derive(Debug, thiserror::Error)]
pub enum ReaPeaksError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a .reapeaks file (bad magic {0:02x?})")]
    BadMagic([u8; 4]),
    #[error("truncated file")]
    Truncated,
}

impl ReaPeaks {
    /// Read and parse a `.reapeaks` file.
    pub fn read(path: impl AsRef<Path>) -> Result<Self, ReaPeaksError> {
        let mut buf = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut buf)?;
        Self::parse(&buf)
    }

    /// Parse from bytes.
    pub fn parse(b: &[u8]) -> Result<Self, ReaPeaksError> {
        let need = |n: usize| {
            if b.len() < n {
                Err(ReaPeaksError::Truncated)
            } else {
                Ok(())
            }
        };
        need(18)?;
        let magic: [u8; 4] = b[0..4].try_into().unwrap();
        // RPKN = current writer; RPKL = older REAPER versions, same layout
        // (validated byte-exact on both).
        if &magic != b"RPKN" && &magic != b"RPKL" {
            return Err(ReaPeaksError::BadMagic(magic));
        }
        let nch = b[4] as usize;
        let nlevels = b[5] as usize;
        let samplerate = u32::from_le_bytes(b[6..10].try_into().unwrap());
        let source_stamp = u64::from_le_bytes(b[10..18].try_into().unwrap());

        let table_end = 18 + nlevels * 8;
        need(table_end)?;
        let mut levels = Vec::with_capacity(nlevels);
        let mut off = table_end;
        for i in 0..nlevels {
            let e = 18 + i * 8;
            let samples_per_peak = u32::from_le_bytes(b[e..e + 4].try_into().unwrap());
            let count = u32::from_le_bytes(b[e + 4..e + 8].try_into().unwrap()) as usize;
            let bytes = count * nch * 4;
            need(off + bytes)?;
            let data = b[off..off + bytes]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            levels.push(PeakLevel {
                samples_per_peak,
                count,
                data,
            });
            off += bytes;
        }
        Ok(Self {
            channels: nch,
            samplerate,
            source_stamp,
            levels,
        })
    }

    /// Compute peak mipmaps from raw samples — for media REAPER hasn't
    /// cached yet. `read(frame, channel)` returns the sample (−1…1).
    /// Levels match REAPER's stock 160 / 2400 / 48000; the coarser two
    /// are folded from the finest, so the source is read exactly once.
    pub fn compute(
        channels: usize,
        samplerate: u32,
        frames: usize,
        read: impl Fn(usize, usize) -> f32,
    ) -> Self {
        let nch = channels.max(1);
        let per = fine_spp(samplerate);
        let count = frames.div_ceil(per);
        let mut data: Vec<i16> = Vec::with_capacity(count * nch * 2);
        for p in 0..count {
            let a = p * per;
            let b = (a + per).min(frames);
            for ch in 0..nch {
                let (mut max, mut min) = (f32::MIN, f32::MAX);
                for f in a..b {
                    let v = read(f, ch);
                    max = max.max(v);
                    min = min.min(v);
                }
                if max < min {
                    max = 0.0;
                    min = 0.0;
                }
                data.push(quantize(max));
                data.push(quantize(min));
            }
        }
        Self::from_fine(
            nch,
            samplerate,
            PeakLevel {
                samples_per_peak: per as u32,
                count,
                data,
            },
        )
    }

    /// The three-level mipmap around an already-built finest level —
    /// REAPER's stock ladder, the coarser two folded from the finest
    /// (2400 = 15×160, 48000 = 20×2400) so the source is read once.
    ///
    /// Split out of [`Self::compute`] so a source that can only be read
    /// *forward* — a decoded Ogg proxy — can build the fine level with
    /// [`PeaksBuilder`] and still land on the same file.
    pub fn from_fine(channels: usize, samplerate: u32, fine: PeakLevel) -> Self {
        let nch = channels.max(1);
        let mid = fold(&fine, nch, 15);
        let coarse = fold(&mid, nch, 20);
        Self {
            channels: nch,
            samplerate,
            source_stamp: 0,
            levels: vec![fine, mid, coarse],
        }
    }

    /// Serialize to REAPER's `.reapeaks` format (`RPKN`).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RPKN");
        out.push(self.channels as u8);
        out.push(self.levels.len() as u8);
        out.extend_from_slice(&self.samplerate.to_le_bytes());
        out.extend_from_slice(&self.source_stamp.to_le_bytes());
        for l in &self.levels {
            out.extend_from_slice(&l.samples_per_peak.to_le_bytes());
            out.extend_from_slice(&(l.count as u32).to_le_bytes());
        }
        for l in &self.levels {
            for v in &l.data {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }

    /// Write a `.reapeaks` cache file (best effort for read-only media).
    pub fn write(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// The source size the stamp records, in bytes (truncated to 32
    /// bits by the format).
    #[must_use]
    pub fn source_size(&self) -> u64 {
        self.source_stamp >> 32
    }

    /// The source mtime the stamp records, in unix seconds.
    #[must_use]
    pub fn source_mtime_secs(&self) -> u64 {
        self.source_stamp & 0xFFFF_FFFF
    }

    /// Source length in samples (from the finest level).
    pub fn length_samples(&self) -> u64 {
        self.levels
            .first()
            .map(|l| l.count as u64 * l.samples_per_peak as u64)
            .unwrap_or(0)
    }

    /// Source length in seconds.
    pub fn length_seconds(&self) -> f64 {
        if self.samplerate == 0 {
            return 0.0;
        }
        self.length_samples() as f64 / self.samplerate as f64
    }

    /// REAPER's mipmap pick: the *coarsest* level whose resolution still
    /// meets the requested samples-per-pixel (i.e. the largest
    /// `samples_per_peak` ≤ `spp`), falling back to the finest level when
    /// zoomed in past it.
    pub fn level_for(&self, spp: f64) -> &PeakLevel {
        self.levels
            .iter()
            .filter(|l| (l.samples_per_peak as f64) <= spp)
            .max_by_key(|l| l.samples_per_peak)
            .or_else(|| self.levels.iter().min_by_key(|l| l.samples_per_peak))
            .expect("reapeaks has at least one level")
    }

    /// The same peaks for a view that never draws blocks finer than
    /// `block` samples: the finest level folded to the coarsest step that
    /// still fits under `block`, the coarser levels kept, nothing finer.
    /// A browser keeps this instead of the whole cache — the finest level
    /// is most of it (a 5-minute stem's 147-sample level is ~700 KB; the
    /// fold to under 1024 samples is ~100 KB).
    #[must_use]
    pub fn coarsened(&self, block: u32) -> Self {
        let mut levels: Vec<&PeakLevel> = self.levels.iter().collect();
        levels.sort_by_key(|l| l.samples_per_peak);
        let Some(fine) = levels.first() else {
            return self.clone();
        };
        let step = (block / fine.samples_per_peak.max(1)).max(1);
        let spp = fine.samples_per_peak.max(1) * step;
        let nch = self.channels.max(1);
        let per = step as usize;
        let count = fine.count.div_ceil(per);
        let mut data = Vec::with_capacity(count * nch * 2);
        for p in 0..count {
            for c in 0..nch {
                let (mut max, mut min) = (i16::MIN, i16::MAX);
                for q in p * per..((p + 1) * per).min(fine.count) {
                    let i = (q * nch + c) * 2;
                    max = max.max(fine.data.get(i).copied().unwrap_or(0));
                    min = min.min(fine.data.get(i + 1).copied().unwrap_or(0));
                }
                data.push(max);
                data.push(min);
            }
        }
        let mut out = vec![PeakLevel {
            samples_per_peak: spp,
            count,
            data,
        }];
        out.extend(
            levels
                .iter()
                .skip(1)
                .filter(|l| l.samples_per_peak > spp)
                .map(|l| (*l).clone()),
        );
        Self {
            levels: out,
            ..self.clone()
        }
    }

    /// Render `columns` min/max pairs for `channel` over a source time range
    /// (seconds) — one pair per pixel column, REAPER's draw model: pick the
    /// mipmap for the zoom, aggregate every covered peak per column.
    /// Returns normalized `(max, min)` in −1…1.
    pub fn columns(
        &self,
        channel: usize,
        start_s: f64,
        end_s: f64,
        columns: usize,
    ) -> Vec<(f32, f32)> {
        if columns == 0 || end_s <= start_s || self.samplerate == 0 {
            return Vec::new();
        }
        let spp = (end_s - start_s) * self.samplerate as f64 / columns as f64;
        let level = self.level_for(spp);
        let per = level.samples_per_peak as f64;
        let start_peak = start_s * self.samplerate as f64 / per;
        let peaks_per_col = spp / per;

        (0..columns)
            .map(|c| {
                let a = (start_peak + c as f64 * peaks_per_col).floor() as usize;
                let b = ((start_peak + (c + 1) as f64 * peaks_per_col).ceil() as usize)
                    .clamp(a + 1, level.count.max(a + 1));
                let (mut max, mut min) = (f32::MIN, f32::MAX);
                for p in a..b.min(level.count) {
                    let (pmax, pmin) = level.pair(self.channels, channel, p);
                    max = max.max(pmax);
                    min = min.min(pmin);
                }
                if max < min {
                    (0.0, 0.0) // out of range — silence
                } else {
                    (max, min)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn coarsened_keeps_the_extremes_at_a_coarser_step() {
        // Mono, 10 peaks of 160 samples; a spike in peak 7.
        let mut data = Vec::new();
        for p in 0..10i16 {
            let v = if p == 7 { 30_000 } else { 100 * p };
            data.push(v);
            data.push(-v);
        }
        let pk = ReaPeaks {
            levels: vec![
                PeakLevel {
                    samples_per_peak: 160,
                    count: 10,
                    data,
                },
                PeakLevel {
                    samples_per_peak: 2400,
                    count: 1,
                    data: vec![30_000, -30_000],
                },
            ],
            ..ReaPeaks::compute(1, 48_000, 1600, |_, _| 0.0)
        };
        let small = pk.coarsened(1024);
        let fine = &small.levels[0];
        assert_eq!(
            fine.samples_per_peak, 960,
            "six peaks folded: the most under 1024"
        );
        assert_eq!(fine.count, 2);
        assert_eq!(
            fine.data,
            vec![500, -500, 30_000, -30_000],
            "each fold keeps its extremes"
        );
        assert_eq!(small.levels.len(), 2, "the coarser level kept");
        assert_eq!(small.levels[1].samples_per_peak, 2400);
    }

    use super::*;

    /// A synthetic two-level mono file.
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"RPKN");
        b.push(1); // nch
        b.push(2); // levels
        b.extend_from_slice(&48000u32.to_le_bytes());
        b.extend_from_slice(&7u64.to_le_bytes());
        // level table
        b.extend_from_slice(&160u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&2400u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        // level 0: 4 peaks (max, min)
        for (max, min) in [(100i16, -50i16), (200, -100), (300, -150), (400, -200)] {
            b.extend_from_slice(&max.to_le_bytes());
            b.extend_from_slice(&min.to_le_bytes());
        }
        // level 1: 1 peak
        b.extend_from_slice(&400i16.to_le_bytes());
        b.extend_from_slice(&(-200i16).to_le_bytes());
        b
    }

    #[test]
    fn parses_header_and_levels() {
        let p = ReaPeaks::parse(&sample()).unwrap();
        assert_eq!(p.channels, 1);
        assert_eq!(p.samplerate, 48000);
        assert_eq!(p.source_stamp, 7);
        assert_eq!(p.levels.len(), 2);
        assert_eq!(p.levels[0].samples_per_peak, 160);
        assert_eq!(p.levels[0].count, 4);
        assert_eq!(p.levels[1].count, 1);
        assert_eq!(p.length_samples(), 640);
        let (max, min) = p.levels[0].pair(1, 0, 3);
        assert!((max - 400.0 / 32767.0).abs() < 1e-6);
        assert!((min + 200.0 / 32767.0).abs() < 1e-6);
    }

    #[test]
    fn level_pick_matches_reaper_mipmapping() {
        let p = ReaPeaks::parse(&sample()).unwrap();
        // Zoomed in (fewer samples per px than the finest level): finest.
        assert_eq!(p.level_for(10.0).samples_per_peak, 160);
        // Between levels: the coarsest that still resolves.
        assert_eq!(p.level_for(1000.0).samples_per_peak, 160);
        assert_eq!(p.level_for(2400.0).samples_per_peak, 2400);
        assert_eq!(p.level_for(1e9).samples_per_peak, 2400);
    }

    #[test]
    fn columns_aggregate_min_max() {
        let p = ReaPeaks::parse(&sample()).unwrap();
        // 640 samples over 2 columns → 2 peaks per column at level 0.
        let cols = p.columns(0, 0.0, 640.0 / 48000.0, 2);
        assert_eq!(cols.len(), 2);
        assert!((cols[0].0 - 200.0 / 32767.0).abs() < 1e-6); // max(100,200)
        assert!((cols[0].1 + 100.0 / 32767.0).abs() < 1e-6); // min(-50,-100)
        assert!((cols[1].0 - 400.0 / 32767.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        assert!(matches!(
            ReaPeaks::parse(b"NOPE0000000000000000"),
            Err(ReaPeaksError::BadMagic(_))
        ));
        let mut b = sample();
        b.truncate(b.len() - 2);
        assert!(matches!(ReaPeaks::parse(&b), Err(ReaPeaksError::Truncated)));
    }

    /// Real-corpus smoke test (skips when the corpus is absent).
    #[test]
    fn reads_real_reaper_peaks() {
        let dir = std::path::Path::new("/home/cody/Documents/REAPER Media/peaks");
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("no local reapeaks corpus — skipping");
            return;
        };
        let mut checked = 0;
        for e in entries.flatten().take(10) {
            let p = ReaPeaks::read(e.path()).expect("parses");
            assert!(p.channels >= 1);
            assert_eq!(p.samplerate, 48000);
            assert_eq!(p.levels.len(), 3);
            assert_eq!(p.levels[0].samples_per_peak, 160);
            assert!(p.length_seconds() > 0.0);
            let cols = p.columns(0, 0.0, p.length_seconds(), 200);
            assert_eq!(cols.len(), 200);
            assert!(cols.iter().all(|(max, min)| max >= min));
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// A forward-only build has to land on the same bytes as a
    /// random-access one, or a proxy-built cache and a WAV-built cache
    /// would draw different waveforms for the same audio.
    #[test]
    fn streaming_builder_matches_compute() {
        let frames = 50_000usize;
        let nch = 2usize;
        let sample = |f: usize, ch: usize| ((f % 997) as f32 / 997.0 - 0.5) * (1.0 + ch as f32);
        let direct = ReaPeaks::compute(nch, 48_000, frames, sample);

        let mut builder = PeaksBuilder::new(nch, 48_000);
        // Deliberately ragged blocks: not a multiple of 160. (Blocks that
        // split a frame are `a_block_may_end_mid_frame`.)
        let mut f = 0;
        while f < frames {
            let n = (frames - f).min(if f % 3 == 0 { 373 } else { 1024 });
            let mut block = Vec::with_capacity(n * nch);
            for i in f..f + n {
                for ch in 0..nch {
                    block.push(sample(i, ch));
                }
            }
            builder.push_interleaved(&block);
            f += n;
        }
        assert_eq!(builder.frames(), frames as u64);
        let streamed = builder.finish();
        assert_eq!(streamed.to_bytes(), direct.to_bytes());
    }

    #[test]
    fn a_block_may_end_mid_frame() {
        let frames = 10_000usize;
        let nch = 3usize;
        let sample = |f: usize, ch: usize| ((f % 211) as f32 / 211.0 - 0.5) * (1.0 + ch as f32);
        let direct = ReaPeaks::compute(nch, 48_000, frames, sample);

        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|f| (0..nch).map(move |ch| sample(f, ch)))
            .collect();
        let mut builder = PeaksBuilder::new(nch, 48_000);
        // 7 and 11 samples: neither divides by three channels.
        for (i, block) in interleaved.chunks(7).enumerate() {
            for part in block.chunks(if i % 2 == 0 { 7 } else { 11 }) {
                builder.push_interleaved(part);
            }
        }
        assert_eq!(builder.frames(), frames as u64);
        assert_eq!(builder.finish().to_bytes(), direct.to_bytes());
    }

    #[test]
    fn compute_and_roundtrip() {
        // 1 kHz of ramp: frame f, mono → f/48000.
        let frames = 48_000usize;
        let pk = ReaPeaks::compute(1, 48_000, frames, |f, _| f as f32 / frames as f32);
        assert_eq!(pk.levels.len(), 3);
        assert_eq!(pk.levels[0].samples_per_peak, 160);
        assert_eq!(pk.levels[1].samples_per_peak, 2400);
        assert_eq!(pk.levels[2].samples_per_peak, 48000);
        assert_eq!(pk.levels[0].count, frames.div_ceil(160));
        // The last fine peak's max ≈ 1.0; the coarse level agrees.
        let (fine_max, _) = pk.levels[0].pair(1, 0, pk.levels[0].count - 1);
        assert!((fine_max - 1.0).abs() < 0.01, "{fine_max}");
        let (coarse_max, coarse_min) = pk.levels[2].pair(1, 0, 0);
        assert!((coarse_max - 1.0).abs() < 0.01);
        assert!(coarse_min.abs() < 0.01);

        // Bytes round-trip through the parser.
        let parsed = ReaPeaks::parse(&pk.to_bytes()).unwrap();
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.samplerate, 48_000);
        assert_eq!(parsed.levels.len(), 3);
        assert_eq!(parsed.levels[0].data, pk.levels[0].data);
    }
}
