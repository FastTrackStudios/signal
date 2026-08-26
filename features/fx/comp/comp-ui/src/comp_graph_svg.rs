//! Portable compressor-graph math + SVG path generation.
//!
//! The pure core of the comp graph — ported from the signal domain's
//! `CompSurface` (features/rigs/guitar/ui/src/comp_surface.rs, itself a 1:1
//! port of the audio-gui vello painter): the soft-knee `compress_transfer`
//! curve, the Catmull-Rom `smooth_path` waveform fills, and the dB ↔ y
//! coordinate mapping. No plugin framework, no Dioxus — compiles for every
//! target (same split as eq-ui's `eq_graph_svg`). The `native`-gated
//! [`crate::comp_graph`] component renders these paths and adds the
//! interactions.

/// Display dB range (0 dBFS at the top … −`RANGE_DB` at the bottom). 60 dB:
/// matches the reference surface — the −20/−30 dB neighborhood where
/// thresholds live sits mid-window.
pub const RANGE_DB: f32 = 60.0;

/// Soft-knee transfer function — same math as audio-gui's
/// `compress_transfer` / CompSurface.
///
/// Below `threshold − knee/2` the output equals the input; above
/// `threshold + knee/2` the output follows the ratio slope; inside the knee
/// a quadratic blends the two with continuous value and first derivative.
pub fn compress_transfer(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    if ratio <= 1.0 {
        return input_db;
    }
    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee_db * 0.5;
    if knee_db > 0.001 && (input_db - threshold_db).abs() < half_knee {
        let x = input_db - threshold_db + half_knee;
        input_db - slope * x * x / (2.0 * knee_db)
    } else if input_db > threshold_db {
        input_db - slope * (input_db - threshold_db)
    } else {
        input_db
    }
}

/// dB (0 at the top … −[`RANGE_DB`] at the bottom) → y within a graph of
/// height `h`.
pub fn db_to_y(db: f64, h: f64) -> f64 {
    ((-db) / RANGE_DB as f64).clamp(0.0, 1.0) * h
}

/// Inverse of [`db_to_y`]: y within a graph of height `h` → dB (clamped to
/// the display range, 0 … −[`RANGE_DB`]).
pub fn y_to_db(y: f64, h: f64) -> f64 {
    -(y / h).clamp(0.0, 1.0) * RANGE_DB as f64
}

/// Input dB (−[`RANGE_DB`] at the left … 0 at the right) → x within a graph
/// of width `w`.
pub fn db_to_x(db: f64, w: f64) -> f64 {
    ((db + RANGE_DB as f64) / RANGE_DB as f64).clamp(0.0, 1.0) * w
}

/// Linear peak sample (0..1) → normalized display amplitude (0..1) through
/// dB, so the waveform is log-scaled like the painter's.
pub fn scale_input_peak(peak: f32) -> f32 {
    if peak <= 0.0 {
        0.0
    } else {
        let db = 20.0 * peak.log10();
        (1.0 + db / RANGE_DB).clamp(0.0, 1.0)
    }
}

/// Gain reduction in dB (positive = reducing) → normalized display fraction
/// of the graph height (0..1), painted downward from the top.
pub fn scale_gr_db(gr_db: f32) -> f32 {
    (gr_db / RANGE_DB).clamp(0.0, 1.0)
}

/// Map a slice of linear input peaks through [`scale_input_peak`].
pub fn scale_input_wave(peaks: &[f32]) -> Vec<f32> {
    peaks.iter().map(|&p| scale_input_peak(p)).collect()
}

/// Map a slice of GR dB values through [`scale_gr_db`].
pub fn scale_gr_wave(gr_db: &[f32]) -> Vec<f32> {
    gr_db.iter().map(|&g| scale_gr_db(g)).collect()
}

