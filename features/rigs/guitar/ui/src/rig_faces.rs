//! The rig's blocks drawn with frame faces.
//!
//! Signal decides what is loaded: presets, modules and overrides put a
//! pedal preset in each drive slot and a NAM capture in each amp. frame's
//! `rig-faces/faces.json` only says what those look like — a drive slot's
//! pedal preset name, or an amp's capture file name, matched against each
//! entry's words (case-insensitive, first entry wins). Nothing matched, the
//! block is a plain card with its name.
//!
//! A face binds to its block by convention: `<ns>/<param>` is the block's
//! param (`drive`, `threshold`, `b3_freq`…), `<ns>/level` its output level
//! (dB), `<ns>/on` its bypass (on = not bypassed), `<ns>/option` its
//! capture choice. `<ns>/tone` is the drive block's EQ, still to come.

use dioxus::prelude::*;
use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::rig::RigClient;
use signal_proto::BlockType;

use crate::frame_surface::FrameSurface;
use crate::param_writer::WriteParam;

/// One face in the manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceEntry {
    /// Its path under the faces directory, without `.fm`.
    pub face: String,
    /// The namespace its params are under.
    pub ns: String,
    /// Words that pick it (lower case).
    pub words: Vec<String>,
    /// Its size, for its box's proportion.
    pub size: (f64, f64),
}

/// `faces.json`, read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faces {
    pub drives: Vec<FaceEntry>,
    pub amps: Vec<FaceEntry>,
    pub gate: Option<FaceEntry>,
    pub eq: Option<FaceEntry>,
}

const SET: &str = "rig-faces";

fn entry(v: &serde_json::Value) -> Option<FaceEntry> {
    let face = v.get("face")?.as_str()?;
    let ns = v.get("ns")?.as_str()?.to_string();
    let words = v
        .get("match")
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|w| w.as_str().map(str::to_lowercase)).collect())
        .unwrap_or_default();
    let size = v
        .get("size")
        .and_then(|s| s.as_array())
        .and_then(|s| Some((s.first()?.as_f64()?, s.get(1)?.as_f64()?)))
        .unwrap_or((1.0, 1.0));
    Some(FaceEntry { face: format!("{SET}/{face}"), ns, words, size })
}

impl Faces {
    /// Read the manifest; none (or a bad one) is an empty set — every
    /// block a card.
    #[must_use]
    pub fn load() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = crate::frame_surface::design_dir().join(SET).join("faces.json");
            let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
                tracing::warn!(target: "frame", manifest = %path.display(), "faces.json does not parse");
                return Self::default();
            };
            let list = |k: &str| doc.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(entry).collect()).unwrap_or_default();
            Self { drives: list("drives"), amps: list("amps"), gate: doc.get("gate").and_then(entry), eq: doc.get("eq").and_then(entry) }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::default()
        }
    }

    /// The face for a drive slot's pedal preset.
    #[must_use]
    pub fn drive(&self, preset: &str) -> Option<&FaceEntry> {
        pick(&self.drives, preset)
    }

    /// The face for an amp's capture file.
    #[must_use]
    pub fn amp(&self, asset: &str) -> Option<&FaceEntry> {
        let file = asset.rsplit(['/', '\\']).next().unwrap_or(asset);
        pick(&self.amps, file)
    }
}

fn pick<'a>(list: &'a [FaceEntry], name: &str) -> Option<&'a FaceEntry> {
    let name = name.to_lowercase();
    if name.is_empty() {
        return None;
    }
    list.iter().find(|e| e.words.iter().any(|w| name.contains(w.as_str())))
}

/// The manifest, read once per mount (a regenerated set is picked up by
/// reopening the view).
pub fn use_faces() -> Faces {
    use_hook(Faces::load)
}

/// A block's values as its face's params, in their own units.
fn values_of(block: &LiveBlock, ns: &str) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = block.params.iter().map(|p| (format!("{ns}/{}", p.name), f64::from(p.value))).collect();
    if let Some(db) = block.output_level_db {
        v.push((format!("{ns}/level"), f64::from(db)));
    }
    v.push((format!("{ns}/on"), if block.bypassed { 0.0 } else { 1.0 }));
    v.push((format!("{ns}/option"), f64::from(block.option)));
    v
}

/// An edit on a face, sent to its block.
fn send(rig: Option<RigClient>, block: &LiveBlock, ns: &str, addr: &str, value: f64) {
    let Some(rig) = rig else { return };
    let Some(name) = addr.strip_prefix(ns).and_then(|s| s.strip_prefix('/')) else { return };
    let id = block.id.clone();
    match name {
        "on" => {
            spawn(async move {
                let _ = rig.set_block_bypass(id, value < 0.5).await;
            });
        }
        "level" => {
            spawn(async move {
                let _ = rig.set_block_level(id, value as f32, true).await;
            });
        }
        "option" => {
            let option = value.round().max(0.0) as u32;
            spawn(async move {
                let _ = rig.set_block_option(id, option).await;
            });
        }
        // The drive block's own EQ is still to come.
        "tone" => {}
        param if block.params.iter().any(|p| p.name == param) => {
            let _ = rig.write_param(id, param.to_string(), value as f32);
        }
        _ => {}
    }
}

