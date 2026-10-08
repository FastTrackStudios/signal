//! The Browser — the prototype's `views/Browser.tsx` on the rig: where you
//! go through what the rig has — songs, the profile's patches, profiles,
//! presets and their variations, module presets, block presets.
//!
//! It reads like a library: the kinds down the left (each tinted by its
//! colour, saying what is in use there), the kind's things on the right.
//! With something to build into — a section's part picked in Build
//! (Setlist mode), the stack playing (Profile), the preset variation
//! playing (Preset) — a row's tap puts it there; with nothing, it browses
//! (and in Setlist mode a song's tap adds it to the set). Search looks
//! through everything.
//!
//! The rig picks modules and blocks for the patch that is playing
//! (`choose_module`, `choose_block`): a per-section module override is
//! rig work still to come.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LibraryModel, PatchInfo, PerformanceModel};
use signal_widgets::PopupHost;

use super::colors::{name_colour, section_colour, song_colour};
use super::marks::{block_colour, module_colour, InheritIcon, ModuleIcon, OverrideIcon, ProfileIcon};
use super::menu::{open_menu, Item as MenuEntry, MoreButton, Picked};
use super::tokens::*;
use super::BuildPick;
use crate::state::RigViewState;

macro_rules! call {
    ($rig:expr, |$r:ident| $body:expr) => {{
        if let Some($r) = $rig.clone() {
            spawn(async move {
                let _ = $body.await;
            });
        }
    }};
}

const MODULES: [&str; 6] = ["Core", "Amp", "Drive", "Time", "Delay", "Reverb"];
const GENRES: [&str; 7] = ["Worship", "Gospel", "Hymn", "Pop", "Rock", "Country", "R&B"];
const KEY_ORDER: [&str; 17] = ["C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"];

// ── What it builds into ────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
enum Target {
    /// A part of the current song, picked in Build (Setlist mode).
    Part(usize),
    /// The stack playing (Profile mode).
    Stack(usize),
    /// The preset variation playing (Preset mode).
    Preset(String, String),
    None,
}

/// Everything the browser reads, fetched together.
#[derive(Clone, PartialEq, Default)]
struct Data {
    perf: PerformanceModel,
    lib: LibraryModel,
    comp: CompositionModel,
    patches: Vec<PatchInfo>,
}

fn use_data(state: RigViewState) -> Signal<Data> {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut data = use_signal(Data::default);
    use_effect(move || {
        let perf = state.perf.read().clone();
        if let Some(r) = rig.clone() {
            spawn(async move {
                let (lib, comp, patches) = (r.library().await, r.compositions().await, r.patches().await);
                data.set(Data {
                    perf,
                    lib: lib.unwrap_or_default(),
                    comp: comp.unwrap_or_default(),
                    patches: patches.unwrap_or_default(),
                });
            });
        }
    });
    data
}

/// The browser's data, for its rows: a tap applies against what is fresh.
#[derive(Clone, Copy)]
struct BrowserCtx {
    data: Signal<Data>,
}

fn apply_now(rig: Option<RigClient>, ctx: BrowserCtx, build: BuildPick, kind: &Kind, item: &Thing) {
    let d = ctx.data.peek().clone();
    let target = target_of(&d, *build.part.peek());
    apply(rig, kind, &target, &d, item);
}

fn target_of(d: &Data, pick: Option<usize>) -> Target {
    match d.perf.perform_mode {
        2 => pick.filter(|k| *k < d.perf.parts.len()).map_or(Target::None, Target::Part),
        1 => d.perf.stacks.iter().position(|s| s.is_active).map_or(Target::None, Target::Stack),
        _ if !d.comp.active_preset.is_empty() => Target::Preset(d.comp.active_preset.clone(), d.comp.active_snapshot.clone()),
        _ => Target::None,
    }
}

// ── The kinds and their things ─────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
enum Kind {
    Songs,
    Patches,
    Profiles,
    Presets,
    Module(&'static str),
    Block(String),
}

impl Kind {
    fn id(&self) -> String {
        match self {
            Self::Songs => "songs".into(),
            Self::Patches => "patches".into(),
            Self::Profiles => "profiles".into(),
            Self::Presets => "presets".into(),
            Self::Module(m) => format!("module:{m}"),
            Self::Block(b) => format!("block:{b}"),
        }
    }
    fn label(&self) -> String {
        match self {
            Self::Songs => "Songs".into(),
            Self::Patches => "Patches".into(),
            Self::Profiles => "Profiles".into(),
            Self::Presets => "Presets".into(),
            Self::Module(m) => (*m).into(),
            Self::Block(b) if b == "eq" => "EQ".into(),
            Self::Block(b) => {
                let mut c = b.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        }
    }
    fn group(&self) -> &'static str {
        match self {
            Self::Songs => "Library",
            Self::Patches | Self::Profiles | Self::Presets => "Sounds",
            Self::Module(_) => "Modules",
            Self::Block(_) => "Blocks",
        }
    }
    fn colour(&self) -> String {
        match self {
            Self::Songs => "#f472b6".into(),
            Self::Patches => "#38bdf8".into(),
            Self::Profiles => "#a78bfa".into(),
            Self::Presets => "#a1a1aa".into(),
            Self::Module(m) => module_colour(m).into(),
            Self::Block(b) => block_colour(b).into(),
        }
    }
}

fn kinds(d: &Data) -> Vec<Kind> {
    let mut out = vec![Kind::Songs, Kind::Patches, Kind::Profiles, Kind::Presets];
    out.extend(MODULES.iter().map(|m| Kind::Module(m)));
    let mut types: Vec<String> = Vec::new();
    for b in &d.comp.block_presets {
        let t = b.block_type.to_lowercase();
        if !types.contains(&t) {
            types.push(t);
        }
    }
    out.extend(types.into_iter().map(Kind::Block));
    out
}

#[derive(Clone, PartialEq, Debug)]
enum State {
    Playing,
    Swapped,
    In,
}

#[derive(Clone, PartialEq, Debug)]
struct Chip {
    icon: &'static str,
    tint: &'static str,
    bold: String,
    text: String,
}

#[derive(Clone, PartialEq, Debug)]
struct Thing {
    /// What applying it needs (a name; `preset · variation` for nested).
    id: String,
    name: String,
    from: String,
    colour: String,
    state: Option<State>,
    group: String,
    nested: bool,
    chips: Vec<Chip>,
    inherited: String,
    search: String,
    profile_mark: bool,
}

impl Thing {
    fn new(id: impl Into<String>, name: impl Into<String>, colour: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            from: String::new(),
            colour: colour.into(),
            state: None,
            group: String::new(),
            nested: false,
            chips: Vec::new(),
            inherited: String::new(),
            search: String::new(),
            profile_mark: false,
        }
    }
}

