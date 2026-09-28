//! A band-limited wavetable bank — the waveforms Omnisphere's wavetable
//! oscillator plays, read from their extracted files.
//!
//! A bank is a run of single-cycle frames (Omnisphere's: 4096 samples each, 64 frames a
//! file — not the 128 × 2048 its `library.styx` claims: a Juno sub square
//! only reads as a square at 4096).
//! Every frame is kept at [`LEVELS`] octave-spaced bandwidths, each with the
//! harmonics above its limit removed (by FFT), so a note reads the copy whose
//! top harmonic still sits under Nyquist and never aliases — the standard
//! mip-mapped wavetable. Playback crossfades between neighbouring frames by
//! position and interpolates linearly within a frame.
//!
//! Loading (file read + FFTs) happens off the audio thread, once per file:
//! banks are shared process-wide through [`WaveBank::load`]'s cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Octave-spaced bandwidth copies per frame: level `k` keeps harmonics up to
/// `cycle / 2 >> k`.
pub const LEVELS: usize = 11;

#[derive(Debug)]
pub struct WaveBank {
    cycle: usize,
    /// `levels[k][frame]` — one cycle, `cycle` samples.
    levels: Vec<Vec<Vec<f32>>>,
}

type Cache = Mutex<HashMap<(PathBuf, usize, usize), Arc<WaveBank>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Cache::default)
}

impl WaveBank {
    /// Build from single-cycle frames (all the same length, a power of two).
    ///
    /// # Panics
    ///
    /// When `frames` is empty or the frames differ in length.
    #[must_use]
    pub fn from_frames(frames: &[Vec<f32>]) -> Self {
        assert!(!frames.is_empty(), "a wavetable needs a frame");
        let cycle = frames[0].len();
        assert!(
            frames.iter().all(|f| f.len() == cycle),
            "frames differ in length"
        );
        let mut planner = realfft::RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(cycle);
        let inv = planner.plan_fft_inverse(cycle);
        let mut levels = vec![Vec::with_capacity(frames.len()); LEVELS];
        let mut spectrum = fwd.make_output_vec();
        let mut scratch = fwd.make_input_vec();
        for frame in frames {
            scratch.copy_from_slice(frame);
            fwd.process(&mut scratch, &mut spectrum).expect("fft");
            // DC out: an offset is not part of a waveform's sound.
            spectrum[0] = realfft::num_complex::Complex::new(0.0, 0.0);
            for (k, level) in levels.iter_mut().enumerate() {
                let keep = (cycle / 2) >> k;
                let mut band = spectrum.clone();
                for (h, bin) in band.iter_mut().enumerate() {
                    if h > keep {
                        *bin = realfft::num_complex::Complex::new(0.0, 0.0);
                    }
                }
                // The Nyquist bin must be real for the inverse.
                if let Some(last) = band.last_mut() {
                    last.im = 0.0;
                }
                let mut out = inv.make_output_vec();
                inv.process(&mut band, &mut out).expect("ifft");
                let norm = 1.0 / cycle as f32;
                level.push(out.into_iter().map(|v| v * norm).collect());
            }
        }
        Self { cycle, levels }
    }

    /// Load `path` — a 32-bit float WAV or a raw headerless `.stmwf` — as
    /// frames of `cycle` samples, keeping every `stride`-th frame. Cached by
    /// `(path, stride)`.
    ///
    /// # Errors
    ///
    /// When the file cannot be read or holds no whole frame.
    pub fn load(path: &Path, cycle: usize, stride: usize) -> Result<Arc<Self>, String> {
        let key = (path.to_path_buf(), cycle, stride.max(1));
        if let Some(b) = cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return Ok(b.clone());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("read {path:?}: {e}"))?;
        let samples =
            float_samples(&bytes).ok_or_else(|| format!("{path:?}: not 32-bit float audio"))?;
        let frames: Vec<Vec<f32>> = samples
            .chunks_exact(cycle)
            .step_by(stride.max(1))
            .map(<[f32]>::to_vec)
            .collect();
        if frames.is_empty() {
            return Err(format!("{path:?}: no whole {cycle}-sample frame"));
        }
        let bank = Arc::new(Self::from_frames(&frames));
        cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, bank.clone());
        Ok(bank)
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.levels[0].len()
    }

    /// The level for a note stepping `inc` cycles per sample: the widest
    /// band whose top harmonic stays under Nyquist.
    #[inline]
    #[must_use]
    pub fn level_for(&self, inc: f32) -> usize {
        let top = (self.cycle / 2) as f32;
        let allowed = (0.5 / inc.abs().max(1e-9)).max(1.0);
        if allowed >= top {
            return 0;
        }
        ((top / allowed).log2().ceil() as usize).min(LEVELS - 1)
    }

    /// The waveform at `phase` (0..1) and table `position` (0..1 across the
    /// frames), band-limited for a note stepping `inc` cycles per sample.
    #[inline]
    #[must_use]
    pub fn sample(&self, position: f32, phase: f32, inc: f32) -> f32 {
        let level = &self.levels[self.level_for(inc)];
        let n = level.len();
        let x = position.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (x as usize).min(n - 1);
        let frac = x - i as f32;
        let read = |frame: &[f32]| {
            let p = phase.rem_euclid(1.0) * self.cycle as f32;
            let j = p as usize % self.cycle;
            let t = p - p.floor();
            let a = frame[j];
            let b = frame[(j + 1) % self.cycle];
            a + (b - a) * t
        };
        let a = read(&level[i]);
        if frac > 0.0 && i + 1 < n {
            a + (read(&level[i + 1]) - a) * frac
        } else {
            a
        }
    }
}

