//! The FX row — the selected block's controls, as Frame draws them: its
//! face from the rig-faces set (`rig_faces`), laid out for the row; a
//! module's blocks side by side; a block with no face its name card. With
//! nothing picked in the routing strip, the drive board's first pedal.

use dioxus::prelude::*;
use signal_guitar_proto::LiveBlock;
use signal_proto::block::BlockType;

use super::routing::{RoutingSel, Selected};
use super::tokens::*;
use crate::rig_faces::{use_faces, BlockFace, FaceEntry, Faces, NameCard};
use crate::state::RigViewState;

/// The row's box, pt — what a landscape iPhone 16 Pro leaves for it: its
/// 750 × 381 safe area less the phone's top bar and routing strip, so one
/// face set serves this row and a phone on its side.
pub const FX_W: f64 = 750.0;
pub const FX_H: f64 = 254.0;

/// A pre effect (before the amp): its face is its pedal's.
fn is_pre(b: &LiveBlock) -> bool {
    b.name.to_lowercase().starts_with("pre ")
}

/// An input-stage block (ahead of the drives): the octaver and harmonizer,
/// the envelope filter or wah, the volume pedal, the dive bomb, the
/// transposer — each wears its own unit.
fn is_input(b: &LiveBlock) -> bool {
    let name = b.name.to_lowercase();
    ["pitch", "harmon", "octav", "filter", "wah", "volume pedal", "dive", "transpose", "doubler"].iter().any(|w| name.contains(w))
}

/// The face a block wears, if the set has one: a drive slot's pedal, an
/// amp's capture, the gate, the EQ, the compressors, a delay machine or
/// reverb algorithm, a modulation effect, a pre effect's pedal, the trim.
pub fn face_for(b: &LiveBlock, faces: &Faces) -> Option<FaceEntry> {
    if b.empty {
        return None;
    }
    let pedal = || faces.pre(&b.preset).or_else(|| faces.pre(&b.name)).cloned();
    let param = |name: &str| b.params.iter().find(|p| p.name == name).map(|p| p.value.round().max(0.0) as usize);
    if is_input(b) {
        return faces.input_for(&b.preset).or_else(|| faces.input_for(&b.name)).cloned();
    }
    match b.block_type {
        // A drive slot: its pedal, by the preset's name or the capture's.
        BlockType::Drive | BlockType::Boost if !is_pre(b) => faces.drive(&b.preset).or_else(|| faces.drive(&b.detail)).cloned().or_else(pedal),
        BlockType::Amp => faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).cloned(),
        BlockType::Cabinet => faces.cab.clone(),
        BlockType::Gate => faces.gate.clone(),
        BlockType::Eq => faces.eq.clone(),
        // The Pre Comp: the compressor's scope (its curve, its gain
        // reduction), as its block in the grid shows it.
        BlockType::Compressor if b.name.to_lowercase().contains("pre") => faces.comp.clone().or_else(|| faces.pre_comp_pedal.clone()).or_else(pedal),
        BlockType::Compressor => faces.post_comp.clone(),
        BlockType::Delay if !is_pre(b) => faces.time(false, crate::control::DELAY_ALGOS.get(param("style").unwrap_or(1)).copied().unwrap_or("")).cloned(),
        BlockType::Reverb if !is_pre(b) => faces.time(true, crate::control::VERB_ALGOS.get(param("algorithm").unwrap_or(1)).copied().unwrap_or("")).cloned(),
        BlockType::Volume if b.name.eq_ignore_ascii_case("Patch Trim") => faces.trim.clone(),
        _ if is_pre(b) => pedal(),
        t => faces.modulation(t.as_str()).cloned().or_else(pedal),
    }
}

