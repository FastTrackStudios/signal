//! Measuring how closely a component reproduces the art it replaces.
//!
//! "Match the original exactly" is not reviewable by eye across hundreds of
//! images — at the sizes REAPER uses (`mcp_bg` is 4×4) the differences that
//! matter are invisible until they are composited into a mixer. So fidelity
//! is a number, and the number is checkable in a test.
//!
//! # What is compared, and what isn't
//!
//! - **Marker pixels are excluded.** They are geometry, copied verbatim, so
//!   scoring them would only inflate the result.
//! - **Colour is compared in *relative* terms.** The goal is not to
//!   reproduce Reapertips' greys — the whole point is to redraw them in our
//!   palette — so an absolute pixel diff would score a correct result as
//!   completely wrong. [`shape_score`] compares the *structure*: which
//!   pixels are opaque, and how each pixel's lightness relates to the
//!   image's own range.
//!
//! That distinction is the crux. A component that gets the silhouette and
//! the internal light/dark relationships right is a faithful replacement in
//! a different colourway; one that matches absolute RGB is just a copy.

use image::RgbaImage;

use crate::derive::MARKERS;

/// How closely two images agree, 0–1.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fidelity {
    /// Agreement on which pixels are drawn at all, 0–1. This is the
    /// silhouette: rounded corners, insets, where a groove sits.
    pub shape: f32,
    /// Agreement on internal light/dark structure, 0–1. This is the
    /// modelling: bevels, ribbing, gradients.
    pub structure: f32,
    /// Pixels compared (excludes markers).
    pub compared: usize,
}

impl Fidelity {
    /// A single number for ranking, weighted toward silhouette — a shape
    /// that is wrong cannot be rescued by good shading.
    pub fn score(self) -> f32 {
        self.shape * 0.6 + self.structure * 0.4
    }
}

/// Is this a WALTER guide rather than art?
fn is_marker(px: [u8; 4]) -> bool {
    MARKERS.contains(&px)
}

/// Perceptual lightness, 0–1.
fn lightness(px: [u8; 4]) -> f32 {
    (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0
}

/// Compare a rendered image against the original it replaces.
///
/// Returns `None` when the two differ in size — that is a spec error, not a
/// fidelity result, and scoring it would hide the real problem.
pub fn compare(rendered: &RgbaImage, original: &RgbaImage) -> Option<Fidelity> {
    if rendered.dimensions() != original.dimensions() {
        return None;
    }

    let mut compared = 0usize;
    let mut shape_hits = 0usize;

    // Lightness is compared *within each image's own range*, so a dark
    // redraw of a light original still scores well when the internal
    // relationships hold. Collect the ranges first.
    let mut pairs: Vec<(f32, f32)> = Vec::new();
    let (mut rmin, mut rmax) = (f32::MAX, f32::MIN);
    let (mut omin, mut omax) = (f32::MAX, f32::MIN);

    for (r, o) in rendered.pixels().zip(original.pixels()) {
        if is_marker(o.0) {
            continue;
        }
        compared += 1;

        // Silhouette: both drawn, or both not.
        let (ra, oa) = (r.0[3] > 8, o.0[3] > 8);
        if ra == oa {
            shape_hits += 1;
        }
        if ra && oa {
            let (rl, ol) = (lightness(r.0), lightness(o.0));
            rmin = rmin.min(rl);
            rmax = rmax.max(rl);
            omin = omin.min(ol);
            omax = omax.max(ol);
            pairs.push((rl, ol));
        }
    }

    if compared == 0 {
        return Some(Fidelity {
            shape: 1.0,
            structure: 1.0,
            compared: 0,
        });
    }

    let norm = |v: f32, lo: f32, hi: f32| {
        let span = hi - lo;
        if span.abs() < 1e-4 {
            0.5
        } else {
            (v - lo) / span
        }
    };
    let structure = if pairs.is_empty() {
        1.0
    } else {
        let err: f32 = pairs
            .iter()
            .map(|&(r, o)| (norm(r, rmin, rmax) - norm(o, omin, omax)).abs())
            .sum::<f32>()
            / pairs.len() as f32;
        (1.0 - err).clamp(0.0, 1.0)
    };

    Some(Fidelity {
        shape: shape_hits as f32 / compared as f32,
        structure,
        compared,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn img(px: &[[u8; 4]]) -> RgbaImage {
        RgbaImage::from_fn(px.len() as u32, 1, |x, _| Rgba(px[x as usize]))
    }

    #[test]
    fn identical_images_score_perfectly() {
        let a = img(&[[10, 10, 10, 255], [200, 200, 200, 255]]);
        let f = compare(&a, &a).unwrap();
        assert_eq!(f.shape, 1.0);
        assert!(f.structure > 0.99, "{f:?}");
    }

    #[test]
    fn a_recolour_that_keeps_the_structure_still_scores_well() {
        // The whole point: we are redrawing Reapertips' greys in our own
        // palette. A dark redraw of a light original is a *success*, and an
        // absolute pixel diff would call it a total failure.
        let original = img(&[[60, 60, 60, 255], [200, 200, 200, 255]]);
        let ours = img(&[[8, 8, 11, 255], [120, 126, 140, 255]]);
        let f = compare(&ours, &original).unwrap();
        assert_eq!(f.shape, 1.0);
        assert!(f.structure > 0.95, "recolour penalised: {f:?}");
    }

    #[test]
    fn inverted_shading_is_caught() {
        // Light where the original is dark: the bevel reads inverted, which
        // is exactly the mistake relative comparison must still catch.
        let original = img(&[[20, 20, 20, 255], [200, 200, 200, 255]]);
        let ours = img(&[[200, 200, 200, 255], [20, 20, 20, 255]]);
        let f = compare(&ours, &original).unwrap();
        assert!(f.structure < 0.2, "inversion not caught: {f:?}");
    }

    #[test]
    fn a_wrong_silhouette_is_caught() {
        let original = img(&[[0, 0, 0, 0], [80, 80, 80, 255]]);
        let ours = img(&[[80, 80, 80, 255], [0, 0, 0, 0]]);
        let f = compare(&ours, &original).unwrap();
        assert_eq!(f.shape, 0.0, "{f:?}");
    }

    #[test]
    fn markers_are_not_scored() {
        // They are copied verbatim, so counting them would inflate every
        // score by however much of the image is guides.
        let original = img(&[[255, 0, 255, 255], [80, 80, 80, 255]]);
        let ours = img(&[[255, 0, 255, 255], [10, 10, 10, 255]]);
        let f = compare(&ours, &original).unwrap();
        assert_eq!(f.compared, 1, "marker was scored");
    }

    #[test]
    fn a_size_mismatch_is_not_a_fidelity_result() {
        // It is a spec error; returning a low score would hide it behind a
        // number that looks like "needs more work on the drawing".
        let a = img(&[[0, 0, 0, 255]]);
        let b = img(&[[0, 0, 0, 255], [0, 0, 0, 255]]);
        assert!(compare(&a, &b).is_none());
    }

    #[test]
    fn a_flat_image_does_not_divide_by_zero() {
        let flat = img(&[[40, 40, 40, 255], [40, 40, 40, 255]]);
        let f = compare(&flat, &flat).unwrap();
        assert!(f.structure.is_finite(), "{f:?}");
    }

    #[test]
    fn score_weights_silhouette_over_shading() {
        let good_shape = Fidelity {
            shape: 1.0,
            structure: 0.0,
            compared: 10,
        };
        let good_shading = Fidelity {
            shape: 0.0,
            structure: 1.0,
            compared: 10,
        };
        assert!(good_shape.score() > good_shading.score());
    }
}
