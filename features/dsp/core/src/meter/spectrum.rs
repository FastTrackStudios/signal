//! FFT spectrum analyzer.
//!
//! Uses `realfft` for efficient real-to-complex FFT with a Hann window.
//! Output bins are smoothed with an exponential moving average (~30 ms time constant).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use atomic_float::AtomicF32;
use parking_lot::RwLock;
use realfft::RealFftPlanner;

// ── Shared state ──────────────────────────────────────────────────────────────

/// Spectrum analyzer state shared with the UI painter.
pub struct SpectrumState {
    /// Output magnitudes in dB, one value per FFT bin (DC to Nyquist).
    /// Length = `fft_size / 2 + 1`.
    pub bins_db: RwLock<Vec<f32>>,
    /// All-time maximum per bin (in dB). Same length as `bins_db`.
    /// Updated every FFT unless `hold` is true.
    pub max_bins_db: RwLock<Vec<f32>>,
    /// When true, both `bins_db` and `max_bins_db` are frozen — no updates.
    pub hold: AtomicBool,
    /// Sample rate, used by the painter to map frequency → bin index.
    /// Stored as atomic so `initialize()` can update it without replacing the Arc.
    pub sample_rate: AtomicF32,
    /// FFT size (must be even). Stored as atomic so rebuild_fft can update it.
    pub fft_size: AtomicUsize,
    /// Running RMS power (EMA ~300ms), in dBFS.
    pub rms_db: AtomicF32,
    /// All-time sample peak level since last reset, in dBFS.
    pub peak_db: AtomicF32,
    /// Whether the signal has clipped (peak > 0 dBFS).
    pub clipped: AtomicBool,
    /// Desired FFT size (set by UI, checked by processor). 0 means "use current".
    pub desired_fft_size: AtomicUsize,
}

impl SpectrumState {
    pub fn new(sample_rate: f32, fft_size: usize) -> Arc<Self> {
        let num_bins = fft_size / 2 + 1;
        Arc::new(Self {
            bins_db: RwLock::new(vec![f32::NEG_INFINITY; num_bins]),
            max_bins_db: RwLock::new(vec![f32::NEG_INFINITY; num_bins]),
            hold: AtomicBool::new(false),
            sample_rate: AtomicF32::new(sample_rate),
            fft_size: AtomicUsize::new(fft_size),
            rms_db: AtomicF32::new(f32::NEG_INFINITY),
            peak_db: AtomicF32::new(f32::NEG_INFINITY),
            clipped: AtomicBool::new(false),
            desired_fft_size: AtomicUsize::new(0),
        })
    }
}

// ── Spectrum analyzer ─────────────────────────────────────────────────────────

/// FFT spectrum analyzer.
///
/// Call [`SpectrumAnalyzer::process`] per audio block from the audio thread.
/// Read the output via the shared [`SpectrumState`] arc.
pub struct SpectrumAnalyzer {
    fft_size: usize,
    sample_rate: f32,

    /// Ring buffer for incoming samples.
    ring: Vec<f32>,
    ring_pos: usize,
    /// Number of new samples since the last FFT (for ~50% overlap).
    samples_since_fft: usize,
    hop_size: usize,

    /// Hann window coefficients, length = fft_size.
    window: Vec<f32>,

    /// Smoothed magnitude bins (linear power, not dB).
    smoothed: Vec<f32>,
    /// Smoothing coefficient per-block; recalculated from block size.
    smooth_alpha: f32,

    /// realfft planner and scratch buffer.
    planner: RealFftPlanner<f32>,
    scratch: Vec<f32>,
    output: Vec<realfft::num_complex::Complex<f32>>,

    /// EMA coefficient for RMS (~300ms time constant).
    rms_alpha: f32,
    /// Running mean power (linear).
    running_rms: f32,

    pub state: Arc<SpectrumState>,
}

impl SpectrumAnalyzer {
    /// Create a spectrum analyzer with the given FFT size (must be even).
    ///
    /// A larger `fft_size` gives better frequency resolution but more latency.
    /// The default of 2048 is a good starting point.
    pub fn new(sample_rate: f32, fft_size: usize) -> Self {
        assert!(
            fft_size >= 4 && fft_size.is_multiple_of(2),
            "fft_size must be even and >= 4"
        );

        let window = hann_window(fft_size);
        let num_bins = fft_size / 2 + 1;
        let hop_size = fft_size / 2; // 50% overlap

        // Smooth time constant ~30 ms: alpha = exp(-hop / (fs * 0.030))
        let smooth_alpha = (-(hop_size as f32) / (sample_rate * 0.030)).exp();

        // RMS time constant ~300 ms
        let rms_alpha = (-1.0_f32 / (sample_rate * 0.300)).exp();

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let scratch = r2c.make_input_vec();
        let output = r2c.make_output_vec();

        Self {
            fft_size,
            sample_rate,
            ring: vec![0.0; fft_size],
            ring_pos: 0,
            samples_since_fft: 0,
            hop_size,
            window,
            smoothed: vec![0.0; num_bins],
            smooth_alpha,
            planner,
            scratch,
            output,
            rms_alpha,
            running_rms: 0.0,
            state: SpectrumState::new(sample_rate, fft_size),
        }
    }

    /// Create a spectrum analyzer with the default FFT size of 2048.
    pub fn default_size(sample_rate: f32) -> Self {
        Self::new(sample_rate, 2048)
    }

