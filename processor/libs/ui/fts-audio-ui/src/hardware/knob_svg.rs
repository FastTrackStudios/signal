//! Geometry for a hardware pointer knob — portable, no Dioxus, unit-testable.
//!
//! An outboard-gear knob is not the FTS knob with a different colour: it has a
//! numbered scale ring printed on the panel around it, a pointer or chicken-
//! head that points *at* those numbers, and a sweep that starts and ends at
//! marked stops rather than running all the way round. The angles and tick
//! positions live here so the faces only place them.
//!
//! Design space is a unit circle centred on the origin — callers scale to
//! whatever the panel needs — and 0° is straight up, growing clockwise, which
//! is how SVG `rotate()` reads.

/// Total sweep of a hardware knob, in degrees. 300° (from -150° to +150°) is
/// the usual pot travel and what LA-2A and 1176 knobs read as.
pub const KNOB_SWEEP_DEG: f64 = 300.0;

/// Angle in degrees for a normalized position; 0° is straight up.
pub fn knob_angle(normalized: f64) -> f64 {
    (normalized.clamp(0.0, 1.0) - 0.5) * KNOB_SWEEP_DEG
}

/// A point on the knob's scale ring at `radius` for a normalized position,
/// relative to the knob centre. Y grows downward, as in SVG.
pub fn ring_point(normalized: f64, radius: f64) -> (f64, f64) {
    let rad = knob_angle(normalized).to_radians();
    (radius * rad.sin(), -radius * rad.cos())
}

/// One mark on the printed scale ring.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleMark {
    /// Position along the sweep, 0..1.
    pub normalized: f64,
    /// What is silkscreened next to the mark, if anything.
    pub label: Option<String>,
    /// Long mark (a numbered one) rather than a short intermediate tick.
    pub major: bool,
}

/// Build an evenly spaced scale ring of `major_count` numbered marks with
/// `minor_per_major` unnumbered ticks between each pair.
///
/// `label` renders the number printed at each major mark from its normalized
/// position, so a face can print 0–10 (LA-2A GAIN), -48…+12 (1176 INPUT) or
/// nothing at all off the same geometry.
pub fn scale_ring(
    major_count: usize,
    minor_per_major: usize,
    label: impl Fn(f64) -> Option<String>,
) -> Vec<ScaleMark> {
    if major_count < 2 {
        return Vec::new();
    }
    let mut marks = Vec::new();
    let spans = major_count - 1;
    for i in 0..major_count {
        let n = i as f64 / spans as f64;
        marks.push(ScaleMark {
            normalized: n,
            label: label(n),
            major: true,
        });
        if i < spans {
            for k in 1..=minor_per_major {
                let step = (k as f64) / (minor_per_major + 1) as f64;
                marks.push(ScaleMark {
                    normalized: (i as f64 + step) / spans as f64,
                    label: None,
                    major: false,
                });
            }
        }
    }
    marks
}

/// The scale ring for a stepped control: one numbered mark per detent, spaced
/// evenly across the sweep, printed with the unit's own legends ("0.1", "All").
pub fn detent_ring(labels: &[&str]) -> Vec<ScaleMark> {
    if labels.len() < 2 {
        return Vec::new();
    }
    let spans = labels.len() - 1;
    labels
        .iter()
        .enumerate()
        .map(|(i, label)| ScaleMark {
            normalized: i as f64 / spans as f64,
            label: Some((*label).to_string()),
            major: true,
        })
        .collect()
}

/// A number printed on a scale ring that runs `from`..`to` across the sweep,
/// rounded to whole units — the usual silkscreen.
pub fn linear_scale_label(from: f64, to: f64) -> impl Fn(f64) -> Option<String> {
    move |n| Some(format!("{:.0}", from + (to - from) * n))
}