/// Who chose each module for the playing preset: its snapshot's picks
/// (chosen by the preset), and theirs in turn (chosen by that module).
fn inherited_from(d: &Data) -> Vec<(String, String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = Vec::new();
    let Some(p) = d.comp.presets.iter().find(|p| p.name == d.comp.active_preset) else { return out };
    let snap = p.snapshots.iter().find(|s| s.name == d.comp.active_snapshot).or_else(|| p.snapshots.first());
    let mut queue: Vec<(String, String, String, String)> =
        snap.map(|s| s.modules.iter().map(|m| (m.module.clone(), m.preset.clone(), m.snapshot.clone(), p.name.clone())).collect()).unwrap_or_default();
    while !queue.is_empty() {
        let (module, preset, snapshot, from) = queue.remove(0);
        if out.iter().any(|o| o.0 == module) {
            continue;
        }
        if let Some(m) = d.comp.modules.iter().find(|m| m.module == module && m.name == preset)
            && let Some(k) = m.snapshots.iter().position(|s| *s == snapshot)
            && let Some(info) = m.snapshot_info.get(k)
        {
            for sub in &info.modules {
                queue.push((sub.module.clone(), sub.preset.clone(), sub.snapshot.clone(), module.clone()));
            }
        }
        out.push((module, preset, snapshot, from));
    }
    out
}

