//! The tablet's setlist sidebar — the touch prototype's `setlist/Setlist.tsx`
//! on the live rig.
//!
//!   Header   the set's name (a tap picks another set), event · date ·
//!            profile in words, and the set as a bar of song colours with
//!            its place in it.
//!   Songs    number on the song's colour, name, Now / Next, key, tempo, ⋯.
//!            The song up opens into its sections.
//!   Sections a part's `section` groups consecutive parts; each shows the
//!            patch it plays with the stack it lands on, its overrides in
//!            words, and a ⋯. The section playing opens into its parts
//!            (when it has several) and the profile's stacks.
//!   Stacks   name in its colour, the patch a tap plays, its rotation as
//!            dots, what the next tap plays; ⋯ makes the patch it shows the
//!            default for the section playing.
//!
//! Everything reads the rig's `PerformanceModel` and `LibraryModel`; every
//! action is a rig call.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LibraryModel, PerfPart, PerfStack, PerformanceModel};
use super::menu::{open_menu, Item, MoreButton, Picked};
use signal_widgets::PopupHost;

use super::colors::{date_label, name_colour, section_colour, set_heading, set_meta, song_colour};
use super::tokens::*;
use crate::state::RigViewState;

/// A rig call from an event handler: clone the client, run it, ignore the
/// reply (the state stream brings the result).
macro_rules! call {
    ($rig:expr, |$r:ident| $body:expr) => {{
        if let Some($r) = $rig.clone() {
            spawn(async move {
                let _ = $body.await;
            });
        }
    }};
}

/// The library, refetched whenever the rig's state moves on.
fn use_library(state: RigViewState) -> Signal<LibraryModel> {
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

/// A static id for a menu item at index `i` under `prefix` (menu ids are
/// `&'static str`; a set's songs and profiles are few).
fn id(prefix: &str, i: usize) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static IDS: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let key = format!("{prefix}:{i}");
    let mut map = IDS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(key.clone()).or_insert_with(|| Box::leak(key.into_boxed_str()))
}

fn index_of(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok()
}

/// The stack a patch of the playing profile lives in.
fn stack_of(lib: &LibraryModel, profile: &str, patch: &str) -> String {
    lib.profiles
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(profile) || p.active)
        .flat_map(|p| p.patch_list.iter())
        .find(|p| p.name.eq_ignore_ascii_case(patch))
        .map(|p| p.stack.clone())
        .unwrap_or_default()
}

/// A section: consecutive parts that share a `section` name (a part with
/// none is its own).
#[derive(Clone, PartialEq)]
struct Section {
    name: String,
    /// Indices into `PerformanceModel::parts`.
    parts: Vec<usize>,
}

fn sections_of(parts: &[PerfPart]) -> Vec<Section> {
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

// ── The sidebar ────────────────────────────────────────────────────────────

#[component]
pub fn TabletSetlist(state: RigViewState) -> Element {
    let lib = use_library(state);
    let perf = state.perf.read().clone();
    let lib_now = lib.read().clone();
    let songs = perf.songs.clone();
    rsx! {
        section { style: "height: 100%; display: flex; flex-direction: column; min-height: 0; background: {SHEET}; font-family: {FONT}; color: {INK};",
            SetHeader { perf: perf.clone(), lib: lib_now.clone() }
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                for (i, song) in songs.iter().enumerate() {
                    SongRow { key: "{i}-{song.name}", perf: perf.clone(), lib: lib_now.clone(), index: i }
                    if i == perf.song_index as usize {
                        Sections { perf: perf.clone(), lib: lib_now.clone() }
                    }
                }
                if songs.is_empty() {
                    div { style: "padding: 32px 16px; text-align: center; font-size: 14px; color: {INK_3};", "No songs in this set" }
                }
            }
        }
    }
}

// ── Header ─────────────────────────────────────────────────────────────────

