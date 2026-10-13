//! Oversampling — Lanczos-windowed sinc resampling with anti-alias filtering.
//!
//! Provides transparent 2x/4x/8x oversampling that wraps any processing
//! callback. Used by limiters, clippers, and saturation plugins to reduce
//! aliasing from nonlinear operations.
//!
//! Architecture (based on LSP's approach):
//! 1. Upsample: Lanczos polyphase scatter-add
//! 2. Process: user callback at oversampled rate
//! 3. Anti-alias: IIR lowpass at original Nyquist
//! 4. Downsample: decimation (take every Nth sample)

use std::f64::consts::PI;

/// Oversampling ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversampleRate {
    /// No oversampling (pass-through).
    X1,
    /// 2x oversampling.
    X2,
    /// 4x oversampling.
    X4,
    /// 8x oversampling.
    X8,
}

impl OversampleRate {
    /// Get the integer ratio.
    pub fn ratio(self) -> usize {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
        }
    }
}

/// Oversampling quality (controls Lanczos kernel size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversampleQuality {
    /// 2-lobe Lanczos (low latency, ~2 samples).
    Low,
    /// 4-lobe Lanczos (balanced, ~4 samples).
    Medium,
    /// 8-lobe Lanczos (high quality, ~8 samples).
    High,
}

impl OversampleQuality {
    /// Number of Lanczos lobes.
    fn lobes(self) -> usize {
        match self {
            Self::Low => 2,
            Self::Medium => 4,
            Self::High => 8,
        }
    }
}

/// Anti-alias filter: 4th-order Butterworth lowpass (cascaded biquads).
struct AntiAliasFilter {
    // Two cascaded 2nd-order sections
    s1: [f64; 4], // z1, z2 for left and right of section 1
    s2: [f64; 4], // z1, z2 for left and right of section 2
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    b0_2: f64,
    b1_2: f64,
    b2_2: f64,
    a1_2: f64,
    a2_2: f64,
}

impl AntiAliasFilter {
    fn new() -> Self {
        Self {
            s1: [0.0; 4],
            s2: [0.0; 4],
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            b0_2: 1.0,
            b1_2: 0.0,
            b2_2: 0.0,
            a1_2: 0.0,
            a2_2: 0.0,
        }
    }

    /// Design a 4th-order Butterworth lowpass at the given cutoff.
    ///
    /// Uses two cascaded biquads with Butterworth pole angles.
    fn design(&mut self, cutoff_hz: f64, sample_rate: f64) {
        // 4th-order Butterworth = two biquad sections
        // Pole angles: pi/8 and 3*pi/8
        let q1 = 1.0 / (2.0 * (PI / 8.0).cos()); // Q = 0.541
        let q2 = 1.0 / (2.0 * (3.0 * PI / 8.0).cos()); // Q = 1.307

        Self::biquad_lpf(
            cutoff_hz,
            q1,
            sample_rate,
            &mut self.b0,
            &mut self.b1,
            &mut self.b2,
            &mut self.a1,
            &mut self.a2,
        );
        Self::biquad_lpf(
            cutoff_hz,
            q2,
            sample_rate,
            &mut self.b0_2,
            &mut self.b1_2,
            &mut self.b2_2,
            &mut self.a1_2,
            &mut self.a2_2,
        );
    }

    #[allow(clippy::too_many_arguments)] // writes one biquad's 5 coeffs in place
    fn biquad_lpf(
        freq: f64,
        q: f64,
        sr: f64,
        b0: &mut f64,
        b1: &mut f64,
        b2: &mut f64,
        a1: &mut f64,
        a2: &mut f64,
    ) {
        let w0 = 2.0 * PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0_inv = 1.0 / (1.0 + alpha);
        *b0 = ((1.0 - cos_w0) / 2.0) * a0_inv;
        *b1 = (1.0 - cos_w0) * a0_inv;
        *b2 = *b0;
        *a1 = (-2.0 * cos_w0) * a0_inv;
        *a2 = (1.0 - alpha) * a0_inv;
    }

    /// Process one sample through both sections (TDF2).
    #[inline]
    fn tick(&mut self, input: f64, ch: usize) -> f64 {
        // Section 1
        let s1_base = ch * 2;
        let out1 = self.b0 * input + self.s1[s1_base];
        self.s1[s1_base] = self.b1 * input - self.a1 * out1 + self.s1[s1_base + 1];
        self.s1[s1_base + 1] = self.b2 * input - self.a2 * out1;

        // Section 2
        let s2_base = ch * 2;
        let out2 = self.b0_2 * out1 + self.s2[s2_base];
        self.s2[s2_base] = self.b1_2 * out1 - self.a1_2 * out2 + self.s2[s2_base + 1];
        self.s2[s2_base + 1] = self.b2_2 * out1 - self.a2_2 * out2;

        out2
    }

