//! **The library picker** — everything the rig plays from, in one place:
//! setlists, songs, profiles, and the active profile's patches, presets and
//! drive pedals.
//!
//! A modal over 90% of the window rather than a sidebar, because it is where
//! you *manage* the library as well as pick from it, and managing wants room:
//! a kind rail on the left, the searchable list in the middle, and the focused
//! item's detail — what it holds, what uses it, what you can do to it — on the
//! right.
//!
//! Picking is keyboard-first, like a command palette: type to filter, arrows
//! to move, Enter to load and close. The detail's own Load button loads and
//! stays open, which is how you audition presets one after another.
//!
//! The search runs over every kind at once — the rail's counts are the
//! matches in each — so a song you typed the name of while on Profiles is one
//! click away rather than "nothing found".
//!
//! Blitz: no `position: fixed`, so the overlay is an absolute box over the
//! rig root (which is `position: relative`); lists scroll with
//! `overflow-y: scroll`; no `<select>`, so choosing among a few things is a
//! row of chips rather than a dropdown that a scrolling pane would clip.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{
    CompositionModel, DriveEntry, LibraryModel, PatchInfo, PerformanceModel, PresetInfo,
    ProfileEntry, SetlistEntry, SongEntry,
};

/// The picker's open state, in context — so a surface deep in the rig (the
/// board's module rows) can open it on a kind without threading a prop.
#[derive(Clone, Copy)]
pub struct OpenLibrary(pub Signal<Option<Kind>>);

/// What the picker is browsing — the tag on the search. [`Kind::All`] is no
/// tag: the search runs over everything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    All,
    Setlists,
    Songs,
    Profiles,
    Patches,
    /// The amp captures in the profile's pool (each a NAM + its cab).
    Presets,
    Drives,
    /// Presets proper: compositions of module presets, with snapshots.
    Compositions,
    /// Module presets, per module — what a row's ▾ on the board opens.
    AmpModules,
    DriveModules,
    DelayModules,
    ReverbModules,
}

impl Kind {
    const ALL: [Self; 11] = [
        Self::Setlists,
        Self::Songs,
        Self::Profiles,
        Self::Patches,
        Self::Compositions,
        Self::AmpModules,
        Self::DriveModules,
        Self::DelayModules,
        Self::ReverbModules,
        Self::Presets,
        Self::Drives,
    ];

    /// The rail: no tag, then every kind.
    const RAIL: [Self; 12] = [
        Self::All,
        Self::Setlists,
        Self::Songs,
        Self::Profiles,
        Self::Patches,
        Self::Compositions,
        Self::AmpModules,
        Self::DriveModules,
        Self::DelayModules,
        Self::ReverbModules,
        Self::Presets,
        Self::Drives,
    ];

    /// The module a module-preset kind browses, as the rig names it.
    #[must_use]
    pub const fn module(self) -> Option<&'static str> {
        match self {
            Self::AmpModules => Some("Amp"),
            Self::DriveModules => Some("Drive"),
            Self::DelayModules => Some("Delay"),
            Self::ReverbModules => Some("Reverb"),
            _ => None,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Setlists => "Setlists",
            Self::Songs => "Songs",
            Self::Profiles => "Profiles",
            Self::Patches => "Patches",
            Self::Presets => "Captures",
            Self::Drives => "Drives",
            Self::Compositions => "Presets",
            Self::AmpModules => "Amp",
            Self::DriveModules => "Drive",
            Self::DelayModules => "Delay",
            Self::ReverbModules => "Reverb",
        }
    }

    /// The singular, for "New song".
    pub(crate) const fn one(self) -> &'static str {
        match self {
            Self::All => "item",
            Self::Setlists => "setlist",
            Self::Songs => "song",
            Self::Profiles => "profile",
            Self::Patches => "patch",
            Self::Presets => "capture",
            Self::Drives => "drive",
            Self::Compositions => "preset",
            Self::AmpModules => "amp preset",
            Self::DriveModules => "drive preset",
            Self::DelayModules => "delay preset",
            Self::ReverbModules => "reverb preset",
        }
    }

    pub(crate) const fn icon(self) -> fts_chrome::Icon {
        match self {
            Self::All => fts_chrome::Icon::Browser,
            Self::Setlists => fts_chrome::Icon::Setlist,
            Self::Songs => fts_chrome::Icon::Note,
            Self::Profiles => fts_chrome::Icon::Profile,
            Self::Patches => fts_chrome::Icon::Perform,
            Self::Presets => fts_chrome::Icon::Star,
            Self::Drives => fts_chrome::Icon::Tones,
            Self::Compositions => fts_chrome::Icon::Preset,
            Self::AmpModules => fts_chrome::Icon::Guitar,
            Self::DriveModules => fts_chrome::Icon::Power,
            Self::DelayModules => fts_chrome::Icon::Refresh,
            Self::ReverbModules => fts_chrome::Icon::Refresh,
        }
    }

    /// Whether the kind belongs to the active profile rather than the library
    /// at large — the rail says so, since switching profile changes them.
    const fn of_profile(self) -> bool {
        matches!(self, Self::Patches | Self::Presets | Self::Drives)
    }

    /// Where the picker opens for a perform mode (0 Preset / 1 Profile /
    /// 2 Setlist): the thing that mode plays from.
    #[must_use]
    pub const fn for_perform_mode(mode: u32) -> Self {
        match mode {
            0 => Self::Compositions,
            2 => Self::Setlists,
            _ => Self::Profiles,
        }
    }
}

/// One row of the list — the same shape for every kind.
#[derive(Clone, PartialEq)]
struct Row {
    /// What it is — a row of the untagged list can be any kind.
    kind: Kind,
    name: String,
    /// Position in the source list, for the index-addressed rig calls.
    idx: usize,
    sub: String,
    /// Playing now: the active setlist, profile, patch, amp.
    active: bool,
}

fn rows(
    kind: Kind,
    lib: &LibraryModel,
    patches: &[PatchInfo],
    presets: &[PresetInfo],
    comp: &CompositionModel,
    model: &PerformanceModel,
) -> Vec<Row> {
    match kind {
        Kind::All => Kind::ALL
            .iter()
            .flat_map(|&k| rows(k, lib, patches, presets, comp, model))
            .collect(),
        Kind::Compositions => comp
            .presets
            .iter()
            .enumerate()
            .map(|(idx, p)| Row {
                kind,
                name: p.name.clone(),
                idx,
                sub: p
                    .snapshots
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
                active: comp.active_preset.eq_ignore_ascii_case(&p.name),
            })
            .collect(),
        Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => {
            let module = kind.module().unwrap_or_default();
            comp.modules
                .iter()
                .enumerate()
                .filter(|(_, m)| m.module.eq_ignore_ascii_case(module))
                .map(|(idx, m)| Row {
                    kind,
                    name: m.name.clone(),
                    idx,
                    sub: m.snapshots.join(" · "),
                    active: comp.active_modules.iter().any(|a| {
                        a.module.eq_ignore_ascii_case(module)
                            && a.preset.eq_ignore_ascii_case(&m.name)
                    }),
                })
                .collect()
        }
        Kind::Setlists => lib
            .setlists
            .iter()
            .enumerate()
            .map(|(idx, s)| Row {
                kind,
                name: s.name.clone(),
                idx,
                sub: count(s.songs.len(), "song"),
                active: s.active,
            })
            .collect(),
        Kind::Songs => lib
            .songs
            .iter()
            .enumerate()
            .map(|(idx, s)| Row {
                kind,
                name: s.name.clone(),
                idx,
                sub: if s.parts.is_empty() {
                    format!("{} · {} bpm", s.key, s.bpm)
                } else {
                    format!(
                        "{} · {} bpm · {}",
                        s.key,
                        s.bpm,
                        count(s.parts.len(), "part")
                    )
                },
                active: model
                    .songs
                    .get(model.song_index as usize)
                    .is_some_and(|cur| cur.name.eq_ignore_ascii_case(&s.name)),
            })
            .collect(),
        Kind::Profiles => lib
            .profiles
            .iter()
            .enumerate()
            .map(|(idx, p)| Row {
                kind,
                name: p.name.clone(),
                idx,
                sub: format!(
                    "{} · {}",
                    count(p.stacks.len(), "stack"),
                    count(p.patches as usize, "patch")
                ),
                active: p.active,
            })
            .collect(),
        Kind::Patches => patches
            .iter()
            .enumerate()
            .map(|(idx, p)| Row {
                kind,
                name: p.name.clone(),
                idx,
                sub: if p.stack.is_empty() {
                    p.preset.clone()
                } else {
                    format!("{} · {}", p.stack, p.preset)
                },
                active: p.active,
            })
            .collect(),
        Kind::Presets => presets
            .iter()
            .enumerate()
            .map(|(idx, p)| Row {
                kind,
                name: p.name.clone(),
                idx,
                sub: {
                    let used = count(p.used_by as usize, "patch");
                    if p.creator.is_empty() {
                        used
                    } else {
                        format!("{used} · {}", p.creator)
                    }
                },
                active: p.active,
            })
            .collect(),
        Kind::Drives => lib
            .drives
            .iter()
            .enumerate()
            .map(|(idx, d)| Row {
                kind,
                name: d.name.clone(),
                idx,
                sub: if d.slots.is_empty() {
                    count(d.options.len(), "capture")
                } else {
                    format!(
                        "{} · {}",
                        count(d.options.len(), "capture"),
                        d.slots.join(", ")
                    )
                },
                active: !d.slots.is_empty(),
            })
            .collect(),
    }
}