/// An input-stage slot the patch leaves empty (`slot:Filter`), as a block
/// of its kind, off: the row shows the unit it would be.
fn slot_block(id: &str) -> Option<LiveBlock> {
    let (name, block_type) = match id.strip_prefix("slot:")? {
        "Filter" => ("Filter", BlockType::Filter),
        "Pitch" => ("Pitch", BlockType::Pitch),
        "Comp" => ("Pre Comp", BlockType::Compressor),
        _ => return None,
    };
    Some(LiveBlock {
        id: id.to_string(),
        engine: 0,
        block_type,
        name: name.to_string(),
        bypassed: true,
        param_name: None,
        param_value: 0.0,
        param_min: 0.0,
        param_max: 1.0,
        output_level_db: None,
        detail: String::new(),
        asset: String::new(),
        module: String::new(),
        empty: false,
        params: Vec::new(),
        preset: String::new(),
        options: Vec::new(),
        option: 0,
        overridden: false,
    })
}

#[component]
pub fn FxRow(state: RigViewState) -> Element {
    let sel = use_context::<RoutingSel>().0;
    let faces = use_faces();
    let blocks = state.blocks.read().clone();
    // What it shows: a block picked, alone, the whole row its own; a module
    // picked, its blocks side by side; nothing picked, the drive board's
    // first pedal.
    let first = || blocks.iter().find(|b| b.block_type == BlockType::Drive && !b.empty).or_else(|| blocks.first()).map(|b| b.id.clone());
    let shown: Vec<LiveBlock> = match sel().or_else(|| first().map(Selected::Block)) {
        // A block: the whole row its own. An empty slot of the input stage
        // (the patch has no filter, say): its unit, off.
        Some(Selected::Block(id)) => blocks.iter().filter(|b| b.id == id).cloned().collect::<Vec<_>>().into_iter().next().or_else(|| slot_block(&id)).into_iter().collect(),
        Some(Selected::Module(_, ids)) => blocks.iter().filter(|b| ids.contains(&b.id)).cloned().collect(),
        None => Vec::new(),
    };
    let one = shown.len() == 1;
    // The row's width, measured: one block's face fills it edge to edge.
    let mut row_w = use_signal(|| FX_W);
    let measure = move |el: std::rc::Rc<MountedData>| {
        spawn(async move {
            for _ in 0..10 {
                if let Ok(r) = el.get_client_rect().await
                    && r.width() > 0.0
                {
                    if (*row_w.peek() - r.width()).abs() > 0.5 {
                        row_w.set(r.width());
                    }
                    return;
                }
                architect::platform::sleep(std::time::Duration::from_millis(30)).await;
            }
        });
    };
    let w = row_w();
    rsx! {
        div { style: "flex-shrink: 0; height: {FX_H}px; box-sizing: border-box; border-top: 1px solid #000; background: #0d0d10; overflow-x: auto; overflow-y: hidden;",
            onmounted: move |e| measure(e.data()),
            div { style: "height: 100%; display: flex; align-items: stretch; justify-content: center; gap: 1px; width: max-content; min-width: 100%;",
                for b in shown.into_iter() {
                    {
                        let face = face_for(&b, &faces);
                        rsx! {
                            div { key: "{b.id}", style: "position: relative; flex-shrink: 0; height: 100%; display: flex; align-items: stretch;",
                                match face {
                                    // One block: its face fills the row's box.
                                    Some(f) if one => rsx! {
                                        div { style: "width: {w}px; height: 100%;",
                                            BlockFace { block: b.clone(), face: f.at_box(w, FX_H), fill: true, stepper: true }
                                        }
                                    },
                                    // A module's: each in a box the row's height
                                    // and its own proportion, so each fits whole.
                                    Some(f) => {
                                        let f = f.at(crate::control::Tier::Ipad);
                                        let w = (FX_H * f.size.0 / f.size.1.max(1.0)).round();
                                        rsx! {
                                            div { style: "width: {w}px; height: 100%;",
                                                BlockFace { block: b.clone(), face: f, fill: true }
                                            }
                                        }
                                    }
                                    None => rsx! {
                                        div { style: "width: {pick(one, w, 260.0)}px; height: 100%; padding: 12px; box-sizing: border-box;",
                                            NameCard { block: b.clone(), aspect: (pick(one, w, 260.0), FX_H) }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
