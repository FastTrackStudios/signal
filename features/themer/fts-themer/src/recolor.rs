//! Recoloring theme artwork.
//!
//! The theme's colored artwork is shaded, not flat — a fader cap has a
//! highlight, a body and a shadow. A naive "replace this RGB with that RGB"
//! flattens it, and a plain hue rotation drifts badly once the source and
//! target differ in saturation. So recoloring works in HSL and moves each
//! channel independently:
//!
//! - **hue** is *set* to the target's, not rotated, so every source accent
//!   lands on exactly the requested color;
//! - **saturation** scales by the target/source ratio, so a desaturated
//!   shadow stays proportionally desaturated;
//! - **lightness** shifts by the target/source delta, preserving the shading
//!   ramp instead of clamping it flat.
//!
//! Pixels below [`GREY_THRESHOLD`] saturation are left alone — that's the
//! neutral chrome around a colored element, which must not pick up a tint.
//! Alpha is never touched.
//!
//! Nor are WALTER's **marker pixels**. Pure magenta and pure yellow are not
//! artwork: they're the nine-slice guides REAPER reads to decide which bands
//! of an image may stretch. They are also maximally saturated, so a recolor
//! that only skips greys will happily turn them green and silently destroy the
//! image's geometry. See [`is_marker`].

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use std::path::Path;

use crate::color::Rgb;

/// Below this saturation a pixel counts as neutral and is passed through.
pub const GREY_THRESHOLD: f32 = 0.12;

/// WALTER's nine-slice guide colors: magenta marks the non-stretched corner
/// regions, yellow the outer extent.
const MARKERS: [Rgb; 2] = [Rgb::new(255, 0, 255), Rgb::new(255, 255, 0)];

/// Is this pixel a WALTER stretch marker rather than artwork?
///
/// Exact match only. REAPER itself tests for the exact values, and the
/// artwork legitimately contains near-magenta and near-yellow tones that
/// *should* recolor, so a tolerance here would do more harm than good.
pub fn is_marker(c: Rgb) -> bool {
    MARKERS.contains(&c)
}

/// A hue/saturation/lightness remap from one accent color to another.
#[derive(Clone, Copy, Debug)]
pub struct Retint {
    from: (f32, f32, f32),
    to: (f32, f32, f32),
    /// Only recolor pixels whose hue is within this many degrees of `from`.
    /// `None` recolors every non-grey pixel.
    pub hue_tolerance: Option<f32>,
    /// Saturation below which a pixel is left alone.
    pub grey_threshold: f32,
    /// Take only hue and saturation from the target, keeping the source's
    /// lightness. Use this to add a color to an existing set: a dark target
    /// would otherwise land visibly heavier than the variants beside it.
    pub keep_lightness: bool,
}

impl Retint {
    /// Remap `from` → `to`.
    pub fn new(from: Rgb, to: Rgb) -> Self {
        Self {
            from: from.hsl(),
            to: to.hsl(),
            hue_tolerance: None,
            grey_threshold: GREY_THRESHOLD,
            keep_lightness: false,
        }
    }

    /// Keep the source artwork's lightness, taking only hue and saturation
    /// from the target — see [`Retint::keep_lightness`].
    pub fn keeping_lightness(mut self) -> Self {
        self.keep_lightness = true;
        self
    }

    /// Only touch pixels within `degrees` of the source hue. Use this on
    /// images that carry more than one colored element.
    pub fn within(mut self, degrees: f32) -> Self {
        self.hue_tolerance = Some(degrees);
        self
    }

    /// Map one color.
    pub fn apply(&self, c: Rgb) -> Rgb {
        // Stretch guides are geometry, not art — recoloring them breaks the
        // image's nine-slice layout.
        if is_marker(c) {
            return c;
        }
        let (h, s, l) = c.hsl();
        if s < self.grey_threshold {
            return c;
        }
        if let Some(tol) = self.hue_tolerance
            && hue_distance(h, self.from.0) > tol
        {
            return c;
        }

        let (fh, fs, fl) = self.from;
        let (th, ts, tl) = self.to;

        // Saturation as a ratio so shading survives; guard the degenerate
        // case where the source accent is itself grey.
        let sat = if fs > 0.001 { s * (ts / fs) } else { ts };
        // Lightness as a delta, keeping the ramp's shape.
        let light = if self.keep_lightness {
            l
        } else {
            l + (tl - fl)
        };
        // Hue set outright, carrying any local deviation from the source hue
        // (highlights are often a few degrees off the body color).
        let hue = th + signed_hue_delta(h, fh);

        Rgb::from_hsl_parts(hue, sat, light)
    }