/// "3 songs", "1 song".
fn count(n: usize, what: &str) -> String {
    match (n, what) {
        (1, _) => format!("1 {what}"),
        (_, "patch") => format!("{n} patches"),
        _ => format!("{n} {what}s"),
    }
}

/// Every word of the query appears in the name or the subtitle.
fn matches(row: &Row, query: &str) -> bool {
    let hay = format!("{} {}", row.name, row.sub).to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

/// Fire a rig call without waiting on it — the result arrives as the next
/// `Perf` event, which re-fetches everything the picker shows.
fn send<F, Fut>(rig: &Option<RigClient>, call: F)
where
    F: FnOnce(RigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { call(r).await });
    }
}

/// Load what a row names: play the set, the song, the profile, the patch,
/// the preset. Returns whether there was anything to load (a drive, or a song
/// outside the active set, is only browsed).
fn activate(rig: &Option<RigClient>, row: &Row, model: &PerformanceModel) -> bool {
    let idx = row.idx as u32;
    let name = row.name.clone();
    match row.kind {
        Kind::Setlists => send(rig, move |r| async move {
            let _ = r.select_setlist(idx).await;
        }),
        Kind::Songs => {
            let Some(pos) = model
                .songs
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(&name))
            else {
                return false;
            };
            send(rig, move |r| async move {
                let _ = r.select_song(pos as u32).await;
            });
        }
        Kind::Profiles => send(rig, move |r| async move {
            let _ = r.select_profile(name).await;
        }),
        Kind::Patches => send(rig, move |r| async move {
            let _ = r.select_patch(idx).await;
        }),
        Kind::Presets => send(rig, move |r| async move {
            let _ = r.play_preset(idx).await;
        }),
        Kind::Compositions => send(rig, move |r| async move {
            let _ = r.choose_preset(name, String::new()).await;
        }),
        Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => {
            let module = row.kind.module().unwrap_or_default().to_string();
            send(rig, move |r| async move {
                let _ = r.choose_module(module, name, String::new()).await;
            });
        }
        Kind::Drives | Kind::All => return false,
    }
    true
}

// ── Palette ────────────────────────────────────────────────────────────────
// Inline, because the picker must lay out without Tailwind (CLAUDE.md), and
// the rig's own greys rather than the theme's so it reads as the same app.

const BG: &str = "#0c0c0f";
const PANE: &str = "#101014";
const LINE: &str = "#222228";
const TEXT: &str = "#e4e4e7";
const MUTED: &str = "#a1a1aa";
const FAINT: &str = "#63636b";
const FOCUS_BG: &str = "#1b2331";
const FOCUS_FG: &str = "#bfdbfe";
const LIVE: &str = "#22c55e";
const DANGER: &str = "#f87171";

