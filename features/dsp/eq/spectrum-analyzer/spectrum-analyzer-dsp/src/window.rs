//! Analysis windows.
//!
//! Each window is built *periodic* (denominator `N`, not `N-1`) — the correct
//! form for spectral analysis where the window wraps. The Hann window also folds
//! in the `2/N` amplitude-correction scale so the squared magnitude out of the
//! FFT reads directly as power per bin.

use std::f32::consts::PI;

/// Window function selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowKind {
    #[default]
    Hann,
    Hamming,
    Blackman,
    FlatTop,
}

/// Fill `out` with the given periodic window, scaled by `2 / N` for amplitude
/// correction (so power read off the FFT is normalized).
pub fn build_scaled(kind: WindowKind, out: &mut [f32]) {
    let n = out.len();
    if n == 0 {
        return;
    }
    let scale = 2.0 / n as f32;
    let nf = n as f32;
    for (i, w) in out.iter_mut().enumerate() {
        let x = i as f32 / nf; // periodic: 0..1 exclusive of 1
        let v = match kind {
            WindowKind::Hann => 0.5 - 0.5 * (2.0 * PI * x).cos(),
            WindowKind::Hamming => 0.54 - 0.46 * (2.0 * PI * x).cos(),
            WindowKind::Blackman => 0.42 - 0.5 * (2.0 * PI * x).cos() + 0.08 * (4.0 * PI * x).cos(),
            WindowKind::FlatTop => {
                // Standard 5-term flat-top (good amplitude accuracy).
                let a = [1.0, 1.93, 1.29, 0.388, 0.028];
                a[0] - a[1] * (2.0 * PI * x).cos() + a[2] * (4.0 * PI * x).cos()
                    - a[3] * (6.0 * PI * x).cos()
                    + a[4] * (8.0 * PI * x).cos()
            }
        };
        *w = v * scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_are_zero() {
        let mut w = vec![0.0f32; 8];
        build_scaled(WindowKind::Hann, &mut w);
        // Periodic Hann starts at 0; symmetric peak in the middle.
        assert!(w[0].abs() < 1e-6);
        assert!(w[4] > w[0]);
    }
}
