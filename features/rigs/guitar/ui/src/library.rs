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

// The look is the rig's own (`theme`); the buttons, prompts and chips are
// the kit's, shared with every sidebar.
use crate::kit::{Button as Act, Chips, DeleteButton as DeleteAct, ListRow, MenuItem, NamePrompt, Picked};
use crate::theme::{BG, FAINT, FOCUS_BG, FOCUS_FG, LINE, LIVE, MUTED, PANE, TEXT};

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
    TimeModules,
    DelayModules,
    ReverbModules,
    /// Block presets — a delay, a reverb, a compressor setting — that module
    /// snapshots and patches put on a block.
    BlockPresets,
}

impl Kind {
    const ALL: [Self; 13] = [
        Self::Setlists,
        Self::Songs,
        Self::Profiles,
        Self::Patches,
        Self::Compositions,
        Self::AmpModules,
        Self::DriveModules,
        Self::TimeModules,
        Self::DelayModules,
        Self::ReverbModules,
        Self::BlockPresets,
        Self::Presets,
        Self::Drives,
    ];

    /// The rail: no tag, then every kind.
    const RAIL: [Self; 14] = [
        Self::All,
        Self::Setlists,
        Self::Songs,
        Self::Profiles,
        Self::Patches,
        Self::Compositions,
        Self::AmpModules,
        Self::DriveModules,
        Self::TimeModules,
        Self::DelayModules,
        Self::ReverbModules,
        Self::BlockPresets,
        Self::Presets,
        Self::Drives,
    ];

    /// The module-preset kind that browses `module`'s presets.
    #[must_use]
    pub fn for_module(module: &str) -> Option<Self> {
        [
            Self::AmpModules,
            Self::DriveModules,
            Self::TimeModules,
            Self::DelayModules,
            Self::ReverbModules,
        ]
        .into_iter()
        .find(|k| k.module().is_some_and(|m| m.eq_ignore_ascii_case(module)))
    }

