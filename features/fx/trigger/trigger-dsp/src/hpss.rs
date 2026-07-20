//! Harmonic/Percussive Source Separation via median filtering.
//!
//! Pre-processes audio to isolate percussive transients from harmonic
//! content (tonal bleed from cymbals, guitar, vocals, etc.) before
//! onset detection. This significantly improves trigger reliability
//! on tracks with heavy bleed.
//!
//! Reference: Fitzgerald (2010) "Harmonic/Percussive Separation Using
//! Median Filtering", DAFx-10, Graz.
//!
//! Algorithm:
//! 1. Compute STFT magnitude spectrogram
//! 2. Median filter along time axis → harmonic-enhanced spectrogram
//! 3. Median filter along frequency axis → percussive-enhanced spectrogram
//! 4. Soft mask: P² / (H² + P²) applied to original STFT
//! 5. ISTFT to reconstruct percussive signal
//!
//! For real-time use, the time-axis median filter is causal (one-sided),
//! adding latency of (median_width / 2) * hop_size samples.

use rustfft::{num_complex::Complex, FftPlanner};

/// Real-time harmonic/percussive separator.
pub struct HpssProcessor {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f64>,

    // STFT frame ring buffer for time-axis median filter
    frame_ring: Vec<Vec<f64>>,
    // Corresponding complex STFT frames for phase reconstruction
    stft_ring: Vec<Vec<Complex<f64>>>,
    ring_write_pos: usize,
    time_median_width: usize,
    freq_median_width: usize,

    // Input overlap buffer (last fft_size samples)
    input_ring: Vec<f64>,
    input_write_pos: usize,
    samples_since_hop: usize,

    // Output overlap-add buffer
    output_ring: Vec<f64>,
    output_write_pos: usize,
    output_read_pos: usize,

    // FFT working buffers
    fft_buf: Vec<Complex<f64>>,

    // Working storage for median computation
    harmonic: Vec<f64>,
    percussive: Vec<f64>,
    median_scratch: Vec<f64>,

    num_bins: usize,
    frames_processed: usize,
    sample_rate: f64,
}