/// The picker. Renders nothing while `open` is `None`; `Some(kind)` shows it
/// on that kind. Mount it inside the rig's root, which must be
/// `position: relative` — the overlay covers that box, not the window.
#[component]
pub fn LibraryPicker(model: PerformanceModel, open: Signal<Option<Kind>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);
    // The focused row, by kind and name: names are stable across the
    // re-fetch that every edit triggers, indices are not, and the untagged
    // list can hold a patch and a preset of the same name.
    let mut focus = use_signal(|| None::<(Kind, String)>);
    // The search field, so a click elsewhere in the picker can hand the
    // keyboard straight back to it.
    let mut search = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let refocus = move || {
        if let Some(el) = search() {
            spawn(async move {
                let _ = el.set_focus(true).await;
            });
        }
    };
    let mut creating = use_signal(|| false);

    // Everything the picker lists, re-read whenever the rig's state moves.
    // The revision goes through a signal: a resource re-runs when a signal
    // it read changes, and a plain captured number would pin it to the
    // first fetch — a profile switch would leave the old profile's patches.
    let mut rev = use_signal(|| model.revision);
    if *rev.peek() != model.revision {
        rev.set(model.revision);
    }
    let data = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => (
                        r.library().await.unwrap_or_default(),
                        r.patches().await.unwrap_or_default(),
                        r.presets().await.unwrap_or_default(),
                        r.compositions().await.unwrap_or_default(),
                    ),
                    None => Default::default(),
                }
            }
        }
    });

    let Some(kind) = open() else {
        return rsx! {};
    };
    let (lib, patches, presets, comp): (
        LibraryModel,
        Vec<PatchInfo>,
        Vec<PresetInfo>,
        CompositionModel,
    ) = data.read().clone().unwrap_or_default();

    let q = query();
    let per_kind: Vec<(Kind, Vec<Row>)> = Kind::RAIL
        .iter()
        .map(|&k| {
            let all = rows(k, &lib, &patches, &presets, &comp, &model);
            (
                k,
                all.into_iter()
                    .filter(|r| q.trim().is_empty() || matches(r, &q))
                    .collect(),
            )
        })
        .collect();
    let list: Vec<Row> = per_kind
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, r)| r.clone())
        .unwrap_or_default();
    // The focused row, else the active one, else the first — so the detail
    // pane always shows something when the list does.
    let is = |r: &Row, f: &(Kind, String)| r.kind == f.0 && r.name == f.1;
    let focused: Option<Row> = focus()
        .and_then(|f| list.iter().find(|r| is(r, &f)).cloned())
        .or_else(|| list.iter().find(|r| r.active).cloned())
        .or_else(|| list.first().cloned());
    let cursor = focused.as_ref().and_then(|f| {
        list.iter()
            .position(|r| r.kind == f.kind && r.name == f.name)
    });

    let mut close = move || {
        open.set(None);
        query.set(String::new());
        focus.set(None);
        creating.set(false);
    };
    let mut go = move |(k, name): (Kind, String)| {
        open.set(Some(k));
        query.set(String::new());
        creating.set(false);
        focus.set(Some((k, name)));
    };

    let context = format!(
        "{} · {}",
        if model.profile_name.is_empty() {
            "No profile"
        } else {
            &model.profile_name
        },
        model
            .setlists
            .get(model.setlist_index as usize)
            .map_or("no set", String::as_str),
    );

    rsx! {
        // The overlay: covers the rig; a click on it (not the panel) closes.
        div {
            style: "position: absolute; inset: 0; z-index: 90; background: rgba(0, 0, 0, 0.55);",
            onclick: move |_| close(),
            div {
                style: "position: absolute; left: 5%; top: 5%; width: 90%; height: 90%; \
                        display: flex; flex-direction: column; min-width: 0; min-height: 0; \
                        overflow: hidden; border-radius: 14px; background: {BG}; \
                        border: 1px solid {LINE}; box-shadow: 0 24px 80px rgba(0, 0, 0, 0.6); \
                        color: {TEXT};",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                onkeydown: {
                    let rig = rig.clone();
                    let list = list.clone();
                    let model = model.clone();
                    move |e: KeyboardEvent| match e.key() {
                        Key::Escape => {
                            e.prevent_default();
                            if creating() { creating.set(false) } else { close() }
                        }
                        Key::ArrowDown | Key::ArrowUp if !list.is_empty() => {
                            e.prevent_default();
                            let at = cursor.unwrap_or(0);
                            let next = if e.key() == Key::ArrowDown {
                                (at + 1).min(list.len() - 1)
                            } else {
                                at.saturating_sub(1)
                            };
                            focus.set(Some((list[next].kind, list[next].name.clone())));
                        }
                        Key::Enter => {
                            if let Some(row) = cursor.and_then(|c| list.get(c)) {
                                e.prevent_default();
                                if activate(&rig, row, &model) {
                                    close();
                                }
                            }
                        }
                        _ => {}
                    }
                },

                // ── Header: title, search, where the rig is, close ──
                div {
                    style: "display: flex; align-items: center; gap: 14px; padding: 0 16px; \
                            height: 56px; flex-shrink: 0; border-bottom: 1px solid {LINE};",
                    div { style: "display: flex; align-items: center; gap: 8px; color: {MUTED}; flex-shrink: 0;",
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Browser, size: 16 }
                        span { style: "font-size: 14px; font-weight: 700; color: {TEXT};", "Library" }
                    }
                    div {
                        style: "flex: 1 1 0; min-width: 0; max-width: 560px; display: flex; \
                                align-items: center; gap: 8px; padding: 0 10px; height: 34px; \
                                border-radius: 9px; background: {PANE}; border: 1px solid {LINE};",
                        SearchGlyph {}
                        // The tag the rail set, as a chip in the field.
                        if kind != Kind::All {
                            button {
                                style: "display: flex; align-items: center; gap: 5px; flex-shrink: 0; \
                                        padding: 2px 6px 2px 8px; border-radius: 6px; border: none; \
                                        cursor: pointer; background: {FOCUS_BG}; color: {FOCUS_FG}; \
                                        font-size: 11px; font-weight: 600;",
                                title: "Search everything (Backspace in an empty field)",
                                onmousedown: move |e: MouseEvent| e.prevent_default(),
                                onclick: move |_| {
                                    open.set(Some(Kind::All));
                                    focus.set(None);
                                    creating.set(false);
                                    refocus();
                                },
                                fts_chrome::Glyph { icon: kind.icon(), size: 11 }
                                "{kind.label()}"
                                fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 10 }
                            }
                        }
                        input {
                            style: "flex: 1; min-width: 0; background: transparent; border: none; \
                                    outline: none; color: {TEXT}; font-size: 13px;",
                            placeholder: if kind == Kind::All {
                                "Search setlists, songs, profiles, patches, presets…".to_string()
                            } else {
                                format!("Search {}…", kind.label().to_lowercase())
                            },
                            value: "{q}",
                            // `autofocus` is ignored on a node inserted after
                            // load; take focus when mounted instead.
                            onmounted: move |e| {
                                let el = e.data();
                                search.set(Some(el.clone()));
                                spawn(async move {
                                    let _ = el.set_focus(true).await;
                                });
                            },
                            // Backspace past the start of the text takes the
                            // tag off, as in a token field.
                            onkeydown: move |e: KeyboardEvent| {
                                if e.key() == Key::Backspace
                                    && query.peek().is_empty()
                                    && kind != Kind::All
                                {
                                    open.set(Some(Kind::All));
                                    focus.set(None);
                                }
                            },
                            oninput: move |e| {
                                query.set(e.value());
                                focus.set(None);
                            },
                        }
                    }
                    span {
                        style: "flex-shrink: 1; min-width: 0; overflow: hidden; white-space: nowrap; \
                                font-size: 11px; color: {FAINT};",
                        "Playing: {context}"
                    }
                    div { style: "flex: 1 1 0;" }
                    button {
                        style: "display: flex; align-items: center; justify-content: center; \
                                width: 30px; height: 30px; flex-shrink: 0; border-radius: 8px; \
                                border: 1px solid {LINE}; background: transparent; color: {MUTED}; \
                                cursor: pointer;",
                        title: "Close (Esc)",
                        onclick: move |_| close(),
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 14 }
                    }
                }

                // ── Body: kinds │ list │ detail ──
                div { style: "flex: 1 1 0; min-height: 0; display: flex; min-width: 0;",
                    // Kinds, with how many match the search.
                    div {
                        style: "width: 190px; flex-shrink: 0; display: flex; flex-direction: column; \
                                gap: 2px; padding: 10px 8px; border-right: 1px solid {LINE}; \
                                background: {PANE};",
                        for (k, hits) in per_kind.iter().map(|(k, r)| (*k, r.len())) {
                            div { key: "{k.label()}", style: "display: contents;",
                            if k == Kind::Patches {
                                div {
                                    style: "margin: 10px 8px 4px; font-size: 9px; font-weight: 700; \
                                            letter-spacing: 0.12em; text-transform: uppercase; \
                                            color: {FAINT}; white-space: nowrap; overflow: hidden;",
                                    "In {model.profile_name}"
                                }
                            }
                            button {
                                style: format!(
                                    "display: flex; align-items: center; gap: 9px; width: 100%; \
                                     padding: 8px 10px; border-radius: 8px; border: none; cursor: pointer; \
                                     justify-content: flex-start; text-align: left; \
                                     background: {}; color: {};",
                                    if k == kind { FOCUS_BG } else { "transparent" },
                                    if k == kind { FOCUS_FG } else if hits == 0 { FAINT } else { MUTED },
                                ),
                                // A tag on the search, not a place to go: the
                                // field keeps the keyboard, so typing carries on.
                                // Pointer-down moves focus to the nearest focusable element
                                // (this button); cancelling it keeps the search field's.
                                onmousedown: move |e: MouseEvent| e.prevent_default(),
                                onclick: move |_| {
                                    open.set(Some(if k == kind { Kind::All } else { k }));
                                    focus.set(None);
                                    creating.set(false);
                                    refocus();
                                },
                                fts_chrome::Glyph { icon: k.icon(), size: 14 }
                                span { style: "flex: 1; min-width: 0; font-size: 12px; font-weight: 600;", "{k.label()}" }
                                span { style: "font-size: 10px; color: {FAINT};", "{hits}" }
                            }
                            }
                        }
                    }

                    // The list.
                    div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column;",
                        div {
                            style: "display: flex; align-items: center; gap: 10px; padding: 12px 16px 8px; \
                                    flex-shrink: 0;",
                            span { style: "font-size: 16px; font-weight: 700;", "{kind.label()}" }
                            if kind.of_profile() {
                                span { style: "font-size: 11px; color: {FAINT};", "of {model.profile_name}" }
                            }
                            div { style: "flex: 1;" }
                            if !matches!(kind, Kind::Drives | Kind::All | Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules) {
                                button {
                                    style: format!(
                                        "padding: 5px 11px; border-radius: 7px; cursor: pointer; font-size: 11px; \
                                         font-weight: 600; border: 1px solid {LINE}; background: {}; color: {};",
                                        if creating() { FOCUS_BG } else { "transparent" },
                                        if creating() { FOCUS_FG } else { MUTED },
                                    ),
                                    onclick: move |_| creating.toggle(),
                                    "+ New {kind.one()}"
                                }
                            }
                        }
                        if creating() {
                            NewForm {
                                kind,
                                lib: lib.clone(),
                                patches: patches.clone(),
                                presets: presets.clone(),
                                model: model.clone(),
                                on_done: move |name: Option<String>| {
                                    creating.set(false);
                                    if let Some(n) = name {
                                        focus.set(Some((kind, n)));
                                        query.set(String::new());
                                    }
                                },
                            }
                        }
                        div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 2px 10px 12px;",
                            if list.is_empty() {
                                div { style: "padding: 24px 8px; font-size: 12px; color: {FAINT}; line-height: 1.6;",
                                    if q.trim().is_empty() {
                                        "Nothing here yet."
                                    } else {
                                        if kind == Kind::All {
                                            "Nothing matches \"{q}\"."
                                        } else {
                                            "No {kind.label().to_lowercase()} match \"{q}\" — the counts on the left show where it is."
                                        }
                                    }
                                }
                            }
                            for row in list.iter().cloned() {
                                {
                                    let is_focus = focused
                                        .as_ref()
                                        .is_some_and(|f| f.kind == row.kind && f.name == row.name);
                                    let rig = rig.clone();
                                    let model = model.clone();
                                    let target = (row.kind, row.name.clone());
                                    rsx! {
                                        button {
                                            key: "{row.kind.label()}-{row.name}",
                                            style: format!(
                                                "display: flex; align-items: center; gap: 10px; width: 100%; \
                                                 padding: 8px 10px; margin-bottom: 1px; border-radius: 8px; \
                                                 border: none; cursor: pointer; text-align: left; \
                                                 justify-content: flex-start; background: {}; color: {};",
                                                if is_focus { FOCUS_BG } else { "transparent" },
                                                if is_focus { FOCUS_FG } else { TEXT },
                                            ),
                                            // No mousedown-preventDefault here (unlike the rail
                                            // and the tag chip): cancelling pointerdown's default
                                            // action on this button also skips the click Blitz's
                                            // pointerup default action queues from it, so the row
                                            // never registered a click at all. `refocus()` below
                                            // puts the keyboard back in the search field instead.
                                            onclick: move |_| {
                                                focus.set(Some(target.clone()));
                                                refocus();
                                            },
                                            ondoubleclick: {
                                                let row = row.clone();
                                                move |_| {
                                                    if activate(&rig, &row, &model) {
                                                        close();
                                                    }
                                                }
                                            },
                                            span {
                                                style: format!(
                                                    "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; background: {};",
                                                    if row.active { LIVE } else { "transparent" },
                                                ),
                                            }
                                            span { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                                span { style: "font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden;",
                                                    "{row.name}"
                                                }
                                                span { style: "font-size: 11px; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                                                    "{row.sub}"
                                                }
                                            }
                                            if kind == Kind::All {
                                                span {
                                                    style: "display: flex; align-items: center; gap: 4px; flex-shrink: 0; \
                                                            font-size: 10px; color: {FAINT};",
                                                    fts_chrome::Glyph { icon: row.kind.icon(), size: 11 }
                                                    "{row.kind.one()}"
                                                }
                                            }
                                            if row.active {
                                                span {
                                                    style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; \
                                                            text-transform: uppercase; color: {LIVE}; flex-shrink: 0;",
                                                    "Playing"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // The detail.
                    div {
                        style: "width: 400px; flex-shrink: 0; min-height: 0; display: flex; \
                                flex-direction: column; border-left: 1px solid {LINE}; background: {PANE};",
                        match focused.clone() {
                            None => rsx! {
                                div { style: "padding: 24px; font-size: 12px; color: {FAINT};", "Nothing selected." }
                            },
                            Some(row) => rsx! {
                                Detail {
                                    key: "{row.kind.label()}-{row.name}",
                                    kind: row.kind,
                                    row,
                                    lib: lib.clone(),
                                    patches: patches.clone(),
                                    presets: presets.clone(),
                                    comp: comp.clone(),
                                    model: model.clone(),
                                    on_go: move |target: (Kind, String)| go(target),
                                }
                            },
                        }
                    }
                }

                // ── Footer: the keys ──
                div {
                    style: "display: flex; align-items: center; gap: 18px; padding: 0 16px; height: 30px; \
                            flex-shrink: 0; border-top: 1px solid {LINE}; font-size: 10px; color: {FAINT};",
                    span { "Up / Down  move" }
                    span { "Enter or double-click  load and close" }
                    span { "Esc  close" }
                    div { style: "flex: 1;" }
                    span { "Load in the detail keeps the library open, to try one after another" }
                }
            }
        }
    }
}

/// A magnifier, drawn — the font has no glyph for one.
#[component]
fn SearchGlyph() -> Element {
    rsx! {
        svg {
            width: "14", height: "14", view_box: "0 0 24 24", fill: "none",
            stroke: MUTED, stroke_width: "2", stroke_linecap: "round",
            style: "display: block; flex-shrink: 0; width: 14px; height: 14px;",
            circle { cx: "11", cy: "11", r: "7" }
            path { d: "M20 20l-3.5-3.5" }
        }
    }
}

// ── Detail ─────────────────────────────────────────────────────────────────

/// The focused item: a header with its name (renamable), then the kind's own
/// facts and actions. Keyed by kind + name, so a rename or a new focus starts
/// every edit field fresh.
#[component]
fn Detail(
    kind: Kind,
    row: Row,
    lib: LibraryModel,
    patches: Vec<PatchInfo>,
    presets: Vec<PresetInfo>,
    comp: CompositionModel,
    model: PerformanceModel,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    let name = row.name.clone();
    let idx = row.idx as u32;

    // Rename, wired per kind; `None` for what cannot be renamed here.
    let rename: Option<Callback<String>> = match kind {
        Kind::Setlists => Some(cbs.cb({
            let rig = rig.clone();
            move |new: String| {
                send(&rig, move |r| async move {
                    let _ = r.rename_setlist(idx, new).await;
                })
            }
        })),
        Kind::Profiles => Some(cbs.cb({
            let rig = rig.clone();
            let old = name.clone();
            move |new: String| {
                let old = old.clone();
                send(&rig, move |r| async move {
                    let _ = r.rename_profile(old, new).await;
                });
            }
        })),
        Kind::Patches => Some(cbs.cb({
            let rig = rig.clone();
            let old = name.clone();
            move |new: String| {
                let old = old.clone();
                send(&rig, move |r| async move {
                    let _ = r.rename_patch(old, new).await;
                });
            }
        })),
        Kind::Presets => Some(cbs.cb({
            let rig = rig.clone();
            let old = name.clone();
            move |new: String| {
                let old = old.clone();
                send(&rig, move |r| async move {
                    let _ = r.rename_preset(old, new).await;
                });
            }
        })),
        // A song's name is edited with its key and tempo, below.
        Kind::Songs | Kind::Drives | Kind::All => None,
        Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => None,
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; padding: 18px 18px 14px; \
                      border-bottom: 1px solid {LINE}; flex-shrink: 0;",
            div { style: "display: flex; align-items: center; gap: 8px; color: {FAINT};",
                fts_chrome::Glyph { icon: kind.icon(), size: 12 }
                span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase;",
                    "{kind.one()}"
                }
                if row.active {
                    span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {LIVE};",
                        "· Playing"
                    }
                }
            }
            NameLine { name: name.clone(), on_commit: rename }
            span { style: "font-size: 12px; color: {MUTED};", "{row.sub}" }
        }
        div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 14px 18px 18px; \
                      display: flex; flex-direction: column; gap: 18px;",
            match kind {
                Kind::Setlists => match lib.setlists.get(row.idx).cloned() {
                    Some(set) => rsx! { SetlistDetail { set, index: idx, lib: lib.clone(), model: model.clone(), on_go } },
                    None => rsx! {},
                },
                Kind::Songs => match lib.songs.get(row.idx).cloned() {
                    Some(song) => rsx! { SongDetail { song, lib: lib.clone(), model: model.clone(), on_go } },
                    None => rsx! {},
                },
                Kind::Profiles => match lib.profiles.get(row.idx).cloned() {
                    Some(profile) => rsx! { ProfileDetail { profile, on_go } },
                    None => rsx! {},
                },
                Kind::Patches => match patches.get(row.idx).cloned() {
                    Some(patch) => rsx! { PatchDetail { patch, index: idx, presets: presets.clone(), on_go } },
                    None => rsx! {},
                },
                Kind::Presets => match presets.get(row.idx).cloned() {
                    Some(preset) => rsx! { PresetDetail { preset, index: idx, patches: patches.clone(), on_go } },
                    None => rsx! {},
                },
                Kind::Drives => match lib.drives.get(row.idx).cloned() {
                    Some(drive) => rsx! { DriveDetail { drive } },
                    None => rsx! {},
                },
                Kind::Compositions => match comp.presets.get(row.idx).cloned() {
                    Some(preset) => rsx! { CompositionDetail { preset, comp: comp.clone() } },
                    None => rsx! {},
                },
                Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => match comp.modules.get(row.idx).cloned() {
                    Some(entry) => rsx! { ModuleDetail { entry, comp: comp.clone() } },
                    None => rsx! {},
                },
                // A row always names its own kind; `All` is only a tag.
                Kind::All => rsx! {},
            }
        }
    }
}

