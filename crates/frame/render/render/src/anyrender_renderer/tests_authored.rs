//! Authored (non-Figma) drawing: geometry as written, open strokes,
//! outline-following effects, exact aligned strokes, text effects. Pixel
//! tests skip without a GPU adapter.

use super::*;
use crate::offscreen::{Offscreen, Pixels};
use document_proto::{self as doc, Node, NodeKind, Scene};

const ROBOTO: &[u8] = include_bytes!("../../../../tools/cli/assets/roboto/Roboto-Regular.ttf");

fn scene_of(nodes: Vec<(Option<usize>, Node)>) -> (Scene, doc::NodeId) {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let ids: Vec<doc::NodeId> = nodes.iter().map(|(_, n)| n.id).collect();
    for (parent, mut node) in nodes {
        let parent_id = parent.map_or(page, |i| ids[i]);
        node.parent = Some(parent_id);
        scene.node_mut(parent_id).unwrap().children.push(node.id);
        scene.nodes.insert(node.id, node);
    }
    (scene, page)
}

fn node(kind: NodeKind, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size { width: w, height: h };
    n
}

fn paint(rgb: (f64, f64, f64)) -> doc::Paint {
    doc::Paint::solid(doc::Color::rgba(rgb.0, rgb.1, rgb.2, 1.0))
}

fn vector(x: f64, y: f64, w: f64, h: f64, d: &str, rgb: (f64, f64, f64)) -> Node {
    let mut n = node(NodeKind::Vector, x, y, w, h);
    n.fill_geometry = vec![d.into()];
    n.fills = vec![paint(rgb)];
    n
}

/// Mark `n` as a Figma import.
fn from_figma(mut n: Node) -> Node {
    n.source = Some(doc::SourceRef { tool: "figma".into(), id: "1:1".into(), node_type: "VECTOR".into(), bounds: None });
    n
}

fn shadow(kind: doc::EffectKind, dx: f64, dy: f64, radius: f64) -> doc::Effect {
    doc::Effect {
        kind,
        visible: true,
        color: doc::Color::rgba(0.0, 0.0, 0.0, 1.0),
        offset_x: dx,
        offset_y: dy,
        radius,
        spread: 0.0,
    }
}

fn render(scene: &Scene, root: doc::NodeId, w: u32, h: u32, font: bool) -> Option<Pixels> {
    let mut vs = vello::Scene::new();
    let book = font.then(|| crate::FontBook::single(ROBOTO));
    paint_scene_into(&mut vs, scene, root, kurbo::Affine::IDENTITY, book.as_ref());
    let Ok(mut offscreen) = Offscreen::new() else {
        eprintln!("skipping pixel check: no GPU adapter");
        return None;
    };
    Some(offscreen.render(&vs, w, h, peniko::Color::from_rgba8(255, 255, 255, 255)).expect("render"))
}

