use super::*;
use crate::document::proto::{self as doc, Node, NodeKind, Scene};

/// A scene whose first page holds `nodes`; each entry is `(parent index or
/// None for the page, node)`. Returns the scene and its page id.
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
    n.size = doc::Size {
        width: w,
        height: h,
    };
    n
}

fn solid(r: f64, g: f64, b: f64, opacity: f64) -> doc::Paint {
    doc::Paint {
        opacity,
        ..doc::Paint::solid(doc::Color::rgba(r, g, b, 1.0))
    }
}

#[test]
fn stroke_style_carries_dash_caps_and_opacity() {
    use super::scene_collect::stroke_style;

    let stroke = doc::Stroke {
        paints: vec![solid(1.0, 0.5, 0.0, 1.0)],
        weight: 2.0,
        cap: doc::StrokeCap::Round,
        join: doc::StrokeJoin::Bevel,
        align: doc::StrokeAlign::Inside,
        miter_limit: 8.0,
        dash_pattern: vec![4.0, 2.0],
        dash_offset: 0.0,
    };
    let stroke = stroke_style(&stroke, 0.75).expect("expected stroke");
    assert_eq!(stroke.width, 2.0);
    assert!(matches!(stroke.cap, StrokeCap::Round));
    assert!(matches!(stroke.join, StrokeJoin::Bevel));
    assert!(matches!(stroke.align, StrokeAlign::Inside));
    assert_eq!(stroke.miter_limit, 8.0);
    assert_eq!(stroke.dash_pattern, vec![4.0, 2.0]);
    assert!((stroke.color.a - 0.75).abs() < 1e-9);
}

#[test]
fn image_fill_reads_asset_bytes() {
    use super::scene_collect::fill_paint;

    let mut scene = Scene::new("Page");
    let asset = scene.add_asset("image/png", b"foobar".to_vec());
    let paint = doc::Paint {
        kind: doc::PaintKind::Image {
            asset,
            source_ref: Some("abc123".into()),
            scale_mode: doc::ImageScaleMode::Fill,
            transform: None,
        },
        opacity: 0.5,
        blend: doc::BlendMode::Normal,
        visible: true,
    };
    let Some(FillPaint::Image {
        image_hash,
        data_base64,
        alpha,
        scale_mode,
        ..
    }) = fill_paint(&paint, &scene)
    else {
        panic!("expected image fill");
    };
    assert_eq!(image_hash.as_deref(), Some("abc123"));
    assert_eq!(&*data_base64, "Zm9vYmFy");
    assert!((alpha - 0.5).abs() < 1e-9);
    assert_eq!(scale_mode, ImageScaleMode::Fill);
}

#[test]
fn gradient_paint_opacity_applied_to_stops() {
    use super::scene_collect::fill_paint;

    let scene = Scene::new("Page");
    let stop = |position, r, b| doc::GradientStop {
        position,
        color: doc::Color::rgba(r, 0.0, b, 1.0),
    };
    let paint = doc::Paint {
        kind: doc::PaintKind::LinearGradient {
            stops: vec![stop(0.0, 1.0, 0.0), stop(1.0, 0.0, 1.0)],
            transform: None,
        },
        opacity: 0.5,
        blend: doc::BlendMode::Normal,
        visible: true,
    };
    let Some(FillPaint::GradientLinear { stops, .. }) = fill_paint(&paint, &scene) else {
        panic!("expected linear gradient");
    };
    assert_eq!(stops.len(), 2);
    assert!(
        (stops[0].color.a - 0.5).abs() < 1e-9,
        "first stop alpha: {}",
        stops[0].color.a
    );
    assert!(
        (stops[1].color.a - 0.5).abs() < 1e-9,
        "second stop alpha: {}",
        stops[1].color.a
    );
}

#[test]
fn inherited_opacity_applies_to_child_fills() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    frame.opacity = 0.5;
    let mut rect = node(NodeKind::Rectangle, 10.0, 10.0, 20.0, 20.0);
    rect.fills = vec![solid(1.0, 0.0, 0.0, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), rect)]);

    let found = build_scene_primitives(&scene, root)
        .iter()
        .any(|p| match p {
            PaintPrimitive::Rect { fills, .. } => fills
                .iter()
                .any(|(f, _)| matches!(f, FillPaint::Solid(c) if (c.a - 0.5).abs() < 1e-9)),
            _ => false,
        });
    assert!(found, "expected child fill alpha to include parent opacity");
}