fn things(kind: &Kind, d: &Data, target: &Target, set_songs: &[String]) -> Vec<Thing> {
    let active_patch = d.patches.iter().find(|p| p.active);
    let part_patch = match target {
        Target::Part(k) => d.perf.parts.get(*k).map(|p| p.patch.clone()),
        _ => None,
    };
    match kind {
        Kind::Songs => d
            .lib
            .songs
            .iter()
            .map(|s| {
                let mut t = Thing::new(&s.name, &s.name, song_colour(&s.name, &s.colour));
                t.from = [s.artist.clone(), s.key.clone(), if s.bpm > 0 { format!("{} bpm", s.bpm) } else { String::new() }]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let cols: Vec<String> = d.lib.collections.iter().filter(|c| c.songs.iter().any(|x| x.eq_ignore_ascii_case(&s.name))).map(|c| c.name.clone()).collect();
                t.search = format!("{} {} {}", s.artist, s.genre, cols.join(" ")).to_lowercase();
                if set_songs.iter().any(|x| x.eq_ignore_ascii_case(&s.name)) {
                    t.state = Some(State::In);
                }
                t
            })
            .collect(),
        Kind::Patches => {
            let profile = d.lib.profiles.iter().find(|p| p.active).or_else(|| d.lib.profiles.first());
            profile
                .map(|p| {
                    p.patch_list
                        .iter()
                        .map(|x| {
                            let (tape, _) = crate::perform::folder_color(&x.stack);
                            let mut t = Thing::new(&x.name, &x.name, tape);
                            t.group = x.stack.clone();
                            t.from = p.name.clone();
                            let on = match &part_patch {
                                Some(pp) => pp.eq_ignore_ascii_case(&x.name),
                                None => active_patch.is_some_and(|a| a.name.eq_ignore_ascii_case(&x.name)),
                            };
                            if on {
                                t.state = Some(State::Playing);
                            }
                            t
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        Kind::Profiles => d
            .lib
            .profiles
            .iter()
            .map(|p| {
                let mut t = Thing::new(&p.name, &p.name, name_colour(&p.name));
                t.from = p.stacks.join(" · ");
                t.profile_mark = true;
                if p.active {
                    t.state = Some(State::Playing);
                }
                t
            })
            .collect(),
        Kind::Presets => d
            .comp
            .presets
            .iter()
            .flat_map(|p| {
                p.snapshots.iter().map(move |s| {
                    let mut t = Thing::new(format!("{} · {}", p.name, s.name), &s.name, "#a1a1aa");
                    t.group = p.name.clone();
                    t.nested = true;
                    if d.comp.active_preset == p.name && d.comp.active_snapshot == s.name {
                        t.state = Some(State::Playing);
                    }
                    t
                })
            })
            .collect(),
        Kind::Module(kind) => {
            let colour = module_colour(kind);
            let part_pick = match target {
                Target::Part(k) => d.perf.parts.get(*k).and_then(|p| p.picks.iter().find(|x| x.kind.eq_ignore_ascii_case(kind)).cloned()),
                _ => None,
            };
            let from = inherited_from(d);
            let active = d.comp.active_modules.iter().find(|m| m.module == *kind);
            let overridden = active_patch.is_some_and(|p| p.override_modules.iter().any(|m| m == kind));
            d.comp
                .modules
                .iter()
                .filter(|m| m.module == *kind)
                .flat_map(|m| {
                    let snaps: Vec<String> = if m.snapshots.is_empty() { vec![m.name.clone()] } else { m.snapshots.clone() };
                    let from = from.clone();
                    let part_pick = part_pick.clone();
                    snaps.into_iter().enumerate().map(move |(k, v)| {
                        let mut t = Thing::new(format!("{} · {}", m.name, v), &v, colour);
                        t.group = m.name.clone();
                        t.nested = true;
                        let info = m.snapshot_info.get(k);
                        // An amp or drive says what it loads; a delay, reverb or time what it runs.
                        if let Some(info) = info {
                            if matches!(*kind, "Amp" | "Drive" | "Core") {
                                for cap in &info.captures {
                                    let drive = *kind == "Drive";
                                    t.chips.push(Chip { icon: if drive { "Drive" } else { "Core" }, tint: if drive { "#ef4444" } else { "#D6B36A" }, bold: String::new(), text: cap.clone() });
                                }
                            } else {
                                for b in &info.blocks {
                                    let verb = b.block.to_uppercase().starts_with("VERB");
                                    t.chips.push(Chip { icon: if verb { "Reverb" } else { "Delay" }, tint: if verb { "#8B5CF6" } else { "#3B82F6" }, bold: b.block.clone(), text: b.preset.clone() });
                                }
                            }
                        }
                        let is_active = active.is_some_and(|a| a.preset == m.name && a.snapshot == v);
                        let picked_here = part_pick.as_ref().is_some_and(|p| p.preset == m.name && (p.snapshot == v || p.snapshot.is_empty()));
                        if part_pick.is_some() {
                            // The section's own pick: that, and nothing else, is its.
                            if picked_here {
                                t.state = Some(State::Swapped);
                            }
                        } else if is_active && overridden {
                            t.state = Some(State::Swapped);
                        } else if let Some(f) = from.iter().find(|f| f.0 == *kind && f.1 == m.name && f.2 == v) {
                            t.inherited = f.3.clone();
                        } else if is_active {
                            t.state = Some(State::Playing);
                        }
                        if k == 0 && !m.used_by.is_empty() {
                            t.from = format!("in {} preset{}", m.used_by.len(), if m.used_by.len() == 1 { "" } else { "s" });
                        }
                        t
                    })
                })
                .collect()
        }
        Kind::Block(kind) => {
            let colour = block_colour(kind);
            d.comp
                .block_presets
                .iter()
                .filter(|b| b.block_type.eq_ignore_ascii_case(kind))
                .map(|b| {
                    let mut t = Thing::new(&b.name, &b.name, colour);
                    t.from = if b.bypass {
                        "off".into()
                    } else if b.used_by.is_empty() {
                        String::new()
                    } else {
                        format!("in {} preset{}", b.used_by.len(), if b.used_by.len() == 1 { "" } else { "s" })
                    };
                    let picked = match target {
                        Target::Part(k) => d.perf.parts.get(*k).is_some_and(|p| p.picks.iter().any(|x| x.kind.starts_with("block:") && x.preset == b.name)),
                        _ => false,
                    };
                    if picked {
                        t.state = Some(State::Swapped);
                    } else if let Some(p) = d.comp.active_blocks.iter().find(|p| p.preset == b.name) {
                        t.inherited = p.block.clone();
                    }
                    t
                })
                .collect()
        }
    }
}

// ── The browser ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug, Default)]
struct SongFilter {
    collection: Option<String>,
    artist: Option<String>,
    key: Option<String>,
    genre: Option<String>,
}

#[component]
pub fn Browser(state: RigViewState, on_close: Option<EventHandler<()>>) -> Element {
    let build = use_context::<BuildPick>();
    let data = use_data(state);
    use_context_provider(|| BrowserCtx { data });
    let d = data.read().clone();
    let target = target_of(&d, *build.part.read());
    let home = match (&target, d.perf.perform_mode) {
        (Target::Part(_) | Target::Stack(_), _) => "patches",
        (_, 0) => "module:Core",
        (_, 2) => "songs",
        _ => "patches",
    };
    let mut kind_id = use_signal(|| home.to_string());
    let mut query = use_signal(String::new);
    let mut searching = use_signal(|| false);
    let filter = use_signal(SongFilter::default);
    let all = kinds(&d);
    let kind = all.iter().find(|k| k.id() == kind_id()).cloned().unwrap_or(Kind::Songs);
    let set_songs: Vec<String> = d.perf.songs.iter().map(|s| s.name.clone()).collect();
    let q = query().trim().to_lowercase();
    let search_open = searching() || !q.is_empty();
    let titled = target != Target::None;
    let filters = kind == Kind::Songs;

    // The target, in words.
    let title = match &target {
        Target::Part(k) => {
            let part = d.perf.parts.get(*k).cloned().unwrap_or_default();
            let song = d.perf.songs.get(d.perf.song_index as usize).cloned().unwrap_or_default();
            let section = if part.section.is_empty() { part.name.clone() } else { part.section.clone() };
            rsx! {
                span { style: "font-size: 13px; color: {INK_3};", "For" }
                span { style: "font-size: 15px; font-weight: 650; color: {song_colour(&song.name, &song.colour)};", "{song.name}" }
                span { style: "font-size: 15px; font-weight: 750; color: {section_colour(&section)};", "{section}" }
                if !part.section.is_empty() && part.name != part.section {
                    span { style: "font-size: 14px; font-weight: 650;", "{part.name}" }
                }
            }
        }
        Target::Stack(i) => {
            let st = d.perf.stacks.get(*i).map(|s| s.name.clone()).unwrap_or_default();
            let (tape, _) = crate::perform::folder_color(&st);
            rsx! {
                span { style: "font-size: 13px; color: {INK_3};", "Filling" }
                span { style: "font-size: 15px; font-weight: 650; color: {name_colour(&d.perf.profile_name)};", "{d.perf.profile_name}" }
                span { style: "font-size: 15px; font-weight: 750; color: {lift(tape)};", "{st}" }
            }
        }
        Target::Preset(p, v) => rsx! {
            span { style: "font-size: 13px; color: {INK_3};", "Shaping" }
            span { style: "font-size: 15px; font-weight: 750;", "{p}" }
            span { style: "font-size: 14px; font-weight: 650; color: {INK_2};", "{v}" }
        },
        Target::None => rsx! {
            span { style: "font-size: 17px; font-weight: 750; color: {lift(&kind.colour())};", "{kind.label()}" }
        },
    };
    let search_field = rsx! {
        label { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 12px; border-radius: {R}; background: {FILL};",
            SearchGlyph { size: 15, colour: INK_3 }
            input {
                autofocus: true,
                value: "{query}",
                placeholder: "Search everything",
                style: "flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT};",
                oninput: move |e| query.set(e.value()),
            }
        }
        button {
            style: "height: 44px; padding: 0 10px; flex-shrink: 0; border: none; background: transparent; font-size: 14px; font-weight: 650; color: {INK_2}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| {
                query.set(String::new());
                searching.set(false);
            },
            "Cancel"
        }
    };
    let song_things = things(&Kind::Songs, &d, &target, &set_songs);
    // The picked section's own pick of this kind, to clear.
    let rig_clear = use_hook(try_consume_context::<RigClient>);
    let own_pick: Option<(String, String)> = match (&target, &kind) {
        (Target::Part(k), Kind::Module(_) | Kind::Block(_)) => d.perf.parts.get(*k).and_then(|p| {
            p.picks
                .iter()
                .find(|x| match &kind {
                    Kind::Module(m) => x.kind.eq_ignore_ascii_case(m),
                    Kind::Block(t) => x.kind.starts_with("block:") && d.comp.block_presets.iter().any(|b| b.name == x.preset && b.block_type.eq_ignore_ascii_case(t)),
                    _ => false,
                })
                .map(|x| (p.name.clone(), x.kind.clone()))
        }),
        _ => None,
    };
    let pick_part = build.part;

    rsx! {
        div { style: "height: 100%; min-height: 0; display: flex; flex-direction: column; background: {MAIN}; font-family: {FONT}; color: {INK};",
            // What it's for, the filters, the search — the macro bar's height.
            div { style: "flex-shrink: 0; height: {HEADER_H}px; display: flex; flex-direction: column; justify-content: center; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                div { style: "display: flex; align-items: center; gap: 8px; height: {pick(titled, 42, 48)}px; padding: {pick(titled, \"0 4px 0 14px\", \"0 4px 0 12px\")};",
                    if !titled && search_open {
                        span { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;", {search_field.clone()} }
                    }
                    if !titled && !search_open && filters {
                        SongFilters { lib: d.lib.clone(), songs: song_things.clone(), filter }
                    }
                    if titled || (!search_open && !filters) {
                        span { style: "flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 7px; white-space: nowrap; overflow: hidden;", {title} }
                    }
                    if let Some((part, pick_kind)) = own_pick.clone() {
                        button {
                            style: "height: 44px; padding: 0 12px; border: none; background: transparent; border-radius: {R}; font-size: 13px; font-weight: 700; color: {INK_2}; font-family: {FONT}; cursor: pointer; display: flex; align-items: center; gap: 6px;",
                            onclick: move |_| {
                                let (part, pick_kind) = (part.clone(), pick_kind.clone());
                                call!(rig_clear, |r| r.clear_part_pick(part, pick_kind));
                            },
                            OverrideIcon { colour: kind.colour(), size: 11 }
                            "Clear"
                        }
                    }
                    if let Target::Part(_) = target {
                        button {
                            style: "height: 44px; padding: 0 10px; border: none; background: transparent; border-radius: {R}; font-size: 13px; font-weight: 650; color: {INK_3}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| {
                                let mut p = pick_part;
                                p.set(None);
                            },
                            "Unpick"
                        }
                    }
                    if !searching() {
                        button {
                            "aria-label": "Search",
                            style: "width: 44px; height: 44px; flex-shrink: 0; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
                            onclick: move |_| searching.set(true),
                            SearchGlyph { size: 17, colour: INK_2 }
                        }
                    }
                    if let Some(close) = on_close {
                        button {
                            "aria-label": "Close the browser",
                            style: "width: 44px; height: 44px; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
                            onclick: move |_| close.call(()),
                            svg { width: "13", height: "13", view_box: "0 0 12 12",
                                path { d: "M2 2l8 8M10 2l-8 8", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                            }
                        }
                    }
                }
                if titled && search_open {
                    div { style: "display: flex; align-items: center; gap: 6px; padding: 0 6px 4px 12px;", {search_field.clone()} }
                }
                if titled && !search_open && filters {
                    div { style: "display: flex; align-items: center; padding: 0 6px 4px 12px;",
                        SongFilters { lib: d.lib.clone(), songs: song_things.clone(), filter }
                    }
                }
            }

            div { style: "flex: 1; min-height: 0; display: flex;",
                // The kinds.
                nav { style: "width: 188px; flex-shrink: 0; overflow-y: auto; border-right: 1px solid {RULE}; padding-bottom: 12px;",
                    for (gi, group) in ["Library", "Sounds", "Modules", "Blocks"].iter().enumerate() {
                        div { key: "{gi}",
                            div { style: "padding: 14px 16px 4px; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "{group}" }
                            for k in all.iter().filter(|k| k.group() == *group).cloned() {
                                KindRow { key: "{k.id()}", kind: k.clone(), on: q.is_empty() && k.id() == kind_id(), items: things(&k, &d, &target, &set_songs), onclick: {
                                    let id = k.id();
                                    move |_| {
                                        kind_id.set(id.clone());
                                        query.set(String::new());
                                    }
                                } }
                            }
                        }
                    }
                }
                // The things.
                div { style: "position: relative; flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column;",
                    if !q.is_empty() {
                        SearchResults { d: d.clone(), target: target.clone(), set_songs: set_songs.clone(), q: q.clone() }
                    } else {
                        match kind.clone() {
                            Kind::Songs => rsx! { SongList { d: d.clone(), songs: song_things.clone(), filter, add: d.perf.perform_mode == 2 } },
                            Kind::Profiles => rsx! { ProfileColumns { d: d.clone(), target: target.clone() } },
                            Kind::Presets | Kind::Module(_) => rsx! { PresetColumns { key: "{kind.id()}", kind: kind.clone(), items: things(&kind, &d, &target, &set_songs), applies: applies(&kind, &target, &d) } },
                            k => rsx! { ThingList { kind: k.clone(), items: things(&k, &d, &target, &set_songs), applies: applies(&k, &target, &d) } },
                        }
                    }
                }
            }
        }
    }
}

/// Whether a tap on one of `kind`'s things does something for `target`.
fn applies(kind: &Kind, target: &Target, d: &Data) -> bool {
    match kind {
        Kind::Songs => d.perf.perform_mode == 2,
        Kind::Patches => matches!(target, Target::Part(_)),
        Kind::Profiles => true,
        Kind::Presets => !matches!(target, Target::Part(_)),
        Kind::Module(_) | Kind::Block(_) => *target != Target::None,
    }
}

/// Apply a thing of `kind` to the target: a rig call.
fn apply(rig: Option<RigClient>, kind: &Kind, target: &Target, d: &Data, t: &Thing) {
    let set = d.perf.setlist_index;
    match kind {
        Kind::Songs => {
            let name = t.name.clone();
            call!(rig, |r| r.add_setlist_entry(set, name));
        }
        Kind::Patches => {
            if let Target::Part(k) = target
                && let Some(part) = d.perf.parts.get(*k)
            {
                let (part, patch) = (part.name.clone(), t.name.clone());
                call!(rig, |r| r.set_part_patch(part, patch));
            }
        }
        Kind::Profiles => {
            let name = t.name.clone();
            call!(rig, |r| r.select_profile(name));
        }
        Kind::Presets => {
            let (p, v) = (t.group.clone(), t.name.clone());
            call!(rig, |r| r.choose_preset(p, v));
        }
        Kind::Module(m) => {
            let (m, p, v) = ((*m).to_string(), t.group.clone(), t.name.clone());
            match target {
                // A section picked: the section's own, over its patch.
                Target::Part(k) => {
                    if let Some(part) = d.perf.parts.get(*k).map(|x| x.name.clone()) {
                        call!(rig, |r| r.choose_part_module(part, m, p, v));
                    }
                }
                _ => call!(rig, |r| r.choose_module(m, p, v)),
            }
        }
        Kind::Block(b) => {
            // The chain's block of this type (its first, when there are several).
            let blocks = d.comp.active_blocks.clone();
            let ty = b.clone();
            let preset = t.name.clone();
            let block = blocks
                .iter()
                .find(|p| d.comp.block_presets.iter().any(|bp| bp.name == p.preset && bp.block_type.eq_ignore_ascii_case(&ty)))
                .map(|p| p.block.clone());
            if let Some(block) = block {
                match target {
                    Target::Part(k) => {
                        if let Some(part) = d.perf.parts.get(*k).map(|x| x.name.clone()) {
                            call!(rig, |r| r.choose_part_block(part, block, preset));
                        }
                    }
                    _ => call!(rig, |r| r.choose_block(block, preset)),
                }
            }
        }
    }
}

#[component]
fn SearchGlyph(size: u32, colour: &'static str) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 16 16", style: "flex-shrink: 0;",
            circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: colour, stroke_width: "1.6" }
            path { d: "M10.6 10.6 14 14", stroke: colour, stroke_width: "1.6", stroke_linecap: "round" }
        }
    }
}

/// A kind in the rail: tinted by its colour (stronger when open), and what
/// is in use there now — an override, or what a preset higher up chose.
#[component]
fn KindRow(kind: Kind, on: bool, items: Vec<Thing>, onclick: EventHandler<MouseEvent>) -> Element {
    let colour = kind.colour();
    let cur = items.iter().find(|i| matches!(i.state, Some(State::Playing | State::Swapped))).or_else(|| items.iter().find(|i| !i.inherited.is_empty()));
    let swapped = cur.is_some_and(|c| c.state == Some(State::Swapped));
    let inherited_only = cur.is_some_and(|c| c.state.is_none() && !c.inherited.is_empty());
    let label = cur.map(|c| if c.nested { c.id.clone() } else { c.name.clone() });
    let bg = format!("color-mix(in oklab, {colour} {}%, {MAIN})", if on { 30 } else { 11 });
    let sub_ink = if swapped { format!("color-mix(in oklab, {colour} 45%, {INK_3})") } else { INK_3.to_string() };
    rsx! {
        button {
            style: "position: relative; width: 100%; min-height: {pick(label.is_some(), 52, 44)}px; display: flex; align-items: center; gap: 10px; padding: 4px 14px 4px 16px; margin-bottom: 1px; border: none; text-align: left; background: {bg}; color: {INK}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if on {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            }
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                span { style: "font-size: 15px; font-weight: {pick(on, 700, 600)}; color: {pick(on, INK, INK_2)};", "{kind.label()}" }
                if let Some(l) = label.clone() {
                    span { style: "display: flex; align-items: center; gap: 5px; min-width: 0; font-size: 12px; font-weight: 600; color: {sub_ink};",
                        if swapped { OverrideIcon { colour: colour.clone(), size: 11 } }
                        if inherited_only { InheritIcon { colour: INK_3.to_string(), size: 11 } }
                        span { style: "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{l}" }
                    }
                }
            }
        }
    }
}

/// A kind's things as rows, under their group's heading (a patch's stack).
#[component]
fn ThingList(kind: Kind, items: Vec<Thing>, applies: bool) -> Element {
    let mut last = String::new();
    let rows: Vec<(Option<String>, Thing)> = items
        .into_iter()
        .map(|i| {
            let head = (!i.group.is_empty() && i.group != last).then(|| i.group.clone());
            if !i.group.is_empty() {
                last = i.group.clone();
            }
            (head, i)
        })
        .collect();
    rsx! {
        div { style: "padding-bottom: 16px;",
            for (n, (head, i)) in rows.into_iter().enumerate() {
                div { key: "{n}",
                    if let Some(h) = head {
                        div { style: "display: flex; align-items: center; gap: 8px; padding: 14px 16px 6px;",
                            span { style: "width: 9px; height: 9px; border-radius: 2px; background: {i.colour};" }
                            span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_2};", "{h}" }
                        }
                    }
                    Row { kind: kind.clone(), item: i.clone(), sub: i.from.clone(), applies }
                }
            }
        }
    }
}

/// One thing: its mark, its name, where it comes from, its state.
#[component]
fn Row(kind: Kind, item: Thing, sub: String, applies: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let on = item.state.is_some();
    let swapped = item.state == Some(State::Swapped);
    let bar = if swapped { item.colour.clone() } else { LIVE.to_string() };
    let state_label = match item.state {
        Some(State::Playing) => "Playing",
        Some(State::Swapped) => "Override",
        Some(State::In) => "In",
        None => "",
    };
    let state_ink = if swapped { lift(&item.colour) } else { LIVE.to_string() };
    let (k2, i2) = (kind.clone(), item.clone());
    let (ctx, build) = (use_context::<BrowserCtx>(), use_context::<BuildPick>());
    rsx! {
        button {
            disabled: !applies,
            style: "position: relative; width: 100%; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 16px; border: none; text-align: left; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: {pick(applies, \"pointer\", \"default\")};",
            onclick: move |_| apply_now(rig.clone(), ctx, build, &k2, &i2),
            if on {
                span { style: "position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: {bar};" }
            }
            span { style: "width: 18px; display: flex; justify-content: center; flex-shrink: 0;",
                if item.profile_mark {
                    ProfileIcon { name: item.name.clone(), colour: item.colour.clone(), size: 16 }
                } else {
                    span { style: "width: 10px; height: 10px; border-radius: 3px; background: {item.colour};" }
                }
            }
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                span { style: "font-size: 16px; font-weight: {pick(on, 700, 560)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{item.name}" }
                if !sub.is_empty() {
                    span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{sub}" }
                }
            }
            if !on && !item.inherited.is_empty() {
                span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 650; color: {INK_2}; max-width: 45%; white-space: nowrap; overflow: hidden;",
                    InheritIcon { colour: INK_2.to_string(), size: 11 }
                    "From {item.inherited}"
                }
            }
            if on {
                span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 700; color: {state_ink};",
                    if swapped { OverrideIcon { colour: item.colour.clone(), size: 11 } }
                    "{state_label}"
                }
            }
        }
    }
}

