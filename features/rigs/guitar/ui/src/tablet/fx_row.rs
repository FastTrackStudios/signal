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

/// The face a block wears, if the set has one: a drive slot's pedal, an
/// amp's capture, the gate, the EQ, the compressors, a delay machine or
/// reverb algorithm, a modulation effect, a pre effect's pedal, the trim.
pub fn face_for(b: &LiveBlock, faces: &Faces) -> Option<FaceEntry> {
    if b.empty {
        return None;
    }
    let pedal = || faces.pre(&b.preset).or_else(|| faces.pre(&b.name)).cloned();
    let param = |name: &str| b.params.iter().find(|p| p.name == name).map(|p| p.value.round().max(0.0) as usize);
    match b.block_type {
        // A drive slot: its pedal, by the preset's name or the capture's.
        BlockType::Drive | BlockType::Boost if !is_pre(b) => faces.drive(&b.preset).or_else(|| faces.drive(&b.detail)).cloned().or_else(pedal),
        BlockType::Amp => faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).cloned(),
        BlockType::Cabinet => faces.cab.clone(),
        BlockType::Gate => faces.gate.clone(),
        BlockType::Eq => faces.eq.clone(),
        BlockType::Compressor if b.name.to_lowercase().contains("pre") => faces.pre_comp_pedal.clone().or_else(pedal),
        BlockType::Compressor => faces.post_comp.clone(),
        BlockType::Delay if !is_pre(b) => faces.time(false, crate::control::DELAY_ALGOS.get(param("style").unwrap_or(1)).copied().unwrap_or("")).cloned(),
        BlockType::Reverb if !is_pre(b) => faces.time(true, crate::control::VERB_ALGOS.get(param("algorithm").unwrap_or(1)).copied().unwrap_or("")).cloned(),
        BlockType::Volume if b.name.eq_ignore_ascii_case("Patch Trim") => faces.trim.clone(),
        _ if is_pre(b) => pedal(),
        t => faces.modulation(t.as_str()).cloned().or_else(pedal),
    }
}

#[component]
pub fn FxRow(state: RigViewState) -> Element {
    let sel = use_context::<RoutingSel>().0;
    let faces = use_faces();
    let blocks = state.blocks.read().clone();
    // What it shows: the selection, else the drive board's first pedal,
    // else the chain's first block.
    let shown: Vec<LiveBlock> = match sel() {
        Some(Selected::Block(id)) => blocks.iter().filter(|b| b.id == id).cloned().collect(),
        Some(Selected::Module(_, ids)) => blocks.iter().filter(|b| ids.contains(&b.id)).cloned().collect(),
        None => blocks
            .iter()
            .find(|b| b.block_type == BlockType::Drive && !b.empty)
            .or_else(|| blocks.first())
            .cloned()
            .into_iter()
            .collect(),
    };
    let one = shown.len() == 1;
    rsx! {
        div { style: "flex-shrink: 0; height: {FX_H}px; border-top: 1px solid #000; background: #0d0d10; overflow-x: auto; overflow-y: hidden;",
            div { style: "height: 100%; display: flex; align-items: stretch; justify-content: {pick(one, \"center\", \"flex-start\")}; gap: 1px; width: max-content; min-width: 100%;",
                for b in shown.into_iter() {
                    {
                        let face = face_for(&b, &faces);
                        rsx! {
                            div { key: "{b.id}", style: "flex-shrink: 0; height: 100%; display: flex; align-items: stretch;",
                                match face {
                                    // One block: its face fills the row's box.
                                    Some(f) if one => rsx! {
                                        div { style: "width: {FX_W}px; height: 100%;",
                                            BlockFace { block: b.clone(), face: f.at_box(FX_W, FX_H), fill: true, stepper: true }
                                        }
                                    },
                                    // A module's: each at the row's height.
                                    Some(f) => rsx! { BlockFace { block: b.clone(), face: f.at(crate::control::Tier::Ipad) } },
                                    None => rsx! {
                                        div { style: "width: {pick(one, FX_W, 260.0)}px; height: 100%; padding: 12px; box-sizing: border-box;",
                                            NameCard { block: b.clone(), aspect: (pick(one, FX_W, 260.0), FX_H) }
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
