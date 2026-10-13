//! The artwork itself.
//!
//! Each component draws one piece of chrome from the palette. The browser
//! renders these live as SVG; `render` rasterises them for REAPER.
//!
//! # Components draw into a size they are given
//!
//! Every component takes the pixel size it must fill. That is not a style
//! choice — REAPER's images have sizes WALTER expects (`mcp_bg` is 4×4),
//! and a component that picks its own produces art REAPER blits wrongly and
//! magenta it renders as visible pixels. The size comes from
//! [`crate::derive::DerivedSpec`], measured off the image being replaced.
//!
//! Which means a component cannot assume it has room for detail. `mcp_bg`
//! at 4×4 is a nine-slice source — REAPER stretches it — so the useful
//! thing to draw is a correct one-pixel border and fill, not a small
//! picture of a mixer strip.

use daw_theme::Theme;
use dioxus::prelude::*;

/// Props every art component takes: the box to fill, in source pixels.
#[derive(Props, Clone, PartialEq)]
pub struct ArtProps {
    pub width: u32,
    pub height: u32,
}
/// The mixer strip's background fill.
///
/// Measured from the original: 4×4, markers at opposite corners, a flat
/// opaque block filling everything inside a **1px transparent gutter**. No
/// radius, no stroke, no highlight — REAPER stretches this across the whole
/// strip, so any modelling drawn here would be smeared over all of it.
///
/// The gutter is not decoration: the marker row and column must stay clear,
/// and the art lives inside them.
#[component]
pub fn McpBg(props: ArtProps) -> Element {
    let t = Theme::default();
    let (w, h) = (props.width as f32, props.height as f32);
    // Inset by the marker gutter. At 4×4 this leaves exactly the 2×2 the
    // original fills.
    let g = 1.0;
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "{g}", y: "{g}",
                width: "{(w - g * 2.0).max(0.0)}",
                height: "{(h - g * 2.0).max(0.0)}",
                fill: "{t.chrome.surface_raised.css()}",
            }
        }
    }
}

/// The mixer fader's groove.
///
/// Measured from the original (23×55): a **1px translucent black hairline**
/// down the centre at alpha 215, with a 1px soft edge either side at alpha
/// 60. Everything else transparent.
///
/// Translucent black rather than a palette colour, deliberately — and this
/// is how most of the theme works. The art *darkens what the palette puts
/// behind it* rather than painting over it, so a groove drawn this way
/// stays correct on any strip colour. Filling it with `surface_sunken`
/// would look right on one background and wrong on every other.
#[component]
pub fn McpVolBg(props: ArtProps) -> Element {
    let (w, h) = (props.width as f32, props.height as f32);
    let cx = (w / 2.0).floor();
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "{(cx - 1.0).max(0.0)}", y: "0", width: "1", height: "{h}",
                fill: "#000000", fill_opacity: "0.235",
            }
            rect {
                x: "{cx + 1.0}", y: "0", width: "1", height: "{h}",
                fill: "#000000", fill_opacity: "0.235",
            }
            rect {
                x: "{cx}", y: "0", width: "1", height: "{h}",
                fill: "#000000", fill_opacity: "0.843",
            }
        }
    }
}

/// Everything this crate can draw, by REAPER image name.
///
/// A component is only used for a name the theme actually ships — the
/// generator looks each one up in the source art and skips what it can't
/// measure.
pub fn registry() -> Vec<(&'static str, fn(ArtProps) -> Element)> {
    vec![("mcp_bg", McpBg), ("mcp_volbg", McpVolBg)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_sized;

    #[test]
    fn every_component_draws_at_the_sizes_reaper_actually_uses() {
        // The sizes that broke the first attempt: tiny nine-slice sources.
        for (name, f) in registry() {
            for (w, h) in [(4, 4), (23, 55), (16, 60), (2, 2)] {
                let img =
                    render_sized(f, w, h).unwrap_or_else(|e| panic!("{name} at {w}x{h}: {e}"));
                assert_eq!(img.dimensions(), (w, h), "{name} at {w}x{h}");
            }
        }
    }

    #[test]
    fn every_component_draws_something_at_a_tiny_size() {
        // A 4×4 that renders empty is the failure mode that looks exactly
        // like "REAPER ignored the file".
        for (name, f) in registry() {
            let img = render_sized(f, 4, 4).unwrap();
            let opaque = img.pixels().filter(|p| p.0[3] > 0).count();
            assert!(opaque > 0, "{name} rendered empty at 4x4");
        }
    }

    #[test]
    fn components_draw_from_the_palette_not_literals() {
        let svg = crate::render::render_to_svg_sized(McpBg, 40, 40);
        assert!(
            svg.contains(&Theme::default().chrome.surface_raised.to_hex()),
            "McpBg did not draw with the palette surface: {svg}"
        );
    }

    #[test]
    fn a_degenerate_size_does_not_produce_invalid_svg() {
        // 1×1 makes radius and inset arithmetic go negative, which yields
        // SVG resvg rejects — and the error surfaces as "component did not
        // produce valid SVG", a long way from the cause.
        for (name, f) in registry() {
            assert!(render_sized(f, 1, 1).is_ok(), "{name} broke at 1x1");
        }
    }
}
