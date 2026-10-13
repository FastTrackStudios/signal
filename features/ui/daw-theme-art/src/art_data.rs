//! Traced artwork: the originals' exact geometry, drawn in our palette.
//!
//! Hand-writing a component per image cannot reach pixel-exact, and there
//! are a thousand of them. Since the goal *is* exact reproduction, the
//! geometry is traced from the source art instead — every run of identical
//! pixels becomes a rect — and only the **colour** is reinterpreted.
//!
//! That split is what makes this both 1:1 and ours:
//!
//! - **Structure comes from the original.** The ribbed fader cap, the
//!   bevelled glyph, the three sprite cells — all reproduced exactly,
//!   because they are traced rather than redrawn.
//! - **Colour comes from the palette**, via [`daw_theme::Ramp`]: each
//!   traced colour's *lightness* is looked up on a ramp built from the
//!   theme. Relative light/dark survives, absolute greys do not.
//!
//! And because the result is SVG in a `viewBox`, the same data renders
//! crisply at any size on the web — which the PNGs never could.

use daw_theme::{Ramp, Theme};
use dioxus::prelude::*;

/// One traced run of identical pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// The colour in the *original*, RGBA. Reinterpreted at render time.
    pub rgba: [u8; 4],
}

/// One traced image: a window onto the packed rect blob.
///
/// The rects are **not** Rust consts. Emitting 338k `Rect { .. }` literals
/// produced 21 MB of source — which compiles, slowly, and bloats every
/// build touching this crate for no benefit. They are packed into a binary
/// blob instead (12 bytes per rect) and decoded on demand; a few hundred
/// rects per image is nothing at render time, and the source stays a small
/// index.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ArtData {
    /// REAPER's image name.
    pub name: &'static str,
    pub width: u16,
    pub height: u16,
    /// Byte offset into the blob.
    pub offset: u32,
    /// How many rects.
    pub count: u32,
    /// Equal-width sprite cells this image packs — REAPER puts a control's
    /// normal/hover/pressed states side by side in one file. 1 means the
    /// image is a single drawing.
    pub cells: u32,
    /// The blob these rects live in.
    pub blob: &'static [u8],
}

/// Bytes per packed rect: x, y, w, h as u16 LE, then rgba.
pub const PACKED_RECT: usize = 12;

impl ArtData {
    /// Decode this image's rects.
    pub fn rects(&self) -> Vec<Rect> {
        let start = self.offset as usize;
        (0..self.count as usize)
            .filter_map(|i| {
                let b = self
                    .blob
                    .get(start + i * PACKED_RECT..start + (i + 1) * PACKED_RECT)?;
                let u16le = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
                Some(Rect {
                    x: u16le(0),
                    y: u16le(2),
                    w: u16le(4),
                    h: u16le(6),
                    rgba: [b[8], b[9], b[10], b[11]],
                })
            })
            .collect()
    }
}

/// Props every control component takes: the box to draw into.
#[derive(Props, Clone, PartialEq)]
pub struct ArtProps {
    pub width: u32,
    pub height: u32,
}

/// How a traced colour is reinterpreted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ColorMode {
    /// Push it through the theme ramp — the default, and what makes a
    /// traced image *ours* rather than a copy.
    #[default]
    Themed,
    /// Leave it exactly as traced. For verifying the tracer itself: a
    /// verbatim render must be pixel-identical to the source, which is the
    /// only way to know a fidelity score is measuring the colour mapping
    /// and not a tracing bug.
    Verbatim,
}

#[derive(Props, Clone, PartialEq)]
pub struct ArtImageProps {
    pub art: ArtData,
    /// Render size. Defaults to the traced size; anything else scales the
    /// vector, which is why this exists at all.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub mode: ColorMode,
    /// Which sprite cell to show. Out of range clamps to the last rather
    /// than rendering empty — a control that silently disappears is worse
    /// than one showing the wrong state.
    #[props(default)]
    pub cell: usize,
}

