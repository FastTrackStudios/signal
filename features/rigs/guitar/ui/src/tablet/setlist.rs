//! The tablet's setlist sidebar — the touch prototype's `setlist/Setlist.tsx`
//! on the live rig.
//!
//!   Header   the set's name (a tap opens the sets), event · date · profile
//!            in words, the set as a bar of song colours; ⋯ manages the set.
//!   Songs    number on the song's colour, name, Now / Next, key, tempo, ⋯.
//!            The song up opens into its sections (a tap on it folds them).
//!            Reorder mode grows handles to drag songs into order.
//!   Sections a part's `section` groups consecutive parts; each shows the
//!            patch it plays, its changes in words, and a ⋯; its grip drags
//!            it. The section playing opens into its parts and the stacks.
//!   Stacks   name in its colour, the patch a tap plays, its rotation as
//!            dots (where each patch comes from), what the next tap plays;
//!            ⋯ jumps to a patch, keeps or drops a by-hand pick, makes the
//!            patch the section's, and manages the stack.
//!
//! The pickers (sets, details, add songs, key, tempo, profile, colour, the
//! start, a section's patch) slide over the list — see `panels`.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LibraryModel, PerfPart, PerfStack, PerformanceModel};
use signal_widgets::drag_bus::{DragBus, DragEvent};
use signal_widgets::PopupHost;

use super::colors::{date_label, name_colour, section_colour, set_heading, set_meta, song_colour, suggest_section, when_label};
use super::marks::{module_colour, OverrideIcon};
use super::menu::{open_naming, Item, MoreButton, Picked};
use super::panels::{Panel, PanelView};
use super::tokens::*;
use crate::state::RigViewState;

/// A rig call from an event handler: clone the client, run it, ignore the
/// reply (the state stream brings the result).
macro_rules! call {
    ($rig:expr, |$r:ident| $body:expr) => {{
        if let Some($r) = $rig.clone() {
            // Forever, not the component's: a panel that closes itself
            // would take the call with it.
            let _ = dioxus_core::spawn_forever(async move {
                let _ = $body.await;
            });
        }
    }};
}
pub(crate) use call;

/// The library, refetched whenever the rig's state moves on.
pub fn use_library(state: RigViewState) -> Signal<LibraryModel> {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut lib = use_signal(LibraryModel::default);
    use_effect(move || {
        let _revision = state.perf.read().revision;
        if let Some(r) = rig.clone() {
            spawn(async move {
                if let Ok(l) = r.library().await {
                    lib.set(l);
                }
            });
        }
    });
    lib
}

/// The stack a patch of `profile` lives in.
pub fn stack_of(lib: &LibraryModel, profile: &str, patch: &str) -> String {
    let in_profile = |p: &signal_guitar_proto::ProfileEntry| p.patch_list.iter().find(|x| x.name.eq_ignore_ascii_case(patch)).map(|x| x.stack.clone());
    lib.profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(profile))
        .and_then(in_profile)
        .or_else(|| lib.profiles.iter().find_map(in_profile))
        .unwrap_or_default()
}

/// The profile a patch belongs to, when it is another than `profile`'s:
/// a borrowed patch says from where.
fn borrowed_from(lib: &LibraryModel, profile: &str, patch: &str) -> Option<String> {
    if patch.is_empty() || lib.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(profile) && p.patch_list.iter().any(|x| x.name.eq_ignore_ascii_case(patch))) {
        return None;
    }
    lib.profiles.iter().find(|p| p.patch_list.iter().any(|x| x.name.eq_ignore_ascii_case(patch))).map(|p| p.name.clone())
}

/// A section: consecutive parts that share a `section` name (a part with
/// none is its own).
#[derive(Clone, PartialEq)]
pub struct Section {
    pub name: String,
    /// Indices into `PerformanceModel::parts`.
    pub parts: Vec<usize>,
}

pub fn sections_of(parts: &[PerfPart]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let name = if p.section.is_empty() { p.name.clone() } else { p.section.clone() };
        match out.last_mut() {
            Some(last) if !p.section.is_empty() && last.name == name => last.parts.push(i),
            _ => out.push(Section { name, parts: vec![i] }),
        }
    }
    out
}

/// Reorder the song's parts to `want` (a permutation of their indices),
/// one `move_part` at a time, in order — what moving a section is.
pub fn reorder_parts(rig: Option<RigClient>, want: Vec<usize>) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        let mut now: Vec<usize> = (0..want.len()).collect();
        for (pos, &part) in want.iter().enumerate() {
            let Some(at) = now.iter().position(|&p| p == part) else { continue };
            if at != pos {
                let _ = r.move_part(at as u32, pos as u32).await;
                let p = now.remove(at);
                now.insert(pos, p);
            }
        }
    });
}

/// The parts in order with section `from` moved to section position `to`.
fn section_moved(sections: &[Section], from: usize, to: usize) -> Vec<usize> {
    let mut order: Vec<Section> = sections.to_vec();
    if from < order.len() && to < order.len() {
        let s = order.remove(from);
        order.insert(to, s);
    }
    order.into_iter().flat_map(|s| s.parts).collect()
}

// ── The sidebar ────────────────────────────────────────────────────────────

/// Where a dragged row lands: rows of `row` pt from `from`, moved `dy`.
fn drop_index(from: usize, dy: f64, row: f64, count: usize) -> usize {
    let moved = (dy / row).round() as i64;
    (from as i64 + moved).clamp(0, count.saturating_sub(1) as i64) as usize
}

