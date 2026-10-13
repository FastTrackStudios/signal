//! Dense pages: many large faces with inner shadows, drop shadows and
//! nested clips on one page. vello's GPU buffers are fixed in size; a page
//! that overflows them used to come back blank. Pixel tests skip without a
//! GPU adapter.

use super::*;
use crate::document::proto::{self as doc, Node, NodeKind, Scene};
use crate::render::offscreen::Offscreen;

fn node(kind: NodeKind, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size {
        width: w,
        height: h,
    };
    n
}

fn add(scene: &mut Scene, parent: doc::NodeId, mut n: Node) -> doc::NodeId {
    let id = n.id;
    n.parent = Some(parent);
    scene.node_mut(parent).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    id
}

fn solid(r: f64, g: f64, b: f64) -> doc::Paint {
    doc::Paint::solid(doc::Color::rgba(r, g, b, 1.0))
}

fn effect(kind: doc::EffectKind, dx: f64, dy: f64, radius: f64, a: f64) -> doc::Effect {
    doc::Effect {
        kind,
        visible: true,
        color: doc::Color::rgba(0.0, 0.0, 0.0, a),
        offset_x: dx,
        offset_y: dy,
        radius,
        spread: 0.0,
    }
}

fn clip_frame(x: f64, y: f64, w: f64, h: f64, radius: f64) -> Node {
    let mut f = node(NodeKind::Frame, x, y, w, h);
    f.clips_content = true;
    f.corner_radii = doc::CornerRadii {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    };
    f
}

/// A face shaped like the kit's amp faces: a clipped cabinet, a
/// half-opaque tolex layer, a clipped grille set in with an inner shadow,
/// panels with inner shadows, all nested a few clips deep.
fn face(scene: &mut Scene, page: doc::NodeId, x: f64, y: f64, w: f64, h: f64) {
    let mut cab = clip_frame(x, y, w, h, 18.0);
    cab.fills = vec![solid(0.24, 0.16, 0.12)];
    cab.effects = vec![effect(doc::EffectKind::DropShadow, 0.0, 12.0, 30.0, 0.6)];
    let cab = add(scene, page, cab);
    let mut tolex = clip_frame(0.0, 0.0, w, h, 18.0);
    tolex.opacity = 0.9;
    let tolex = add(scene, cab, tolex);
    let mut base = node(NodeKind::Rectangle, 0.0, 0.0, w, h);
    base.fills = vec![solid(0.3, 0.2, 0.15)];
    base.effects = vec![effect(doc::EffectKind::InnerShadow, 0.0, 6.0, 24.0, 0.7)];
    add(scene, tolex, base);
    let mut grille = clip_frame(24.0, 140.0, w - 48.0, h - 170.0, 6.0);
    grille.opacity = 0.95;
    let grille = add(scene, tolex, grille);
    let mut cloth = node(NodeKind::Rectangle, 0.0, 0.0, w - 48.0, h - 170.0);
    cloth.fills = vec![solid(0.75, 0.55, 0.4)];
    cloth.effects = vec![
        effect(doc::EffectKind::InnerShadow, 0.0, 8.0, 40.0, 0.8),
        effect(doc::EffectKind::InnerShadow, 0.0, -2.0, 6.0, 0.4),
    ];
    add(scene, grille, cloth);
    let mut panel = clip_frame(20.0, 16.0, w - 40.0, 110.0, 4.0);
    panel.fills = vec![solid(0.05, 0.05, 0.05)];
    let panel = add(scene, tolex, panel);
    let mut inset = node(NodeKind::Rectangle, 0.0, 0.0, w - 40.0, 110.0);
    inset.fills = vec![solid(0.08, 0.08, 0.08)];
    inset.effects = vec![effect(doc::EffectKind::InnerShadow, 0.0, 3.0, 12.0, 0.9)];
    add(scene, panel, inset);
}

