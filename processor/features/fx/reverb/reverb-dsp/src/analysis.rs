//! Display probes: the algorithm's own impulse response, folded small.
//!
//! A reverb's decay knob is a number; what the reverb DOES with it is
//! an impulse response, and the two are not the same picture. A plate
//! is dense from the first millisecond, a room shows its reflections
//! before its tail, a spring drips, a bloom rises before it falls — and
//! an exponential drawn from the decay time shows none of that. This
//! module renders the real thing so a host can draw it instead.
//!
//! Nothing here is realtime-safe and nothing needs to be: an envelope
//! is rendered once per settings change, on whatever thread the host
//! keeps for work that takes milliseconds.

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, AlgorithmType};

/// The rate the probe renders at.
///
/// The engines' own reference rate — the one `ir_metrics` measures them
/// at — so the envelope is of the reverb as shipped rather than of a
/// cheaper approximation of it.
const RATE: f64 = 48_000.0;

/// How far down the tail the envelope is followed, in dB below its peak.
pub const FLOOR_DB: f32 = -60.0;

/// Render `algorithm`'s impulse response and fold it into `out`.
///
/// `seconds` is the window the envelope covers, left to right; `out`
/// receives one value per equal slice of it, the RMS level in that slice
/// in dB relative to the loudest slice, clamped to [`FLOOR_DB`]..0. A
/// slice after the tail has died reads [`FLOOR_DB`].
///
/// Linear in time rather than logarithmic, because that is how a tail
/// is read: the question is how far right it reaches, and a log axis
/// would put every reverb's first hundred milliseconds across half the
/// picture.
pub fn impulse_envelope(
    algorithm: AlgorithmType,
    variant: usize,
    params: &AlgorithmParams,
    seconds: f64,
    out: &mut [f32],
) {
    if out.is_empty() {
        return;
    }
    let seconds = seconds.clamp(0.05, 12.0);
    let total = num::f64_to_index(seconds * RATE).max(out.len());
    let per_slice = total.checked_div(out.len()).unwrap_or(1).max(1);

    let mut engine = crate::algorithms::create(algorithm, variant, RATE);
    engine.set_params(params);
    engine.reset();

    let mut peak = 0.0_f64;
    for (i, slot) in out.iter_mut().enumerate() {
        let mut energy = 0.0_f64;
        for n in 0..per_slice {
            let x = if i == 0 && n == 0 { 1.0 } else { 0.0 };
            let (l, r) = engine.tick(x, x);
            if !(l.is_finite() && r.is_finite()) {
                break;
            }
            energy = l.mul_add(l, r.mul_add(r, energy));
        }
        let rms = (energy / num::count_to_f64(per_slice)).sqrt();
        peak = peak.max(rms);
        // Linear for now; turned into dB against the peak below.
        *slot = num::f64_to_f32(rms);
    }
    let peak = num::f64_to_f32(peak.max(1.0e-12));
    for slot in out.iter_mut() {
        let db = 20.0 * (*slot / peak).max(1.0e-9).log10();
        *slot = db.clamp(FLOOR_DB, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{FLOOR_DB, impulse_envelope};
    use crate::algorithm::{AlgorithmParams, AlgorithmType};

    /// The peak slice reads 0 dB and the tail falls from it.
    #[test]
    fn the_envelope_is_relative_to_its_peak_and_falls() {
        let mut out = [0.0_f32; 48];
        impulse_envelope(AlgorithmType::Room, 0, &AlgorithmParams::default(), 2.0, &mut out);
        let peak = out.iter().copied().fold(f32::MIN, f32::max);
        assert!((peak - 0.0).abs() < 1e-5, "peak slice is 0 dB, got {peak}");
        let head = out[..8].iter().copied().fold(f32::MIN, f32::max);
        let tail = out[40..].iter().copied().fold(f32::MIN, f32::max);
        assert!(tail < head - 6.0, "tail {tail} should sit well under head {head}");
        assert!(out.iter().all(|v| (FLOOR_DB..=0.0).contains(v)));
    }

    /// A longer decay reaches further right: the envelope is of the
    /// setting, not of a fixed shape.
    #[test]
    fn a_longer_decay_reaches_further() {
        let short = AlgorithmParams {
            decay: 0.2,
            ..AlgorithmParams::default()
        };
        let long = AlgorithmParams {
            decay: 0.8,
            ..AlgorithmParams::default()
        };
        let (mut a, mut b) = ([0.0_f32; 48], [0.0_f32; 48]);
        impulse_envelope(AlgorithmType::Hall, 0, &short, 3.0, &mut a);
        impulse_envelope(AlgorithmType::Hall, 0, &long, 3.0, &mut b);
        let late = 36;
        assert!(b[late] > a[late] + 6.0, "long {} vs short {}", b[late], a[late]);
    }
}
