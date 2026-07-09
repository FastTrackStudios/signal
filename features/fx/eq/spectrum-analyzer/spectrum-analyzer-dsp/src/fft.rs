//! Real-input FFT wrapper producing a squared-magnitude half-spectrum.

use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Wraps a real-to-complex FFT of a fixed size and exposes a one-shot
/// "window, transform, squared magnitude" path.
pub struct RealFft {
    fft: Arc<dyn RealToComplex<f32>>,
    size: usize,
    scratch: Vec<realfft::num_complex::Complex<f32>>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
}

impl RealFft {
    pub fn new(size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(size);
        let scratch = fft.make_scratch_vec();
        let spectrum = fft.make_output_vec();
        Self {
            fft,
            size,
            scratch,
            spectrum,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Number of half-spectrum bins (`size/2 + 1`).
    pub fn num_bins(&self) -> usize {
        self.size / 2 + 1
    }

    /// Transform `input` (length == size, already windowed) and write the
    /// squared magnitude of each half-spectrum bin into `out` (length == num_bins).
    pub fn forward_sqr_mag(&mut self, input: &mut [f32], out: &mut [f32]) {
        debug_assert_eq!(input.len(), self.size);
        debug_assert_eq!(out.len(), self.num_bins());
        // realfft requires output buffer of num_bins complex values.
        let _ = self
            .fft
            .process_with_scratch(input, &mut self.spectrum, &mut self.scratch);
        for (o, c) in out.iter_mut().zip(self.spectrum.iter()) {
            *o = c.re * c.re + c.im * c.im;
        }
    }
}
