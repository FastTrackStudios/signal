//! Portable trigger-waveform math + SVG path generation.
//!
//! The pure core of the trigger analysis display — ported from the legacy
//! FTS-Trigger `TriggerWaveform` (scrolling input-peak bars, dB scale
//! markers, draggable threshold line, trigger-event markers), remapped from
//! the legacy linear-amplitude y axis onto the dB-linear axis the comp graph
//! uses (0 dBFS at the top … −[`RANGE_DB`] at the bottom), so a threshold
//! drag lands exactly on the pointer's dB. No plugin framework, no Dioxus —
//! compiles for every target (same split as comp-ui's `comp_graph_svg`). The
//! `native`-gated [`crate::trigger_waveform`] component renders these paths
//! and adds the interactions.

/// Display dB range (0 dBFS at the top … −`RANGE_DB` at the bottom).
/// Matches the threshold param's −60..0 dB range, so pointer y ↔ threshold
/// dB is a straight linear map.
pub const RANGE_DB: f32 = 60.0;

/// dB scale grid: the legacy display's marker set.
pub const DB_MARKERS: &[(f32, &str)] = &[
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];

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

/// y of the threshold line for a threshold in dB.
pub fn threshold_line_y(threshold_db: f32, h: f64) -> f64 {
    db_to_y(threshold_db as f64, h)
}

/// Linear peak sample (0..1) → normalized display amplitude (0..1) through
/// dB, so the bar heights are log-scaled onto the same axis as the
/// threshold line (a bar exactly reaches the line when its peak equals the
/// threshold).
pub fn scale_peak(peak: f32) -> f32 {
    if peak <= 0.0 {
        0.0
    } else {
        let db = 20.0 * peak.log10();
        (1.0 + db / RANGE_DB).clamp(0.0, 1.0)
    }
}

/// Map a slice of linear input peaks through [`scale_peak`].
pub fn scale_peaks(peaks: &[f32]) -> Vec<f32> {
    peaks.iter().map(|&p| scale_peak(p)).collect()
}

/// One SVG path containing every peak-bar column: for each normalized sample
/// a rect subpath from the bottom edge up to its display amplitude. Columns
/// get a 1-px-ish gap (20 % of the column) like the legacy bar rendering;
/// near-silent columns are skipped so an idle display is an empty path.
pub fn bars_path(samples: &[f32], w: f64, h: f64) -> String {
    let n = samples.len();
    if n == 0 {
        return String::new();
    }
    let step = w / n as f64;
    let bar_w = (step * 0.8).max(0.5);
    let mut d = String::new();
    for (i, &s) in samples.iter().enumerate() {
        let amp = s.clamp(0.0, 1.0) as f64;
        if amp < 0.001 {
            continue;
        }
        let x = i as f64 * step;
        let y = h - amp * h;
        d.push_str(&format!(
            "M {x:.1} {h:.1} L {x:.1} {y:.1} L {:.1} {y:.1} L {:.1} {h:.1} Z ",
            x + bar_w,
            x + bar_w,
        ));
    }
    d
}

