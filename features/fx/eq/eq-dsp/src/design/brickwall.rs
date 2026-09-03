//! Brickwall cut — the elliptic design behind Pro-Q's steepest slope.
//!
//! Brickwall is a slope *setting* on Low Cut and High Cut, sitting past 96
//! dB/oct in the list, and it had been implemented as "96 dB/oct again":
//! `Slope::order` returned 16 for both. That is not what the plugin does, and
//! the gap is not subtle. Swept at a 5 kHz high cut, twelve steps to the
//! octave:
//!
//! ```text
//!      Hz     Pro-Q       order 16
//!    4968      0.02          -0.00
//!    5004     -0.02          -0.00
//!    5040     -0.68          -0.02
//!    5340    -38.10          -9.63
//!    5658    -98.67         -17.25
//!    6350   -100.13         -33.26
//!   12699    -89.98        -131.38
//! ```
//!
//! Two things in that table say elliptic rather than Butterworth. The
//! transition is far too fast for any all-pole filter — flat at 5004 Hz and
//! ninety decibels down by 5658, an eighth of an octave later — and the
//! stopband then **stops falling**, holding at 90 dB for the rest of the
//! spectrum instead of running away to -157. Equiripple stopband with finite
//! transmission zeros is an elliptic (Cauer) response and nothing else.
//!
//! The design constants are read straight off that sweep:
//!
//! - **Stopband 90 dB.** The stopband minima sit on -89.98, -90.01, -90.14,
//!   -90.45 at 12.7, 6.0, 6.1 and 13.5 kHz. That is the equiripple floor to
//!   two decimal places.
//! - **Passband edge is the band's own frequency.** The last flat reading is
//!   5004 Hz on a 5 kHz band, and 996 Hz on a 1 kHz one.
//! - **Passband ripple under 0.05 dB.** Nothing in the passband deviates by
//!   more than 0.03 dB, which is the noise floor of the measurement.
//! - **Order 12.** The transition ratio is what fixes it: 90 dB is reached at
//!   1.128x the corner at 5 kHz and 1.14x at 1 kHz. The elliptic order
//!   equation at those ripple figures gives 11.8, and the order must be even
//!   to have no real pole. Order 14 is plainly too steep — it is 90 dB down
//!   at 1.077x, where the plugin is still at -40.
//!
//! The ratio holding across two decades of corner frequency, and across Q 0.3
//! to Q 1, is what says this is one fixed prototype scaled to the corner
//! rather than a Q-dependent design: Pro-Q's Q control does nothing at all on
//! Brickwall.

use std::f64::consts::PI;

use crate::biquad::{self, Coeffs};
use crate::elliptic::{ellipdeg, elliptic_asn, elliptic_k_complete, elliptic_sncndn};
use crate::transform;
use crate::zpk::{Complex, Zpk};

/// Prototype order. Even, so the response has no real pole and no zero at
/// infinity — every one of the twelve zeros is a finite notch in the stopband.
const ORDER: usize = 12;
/// Passband ripple in dB.
///
/// Fitted, not assumed. With the order and the stopband fixed, this is the
/// only knob left on the transition, and it is sharp: at 0.01 dB the curve
/// lags the plugin by 26 dB in the middle of the transition, at 0.05 it leads
/// by 8, and at 0.02 it tracks the whole hundred-decibel fall within about
/// three:
///
/// ```text
///      Hz     Pro-Q     0.01 dB    0.02 dB    0.05 dB
///    5120      -7.92      -4.84      -7.89     -12.84
///    5225     -23.48     -17.24     -21.80     -28.23
///    5330     -36.70     -29.58     -34.95     -42.73
///    5545     -67.70     -55.15     -64.05     -82.31
///    5657     -98.67     -72.51     -95.90     -90.40
/// ```
///
/// It is also consistent with the passband: 0.02 dB of ripple is exactly the
/// 0.00..0.03 spread the plugin's passband was measured at.
const PASSBAND_RIPPLE_DB: f64 = 0.02;

