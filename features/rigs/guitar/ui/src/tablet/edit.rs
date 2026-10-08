//! Edit — the prototype's `views/Edit.tsx` on the rig: the sound playing,
//! taken apart. Routing along the top (a stand-in until the block and
//! module system is remade: the chain's blocks in signal order, grouped by
//! kind, one picked), whose edits these are, and the FX row along the foot:
//! the picked block's controls.
//!
//! Edits are the song's own (an override over the patch, marked) until
//! Save to patch writes them into the profile (`save_song_changes`) or
//! Discard drops them (`discard_song_changes`).

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LiveBlock, PerformanceModel};
use signal_widgets::drag_bus::{DragBus, DragEvent};

use super::colors::section_colour;
use super::tokens::*;
use crate::state::RigViewState;

/// The FX row's height — the landscape iPhone's safe area.
const FX_ROW_H: u32 = 381;

fn kind_of(b: &LiveBlock) -> String {
    let t = format!("{:?}", b.block_type);
    if t.is_empty() { "Block".into() } else { t }
}

#[component]
pub fn EditView(state: RigViewState, perf: PerformanceModel) -> Element {
    let blocks: Vec<LiveBlock> = state.blocks.read().iter().filter(|b| !b.empty).cloned().collect();
    let mut picked = use_signal(String::new);
    let current = blocks
        .iter()
        .find(|b| b.id == picked())
        .or_else(|| blocks.iter().find(|b| kind_of(b).eq_ignore_ascii_case("drive")))
        .or_else(|| blocks.first())
        .cloned();
    // The chain in signal order, consecutive blocks of a kind together.
    let mut groups: Vec<(String, Vec<LiveBlock>)> = Vec::new();
    for b in &blocks {
        let k = kind_of(b);
        match groups.last_mut() {
            Some(g) if g.0 == k => g.1.push(b.clone()),
            _ => groups.push((k, vec![b.clone()])),
        }
    }
    let current_id = current.as_ref().map(|b| b.id.clone()).unwrap_or_default();
    rsx! {
        div { style: "height: 100%; display: flex; flex-direction: column; min-height: 0;",
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px; box-sizing: border-box;",
                span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "Routing" }
                div { style: "display: flex; flex-wrap: wrap; gap: 6px; align-content: flex-start;",
                    for (i, (kind, list)) in groups.into_iter().enumerate() {
                        div { key: "{i}-{kind}", style: "display: flex; flex-direction: column; gap: 4px; padding: 6px; border-radius: {R_MD}; background: {SHEET_2}; border: 1px solid {RULE};",
                            span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3}; padding: 0 4px;", "{kind}" }
                            div { style: "display: flex; gap: 4px;",
                                for b in list {
                                    {
                                        let on = b.id == current_id;
                                        let id = b.id.clone();
                                        rsx! {
                                            button {
                                                key: "{b.id}",
                                                style: "position: relative; height: 44px; padding: 0 12px; border: none; border-radius: {R}; font-size: 13px; font-weight: {pick(on, 700, 560)}; white-space: nowrap; color: {pick(b.bypassed, INK_3, INK)}; background: {pick(on, PRESSED, KEY)}; font-family: {FONT}; cursor: pointer;",
                                                onclick: move |_| picked.set(id.clone()),
                                                if on {
                                                    span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: {INK_2};" }
                                                }
                                                if b.overridden {
                                                    span { style: "position: absolute; top: 5px; right: 5px; width: 5px; height: 5px; border-radius: 999px; background: {MODIFIED};" }
                                                }
                                                "{b.name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            OverrideBar { perf: perf.clone(), edits: blocks.iter().map(|b| b.params.iter().filter(|p| p.overridden).count()).sum::<usize>() }
            FxRow { block: current }
        }
    }
}

const PRESSED: &str = "rgba(0,0,0,0.5)";
const KEY: &str = "#1b1b20";
const EDITS_BG: &str = "rgba(245,158,11,0.08)";

/// Whose edits these are, and the way to keep them for good.
#[component]
fn OverrideBar(perf: PerformanceModel, edits: usize) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let part = perf.parts.get(perf.part_index as usize).cloned();
    let patch = part.as_ref().map(|p| p.patch.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| perf.stacks.iter().find(|s| s.is_active).map(|s| s.current_patch.clone()).unwrap_or_default());
    let section = part.as_ref().map(|p| if p.section.is_empty() { p.name.clone() } else { p.section.clone() }).unwrap_or_default();
    let several = part.as_ref().is_some_and(|p| !p.section.is_empty() && p.name != p.section);
    let note = if edits > 0 { format!("{edits} change{} — this song's own", if edits > 1 { "s" } else { "" }) } else { format!("plays {patch}") };
    let (r1, r2, p1, p2) = (rig.clone(), rig.clone(), patch.clone(), patch.clone());
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 0 6px 0 14px; border-top: 1px solid {RULE}; background: {pick(edits > 0, EDITS_BG, SHEET)};",
            if section.is_empty() && patch.is_empty() {
                span { style: "font-size: 13.5px; color: {INK_3};", "Pick a section in the setlist" }
            } else {
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

/// The FX row: the picked block's controls, an upright fader each.
#[component]
fn FxRow(block: Option<LiveBlock>) -> Element {
    let Some(b) = block else {
        return rsx! { div { style: "flex-shrink: 0; height: {FX_ROW_H}px; border-top: 1px solid #000; background: #0d0d10;" } };
    };
    let params: Vec<_> = b.params.iter().take(8).cloned().collect();
    let cols = params.len().max(1);
    rsx! {
        div { style: "flex-shrink: 0; height: {FX_ROW_H}px; display: flex; flex-direction: column; border-top: 1px solid #000; background: #0d0d10;",
            div { style: "display: flex; align-items: baseline; gap: 10px; padding: 12px 16px 6px;",
                span { style: "font-size: 17px; font-weight: 750;", "{b.name}" }
                span { style: "font-size: 13px; color: {INK_3};", "{b.preset}" }
            }
            div { style: "flex: 1; min-height: 0; display: grid; grid-template-columns: repeat({cols}, minmax(0, 1fr)); gap: 1px; background: #000;",
                for p in params {
                    Param { key: "{b.id}-{p.name}", block: b.id.clone(), name: p.name.clone(), value: p.value, min: p.min, max: p.max, changed: p.overridden }
                }
            }
        }
    }
}

/// One control: its name, an upright fader, its value. A drag moves the
/// rig's block as it goes.
#[component]
fn Param(block: String, name: String, value: f32, min: f32, max: f32, changed: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let mut live = use_signal(|| None::<f32>);
    let v = live().unwrap_or(value);
    let span = (max - min).max(1e-6);
    let fill = f64::from(((v - min) / span).clamp(0.0, 1.0));
    const H: f64 = 180.0;
    let shown = if span > 20.0 { format!("{v:.0}") } else if span > 2.0 { format!("{v:.1}") } else { format!("{:.0}", fill * 100.0) };
    let ink = if changed { MODIFIED } else { INK_3 };
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; background: #111114; min-width: 0;",
            span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%;", "{name}" }
            div {
                style: "position: relative; width: 44px; height: {H}px; cursor: ns-resize;",
                onpointerdown: move |e| {
                    e.prevent_default();
                    let (cy, ey) = (e.client_coordinates().y, e.element_coordinates().y);
                    let top = cy - ey;
                    let at = move |y: f64| min + span * (1.0 - ((y - top) / H).clamp(0.0, 1.0)) as f32;
                    let send = {
                        let (rig, block, name) = (rig.clone(), block.clone(), name.clone());
                        move |x: f32| {
                            if let Some(r) = rig.clone() {
                                let (b, n) = (block.clone(), name.clone());
                                spawn(async move { let _ = r.set_block_param(b, n, x).await; });
                            }
                        }
                    };
                    let x = at(cy);
                    live.set(Some(x));
                    send(x);
                    let Some(bus) = bus else { return };
                    bus.begin(move |ev| {
                        let mut live = live;
                        match ev {
                            DragEvent::Move { y, .. } => {
                                let x = at(y);
                                live.set(Some(x));
                                send(x);
                            }
                            DragEvent::End => live.set(None),
                        }
                    });
                },
                span { style: "position: absolute; top: 0; bottom: 0; left: 20px; width: 4px; border-radius: 2px; background: {WELL};" }
                span { style: "position: absolute; bottom: 0; left: 20px; width: 4px; height: {fill * 100.0}%; border-radius: 2px; background: {pick(changed, MODIFIED, INK_2)};" }
                span { style: "position: absolute; left: 10px; width: 24px; height: 10px; top: {(1.0 - fill) * (H - 10.0)}px; border-radius: 3px; background: #f4f4f5;" }
            }
            span { style: "font-size: 14px; font-weight: 700; color: {pick(changed, INK, INK_3)};", "{shown}" }
        }
    }
}
