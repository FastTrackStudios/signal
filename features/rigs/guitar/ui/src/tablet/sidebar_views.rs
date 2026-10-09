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
pub(super) fn use_compositions(state: RigViewState) -> Signal<CompositionModel> {
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
fn Header(title: String, sub: String, icon: Option<String>, #[props(default)] actions: Vec<Item>, #[props(default)] on_pick: Option<EventHandler<Picked>>) -> Element {
    rsx! {
        header { style: "flex-shrink: 0; height: {HEADER_H}px; display: flex; align-items: center; gap: 12px; padding: 0 16px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
            if let Some(p) = icon {
                ProfileIcon { name: p.clone(), colour: name_colour(&p).to_string(), size: 22 }
            }
            span { style: "display: flex; flex-direction: column; gap: 3px; min-width: 0;",
                span { style: "font-size: 22px; font-weight: 750; letter-spacing: -0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
                span { style: "font-size: 13px; color: {INK_3};", "{sub}" }
            }
            if let Some(pick) = on_pick.filter(|_| !actions.is_empty()) {
                span { style: "flex: 1;" }
                MoreButton { label: format!("{title} actions"), items: actions.clone(), on_pick: pick }
            }
        }
    }
}

// ── Presets ────────────────────────────────────────────────────────────────

/// A window at most `max_h` tall onto what it holds, with a scrollbar that
/// says there is more and where it is. Its content's height is measured
/// (as it mounts, as it scrolls, and when `watch` changes — a stack opened,
/// a variation added), so rows of any height scroll in it.
#[component]
fn ScrollWindow(max_h: f64, #[props(default)] watch: String, children: Element) -> Element {
    let mut el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    // (scroll top, how far it scrolls, window height). Blitz's scroll size
    // is the overflow — how far the content scrolls — not its height; its
    // scroll events say either, by path, so only their scroll top is taken.
    let mut m = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64));
    let measure = move || {
        if let Some(e) = el.peek().clone() {
            // Until it has been laid out (it mounts before layout runs).
            spawn(async move {
                for _ in 0..10 {
                    if let (Ok(size), Ok(rect)) = (e.get_scroll_size().await, e.get_client_rect().await)
                        && rect.height() > 0.0
                    {
                        let next = (m.peek().0, size.height, rect.height());
                        if *m.peek() != next {
                            m.set(next);
                        }
                        return;
                    }
                    architect::platform::sleep(std::time::Duration::from_millis(30)).await;
                }
            });
        }
    };
    use_effect(use_reactive!(|watch| {
        let _ = watch;
        measure();
    }));
    let (top, overflow, view) = m();
    let total = view + overflow;
    let more = view > 0.0 && overflow > 0.5;
    let thumb = if more { (view / total * view).max(24.0) } else { 0.0 };
    let at = if more { (top / overflow).clamp(0.0, 1.0) * (view - thumb) } else { 0.0 };
    rsx! {
        div { style: "position: relative;",
            div {
                style: "max-height: {max_h}px; overflow-y: auto;",
                onmounted: move |e| {
                    el.set(Some(e.data()));
                    measure();
                },
                onscroll: move |e| {
                    let (_, o, v) = *m.peek();
                    m.set((e.data().scroll_top(), o, v));
                    measure();
                },
                {children}
            }
            if more {
                div { style: "position: absolute; top: 4px; bottom: 4px; right: 3px; width: 4px; border-radius: 2px; background: {FILL_ON}; pointer-events: none;",
                    div { style: "position: absolute; left: 0; right: 0; top: {at}px; height: {thumb - 8.0}px; border-radius: 2px; background: rgba(255,255,255,0.45);" }
                }
            }
        }
    }
}

