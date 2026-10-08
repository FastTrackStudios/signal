//! The sidebar in the other footswitch modes — the prototype's `PresetView`
//! and `ProfileView`:
//!
//!   Preset   the presets; the one playing (or opened) opens into its
//!            variations — tap one to play it; add one at the foot (the
//!            sound playing, saved as a new variation).
//!   Profile  the profile's stacks on their own, grips to drag them into
//!            order; the stack playing opens into its patches — tap one to
//!            play it, ⋯ to rename, move or remove it, add one at the foot.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, PerformanceModel};
use signal_widgets::drag_bus::{DragBus, DragEvent};
use signal_widgets::PopupHost;

use super::colors::name_colour;
use super::marks::ProfileIcon;
use super::menu::{open_naming, Item, MoreButton, Picked};
use super::setlist::{call, play_patch, use_library, Plus, StackRow};
use super::tokens::*;
use crate::state::RigViewState;

/// The compositions, refetched whenever the rig's state moves on.
fn use_compositions(state: RigViewState) -> Signal<CompositionModel> {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut comp = use_signal(CompositionModel::default);
    use_effect(move || {
        let _revision = state.perf.read().revision;
        if let Some(r) = rig.clone() {
            spawn(async move {
                if let Ok(c) = r.compositions().await {
                    comp.set(c);
                }
            });
        }
    });
    comp
}

