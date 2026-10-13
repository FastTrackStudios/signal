//! Loudness metering and automatic gain control.
//!
//! # Metering
//!
//! When the `loudness` feature is enabled (default), [`LoudnessMeter`] uses
//! the `ebur128` crate for EBU R128 / ITU-R BS.1770-4 compliant measurement
//! including true-peak detection and gated integrated loudness.
//!
//! Without the feature, a lightweight fallback implementation is used that
//! provides approximate momentary/short-term readings via hand-rolled
//! K-weighting filters and ring-buffer mean-square computation.
//!
//! # AutoGain
//!
//! [`AutoGain`] provides dual-timeline automatic gain control (based on LSP's
//! approach) using the loudness meter for measurement. Long-time gain tracks
//! slowly to match target loudness; short-time gain reacts fast to
//! transients/surges.
//!
//! # K-Weighting Filter
//!
//! [`KWeightingFilter`] is always available as a standalone BS.1770-4
//! K-weighting filter for use in sidechain processing or custom metering.

use std::f64::consts::PI;

// ── K-weighting filter (always available) ───────────────────────────────

/// K-weighting filter (BS.1770-4): two cascaded biquads.
///
/// Stage 1: Pre-filter (high shelf ~1682 Hz, +4 dB)
/// Stage 2: RLB high-pass (~38 Hz)
///
/// Coefficients are recomputed from analog prototypes for any sample rate.
/// This is a lightweight standalone filter — for full EBU R128 compliance,
/// use [`LoudnessMeter`] instead.
pub struct KWeightingFilter {
    // Stage 1: pre-filter
    b0_1: f64,
    b1_1: f64,
    b2_1: f64,
    a1_1: f64,
    a2_1: f64,
    z1_1: f64,
    z2_1: f64,

    // Stage 2: RLB high-pass
    b0_2: f64,
    b1_2: f64,
    b2_2: f64,
    a1_2: f64,
    a2_2: f64,
    z1_2: f64,
    z2_2: f64,
}

impl KWeightingFilter {
    pub fn new() -> Self {
        Self {
            b0_1: 1.0,
            b1_1: 0.0,
            b2_1: 0.0,
            a1_1: 0.0,
            a2_1: 0.0,
            z1_1: 0.0,
            z2_1: 0.0,
            b0_2: 1.0,
            b1_2: 0.0,
            b2_2: 0.0,
            a1_2: 0.0,
            a2_2: 0.0,
            z1_2: 0.0,
            z2_2: 0.0,
        }
    }

    /// Compute coefficients from analog prototypes for the given sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        // Stage 1: Pre-filter (high shelf)
        {
            let f0 = 1681.97;
            let q = 0.7071752;
            let vh = 1.584_864_701_13;
            let vb = 1.258_720_930_23;

            let k = (PI * f0 / sample_rate).tan();
            let k2 = k * k;
            let denom = 1.0 + k / q + k2;

            self.b0_1 = (vh + vb * k / q + k2) / denom;
            self.b1_1 = 2.0 * (k2 - vh) / denom;
            self.b2_1 = (vh - vb * k / q + k2) / denom;
            self.a1_1 = 2.0 * (k2 - 1.0) / denom;
            self.a2_1 = (1.0 - k / q + k2) / denom;
        }

        // Stage 2: RLB high-pass
        {
            let f0 = 38.1355;
            let q = 0.5003270;

            let k = (PI * f0 / sample_rate).tan();
            let k2 = k * k;
            let denom = 1.0 + k / q + k2;

            self.b0_2 = 1.0 / denom;
            self.b1_2 = -2.0 / denom;
            self.b2_2 = 1.0 / denom;
            self.a1_2 = 2.0 * (k2 - 1.0) / denom;
            self.a2_2 = (1.0 - k / q + k2) / denom;
        }
    }

    /// Process one sample through both K-weighting stages (TDF2).
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let out1 = self.b0_1 * input + self.z1_1;
        self.z1_1 = self.b1_1 * input - self.a1_1 * out1 + self.z2_1;
        self.z2_1 = self.b2_1 * input - self.a2_1 * out1;

        let out2 = self.b0_2 * out1 + self.z1_2;
        self.z1_2 = self.b1_2 * out1 - self.a1_2 * out2 + self.z2_2;
        self.z2_2 = self.b2_2 * out1 - self.a2_2 * out2;

        out2
    }

    pub fn reset(&mut self) {
        self.z1_1 = 0.0;
        self.z2_1 = 0.0;
        self.z1_2 = 0.0;
        self.z2_2 = 0.0;
    }
}