    fn reset(&mut self) {
        self.s1 = [0.0; 4];
        self.s2 = [0.0; 4];
    }
}

/// Stereo oversampler.
///
/// Wraps a processing step with transparent up/downsampling.
///
/// # Usage
/// ```ignore
/// let mut os = Oversampler::new(OversampleRate::X4, OversampleQuality::Medium);
/// os.update(48000.0);
///
/// // In process callback:
/// os.process_stereo(&mut left, &mut right, |l, r| {
///     // Your nonlinear processing at 4x rate
///     for i in 0..l.len() {
///         l[i] = l[i].tanh(); // example: soft clip
///         r[i] = r[i].tanh();
///     }
/// });
/// ```
pub struct Oversampler {
    rate: OversampleRate,
    quality: OversampleQuality,

    /// Pre-computed Lanczos kernel coefficients.
    kernel: Vec<f64>,
    /// Kernel half-length in output samples.
    kernel_half: usize,

    /// Internal upsampled buffers (left, right).
    up_left: Vec<f64>,
    up_right: Vec<f64>,

    /// History buffer for kernel overlap (left, right).
    hist_left: Vec<f64>,
    hist_right: Vec<f64>,

    /// Anti-alias filter (applied before decimation).
    aa_filter: AntiAliasFilter,

    sample_rate: f64,
}

impl Oversampler {
    pub fn new(rate: OversampleRate, quality: OversampleQuality) -> Self {
        let mut s = Self {
            rate,
            quality,
            kernel: Vec::new(),
            kernel_half: 0,
            up_left: Vec::new(),
            up_right: Vec::new(),
            hist_left: Vec::new(),
            hist_right: Vec::new(),
            aa_filter: AntiAliasFilter::new(),
            sample_rate: 48000.0,
        };
        s.build_kernel();
        s
    }

    /// Change the oversampling rate.
    pub fn set_rate(&mut self, rate: OversampleRate) {
        if rate != self.rate {
            self.rate = rate;
            self.build_kernel();
            self.update(self.sample_rate);
        }
    }

    /// Change the quality.
    pub fn set_quality(&mut self, quality: OversampleQuality) {
        if quality != self.quality {
            self.quality = quality;
            self.build_kernel();
            self.update(self.sample_rate);
        }
    }

    /// Get the current rate.
    pub fn rate(&self) -> OversampleRate {
        self.rate
    }

