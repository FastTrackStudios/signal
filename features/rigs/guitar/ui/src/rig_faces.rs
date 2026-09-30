//! The rig's blocks drawn with frame faces.
//!
//! Signal decides what is loaded: presets, modules and overrides put a
//! pedal preset in each drive slot and a NAM capture in each amp. frame's
//! `rig-faces/faces.json` only says what those look like — a drive slot's
//! pedal preset name, or an amp's capture file name, matched against each
//! entry's words (case-insensitive, first entry wins; `a+b` needs every
//! word). Nothing matched, the
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
    /// Other blocks the face also draws: (namespace, block name) — the
    /// Gravity Tank's `trem/` controls are the Pre Motion's.
    pub also: Vec<(String, String)>,
}

/// `faces.json`, read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faces {
    pub drives: Vec<FaceEntry>,
    pub amps: Vec<FaceEntry>,
    pub gate: Option<FaceEntry>,
    pub eq: Option<FaceEntry>,
    /// Pedals for the pre effects, by block preset name.
    pub pre: Vec<FaceEntry>,
    /// Pictures of what a block is doing, by its kind (`reverb`).
    pub pictures: Vec<FaceEntry>,
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
    let also = v
        .get("also")
        .and_then(|a| a.as_object())
        .map(|o| o.iter().filter_map(|(k, b)| Some((k.clone(), b.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    Some(FaceEntry { face: format!("{SET}/{face}"), ns, words, size, also })
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
            Self { drives: list("drives"), amps: list("amps"), gate: doc.get("gate").and_then(entry), eq: doc.get("eq").and_then(entry), pre: list("pre"), pictures: list("pictures") }
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

    /// The face for a pre effect's block preset.
    #[must_use]
    pub fn pre(&self, preset: &str) -> Option<&FaceEntry> {
        pick(&self.pre, preset)
    }

    /// The picture for a kind of block (`reverb`).
    #[must_use]
    pub fn picture(&self, kind: &str) -> Option<&FaceEntry> {
        pick(&self.pictures, kind)
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
    // `a+b` needs every word; otherwise any one.
    list.iter().find(|e| e.words.iter().any(|w| w.split('+').all(|part| name.contains(part.trim()))))
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
        // A param of the block's own first: a delay's `level` is its
        // wet's level, not the block's output.
        param if block.params.iter().any(|p| p.name == param) => {
            let _ = rig.write_param(id, param.to_string(), value as f32);
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
        _ => {}
    }
}

/// A block drawn by its face: the box takes the face's proportion from
/// the row's height.
///
/// `extra` are the other blocks it draws (its `also`), by namespace.
#[component]
pub fn BlockFace(block: LiveBlock, face: FaceEntry, #[props(default)] extra: Vec<(String, LiveBlock)>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut values = values_of(&block, &face.ns);
    for (ns, b) in &extra {
        values.extend(values_of(b, ns));
    }
    let (w, h) = face.size;
    let mut targets: Vec<(String, LiveBlock)> = vec![(face.ns.clone(), block.clone())];
    targets.extend(extra);
    rsx! {
        div { style: "height: 100%; max-width: 100%; aspect-ratio: {w} / {h}; flex: 0 1 auto; min-width: 0; min-height: 0;",
            FrameSurface {
                key: "{block.id}-{face.face}",
                name: face.face.clone(),
                values,
                on_edit: move |(addr, value): (String, f64)| {
                    if let Some((ns, b)) = targets.iter().find(|(ns, _)| addr.starts_with(&format!("{ns}/"))) {
                        send(rig.clone(), b, ns, &addr, value);
                    }
                },
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
            style: "height: 100%; width: 100%; min-width: 0; min-height: 0; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 6px; padding: 10px; box-sizing: border-box; opacity: {opacity};",
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
/// The height-to-width an amp card stands in.
const AMP: (f64, f64) = (792.0, 392.0);

/// A panel in a row: its own ground, flush against its neighbours (no
/// gaps, no padding), taking `weight` of the row's width — a face's
/// proportion, so the row is shared the way the faces would share it.
#[component]
fn Panel(weight: f64, children: Element) -> Element {
    rsx! {
        div {
            style: "flex: {weight} 1 0%; min-width: 0; height: 100%; min-height: 0; display: flex; flex-direction: row; align-items: stretch; justify-content: center; background: #121317; border-right: 1px solid #1d1f24; box-sizing: border-box; overflow: hidden;",
            {children}
        }
    }
}

/// A row of panels, flush, the full width and height it is given.
#[component]
fn Row(children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: row; align-items: stretch; width: 100%; height: 100%; min-height: 0; gap: 0; padding: 0;",
            {children}
        }
    }
}

fn ratio((w, h): (f64, f64)) -> f64 {
    if h > 0.0 { w / h } else { 1.0 }
}

/// A block in its panel: its face, or its card.
#[component]
fn BlockPanel(block: LiveBlock, face: Option<FaceEntry>, card: (f64, f64)) -> Element {
    let size = face.as_ref().map_or(card, |f| f.size);
    rsx! {
        Panel { weight: ratio(size),
            if let Some(f) = face {
                BlockFace { block, face: f }
            } else {
                NameCard { block, aspect: card }
            }
        }
    }
}

/// The Drives row: the drive board's slots in chain order, each its
/// pedal's face or a card, in panels sharing the row.
#[component]
pub fn DrivesRow(blocks: Vec<LiveBlock>) -> Element {
    let faces = use_faces();
    let slots: Vec<LiveBlock> = blocks.iter().filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive)).cloned().collect();
    rsx! {
        Row {
            for b in slots {
                BlockPanel { key: "{b.id}", face: (!b.empty).then(|| faces.drive(&b.preset).cloned()).flatten(), block: b.clone(), card: PEDAL }
            }
        }
    }
}

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
    // The Amp module's pick names the amp ("Vibroverb + Dumble · Clean").
    let amp_label = amps.first().map(|b| b.preset.clone()).unwrap_or_default();
    rsx! {
        Row {
            for b in amps {
                BlockPanel { key: "{b.id}", face: faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).cloned(), block: b.clone(), card: AMP }
            }
            if one {
                if let (Some(e), Some(f)) = (eq, faces.eq.clone()) {
                    BlockPanel { block: e, face: Some(f), card: AMP }
                }
            }
            // Kept for the Amp module's presets: what is loaded now.
            Panel { weight: 1.2,
                PresetsPanel { label: amp_label }
            }
            if let (Some(g), Some(f)) = (gate, faces.gate.clone()) {
                BlockPanel { block: g, face: Some(f), card: (250.0, 392.0) }
            }
        }
    }
}

/// The pre effects row: the blocks in front of the amp, each its pedal's
/// face (by the block preset loaded) beside a picture of what it is doing
/// — the delay's repeats, the spring's tail — or a card. The delay first
/// (the John Mayer chain's slapback ahead of its tank).
#[component]
pub fn PreFxRow(blocks: Vec<LiveBlock>, tempo_bpm: u32) -> Element {
    let faces = use_faces();
    let pre: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.name.starts_with("Pre ") && !b.name.eq_ignore_ascii_case("Pre Comp"))
        .cloned()
        .collect();
    let picked: Vec<(LiveBlock, Option<FaceEntry>)> = pre.iter().map(|b| (b.clone(), faces.pre(&b.preset).cloned())).collect();
    let absorbed: Vec<String> = picked
        .iter()
        .filter_map(|(_, f)| f.as_ref())
        .flat_map(|f| f.also.iter().map(|(_, name)| name.to_lowercase()))
        .collect();
    let mut items: Vec<(LiveBlock, Option<FaceEntry>, Vec<(String, LiveBlock)>)> = picked
        .into_iter()
        .filter(|(b, _)| !absorbed.contains(&b.name.to_lowercase()))
        .map(|(b, f)| {
            let extra = f
                .as_ref()
                .map(|f| {
                    f.also
                        .iter()
                        .filter_map(|(ns, name)| pre.iter().find(|x| x.name.eq_ignore_ascii_case(name)).map(|x| (ns.clone(), x.clone())))
                        .collect()
                })
                .unwrap_or_default();
            (b, f, extra)
        })
        .collect();
    items.sort_by_key(|(b, _, _)| b.block_type != BlockType::Delay);
    rsx! {
        Row {
            for (b, f, extra) in items {
                if let Some(f) = f {
                    // The pedal, then what its block is doing — a delay's
                    // repeats, a reverb's tail. A tremolo shows its pulse on
                    // its own lamp.
                    Panel { key: "{b.id}", weight: ratio(f.size) + if shows_viz(&b) { 1.6 } else { 0.0 },
                        BlockFace { block: b.clone(), face: f.clone(), extra: extra.clone() }
                        if shows_viz(&b) {
                            div { style: "flex: 1 1 0%; min-width: 0; height: 100%; position: relative; display: flex; justify-content: center; border-left: 1px solid #1d1f24;",
                                // A reverb's own picture from frame (its
                                // springs, the algorithm it runs), else the
                                // effect's visualiser.
                                if let Some(p) = (b.block_type == BlockType::Reverb).then(|| faces.picture("reverb").cloned()).flatten() {
                                    BlockFace { block: b.clone(), face: p }
                                } else {
                                    {viz_of(&b, tempo_bpm)}
                                }
                            }
                        }
                    }
                } else {
                    BlockPanel { key: "{b.id}", block: b.clone(), face: None, card: PEDAL }
                }
            }
        }
    }
}

/// Whether a pre effect's panel carries a picture beside its pedal.
fn shows_viz(b: &LiveBlock) -> bool {
    matches!(b.block_type, BlockType::Delay | BlockType::Reverb)
}

/// What a pre effect is doing, drawn by the effect's own visualiser.
fn viz_of(b: &LiveBlock, tempo_bpm: u32) -> Element {
    match b.block_type {
        BlockType::Delay => rsx! { crate::control::DelayBlockViz { block: b.clone(), tempo_bpm } },
        BlockType::Reverb => rsx! { crate::control::ReverbBlockViz { block: b.clone(), tempo_bpm } },
        _ => rsx! { crate::control::ModBlockViz { block: b.clone() } },
    }
}

/// The space kept beside the amps for the Amp module's presets: for now,
/// what is loaded.
#[component]
fn PresetsPanel(label: String) -> Element {
    let (module, snapshot) = label.split_once(" · ").map_or((label.clone(), String::new()), |(m, s)| (m.to_string(), s.to_string()));
    rsx! {
        div { style: "display: flex; flex-direction: column; justify-content: center; align-items: flex-start; gap: 6px; width: 100%; height: 100%; padding: 14px; box-sizing: border-box;",
            div { style: "color: #6b6f78; font-size: 9px; font-weight: 700; letter-spacing: 1.5px;", "AMP MODULE" }
            div { style: "color: #e8e6e1; font-size: 15px; font-weight: 600;", "{module}" }
            if !snapshot.is_empty() {
                div { style: "color: #9a9ea6; font-size: 12px;", "{snapshot}" }
            }
            div { style: "color: #4a4d55; font-size: 10px; margin-top: 8px;", "Presets" }
        }
    }
}
