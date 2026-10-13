//! The furniture a time-domain effect panel draws under everything else.
//!
//! A delay and a reverb are different pictures, but they are read the same
//! way: a lane of time running left to right, ruled by the tempo, in the
//! colour of whichever effect owns it. That common half lives here rather
//! than once per effect crate, so the two cannot drift about what a beat
//! line looks like — a grid that disagreed between two panels in the same
//! rack would read as one of them being wrong about the tempo.

use anyrender::{PaintScene, Scene};
use peniko::kurbo::{Affine, Line, Point, Stroke};
use peniko::Color;

/// A grid denser than this has stopped being a ruler and become a texture.
const MAX_BEAT_LINES: usize = 64;

/// The lane's colour and a highlight lifted out of it, dimmed when bypassed.
///
/// The highlight is the same hue raised toward white rather than a second
/// colour: a bloom in an unrelated hue reads as a different signal arriving,
/// which is exactly the wrong thing to say about a repeat of the one already
/// there.
///
/// Bypassed draws grey rather than nothing, because "off" and "not
/// configured" must not look the same.
#[must_use]
pub fn palette(on: bool, color: [u8; 3]) -> (Color, Color) {
    if !on {
        let grey = Color::from_rgba8(63, 63, 70, 255);
        return (grey, grey);
    }
    let [r, g, b] = color;
    let lift = |c: u8| -> u8 { f32::from(c).mul_add(0.45, 255.0 * 0.55) as u8 };
    (
        Color::from_rgba8(r, g, b, 255),
        Color::from_rgba8(lift(r), lift(g), lift(b), 255),
    )
}

/// `c` at `a` of its opacity.
#[must_use]
pub fn faded(c: Color, a: f32) -> Color {
    c.multiply_alpha(a.clamp(0.0, 1.0))
}

/// The beat grid: a line per beat across `window` seconds, the downbeat of
/// every bar brighter.
///
/// Drawn under everything else, because it is the ruler the rest is read
/// against — a tap sitting exactly on a line is the whole message, and a
/// quarter-note delay is one you can see is a quarter note without reading a
/// number.
pub fn beats(scene: &mut Scene, w: f64, h: f64, window: f64, beat: f64, lit: bool) {
    if beat <= 0.0 || window <= 0.0 {
        return;
    }
    let count = (window / beat).ceil() as usize;
    if count > MAX_BEAT_LINES {
        return;
    }
    for i in 1..=count {
        let t = beat * i as f64;
        if t > window {
            break;
        }
        let x = t / window * w;
        let bar = i % 4 == 0;
        // Weighted to be read against a LIT panel. The grid is the ruler the
        // taps are measured against — "this repeat is on the beat" is the
        // whole message — and at 0.13 a beat line was fainter than the tap
        // sitting on it, which loses the relationship the picture exists for.
        let alpha = if !lit {
            0.07
        } else if bar {
            0.46
        } else {
            0.22
        };
        scene.stroke(
            &Stroke::new(if bar { 1.5 } else { 1.0 }),
            Affine::IDENTITY,
            Color::from_rgba8(148, 163, 184, 255).multiply_alpha(alpha),
            None,
            &Line::new(Point::new(x, 0.0), Point::new(x, h)),
        );
    }
}

/// `#rrggbb` → components, falling back to a neutral grey.
///
/// A panel's colour constants stay strings because that is what the DOM half
/// of the same lane needs; this is where the painted half reads them, rather
/// than a second list that could drift out of step with the first.
#[must_use]
pub fn rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return [148, 163, 184];
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(148);
    [byte(0), byte(2), byte(4)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bypassed_lane_is_grey_but_still_drawn() {
        let (a, b) = palette(false, [59, 130, 246]);
        assert_eq!(a, b, "bypassed has no highlight to lift");
        assert_ne!(a, Color::TRANSPARENT, "bypassed is dim, not invisible");
    }

    #[test]
    fn the_highlight_is_the_same_hue_raised() {
        let (base, hi) = palette(true, [30, 100, 200]);
        let (b, h) = (base.to_rgba8(), hi.to_rgba8());
        assert!(h.r > b.r && h.g > b.g && h.b > b.b, "raised toward white");
        assert!(h.b > h.r, "and still the same hue");
    }

    /// A grid is only useful while it can be counted.
    #[test]
    fn an_uncountable_grid_is_not_drawn() {
        let mut scene = Scene::new();
        beats(&mut scene, 400.0, 60.0, 100.0, 0.01, true);
        let mut drawn = Scene::new();
        beats(&mut drawn, 400.0, 60.0, 4.0, 1.0, true);
        assert!(
            format!("{drawn:?}").len() > format!("{scene:?}").len(),
            "a countable grid draws lines and a 10,000-line one draws none"
        );
    }

    #[test]
    fn a_hex_colour_round_trips_and_a_broken_one_does_not_panic() {
        assert_eq!(rgb("#3b82f6"), [0x3b, 0x82, 0xf6]);
        assert_eq!(rgb("3b82f6"), [0x3b, 0x82, 0xf6]);
        assert_eq!(rgb("nope"), [148, 163, 184]);
    }
}