#[test]
fn emits_layer_primitives_for_non_normal_blend_nodes() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 50.0, 50.0);
    frame.blend = doc::BlendMode::Multiply;
    let (scene, root) = scene_of(vec![(None, frame)]);

    let primitives = build_scene_primitives(&scene, root);
    let start = primitives
        .iter()
        .filter(|p| matches!(p, PaintPrimitive::LayerStart { .. }))
        .count();
    let end = primitives
        .iter()
        .filter(|p| matches!(p, PaintPrimitive::LayerEnd { .. }))
        .count();
    assert!(start >= 1, "expected at least one layer start");
    assert_eq!(start, end, "layer boundaries should be balanced");
}

#[test]
fn extracts_rotation_from_transform() {
    let frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    let mut rect = node(NodeKind::Rectangle, 0.0, 0.0, 15.0, 1.0);
    rect.transform = doc::Transform {
        a: 0.934,
        b: -0.358,
        tx: 4.584,
        c: 0.358,
        d: 0.934,
        ty: 22.253,
    };
    rect.fills = vec![solid(1.0, 1.0, 1.0, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), rect)]);

    // atan2(0.358, 0.934) ≈ 0.3649 radians ≈ 20.9°
    let found = build_scene_primitives(&scene, root)
        .iter()
        .any(|p| matches!(p, PaintPrimitive::Rect { rotation, .. } if rotation.abs() > 0.3 && rotation.abs() < 0.4));
    assert!(found, "expected rotated rect with ~0.365 radian rotation");
}

#[test]
fn per_fill_blend_mode_is_kept() {
    let mut rect = node(NodeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    rect.fills = vec![
        solid(1.0, 0.0, 0.0, 1.0),
        doc::Paint {
            blend: doc::BlendMode::SoftLight,
            ..solid(0.0, 0.0, 1.0, 0.5)
        },
    ];
    let (scene, root) = scene_of(vec![(None, rect)]);

    let found = build_scene_primitives(&scene, root)
        .iter()
        .any(|p| match p {
            PaintPrimitive::Rect { fills, .. } => {
                fills.iter().any(|(_, b)| *b == BlendMix::SoftLight)
            }
            _ => false,
        });
    assert!(
        found,
        "expected at least one fill with SoftLight blend mode"
    );
}

#[test]
fn ellipse_without_geometry_emits_round_rect() {
    // An ellipse with no exported geometry draws as a rect with circular radii.
    let frame = node(NodeKind::Frame, 0.0, 0.0, 200.0, 200.0);
    let mut ellipse = node(NodeKind::Ellipse, 10.0, 10.0, 40.0, 40.0);
    ellipse.fills = vec![solid(0.5, 0.5, 0.5, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), ellipse)]);

    let rect = build_scene_primitives(&scene, root)
        .into_iter()
        .find_map(|p| match p {
            PaintPrimitive::Rect {
                width,
                height,
                corner_radii,
                ..
            } if (width - 40.0).abs() < 1e-6 && (height - 40.0).abs() < 1e-6 => Some(corner_radii),
            _ => None,
        });
    let radii = rect.expect("ELLIPSE without geometry should emit a Rect, not a Path");
    assert!(
        (radii.top_left - 20.0).abs() < 1e-6,
        "expected circular corner radius 20.0, got {}",
        radii.top_left
    );
}

#[test]
fn diagnostics_count_typed_features() {
    let mut blended = node(NodeKind::Frame, 0.0, 0.0, 10.0, 10.0);
    blended.blend = doc::BlendMode::Multiply;
    let mut text = node(
        NodeKind::Text(doc::TextData::default()),
        0.0,
        0.0,
        10.0,
        10.0,
    );
    text.effects = vec![doc::Effect {
        kind: doc::EffectKind::LayerBlur,
        visible: true,
        color: doc::Color::rgba(0.0, 0.0, 0.0, 0.25),
        offset_x: 0.0,
        offset_y: 0.0,
        radius: 4.0,
        spread: 0.0,
    }];
    let mut masked = node(NodeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    masked.mask = Some(doc::MaskKind::Luminance);
    let mut hidden_fill = node(NodeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    hidden_fill.fills = vec![doc::Paint {
        visible: false,
        ..solid(0.0, 0.0, 0.0, 1.0)
    }];
    let (scene, root) = scene_of(vec![
        (None, blended),
        (None, text),
        (None, masked),
        (None, hidden_fill),
    ]);

    let d = collect_render_diagnostics(&scene, root);
    assert_eq!(d.total_nodes, 5);
    assert_eq!(d.non_normal_blend_nodes, 1);
    assert_eq!((d.layer_blur_nodes, d.layer_blur_approx_nodes), (1, 1));
    assert_eq!((d.alpha_mask_nodes, d.luminance_mask_nodes), (0, 1));
    assert_eq!(d.gradient_fill_nodes + d.image_fill_nodes, 0);
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_preserves_local_geometry_without_forced_scaling() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 12 12 L 20 20 L 12 20 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0, false);
    assert_eq!(transform, kurbo::Affine::IDENTITY);
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_cancels_node_origin_for_absolute_geometry() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 100 200 L 200 200 L 200 300 L 100 300 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0, false);
    assert_eq!(transform, kurbo::Affine::translate((-100.0, -200.0)));
}