impl Default for KWeightingFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Loudness meter (ebur128 backend) ────────────────────────────────────

#[cfg(feature = "loudness")]
mod meter_ebur128 {
    use ebur128::{EbuR128, Mode};

    /// Stereo loudness meter backed by the `ebur128` crate.
    ///
    /// Provides EBU R128 compliant momentary, short-term, integrated loudness,
    /// true peak, and loudness range measurements.
    pub struct LoudnessMeter {
        inner: Option<EbuR128>,
        sample_rate: u32,

        // Cached readings (updated after each process call)
        momentary: f64,
        short_term: f64,
        integrated: f64,
        true_peak_l: f64,
        true_peak_r: f64,
    }

    impl LoudnessMeter {
        pub fn new() -> Self {
            Self {
                inner: None,
                sample_rate: 48000,
                momentary: -200.0,
                short_term: -200.0,
                integrated: -200.0,
                true_peak_l: 0.0,
                true_peak_r: 0.0,
            }
        }

        /// Initialize or reconfigure for the given sample rate.
        pub fn update(&mut self, sample_rate: f64) {
            let sr = sample_rate as u32;
            if self.inner.is_some() && self.sample_rate == sr {
                return;
            }
            self.sample_rate = sr;
            let mode = Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK;
            self.inner = EbuR128::new(2, sr, mode).ok();
        }

        /// Process a buffer of stereo audio (planar f64).
        pub fn process(&mut self, left: &[f64], right: &[f64]) {
            let inner = match self.inner.as_mut() {
                Some(i) => i,
                None => return,
            };

            let _ = inner.add_frames_planar_f64(&[left, right]);

            // Update cached readings
            self.momentary = inner.loudness_momentary().unwrap_or(-200.0);
            self.short_term = inner.loudness_shortterm().unwrap_or(-200.0);
            self.integrated = inner.loudness_global().unwrap_or(-200.0);
            self.true_peak_l = inner.true_peak(0).unwrap_or(0.0);
            self.true_peak_r = inner.true_peak(1).unwrap_or(0.0);
        }

        /// Process a single stereo sample pair.
        ///
        /// Note: For performance, prefer calling [`Self::process`] with full buffers.
        /// This method feeds one frame at a time.
        pub fn process_sample(&mut self, left: f64, right: f64) {
            let inner = match self.inner.as_mut() {
                Some(i) => i,
                None => return,
            };

            let _ = inner.add_frames_f64(&[left, right]);

            // Only update momentary on each sample (other readings are
            // expensive to query per-sample; update on process() calls)
            self.momentary = inner.loudness_momentary().unwrap_or(-200.0);
        }

        /// Flush cached short-term and integrated readings.
        ///
        /// Call after a batch of `process_sample` calls to update all readings.
        pub fn flush_readings(&mut self) {
            let inner = match self.inner.as_ref() {
                Some(i) => i,
                None => return,
            };
            self.short_term = inner.loudness_shortterm().unwrap_or(-200.0);
            self.integrated = inner.loudness_global().unwrap_or(-200.0);
            self.true_peak_l = inner.true_peak(0).unwrap_or(0.0);
            self.true_peak_r = inner.true_peak(1).unwrap_or(0.0);
        }

        /// Get momentary loudness in LUFS (400ms window).
        pub fn momentary(&self) -> f64 {
            self.momentary
        }