/// The item's name, large, with a pencil that turns it into a field. Enter
/// commits, Escape (or leaving the field) puts it back.
#[component]
fn NameLine(name: String, on_commit: Option<Callback<String>>) -> Element {
    let mut editing = use_signal(|| false);
    let mut text = use_signal(|| name.clone());

    if editing() {
        let original = name.clone();
        return rsx! {
            input {
                style: "font-size: 18px; font-weight: 700; color: {TEXT}; background: {BG}; \
                        border: 1px solid {FOCUS_FG}; border-radius: 7px; padding: 4px 8px; \
                        outline: none; min-width: 0;",
                value: "{text}",
                onmounted: move |e| {
                    spawn(async move {
                        let _ = e.data().set_focus(true).await;
                    });
                },
                oninput: move |e| text.set(e.value()),
                onblur: move |_| editing.set(false),
                onkeydown: move |e: KeyboardEvent| {
                    // The field owns these keys: Enter here is "rename",
                    // not the picker's "load and close".
                    e.stop_propagation();
                    match e.key() {
                        Key::Enter => {
                            let new = text.peek().trim().to_string();
                            if !new.is_empty() && new != original {
                                if let Some(cb) = on_commit {
                                    cb.call(new);
                                }
                            }
                            editing.set(false);
                        }
                        Key::Escape => {
                            text.set(original.clone());
                            editing.set(false);
                        }
                        _ => {}
                    }
                },
            }
        };
    }
    rsx! {
        div { style: "display: flex; align-items: center; gap: 8px; min-width: 0;",
            span { style: "font-size: 20px; font-weight: 700; line-height: 1.2; min-width: 0; overflow-wrap: anywhere;",
                "{name}"
            }
            if on_commit.is_some() {
                button {
                    style: "display: flex; align-items: center; justify-content: center; width: 24px; \
                            height: 24px; flex-shrink: 0; border-radius: 6px; border: none; \
                            background: transparent; color: {FAINT}; cursor: pointer;",
                    title: "Rename",
                    onclick: move |_| {
                        text.set(name.clone());
                        editing.set(true);
                    },
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Pencil, size: 13 }
                }
            }
        }
    }
}

