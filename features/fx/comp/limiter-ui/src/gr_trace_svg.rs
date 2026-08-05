//! Geometry for the limiter's scrolling gain-reduction trace.
//!
//! Portable on purpose — no Dioxus, no nice_plug, no plugin framework — so the
//! shape math is unit-testable and can be reused by a wasm remote. The
//! component in [`crate::gr_trace`] does nothing but feed these functions and
//! emit SVG.
//!
//! The view is a time window: oldest sample on the left, newest on the right.
//! Two layers share it — the output waveform as a filled envelope around the
//! centre line, and the gain reduction hanging down from the top.

/// Trace height in CSS px. The component pins its container to this so pointer
/// y maps 1:1 onto the viewBox.
pub const TRACE_H: f64 = 180.0;

/// Trace width in viewBox units.
pub const TRACE_W: f64 = 600.0;

/// Gain reduction (dB) shown at the bottom of the trace. Beyond this the trace
/// clamps — a limiter pulling more than 24 dB is off the rails anyway.
pub const GR_RANGE_DB: f64 = 24.0;

/// Map a gain-reduction amount in dB to a y coordinate.
///
/// 0 dB (no reduction) sits at the top; [`GR_RANGE_DB`] at the bottom.
pub fn gr_to_y(gr_db: f64, height: f64) -> f64 {
    let t = (gr_db.max(0.0) / GR_RANGE_DB).min(1.0);
    t * height
}

/// Map a linear sample magnitude (0..=1) to a half-height offset from the
/// waveform's centre line.
pub fn amp_to_half_height(amp: f64, height: f64) -> f64 {
    amp.clamp(0.0, 1.0) * height * 0.5
}

/// Evenly space `n` samples across the trace width.
///
/// With a single sample the point sits at the left edge; the caller draws a
/// degenerate path, which is correct for a ring that has only been written
/// once.
pub fn sample_x(index: usize, n: usize, width: f64) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    index as f64 / (n - 1) as f64 * width
}

/// Build the `d` of the gain-reduction area: a filled region hanging from the
/// top edge down to the reduction at each point.
///
/// Returns an empty string for an empty window so the caller can skip the
/// path element entirely.
pub fn gr_area_path(gr_db: &[f32], width: f64, height: f64) -> String {
    if gr_db.is_empty() {
        return String::new();
    }
    let n = gr_db.len();
    let mut d = String::with_capacity(n * 16 + 32);
    d.push_str("M 0 0");
    for (i, &g) in gr_db.iter().enumerate() {
        let x = sample_x(i, n, width);
        let y = gr_to_y(g as f64, height);
        d.push_str(&format!(" L {x:.2} {y:.2}"));
    }
    // Close back along the top edge.
    d.push_str(&format!(" L {width:.2} 0 Z"));
    d
}

/// Build the `d` of the output waveform envelope: a symmetric filled shape
/// around the vertical centre of the trace.
pub fn waveform_path(peaks: &[f32], width: f64, height: f64) -> String {
    if peaks.is_empty() {
        return String::new();
    }
    let n = peaks.len();
    let mid = height * 0.5;
    let mut d = String::with_capacity(n * 32 + 32);
    // Upper edge, left → right.
    for (i, &p) in peaks.iter().enumerate() {
        let x = sample_x(i, n, width);
        let y = mid - amp_to_half_height(p as f64, height);
        d.push_str(if i == 0 { "M " } else { " L " });
        d.push_str(&format!("{x:.2} {y:.2}"));
    }
    // Lower edge, right → left.
    for (i, &p) in peaks.iter().enumerate().rev() {
        let x = sample_x(i, n, width);
        let y = mid + amp_to_half_height(p as f64, height);
        d.push_str(&format!(" L {x:.2} {y:.2}"));
    }
    d.push_str(" Z");
    d
}

/// Horizontal gridlines, as (y, label) pairs, every 6 dB of reduction.
pub fn gr_gridlines(height: f64) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    let mut db = 6.0;
    while db <= GR_RANGE_DB {
        out.push((gr_to_y(db, height), format!("-{db:.0}")));
        db += 6.0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reduction_sits_at_the_top_and_full_range_at_the_bottom() {
        assert_eq!(gr_to_y(0.0, TRACE_H), 0.0);
        assert_eq!(gr_to_y(GR_RANGE_DB, TRACE_H), TRACE_H);
        // Halfway through the range is halfway down.
        assert!((gr_to_y(GR_RANGE_DB / 2.0, TRACE_H) - TRACE_H / 2.0).abs() < 1e-9);
    }

    #[test]
    fn gain_reduction_clamps_rather_than_running_off_the_trace() {
        assert_eq!(gr_to_y(-5.0, TRACE_H), 0.0, "negative GR pins to the top");
        assert_eq!(
            gr_to_y(GR_RANGE_DB * 3.0, TRACE_H),
            TRACE_H,
            "excess GR pins to the bottom"
        );
    }

    #[test]
    fn samples_span_the_full_width() {
        assert_eq!(sample_x(0, 4, TRACE_W), 0.0);
        assert_eq!(sample_x(3, 4, TRACE_W), TRACE_W);
        assert_eq!(sample_x(0, 1, TRACE_W), 0.0, "single sample degenerates");
    }

    #[test]
    fn gr_area_is_closed_and_has_one_point_per_sample() {
        let d = gr_area_path(&[0.0, 3.0, 6.0], TRACE_W, TRACE_H);
        assert!(d.starts_with("M 0 0"), "area must start at the top-left: {d}");
        assert!(d.ends_with(" Z"), "area must be closed: {d}");
        // 3 samples + the closing corner along the top edge.
        assert_eq!(d.matches("L ").count(), 4, "{d}");
        assert!(gr_area_path(&[], TRACE_W, TRACE_H).is_empty());
    }

    #[test]
    fn waveform_is_symmetric_about_the_centre_line() {
        let d = waveform_path(&[1.0], TRACE_W, TRACE_H);
        // One sample: top edge then bottom edge, mirrored about mid.
        assert!(d.contains(&format!("0.00 {:.2}", TRACE_H * 0.5 - TRACE_H * 0.5)));
        assert!(d.contains(&format!("0.00 {:.2}", TRACE_H * 0.5 + TRACE_H * 0.5)));
        assert!(d.ends_with(" Z"));
        assert!(waveform_path(&[], TRACE_W, TRACE_H).is_empty());
    }

    #[test]
    fn gridlines_step_every_6_db_within_the_range() {
        let lines = gr_gridlines(TRACE_H);
        assert_eq!(lines.len(), (GR_RANGE_DB / 6.0) as usize);
        assert_eq!(lines[0].1, "-6");
        assert!(lines.iter().all(|(y, _)| *y > 0.0 && *y <= TRACE_H));
    }
}
