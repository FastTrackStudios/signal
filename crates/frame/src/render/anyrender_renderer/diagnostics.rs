//! Feature counts over a scene subtree: which render paths a document
//! exercises, and how many of them are approximated.

use crate::document::proto::{BlendMode, EffectKind, MaskKind, NodeId, NodeKind, PaintKind, Scene};

use super::types::RenderDiagnostics;

/// Count the render features used by `root`'s subtree (every node, visible
/// or not; only visible paints and effects count).
#[must_use]
pub fn collect_render_diagnostics(scene: &Scene, root: NodeId) -> RenderDiagnostics {
    let mut out = RenderDiagnostics::default();
    for id in scene.walk(root) {
        let Some(node) = scene.node(id) else { continue };
        out.total_nodes += 1;
        if !matches!(node.blend, BlendMode::Normal | BlendMode::PassThrough) {
            out.non_normal_blend_nodes += 1;
        }
        let fills = || node.fills.iter().filter(|p| p.visible);
        if fills().any(|p| {
            matches!(
                p.kind,
                PaintKind::LinearGradient { .. }
                    | PaintKind::RadialGradient { .. }
                    | PaintKind::AngularGradient { .. }
            )
        }) {
            out.gradient_fill_nodes += 1;
        }
        if fills().any(|p| matches!(p.kind, PaintKind::Image { .. })) {
            out.image_fill_nodes += 1;
        }
        let has = |kind: EffectKind| node.effects.iter().any(|e| e.visible && e.kind == kind);
        if has(EffectKind::DropShadow) {
            out.drop_shadow_nodes += 1;
        }
        if has(EffectKind::InnerShadow) {
            out.inner_shadow_nodes += 1;
        }
        if has(EffectKind::LayerBlur) {
            out.layer_blur_nodes += 1;
            // Layer blur is only approximated on text and non-drawing nodes.
            if matches!(
                node.kind,
                NodeKind::Text(_) | NodeKind::Utility | NodeKind::Other { .. }
            ) {
                out.layer_blur_approx_nodes += 1;
            }
        }
        if has(EffectKind::BackgroundBlur) {
            out.background_blur_nodes += 1;
            out.background_blur_approx_nodes += 1;
        }
        match node.mask {
            Some(MaskKind::Alpha) => out.alpha_mask_nodes += 1,
            Some(MaskKind::Luminance) => out.luminance_mask_nodes += 1,
            Some(MaskKind::Vector) | None => {}
        }
    }
    out
}