/// `cols × rows` faces of `fw × fh` on one page.
fn dense_page(cols: usize, rows: usize, fw: f64, fh: f64) -> (Scene, doc::NodeId, u32, u32) {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let gap = 40.0;
    for j in 0..rows {
        for i in 0..cols {
            face(
                &mut scene,
                page,
                gap + i as f64 * (fw + gap),
                gap + j as f64 * (fh + gap),
                fw,
                fh,
            );
        }
    }
    let w = (gap + cols as f64 * (fw + gap)).ceil() as u32;
    let h = (gap + rows as f64 * (fh + gap)).ceil() as u32;
    (scene, page, w, h)
}

/// Render `cols × rows` faces at `scale`; `None` without a GPU. Returns
/// the pixels and the bands the frame took.
fn render_dense(
    o: &mut Offscreen,
    cols: usize,
    rows: usize,
    scale: f64,
) -> (crate::render::offscreen::Pixels, u32) {
    let (scene, page, w, h) = dense_page(cols, rows, 1000.0, 770.0);
    let mut vs = vello::Scene::new();
    paint_scene_into(&mut vs, &scene, page, kurbo::Affine::scale(scale), None);
    let (w, h) = ((f64::from(w) * scale) as u32, (f64::from(h) * scale) as u32);
    let px = o
        .render(&vs, w, h, peniko::Color::from_rgba8(255, 255, 255, 255))
        .expect("render");
    (px, o.last_bands())
}

fn at(p: &crate::render::offscreen::Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

/// Every face drawn: the grille cloth's middle is its colour (no shadow),
/// the gaps between faces the white base, nothing left untouched.
fn assert_drawn(p: &crate::render::offscreen::Pixels, cols: usize, rows: usize, scale: f64) {
    assert!(
        !crate::render::offscreen::is_untouched(&p.rgba),
        "blank page"
    );
    assert!(
        p.rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 255),
        "every pixel drawn opaque"
    );
    for j in 0..rows {
        for i in 0..cols {
            let (x0, y0) = (40.0 + i as f64 * 1040.0, 40.0 + j as f64 * 810.0);
            let cloth = at(
                p,
                ((x0 + 500.0) * scale) as u32,
                ((y0 + 500.0) * scale) as u32,
            );
            assert!(
                cloth[0] > 150 && cloth[0] < 200 && cloth[2] < 120,
                "face {i},{j} cloth: {cloth:?}"
            );
            let gap = at(
                p,
                ((x0 - 20.0) * scale) as u32,
                ((y0 + 400.0) * scale) as u32,
            );
            assert!(gap[0] > 120, "face {i},{j} gap: {gap:?}");
        }
    }
}

/// Four amp-sized faces at 2× used to leave vello's blend stack and
/// command buffers overflowing (inner shadows were layers over each whole
/// node) and the page came back blank. Bounded to the edge band, they now
/// fit in one pass.
#[test]
fn inner_shadowed_faces_fit_one_pass() {
    let Ok(mut o) = Offscreen::new() else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let (p, bands) = render_dense(&mut o, 2, 2, 2.0);
    assert_drawn(&p, 2, 2, 2.0);
    assert_eq!(bands, 1, "the page fits vello's buffers whole");
}

/// A 6320 × 4940 page overflows vello's per-tile command buffer on its
/// own: it is rendered in bands, never blank.
#[test]
fn a_page_too_dense_for_one_pass_is_rendered_in_bands() {
    let Ok(mut o) = Offscreen::new() else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let (p, bands) = render_dense(&mut o, 3, 3, 2.0);
    assert!(bands > 1, "needed bands: {bands}");
    assert_drawn(&p, 3, 3, 2.0);
}

/// A deferred check reads the same answer the waited-for one does: a page
/// that fits comes back clean, the page that overflows vello's buffers
/// comes back overflowed — the host then renders it again, banded.
#[test]
fn a_deferred_check_tells_an_overflow_from_a_page_that_fits() {
    let Ok(mut o) = Offscreen::new() else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let mut overflowed = |cols: usize, rows: usize| {
        let (scene, page, w, h) = dense_page(cols, rows, 1000.0, 770.0);
        let mut vs = vello::Scene::new();
        paint_scene_into(&mut vs, &scene, page, kurbo::Affine::scale(2.0), None);
        o.deferred_overflow(&vs, w * 2, h * 2)
    };
    assert!(!overflowed(2, 2), "four faces fit one pass");
    assert!(overflowed(3, 3), "nine overflow it");
}
