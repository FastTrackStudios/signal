//! The wheel, as the shared bindings see it — plus the one piece that
//! needs a dioxus type: turning a `WheelDelta` into notches.
//!
//! Everything else lives in `expression_editor_paint::scroll` and is
//! re-exported here so the components keep their paths.

pub use expression_editor_paint::scroll::*;

/// A wheel delta in notches, whatever unit it arrived in.
///
/// Normalising here rather than at each use means the gain constants
/// stay tuned for one unit, and a device that reports a new one is a
/// change in this function alone.
pub fn notches(delta: &dioxus::html::geometry::WheelDelta) -> (f64, f64) {
    use dioxus::html::geometry::WheelDelta;
    match delta {
        WheelDelta::Lines(v) => (v.x, v.y),
        WheelDelta::Pixels(v) => (v.x / PIXELS_PER_NOTCH, v.y / PIXELS_PER_NOTCH),
        // A page is a screenful; treat it as a firm push rather than
        // scaling it into the thousands.
        WheelDelta::Pages(v) => (v.x * 4.0, v.y * 4.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A touchpad and a mouse must ask for comparable amounts.
    ///
    /// The bug this pins was not a difference in feel. Every gain
    /// downstream is tuned for a mouse's line deltas — zoom is
    /// `e^(travel / 3)` — and a touchpad's pixel deltas were handed over
    /// unconverted, so a forty-pixel two-finger gesture asked for a
    /// six-hundred-thousand-fold zoom.
    #[test]
    fn a_touchpads_pixels_and_a_mouses_lines_are_comparable() {
        use dioxus::html::geometry::{LinesVector, PixelsVector3D, WheelDelta};

        // One notch of a mouse wheel.
        let mouse = notches(&WheelDelta::Lines(LinesVector::new(0.0, 1.0, 0.0)));
        // A touchpad flick of about one notch's worth of travel.
        let pad = notches(&WheelDelta::Pixels(PixelsVector3D::new(
            0.0,
            PIXELS_PER_NOTCH,
            0.0,
        )));
        assert!(
            (mouse.1 - pad.1).abs() < 1e-9,
            "a notch and {PIXELS_PER_NOTCH}px should agree: {mouse:?} vs {pad:?}"
        );

        // And the thing that made it unusable: a raw pixel delta must
        // never reach the gain constants at full size.
        let big = notches(&WheelDelta::Pixels(PixelsVector3D::new(0.0, 40.0, 0.0)));
        assert!(
            big.1 < 4.0,
            "40px of travel became {} notches — the zoom would be e^{}",
            big.1,
            big.1 / 3.0
        );
    }
}