/// The elliptic analog prototype, passband edge at `omega = 1`.
///
/// Orfanidis' construction: the transmission zeros are `j / (k * cd(u_i K, k))`
/// and the poles `j * cd((u_i - j v0) K, k)`, with `u_i = (2i-1)/N` and `v0`
/// fixed by the passband ripple.
fn prototype() -> Zpk {
    let ep = (10.0f64.powf(PASSBAND_RIPPLE_DB / 10.0) - 1.0).sqrt();
    let es = (10.0f64.powi(9) - 1.0).sqrt();
    let k1 = ep / es;
    let mod_k = ellipdeg(ORDER, k1);
    let kp = (1.0 - mod_k * mod_k).max(0.0).sqrt();
    let big_k = elliptic_k_complete(mod_k * mod_k);

    // v0 solves sn(j v0 N K1, k1) = j/ep. Rather than an inverse sn of an
    // imaginary argument, use sn(jx, k) = j sc(x, k'): the condition becomes
    // sc(w, k1') = 1/ep, which is a real sn of a real value.
    let k1p = (1.0 - k1 * k1).max(0.0).sqrt();
    let v0_param = elliptic_asn(1.0 / (1.0 + ep * ep).sqrt(), k1p);
    let v0 = v0_param / (f64::from(u32::try_from(ORDER).unwrap_or(u32::MAX)) * elliptic_k_complete(k1 * k1));

    // cd(x + jy, k) = cn(x + jy) / dn(x + jy); the shared denominator of the
    // complex-argument formulas cancels, leaving only real sn/cn/dn at x
    // (modulus k) and y (modulus k').
    let cd = |x: f64, y: f64| -> Complex {
        let (sn, cn, dn) = elliptic_sncndn(x, mod_k);
        let (s1, c1, d1) = elliptic_sncndn(y, kp);
        let k_squared = mod_k * mod_k;
        let num = Complex::new(cn * c1, -sn * dn * s1 * d1);
        let den = Complex::new(dn * c1 * d1, -k_squared * sn * cn * s1);
        let inv_den = den.inv();
        num * inv_den
    };

    let mut zeros = Vec::with_capacity(ORDER);
    let mut poles = Vec::with_capacity(ORDER);
    for i in 1..=ORDER / 2 {
        let u = 2.0f64.mul_add(i as i32 as f64, -1.0) / (ORDER as f64);
        let x = u * big_k;

        // Zero: purely imaginary, at 1/(k * cd(u K, k)) up the axis.
        let (_, cn_zero, dn_zero) = elliptic_sncndn(x, mod_k);
        let zeta = cn_zero / dn_zero;
        let z = 1.0 / (mod_k * zeta);
        zeros.push(Complex::new(0.0, z));
        zeros.push(Complex::new(0.0, -z));

        // Pole: j * cd((u - j v0) K, k).
        let cdv = cd(x, -v0 * big_k);
        let p = Complex::new(-cdv.im, cdv.re);
        poles.push(p);
        poles.push(p.conj());
    }

    Zpk::new(zeros, poles, 1.0)
}

