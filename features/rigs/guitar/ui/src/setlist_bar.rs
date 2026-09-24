//! **The setlist sidebar** — the left sidebar in Setlist mode.
//!
//! One tree, in the shape a set is played: the set, its songs in order, and
//! under the song that is up, its **map** — its sections in order, each a
//! card, with a progress strip on top:
//!
//! ```text
//! SETLIST
//! CYA 9-24-26 ▾
//!  1  AMAZING!          B · 145
//!  2  WASHED            B · 139     ← the song that is up
//!     ▬▬▬▬▬▬▬▬▬▬▬▬▬▬
//!     Section 3 of 6    next: Chorus 2
//!     ✓ Verse 1   Dry Chorus Clean
//!     ✓ Chorus 1  Ambient Clean
//!     ③ Verse 2   Dry Chorus Clean L · ⇄ 1     ← up (green edge)
//!     ④ Chorus 2  Drive · profile switches  NEXT
//!     ⑤ Bridge    Ambient Delay Flute · ⇄ 5
//!     ⑥ Chorus 3  Drive · profile switches
//!  3  TAKEOVER          C · 66
//! ```
//!
//! A section of one part is one row; a section of several lists its parts
//! under its name. A click on a section plays it; a click on the one that is
//! up opens its editor (what it recalls, its section, whether it plays the
//! profile's switches, rename, move, remove). Switch 5 steps through the
//! sections (hold: back), and its tile names the next one.
//!
//! Building sets — new sets, new songs, adding a song to a set — is the
//! library's job; this sidebar hands off to it rather than growing forms.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LibraryModel, PerformanceModel, ProfileEntry};
use signal_widgets::{Picker, PickerSize};

use crate::library::Kind;

use crate::kit::{MenuItem, PickOption, Picked, PresetBar};
use crate::theme::{
    DIM, FAINT, FIELD, LINE, LINE_STRONG, LIVE, LIVE_BG, MUTED, SIDEBAR, SIDEBAR_W, TEXT,
};

/// The set's menu: rename, duplicate, a new set, move it in the list,
/// delete (refused for the only one), and the library.
fn set_items(sets: &[String], index: usize) -> Vec<MenuItem> {
    let name = sets.get(index).cloned().unwrap_or_default();
    let others: Vec<String> = sets.iter().filter(|n| !n.eq_ignore_ascii_case(&name)).cloned().collect();
    vec![
        MenuItem::head(format!("Setlist · {name}")),
        MenuItem::name("rename", "Rename…", "Rename", &name, others),
        MenuItem::name(
            "duplicate",
            "Duplicate…",
            "Duplicate",
            crate::module_sidebar::next_name(&name, sets),
            sets.to_vec(),
        ),
        MenuItem::name("new", "New setlist…", "Create", "", sets.to_vec()),
        MenuItem::sep(),
        MenuItem::run("up", "Move up the list").unless((index == 0).then(|| "First".to_string())),
        MenuItem::run("down", "Move down the list").unless((index + 1 >= sets.len()).then(|| "Last".to_string())),
        MenuItem::delete(
            "delete",
            "Delete setlist",
            (sets.len() <= 1).then(|| "The only setlist — make another first".to_string()),
        ),
        MenuItem::sep(),
        MenuItem::run("library", "All sets and songs in the library"),
    ]
}

/// Fire a rig call without waiting — the next `Perf` event redraws.
fn send<F, Fut>(rig: &Option<RigClient>, call: F)
where
    F: FnOnce(RigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { call(r).await });
    }
}