#[component]
pub fn TabletSetlist(state: RigViewState) -> Element {
    let lib = use_library(state);
    let perf = state.perf.read().clone();
    let lib_now = lib.read().clone();
    let songs = perf.songs.clone();
    let mut panel = use_signal(|| None::<Panel>);
    let reordering = use_signal(|| false);
    // The song up folds its sections away on a tap.
    let mut folded = use_signal(|| false);
    let at = perf.song_index as usize;
    let mut last_at = use_signal(|| at);
    if *last_at.peek() != at {
        last_at.set(at);
        folded.set(false);
    }
    // A song being dragged into order: (from, to).
    let drag = use_signal(|| None::<(usize, usize)>);
    rsx! {
        section { style: "position: relative; height: 100%; display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: {SHEET}; font-family: {FONT}; color: {INK};",
            SetHeader { perf: perf.clone(), lib: lib_now.clone(), panel, reordering }
            div { style: "position: relative; flex: 1; min-height: 0; overflow-y: auto;",
                for (i, song) in songs.iter().enumerate() {
                    div { key: "{i}-{song.name}",
                        SongRow {
                            perf: perf.clone(),
                            lib: lib_now.clone(),
                            index: i,
                            open: i == at && !folded() && !reordering(),
                            reordering: reordering(),
                            drag,
                            panel,
                            on_toggle: move |()| folded.toggle(),
                        }
                        if i == at && !folded() && !reordering() {
                            Sections { perf: perf.clone(), lib: lib_now.clone(), panel }
                        }
                    }
                }
                if !songs.is_empty() && !reordering() {
                    button {
                        style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 52px; padding: 0 18px; border: none; border-top: 1px solid {RULE}; background: transparent; color: {INK_3}; font-size: 15px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                        onclick: move |_| panel.set(Some(Panel::Add)),
                        Plus { size: 14 }
                        "Add songs"
                    }
                }
                if songs.is_empty() {
                    div { style: "padding: 28px 20px; display: flex; flex-direction: column; align-items: flex-start; gap: 12px;",
                        div { style: "font-size: 18px; font-weight: 700;", "No songs yet" }
                        PrimaryButton { label: "Add songs", onclick: move |()| panel.set(Some(Panel::Add)) }
                    }
                }
            }
            if let Some(p) = panel() {
                PanelView { panel: p, perf: perf.clone(), lib: lib_now.clone(), on_close: move |()| panel.set(None) }
            }
        }
    }
}

#[component]
pub fn Plus(size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 12 12", style: "flex-shrink: 0;",
            path { d: "M6 1v10M1 6h10", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
        }
    }
}

/// The primary button: the one thing to do here.
#[component]
pub fn PrimaryButton(label: String, onclick: EventHandler<()>, disabled: Option<bool>) -> Element {
    let off = disabled.unwrap_or(false);
    rsx! {
        button {
            disabled: off,
            style: "min-height: 48px; padding: 0 18px; border-radius: {R}; border: 1px solid {pick(off, RULE_STRONG, DIM)}; background: {pick(off, CLEAR, DIM)}; color: {pick(off, INK_3, \"#ffffff\")}; font-size: 15px; font-weight: 650; font-family: {FONT}; white-space: nowrap; cursor: pointer;",
            onclick: move |_| if !off { onclick.call(()) },
            "{label}"
        }
    }
}

// ── Header ─────────────────────────────────────────────────────────────────

#[component]
fn SetHeader(perf: PerformanceModel, lib: LibraryModel, panel: Signal<Option<Panel>>, reordering: Signal<bool>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let set_index = perf.setlist_index as usize;
    let entry = lib.setlists.get(set_index).cloned().unwrap_or_default();
    let name = perf.setlists.get(set_index).cloned().unwrap_or_else(|| entry.name.clone());
    let meta = set_meta(&name, &entry.event, &entry.date, &entry.title);
    let heading = set_heading(&meta);
    let date = date_label(&meta.date);
    let when = when_label(&meta.date);
    // Only today and tomorrow are worth a word beside the date.
    let soon = (when == "Today" || when == "Tomorrow").then(|| when.clone());
    let profile = if entry.profile.is_empty() { perf.profile_name.clone() } else { entry.profile.clone() };
    let count = perf.songs.len();
    let at = perf.song_index as usize;
    let sets = perf.setlists.len();

    let actions = vec![
        Item::head(heading.clone()),
        Item::run("add", "Add songs…"),
        Item::run("edit", "Edit details…").detail("event · date · title"),
        Item::run("profile", "Default profile…").detail(if entry.profile.is_empty() { "the rig's".to_string() } else { entry.profile.clone() }),
        Item::run("new", "New set…"),
        Item::run("duplicate", "Duplicate for next week"),
        Item::run("sets", "All sets…").detail(format!("{sets}")),
        Item::run("reorder", "Reorder songs").unless((count < 2).then(|| "Fewer than two songs".to_string())),
        Item::Sep,
        Item::delete("delete", "Delete set").unless((sets <= 1).then(|| "The only set — make another first".to_string())),
    ];
    let mut panel_w = panel;
    let mut reorder_w = reordering;

    rsx! {
        header { style: "flex-shrink: 0; height: {HEADER_H}px; padding: 0 6px 0 18px; border-bottom: 1px solid {RULE}; display: flex; flex-direction: column; justify-content: center; gap: 7px; box-sizing: border-box;",
            div { style: "display: flex; align-items: center; gap: 2px;",
                // The set's name and a chevron: a tap opens the sets.
                button {
                    style: "flex: 1; min-width: 0; min-height: 44px; display: flex; flex-direction: column; justify-content: center; align-items: flex-start; gap: 3px; text-align: left; padding: 0 8px; margin: 0 0 0 -8px; border: none; border-radius: {R_MD}; background: transparent; color: {INK}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| panel_w.set(Some(Panel::Sets)),
                    span { style: "display: flex; align-items: center; gap: 8px; min-width: 0;",
                        span { style: "min-width: 0; font-size: 21px; font-weight: 750; letter-spacing: -0.02em; line-height: 1.1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{heading}" }
                        svg { width: "10", height: "6", view_box: "0 0 10 6", style: "flex-shrink: 0;",
                            path { d: "M1 1 L5 5 L9 1", fill: "none", stroke: INK_3, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                        }
                    }
                    // Event · date · profile — words, no chips.
                    div { style: "display: flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 560; color: {INK_3}; white-space: nowrap; overflow: hidden;",
                        if !meta.title.is_empty() && !meta.event.is_empty() {
                            span { style: "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; background: {name_colour(&meta.event)};" }
                            span { style: "color: {INK_2}; font-weight: 650;", "{meta.event}" }
                            span { "·" }
                        }
                        span { if date.is_empty() { "No date" } else { "{date}" } }
                        if let Some(w) = soon.clone() {
                            span { style: "color: {pick(w == \"Today\", LIVE, INK_2)}; font-weight: 650;", "{w}" }
                        }
                        span { "·" }
                        span { style: "overflow: hidden; text-overflow: ellipsis;", "{profile}" }
                    }
                }
                if reordering() {
                    PrimaryButton { label: "Done", onclick: move |()| reorder_w.set(false) }
                    span { style: "width: 6px;" }
                } else {
                    MoreButton {
                        items: actions,
                        label: "Set actions".to_string(),
                        on_pick: {
                            let rig = rig.clone();
                            move |p: Picked| match p.id.as_str() {
                                "add" => panel_w.set(Some(Panel::Add)),
                                "edit" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::Edit))),
                                "profile" => panel_w.set(Some(Panel::Profile(None))),
                                "new" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::New))),
                                "duplicate" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::Duplicate))),
                                "sets" => panel_w.set(Some(Panel::Sets)),
                                "reorder" => reorder_w.set(true),
                                "delete" => call!(rig, |r| r.delete_setlist(set_index as u32)),
                                _ => {}
                            }
                        },
                    }
                }
            }
            if count > 0 {
                // The set as a bar: a segment per song in its colour — played
                // ones dim, the one up full and taller — and its place.
                div { style: "display: flex; align-items: center; gap: 10px; padding-right: 12px;",
                    div { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 3px; height: 10px;",
                        for (i, s) in perf.songs.iter().enumerate() {
                            span {
                                key: "{i}",
                                style: "flex: 1 1 0; height: {pick(i == at, 8, 4)}px; border-radius: 2px; background: {song_colour(&s.name, &s.colour)}; opacity: {pick(i < at, 0.3, pick(i == at, 1.0, 0.6))};",
                            }
                        }
                    }
                    span { style: "flex-shrink: 0; font-size: 13px; font-weight: 650; color: {INK_2}; font-variant-numeric: tabular-nums;",
                        "{at + 1}"
                        span { style: "color: {INK_3}; font-weight: 560;", " / {count}" }
                    }
                }
            }
        }
    }
}