/// Place hit markers over the scrolling window.
///
/// `hits` are `(block_index, velocity)` pairs from the shared hit ring —
/// `block_index` is the monotonic count of processed blocks at the time of
/// the hit, `head` is the wave ring's current monotonic block count, and
/// `len` is the window length in columns. Hits still inside the window come
/// back as `(x_center, velocity)` with the newest block at the right edge
/// (column `len − 1`), exactly aligned with the wave ring's oldest→newest
/// snapshot; hits that scrolled out (or from the future — a torn read) are
/// dropped.
pub fn marker_columns(hits: &[(u64, f32)], head: u64, len: usize, w: f64) -> Vec<(f64, f32)> {
    if len == 0 {
        return Vec::new();
    }
    let step = w / len as f64;
    let mut out = Vec::new();
    for &(block, vel) in hits {
        if block >= head {
            continue; // future/torn — not in the window yet
        }
        let age = head - block; // 1 = newest pushed block
        if age > len as u64 {
            continue; // scrolled out
        }
        let idx = len as u64 - age; // 0 = oldest column, len-1 = newest
        out.push(((idx as f64 + 0.5) * step, vel.clamp(0.0, 1.0)));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_y_mapping_round_trips() {
        for db in [0.0f64, -6.0, -12.0, -30.0, -48.0, -60.0] {
            let y = db_to_y(db, 260.0);
            assert!((y_to_db(y, 260.0) - db).abs() < 1e-9, "round trip at {db}");
        }
        assert_eq!(db_to_y(0.0, 260.0), 0.0);
        assert_eq!(db_to_y(-60.0, 260.0), 260.0);
        // Out-of-range clamps.
        assert_eq!(db_to_y(6.0, 260.0), 0.0);
        assert_eq!(db_to_y(-90.0, 260.0), 260.0);
        assert_eq!(y_to_db(-10.0, 260.0), 0.0);
        assert_eq!(y_to_db(400.0, 260.0), -60.0);
    }

    #[test]
    fn threshold_line_tracks_db() {
        // −30 dB sits mid-window on the 60 dB range.
        assert!((threshold_line_y(-30.0, 260.0) - 130.0).abs() < 1e-9);
        assert_eq!(threshold_line_y(0.0, 260.0), 0.0);
        assert_eq!(threshold_line_y(-60.0, 260.0), 260.0);
    }

    #[test]
    fn peak_scaling_is_db_linear() {
        assert_eq!(scale_peak(0.0), 0.0);
        assert!((scale_peak(1.0) - 1.0).abs() < 1e-6);
        // −30 dBFS sits mid-scale on the 60 dB range.
        let mid = scale_peak(10f32.powf(-30.0 / 20.0));
        assert!((mid - 0.5).abs() < 1e-4, "−30 dBFS: {mid}");
        // Below the floor clamps to zero.
        assert_eq!(scale_peak(10f32.powf(-90.0 / 20.0)), 0.0);
        assert_eq!(scale_peaks(&[0.0, 1.0]), vec![0.0, 1.0]);
        // A bar whose peak equals the threshold dB reaches exactly the
        // threshold line: display amp 1 − y/h.
        let thr = -18.0f32;
        let amp = scale_peak(10f32.powf(thr / 20.0)) as f64;
        let bar_top = 260.0 - amp * 260.0;
        assert!((bar_top - threshold_line_y(thr, 260.0)).abs() < 1e-3);
    }

    #[test]
    fn bars_path_shapes() {
        assert_eq!(bars_path(&[], 360.0, 260.0), "");
        // Silence renders no bars at all.
        assert_eq!(bars_path(&[0.0; 8], 360.0, 260.0), "");
        // Two live columns → two closed rect subpaths anchored at the bottom.
        let d = bars_path(&[0.5, 0.0, 1.0], 360.0, 260.0);
        assert_eq!(d.matches("M ").count(), 2, "expected 2 bars: {d}");
        assert_eq!(d.matches('Z').count(), 2, "bars not closed: {d}");
        assert!(d.starts_with("M 0.0 260.0 L 0.0 130.0"), "first bar wrong: {d}");
        // Full-scale column reaches the top edge.
        assert!(d.contains("L 240.0 0.0"), "full-scale bar missing: {d}");
    }

    #[test]
    fn marker_columns_window_the_hits() {
        let w = 360.0;
        let len = 256usize;
        let head = 1000u64;
        let hits = [
            (999u64, 0.9f32), // newest block → rightmost column
            (1000 - 256, 0.5),          // oldest still-visible block → column 0
            (1000 - 257, 0.5),          // just scrolled out
            (1200, 1.0),                // future/torn
        ];
        let cols = marker_columns(&hits, head, len, w);
        assert_eq!(cols.len(), 2, "windowing wrong: {cols:?}");
        let step = w / len as f64;
        assert!((cols[0].0 - (255.5 * step)).abs() < 1e-9, "newest not at right edge");
        assert!((cols[0].1 - 0.9).abs() < 1e-6);
        assert!((cols[1].0 - (0.5 * step)).abs() < 1e-9, "oldest not at left edge");
        // Empty window / empty hits.
        assert!(marker_columns(&hits, head, 0, w).is_empty());
        assert!(marker_columns(&[], head, len, w).is_empty());
        // Velocity clamps.
        let cols = marker_columns(&[(999, 7.0)], head, len, w);
        assert_eq!(cols[0].1, 1.0);
    }
}
