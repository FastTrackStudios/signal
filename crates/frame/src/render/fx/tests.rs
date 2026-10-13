//! GPU pixel tests of the effects stage (skipped without a GPU adapter).

use std::collections::BTreeMap;

use crate::document::proto::{self as doc, Node, NodeKind, Scene};

use super::stage::slice_with_context;
use super::{Frame, Mode};
use crate::render::PaintPrimitive;
use crate::render::offscreen::{Offscreen, Pixels};

const SIZE: u32 = 100;

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

fn shape(kind: NodeKind, x: f64, y: f64, w: f64, h: f64, rgb: (f64, f64, f64)) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size {
        width: w,
        height: h,
    };
    n.fills = vec![doc::Paint::solid(doc::Color::rgba(
        rgb.0, rgb.1, rgb.2, 1.0,
    ))];
    n
}

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("skipping GPU effects test: {e}");
            None
        }
    }
}

fn render(o: &mut Offscreen, scene: &Scene, root: doc::NodeId, mode: Mode) -> Pixels {
    let frame = Frame {
        mode,
        ..Frame::new(scene, root, SIZE, SIZE)
    };
    o.render_with_effects(&frame).expect("render")
}

fn px(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

/// A red dot in the middle of a black canvas that emits red light.
fn emissive_dot() -> (Scene, doc::NodeId) {
    let mut dot = shape(NodeKind::Ellipse, 44.0, 44.0, 12.0, 12.0, (1.0, 0.1, 0.1));
    dot.emission = Some(doc::Emission {
        color: doc::Color::rgba(1.0, 0.2, 0.1, 1.0),
        intensity: 1.5,
        radius: 16.0,
    });
    scene_of(vec![(None, dot)])
}

#[test]
fn emission_blooms_outside_its_geometry_on_the_gpu_only_softly_in_fallback() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = emissive_dot();
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let fallback = render(&mut o, &scene, root, Mode::Fallback);
    // 8 px outside the dot's edge.
    let (x, y) = (50, 36);
    assert_eq!(px(&off, x, y)[0], 0, "no light without effects");
    assert!(
        px(&gpu, x, y)[0] > 12,
        "GPU bloom lights the surroundings: {:?}",
        px(&gpu, x, y)
    );
    assert!(
        px(&fallback, x, y)[0] > 4,
        "fallback glow lights the surroundings: {:?}",
        px(&fallback, x, y)
    );
    // The dot itself is still drawn; far corners stay dark.
    assert!(px(&off, 50, 50)[0] > 200);
    assert!(px(&gpu, 2, 2)[0] < 3, "{:?}", px(&gpu, 2, 2));
}

#[test]
fn emission_scales_with_opacity() {
    let Some(mut o) = offscreen() else { return };
    let (mut scene, root) = emissive_dot();
    let bright = render(&mut o, &scene, root, Mode::Gpu);
    let dot = scene.node(root).unwrap().children[0];
    scene.node_mut(dot).unwrap().opacity = 0.0;
    let dark = render(&mut o, &scene, root, Mode::Gpu);
    assert!(px(&bright, 50, 36)[0] > 12);
    assert_eq!(px(&dark, 50, 36)[0], 0, "a dimmed lamp gives no light");
}

/// Vertical stripes 20..80 × 20..80 under a shader node covering 30..70.
fn shaded(shader: &str) -> (Scene, doc::NodeId) {
    let mut panel = shape(NodeKind::Frame, 30.0, 30.0, 40.0, 40.0, (0.2, 0.5, 0.9));
    panel.shader = Some(doc::ShaderFx {
        shader: shader.to_string(),
        params: BTreeMap::new(),
        inputs: Vec::new(),
    });
    let mut nodes = vec![
        (
            None,
            shape(NodeKind::Rectangle, 10.0, 10.0, 80.0, 80.0, (0.5, 0.5, 0.5)),
        ),
        (None, panel),
    ];
    for i in 0..8 {
        nodes.push((
            Some(1),
            shape(
                NodeKind::Rectangle,
                f64::from(i) * 5.0,
                0.0,
                2.5,
                40.0,
                (1.0, 1.0, 1.0),
            ),
        ));
    }
    scene_of(nodes)
}