// ── A song ─────────────────────────────────────────────────────────────────

/// A song row's height (reorder drags count in it).
const SONG_ROW_H: f64 = 65.0;

#[component]
fn SongRow(
    perf: PerformanceModel,
    lib: LibraryModel,
    index: usize,
    open: bool,
    reordering: bool,
    drag: Signal<Option<(usize, usize)>>,
    panel: Signal<Option<Panel>>,
    on_toggle: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let Some(song) = perf.songs.get(index).cloned() else { return rsx! {} };
    let at = perf.song_index as usize;
    let up = index == at;
    let played = index < at;
    let next = index == at + 1;
    let count = perf.songs.len();
    let last = count.saturating_sub(1);
    let colour = song_colour(&song.name, &song.colour);
    let lib_song = lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(&song.name)).cloned();
    let sections = lib_song.as_ref().map_or(0, |s| s.parts.len());
    let own_profile = lib_song.as_ref().map(|s| s.profile.clone()).unwrap_or_default();
    let set_profile = lib.setlists.get(perf.setlist_index as usize).map(|s| s.profile.clone()).unwrap_or_default();
    let shown_profile = if own_profile.is_empty() { if set_profile.is_empty() { perf.profile_name.clone() } else { set_profile.clone() } } else { own_profile.clone() };
    let start = lib_song.as_ref().map(|s| s.start_part.clone()).unwrap_or_default();
    let set_index = perf.setlist_index;
    let (dragging, drop_above) = match drag() {
        Some((from, to)) => (from == index, to == index && from != index),
        None => (false, false),
    };

    let items = vec![
        Item::head(format!("{} · {}", index + 1, song.name)),
        Item::run("go", "Play from here").unless(up.then(|| "It's up now".to_string())),
        Item::Sep,
        Item::run("key", "Key…").detail(if song.key.is_empty() { "—".to_string() } else { song.key.clone() }),
        Item::run("tempo", "Tempo…").detail(if song.bpm > 0 { format!("{} BPM", song.bpm) } else { "—".to_string() }),
        Item::run("start", "Starts on…").detail(if start.is_empty() { "default".to_string() } else { start.clone() }),
        Item::run("profile", "Profile…").detail(if own_profile.is_empty() { format!("{shown_profile} (set's)") } else { own_profile.clone() }),
        Item::run("colour", "Colour…").detail(if song.colour.is_empty() { "from its name" } else { "set" }),
        Item::Sep,
        Item::run("up", "Move up").unless((index == 0).then(|| "Already first".to_string())),
        Item::run("down", "Move down").unless((index == last).then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("remove", "Remove from this set"),
    ];
    // Its changes over its patches: the module kinds its sections pick, and
    // whether it has settings changed and not saved to the profile.
    let (modules, unsaved) = if up {
        let mut m: Vec<String> = Vec::new();
        for p in &perf.parts {
            for pk in &p.picks {
                if !pk.kind.starts_with("block:") && !m.contains(&pk.kind) {
                    m.push(pk.kind.clone());
                }
            }
        }
        (m, !perf.song_changes.is_empty())
    } else {
        (Vec::new(), false)
    };
    let mut panel_w = panel;
    let bg = if dragging { SHEET_2.to_string() } else if up { tint(&colour, 7) } else { CLEAR.to_string() };
    let top_rule = if drop_above { format!("border-top: 3px solid {FOCUS_FG};") } else { format!("border-top: 1px solid {RULE};") };

    rsx! {
        div {
            style: "position: relative; display: flex; align-items: stretch; min-height: 64px; {top_rule} background: {bg}; opacity: {pick(dragging, 0.6, 1.0)};",
            if up {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 10px 2px 10px 14px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        if reordering {
                            return;
                        }
                        if up { on_toggle.call(()) } else { call!(rig, |r| r.select_song(index as u32)) }
                    }
                },
                span { style: "width: 28px; height: 28px; border-radius: 7px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 750; color: #0b0b0e; background: {pick(played, tint(&colour, 35), colour.clone())};",
                    "{index + 1}"
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "display: flex; align-items: center; gap: 8px; min-width: 0;",
                        span { style: "min-width: 0; font-size: 17px; font-weight: {pick(up, 750, 600)}; color: {pick(played, INK_3, INK)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; {pick(played, STRIKE, NOTHING)}",
                            "{song.name}"
                        }
                        if up { Badge { live: true, "Now" } }
                        if next { Badge { live: false, "Next" } }
                    }
                    span { style: "display: flex; align-items: center; gap: 6px; font-size: 13px; color: {INK_3}; min-width: 0; white-space: nowrap; overflow: hidden;",
                        if !start.is_empty() {
                            span { style: "flex-shrink: 0; color: {INK_2}; font-weight: 600;", "from {start}" }
                        }
                        if !own_profile.is_empty() {
                            span { style: "flex-shrink: 0; color: {INK_2}; font-weight: 650;", "{own_profile}" }
                        }
                        Changes { modules: modules.clone(), unsaved, max: 2 }
                        if !open && sections > 0 {
                            span { style: "flex-shrink: 0;", "{sections} sections" }
                        }
                    }
                }
                if !song.key.is_empty() {
                    span { style: "flex-shrink: 0; min-width: 30px; height: 28px; padding: 0 6px; box-sizing: border-box; border: 1px solid {RULE_STRONG}; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 650; color: {INK_2}; font-variant-numeric: tabular-nums;",
                        "{song.key}"
                    }
                }
                span { style: "width: 34px; text-align: right; font-size: 14px; font-weight: 600; color: {INK_2}; flex-shrink: 0; font-variant-numeric: tabular-nums;",
                    if song.bpm > 0 { "{song.bpm}" }
                }
            }
            div { style: "display: flex; align-items: center; padding-right: 2px;",
                if reordering {
                    // The handle: drag the song into its place.
                    span {
                        "aria-label": "Drag {song.name}",
                        style: "width: 52px; height: 52px; display: flex; align-items: center; justify-content: center; cursor: grab; touch-action: none;",
                        onpointerdown: {
                            let rig = rig.clone();
                            move |e: PointerEvent| {
                            e.prevent_default();
                            e.stop_propagation();
                            let y0 = e.client_coordinates().y;
                            let mut drag = drag;
                            drag.set(Some((index, index)));
                            let Some(bus) = bus else { return };
                            let rig = rig.clone();
                            bus.begin(move |ev| {
                                let mut drag = drag;
                                match ev {
                                    DragEvent::Move { y, .. } => drag.set(Some((index, drop_index(index, y - y0, SONG_ROW_H, count)))),
                                    DragEvent::End => {
                                        let to = drag.peek().map_or(index, |d| d.1);
                                        drag.set(None);
                                        if to != index {
                                            call!(rig, |r| r.move_setlist_entry(set_index, index as u32, to as u32));
                                        }
                                    }
                                }
                            });
                        }},
                        svg { width: "14", height: "10", view_box: "0 0 14 10",
                            path { d: "M1 1h12M1 5h12M1 9h12", stroke: INK_2, stroke_width: "1.8", stroke_linecap: "round" }
                        }
                    }
                } else {
                    MoreButton {
                        items,
                        label: format!("{} actions", song.name),
                        on_pick: {
                            let rig = rig.clone();
                            move |p: Picked| match p.id.as_str() {
                                "go" => call!(rig, |r| r.select_song(index as u32)),
                                "key" => panel_w.set(Some(Panel::Key(index))),
                                "tempo" => panel_w.set(Some(Panel::Tempo(index))),
                                "start" => panel_w.set(Some(Panel::Start(index))),
                                "profile" => panel_w.set(Some(Panel::Profile(Some(index)))),
                                "colour" => panel_w.set(Some(Panel::Colour(index))),
                                "up" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 - 1)),
                                "down" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 + 1)),
                                "remove" => call!(rig, |r| r.remove_setlist_entry(set_index, index as u32)),
                                _ => {}
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Its own changes, in words: the override icon, each module it picks in
/// that module's colour; "Unsaved" in amber for changes not saved.
#[component]
fn Changes(modules: Vec<String>, unsaved: bool, max: usize) -> Element {
    if modules.is_empty() && !unsaved {
        return rsx! {};
    }
    let shown: Vec<String> = modules.iter().take(max).cloned().collect();
    let more = modules.len().saturating_sub(shown.len());
    let n = shown.len();
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 6px; min-width: 0; font-size: 12px; font-weight: 650; white-space: nowrap; overflow: hidden;",
            if !modules.is_empty() {
                OverrideIcon { colour: INK_2.to_string(), size: 11 }
                for (k, m) in shown.into_iter().enumerate() {
                    span { key: "{m}", style: "color: {lift(module_colour(&m))};",
                        "{m}"
                        if k + 1 < n { span { style: "color: {INK_3};", " ·" } }
                    }
                }
                if more > 0 { span { style: "color: {INK_3};", "+{more}" } }
            }
            if unsaved {
                span { style: "color: {MODIFIED};", "Unsaved" }
            }
        }
    }
}

