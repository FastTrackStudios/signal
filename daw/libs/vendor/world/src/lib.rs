//! Safe Rust surface over the vendored WORLD vocoder (M. Morise,
//! modified-BSD): Harvest f0 → CheapTrick spectral envelope → D4C
//! aperiodicity → Synthesis.
//!
//! The reason this exists (see features/fx/tune/spec/): synthesis
//! takes `(f0[], sp[][], ap[][])` — rewrite `f0` and the formants (sp)
//! stay untouched **by construction**, which is the formant-preserving
//! pitch-editing renderer the offline editor needs. Time-stretch =
//! frame-index remapping before synthesis.

use core::ffi::c_int;

unsafe extern "C" {
    fn fts_world_num_frames(x_length: c_int, fs: c_int, frame_period_ms: f64) -> c_int;
    fn fts_world_harvest(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        frame_period_ms: f64,
        f0_floor: f64,
        f0_ceil: f64,
        temporal_positions: *mut f64,
        f0: *mut f64,
    );
    fn fts_world_fft_size(fs: c_int) -> c_int;
    fn fts_world_cheaptrick(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        temporal_positions: *const f64,
        f0: *const f64,
        f0_length: c_int,
        sp: *mut f64,
    );
    fn fts_world_d4c(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        temporal_positions: *const f64,
        f0: *const f64,
        f0_length: c_int,
        fft_size: c_int,
        ap: *mut f64,
    );
    fn fts_world_synthesis(
        f0: *const f64,
        f0_length: c_int,
        sp: *const f64,
        ap: *const f64,
        fft_size: c_int,
        frame_period_ms: f64,
        fs: c_int,
        y_length: c_int,
        y: *mut f64,
    );
}

/// A full WORLD analysis of a mono clip — the resynthesis source.
/// `sp`/`ap` are flat `[frames][bins]` with `bins = fft_size/2 + 1`.
#[derive(Debug, Clone)]
pub struct WorldAnalysis {
    pub f0: Vec<f64>,
    pub temporal_positions: Vec<f64>,
    pub sp: Vec<f64>,
    pub ap: Vec<f64>,
    pub fft_size: usize,
    pub frame_period_ms: f64,
    pub sample_rate: u32,
}

impl WorldAnalysis {
    pub fn bins(&self) -> usize {
        self.fft_size / 2 + 1
    }

    pub fn frames(&self) -> usize {
        self.f0.len()
    }

    /// Analyze a mono clip. `frame_period_ms` ≈ 5.0 is the WORLD
    /// default and matches an editing-grade time resolution.
    pub fn analyze(x: &[f64], sample_rate: u32, frame_period_ms: f64) -> Self {
        let fs = sample_rate as c_int;
        let n = x.len() as c_int;
        let frames = unsafe { fts_world_num_frames(n, fs, frame_period_ms) }.max(0) as usize;
        let mut f0 = vec![0.0; frames];
        let mut tp = vec![0.0; frames];
        unsafe {
            fts_world_harvest(
                x.as_ptr(),
                n,
                fs,
                frame_period_ms,
                0.0,
                0.0,
                tp.as_mut_ptr(),
                f0.as_mut_ptr(),
            );
        }
        let fft_size = unsafe { fts_world_fft_size(fs) }.max(2) as usize;
        let bins = fft_size / 2 + 1;
        let mut sp = vec![0.0; frames * bins];
        let mut ap = vec![0.0; frames * bins];
        unsafe {
            fts_world_cheaptrick(
                x.as_ptr(),
                n,
                fs,
                tp.as_ptr(),
                f0.as_ptr(),
                frames as c_int,
                sp.as_mut_ptr(),
            );
            fts_world_d4c(
                x.as_ptr(),
                n,
                fs,
                tp.as_ptr(),
                f0.as_ptr(),
                frames as c_int,
                fft_size as c_int,
                ap.as_mut_ptr(),
            );
        }
        Self {
            f0,
            temporal_positions: tp,
            sp,
            ap,
            fft_size,
            frame_period_ms,
            sample_rate,
        }
    }

    /// Synthesize with an edited f0 contour (same frame count).
    /// Formants (sp) and breath (ap) are untouched — pitch moves,
    /// the voice's character stays.
    pub fn synthesize_with_f0(&self, f0: &[f64]) -> Vec<f64> {
        assert_eq!(
            f0.len(),
            self.frames(),
            "f0 contour must match analysis frames"
        );
        let y_len = ((self.frames() as f64 - 1.0) * self.frame_period_ms / 1000.0
            * self.sample_rate as f64) as usize
            + 1;
        let mut y = vec![0.0; y_len];
        unsafe {
            fts_world_synthesis(
                f0.as_ptr(),
                f0.len() as c_int,
                self.sp.as_ptr(),
                self.ap.as_ptr(),
                self.fft_size as c_int,
                self.frame_period_ms,
                self.sample_rate as c_int,
                y_len as c_int,
                y.as_mut_ptr(),
            );
        }
        y
    }

