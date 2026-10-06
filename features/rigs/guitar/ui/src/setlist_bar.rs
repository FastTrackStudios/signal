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
use signal_guitar_proto::{LibraryModel, PerformanceModel};
use signal_widgets::{Picker, PickerSize};

use crate::library::Kind;

use crate::kit::{MenuItem, PickOption, Picked, PresetBar};
use crate::theme::{
    DIM, FAINT, FIELD, LINE, LINE_STRONG, LIVE, LIVE_BG, MUTED, SIDEBAR, TEXT,
};

/// The set's menu: rename, duplicate, a new set, move it in the list,
/// delete (refused for the only one), and the library.
fn set_items(sets: &[String], index: usize) -> Vec<MenuItem> {
    let name = sets.get(index).cloned().unwrap_or_default();
    let others: Vec<String> = sets
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&name))
        .cloned()
        .collect();
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
        MenuItem::run("down", "Move down the list")
            .unless((index + 1 >= sets.len()).then(|| "Last".to_string())),
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
pub fn SetlistSidebar(
    model: PerformanceModel,
    on_browse: EventHandler<Kind>,
    /// The full sidebar (a phone's width): each section's patch and marks
    /// beside it. Minimal keeps the song, its key, tempo and what it starts
    /// on, and the playing section's patch.
    #[props(default)]
    full: bool,
) -> Element {
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
        (
            patch.to_string(),
            stack.map_or(DIM, |s| crate::perform::folder_color(&s).0),
        )
    };

    rsx! {
        aside {
            style: "width: {crate::kit::pane_w(full)}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",

            // ── The set: its name large, ‹ › to the next set, ⋯ to manage ──
            div { style: "display: flex; flex-direction: column; gap: 2px; padding: 10px 12px 10px 14px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                // Right-click: the set's menu (rename, duplicate, move…), as ⋯.
                oncontextmenu: {
                    let rig = rig.clone();
                    let (sets, index) = (model.setlists.clone(), model.setlist_index);
                    move |e: MouseEvent| {
                        e.prevent_default();
                        let rig = rig.clone();
                        crate::kit::context_menu(popup_host, &e, set_items(&sets, index as usize), EventHandler::new(move |p: Picked| set_act(&rig, index, on_browse, p)));
                    }
                },
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
                        move |p: Picked| set_act(&rig, index, on_browse, p)
                    },
                }
            }

            // ── The songs in set order, on one timeline edge, each with the
            // patch it starts on. What a song does beyond that (its parts,
            // its switches) is the song's own page, not the set's list. ──
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
                                        class: if state == Node::Now { "sg-row" } else { "sg-row sg-hover" },
                                        style: format!(
                                            "display: flex; align-items: center; gap: 8px; min-width: 0; padding: 7px 6px 7px 0; \
                                             border-radius: 6px; cursor: pointer; opacity: {};",
                                            if state == Node::Done { "0.5" } else { "1" },
                                        ),
                                        onclick: move |_| {
                                            send(&rig_play, move |r| async move { let _ = r.select_song(i as u32).await; });
                                        },
                                        // Right-click: edit the song (its name, key and
                                        // tempo) and its place in this set.
                                        oncontextmenu: {
                                            let rig = rig.clone();
                                            let row = song.clone();
                                            let entry = lib.songs.iter().find(|s| s.name == song.name).cloned();
                                            let names: Vec<String> = lib.songs.iter().map(|s| s.name.clone()).collect();
                                            let (count, setlist) = (model.songs.len(), model.setlist_index);
                                            move |e: MouseEvent| {
                                                e.prevent_default();
                                                let items = song_items(&row, entry.as_ref(), &names, i, count);
                                                let (rig, row, entry) = (rig.clone(), row.clone(), entry.clone());
                                                crate::kit::context_menu(popup_host, &e, items, EventHandler::new(move |p: Picked| song_act(&rig, &row, entry.as_ref(), i, setlist, p)));
                                            }
                                        },
                                        TimelineNode { state, accent: false }
                                        span { style: "width: 14px; flex-shrink: 0; font-size: 11px; font-family: monospace; color: {FAINT};",
                                            "{i + 1}"
                                        }
                                        // The song, and under it the patch it starts on (and
                                        // NEXT) — what you need before counting in. The name
                                        // has its line to itself, so it wraps, never clips.
                                        // (Basis `auto`, not 0: Blitz lays wrapping text out at
                                        // the flex basis, so a 0 basis wrapped it a word wide.)
                                        div { style: "flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                            span {
                                                style: format!(
                                                    "align-self: stretch; min-width: 0; white-space: normal; line-height: 1.25; font-size: 13px; \
                                                     font-weight: {}; color: {};",
                                                    if state == Node::Now { 700 } else { 500 },
                                                    if state == Node::Now { TEXT } else { MUTED },
                                                ),
                                                "{song.name}"
                                            }
                                            if !song.start.is_empty() || i == current + 1 {
                                                div { style: "display: flex; flex-wrap: wrap; align-items: center; gap: 4px; min-width: 0;",
                                                    if !song.start.is_empty() {
                                                        {
                                                            let (label, colour) = patch_chip(&song.start);
                                                            rsx! { PatchChip { label, colour, lit: state == Node::Now } }
                                                        }
                                                    }
                                                    if i == current + 1 {
                                                        span { style: "flex-shrink: 0; padding: 1px 5px; border-radius: 4px; background: rgba(255,255,255,0.07); \
                                                                        font-size: 10px; font-weight: 700; letter-spacing: 0.1em; color: {MUTED};",
                                                            "NEXT"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        KeyChip { key_name: song.key.clone() }
                                        // The tempo, small: what the count-in will be.
                                        if song.bpm > 0 {
                                            span { style: "width: 24px; flex-shrink: 0; text-align: right; font-size: 11px; font-family: monospace; color: {FAINT};",
                                                title: "{song.bpm} BPM",
                                                "{song.bpm}"
                                            }
                                        }
                                    }
                                    // The song up: its sections, to see where the song is
                                    // and play any of them (the footswitches step them).
                                    if state == Node::Now && !model.parts.is_empty() {
                                        {
                                            let mut sections: Vec<(String, Vec<usize>)> = Vec::new();
                                            for (pi, p) in model.parts.iter().enumerate() {
                                                match sections.last_mut() {
                                                    Some((name, idx)) if name.eq_ignore_ascii_case(&p.section) => idx.push(pi),
                                                    _ => sections.push((p.section.clone(), vec![pi])),
                                                }
                                            }
                                            let at = model.part_index as usize;
                                            let cur_sec = sections.iter().position(|(_, idx)| idx.contains(&at)).unwrap_or(0);
                                            rsx! {
                                                div { style: "position: relative; display: flex; flex-direction: column; gap: 1px; margin: 0 0 8px 28px;",
                                                    div { style: "position: absolute; left: 13px; top: 12px; bottom: 12px; width: 1px; background: {LINE}; z-index: 0;" }
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
                                                                            class: if live { "" } else { "sg-hover" },
                                                                            style: format!(
                                                                                "display: flex; align-items: center; gap: 8px; min-width: 0; padding: 6px 6px 6px 0; \
                                                                                 border-radius: 0 6px 6px 0; cursor: pointer; background: {}; opacity: {};",
                                                                                if live { LIVE_BG } else { "transparent" },
                                                                                if state == Node::Done { "0.5" } else { "1" },
                                                                            ),
                                                                            title: "Play this section",
                                                                            onclick: move |_| {
                                                                                send(&rig_sec, move |r| async move {
                                                                                    let _ = r.select_part(first as u32).await;
                                                                                });
                                                                            },
                                                                            TimelineNode { state, accent: true }
                                                                            span {
                                                                                style: format!(
                                                                                    "flex: 1 1 auto; min-width: 56px; font-size: 12px; white-space: normal; line-height: 1.25; \
                                                                                     font-weight: {}; color: {};",
                                                                                    if live { 700 } else { 500 },
                                                                                    if live { TEXT } else { MUTED },
                                                                                ),
                                                                                "{sec_name}"
                                                                            }
                                                                            if is_next {
                                                                                span { style: "flex-shrink: 0; padding: 1px 5px; border-radius: 4px; background: rgba(255,255,255,0.07); \
                                                            font-size: 10px; font-weight: 700; letter-spacing: 0.1em; color: {MUTED};",
                                                                                    "NEXT"
                                                                                }
                                                                            }
                                                                            // Full: the patch beside the name. Minimal has
                                                                            // no room for both — the playing one's patch
                                                                            // goes on a line of its own, below.
                                                                            if single && full {
                                                                                PartMarks { part: lead.clone() }
                                                                                PatchChip { label: chip.0.clone(), colour: chip.1, lit: live }
                                                                            } else if !single {
                                                                                span { style: "flex-shrink: 0; font-size: 11px; font-family: monospace; color: {FAINT};",
                                                                                    "{idx.len()} parts"
                                                                                }
                                                                            }
                                                                        }
                                                                        if single && !full && live {
                                                                            div { style: "display: flex; min-width: 0; padding: 0 6px 4px 28px;",
                                                                                PatchChip { label: chip.0.clone(), colour: chip.1, lit: true }
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
                                                                                            class: if part_on { "" } else { "sg-hover" },
                                                                                            style: format!(
                                                                                                "display: flex; align-items: center; gap: 6px; min-width: 0; margin-left: 28px; \
                                                                                                 padding: 3px 6px 3px 0; border-radius: 5px; cursor: pointer; color: {};",
                                                                                                if part_on { TEXT } else { FAINT },
                                                                                            ),
                                                                                            onclick: move |_| {
                                                                                                send(&rig, move |r| async move { let _ = r.select_part(pi as u32).await; });
                                                                                            },
                                                                                            span { style: "flex: 1 1 auto; min-width: 48px; font-size: 11px; white-space: normal; line-height: 1.25;",
                                                                                                "{part.name}"
                                                                                            }
                                                                                            if full { PartMarks { part: part.clone() } }
                                                                                            if full {
                                                                                                PatchChip { label: chip.0.clone(), colour: chip.1, lit: part_on }
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
                    font-size: 11px; font-weight: 600; color: {MUTED};",
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
            style: "flex-shrink: 1; min-width: 0; max-width: 100%; display: flex; align-items: center; gap: 5px; \
                    padding: 1px 6px 1px 5px; border-radius: 4px; background: {colour}22; \
                    font-size: 11px; line-height: 1.25; color: {ink};",
            title: "{label}",
            // The stack's colour as a swatch, not an edge.
            span { style: "width: 6px; height: 6px; border-radius: 2px; flex-shrink: 0; background: {colour};" }
            // Wraps rather than clips: a long patch name in a narrow sidebar.
            span { style: "min-width: 0; white-space: normal;", "{label}" }
        }
    }
}

/// What a part does beyond its patch, as small grey marks: it plays the
/// profile's switches, it tunes n switches, it changes n parameters.
#[component]
fn PartMarks(part: signal_guitar_proto::PerfPart) -> Element {
    rsx! {
        // A repeat: linked to the part it repeats (same sound, edited
        // together).
        if !part.repeat_of.is_empty() {
            span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 3px; font-size: 11px; color: {MUTED}; white-space: nowrap;",
                title: "Repeats {part.repeat_of} — the same sound; editing one edits both",
                fts_chrome::Glyph { icon: fts_chrome::Icon::Refresh, size: 10 }
                "{part.repeat_of}"
            }
        }
        if part.profile_switches {
            span { style: "flex-shrink: 0; display: flex; color: {FAINT};", title: "Plays the profile's switches",
                fts_chrome::Glyph { icon: fts_chrome::Icon::Profile, size: 10 }
            }
        }
        if part.switch_count > 0 {
            // The font has no ⇄: the switches glyph and the count.
            span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 2px; font-size: 11px; \
                           font-family: monospace; color: {FAINT};",
                title: "Tunes {part.switch_count} switches",
                fts_chrome::Glyph { icon: fts_chrome::Icon::Perform, size: 10 }
                "{part.switch_count}"
            }
        }
        if !part.overrides.is_empty() {
            span { style: "flex-shrink: 0; font-size: 11px; font-family: monospace; color: {FAINT};",
                title: "Changes {part.overrides.len()} settings", "±{part.overrides.len()}"
            }
        }
        if !part.profile.is_empty() {
            span { style: "flex-shrink: 0; font-size: 11px; color: {FAINT};", title: "Played on {part.profile}", "{part.profile}" }
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
/// What the set's menu does (from ⋯ or a right-click on its name).
fn set_act(rig: &Option<RigClient>, index: u32, on_browse: EventHandler<Kind>, p: Picked) {
    let text = p.text;
    match p.id {
        "rename" => send(rig, move |r| async move { let _ = r.rename_setlist(index, text).await; }),
        "duplicate" => send(rig, move |r| async move { let _ = r.duplicate_setlist(index, text).await; }),
        "new" => send(rig, move |r| async move { let _ = r.add_setlist(text).await; }),
        "up" => send(rig, move |r| async move { let _ = r.move_setlist(index, index.saturating_sub(1)).await; }),
        "down" => send(rig, move |r| async move { let _ = r.move_setlist(index, index + 1).await; }),
        "delete" => send(rig, move |r| async move { let _ = r.delete_setlist(index).await; }),
        "library" => on_browse.call(Kind::Setlists),
        _ => {}
    }
}

/// A song row's menu: the song itself (its name, its key and tempo — the
/// library's, wherever it is played), then its key and tempo in this set
/// and its place in it.
fn song_items(row: &signal_guitar_proto::SongSlot, entry: Option<&signal_guitar_proto::SongEntry>, names: &[String], index: usize, count: usize) -> Vec<MenuItem> {
    let others: Vec<String> = names.iter().filter(|n| **n != row.name).cloned().collect();
    let (key, bpm) = entry.map_or((row.key.clone(), row.bpm), |e| (e.key.clone(), e.bpm));
    vec![
        MenuItem::head(format!("Song · {}", row.name)),
        MenuItem::name("song_rename", "Rename…", "Rename", &row.name, others),
        MenuItem::name("song_key", format!("Key ({key})…"), "Set", &key, Vec::new()),
        MenuItem::name("song_bpm", format!("Tempo ({bpm} BPM)…"), "Set", bpm.to_string(), Vec::new()),
        MenuItem::sep(),
        MenuItem::head("In this set"),
        MenuItem::name("set_key", format!("Key here ({})…", row.key), "Set", &row.key, Vec::new()),
        MenuItem::name("set_bpm", format!("Tempo here ({} BPM)…", row.bpm), "Set", row.bpm.to_string(), Vec::new()),
        MenuItem::run("up", "Move up").unless((index == 0).then(|| "First".to_string())),
        MenuItem::run("down", "Move down").unless((index + 1 >= count).then(|| "Last".to_string())),
        MenuItem::delete("remove", "Remove from this set", None),
    ]
}

/// What a song row's menu does.
fn song_act(rig: &Option<RigClient>, row: &signal_guitar_proto::SongSlot, entry: Option<&signal_guitar_proto::SongEntry>, index: usize, setlist: u32, p: Picked) {
    let name = row.name.clone();
    let (key, bpm) = entry.map_or((row.key.clone(), row.bpm), |e| (e.key.clone(), e.bpm));
    let (here_key, here_bpm) = (row.key.clone(), row.bpm);
    let text = p.text.trim().to_string();
    let i = index as u32;
    match p.id {
        "song_rename" if !text.is_empty() => send(rig, move |r| async move { let _ = r.edit_song(name, text, key, bpm).await; }),
        "song_key" => send(rig, move |r| async move { let _ = r.edit_song(name.clone(), name, text, bpm).await; }),
        "song_bpm" => {
            if let Ok(b) = text.parse::<u32>() {
                send(rig, move |r| async move { let _ = r.edit_song(name.clone(), name, key, b).await; });
            }
        }
        "set_key" => send(rig, move |r| async move { let _ = r.set_setlist_entry(i, text, here_bpm).await; }),
        "set_bpm" => {
            if let Ok(b) = text.parse::<u32>() {
                send(rig, move |r| async move { let _ = r.set_setlist_entry(i, here_key, b).await; });
            }
        }
        "up" if index > 0 => send(rig, move |r| async move { let _ = r.move_song(i, i - 1).await; }),
        "down" => send(rig, move |r| async move { let _ = r.move_song(i, i + 1).await; }),
        "remove" => send(rig, move |r| async move { let _ = r.remove_setlist_entry(setlist, i).await; }),
        _ => {}
    }
}

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
            span { style: "font-size: 11px; color: {FAINT};", "Played on" }
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
            span { style: "font-size: 11px; color: {FAINT};", "Starts on" }
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
            span { style: "font-size: 11px; color: {FAINT};", "In this set" }
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
    /// The part it repeats; empty = its own sound.
    #[props(default)]
    repeat_of: String,
    /// The song's other parts, for the Repeats picker.
    #[props(default)]
    others: Vec<String>,
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
            span { style: "font-size: 11px; color: {FAINT};", "Repeats — linked: the same sound, edited together" }
            Picker {
                options: std::iter::once("— its own sound —".to_string()).chain(others.iter().cloned()).collect::<Vec<_>>(),
                selected: others.iter().position(|p| p.eq_ignore_ascii_case(&repeat_of)).map_or(0, |p| p as u32 + 1),
                width: "100%".to_string(),
                on_select: {
                    let (rig, part, others) = (rig.clone(), name.clone(), others.clone());
                    move |i: u32| {
                        let of = if i == 0 { String::new() } else { others.get(i as usize - 1).cloned().unwrap_or_default() };
                        let part = part.clone();
                        send(&rig, move |r| async move { let _ = r.set_part_repeat(part, of).await; });
                    }
                },
            }
            span { style: "font-size: 11px; color: {FAINT};", "Profile" }
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
            span { style: "font-size: 11px; color: {FAINT};", "Recalls" }
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
            span { style: "font-size: 11px; color: {FAINT};", "Section — parts in a row with the same section are one" }
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
            span { style: "font-size: 11px; color: {FAINT};", "Name" }
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