#[component]
fn Badge(live: bool, children: Element) -> Element {
    let (bg, fg) = if live { (LIVE_BG, LIVE) } else { ("rgba(255,255,255,0.07)", INK_2) };
    rsx! {
        span { style: "flex-shrink: 0; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; background: {bg}; color: {fg};",
            {children}
        }
    }
}

/// A patch with the stack it lands on: the stack named in its colour; a
/// patch borrowed from another profile says from where.
#[component]
pub fn PatchChip(patch: String, stack: String, small: bool, lit: bool, borrowed: Option<String>) -> Element {
    let (tape, _) = crate::perform::folder_color(&stack);
    let gaffer = stack.is_empty() || tape == "#3f3f46";
    let stack_ink = if gaffer { INK_3.to_string() } else { lift(tape) };
    let (pad, size, stack_size) = if small { ("2px 8px 2px 7px", 12, 10.5) } else { ("5px 9px 5px 8px", 13, 11.0) };
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 7px; min-width: 0; max-width: 100%; padding: {pad}; border-radius: 4px; background: {pick(lit, FILL_ON, FILL)}; color: {pick(lit, INK, INK_2)}; font-size: {size}px; font-weight: 600; white-space: nowrap; box-sizing: border-box;",
            if !stack.is_empty() {
                span { style: "flex-shrink: 0; font-size: {stack_size}px; font-weight: 750; letter-spacing: 0.07em; text-transform: uppercase; color: {stack_ink};", "{stack}" }
            }
            span { style: "overflow: hidden; text-overflow: ellipsis;", "{patch}" }
            if let Some(b) = borrowed {
                span { style: "flex-shrink: 0; color: {name_colour(&b)}; font-weight: 650;", "{b}" }
            }
        }
    }
}

// ── Its sections ───────────────────────────────────────────────────────────

/// A section row's height (drags count in it).
const SECTION_ROW_H: f64 = 50.0;

#[component]
fn Sections(perf: PerformanceModel, lib: LibraryModel, panel: Signal<Option<Panel>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let sections = sections_of(&perf.parts);
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let now = perf.part_index as usize;
    let drag = use_signal(|| None::<(usize, usize)>);
    let names: Vec<String> = sections.iter().map(|s| s.name.clone()).collect();
    let part_names: Vec<String> = perf.parts.iter().map(|p| p.name.clone()).collect();
    rsx! {
        div { style: "position: relative; padding: 2px 0 10px; background: {tint(&colour, 7)};",
            span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            for (j, sec) in sections.iter().enumerate() {
                SectionRow { key: "{j}-{sec.name}", perf: perf.clone(), lib: lib.clone(), section: sec.clone(), index: j, sections: sections.clone(), drag, panel }
                if sec.parts.contains(&now) {
                    if sec.parts.len() > 1 {
                        Parts { perf: perf.clone(), lib: lib.clone(), section: sec.clone(), panel }
                    }
                    Stacks { perf: perf.clone(), lib: lib.clone(), section: sec.name.clone() }
                }
            }
            if sections.is_empty() {
                Stacks { perf: perf.clone(), lib: lib.clone(), section: String::new() }
            }
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 14px 0 28px; border: none; background: transparent; color: {INK_2}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onclick: move |e: MouseEvent| {
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let rig = rig.clone();
                    let taken: Vec<String> = names.iter().chain(part_names.iter()).cloned().collect();
                    open_naming(host, c.x - el.x + 20.0, c.y - el.y + 44.0, Item::name("add", "New section…", suggest_section(&names), "Add", taken), EventHandler::new(move |p: Picked| {
                        let name = p.text.clone();
                        call!(rig, |r| r.add_part(name));
                    }));
                },
                span { style: "width: 16px; height: 16px; border-radius: 999px; border: 1px dashed {INK_3}; box-sizing: border-box; display: flex; align-items: center; justify-content: center;",
                    svg { width: "8", height: "8", view_box: "0 0 8 8",
                        path { d: "M4 0.5v7M0.5 4h7", stroke: INK_2, stroke_width: "1.5", stroke_linecap: "round" }
                    }
                }
                "Add a section"
            }
        }
    }
}