/// A labelled group in the detail pane.
#[component]
fn Section(label: String, children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px;",
            span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.12em; \
                           text-transform: uppercase; color: {FAINT};",
                "{label}"
            }
            {children}
        }
    }
}

/// A row of action buttons.
#[component]
fn Actions(children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-wrap: wrap; gap: 6px;", {children} }
    }
}

/// An action button. `primary` is the one thing you came to do.
#[component]
fn Act(
    label: String,
    #[props(default = false)] primary: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = String::new())] title: String,
    onclick: EventHandler<()>,
) -> Element {
    let (bg, fg, border) = match (primary, disabled) {
        (_, true) => ("transparent", FAINT, LINE),
        (true, false) => ("#2563eb", "#ffffff", "#2563eb"),
        (false, false) => ("transparent", TEXT, "#34343c"),
    };
    let cursor = if disabled { "default" } else { "pointer" };
    rsx! {
        button {
            style: "padding: 6px 12px; border-radius: 7px; font-size: 12px; font-weight: 600; \
                    background: {bg}; color: {fg}; border: 1px solid {border}; \
                    cursor: {cursor};",
            title: "{title}",
            disabled,
            onclick: move |_| {
                if !disabled {
                    onclick.call(());
                }
            },
            "{label}"
        }
    }
}

/// Delete, in two steps — the first click arms it, the second deletes.
/// Disabled with the reason as its tooltip when the rig would refuse.
#[component]
fn DeleteAct(#[props(default)] refused: Option<String>, on_delete: EventHandler<()>) -> Element {
    let mut armed = use_signal(|| false);
    if let Some(why) = refused {
        return rsx! {
            Act { label: "Delete", disabled: true, title: why, onclick: |()| {} }
        };
    }
    rsx! {
        button {
            style: format!(
                "padding: 6px 12px; border-radius: 7px; font-size: 12px; font-weight: 600; cursor: pointer; \
                 background: {}; color: {}; border: 1px solid {};",
                if armed() { DANGER } else { "transparent" },
                if armed() { "#1a0505" } else { DANGER },
                if armed() { DANGER } else { "#4a1f22" },
            ),
            onclick: move |_| {
                if armed() {
                    on_delete.call(());
                    armed.set(false);
                } else {
                    armed.set(true);
                }
            },
            onmouseleave: move |_| armed.set(false),
            if armed() { "Click again to delete" } else { "Delete" }
        }
    }
}

/// A name typed in place, for Duplicate: a field, a confirm and a cancel.
#[component]
fn NamePrompt(label: String, initial: String, on_done: EventHandler<Option<String>>) -> Element {
    let mut text = use_signal(|| initial.clone());
    let commit = move || {
        let t = text.peek().trim().to_string();
        on_done.call((!t.is_empty()).then_some(t));
    };
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px;",
            input {
                style: "flex: 1; min-width: 0; font-size: 12px; color: {TEXT}; background: {BG}; \
                        border: 1px solid {LINE}; border-radius: 7px; padding: 6px 8px; outline: none;",
                value: "{text}",
                onmounted: move |e| {
                    spawn(async move {
                        let _ = e.data().set_focus(true).await;
                    });
                },
                oninput: move |e| text.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    e.stop_propagation();
                    match e.key() {
                        Key::Enter => commit(),
                        Key::Escape => on_done.call(None),
                        _ => {}
                    }
                },
            }
            Act { label: label.clone(), primary: true, onclick: move |()| commit() }
            Act { label: "Cancel", onclick: move |()| on_done.call(None) }
        }
    }
}

/// A clickable list line — a song in a set, a patch on a preset.
#[component]
fn LinkRow(
    title: String,
    #[props(default = String::new())] sub: String,
    #[props(default = false)] live: bool,
    onclick: EventHandler<()>,
    #[props(default)] on_remove: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 4px;",
            button {
                style: "flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: 8px; \
                        justify-content: flex-start; text-align: left; padding: 6px 8px; \
                        border-radius: 7px; border: none; background: {BG}; color: {TEXT}; cursor: pointer;",
                onclick: move |_| onclick.call(()),
                span {
                    style: format!(
                        "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {};",
                        if live { LIVE } else { "transparent" },
                    ),
                }
                span { style: "flex: 1 1 0; min-width: 0; font-size: 12px; font-weight: 600; white-space: nowrap; overflow: hidden;",
                    "{title}"
                }
                if !sub.is_empty() {
                    span { style: "font-size: 11px; color: {FAINT}; flex-shrink: 0;", "{sub}" }
                }
            }
            if let Some(remove) = on_remove {
                button {
                    style: "display: flex; align-items: center; justify-content: center; width: 26px; \
                            height: 26px; flex-shrink: 0; border-radius: 7px; border: none; \
                            background: transparent; color: {FAINT}; cursor: pointer;",
                    title: "Remove",
                    onclick: move |_| remove.call(()),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 12 }
                }
            }
        }
    }
}

/// A choice among a few names, as chips.
#[component]
fn Chips(
    options: Vec<String>,
    #[props(default = String::new())] selected: String,
    on_pick: EventHandler<String>,
) -> Element {
    rsx! {
        div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
            for opt in options {
                {
                    let on = opt.eq_ignore_ascii_case(&selected);
                    let pick = opt.clone();
                    rsx! {
                        button {
                            key: "{opt}",
                            style: format!(
                                "padding: 4px 10px; border-radius: 999px; font-size: 11px; font-weight: 600; \
                                 cursor: pointer; border: 1px solid {}; background: {}; color: {};",
                                if on { FOCUS_FG } else { LINE },
                                if on { FOCUS_BG } else { "transparent" },
                                if on { FOCUS_FG } else { MUTED },
                            ),
                            onclick: move |_| on_pick.call(pick.clone()),
                            "{opt}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SetlistDetail(
    set: SetlistEntry,
    index: u32,
    lib: LibraryModel,
    model: PerformanceModel,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut duplicating = use_signal(|| false);
    let mut adding = use_signal(|| false);
    let mut filter = use_signal(String::new);
    let last = lib.setlists.len() <= 1;

    let addable: Vec<String> = lib
        .songs
        .iter()
        .filter(|s| {
            !set.songs
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(&s.name))
        })
        .filter(|s| {
            let f = filter();
            f.trim().is_empty() || s.name.to_lowercase().contains(&f.trim().to_lowercase())
        })
        .map(|s| s.name.clone())
        .collect();

    rsx! {
        Actions {
            if !set.active {
                Act {
                    label: "Play this set",
                    primary: true,
                    onclick: {
                        let rig = rig.clone();
                        move |()| send(&rig, move |r| async move { let _ = r.select_setlist(index).await; })
                    },
                }
            }
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
            DeleteAct {
                refused: last.then(|| "The only setlist — make another first".to_string()),
                on_delete: {
                    let rig = rig.clone();
                    move |()| send(&rig, move |r| async move { let _ = r.delete_setlist(index).await; })
                },
            }
        }
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: format!("{} copy", set.name),
                on_done: {
                    let rig = rig.clone();
                    move |name: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = name {
                            let go = n.clone();
                            send(&rig, move |r| async move { let _ = r.duplicate_setlist(index, n).await; });
                            on_go.call((Kind::Setlists, go));
                        }
                    }
                },
            }
        }
        Section { label: "Songs",
            if set.songs.is_empty() {
                span { style: "font-size: 12px; color: {FAINT};", "No songs yet." }
            }
            for (j, slot) in set.songs.iter().cloned().enumerate() {
                LinkRow {
                    key: "{j}-{slot.name}",
                    title: format!("{}. {}", j + 1, slot.name),
                    sub: format!("{} · {}", slot.key, slot.bpm),
                    live: set.active && j == model.song_index as usize,
                    // In the playing set a click plays the song; elsewhere it
                    // opens the song.
                    onclick: {
                        let rig = rig.clone();
                        let name = slot.name.clone();
                        let active = set.active;
                        move |()| {
                            if active {
                                send(&rig, move |r| async move { let _ = r.select_song(j as u32).await; });
                            } else {
                                on_go.call((Kind::Songs, name.clone()));
                            }
                        }
                    },
                    on_remove: {
                        let rig = rig.clone();
                        move |()| send(&rig, move |r| async move {
                            let _ = r.remove_setlist_entry(index, j as u32).await;
                        })
                    },
                }
            }
            if adding() {
                input {
                    style: "font-size: 12px; color: {TEXT}; background: {BG}; border: 1px solid {LINE}; \
                            border-radius: 7px; padding: 6px 8px; outline: none; margin-top: 4px;",
                    placeholder: "Filter songs…",
                    value: "{filter}",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    },
                    oninput: move |e| filter.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        if e.key() == Key::Escape {
                            adding.set(false);
                        }
                    },
                }
                Chips {
                    options: addable,
                    on_pick: {
                        let rig = rig.clone();
                        move |song: String| send(&rig, move |r| async move {
                            let _ = r.add_setlist_entry(index, song).await;
                        })
                    },
                }
                Actions { Act { label: "Done", onclick: move |()| adding.set(false) } }
            } else {
                Actions { Act { label: "+ Add songs", onclick: move |()| { filter.set(String::new()); adding.set(true); } } }
            }
        }
    }
}