fn changed_inside_only(off: &Pixels, fx: &Pixels, what: &str) {
    let mut inside = 0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let differs = px(off, x, y) != px(fx, x, y);
            let in_box = (30..70).contains(&x) && (30..70).contains(&y);
            if differs {
                assert!(
                    in_box,
                    "{what} changed ({x},{y}) outside its box: {:?} → {:?}",
                    px(off, x, y),
                    px(fx, x, y)
                );
                inside += 1;
            }
        }
    }
    assert!(
        inside > 100,
        "{what} changed only {inside} px inside its box"
    );
}

#[test]
fn glass_and_crt_change_pixels_only_inside_their_box() {
    let Some(mut o) = offscreen() else { return };
    for shader in ["glass", "crt", "lighting"] {
        let (scene, root) = shaded(shader);
        let off = render(&mut o, &scene, root, Mode::Off);
        let fallback = render(&mut o, &scene, root, Mode::Fallback);
        assert_eq!(off, fallback, "{shader}: the fallback skips shaders");
        let gpu = render(&mut o, &scene, root, Mode::Gpu);
        changed_inside_only(&off, &gpu, shader);
    }
}

#[test]
fn shader_keeps_paint_order_with_later_siblings() {
    let Some(mut o) = offscreen() else { return };
    let (mut scene, root) = shaded("crt");
    // A green bar painted after the shaded panel, across it.
    let bar = shape(NodeKind::Rectangle, 0.0, 45.0, 100.0, 10.0, (0.0, 1.0, 0.0));
    let id = bar.id;
    scene.node_mut(root).unwrap().children.push(id);
    scene.nodes.insert(
        id,
        Node {
            parent: Some(root),
            ..bar
        },
    );
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let c = px(&gpu, 50, 50);
    assert!(
        c[1] > 240 && c[0] < 10,
        "the later sibling stays on top: {c:?}"
    );
}

fn with_asset_shader(source: &[u8]) -> (Scene, doc::NodeId) {
    let (mut scene, root) = shaded("");
    let asset = scene.add_asset("text/wgsl", source.to_vec());
    let panel = scene.node(root).unwrap().children[1];
    scene.node_mut(panel).unwrap().shader = Some(doc::ShaderFx {
        shader: format!("asset:{}", asset.0),
        params: BTreeMap::new(),
        inputs: Vec::new(),
    });
    (scene, root)
}

#[test]
fn custom_asset_shader_runs() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = with_asset_shader(
        b"fn effect(uv: vec2<f32>) -> vec4<f32> { let c = sample_src(uv); return vec4<f32>(vec3<f32>(c.a) - c.rgb, c.a); }",
    );
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    changed_inside_only(&off, &gpu, "invert");
    // A white stripe inverts to black; the blue panel to orange.
    let (w, b) = (px(&off, 31, 50), px(&gpu, 31, 50));
    assert!(w[0] > 240 && b[0] < 15, "white → black: {w:?} → {b:?}");
    let (p, q) = (px(&off, 34, 50), px(&gpu, 34, 50));
    assert!(
        (i32::from(p[0]) + i32::from(q[0]) - 255).abs() < 6,
        "blue → orange: {p:?} → {q:?}"
    );
}

#[test]
fn broken_custom_shader_falls_back_to_plain() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) =
        with_asset_shader(b"fn effect(uv: vec2<f32>) -> vec4<f32> { return nope; }");
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    assert_eq!(off, gpu, "a broken shader draws its subtree plainly");
    // Missing asset / unknown built-in: the same.
    let (scene, root) = shaded("no-such-shader");
    assert_eq!(
        render(&mut o, &scene, root, Mode::Off),
        render(&mut o, &scene, root, Mode::Gpu)
    );
}

#[test]
fn no_effects_renders_like_plain_vello() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = shaded("");
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let mut vs = vello::Scene::new();
    crate::render::paint_scene_into(&mut vs, &scene, root, kurbo::Affine::IDENTITY, None);
    let plain = o
        .render(&vs, SIZE, SIZE, peniko::Color::from_rgba8(0, 0, 0, 255))
        .unwrap();
    assert_eq!(gpu, plain);
}

