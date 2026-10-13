//! A luminance → colour ramp for restyling neutral theme artwork.
//!
//! REAPER's chrome — toolbar backgrounds, mixer strips, button faces, the
//! TCP frame — is **PNG art, not palette keys**, so no amount of colour
//! work reaches it. That is why a fully-mapped palette still leaves the
//! mixer looking like the theme it was forked from.
//!
//! The art is almost entirely neutral grey at various lightnesses, which
//! makes it remappable: read each pixel's luminance, look it up on a ramp
//! built from the theme, and the whole chrome moves onto the theme's
//! surfaces while every bevel, gradient and antialiased edge survives
//! intact — because their *relative* lightness is preserved.
//!
//! Coloured pixels pass through untouched (see [`Ramp::saturation_guard`])
//! so LEDs, record buttons and fader caps keep their meaning. This is the
//! exact complement of `fts_themer::recolor::Retint`, which touches only
//! the saturated pixels.

use crate::color::Color;
use crate::palette::{Chrome, Theme};

/// A luminance ramp plus the rule for what it refuses to touch.
#[derive(Clone, Debug, PartialEq)]
pub struct Ramp {
    /// `(luminance, colour)` stops, ascending. Values between stops are
    /// interpolated.
    pub stops: Vec<(f32, Color)>,
    /// Saturation above which a pixel counts as a coloured element and is
    /// passed through unchanged.
    pub saturation_guard: f32,
}

impl Ramp {
    /// The ramp that maps dark-DAW chrome art onto this theme.
    ///
    /// Stops bunch at the dark end because that is where chrome art
    /// actually lives — a mixer strip spans roughly 0.01–0.15 luminance,
    /// and evenly-spaced stops would flatten all of it onto one value,
    /// erasing exactly the bevels this is trying to preserve.
    pub fn for_chrome(theme: &Theme) -> Self {
        let c = &theme.chrome;
        let mut colours = [
            c.surface_deep(),
            c.surface,
            c.surface_raised,
            c.border,
            c.text_faint,
            c.text_dim,
            c.text,
            c.selected,
        ];
        // Sorted by lightness, because the *palette* need not be.
        //
        // These names ascend in the FTS palette, so listing them in order
        // was enough. They do not ascend everywhere: ReaperTips' border is
        // `col_main_3dsh`, a shadow at #323232, which is darker than its
        // #424242 surfaces — and a ramp whose stops dip maps two different
        // inputs to the same output and inverts the shading between them.
        // Sorting keeps every authored colour in the ramp while making
        // monotonicity a property of the construction rather than a
        // coincidence of the names.
        colours.sort_by(|a, b| a.luminance().total_cmp(&b.luminance()));

        // Positions bunch at the dark end because that is where chrome art
        // actually lives — a mixer strip spans roughly 0.01–0.15 luminance,
        // and evenly-spaced stops would flatten all of it onto one value,
        // erasing exactly the bevels this is trying to preserve.
        let at = [0.000, 0.010, 0.035, 0.080, 0.200, 0.400, 0.700, 1.000];
        Self {
            stops: at.into_iter().zip(colours).collect(),
            saturation_guard: 0.18,
        }
    }

    /// Map one colour.
    pub fn apply(&self, c: Color) -> Color {
        if is_coloured(c, self.saturation_guard) {
            return c;
        }
        self.sample(c.luminance())
    }

    /// The ramp value at `l`, interpolating between stops and clamping
    /// outside them.
    pub fn sample(&self, l: f32) -> Color {
        match self.stops.as_slice() {
            [] => Color::rgb(0, 0, 0),
            [(_, only)] => *only,
            stops => {
                if l <= stops[0].0 {
                    return stops[0].1;
                }
                for pair in stops.windows(2) {
                    let ((l0, c0), (l1, c1)) = (pair[0], pair[1]);
                    if l <= l1 {
                        let span = l1 - l0;
                        let t = if span.abs() < f32::EPSILON {
                            0.0
                        } else {
                            (l - l0) / span
                        };
                        return c0.mix(c1, t);
                    }
                }
                stops[stops.len() - 1].1
            }
        }
    }
}

impl Chrome {
    /// The deepest surface step — a floor for [`Ramp`] without widening the
    /// authored struct.
    pub fn surface_deep(&self) -> Color {
        self.surface.shade(-0.35)
    }
}

/// Is this pixel a coloured element rather than neutral chrome?
///
/// Saturation alone is not enough. HSL saturation is a *ratio*, so it blows
/// up at the ends of the range: `#010203` — three units apart, visually
/// black — computes as 50% saturated, and a naive guard would refuse to
/// remap it. Chrome art is full of such near-black and near-white pixels,
/// so guarding on saturation alone silently skips the darkest chrome, which
/// is most of it.
///
/// So the guard only applies in the middle of the lightness range, where
/// saturation actually means something.
fn is_coloured(c: Color, guard: f32) -> bool {
    let l = lightness(c);
    if !(0.06..=0.94).contains(&l) {
        return false;
    }
    saturation(c) > guard
}

