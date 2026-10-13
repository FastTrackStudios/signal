//! The effects stage: optional light (`emission`) and shader (`shader`)
//! effects on top of the vector renderer.
//!
//! - [`Mode::Gpu`] — the scene renders as usual; emissive nodes render again in
//!   their emission colour at half resolution, are Gaussian-blurred (with
//!   downsampling for large radii) and added over the frame (bloom). Shader
//!   nodes render their subtree to its own texture, run a WGSL effect over it
//!   (built-ins in `wgsl/`, or `asset:<id>` custom WGSL — see `CONTRACT.md`)
//!   and composite back in paint order.
//! - [`Mode::Fallback`] — no GPU passes; emission draws as a soft glow in the
//!   vector path ([`EffectKind::Glow`](crate::render::anyrender_renderer::EffectKind)),
//!   shaders are skipped. The SVG exporter always does this.
//! - [`Mode::Off`] — neither.

mod passes;
mod stage;

use std::collections::{BTreeMap, HashMap};

pub use crate::render::anyrender_renderer::{
    EffectsPlan, EmissiveDraw, ShaderRegion, flicker_level,
};
pub use stage::{Check, Frame, SharedPasses, Stage};

/// Live shader data for one frame: per shader node, each input's values by
/// input name (see `crate::document::proto::ShaderInput`; hosts resolve them —
/// `crate::surface::runtime::Runtime::shader_inputs`).
pub type FxInputs = HashMap<crate::document::proto::NodeId, BTreeMap<String, Vec<f32>>>;

/// The most floats one shader region uploads per frame (16 KiB).
pub const MAX_DATA: usize = 4096;

/// How a renderer treats `emission`/`shader`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// GPU bloom + WGSL shaders.
    #[default]
    Gpu,
    /// Soft vector glows for emission; shaders skipped.
    Fallback,
    /// Neither.
    Off,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gpu" => Ok(Self::Gpu),
            "fallback" => Ok(Self::Fallback),
            "off" => Ok(Self::Off),
            other => Err(format!("unknown fx mode `{other}` (gpu|fallback|off)")),
        }
    }
}

/// The 16 named shader params, in uniform-slot order (`params[i / 4][i % 4]`
/// in WGSL, with `P_<NAME>` constants). Names not listed are ignored.
pub const PARAM_SLOTS: [&str; 16] = [
    "strength",
    "speed",
    "amount",
    "scale",
    "angle",
    "elevation",
    "softness",
    "radius",
    "tint_r",
    "tint_g",
    "tint_b",
    "tint_a",
    "p0",
    "p1",
    "p2",
    "p3",
];

/// The built-in effects, by id: post-effects over a subtree, then the
/// visualizers (which draw their own picture from live inputs and replace
/// the subtree, their vector fallback — see `CONTRACT.md`).
pub const BUILTINS: [&str; 10] = [
    "glass",
    "crt",
    "lighting",
    "flicker",
    "viz.comp",
    "viz.delay",
    "viz.reverb",
    "viz.modulation",
    "eq.glow",
    "knob.3d",
];

/// The slot of a param name, if it has one.
#[must_use]
pub fn param_slot(name: &str) -> Option<usize> {
    PARAM_SLOTS.iter().position(|n| *n == name)
}

/// Pack a node's params into the uniform layout: 16 floats and a bit mask of
/// the slots the document set.
#[must_use]
pub fn pack_params(params: &BTreeMap<String, f64>) -> ([f32; 16], u32) {
    let mut values = [0.0_f32; 16];
    let mut mask = 0_u32;
    for (name, v) in params {
        if let Some(i) = param_slot(name) {
            values[i] = *v as f32;
            mask |= 1 << i;
        }
    }
    (values, mask)
}

/// The WGSL body of a built-in effect.
#[must_use]
pub fn builtin_source(id: &str) -> Option<&'static str> {
    Some(match id {
        "glass" => include_str!("wgsl/glass.wgsl"),
        "crt" => include_str!("wgsl/crt.wgsl"),
        "lighting" => include_str!("wgsl/lighting.wgsl"),
        "flicker" => include_str!("wgsl/flicker.wgsl"),
        "viz.comp" => concat!(
            include_str!("wgsl/viz/common.wgsl"),
            include_str!("wgsl/viz/comp.wgsl")
        ),
        "viz.delay" => concat!(
            include_str!("wgsl/viz/common.wgsl"),
            include_str!("wgsl/viz/delay.wgsl")
        ),
        "viz.reverb" => concat!(
            include_str!("wgsl/viz/common.wgsl"),
            include_str!("wgsl/viz/reverb.wgsl")
        ),
        "viz.modulation" => concat!(
            include_str!("wgsl/viz/common.wgsl"),
            include_str!("wgsl/viz/modulation.wgsl")
        ),
        "eq.glow" => concat!(
            include_str!("wgsl/viz/common.wgsl"),
            include_str!("wgsl/viz/eq_glow.wgsl")
        ),
        "knob.3d" => include_str!("wgsl/knob3d.wgsl"),
        _ => return None,
    })
}