#[component]
fn SongDetail(
    song: SongEntry,
    lib: LibraryModel,
    model: PerformanceModel,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut name = use_signal(|| song.name.clone());
    let mut key = use_signal(|| song.key.clone());
    let mut bpm = use_signal(|| song.bpm.to_string());
    let dirty = name() != song.name || key() != song.key || bpm() != song.bpm.to_string();
    let in_set = model
        .songs
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(&song.name));
    let other_sets: Vec<(u32, String)> = lib
        .setlists
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            !song
                .setlists
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&s.name))
        })
        .map(|(i, s)| (i as u32, s.name.clone()))
        .collect();

    let save = {
        let rig = rig.clone();
        let old = song.name.clone();
        move || {
            let (n, k) = (
                name.peek().trim().to_string(),
                key.peek().trim().to_string(),
            );
            let b = bpm.peek().trim().parse::<u32>().unwrap_or(0);
            let old = old.clone();
            let go = n.clone();
            send(&rig, move |r| async move {
                let _ = r.edit_song(old, n, k, b).await;
            });
            on_go.call((Kind::Songs, go));
        }
    };

    let field = |label: &'static str, value: Signal<String>, width: &'static str| {
        let mut value = value;
        let save = save.clone();
        rsx! {
            label { style: "display: flex; flex-direction: column; gap: 4px; flex: {width}; min-width: 0;",
                span { style: "font-size: 10px; color: {FAINT};", "{label}" }
                input {
                    style: "font-size: 13px; color: {TEXT}; background: {BG}; border: 1px solid {LINE}; \
                            border-radius: 7px; padding: 6px 8px; outline: none; min-width: 0;",
                    value: "{value}",
                    oninput: move |e| value.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        if e.key() == Key::Enter {
                            save();
                        }
                    },
                }
            }
        }
    };

    rsx! {
        Actions {
            if let Some(pos) = in_set {
                Act {
                    label: "Play",
                    primary: true,
                    onclick: {
                        let rig = rig.clone();
                        move |()| send(&rig, move |r| async move { let _ = r.select_song(pos as u32).await; })
                    },
                }
            }
            DeleteAct {
                refused: (!song.setlists.is_empty()).then(|| {
                    format!("In {} — remove it there first", song.setlists.join(", "))
                }),
                on_delete: {
                    let rig = rig.clone();
                    let name = song.name.clone();
                    move |()| {
                        let name = name.clone();
                        send(&rig, move |r| async move { let _ = r.delete_song(name).await; });
                    }
                },
            }
        }
        Section { label: "Song",
            div { style: "display: flex; gap: 8px;",
                {field("Name", name, "3 1 0")}
            }
            div { style: "display: flex; gap: 8px;",
                {field("Key", key, "1 1 0")}
                {field("Tempo (bpm)", bpm, "1 1 0")}
            }
            if dirty {
                Actions {
                    Act { label: "Save", primary: true, onclick: { let save = save.clone(); move |()| save() } }
                    Act {
                        label: "Revert",
                        onclick: {
                            let song = song.clone();
                            move |()| {
                                name.set(song.name.clone());
                                key.set(song.key.clone());
                                bpm.set(song.bpm.to_string());
                            }
                        },
                    }
                }
            }
        }
        Section { label: "Played on",
            Chips {
                options: std::iter::once("Whatever is loaded".to_string())
                    .chain(lib.profiles.iter().map(|p| p.name.clone()))
                    .collect::<Vec<_>>(),
                selected: if song.profile.is_empty() { "Whatever is loaded".to_string() } else { song.profile.clone() },
                on_pick: {
                    let rig = rig.clone();
                    let name = song.name.clone();
                    move |p: String| {
                        let p = if p == "Whatever is loaded" { String::new() } else { p };
                        let name = name.clone();
                        send(&rig, move |r| async move { let _ = r.set_song_profile(name, p).await; });
                    }
                },
            }
        }
        if !song.parts.is_empty() {
            Section { label: "Starts on",
                Chips {
                    options: std::iter::once("Profile default".to_string())
                        .chain(song.parts.iter().cloned())
                        .collect::<Vec<_>>(),
                    selected: if song.start_part.is_empty() { "Profile default".to_string() } else { song.start_part.clone() },
                    on_pick: {
                        let rig = rig.clone();
                        let name = song.name.clone();
                        move |p: String| {
                            let p = if p == "Profile default" { String::new() } else { p };
                            let name = name.clone();
                            send(&rig, move |r| async move { let _ = r.set_song_start_part(name, p).await; });
                        }
                    },
                }
            }
        }
        Section { label: "Song parts",
            if song.parts.is_empty() {
                span { style: "font-size: 12px; color: {FAINT}; line-height: 1.5;",
                    "No song parts yet — add them in Setlist mode's sidebar, with the song playing."
                }
            } else {
                Chips { options: song.parts.clone(), on_pick: |_: String| {} }
            }
        }
        Section { label: "In setlists",
            if song.setlists.is_empty() {
                span { style: "font-size: 12px; color: {FAINT};", "Not in a set." }
            }
            for set in song.setlists.iter().cloned() {
                LinkRow {
                    key: "{set}",
                    title: set.clone(),
                    onclick: move |()| on_go.call((Kind::Setlists, set.clone())),
                }
            }
        }
        if !other_sets.is_empty() {
            Section { label: "Add to a set",
                Chips {
                    options: other_sets.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>(),
                    on_pick: {
                        let rig = rig.clone();
                        let song = song.name.clone();
                        move |set: String| {
                            let Some((i, _)) = other_sets.iter().find(|(_, n)| *n == set) else { return };
                            let (i, song) = (*i, song.clone());
                            send(&rig, move |r| async move { let _ = r.add_setlist_entry(i, song).await; });
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn ProfileDetail(profile: ProfileEntry, on_go: EventHandler<(Kind, String)>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut duplicating = use_signal(|| false);
    rsx! {
        Actions {
            if !profile.active {
                Act {
                    label: "Load profile",
                    primary: true,
                    onclick: {
                        let rig = rig.clone();
                        let name = profile.name.clone();
                        move |()| {
                            let name = name.clone();
                            send(&rig, move |r| async move { let _ = r.select_profile(name).await; });
                        }
                    },
                }
            }
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
            DeleteAct {
                refused: profile.active.then(|| "Playing — load another profile first".to_string()),
                on_delete: {
                    let rig = rig.clone();
                    let name = profile.name.clone();
                    move |()| {
                        let name = name.clone();
                        send(&rig, move |r| async move { let _ = r.delete_profile(name).await; });
                    }
                },
            }
        }
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: format!("{} copy", profile.name),
                on_done: {
                    let rig = rig.clone();
                    let from = profile.name.clone();
                    move |name: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = name {
                            let (from, go) = (from.clone(), n.clone());
                            send(&rig, move |r| async move { let _ = r.add_profile(n, from).await; });
                            on_go.call((Kind::Profiles, go));
                        }
                    }
                },
            }
        }
        // Its default scene: where it lands when loaded. Keeps the slot
        // convention (1 clean … 4 lead) while a metal profile still starts
        // on the chug in slot 3.
        Section { label: "Lands on",
            Chips {
                options: profile
                    .patch_list
                    .iter()
                    .map(|p| if p.stack.is_empty() { p.name.clone() } else { format!("{} · {}", p.stack, p.name) })
                    .collect::<Vec<_>>(),
                selected: profile
                    .patch_list
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&profile.default_patch))
                    .or_else(|| profile.patch_list.first())
                    .map(|p| if p.stack.is_empty() { p.name.clone() } else { format!("{} · {}", p.stack, p.name) })
                    .unwrap_or_default(),
                on_pick: {
                    let rig = rig.clone();
                    let name = profile.name.clone();
                    let patches = profile.patch_list.clone();
                    move |label: String| {
                        let Some(p) = patches.iter().find(|p| {
                            label == p.name || label == format!("{} · {}", p.stack, p.name)
                        }) else {
                            return;
                        };
                        let (name, patch) = (name.clone(), p.name.clone());
                        send(&rig, move |r| async move { let _ = r.set_profile_default(name, patch).await; });
                    }
                },
            }
        }
        Section { label: "Stacks",
            if profile.stacks.is_empty() {
                span { style: "font-size: 12px; color: {FAINT};", "No stacks." }
            } else {
                Chips { options: profile.stacks.clone(), on_pick: |_: String| {} }
            }
        }
        Section { label: "Amps",
            if profile.presets.is_empty() {
                span { style: "font-size: 12px; color: {FAINT};", "No presets." }
            } else {
                Chips { options: profile.presets.clone(), on_pick: |_: String| {} }
            }
        }
        if profile.active {
            span { style: "font-size: 11px; color: {FAINT}; line-height: 1.5;",
                "Its patches, presets and drives are under \"In {profile.name}\" on the left."
            }
        }
    }
}

#[component]
fn PatchDetail(
    patch: PatchInfo,
    index: u32,
    presets: Vec<PresetInfo>,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        Actions {
            Act {
                label: if patch.active { "Playing" } else { "Load" },
                primary: !patch.active,
                disabled: patch.active,
                onclick: {
                    let rig = rig.clone();
                    move |()| send(&rig, move |r| async move { let _ = r.select_patch(index).await; })
                },
            }
            DeleteAct {
                on_delete: {
                    let rig = rig.clone();
                    let name = patch.name.clone();
                    move |()| {
                        let name = name.clone();
                        send(&rig, move |r| async move { let _ = r.delete_patch(name).await; });
                    }
                },
            }
        }
        Section { label: "Amp",
            span { style: "font-size: 11px; color: {FAINT};", "Point this patch at a different preset:" }
            Chips {
                options: presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                selected: patch.preset.clone(),
                on_pick: {
                    let rig = rig.clone();
                    let names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
                    move |name: String| {
                        if let Some(p) = names.iter().position(|n| *n == name) {
                            send(&rig, move |r| async move {
                                let _ = r.set_patch_preset(index, p as u32).await;
                            });
                        }
                    }
                },
            }
            LinkRow {
                title: format!("Open {}", patch.preset),
                onclick: {
                    let preset = patch.preset.clone();
                    move |()| on_go.call((Kind::Presets, preset.clone()))
                },
            }
        }
        Section { label: "Stack",
            span { style: "font-size: 12px; color: {MUTED};",
                if patch.stack.is_empty() {
                    "In no stack — reachable from here and the palette only."
                } else if patch.default_in_stack {
                    "{patch.stack} — its default (where the footswitch lands)"
                } else {
                    "{patch.stack}"
                }
            }
        }
        if !patch.override_modules.is_empty() {
            Section { label: "Changes on its preset",
                Chips { options: patch.override_modules.clone(), on_pick: |_: String| {} }
            }
        }
    }
}