// ── inherited (group) transforms ─────────────────────────────────────────

/// A 90° rotation (x axis → +y, i.e. clockwise on a y-down screen) with its
/// origin at `(tx, ty)`.
fn rot90(tx: f64, ty: f64) -> doc::Transform {
    doc::Transform {
        a: 0.0,
        b: -1.0,
        tx,
        c: 1.0,
        d: 0.0,
        ty,
    }
}

fn red_rect(x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut rect = node(NodeKind::Rectangle, x, y, w, h);
    rect.fills = vec![solid(1.0, 0.0, 0.0, 1.0)];
    rect
}

fn groups(primitives: &[PaintPrimitive]) -> Vec<AffineTransform2D> {
    primitives
        .iter()
        .filter_map(|p| match p {
            PaintPrimitive::GroupStart { transform, .. } => Some(*transform),
            _ => None,
        })
        .collect()
}

fn approx(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

/// The filled rect's local origin, mapped through every enclosing group.
fn world_origin_of_child_rect(primitives: &[PaintPrimitive]) -> (f64, f64) {
    let mut stack = vec![AffineTransform2D::identity()];
    for p in primitives {
        match p {
            PaintPrimitive::GroupStart { transform, .. } => {
                stack.push(*stack.last().unwrap() * *transform)
            }
            PaintPrimitive::GroupEnd { .. } => {
                stack.pop();
            }
            PaintPrimitive::Rect {
                x,
                y,
                fills,
                rotation,
                ..
            } if !fills.is_empty() => {
                assert!(
                    rotation.abs() < 1e-12,
                    "the group carries the rotation, not the child"
                );
                return stack.last().unwrap().transform_point(*x, *y);
            }
            _ => {}
        }
    }
    panic!("no filled rect");
}

#[test]
fn untransformed_containers_and_rotated_leaves_emit_no_groups() {
    let frame = node(NodeKind::Frame, 10.0, 10.0, 100.0, 100.0);
    let mut leaf = red_rect(5.0, 5.0, 20.0, 10.0);
    leaf.transform = rot90(40.0, 5.0);
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), leaf)]);
    let primitives = build_scene_primitives(&scene, root);
    assert!(groups(&primitives).is_empty());
    assert!(primitives.iter().any(
        |p| matches!(p, PaintPrimitive::Rect { rotation, .. } if (rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12)
    ));
}

#[test]
fn rotated_frame_rotates_its_children_about_its_origin() {
    // Frame origin at (100, 100), rotated 90°; child 20×10 at local (10, 0).
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    frame.transform = rot90(100.0, 100.0);
    let (scene, root) = scene_of(vec![
        (None, frame),
        (Some(0), red_rect(10.0, 0.0, 20.0, 10.0)),
    ]);
    let primitives = build_scene_primitives(&scene, root);

    // Pivot about the frame's origin: T(100,100) · R90 · T(-100,-100).
    assert_eq!(
        groups(&primitives),
        vec![AffineTransform2D {
            a: 0.0,
            b: -1.0,
            tx: 200.0,
            c: 1.0,
            d: 0.0,
            ty: 0.0
        }]
    );
    assert!(matches!(
        primitives.first(),
        Some(PaintPrimitive::GroupStart { .. })
    ));
    assert!(matches!(
        primitives.last(),
        Some(PaintPrimitive::GroupEnd { .. })
    ));
    // Local (10, 0) → frame origin + R90·(10, 0) = (100, 110).
    assert!(approx(
        world_origin_of_child_rect(&primitives),
        (100.0, 110.0)
    ));
}