/// A block drawn by its face: the box takes the face's proportion from
/// the row's height.
#[component]
pub fn BlockFace(block: LiveBlock, face: FaceEntry) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let values = values_of(&block, &face.ns);
    let (w, h) = face.size;
    let ns = face.ns.clone();
    let b = block.clone();
    rsx! {
        div { style: "height: 100%; aspect-ratio: {w} / {h}; flex: 0 1 auto; min-width: 0; min-height: 0;",
            FrameSurface {
                key: "{block.id}-{face.face}",
                name: face.face.clone(),
                values,
                on_edit: move |(addr, value): (String, f64)| send(rig.clone(), &b, &ns, &addr, value),
            }
        }
    }
}

/// A block with no face: a card with what it plays, bypass on a click.
/// `aspect` is the box it stands in for (a pedal's, an amp's).
#[component]
pub fn NameCard(block: LiveBlock, aspect: (f64, f64)) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let (w, h) = aspect;
    let title = if block.empty { "Empty".to_string() } else if block.preset.is_empty() { block.name.clone() } else { block.preset.clone() };
    let detail = if !block.detail.is_empty() {
        block.detail.clone()
    } else {
        block.asset.rsplit(['/', '\\']).next().unwrap_or_default().trim_end_matches(".nam").to_string()
    };
    let on = !block.bypassed && !block.empty;
    let id = block.id.clone();
    let empty = block.empty;
    let opacity = if empty { 0.45 } else { 1.0 };
    let lamp = if on { "#4ade80" } else { "#3a3d44" };
    rsx! {
        div {
            style: "height: 100%; aspect-ratio: {w} / {h}; flex: 0 1 auto; min-width: 0; min-height: 0; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 6px; padding: 10px; border-radius: 10px; border: 1px solid #2a2d33; background: #15171b; opacity: {opacity};",
            onclick: move |_| {
                if empty {
                    return;
                }
                let (rig, id) = (rig.clone(), id.clone());
                spawn(async move {
                    if let Some(r) = rig {
                        let _ = r.toggle_block_bypass(id).await;
                    }
                });
            },
            div { style: "width: 8px; height: 8px; border-radius: 4px; background: {lamp};" }
            div { style: "color: #e8e6e1; font-size: 13px; font-weight: 600; text-align: center; overflow: hidden;", "{title}" }
            if !detail.is_empty() {
                div { style: "color: #8a8d94; font-size: 10px; text-align: center; overflow: hidden;", "{detail}" }
            }
            div { style: "color: #5c5f66; font-size: 9px; letter-spacing: 1px; text-transform: uppercase;", "{block.name}" }
        }
    }
}

/// The height-to-width a pedal card stands in (a Standard pedal).
const PEDAL: (f64, f64) = (180.0, 286.0);

/// The Drives row: the drive board's slots in chain order, each its
/// pedal's face or a card.
#[component]
pub fn DrivesRow(blocks: Vec<LiveBlock>) -> Element {
    let faces = use_faces();
    let slots: Vec<LiveBlock> = blocks.iter().filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive)).cloned().collect();
    rsx! {
        div { style: "display: flex; flex-direction: row; justify-content: space-evenly; align-items: stretch; gap: 12px; width: 100%; height: 100%; min-height: 0; padding: 8px; box-sizing: border-box;",
            for b in slots {
                if let Some(f) = (!b.empty).then(|| faces.drive(&b.preset).cloned()).flatten() {
                    BlockFace { key: "{b.id}", block: b.clone(), face: f }
                } else {
                    NameCard { key: "{b.id}", block: b.clone(), aspect: PEDAL }
                }
            }
        }
    }
}

/// The height-to-width an amp card stands in.
const AMP: (f64, f64) = (792.0, 392.0);

/// The Amp row: every loaded amp, its face or a card; with one amp, the
/// Amp EQ's graph beside it; the gate's picture on the right.
#[component]
pub fn AmpRow(blocks: Vec<LiveBlock>) -> Element {
    let faces = use_faces();
    // Every amp the preset loaded: the first always, any other when it
    // plays something (a stereo module's Amp R).
    let amps: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Amp && !b.empty)
        .enumerate()
        .filter(|(i, b)| *i == 0 || !b.asset.is_empty())
        .map(|(_, b)| b.clone())
        .collect();
    let eq = blocks.iter().find(|b| b.block_type == BlockType::Eq && b.name.eq_ignore_ascii_case("Amp EQ")).cloned();
    let gate = blocks.iter().find(|b| b.block_type == BlockType::Gate).cloned();
    let one = amps.len() == 1;
    rsx! {
        div { style: "display: flex; flex-direction: row; justify-content: space-between; align-items: stretch; gap: 12px; width: 100%; height: 100%; min-height: 0; padding: 8px; box-sizing: border-box;",
            for b in amps {
                if let Some(f) = faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).cloned() {
                    BlockFace { key: "{b.id}", block: b.clone(), face: f }
                } else {
                    NameCard { key: "{b.id}", block: b.clone(), aspect: AMP }
                }
            }
            if one {
                if let (Some(e), Some(f)) = (eq, faces.eq.clone()) {
                    BlockFace { block: e, face: f }
                }
            }
            div { style: "flex: 1 1 0%;" }
            if let (Some(g), Some(f)) = (gate, faces.gate.clone()) {
                BlockFace { block: g, face: f }
            }
        }
    }
}
