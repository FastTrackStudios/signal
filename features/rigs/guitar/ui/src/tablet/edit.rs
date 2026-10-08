//! Edit — the prototype's `views/Edit.tsx` on the rig: the sound playing,
//! taken apart. Routing along the top and the FX row along the foot are
//! empty until the block and module system is remade and Frame lands;
//! between them, whose edits these are.
//!
//! Edits are the song's own (an override over the patch, marked) until
//! Save to patch writes them into the profile (`save_song_changes`) or
//! Discard drops them (`discard_song_changes`).

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::PerformanceModel;

use super::colors::section_colour;
use super::tokens::*;
use crate::state::RigViewState;

/// The FX row's height — the landscape iPhone's safe area.
const FX_ROW_H: u32 = 381;

#[component]
pub fn EditView(state: RigViewState, perf: PerformanceModel) -> Element {
    let edits: usize = state.blocks.read().iter().map(|b| b.params.iter().filter(|p| p.overridden).count()).sum();
    rsx! {
        div { style: "height: 100%; display: flex; flex-direction: column; min-height: 0;",
            // Routing: the block and module system, to be remade — empty until then.
            div { style: "flex: 1; min-height: 0;" }
            OverrideBar { perf: perf.clone(), edits, playing: state.active_patch.read().clone().unwrap_or_default() }
            // The FX row: Frame's place — empty until it lands.
            div { style: "flex-shrink: 0; height: {FX_ROW_H}px; border-top: 1px solid #000; background: #0d0d10;" }
        }
    }
}

const EDITS_BG: &str = "rgba(245,158,11,0.08)";

/// Whose edits these are, and the way to keep them for good.
#[component]
fn OverrideBar(perf: PerformanceModel, edits: usize, playing: String) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let part = perf.parts.get(perf.part_index as usize).cloned();
    let patch = part.as_ref().map(|p| p.patch.clone()).filter(|p| !p.is_empty()).unwrap_or(playing);
    let section = part.as_ref().map(|p| if p.section.is_empty() { p.name.clone() } else { p.section.clone() }).unwrap_or_default();
    let several = part.as_ref().is_some_and(|p| !p.section.is_empty() && p.name != p.section);
    let note = if edits > 0 { format!("{edits} change{} — this song's own", if edits > 1 { "s" } else { "" }) } else { format!("plays {patch}") };
    let (r1, r2, p1, p2) = (rig.clone(), rig.clone(), patch.clone(), patch.clone());
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 0 6px 0 14px; border-top: 1px solid {RULE}; background: {pick(edits > 0, EDITS_BG, SHEET)};",
            if !(section.is_empty() && patch.is_empty()) {
                if !section.is_empty() {
                    span { style: "font-size: 14px; font-weight: 750; color: {section_colour(&section)}; white-space: nowrap;", "{section}" }
                }
                if several {
                    span { style: "font-size: 13.5px; font-weight: 650; white-space: nowrap;", "{part.as_ref().map(|p| p.name.clone()).unwrap_or_default()}" }
                }
                span { style: "font-size: 13px; color: {pick(edits > 0, MODIFIED, INK_3)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{note}" }
                span { style: "flex: 1;" }
                if edits > 0 {
                    button {
                        style: "height: 36px; padding: 0 12px; border: none; border-radius: {R}; background: transparent; font-size: 13px; font-weight: 650; color: {INK_2}; font-family: {FONT}; cursor: pointer;",
                        onclick: move |_| {
                            if let Some(r) = r1.clone() {
                                let p = p1.clone();
                                spawn(async move { let _ = r.discard_song_changes(p).await; });
                            }
                        },
                        "Discard"
                    }
                    button {
                        style: "height: 36px; padding: 0 14px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; font-size: 13px; font-weight: 700; color: {INK}; font-family: {FONT}; cursor: pointer;",
                        onclick: move |_| {
                            if let Some(r) = r2.clone() {
                                let p = p2.clone();
                                spawn(async move { let _ = r.save_song_changes(p).await; });
                            }
                        },
                        "Save to {patch}"
                    }
                }
            }
        }
    }
}