#[component]
fn Header(title: String, sub: String, icon: Option<String>) -> Element {
    rsx! {
        header { style: "flex-shrink: 0; height: {HEADER_H}px; display: flex; align-items: center; gap: 12px; padding: 0 16px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
            if let Some(p) = icon {
                ProfileIcon { name: p.clone(), colour: name_colour(&p).to_string(), size: 22 }
            }
            span { style: "display: flex; flex-direction: column; gap: 3px; min-width: 0;",
                span { style: "font-size: 22px; font-weight: 750; letter-spacing: -0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
                span { style: "font-size: 13px; color: {INK_3};", "{sub}" }
            }
        }
    }
}

// ── Presets ────────────────────────────────────────────────────────────────

#[component]
pub fn PresetView(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let comp = use_compositions(state);
    let c = comp.read().clone();
    let mut open = use_signal(|| None::<String>);
    let playing = c.active_preset.clone();
    let opened = open().or_else(|| (!playing.is_empty()).then(|| playing.clone()));
    let variations: usize = c.presets.iter().map(|p| p.snapshots.len()).sum();
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            Header { title: "Presets".to_string(), sub: format!("{} presets · {variations} variations", c.presets.len()), icon: None }
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                for p in c.presets.iter().cloned() {
                    {
                        let is_playing = p.name == playing;
                        let is_open = opened.as_deref() == Some(p.name.as_str());
                        let (tape, _) = crate::perform::folder_color(&p.name);
                        let first = p.snapshots.first().map(|s| s.name.clone()).unwrap_or_default();
                        let rig = rig.clone();
                        let name = p.name.clone();
                        let n = p.snapshots.len();
                        rsx! {
                            div { key: "{p.name}", style: "border-bottom: 1px solid {RULE};",
                                button {
                                    style: "position: relative; width: 100%; display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 0 14px 0 16px; text-align: left; border: none; background: {pick(is_playing, ROW_ON, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        open.set(if is_open { Some(String::new()) } else { Some(name.clone()) });
                                        if !is_playing {
                                            let (n, f) = (name.clone(), first.clone());
                                            call!(rig, |r| r.choose_preset(n, f));
                                        }
                                    },
                                    if is_playing {
                                        span { style: "position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 2px; background: {LIVE};" }
                                    }
                                    span { style: "width: 9px; height: 9px; border-radius: 2px; flex-shrink: 0; background: {pick(tape == \"#3f3f46\", INK_3, tape)};" }
                                    span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: {pick(is_playing, 700, 600)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                    span { style: "font-size: 13px; color: {INK_3};", "{n} {pick(n == 1, \"variation\", \"variations\")}" }
                                    svg { key: "{is_open}", width: "10", height: "6", view_box: "0 0 10 6",
                                        path { d: pick(is_open, "M1 5 L5 1 L9 5", "M1 1 L5 5 L9 1"), fill: "none", stroke: INK_3, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                                    }
                                }
                                if is_open {
                                    Variations { preset: p.name.clone(), variations: p.snapshots.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), playing: (c.active_preset.clone(), c.active_snapshot.clone()) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Variations(preset: String, variations: Vec<String>, playing: (String, String)) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let n = variations.len();
    rsx! {
        div { style: "background: rgba(0,0,0,0.25); padding-bottom: 2px;",
            for v in variations.iter().cloned() {
                {
                    let on = playing.0 == preset && playing.1 == v;
                    let (rig, p, vv) = (rig.clone(), preset.clone(), v.clone());
                    rsx! {
                        button {
                            key: "{v}",
                            style: "width: 100%; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 16px 0 36px; text-align: left; border: none; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| {
                                let (p, vv) = (p.clone(), vv.clone());
                                call!(rig, |r| r.choose_preset(p, vv));
                            },
                            span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {state_dot(on, INK_3)}" }
                            span { style: "flex: 1; font-size: 15px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)};", "{v}" }
                            if on { span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", "Playing" } }
                        }
                    }
                }
            }
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 16px 0 36px; border: none; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onclick: {
                    let (rig, preset, variations) = (rig.clone(), preset.clone(), variations.clone());
                    move |e: MouseEvent| {
                        let (c, el) = (e.client_coordinates(), e.element_coordinates());
                        let (rig, preset) = (rig.clone(), preset.clone());
                        open_naming(host, c.x - el.x + 20.0, c.y - el.y + 44.0, Item::name("add", format!("New variation of {preset}…"), format!("Variation {}", n + 1), "Add", variations.clone()), EventHandler::new(move |p: Picked| {
                            let (preset, name) = (preset.clone(), p.text.clone());
                            call!(rig, |r| r.save_core_snapshot(preset, name));
                        }));
                    }
                },
                Plus { size: 12 }
                "Add a variation"
            }
        }
    }
}

// ── The profile's stacks ───────────────────────────────────────────────────

#[component]
pub fn ProfileView(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let lib = use_library(state);
    let perf: PerformanceModel = state.perf.read().clone();
    let l = lib.read().clone();
    let profile = perf.profile_name.clone();
    let patches: usize = perf.stacks.iter().map(|s| s.patches.len()).sum();
    let count = perf.stacks.len();
    let drag = use_signal(|| None::<(usize, usize)>);
    let colour = name_colour(&profile).to_string();
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            Header { title: profile.clone(), sub: format!("{count} stacks · {patches} patches"), icon: Some(profile.clone()) }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 8px 0 0 8px;",
                div { style: "background: rgba(0,0,0,0.18); display: flex; flex-direction: column; border-bottom: 1px solid {RULE};",
                    for (i, st) in perf.stacks.iter().cloned().enumerate() {
                        {
                            let (dragging, drop_at) = match drag() {
                                Some((from, to)) => (from == i, to == i && from != i),
                                None => (false, false),
                            };
                            let (tape, _) = crate::perform::folder_color(&st.name);
                            let rig = rig.clone();
                            rsx! {
                                div { key: "{st.name}", style: "display: flex; flex-direction: column; opacity: {pick(dragging, 0.5, 1.0)}; {drop_line(drop_at)}",
                                    div { style: "display: flex; align-items: stretch;",
                                        // The grip: drag the stack into its place.
                                        span {
                                            "aria-label": "Drag {st.name} to move it",
                                            style: "width: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border-top: 1px solid {RULE}; cursor: grab; touch-action: none;",
                                            onpointerdown: move |e: PointerEvent| {
                                                e.prevent_default();
                                                e.stop_propagation();
                                                let y0 = e.client_coordinates().y;
                                                let mut drag = drag;
                                                drag.set(Some((i, i)));
                                                let Some(bus) = bus else { return };
                                                let rig = rig.clone();
                                                bus.begin(move |ev| {
                                                    let mut drag = drag;
                                                    match ev {
                                                        DragEvent::Move { y, .. } => {
                                                            let to = (i as i64 + ((y - y0) / 45.0).round() as i64).clamp(0, count as i64 - 1) as usize;
                                                            drag.set(Some((i, to)));
                                                        }
                                                        DragEvent::End => {
                                                            let to = drag.peek().map_or(i, |d| d.1);
                                                            drag.set(None);
                                                            if to != i {
                                                                call!(rig, |r| r.move_stack(i as u32, to as u32));
                                                            }
                                                        }
                                                    }
                                                });
                                            },
                                            svg { width: "14", height: "12", view_box: "0 0 14 12",
                                                path { d: "M1.5 2h11M1.5 6h11M1.5 10h11", stroke: pick(tape == "#3f3f46", INK_3, tape), stroke_width: "1.7", stroke_linecap: "round" }
                                            }
                                        }
                                        div { style: "flex: 1; min-width: 0;",
                                            StackRow { stack: st.clone(), index: i, count, lib: l.clone(), profile: profile.clone(), part: Default::default(), where_: String::new(), song_colour: colour.clone(), in_song: false }
                                        }
                                    }
                                    if st.is_active {
                                        StackPatches { stack: st.name.clone(), patches: st.patches.clone(), pos: st.position as usize }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Every patch in the stack playing: tap one to play it, ⋯ to rename,
/// move or remove it; "Add a patch" at the foot.
#[component]
fn StackPatches(stack: String, patches: Vec<String>, pos: usize) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let (tape, _) = crate::perform::folder_color(&stack);
    let n = patches.len();
    rsx! {
        div { style: "background: rgba(0,0,0,0.25); padding-bottom: 2px;",
            for (k, p) in patches.iter().cloned().enumerate() {
                {
                    let on = k == pos;
                    let (rig, stack_name, name) = (rig.clone(), stack.clone(), p.clone());
                    let (rig_tap, name_tap) = (rig.clone(), name.clone());
                    let items = vec![
                        Item::head(p.clone()),
                        Item::name("rename", "Rename…", p.clone(), "Rename", patches.clone()),
                        Item::run("up", "Move up").unless((k == 0).then(|| "Already first".to_string())),
                        Item::run("down", "Move down").unless((k + 1 == n).then(|| "Already last".to_string())),
                        Item::Sep,
                        Item::delete("remove", "Remove from the stack").unless((n <= 1).then(|| "A stack keeps at least one patch".to_string())),
                    ];
                    rsx! {
                        div { key: "{p}", style: "position: relative; display: flex; align-items: center; min-height: 44px; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)};",
                            button {
                                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 4px 0 52px; text-align: left; border: none; background: transparent; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| play_patch(rig_tap.clone(), name_tap.clone()),
                                span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {state_dot(on, tape_ink(tape))}" }
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p}" }
                            }
                            MoreButton {
                                label: format!("{p} actions"),
                                items,
                                on_pick: move |x: Picked| {
                                    let (stack_name, name) = (stack_name.clone(), name.clone());
                                    match x.id.as_str() {
                                        "rename" => {
                                            let new_name = x.text.clone();
                                            call!(rig, |r| r.rename_patch(name, new_name));
                                        }
                                        "up" => call!(rig, |r| r.move_stack_patch(stack_name, k as u32, k as u32 - 1)),
                                        "down" => call!(rig, |r| r.move_stack_patch(stack_name, k as u32, k as u32 + 1)),
                                        "remove" => call!(rig, |r| r.set_stack_patch(stack_name, name, false)),
                                        _ => {}
                                    }
                                },
                            }
                        }
                    }
                }
            }
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 12px 0 52px; border: none; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onclick: {
                    let (rig, stack, patches) = (rig.clone(), stack.clone(), patches.clone());
                    move |e: MouseEvent| {
                        let (c, el) = (e.client_coordinates(), e.element_coordinates());
                        let (rig, stack) = (rig.clone(), stack.clone());
                        open_naming(host, c.x - el.x + 40.0, c.y - el.y + 44.0, Item::name("add", format!("New patch in {stack}…"), format!("{stack} {}", n + 1), "Add", patches.clone()), EventHandler::new(move |p: Picked| {
                            let (name, stack) = (p.text.clone(), stack.clone());
                            call!(rig, |r| r.set_stack_patch(stack, name, true));
                        }));
                    }
                },
                Plus { size: 12 }
                "Add a patch to {stack}"
            }
        }
    }
}