    /// Straight resynthesis (the null test).
    pub fn synthesize(&self) -> Vec<f64> {
        self.synthesize_with_f0(&self.f0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    /// A vowel-ish source: 150 Hz pulse train through two resonators
    /// (700 Hz and 1200 Hz "formants").
    fn vowel(n: usize, f0: f64) -> Vec<f64> {
        let mut out = vec![0.0; n];
        let period = SR as f64 / f0;
        let mut next_pulse = 0.0f64;
        for (i, o) in out.iter_mut().enumerate() {
            if i as f64 >= next_pulse {
                *o = 1.0;
                next_pulse += period;
            }
        }
        // Two simple resonators in series.
        for &(freq, r) in &[(700.0f64, 0.985f64), (1200.0, 0.985)] {
            let w = core::f64::consts::TAU * freq / SR as f64;
            let (a1, a2) = (2.0 * r * w.cos(), -r * r);
            let (mut y1, mut y2) = (0.0f64, 0.0f64);
            for o in out.iter_mut() {
                let y = *o + a1 * y1 + a2 * y2;
                y2 = y1;
                y1 = y;
                *o = y * 0.02;
            }
        }
        out
    }

    fn band_energy(x: &[f64], center: f64, width: f64) -> f64 {
        let mut e = 0.0;
        let mut steps = 0;
        let mut f = center - width / 2.0;
        while f <= center + width / 2.0 {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &s) in x.iter().enumerate() {
                let ph = core::f64::consts::TAU * f * i as f64 / SR as f64;
                re += s * ph.cos();
                im += s * ph.sin();
            }
            e += (re * re + im * im).sqrt();
            steps += 1;
            f += width / 8.0;
        }
        e / steps as f64
    }

    #[test]
    fn analyzes_and_resynthesizes() {
        let x = vowel(SR as usize, 150.0);
        let a = WorldAnalysis::analyze(&x, SR, 5.0);
        // Harvest should find ~150 Hz on voiced frames.
        let voiced: Vec<f64> = a.f0.iter().cloned().filter(|&f| f > 0.0).collect();
        assert!(!voiced.is_empty(), "some frames voiced");
        let med = {
            let mut v = voiced.clone();
            v.sort_by(|p, q| p.partial_cmp(q).unwrap());
            v[v.len() / 2]
        };
        assert!((med - 150.0).abs() < 5.0, "Harvest finds the f0: {med:.1}");
        let y = a.synthesize();
        let e = (y.iter().map(|s| s * s).sum::<f64>() / y.len() as f64).sqrt();
        assert!(e > 1.0e-4, "resynthesis produces signal: rms={e:e}");
    }

    #[test]
    fn pitch_shift_preserves_formants() {
        let x = vowel(2 * SR as usize, 150.0);
        let a = WorldAnalysis::analyze(&x, SR, 5.0);
        // Shift up a major third (×1.26): rewrite f0 only.
        let shifted: Vec<f64> = a.f0.iter().map(|&f| f * 1.26).collect();
        let y = a.synthesize_with_f0(&shifted);
        let late = &y[SR as usize / 2..];

        // New fundamental ≈ 189 Hz.
        let e_new_f0 = band_energy(late, 189.0, 12.0);
        let e_old_f0 = band_energy(late, 150.0, 12.0);
        assert!(
            e_new_f0 > e_old_f0 * 1.5,
            "pitch moved to 189 Hz: new={e_new_f0:.4} old={e_old_f0:.4}"
        );

        // Formant preservation, measured at HARMONICS of the new
        // fundamental (the spectrum is sparse — band energy between
        // harmonics is meaningless): the 6th harmonic (1134 Hz, near
        // the 1200 Hz formant) must beat the 8th (1512 Hz, past it).
        // Had the envelope moved with the pitch (resampling shifter),
        // the peak would sit at 1200·1.26 ≈ 1512 and the 8th would win.
        let e_h6 = band_energy(late, 189.0 * 6.0, 24.0);
        let e_h8 = band_energy(late, 189.0 * 8.0, 24.0);
        assert!(
            e_h6 > e_h8,
            "envelope peak stays near 1200 Hz: h6(1134)={e_h6:.4} h8(1512)={e_h8:.4}"
        );
    }
}