#[component]
fn SetHeader(perf: PerformanceModel, lib: LibraryModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let set_index = perf.setlist_index as usize;
    let entry = lib.setlists.get(set_index).cloned().unwrap_or_default();
    let name = perf.setlists.get(set_index).cloned().unwrap_or_else(|| entry.name.clone());
    let meta = set_meta(&name, &entry.event, &entry.date, &entry.title);
    let heading = set_heading(&meta);
    let date = date_label(&meta.date);
    let profile = if entry.profile.is_empty() { perf.profile_name.clone() } else { entry.profile.clone() };
    let count = perf.songs.len();
    let at = perf.song_index as usize;

    // The set picker: every set, the one playing ticked.
    let mut sets_items = vec![Item::head("Sets")];
    for (i, s) in perf.setlists.iter().enumerate() {
        let item = Item::run(id("set", i), s.clone()).checked(i == set_index);
        sets_items.push(item);
    }
    // The set's actions: its default profile.
    let mut actions = vec![Item::head(heading.clone()), Item::head("Default profile")];
    for (i, p) in lib.profiles.iter().enumerate() {
        let item = Item::run(id("profile", i), p.name.clone()).checked(p.name.eq_ignore_ascii_case(&entry.profile));
        actions.push(item);
    }
    let profiles: Vec<String> = lib.profiles.iter().map(|p| p.name.clone()).collect();

    rsx! {
        header { style: "flex-shrink: 0; height: {HEADER_H}px; padding: 0 6px 0 18px; border-bottom: 1px solid {RULE}; display: flex; flex-direction: column; justify-content: center; gap: 7px; box-sizing: border-box;",
            div { style: "display: flex; align-items: center; gap: 2px;",
                // The set's name and a chevron: a tap picks another set.
                button {
                    style: "flex: 1; min-width: 0; min-height: 44px; display: flex; flex-direction: column; justify-content: center; align-items: flex-start; gap: 3px; text-align: left; padding: 0 8px; margin: 0 0 0 -8px; border: none; border-radius: {R_MD}; background: transparent; color: {INK}; font-family: {FONT}; cursor: pointer;",
                    onclick: {
                        let rig = rig.clone();
                        move |e: MouseEvent| {
                            let (c, el) = (e.client_coordinates(), e.element_coordinates());
                            let rig = rig.clone();
                            open_menu(host, c.x - el.x, c.y - el.y + 48.0, sets_items.clone(), EventHandler::new(move |p: Picked| {
                                if let Some(i) = index_of(&p.id, "set") {
                                    call!(rig, |r| r.select_setlist(i as u32));
                                }
                            }));
                        }
                    },
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
                        if !date.is_empty() {
                            span { "{date}" }
                            span { "·" }
                        }
                        span { style: "overflow: hidden; text-overflow: ellipsis;", "{profile}" }
                    }
                }
                MoreButton {
                    items: actions,
                    label: "Set actions".to_string(),
                    on_pick: {
                        let rig = rig.clone();
                        move |p: Picked| {
                            if let Some(i) = index_of(&p.id, "profile")
                                && let Some(name) = profiles.get(i).cloned()
                            {
                                call!(rig, |r| r.set_setlist_profile(set_index as u32, name));
                            }
                        }
                    },
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

#[component]
fn SongRow(perf: PerformanceModel, lib: LibraryModel, index: usize) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let Some(song) = perf.songs.get(index).cloned() else { return rsx! {} };
    let at = perf.song_index as usize;
    let up = index == at;
    let played = index < at;
    let next = index == at + 1;
    let last = perf.songs.len().saturating_sub(1);
    let colour = song_colour(&song.name, &song.colour);
    let lib_song = lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(&song.name)).cloned();
    let sections = lib_song.as_ref().map_or(0, |s| s.parts.len());
    let own_profile = lib_song.as_ref().map(|s| s.profile.clone()).unwrap_or_default();
    let start_stack = stack_of(&lib, &perf.profile_name, &song.start);
    let set_index = perf.setlist_index;

    let mut items = vec![
        Item::head(format!("{} · {}", index + 1, song.name)),
        Item::run("go", "Play from here").unless(up.then(|| "It's up now".to_string())),
        Item::Sep,
        Item::head("Profile"),
    ];
    let own = Item::run("profile:none", "The set's").checked(own_profile.is_empty());
    items.push(own);
    for (i, p) in lib.profiles.iter().enumerate() {
        let item = Item::run(id("profile", i), p.name.clone()).checked(p.name.eq_ignore_ascii_case(&own_profile));
        items.push(item);
    }
    items.push(Item::head("Colour"));
    let by_name = Item::run("colour:none", "From its name").checked(song.colour.is_empty());
    items.push(by_name);
    for (i, c) in super::colors::SONG_PALETTE.iter().enumerate() {
        let item = Item::run(id("colour", i), COLOUR_NAMES[i]).checked(song.colour.eq_ignore_ascii_case(c));
        items.push(item);
    }
    items.extend([
        Item::Sep,
        Item::run("up", "Move up").unless((index == 0).then(|| "Already first".to_string())),
        Item::run("down", "Move down").unless((index == last).then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("remove", "Remove from this set"),
    ]);
    let profiles: Vec<String> = lib.profiles.iter().map(|p| p.name.clone()).collect();
    let name = song.name.clone();

    rsx! {
        div {
            style: "position: relative; display: flex; align-items: stretch; min-height: 64px; border-top: 1px solid {RULE}; background: {pick(up, tint(&colour, 7), CLEAR.to_string())};",
            if up {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 10px 2px 10px 14px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| if !up { call!(rig, |r| r.select_song(index as u32)); }
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
                        if !song.start.is_empty() {
                            PatchChip { patch: song.start.clone(), stack: start_stack.clone(), small: true, lit: false }
                        }
                        if !own_profile.is_empty() {
                            span { style: "flex-shrink: 0; color: {INK_2}; font-weight: 650;", "{own_profile}" }
                        }
                        if !up && sections > 0 {
                            span { style: "flex-shrink: 0;", "{sections} sections" }
                        }
                    }
                }
                if !song.key.is_empty() {
                    span { style: "flex-shrink: 0; min-width: 28px; height: 28px; padding: 0 6px; box-sizing: border-box; border-radius: 6px; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 700; color: {INK_2}; box-shadow: inset 0 0 0 1px {RULE_STRONG};",
                        "{song.key}"
                    }
                }
                span { style: "width: 34px; text-align: right; font-size: 14px; font-weight: 600; color: {INK_2}; flex-shrink: 0; font-variant-numeric: tabular-nums;",
                    if song.bpm > 0 { "{song.bpm}" }
                }
            }
            div { style: "display: flex; align-items: center; padding-right: 2px;",
                MoreButton {
                    items,
                    label: format!("{} actions", song.name),
                    on_pick: {
                        let rig = rig.clone();
                        move |p: Picked| {
                            let name = name.clone();
                            match p.id.as_str() {
                                "go" => call!(rig, |r| r.select_song(index as u32)),
                                "up" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 - 1)),
                                "down" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 + 1)),
                                "remove" => call!(rig, |r| r.remove_setlist_entry(set_index, index as u32)),
                                "profile:none" => call!(rig, |r| r.set_song_profile(name, String::new())),
                                "colour:none" => call!(rig, |r| r.set_song_colour(name, String::new())),
                                other => {
                                    if let Some(i) = index_of(other, "profile")
                                        && let Some(pr) = profiles.get(i).cloned()
                                    {
                                        call!(rig, |r| r.set_song_profile(name, pr));
                                    } else if let Some(i) = index_of(other, "colour")
                                        && let Some(c) = super::colors::SONG_PALETTE.get(i)
                                    {
                                        let c = (*c).to_string();
                                        call!(rig, |r| r.set_song_colour(name, c));
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

const COLOUR_NAMES: [&str; 14] = [
    "Sky", "Blue", "Indigo", "Violet", "Purple", "Fuchsia", "Pink", "Rose", "Orange", "Amber", "Yellow", "Lime", "Teal", "Cyan",
];

#[component]
fn Badge(live: bool, children: Element) -> Element {
    let (bg, fg) = if live { (LIVE_BG, LIVE) } else { ("rgba(255,255,255,0.07)", INK_2) };
    rsx! {
        span { style: "flex-shrink: 0; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; background: {bg}; color: {fg};",
            {children}
        }
    }
}

/// A patch with the stack it lands on: the stack named in its colour.
#[component]
fn PatchChip(patch: String, stack: String, small: bool, lit: bool) -> Element {
    let (tape, _) = crate::perform::folder_color(&stack);
    let gaffer = stack.is_empty() || tape == "#3f3f46";
    let stack_ink = if gaffer { INK_3.to_string() } else { lift(tape) };
    let (pad, size, stack_size) = if small { ("2px 8px 2px 7px", 12, 10.5) } else { ("5px 9px 5px 8px", 13, 11.0) };
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 7px; min-width: 0; max-width: 100%; padding: {pad}; border-radius: 4px; background: {pick(lit, FILL_ON, FILL)}; color: {pick(lit, INK, INK_2)}; font-size: {size}px; font-weight: 600; white-space: nowrap;",
            if !stack.is_empty() {
                span { style: "flex-shrink: 0; font-size: {stack_size}px; font-weight: 750; letter-spacing: 0.07em; text-transform: uppercase; color: {stack_ink};", "{stack}" }
            }
            span { style: "overflow: hidden; text-overflow: ellipsis;", "{patch}" }
        }
    }
}

/// The override icon: a square laid over another.
#[component]
fn OverrideIcon(colour: String, size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 12 12", style: "flex-shrink: 0; display: block;",
            rect { x: "1", y: "1", width: "7", height: "7", rx: "1.6", fill: "none", stroke: "{colour}", stroke_width: "1.3", opacity: "0.45" }
            rect { x: "4", y: "4", width: "7", height: "7", rx: "1.6", fill: "{colour}" }
        }
    }
}

// ── Its sections ───────────────────────────────────────────────────────────

#[component]
fn Sections(perf: PerformanceModel, lib: LibraryModel) -> Element {
    let sections = sections_of(&perf.parts);
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let now = perf.part_index as usize;
    rsx! {
        div { style: "position: relative; padding: 2px 0 10px; background: {tint(&colour, 7)};",
            span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            for (j, sec) in sections.iter().enumerate() {
                SectionRow { key: "{j}-{sec.name}", perf: perf.clone(), lib: lib.clone(), section: sec.clone(), index: j, count: sections.len() }
                if sec.parts.contains(&now) {
                    if sec.parts.len() > 1 {
                        for k in sec.parts.iter().copied() {
                            PartRow { key: "p{k}", perf: perf.clone(), lib: lib.clone(), index: k }
                        }
                    }
                    Stacks { perf: perf.clone(), section: sec.name.clone() }
                }
            }
            if sections.is_empty() {
                Stacks { perf: perf.clone(), section: String::new() }
            }
        }
    }
}

/// The blocks a section's parts change, in order, once each.
fn overridden_blocks(perf: &PerformanceModel, sec: &Section) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &k in &sec.parts {
        for o in perf.parts.get(k).map(|p| p.overrides.as_slice()).unwrap_or_default() {
            if !out.iter().any(|b| b.eq_ignore_ascii_case(&o.block)) {
                out.push(o.block.clone());
            }
        }
    }
    out
}

#[component]
fn SectionRow(perf: PerformanceModel, lib: LibraryModel, section: Section, index: usize, count: usize) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let now = perf.part_index as usize;
    let first = section.parts.first().copied().unwrap_or(0);
    let state = if section.parts.contains(&now) { "now" } else if first < now { "done" } else { "ahead" };
    let is_now = state == "now";
    let several = section.parts.len() > 1;
    let part = perf.parts.get(first).cloned().unwrap_or_default();
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let stack = stack_of(&lib, &perf.profile_name, &part.patch);
    let overrides = overridden_blocks(&perf, &section);
    let shown: Vec<String> = overrides.iter().take(3).cloned().collect();
    let more = overrides.len().saturating_sub(shown.len());
    let overrides_line = shown.join(" · ");
    let names: Vec<String> = section.parts.iter().filter_map(|&k| perf.parts.get(k).map(|p| p.name.clone())).collect();
    let grouped = !part.section.is_empty();

    let items = vec![
        Item::head(section.name.clone()),
        Item::run("go", "Play from here").unless((state == "now").then(|| "It's playing".to_string())),
        Item::name("rename", "Rename…", section.name.clone(), "Rename", Vec::new()),
        Item::name("part", "Add a part…", format!("{} · {}", section.name, section.parts.len() + 1), "Add", names.clone()),
        Item::Sep,
        Item::run("earlier", "Move earlier").unless((index == 0 || several).then(|| if several { "Move its parts".to_string() } else { "Already first".to_string() })),
        Item::run("later", "Move later").unless((index + 1 == count || several).then(|| if several { "Move its parts".to_string() } else { "Already last".to_string() })),
        Item::Sep,
        Item::delete("delete", "Delete section"),
    ];
    let ink = match state {
        "done" => INK_3,
        "now" => INK,
        _ => INK_2,
    };
    let grip = section_colour(&section.name);
    let row_bg = pick(state == "now", tint(&colour, pick(several, 10, 18)), "transparent".to_string());

    rsx! {
        div {
            style: "position: relative; display: flex; align-items: center; min-height: 50px; background: {row_bg};",
            // The section's colour, as its grip.
            span { style: "width: 40px; align-self: stretch; flex-shrink: 0; display: flex; align-items: center; justify-content: flex-end; padding-right: 6px; box-sizing: border-box;",
                svg { width: "14", height: "12", view_box: "0 0 14 12",
                    path { d: "M1.5 2h11M1.5 6h11M1.5 10h11", stroke: "{grip}", stroke_opacity: if state == "done" { "0.45" } else { "1" }, stroke_width: "1.7", stroke_linecap: "round" }
                }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; min-height: 50px; padding: 4px 4px 4px 6px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| call!(rig, |r| r.select_part(first as u32))
                },
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                    span { style: "min-width: 0; font-size: 15px; font-weight: {pick(is_now, 700, 520)}; color: {ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{section.name}"
                    }
                    if !shown.is_empty() {
                        // Its overrides, in words.
                        span { style: "display: inline-flex; align-items: center; gap: 6px; min-width: 0; font-size: 12px; font-weight: 650; color: {INK_2}; white-space: nowrap; overflow: hidden;",
                            OverrideIcon { colour: INK_2.to_string(), size: 11 }
                            "{overrides_line}"
                            if more > 0 { span { style: "color: {INK_3};", "+{more}" } }
                        }
                    }
                }
                // What it plays — set from a stack's ⋯ (Make default).
                span { style: "flex: 0 1 auto; max-width: 64%; min-width: 0; display: flex; padding-right: 10px;",
                    if several {
                        span { style: "font-size: 13px; padding: 3px 8px; border-radius: 4px; background: {FILL}; color: {INK_2}; font-weight: 600;", "{section.parts.len()} parts" }
                    } else if !part.patch.is_empty() {
                        PatchChip { patch: part.patch.clone(), stack: stack.clone(), small: false, lit: state == "now" }
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
                    move |p: Picked| {
                        let names = names.clone();
                        match p.id.as_str() {
                            "go" => call!(rig, |r| r.select_part(first as u32)),
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
                                let new_part = p.text.clone();
                                call!(rig, |r| r.add_part(new_part));
                            }
                            "earlier" => call!(rig, |r| r.move_part(first as u32, first as u32 - 1)),
                            "later" => call!(rig, |r| r.move_part(first as u32, first as u32 + 1)),
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

#[component]
fn PartRow(perf: PerformanceModel, lib: LibraryModel, index: usize) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let Some(part) = perf.parts.get(index).cloned() else { return rsx! {} };
    let now = perf.part_index as usize;
    let state = if index == now { "now" } else if index < now { "done" } else { "ahead" };
    let (is_now, is_done) = (state == "now", state == "done");
    let colour = perf.songs.get(perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let stack = stack_of(&lib, &perf.profile_name, &part.patch);
    let dot = match state {
        "now" => format!("background: {LIVE}; border: 1.5px solid {LIVE};"),
        "done" => format!("background: {SHEET}; border: 1.5px solid {INK_3};"),
        _ => format!("background: {SHEET}; border: 1.5px solid {DIM};"),
    };
    let items = vec![
        Item::head(part.name.clone()),
        Item::run("go", "Play from here").unless((state == "now").then(|| "It's playing".to_string())),
        Item::name("rename", "Rename…", part.name.clone(), "Rename", Vec::new()),
        Item::Sep,
        Item::delete("delete", "Delete part"),
    ];
    let name = part.name.clone();
    rsx! {
        div { style: "display: flex; align-items: center; min-height: 44px; background: {pick(is_now, tint(&colour, 18), CLEAR.to_string())};",
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 2px 4px 2px 46px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| call!(rig, |r| r.select_part(index as u32))
                },
                span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {dot}" }
                span { style: "flex: 1; min-width: 0; font-size: 14px; font-weight: {pick(is_now, 650, 500)}; color: {pick(is_done, INK_3, pick(is_now, INK, INK_2))}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{part.name}"
                }
                span { style: "flex: 0 1 auto; max-width: 64%; min-width: 0; display: flex; padding-right: 10px;",
                    if part.patch.is_empty() {
                        span { style: "font-size: 12px; color: {INK_3};", "keeps" }
                    } else {
                        PatchChip { patch: part.patch.clone(), stack, small: true, lit: state == "now" }
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
                            "rename" => {
                                let new_name = p.text.clone();
                                call!(rig, |r| r.rename_part(name, new_name));
                            }
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

/// The profile's stacks under the section playing: a tap plays a stack (a
/// tap on the one playing steps it); each shows the patch a tap plays, its
/// rotation, and what the next tap plays.
#[component]
fn Stacks(perf: PerformanceModel, section: String) -> Element {
    let part = perf.parts.get(perf.part_index as usize).cloned();
    let where_ = part.as_ref().map(|p| if p.section.is_empty() || p.name == section { section.clone() } else { p.name.clone() }).unwrap_or_default();
    rsx! {
        div { style: "padding: 2px 0 8px 46px;",
            div { style: "background: rgba(0,0,0,0.18); display: flex; flex-direction: column; border-bottom: 1px solid {RULE};",
                for (i, st) in perf.stacks.iter().enumerate() {
                    StackRow { key: "{st.name}", stack: st.clone(), index: i, part: part.clone().unwrap_or_default(), where_: where_.clone() }
                }
            }
        }
    }
}

/// A rotation dot: the patch a tap plays, and the others.
const DOT_ON: &str = "background: #a1a1aa;";
const DOT_OFF: &str = "box-shadow: inset 0 0 0 1.25px #8e8e98;";

#[component]
fn StackRow(stack: PerfStack, index: usize, part: PerfPart, where_: String) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let on = stack.is_active;
    let (tape, _) = crate::perform::folder_color(&stack.name);
    let count = stack.patches.len().max(stack.patch_count as usize);
    let pos = stack.position as usize;
    let next = if stack.patches.len() > 1 { stack.patches.get((pos + 1) % stack.patches.len()).cloned() } else { None };
    let is_default = !part.patch.is_empty() && part.patch.eq_ignore_ascii_case(&stack.current_patch);
    let mut items = vec![Item::head(format!("{} · {}", stack.name, stack.current_patch))];
    if !part.name.is_empty() && !where_.is_empty() {
        items.push(
            Item::run("default", format!("Make “{}” default for {}", stack.current_patch, where_))
                .unless(is_default.then(|| format!("It's {where_}'s already"))),
        );
    }
    let part_name = part.name.clone();
    let patch = stack.current_patch.clone();
    rsx! {
        div { style: "display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {pick(on, ROW_ON, CLEAR)};",
            button {
                style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 4px 0 10px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| call!(rig, |r| r.press_stack(index as u32))
                },
                if on {
                    span { style: "position: absolute; left: 0; top: 6px; bottom: 6px; width: 3px; border-radius: 2px; background: {LIVE};" }
                }
                span { style: "min-width: 70px; flex-shrink: 0; display: flex; align-items: center; gap: 6px;",
                    span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {tape};" }
                    span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; white-space: nowrap; color: {pick(on, INK, INK_3)};", "{stack.name}" }
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "font-size: 14px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{stack.current_patch}"
                    }
                    if count > 1 {
                        span { style: "display: flex; align-items: center; gap: 4px; min-width: 0;",
                            for k in 0..count {
                                span { key: "{k}", style: "width: {pick(k == pos, 12, 5)}px; height: 5px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; {pick(k == pos, DOT_ON, DOT_OFF)}" }
                            }
                            if on && let Some(n) = next.clone() {
                                span { style: "margin-left: 6px; min-width: 0; font-size: 12px; font-weight: 600; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "next › "
                                    span { style: "color: {INK_2};", "{n}" }
                                }
                            }
                        }
                    }
                }
                if is_default {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 650; color: {INK_3}; padding-right: 4px;", "Default" }
                }
            }
            if items.len() > 1 {
                MoreButton {
                    items,
                    label: format!("{} actions", stack.name),
                    on_pick: {
                        let rig = rig.clone();
                        move |p: Picked| {
                            if p.id == "default" {
                                let (part_name, patch) = (part_name.clone(), patch.clone());
                                call!(rig, |r| r.set_part_patch(part_name, patch));
                            }
                        }
                    },
                }
            }
        }
    }
}