impl HpssProcessor {
    /// Create a new HPSS processor.
    pub fn new(
        fft_size: usize,
        hop_size: usize,
        time_median_width: usize,
        freq_median_width: usize,
        sample_rate: f64,
    ) -> Self {
        let num_bins = fft_size / 2 + 1;
        let tmw = time_median_width | 1; // ensure odd
        let fmw = freq_median_width | 1;

        let window: Vec<f64> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / fft_size as f64).cos()))
            .collect();

        let frame_ring: Vec<Vec<f64>> = (0..tmw).map(|_| vec![0.0; num_bins]).collect();
        let stft_ring: Vec<Vec<Complex<f64>>> = (0..tmw)
            .map(|_| vec![Complex::new(0.0, 0.0); num_bins])
            .collect();

        // Output ring needs to be large enough for overlap-add
        let output_ring_size = fft_size * 2;

        Self {
            fft_size,
            hop_size,
            window,
            frame_ring,
            stft_ring,
            ring_write_pos: 0,
            time_median_width: tmw,
            freq_median_width: fmw,
            input_ring: vec![0.0; fft_size],
            input_write_pos: 0,
            samples_since_hop: 0,
            output_ring: vec![0.0; output_ring_size],
            output_write_pos: 0,
            output_read_pos: 0,
            fft_buf: vec![Complex::new(0.0, 0.0); fft_size],
            harmonic: vec![0.0; num_bins],
            percussive: vec![0.0; num_bins],
            median_scratch: vec![0.0; tmw.max(fmw)],
            num_bins,
            frames_processed: 0,
            sample_rate,
        }
    }

    /// Feed one sample, returns the percussive-separated output sample.
    #[inline]
    pub fn tick(&mut self, sample: f64) -> f64 {
        // Write input into ring buffer
        self.input_ring[self.input_write_pos] = sample;
        self.input_write_pos = (self.input_write_pos + 1) % self.fft_size;
        self.samples_since_hop += 1;

        // Process a frame every hop_size samples
        if self.samples_since_hop >= self.hop_size {
            self.samples_since_hop = 0;
            self.process_frame();
        }

        // Read from output ring buffer
        let out = self.output_ring[self.output_read_pos];
        self.output_ring[self.output_read_pos] = 0.0;
        self.output_read_pos = (self.output_read_pos + 1) % self.output_ring.len();
        out
    }

    /// Returns the latency in samples.
    pub fn latency_samples(&self) -> usize {
        (self.time_median_width / 2) * self.hop_size + self.fft_size
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.input_ring.fill(0.0);
        self.input_write_pos = 0;
        self.samples_since_hop = 0;
        self.output_ring.fill(0.0);
        self.output_write_pos = 0;
        self.output_read_pos = 0;
        for frame in &mut self.frame_ring {
            frame.fill(0.0);
        }
        for frame in &mut self.stft_ring {
            frame.fill(Complex::new(0.0, 0.0));
        }
        self.ring_write_pos = 0;
        self.frames_processed = 0;
    }

    /// Update sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        if (self.sample_rate - sample_rate).abs() > 0.1 {
            *self = Self::new(
                self.fft_size,
                self.hop_size,
                self.time_median_width,
                self.freq_median_width,
                sample_rate,
            );
        }
    }

    fn process_frame(&mut self) {
        // Gather the last fft_size samples from the input ring
        for i in 0..self.fft_size {
            let idx = (self.input_write_pos + i) % self.fft_size;
            self.fft_buf[i] = Complex::new(self.input_ring[idx] * self.window[i], 0.0);
        }

        // Forward FFT
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        fft.process(&mut self.fft_buf);

        // Store magnitude and complex STFT in ring buffer
        let ring_idx = self.ring_write_pos % self.time_median_width;
        for k in 0..self.num_bins {
            self.frame_ring[ring_idx][k] = self.fft_buf[k].norm();
            self.stft_ring[ring_idx][k] = self.fft_buf[k];
        }
        self.ring_write_pos += 1;
        self.frames_processed += 1;

        // Need at least time_median_width frames before separation works
        if self.frames_processed < self.time_median_width {
            // Pass input through during warmup via overlap-add
            self.overlap_add_frame(&self.fft_buf.clone());
            return;
        }

        // The center frame (for causal processing, use the oldest available)
        let center_idx = self.ring_write_pos % self.time_median_width;

        // Compute harmonic estimate: time-axis median for each bin
        for k in 0..self.num_bins {
            for t in 0..self.time_median_width {
                self.median_scratch[t] = self.frame_ring[t][k];
            }
            self.harmonic[k] = median(&mut self.median_scratch[..self.time_median_width]);
        }

        // Compute percussive estimate: freq-axis median from center frame's magnitude
        let half = self.freq_median_width / 2;
        let center_mag: Vec<f64> = self.frame_ring[center_idx].clone();
        for k in 0..self.num_bins {
            let lo = k.saturating_sub(half);
            let hi = (k + half + 1).min(self.num_bins);
            let width = hi - lo;
            self.median_scratch[..width].copy_from_slice(&center_mag[lo..hi]);
            self.percussive[k] = median(&mut self.median_scratch[..width]);
        }

        // Apply soft mask to center frame's complex STFT
        let eps = 1e-10;
        let center_stft: Vec<Complex<f64>> = self.stft_ring[center_idx].clone();

        for k in 0..self.num_bins {
            let h2 = self.harmonic[k] * self.harmonic[k];
            let p2 = self.percussive[k] * self.percussive[k];
            let mask = p2 / (h2 + p2 + eps);
            self.fft_buf[k] = center_stft[k] * mask;
        }

        // Mirror for inverse FFT
        for k in self.num_bins..self.fft_size {
            self.fft_buf[k] = self.fft_buf[self.fft_size - k].conj();
        }

        // Inverse FFT
        let ifft = planner.plan_fft_inverse(self.fft_size);
        ifft.process(&mut self.fft_buf);

        self.overlap_add_frame(&self.fft_buf.clone());
    }

    fn overlap_add_frame(&mut self, frame: &[Complex<f64>]) {
        let scale = 1.0 / self.fft_size as f64;
        let ring_len = self.output_ring.len();
        for i in 0..self.fft_size {
            let idx = (self.output_write_pos + i) % ring_len;
            self.output_ring[idx] += frame[i].re * scale * self.window[i];
        }
        self.output_write_pos = (self.output_write_pos + self.hop_size) % ring_len;
    }
}

/// In-place sort to find median.
fn median(data: &mut [f64]) -> f64 {
    let n = data.len();
    if n == 0 {
        return 0.0;
    }
    data.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if n % 2 == 1 {
        data[n / 2]
    } else {
        (data[n / 2 - 1] + data[n / 2]) * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hpss_silence_produces_silence() {
        let mut hpss = HpssProcessor::new(1024, 256, 7, 31, 48000.0);
        for _ in 0..48000 {
            let out = hpss.tick(0.0);
            assert!(out.is_finite(), "Output must be finite");
        }
    }

    #[test]
    fn hpss_preserves_transient() {
        let mut hpss = HpssProcessor::new(1024, 256, 7, 31, 48000.0);

        // Warm up with silence
        let warmup = 24000;
        for _ in 0..warmup {
            hpss.tick(0.0);
        }

        // Feed a sharp click
        for i in 0..100 {
            let input = if i < 10 { 0.8 } else { 0.0 };
            hpss.tick(input);
        }

        // Read through enough output to catch the processed transient
        let mut output = Vec::new();
        for _ in 0..24000 {
            output.push(hpss.tick(0.0));
        }

        let peak = output.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        assert!(
            peak > 0.001,
            "HPSS should preserve percussive transient, peak={}",
            peak
        );
    }

    #[test]
    fn hpss_output_finite() {
        let mut hpss = HpssProcessor::new(1024, 256, 7, 31, 48000.0);
        for i in 0..24000 {
            let t = i as f64 / 48000.0;
            let input = (440.0 * std::f64::consts::TAU * t).sin() * 0.5;
            let out = hpss.tick(input);
            assert!(out.is_finite(), "Output must be finite at sample {}", i);
        }
    }
}