        /// Get short-term loudness in LUFS (3s window).
        pub fn short_term(&self) -> f64 {
            self.short_term
        }

        /// Get integrated loudness in LUFS (gated per EBU R128).
        pub fn integrated(&self) -> f64 {
            self.integrated
        }

        /// Get true peak for the left channel (linear, cumulative).
        pub fn true_peak_left(&self) -> f64 {
            self.true_peak_l
        }

        /// Get true peak for the right channel (linear, cumulative).
        pub fn true_peak_right(&self) -> f64 {
            self.true_peak_r
        }

        /// Get true peak in dBTP (max of both channels).
        pub fn true_peak_dbtp(&self) -> f64 {
            let peak = self.true_peak_l.max(self.true_peak_r);
            if peak <= 0.0 {
                -200.0
            } else {
                20.0 * peak.log10()
            }
        }

        /// Get loudness range in LU (requires sufficient data).
        pub fn loudness_range(&self) -> f64 {
            self.inner
                .as_ref()
                .and_then(|i| i.loudness_range().ok())
                .unwrap_or(0.0)
        }

        /// Get the relative gating threshold in LUFS.
        pub fn relative_threshold(&self) -> f64 {
            self.inner
                .as_ref()
                .and_then(|i| i.relative_threshold().ok())
                .unwrap_or(-70.0)
        }

        pub fn reset(&mut self) {
            if let Some(inner) = self.inner.as_mut() {
                inner.reset();
            }
            self.momentary = -200.0;
            self.short_term = -200.0;
            self.integrated = -200.0;
            self.true_peak_l = 0.0;
            self.true_peak_r = 0.0;
        }
    }

    impl Default for LoudnessMeter {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Loudness meter (fallback without ebur128) ───────────────────────────

#[cfg(not(feature = "loudness"))]
mod meter_fallback {
    use super::{KWeightingFilter, ms_to_lufs};

    /// Lightweight stereo loudness meter (no external dependencies).
    ///
    /// Provides approximate momentary and short-term loudness via
    /// K-weighting filters and ring-buffer mean-square computation.
    /// For EBU R128 compliance, enable the `loudness` feature.
    pub struct LoudnessMeter {
        filter_l: KWeightingFilter,
        filter_r: KWeightingFilter,
        ring: Vec<f64>,
        ring_pos: usize,
        ring_sum: f64,
        ring_len: usize,
        short_ring: Vec<f64>,
        short_ring_pos: usize,
        short_ring_sum: f64,
        short_ring_len: usize,
        pub(crate) momentary_val: f64,
        pub(crate) short_term_val: f64,
        pub(crate) integrated_val: f64,
        integrated_sum: f64,
        integrated_count: u64,
        ungated_sum: f64,
        ungated_count: u64,
        block_sum: f64,
        block_count: usize,
        block_size: usize,
    }

    impl LoudnessMeter {
        pub fn new() -> Self {
            Self {
                filter_l: KWeightingFilter::new(),
                filter_r: KWeightingFilter::new(),
                ring: Vec::new(),
                ring_pos: 0,
                ring_sum: 0.0,
                ring_len: 0,
                short_ring: Vec::new(),
                short_ring_pos: 0,
                short_ring_sum: 0.0,
                short_ring_len: 0,
                momentary_val: -200.0,
                short_term_val: -200.0,
                integrated_val: -200.0,
                integrated_sum: 0.0,
                integrated_count: 0,
                ungated_sum: 0.0,
                ungated_count: 0,
                block_sum: 0.0,
                block_count: 0,
                block_size: 0,
            }
        }

        pub fn update(&mut self, sample_rate: f64) {
            self.filter_l.update(sample_rate);
            self.filter_r.update(sample_rate);
            self.ring_len = (sample_rate * 0.4) as usize;
            self.ring = vec![0.0; self.ring_len];
            self.ring_pos = 0;
            self.ring_sum = 0.0;
            self.short_ring_len = (sample_rate * 3.0) as usize;
            self.short_ring = vec![0.0; self.short_ring_len];
            self.short_ring_pos = 0;
            self.short_ring_sum = 0.0;
            self.block_size = self.ring_len;
        }