    /// Map every pixel of an image, preserving alpha.
    pub fn apply_image(&self, img: &RgbaImage) -> RgbaImage {
        let mut out = img.clone();
        for Rgba([r, g, b, a]) in out.pixels_mut() {
            if *a == 0 {
                continue;
            }
            let c = self.apply(Rgb::new(*r, *g, *b));
            (*r, *g, *b) = (c.r, c.g, c.b);
        }
        out
    }

    /// Read a PNG, recolor it, write it to `dst` (creating parent dirs).
    pub fn apply_file(&self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
        let (src, dst) = (src.as_ref(), dst.as_ref());
        let img = image::open(src)
            .with_context(|| format!("read {}", src.display()))?
            .to_rgba8();
        let out = self.apply_image(&img);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        out.save(dst)
            .with_context(|| format!("write {}", dst.display()))?;
        Ok(())
    }
}

/// Shortest angular distance between two hues, in degrees (0–180).
fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

/// Signed shortest offset from `reference` to `h`, in degrees (-180..180).
fn signed_hue_delta(h: f32, reference: f32) -> f32 {
    (h - reference + 540.0) % 360.0 - 180.0
}

/// The dominant saturated color of an image — the accent it's built around.
///
/// Used to infer a source color when the caller doesn't name one: takes the
/// most-saturated-and-opaque pixels and averages their hue, which is stabler
/// than a plain modal color on antialiased art.
pub fn dominant_accent(img: &RgbaImage) -> Option<Rgb> {
    let mut best: Option<(f32, Rgb)> = None;
    let mut hue_x = 0.0f32;
    let mut hue_y = 0.0f32;
    let mut sat_sum = 0.0f32;
    let mut count = 0u32;

    for Rgba([r, g, b, a]) in img.pixels() {
        if *a < 250 {
            continue;
        }
        let c = Rgb::new(*r, *g, *b);
        // Markers are fully saturated, so they'd otherwise win the
        // most-saturated-pixel vote and be read as the image's accent.
        if is_marker(c) {
            continue;
        }
        let (h, s, l) = c.hsl();
        if s < GREY_THRESHOLD || !(0.08..=0.97).contains(&l) {
            continue;
        }
        // Circular mean, weighted by saturation.
        let rad = h.to_radians();
        hue_x += rad.cos() * s;
        hue_y += rad.sin() * s;
        sat_sum += s;
        count += 1;
        if best.map_or(true, |(bs, _)| s > bs) {
            best = Some((s, c));
        }
    }

    if count == 0 {
        return None;
    }
    let hue = hue_y.atan2(hue_x).to_degrees().rem_euclid(360.0);
    // Lightness from the most saturated pixel — the accent's "true" tone.
    let (_, l) = best.map(|(_, c)| (c, c.hsl().2))?;
    Some(Rgb::from_hsl_parts(hue, sat_sum / count as f32, l))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(pixels: &[[u8; 4]]) -> RgbaImage {
        RgbaImage::from_fn(pixels.len() as u32, 1, |x, _| Rgba(pixels[x as usize]))
    }

    #[test]
    fn source_accent_lands_exactly_on_the_target() {
        let from = Rgb::new(0x46, 0xb9, 0xfe);
        let to = Rgb::new(0xb4, 0x99, 0xff);
        let out = Retint::new(from, to).apply(from);
        // Allow a rounding wobble from the HSL round trip.
        for (a, b) in [(out.r, to.r), (out.g, to.g), (out.b, to.b)] {
            assert!(a.abs_diff(b) <= 2, "{out:?} != {to:?}");
        }
    }

    #[test]
    fn greys_survive_untouched() {
        let rt = Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xff, 0x00, 0x00));
        for grey in [
            Rgb::new(0, 0, 0),
            Rgb::new(0x45, 0x45, 0x45),
            Rgb::new(255, 255, 255),
        ] {
            assert_eq!(rt.apply(grey), grey, "grey {grey:?} got tinted");
        }
    }

    #[test]
    fn shading_ramp_is_preserved_not_flattened() {
        let from = Rgb::new(0x46, 0xb9, 0xfe);
        let rt = Retint::new(from, Rgb::new(0xb4, 0x99, 0xff));
        // A darker and a lighter shade of the source.
        let (h, s, l) = from.hsl();
        let dark = Rgb::from_hsl_parts(h, s, l - 0.2);
        let light = Rgb::from_hsl_parts(h, s, l + 0.15);
        let (od, ol) = (rt.apply(dark), rt.apply(light));
        // The ordering and rough spacing of the ramp must survive.
        assert!(od.luminance() < ol.luminance(), "ramp inverted");
        assert!(od.hsl().2 < rt.apply(from).hsl().2);
        assert!(ol.hsl().2 > rt.apply(from).hsl().2);
    }

    #[test]
    fn alpha_is_never_modified() {
        let src = img(&[[0x46, 0xb9, 0xfe, 128], [0x46, 0xb9, 0xfe, 0]]);
        let out = Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xff, 0, 0)).apply_image(&src);
        assert_eq!(out.get_pixel(0, 0).0[3], 128);
        assert_eq!(out.get_pixel(1, 0).0[3], 0);
    }

    #[test]
    fn fully_transparent_pixels_keep_their_rgb() {
        // Recoloring invisible pixels wastes work and can fringe on scaling.
        let src = img(&[[0x46, 0xb9, 0xfe, 0]]);
        let out = Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xff, 0, 0)).apply_image(&src);
        assert_eq!(out.get_pixel(0, 0).0, [0x46, 0xb9, 0xfe, 0]);
    }

    #[test]
    fn hue_tolerance_spares_other_colored_elements() {
        let rt = Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xff, 0x00, 0x00)).within(30.0);
        let unrelated = Rgb::new(0x3c, 0xe0, 0x5a); // green, far from the blue source
        assert_eq!(rt.apply(unrelated), unrelated);
    }

    #[test]
    fn dominant_accent_ignores_grey_chrome() {
        let src = img(&[
            [0x45, 0x45, 0x45, 255],
            [0x46, 0xb9, 0xfe, 255],
            [0x2e, 0x2e, 0x2e, 255],
        ]);
        let found = dominant_accent(&src).expect("an accent");
        let (h, _, _) = found.hsl();
        let (want, _, _) = Rgb::new(0x46, 0xb9, 0xfe).hsl();
        assert!(hue_distance(h, want) < 6.0, "got hue {h}, want ~{want}");
    }

    #[test]
    fn walter_stretch_markers_survive_recoloring() {
        // Regression: these are nine-slice geometry, and recoloring them
        // turned every generated accent's magenta guides green — silently
        // breaking how the image stretches.
        let rt = Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xd1, 0x28, 0x3c));
        for marker in [Rgb::new(255, 0, 255), Rgb::new(255, 255, 0)] {
            assert_eq!(rt.apply(marker), marker, "marker {marker:?} was recolored");
        }
    }

    #[test]
    fn markers_survive_a_whole_image_pass() {
        let src = img(&[
            [255, 0, 255, 255],
            [0x46, 0xb9, 0xfe, 255],
            [255, 255, 0, 255],
        ]);
        let out =
            Retint::new(Rgb::new(0x46, 0xb9, 0xfe), Rgb::new(0xd1, 0x28, 0x3c)).apply_image(&src);
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 255, 255]);
        assert_eq!(out.get_pixel(2, 0).0, [255, 255, 0, 255]);
        assert_ne!(out.get_pixel(1, 0).0, [0x46, 0xb9, 0xfe, 255]);
    }

    #[test]
    fn dominant_accent_ignores_markers_not_just_greys() {
        // Magenta is the most saturated pixel here; the real accent is blue.
        let src = img(&[[255, 0, 255, 255], [0x46, 0xb9, 0xfe, 255]]);
        let found = dominant_accent(&src).expect("an accent");
        let (h, _, _) = found.hsl();
        let (want, _, _) = Rgb::new(0x46, 0xb9, 0xfe).hsl();
        assert!(
            hue_distance(h, want) < 6.0,
            "marker skewed detection: hue {h}"
        );
    }

    #[test]
    fn dominant_accent_is_none_for_pure_greyscale() {
        assert!(dominant_accent(&img(&[[0x45, 0x45, 0x45, 255]])).is_none());
    }
}