fn px(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

fn is_red(c: [u8; 4]) -> bool {
    c[0] > 200 && c[1] < 60 && c[2] < 60
}

fn is_white(c: [u8; 4]) -> bool {
    c[0] > 250 && c[1] > 250 && c[2] > 250
}

fn path_of(prims: &[PaintPrimitive]) -> Option<&PaintPrimitive> {
    prims.iter().find(|p| matches!(p, PaintPrimitive::Path { .. }))
}

#[test]
fn figma_imports_are_not_authored_everything_else_is() {
    let (scene, root) = scene_of(vec![(None, vector(0.0, 0.0, 10.0, 10.0, "M0 0 L10 0 L10 10 Z", (1.0, 0.0, 0.0)))]);
    assert!(matches!(path_of(&build_scene_primitives(&scene, root)), Some(PaintPrimitive::Path { authored: true, .. })));
    let (scene, root) = scene_of(vec![(None, from_figma(vector(0.0, 0.0, 10.0, 10.0, "M0 0 L10 0 L10 10 Z", (1.0, 0.0, 0.0))))]);
    assert!(matches!(path_of(&build_scene_primitives(&scene, root)), Some(PaintPrimitive::Path { authored: false, .. })));
}

/// A triangle in the top-left half of its 60×60 box, shadowed 16 px down
/// and to the right. The box's bottom-left corner is outside both the
/// triangle and its shadow.
fn shadowed_triangle(figma: bool, kind: doc::EffectKind) -> (Scene, doc::NodeId) {
    let mut v = vector(20.0, 20.0, 60.0, 60.0, "M0 0 L60 0 L0 60 Z", (1.0, 0.0, 0.0));
    v.effects = vec![shadow(kind, 16.0, 16.0, 2.0)];
    scene_of(vec![(None, if figma { from_figma(v) } else { v })])
}

#[test]
fn drop_shadows_follow_a_vectors_outline() {
    let (scene, root) = shadowed_triangle(false, doc::EffectKind::DropShadow);
    let Some(p) = render(&scene, root, 120, 120, false) else { return };
    // Under the offset triangle, clear of the triangle itself: shadow.
    assert!(px(&p, 60, 55)[0] < 60, "shadow under the shifted outline: {:?}", px(&p, 60, 55));
    // Inside the node's box but outside the shifted triangle: no shadow.
    assert!(is_white(px(&p, 78, 78)), "the box's far corner stays clear: {:?}", px(&p, 78, 78));
    assert!(is_white(px(&p, 70, 70)), "the box's far corner stays clear: {:?}", px(&p, 70, 70));
    assert!(is_red(px(&p, 30, 30)));
}

#[test]
fn figma_imports_keep_their_box_shadows() {
    let (scene, root) = shadowed_triangle(true, doc::EffectKind::DropShadow);
    let Some(p) = render(&scene, root, 120, 120, false) else { return };
    assert!(px(&p, 90, 90)[0] < 60, "the old box shadow: {:?}", px(&p, 90, 90));
}

#[test]
fn inner_shadows_and_glows_follow_the_outline() {
    let (scene, root) = shadowed_triangle(false, doc::EffectKind::InnerShadow);
    let Some(p) = render(&scene, root, 120, 120, false) else { return };
    // The shadow darkens the triangle's top-left edge inside it…
    let edge = px(&p, 22, 40);
    assert!(edge[0] < 120, "inner shadow along the edge: {edge:?}");
    // …and nothing outside the triangle.
    assert!(is_white(px(&p, 70, 70)), "{:?}", px(&p, 70, 70));

    // A glow (emission fallback) is a halo around the outline, not its box.
    let mut v = vector(20.0, 20.0, 60.0, 60.0, "M0 0 L60 0 L0 60 Z", (1.0, 0.0, 0.0));
    v.emission = Some(doc::Emission { color: doc::Color::rgba(0.0, 0.0, 1.0, 1.0), intensity: 1.0, radius: 8.0 });
    let (scene, root) = scene_of(vec![(None, v)]);
    let Some(p) = render(&scene, root, 120, 120, false) else { return };
    let near = px(&p, 50, 55);
    assert!(near[0] < 245 && near[2] > near[0], "halo next to the diagonal edge: {near:?}");
    assert!(is_white(px(&p, 75, 75)), "no glow in the box's empty corner: {:?}", px(&p, 75, 75));
}

#[test]
fn authored_geometry_is_drawn_as_written_not_fitted_into_the_box() {
    // A 10×10 box whose geometry reaches (40, 40).
    let d = "M0 0 L40 0 L40 40 L0 40 Z";
    let (scene, root) = scene_of(vec![(None, vector(10.0, 10.0, 10.0, 10.0, d, (1.0, 0.0, 0.0)))]);
    if let Some(p) = render(&scene, root, 64, 64, false) {
        assert!(is_red(px(&p, 45, 45)), "drawn at its authored size: {:?}", px(&p, 45, 45));
    }
    // Figma imports still fit stray geometry into the box.
    let (scene, root) = scene_of(vec![(None, from_figma(vector(10.0, 10.0, 10.0, 10.0, d, (1.0, 0.0, 0.0))))]);
    if let Some(p) = render(&scene, root, 64, 64, false) {
        assert!(!is_red(px(&p, 45, 45)), "fitted: {:?}", px(&p, 45, 45));
        assert!(is_red(px(&p, 15, 15)));
    }
}

#[test]
fn stroked_open_paths_stroke_the_line_and_fill_nothing() {
    // An arc and a horizontal line, open; filled *and* stroked red.
    let mut v = vector(0.0, 0.0, 100.0, 100.0, "M10 80 A40 40 0 0 1 90 80 M10 95 L90 95", (1.0, 0.0, 0.0));
    v.stroke = doc::Stroke { paints: vec![paint((1.0, 0.0, 0.0))], weight: 4.0, ..doc::Stroke::default() };
    let (scene, root) = scene_of(vec![(None, v)]);
    let Some(p) = render(&scene, root, 100, 100, false) else { return };
    assert!(is_red(px(&p, 50, 40)), "the arc's apex: {:?}", px(&p, 50, 40));
    assert!(is_red(px(&p, 50, 95)), "the line: {:?}", px(&p, 50, 95));
    assert!(is_white(px(&p, 50, 65)), "the chord isn't filled: {:?}", px(&p, 50, 65));
    assert!(is_white(px(&p, 2, 2)), "no box stroke: {:?}", px(&p, 2, 2));

    // A closed path with an inside stroke: the stroke stays inside.
    let mut v = vector(0.0, 0.0, 100.0, 100.0, "M20 20 L80 20 L80 80 L20 80 Z", (1.0, 1.0, 1.0));
    v.stroke = doc::Stroke { paints: vec![paint((1.0, 0.0, 0.0))], weight: 6.0, align: doc::StrokeAlign::Inside, ..doc::Stroke::default() };
    let (scene, root) = scene_of(vec![(None, v)]);
    let Some(p) = render(&scene, root, 100, 100, false) else { return };
    assert!(is_red(px(&p, 50, 23)), "inside the edge: {:?}", px(&p, 50, 23));
    assert!(is_white(px(&p, 50, 17)), "not outside: {:?}", px(&p, 50, 17));
}

#[test]
fn inside_strokes_on_rounded_panels_leave_no_band_along_the_top() {
    // The 1176 faceplate: 960 wide at the canvas top, radius 3.2, a
    // 1.0667 px inside stroke. vello's stroker used to fill the whole
    // first 16 px tile row with the stroke colour.
    let mut panel = node(NodeKind::Frame, 0.0, 0.0, 960.0, 300.0);
    panel.corner_radii = doc::CornerRadii::uniform(3.2);
    panel.fills = vec![paint((0.8, 0.8, 0.8))];
    panel.stroke = doc::Stroke {
        paints: vec![paint((0.0, 0.0, 0.0))],
        weight: 1.066_666_666_666_666_7,
        align: doc::StrokeAlign::Inside,
        ..doc::Stroke::default()
    };
    let (scene, root) = scene_of(vec![(None, panel)]);
    let Some(p) = render(&scene, root, 960, 300, false) else { return };
    for y in [3, 8, 14] {
        let c = px(&p, 480, y);
        assert!(c[0] > 200, "row {y} is the panel's own paint: {c:?}");
    }
    assert!(px(&p, 480, 0)[0] < 60, "the stroke itself: {:?}", px(&p, 480, 0));
    assert!(px(&p, 480, 299)[0] < 60, "…at the bottom too: {:?}", px(&p, 480, 299));
}

#[test]
fn text_carries_glows_and_authored_shadows() {
    let mut t = node(NodeKind::Text(doc::TextData { characters: "IIII".into(), font_size: 40.0, ..doc::TextData::default() }), 10.0, 10.0, 120.0, 50.0);
    t.fills = vec![paint((1.0, 0.0, 0.0))];
    t.emission = Some(doc::Emission { color: doc::Color::rgba(0.0, 0.0, 1.0, 1.0), intensity: 1.0, radius: 10.0 });
    t.effects = vec![shadow(doc::EffectKind::DropShadow, 0.0, 30.0, 0.0)];
    let text_effects = |scene: &Scene, root, options| {
        build_scene_primitives_with(scene, root, options).into_iter().find_map(|p| match p {
            PaintPrimitive::Text { effects, .. } => Some(effects.iter().map(|e| e.kind.clone()).collect::<Vec<_>>()),
            _ => None,
        })
    };
    let (scene, root) = scene_of(vec![(None, t.clone())]);
    assert_eq!(text_effects(&scene, root, CollectOptions::default()), Some(vec![EffectKind::DropShadow, EffectKind::Glow]));
    // The GPU stage blooms emission itself.
    assert_eq!(text_effects(&scene, root, CollectOptions { emission_glow: false }), Some(vec![EffectKind::DropShadow]));
    // Figma text shadows come from its exported outlines, not here.
    let (figma, froot) = scene_of(vec![(None, from_figma(t))]);
    assert_eq!(text_effects(&figma, froot, CollectOptions::default()), Some(vec![EffectKind::Glow]));

    let Some(p) = render(&scene, root, 160, 110, true) else { return };
    // Find a glyph stem on the text's middle row, then look beside it.
    let row = 35;
    let stem = (10..150).find(|&x| is_red(px(&p, x, row))).expect("glyphs drawn");
    let beside = px(&p, stem - 3, row);
    assert!(beside[2] > beside[0] && beside[0] < 250, "glow beside the stem: {beside:?}");
    // The hard shadow repeats the stems 30 px lower, in black.
    let below = px(&p, stem + 2, row + 30);
    assert!(below[0] < 60 && below[2] < 60, "shadow of the stem: {below:?}");
    assert!(is_white(px(&p, 155, 105)) || px(&p, 155, 105)[2] > 200, "far corner clear");
}

#[test]
fn clipping_frames_clip_authored_geometry_that_reaches_past_its_box() {
    let clipped = |figma: bool| {
        let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
        frame.clips_content = true;
        frame.fills = vec![paint((0.5, 0.5, 0.5))];
        // Boxed inside the frame; its geometry starts 20 px left of it.
        let v = vector(0.0, 10.0, 100.0, 10.0, "M-20 0 H80 V2 H-20 Z", (1.0, 0.0, 0.0));
        let tag = |n: Node| if figma { from_figma(n) } else { n };
        let (scene, root) = scene_of(vec![(None, tag(frame)), (Some(0), tag(v))]);
        build_scene_primitives(&scene, root).iter().any(|p| matches!(p, PaintPrimitive::ClipStart { .. }))
    };
    assert!(clipped(false));
    assert!(!clipped(true), "Figma imports keep their clip decisions");
}

/// Ink across one row: how many pixels are red.
fn red_run(p: &Pixels, row: u32) -> usize {
    (0..p.width).filter(|&x| is_red(px(p, x, row))).count()
}

fn render_with(scene: &Scene, root: doc::NodeId, w: u32, h: u32, book: &crate::FontBook) -> Option<Pixels> {
    let mut vs = vello::Scene::new();
    paint_scene_into(&mut vs, scene, root, kurbo::Affine::IDENTITY, Some(book));
    let Ok(mut offscreen) = Offscreen::new() else {
        eprintln!("skipping pixel check: no GPU adapter");
        return None;
    };
    Some(offscreen.render(&vs, w, h, peniko::Color::from_rgba8(255, 255, 255, 255)).expect("render"))
}

fn text_node(chars: &str, weight: Option<f64>, family: Option<&str>) -> Node {
    let mut t = node(
        NodeKind::Text(doc::TextData {
            characters: chars.into(),
            font_size: 60.0,
            font_weight: weight,
            font_family: family.map(Into::into),
            ..doc::TextData::default()
        }),
        10.0,
        10.0,
        300.0,
        80.0,
    );
    t.fills = vec![paint((1.0, 0.0, 0.0))];
    t
}

#[test]
fn node_weight_thickens_the_stems() {
    // One stem (an `I`): at weight 900 it is a heavier instance of the
    // glyph, far wider than regular — not the regular glyph drawn twice.
    let book = crate::FontBook::bundled();
    let stem = |weight| {
        let (scene, root) = scene_of(vec![(None, text_node("I", weight, None))]);
        render_with(&scene, root, 120, 100, book).map(|p| red_run(&p, 50))
    };
    let (Some(regular), Some(black)) = (stem(None), stem(Some(900.0))) else { return };
    assert!(regular >= 4, "regular stem {regular}px");
    assert!(black as f64 >= regular as f64 * 1.8, "black stem {black}px vs regular {regular}px");
}

#[test]
fn family_changes_the_letterforms() {
    let book = crate::FontBook::bundled();
    let ink = |family| {
        let (scene, root) = scene_of(vec![(None, text_node("88", None, family))]);
        render_with(&scene, root, 320, 100, book).map(|p| (0..100).map(|y| red_run(&p, y)).sum::<usize>())
    };
    let (Some(inter), Some(lcd), Some(unknown)) = (ink(None), ink(Some("DSEG7 Classic")), ink(Some("No Such Family"))) else { return };
    assert_ne!(inter, lcd, "DSEG7 draws segments, not Inter's 8");
    assert_eq!(inter, unknown, "an unknown family falls back to Inter");
}
