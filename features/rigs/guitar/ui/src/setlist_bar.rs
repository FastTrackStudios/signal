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

const LINE: &str = "#1f1f24";
const TEXT: &str = "#e4e4e7";
const MUTED: &str = "#a1a1aa";
const FAINT: &str = "#63636b";
const SONG_BG: &str = "#1b2331";
const SONG_FG: &str = "#bfdbfe";
const PART_BG: &str = "#26324a";
const LIVE: &str = "#22c55e";

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

    rsx! {
        aside {
            style: "width: 272px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: #0e0e11; color: {TEXT};",

            // ── Which set ──
            div { style: "display: flex; flex-direction: column; gap: 4px; padding: 12px 12px 10px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.14em; \
                               text-transform: uppercase; color: {FAINT};",
                    "Setlist"
                }
                button {
                    style: "display: flex; align-items: center; gap: 6px; width: 100%; padding: 0; \
                            border: none; background: transparent; color: {TEXT}; cursor: pointer; \
                            justify-content: flex-start; text-align: left;",
                    title: "Switch set — opens the library",
                    onclick: move |_| on_browse.call(Kind::Setlists),
                    span { style: "flex: 1 1 0; min-width: 0; font-size: 15px; font-weight: 700; \
                                   white-space: nowrap; overflow: hidden;",
                        "{set_name}"
                    }
                    span { style: "color: {FAINT}; display: flex;",
                        fts_chrome::Glyph { icon: fts_chrome::Icon::ChevronDown, size: 14 }
                    }
                }
                span { style: "font-size: 11px; color: {FAINT};",
                    if model.songs.len() == 1 { "1 song" } else { "{model.songs.len()} songs" }
                }
            }

            // ── Songs, and the parts of the one that is up ──
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 8px;",
                if model.songs.is_empty() {
                    div { style: "padding: 10px 6px; font-size: 12px; color: {FAINT}; line-height: 1.5;",
                        "This set has no songs yet."
                    }
                }
                for (i, song) in model.songs.iter().enumerate() {
                    {
                        let is_current = i == current;
                        let rig_play = rig.clone();
                        let song = song.clone();
                        rsx! {
                            div { key: "{i}-{song.name}", style: "display: flex; flex-direction: column; margin-bottom: 2px;",
                                // The song.
                                div {
                                    style: format!(
                                        "display: flex; align-items: center; gap: 8px; padding: 8px 8px; \
                                         border-radius: 8px; background: {};",
                                        if is_current { SONG_BG } else { "transparent" },
                                    ),
                                    button {
                                        style: format!(
                                            "flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: 8px; \
                                             padding: 0; border: none; background: transparent; cursor: pointer; \
                                             justify-content: flex-start; text-align: left; color: {};",
                                            if is_current { SONG_FG } else { MUTED },
                                        ),
                                        onclick: move |_| {
                                            send(&rig_play, move |r| async move { let _ = r.select_song(i as u32).await; });
                                        },
                                        span { style: "width: 16px; flex-shrink: 0; font-size: 10px; font-family: monospace; color: {FAINT};",
                                            "{i + 1}"
                                        }
                                        span {
                                            style: format!(
                                                "flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden; \
                                                 font-size: 13px; font-weight: {};",
                                                if is_current { 700 } else { 500 },
                                            ),
                                            "{song.name}"
                                        }
                                        span { style: "flex-shrink: 0; font-size: 10px; font-family: monospace; color: {FAINT};",
                                            "{song.key} · {song.bpm}"
                                        }
                                    }
                                    if is_current {
                                        button {
                                            style: format!(
                                                "display: flex; align-items: center; justify-content: center; width: 22px; \
                                                 height: 22px; flex-shrink: 0; border-radius: 6px; border: none; \
                                                 cursor: pointer; background: {}; color: {};",
                                                if editing_song() { PART_BG } else { "transparent" },
                                                if editing_song() { SONG_FG } else { FAINT },
                                            ),
                                            title: "Key, tempo and place in this set",
                                            onclick: move |_| {
                                                editing_song.toggle();
                                                editing_part.set(None);
                                            },
                                            fts_chrome::Glyph { icon: fts_chrome::Icon::Pencil, size: 12 }
                                        }
                                    }
                                }
                                // What the song is played on, under the song that is up.
                                if is_current {
                                    span { style: "margin: 2px 0 0 32px; font-size: 10px; color: {FAINT};",
                                        if model.song_profile.is_empty() {
                                            "on {loaded} (whatever is loaded)"
                                        } else {
                                            "on {model.song_profile}"
                                        }
                                    }
                                }
                                if is_current && editing_song() {
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
                                // Its map — sections in order, only under the song that is up.
                                if is_current {
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
                                        let next_name = sections.get(cur_sec + 1).map(|(n, _)| n.clone());
                                        rsx! {
                                            div { style: "display: flex; flex-direction: column; gap: 4px; margin: 6px 0 8px 8px;",
                                                if count > 0 {
                                                    // Where we are in the song, at a glance.
                                                    div { style: "display: flex; flex-direction: column; gap: 4px; padding: 2px 4px 6px;",
                                                        div { style: "display: flex; gap: 2px;",
                                                            for k in 0..count {
                                                                span {
                                                                    key: "{k}",
                                                                    style: format!(
                                                                        "flex: 1; height: 4px; border-radius: 2px; background: {};",
                                                                        if k < cur_sec { "#2f4a2a" } else if k == cur_sec { LIVE } else { "#27272a" },
                                                                    ),
                                                                }
                                                            }
                                                        }
                                                        div { style: "display: flex; font-size: 10px; color: {FAINT};",
                                                            span { "Section {cur_sec + 1} of {count}" }
                                                            span { style: "margin-left: auto;",
                                                                match &next_name {
                                                                    Some(n) => format!("next: {n}"),
                                                                    None => "last section".to_string(),
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                for (si, (sec_name, idx)) in sections.iter().enumerate() {
                                                    {
                                                        let first = idx[0];
                                                        let single = idx.len() == 1
                                                            && model.parts[first].name.eq_ignore_ascii_case(sec_name);
                                                        let live = si == cur_sec;
                                                        let past = si < cur_sec;
                                                        let is_next = si == cur_sec + 1;
                                                        let starts_here = idx
                                                            .iter()
                                                            .any(|&pi| model.parts[pi].name.eq_ignore_ascii_case(&model.start_part));
                                                        let summary = if single { part_summary(&model.parts[first]) } else { String::new() };
                                                        let rig_sec = rig.clone();
                                                        let sec_name = sec_name.clone();
                                                        let idx = idx.clone();
                                                        rsx! {
                                                            div {
                                                                key: "sec-{si}-{sec_name}",
                                                                style: format!(
                                                                    "display: flex; flex-direction: column; border-radius: 8px; \
                                                                     border: 1px solid {}; border-left: 3px solid {}; background: {}; opacity: {};",
                                                                    if live { "#2c3b57" } else { LINE },
                                                                    if live { LIVE } else if is_next { "#6366f1" } else { LINE },
                                                                    if live { PART_BG } else { "transparent" },
                                                                    if past { "0.55" } else { "1" },
                                                                ),
                                                                // The section: a click plays it (its first part);
                                                                // on the one-part section that is up, edits it.
                                                                button {
                                                                    style: "display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 8px; \
                                                                            border: none; background: transparent; cursor: pointer; text-align: left; \
                                                                            justify-content: flex-start; color: {TEXT};",
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
                                                                    span {
                                                                        style: format!(
                                                                            "width: 18px; height: 18px; flex-shrink: 0; border-radius: 999px; display: flex; \
                                                                             align-items: center; justify-content: center; font-size: 10px; font-weight: 700; \
                                                                             font-family: monospace; background: {}; color: {};",
                                                                            if live { LIVE } else { "#1f1f24" },
                                                                            if live { "#052e16" } else { MUTED },
                                                                        ),
                                                                        if past { "✓" } else { "{si + 1}" }
                                                                    }
                                                                    div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                                                                        span { style: format!(
                                                                                "font-size: 13px; font-weight: {}; white-space: nowrap; overflow: hidden; color: {};",
                                                                                if live { 700 } else { 600 },
                                                                                if live { TEXT } else { MUTED },
                                                                            ),
                                                                            "{sec_name}"
                                                                        }
                                                                        if !summary.is_empty() {
                                                                            span { style: "font-size: 10px; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                                                                                "{summary}"
                                                                            }
                                                                        }
                                                                    }
                                                                    if starts_here {
                                                                        span { style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: {SONG_FG};",
                                                                            title: "The song starts here", "START" }
                                                                    }
                                                                    if is_next {
                                                                        span { style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.1em; \
                                                                                       padding: 2px 5px; border-radius: 4px; background: #1e1b4b; color: #c7d2fe;",
                                                                            title: "Switch 5 goes here", "NEXT" }
                                                                    }
                                                                }
                                                                // A section of several parts lists them.
                                                                if !single {
                                                                    div { style: "display: flex; flex-direction: column; gap: 1px; padding: 0 6px 6px 34px;",
                                                                        for &pi in idx.iter() {
                                                                            {
                                                                                let part = model.parts[pi].clone();
                                                                                let part_on = pi == at;
                                                                                let rig = rig.clone();
                                                                                rsx! {
                                                                                    button {
                                                                                        key: "{pi}-{part.name}",
                                                                                        style: format!(
                                                                                            "display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 6px; \
                                                                                             border-radius: 5px; border: none; cursor: pointer; text-align: left; \
                                                                                             justify-content: flex-start; background: {}; color: {};",
                                                                                            if part_on { "#334467" } else { "transparent" },
                                                                                            if part_on { TEXT } else { MUTED },
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
                                                                                        span { style: format!(
                                                                                            "width: 5px; height: 5px; border-radius: 999px; flex-shrink: 0; background: {};",
                                                                                            if part_on { LIVE } else { "#3f3f46" }) }
                                                                                        span { style: "flex: 1 1 0; min-width: 0; font-size: 11px; font-weight: 600; white-space: nowrap; overflow: hidden;",
                                                                                            "{part.name}"
                                                                                        }
                                                                                        span { style: "flex-shrink: 0; font-size: 10px; color: {FAINT}; white-space: nowrap;",
                                                                                            "{part_summary(&part)}"
                                                                                        }
                                                                                    }
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
                                                                            div { key: "ed-{pi}", style: "padding: 0 6px 6px;",
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
                                                if model.parts.is_empty() && !adding_part() {
                                                    span { style: "padding: 4px 8px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                                                        "No sections yet — name the moments of the song: Verse 1, Chorus, Bridge…"
                                                    }
                                                }
                                                if adding_part() {
                                                    NewPart { on_done: move |()| adding_part.set(false) }
                                                } else {
                                                    button {
                                                        style: "align-self: flex-start; padding: 4px 8px; border: none; background: transparent; \
                                                                cursor: pointer; font-size: 11px; font-weight: 600; color: {FAINT};",
                                                        onclick: move |_| {
                                                            adding_part.set(true);
                                                            editing_part.set(None);
                                                        },
                                                        "+ section"
                                                    }
                                                }
                                                if count > 0 {
                                                    span { style: "padding: 2px 8px; font-size: 10px; color: #4b4b52;",
                                                        "Switch 5: next section · hold: back"
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

#[component]
fn FootButton(label: String, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "flex: 1 1 0; min-width: 0; padding: 7px 8px; border-radius: 7px; cursor: pointer; \
                    font-size: 11px; font-weight: 600; border: 1px solid {LINE}; background: transparent; \
                    color: {MUTED};",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

/// A small icon button.
#[component]
fn Tool(
    icon: fts_chrome::Icon,
    title: String,
    #[props(default = false)] flip: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    let colour = if disabled {
        "#3a3a40"
    } else if danger {
        "#f87171"
    } else {
        MUTED
    };
    let rotate = if flip {
        "transform: rotate(180deg);"
    } else {
        ""
    };
    rsx! {
        button {
            style: "display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; \
                    flex-shrink: 0; border-radius: 6px; border: 1px solid {LINE}; background: transparent; \
                    cursor: pointer; color: {colour};",
            title: "{title}",
            disabled,
            onclick: move |_| {
                if !disabled {
                    onclick.call(());
                }
            },
            span { style: "display: flex; {rotate}", fts_chrome::Glyph { icon, size: 12 } }
        }
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
            style: "flex: {flex}; min-width: 0; font-size: 12px; color: {TEXT}; background: #0a0a0d; \
                    border: 1px solid #2a2a31; border-radius: 6px; padding: 5px 7px; outline: none;",
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

/// A part's one-line summary: what it plays and what it changes.
fn part_summary(part: &signal_guitar_proto::PerfPart) -> String {
    let mut bits: Vec<String> = Vec::new();
    if !part.profile.is_empty() {
        bits.push(part.profile.clone());
    }
    bits.push(if part.patch.is_empty() { "stays on the patch".to_string() } else { part.patch.clone() });
    if part.profile_switches {
        bits.push("profile switches".to_string());
    }
    if part.switch_count > 0 {
        bits.push(format!("⇄ {} switches", part.switch_count));
    }
    if !part.overrides.is_empty() {
        bits.push(format!("±{}", part.overrides.len()));
    }
    bits.join(" · ")
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