/// The pointer as an SVG polygon, in knob-local coordinates before rotation:
/// a tapered blade from the hub out to `length`, `half_width` wide at the hub.
pub fn pointer_polygon(length: f64, half_width: f64) -> String {
    format!(
        "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
        -half_width,
        0.0,
        half_width,
        0.0,
        half_width * 0.35,
        -length,
        -half_width * 0.35,
        -length,
    )
}

/// The arc the scale ring is printed along, as an SVG path at `radius` — the
/// faint band under the tick marks.
pub fn ring_arc_path(radius: f64) -> String {
    let (x0, y0) = ring_point(0.0, radius);
    let (x1, y1) = ring_point(1.0, radius);
    // The sweep is > 180°, so the arc needs the large-arc flag.
    format!("M {x0:.2} {y0:.2} A {radius:.2} {radius:.2} 0 1 1 {x1:.2} {y1:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_is_symmetric_about_straight_up() {
        assert_eq!(knob_angle(0.5), 0.0);
        assert_eq!(knob_angle(0.0), -KNOB_SWEEP_DEG / 2.0);
        assert_eq!(knob_angle(1.0), KNOB_SWEEP_DEG / 2.0);
    }

    #[test]
    fn the_stops_sit_below_the_centre_like_a_real_pot() {
        // A 300° sweep leaves a 60° gap at the bottom, so both stops are
        // below the knob centre and the minimum is on the left.
        let (lx, ly) = ring_point(0.0, 10.0);
        let (rx, ry) = ring_point(1.0, 10.0);
        assert!(lx < 0.0 && rx > 0.0);
        assert!(ly > 0.0 && ry > 0.0, "stops should hang below centre");
    }

    #[test]
    fn out_of_range_positions_pin_to_the_stops() {
        assert_eq!(knob_angle(-1.0), knob_angle(0.0));
        assert_eq!(knob_angle(2.0), knob_angle(1.0));
    }

    #[test]
    fn a_scale_ring_numbers_only_its_major_marks() {
        let marks = scale_ring(6, 1, linear_scale_label(0.0, 10.0));
        // 6 majors + 5 minors between them.
        assert_eq!(marks.len(), 11);
        assert_eq!(marks.iter().filter(|m| m.major).count(), 6);
        assert!(marks.iter().all(|m| m.major == m.label.is_some()));
        assert_eq!(marks[0].label.as_deref(), Some("0"));
        assert_eq!(marks.last().unwrap().label.as_deref(), Some("10"));
    }

    #[test]
    fn scale_marks_run_monotonically_across_the_sweep() {
        let marks = scale_ring(4, 3, |_| None);
        assert!(marks.windows(2).all(|w| w[0].normalized < w[1].normalized));
        assert_eq!(marks.first().unwrap().normalized, 0.0);
        assert_eq!(marks.last().unwrap().normalized, 1.0);
    }

    #[test]
    fn a_degenerate_ring_has_no_marks() {
        assert!(scale_ring(1, 4, |_| None).is_empty());
        assert!(detent_ring(&["only"]).is_empty());
    }

    #[test]
    fn a_detent_ring_prints_one_number_per_position() {
        let marks = detent_ring(&["4", "8", "12", "20", "All"]);
        assert_eq!(marks.len(), 5);
        assert!(marks.iter().all(|m| m.major && m.label.is_some()));
        assert_eq!(marks[0].normalized, 0.0);
        assert_eq!(marks[4].normalized, 1.0);
        assert_eq!(marks[4].label.as_deref(), Some("All"));
    }

    #[test]
    fn the_pointer_tapers_toward_its_tip() {
        let poly = pointer_polygon(20.0, 4.0);
        assert!(poly.starts_with("-4.00,0.00 4.00,0.00"));
        assert!(poly.contains("-20.00"), "tip should reach the length");
    }

    #[test]
    fn the_ring_arc_takes_the_long_way_round() {
        let d = ring_arc_path(18.0);
        assert!(d.starts_with("M "));
        // large-arc-flag = 1: the sweep is wider than a semicircle.
        assert!(d.contains(" 0 1 1 "));
    }
}
