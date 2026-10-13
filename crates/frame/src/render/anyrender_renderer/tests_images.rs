//! Image fills: tiles repeat, image data is encoded and decoded once, and
//! textures survive every frame of the GPU effects stage. Pixel tests skip
//! without a GPU adapter.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::document::proto::{self as doc, Node, NodeKind, Scene};
use crate::render::offscreen::{Offscreen, Pixels};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// A `w × h` PNG of red and blue pixels in a checkerboard.
fn checker_png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba(if (x + y) % 2 == 0 { RED } else { BLUE })
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

fn add(scene: &mut Scene, parent: doc::NodeId, mut n: Node) -> doc::NodeId {
    let id = n.id;
    n.parent = Some(parent);
    scene.node_mut(parent).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    id
}

fn node(kind: NodeKind, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size {
        width: w,
        height: h,
    };
    n
}

fn image_paint(
    asset: doc::AssetId,
    scale_mode: doc::ImageScaleMode,
    transform: Option<doc::Transform>,
) -> doc::Paint {
    doc::Paint {
        kind: doc::PaintKind::Image {
            asset,
            source_ref: None,
            scale_mode,
            transform,
        },
        opacity: 1.0,
        blend: doc::BlendMode::Normal,
        visible: true,
    }
}

/// A page with one `64 × 64` rect at (8, 8) filled with a 2×2 checker
/// PNG in `scale_mode`, placed by `transform`.
fn checker_page(
    scale_mode: doc::ImageScaleMode,
    transform: Option<doc::Transform>,
) -> (Scene, doc::NodeId) {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let asset = scene.add_asset("image/png", checker_png(2, 2));
    let mut r = node(NodeKind::Rectangle, 8.0, 8.0, 64.0, 64.0);
    r.fills = vec![image_paint(asset, scale_mode, transform)];
    add(&mut scene, page, r);
    (scene, page)
}

fn render(scene: &Scene, root: doc::NodeId) -> Option<Pixels> {
    let mut vs = vello::Scene::new();
    paint_scene_into(&mut vs, scene, root, kurbo::Affine::IDENTITY, None);
    let Ok(mut o) = Offscreen::new() else {
        eprintln!("skipping pixel check: no GPU adapter");
        return None;
    };
    Some(o.render(&vs, 80, 80, peniko::Color::WHITE).expect("render"))
}

fn px(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) < 48)
}

#[test]
fn a_tile_repeats_at_the_size_its_transform_gives_it() {
    // Each image pixel 8 px: a 16 px tile, repeated four times across.
    let (scene, root) = checker_page(
        doc::ImageScaleMode::Tile,
        Some(doc::Transform {
            a: 8.0,
            b: 0.0,
            c: 0.0,
            d: 8.0,
            tx: 0.0,
            ty: 0.0,
        }),
    );
    let Some(p) = render(&scene, root) else {
        return;
    };
    for tile in 0..4 {
        let x0 = 8 + tile * 16;
        assert!(
            close(px(&p, x0 + 4, 12), RED),
            "tile {tile} first cell: {:?}",
            px(&p, x0 + 4, 12)
        );
        assert!(
            close(px(&p, x0 + 12, 12), BLUE),
            "tile {tile} second cell: {:?}",
            px(&p, x0 + 12, 12)
        );
        assert!(
            close(px(&p, x0 + 4, 60), RED),
            "tile {tile}, fourth row: {:?}",
            px(&p, x0 + 4, 60)
        );
    }
    // Nothing outside the rect.
    assert!(close(px(&p, 76, 40), [255, 255, 255, 255]));
}

#[test]
fn a_tile_without_a_transform_repeats_at_its_own_size() {
    let (scene, root) = checker_page(doc::ImageScaleMode::Tile, None);
    let Some(p) = render(&scene, root) else {
        return;
    };
    // One image pixel per px: a checkerboard all the way across (was the
    // tile once, then its edge pixels smeared).
    for x in [8, 9, 40, 41, 70, 71] {
        let want = if (x + 8) % 2 == 0 { RED } else { BLUE };
        assert!(close(px(&p, x, 8), want), "x {x}: {:?}", px(&p, x, 8));
    }
}