/// Smooth SVG path through normalized samples (Catmull-Rom → cubic Bézier,
/// the painter's `build_smooth_path`), optionally closed to `baseline` for a
/// fill. `from_bottom = true` draws amplitude up from the bottom edge (the
/// input waveform); `false` draws downward from the top (the GR trace).
pub fn smooth_path(samples: &[f32], w: f64, h: f64, from_bottom: bool, close: bool) -> String {
    let n = samples.len();
    if n == 0 {
        return String::new();
    }
    let step = w / n as f64;
    let ys: Vec<f64> = samples
        .iter()
        .map(|&s| {
            let amp = s.clamp(0.0, 1.0) as f64;
            if from_bottom {
                h - amp * h
            } else {
                amp * h
            }
        })
        .collect();
    let baseline = if from_bottom { h } else { 0.0 };
    let mut d = String::new();
    if close {
        d.push_str(&format!("M 0 {baseline:.1} L 0 {:.1} ", ys[0]));
    } else {
        d.push_str(&format!("M 0 {:.1} ", ys[0]));
    }
    for i in 0..n - 1 {
        let x0 = i as f64 * step;
        let x1 = (i + 1) as f64 * step;
        let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
        let (y_curr, y_next) = (ys[i], ys[i + 1]);
        let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };
        let t1 = (y_next - y_prev) / 2.0;
        let t2 = (y_next2 - y_curr) / 2.0;
        d.push_str(&format!(
            "C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1} ",
            x0 + step / 3.0,
            y_curr + t1 / 3.0,
            x1 - step / 3.0,
            y_next - t2 / 3.0,
            x1,
            y_next,
        ));
    }
    if close {
        d.push_str(&format!("L {w:.1} {baseline:.1} Z"));
    }
    d
}

/// SVG polyline path of the transfer curve across the full display range
/// (input −[`RANGE_DB`]..0 dB left→right), 61 sample points.
pub fn transfer_curve_path(threshold_db: f32, ratio: f32, knee_db: f32, w: f64, h: f64) -> String {
    let mut d = String::new();
    for i in 0..=60 {
        let input = -(RANGE_DB as f64) + (i as f64 / 60.0) * RANGE_DB as f64;
        let output = compress_transfer(input as f32, threshold_db, ratio, knee_db) as f64;
        let x = db_to_x(input, w);
        let y = db_to_y(output, h);
        d.push_str(if i == 0 { "M " } else { "L " });
        d.push_str(&format!("{x:.1} {y:.1} "));
    }
    d
}