        pub fn process_sample(&mut self, left: f64, right: f64) {
            let kl = self.filter_l.tick(left);
            let kr = self.filter_r.tick(right);
            let ms = kl * kl + kr * kr;

            if self.ring_len > 0 {
                self.ring_sum -= self.ring[self.ring_pos];
                self.ring[self.ring_pos] = ms;
                self.ring_sum += ms;
                self.ring_pos = (self.ring_pos + 1) % self.ring_len;
                self.momentary_val = ms_to_lufs(self.ring_sum / self.ring_len as f64);
            }

            if self.short_ring_len > 0 {
                self.short_ring_sum -= self.short_ring[self.short_ring_pos];
                self.short_ring[self.short_ring_pos] = ms;
                self.short_ring_sum += ms;
                self.short_ring_pos = (self.short_ring_pos + 1) % self.short_ring_len;
                self.short_term_val = ms_to_lufs(self.short_ring_sum / self.short_ring_len as f64);
            }

            self.block_sum += ms;
            self.block_count += 1;
            if self.block_count >= self.block_size && self.block_size > 0 {
                let block_ms = self.block_sum / self.block_count as f64;
                let block_lufs = ms_to_lufs(block_ms);
                if block_lufs > -70.0 {
                    self.ungated_sum += block_ms;
                    self.ungated_count += 1;
                    let ungated_lufs = ms_to_lufs(self.ungated_sum / self.ungated_count as f64);
                    if block_lufs > ungated_lufs - 10.0 {
                        self.integrated_sum += block_ms;
                        self.integrated_count += 1;
                        self.integrated_val =
                            ms_to_lufs(self.integrated_sum / self.integrated_count as f64);
                    }
                }
                self.block_sum = 0.0;
                self.block_count = 0;
            }
        }

        pub fn process(&mut self, left: &[f64], right: &[f64]) {
            for i in 0..left.len() {
                self.process_sample(left[i], right[i]);
            }
        }

        pub fn momentary(&self) -> f64 {
            self.momentary_val
        }
        pub fn short_term(&self) -> f64 {
            self.short_term_val
        }
        pub fn integrated(&self) -> f64 {
            self.integrated_val
        }

        pub fn reset(&mut self) {
            self.filter_l.reset();
            self.filter_r.reset();
            self.ring.fill(0.0);
            self.ring_pos = 0;
            self.ring_sum = 0.0;
            self.short_ring.fill(0.0);
            self.short_ring_pos = 0;
            self.short_ring_sum = 0.0;
            self.integrated_sum = 0.0;
            self.integrated_count = 0;
            self.ungated_sum = 0.0;
            self.ungated_count = 0;
            self.block_sum = 0.0;
            self.block_count = 0;
            self.momentary_val = -200.0;
            self.short_term_val = -200.0;
            self.integrated_val = -200.0;
        }
    }

    impl Default for LoudnessMeter {
        fn default() -> Self {
            Self::new()
        }
    }
}

// Re-export the active implementation
#[cfg(feature = "loudness")]
pub use meter_ebur128::LoudnessMeter;
#[cfg(not(feature = "loudness"))]
pub use meter_fallback::LoudnessMeter;

// ── Autogain processor ──────────────────────────────────────────────────

/// Automatic gain control with dual-timeline LUFS tracking.
///
/// Uses a long-time window for gradual correction and a short-time window
/// for fast transient/surge handling (LSP's dual-timeline approach).
///
/// The metering backend depends on whether the `loudness` feature is enabled:
/// - With `loudness`: uses ebur128 for EBU R128 compliant measurement
/// - Without: uses the lightweight fallback K-weighting meter
pub struct AutoGain {
    /// EBU R128 meter for long/short measurement.
    meter: LoudnessMeter,

