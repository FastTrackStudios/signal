//! The compressors' materials and light on the GPU effects stage: every
//! comp face gets, after it is built,
//!
//! - **plate light** — a light layer over the faceplate's paint, under its
//!   parts (`comp_look/plate.wgsl`): brushed plates show the key light as
//!   an upright anisotropic band streaked by the grain, painted ones a
//!   satin lobe and a softbox reflection; lit top edge, darker foot;
//! - **knob light** on every knob (`comp_look/knob.wgsl`): a bevel round
//!   its edges, a domed top, a specular hotspot and fill glint under the
//!   kit's key light, and the radial streak of a spun-aluminium cap;
//! - **meter glass** over every movement's card in place of the built-in
//!   `glass` (`comp_look/glass.wgsl`): a curved window reflecting a
//!   softbox, a rim and a glint, and a warm lamp behind backlit cards.
//!
//! This is light, not layout: no node moves, no control or address
//! changes. The plate light is an empty frame (renderers without the
//! stage draw nothing for it); the knobs and meters keep their vector art,
//! which those renderers draw as before. None of the shaders reads the
//! clock, so a face renders once and is re-composited until a value moves.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Scene, ShaderFx};
use surface_proto::ControlKind;

const PLATE: &str = include_str!("comp_look/plate.wgsl");
const KNOB: &str = include_str!("comp_look/knob.wgsl");
const GLASS: &str = include_str!("comp_look/glass.wgsl");

/// How a face's plate, knobs and meters take the light.
#[derive(Clone, Copy)]
struct Look {
    /// Plate: brushed (1) or painted (0); light strength; roughness;
    /// the band's centre; grain contrast; the light's tint.
    brushed: f64,
    plate: f64,
    rough: f64,
    band: f64,
    grain: f64,
    tint: [f64; 3],
    /// Knobs: light strength, dome, roughness, spun cap radius (body radii).
    knob: f64,
    dome: f64,
    knob_rough: f64,
    spun: f64,
    /// Meters: reflection strength and backlight.
    glass: f64,
    backlight: f64,
}

const COOL: [f64; 3] = [0.93, 0.96, 1.0];
const NEUTRAL: [f64; 3] = [1.0, 0.99, 0.97];

/// The look for a comp face, by its file (`01-fet-black-tall` …).
fn look(file: &str) -> Look {
    let base = Look {
        brushed: 0.0,
        plate: 0.4,
        rough: 0.6,
        band: 0.3,
        grain: 1.0,
        tint: NEUTRAL,
        knob: 0.8,
        dome: 0.35,
        knob_rough: 0.35,
        spun: 0.0,
        glass: 0.7,
        backlight: 0.5,
    };
    match file.get(..2).unwrap_or("") {
        // Black anodised aluminium: the band dimmer and cooler on it.
        "01" => Look { brushed: 1.0, plate: 0.32, band: 0.28, tint: COOL, dome: 0.12, spun: 0.78, knob_rough: 0.25, ..base },
        // Clear-anodised, brushed.
        "02" | "03" => Look { brushed: 1.0, plate: 0.42, band: 0.28, tint: COOL, dome: 0.12, spun: 0.78, knob_rough: 0.25, ..base },
        // The leveler's light grey enamel, satin.
        "04" => Look { plate: 0.34, rough: 0.55, dome: 0.5, knob_rough: 0.22, backlight: 0.55, ..base },
        // Blue enamel.
        "05" => Look { plate: 0.36, rough: 0.5, knob_rough: 0.3, ..base },
        // Grey cabinet, black plate; the movement isn't lit.
        "06" => Look { plate: 0.34, rough: 0.5, knob_rough: 0.28, backlight: 0.0, ..base },
        // Brushed upper plate.
        "07" => Look { brushed: 1.0, plate: 0.36, band: 0.5, tint: COOL, dome: 0.3, ..base },
        // Painted grey, the meter amber-lit.
        "08" => Look { plate: 0.34, rough: 0.55, dome: 0.15, knob_rough: 0.3, backlight: 0.45, ..base },
        // Gloss black in a dark anodised chassis; the meter lamp warm.
        "09" => Look { plate: 0.36, rough: 0.25, dome: 0.15, spun: 0.48, knob_rough: 0.25, glass: 0.8, backlight: 0.55, ..base },
        // Satin black.
        _ => Look { plate: 0.32, rough: 0.45, dome: 0.3, knob_rough: 0.3, ..base },
    }
}

