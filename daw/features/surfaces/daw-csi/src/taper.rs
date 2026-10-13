//! Fader taper: 14-bit fader position ↔ linear track volume.
//!
//! Piecewise-linear in dB through anchor points matched to the
//! X-Touch's printed fader scale (+10 at the top, 0 dB at ~72%
//! throw, −∞ at the bottom). The DAW stores volume as a linear
//! multiplier (1.0 = unity), same as REAPER's `D_VOL`.

/// `(normalized_position, dB)` anchors, descending. Matches the
/// silkscreen on an X-Touch / MCU fader.
const ANCHORS: [(f64, f64); 7] = [
    (1.00, 10.0),
    (0.72, 0.0),
    (0.50, -10.0),
    (0.32, -20.0),
    (0.20, -30.0),
    (0.08, -50.0),
    (0.00, -72.0), // treated as −∞ at exactly 0
];

/// Fader position (0–16383) → linear volume.
pub fn fader_to_volume(pos: u16) -> f64 {
    let n = pos as f64 / 16383.0;
    if n <= 0.0 {
        return 0.0;
    }
    let db = pos_to_db(n);
    10f64.powf(db / 20.0)
}

/// Linear volume → fader position (0–16383).
pub fn volume_to_fader(volume: f64) -> u16 {
    if volume <= 0.0 {
        return 0;
    }
    let db = 20.0 * volume.log10();
    let n = db_to_pos(db).clamp(0.0, 1.0);
    (n * 16383.0).round() as u16
}

fn pos_to_db(n: f64) -> f64 {
    let n = n.clamp(0.0, 1.0);
    for w in ANCHORS.windows(2) {
        let (hi_n, hi_db) = w[0];
        let (lo_n, lo_db) = w[1];
        if n >= lo_n {
            let f = (n - lo_n) / (hi_n - lo_n);
            return lo_db + f * (hi_db - lo_db);
        }
    }
    ANCHORS.last().unwrap().1
}

fn db_to_pos(db: f64) -> f64 {
    if db >= ANCHORS[0].1 {
        return 1.0;
    }
    for w in ANCHORS.windows(2) {
        let (hi_n, hi_db) = w[0];
        let (lo_n, lo_db) = w[1];
        if db >= lo_db {
            let f = (db - lo_db) / (hi_db - lo_db);
            return lo_n + f * (hi_n - lo_n);
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_sits_at_printed_zero() {
        let pos = volume_to_fader(1.0);
        assert_eq!(pos, (0.72f64 * 16383.0).round() as u16);
        // And maps back to unity within rounding.
        assert!((fader_to_volume(pos) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn endpoints() {
        assert_eq!(fader_to_volume(0), 0.0);
        assert_eq!(volume_to_fader(0.0), 0);
        // Top of throw = +10 dB.
        assert!((fader_to_volume(16383) - 10f64.powf(0.5)).abs() < 1e-3);
        // Anything ≥ +10 dB clamps to the top.
        assert_eq!(volume_to_fader(10.0), 16383);
    }

    #[test]
    fn roundtrip_monotonic() {
        let mut last = -1.0;
        for pos in (0..=16383).step_by(127) {
            let vol = fader_to_volume(pos);
            assert!(vol >= last, "taper must be monotonic");
            last = vol;
            if pos > 0 {
                let back = volume_to_fader(vol);
                assert!(
                    (back as i32 - pos as i32).abs() <= 2,
                    "roundtrip {pos} → {vol} → {back}"
                );
            }
        }
    }
}
