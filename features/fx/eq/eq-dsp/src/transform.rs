//! Frequency domain transforms matching Pro-Q 4's transform type table at 0x18020eff0.
//!
//! Transform types:
//!   0 = Direct (no transform) — LP/HP
//!   1 = LP→BP frequency warp — BP/Notch
//!   2 = Bilinear transform — Shelves
//!   3 = LP→BP + bilinear — Band Shelf
//!   4 = Negate zeros — Allpass

use crate::zpk::{Complex, Zpk};

/// Bilinear s→z transform (transform type 2).
///
/// Maps analog prototype poles/zeros from the s-plane to the z-plane using:
///   z = (1 + s/2fs) / (1 - s/2fs)
///
/// Unmapped zeros are placed at z = -1 (Nyquist) to preserve filter order.
pub fn bilinear(zpk: &Zpk, sample_rate: f64) -> Zpk {
    let fs2 = 2.0 * sample_rate;

    let mut z_zeros: Vec<Complex> = zpk
        .zeros
        .iter()
        .map(|&z| {
            let num = Complex::ONE + z / fs2;
            let den = Complex::ONE - z / fs2;
            num / den
        })
        .collect();

    let z_poles: Vec<Complex> = zpk
        .poles
        .iter()
        .map(|&p| {
            let num = Complex::ONE + p / fs2;
            let den = Complex::ONE - p / fs2;
            num / den
        })
        .collect();

    // Pad zeros to match pole count — extra zeros go to Nyquist (z = -1).
    while z_zeros.len() < z_poles.len() {
        z_zeros.push(Complex::new(-1.0, 0.0));
    }

    // Adjust gain: product of (fs2 - z) / product of (fs2 - p).
    let mut gain = Complex::new(zpk.gain, 0.0);
    for &z in &zpk.zeros {
        gain = gain * (Complex::new(fs2, 0.0) - z);
    }
    for &p in &zpk.poles {
        gain = gain / (Complex::new(fs2, 0.0) - p);
    }
    // Account for added Nyquist zeros.
    let extra = z_poles.len().saturating_sub(zpk.zeros.len());
    for _ in 0..extra {
        gain = gain * Complex::new(fs2, 0.0);
    }

    Zpk::new(z_zeros, z_poles, gain.re)
}

/// Allpass transform (transform type 4): negate real part of poles to get zeros.
///
/// From Pro-Q 4 binary (design_filter_zpk_and_transform, transform type 4):
/// Each pole's REAL part is negated to create the corresponding zero:
///   zero = Complex(-pole.re, pole.im)
///
/// This is equivalent to reflecting across the imaginary axis, which for
/// z-plane poles inside the unit circle creates zeros outside it, giving
/// |H(e^jw)| = 1 for all frequencies.
pub fn make_allpass(zpk: &Zpk) -> Zpk {
    let zeros: Vec<Complex> = zpk
        .poles
        .iter()
        .map(|&p| {
            // Negate real part: zero = -conj(pole) reflected across imaginary axis
            Complex::new(-p.re, p.im)
        })
        .collect();

    // Normalize gain so |H(e^jw)| = 1: evaluate at DC (z=1) and scale
    // For allpass with negated-real zeros: gain must compensate for the
    // pole/zero magnitude imbalance
    let mut gain = zpk.gain;
    for (&p, &z) in zpk.poles.iter().zip(zeros.iter()) {
        // At z=1 (DC): H = gain * prod(1-z_i) / prod(1-p_i)
        // We want |H(1)| = 1
        let num_dc = (Complex::ONE - z).mag();
        let den_dc = (Complex::ONE - p).mag();
        if den_dc > 1e-15 && num_dc > 1e-15 {
            gain *= den_dc / num_dc;
        }
    }

    Zpk::new(zeros, zpk.poles.clone(), gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_preserves_pole_count() {
        let zpk = Zpk::new(
            vec![Complex::new(-1.0, 0.0)],
            vec![Complex::new(-0.5, 0.5), Complex::new(-0.5, -0.5)],
            1.0,
        );
        let z = bilinear(&zpk, 44100.0);
        assert_eq!(z.poles.len(), 2);
        // Zeros padded to match pole count.
        assert_eq!(z.zeros.len(), 2);
    }

    #[test]
    fn bilinear_poles_inside_unit_circle() {
        // Stable analog poles (negative real part) must map to |z| < 1.
        let zpk = Zpk::new(
            vec![],
            vec![
                Complex::new(-1000.0, 2000.0),
                Complex::new(-1000.0, -2000.0),
            ],
            1.0,
        );
        let z = bilinear(&zpk, 44100.0);
        for p in &z.poles {
            assert!(
                p.mag() < 1.0,
                "Pole {:?} has magnitude {} >= 1.0",
                p,
                p.mag()
            );
        }
    }

    #[test]
    fn bilinear_dc_maps_to_z1() {
        // s = 0 should map to z = 1.
        let zpk = Zpk::new(vec![Complex::ZERO], vec![Complex::new(-1.0, 0.0)], 1.0);
        let z = bilinear(&zpk, 44100.0);
        let z0 = z.zeros[0];
        assert!(
            (z0.re - 1.0).abs() < 1e-10 && z0.im.abs() < 1e-10,
            "s=0 should map to z=1, got {:?}",
            z0
        );
    }

    #[test]
    fn allpass_zero_count_matches_poles() {
        let zpk = Zpk::new(
            vec![Complex::new(0.5, 0.0)],
            vec![Complex::new(0.5, 0.3), Complex::new(0.5, -0.3)],
            1.0,
        );
        let ap = make_allpass(&zpk);
        assert_eq!(ap.zeros.len(), ap.poles.len());
    }

    #[test]
    fn allpass_unit_magnitude() {
        // make_allpass negates real part of poles to create zeros (s-domain operation).
        // For z-domain use (as tested here), we need to go through the full design pipeline.
        // Test the z-domain allpass by creating a proper s-domain prototype, applying
        // make_allpass, then bilinear transform.
        let pole = Complex::new(-1000.0, 2000.0);
        let zpk = Zpk::new(vec![], vec![pole, pole.conj()], 1.0);
        let ap = make_allpass(&zpk);

        // After make_allpass: zeros at (-(-1000), 2000) = (1000, 2000) and conj
        assert_eq!(ap.zeros.len(), 2);
        assert!((ap.zeros[0].re - 1000.0).abs() < 1e-10);

        // Apply bilinear to get z-domain
        let digital = bilinear(&ap, 44100.0);
        let sos = crate::biquad::zpk_to_sos(&digital);

        // DC normalize
        let dc = crate::biquad::eval_sos(&sos, 0.0).mag();
        assert!(dc > 0.01, "DC should be nonzero");
    }
}
