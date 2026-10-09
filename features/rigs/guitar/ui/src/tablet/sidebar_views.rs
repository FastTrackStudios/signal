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
use signal_widgets::drag_bus::DragBus;
use signal_widgets::PopupHost;

use super::colors::{name_colour, tape_mark};
use super::marks::ProfileIcon;
use super::menu::{open_naming_under, Item, MoreButton, Picked, PressMenu};
use super::setlist::{begin_row_drag, call, is_live, keep_row, part_patch, profile_stacks, stack_for, use_library, Plus, Rows, StackRow, StackView};
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

/// A variation's row, for the loaded preset's window of four.
const VARIATION_ROW: f64 = 44.0;

/// The kinds of sound a variation can be: what the list sorts by.
const ROLES: [&str; 5] = ["Clean", "Crunch", "Drive", "Lead", "Ambient"];

/// The role a variation's name says, if any.
fn role_of(variation: &str) -> Option<&'static str> {
    let v = variation.to_lowercase();
    ROLES.iter().find(|r| v.contains(&r.to_lowercase())).copied()
}

/// The presets: the loaded one held at the top, its variations in a window
/// four rows tall; under it a search and the kinds of sound; then the rest,
/// scrolling on their own.
#[component]
pub fn PresetView(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let comp = use_compositions(state);
    let c = comp.read().clone();
    let lib = use_library(state);
    let l = lib.read().clone();
    let perf: PerformanceModel = state.perf.read().clone();
    let mut open = use_signal(|| None::<String>);
    let mut role = use_signal(|| None::<&'static str>);
    let mut query = use_signal(String::new);
    let playing = c.active_preset.clone();
    let loaded = c.presets.iter().find(|p| p.name == playing).cloned();
    let fits = |p: &signal_guitar_proto::PresetEntry| role().is_none_or(|r| p.snapshots.iter().any(|s| role_of(&s.name) == Some(r)));
    let q = query().trim().to_lowercase();
    let found = |p: &signal_guitar_proto::PresetEntry| q.is_empty() || p.name.to_lowercase().contains(&q) || p.snapshots.iter().any(|s| s.name.to_lowercase().contains(&q));
    let rest: Vec<signal_guitar_proto::PresetEntry> = c.presets.iter().filter(|p| p.name != playing && fits(p) && found(p)).cloned().collect();
    let chip = |on: bool| format!("height: 32px; padding: 0 12px; border-radius: 16px; border: 1px solid {}; background: {}; color: {}; font-size: 13px; font-weight: 650; font-family: {FONT}; display: flex; align-items: center; gap: 6px; cursor: pointer; flex-shrink: 0;", pick(on, INK_2, RULE_STRONG), pick(on, "rgba(255,255,255,0.08)", "transparent"), pick(on, INK, INK_2));
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            // The one loaded, held: big, its variations four rows tall.
            if let Some(p) = loaded.clone() {
                div { style: "flex-shrink: 0; border-bottom: 1px solid {RULE}; background: {ROW_ON};",
                    div { style: "display: flex; flex-direction: column; gap: 3px; padding: 14px 16px 8px;",
                        span { style: "font-size: 11px; font-weight: 800; letter-spacing: 0.1em; color: {LIVE};", "LOADED" }
                        span { style: "font-size: 22px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                        span { style: "font-size: 13px; color: {INK_3};", "{c.active_snapshot} · {p.snapshots.len()} variations" }
                    }
                    div { style: "max-height: {VARIATION_ROW * 4.0}px; overflow-y: auto;",
                        Variations { preset: p.name.clone(), variations: p.snapshots.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), playing: (c.active_preset.clone(), c.active_snapshot.clone()) }
                    }
                }
            }
            // Find the rest: by name, by the kind of sound.
            div { style: "flex-shrink: 0; display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; border-bottom: 1px solid {RULE};",
                label { style: "display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 12px; border-radius: {R}; background: {FILL};",
                    svg { width: "16", height: "16", view_box: "0 0 16 16",
                        circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                        path { d: "M10.6 10.6 14 14", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                    input {
                        style: "flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT};",
                        placeholder: "Search presets",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
                div { style: "display: flex; gap: 6px; overflow-x: auto;",
                    button { style: "{chip(role().is_none())}", onclick: move |_| role.set(None), "All" }
                    for r in ROLES {
                        button { key: "{r}", style: "{chip(role() == Some(r))}", onclick: move |_| role.set(if role() == Some(r) { None } else { Some(r) }),
                            span { style: "width: 8px; height: 8px; border-radius: 2px; background: {tape_mark(r)};" }
                            "{r}"
                        }
                    }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                for p in rest.into_iter() {
                    {
                        let is_open = open().as_deref() == Some(p.name.as_str());
                        // The swatch: the tape of the stack it plays in.
                        let swatch = tape_mark(&stack_for(&perf, &l, &perf.profile_name, &p.name));
                        let first = p.snapshots.iter().find(|s| role().is_none_or(|r| role_of(&s.name) == Some(r))).or(p.snapshots.first()).map(|s| s.name.clone()).unwrap_or_default();
                        let rig = rig.clone();
                        let name = p.name.clone();
                        let shown: Vec<String> = p.snapshots.iter().map(|s| s.name.clone()).filter(|s| role().is_none_or(|r| role_of(s) == Some(r))).collect();
                        let n = p.snapshots.len();
                        rsx! {
                            div { key: "{p.name}", style: "border-bottom: 1px solid {RULE};",
                                button {
                                    style: "position: relative; width: 100%; display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 0 14px 0 16px; text-align: left; border: none; background: {CLEAR}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        open.set(if is_open { None } else { Some(name.clone()) });
                                        if !is_open {
                                            let (n, f) = (name.clone(), first.clone());
                                            call!(rig, |r| r.choose_preset(n, f));
                                        }
                                    },
                                    span { style: "width: 9px; height: 9px; border-radius: 2px; flex-shrink: 0; background: {swatch};" }
                                    span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                    span { style: "font-size: 13px; color: {INK_3};", "{n}" }
                                    svg { key: "{is_open}", width: "10", height: "6", view_box: "0 0 10 6",
                                        path { d: pick(is_open, "M1 5 L5 1 L9 5", "M1 1 L5 5 L9 1"), fill: "none", stroke: INK_3, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                                    }
                                }
                                if is_open {
                                    Variations { preset: p.name.clone(), variations: shown, playing: (c.active_preset.clone(), c.active_snapshot.clone()) }
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
    let mut add_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
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
                            if let Some(r) = role_of(&v) {
                                span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {tape_mark(r)};" }
                            }
                            if on { span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", "Playing" } }
                        }
                    }
                }
            }
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 16px 0 36px; border: none; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onmounted: move |e| add_el.set(Some(e.data())),
                onclick: {
                    let (rig, preset, variations) = (rig.clone(), preset.clone(), variations.clone());
                    move |_| {
                        let (rig, preset) = (rig.clone(), preset.clone());
                        open_naming_under(host, add_el.peek().clone(), Item::name("add", format!("New variation of {preset}…"), format!("Variation {}", n + 1), "Add", variations.clone()), EventHandler::new(move |p: Picked| {
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

/// The profile's stacks on their own — the same rows the setlist shows
/// under a section, with grips to drag them into order; the stack playing
/// opens into all its patches.
#[component]
pub fn ProfileView(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let lib = use_library(state);
    let perf: PerformanceModel = state.perf.read().clone();
    let l = lib.read().clone();
    let profile = perf.profile_name.clone();
    let views = profile_stacks(&perf, &l);
    let patches: usize = views.iter().map(|v| v.patches.len()).sum();
    let count = views.len();
    let drag = use_signal(|| None::<(usize, usize)>);
    let rows: Rows = use_signal(Vec::new);
    let colour = name_colour(&profile).to_string();
    let own = part_patch(&perf);
    let live = is_live(&perf, own.as_deref());
    let full: Vec<usize> = views.iter().map(|v| v.index).collect();
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            Header { title: profile.clone(), sub: format!("{count} stacks · {patches} patches"), icon: Some(profile.clone()) }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding-top: 8px;",
                div { style: "position: relative; padding: 2px 0 8px 8px;",
                    div { style: "background: rgba(0,0,0,0.18); display: flex; flex-direction: column; border-bottom: 1px solid {RULE};",
                        for (k, v) in views.iter().cloned().enumerate() {
                            {
                                let (lifted, line) = match drag() {
                                    Some((from, to)) if to == k && from != k => (false, Some(to < from)),
                                    Some((from, _)) => (from == k, None),
                                    None => (false, None),
                                };
                                let rig = rig.clone();
                                let full = full.clone();
                                rsx! {
                                    div {
                                        key: "{v.name}",
                                        style: "position: relative; display: flex; flex-direction: column; opacity: {pick(lifted, 0.5, 1.0)};",
                                        onmounted: move |e| keep_row(rows, k, &e),
                                        StackRow {
                                            view: v.clone(),
                                            shown: k,
                                            count,
                                            prev: k.checked_sub(1).map(|p| full[p]),
                                            next: full.get(k + 1).copied(),
                                            lib: l.clone(),
                                            profile: profile.clone(),
                                            part: perf.parts.get(perf.part_index as usize).cloned().unwrap_or_default(),
                                            own: own.clone(),
                                            live,
                                            where_: "this song".to_string(),
                                            song_colour: colour.clone(),
                                            in_song: false,
                                            grip: move |_: PointerEvent| {
                                                let (rig, full) = (rig.clone(), full.clone());
                                                begin_row_drag(bus, drag, rows, count, k, 16.0, move |from, to| {
                                                    let (a, b) = (full[from], full[to]);
                                                    call!(rig, |r| r.move_stack(a as u32, b as u32));
                                                });
                                            },
                                        }
                                        if v.on {
                                            StackPatches { view: v.clone() }
                                        }
                                        if let Some(above) = line {
                                            span { style: "position: absolute; left: 0; right: 0; {pick(above, \"top\", \"bottom\")}: 0px; height: 2px; background: {INK_2}; z-index: 4; pointer-events: none;" }
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
}

/// Every patch in the stack playing: tap one to play it; ⋯ (or a
/// long-press) to rename, move or remove it; "Add a patch" at the foot.
#[component]
fn StackPatches(view: StackView) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let tape = tape_mark(&view.name);
    let stack = view.name.clone();
    let patches: Vec<String> = view.patches.iter().map(|p| p.0.clone()).collect();
    let n = patches.len();
    let index = view.index;
    let mut add_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    rsx! {
        div { style: "background: rgba(0,0,0,0.25); padding-bottom: 2px;",
            for (k, p) in patches.iter().cloned().enumerate() {
                {
                    let on = k == view.cursor;
                    let at = view.at.get(k).copied().unwrap_or(k);
                    let (rig, stack_name, name) = (rig.clone(), stack.clone(), p.clone());
                    let rig_tap = rig.clone();
                    let items = vec![
                        Item::head(p.clone()),
                        Item::name("rename", "Rename…", p.clone(), "Rename", patches.clone()),
                        Item::run("up", "Move up").unless((k == 0).then(|| "Already first".to_string())),
                        Item::run("down", "Move down").unless((k + 1 == n).then(|| "Already last".to_string())),
                        Item::Sep,
                        Item::delete("remove", "Remove from the stack").unless((n <= 1).then(|| "A stack keeps at least one patch".to_string())),
                    ];
                    let to_up = k.checked_sub(1).and_then(|j| view.at.get(j).copied()).unwrap_or(at);
                    let to_down = view.at.get(k + 1).copied().unwrap_or(at);
                    let on_pick = EventHandler::new(move |x: Picked| {
                        let (stack_name, name) = (stack_name.clone(), name.clone());
                        match x.id.as_str() {
                            "rename" => {
                                let new_name = x.text.clone();
                                call!(rig, |r| r.rename_patch(name, new_name));
                            }
                            "up" => call!(rig, |r| r.move_stack_patch(stack_name, at as u32, to_up as u32)),
                            "down" => call!(rig, |r| r.move_stack_patch(stack_name, at as u32, to_down as u32)),
                            "remove" => call!(rig, |r| r.set_stack_patch(stack_name, name, false)),
                            _ => {}
                        }
                    });
                    rsx! {
                        PressMenu {
                            key: "{p}",
                            items: items.clone(),
                            on_pick,
                            style: "position: relative; display: flex; align-items: center; min-height: 44px; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)};",
                            button {
                                style: "flex: 1; min-width: 0; display: flex; align-items: center; justify-content: flex-start; gap: 10px; min-height: 44px; padding: 0 4px 0 52px; text-align: left; border: none; background: transparent; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                // Play this one: the stack lands on it.
                                onclick: move |_| call!(rig_tap, |r| r.play_stack_patch(index as u32, at as u32)),
                                span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {state_dot(on, tape)}" }
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p}" }
                            }
                            MoreButton { label: format!("{p} actions"), items, on_pick }
                        }
                    }
                }
            }
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 12px 0 52px; border: none; background: transparent; color: {INK_3}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onmounted: move |e| add_el.set(Some(e.data())),
                onclick: {
                    let (rig, stack, patches) = (rig.clone(), stack.clone(), patches.clone());
                    move |_| {
                        let (rig, stack) = (rig.clone(), stack.clone());
                        open_naming_under(host, add_el.peek().clone(), Item::name("add", format!("New patch in {stack}…"), format!("{stack} {}", n + 1), "Add", patches.clone()), EventHandler::new(move |p: Picked| {
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