    /// The module a module-preset kind browses, as the rig names it.
    #[must_use]
    pub const fn module(self) -> Option<&'static str> {
        match self {
            Self::AmpModules => Some("Amp"),
            Self::DriveModules => Some("Drive"),
            Self::TimeModules => Some("Time"),
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
            Self::TimeModules => "Time",
            Self::DelayModules => "Delay",
            Self::ReverbModules => "Reverb",
            Self::BlockPresets => "Blocks",
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
            Self::TimeModules => "time preset",
            Self::DelayModules => "delay preset",
            Self::ReverbModules => "reverb preset",
            Self::BlockPresets => "block preset",
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
            Self::TimeModules => fts_chrome::Icon::Refresh,
            Self::DelayModules => fts_chrome::Icon::Refresh,
            Self::ReverbModules => fts_chrome::Icon::Refresh,
            Self::BlockPresets => fts_chrome::Icon::Control,
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
        Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules => {
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
        Kind::BlockPresets => {
            let mut v: Vec<Row> = comp
            .block_presets
            .iter()
            .enumerate()
            .map(|(idx, b)| Row {
                kind,
                name: b.name.clone(),
                idx,
                sub: {
                    let what = if b.bypass { format!("{} · off", b.block_type) } else { b.block_type.clone() };
                    if b.used_by.is_empty() {
                        what
                    } else {
                        format!("{what} · used by {}", b.used_by.len())
                    }
                },
                active: comp.active_blocks.iter().any(|a| a.preset.eq_ignore_ascii_case(&b.name)),
            })
            .collect();
            // Grouped by type (see `group_of`), in library order within one.
            v.sort_by_key(|r| r.sub.split(" · ").next().unwrap_or_default().to_string());
            v
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

/// The group a row is listed under: its kind in the untagged list, its
/// block type among block presets; else none.
fn group_of(kind: Kind, row: &Row) -> String {
    match kind {
        Kind::All => row.kind.label().to_string(),
        Kind::BlockPresets => row.sub.split(" · ").next().unwrap_or_default().to_string(),
        _ => String::new(),
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
        Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules => {
            let module = row.kind.module().unwrap_or_default().to_string();
            send(rig, move |r| async move {
                let _ = r.choose_module(module, name, String::new()).await;
            });
        }
        Kind::Drives | Kind::BlockPresets | Kind::All => return false,
    }
    true
}

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
                            if !matches!(kind, Kind::Drives | Kind::All | Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules | Kind::BlockPresets) {
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
                            for (n, row) in list.iter().cloned().enumerate() {
                                {
                                    // A quiet header where the group changes: the
                                    // kind in the untagged list, the block type
                                    // among block presets.
                                    let group = group_of(kind, &row);
                                    let new_group = !group.is_empty()
                                        && (n == 0 || group_of(kind, &list[n - 1]) != group);
                                    let is_focus = focused
                                        .as_ref()
                                        .is_some_and(|f| f.kind == row.kind && f.name == row.name);
                                    let rig = rig.clone();
                                    let model = model.clone();
                                    let target = (row.kind, row.name.clone());
                                    rsx! {
                                        div { key: "{row.kind.label()}-{row.name}", style: "display: contents;",
                                        if new_group {
                                            div {
                                                style: "padding: 12px 10px 4px; font-size: 9px; font-weight: 700; letter-spacing: 0.14em; \
                                                        text-transform: uppercase; color: {FAINT};",
                                                "{group}"
                                            }
                                        }
                                        button {
                                            style: format!(
                                                "display: flex; align-items: center; gap: 10px; width: 100%; \
                                                 max-width: 880px; padding: 8px 10px; margin-bottom: 1px; border-radius: 8px; \
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
        Kind::Compositions => Some(cbs.cb({
            let rig = rig.clone();
            let old = name.clone();
            move |new: String| {
                let old = old.clone();
                send(&rig, move |r| async move {
                    let _ = r.rename_rig_preset(old, new).await;
                });
            }
        })),
        Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules => {
            let module = kind.module().unwrap_or_default().to_string();
            Some(cbs.cb({
                let rig = rig.clone();
                let old = name.clone();
                move |new: String| {
                    let (module, old) = (module.clone(), old.clone());
                    send(&rig, move |r| async move {
                        let _ = r.rename_module_preset(module, old, new).await;
                    });
                }
            }))
        }
        Kind::BlockPresets => Some(cbs.cb({
            let rig = rig.clone();
            let old = name.clone();
            move |new: String| {
                let old = old.clone();
                send(&rig, move |r| async move {
                    let _ = r.rename_block_preset(old, new).await;
                });
            }
        })),
        // A song's name is edited with its key and tempo, below.
        Kind::Songs | Kind::Drives | Kind::All => None,
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
                    Some(profile) => rsx! { ProfileDetail { profile, profiles: lib.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), on_go } },
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
                    Some(preset) => rsx! { CompositionDetail { preset, comp: comp.clone(), on_go } },
                    None => rsx! {},
                },
                Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules => match comp.modules.get(row.idx).cloned() {
                    Some(entry) => rsx! { ModuleDetail { entry, comp: comp.clone(), kind, on_go } },
                    None => rsx! {},
                },
                Kind::BlockPresets => match comp.block_presets.get(row.idx).cloned() {
                    Some(preset) => rsx! { BlockPresetDetail { preset, comp: comp.clone(), on_go } },
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

/// A clickable list line — a song in a set, a patch on a preset.
#[component]
fn LinkRow(
    title: String,
    #[props(default = String::new())] sub: String,
    #[props(default = false)] live: bool,
    onclick: EventHandler<()>,
    #[props(default)] on_remove: Option<EventHandler<()>>,
    /// Move it up (−1) or down (+1) in its list; `None` for a fixed list.
    #[props(default)]
    on_move: Option<EventHandler<i32>>,
    #[props(default)] first: bool,
    #[props(default)] last: bool,
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
            if let Some(mv) = on_move {
                crate::kit::IconButton {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Move up",
                    flip: true,
                    disabled: first,
                    size: 24,
                    onclick: move |()| mv.call(-1),
                }
                crate::kit::IconButton {
                    icon: fts_chrome::Icon::ChevronDown,
                    title: "Move down",
                    disabled: last,
                    size: 24,
                    onclick: move |()| mv.call(1),
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
            crate::kit::IconButton {
                icon: fts_chrome::Icon::ChevronDown,
                title: "Move this set up the list",
                flip: true,
                disabled: index == 0,
                size: 30,
                onclick: {
                    let rig = rig.clone();
                    move |()| send(&rig, move |r| async move { let _ = r.move_setlist(index, index.saturating_sub(1)).await; })
                },
            }
            crate::kit::IconButton {
                icon: fts_chrome::Icon::ChevronDown,
                title: "Move this set down the list",
                disabled: index as usize + 1 >= lib.setlists.len(),
                size: 30,
                onclick: {
                    let rig = rig.clone();
                    move |()| send(&rig, move |r| async move { let _ = r.move_setlist(index, index + 1).await; })
                },
            }
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
                initial: crate::module_sidebar::next_name(&set.name, &lib.setlists.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
                taken: lib.setlists.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
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
                    first: j == 0,
                    last: j + 1 == set.songs.len(),
                    on_move: {
                        let rig = rig.clone();
                        move |d: i32| {
                            let to = (j as i32 + d).max(0) as u32;
                            send(&rig, move |r| async move { let _ = r.move_setlist_entry(index, j as u32, to).await; });
                        }
                    },
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
    let mut duplicating = use_signal(|| false);
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
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
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
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: crate::module_sidebar::next_name(&song.name, &lib.songs.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
                taken: lib.songs.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                on_done: {
                    let rig = rig.clone();
                    let from = song.name.clone();
                    move |n: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = n {
                            let (from, go) = (from.clone(), n.clone());
                            send(&rig, move |r| async move { let _ = r.duplicate_song(from, n).await; });
                            on_go.call((Kind::Songs, go));
                        }
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
fn ProfileDetail(profile: ProfileEntry, profiles: Vec<String>, on_go: EventHandler<(Kind, String)>) -> Element {
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
                initial: crate::module_sidebar::next_name(&profile.name, &profiles),
                taken: profiles.clone(),
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
                    label: "Set".to_string(),
                    placeholder: "IR wav path (blank = none / built-in)".to_string(),
                    allow_blank: true,
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

/// "Used by": what refers to an item — the reason a delete is refused,
/// listed so it can be undone one by one.
#[component]
fn UsedBy(users: Vec<String>, #[props(default)] none: String) -> Element {
    rsx! {
        Section { label: "Used by",
            if users.is_empty() {
                span { style: "font-size: 12px; color: {FAINT}; line-height: 1.5;",
                    if none.is_empty() { "Nothing — it can be deleted." } else { "{none}" }
                }
            }
            for u in users {
                span { key: "{u}", style: "font-size: 12px; color: {MUTED}; padding: 2px 0;", "{u}" }
            }
        }
    }
}

/// "In use: …" for a refused delete.
fn in_use(users: &[String]) -> Option<String> {
    (!users.is_empty()).then(|| format!("In use: {} — change those first", users.join(", ")))
}

/// A module preset: its snapshots — the one playing lit, a tap plays another
/// on the active patch — each with its menu (rename, delete); and the
/// preset's own management (rename above, duplicate, delete).
#[component]
fn ModuleDetail(
    entry: signal_guitar_proto::ModulePresetEntry,
    comp: CompositionModel,
    kind: Kind,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut duplicating = use_signal(|| false);
    let siblings: Vec<String> = comp
        .modules
        .iter()
        .filter(|m| m.module.eq_ignore_ascii_case(&entry.module))
        .map(|m| m.name.clone())
        .collect();
    let playing = comp
        .active_modules
        .iter()
        .find(|a| {
            a.module.eq_ignore_ascii_case(&entry.module)
                && a.preset.eq_ignore_ascii_case(&entry.name)
        })
        .map(|a| a.snapshot.clone());
    rsx! {
        Actions {
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
            DeleteAct {
                refused: in_use(&entry.used_by),
                on_delete: {
                    let rig = rig.clone();
                    let (m, n) = (entry.module.clone(), entry.name.clone());
                    move |()| {
                        let (m, n) = (m.clone(), n.clone());
                        send(&rig, move |r| async move { let _ = r.delete_module_preset(m, n).await; });
                    }
                },
            }
        }
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: crate::module_sidebar::next_name(&entry.name, &siblings),
                taken: siblings.clone(),
                on_done: {
                    let rig = rig.clone();
                    let (m, from) = (entry.module.clone(), entry.name.clone());
                    move |name: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = name {
                            let (m, from, go) = (m.clone(), from.clone(), n.clone());
                            send(&rig, move |r| async move { let _ = r.duplicate_module_preset(m, from, n).await; });
                            on_go.call((kind, go));
                        }
                    }
                },
            }
        }
        Section { label: "Snapshots",
            for (i, snap) in entry.snapshots.iter().cloned().enumerate() {
                {
                    let lit = playing.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(&snap)
                        || (p.is_empty() && i == 0));
                    let users = entry.snapshot_used_by.get(i).cloned().unwrap_or_default();
                    let items: Vec<MenuItem> = crate::module_sidebar::snapshot_items(&entry, &snap);
                    rsx! {
                        ListRow {
                            key: "{snap}",
                            title: snap.clone(),
                            note: if users.is_empty() { String::new() } else { format!("{}", users.split(", ").count()) },
                            live: lit,
                            onclick: {
                                let rig = rig.clone();
                                let (m, p, s) = (entry.module.clone(), entry.name.clone(), snap.clone());
                                move |()| {
                                    let (m, p, s) = (m.clone(), p.clone(), s.clone());
                                    send(&rig, move |r| async move { let _ = r.choose_module(m, p, s).await; });
                                }
                            },
                            menu: items,
                            on_menu: {
                                let rig = rig.clone();
                                let (m, p, s) = (entry.module.clone(), entry.name.clone(), snap.clone());
                                move |x: Picked| crate::module_sidebar::module_act(&rig, None, &m, &p, &s, x)
                            },
                        }
                    }
                }
            }
        }
        UsedBy { users: entry.used_by.clone() }
        span { style: "font-size: 11px; color: {FAINT}; line-height: 1.5;",
            "A pick here plays on this patch only. Save edits into a snapshot from the module's bar."
        }
    }
}

/// A preset: each snapshot with the module snapshots it plays; rename (above),
/// duplicate, delete — refused while a patch plays it.
#[component]
fn CompositionDetail(
    preset: signal_guitar_proto::PresetEntry,
    comp: CompositionModel,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut duplicating = use_signal(|| false);
    let siblings: Vec<String> = comp.presets.iter().map(|p| p.name.clone()).collect();
    let playing_here = comp.active_preset.eq_ignore_ascii_case(&preset.name);
    rsx! {
        Actions {
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
            DeleteAct {
                refused: in_use(&preset.used_by),
                on_delete: {
                    let rig = rig.clone();
                    let n = preset.name.clone();
                    move |()| {
                        let n = n.clone();
                        send(&rig, move |r| async move { let _ = r.delete_rig_preset(n).await; });
                    }
                },
            }
        }
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: crate::module_sidebar::next_name(&preset.name, &siblings),
                taken: siblings.clone(),
                on_done: {
                    let rig = rig.clone();
                    let from = preset.name.clone();
                    move |name: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = name {
                            let (from, go) = (from.clone(), n.clone());
                            send(&rig, move |r| async move { let _ = r.duplicate_rig_preset(from, n).await; });
                            on_go.call((Kind::Compositions, go));
                        }
                    }
                },
            }
        }
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
        UsedBy { users: preset.used_by.clone(), none: "No patch plays it — it is auditioned from the Preset sidebar." }
    }
}

/// A block preset: its type and who uses it; rename (above), duplicate,
/// delete — refused while anything puts it on a block.
#[component]
fn BlockPresetDetail(
    preset: signal_guitar_proto::BlockPresetEntry,
    comp: CompositionModel,
    on_go: EventHandler<(Kind, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut duplicating = use_signal(|| false);
    let siblings: Vec<String> = comp.block_presets.iter().map(|b| b.name.clone()).collect();
    let on_blocks: Vec<String> = comp
        .active_blocks
        .iter()
        .filter(|b| b.preset.eq_ignore_ascii_case(&preset.name))
        .map(|b| b.block.clone())
        .collect();
    rsx! {
        Actions {
            Act { label: "Duplicate", onclick: move |()| duplicating.set(true) }
            DeleteAct {
                refused: in_use(&preset.used_by),
                on_delete: {
                    let rig = rig.clone();
                    let n = preset.name.clone();
                    move |()| {
                        let n = n.clone();
                        send(&rig, move |r| async move { let _ = r.delete_block_preset(n).await; });
                    }
                },
            }
        }
        if duplicating() {
            NamePrompt {
                label: "Duplicate",
                initial: crate::module_sidebar::next_name(&preset.name, &siblings),
                taken: siblings.clone(),
                on_done: {
                    let rig = rig.clone();
                    let from = preset.name.clone();
                    move |name: Option<String>| {
                        duplicating.set(false);
                        if let Some(n) = name {
                            let (from, go) = (from.clone(), n.clone());
                            send(&rig, move |r| async move { let _ = r.duplicate_block_preset(from, n).await; });
                            on_go.call((Kind::BlockPresets, go));
                        }
                    }
                },
            }
        }
        Section { label: "Block",
            span { style: "font-size: 12px; color: {MUTED};",
                if preset.bypass { "A {preset.block_type} preset that turns the block off." } else { "A {preset.block_type} preset." }
            }
            span { style: "font-size: 12px; color: {MUTED};",
                if on_blocks.is_empty() {
                    "Not on the playing patch."
                } else {
                    "On the playing patch: {on_blocks.join(\", \")}"
                }
            }
        }
        UsedBy { users: preset.used_by.clone() }
        span { style: "font-size: 11px; color: {FAINT}; line-height: 1.5;",
            "Pick it on a block from the block's sidebar (click the block on the Control view)."
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
            Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules | Kind::BlockPresets => false,
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
                Kind::Compositions | Kind::AmpModules | Kind::DriveModules | Kind::TimeModules | Kind::DelayModules | Kind::ReverbModules | Kind::BlockPresets => {
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