/// Whether an effect body draws its own picture without reading its source
/// (no `sample_src*`, no `src_tex`): the stage then skips rendering the
/// node's subtree for it (see `CONTRACT.md`, "Generators").
#[must_use]
pub fn generates(body: &str) -> bool {
    !body.contains("sample_src") && !body.contains("src_tex")
}

/// The inputs an effect body reads: every `IN_<NAME>` it mentions, sorted,
/// at most 16 — slot `i` of the uniform table is the `i`-th.
#[must_use]
pub fn effect_inputs(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut names = std::collections::BTreeSet::new();
    let mut i = 0;
    while let Some(at) = body[i..].find("IN_") {
        let start = i + at;
        let mut end = start + 3;
        while end < bytes.len() && ident(bytes[end]) {
            end += 1;
        }
        if (start == 0 || !ident(bytes[start - 1])) && end > start + 3 {
            names.insert(body[start..end].to_string());
        }
        i = end;
    }
    names.into_iter().take(16).collect()
}

/// The effect body for `id` in `doc`: a built-in, or the UTF-8 WGSL of the
/// asset an `asset:<id>` names.
#[must_use]
pub fn effect_source(doc: &crate::document::proto::Scene, id: &str) -> Option<String> {
    if let Some(asset) = id.strip_prefix("asset:") {
        let a = doc.asset(&crate::document::proto::AssetId(asset.to_string()))?;
        return String::from_utf8(a.bytes.clone()).ok();
    }
    builtin_source(id).map(str::to_string)
}

/// A complete effect module: the contract prelude, the effect, the entry
/// points.
#[must_use]
pub fn effect_module(body: &str) -> String {
    use std::fmt::Write as _;
    let mut consts = String::new();
    for (i, name) in effect_inputs(body).iter().enumerate() {
        let _ = writeln!(consts, "const {name}: u32 = {i}u;");
    }
    format!(
        "{}\n{consts}\n{body}\n{}",
        include_str!("wgsl/prelude.wgsl"),
        include_str!("wgsl/entry.wgsl")
    )
}

/// Pack a region's live inputs: the slot table (`(offset, len)` per slot,
/// in [`effect_inputs`] order) and the values back to back, at most
/// [`MAX_DATA`] floats (later inputs are cut short, then empty). A `Const`
/// input the host didn't resolve is taken from the document.
#[must_use]
pub fn pack_inputs(
    slots: &[String],
    inputs: &[crate::document::proto::ShaderInput],
    data: Option<&BTreeMap<String, Vec<f32>>>,
) -> ([[u32; 4]; 8], Vec<f32>) {
    let mut table = [[0_u32; 4]; 8];
    let mut values = Vec::new();
    for (i, slot) in slots.iter().enumerate().take(16) {
        let Some(input) = inputs.iter().find(|n| n.constant_name() == *slot) else {
            continue;
        };
        // A `Const` input needs no host: its values are in the document.
        let constant;
        let v = match (data.and_then(|d| d.get(&input.name)), &input.source) {
            (Some(v), _) => v,
            (None, crate::document::proto::ShaderInputSource::Const { values }) => {
                constant = input.shape(values);
                &constant
            }
            (None, _) => continue,
        };
        let take = v.len().min(MAX_DATA - values.len());
        let offset = values.len() as u32;
        values.extend_from_slice(&v[..take]);
        table[i / 2][(i % 2) * 2] = offset;
        table[i / 2][(i % 2) * 2 + 1] = take as u32;
    }
    (table, values)
}

/// Whether `doc` under `root` needs continuous redraws with effects on (a
/// time-dependent shader or flickering light).
#[must_use]
pub fn time_dependent(
    doc: &crate::document::proto::Scene,
    root: crate::document::proto::NodeId,
) -> bool {
    let (_, plan) = crate::render::build_scene_primitives_and_effects(doc, root);
    plan.time_dependent(|id| effect_source(doc, id))
}

#[cfg(test)]
mod tests;