fn fx(shader: &str, params: &[(&str, f64)]) -> ShaderFx {
    let params: BTreeMap<String, f64> = params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
    ShaderFx { shader: shader.into(), params, inputs: Vec::new() }
}

/// Light `scene` (a comp face built from `file`).
pub fn apply(scene: &mut Scene, file: &str) {
    light(scene, &look(file));
}

/// The look for a redrawn saturator face, by its file (`03-transformer`
/// …): `None` for those still drawn without the stage.
fn saturator_look(file: &str) -> Option<Look> {
    let base = Look {
        brushed: 0.0,
        plate: 0.36,
        rough: 0.5,
        band: 0.3,
        grain: 1.0,
        tint: NEUTRAL,
        knob: 0.8,
        dome: 0.35,
        knob_rough: 0.3,
        spun: 0.0,
        glass: 0.7,
        backlight: 0.55,
    };
    match file.get(..3).unwrap_or("") {
        // Pultec blue-grey enamel, satin; the VU lamp-lit.
        "03-" => Some(Look { plate: 0.34, rough: 0.5, ..base }),
        // The deck's warm grey enamel; the VUs lamp-lit.
        "02-" => Some(Look { plate: 0.32, rough: 0.55, backlight: 0.6, ..base }),
        _ => None,
    }
}

/// Light a redrawn saturator face (`file`) as the compressors are lit,
/// and glaze its window into the unit (`Chassis window`, `Deck window`):
/// the same curved glass as the meters, unlit from behind.
pub fn apply_saturator(scene: &mut Scene, file: &str) {
    let Some(l) = saturator_look(file) else { return };
    light(scene, &l);
    let glass_id = format!("asset:{}", scene.add_asset("text/wgsl", GLASS.as_bytes().to_vec()).0);
    let windows: Vec<NodeId> = scene.walk(scene.root).into_iter().filter(|id| scene.node(*id).is_some_and(|n| matches!(n.name.as_str(), "Chassis window" | "Deck window"))).collect();
    for id in windows {
        if let Some(n) = scene.node_mut(id) {
            n.shader = Some(fx(&glass_id, &[("strength", 0.42), ("p0", 0.0), ("p1", 0.4)]));
        }
    }
    // Tape reels: the knob light on their spun flanges, from a frame that
    // stays put while the flange turns under it (`reel lit`, the reel's
    // radius 5 in from its sides).
    let reels: Vec<NodeId> = scene.walk(scene.root).into_iter().filter(|id| scene.node(*id).is_some_and(|n| n.name == "reel lit")).collect();
    if reels.is_empty() {
        return;
    }
    let knob_id = format!("asset:{}", scene.add_asset("text/wgsl", KNOB.as_bytes().to_vec()).0);
    for id in reels {
        if let Some(n) = scene.node_mut(id) {
            let side = n.size.width.max(1.0);
            let radius = ((side - 10.0) / 2.0 / side).clamp(0.05, 0.5);
            n.shader = Some(fx(&knob_id, &[("strength", 0.75), ("radius", radius), ("p0", 0.0), ("p1", 0.97), ("p2", 1.02), ("softness", 0.22)]));
        }
    }
}