/// HSL lightness, 0–1.
fn lightness(c: Color) -> f32 {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    (r.max(g).max(b) + r.min(g).min(b)) / 2.0
}

/// HSL saturation of a colour, 0–1.
fn saturation(c: Color) -> f32 {
    let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return 0.0;
    }
    let l = (max + min) / 2.0;
    if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Ramp {
        Ramp::for_chrome(&Theme::default())
    }

    #[test]
    fn neutral_chrome_lands_on_theme_surfaces() {
        // A mid-dark grey from the source art must come back as one of the
        // theme's dark surfaces, not stay grey.
        let out = ramp().apply(Color::rgb(0x3e, 0x3e, 0x3e));
        assert!(out.luminance() < 0.12, "chrome grey not darkened: {out:?}");
    }

    #[test]
    fn coloured_elements_survive_untouched() {
        // LEDs, record buttons and fader caps carry meaning; flattening
        // them onto the surface ramp would erase it.
        for c in [
            Color::rgb(0xe1, 0x3a, 0x53),
            Color::rgb(0x46, 0xb9, 0xfe),
            Color::rgb(0x22, 0xc5, 0x5e),
        ] {
            assert_eq!(ramp().apply(c), c, "coloured pixel {c:?} was remapped");
        }
    }

    #[test]
    fn the_ramp_is_monotonic() {
        // Brighter art must stay brighter, or bevels invert and every
        // button reads as pressed.
        let r = ramp();
        let mut last = -1.0;
        for i in 0..=40 {
            let l = r.sample(i as f32 / 40.0).luminance();
            assert!(l >= last - 0.001, "ramp dips at {i}: {l} < {last}");
            last = l;
        }
    }

    #[test]
    fn relative_lightness_is_preserved_within_chrome() {
        // The whole reason to use a ramp rather than a flat replacement:
        // a bevel's highlight must stay lighter than its face.
        let r = ramp();
        let face = r.apply(Color::rgb(0x2a, 0x2a, 0x2a));
        let hi = r.apply(Color::rgb(0x4a, 0x4a, 0x4a));
        let sh = r.apply(Color::rgb(0x18, 0x18, 0x18));
        assert!(hi.luminance() > face.luminance());
        assert!(face.luminance() > sh.luminance());
    }

    #[test]
    fn endpoints_clamp_rather_than_wrap() {
        let r = ramp();
        assert_eq!(r.sample(-1.0), r.stops[0].1);
        assert_eq!(r.sample(2.0), r.stops[r.stops.len() - 1].1);
    }

    #[test]
    fn white_stays_bright() {
        // Icon glyphs are near-white; sending them dark would erase the
        // toolbar.
        let out = ramp().apply(Color::rgb(255, 255, 255));
        assert!(out.luminance() > 0.5, "white went dark: {out:?}");
    }

    #[test]
    fn a_degenerate_ramp_does_not_panic() {
        let empty = Ramp {
            stops: vec![],
            saturation_guard: 0.2,
        };
        let _ = empty.apply(Color::rgb(40, 40, 40));
        let single = Ramp {
            stops: vec![(0.5, Color::rgb(9, 9, 9))],
            saturation_guard: 0.2,
        };
        assert_eq!(single.apply(Color::rgb(40, 40, 40)), Color::rgb(9, 9, 9));
    }

    #[test]
    fn near_black_chrome_is_still_remapped() {
        // HSL saturation is a ratio and blows up at the ends: #010203 is
        // visually black but computes as 50% saturated. Guarding on
        // saturation alone would skip the darkest chrome — which is most of
        // it — and leave the mixer looking untouched.
        let r = ramp();
        for c in [
            Color::rgb(1, 2, 3),
            Color::rgb(8, 9, 11),
            Color::rgb(250, 252, 255),
        ] {
            assert_ne!(r.apply(c), c, "near-extreme chrome {c:?} was skipped");
        }
    }

    #[test]
    fn the_stops_themselves_ascend() {
        // Catch a bad stop table directly rather than inferring it from a
        // sampling dip — the first version had `border` darker than the
        // stop below it, which read as "the ramp is broken somewhere".
        let r = ramp();
        for pair in r.stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                a.0 < b.0,
                "stop luminances out of order: {} !< {}",
                a.0,
                b.0
            );
            assert!(
                a.1.luminance() <= b.1.luminance(),
                "stop colours out of order at {}: {:?} brighter than {:?}",
                b.0,
                a.1,
                b.1
            );
        }
    }
}