/// The modules a section's parts pick of their own, once each.
fn section_modules(perf: &PerformanceModel, sec: &Section) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &k in &sec.parts {
        for pk in perf.parts.get(k).map(|p| p.picks.as_slice()).unwrap_or_default() {
            let kind = pk.kind.strip_prefix("block:").unwrap_or(&pk.kind).to_string();
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
        for o in perf.parts.get(k).map(|p| p.overrides.as_slice()).unwrap_or_default() {
            if !out.iter().any(|b| b.eq_ignore_ascii_case(&o.block)) && perf.parts[k].picks.is_empty() {
                out.push(o.block.clone());
            }
        }
    }
    out
}

/// Pick a part: in Build (or with the browser on call), it is what the
/// browser builds into; `open` opens the browser for it.
#[derive(Clone, Copy)]
pub struct PickPart {
    pub open: Callback<()>,
}

#[component]
fn SectionRow(perf: PerformanceModel, lib: LibraryModel, section: Section, index: usize, sections: Vec<Section>, drag: Signal<Option<(usize, usize)>>, panel: Signal<Option<Panel>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let build_pick = try_use_context::<super::BuildPick>();
    let pick_part = try_use_context::<PickPart>();
    let count = sections.len();
    let picked = build_pick.is_some_and(|b| (b.building)() && (b.part)().is_some_and(|k| section.parts.contains(&k)));
    let now = perf.part_index as usize;
    let first = section.parts.first().copied().unwrap_or(0);
    let state = if section.parts.contains(&now) { "now" } else if first < now { "done" } else { "ahead" };
    let is_now = state == "now";
    let several = section.parts.len() > 1;
    let part = perf.parts.get(first).cloned().unwrap_or_default();
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let profile = if part.profile.is_empty() { perf.profile_name.clone() } else { part.profile.clone() };
    let stack = stack_of(&lib, &profile, &part.patch);
    let borrowed = borrowed_from(&lib, &perf.profile_name, &part.patch);
    let modules = section_modules(&perf, &section);
    let unsaved = perf.song_changes.iter().any(|c| c.patch.eq_ignore_ascii_case(&part.patch)) && !part.patch.is_empty();
    let names: Vec<String> = section.parts.iter().filter_map(|&k| perf.parts.get(k).map(|p| p.name.clone())).collect();
    let section_names: Vec<String> = sections.iter().map(|s| s.name.clone()).filter(|n| *n != section.name).collect();
    let all_parts: Vec<String> = perf.parts.iter().map(|p| p.name.clone()).collect();
    let grouped = !part.section.is_empty();
    let (dragging, drop_at) = match drag() {
        Some((from, to)) => (from == index, to == index && from != index),
        None => (false, false),
    };

    let mut items = vec![
        Item::head(section.name.clone()),
        Item::run("go", "Play from here").unless(is_now.then(|| "It's playing".to_string())),
    ];
    items.push(if several {
        Item::run("parts", "Its parts").detail(format!("{}", section.parts.len()))
    } else {
        Item::run("patch", "Patch…").detail(if part.patch.is_empty() { "keeps".to_string() } else { part.patch.clone() })
    });
    items.extend([
        Item::name("part", "Add a part…", format!("{} · {}", section.name, section.parts.len() + 1), "Add", all_parts.clone()),
        Item::name("rename", "Rename…", section.name.clone(), "Rename", section_names),
        Item::Sep,
        Item::run("earlier", "Move earlier").unless((index == 0).then(|| "Already first".to_string())),
        Item::run("later", "Move later").unless((index + 1 == count).then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("delete", "Delete section"),
    ]);
    let ink = match state {
        "done" => INK_3,
        "now" => INK,
        _ => INK_2,
    };
    let grip = section_colour(&section.name);
    let row_bg = pick(is_now, tint(&colour, pick(several, 10, 18)), CLEAR.to_string());
    let drop_line = if drop_at { format!("border-top: 2px solid {INK_2};") } else { String::new() };
    let last_part = section.parts.last().copied().unwrap_or(first);
    let parts_now = perf.parts.len();

    rsx! {
        div {
            style: "position: relative; display: flex; align-items: center; min-height: 50px; background: {row_bg}; opacity: {pick(dragging, 0.5, 1.0)}; {drop_line}",
            if picked {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; right: 0; border: 2px solid {FOCUS_FG}; border-radius: {R}; box-sizing: border-box; pointer-events: none;" }
            }
            // The grip, in the section's colour: drag to move the section.
            span {
                "aria-label": "Drag {section.name} to move it",
                style: "width: 40px; align-self: stretch; flex-shrink: 0; display: flex; align-items: center; justify-content: flex-end; padding-right: 6px; box-sizing: border-box; cursor: grab; touch-action: none;",
                onpointerdown: {
                    let rig = rig.clone();
                    let sections = sections.clone();
                    move |e: PointerEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        let y0 = e.client_coordinates().y;
                        let mut drag = drag;
                        drag.set(Some((index, index)));
                        let Some(bus) = bus else { return };
                        let (rig, sections) = (rig.clone(), sections.clone());
                        bus.begin(move |ev| {
                            let mut drag = drag;
                            match ev {
                                DragEvent::Move { y, .. } => drag.set(Some((index, drop_index(index, y - y0, SECTION_ROW_H, count)))),
                                DragEvent::End => {
                                    let to = drag.peek().map_or(index, |d| d.1);
                                    drag.set(None);
                                    if to != index {
                                        reorder_parts(rig.clone(), section_moved(&sections, index, to));
                                    }
                                }
                            }
                        });
                    }
                },
                svg { width: "14", height: "12", view_box: "0 0 14 12",
                    path { d: "M1.5 2h11M1.5 6h11M1.5 10h11", stroke: "{grip}", stroke_opacity: if state == "done" { "0.45" } else { "1" }, stroke_width: "1.7", stroke_linecap: "round" }
                }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; min-height: 50px; padding: 4px 4px 4px 6px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        // Building: a tap picks the section to build into (one
                        // with several parts plays — its parts are picked).
                        if let Some(b) = build_pick
                            && *b.building.peek()
                            && !several
                        {
                            let mut p = b.part;
                            p.set(Some(first));
                            return;
                        }
                        call!(rig, |r| r.select_part(first as u32))
                    }
                },
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                    span { style: "min-width: 0; font-size: 15px; font-weight: {pick(is_now, 700, 520)}; color: {ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{section.name}"
                    }
                    Changes { modules: modules.clone(), unsaved, max: 4 }
                }
                // What it plays.
                span { style: "flex: 0 1 auto; max-width: 64%; min-width: 0; display: flex; padding-right: 10px;",
                    if several {
                        span { style: "font-size: 13px; padding: 3px 8px; border-radius: 4px; background: {FILL}; color: {INK_2}; font-weight: 600;", "{section.parts.len()} parts" }
                    } else if !part.patch.is_empty() {
                        PatchChip { patch: part.patch.clone(), stack: stack.clone(), small: false, lit: is_now, borrowed: borrowed.clone() }
                    } else {
                        span { style: "font-size: 13px; color: {INK_3}; padding: 0 4px;", "keeps" }
                    }
                }
            }
            MoreButton {
                items,
                label: format!("{} actions", section.name),
                on_pick: {
                    let rig = rig.clone();
                    let names = names.clone();
                    let sections = sections.clone();
                    let sec_name = section.name.clone();
                    move |p: Picked| {
                        let names = names.clone();
                        match p.id.as_str() {
                            "go" | "parts" => call!(rig, |r| r.select_part(first as u32)),
                            "patch" => {
                                // Pick the part and open the browser on it (the
                                // patch picker when there is no browser here).
                                if let (Some(b), Some(pp)) = (build_pick, pick_part) {
                                    let mut part = b.part;
                                    part.set(Some(first));
                                    pp.open.call(());
                                } else {
                                    let mut panel = panel;
                                    panel.set(Some(Panel::Patch(first)));
                                }
                            }
                            "rename" => {
                                let new_name = p.text.clone();
                                if grouped {
                                    for n in names {
                                        let new_name = new_name.clone();
                                        call!(rig, |r| r.set_part_section(n, new_name));
                                    }
                                } else if let Some(n) = names.first().cloned() {
                                    call!(rig, |r| r.rename_part(n, new_name));
                                }
                            }
                            "part" => {
                                // A new part, in this section, after its last.
                                let (new_part, sec_name, first_name) = (p.text.clone(), sec_name.clone(), names.first().cloned().unwrap_or_default());
                                if let Some(r) = rig.clone() {
                                    let _ = dioxus_core::spawn_forever(async move {
                                        if !grouped {
                                            let _ = r.set_part_section(first_name, sec_name.clone()).await;
                                        }
                                        let _ = r.add_part(new_part.clone()).await;
                                        let _ = r.set_part_section(new_part, sec_name).await;
                                        let _ = r.move_part(parts_now as u32, last_part as u32 + 1).await;
                                    });
                                }
                            }
                            "earlier" => reorder_parts(rig.clone(), section_moved(&sections, index, index - 1)),
                            "later" => reorder_parts(rig.clone(), section_moved(&sections, index, index + 1)),
                            "delete" => {
                                for n in names {
                                    call!(rig, |r| r.remove_part(n));
                                }
                            }
                            _ => {}
                        }
                    }
                },
            }
        }
    }
}