/// The search at a sidebar's foot: the whole browser, on `kind`, the
/// keyboard up.
#[component]
fn SearchFoot(label: &'static str, kind: &'static str, at: String, variation: String) -> Element {
    let focus = try_use_context::<super::routing::BrowserFocus>();
    let picker = try_use_context::<super::setlist::PickPart>();
    rsx! {
        button {
            style: "display: flex; align-items: center; gap: 8px; height: {HIT}px; padding: 0 12px; border: none; border-radius: {R}; background: {FILL}; color: {INK_3}; font-size: 15px; font-family: {FONT}; text-align: left; cursor: pointer;",
            onclick: move |_| {
                if let Some(super::routing::BrowserFocus(mut f)) = focus {
                    f.set(Some(super::routing::Focus { kind: kind.into(), preset: at.clone(), variation: variation.clone(), search: true }));
                }
                if let Some(pick) = picker.as_ref() {
                    pick.open.call(());
                }
            },
            svg { width: "16", height: "16", view_box: "0 0 16 16",
                circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                path { d: "M10.6 10.6 14 14", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
            }
            "{label}"
        }
    }
}

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
/// four rows tall; the rest scrolling under it; at the foot the kinds of
/// sound, and a search that opens the whole browser on the presets.
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
    let playing = c.active_preset.clone();
    let loaded = c.presets.iter().find(|p| p.name == playing).cloned();
    let fits = |p: &signal_guitar_proto::PresetEntry| role().is_none_or(|r| p.snapshots.iter().any(|s| role_of(&s.name) == Some(r)));
    let rest: Vec<signal_guitar_proto::PresetEntry> = c.presets.iter().filter(|p| p.name != playing && fits(p)).cloned().collect();
    let (playing_for_browser, snapshot_for_browser) = (playing.clone(), c.active_snapshot.clone());
    // The loaded preset's menu: a new one (a copy of it), its name, gone.
    let preset_names: Vec<String> = c.presets.iter().map(|p| p.name.clone()).collect();
    let preset_actions = vec![
        Item::head(playing.clone()),
        Item::name("new", "New preset…", format!("{playing} 2"), "Add", preset_names.clone()),
        Item::name("rename", "Rename…", playing.clone(), "Rename", preset_names.clone()),
        Item::Sep,
        Item::delete("delete", "Delete preset").unless((preset_names.len() <= 1).then(|| "The only preset".to_string())),
    ];
    let next_preset = c.presets.iter().find(|p| p.name != playing).map(|p| (p.name.clone(), p.snapshots.first().map(|s| s.name.clone()).unwrap_or_default()));
    let on_preset = {
        let (rig, from, variation) = (rig.clone(), playing.clone(), c.active_snapshot.clone());
        EventHandler::new(move |p: Picked| {
            let (from, name, variation) = (from.clone(), p.text.clone(), variation.clone());
            match p.id.as_str() {
                "new" => call!(rig, |r| async move {
                    let _ = r.duplicate_rig_preset(from, name.clone()).await;
                    r.choose_preset(name, variation).await
                }),
                "rename" => call!(rig, |r| r.rename_rig_preset(from, name)),
                // It is the one loaded: load another, then let it go.
                "delete" => {
                    let next = next_preset.clone();
                    call!(rig, |r| async move {
                        if let Some((p, v)) = next {
                            let _ = r.choose_preset(p, v).await;
                        }
                        r.delete_rig_preset(from).await
                    });
                }
                _ => {}
            }
        })
    };
    let chip = |on: bool| format!("height: {HIT}px; padding: 0 12px; border-radius: 999px; border: 1px solid {}; background: {}; color: {}; font-size: 13px; font-weight: 650; font-family: {FONT}; display: flex; align-items: center; gap: 6px; cursor: pointer; flex-shrink: 0;", pick(on, INK_2, RULE_STRONG), pick(on, "rgba(255,255,255,0.08)", "transparent"), pick(on, INK, INK_2));
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            // The one loaded, held: big, its variations four rows tall.
            if let Some(p) = loaded.clone() {
                div { style: "flex-shrink: 0; border-bottom: 1px solid {RULE}; background: {ROW_ON};",
                    div { style: "display: flex; align-items: center; gap: 8px; padding: 14px 6px 8px 16px;",
                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                            span { style: "font-size: 12px; font-weight: 800; letter-spacing: 0.1em; color: {LIVE};", "LOADED" }
                            span { style: "font-size: 22px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                            span { style: "font-size: 13px; color: {INK_3};", "{c.active_snapshot} · {p.snapshots.len()} variations" }
                        }
                        MoreButton { label: format!("{} actions", p.name), items: preset_actions.clone(), on_pick: on_preset }
                    }
                    ScrollWindow { max_h: VARIATION_ROW * 4.0, watch: format!("{}", p.snapshots.len()),
                        Variations { preset: p.name.clone(), variations: p.snapshots.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), playing: (c.active_preset.clone(), c.active_snapshot.clone()) }
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
                        let items = vec![
                            Item::head(p.name.clone()),
                            Item::run("load", "Load"),
                            Item::name("new", "New preset…", format!("{} 2", p.name), "Add", preset_names.clone()),
                            Item::name("rename", "Rename…", p.name.clone(), "Rename", preset_names.clone()),
                            Item::Sep,
                            Item::delete("delete", "Delete preset"),
                        ];
                        let on_pick = {
                            let (rig, from, first) = (rig.clone(), p.name.clone(), first.clone());
                            EventHandler::new(move |x: Picked| {
                                let (from, name, first) = (from.clone(), x.text.clone(), first.clone());
                                match x.id.as_str() {
                                    "load" => call!(rig, |r| r.choose_preset(from, first)),
                                    "new" => call!(rig, |r| async move {
                                        let _ = r.duplicate_rig_preset(from, name.clone()).await;
                                        r.choose_preset(name, first).await
                                    }),
                                    "rename" => call!(rig, |r| r.rename_rig_preset(from, name)),
                                    "delete" => call!(rig, |r| r.delete_rig_preset(from)),
                                    _ => {}
                                }
                            })
                        };
                        rsx! {
                            div { key: "{p.name}", style: "border-bottom: 1px solid {RULE};",
                                div { style: "display: flex; align-items: center; padding-right: 6px;",
                                button {
                                    style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 0 14px 0 16px; text-align: left; border: none; background: {CLEAR}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        open.set(if is_open { None } else { Some(name.clone()) });
                                        if !is_open {
                                            let (n, f) = (name.clone(), first.clone());
                                            call!(rig, |r| r.choose_preset(n, f));
                                        }
                                    },
                                    span { style: "width: 9px; height: 9px; border-radius: 2px; flex-shrink: 0; background: {swatch};" }
                                    span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                    svg { key: "{is_open}", width: "10", height: "6", view_box: "0 0 10 6",
                                        path { d: pick(is_open, "M1 5 L5 1 L9 5", "M1 1 L5 5 L9 1"), fill: "none", stroke: INK_3, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                                    }
                                }
                                MoreButton { label: format!("{} actions", p.name), items, on_pick }
                                }
                                if is_open {
                                    Variations { preset: p.name.clone(), variations: shown, playing: (c.active_preset.clone(), c.active_snapshot.clone()) }
                                }
                            }
                        }
                    }
                }
            }
            // At the foot: the kinds of sound, and the search — the whole
            // browser, on the presets.
            div { style: "flex-shrink: 0; display: flex; flex-direction: column-reverse; gap: 10px; padding: 12px 14px; border-top: 1px solid {RULE};",
                SearchFoot { label: "Search presets", kind: "presets", at: playing_for_browser.clone(), variation: snapshot_for_browser.clone() }
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
    // The profile's menu: a new one (a copy of this), its name, the others.
    let names: Vec<String> = l.profiles.iter().map(|p| p.name.clone()).collect();
    let profile_actions = vec![
        Item::head(profile.clone()),
        Item::name("new", "New profile…", format!("{profile} 2"), "Add", names.clone()),
        Item::name("rename", "Rename…", profile.clone(), "Rename", names.clone()),
        Item::run("all", "All profiles…").detail(format!("{}", names.len())),
        Item::Sep,
        Item::delete("delete", "Delete profile").unless((names.len() <= 1).then(|| "The only profile".to_string())),
    ];
    let focus = try_use_context::<super::routing::BrowserFocus>();
    let picker = try_use_context::<super::setlist::PickPart>();
    let others: Vec<String> = names.iter().filter(|n| **n != profile).cloned().collect();
    let mut drawer = use_signal(|| false);
    let mut query = use_signal(String::new);
    let q = query().trim().to_lowercase();
    let rest: Vec<signal_guitar_proto::ProfileEntry> = l.profiles.iter().filter(|p| p.name != profile && (q.is_empty() || p.name.to_lowercase().contains(&q) || p.stacks.iter().any(|st| st.to_lowercase().contains(&q)))).cloned().collect();
    let on_profile = {
        let (rig, profile) = (rig.clone(), profile.clone());
        Some(EventHandler::new(move |p: Picked| {
            let (from, name) = (profile.clone(), p.text.clone());
            match p.id.as_str() {
                "new" => call!(rig, |r| async move {
                    let _ = r.add_profile(name.clone(), from).await;
                    r.select_profile(name).await
                }),
                "rename" => call!(rig, |r| r.rename_profile(from, name)),
                // It is the one playing: play another, then let it go.
                "delete" => {
                    let next = others.first().cloned().unwrap_or_default();
                    call!(rig, |r| async move {
                        let _ = r.select_profile(next).await;
                        r.delete_profile(from).await
                    });
                }
                "all" => {
                    if let Some(super::routing::BrowserFocus(mut f)) = focus {
                        f.set(Some(super::routing::Focus { kind: "profiles".into(), preset: from, variation: String::new(), search: false }));
                    }
                    if let Some(pick) = picker.as_ref() {
                        pick.open.call(());
                    }
                }
                _ => {}
            }
        }))
    };
    rsx! {
        section { style: "position: relative; height: 100%; display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: {SHEET}; font-family: {FONT}; color: {INK};",
            // The one loaded, whole: its name, then every stack (the one
            // playing open on its patches), scrolling.
            div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; background: {ROW_ON};",
                div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 14px 6px 8px 16px; border-bottom: 1px solid {RULE};",
                    ProfileIcon { name: profile.clone(), colour: colour.clone(), size: 24 }
                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                        span { style: "font-size: 12px; font-weight: 800; letter-spacing: 0.1em; color: {LIVE};", "LOADED" }
                        span { style: "font-size: 22px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{profile}" }
                        span { style: "font-size: 13px; color: {INK_3};", "{count} stacks · {patches} patches" }
                    }
                    if let Some(pick) = on_profile {
                        MoreButton { label: format!("{profile} actions"), items: profile_actions.clone(), on_pick: pick }
                    }
                }
                div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                    div { style: "position: relative; padding: 0 0 8px 0;",
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
            // At the foot: the other profiles, in a drawer.
            div { style: "flex-shrink: 0; display: flex; flex-direction: column; padding: 12px 14px; border-top: 1px solid {RULE};",
                button {
                    style: "display: flex; align-items: center; gap: 10px; height: 44px; padding: 0 14px; border: none; border-radius: {R}; background: {FILL}; color: {INK_2}; font-size: 15px; font-weight: 650; font-family: {FONT}; text-align: left; cursor: pointer;",
                    onclick: move |_| {
                        query.set(String::new());
                        drawer.set(true);
                    },
                    svg { width: "16", height: "16", view_box: "0 0 16 16",
                        circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                        path { d: "M10.6 10.6 14 14", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                    span { style: "flex: 1;", "Profiles" }
                    span { style: "font-size: 13px; color: {INK_3};", "{names.len()}" }
                }
            }
            // The drawer: search, and every other profile — a tap loads it.
            if drawer() {
                div { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; z-index: 5; background: rgba(0,0,0,0.5);", onclick: move |_| drawer.set(false) }
                div { style: "position: absolute; left: 0; right: 0; bottom: 0; top: 22%; z-index: 6; display: flex; flex-direction: column; background: {SHEET}; border-top: 1px solid {RULE_STRONG}; border-radius: {R_MD} {R_MD} 0 0;",
                    div { style: "flex-shrink: 0; display: flex; justify-content: center; padding: 8px 0 2px;",
                        span { style: "width: 36px; height: 5px; border-radius: 3px; background: {RULE_STRONG};" }
                    }
                    div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 8px; padding: 8px 10px 10px 14px;",
                        label { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 12px; border-radius: {R}; background: {FILL};",
                            svg { width: "15", height: "15", view_box: "0 0 16 16",
                                circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                                path { d: "M10.6 10.6 14 14", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                            }
                            input {
                                value: "{query}",
                                placeholder: "Search profiles",
                                style: "flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT};",
                                oninput: move |e| query.set(e.value()),
                            }
                        }
                        button {
                            style: "height: {HIT}px; padding: 0 10px; border: none; background: transparent; font-size: 14px; font-weight: 650; color: {INK_2}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| drawer.set(false),
                            "Done"
                        }
                    }
                    div { style: "flex: 1; min-height: 0; overflow-y: auto; border-top: 1px solid {RULE};",
                if rest.is_empty() {
                    super::setlist::EmptyLine { text: "No other profiles".to_string() }
                }
                for p in rest.into_iter() {
                    {
                        let name = p.name.clone();
                        let rig = rig.clone();
                        let items = vec![
                            Item::head(p.name.clone()),
                            Item::run("load", "Load"),
                            Item::name("new", "New profile…", format!("{} 2", p.name), "Add", names.clone()),
                            Item::name("rename", "Rename…", p.name.clone(), "Rename", names.clone()),
                            Item::Sep,
                            Item::delete("delete", "Delete profile"),
                        ];
                        let on_pick = {
                            let (rig, from) = (rig.clone(), p.name.clone());
                            EventHandler::new(move |x: Picked| {
                                let (from, name) = (from.clone(), x.text.clone());
                                match x.id.as_str() {
                                    "load" => call!(rig, |r| r.select_profile(from)),
                                    "new" => call!(rig, |r| async move {
                                        let _ = r.add_profile(name.clone(), from).await;
                                        r.select_profile(name).await
                                    }),
                                    "rename" => call!(rig, |r| r.rename_profile(from, name)),
                                    "delete" => call!(rig, |r| r.delete_profile(from)),
                                    _ => {}
                                }
                            })
                        };
                        rsx! {
                            div { key: "{p.name}", style: "display: flex; align-items: center; border-bottom: 1px solid {RULE}; padding-right: 6px;",
                            button {
                                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; min-height: 60px; padding: 8px 6px 8px 16px; text-align: left; border: none; background: {CLEAR}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| {
                                    let n = name.clone();
                                    call!(rig, |r| r.select_profile(n));
                                    drawer.set(false);
                                },
                                ProfileIcon { name: p.name.clone(), colour: name_colour(&p.name).to_string(), size: 18 }
                                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 5px;",
                                    span { style: "font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                    // Its stacks, as their tapes.
                                    span { style: "display: flex; gap: 4px;",
                                        for (i, st) in p.stacks.iter().enumerate() {
                                            span { key: "{i}", style: "width: 16px; height: 4px; border-radius: 2px; background: {tape_mark(st)};" }
                                        }
                                    }
                                }
                            }
                            MoreButton { label: format!("{} actions", p.name), items, on_pick }
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
