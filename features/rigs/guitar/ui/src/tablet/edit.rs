//! Edit — the prototype's `views/Edit.tsx` on the rig: the sound of the
//! section picked in the setlist, taken apart. Routing along the top and
//! the FX row along the foot are empty until the block and module system
//! is remade and Frame lands; between them, whose edits these are.
//!
//! Edits are the section's own (its overrides and picks) until Save writes
//! them into the patch it plays (`save_part_changes`) or Discard drops them.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::PerformanceModel;

use super::colors::section_colour;
use super::setlist::{sections_of, sound_of};
use super::tokens::*;
use super::BuildPick;
use crate::state::RigViewState;


#[component]
pub fn EditView(state: RigViewState, perf: PerformanceModel) -> Element {
    let picked = try_use_context::<BuildPick>().and_then(|b| (b.part)());
    // The section's own changes: its overrides and its module/block picks.
    let edits = picked.and_then(|i| perf.parts.get(i)).map_or(0, |p| p.overrides.len() + p.picks.len());
    rsx! {
        div { style: "height: 100%; display: flex; flex-direction: column; min-height: 0;",
            // Routing: the patch's modules and blocks, in chain order.
            div { style: "flex: 1; min-height: 0;",
                super::routing::Routing { state }
            }
            OverrideBar { perf: perf.clone(), edits, picked }
            // The FX row: the selected block's controls, as Frame draws them.
            super::fx_row::FxRow { state }
        }
    }
}

const EDITS_BG: &str = "rgba(245,158,11,0.08)";

/// Whose edits these are, and the way to keep them for good.
#[component]
fn OverrideBar(perf: PerformanceModel, edits: usize, picked: Option<usize>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let sections = sections_of(&perf.parts);
    let section = picked.and_then(|i| sections.iter().find(|s| s.parts.contains(&i)).cloned());
    let part = picked.and_then(|i| perf.parts.get(i).cloned());
    let (Some(sec), Some(part)) = (section, part) else {
        return rsx! {};
    };
    let several = sec.parts.len() > 1;
    let preset = sound_of(&part);
    let note = if edits > 0 {
        format!("{edits} change{} — this section's own", if edits > 1 { "s" } else { "" })
    } else {
        format!("plays {}", preset.clone().unwrap_or_else(|| "what came before".to_string()))
    };
    let save_label = format!("Save to {}", preset.clone().unwrap_or_else(|| "preset".to_string()));
    let can_save = preset.is_some();
    let (r1, r2) = (rig.clone(), rig.clone());
    let (name1, name2) = (part.name.clone(), part.name.clone());
    let picks: Vec<String> = part.picks.iter().map(|p| p.kind.clone()).collect();
    let colour = section_colour(&sec.name);
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 0 6px 0 14px; border-top: 1px solid {RULE}; background: {pick(edits > 0, EDITS_BG, SHEET)}; box-sizing: border-box;",
            span { style: "font-size: 14px; font-weight: 750; color: {colour}; white-space: nowrap;", "{sec.name}" }
            if several {
                span { style: "font-size: 13.5px; font-weight: 650; white-space: nowrap;", "{part.name}" }
            }
            span { style: "font-size: 13px; color: {pick(edits > 0, MODIFIED, INK_3)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{note}" }
            span { style: "flex: 1;" }
            if edits > 0 {
                button {
                    style: "height: 36px; padding: 0 12px; border: none; border-radius: {R}; background: transparent; font-size: 13px; font-weight: 650; color: {INK_2}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| {
                        // Back to its sound as it is: no overrides, no picks.
                        if let Some(r) = r1.clone() {
                            let (part, picks) = (name1.clone(), picks.clone());
                            let _ = dioxus_core::spawn_forever(async move {
                                let _ = r.set_part_overrides(part.clone(), Vec::new()).await;
                                for kind in picks {
                                    let _ = r.clear_part_pick(part.clone(), kind).await;
                                }
                            });
                        }
                    },
                    "Discard"
                }
                button {
                    disabled: !can_save,
                    style: "height: 36px; padding: 0 14px; border-radius: {R}; border: 1px solid {RULE_STRONG}; box-sizing: border-box; background: transparent; font-size: 13px; font-weight: 700; color: {INK}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| {
                        if !can_save { return; }
                        if let Some(r) = r2.clone() {
                            let part = name2.clone();
                            let _ = dioxus_core::spawn_forever(async move { let _ = r.save_part_changes(part).await; });
                        }
                    },
                    "{save_label}"
                }
            }
        }
    }
}