/// The parts of the section playing: where in it you are, what each plays,
/// and a way to add another — the section's own little timeline.
#[component]
fn Parts(perf: PerformanceModel, lib: LibraryModel, section: Section, panel: Signal<Option<Panel>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let names: Vec<String> = perf.parts.iter().map(|p| p.name.clone()).collect();
    let last = section.parts.last().copied().unwrap_or(0);
    let parts_now = perf.parts.len();
    let sec_name = section.name.clone();
    let count = section.parts.len();
    rsx! {
        div { style: "position: relative; padding-bottom: 4px;",
            span { style: "position: absolute; left: 49px; top: 0; bottom: 26px; width: 1px; background: color-mix(in oklab, {LIVE} 40%, {RULE_STRONG});" }
            for (k, i) in section.parts.iter().copied().enumerate() {
                PartRow { key: "p{i}", perf: perf.clone(), lib: lib.clone(), index: i, first: k == 0, last: k + 1 == count, only: count == 1, panel }
            }
            button {
                style: "display: flex; align-items: center; gap: 8px; width: 100%; min-height: 36px; padding: 0 14px 0 44px; border: none; background: transparent; color: {INK_3}; font-size: 13px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onclick: move |e: MouseEvent| {
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (rig, sec_name) = (rig.clone(), sec_name.clone());
                    open_naming(host, c.x - el.x + 20.0, c.y - el.y + 36.0, Item::name("add", "New part…", format!("{} · {}", section.name, count + 1), "Add", names.clone()), EventHandler::new(move |p: Picked| {
                        let (new_part, sec_name) = (p.text.clone(), sec_name.clone());
                        if let Some(r) = rig.clone() {
                            let _ = dioxus_core::spawn_forever(async move {
                                let _ = r.add_part(new_part.clone()).await;
                                let _ = r.set_part_section(new_part, sec_name).await;
                                let _ = r.move_part(parts_now as u32, last as u32 + 1).await;
                            });
                        }
                    }));
                },
                Plus { size: 12 }
                "Add a part"
            }
        }
    }
}