    /// Lightweight windowed meters for the dual-timeline gain control.
    /// These use custom window sizes (not the standard 400ms/3s).
    meter_long: WindowMeter,
    meter_short: WindowMeter,
    filter_l: KWeightingFilter,
    filter_r: KWeightingFilter,

    /// Current gain (linear).
    gain: f64,

    // Per-sample gain change rates (exponential multipliers)
    long_grow: f64,
    long_fall: f64,
    short_grow: f64,
    short_fall: f64,

    // Parameters
    /// Target loudness in LUFS.
    pub target_lufs: f64,
    /// Long measurement period in ms (100-2000).
    pub long_period_ms: f64,
    /// Short measurement period in ms (5-100).
    pub short_period_ms: f64,
    /// Long grow speed in dB/s.
    pub long_grow_dbs: f64,
    /// Long fall speed in dB/s.
    pub long_fall_dbs: f64,
    /// Short grow speed in dB/s.
    pub short_grow_dbs: f64,
    /// Short fall speed in dB/s.
    pub short_fall_dbs: f64,
    /// Silence threshold in LUFS (below this, gain is frozen).
    pub silence_lufs: f64,
    /// Maximum gain in dB (0 = unlimited).
    pub max_gain_db: f64,
    /// Deviation in dB — when the short-term level deviates by more than
    /// this from the target, the short-time gain speed kicks in.
    pub deviation_db: f64,

    /// Last applied gain in dB (for metering).
    pub last_gain_db: f64,

    sample_rate: f64,
}

impl AutoGain {
    pub fn new() -> Self {
        Self {
            meter: LoudnessMeter::new(),
            meter_long: WindowMeter::new(),
            meter_short: WindowMeter::new(),
            filter_l: KWeightingFilter::new(),
            filter_r: KWeightingFilter::new(),
            gain: 1.0,
            long_grow: 1.0,
            long_fall: 1.0,
            short_grow: 1.0,
            short_fall: 1.0,
            target_lufs: -14.0,
            long_period_ms: 400.0,
            short_period_ms: 20.0,
            long_grow_dbs: 24.0,
            long_fall_dbs: 24.0,
            short_grow_dbs: 300.0,
            short_fall_dbs: 1200.0,
            silence_lufs: -72.0,
            max_gain_db: 36.0,
            deviation_db: 12.0,
            last_gain_db: 0.0,
            sample_rate: 48000.0,
        }
    }

    /// Update coefficients after parameter changes.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.meter.update(sample_rate);
        self.filter_l.update(sample_rate);
        self.filter_r.update(sample_rate);

        let long_samples = (self.long_period_ms * 0.001 * sample_rate) as usize;
        let short_samples = (self.short_period_ms * 0.001 * sample_rate) as usize;
        self.meter_long.resize(long_samples.max(1));
        self.meter_short.resize(short_samples.max(1));

        let ksr = (10.0_f64.ln() / 20.0) / sample_rate;
        self.long_grow = (self.long_grow_dbs * ksr).exp();
        self.long_fall = (-self.long_fall_dbs * ksr).exp();
        self.short_grow = (self.short_grow_dbs * ksr).exp();
        self.short_fall = (-self.short_fall_dbs * ksr).exp();
    }