/// Presets on the left, the picked preset's variations on the right — each
/// variation with what it loads or runs.
#[component]
fn PresetColumns(kind: Kind, items: Vec<Thing>, applies: bool) -> Element {
    let mut groups: Vec<(String, String, Vec<Thing>)> = Vec::new();
    for i in items {
        match groups.last_mut() {
            Some(g) if g.0 == i.group => g.2.push(i),
            _ => groups.push((i.group.clone(), i.colour.clone(), vec![i])),
        }
    }
    let in_use = groups
        .iter()
        .find(|g| g.2.iter().any(|i| i.state.is_some()))
        .or_else(|| groups.iter().find(|g| g.2.iter().any(|i| !i.inherited.is_empty())))
        .map(|g| g.0.clone());
    let mut picked = use_signal(|| in_use.clone());
    let current = picked().or(in_use).or_else(|| groups.first().map(|g| g.0.clone()));
    let preset = groups.iter().find(|g| Some(&g.0) == current.as_ref()).cloned();
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; border-top: 1px solid {RULE};",
            div { style: "position: relative; width: 38%; max-width: 240px; flex-shrink: 0; overflow-y: auto; border-right: 1px solid {RULE};",
                for g in groups.iter().cloned() {
                    {
                        let on = Some(&g.0) == current.as_ref();
                        let over = g.2.iter().any(|i| i.state == Some(State::Swapped));
                        let plays = g.2.iter().any(|i| i.state == Some(State::Playing));
                        let from = g.2.iter().any(|i| !i.inherited.is_empty());
                        let wash = format!("color-mix(in oklab, {} 18%, {MAIN})", g.1);
                        let name = g.0.clone();
                        rsx! {
                            button {
                                key: "{g.0}",
                                style: "position: relative; width: 100%; min-height: 46px; display: flex; align-items: center; gap: 8px; padding: 0 12px 0 16px; border: none; text-align: left; background: {pick(on, wash.as_str(), CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| picked.set(Some(name.clone())),
                                if on {
                                    span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {g.1};" }
                                }
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{g.0}" }
                                if over { OverrideIcon { colour: g.1.clone(), size: 12 } }
                                if plays { span { style: "width: 7px; height: 7px; border-radius: 999px; background: {LIVE};" } }
                                if from && !over { InheritIcon { colour: INK_3.to_string(), size: 12 } }
                            }
                        }
                    }
                }
            }
            if let Some((name, colour, vars)) = preset {
                div { style: "position: relative; flex: 1; min-width: 0; overflow-y: auto;",
                    div { style: "display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 8px 16px 6px; background: {MAIN}; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                        span { style: "width: 10px; height: 10px; border-radius: 3px; background: {colour}; flex-shrink: 0;" }
                        span { style: "flex: 1; min-width: 0; font-size: 17px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                    }
                    for v in vars.into_iter() {
                        Variation { key: "{v.id}", kind: kind.clone(), item: v.clone(), colour: colour.clone(), applies }
                    }
                }
            }
        }
    }
}