#[component]
fn PartRow(perf: PerformanceModel, lib: LibraryModel, index: usize, first: bool, last: bool, only: bool, panel: Signal<Option<Panel>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let build_pick = try_use_context::<super::BuildPick>();
    let pick_part = try_use_context::<PickPart>();
    let picked = build_pick.is_some_and(|b| (b.building)() && (b.part)() == Some(index));
    let Some(part) = perf.parts.get(index).cloned() else { return rsx! {} };
    let now = perf.part_index as usize;
    let state = if index == now { "now" } else if index < now { "done" } else { "ahead" };
    let (is_now, is_done) = (state == "now", state == "done");
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let profile = if part.profile.is_empty() { perf.profile_name.clone() } else { part.profile.clone() };
    let stack = stack_of(&lib, &profile, &part.patch);
    let borrowed = borrowed_from(&lib, &perf.profile_name, &part.patch);
    let dot = match state {
        "now" => format!("background: {LIVE}; border: 1.5px solid {LIVE};"),
        "done" => format!("background: {SHEET}; border: 1.5px solid {INK_3};"),
        _ => format!("background: {SHEET}; border: 1.5px solid {DIM};"),
    };
    let others: Vec<String> = perf.parts.iter().map(|p| p.name.clone()).filter(|n| *n != part.name).collect();
    let items = vec![
        Item::head(part.name.clone()),
        Item::run("go", "Play from here").unless(is_now.then(|| "It's playing".to_string())),
        Item::run("patch", "Patch…").detail(if part.patch.is_empty() { "keeps".to_string() } else { part.patch.clone() }),
        Item::name("rename", "Rename…", part.name.clone(), "Rename", others),
        Item::Sep,
        Item::run("earlier", "Move earlier").unless(first.then(|| "Already first".to_string())),
        Item::run("later", "Move later").unless(last.then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("delete", "Delete part").unless(only.then(|| "A section keeps at least one part".to_string())),
    ];
    let name = part.name.clone();
    rsx! {
        div { style: "position: relative; display: flex; align-items: center; min-height: 44px; background: {pick(is_now, tint(&colour, 18), CLEAR.to_string())};",
            if picked {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; right: 0; border: 2px solid {FOCUS_FG}; border-radius: {R}; box-sizing: border-box; pointer-events: none;" }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 2px 4px 2px 46px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        // Building: a tap picks the part to build into; else it plays it.
                        if let Some(b) = build_pick
                            && *b.building.peek()
                        {
                            let mut p = b.part;
                            p.set(Some(index));
                            return;
                        }
                        call!(rig, |r| r.select_part(index as u32))
                    }
                },
                span { style: "position: relative; width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {dot}" }
                span { style: "flex: 1; min-width: 0; font-size: 14px; font-weight: {pick(is_now, 650, 500)}; color: {pick(is_done, INK_3, pick(is_now, INK, INK_2))}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{part.name}"
                }
                span { style: "flex: 0 1 auto; max-width: 64%; min-width: 0; display: flex; padding-right: 10px;",
                    if part.patch.is_empty() {
                        span { style: "font-size: 12px; color: {INK_3}; padding: 0 4px;", "keeps" }
                    } else {
                        PatchChip { patch: part.patch.clone(), stack, small: true, lit: is_now, borrowed }
                    }
                }
            }
            MoreButton {
                items,
                label: format!("{} actions", part.name),
                on_pick: {
                    let rig = rig.clone();
                    move |p: Picked| {
                        let name = name.clone();
                        match p.id.as_str() {
                            "go" => call!(rig, |r| r.select_part(index as u32)),
                            "patch" => {
                                if let (Some(b), Some(pp)) = (build_pick, pick_part) {
                                    let mut part = b.part;
                                    part.set(Some(index));
                                    pp.open.call(());
                                } else {
                                    let mut panel = panel;
                                    panel.set(Some(Panel::Patch(index)));
                                }
                            }
                            "rename" => {
                                let new_name = p.text.clone();
                                call!(rig, |r| r.rename_part(name, new_name));
                            }
                            "earlier" => call!(rig, |r| r.move_part(index as u32, index as u32 - 1)),
                            "later" => call!(rig, |r| r.move_part(index as u32, index as u32 + 1)),
                            "delete" => call!(rig, |r| r.remove_part(name)),
                            _ => {}
                        }
                    }
                },
            }
        }
    }
}

// ── The stacks ─────────────────────────────────────────────────────────────

/// The profile's stacks under the section playing (or the Profile view):
/// a tap plays a stack (a tap on the one playing steps it); each shows the
/// patch a tap plays, its rotation, and what the next tap plays.
#[component]
pub fn Stacks(perf: PerformanceModel, lib: LibraryModel, section: String) -> Element {
    let part = perf.parts.get(perf.part_index as usize).cloned();
    let where_ = part.as_ref().map(|p| if p.section.is_empty() || p.name == section { section.clone() } else { p.name.clone() }).unwrap_or_default();
    let song = perf.songs.get(perf.song_index as usize).cloned();
    let colour = song.as_ref().map(|s| song_colour(&s.name, &s.colour)).unwrap_or_else(|| name_colour(&perf.profile_name).to_string());
    let count = perf.stacks.len();
    rsx! {
        div { style: "padding: 2px 0 8px 46px;",
            div { style: "background: rgba(0,0,0,0.18); display: flex; flex-direction: column; border-bottom: 1px solid {RULE};",
                for (i, st) in perf.stacks.iter().enumerate() {
                    StackRow { key: "{st.name}", stack: st.clone(), index: i, count, lib: lib.clone(), profile: perf.profile_name.clone(), part: part.clone().unwrap_or_default(), where_: where_.clone(), song_colour: colour.clone(), in_song: !perf.parts.is_empty() }
                }
            }
        }
    }
}

/// A patch's source: the profile's own, the song's own, or borrowed.
fn source_of(lib: &LibraryModel, profile: &str, patch: &str) -> (bool, Option<String>) {
    let mine = lib.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(profile) && p.patch_list.iter().any(|x| x.name.eq_ignore_ascii_case(patch)));
    if mine {
        return (true, None);
    }
    (false, lib.profiles.iter().find(|p| p.patch_list.iter().any(|x| x.name.eq_ignore_ascii_case(patch))).map(|p| p.name.clone()))
}

/// Play `patch` by name: the rig's patch list says its index.
pub fn play_patch(rig: Option<RigClient>, patch: String) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        if let Ok(list) = r.patches().await
            && let Some(i) = list.iter().position(|p| p.name.eq_ignore_ascii_case(&patch))
        {
            let _ = r.select_patch(i as u32).await;
        }
    });
}