#[test]
fn nested_rotations_compose() {
    // Outer: origin (100, 100), 90°. Inner: at outer-local (50, 0), another 90°.
    let mut outer = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    outer.transform = rot90(100.0, 100.0);
    let mut inner = node(NodeKind::Frame, 0.0, 0.0, 40.0, 40.0);
    inner.transform = rot90(50.0, 0.0);
    let (scene, root) = scene_of(vec![
        (None, outer),
        (Some(0), inner),
        (Some(1), red_rect(10.0, 0.0, 20.0, 10.0)),
    ]);
    let primitives = build_scene_primitives(&scene, root);
    assert_eq!(groups(&primitives).len(), 2);
    // 180° total: inner-local (10, 0) → outer-local (50, 10) → world (90, 150).
    assert!(approx(
        world_origin_of_child_rect(&primitives),
        (90.0, 150.0)
    ));
}

#[test]
fn scaled_parent_scales_children() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 50.0, 50.0);
    frame.transform = doc::Transform {
        a: 2.0,
        b: 0.0,
        tx: 10.0,
        c: 0.0,
        d: 2.0,
        ty: 10.0,
    };
    let (scene, root) = scene_of(vec![
        (None, frame),
        (Some(0), red_rect(5.0, 5.0, 10.0, 10.0)),
    ]);
    let primitives = build_scene_primitives(&scene, root);
    assert_eq!(
        groups(&primitives),
        vec![AffineTransform2D {
            a: 2.0,
            b: 0.0,
            tx: -10.0,
            c: 0.0,
            d: 2.0,
            ty: -10.0
        }]
    );
    assert!(approx(
        world_origin_of_child_rect(&primitives),
        (20.0, 20.0)
    ));
}

#[test]
fn groups_are_transparent_to_their_childrens_transforms() {
    // Figma semantics: a GROUP's children already carry its transform.
    let mut group = node(NodeKind::Group, 0.0, 0.0, 100.0, 100.0);
    group.transform = rot90(100.0, 0.0);
    let (scene, root) = scene_of(vec![
        (None, group),
        (Some(0), red_rect(10.0, 0.0, 20.0, 10.0)),
    ]);
    assert!(groups(&build_scene_primitives(&scene, root)).is_empty());
}

#[test]
fn transformed_mask_bounds_cover_the_transformed_box() {
    let frame = node(NodeKind::Frame, 0.0, 0.0, 200.0, 200.0);
    let mut mask = red_rect(0.0, 0.0, 40.0, 10.0);
    mask.transform = rot90(100.0, 20.0);
    mask.mask = Some(doc::MaskKind::Alpha);
    let (scene, root) = scene_of(vec![
        (None, frame),
        (Some(0), mask),
        (Some(0), red_rect(0.0, 0.0, 200.0, 200.0)),
    ]);
    let bounds = build_scene_primitives(&scene, root)
        .into_iter()
        .find_map(|p| match p {
            PaintPrimitive::MaskDefStart {
                x,
                y,
                width,
                height,
                ..
            } => Some((x, y, width, height)),
            _ => None,
        });
    let (x, y, w, h) = bounds.expect("mask");
    assert!(
        approx((x, y), (90.0, 20.0)) && approx((w, h), (10.0, 40.0)),
        "{bounds:?}"
    );
}

#[cfg(feature = "anyrender")]
mod pixels {
    use super::*;

    /// Render `scene` (no text font) and report which `points` are red;
    /// `None` when there is no GPU.
    fn red_at(
        scene: &Scene,
        root: doc::NodeId,
        size: u32,
        points: &[(u32, u32)],
    ) -> Option<Vec<bool>> {
        let mut vs = vello::Scene::new();
        paint_scene_into(&mut vs, scene, root, kurbo::Affine::IDENTITY, None);
        let Ok(mut offscreen) = crate::render::offscreen::Offscreen::new() else {
            eprintln!("skipping pixel check: no GPU adapter");
            return None;
        };
        let px = offscreen
            .render(&vs, size, size, peniko::Color::from_rgba8(0, 0, 0, 255))
            .expect("render");
        Some(
            points
                .iter()
                .map(|&(x, y)| {
                    let i = ((y * px.width + x) * 4) as usize;
                    px.rgba[i] > 200 && px.rgba[i + 1] < 50 && px.rgba[i + 2] < 50
                })
                .collect(),
        )
    }