/// The samples of a 32-bit float WAV (first channel), or of a raw
/// headerless float file.
fn float_samples(bytes: &[u8]) -> Option<Vec<f32>> {
    let floats = |b: &[u8]| -> Vec<f32> {
        b.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Some(floats(bytes));
    }
    let (mut i, mut channels, mut format, mut bits) = (12usize, 1u16, 0u16, 0u16);
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = bytes.get(i + 8..(i + 8 + size).min(bytes.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            format = u16::from_le_bytes([body[0], body[1]]);
            channels = u16::from_le_bytes([body[2], body[3]]).max(1);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            // 3 = IEEE float; 0xFFFE = extensible (float by bit depth here).
            if !(format == 3 || (format == 0xFFFE && bits == 32)) || bits != 32 {
                return None;
            }
            let all = floats(body);
            return Some(all.into_iter().step_by(channels as usize).collect());
        }
        i += 8 + size + (size & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saw(cycle: usize) -> Vec<f32> {
        (0..cycle)
            .map(|i| 2.0 * i as f32 / cycle as f32 - 1.0)
            .collect()
    }

    #[test]
    fn high_notes_read_a_band_limited_copy_and_low_notes_the_full_one() {
        let bank = WaveBank::from_frames(&[saw(2048)]);
        // 20 Hz at 48 kHz: all 1024 harmonics fit (1200 would).
        assert_eq!(bank.level_for(20.0 / 48_000.0), 0);
        // 40 Hz: only 600 fit, so the 512-harmonic copy.
        assert_eq!(bank.level_for(40.0 / 48_000.0), 1);
        // 4 kHz: only 6 harmonics fit under 24 kHz.
        let lvl = bank.level_for(4_000.0 / 48_000.0);
        assert!((1024 >> lvl) <= 6 && (1024 >> (lvl - 1)) > 6);
        // The band-limited copy has no energy above its harmonic limit:
        // a 2-harmonic copy of a saw is smooth (small sample-to-sample steps).
        let top = &bank.levels[9][0];
        let max_step = top
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_step < 0.01, "{max_step}");
    }

    /// Machine-local: the real Jupiter 8 Saw reads back as a saw.
    #[test]
    fn a_real_omnisphere_saw_reads_back_as_a_saw() {
        let p = std::path::Path::new(
            "/Volumes/dev-drive/AudioHaven/Sampled/Synth/Omnisphere-Wavetables/1 - Classic Waveforms/Sawtooth/JP-8 Saw/Jupiter 8 Saw.wav",
        );
        if !p.exists() {
            return;
        }
        let bank = WaveBank::load(p, 4096, 1).unwrap();
        assert_eq!(bank.frames(), 64);
        let inc = 130.8 / 48_000.0;
        let n = 2048;
        let cycle: Vec<f32> = (0..n).map(|i| bank.sample(0.0, i as f32 / n as f32, inc)).collect();
        let h = |k: usize| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, v) in cycle.iter().enumerate() {
                let a = std::f64::consts::TAU * (k * i) as f64 / n as f64;
                re += f64::from(*v) * a.cos();
                im += f64::from(*v) * a.sin();
            }
            re.hypot(im)
        };
        let db = |k: usize| 20.0 * (h(k) / h(1)).log10();
        eprintln!("frames {} h2 {:.1} h3 {:.1} h4 {:.1}", bank.frames(), db(2), db(3), db(4));
        assert!((db(2) + 6.1).abs() < 1.0, "h2 {}", db(2));
        assert!((db(3) + 9.6).abs() < 1.0, "h3 {}", db(3));
    }

    /// Machine-local: the Juno 60 Sub is a square (odd harmonics only) — which
    /// it is only when read as 4096-sample cycles.
    #[test]
    fn a_real_omnisphere_sub_square_reads_back_as_a_square() {
        let p = std::path::Path::new(
            "/Volumes/dev-drive/AudioHaven/Sampled/Synth/Omnisphere-Wavetables/1 - Classic Waveforms/Sub Osc/Juno 60 Sub/Juno 60 Sub.wav",
        );
        if !p.exists() {
            return;
        }
        let bank = WaveBank::load(p, 4096, 1).unwrap();
        let n = 4096;
        let cycle: Vec<f32> = (0..n)
            .map(|i| bank.sample(0.0, i as f32 / n as f32, 100.0 / 48_000.0))
            .collect();
        let h = |k: usize| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, v) in cycle.iter().enumerate() {
                let a = std::f64::consts::TAU * (k * i) as f64 / n as f64;
                re += f64::from(*v) * a.cos();
                im += f64::from(*v) * a.sin();
            }
            re.hypot(im)
        };
        let db = |k: usize| 20.0 * (h(k) / h(1)).log10();
        assert!(db(2) < -40.0, "h2 {}", db(2));
        assert!((db(3) + 9.5).abs() < 1.0, "h3 {}", db(3));
    }

    #[test]
    fn position_crossfades_frames() {
        let a = vec![0.5f32; 64]
            .into_iter()
            .enumerate()
            .map(|(i, _)| (i as f32 / 64.0 * std::f32::consts::TAU).sin())
            .collect::<Vec<_>>();
        let b: Vec<f32> = a.iter().map(|v| -v).collect();
        let bank = WaveBank::from_frames(&[a.clone(), b]);
        let q = 0.25; // a quarter cycle: sin = 1
        assert!((bank.sample(0.0, q, 1e-4) - 1.0).abs() < 1e-3);
        assert!((bank.sample(1.0, q, 1e-4) + 1.0).abs() < 1e-3);
        assert!(bank.sample(0.5, q, 1e-4).abs() < 1e-3);
    }
}