#[component]
pub fn StackRow(stack: PerfStack, index: usize, count: usize, lib: LibraryModel, profile: String, part: PerfPart, where_: String, song_colour: String, in_song: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let on = stack.is_active;
    let (tape, _) = crate::perform::folder_color(&stack.name);
    let n = stack.patches.len().max(stack.patch_count as usize);
    let pos = stack.position as usize;
    // The patch a tap plays, by its own name.
    let showing = stack.patches.get(pos).cloned().unwrap_or_else(|| stack.current_patch.clone());
    let next = if stack.patches.len() > 1 { stack.patches.get((pos + 1) % stack.patches.len()).cloned() } else { None };
    // The part's own patch: home, while something else plays by hand.
    let part_patch = part.patch.clone();
    let home = !on && !part_patch.is_empty() && stack.patches.iter().any(|p| p.eq_ignore_ascii_case(&part_patch));
    let by_hand = on && !part_patch.is_empty() && !showing.eq_ignore_ascii_case(&part_patch);
    let is_default = !part_patch.is_empty() && part_patch.eq_ignore_ascii_case(&showing);
    let (mine, borrowed) = source_of(&lib, &profile, &showing);
    let stack_names: Vec<String> = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(&profile)).map(|p| p.stacks.clone()).unwrap_or_default();

    let mut items = vec![Item::head(format!("{} · {}", stack.name, showing))];
    for (k, p) in stack.patches.iter().enumerate() {
        let (own, from) = source_of(&lib, &profile, p);
        let detail = if own { String::new() } else { from.map_or_else(|| "the song's own".to_string(), |f| format!("from {f}")) };
        items.push(Item::run(format!("p{k}"), p.clone()).detail(detail).checked(k == pos && on));
    }
    if by_hand && !where_.is_empty() {
        items.push(Item::Sep);
        items.push(Item::run("keep", format!("Keep for {where_}")));
        items.push(Item::run("back", format!("Back to {where_}'s patch")));
    }
    if in_song && !part.name.is_empty() && !where_.is_empty() {
        items.push(Item::Sep);
        items.push(Item::run("default", format!("Make “{showing}” default for {where_}")).unless(is_default.then(|| format!("It's {where_}'s already"))));
    }
    items.push(Item::Sep);
    items.push(Item::name("add", "Add a patch…", format!("{} {}", stack.name, stack.patches.len() + 1), "Add", stack.patches.clone()));
    if mine {
        items.push(Item::name("rename_patch", format!("Rename “{showing}”…"), showing.clone(), "Rename", stack.patches.clone()));
    }
    items.push(Item::name("rename", "Rename stack…", stack.name.clone(), "Rename", stack_names));
    items.push(Item::run("up", "Move up").unless((index == 0).then(|| "Already first".to_string())));
    items.push(Item::run("down", "Move down").unless((index + 1 == count).then(|| "Already last".to_string())));
    if mine {
        items.push(Item::Sep);
        items.push(Item::delete("remove_patch", format!("Remove “{showing}”")).unless((stack.patches.len() <= 1).then(|| "A stack keeps at least one patch".to_string())));
    }

    let bar = if on {
        let c = if by_hand { MODIFIED } else { LIVE };
        format!("background: {c};")
    } else if home {
        format!("border: 1.25px solid {LIVE}; box-sizing: border-box;")
    } else {
        String::new()
    };
    let part_name = part.name.clone();
    let stack_name = stack.name.clone();
    let patches = stack.patches.clone();
    let part_index_now = part.name.clone();
    rsx! {
        div { style: "display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {pick(on, tint(&song_colour, 18), CLEAR.to_string())};",
            button {
                style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 4px 0 10px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    let part_patch = part_patch.clone();
                    move |_| {
                        // Home: back to the part's own patch. Else: play the stack.
                        if home {
                            play_patch(rig.clone(), part_patch.clone());
                        } else {
                            call!(rig, |r| r.press_stack(index as u32));
                        }
                    }
                },
                if on || home {
                    span { style: "position: absolute; left: 0; top: 6px; bottom: 6px; width: 3px; border-radius: 2px; {bar}" }
                }
                span { style: "width: 70px; flex-shrink: 0; display: flex; align-items: center; gap: 6px;",
                    span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {tape};" }
                    span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; white-space: nowrap; overflow: hidden; color: {pick(on, INK, INK_3)};", "{stack.name}" }
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "font-size: 14px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{showing}"
                    }
                    if n > 1 {
                        span { style: "display: flex; align-items: center; gap: 4px; min-width: 0;",
                            for k in 0..n {
                                RotationDot { key: "{k}", lit: k == pos, source: stack.patches.get(k).map(|p| source_of(&lib, &profile, p)).unwrap_or((true, None)), song_colour: song_colour.clone() }
                            }
                            if on && let Some(nx) = next.clone() {
                                span { style: "margin-left: 6px; min-width: 0; font-size: 12px; font-weight: 600; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "next › "
                                    span { style: "color: {INK_2};", "{nx}" }
                                }
                            }
                        }
                    }
                }
                if is_default {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 650; color: {INK_3}; padding-right: 4px;", "Default" }
                }
                if let Some(b) = borrowed.clone() {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 650; color: {name_colour(&b)}; padding-right: 4px;", "{b}" }
                }
            }
            MoreButton {
                items,
                label: format!("{} actions", stack.name),
                on_pick: {
                    let rig = rig.clone();
                    let showing = showing.clone();
                    move |p: Picked| {
                        let (part_name, stack_name, showing) = (part_name.clone(), stack_name.clone(), showing.clone());
                        match p.id.as_str() {
                            "keep" => call!(rig, |r| r.set_part_patch(part_name, showing)),
                            "back" => {
                                let _ = &part_index_now;
                                play_patch(rig.clone(), part.patch.clone());
                            }
                            "default" => call!(rig, |r| r.set_part_patch(part_name, showing)),
                            "add" => {
                                let name = p.text.clone();
                                call!(rig, |r| r.add_patch(name, stack_name, String::new()));
                            }
                            "rename_patch" => {
                                let new_name = p.text.clone();
                                call!(rig, |r| r.rename_patch(showing, new_name));
                            }
                            "rename" => {
                                let new_name = p.text.clone();
                                call!(rig, |r| r.rename_stack(stack_name, new_name));
                            }
                            "up" => call!(rig, |r| r.move_stack(index as u32, index as u32 - 1)),
                            "down" => call!(rig, |r| r.move_stack(index as u32, index as u32 + 1)),
                            "remove_patch" => call!(rig, |r| r.delete_patch(showing)),
                            other => {
                                if let Some(k) = other.strip_prefix('p').and_then(|k| k.parse::<usize>().ok())
                                    && let Some(name) = patches.get(k).cloned()
                                {
                                    play_patch(rig.clone(), name);
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// A patch in a stack's rotation: the song's own filled in the song's
/// colour, one borrowed in its profile's, the profile's own open.
#[component]
fn RotationDot(lit: bool, source: (bool, Option<String>), song_colour: String) -> Element {
    let style = match &source {
        (false, None) => format!("background: {song_colour}; opacity: {};", pick(lit, 1.0, 0.55)),
        (false, Some(p)) => format!("background: {};", name_colour(p)),
        (true, _) if lit => format!("background: {INK_2};"),
        (true, _) => format!("border: 1.25px solid {INK_3}; box-sizing: border-box;"),
    };
    rsx! {
        span { style: "width: {pick(lit, 12, 5)}px; height: 5px; border-radius: 999px; flex-shrink: 0; {style}" }
    }
}