    #[test]
    fn rotated_frame_child_pixels_land_rotated() {
        let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
        frame.transform = rot90(100.0, 100.0);
        let (scene, root) = scene_of(vec![
            (None, frame),
            (Some(0), red_rect(10.0, 0.0, 20.0, 10.0)),
        ]);
        // Rotated: x ∈ [90, 100], y ∈ [110, 130]. Unrotated: x ∈ [110, 130], y ∈ [100, 110].
        if let Some(hits) = red_at(&scene, root, 220, &[(95, 120), (120, 105)]) {
            assert_eq!(hits, vec![true, false]);
        }
    }

    #[test]
    fn nested_and_scaled_pixels() {
        let mut outer = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
        outer.transform = rot90(100.0, 100.0);
        let mut inner = node(NodeKind::Frame, 0.0, 0.0, 40.0, 40.0);
        inner.transform = rot90(50.0, 0.0);
        let (scene, root) = scene_of(vec![
            (None, outer),
            (Some(0), inner),
            (Some(1), red_rect(10.0, 0.0, 20.0, 10.0)),
        ]);
        // 180°: x ∈ [70, 90], y ∈ [140, 150]. Without inheritance: [160, 180] × [100, 110].
        if let Some(hits) = red_at(&scene, root, 220, &[(80, 145), (170, 105)]) {
            assert_eq!(hits, vec![true, false]);
        }

        let mut frame = node(NodeKind::Frame, 0.0, 0.0, 50.0, 50.0);
        frame.transform = doc::Transform {
            a: 2.0,
            b: 0.0,
            tx: 10.0,
            c: 0.0,
            d: 2.0,
            ty: 10.0,
        };
        let (scene, root) = scene_of(vec![
            (None, frame),
            (Some(0), red_rect(5.0, 5.0, 10.0, 10.0)),
        ]);
        // Scaled: [20, 40]²; unscaled would be [15, 25]².
        if let Some(hits) = red_at(&scene, root, 64, &[(35, 35), (21, 21), (17, 17)]) {
            assert_eq!(hits, vec![true, true, false]);
        }
    }
}

// ── effects plan ─────────────────────────────────────────────────────────

fn emission(intensity: f64, radius: f64) -> Option<doc::Emission> {
    Some(doc::Emission {
        color: doc::Color::rgba(1.0, 0.5, 0.0, 1.0),
        intensity,
        radius,
    })
}

#[test]
fn scenes_without_effects_have_an_empty_plan_and_unchanged_primitives() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    frame.transform = rot90(100.0, 100.0);
    frame.opacity = 0.5;
    let (scene, root) = scene_of(vec![
        (None, frame),
        (Some(0), red_rect(10.0, 0.0, 20.0, 10.0)),
    ]);
    let (prims, plan) = build_scene_primitives_and_effects(&scene, root);
    assert!(plan.is_empty());
    assert_eq!(prims, build_scene_primitives(&scene, root));
    assert_eq!(
        prims,
        build_scene_primitives_with(
            &scene,
            root,
            CollectOptions {
                emission_glow: false
            }
        )
    );
}

#[test]
fn emissive_draws_carry_world_transform_and_effective_opacity() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    frame.transform = rot90(100.0, 100.0);
    frame.opacity = 0.5;
    let mut lamp = red_rect(10.0, 0.0, 20.0, 10.0);
    lamp.opacity = 0.5;
    lamp.emission = emission(2.0, 12.0);
    let lamp_id = lamp.id;
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), lamp)]);
    let (prims, plan) = build_scene_primitives_and_effects(&scene, root);
    assert_eq!(plan.emissive.len(), 1);
    let e = &plan.emissive[0];
    assert_eq!(e.node_id, lamp_id);
    assert!(
        (e.strength - 2.0 * 0.25).abs() < 1e-9,
        "intensity × inherited opacity: {}",
        e.strength
    );
    assert_eq!(e.radius, 12.0);
    // World = the rotated frame's group.
    assert_eq!(Some(e.world), groups(&prims).first().copied());
    // The geometry is recoloured to the emission colour.
    let Some(PaintPrimitive::Rect { fills, x, y, .. }) = e.primitives.first() else {
        panic!("{:?}", e.primitives)
    };
    assert_eq!((*x, *y), (110.0, 100.0));
    assert!(matches!(&fills[0].0, FillPaint::Solid(c) if c.r == 1.0 && c.g == 0.5 && c.a == 1.0));
    // No glow in the primitives the GPU stage paints.
    assert!(
        prims
            .iter()
            .all(|p| !matches!(p, PaintPrimitive::Rect { effects, .. } if !effects.is_empty()))
    );
}