#[component]
fn PresetDetail(
    preset: PresetInfo,
    index: u32,
    patches: Vec<PatchInfo>,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut editing_cab = use_signal(|| false);
    let users: Vec<PatchInfo> = patches
        .iter()
        .filter(|p| p.preset.eq_ignore_ascii_case(&preset.name))
        .cloned()
        .collect();
    rsx! {
        Actions {
            Act {
                label: "Play",
                primary: true,
                title: "Plays its first patch — Load keeps the library open, to try the next",
                onclick: {
                    let rig = rig.clone();
                    move |()| send(&rig, move |r| async move { let _ = r.play_preset(index).await; })
                },
            }
            DeleteAct {
                refused: (preset.used_by > 0).then(|| {
                    format!("Used by {} — repoint them first", count(preset.used_by as usize, "patch"))
                }),
                on_delete: {
                    let rig = rig.clone();
                    let name = preset.name.clone();
                    move |()| {
                        let name = name.clone();
                        send(&rig, move |r| async move { let _ = r.delete_preset(name).await; });
                    }
                },
            }
        }
        Section { label: "Used by",
            if users.is_empty() {
                span { style: "font-size: 12px; color: {FAINT}; line-height: 1.5;",
                    "No patch yet. Play makes it audible anyway; + New patch puts it on a footswitch."
                }
            }
            for p in users {
                LinkRow {
                    key: "{p.name}",
                    title: p.name.clone(),
                    sub: p.stack.clone(),
                    live: p.active,
                    onclick: {
                        let name = p.name.clone();
                        move |()| on_go.call((Kind::Patches, name.clone()))
                    },
                }
            }
        }
        if !(preset.creator.is_empty() && preset.gear.is_empty() && preset.license.is_empty()) {
            Section { label: "Capture",
                if !preset.gear.is_empty() {
                    span { style: "font-size: 12px; color: {MUTED};", "Gear: {preset.gear}" }
                }
                if !preset.creator.is_empty() {
                    span { style: "font-size: 12px; color: {MUTED};", "By {preset.creator}" }
                }
                if !preset.license.is_empty() {
                    span { style: "font-size: 12px; color: {MUTED};", "Licence: {preset.license}" }
                }
                if !preset.tone_url.is_empty() {
                    span { style: "font-size: 11px; color: {FAINT}; overflow-wrap: anywhere;", "{preset.tone_url}" }
                }
            }
        }
        Section { label: "Cab",
            if editing_cab() {
                NamePrompt {
                    label: "IR wav path (blank = none / built-in)".to_string(),
                    initial: preset.cab.clone(),
                    on_done: {
                        let rig = rig.clone();
                        move |v: Option<String>| {
                            editing_cab.set(false);
                            let path = v.unwrap_or_default();
                            send(&rig, move |r| async move { let _ = r.set_preset_cab(index, path).await; });
                        }
                    },
                }
            } else {
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span { style: "font-size: 12px; color: {MUTED}; flex: 1; overflow-wrap: anywhere;",
                        if !preset.cab.is_empty() {
                            "{preset.cab}"
                        } else if preset.gear.eq_ignore_ascii_case("amp-cab") {
                            "None needed — this capture is already a full rig"
                        } else {
                            "None — this amp is played dry until one is picked"
                        }
                    }
                    Act {
                        label: if preset.cab.is_empty() { "Pick" } else { "Change" },
                        onclick: move |()| editing_cab.set(true),
                    }
                }
            }
        }
    }
}

#[component]
fn DriveDetail(drive: DriveEntry) -> Element {
    rsx! {
        Section { label: "Captures",
            for (i, opt) in drive.options.iter().enumerate() {
                span { key: "{i}", style: "font-size: 12px; color: {TEXT}; padding: 4px 0;", "{opt}" }
            }
        }
        Section { label: "On the board",
            span { style: "font-size: 12px; color: {MUTED}; line-height: 1.5;",
                if drive.slots.is_empty() {
                    "Not in a drive slot of this profile."
                } else {
                    "{drive.slots.join(\", \")} — choose the capture on the Control view's drive board."
                }
            }
        }
        span { style: "font-size: 11px; color: {FAINT}; line-height: 1.5;",
            "New pedals arrive from Tones: a downloaded pedal capture becomes a drive preset."
        }
    }
}

/// A module preset: its snapshots as chips — the one playing is lit, a tap
/// plays another on the active patch.
#[component]
fn ModuleDetail(entry: signal_guitar_proto::ModulePresetEntry, comp: CompositionModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let playing = comp
        .active_modules
        .iter()
        .find(|a| {
            a.module.eq_ignore_ascii_case(&entry.module)
                && a.preset.eq_ignore_ascii_case(&entry.name)
        })
        .map(|a| a.snapshot.clone());
    rsx! {
        Section { label: "Snapshots",
            div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                for (i, snap) in entry.snapshots.iter().enumerate() {
                    {
                        let lit = playing.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(snap)
                            || (p.is_empty() && i == 0));
                        let (module, preset, snap_name) = (entry.module.clone(), entry.name.clone(), snap.clone());
                        let rig = rig.clone();
                        rsx! {
                            button {
                                key: "{snap}",
                                style: format!(
                                    "padding: 6px 12px; border-radius: 7px; font-size: 12px; font-weight: 600; cursor: pointer; \
                                     border: 1px solid {}; background: {}; color: {};",
                                    if lit { LIVE } else { LINE },
                                    if lit { "rgba(34,197,94,0.12)" } else { "transparent" },
                                    if lit { TEXT } else { MUTED },
                                ),
                                onclick: move |_| {
                                    let (m, p, s) = (module.clone(), preset.clone(), snap_name.clone());
                                    send(&rig, move |r| async move { let _ = r.choose_module(m, p, s).await; });
                                },
                                "{snap}"
                            }
                        }
                    }
                }
            }
        }
        span { style: "font-size: 11px; color: {FAINT}; line-height: 1.5;",
            "Choosing here sets this module on the playing patch only — its preset keeps its own pick."
        }
    }
}