/// One variation: a state glyph, its name, and what it loads or runs as
/// chips. The one in use sits in a wash of its effect's colour.
#[component]
fn Variation(kind: Kind, item: Thing, colour: String, applies: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let on = item.state.is_some();
    let underneath = !item.inherited.is_empty();
    let swapped = item.state == Some(State::Swapped);
    let bg = if on {
        format!("color-mix(in oklab, {colour} 16%, {MAIN})")
    } else if underneath {
        "rgba(255,255,255,0.025)".to_string()
    } else {
        CLEAR.to_string()
    };
    let bar = if swapped { colour.clone() } else { LIVE.to_string() };
    let (k2, i2) = (kind.clone(), item.clone());
    let (ctx, build) = (use_context::<BrowserCtx>(), use_context::<BuildPick>());
    rsx! {
        button {
            disabled: !applies,
            style: "position: relative; width: 100%; display: flex; align-items: flex-start; gap: 12px; min-height: 52px; padding: 12px 16px; border: none; border-bottom: 1px solid {RULE}; text-align: left; background: {bg}; color: {INK}; font-family: {FONT}; cursor: {pick(applies, \"pointer\", \"default\")};",
            onclick: move |_| apply_now(rig.clone(), ctx, build, &k2, &i2),
            if on {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {bar};" }
            }
            span { style: "width: 16px; height: 20px; flex-shrink: 0; display: flex; align-items: center; justify-content: center;",
                if swapped {
                    OverrideIcon { colour: colour.clone(), size: 14 }
                } else if on {
                    span { style: "width: 9px; height: 9px; border-radius: 999px; background: {LIVE};" }
                } else if underneath {
                    InheritIcon { colour: INK_2.to_string(), size: 13 }
                } else {
                    span { style: "width: 9px; height: 9px; border-radius: 999px; border: 1.5px solid {DIM}; box-sizing: border-box;" }
                }
            }
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 7px;",
                span { style: "display: flex; align-items: baseline; gap: 10px; min-width: 0;",
                    span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: {pick(on, 700, 600)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{item.name}" }
                    if swapped {
                        span { style: "font-size: 12px; font-weight: 700; color: {lift(&colour)}; white-space: nowrap;", "Override" }
                    } else if on {
                        span { style: "font-size: 12px; font-weight: 700; color: {LIVE}; white-space: nowrap;", "Playing" }
                    } else if underneath {
                        span { style: "font-size: 12px; font-weight: 650; color: {INK_3}; white-space: nowrap;", "From {item.inherited}" }
                    }
                }
                if !item.chips.is_empty() {
                    span { style: "display: flex; flex-wrap: wrap; gap: 5px;",
                        for (k, c) in item.chips.iter().cloned().enumerate() {
                            span { key: "{k}", style: "display: inline-flex; align-items: center; gap: 6px; max-width: 100%; height: 24px; padding: 0 8px 0 6px; border-radius: 5px; background: color-mix(in oklab, {c.tint} 10%, #17171b); font-size: 12px; color: {INK_3}; box-sizing: border-box;",
                                ModuleIcon { kind: c.icon.to_string(), size: 11, colour: c.tint.to_string() }
                                span { style: "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    if !c.bold.is_empty() {
                                        span { style: "color: {INK_2}; font-weight: 650;", "{c.bold} " }
                                    }
                                    "{c.text}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Profiles on the left; the picked one's stacks and their patches on the
/// right. With a part picked, a patch row makes the part play it (on that
/// profile when it isn't the song's).
#[component]
fn ProfileColumns(d: Data, target: Target) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let active = d.lib.profiles.iter().find(|p| p.active).map(|p| p.name.clone()).unwrap_or_default();
    let mut picked = use_signal(|| active.clone());
    let current = if picked().is_empty() { active.clone() } else { picked() };
    let profile = d.lib.profiles.iter().find(|p| p.name == current).cloned();
    let part = match &target {
        Target::Part(k) => d.perf.parts.get(*k).cloned(),
        _ => None,
    };
    let song = d.perf.songs.get(d.perf.song_index as usize).map(|s| s.name.clone()).unwrap_or_default();
    rsx! {
        div { style: "height: 100%; display: flex; min-height: 0;",
            div { style: "width: 38%; max-width: 240px; flex-shrink: 0; overflow-y: auto; border-right: 1px solid {RULE};",
                for p in d.lib.profiles.iter().cloned() {
                    {
                        let on = p.name == current;
                        let name = p.name.clone();
                        rsx! {
                            button {
                                key: "{p.name}",
                                style: "position: relative; width: 100%; min-height: 52px; display: flex; align-items: center; gap: 10px; padding: 6px 12px 6px 14px; border: none; text-align: left; background: {pick(on, \"rgba(255,255,255,0.07)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| picked.set(name.clone()),
                                if on {
                                    span { style: "position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 0 2px 2px 0; background: {name_colour(&p.name)};" }
                                }
                                ProfileIcon { name: p.name.clone(), colour: name_colour(&p.name).to_string(), size: 16 }
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 600)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                if p.active {
                                    span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", "Playing" }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(p) = profile {
                div { style: "flex: 1; min-width: 0; overflow-y: auto;",
                    div { style: "display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 8px 12px 8px 16px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                        ProfileIcon { name: p.name.clone(), colour: name_colour(&p.name).to_string(), size: 18 }
                        span { style: "flex: 1; min-width: 0; font-size: 17px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                        if !song.is_empty() && !p.active {
                            button {
                                style: "height: 36px; padding: 0 12px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 13px; font-weight: 700; font-family: {FONT}; white-space: nowrap; cursor: pointer;",
                                onclick: {
                                    let (rig, song, name) = (rig.clone(), song.clone(), p.name.clone());
                                    move |_| {
                                        let (song, name) = (song.clone(), name.clone());
                                        call!(rig, |r| r.set_song_profile(song, name));
                                    }
                                },
                                "Play {song} on it"
                            }
                        }
                    }
                    for st in p.stacks.iter().cloned() {
                        {
                            let (tape, _) = crate::perform::folder_color(&st);
                            let patches: Vec<String> = p.patch_list.iter().filter(|x| x.stack == st).map(|x| x.name.clone()).collect();
                            rsx! {
                                section { key: "{st}", style: "border-bottom: 1px solid {RULE};",
                                    div { style: "display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 6px 16px;",
                                        span { style: "width: 12px; height: 12px; border-radius: 3px; background: {tape}; flex-shrink: 0;" }
                                        span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 750;", "{st}" }
                                    }
                                    for name in patches {
                                        {
                                            let on = part.as_ref().is_some_and(|pp| pp.patch.eq_ignore_ascii_case(&name));
                                            let can = part.is_some();
                                            let (rig, part_name, patch, borrowed) = (rig.clone(), part.as_ref().map(|pp| pp.name.clone()).unwrap_or_default(), name.clone(), (!p.active).then(|| p.name.clone()));
                                            rsx! {
                                                button {
                                                    key: "{name}",
                                                    disabled: !can,
                                                    style: "position: relative; width: 100%; min-height: 44px; display: flex; align-items: center; gap: 10px; padding: 4px 16px 4px 38px; border: none; text-align: left; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: {pick(can, \"pointer\", \"default\")};",
                                                    onclick: move |_| {
                                                        let (pn, pa) = (part_name.clone(), patch.clone());
                                                        call!(rig, |r| r.set_part_patch(pn, pa));
                                                        if let Some(b) = borrowed.clone() {
                                                            let pn = part_name.clone();
                                                            call!(rig, |r| r.set_part_profile(pn, b));
                                                        }
                                                    },
                                                    span { style: "position: absolute; left: 21px; top: 0; bottom: 0; width: 1.5px; background: color-mix(in oklab, {tape} 45%, transparent);" }
                                                    span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; background: {pick(on, tape, CLEAR)}; border: 1.5px solid {tape};" }
                                                    span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 540)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                                                    if on { span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", "Playing" } }
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

#[component]
fn SearchResults(d: Data, target: Target, set_songs: Vec<String>, q: String) -> Element {
    let found: Vec<(Kind, Thing)> = kinds(&d)
        .into_iter()
        .flat_map(|k| {
            things(&k, &d, &target, &set_songs)
                .into_iter()
                .filter(|i| i.name.to_lowercase().contains(&q) || i.group.to_lowercase().contains(&q) || i.search.contains(&q))
                .map(move |i| (k.clone(), i))
                .collect::<Vec<_>>()
        })
        .collect();
    rsx! {
        if found.is_empty() {
            div { style: "padding: 32px 16px; font-size: 13.5px; color: {INK_3}; text-align: center;", "Nothing called “{q}”." }
        }
        for (n, (k, i)) in found.into_iter().enumerate() {
            {
                let shown = if i.nested { Thing { name: i.id.clone(), ..i.clone() } } else { i.clone() };
                let sub = if i.from.is_empty() { k.label() } else { format!("{} · {}", k.label(), i.from) };
                let can = applies(&k, &target, &d);
                rsx! { Row { key: "{n}", kind: k.clone(), item: shown, sub, applies: can } }
            }
        }
    }
}

// ── Songs ──────────────────────────────────────────────────────────────────

fn key_rank(k: &str) -> usize {
    KEY_ORDER.iter().position(|x| *x == k).unwrap_or(99)
}

/// The song filters: a collection (All, or one of the player's) and
/// artist, key and genre — offering only what the library has.
#[component]
fn SongFilters(lib: LibraryModel, songs: Vec<Thing>, filter: Signal<SongFilter>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let f = filter();
    let mut artists: Vec<String> = lib.songs.iter().map(|s| s.artist.clone()).filter(|a| !a.is_empty()).collect();
    artists.sort();
    artists.dedup();
    let mut keys: Vec<String> = lib.songs.iter().map(|s| s.key.clone()).filter(|a| !a.is_empty()).collect();
    keys.sort_by_key(|k| key_rank(k));
    keys.dedup();
    let mut genres: Vec<String> = lib.songs.iter().map(|s| s.genre.clone()).filter(|a| !a.is_empty()).collect();
    genres.sort();
    genres.dedup();
    let _ = songs;
    let col = f.collection.clone().and_then(|c| lib.collections.iter().find(|x| x.name == c).cloned());
    let taken: Vec<String> = lib.collections.iter().map(|c| c.name.clone()).collect();
    let mut tabs: Vec<(Option<String>, String)> = vec![(None, String::new())];
    tabs.extend(lib.collections.iter().map(|c| (Some(c.name.clone()), c.colour.clone())));
    rsx! {
        div { style: "display: flex; align-items: center; gap: 4px; min-width: 0; flex: 1; overflow-x: auto;",
            span { style: "display: flex; align-items: center; flex-shrink: 0; padding: 2px; border-radius: {R}; background: {FILL};",
                for (name, colour) in tabs {
                    {
                        let on = name == f.collection;
                        let n2 = name.clone();
                        rsx! {
                            button {
                                key: "{name.clone().unwrap_or_default()}",
                                style: "height: 40px; display: flex; align-items: center; gap: 6px; padding: 0 12px; border: none; border-radius: 4px; font-size: 14px; font-weight: {pick(on, 750, 600)}; white-space: nowrap; color: {pick(on, INK, INK_3)}; background: {pick(on, FILL_ON, CLEAR)}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| {
                                    let mut fl = filter;
                                    fl.write().collection = n2.clone();
                                },
                                if name.is_some() {
                                    span { style: "width: 7px; height: 7px; border-radius: 999px; background: {colour};" }
                                }
                                "{name.clone().unwrap_or_else(|| \"All\".to_string())}"
                            }
                        }
                    }
                }
                button {
                    "aria-label": "New collection",
                    style: "width: 40px; height: 40px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                    onclick: {
                        let rig = rig.clone();
                        let taken = taken.clone();
                        move |e: MouseEvent| {
                            let (c, el) = (e.client_coordinates(), e.element_coordinates());
                            let rig = rig.clone();
                            open_menu(host, c.x - el.x, c.y - el.y + 44.0, vec![MenuEntry::name("add", "New collection…", "", "Add", taken.clone())], EventHandler::new(move |p: Picked| {
                                let name = p.text.clone();
                                call!(rig, |r| r.set_collection(name, "#a78bfa".to_string(), Vec::new()));
                                let mut fl = filter;
                                fl.write().collection = Some(p.text.clone());
                            }));
                        }
                    },
                    svg { width: "11", height: "11", view_box: "0 0 12 12",
                        path { d: "M6 1v10M1 6h10", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                }
            }
            if let Some(c) = col {
                MoreButton {
                    label: format!("{} actions", c.name),
                    items: vec![
                        MenuEntry::head(c.name.clone()),
                        MenuEntry::name("rename", "Rename…", c.name.clone(), "Rename", taken.clone()),
                        MenuEntry::Sep,
                        MenuEntry::delete("delete", "Delete collection"),
                    ],
                    on_pick: {
                        let rig = rig.clone();
                        let name = c.name.clone();
                        move |p: Picked| {
                            let name = name.clone();
                            let mut fl = filter;
                            match p.id.as_str() {
                                "rename" => {
                                    let new = p.text.clone();
                                    call!(rig, |r| r.rename_collection(name, new));
                                    fl.write().collection = Some(p.text.clone());
                                }
                                "delete" => {
                                    call!(rig, |r| r.delete_collection(name));
                                    fl.write().collection = None;
                                }
                                _ => {}
                            }
                        }
                    },
                }
            }
            span { style: "width: 1px; height: 22px; margin: 0 4px; flex-shrink: 0; background: {RULE_STRONG};" }
            Facet { label: "Artist", value: f.artist.clone(), options: artists, onpick: move |v| { let mut fl = filter; fl.write().artist = v; } }
            Facet { label: "Key", value: f.key.clone(), options: keys, onpick: move |v| { let mut fl = filter; fl.write().key = v; } }
            Facet { label: "Genre", value: f.genre.clone(), options: genres, onpick: move |v| { let mut fl = filter; fl.write().genre = v; } }
        }
    }
}

/// A filter: its name when open to anything, its value (and a clear) when
/// set; the choices open as a menu.
#[component]
fn Facet(label: &'static str, value: Option<String>, options: Vec<String>, onpick: EventHandler<Option<String>>) -> Element {
    let host = PopupHost::try_use();
    let on = value.is_some();
    let mut items = vec![MenuEntry::head(label), MenuEntry::run("__any", format!("Any {}", label.to_lowercase())).checked(!on)];
    items.extend(options.iter().map(|o| MenuEntry::run(o.clone(), o.clone()).checked(value.as_deref() == Some(o.as_str()))));
    let shown = value.clone().unwrap_or_else(|| label.to_string());
    rsx! {
        span { style: "display: flex; align-items: center; flex-shrink: 0; border-radius: {R}; background: {pick(on, FILL_ON, FILL)};",
            button {
                style: "height: 44px; display: flex; align-items: center; gap: 7px; padding: {pick(on, \"0 6px 0 12px\", \"0 10px 0 12px\")}; border: none; background: transparent; font-size: 14px; font-weight: {pick(on, 700, 600)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; font-family: {FONT}; cursor: pointer;",
                onclick: move |e: MouseEvent| {
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    open_menu(host, c.x - el.x, c.y - el.y + 48.0, items.clone(), EventHandler::new(move |p: Picked| {
                        onpick.call(if p.id == "__any" { None } else { Some(p.id.clone()) });
                    }));
                },
                "{shown}"
                if !on {
                    svg { width: "10", height: "6", view_box: "0 0 11 7",
                        path { d: "M1 1l4.5 4.5L10 1", fill: "none", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                }
            }
            if on {
                button {
                    "aria-label": "Clear {label}",
                    style: "width: 36px; height: 44px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                    onclick: move |_| onpick.call(None),
                    svg { width: "10", height: "10", view_box: "0 0 10 10",
                        path { d: "M2 2l6 6M8 2l-6 6", stroke: INK_2, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                }
            }
        }
    }
}

/// The songs the filters let through; a tap adds one to the set (Setlist
/// mode); ⋯ puts it in collections and sets its artist and genre.
#[component]
fn SongList(d: Data, songs: Vec<Thing>, filter: Signal<SongFilter>, add: bool) -> Element {
    let f = filter();
    let col = f.collection.clone().and_then(|c| d.lib.collections.iter().find(|x| x.name == c).cloned());
    let shown: Vec<Thing> = songs
        .into_iter()
        .filter(|t| {
            let s = d.lib.songs.iter().find(|s| s.name == t.name);
            col.as_ref().is_none_or(|c| c.songs.iter().any(|x| x.eq_ignore_ascii_case(&t.name)))
                && f.artist.as_ref().is_none_or(|a| s.is_some_and(|s| &s.artist == a))
                && f.key.as_ref().is_none_or(|k| s.is_some_and(|s| &s.key == k))
                && f.genre.as_ref().is_none_or(|g| s.is_some_and(|s| &s.genre == g))
        })
        .collect();
    let filtered = f != SongFilter::default();
    rsx! {
        div { style: "padding-bottom: 16px;",
            for t in shown.iter().cloned() {
                SongRow { key: "{t.id}", d: d.clone(), item: t.clone(), add }
            }
            if shown.is_empty() {
                div { style: "display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 32px 16px;",
                    span { style: "font-size: 14px; color: {INK_3};",
                        if col.is_some() && f.artist.is_none() && f.key.is_none() && f.genre.is_none() {
                            "{col.as_ref().map(|c| c.name.clone()).unwrap_or_default()} is empty"
                        } else {
                            "No songs match"
                        }
                    }
                    if filtered {
                        button {
                            style: "height: 44px; padding: 0 16px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 14px; font-weight: 700; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| filter.set(SongFilter::default()),
                            "Clear filters"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SongRow(d: Data, item: Thing, add: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let song = d.lib.songs.iter().find(|s| s.name == item.name).cloned().unwrap_or_default();
    let ins: Vec<(String, String)> = d.lib.collections.iter().filter(|c| c.songs.iter().any(|x| x.eq_ignore_ascii_case(&item.name))).map(|c| (c.name.clone(), c.colour.clone())).collect();
    let on = item.state == Some(State::In);
    let taken: Vec<String> = d.lib.collections.iter().map(|c| c.name.clone()).collect();
    let mut items = vec![MenuEntry::head(item.name.clone())];
    for c in &d.lib.collections {
        items.push(MenuEntry::run(format!("col:{}", c.name), c.name.clone()).checked(c.songs.iter().any(|x| x.eq_ignore_ascii_case(&item.name))));
    }
    items.push(MenuEntry::name("newcol", "New collection…", "", "Add", taken));
    items.push(MenuEntry::Sep);
    items.push(MenuEntry::name("artist", if song.artist.is_empty() { "Artist…".to_string() } else { format!("Artist · {}", song.artist) }, song.artist.clone(), "Set", Vec::new()));
    items.push(MenuEntry::head("Genre"));
    for g in GENRES {
        items.push(MenuEntry::run(format!("genre:{g}"), g).checked(song.genre == g));
    }
    let name = item.name.clone();
    let set = d.perf.setlist_index;
    rsx! {
        div { style: "position: relative; display: flex; align-items: center; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)};",
            if on {
                span { style: "position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: {LIVE};" }
            }
            button {
                disabled: !add,
                style: "flex: 1; min-width: 0; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 4px 6px 16px; border: none; text-align: left; background: transparent; color: {INK}; font-family: {FONT}; cursor: {pick(add, \"pointer\", \"default\")};",
                onclick: {
                    let rig = rig.clone();
                    let name = name.clone();
                    move |_| {
                        let name = name.clone();
                        call!(rig, |r| r.add_setlist_entry(set, name));
                    }
                },
                span { style: "width: 10px; height: 10px; border-radius: 3px; flex-shrink: 0; background: {item.colour};" }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                    span { style: "font-size: 16px; font-weight: {pick(on, 700, 560)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{item.name}" }
                    if !item.from.is_empty() {
                        span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{item.from}" }
                    }
                }
                if !ins.is_empty() {
                    span { style: "display: flex; gap: 4px; flex-shrink: 0;",
                        for (cn, cc) in ins {
                            span { key: "{cn}", style: "width: 7px; height: 7px; border-radius: 999px; background: {cc};" }
                        }
                    }
                }
                if on {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 700; color: {LIVE};", "In" }
                }
            }
            MoreButton {
                label: format!("{} actions", item.name),
                items,
                on_pick: {
                    let rig = rig.clone();
                    let name = name.clone();
                    let (artist, genre) = (song.artist.clone(), song.genre.clone());
                    move |p: Picked| {
                        let name = name.clone();
                        if let Some(c) = p.id.strip_prefix("col:") {
                            let c = c.to_string();
                            call!(rig, |r| r.toggle_in_collection(c, name));
                        } else if p.id == "newcol" {
                            let c = p.text.clone();
                            call!(rig, |r| r.set_collection(c, "#a78bfa".to_string(), vec![name]));
                        } else if p.id == "artist" {
                            let (a, g) = (p.text.clone(), genre.clone());
                            call!(rig, |r| r.set_song_info(name, a, g));
                        } else if let Some(g) = p.id.strip_prefix("genre:") {
                            let (a, g) = (artist.clone(), g.to_string());
                            call!(rig, |r| r.set_song_info(name, a, g));
                        }
                    }
                },
            }
        }
    }
}