#[test]
fn fallback_draws_emission_as_a_glow_effect() {
    let mut lamp = red_rect(10.0, 10.0, 20.0, 20.0);
    lamp.opacity = 0.5;
    lamp.emission = emission(1.0, 8.0);
    let (scene, root) = scene_of(vec![(None, lamp)]);
    let glow = |options| {
        build_scene_primitives_with(&scene, root, options)
            .into_iter()
            .find_map(|p| match p {
                PaintPrimitive::Rect { effects, .. } => {
                    effects.into_iter().find(|e| e.kind == EffectKind::Glow)
                }
                _ => None,
            })
    };
    let g = glow(CollectOptions::default()).expect("a glow");
    assert_eq!((g.offset_x, g.offset_y, g.radius), (0.0, 0.0, 8.0));
    assert!(
        (g.color.a - 0.5 * 0.85).abs() < 1e-9,
        "scales with opacity: {}",
        g.color.a
    );
    assert!(
        glow(CollectOptions {
            emission_glow: false
        })
        .is_none()
    );
}

#[test]
fn shader_regions_cover_the_subtree_in_world_space() {
    let mut outer = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    outer.transform = rot90(100.0, 100.0);
    let mut glass = node(NodeKind::Frame, 10.0, 20.0, 40.0, 30.0);
    glass.fills = vec![solid(0.0, 0.0, 1.0, 1.0)];
    glass.shader = Some(doc::ShaderFx {
        shader: "glass".into(),
        params: [("strength".to_string(), 0.3)].into(),
        inputs: Vec::new(),
    });
    // A nested shader draws plainly inside the outer one.
    let mut inner = red_rect(5.0, 5.0, 10.0, 10.0);
    inner.shader = Some(doc::ShaderFx {
        shader: "crt".into(),
        params: Default::default(),
        inputs: Vec::new(),
    });
    let glass_id = glass.id;
    let after = red_rect(0.0, 90.0, 10.0, 10.0);
    let (scene, root) = scene_of(vec![
        (None, outer),
        (Some(0), glass),
        (Some(1), inner),
        (Some(0), after),
    ]);
    let (prims, plan) = build_scene_primitives_and_effects(&scene, root);
    assert_eq!(plan.shaders.len(), 1);
    let r = &plan.shaders[0];
    assert_eq!(r.node_id, glass_id);
    assert_eq!(r.shader, "glass");
    assert_eq!(r.params.get("strength"), Some(&0.3));
    assert_eq!((r.width, r.height), (40.0, 30.0));
    // The glass frame and its child, and nothing after.
    let ids: Vec<_> = prims[r.range.clone()]
        .iter()
        .filter_map(|p| match p {
            PaintPrimitive::Rect { node_id, .. } => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], glass_id);
    // Local (0,0) is the box origin, rotated with the outer frame:
    // flattened (110, 120) → rotated about (100, 100).
    let g = groups(&prims)[0];
    assert_eq!(
        r.world.transform_point(0.0, 0.0),
        g.transform_point(110.0, 120.0)
    );
    assert_eq!(
        r.world.transform_point(40.0, 0.0),
        g.transform_point(150.0, 120.0)
    );
    assert!(!plan.time_dependent(|_| None));
}

#[test]
fn flicker_is_time_dependent_and_modulates_its_subtrees_light() {
    let mut tube = node(NodeKind::Frame, 0.0, 0.0, 50.0, 50.0);
    tube.shader = Some(doc::ShaderFx {
        shader: "flicker".into(),
        params: Default::default(),
        inputs: Vec::new(),
    });
    let mut lamp = red_rect(5.0, 5.0, 10.0, 10.0);
    lamp.emission = emission(1.0, 6.0);
    let (scene, root) = scene_of(vec![(None, tube), (Some(0), lamp)]);
    let (_, plan) = build_scene_primitives_and_effects(&scene, root);
    assert!(plan.time_dependent(|_| None));
    assert!(plan.emissive[0].flicker.is_some());
    let levels: Vec<f64> = (0..50)
        .map(|i| flicker_level(f64::from(i) * 0.037, &Default::default()))
        .collect();
    assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
    assert!(
        levels.iter().any(|l| (l - levels[0]).abs() > 0.01),
        "it varies over time"
    );
}