fn light(scene: &mut Scene, l: &Look) {
    let l = *l;
    let plate_id = format!("asset:{}", scene.add_asset("text/wgsl", PLATE.as_bytes().to_vec()).0);
    let knob_id = format!("asset:{}", scene.add_asset("text/wgsl", KNOB.as_bytes().to_vec()).0);
    let glass_id = format!("asset:{}", scene.add_asset("text/wgsl", GLASS.as_bytes().to_vec()).0);

    let page = scene.pages()[0];
    let Some(top) = scene.node(page).and_then(|p| p.children.first().copied()) else { return };
    let Some(plate) = scene.node(top).and_then(|t| t.children.first().copied()) else { return };
    plate_light(scene, plate, &plate_id, &l);

    for id in scene.walk(top) {
        let Some(n) = scene.node(id) else { continue };
        if n.shader.as_ref().is_some_and(|s| s.shader == "glass") {
            let s = fx(&glass_id, &[("strength", l.glass), ("p0", l.backlight)]);
            if let Some(n) = scene.node_mut(id) {
                n.shader = Some(s);
            }
            continue;
        }
        if !n.control.as_ref().is_some_and(|c| matches!(c.kind, ControlKind::Knob { .. })) {
            continue;
        }
        knob_light(scene, id, &knob_id, &l);
    }
}

/// The light layer over `plate`'s paint: after the paint's own finish
/// (grain, sheen, gloss, printed fields, an inner plate), under the rest —
/// the silkscreen, the hardware and the parts.
fn plate_light(scene: &mut Scene, plate: NodeId, shader: &str, l: &Look) {
    let Some(p) = scene.node(plate) else { return };
    let (w, h) = (p.size.width, p.size.height);
    let radius = p.corner_radii.top_left;
    let at = p
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            scene.node(**c).is_some_and(|c| {
                matches!(c.kind, NodeKind::Rectangle | NodeKind::Vector)
                    && matches!(c.name.as_str(), "grain" | "smear" | "sheen" | "plate" | "gloss" | "left field" | "right field" | "top field" | "bottom field")
            })
        })
        .map(|(i, _)| i + 1)
        .max()
        .unwrap_or(0);
    let mut n = Node::new(NodeKind::Frame, "plate light");
    n.size = document_proto::Size { width: w, height: h };
    n.layout_child.absolute = true;
    n.parent = Some(plate);
    n.shader = Some(fx(
        shader,
        &[
            ("strength", l.plate),
            ("p0", l.brushed),
            ("softness", l.rough),
            ("p1", l.band),
            ("p2", l.grain),
            ("radius", radius),
            ("tint_r", l.tint[0]),
            ("tint_g", l.tint[1]),
            ("tint_b", l.tint[2]),
        ],
    ));
    let id = n.id;
    scene.nodes.insert(id, n);
    if let Some(p) = scene.node_mut(plate) {
        p.children.insert(at, id);
    }
}

/// Knob light on the knob control `root`'s drawn knob (its `knob` frame —
/// square, on the shaft — whose `body shadow` is the body's footprint).
fn knob_light(scene: &mut Scene, root: NodeId, shader: &str, l: &Look) {
    let Some(r) = scene.node(root) else { return };
    let Some(knob) = r.children.iter().copied().find(|c| scene.node(*c).is_some_and(|n| n.name == "knob")) else { return };
    let Some(k) = scene.node(knob) else { return };
    // A knob the stage already draws (`knob.3d`) is lit by its own shader.
    if scene.walk(knob).into_iter().any(|c| scene.node(c).is_some_and(|n| n.shader.is_some())) {
        return;
    }
    let side = k.size.width.min(k.size.height).max(1.0);
    // The body's diameter: its shadow's, the collar's, else the control's.
    let body = k
        .children
        .iter()
        .filter_map(|c| scene.node(*c))
        .find(|c| matches!(c.name.as_str(), "body shadow" | "collar"))
        .map_or(r.size.width.min(r.size.height), |c| c.size.width);
    let radius = (body / 2.0 / side).clamp(0.05, 0.5);
    // Pointer knobs (tabs, fins, paddles) reach past the body.
    let reach = (0.5 / radius).min(1.45);
    let s = fx(
        shader,
        &[
            ("strength", l.knob),
            ("radius", radius),
            ("p0", l.dome),
            ("p1", l.spun),
            ("p2", reach),
            ("softness", l.knob_rough),
        ],
    );
    if let Some(k) = scene.node_mut(knob) {
        k.shader = Some(s);
    }
}
