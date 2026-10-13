//! What the SVG exporter reads from the scene beyond the paint primitives:
//! the exported node, the source tool's absolute bounds, raw stroke stacks.

#[cfg(feature = "svg_export")]
use crate::document::proto::{Node, PaintKind, Scene};

/// The exported node: the first page's top-level node whose source id is
/// `root_source_id`, else its first top-level node.
#[cfg(feature = "svg_export")]
pub(super) fn export_root<'a>(scene: &'a Scene, root_source_id: Option<&str>) -> Option<&'a Node> {
    let page = scene.node(*scene.pages().first()?)?;
    let mut tops = page.children.iter().filter_map(|id| scene.node(*id));
    let first = tops.clone().next()?;
    Some(
        root_source_id
            .and_then(|id| tops.find(|n| source_id(n) == Some(id)))
            .unwrap_or(first),
    )
}

/// The node's source-tool id (a Figma node id).
#[cfg(feature = "svg_export")]
pub(super) fn source_id(node: &Node) -> Option<&str> {
    node.source.as_ref().map(|s| s.id.as_str())
}

#[cfg(feature = "svg_export")]
thread_local! {
    /// How many transform groups (`PaintPrimitive::GroupStart`) the exporter
    /// is currently inside.
    static GROUP_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Track entering (`+1`) / leaving (`-1`) a transform group; `None` resets.
#[cfg(feature = "svg_export")]
pub(super) fn set_group_depth(delta: Option<isize>) {
    GROUP_DEPTH.with(|d| d.set(delta.map_or(0, |delta| d.get().saturating_add_signed(delta))));
}

/// The source tool's absolute bounds `(x, y, w, h)`, sizes clamped to ≥ 0.
///
/// `None` inside a transform group: absolute bounds already include the
/// ancestors' transforms, which the enclosing `<g transform>` applies again.
#[cfg(feature = "svg_export")]
pub(super) fn source_bounds(node: &Node) -> Option<(f64, f64, f64, f64)> {
    if GROUP_DEPTH.with(std::cell::Cell::get) > 0 {
        return None;
    }
    let b = node.source.as_ref()?.bounds?;
    Some((b.x, b.y, b.width.max(0.0), b.height.max(0.0)))
}

/// The exported node's bounds, when it has a positive area.
#[cfg(feature = "svg_export")]
pub(super) fn export_bbox(node: &Node) -> Option<(f64, f64, f64, f64)> {
    let b = node.source.as_ref()?.bounds?;
    (b.width > 0.0 && b.height > 0.0).then_some((b.x, b.y, b.width, b.height))
}

/// The exported node's first visible solid fill, as the canvas background
/// (the paint opacity is the alpha; the color's own alpha is ignored).
#[cfg(feature = "svg_export")]
pub(super) fn export_base_color(node: &Node) -> Option<(u8, u8, u8, u8)> {
    fn to_u8(v: f64) -> u8 {
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    }
    node.fills
        .iter()
        .filter(|p| p.visible)
        .find_map(|p| match &p.kind {
            PaintKind::Solid { color } => Some((
                to_u8(color.r),
                to_u8(color.g),
                to_u8(color.b),
                to_u8(p.opacity),
            )),
            _ => None,
        })
}

/// The node's visible stroke paints (solid, linear and radial only), in
/// stack order.
#[cfg(feature = "svg_export")]
pub(super) fn stroke_paints(
    node: &Node,
    scene: &Scene,
) -> Vec<crate::render::anyrender_renderer::FillPaint> {
    node.stroke
        .paints
        .iter()
        .filter(|p| {
            p.visible
                && matches!(
                    p.kind,
                    PaintKind::Solid { .. }
                        | PaintKind::LinearGradient { .. }
                        | PaintKind::RadialGradient { .. }
                )
        })
        .filter_map(|p| crate::render::anyrender_renderer::fill_paint(p, scene))
        .collect()
}
