//! **The setlist sidebar** — the left sidebar in Setlist mode.
//!
//! One tree, in the shape a set is played: the set, its songs in order, and
//! under the song that is up, its **song parts**. Nothing else is nested and
//! nothing lives in a second sidebar, so there is one place to look for
//! "where am I in the set" and it reads top to bottom:
//!
//! ```text
//! SETLIST
//! CYA 9-24-26 ▾                  ← opens the library on setlists
//!  1  AMAZING!        B · 145    ← the song that is up
//!     │ Intro      → Clean
//!     │ Rhythm     → Clean · ±2   ← the part that is up
//!     │ + part
//!  2  WASHED          B · 139
//!  3  TAKEOVER        C · 66
//! + Add song                     ← opens the library on songs
//! ```
//!
//! A part is not a verse number: it is whatever the player calls a moment of
//! the song — "Verse 1", or "Rhythm", or "Clean lead" — and it **overlays
//! the profile**: it picks another patch from a stack and/or changes a few
//! things on top. The footswitches stay the profile's.
//!
//! A click on a song plays it; a click on a part recalls it. Editing is one
//! step further in, so the tree stays calm while playing: a click on the
//! part that is *already* up opens its editor (what it recalls, rename,
//! move, remove), and the pencil on the current song edits its key and tempo
//! for this set and its place in it.
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
                                // Its parts — only under the song that is up.
                                if is_current {
                                    div { style: "display: flex; flex-direction: column; gap: 2px; margin: 4px 0 6px 20px; \
                                                  padding-left: 8px; border-left: 1px solid {LINE};",
                                        for (pi, part) in model.parts.iter().enumerate() {
                                            {
                                                let part_on = pi == model.part_index as usize;
                                                let open = editing_part() == Some(pi);
                                                let rig = rig.clone();
                                                let part = part.clone();
                                                // A section of several parts (or one
                                                // named apart from its part) gets a
                                                // heading where it starts.
                                                let grouped = !part.section.eq_ignore_ascii_case(&part.name);
                                                let starts = pi == 0
                                                    || !model.parts[pi - 1].section.eq_ignore_ascii_case(&part.section);
                                                rsx! {
                                                    if grouped && starts {
                                                        span {
                                                            key: "sec-{pi}",
                                                            style: "margin-top: 4px; padding: 2px 8px; font-size: 9px; font-weight: 700; \
                                                                    letter-spacing: 0.12em; text-transform: uppercase; color: {FAINT};",
                                                            "{part.section}"
                                                        }
                                                    }
                                                    button {
                                                        key: "{pi}-{part.name}",
                                                        style: format!(
                                                            "display: flex; align-items: center; gap: 6px; width: 100%; \
                                                             padding: 6px 8px; border-radius: 6px; border: none; cursor: pointer; \
                                                             justify-content: flex-start; text-align: left; background: {}; color: {};",
                                                            if part_on { PART_BG } else { "transparent" },
                                                            if part_on { TEXT } else { MUTED },
                                                        ),
                                                        title: if part_on { "Click again to edit this part" } else { "Recall this part" },
                                                        // First click recalls it; a click on the
                                                        // part already up opens its editor.
                                                        onclick: move |_| {
                                                            if part_on {
                                                                editing_part.set(if open { None } else { Some(pi) });
                                                                editing_song.set(false);
                                                            } else {
                                                                editing_part.set(None);
                                                                send(&rig, move |r| async move {
                                                                    let _ = r.select_part(pi as u32).await;
                                                                });
                                                            }
                                                        },
                                                        span {
                                                            style: format!(
                                                                "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {};",
                                                                if part_on { LIVE } else { "transparent" },
                                                            ),
                                                        }
                                                        span { style: "flex: 1 1 0; min-width: 0; font-size: 12px; font-weight: 600; \
                                                                       white-space: nowrap; overflow: hidden;",
                                                            "{part.name}"
                                                        }
                                                        if part.name.eq_ignore_ascii_case(&model.start_part) {
                                                            span {
                                                                style: "flex-shrink: 0; font-size: 8px; font-weight: 700; letter-spacing: 0.1em; \
                                                                        text-transform: uppercase; color: {SONG_FG};",
                                                                title: "The song starts here",
                                                                "start"
                                                            }
                                                        }
                                                        span { style: "flex-shrink: 0; font-size: 10px; color: {FAINT}; white-space: nowrap;",
                                                            if !part.profile.is_empty() { "{part.profile} · " }
                                                            if part.patch.is_empty() { "" } else { "{part.patch}" }
                                                            if !part.overrides.is_empty() { " · ±{part.overrides.len()}" }
                                                            if part.profile_switches { " · profile switches" }
                                                            if part.switch_count > 0 { " · ⇄{part.switch_count}" }
                                                        }
                                                    }
                                                    if open {
                                                        {
                                                            // The recall list is the patches of the
                                                            // profile the part is played on.
                                                            let base = if !part.profile.is_empty() {
                                                                part.profile.clone()
                                                            } else if !model.song_profile.is_empty() {
                                                                model.song_profile.clone()
                                                            } else {
                                                                loaded.clone()
                                                            };
                                                            let (names, labels) = patches_of(&base);
                                                            rsx! {
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
                                        if model.parts.is_empty() && !adding_part() {
                                            span { style: "padding: 4px 8px; font-size: 11px; color: {FAINT}; line-height: 1.5;",
                                                "No parts yet — name the moments of the song: Verse, Rhythm, Clean lead…"
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
                                                "+ part"
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
