//! How a fixed-aspect faceplate fits into a resizable editor window.
//!
//! A hardware panel is a physical object: the rack ears, the VU cutout and the
//! knob spacing are one drawing, and reflowing them the way a web layout
//! reflows is exactly the thing that stops it looking like a unit. So the
//! panel keeps its design size and is drawn at a single scale factor instead —
//! which also sidesteps blitz's habit of collapsing what does not fit rather
//! than clipping it, since at any scale the panel fits by construction.
//!
//! Geometry only; the components multiply their pixel sizes by
//! [`fit_scale`].

/// Smallest scale a panel is drawn at. The panel tracks the window all the
/// way down — a resizable editor that stops shrinking and crops instead is
/// broken resizing, not legibility protection. The floor only guards the
/// degenerate zero-size window.
pub const MIN_SCALE: f64 = 0.1;
/// Largest scale. Past roughly twice the design size a faceplate is just a
/// blurry enlargement, and the extra window space is better left as margin.
pub const MAX_SCALE: f64 = 2.0;

/// The scale factor that fits a `design_w` x `design_h` panel inside a
/// `win_w` x `win_h` window without distorting it.
///
/// One factor for both axes — a panel stretched on one axis reads as a
/// rendering bug, not as a bigger unit.
pub fn fit_scale(win_w: f64, win_h: f64, design_w: f64, design_h: f64) -> f64 {
    if design_w <= 0.0 || design_h <= 0.0 {
        return 1.0;
    }
    let s = (win_w / design_w).min(win_h / design_h);
    if !s.is_finite() {
        return 1.0;
    }
    s.clamp(MIN_SCALE, MAX_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_at_the_design_size_draws_the_panel_unscaled() {
        assert_eq!(fit_scale(900.0, 500.0, 900.0, 500.0), 1.0);
    }

    #[test]
    fn the_tighter_axis_decides_the_scale() {
        // Twice as wide, same height — height is what constrains it.
        assert_eq!(fit_scale(1800.0, 500.0, 900.0, 500.0), 1.0);
        // Half the height — the panel shrinks to fit vertically, not to the
        // width it could have had.
        assert_eq!(fit_scale(1800.0, 375.0, 900.0, 500.0), 0.75);
    }

    #[test]
    fn scaling_is_uniform_so_the_panel_never_stretches() {
        // The same factor applies to both axes: an 800x300 window on a
        // 900x500 design fits by height (0.6), not 0.888 by width.
        let s = fit_scale(800.0, 300.0, 900.0, 500.0);
        assert!((s - 0.6).abs() < 1e-9);
    }

    #[test]
    fn extreme_windows_stay_within_the_readable_range() {
        assert_eq!(fit_scale(80.0, 40.0, 900.0, 500.0), MIN_SCALE);
        assert_eq!(fit_scale(9000.0, 9000.0, 900.0, 500.0), MAX_SCALE);
    }

    #[test]
    fn a_degenerate_design_size_does_not_produce_a_nonsense_scale() {
        assert_eq!(fit_scale(900.0, 500.0, 0.0, 500.0), 1.0);
        assert_eq!(fit_scale(900.0, 500.0, 900.0, -1.0), 1.0);
    }
}