/// Graph position of the live input "ball" riding the transfer curve.
pub fn transfer_ball(
    in_db: f32,
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    w: f64,
    h: f64,
) -> (f64, f64) {
    let level = in_db.clamp(-RANGE_DB, 0.0);
    let out = compress_transfer(level, threshold_db, ratio, knee_db) as f64;
    (db_to_x(level as f64, w), db_to_y(out, h))
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_identity_below_threshold() {
        // Hard knee: everything below the threshold passes through 1:1.
        for db in [-60.0f32, -40.0, -25.0, -20.01] {
            assert!(
                (compress_transfer(db, -20.0, 4.0, 0.0) - db).abs() < 1e-6,
                "below-threshold not identity at {db}"
            );
        }
        // Ratio 1:1 is identity everywhere, knee or not.
        for db in [-30.0f32, -10.0, 0.0] {
            assert_eq!(compress_transfer(db, -20.0, 1.0, 12.0), db);
        }
    }

    #[test]
    fn transfer_follows_ratio_slope_above_threshold() {
        // Hard knee, 4:1 @ −20 dB: 12 dB over the threshold → 3 dB out over.
        let out = compress_transfer(-8.0, -20.0, 4.0, 0.0);
        assert!((out - (-17.0)).abs() < 1e-5, "4:1 slope wrong: {out}");
        // 20:1 approaches limiting.
        let out = compress_transfer(0.0, -20.0, 20.0, 0.0);
        assert!((out - (-19.0)).abs() < 1e-5, "20:1 slope wrong: {out}");
    }

    #[test]
    fn knee_blends_smoothly_and_continuously() {
        let (thr, ratio, knee) = (-20.0f32, 4.0f32, 12.0f32);
        // Continuity at both knee edges against the outer branches.
        let lo = thr - knee / 2.0;
        let hi = thr + knee / 2.0;
        assert!((compress_transfer(lo, thr, ratio, knee) - lo).abs() < 1e-4);
        let hard_hi = hi - (1.0 - 1.0 / ratio) * (hi - thr);
        assert!((compress_transfer(hi, thr, ratio, knee) - hard_hi).abs() < 1e-3);
        // At the threshold itself the soft knee sits between identity and
        // the hard-knee output, exactly slope·knee/8 below the input.
        let at = compress_transfer(thr, thr, ratio, knee);
        let expected = thr - (1.0 - 1.0 / ratio) * knee / 8.0;
        assert!(
            (at - expected).abs() < 1e-4,
            "knee midpoint: {at} vs {expected}"
        );
        // Monotonic non-decreasing across the knee (no dents).
        let mut prev = f32::MIN;
        for i in 0..=100 {
            let x = lo - 2.0 + (i as f32 / 100.0) * (knee + 4.0);
            let y = compress_transfer(x, thr, ratio, knee);
            assert!(y >= prev - 1e-4, "transfer not monotonic at {x}");
            prev = y;
        }
    }

    #[test]
    fn db_y_mapping_round_trips() {
        for db in [0.0f64, -10.0, -20.0, -45.0, -60.0] {
            let y = db_to_y(db, 300.0);
            assert!((y_to_db(y, 300.0) - db).abs() < 1e-9, "round trip at {db}");
        }
        assert_eq!(db_to_y(0.0, 300.0), 0.0);
        assert_eq!(db_to_y(-60.0, 300.0), 300.0);
        assert_eq!(db_to_x(0.0, 360.0), 360.0);
        assert_eq!(db_to_x(-60.0, 360.0), 0.0);
    }

    #[test]
    fn input_and_gr_scaling() {
        assert_eq!(scale_input_peak(0.0), 0.0);
        assert!((scale_input_peak(1.0) - 1.0).abs() < 1e-6);
        // −30 dBFS sits mid-window on the 60 dB range.
        let mid = scale_input_peak(10f32.powf(-30.0 / 20.0));
        assert!((mid - 0.5).abs() < 1e-4, "−30 dBFS: {mid}");
        assert_eq!(scale_gr_db(0.0), 0.0);
        assert!((scale_gr_db(30.0) - 0.5).abs() < 1e-6);
        assert_eq!(scale_gr_db(120.0), 1.0);
        assert_eq!(scale_input_wave(&[0.0, 1.0]), vec![0.0, 1.0]);
        assert_eq!(scale_gr_wave(&[0.0]), vec![0.0]);
    }

    #[test]
    fn smooth_path_shapes() {
        assert_eq!(smooth_path(&[], 360.0, 300.0, true, true), "");
        // Closed fill starts at the baseline and closes with Z.
        let fill = smooth_path(&[0.2, 0.5, 0.3], 360.0, 300.0, true, true);
        assert!(fill.starts_with("M 0 300.0"), "fill start: {fill}");
        assert!(fill.ends_with('Z'), "fill not closed: {fill}");
        assert!(fill.contains('C'), "no Catmull-Rom segments: {fill}");
        // Open edge starts on the first sample and does not close.
        let edge = smooth_path(&[0.2, 0.5, 0.3], 360.0, 300.0, true, false);
        assert!(edge.starts_with("M 0 240.0"), "edge start: {edge}");
        assert!(!edge.contains('Z'));
        // Top-anchored (GR) fill baselines at y = 0.
        let gr = smooth_path(&[0.1, 0.2], 360.0, 300.0, false, true);
        assert!(gr.starts_with("M 0 0.0"), "gr baseline: {gr}");
    }

    #[test]
    fn transfer_curve_path_reacts_to_params() {
        let a = transfer_curve_path(-20.0, 4.0, 6.0, 360.0, 300.0);
        assert!(a.starts_with("M "), "path start: {a}");
        assert_eq!(a.matches("L ").count(), 60, "expected 61 points");
        let b = transfer_curve_path(-30.0, 4.0, 6.0, 360.0, 300.0);
        assert_ne!(a, b, "threshold change must move the curve");
        let c = transfer_curve_path(-20.0, 10.0, 6.0, 360.0, 300.0);
        assert_ne!(a, c, "ratio change must move the curve");
    }

    #[test]
    fn transfer_ball_rides_the_curve() {
        // Below threshold: on the identity diagonal.
        let (x, y) = transfer_ball(-40.0, -20.0, 4.0, 0.0, 360.0, 300.0);
        assert!((x - db_to_x(-40.0, 360.0)).abs() < 1e-9);
        assert!((y - db_to_y(-40.0, 300.0)).abs() < 1e-9);
        // Above threshold: pushed down by the ratio.
        let (_, y) = transfer_ball(-8.0, -20.0, 4.0, 0.0, 360.0, 300.0);
        assert!((y - db_to_y(-17.0, 300.0)).abs() < 1e-4);
    }
}