/// Brickwall low-pass (`High Cut`) or high-pass (`Low Cut`) at `freq_hz`.
///
/// Q is deliberately absent: swept from 0.3 to 1 the plugin's Brickwall
/// response does not move, so a Q term here would be inventing behaviour.
pub(super) fn brickwall_cascade(freq_hz: f64, sample_rate: f64, highpass: bool) -> Vec<Coeffs> {
    // The transition needs room above the corner or it folds at Nyquist and
    // the stopband ripple lands back in the passband. Leave the whole
    // transition inside the band.
    let ceiling = sample_rate * 0.5 / 1.25;
    let fc = freq_hz.clamp(sample_rate / 4800.0, ceiling);

    let proto = prototype();
    // Pre-warp, so the passband edge lands on the corner after the bilinear
    // transform rather than a few percent under it.
    let wa = 2.0 * sample_rate * (PI * fc / sample_rate).tan();

    let analog = if highpass {
        // LP->HP is s -> wa/s. The order is even, so every zero is finite and
        // the inversion leaves the counts matched — nothing has to be added
        // at the origin.
        Zpk::new(
            proto.zeros.iter().map(|&z| Complex::new(wa, 0.0) / z).collect(),
            proto.poles.iter().map(|&p| Complex::new(wa, 0.0) / p).collect(),
            1.0,
        )
    } else {
        Zpk::new(
            proto.zeros.iter().map(|&z| z * wa).collect(),
            proto.poles.iter().map(|&p| p * wa).collect(),
            1.0,
        )
    };

    let digital = transform::bilinear(&analog, sample_rate);
    let mut sos = biquad::zpk_to_sos(&digital);

    // Normalise in the passband: DC for a low cut's stopband is the wrong end,
    // so each shape is referenced to the frequency it passes.
    let w_ref = if highpass { PI } else { 0.0 };
    let mag = biquad::eval_sos(&sos, w_ref).mag();
    if mag > 1.0e-30 {
        let g = 1.0 / mag;
        if let Some(first) = sos.first_mut() {
            first[3] *= g;
            first[4] *= g;
            first[5] *= g;
        }
    }
    sos
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn db_at(sos: &[Coeffs], hz: f64) -> f64 {
        biquad::mag_db_sos(sos, 2.0 * PI * hz / SR)
    }

    /// The shape the plugin was measured to have, at the frequencies it was
    /// measured at. These are the readings in this module's header.
    #[test]
    fn high_cut_matches_the_measured_brickwall() {
        let sos = brickwall_cascade(5000.0, SR, false);
        for (hz, want) in [(1000.0, 0.0), (4000.0, 0.0), (4968.0, 0.0)] {
            let got = db_at(&sos, hz);
            assert!(
                (got - want).abs() < 0.1,
                "passband should be flat at {hz} Hz, got {got:.2} dB"
            );
        }
        // Ninety decibels down within an eighth of an octave.
        let edge = db_at(&sos, 5658.0);
        assert!(edge < -85.0, "should be past 85 dB down at 5658 Hz, got {edge:.2}");
        // And it must STAY there rather than running away like an all-pole
        // cascade — this is the half of the shape a Butterworth cannot do.
        // Equiripple means the stopband dips to a null at every transmission
        // zero, so what is testable is the ceiling between them.
        let mut peak = f64::NEG_INFINITY;
        let mut hz = 5800.0;
        while hz < 23_000.0 {
            peak = peak.max(db_at(&sos, hz));
            hz *= 1.002;
        }
        assert!(
            (-92.0..-87.0).contains(&peak),
            "the stopband ceiling should sit on -90 dB, got {peak:.2}"
        );
    }

    #[test]
    fn low_cut_is_the_mirror() {
        let sos = brickwall_cascade(1000.0, SR, true);
        assert!(db_at(&sos, 4000.0).abs() < 0.1, "passband above the corner is flat");
        assert!(db_at(&sos, 1004.0).abs() < 0.2, "flat right up to the corner");
        let edge = db_at(&sos, 1000.0 / 1.14);
        assert!(edge < -85.0, "85 dB down an eighth of an octave below, got {edge:.2}");
        for hz in [100.0, 300.0, 700.0] {
            let got = db_at(&sos, hz);
            assert!(got < -80.0, "stopband at {hz} Hz should hold down, got {got:.2}");
        }
    }

    /// The transition ratio is the same wherever the band sits — one
    /// prototype, scaled. Measured at 1 kHz and 5 kHz on the plugin.
    #[test]
    fn the_transition_ratio_does_not_move_with_frequency() {
        for fc in [200.0f64, 1000.0, 5000.0] {
            let sos = brickwall_cascade(fc, SR, false);
            let flat = db_at(&sos, fc * 0.99);
            let down = db_at(&sos, fc * 1.14);
            assert!(flat.abs() < 0.1, "flat below {fc} Hz, got {flat:.2}");
            assert!(down < -85.0, "90 dB down at 1.14x {fc} Hz, got {down:.2}");
        }
    }
}