    /// Process a mono block of audio.
    ///
    /// Internally accumulates samples and computes the FFT when a full hop has
    /// been collected. The output state is updated after each FFT.
    pub fn process(&mut self, samples: &[f32]) {
        // Check if a new FFT size has been requested by the UI.
        let desired = self.state.desired_fft_size.load(Ordering::Relaxed);
        if desired != 0 && desired != self.fft_size {
            self.rebuild_fft(desired);
            self.state.desired_fft_size.store(0, Ordering::Relaxed);
        }

        for &s in samples {
            // Update running RMS (power domain EMA)
            self.running_rms = self.rms_alpha * self.running_rms + (1.0 - self.rms_alpha) * s * s;

            // Update peak
            let abs_s = s.abs();
            if abs_s > 0.0 {
                let peak_db_new = 20.0 * abs_s.log10();
                let current_peak = self.state.peak_db.load(Ordering::Relaxed);
                if peak_db_new > current_peak {
                    self.state.peak_db.store(peak_db_new, Ordering::Relaxed);
                }
                if abs_s >= 1.0 {
                    self.state.clipped.store(true, Ordering::Relaxed);
                }
            }

            // Write into ring buffer
            self.ring[self.ring_pos] = s;
            self.ring_pos = (self.ring_pos + 1) % self.fft_size;
            self.samples_since_fft += 1;

            if self.samples_since_fft >= self.hop_size {
                self.samples_since_fft = 0;
                self.run_fft();
            }
        }

        // Update RMS state
        let rms_db = if self.running_rms > 0.0 {
            10.0 * self.running_rms.log10()
        } else {
            f32::NEG_INFINITY
        };
        self.state.rms_db.store(rms_db, Ordering::Relaxed);
    }

    /// Reset peak and clip stats.
    pub fn reset_stats(&self) {
        self.state
            .peak_db
            .store(f32::NEG_INFINITY, Ordering::Relaxed);
        self.state.clipped.store(false, Ordering::Relaxed);
        // rms will decay naturally
    }

    /// Reset the all-time maximum spectrum to NEG_INFINITY, and stats.
    pub fn reset_max(&mut self) {
        let num_bins = self.fft_size / 2 + 1;
        let mut max_guard = self.state.max_bins_db.write();
        max_guard
            .iter_mut()
            .take(num_bins)
            .for_each(|v| *v = f32::NEG_INFINITY);
        drop(max_guard);
        self.reset_stats();
    }

    /// Rebuild the FFT engine for a new size.
    fn rebuild_fft(&mut self, new_size: usize) {
        if new_size < 4 || !new_size.is_multiple_of(2) {
            return;
        }
        let num_bins = new_size / 2 + 1;
        let hop_size = new_size / 2;
        let smooth_alpha = (-(hop_size as f32) / (self.sample_rate * 0.030)).exp();
        let window = hann_window(new_size);
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(new_size);
        let scratch = r2c.make_input_vec();
        let output = r2c.make_output_vec();

        self.fft_size = new_size;
        self.ring = vec![0.0; new_size];
        self.ring_pos = 0;
        self.samples_since_fft = 0;
        self.hop_size = hop_size;
        self.window = window;
        self.smoothed = vec![0.0; num_bins];
        self.smooth_alpha = smooth_alpha;
        self.planner = planner;
        self.scratch = scratch;
        self.output = output;

        // Resize state bins
        {
            let mut bins = self.state.bins_db.write();
            *bins = vec![f32::NEG_INFINITY; num_bins];
        }
        {
            let mut max_bins = self.state.max_bins_db.write();
            *max_bins = vec![f32::NEG_INFINITY; num_bins];
        }

        self.state.fft_size.store(new_size, Ordering::Relaxed);
    }

    fn run_fft(&mut self) {
        // Skip all updates when hold is active.
        if self.state.hold.load(Ordering::Relaxed) {
            return;
        }

        // Copy the ring buffer (oldest first) into the scratch buffer and apply window.
        for i in 0..self.fft_size {
            let ring_idx = (self.ring_pos + i) % self.fft_size;
            self.scratch[i] = self.ring[ring_idx] * self.window[i];
        }

        let r2c = self.planner.plan_fft_forward(self.fft_size);
        if r2c.process(&mut self.scratch, &mut self.output).is_err() {
            return;
        }

        let scale = 2.0 / self.fft_size as f32;
        let alpha = self.smooth_alpha;
        let one_minus = 1.0 - alpha;

        for (bin, cx) in self.output.iter().enumerate() {
            let power = (cx.re * cx.re + cx.im * cx.im) * scale * scale;
            self.smoothed[bin] = alpha * self.smoothed[bin] + one_minus * power;
        }

        // Convert smoothed power to dB and publish
        let num_bins = self.fft_size / 2 + 1;
        let mut guard = self.state.bins_db.write();
        guard.resize(num_bins, f32::NEG_INFINITY);
        for (i, &power) in self.smoothed.iter().enumerate() {
            guard[i] = if power > 0.0 {
                10.0 * power.log10()
            } else {
                f32::NEG_INFINITY
            };
        }

        // Update all-time maximum per bin.
        let mut max_guard = self.state.max_bins_db.write();
        max_guard.resize(num_bins, f32::NEG_INFINITY);
        for i in 0..num_bins {
            if guard[i] > max_guard[i] {
                max_guard[i] = guard[i];
            }
        }
    }
}

// ── Hann window ───────────────────────────────────────────────────────────────

fn hann_window(size: usize) -> Vec<f32> {
    use std::f64::consts::PI;
    (0..size)
        .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / size as f64).cos())) as f32)
        .collect()
}