    /// Process a stereo sample pair, applying auto-gain in-place.
    #[inline]
    pub fn process_sample(&mut self, left: &mut f64, right: &mut f64) {
        // K-weight for the custom-window dual-timeline meters
        let kl = self.filter_l.tick(*left);
        let kr = self.filter_r.tick(*right);
        let ms = kl * kl + kr * kr;

        self.meter_long.push(ms);
        self.meter_short.push(ms);

        let long_lufs = ms_to_lufs(self.meter_long.mean());
        let short_lufs = ms_to_lufs(self.meter_short.mean());

        // Silence detection: freeze gain
        if short_lufs <= self.silence_lufs {
            *left *= self.gain;
            *right *= self.gain;
            self.last_gain_db = gain_to_db(self.gain);
            return;
        }

        let effective_long = long_lufs + gain_to_db(self.gain);
        let effective_short = short_lufs + gain_to_db(self.gain);
        let error = self.target_lufs - effective_long;
        let short_error = self.target_lufs - effective_short;

        let use_short = short_error.abs() > self.deviation_db;

        if use_short {
            if short_error > 0.0 {
                self.gain *= self.short_grow;
            } else {
                self.gain *= self.short_fall;
            }
        } else if error > 0.0 {
            self.gain *= self.long_grow;
        } else {
            self.gain *= self.long_fall;
        }

        // Max gain limiting
        if self.max_gain_db > 0.0 {
            let max_gain = db_to_gain(self.max_gain_db);
            if self.gain > max_gain {
                self.gain = max_gain;
            }
        }

        // Min gain: don't attenuate more than 36dB
        let min_gain = db_to_gain(-36.0);
        if self.gain < min_gain {
            self.gain = min_gain;
        }

        *left *= self.gain;
        *right *= self.gain;
        self.last_gain_db = gain_to_db(self.gain);
    }

    /// Process a buffer of stereo audio, applying auto-gain in-place.
    ///
    /// Also feeds the EBU R128 meter for standards-compliant readings.
    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Feed the EBU R128 meter with the input (pre-gain) signal
        self.meter.process(left, right);

        for i in 0..left.len() {
            self.process_sample(&mut left[i], &mut right[i]);
        }
    }

    /// Get the current gain in dB.
    pub fn gain_db(&self) -> f64 {
        self.last_gain_db
    }

    /// Access the underlying loudness meter for EBU R128 readings.
    pub fn meter(&self) -> &LoudnessMeter {
        &self.meter
    }

    pub fn reset(&mut self) {
        self.meter.reset();
        self.filter_l.reset();
        self.filter_r.reset();
        self.meter_long.clear();
        self.meter_short.clear();
        self.gain = 1.0;
        self.last_gain_db = 0.0;
    }
}

impl Default for AutoGain {
    fn default() -> Self {
        Self::new()
    }
}

// ── Windowed mean-square meter (internal) ───────────────────────────────

/// Simple ring-buffer mean-square meter for the dual-timeline AutoGain.
struct WindowMeter {
    ring: Vec<f64>,
    pos: usize,
    sum: f64,
}

impl WindowMeter {
    fn new() -> Self {
        Self {
            ring: Vec::new(),
            pos: 0,
            sum: 0.0,
        }
    }

    fn resize(&mut self, len: usize) {
        self.ring = vec![0.0; len];
        self.pos = 0;
        self.sum = 0.0;
    }

    #[inline]
    fn push(&mut self, ms: f64) {
        if self.ring.is_empty() {
            return;
        }
        self.sum -= self.ring[self.pos];
        self.ring[self.pos] = ms;
        self.sum += ms;
        self.pos = (self.pos + 1) % self.ring.len();
    }

    #[inline]
    fn mean(&self) -> f64 {
        if self.ring.is_empty() {
            return 0.0;
        }
        (self.sum / self.ring.len() as f64).max(0.0)
    }

    fn clear(&mut self) {
        self.ring.fill(0.0);
        self.pos = 0;
        self.sum = 0.0;
    }
}

// ── Utilities ───────────────────────────────────────────────────────────

/// dBFS to LUFS offset per BS.1770: -0.691 dB.
const DBFS_TO_LUFS_DB: f64 = -0.691;

/// Convert mean-square power to LUFS.
#[inline]
fn ms_to_lufs(ms: f64) -> f64 {
    if ms <= 0.0 {
        return -200.0;
    }
    DBFS_TO_LUFS_DB + 10.0 * ms.log10()
}

#[inline]
fn db_to_gain(db: f64) -> f64 {
    if db <= -200.0 {
        0.0
    } else {
        10.0_f64.powf(db / 20.0)
    }
}

#[inline]
fn gain_to_db(gain: f64) -> f64 {
    if gain <= 0.0 {
        -200.0
    } else {
        20.0 * gain.log10()
    }
}