#[test]
fn context_slices_reopen_and_close_enclosing_scopes() {
    let (mut scene, root) = shaded("crt");
    let panel = scene.node(root).unwrap().children[1];
    scene.node_mut(panel).unwrap().opacity = 0.5;
    scene.node_mut(panel).unwrap().blend = doc::BlendMode::Multiply;
    let (prims, plan) = crate::render::build_scene_primitives_and_effects(&scene, root);
    let r = &plan.shaders[0];
    let sub = slice_with_context(&prims, r.range.clone());
    assert!(matches!(
        sub.first(),
        Some(PaintPrimitive::LayerStart { .. })
    ));
    assert!(matches!(sub.last(), Some(PaintPrimitive::LayerEnd { .. })));
    // A slice starting inside the layer reopens it and closes it.
    let inner = slice_with_context(&prims, r.range.start + 1..r.range.start + 2);
    assert!(matches!(
        inner.first(),
        Some(PaintPrimitive::LayerStart { .. })
    ));
    assert!(matches!(
        inner.last(),
        Some(PaintPrimitive::LayerEnd { .. })
    ));
    assert_eq!(inner.len(), 3);
}

#[test]
fn mode_parses_and_params_pack_into_named_slots() {
    assert_eq!("gpu".parse::<Mode>(), Ok(Mode::Gpu));
    assert_eq!("fallback".parse::<Mode>(), Ok(Mode::Fallback));
    assert_eq!("off".parse::<Mode>(), Ok(Mode::Off));
    assert!("nope".parse::<Mode>().is_err());
    let params = BTreeMap::from([
        ("strength".to_string(), 0.25),
        ("angle".to_string(), 90.0),
        ("bogus".to_string(), 1.0),
    ]);
    let (values, mask) = super::pack_params(&params);
    assert_eq!(values[0], 0.25);
    assert_eq!(values[4], 90.0);
    assert_eq!(mask, 0b1_0001);
}

#[test]
fn builtins_compile_through_naga() {
    for id in super::BUILTINS {
        let src = super::effect_module(super::builtin_source(id).unwrap());
        let module = vello::wgpu::naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&src)));
        vello::wgpu::naga::valid::Validator::new(
            vello::wgpu::naga::valid::ValidationFlags::all(),
            vello::wgpu::naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&src)));
    }
}

#[test]
fn emission_is_cut_to_its_ancestors_clips() {
    let Some(mut o) = offscreen() else { return };
    // A window 0..100 × 0..50 that clips a bar hanging out of it (20..80).
    let mut window = Node::new(NodeKind::Frame, "window");
    window.size = doc::Size {
        width: 100.0,
        height: 50.0,
    };
    window.clips_content = true;
    let mut bar = shape(NodeKind::Rectangle, 48.0, 20.0, 4.0, 60.0, (1.0, 0.1, 0.1));
    bar.emission = Some(doc::Emission {
        color: doc::Color::rgba(1.0, 0.2, 0.1, 1.0),
        intensity: 1.5,
        radius: 16.0,
    });
    let (scene, root) = scene_of(vec![(None, window), (Some(0), bar)]);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    assert!(
        px(&gpu, 56, 40)[0] > 12,
        "the visible part glows: {:?}",
        px(&gpu, 56, 40)
    );
    // Just past where the clipped-away bar would end: no light leaks out.
    assert!(
        px(&gpu, 50, 85)[0] < 3,
        "light from the clipped part: {:?}",
        px(&gpu, 50, 85)
    );
}

#[test]
fn a_stroked_line_glows_along_the_line_not_its_box() {
    let Some(mut o) = offscreen() else { return };
    // A diagonal open line across a 60×60 box at 20..80, stroke only.
    let mut line = Node::new(NodeKind::Vector, "line");
    line.transform = doc::Transform::translate(20.0, 20.0);
    line.size = doc::Size {
        width: 60.0,
        height: 60.0,
    };
    line.fill_geometry = vec!["M 0 60 L 60 0".to_string()];
    line.stroke = doc::Stroke {
        paints: vec![doc::Paint::solid(doc::Color::rgba(1.0, 0.1, 0.1, 1.0))],
        weight: 2.0,
        align: doc::StrokeAlign::Center,
        ..doc::Stroke::default()
    };
    line.emission = Some(doc::Emission {
        color: doc::Color::rgba(1.0, 0.2, 0.1, 1.0),
        intensity: 1.5,
        radius: 6.0,
    });
    let (scene, root) = scene_of(vec![(None, line)]);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    assert!(
        px(&gpu, 53, 50)[0] > 12,
        "glow beside the line: {:?}",
        px(&gpu, 53, 50)
    );
    // The box's far corners are ~42 px from the line: dark.
    assert!(
        px(&gpu, 24, 24)[0] < 3,
        "the box glows: {:?}",
        px(&gpu, 24, 24)
    );
    assert!(
        px(&gpu, 76, 76)[0] < 3,
        "the box glows: {:?}",
        px(&gpu, 76, 76)
    );
}

