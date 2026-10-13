//! Rasterising a component to a REAPER-ready PNG.
//!
//! `rsx!` → SVG string (dioxus-ssr) → pixmap (resvg) → PNG, then the source
//! image's marker pixels are stamped back.
//!
//! Nothing scales here. Each DPI variant is rendered at the size of *its
//! own* source file — REAPER's 150% art is not reliably 1.5× the 100% art,
//! and rendering the vector at the real target size is both more correct
//! and sharper than scaling either the vector or the raster.

use dioxus::prelude::*;
use image::RgbaImage;

use crate::components::ArtProps;
use crate::derive::DerivedSpec;

/// resvg options with system fonts loaded.
///
/// `Options::default()` ships an **empty font database**, and SVG `<text>`
/// against an empty database renders *nothing at all* — no error, no
/// fallback, no glyphs. Button labels simply vanished, which looks exactly
/// like a component that forgot to draw them.
fn options() -> resvg::usvg::Options<'static> {
    use std::sync::OnceLock;
    static FONTS: OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let db = FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        // Loading system fonts costs ~100ms and this runs per image across
        // a few thousand of them, so it happens once.
        db.load_system_fonts();
        std::sync::Arc::new(db)
    });
    let mut opts = resvg::usvg::Options::default();
    opts.fontdb = db.clone();
    opts
}

#[derive(Debug)]
pub enum RenderError {
    /// The component produced markup resvg could not parse.
    Svg(String),
    /// resvg refused the target size.
    Pixmap(u32, u32),
    /// The component drew at a size other than the one it was given, which
    /// would land wrongly in REAPER.
    WrongSize { want: (u32, u32), got: (u32, u32) },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Svg(e) => write!(f, "component did not produce valid SVG: {e}"),
            Self::Pixmap(w, h) => write!(f, "could not allocate a {w}x{h} pixmap"),
            Self::WrongSize { want, got } => write!(
                f,
                "component drew {}x{} but the source image is {}x{}",
                got.0, got.1, want.0, want.1
            ),
        }
    }
}

impl std::error::Error for RenderError {}

/// Rasterise SVG markup into a box of a given size.
///
/// The vector is *scaled* to the box rather than rendered at its own size
/// and resampled — which is the whole reason the art is vector now, and the
/// only way a 150% variant comes out sharper than an upscaled PNG.
pub fn rasterise(markup: &str, width: u32, height: u32) -> Result<RgbaImage, RenderError> {
    let tree = resvg::usvg::Tree::from_str(markup, &options())
        .map_err(|e| RenderError::Svg(format!("{e}")))?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or(RenderError::Pixmap(width, height))?;
    let (vw, vh) = (tree.size().width(), tree.size().height());
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / vw.max(1.0),
        height as f32 / vh.max(1.0),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(to_rgba(&pixmap))
}

/// Render a component at an exact pixel size.
pub fn render_sized(
    component: fn(ArtProps) -> Element,
    width: u32,
    height: u32,
) -> Result<RgbaImage, RenderError> {
    let markup = render_to_svg_sized(component, width, height);

    let opts = options();
    let tree = resvg::usvg::Tree::from_str(&markup, &opts)
        .map_err(|e| RenderError::Svg(format!("{e}")))?;

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or(RenderError::Pixmap(width, height))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    Ok(to_rgba(&pixmap))
}

/// Convert a rendered pixmap to straight-alpha RGBA.
///
/// tiny_skia stores pixels **premultiplied**, so reading `take()` directly
/// into an `RgbaImage` silently darkens every translucent pixel in
/// proportion to its transparency. On this theme — which is largely
/// translucent overlays — that is most of the artwork, and it shows up as
/// "the trace does not round-trip" rather than as anything alpha-shaped.
pub fn to_rgba(pixmap: &resvg::tiny_skia::Pixmap) -> RgbaImage {
    let mut out = RgbaImage::new(pixmap.width(), pixmap.height());
    for (px, dst) in pixmap.pixels().iter().zip(out.pixels_mut()) {
        let c = px.demultiply();
        *dst = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

/// Render a component into the geometry measured from its source image,
/// stamping the original marker pixels back.
pub fn render_for(
    component: fn(ArtProps) -> Element,
    spec: &DerivedSpec,
) -> Result<RgbaImage, RenderError> {
    let mut img = render_sized(component, spec.width, spec.height)?;
    if img.dimensions() != (spec.width, spec.height) {
        return Err(RenderError::WrongSize {
            want: (spec.width, spec.height),
            got: img.dimensions(),
        });
    }
    spec.stamp(&mut img);
    Ok(img)
}

/// Render any component with props to markup.
///
/// The primitives are SVG and the strip is HTML composing them, so this is
/// deliberately untyped about which — it is the same dioxus-ssr call either
/// way, and tests want both.
pub fn render_svg<P: dioxus::dioxus_core::Properties>(
    component: fn(P) -> Element,
    props: P,
) -> String {
    let mut dom = VirtualDom::new_with_props(component, props);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// Render a component to SVG markup at a size.
///
/// The same components render live in the browser; this is only how they
/// reach a DAW that cannot load SVG.
pub fn render_to_svg_sized(component: fn(ArtProps) -> Element, width: u32, height: u32) -> String {
    let mut dom = VirtualDom::new_with_props(component, ArtProps { width, height });
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::McpBg;

    #[test]
    fn renders_at_the_exact_size_asked_for() {
        for (w, h) in [(4, 4), (23, 55), (40, 40)] {
            let img = render_sized(McpBg, w, h).expect("render");
            assert_eq!(img.dimensions(), (w, h));
        }
    }

    #[test]
    fn markers_come_from_the_source_verbatim() {
        // mcp_bg's real layout: 4x4, markers at opposite corners. Copying
        // rather than reconstructing is what makes this correct without
        // having to model WALTER's semantics.
        let spec = DerivedSpec {
            art_x: 0,
            width: 4,
            height: 4,
            markers: vec![(0, 0, [255, 0, 255, 255]), (3, 3, [255, 0, 255, 255])],
            cells: vec![(0, 4)],
        };
        let img = render_for(McpBg, &spec).unwrap();
        assert_eq!(img.dimensions(), (4, 4));
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 255, 255]);
        assert_eq!(img.get_pixel(3, 3).0, [255, 0, 255, 255]);
        // And nothing else became a marker — the earlier bug drew guides
        // along every edge, which REAPER rendered as visible magenta.
        assert_ne!(img.get_pixel(0, 3).0, [255, 0, 255, 255]);
        assert_ne!(img.get_pixel(3, 0).0, [255, 0, 255, 255]);
    }

    #[test]
    fn an_unsliced_source_gets_no_markers_invented() {
        let spec = DerivedSpec {
            art_x: 0,
            width: 8,
            height: 8,
            markers: vec![],
            cells: vec![(0, 8)],
        };
        let img = render_for(McpBg, &spec).unwrap();
        assert!(
            img.pixels().all(|p| p.0 != [255, 0, 255, 255]),
            "invented markers on an unsliced image"
        );
    }

    #[test]
    fn svg_markup_carries_the_size_it_was_given() {
        let svg = render_to_svg_sized(McpBg, 23, 55);
        assert!(svg.contains("width=\"23\""), "{svg}");
        assert!(svg.contains("height=\"55\""), "{svg}");
        assert!(svg.contains("xmlns"), "resvg needs xmlns: {svg}");
    }
}