/// Draw a traced image.
#[component]
pub fn ArtImage(props: ArtImageProps) -> Element {
    let art = props.art;
    let ramp = Ramp::for_chrome(&Theme::default());

    // A sprite strip shows one cell: shift the viewBox rather than filter
    // rects, so a shape straddling a cell edge still clips correctly.
    let cells = art.cells.max(1);
    let cell_w = (art.width as u32 / cells).max(1);
    let cell = (props.cell as u32).min(cells - 1);
    let vx = cell * cell_w;

    let w = props.width.unwrap_or(cell_w);
    let h = props.height.unwrap_or(art.height as u32);

    rsx! {
        svg {
            width: "{w}", height: "{h}",
            view_box: "{vx} 0 {cell_w} {art.height}",
            // Theme art is pixel-aligned by nature; smoothing it on scale
            // turns a 1px bevel into a blur.
            shape_rendering: "crispEdges",
            xmlns: "http://www.w3.org/2000/svg",
            for (i, r) in art.rects().iter().enumerate() {
                {
                    let c = daw_theme::Color::rgba(r.rgba[0], r.rgba[1], r.rgba[2], r.rgba[3]);
                    let mapped = match props.mode {
                        ColorMode::Verbatim => c,
                        // Alpha is carried through untouched: the theme's
                        // overlays *are* their alpha, and remapping it
                        // would flatten every translucent shadow.
                        ColorMode::Themed => ramp.apply(c).with_alpha(c.a),
                    };
                    rsx! {
                        rect {
                            key: "{i}",
                            x: "{r.x}", y: "{r.y}",
                            width: "{r.w}", height: "{r.h}",
                            // Per-rect, not just on the root: resvg does
                            // not inherit shape-rendering, and antialiased
                            // rect edges stop the trace being pixel-exact.
                            shape_rendering: "crispEdges",
                            fill: "{mapped.css()}",
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_svg;

    /// Two packed rects: (0,0,2x1) dark grey, (2,0,1x1) light grey.
    const BLOB: &[u8] = &[
        0, 0, 0, 0, 2, 0, 1, 0, 40, 40, 40, 255, //
        2, 0, 0, 0, 1, 0, 1, 0, 200, 200, 200, 255,
    ];
    const ART: ArtData = ArtData {
        name: "test",
        width: 3,
        height: 1,
        offset: 0,
        count: 2,
        cells: 1,
        blob: BLOB,
    };

    #[test]
    fn decodes_packed_rects() {
        let rects = ART.rects();
        assert_eq!(rects.len(), 2);
        assert_eq!(
            rects[0],
            Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
                rgba: [40, 40, 40, 255]
            }
        );
        assert_eq!(
            rects[1],
            Rect {
                x: 2,
                y: 0,
                w: 1,
                h: 1,
                rgba: [200, 200, 200, 255]
            }
        );
    }

    #[test]
    fn a_truncated_blob_yields_fewer_rects_rather_than_panicking() {
        // A generated index and a generated blob can disagree if codegen is
        // interrupted; losing a rect beats taking the process down.
        const SHORT: ArtData = ArtData {
            name: "s",
            width: 3,
            height: 1,
            offset: 0,
            count: 9,
            cells: 1,
            blob: BLOB,
        };
        assert_eq!(SHORT.rects().len(), 2);
    }

    #[test]
    fn draws_one_rect_per_traced_run() {
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: ART,
                width: None,
                height: None,
                mode: ColorMode::Themed,
                cell: 0,
            },
        );
        assert_eq!(svg.matches("<rect").count(), 2, "{svg}");
    }

    #[test]
    fn verbatim_keeps_the_original_colours() {
        // The tracer's own check: without this there is no way to tell a
        // bad trace from a bad colour mapping.
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: ART,
                width: None,
                height: None,
                mode: ColorMode::Verbatim,
                cell: 0,
            },
        );
        assert!(svg.contains("#282828"), "{svg}");
        assert!(svg.contains("#c8c8c8"), "{svg}");
    }

    #[test]
    fn themed_moves_the_colours_onto_the_palette() {
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: ART,
                width: None,
                height: None,
                mode: ColorMode::Themed,
                cell: 0,
            },
        );
        assert!(!svg.contains("#282828"), "colour was not themed: {svg}");
    }

    #[test]
    fn translucency_survives_theming() {
        // The theme's technique is translucent overlays; remapping alpha
        // would flatten every shadow in the theme into an opaque block.
        const SOFT: &[u8] = &[0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 60];
        const A: ArtData = ArtData {
            name: "a",
            width: 1,
            height: 1,
            offset: 0,
            count: 1,
            cells: 1,
            blob: SOFT,
        };
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: A,
                width: None,
                height: None,
                mode: ColorMode::Themed,
                cell: 0,
            },
        );
        assert!(svg.contains("rgba("), "alpha lost: {svg}");
    }

    /// A 3-cell strip over the same two-rect blob.
    const STRIP: ArtData = ArtData {
        name: "strip",
        width: 3,
        height: 1,
        offset: 0,
        count: 2,
        cells: 3,
        blob: BLOB,
    };

    #[test]
    fn a_strip_shows_one_cell() {
        // The bug the comparison sheet exposed: drawing a 3-cell strip drew
        // all three buttons and spilled over its neighbours.
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: STRIP,
                width: None,
                height: None,
                mode: ColorMode::Themed,
                cell: 1,
            },
        );
        assert!(svg.contains(r#"viewBox="1 0 1 1""#), "{svg}");
        // And the natural width is one cell, not the whole strip.
        assert!(svg.contains(r#"width="1""#), "{svg}");
    }

    #[test]
    fn an_out_of_range_cell_clamps() {
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: STRIP,
                width: None,
                height: None,
                mode: ColorMode::Themed,
                cell: 99,
            },
        );
        assert!(svg.contains(r#"viewBox="2 0 1 1""#), "{svg}");
    }

    #[test]
    fn the_viewbox_is_the_traced_size_so_it_scales() {
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: ART,
                width: Some(30),
                height: Some(10),
                mode: ColorMode::Themed,
                cell: 0,
            },
        );
        assert!(svg.contains("viewBox=\"0 0 3 1\""), "{svg}");
        assert!(svg.contains("width=\"30\""), "{svg}");
    }

    #[test]
    fn pixel_art_is_not_smoothed_when_scaled() {
        // A 1px bevel blurred into nothing is the classic way traced art
        // stops looking like the original.
        let svg = render_svg(
            ArtImage,
            ArtImageProps {
                art: ART,
                width: Some(30),
                height: Some(10),
                mode: ColorMode::Themed,
                cell: 0,
            },
        );
        assert!(svg.contains("crispEdges"), "{svg}");
    }
}
