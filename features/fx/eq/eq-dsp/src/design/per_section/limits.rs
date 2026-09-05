//! Band frequency limits, and the tracked-frequency bookkeeping.
//!
//! Apart from the shape helpers because it is the same for all of them:
//! whether a band's centre has drifted outside what the current sample rate
//! can represent, and what to do about it.

use super::{eval_squared_mag_scalar, Prototype};

/// Pro-Q's `check_frequency_within_band_limits @ 0x18010e7f0`.
///
/// Returns `false` when `freq` is outside the band's valid magnitude range.
///
/// Logic from the binary:
/// 1. If `upper > 0` and `upper < freq` → out of range, return `false`.
/// 2. If `abs_threshold == 0` → no magnitude check, return `true`.
/// 3. Otherwise compare `mp_at_freq · abs_threshold` against
///    `min(|freq - band_edge_low|, |freq - alt_edge|)`, where `alt_edge =
///    band_edge_high` if `upper ≤ 0` else `vt10(...)` (binary uses an
///    additional vtable lookup we don't model when `upper > 0`).
///
/// Returns `true` (in-band) when the magnitude term is strictly less than
/// the distance term — i.e., the magnitude is below threshold for the
/// frequency's distance to nearest band edge.
#[must_use]
pub fn check_frequency_within_band_limits(
    proto: &Prototype,
    freq: f64,
    mp_at_freq: f64,
    abs_threshold: f64,
    upper: f64,
) -> bool {
    if upper > 0.0 && upper < freq {
        return false;
    }
    if abs_threshold == 0.0 {
        return true;
    }
    // Binary: vt10(band, upper). Without an analog handle we
    // conservatively reuse band_edge_high — callers in our wired
    // pipeline supply `upper <= 0` for the no-vt10 path.
    let alt_edge = proto.band_edge_high;
    let d_low = (freq - proto.band_edge_low).abs();
    let d_alt = (freq - alt_edge).abs();
    let d_min = d_low.min(d_alt);
    mp_at_freq * abs_threshold < d_min
}

/// Pro-Q's `update_tracked_band_frequencies @ 0x18010ce20`.
///
/// Side-effect helper that:
/// 1. For `mode == 2`: refreshes `prev_wz` magnitude cache via
///    `vt10(wz)`. If wz fails the band-limits check, zeros wz and
///    downgrades mode to 1.
/// 2. For any `mode > 0` (including post-downgrade): refreshes
///    `prev_wp` similarly, and on band-limits failure either snaps
///    `wp = wz` and downgrades mode to 1 (when mode was 2) or zeros
///    wp and downgrades mode to 0 (when mode was 1).
///
/// `mode == 0` short-circuits to a no-op.
pub fn update_tracked_band_frequencies(proto: &mut Prototype, abs_threshold: f64, upper: f64) {
    if proto.mode == 0 {
        return;
    }

    if proto.mode == 2 {
        let mp_wz = if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            eval_squared_mag_scalar(&coeffs, proto.wz)
        } else {
            0.0
        };
        proto.prev_wz = mp_wz;
        if !check_frequency_within_band_limits(proto, proto.wz, mp_wz, abs_threshold, upper) {
            proto.wz = 0.0;
            proto.mode = 1;
        }
    }

    if proto.mode > 0 {
        let mp_wp = if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            eval_squared_mag_scalar(&coeffs, proto.wp)
        } else {
            0.0
        };
        proto.prev_wp = mp_wp;
        if !check_frequency_within_band_limits(proto, proto.wp, mp_wp, abs_threshold, upper) {
            proto.wp = 0.0;
            if proto.mode == 2 {
                proto.wp = proto.wz;
                proto.mode = 1;
                return;
            }
            proto.mode = 0;
        }
    }
}