#[test]
fn image_data_is_encoded_once_and_shared_across_collects() {
    let (scene, root) = checker_page(doc::ImageScaleMode::Fill, None);
    let data = |prims: Vec<PaintPrimitive>| {
        prims.into_iter().find_map(|p| match p {
            PaintPrimitive::Rect { fills, .. } => fills.into_iter().find_map(|(f, _)| match f {
                FillPaint::Image { data_base64, .. } => Some(data_base64),
                _ => None,
            }),
            _ => None,
        })
    };
    let a = data(build_scene_primitives(&scene, root)).expect("image fill");
    let b = data(build_scene_primitives(&scene, root)).expect("image fill");
    assert!(
        Arc::ptr_eq(&a, &b),
        "the second collect reuses the first's encoding"
    );
    use base64::Engine as _;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&*a)
            .unwrap(),
        checker_png(2, 2)
    );
}

/// Every frame of the GPU effects stage draws the textures: with a shader
/// region and an emitter, a frame is several vello renders, and textures
/// used to vanish from the second frame on.
#[test]
fn textures_draw_in_every_gpu_effects_frame() {
    let Ok(mut o) = Offscreen::new() else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let (mut scene, page) = checker_page(
        doc::ImageScaleMode::Tile,
        Some(doc::Transform {
            a: 8.0,
            b: 0.0,
            c: 0.0,
            d: 8.0,
            tx: 0.0,
            ty: 0.0,
        }),
    );
    let mut panel = node(NodeKind::Frame, 40.0, 40.0, 30.0, 30.0);
    panel.fills = vec![doc::Paint::solid(doc::Color::rgba(0.2, 0.5, 0.9, 1.0))];
    panel.shader = Some(doc::ShaderFx {
        shader: "glass".into(),
        params: BTreeMap::new(),
        inputs: Vec::new(),
    });
    add(&mut scene, page, panel);
    let mut lamp = node(NodeKind::Ellipse, 60.0, 4.0, 6.0, 6.0);
    lamp.fills = vec![doc::Paint::solid(doc::Color::rgba(1.0, 0.8, 0.2, 1.0))];
    lamp.emission = Some(doc::Emission {
        color: doc::Color::rgba(1.0, 0.8, 0.2, 1.0),
        intensity: 1.0,
        radius: 6.0,
    });
    add(&mut scene, page, lamp);
    let frame = crate::render::fx::Frame {
        base: peniko::Color::WHITE,
        ..crate::render::fx::Frame::new(&scene, page, 80, 80)
    };
    let frames: Vec<Pixels> = (0..3)
        .map(|_| o.render_with_effects(&frame).expect("render"))
        .collect();
    for (i, f) in frames.iter().enumerate() {
        assert!(
            close(px(f, 12, 12), RED) && close(px(f, 20, 12), BLUE),
            "frame {i} lost the texture: {:?} {:?}",
            px(f, 12, 12),
            px(f, 20, 12)
        );
    }
    assert!(
        frames[1].rgba == frames[0].rgba && frames[2].rgba == frames[0].rgba,
        "frames of an unchanged scene are identical"
    );
}

/// A line stroked with a red → blue gradient draws (it used to vanish),
/// red at its start and blue at its end.
#[test]
fn a_gradient_stroke_draws_its_gradient() {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let mut line = node(NodeKind::Line, 10.0, 40.0, 60.0, 0.0);
    line.fill_geometry = vec!["M0 0 L60 0".into()];
    line.stroke = doc::Stroke {
        paints: vec![doc::Paint {
            kind: doc::PaintKind::LinearGradient {
                stops: vec![
                    doc::GradientStop {
                        position: 0.0,
                        color: doc::Color::rgba(1.0, 0.0, 0.0, 1.0),
                    },
                    doc::GradientStop {
                        position: 1.0,
                        color: doc::Color::rgba(0.0, 0.0, 1.0, 1.0),
                    },
                ],
                transform: None,
            },
            opacity: 1.0,
            blend: doc::BlendMode::Normal,
            visible: true,
        }],
        weight: 8.0,
        ..doc::Stroke::default()
    };
    add(&mut scene, page, line);
    let prims = build_scene_primitives(&scene, page);
    assert!(
        prims.iter().any(|p| matches!(
            p,
            PaintPrimitive::Path {
                stroke: Some(StrokeStyle {
                    gradient: Some(FillPaint::GradientLinear { .. }),
                    ..
                }),
                ..
            }
        )),
        "the stroke keeps its gradient"
    );
    let Some(p) = render(&scene, page) else {
        return;
    };
    let (start, end) = (px(&p, 12, 40), px(&p, 68, 40));
    assert!(
        start[0] > 200 && start[2] < 60,
        "red at the start: {start:?}"
    );
    assert!(end[2] > 200 && end[0] < 60, "blue at the end: {end:?}");
}