#[test]
fn every_builtin_validates_as_the_stage_composes_it() {
    use vello::wgpu::naga;
    for id in super::BUILTINS {
        let body = super::builtin_source(id).expect("builtin");
        let source = super::effect_module(body);
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&source)));
    }
}

#[test]
fn inputs_are_the_in_names_an_effect_mentions_in_slot_order() {
    let body = "fn effect(uv: vec2<f32>) -> vec4<f32> { let a = input_or(IN_THRESHOLD, 0.0) + input_at(IN_GR, uv.x) + input_or(IN_GR, 1.0); let b = MIN_X; return vec4<f32>(a); }";
    assert_eq!(super::effect_inputs(body), ["IN_GR", "IN_THRESHOLD"]);
    assert!(
        super::effect_module(body)
            .contains("const IN_GR: u32 = 0u;\nconst IN_THRESHOLD: u32 = 1u;")
    );
    let inputs = [
        doc::ShaderInput::param("threshold", "c/t"),
        doc::ShaderInput::stream("gr", "c/gr"),
    ];
    let data = BTreeMap::from([
        ("gr".to_string(), vec![0.1, 0.2]),
        ("threshold".to_string(), vec![-18.0]),
    ]);
    let (table, values) = super::pack_inputs(&super::effect_inputs(body), &inputs, Some(&data));
    assert_eq!(table[0], [0, 2, 2, 1]);
    assert_eq!(values, [0.1, 0.2, -18.0]);
}

#[test]
fn generators_are_the_effects_that_never_read_their_source() {
    let body = |id| super::builtin_source(id).unwrap();
    assert!(super::generates(body("knob.3d")) && super::generates(body("viz.comp")));
    assert!(!super::generates(body("glass")) && !super::generates(body("lighting")));
}

#[test]
fn const_inputs_need_no_host() {
    let body = "fn effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(input_or(IN_LOOK, 0.0) + input_or(IN_LIVE, 0.0)); }";
    let inputs = [
        doc::ShaderInput::constant("look", vec![0.25, 0.5]).map(0.0, 0.5),
        doc::ShaderInput::param("live", "a/b"),
    ];
    let (table, values) = super::pack_inputs(&super::effect_inputs(body), &inputs, None);
    // `live` (slot 0) has no host data; `look` (slot 1) comes from the document, shaped.
    assert_eq!(table[0], [0, 0, 0, 2]);
    assert_eq!(values, [0.5, 1.0]);
    // A host's values still win.
    let data = BTreeMap::from([("look".to_string(), vec![9.0])]);
    assert_eq!(
        super::pack_inputs(&super::effect_inputs(body), &inputs, Some(&data)).1,
        [9.0]
    );
}

/// A 60 × 60 `knob.3d` at 20..80 over a grey panel, at `value` (fed as the
/// `value` input: no control needed).
fn knob_3d(value: f64, params: &[(&str, f64)]) -> (Scene, doc::NodeId) {
    let panel = shape(NodeKind::Rectangle, 0.0, 0.0, 100.0, 100.0, (0.6, 0.6, 0.6));
    let mut knob = Node::new(NodeKind::Frame, "cap");
    knob.transform = doc::Transform::translate(20.0, 20.0);
    knob.size = doc::Size {
        width: 60.0,
        height: 60.0,
    };
    let params = params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
    knob.shader = Some(doc::ShaderFx {
        shader: "knob.3d".into(),
        params,
        inputs: vec![doc::ShaderInput::constant("value", vec![value])],
    });
    scene_of(vec![(None, panel), (None, knob)])
}

#[test]
fn knob_3d_draws_a_knob_and_its_shadow_inside_its_box() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = knob_3d(0.5, &[("tint_r", 0.1), ("tint_g", 0.1), ("tint_b", 0.1)]);
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if !((20..80).contains(&x) && (20..80).contains(&y)) {
                assert_eq!(
                    px(&off, x, y),
                    px(&gpu, x, y),
                    "changed ({x},{y}) outside its box"
                );
            }
        }
    }
    let (c, p) = (px(&gpu, 50, 45), px(&off, 50, 45));
    assert!(
        c[0] + 40 < p[0],
        "a dark cap in the middle: {c:?} over {p:?}"
    );
    assert_eq!(
        px(&gpu, 21, 78),
        px(&off, 21, 78),
        "the box's far corner stays clear"
    );
    let (s, p) = (px(&gpu, 62, 63), px(&off, 62, 63));
    assert!(
        s[0] + 10 < p[0],
        "a shadow down and right of the knob: {s:?} over {p:?}"
    );
}