    /// Update for the given base sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let ratio = self.rate.ratio();
        if ratio > 1 {
            let os_rate = sample_rate * ratio as f64;
            // Anti-alias cutoff: just below original Nyquist
            let cutoff = (sample_rate * 0.45).min(20000.0);
            self.aa_filter.design(cutoff, os_rate);
        }
    }

    /// Latency in input samples introduced by the oversampler.
    pub fn latency(&self) -> usize {
        if self.rate == OversampleRate::X1 {
            return 0;
        }
        self.quality.lobes()
    }

    /// Process stereo audio with oversampling.
    ///
    /// The callback receives mutable slices at the oversampled rate.
    /// Input and output are at the original rate.
    pub fn process_stereo<F>(&mut self, left: &mut [f64], right: &mut [f64], mut callback: F)
    where
        F: FnMut(&mut [f64], &mut [f64]),
    {
        let ratio = self.rate.ratio();

        if ratio == 1 {
            callback(left, right);
            return;
        }

        let n = left.len();
        let os_len = n * ratio;

        // Ensure buffers are large enough
        self.ensure_buffers(os_len);

        // Clear upsampled buffers (including overlap zone)
        let total_len = os_len + self.kernel_half * 2;
        for i in 0..total_len.min(self.up_left.len()) {
            self.up_left[i] = 0.0;
            self.up_right[i] = 0.0;
        }

        // Copy history into the start of the buffer
        let hist_len = self.hist_left.len();
        for i in 0..hist_len {
            self.up_left[i] = self.hist_left[i];
            self.up_right[i] = self.hist_right[i];
        }

        // Upsample: scatter-add each input sample through the Lanczos kernel
        let kernel_len = self.kernel.len();
        for i in 0..n {
            let center = hist_len + i * ratio;
            let sl = left[i];
            let sr = right[i];
            for k in 0..kernel_len {
                let pos = center + k;
                if pos < total_len && pos < self.up_left.len() {
                    self.up_left[pos] += sl * self.kernel[k];
                    self.up_right[pos] += sr * self.kernel[k];
                }
            }
        }

        // Process at oversampled rate
        let os_start = hist_len;
        let os_end = os_start + os_len;
        callback(
            &mut self.up_left[os_start..os_end],
            &mut self.up_right[os_start..os_end],
        );

        // Anti-alias filter + downsample
        for i in 0..n {
            let os_idx = os_start + i * ratio;
            // Apply AA filter to all oversampled samples in this input period
            for j in 0..ratio {
                let idx = os_idx + j;
                self.up_left[idx] = self.aa_filter.tick(self.up_left[idx], 0);
                self.up_right[idx] = self.aa_filter.tick(self.up_right[idx], 1);
            }
            // Decimate: take the center sample, compensating for kernel delay
            left[i] = self.up_left[os_idx];
            right[i] = self.up_right[os_idx];
        }

        // Save overlap history for next block
        let new_hist_start = os_end;
        for i in 0..hist_len {
            let src = new_hist_start + i;
            if src < self.up_left.len() {
                self.hist_left[i] = self.up_left[src];
                self.hist_right[i] = self.up_right[src];
            } else {
                self.hist_left[i] = 0.0;
                self.hist_right[i] = 0.0;
            }
        }
    }

    /// Process mono audio with oversampling.
    pub fn process_mono<F>(&mut self, data: &mut [f64], mut callback: F)
    where
        F: FnMut(&mut [f64]),
    {
        let ratio = self.rate.ratio();

        if ratio == 1 {
            callback(data);
            return;
        }

        let n = data.len();
        let os_len = n * ratio;

        self.ensure_buffers(os_len);

        let total_len = os_len + self.kernel_half * 2;
        for i in 0..total_len.min(self.up_left.len()) {
            self.up_left[i] = 0.0;
        }

        let hist_len = self.hist_left.len();
        for i in 0..hist_len {
            self.up_left[i] = self.hist_left[i];
        }

        // Upsample
        let kernel_len = self.kernel.len();
        #[allow(clippy::needless_range_loop)] // i also drives the stride into up_left
        for i in 0..n {
            let center = hist_len + i * ratio;
            let s = data[i];
            for k in 0..kernel_len {
                let pos = center + k;
                if pos < total_len && pos < self.up_left.len() {
                    self.up_left[pos] += s * self.kernel[k];
                }
            }
        }

        // Process
        let os_start = hist_len;
        let os_end = os_start + os_len;
        callback(&mut self.up_left[os_start..os_end]);

        // AA filter + downsample
        #[allow(clippy::needless_range_loop)] // i also drives the stride into up_left
        for i in 0..n {
            let os_idx = os_start + i * ratio;
            for j in 0..ratio {
                let idx = os_idx + j;
                self.up_left[idx] = self.aa_filter.tick(self.up_left[idx], 0);
            }
            data[i] = self.up_left[os_idx];
        }

        // Save history
        let new_hist_start = os_end;
        for i in 0..hist_len {
            let src = new_hist_start + i;
            self.hist_left[i] = if src < self.up_left.len() {
                self.up_left[src]
            } else {
                0.0
            };
        }
    }

    pub fn reset(&mut self) {
        self.hist_left.fill(0.0);
        self.hist_right.fill(0.0);
        self.aa_filter.reset();
    }

    /// Build the Lanczos kernel for the current rate and quality.
    fn build_kernel(&mut self) {
        let ratio = self.rate.ratio();
        if ratio <= 1 {
            self.kernel.clear();
            self.kernel_half = 0;
            self.hist_left.clear();
            self.hist_right.clear();
            return;
        }

        let lobes = self.quality.lobes();
        let half = lobes * ratio; // half-kernel length in output samples
        let len = half * 2 + 1; // full kernel length

        self.kernel.resize(len, 0.0);
        self.kernel_half = half;

        // Compute Lanczos kernel
        for i in 0..len {
            let x = (i as f64 - half as f64) / ratio as f64;
            self.kernel[i] = lanczos(x, lobes);
        }

        // Normalize: the kernel should sum to `ratio` for unity gain
        // (since we're inserting ratio-1 zeros between each input sample)
        let sum: f64 = self.kernel.iter().sum();
        if sum.abs() > 1e-10 {
            let scale = ratio as f64 / sum;
            for k in &mut self.kernel {
                *k *= scale;
            }
        }

        // Allocate history buffers for kernel overlap
        self.hist_left = vec![0.0; half];
        self.hist_right = vec![0.0; half];
    }

    fn ensure_buffers(&mut self, os_len: usize) {
        let total = os_len + self.kernel_half * 2 + self.rate.ratio();
        if self.up_left.len() < total {
            self.up_left.resize(total, 0.0);
            self.up_right.resize(total, 0.0);
        }
    }
}

/// Lanczos windowed sinc function.
///
/// `lanczos(x, a) = sinc(x) * sinc(x/a)` for |x| < a, else 0.
#[inline]
fn lanczos(x: f64, a: usize) -> f64 {
    if x.abs() < 1e-10 {
        return 1.0;
    }
    let a = a as f64;
    if x.abs() >= a {
        return 0.0;
    }
    let px = PI * x;
    let pxa = px / a;
    (px.sin() / px) * (pxa.sin() / pxa)
}