#[component]
pub fn SetlistSidebar(model: PerformanceModel, on_browse: EventHandler<Kind>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let popup_host = signal_widgets::PopupHost::try_use();

    // The patch list, for telling a part what to recall — re-read when
    // the rig's state moves.
    let mut rev = use_signal(|| model.revision);
    if *rev.peek() != model.revision {
        rev.set(model.revision);
    }
    let patches = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.library().await.unwrap_or_default(),
                    None => LibraryModel::default(),
                }
            }
        }
    });
    let lib: LibraryModel = patches.read().clone().unwrap_or_default();
    let profile_names: Vec<String> = lib.profiles.iter().map(|p| p.name.clone()).collect();
    let loaded = model.profile_name.clone();
    // A profile's patches as a part's recall list: "Stack · Patch".
    let patches_of = |name: &str| -> (Vec<String>, Vec<String>) {
        lib.profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p: &ProfileEntry| {
                (
                    p.patch_list.iter().map(|x| x.name.clone()).collect(),
                    p.patch_list
                        .iter()
                        .map(|x| {
                            if x.stack.is_empty() {
                                x.name.clone()
                            } else {
                                format!("{} · {}", x.stack, x.name)
                            }
                        })
                        .collect(),
                )
            })
            .unwrap_or_default()
    };

    // One editor open at a time: the current song's entry, or a part.
    let mut editing_song = use_signal(|| false);
    let mut editing_part = use_signal(|| None::<usize>);
    let mut adding_part = use_signal(|| false);

    let set_name = model
        .setlists
        .get(model.setlist_index as usize)
        .cloned()
        .unwrap_or_else(|| "No setlist".to_string());
    let current = model.song_index as usize;
    // A patch as its chip: the name, tinted with the colour of the stack
    // that holds it (the song's rotations are in the stacks already), else
    // the profile's own list.
    let patch_chip = |patch: &str| -> (String, &'static str) {
        if patch.is_empty() {
            return ("stays".to_string(), DIM);
        }
        let stack = model
            .stacks
            .iter()
            .find(|s| s.patches.iter().any(|p| p.eq_ignore_ascii_case(patch)))
            .map(|s| s.name.clone())
            .or_else(|| {
                lib.profiles
                    .iter()
                    .flat_map(|p| p.patch_list.iter())
                    .find(|x| x.name.eq_ignore_ascii_case(patch))
                    .map(|x| x.stack.clone())
            });
        (patch.to_string(), stack.map_or(DIM, |s| crate::perform::folder_color(&s).0))
    };

    rsx! {
        aside {
            style: "width: {SIDEBAR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",

            // ── The set: its name large, ‹ › to the next set, ⋯ to manage ──
            div { style: "display: flex; flex-direction: column; gap: 2px; padding: 10px 12px 10px 14px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                PresetBar {
                    label: "Setlist",
                    name: set_name.clone(),
                    sub: if model.songs.len() == 1 { "1 song".to_string() } else { format!("{} songs", model.songs.len()) },
                    large: true,
                    options: model
                        .setlists
                        .iter()
                        .enumerate()
                        .map(|(i, n)| PickOption {
                            label: n.clone(),
                            live: i == model.setlist_index as usize,
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    on_pick: {
                        let rig = rig.clone();
                        move |i: usize| send(&rig, move |r| async move { let _ = r.select_setlist(i as u32).await; })
                    },
                    on_step: {
                        let rig = rig.clone();
                        let (at, n) = (model.setlist_index as i32, model.setlists.len() as i32);
                        move |d: i32| {
                            if n > 0 {
                                let to = (at + d).rem_euclid(n) as u32;
                                send(&rig, move |r| async move { let _ = r.select_setlist(to).await; });
                            }
                        }
                    },
                    menu: set_items(&model.setlists, model.setlist_index as usize),
                    on_menu: {
                        let rig = rig.clone();
                        let index = model.setlist_index;
                        move |p: Picked| {
                            let text = p.text;
                            match p.id {
                                "rename" => send(&rig, move |r| async move { let _ = r.rename_setlist(index, text).await; }),
                                "duplicate" => send(&rig, move |r| async move { let _ = r.duplicate_setlist(index, text).await; }),
                                "new" => send(&rig, move |r| async move { let _ = r.add_setlist(text).await; }),
                                "up" => send(&rig, move |r| async move { let _ = r.move_setlist(index, index.saturating_sub(1)).await; }),
                                "down" => send(&rig, move |r| async move { let _ = r.move_setlist(index, index + 1).await; }),
                                "delete" => send(&rig, move |r| async move { let _ = r.delete_setlist(index).await; }),
                                "library" => on_browse.call(Kind::Setlists),
                                _ => {}
                            }
                        }
                    },
                }
            }

            // ── The songs in set order, on one timeline edge; the song that
            // is up opens into its map ──
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 8px 8px 12px 6px;",
                if model.songs.is_empty() {
                    div { style: "padding: 10px 8px; font-size: 12px; color: {FAINT}; line-height: 1.5;",
                        "This set has no songs yet."
                    }
                }
                div { style: "position: relative; display: flex; flex-direction: column;",
                    // The timeline: one line the song nodes sit on.
                    if !model.songs.is_empty() {
                        div { style: "position: absolute; left: 13px; top: 14px; bottom: 14px; width: 1px; background: {LINE}; z-index: 0;" }
                    }
                    for (i, song) in model.songs.iter().cloned().enumerate() {
                        {
                            let state = if i < current { Node::Done } else if i == current { Node::Now } else { Node::Ahead };
                            let rig_play = rig.clone();
                            rsx! {
                                div { key: "{i}-{song.name}", style: "display: flex; flex-direction: column;",
                                    div {
                                        class: if state == Node::Now { "group" } else { "group hover:bg-accent/30" },
                                        style: format!(
                                            "display: flex; align-items: center; gap: 8px; min-width: 0; padding: 7px 6px 7px 0; \
                                             border-radius: 6px; cursor: pointer; opacity: {};",
                                            if state == Node::Done { "0.5" } else { "1" },
                                        ),
                                        onclick: move |_| {
                                            send(&rig_play, move |r| async move { let _ = r.select_song(i as u32).await; });
                                        },
                                        TimelineNode { state, accent: false }
                                        span { style: "width: 14px; flex-shrink: 0; font-size: 10px; font-family: monospace; color: {FAINT};",
                                            "{i + 1}"
                                        }
                                        span {
                                            style: format!(
                                                "flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden; font-size: 13px; \
                                                 font-weight: {}; color: {};",
                                                if state == Node::Now { 700 } else { 500 },
                                                if state == Node::Now { TEXT } else { MUTED },
                                            ),
                                            "{song.name}"
                                        }
                                        if i == current + 1 {
                                            span { style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.12em; color: {FAINT};",
                                                "NEXT"
                                            }
                                        }
                                        KeyChip { key_name: song.key.clone() }
                                        span { style: "width: 26px; flex-shrink: 0; text-align: right; font-size: 10px; font-family: monospace; color: {FAINT};",
                                            "{song.bpm}"
                                        }
                                        if state == Node::Now {
                                            div { class: if editing_song() { "" } else { "opacity-40 group-hover:opacity-100" }, style: "display: flex; flex-shrink: 0;",
                                                Tool {
                                                    icon: fts_chrome::Icon::Pencil,
                                                    title: "Key, tempo and place in this set",
                                                    onclick: move |()| {
                                                        editing_song.toggle();
                                                        editing_part.set(None);
                                                    },
                                                }
                                            }
                                        }
                                    }
                                    if state == Node::Now && editing_song() {
                                        div { style: "margin-left: 28px;",
                                            SongEntryEditor {
                                                key: "{song.name}-{song.key}-{song.bpm}-{model.song_profile}-{model.start_part}",
                                                song: song.name.clone(),
                                                profile: model.song_profile.clone(),
                                                start_part: model.start_part.clone(),
                                                profiles: profile_names.clone(),
                                                parts: model.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                                                index: i,
                                                count: model.songs.len(),
                                                setlist: model.setlist_index,
                                                song_key: song.key.clone(),
                                                bpm: song.bpm,
                                                on_done: move |()| editing_song.set(false),
                                            }
                                        }
                                    }
                                    if state == Node::Now {
                                        {
                                            // Sections: runs of parts with one section name.
                                            let mut sections: Vec<(String, Vec<usize>)> = Vec::new();
                                            for (pi, p) in model.parts.iter().enumerate() {
                                                match sections.last_mut() {
                                                    Some((name, idx)) if name.eq_ignore_ascii_case(&p.section) => idx.push(pi),
                                                    _ => sections.push((p.section.clone(), vec![pi])),
                                                }
                                            }
                                            let at = model.part_index as usize;
                                            let cur_sec = sections.iter().position(|(_, idx)| idx.contains(&at)).unwrap_or(0);
                                            let count = sections.len();
                                            let on = if model.song_profile.is_empty() { loaded.clone() } else { model.song_profile.clone() };
                                            let starts = if model.start_part.is_empty() { String::new() } else { format!(" · starts {}", model.start_part) };
                                            // Sections only when parts have been grouped into them;
                                            // otherwise the song is its parts.
                                            let grouped = model.parts.iter().any(|p| !p.section.eq_ignore_ascii_case(&p.name));
                                            let unit = if grouped { "section" } else { "part" };
                                            // Where a song with no start part opens.
                                            let start_chip = (model.start_part.is_empty() && !model.start_patch.is_empty())
                                                .then(|| patch_chip(&model.start_patch));
                                            // The switches the song (or the part up) tunes.
                                            let tuned: Vec<signal_guitar_proto::PerfStack> = model
                                                .stacks
                                                .iter()
                                                .filter(|st| st.song_tuned || st.part_tuned)
                                                .cloned()
                                                .collect();
                                            rsx! {
                                                div { style: "display: flex; flex-direction: column; gap: 1px; margin: 0 0 8px 28px;",
                                                    span { style: "padding: 0 0 6px; font-size: 10px; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                                                        "on {on}{starts}"
                                                    }
                                                    if let Some(chip) = start_chip {
                                                        div { style: "display: flex; align-items: center; gap: 8px; padding: 0 6px 6px 0;",
                                                            span { style: "{crate::theme::EYEBROW}", "Starts" }
                                                            div { style: "flex: 1;" }
                                                            PatchChip { label: chip.0.clone(), colour: chip.1, lit: true }
                                                        }
                                                    }
                                                    if count > 0 {
                                                        div { style: "display: flex; gap: 2px; padding: 0 6px 6px 0;",
                                                            for k in 0..count {
                                                                span {
                                                                    key: "{k}",
                                                                    style: format!(
                                                                        "flex: 1; height: 3px; border-radius: 2px; background: {};",
                                                                        if k < cur_sec { "#3f5a3a" } else if k == cur_sec { LIVE } else { "#27272a" },
                                                                    ),
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { style: "position: relative; display: flex; flex-direction: column; gap: 1px;",
                                                        if count > 0 {
                                                            div { style: "position: absolute; left: 13px; top: 12px; bottom: 12px; width: 1px; background: {LINE}; z-index: 0;" }
                                                        }
                                                        for (si, (sec_name, idx)) in sections.iter().cloned().enumerate() {
                                                            {
                                                                let first = idx[0];
                                                                let single = idx.len() == 1
                                                                    && model.parts[first].name.eq_ignore_ascii_case(&sec_name);
                                                                let live = si == cur_sec;
                                                                let state = if si < cur_sec { Node::Done } else if live { Node::Now } else { Node::Ahead };
                                                                let is_next = si == cur_sec + 1;
                                                                let lead = model.parts[first].clone();
                                                                let rig_sec = rig.clone();
                                                                let chip = patch_chip(&lead.patch);
                                                                rsx! {
                                                                    div { key: "sec-{si}-{sec_name}", style: "display: flex; flex-direction: column;",
                                                                        div {
                                                                            class: if live { "" } else { "hover:bg-accent/30" },
                                                                            style: format!(
                                                                                "display: flex; align-items: center; gap: 8px; min-width: 0; padding: 6px 6px 6px 0; \
                                                                                 border-radius: 0 6px 6px 0; cursor: pointer; background: {}; opacity: {};",
                                                                                if live { LIVE_BG } else { "transparent" },
                                                                                if state == Node::Done { "0.5" } else { "1" },
                                                                            ),
                                                                            title: if live && single { "Edit this section" } else { "Play this section" },
                                                                            onclick: move |_| {
                                                                                if live && single {
                                                                                    editing_part.set(if editing_part() == Some(first) { None } else { Some(first) });
                                                                                    editing_song.set(false);
                                                                                } else {
                                                                                    editing_part.set(None);
                                                                                    send(&rig_sec, move |r| async move {
                                                                                        let _ = r.select_part(first as u32).await;
                                                                                    });
                                                                                }
                                                                            },
                                                                            TimelineNode { state, accent: true }
                                                                            span {
                                                                                style: format!(
                                                                                    "flex: 1 1 0; min-width: 0; font-size: 12px; white-space: nowrap; overflow: hidden; \
                                                                                     font-weight: {}; color: {};",
                                                                                    if live { 700 } else { 500 },
                                                                                    if live { TEXT } else { MUTED },
                                                                                ),
                                                                                "{sec_name}"
                                                                            }
                                                                            if is_next {
                                                                                span { style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.12em; color: {FAINT};",
                                                                                    "NEXT"
                                                                                }
                                                                            }
                                                                            if single {
                                                                                PartMarks { part: lead.clone() }
                                                                                PatchChip { label: chip.0.clone(), colour: chip.1, lit: live }
                                                                            } else {
                                                                                span { style: "flex-shrink: 0; font-size: 10px; font-family: monospace; color: {DIM};",
                                                                                    "{idx.len()} parts"
                                                                                }
                                                                            }
                                                                        }
                                                                        // A section of several parts: its parts as sublines.
                                                                        if !single {
                                                                            for &pi in idx.iter() {
                                                                                {
                                                                                    let part = model.parts[pi].clone();
                                                                                    let part_on = pi == at;
                                                                                    let rig = rig.clone();
                                                                                    let chip = patch_chip(&part.patch);
                                                                                    rsx! {
                                                                                        div {
                                                                                            key: "{pi}-{part.name}",
                                                                                            class: if part_on { "" } else { "hover:bg-accent/30" },
                                                                                            style: format!(
                                                                                                "display: flex; align-items: center; gap: 6px; min-width: 0; margin-left: 28px; \
                                                                                                 padding: 3px 6px 3px 0; border-radius: 5px; cursor: pointer; color: {};",
                                                                                                if part_on { TEXT } else { FAINT },
                                                                                            ),
                                                                                            onclick: move |_| {
                                                                                                if part_on {
                                                                                                    editing_part.set(if editing_part() == Some(pi) { None } else { Some(pi) });
                                                                                                    editing_song.set(false);
                                                                                                } else {
                                                                                                    editing_part.set(None);
                                                                                                    send(&rig, move |r| async move { let _ = r.select_part(pi as u32).await; });
                                                                                                }
                                                                                            },
                                                                                            span { style: "flex: 1 1 0; min-width: 0; font-size: 11px; white-space: nowrap; overflow: hidden;",
                                                                                                "{part.name}"
                                                                                            }
                                                                                            PartMarks { part: part.clone() }
                                                                                            PatchChip { label: chip.0.clone(), colour: chip.1, lit: part_on }
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        // The editor of a part of this section, when open.
                                                                        for &pi in idx.iter().filter(|&&pi| editing_part() == Some(pi)) {
                                                                            {
                                                                                let part = model.parts[pi].clone();
                                                                                let grouped = !part.section.eq_ignore_ascii_case(&part.name);
                                                                                let base = if !part.profile.is_empty() {
                                                                                    part.profile.clone()
                                                                                } else if !model.song_profile.is_empty() {
                                                                                    model.song_profile.clone()
                                                                                } else {
                                                                                    loaded.clone()
                                                                                };
                                                                                let (names, labels) = patches_of(&base);
                                                                                rsx! {
                                                                                    div { key: "ed-{pi}", style: "padding: 2px 0 6px 28px;",
                                                                                        PartEditor {
                                                                                            key: "{pi}-{part.name}-{part.profile}-editor",
                                                                                            index: pi,
                                                                                            count: model.parts.len(),
                                                                                            name: part.name.clone(),
                                                                                            patch: part.patch.clone(),
                                                                                            section: if grouped { part.section.clone() } else { String::new() },
                                                                                            profile_switches: part.profile_switches,
                                                                                            profile: part.profile.clone(),
                                                                                            song_profile: if model.song_profile.is_empty() { loaded.clone() } else { model.song_profile.clone() },
                                                                                            profiles: profile_names.clone(),
                                                                                            patches: names,
                                                                                            labels,
                                                                                            on_done: move |()| editing_part.set(None),
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
                                                    // The song's switch setup: each switch it tunes, with its
                                                    // rotation, in the switch's colour.
                                                    if !tuned.is_empty() {
                                                        div { style: "display: flex; flex-direction: column; gap: 2px; padding: 8px 6px 4px 0;",
                                                            span { style: "{crate::theme::EYEBROW}", "Switches" }
                                                            for st in tuned.iter() {
                                                                {
                                                                    let (colour, _) = crate::perform::folder_color(&st.name);
                                                                    let rotation = st.patches.join(" · ");
                                                                    let mut tags: Vec<&str> = Vec::new();
                                                                    if st.part_tuned { tags.push("PART"); }
                                                                    if st.momentary { tags.push("HOLD"); }
                                                                    if st.no_rotate { tags.push("NO ROTATE"); }
                                                                    let tags = tags.join(" · ");
                                                                    rsx! {
                                                                        div { key: "sw-{st.name}",
                                                                            class: "hover:bg-accent/30",
                                                                            style: "display: flex; align-items: center; gap: 6px; min-width: 0; padding: 2px 4px 2px 0; border-radius: 4px; cursor: context-menu;",
                                                                            title: "Right-click: make this switch's patch a part, or rename it",
                                                                            oncontextmenu: {
                                                                                let parts = crate::part_menu::parts_of(&model);
                                                                                let patch = crate::part_menu::stack_patch(st);
                                                                                let rig = rig.clone();
                                                                                move |e: MouseEvent| {
                                                                                    e.prevent_default();
                                                                                    let items = crate::part_menu::items(&parts, &patch);
                                                                                    let (rig, parts, patch) = (rig.clone(), parts.clone(), patch.clone());
                                                                                    crate::kit::context_menu(popup_host, &e, items, EventHandler::new(move |p: crate::kit::Picked| {
                                                                                        crate::part_menu::act(&rig, &parts, &patch, p);
                                                                                    }));
                                                                                }
                                                                            },
                                                                            span { style: "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {colour};" }
                                                                            span { style: "flex-shrink: 0; font-size: 11px; font-weight: 600; color: {TEXT};", "{st.name}" }
                                                                            span { style: "flex: 1 1 auto; min-width: 0; font-size: 10px; color: {MUTED}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                                                title: "{rotation}",
                                                                                "{rotation}"
                                                                            }
                                                                            if !tags.is_empty() {
                                                                                span { style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: {FAINT};", "{tags}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if adding_part() {
                                                        NewPart { on_done: move |()| adding_part.set(false) }
                                                    } else {
                                                        button {
                                                            style: "align-self: flex-start; margin-top: 2px; padding: 4px 0; border: none; background: transparent; \
                                                                    cursor: pointer; font-size: 11px; font-weight: 600; color: {FAINT};",
                                                            onclick: move |_| {
                                                                adding_part.set(true);
                                                                editing_part.set(None);
                                                            },
                                                            if grouped { "+ section" } else { "+ part" }
                                                        }
                                                    }
                                                    if count > 0 {
                                                        span { style: "padding: 2px 0; font-size: 10px; color: {DIM};",
                                                            "Switch 5: next {unit} · hold: back"
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
            }

            // ── Building the set is the library's job ──
            div { style: "display: flex; gap: 6px; padding: 10px 12px; border-top: 1px solid {LINE}; flex-shrink: 0;",
                FootButton { label: "+ Add song", onclick: move |()| on_browse.call(Kind::Songs) }
                FootButton { label: "All sets", onclick: move |()| on_browse.call(Kind::Setlists) }
            }
        }
    }
}

/// Where a song or a section sits on the timeline.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Node {
    /// Played: a small check.
    Done,
    /// Up now: filled.
    Now,
    /// To come: hollow.
    Ahead,
}

/// A node on the timeline edge. `accent`: the one that plays in the view
/// (the section up) gets the green; a song up is filled white.
#[component]
fn TimelineNode(state: Node, accent: bool) -> Element {
    let (size, bg, border, glyph) = match state {
        Node::Done => (10, SIDEBAR, DIM, "✓"),
        Node::Now if accent => (10, LIVE, LIVE, ""),
        Node::Now => (10, TEXT, TEXT, ""),
        Node::Ahead => (8, SIDEBAR, DIM, ""),
    };
    rsx! {
        // Positioned above the timeline line, which is itself positioned
        // and so would paint over an in-flow node.
        span { style: "width: 28px; flex-shrink: 0; display: flex; justify-content: center; position: relative; z-index: 1;",
            span {
                style: "width: {size}px; height: {size}px; border-radius: 999px; background: {bg}; \
                        border: 1.5px solid {border}; display: flex; align-items: center; justify-content: center; \
                        font-size: 7px; line-height: 1; color: {FAINT};",
                "{glyph}"
            }
        }
    }
}

/// A song's key, in a small rounded square.
#[component]
fn KeyChip(key_name: String) -> Element {
    if key_name.is_empty() {
        return rsx! {};
    }
    rsx! {
        span {
            style: "flex-shrink: 0; min-width: 18px; height: 16px; padding: 0 3px; border-radius: 4px; \
                    border: 1px solid {LINE_STRONG}; display: flex; align-items: center; justify-content: center; \
                    font-size: 10px; font-weight: 600; color: {MUTED};",
            "{key_name}"
        }
    }
}

/// The patch a section loads: its name on a tint of its stack's colour.
#[component]
fn PatchChip(label: String, colour: &'static str, lit: bool) -> Element {
    let ink = if lit { TEXT } else { MUTED };
    rsx! {
        span {
            style: "flex-shrink: 1; min-width: 0; max-width: 118px; display: flex; align-items: center; gap: 4px; \
                    padding: 1px 6px 1px 4px; border-radius: 4px; background: {colour}22; \
                    border-left: 2px solid {colour}; font-size: 10px; color: {ink}; white-space: nowrap; overflow: hidden;",
            title: "{label}",
            "{label}"
        }
    }
}

/// What a part does beyond its patch, as small grey marks: it plays the
/// profile's switches, it tunes n switches, it changes n parameters.
#[component]
fn PartMarks(part: signal_guitar_proto::PerfPart) -> Element {
    rsx! {
        if part.profile_switches {
            span { style: "flex-shrink: 0; display: flex; color: {FAINT};", title: "Plays the profile's switches",
                fts_chrome::Glyph { icon: fts_chrome::Icon::Profile, size: 10 }
            }
        }
        if part.switch_count > 0 {
            // The font has no ⇄: the switches glyph and the count.
            span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 2px; font-size: 10px; \
                           font-family: monospace; color: {FAINT};",
                title: "Tunes {part.switch_count} switches",
                fts_chrome::Glyph { icon: fts_chrome::Icon::Perform, size: 10 }
                "{part.switch_count}"
            }
        }
        if !part.overrides.is_empty() {
            span { style: "flex-shrink: 0; font-size: 10px; font-family: monospace; color: {FAINT};",
                title: "Changes {part.overrides.len()} settings", "±{part.overrides.len()}"
            }
        }
        if !part.profile.is_empty() {
            span { style: "flex-shrink: 0; font-size: 10px; color: {FAINT};", title: "Played on {part.profile}", "{part.profile}" }
        }
    }
}

#[component]
fn FootButton(label: String, onclick: EventHandler<()>) -> Element {
    rsx! {
        crate::kit::Button { label, grow: true, small: true, onclick }
    }
}

/// A small icon button — the kit's.
#[component]
fn Tool(
    icon: fts_chrome::Icon,
    title: String,
    #[props(default = false)] flip: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        crate::kit::IconButton { icon, title, flip, disabled, danger, onclick }
    }
}

/// A text field for the editors. Enter commits; Escape cancels.
#[component]
fn Field(
    value: String,
    #[props(default = String::new())] placeholder: String,
    #[props(default = "1 1 0".to_string())] flex: String,
    #[props(default = false)] autofocus: bool,
    on_input: EventHandler<String>,
    on_enter: EventHandler<()>,
    on_escape: EventHandler<()>,
) -> Element {
    rsx! {
        input {
            style: "flex: {flex}; min-width: 0; font-size: 12px; color: {TEXT}; background: {FIELD}; \
                    border: 1px solid {LINE_STRONG}; border-radius: 6px; padding: 5px 7px; outline: none;",
            value: "{value}",
            placeholder: "{placeholder}",
            onmounted: move |e| {
                if autofocus {
                    spawn(async move {
                        let _ = e.data().set_focus(true).await;
                    });
                }
            },
            oninput: move |e| on_input.call(e.value()),
            onkeydown: move |e: KeyboardEvent| match e.key() {
                Key::Enter => on_enter.call(()),
                Key::Escape => on_escape.call(()),
                _ => {}
            },
        }
    }
}

/// The current song's place in this set: key and tempo for the set (empty /
/// zero fall back to the song's own), its position, and taking it out.
#[component]
fn SongEntryEditor(
    song: String,
    /// The song's profile; empty keeps whatever is loaded.
    profile: String,
    start_part: String,
    profiles: Vec<String>,
    parts: Vec<String>,
    index: usize,
    count: usize,
    setlist: u32,
    song_key: String,
    bpm: u32,
    on_done: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut key = use_signal(|| song_key.clone());
    let mut tempo = use_signal(|| bpm.to_string());
    let save = {
        let rig = rig.clone();
        move || {
            let (k, b) = (
                key.peek().trim().to_string(),
                tempo.peek().trim().parse().unwrap_or(0),
            );
            send(&rig, move |r| async move {
                let _ = r.set_setlist_entry(index as u32, k, b).await;
            });
            on_done.call(());
        }
    };
    let i = index as u32;
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; margin: 4px 0 4px 24px; padding: 8px; \
                      border-radius: 8px; border: 1px solid {LINE};",
            span { style: "font-size: 10px; color: {FAINT};", "Played on" }
            // Index 0 is "nothing chosen": the song keeps whatever is loaded.
            Picker {
                options: std::iter::once("— whatever is loaded —".to_string()).chain(profiles.iter().cloned()).collect::<Vec<_>>(),
                selected: profiles.iter().position(|p| p.eq_ignore_ascii_case(&profile)).map_or(0, |p| p as u32 + 1),
                width: "100%".to_string(),
                on_select: {
                    let (rig, song, profiles) = (rig.clone(), song.clone(), profiles.clone());
                    move |i: u32| {
                        let chosen = if i == 0 { String::new() } else { profiles.get(i as usize - 1).cloned().unwrap_or_default() };
                        let song = song.clone();
                        send(&rig, move |r| async move { let _ = r.set_song_profile(song, chosen).await; });
                    }
                },
            }
            span { style: "font-size: 10px; color: {FAINT};", "Starts on" }
            Picker {
                options: std::iter::once("— the profile's default —".to_string()).chain(parts.iter().cloned()).collect::<Vec<_>>(),
                selected: parts.iter().position(|p| p.eq_ignore_ascii_case(&start_part)).map_or(0, |p| p as u32 + 1),
                width: "100%".to_string(),
                on_select: {
                    let (rig, song, parts) = (rig.clone(), song.clone(), parts.clone());
                    move |i: u32| {
                        let chosen = if i == 0 { String::new() } else { parts.get(i as usize - 1).cloned().unwrap_or_default() };
                        let song = song.clone();
                        send(&rig, move |r| async move { let _ = r.set_song_start_part(song, chosen).await; });
                    }
                },
            }
            span { style: "font-size: 10px; color: {FAINT};", "In this set" }
            div { style: "display: flex; gap: 6px; align-items: center;",
                Field {
                    value: key(),
                    placeholder: "Key",
                    flex: "0 0 56px",
                    autofocus: true,
                    on_input: move |v: String| key.set(v),
                    on_enter: { let save = save.clone(); move |()| save() },
                    on_escape: move |()| on_done.call(()),
                }
                Field {
                    value: tempo(),
                    placeholder: "bpm",
                    flex: "0 0 64px",
                    on_input: move |v: String| tempo.set(v),
                    on_enter: { let save = save.clone(); move |()| save() },
                    on_escape: move |()| on_done.call(()),
                }
                Tool { icon: fts_chrome::Icon::Check, title: "Save", onclick: { let save = save.clone(); move |()| save() } }
            }
            div { style: "display: flex; gap: 6px; align-items: center;",
                Tool {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Earlier in the set",
                    flip: true,
                    disabled: index == 0,
                    onclick: { let rig = rig.clone(); move |()| send(&rig, move |r| async move { let _ = r.move_song(i, i - 1).await; }) },
                }
                Tool {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Later in the set",
                    disabled: index + 1 >= count,
                    onclick: { let rig = rig.clone(); move |()| send(&rig, move |r| async move { let _ = r.move_song(i, i + 1).await; }) },
                }
                div { style: "flex: 1;" }
                Tool {
                    icon: fts_chrome::Icon::Close,
                    title: "Take it out of this set",
                    danger: true,
                    onclick: {
                        let rig = rig.clone();
                        move |()| {
                            send(&rig, move |r| async move { let _ = r.remove_setlist_entry(setlist, i).await; });
                            on_done.call(());
                        }
                    },
                }
            }
        }
    }
}

/// A part's editor: which patch it lays over the profile, its name, its
/// place, removing it.
#[component]
fn PartEditor(
    index: usize,
    count: usize,
    name: String,
    patch: String,
    /// The section it belongs to; empty = its own.
    section: String,
    /// It plays the profile's own switches.
    profile_switches: bool,
    /// The part's own profile; empty = the song's.
    profile: String,
    /// What "the song's" means right now, for the placeholder.
    song_profile: String,
    profiles: Vec<String>,
    patches: Vec<String>,
    /// `patches` as the picker shows them — "Stack · Patch", so a part reads
    /// as "this stack's other patch".
    labels: Vec<String>,
    on_done: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut rename = use_signal(|| name.clone());
    let mut section_name = use_signal(|| section.clone());
    let i = index as u32;
    let selected = patches
        .iter()
        .position(|p| *p == patch)
        .map_or(u32::MAX, |p| p as u32);
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; margin: 2px 0 6px; padding: 8px; \
                      border-radius: 8px; border: 1px solid {LINE};",
            span { style: "font-size: 10px; color: {FAINT};", "Profile" }
            Picker {
                options: std::iter::once(format!("— the song's ({song_profile}) —")).chain(profiles.iter().cloned()).collect::<Vec<_>>(),
                selected: profiles.iter().position(|p| p.eq_ignore_ascii_case(&profile)).map_or(0, |p| p as u32 + 1),
                width: "100%".to_string(),
                on_select: {
                    let (rig, part, profiles) = (rig.clone(), name.clone(), profiles.clone());
                    move |i: u32| {
                        let chosen = if i == 0 { String::new() } else { profiles.get(i as usize - 1).cloned().unwrap_or_default() };
                        let part = part.clone();
                        send(&rig, move |r| async move { let _ = r.set_part_profile(part, chosen).await; });
                    }
                },
            }
            span { style: "font-size: 10px; color: {FAINT};", "Recalls" }
            div { style: "display: flex; gap: 6px; align-items: center;",
                Picker {
                    options: labels.clone(),
                    selected,
                    placeholder: "— the profile's current patch —".to_string(),
                    size: PickerSize::Normal,
                    width: "100%".to_string(),
                    on_select: {
                        let (rig, part, patches) = (rig.clone(), name.clone(), patches.clone());
                        move |p: u32| {
                            let Some(patch) = patches.get(p as usize).cloned() else { return };
                            let part = part.clone();
                            send(&rig, move |r| async move { let _ = r.set_part_patch(part, patch).await; });
                        }
                    },
                }
                if !patch.is_empty() {
                    Tool {
                        icon: fts_chrome::Icon::Close,
                        title: "Recall nothing — a label only",
                        onclick: {
                            let (rig, part) = (rig.clone(), name.clone());
                            move |()| {
                                let part = part.clone();
                                send(&rig, move |r| async move { let _ = r.set_part_patch(part, String::new()).await; });
                            }
                        },
                    }
                }
            }
            span { style: "font-size: 10px; color: {FAINT};", "Section — parts in a row with the same section are one" }
            Field {
                value: section_name(),
                placeholder: "its own section".to_string(),
                on_input: move |v: String| section_name.set(v),
                on_enter: {
                    let (rig, part) = (rig.clone(), name.clone());
                    move |()| {
                        let sec = section_name.peek().trim().to_string();
                        let part = part.clone();
                        send(&rig, move |r| async move { let _ = r.set_part_section(part, sec).await; });
                    }
                },
                on_escape: move |()| on_done.call(()),
            }
            button {
                style: "display: flex; align-items: center; gap: 6px; padding: 2px 0; border: none; background: transparent; \
                        cursor: pointer; font-size: 11px; color: {MUTED}; text-align: left;",
                title: "The song's switch tuning steps aside for this part (its own switch changes still apply)",
                onclick: {
                    let (rig, part) = (rig.clone(), name.clone());
                    move |_| {
                        let part = part.clone();
                        send(&rig, move |r| async move { let _ = r.set_part_profile_switches(part, !profile_switches).await; });
                    }
                },
                span { style: "width: 12px; color: #22c55e;", if profile_switches { "✓" } else { "○" } }
                "Plays the profile's switches"
            }
            span { style: "font-size: 10px; color: {FAINT};", "Name" }
            div { style: "display: flex; gap: 6px; align-items: center;",
                Field {
                    value: rename(),
                    on_input: move |v: String| rename.set(v),
                    on_enter: {
                        let (rig, old) = (rig.clone(), name.clone());
                        move |()| {
                            let new = rename.peek().trim().to_string();
                            if !new.is_empty() && new != old {
                                let old = old.clone();
                                send(&rig, move |r| async move { let _ = r.rename_part(old, new).await; });
                            }
                            on_done.call(());
                        }
                    },
                    on_escape: move |()| on_done.call(()),
                }
            }
            div { style: "display: flex; gap: 6px; align-items: center;",
                Tool {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Earlier in the song",
                    flip: true,
                    disabled: index == 0,
                    onclick: { let rig = rig.clone(); move |()| send(&rig, move |r| async move { let _ = r.move_part(i, i - 1).await; }) },
                }
                Tool {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Later in the song",
                    disabled: index + 1 >= count,
                    onclick: { let rig = rig.clone(); move |()| send(&rig, move |r| async move { let _ = r.move_part(i, i + 1).await; }) },
                }
                div { style: "flex: 1;" }
                Tool {
                    icon: fts_chrome::Icon::Close,
                    title: "Remove this part",
                    danger: true,
                    onclick: {
                        let (rig, part) = (rig.clone(), name.clone());
                        move |()| {
                            let part = part.clone();
                            send(&rig, move |r| async move { let _ = r.remove_part(part).await; });
                            on_done.call(());
                        }
                    },
                }
            }
        }
    }
}

/// Naming a new part on the current song.
#[component]
fn NewPart(on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut name = use_signal(String::new);
    rsx! {
        div { style: "display: flex; gap: 6px; align-items: center; padding: 2px 0;",
            Field {
                value: name(),
                placeholder: "Verse 1, Rhythm, Clean lead…",
                autofocus: true,
                on_input: move |v: String| name.set(v),
                // Enter adds and stays open for the next one: parts are
                // named in a run, top to bottom.
                on_enter: {
                    let rig = rig.clone();
                    move |()| {
                        let n = name.peek().trim().to_string();
                        if !n.is_empty() {
                            send(&rig, move |r| async move { let _ = r.add_part(n).await; });
                            name.set(String::new());
                        }
                    }
                },
                on_escape: move |()| on_done.call(()),
            }
            Tool { icon: fts_chrome::Icon::Close, title: "Done", onclick: move |()| on_done.call(()) }
        }
    }
}