#[test]
fn knob_3d_turns_its_ribs_under_a_fixed_light() {
    let Some(mut o) = offscreen() else { return };
    let look = [
        ("p0", 2.0),
        ("p1", 8.0),
        ("tint_r", 0.8),
        ("tint_g", 0.8),
        ("tint_b", 0.78),
    ];
    let (low, low_root) = knob_3d(0.0, &look);
    let (high, high_root) = knob_3d(1.0, &look);
    let (a, b) = (
        render(&mut o, &low, low_root, Mode::Gpu),
        render(&mut o, &high, high_root, Mode::Gpu),
    );
    let changed = (0..SIZE * SIZE)
        .filter(|i| px(&a, i % SIZE, i / SIZE) != px(&b, i % SIZE, i / SIZE))
        .count();
    assert!(
        changed > 150,
        "the flutes and pointer turn: {changed} px changed"
    );
    // The top's upper-left, clear of the pointer at either end: lit alike.
    let (p, q) = (px(&a, 45, 38), px(&b, 45, 38));
    assert!(
        p.iter().zip(q).all(|(x, y)| x.abs_diff(y) <= 2),
        "the light stays put: {p:?} vs {q:?}"
    );
}

/// GPU cost of `knob.3d` alone: 30 knobs in 97 × 97-unit boxes (a 64 px
/// knob's) at 2×, the effect passes timed against a trivial effect over the
/// same boxes.
/// `cargo test -p render --release --lib knob_3d_cost -- --ignored --nocapture`
#[test]
#[ignore = "timing"]
fn knob_3d_cost() {
    use super::passes::{FxUniforms, Passes, TARGET};
    use vello::wgpu;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        return;
    };
    // GPU timestamps round the passes, where the adapter has them (else
    // wall-clock time round submit + wait).
    let stamps = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let stamps = adapter.features().contains(stamps).then_some(stamps);
    let desc = wgpu::DeviceDescriptor {
        required_features: stamps.unwrap_or_default(),
        ..Default::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&desc)).expect("device");
    eprintln!(
        "adapter: {:?}, timestamps: {}",
        adapter.get_info().name,
        stamps.is_some()
    );
    let queries = stamps.map(|_| {
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: None,
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let usage = wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage,
            mapped_at_creation: false,
        });
        let usage = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST;
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage,
            mapped_at_creation: false,
        });
        (set, resolve, read)
    });
    let (side, n) = (97.0_f32, 30_u32);
    let (w, h) = (6 * 200, 5 * 200);
    let tex = |label| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let (src, dst) = (
        tex("src").create_view(&Default::default()),
        tex("dst").create_view(&Default::default()),
    );
    let mut passes = Passes::new(&device);
    let mut scene = Scene::new("Page");
    let trivial = scene.add_asset(
        "text/wgsl",
        b"fn effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(uv * 0.5, 0.0, 0.5); }".to_vec(),
    );
    let trivial = format!("asset:{}", trivial.0);
    let inputs = [
        doc::ShaderInput::constant("shape", vec![0.64, 0.6, 0.0, 0.05]),
        doc::ShaderInput::constant("insert", vec![0.9, 0.9, 0.9, 1.0, 0.12]),
    ];
    let mut time = |id: &str, scale: f32| -> f64 {
        passes.effect(&device, &scene, id).expect("compiles");
        let slots = passes.effect_slots(id).to_vec();
        let pipeline = passes.cached_effect(id).unwrap();
        // Long enough a warm-up for the clocks to ramp up.
        let (warm, frames) = (1500, 500);
        let mut total = 0.0;
        for f in 0..frames + warm {
            let mut enc = device.create_command_encoder(&Default::default());
            if let Some((set, _, _)) = &queries {
                enc.write_timestamp(set, 0);
            }
            for i in 0..n {
                // The profiles in turn: insert, chicken head, knurl, flutes, …
                let params = BTreeMap::from([
                    (
                        "p0".to_string(),
                        f64::from([8_u8, 4, 3, 2, 1, 6, 5, 7, 0][i as usize % 9]),
                    ),
                    ("p2".to_string(), f64::from(i % 4)),
                    ("p1".to_string(), if i % 9 == 2 { 64.0 } else { 0.0 }),
                ]);
                let (values, param_mask) = super::pack_params(&params);
                let (x0, y0) = ((i % 6) as f32 * 200.0, ((i / 6) % 5) as f32 * 200.0);
                let (table, data) = super::pack_inputs(&slots, &inputs, None);
                let u = FxUniforms {
                    time_s: 0.0,
                    value: (f as f32 * 0.01 + i as f32 * 0.03) % 1.0,
                    px_per_unit: scale,
                    param_mask,
                    resolution: [w as f32, h as f32],
                    node_size: [side, side],
                    to_local: [
                        [1.0 / scale, 0.0, -x0 / scale, 0.0],
                        [0.0, 1.0 / scale, -y0 / scale, 0.0],
                    ],
                    to_px: [[scale, 0.0, x0, 0.0], [0.0, scale, y0, 0.0]],
                    params: values,
                    inputs: table,
                };
                let px = (side * scale) as u32;
                passes.effect_pass(
                    &device,
                    &mut enc,
                    pipeline,
                    &u,
                    &data,
                    &src,
                    &dst,
                    Some([x0 as u32, y0 as u32, px, px]),
                );
            }
            if let Some((set, resolve, read)) = &queries {
                enc.write_timestamp(set, 1);
                enc.resolve_query_set(set, 0..2, resolve, 0);
                enc.copy_buffer_to_buffer(resolve, 0, read, 0, 16);
            }
            let t0 = std::time::Instant::now();
            queue.submit([enc.finish()]);
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let mut elapsed = t0.elapsed().as_secs_f64();
            if let Some((_, _, read)) = &queries {
                read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                let ticks: Vec<u64> = read
                    .slice(..)
                    .get_mapped_range()
                    .chunks(8)
                    .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                    .collect();
                read.unmap();
                elapsed = ticks[1].saturating_sub(ticks[0]) as f64
                    * f64::from(queue.get_timestamp_period())
                    * 1e-9;
            }
            if f >= warm {
                total += elapsed;
            }
        }
        total / f64::from(frames) * 1000.0
    };
    for scale in [1.0, 2.0] {
        let base = time(&trivial, scale);
        let knob = time("knob.3d", scale);
        let px = side * scale;
        eprintln!(
            "30 knobs at {scale}x ({px} px boxes): knob.3d {knob:.3} ms/frame, trivial effect {base:.3} ms, shader cost ~ {:.3} ms",
            knob - base
        );
    }

    // The whole stage (wall clock, read-back included): each shader region
    // also renders its subtree alone and composites it.
    let Some(mut o) = offscreen() else { return };
    let mut nodes = vec![(
        None,
        shape(NodeKind::Rectangle, 0.0, 0.0, 600.0, 500.0, (0.6, 0.6, 0.6)),
    )];
    for i in 0..30_u32 {
        let mut knob = Node::new(NodeKind::Frame, "cap");
        knob.transform =
            doc::Transform::translate(f64::from(i % 6) * 100.0, f64::from(i / 6) * 100.0);
        knob.size = doc::Size {
            width: 97.0,
            height: 97.0,
        };
        let params = BTreeMap::from([("p0".to_string(), f64::from(i % 9))]);
        knob.shader = Some(doc::ShaderFx {
            shader: "knob.3d".into(),
            params,
            inputs: Vec::new(),
        });
        nodes.push((None, knob));
        nodes.push((
            Some(nodes.len() - 1),
            shape(NodeKind::Ellipse, 16.0, 16.0, 64.0, 64.0, (0.1, 0.1, 0.1)),
        ));
    }
    let (scene, root) = scene_of(nodes);
    for mode in [Mode::Off, Mode::Gpu] {
        let frame = Frame {
            mode,
            transform: kurbo::Affine::scale(2.0),
            ..Frame::new(&scene, root, 1200, 1000)
        };
        let _ = o.render_with_effects(&frame);
        let t0 = std::time::Instant::now();
        for _ in 0..20 {
            let _ = o.render_with_effects(&frame);
        }
        eprintln!(
            "stage, 30 knob.3d regions at 2x (1200 x 1000), {mode:?}: {:.2} ms/frame",
            t0.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
    }
}