/// A preset: each snapshot with the module snapshots it plays.
#[component]
fn CompositionDetail(preset: signal_guitar_proto::PresetEntry, comp: CompositionModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let playing_here = comp.active_preset.eq_ignore_ascii_case(&preset.name);
    rsx! {
        Section { label: "Snapshots",
            for (i, snap) in preset.snapshots.iter().enumerate() {
                {
                    let lit = playing_here
                        && (comp.active_snapshot.eq_ignore_ascii_case(&snap.name)
                            || (comp.active_snapshot.is_empty() && i == 0));
                    let (name, snap_name) = (preset.name.clone(), snap.name.clone());
                    let rig = rig.clone();
                    let picks = snap
                        .modules
                        .iter()
                        .map(|m| format!("{}: {} · {}", m.module, m.preset, if m.snapshot.is_empty() { "—" } else { &m.snapshot }))
                        .collect::<Vec<_>>();
                    rsx! {
                        button {
                            key: "{snap.name}",
                            style: format!(
                                "display: flex; flex-direction: column; align-items: flex-start; gap: 3px; text-align: left; \
                                 padding: 8px 10px; border-radius: 8px; cursor: pointer; border: 1px solid {}; background: {};",
                                if lit { LIVE } else { LINE },
                                if lit { "rgba(34,197,94,0.10)" } else { "transparent" },
                            ),
                            onclick: move |_| {
                                let (n, s) = (name.clone(), snap_name.clone());
                                send(&rig, move |r| async move { let _ = r.choose_preset(n, s).await; });
                            },
                            span { style: "font-size: 13px; font-weight: 700; color: {TEXT};", "{snap.name}" }
                            for line in picks {
                                span { style: "font-size: 11px; color: {MUTED};", "{line}" }
                            }
                            if snap.overrides > 0 {
                                span { style: "font-size: 11px; color: {FAINT};", "+ {snap.overrides} overrides" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── New ────────────────────────────────────────────────────────────────────

/// The "+ New" form for a kind, inline above the list. `on_done` gets the
/// new item's name, to focus it, or `None` on cancel.
#[component]
fn NewForm(
    kind: Kind,
    lib: LibraryModel,
    patches: Vec<PatchInfo>,
    presets: Vec<PresetInfo>,
    model: PerformanceModel,
    on_done: EventHandler<Option<String>>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut name = use_signal(String::new);
    let mut key = use_signal(|| "C".to_string());
    let mut bpm = use_signal(|| "120".to_string());
    let mut path = use_signal(String::new);
    // Profiles: what to start from ("" = a starter). Patches: stack, preset.
    let mut from = use_signal(String::new);
    let mut stack = use_signal(|| {
        model
            .stacks
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default()
    });
    let mut amp = use_signal(|| presets.first().map(|p| p.name.clone()).unwrap_or_default());

    let taken = |n: &str| -> bool {
        let n = n.trim();
        match kind {
            Kind::Setlists => lib.setlists.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            Kind::Songs => lib.songs.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            Kind::Profiles => lib.profiles.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            Kind::Patches => patches.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            Kind::Presets => presets.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            Kind::Drives | Kind::All => false,
            Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => false,
        }
    };
    let n = name();
    let problem = if n.trim().is_empty() {
        Some("Give it a name".to_string())
    } else if taken(&n) {
        Some(format!(
            "There is already a {} called {}",
            kind.one(),
            n.trim()
        ))
    } else if kind == Kind::Presets && path().trim().is_empty() {
        Some("Path to a .nam capture".to_string())
    } else if kind == Kind::Patches && (stack().is_empty() || amp().is_empty()) {
        Some("Choose a stack and a preset".to_string())
    } else {
        None
    };

    let ok = problem.is_none();
    let create = use_callback({
        let rig = rig.clone();
        move |()| {
            if !ok {
                return;
            }
            let n = name.peek().trim().to_string();
            let done = n.clone();
            match kind {
                Kind::Setlists => send(&rig, move |r| async move {
                    let _ = r.add_setlist(n).await;
                }),
                Kind::Songs => {
                    let (k, b) = (
                        key.peek().trim().to_string(),
                        bpm.peek().trim().parse().unwrap_or(0),
                    );
                    send(&rig, move |r| async move {
                        let _ = r.add_song(n, k, b).await;
                    });
                }
                Kind::Profiles => {
                    let f = from.peek().clone();
                    send(&rig, move |r| async move {
                        let _ = r.add_profile(n, f).await;
                    });
                }
                Kind::Patches => {
                    let (s, p) = (stack.peek().clone(), amp.peek().clone());
                    send(&rig, move |r| async move {
                        let _ = r.add_patch(n, s, p).await;
                    });
                }
                Kind::Presets => {
                    let p = path.peek().trim().to_string();
                    send(&rig, move |r| async move {
                        let _ = r.add_preset(n, p).await;
                    });
                }
                Kind::Drives | Kind::All => return,
                Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::DelayModules | Kind::ReverbModules => {
                    return;
                }
            }
            on_done.call(Some(done));
        }
    });

    let input_style = format!(
        "font-size: 12px; color: {TEXT}; background: {BG}; border: 1px solid {LINE}; \
         border-radius: 7px; padding: 6px 8px; outline: none; min-width: 0;"
    );
    let keys = use_callback(move |e: KeyboardEvent| {
        e.stop_propagation();
        match e.key() {
            Key::Enter => create.call(()),
            Key::Escape => on_done.call(None),
            _ => {}
        }
    });

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 8px; margin: 0 16px 10px; padding: 12px; \
                    border-radius: 10px; border: 1px solid {LINE}; background: {PANE}; flex-shrink: 0;",
            div { style: "display: flex; gap: 8px;",
                input {
                    style: "{input_style} flex: 2 1 0;",
                    placeholder: "New {kind.one()} name",
                    value: "{n}",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    },
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| keys.call(e),
                }
                if kind == Kind::Songs {
                    input {
                        style: "{input_style} flex: 0 0 64px;",
                        placeholder: "Key",
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                        onkeydown: move |e| keys.call(e),
                    }
                    input {
                        style: "{input_style} flex: 0 0 72px;",
                        placeholder: "bpm",
                        value: "{bpm}",
                        oninput: move |e| bpm.set(e.value()),
                        onkeydown: move |e| keys.call(e),
                    }
                }
                if kind == Kind::Presets {
                    input {
                        style: "{input_style} flex: 3 1 0;",
                        placeholder: "/path/to/capture.nam",
                        value: "{path}",
                        oninput: move |e| path.set(e.value()),
                        onkeydown: move |e| keys.call(e),
                    }
                }
            }
            if kind == Kind::Profiles {
                span { style: "font-size: 10px; color: {FAINT};", "Start from" }
                Chips {
                    options: std::iter::once("Starter".to_string())
                        .chain(lib.profiles.iter().map(|p| p.name.clone()))
                        .collect::<Vec<_>>(),
                    selected: if from().is_empty() { "Starter".to_string() } else { from() },
                    on_pick: move |p: String| from.set(if p == "Starter" { String::new() } else { p }),
                }
                span { style: "font-size: 10px; color: {FAINT}; line-height: 1.5;",
                    if from().is_empty() {
                        "Starter: the presets and pedals of {model.profile_name}, one Clean stack, one patch."
                    } else {
                        "A copy of {from()} — every preset, patch and stack."
                    }
                }
            }
            if kind == Kind::Patches {
                span { style: "font-size: 10px; color: {FAINT};", "Stack" }
                Chips {
                    options: model.stacks.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                    selected: stack(),
                    on_pick: move |s: String| stack.set(s),
                }
                span { style: "font-size: 10px; color: {FAINT};", "Preset" }
                Chips {
                    options: presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    selected: amp(),
                    on_pick: move |p: String| amp.set(p),
                }
            }
            div { style: "display: flex; align-items: center; gap: 8px;",
                Act {
                    label: "Create",
                    primary: true,
                    disabled: problem.is_some(),
                    onclick: move |()| create.call(()),
                }
                Act { label: "Cancel", onclick: move |()| on_done.call(None) }
                if let Some(p) = problem {
                    span { style: "font-size: 11px; color: {FAINT};", "{p}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, sub: &str) -> Row {
        Row {
            kind: Kind::Songs,
            name: name.into(),
            idx: 0,
            sub: sub.into(),
            active: false,
        }
    }

    #[test]
    fn search_matches_every_word_in_name_or_subtitle() {
        let r = row("WASHED", "B · 139 bpm");
        assert!(matches(&r, "washed"));
        assert!(matches(&r, "wash 139"));
        assert!(!matches(&r, "washed 145"));
    }

    #[test]
    fn counts_read_as_english() {
        assert_eq!(count(1, "song"), "1 song");
        assert_eq!(count(3, "song"), "3 songs");
        assert_eq!(count(2, "patch"), "2 patches");
        assert_eq!(count(0, "stack"), "0 stacks");
    }

    #[test]
    fn each_perform_mode_opens_on_what_it_plays_from() {
        assert_eq!(Kind::for_perform_mode(0), Kind::Compositions);
        assert_eq!(Kind::for_perform_mode(1), Kind::Profiles);
        assert_eq!(Kind::for_perform_mode(2), Kind::Setlists);
    }
}