/// The stage keeps its renders between frames and re-renders only what
/// changed (a run of primitives; a faded layer by fading it): frame after
/// frame, a pulsing lamp under a shape must look exactly as a fresh
/// render of the same frame does.
#[test]
fn kept_renders_match_fresh_ones_frame_after_frame() {
    let Some(mut kept) = offscreen() else { return };
    let bg = shape(
        NodeKind::Rectangle,
        0.0,
        0.0,
        100.0,
        100.0,
        (0.1, 0.1, 0.12),
    );
    let mut lamp = shape(NodeKind::Ellipse, 30.0, 30.0, 30.0, 30.0, (0.2, 1.0, 0.6));
    lamp.emission = Some(doc::Emission {
        color: doc::Color::rgba(0.2, 1.0, 0.6, 1.0),
        intensity: 1.0,
        radius: 10.0,
    });
    let needle = shape(NodeKind::Rectangle, 20.0, 20.0, 6.0, 60.0, (0.9, 0.9, 0.9));
    let cover = shape(NodeKind::Rectangle, 45.0, 45.0, 30.0, 30.0, (0.8, 0.2, 0.2));
    // A lamp of parts (a frame, faded as a whole): a layer.
    let mut glow = shape(NodeKind::Frame, 60.0, 10.0, 30.0, 30.0, (0.1, 0.3, 1.0));
    glow.clips_content = false;
    let bulb = shape(NodeKind::Ellipse, 5.0, 5.0, 20.0, 20.0, (0.9, 0.9, 0.2));
    let spark = shape(NodeKind::Rectangle, 12.0, 0.0, 6.0, 30.0, (1.0, 1.0, 1.0));
    let (lamp_id, needle_id, glow_id) = (lamp.id, needle.id, glow.id);
    let (mut scene, root) = scene_of(vec![
        (None, bg),
        (None, lamp),
        (None, needle),
        (None, glow),
        (Some(3), bulb),
        (Some(3), spark),
        (None, cover),
    ]);
    for (alpha, x) in [
        (0.2, 20.0),
        (0.9, 20.0),
        (0.5, 20.0),
        (0.5, 60.0),
        (0.35, 70.0),
        (1.0, 70.0),
        (0.6, 70.0),
    ] {
        scene.node_mut(lamp_id).unwrap().opacity = alpha;
        scene.node_mut(glow_id).unwrap().opacity = 1.0 - alpha * 0.5;
        scene.node_mut(needle_id).unwrap().transform = doc::Transform::translate(x, 20.0);
        let got = render(&mut kept, &scene, root, Mode::Gpu);
        let mut fresh = offscreen().expect("an adapter a moment ago");
        let want = render(&mut fresh, &scene, root, Mode::Gpu);
        let worst = got
            .rgba
            .iter()
            .zip(&want.rgba)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 2,
            "alpha {alpha}, needle at {x}: kept render differs by {worst}"
        );
    }
    // Only the layer fading: rendered once, faded as it's laid over.
    for fade in [0.3, 0.8, 0.55, 1.0, 0.1] {
        scene.node_mut(glow_id).unwrap().opacity = fade;
        let got = render(&mut kept, &scene, root, Mode::Gpu);
        let mut fresh = offscreen().expect("an adapter a moment ago");
        let want = render(&mut fresh, &scene, root, Mode::Gpu);
        let worst = got
            .rgba
            .iter()
            .zip(&want.rgba)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 2,
            "layer at {fade}: kept render differs by {worst}"
        );
    }
}

/// Stages made from one `SharedPasses` share their compiled effects: one
/// compiles, the rest find it compiled — and a stage made on its own does
/// not see them.
#[test]
fn stages_on_shared_passes_compile_an_effect_once() {
    use super::passes::Passes;
    use vello::wgpu;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        return;
    };
    let Ok((device, _queue)) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
    else {
        return;
    };
    let (scene, _) = shaded("glass");
    let mut first = Passes::new(&device);
    let second = first.clone();
    let alone = Passes::new(&device);
    assert!(
        first.effect(&device, &scene, "glass").is_some(),
        "glass compiles"
    );
    assert!(
        second.has_compiled("glass"),
        "compiled by the first, known to the second"
    );
    assert!(
        !alone.has_compiled("glass"),
        "a stage of its own compiles its own"
    );
}
