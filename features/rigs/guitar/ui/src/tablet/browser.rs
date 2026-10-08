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

use std::rc::Rc;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LibraryModel, PatchInfo, PerformanceModel};
use signal_widgets::PopupHost;

use super::colors::{name_colour, section_colour, song_colour, tape_for, tape_mark, TAPE_GAFFER};
use super::marks::{block_colour, module_colour, InheritIcon, ModuleIcon, OverrideIcon, ProfileIcon, SourceIcon};
use super::menu::{open_menu, open_naming, Item as MenuEntry, MoreButton, Picked, MENU_W};
use super::tokens::*;
use super::BuildPick;
use crate::state::RigViewState;

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

const MODULES: [&str; 6] = ["Core", "Amp", "Drive", "Time", "Delay", "Reverb"];
const GENRES: [&str; 7] = ["Worship", "Gospel", "Hymn", "Pop", "Rock", "Country", "R&B"];
const KEY_ORDER: [&str; 17] = ["C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"];
/// A new collection's colour, by how many there are.
const COLLECTION_COLOURS: [&str; 7] = ["#f472b6", "#a78bfa", "#38bdf8", "#34d399", "#fbbf24", "#fb923c", "#f87171"];

/// The browser's own width below which its kinds and things are steps.
const NARROW_W: f64 = 560.0;
/// A column browser's (presets, profiles) width below which its two
/// columns are steps.
const COLUMNS_NARROW_W: f64 = 480.0;

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

impl Target {
    /// Which target it is and which one — a new one re-centres the lists.
    fn key(&self) -> String {
        match self {
            Self::Part(k) => format!("part:{k}"),
            Self::Stack(i) => format!("stack:{i}"),
            Self::Preset(p, v) => format!("preset:{p}|{v}"),
            Self::None => "none".into(),
        }
    }
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

/// The current song's name.
fn song_now(d: &Data) -> String {
    d.perf.songs.get(d.perf.song_index as usize).map(|s| s.name.clone()).unwrap_or_default()
}

/// The profile a song plays on (the prototype's `profileOf`): its own,
/// else the set's, else the one the rig has active. `song` false: the
/// set's, else the rig's.
fn profile_of(d: &Data, song: bool) -> String {
    if song {
        if !d.perf.song_profile.is_empty() {
            return d.perf.song_profile.clone();
        }
        let name = song_now(d);
        if let Some(p) = d.lib.songs.iter().find(|s| s.name == name).map(|s| s.profile.clone()).filter(|p| !p.is_empty()) {
            return p;
        }
    }
    let set = d.lib.setlists.get(d.perf.setlist_index as usize).or_else(|| d.lib.setlists.iter().find(|s| s.active));
    if let Some(p) = set.map(|s| s.profile.clone()).filter(|p| !p.is_empty()) {
        return p;
    }
    d.lib.profiles.iter().find(|p| p.active).or_else(|| d.lib.profiles.first()).map(|p| p.name.clone()).unwrap_or_default()
}

/// The stack a name belongs to (the prototype's `stackOf`), for its tape:
/// the switch holding it, else the stack a patch of that name is in, else
/// the stack its name says.
fn stack_of(d: &Data, name: &str) -> String {
    let real = |st: &str| d.perf.stacks.iter().any(|s| s.name == st).then(|| st.to_string());
    d.perf
        .stacks
        .iter()
        .find(|s| s.patches.iter().any(|p| p.eq_ignore_ascii_case(name)))
        .map(|s| s.name.clone())
        .or_else(|| d.patches.iter().find(|p| p.name.eq_ignore_ascii_case(name)).and_then(|p| real(&p.stack)))
        .or_else(|| d.lib.profiles.iter().flat_map(|p| p.patch_list.iter()).find(|p| p.name.eq_ignore_ascii_case(name)).and_then(|p| real(&p.stack)))
        .or_else(|| {
            let words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
            d.perf.stacks.iter().find(|s| words.contains(&s.name.to_lowercase())).map(|s| s.name.clone())
        })
        .unwrap_or_default()
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

/// Where a patch in the song's stacks comes from (the prototype's
/// `SourceIcon` props): `song`, `profile` or `other`.
#[derive(Clone, PartialEq, Debug)]
struct Source {
    from: &'static str,
    colour: String,
    profile: Option<String>,
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
    /// A patch of the song's stacks: whose it is, as its mark.
    source: Option<Source>,
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
            source: None,
        }
    }
}

/// A module or block a preset higher up chose: which, and who chose it.
#[derive(Clone, Debug)]
struct Chosen {
    preset: String,
    variation: String,
    from: String,
}

/// A block preset a preset higher up put on a block of a type.
#[derive(Clone, Debug)]
struct ChosenBlock {
    preset: String,
    block: String,
    from: String,
}

/// The preset (and variation) a patch plays.
fn patch_preset(d: &Data, patch: &str) -> (String, String) {
    match d.patches.iter().find(|p| p.name.eq_ignore_ascii_case(patch)) {
        Some(p) if !p.rig_preset.is_empty() => (p.rig_preset.clone(), p.variation.clone()),
        _ => (patch.to_string(), String::new()),
    }
}

/// The target's own sound, as a preset and variation: a part's preset, the
/// first patch of its stack, or its patch; a preset target's own.
fn sound_of(d: &Data, target: &Target) -> Option<(String, String)> {
    match target {
        Target::Part(k) => {
            let part = d.perf.parts.get(*k)?;
            if !part.preset.is_empty() {
                let (p, v) = part.preset.split_once(" · ").unwrap_or((part.preset.as_str(), ""));
                Some((p.to_string(), v.to_string()))
            } else if !part.stack.is_empty() {
                let profile = if part.profile.is_empty() { profile_of(d, true) } else { part.profile.clone() };
                let first = d
                    .lib
                    .profiles
                    .iter()
                    .find(|p| p.name == profile)
                    .and_then(|p| p.patch_list.iter().find(|x| x.song.is_empty() && x.stack == part.stack).map(|x| x.name.clone()))
                    .or_else(|| d.perf.stacks.iter().find(|s| s.name == part.stack).and_then(|s| s.patches.first().cloned()))?;
                Some(patch_preset(d, &first))
            } else if !part.patch.is_empty() {
                Some(patch_preset(d, &part.patch))
            } else {
                None
            }
        }
        Target::Preset(p, v) => Some((p.clone(), v.clone())),
        _ => None,
    }
}

/// What the target inherits (the prototype's `inherited`): module
/// variations, and block presets by block type — each picked by a preset
/// higher up (the preset, its Core, its Time…). Followed down, first
/// choice wins.
fn inherited(d: &Data, target: &Target) -> (Vec<(String, Chosen)>, Vec<(String, Vec<ChosenBlock>)>) {
    let mut modules: Vec<(String, Chosen)> = Vec::new();
    let mut blocks: Vec<(String, Vec<ChosenBlock>)> = Vec::new();
    let Some((preset, variation)) = sound_of(d, target) else { return (modules, blocks) };
    let Some(root) = d.comp.presets.iter().find(|p| p.name == preset) else { return (modules, blocks) };
    let at = root.snapshots.iter().position(|s| s.name == variation).unwrap_or(0);
    let mut queue: Vec<(String, String, String, String)> =
        root.snapshots.get(at).map(|s| s.modules.iter().map(|m| (m.module.clone(), m.preset.clone(), m.snapshot.clone(), root.name.clone())).collect()).unwrap_or_default();
    // The variation's own block picks come first.
    for b in root.snapshots.get(at).map(|s| s.blocks.clone()).unwrap_or_default() {
        let Some(ty) = d.comp.block_presets.iter().find(|p| p.name == b.preset).map(|p| p.block_type.to_lowercase()) else { continue };
        match blocks.iter_mut().find(|x| x.0 == ty) {
            Some(x) => x.1.push(ChosenBlock { preset: b.preset.clone(), block: b.block.clone(), from: root.name.clone() }),
            None => blocks.push((ty, vec![ChosenBlock { preset: b.preset.clone(), block: b.block.clone(), from: root.name.clone() }])),
        }
    }
    while !queue.is_empty() {
        let (module, preset, snapshot, from) = queue.remove(0);
        if modules.iter().any(|o| o.0 == module) {
            continue;
        }
        let comp = d.comp.modules.iter().find(|m| m.module == module && m.name == preset);
        // An unnamed snapshot is the module's first.
        let snapshot = if snapshot.is_empty() { comp.and_then(|m| m.snapshots.first().cloned()).unwrap_or_else(|| preset.clone()) } else { snapshot };
        if let Some(m) = comp
            && let Some(k) = m.snapshots.iter().position(|s| *s == snapshot)
            && let Some(info) = m.snapshot_info.get(k)
        {
            // The module's own block picks (Core: its gate, its comps, its EQ).
            for b in &info.blocks {
                let Some(ty) = d.comp.block_presets.iter().find(|p| p.name == b.preset).map(|p| p.block_type.to_lowercase()) else { continue };
                let at = match blocks.iter().position(|x| x.0 == ty) {
                    Some(i) => i,
                    None => {
                        blocks.push((ty, Vec::new()));
                        blocks.len() - 1
                    }
                };
                if !blocks[at].1.iter().any(|x| x.block == b.block) {
                    blocks[at].1.push(ChosenBlock { preset: b.preset.clone(), block: b.block.clone(), from: module.clone() });
                }
            }
            for sub in &info.modules {
                queue.push((sub.module.clone(), sub.preset.clone(), sub.snapshot.clone(), module.clone()));
            }
        }
        modules.push((module, Chosen { preset, variation: snapshot, from }));
    }
    (modules, blocks)
}

fn plural(n: usize, w: &str) -> String {
    format!("{n} {w}{}", if n == 1 { "" } else { "s" })
}

/// The block chips of a module snapshot — a Time's follow its Delay and
/// Reverb.
fn block_chips(d: &Data, info: &signal_guitar_proto::ModuleSnapshotInfo, follow: bool) -> Vec<Chip> {
    let mut picks: Vec<signal_guitar_proto::BlockPick> = info.blocks.clone();
    if follow {
        for sub in &info.modules {
            if let Some(m) = d.comp.modules.iter().find(|m| m.module == sub.module && m.name == sub.preset) {
                let k = if sub.snapshot.is_empty() { Some(0) } else { m.snapshots.iter().position(|s| *s == sub.snapshot) };
                if let Some(i) = k.and_then(|k| m.snapshot_info.get(k)) {
                    picks.extend(i.blocks.iter().cloned());
                }
            }
        }
    }
    // Each block's algorithm, then its preset (blocks that are off, or
    // run nothing named, left out).
    picks
        .into_iter()
        .filter_map(|b| {
            let algo = algo_of(d, &b.preset).filter(|a| a != "off")?;
            let verb = b.block.to_uppercase().starts_with("VERB");
            Some(Chip { icon: if verb { "Reverb" } else { "Delay" }, tint: if verb { "#8B5CF6" } else { "#3B82F6" }, bold: algo, text: b.preset })
        })
        .collect()
}

/// The algorithm a block preset runs, in the app's names: a delay's style,
/// a reverb's algorithm, a modulation's engine — "off" when it bypasses.
fn algo_of(d: &Data, preset: &str) -> Option<String> {
    let b = d.comp.block_presets.iter().find(|x| x.name == preset)?;
    if b.bypass {
        return Some("off".to_string());
    }
    let at = |param: &str| b.params.iter().find(|p| p.name == param).map(|p| p.value.round().max(0.0) as usize);
    let name = match b.block_type.to_lowercase().as_str() {
        "delay" => crate::control::DELAY_ALGOS.get(at("style")?),
        "reverb" => crate::control::VERB_ALGOS.get(at("algorithm")?),
        "chorus" | "flanger" | "vibrato" => crate::control::MOD_ENGINES.get(at("engine")?),
        _ => None,
    }?;
    Some((*name).to_string())
}

fn things(kind: &Kind, d: &Data, target: &Target, set_songs: &[String]) -> Vec<Thing> {
    let part = match target {
        Target::Part(k) => d.perf.parts.get(*k),
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
            if let Target::Stack(i) = target {
                // Filling a stack: every patch there is, this stack's first,
                // marked where it is in already.
                let Some(st) = d.perf.stacks.get(*i) else { return Vec::new() };
                let mut all = all_patches(d);
                all.sort_by_key(|p| p.1 != st.name);
                return all
                    .into_iter()
                    .map(|(name, stack, from)| {
                        let mut t = Thing::new(format!("{stack}/{name}"), &name, tape_for(&stack));
                        t.group = stack;
                        t.from = from;
                        if st.patches.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
                            t.state = Some(State::In);
                        }
                        t
                    })
                    .collect();
            }
            // The song's stacks as they play it: the song's own patches, the
            // profile's, and those it borrows, each saying whose it is.
            let song = song_now(d);
            let song_ink = d.perf.songs.get(d.perf.song_index as usize).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_else(|| INK_3.to_string());
            // What the part plays, when it plays a patch.
            let now = part.filter(|p| p.stack.is_empty() && p.preset.is_empty()).map(|p| p.patch.clone()).unwrap_or_default();
            let mut out = Vec::new();
            for st in d.perf.stacks.iter().filter(|st| !st.patches.is_empty()) {
                let mut rows: Vec<(u8, Thing)> = st
                    .patches
                    .iter()
                    .map(|name| {
                        let mut t = Thing::new(format!("{}/{name}", st.name), name, tape_for(&st.name));
                        t.group = st.name.clone();
                        let (mine, borrowed) = source_of(&d.lib, &d.perf.profile_name, name);
                        let (rank, from, source) = match (mine, borrowed) {
                            (true, _) => (1, d.perf.profile_name.clone(), Source { from: "profile", colour: INK_3.to_string(), profile: Some(d.perf.profile_name.clone()) }),
                            (false, Some(p)) => (2, format!("from {p}"), Source { from: "other", colour: name_colour(&p).to_string(), profile: Some(p) }),
                            (false, None) => (0, format!("{song}'s own"), Source { from: "song", colour: song_ink.clone(), profile: None }),
                        };
                        t.from = from;
                        t.source = Some(source);
                        if !now.is_empty() && now.eq_ignore_ascii_case(name) {
                            t.state = Some(State::Playing);
                        }
                        (rank, t)
                    })
                    .collect();
                rows.sort_by_key(|r| r.0);
                out.extend(rows.into_iter().map(|r| r.1));
            }
            out
        }
        Kind::Profiles => {
            let on = profile_of(d, part.is_some());
            d.lib
                .profiles
                .iter()
                .map(|p| {
                    let mut t = Thing::new(&p.name, &p.name, name_colour(&p.name));
                    t.from = p.stacks.iter().filter(|st| p.patch_list.iter().any(|x| &x.stack == *st)).cloned().collect::<Vec<_>>().join(" · ");
                    t.profile_mark = true;
                    if p.name == on {
                        t.state = Some(State::Playing);
                    }
                    t
                })
                .collect()
        }
        Kind::Presets => d
            .comp
            .presets
            .iter()
            .flat_map(|p| {
                let colour = tape_mark(&stack_of(d, &p.name));
                p.snapshots.iter().map(move |s| {
                    let id = format!("{} · {}", p.name, s.name);
                    let mut t = Thing::new(&id, &s.name, colour);
                    t.group = p.name.clone();
                    t.nested = true;
                    let playing = part.is_some_and(|x| x.preset == id) || (d.perf.perform_mode == 0 && d.comp.active_preset == p.name && d.comp.active_snapshot == s.name);
                    if playing {
                        t.state = Some(State::Playing);
                    }
                    t
                })
            })
            .collect(),
        Kind::Module(kind) => {
            let colour = module_colour(kind);
            // The part's own pick here. (A preset variation's picks are not
            // on the wire.)
            let swapped = part
                .and_then(|p| p.picks.iter().find(|x| x.kind.eq_ignore_ascii_case(kind)).cloned())
                .or_else(|| variation_picks(d, target).into_iter().find(|x| x.kind.eq_ignore_ascii_case(kind)));
            // What a preset higher up chose here — shown with an override
            // too, so clearing one shows what comes back.
            let chosen = inherited(d, target).0.into_iter().find(|c| c.0 == *kind).map(|c| c.1);
            d.comp
                .modules
                .iter()
                .filter(|m| m.module == *kind)
                .flat_map(|m| {
                    let snaps: Vec<String> = if m.snapshots.is_empty() { vec![m.name.clone()] } else { m.snapshots.clone() };
                    let (swapped, chosen) = (swapped.clone(), chosen.clone());
                    snaps.into_iter().enumerate().map(move |(k, v)| {
                        let mut t = Thing::new(format!("{} · {}", m.name, v), &v, colour);
                        t.group = m.name.clone();
                        t.nested = true;
                        // An amp or drive says what it loads; a delay, reverb
                        // or time what it runs.
                        if let Some(info) = m.snapshot_info.get(k) {
                            match *kind {
                                "Amp" | "Drive" => {
                                    // By role: the amp, the cab, a drive's
                                    // pedal and the option it plays.
                                    for m in &info.models {
                                        let (icon, tint) = match m.role.as_str() {
                                            "amp" => ("Core", "#D6B36A"),
                                            "drive" => ("Drive", "#ef4444"),
                                            _ => ("Amp", "#a1a1aa"),
                                        };
                                        let (bold, text) = if m.role == "drive" && !m.pedal.is_empty() { (m.pedal.clone(), m.option.clone()) } else { (String::new(), m.name.clone()) };
                                        t.chips.push(Chip { icon, tint, bold, text });
                                    }
                                }
                                "Delay" | "Reverb" | "Time" => t.chips = block_chips(d, info, *kind == "Time"),
                                _ => {}
                            }
                        }
                        if swapped.as_ref().is_some_and(|p| p.preset == m.name && p.snapshot == v) {
                            t.state = Some(State::Swapped);
                        }
                        if let Some(c) = chosen.as_ref().filter(|c| c.preset == m.name && c.variation == v) {
                            t.inherited = c.from.clone();
                        }
                        if k == 0 && !m.used_by.is_empty() {
                            t.from = format!("in {}", plural(m.used_by.len(), "preset"));
                        }
                        t
                    })
                })
                .collect()
        }
        Kind::Block(kind) => {
            let colour = block_colour(kind);
            let chosen: Vec<ChosenBlock> = inherited(d, target).1.into_iter().find(|b| b.0 == *kind).map(|b| b.1).unwrap_or_default();
            d.comp
                .block_presets
                .iter()
                .filter(|b| b.block_type.eq_ignore_ascii_case(kind))
                .map(|b| {
                    let mut t = Thing::new(&b.name, &b.name, colour);
                    // Its algorithm first ("Tape · in 3 presets"), or off.
                    t.from = if b.bypass {
                        "off".into()
                    } else {
                        [algo_of(d, &b.name).unwrap_or_default(), if b.used_by.is_empty() { String::new() } else { format!("in {}", plural(b.used_by.len(), "preset")) }]
                            .into_iter()
                            .filter(|x| !x.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    };
                    let picked = |x: &signal_guitar_proto::PartPick| x.kind.starts_with("block:") && x.preset == b.name;
                    if part.is_some_and(|p| p.picks.iter().any(picked)) || variation_picks(d, target).iter().any(picked) {
                        t.state = Some(State::Swapped);
                    }
                    t.inherited = chosen.iter().filter(|c| c.preset == b.name).map(|c| format!("{} · {}", c.from, c.block)).collect::<Vec<_>>().join(", ");
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

/// Measure `el`'s width into `width` — asked again for a moment while it
/// has none yet (mounted, not laid out).
fn measure(el: Rc<MountedData>, mut width: Signal<Option<f64>>) {
    spawn(async move {
        for _ in 0..10 {
            if let Ok(r) = el.get_client_rect().await
                && r.width() > 0.0
            {
                if *width.peek() != Some(r.width()) {
                    width.set(Some(r.width()));
                }
                return;
            }
            architect::platform::sleep(std::time::Duration::from_millis(30)).await;
        }
    });
}

#[component]
pub fn Browser(state: RigViewState, on_close: Option<EventHandler<()>>, narrow: Option<bool>) -> Element {
    // Narrow (a sidebar, or measured under 560): the kinds, then a kind's
    // things, as steps.
    let mut root = use_signal(|| None::<Rc<MountedData>>);
    let width = use_signal(|| None::<f64>);
    let narrow = narrow.unwrap_or(false) || width().is_some_and(|w| w < NARROW_W);
    let mut opened = use_signal(|| false);
    let build = use_context::<BuildPick>();
    let data = use_data(state);
    use_context_provider(|| BrowserCtx { data });
    // Measured again as the rig moves on (a pane opened or closed beside it).
    use_effect(move || {
        let _ = data.read().perf.revision;
        if let Some(el) = root.peek().clone() {
            measure(el, width);
        }
    });
    let d = data.read().clone();
    let target = target_of(&d, *build.part.read());
    let home = match (&target, d.perf.perform_mode) {
        (Target::Part(_) | Target::Stack(_), _) => "patches",
        (_, 0) => "module:Core",
        (_, 2) => "songs",
        _ => "patches",
    };
    let mut kind_id = use_signal(|| home.to_string());
    // A new kind of target, or a new mode, takes the browser to where it
    // builds (a part or a stack: patches; Preset: Core; Setlist: songs).
    let home_key = format!(
        "{}:{}",
        match &target {
            Target::Part(_) => "part",
            Target::Stack(_) => "stack",
            Target::Preset(..) => "preset",
            Target::None => "none",
        },
        d.perf.perform_mode
    );
    let mut last_home = use_signal(|| home_key.clone());
    if *last_home.peek() != home_key {
        last_home.set(home_key.clone());
        kind_id.set(home.to_string());
    }
    let mut query = use_signal(String::new);
    // A block or module picked in Edit's routing opens it here: its kind,
    // and inside, the preset (and variation) it plays.
    let mut reveal = use_context_provider(|| Reveal(Signal::new(None))).0;
    if let Some(super::routing::BrowserFocus(mut focus)) = try_use_context::<super::routing::BrowserFocus>()
        && let Some(at) = focus()
    {
        focus.set(None);
        kind_id.set(at.kind.clone());
        opened.set(true);
        query.set(String::new());
        reveal.set(Some((at.preset, at.variation)));
    }
    let mut searching = use_signal(|| false);
    let filter = use_signal(SongFilter::default);
    let all = kinds(&d);
    let kind = all.iter().find(|k| k.id() == kind_id()).cloned().unwrap_or(Kind::Songs);
    let set_songs: Vec<String> = d.perf.songs.iter().map(|s| s.name.clone()).collect();
    let raw = query();
    let q = raw.trim().to_lowercase();
    let search_open = searching() || !q.is_empty();
    let titled = target != Target::None;
    // The song filters, with Songs open (narrow: once inside it).
    let filters = kind == Kind::Songs && (!narrow || opened());
    // A new list (kind, target, search, step) opens on what is in use.
    let list_key = format!("{}|{}|{}|{}", kind.id(), target.key(), if q.is_empty() { "" } else { "q" }, opened());

    // The target, in words.
    let title = match &target {
        Target::Part(k) => {
            let part = d.perf.parts.get(*k).cloned().unwrap_or_default();
            let song = d.perf.songs.get(d.perf.song_index as usize).cloned().unwrap_or_default();
            let section_of = |p: &signal_guitar_proto::PerfPart| if p.section.is_empty() { p.name.clone() } else { p.section.clone() };
            let section = section_of(&part);
            let parts_in = d.perf.parts.iter().filter(|p| section_of(p) == section).count();
            rsx! {
                span { style: "font-size: 13px; color: {INK_3};", "For" }
                span { style: "font-size: 15px; font-weight: 650; color: {song_colour(&song.name, &song.colour)};", "{song.name}" }
                span { style: "font-size: 15px; font-weight: 750; color: {section_colour(&section)};", "{section}" }
                if parts_in > 1 {
                    span { style: "font-size: 14px; font-weight: 650;", "{part.name}" }
                }
            }
        }
        Target::Stack(i) => {
            let st = d.perf.stacks.get(*i).map(|s| s.name.clone()).unwrap_or_default();
            let tape = tape_for(&st);
            let ink = if tape == TAPE_GAFFER { INK.to_string() } else { lift(tape) };
            rsx! {
                span { style: "font-size: 13px; color: {INK_3};", "Filling" }
                span { style: "font-size: 15px; font-weight: 650; color: {name_colour(&d.perf.profile_name)};", "{d.perf.profile_name}" }
                span { style: "font-size: 15px; font-weight: 750; color: {ink};", "{st}" }
            }
        }
        Target::Preset(p, v) => rsx! {
            span { style: "font-size: 13px; color: {INK_3};", "Shaping" }
            span { style: "font-size: 15px; font-weight: 750;", "{p}" }
            span { style: "font-size: 14px; font-weight: 650; color: {INK_2};", "{v}" }
        },
        Target::None => rsx! {
            if narrow && opened() {
                span { style: "font-size: 15px; font-weight: 700;", "{kind.label()}" }
            }
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
    let rig_clear = use_hook(try_consume_context::<RigClient>);
    let rig_clear_variation = rig_clear.clone();
    // Everything the picked part carries of its own — its picks and its
    // overrides — to clear at once.
    let own: Option<(String, Vec<String>, bool, usize)> = match &target {
        Target::Part(k) => d.perf.parts.get(*k).map(|p| (p.name.clone(), p.picks.iter().map(|x| x.kind.clone()).collect(), !p.overrides.is_empty(), p.picks.len() + p.overrides.len())).filter(|o| o.3 > 0),
        _ => None,
    };
    // The playing variation's picks, to clear at once.
    let own_variation: Option<(String, String, usize)> = match &target {
        Target::Preset(p, v) => Some((p.clone(), v.clone(), variation_picks(&d, &target).len())).filter(|o| o.2 > 0),
        _ => None,
    };
    // ‹ › through the song's parts.
    let step = match &target {
        Target::Part(k) => Some((*k, d.perf.parts.len())),
        _ => None,
    };
    let pick_part = build.part;

    rsx! {
        div {
            style: "height: 100%; min-height: 0; display: flex; flex-direction: column; background: {MAIN}; font-family: {FONT}; color: {INK};",
            onmounted: move |e| {
                let el = e.data();
                root.set(Some(el.clone()));
                measure(el, width);
            },
            // What it's for, the filters, the search — the macro bar's height.
            div { style: "flex-shrink: 0; height: {HEADER_H}px; display: flex; flex-direction: column; justify-content: center; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                div { style: "display: flex; align-items: center; gap: 8px; height: {pick(titled, 42, 48)}px; padding: {pick(titled, \"0 4px 0 14px\", \"0 4px 0 12px\")};",
                    // Narrow, inside a kind: back to them all.
                    if narrow && opened() && q.is_empty() {
                        button {
                            "aria-label": "All kinds",
                            style: "width: 44px; height: 44px; margin-left: -12px; flex-shrink: 0; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
                            onclick: move |_| opened.set(false),
                            svg { width: "9", height: "15", view_box: "0 0 9 15",
                                path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                        }
                    }
                    if !titled && search_open {
                        span { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;", {search_field.clone()} }
                    }
                    if !titled && !search_open && filters {
                        SongFilters { lib: d.lib.clone(), filter }
                    }
                    if titled || (!search_open && !filters) {
                        span { style: "flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 7px; white-space: nowrap; overflow: hidden;", {title} }
                    }
                    // Clear what the part carries of its own, in one go.
                    if let Some((part, kinds, overrides, n)) = own.clone() {
                        button {
                            style: "height: 32px; padding: 0 10px; border: 1px solid {RULE_STRONG}; background: transparent; border-radius: {R}; font-size: 12.5px; font-weight: 700; color: {INK_2}; font-family: {FONT}; cursor: pointer; display: flex; align-items: center; gap: 6px; white-space: nowrap; box-sizing: border-box;",
                            onclick: move |_| {
                                for k in kinds.clone() {
                                    let part = part.clone();
                                    call!(rig_clear, |r| r.clear_part_pick(part, k));
                                }
                                if overrides {
                                    let part = part.clone();
                                    call!(rig_clear, |r| r.set_part_overrides(part, Vec::new()));
                                }
                            },
                            svg { width: "12", height: "12", view_box: "0 0 12 12",
                                path { d: "M2 2l8 8M10 2l-8 8", stroke: INK_2, stroke_width: "1.6", stroke_linecap: "round" }
                            }
                            "Clear {kinds_label(n)}"
                        }
                    }
                    if let Some((p, v, n)) = own_variation.clone() {
                        button {
                            style: "height: 32px; padding: 0 10px; border: 1px solid {RULE_STRONG}; background: transparent; border-radius: {R}; font-size: 12.5px; font-weight: 700; color: {INK_2}; font-family: {FONT}; cursor: pointer; display: flex; align-items: center; gap: 6px; white-space: nowrap; box-sizing: border-box;",
                            onclick: move |_| {
                                let (p, v) = (p.clone(), v.clone());
                                call!(rig_clear_variation, |r| r.clear_variation_picks(p, v));
                            },
                            svg { width: "12", height: "12", view_box: "0 0 12 12",
                                path { d: "M2 2l8 8M10 2l-8 8", stroke: INK_2, stroke_width: "1.6", stroke_linecap: "round" }
                            }
                            "Clear {kinds_label(n)}"
                        }
                    }
                    // Step the part through the song — set a whole song
                    // without going back to the setlist.
                    if let Some((at, n)) = step {
                        span { style: "display: flex; align-items: center;",
                            StepButton { back: true, to: at.checked_sub(1), part: pick_part }
                            span { style: "font-size: 12px; color: {INK_3}; min-width: 34px; text-align: center; font-variant-numeric: tabular-nums;", "{at + 1}/{n}" }
                            StepButton { back: false, to: (at + 1 < n).then_some(at + 1), part: pick_part }
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
                        SongFilters { lib: d.lib.clone(), filter }
                    }
                }
            }

            div { style: "flex: 1; min-height: 0; display: flex;",
                // The kinds (narrow: the first step, the whole width).
                if !narrow || (!opened() && q.is_empty()) {
                nav { style: "width: {pick(narrow, \"100%\", \"188px\")}; flex-shrink: 0; overflow-y: auto; border-right: {pick(narrow, \"none\", RULE_LINE)}; padding-bottom: 12px; box-sizing: border-box;",
                    for (gi, group) in ["Library", "Sounds", "Modules", "Blocks"].iter().enumerate() {
                        div { key: "{gi}",
                            div { style: "padding: 14px 16px 4px; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "{group}" }
                            for k in all.iter().filter(|k| k.group() == *group).cloned() {
                                KindRow { key: "{k.id()}", kind: k.clone(), on: !narrow && q.is_empty() && k.id() == kind_id(), chevron: narrow, items: things(&k, &d, &target, &set_songs), onclick: {
                                    let id = k.id();
                                    move |_| {
                                        kind_id.set(id.clone());
                                        opened.set(true);
                                        query.set(String::new());
                                    }
                                } }
                            }
                        }
                    }
                }
                }
                // The things.
                if !narrow || opened() || !q.is_empty() {
                div { style: "position: relative; flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column;",
                    if !q.is_empty() {
                        SearchResults { key: "{list_key}", d: d.clone(), target: target.clone(), set_songs: set_songs.clone(), q: q.clone(), raw: raw.clone() }
                    } else {
                        match kind.clone() {
                            Kind::Songs => rsx! { SongList { key: "{list_key}", d: d.clone(), songs: song_things.clone(), filter, add: d.perf.perform_mode == 2 } },
                            Kind::Profiles => rsx! { ProfileColumns { d: d.clone(), target: target.clone() } },
                            Kind::Presets | Kind::Module(_) => rsx! { PresetColumns { key: "{kind.id()}", kind: kind.clone(), items: things(&kind, &d, &target, &set_songs), applies: applies(&kind, &target, &d) } },
                            k => rsx! { ThingList { key: "{list_key}", kind: k.clone(), items: things(&k, &d, &target, &set_songs), applies: applies(&k, &target, &d) } },
                        }
                    }
                }
                }
            }
        }
    }
}

/// The chain's block of block type `ty` (its first, when there are
/// several) — where a block preset of that type goes.
fn chain_block(d: &Data, ty: &str) -> Option<String> {
    d.comp
        .active_blocks
        .iter()
        .find(|p| d.comp.block_presets.iter().any(|bp| bp.name == p.preset && bp.block_type.eq_ignore_ascii_case(ty)))
        .map(|p| p.block.clone())
}

/// Whether a tap on one of `kind`'s things does something for `target`.
fn applies(kind: &Kind, target: &Target, d: &Data) -> bool {
    let part = matches!(target, Target::Part(_));
    match kind {
        Kind::Songs => d.perf.perform_mode == 2,
        Kind::Patches => matches!(target, Target::Part(_) | Target::Stack(_)),
        // The song's profile for a part; the set's in Setlist or Profile mode.
        Kind::Profiles => part || d.perf.perform_mode != 0,
        Kind::Presets => part || d.perf.perform_mode == 0,
        Kind::Module(_) => matches!(target, Target::Part(_) | Target::Preset(..)),
        Kind::Block(b) => matches!(target, Target::Part(_) | Target::Preset(..)) && chain_block(d, b).is_some(),
    }
}

/// Apply a thing of `kind` to the target: a rig call.
fn apply(rig: Option<RigClient>, kind: &Kind, target: &Target, d: &Data, t: &Thing) {
    if !applies(kind, target, d) {
        return;
    }
    let set = d.perf.setlist_index;
    match kind {
        Kind::Songs => {
            let name = t.name.clone();
            call!(rig, |r| r.add_setlist_entry(set, name));
        }
        Kind::Patches => match target {
            Target::Part(k) => {
                if let Some(part) = d.perf.parts.get(*k) {
                    let (part, patch) = (part.name.clone(), t.name.clone());
                    call!(rig, |r| r.set_part_patch(part, patch));
                }
            }
            // In the stack already: a tap takes it out; else in it goes.
            Target::Stack(i) => {
                if let Some(st) = d.perf.stacks.get(*i) {
                    let (stack, patch, on) = (st.name.clone(), t.name.clone(), t.state != Some(State::In));
                    call!(rig, |r| r.set_stack_patch(stack, patch, on));
                }
            }
            _ => {}
        },
        Kind::Profiles => {
            let name = t.name.clone();
            match target {
                Target::Part(_) => {
                    let song = song_now(d);
                    call!(rig, |r| r.set_song_profile(song, name));
                }
                _ => call!(rig, |r| r.set_setlist_profile(set, name)),
            }
        }
        Kind::Presets => {
            let (p, v) = (t.group.clone(), t.name.clone());
            match target {
                // A section plays the preset's variation.
                Target::Part(k) => {
                    if let Some(part) = d.perf.parts.get(*k).map(|x| x.name.clone()) {
                        call!(rig, |r| r.set_part_preset(part, p, v));
                    }
                }
                _ => call!(rig, |r| r.choose_preset(p, v)),
            }
        }
        Kind::Module(m) => {
            let (m, p, v) = ((*m).to_string(), t.group.clone(), t.name.clone());
            match target {
                // A section picked: the section's own, over its sound.
                Target::Part(k) => {
                    if let Some(part) = d.perf.parts.get(*k).map(|x| x.name.clone()) {
                        // Picking what is inherited anyway clears the override.
                        if !t.inherited.is_empty() {
                            call!(rig, |r| r.clear_part_pick(part, m));
                        } else {
                            call!(rig, |r| r.choose_part_module(part, m, p, v));
                        }
                    }
                }
                // The playing variation's pick (tapping what it chose itself
                // clears it).
                Target::Preset(pp, vv) => {
                    let (pp, vv) = (pp.clone(), vv.clone());
                    let value = if t.inherited.is_empty() { format!("{p} · {v}") } else { String::new() };
                    call!(rig, |r| r.set_variation_pick(pp, vv, m, value));
                }
                _ => call!(rig, |r| r.choose_module(m, p, v)),
            }
        }
        Kind::Block(b) => {
            let Some(block) = chain_block(d, b) else { return };
            let preset = t.name.clone();
            match target {
                Target::Part(k) => {
                    if let Some(part) = d.perf.parts.get(*k).map(|x| x.name.clone()) {
                        call!(rig, |r| r.choose_part_block(part, block, preset));
                    }
                }
                Target::Preset(pp, vv) => {
                    let (pp, vv) = (pp.clone(), vv.clone());
                    let value = if t.inherited.is_empty() { preset } else { String::new() };
                    let kind = format!("block:{block}");
                    call!(rig, |r| r.set_variation_pick(pp, vv, kind, value));
                }
                _ => call!(rig, |r| r.choose_block(block, preset)),
            }
        }
    }
}

/// Bring a row into the middle of its list — the one in use, when a list
/// opens on it further down.
fn into_view(e: MountedEvent) {
    let el = e.data();
    spawn(async move {
        let _ = el.scroll_to(ScrollBehavior::Instant).await;
    });
}

/// The first of `items` in use (its state, or inherited): the one a list
/// opens on.
fn first_current(items: &[Thing]) -> Option<usize> {
    items.iter().position(|i| i.state.is_some() || !i.inherited.is_empty())
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
fn KindRow(kind: Kind, on: bool, chevron: bool, items: Vec<Thing>, onclick: EventHandler<MouseEvent>) -> Element {
    let colour = kind.colour();
    let cur = items.iter().find(|i| matches!(i.state, Some(State::Playing | State::Swapped))).or_else(|| items.iter().find(|i| !i.inherited.is_empty()));
    let swapped = cur.is_some_and(|c| c.state == Some(State::Swapped));
    let inherited_only = cur.is_some_and(|c| c.state.is_none() && !c.inherited.is_empty());
    let label = cur.map(|c| if c.nested { c.id.clone() } else { c.name.clone() });
    let bg = format!("color-mix(in oklab, {colour} {}%, {MAIN})", if on { 30 } else { 11 });
    let sub_ink = if swapped { format!("color-mix(in oklab, {colour} 45%, {INK_3})") } else { INK_3.to_string() };
    rsx! {
        button {
            style: "position: relative; width: 100%; min-height: {pick(label.is_some(), 52, 44)}px; display: flex; align-items: center; justify-content: flex-start; gap: 10px; padding: 4px 14px 4px 16px; margin-bottom: 1px; border: none; text-align: left; background: {bg}; color: {INK}; font-family: {FONT}; cursor: pointer;",
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
            if chevron {
                svg { width: "7", height: "12", view_box: "0 0 7 12", style: "flex-shrink: 0;",
                    path { d: "M1 1l5 5-5 5", fill: "none", stroke: INK_3, stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
        }
    }
}

/// A kind's things as rows, under their group's heading (a patch's stack).
#[component]
fn ThingList(kind: Kind, items: Vec<Thing>, applies: bool) -> Element {
    // The row a routing pick points at, else the first in use.
    let revealed = try_use_context::<Reveal>().and_then(|r| r.0.peek().clone()).and_then(|(p, _)| items.iter().position(|i| !p.is_empty() && i.name == p));
    let first = revealed.or_else(|| first_current(&items));
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
                div { key: "{i.id}",
                    if let Some(h) = head {
                        div { style: "display: flex; align-items: center; gap: 8px; padding: 14px 16px 6px;",
                            span { style: "width: 9px; height: 9px; border-radius: 2px; background: {tape_mark(&h)};" }
                            span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_2};", "{h}" }
                        }
                    }
                    Row { kind: kind.clone(), item: i.clone(), sub: i.from.clone(), applies, scroll: first == Some(n) }
                }
            }
        }
    }
}

/// One thing: its mark, its name, where it comes from, its state. A
/// variation (found by search) sits indented on its preset's guide line.
#[component]
fn Row(kind: Kind, item: Thing, sub: String, applies: bool, scroll: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let on = item.state.is_some();
    let nested = item.nested;
    let swapped = item.state == Some(State::Swapped);
    let bar = if swapped { item.colour.clone() } else { LIVE.to_string() };
    let state_label = match item.state {
        Some(State::Playing) => "Playing",
        Some(State::Swapped) => "Override",
        Some(State::In) => "In",
        None => "",
    };
    let state_ink = if swapped { format!("color-mix(in oklab, {} 70%, white)", item.colour) } else { LIVE.to_string() };
    let mark = if item.colour == TAPE_GAFFER { INK_3.to_string() } else { item.colour.clone() };
    let guide = format!("color-mix(in oklab, {} 45%, transparent)", item.colour);
    let dot = if on { item.colour.clone() } else { CLEAR.to_string() };
    let weight = if on { 700 } else if nested { 520 } else { 560 };
    let name_ink = if nested && !on { INK_2 } else { INK };
    let (k2, i2) = (kind.clone(), item.clone());
    let (ctx, build) = (use_context::<BrowserCtx>(), use_context::<BuildPick>());
    rsx! {
        button {
            disabled: !applies,
            style: "position: relative; width: 100%; min-height: {pick(nested, 46, 56)}px; display: flex; align-items: center; justify-content: flex-start; gap: 12px; padding: {pick(nested, \"4px 16px 4px 38px\", \"6px 16px\")}; border: none; text-align: left; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: {pick(applies, \"pointer\", \"default\")};",
            onclick: move |_| apply_now(rig.clone(), ctx, build, &k2, &i2),
            onmounted: move |e| if scroll { into_view(e) },
            if on {
                span { style: "position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: {bar};" }
            }
            if nested {
                span { style: "position: absolute; left: 21px; top: 0; bottom: 0; width: 1.5px; background: {guide};" }
            }
            span { style: "width: {pick(nested, 10, 18)}px; display: flex; justify-content: center; flex-shrink: 0;",
                if nested {
                    span { style: "width: 8px; height: 8px; border-radius: 999px; box-sizing: border-box; background: {dot}; border: 1.5px solid {item.colour};" }
                } else if item.profile_mark {
                    ProfileIcon { name: item.name.clone(), colour: item.colour.clone(), size: 16 }
                } else if let Some(s) = item.source.clone() {
                    SourceIcon { key: "{s.from}{s.colour}", from: s.from, colour: s.colour, size: 15, profile: s.profile }
                } else {
                    span { style: "width: 10px; height: 10px; border-radius: 3px; background: {mark};" }
                }
            }
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                span { style: "font-size: {pick(nested, 15, 16)}px; font-weight: {weight}; color: {name_ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{item.name}" }
                if !sub.is_empty() {
                    span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{sub}" }
                }
            }
            if !on && !item.inherited.is_empty() {
                span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 650; color: {INK_2}; max-width: 45%; min-width: 0; overflow: hidden;",
                    InheritIcon { colour: INK_2.to_string(), size: 11 }
                    span { style: "min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "From {item.inherited}" }
                }
            }
            if on {
                span { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 700; color: {state_ink};",
                    if swapped { OverrideIcon { key: "{item.colour}", colour: item.colour.clone(), size: 11 } }
                    "{state_label}"
                }
            }
        }
    }
}

/// Presets on the left, the picked preset's variations on the right — each
/// variation with what it loads or runs. Narrow, the two become steps.
#[component]
fn PresetColumns(kind: Kind, items: Vec<Thing>, applies: bool) -> Element {
    let mut groups: Vec<(String, String, Vec<Thing>)> = Vec::new();
    for i in items {
        match groups.last_mut() {
            Some(g) if g.0 == i.group => g.2.push(i),
            _ => groups.push((i.group.clone(), i.colour.clone(), vec![i])),
        }
    }
    // The preset it opens on, fixed when it opens: the one in use, else the
    // first.
    let in_use = groups
        .iter()
        .find(|g| g.2.iter().any(|i| i.state.is_some()))
        .or_else(|| groups.iter().find(|g| g.2.iter().any(|i| !i.inherited.is_empty())))
        .or_else(|| groups.first())
        .map(|g| g.0.clone());
    // A routing pick: its preset open, on its variations.
    let revealed = try_use_context::<Reveal>().and_then(|r| r.0.peek().clone()).filter(|(p, _)| groups.iter().any(|g| g.0 == *p));
    let mut picked = use_signal(|| revealed.as_ref().map(|r| r.0.clone()).or(in_use));
    let mut variations = use_signal(|| revealed.is_some());
    let mut seen = use_signal(|| revealed.clone());
    if let Some(r) = try_use_context::<Reveal>().and_then(|r| r.0.read().clone())
        && groups.iter().any(|g| g.0 == r.0)
        && *seen.peek() != Some(r.clone())
    {
        seen.set(Some(r.clone()));
        picked.set(Some(r.0.clone()));
        variations.set(true);
    }
    let reveal_variation = seen().map(|r| r.1).unwrap_or_default();
    let width = use_signal(|| None::<f64>);
    let narrow = width().is_some_and(|w| w < COLUMNS_NARROW_W);
    let current = groups.iter().find(|g| Some(&g.0) == picked().as_ref()).or_else(|| groups.first()).map(|g| g.0.clone());
    let preset = groups.iter().find(|g| Some(&g.0) == current.as_ref()).cloned();
    let show_presets = !narrow || !variations();
    let show_variations = !narrow || variations();
    rsx! {
        div {
            style: "flex: 1; min-height: 0; display: flex; border-top: 1px solid {RULE};",
            onmounted: move |e| measure(e.data(), width),
            if show_presets {
                div { style: "position: relative; width: {pick(narrow, \"100%\", \"38%\")}; max-width: {pick(narrow, \"none\", \"240px\")}; flex-shrink: 0; overflow-y: auto; border-right: {pick(narrow, \"none\", RULE_LINE)};",
                    for g in groups.iter().cloned() {
                        {
                            let on = Some(&g.0) == current.as_ref();
                            let lit = on && !narrow;
                            let over = g.2.iter().any(|i| i.state == Some(State::Swapped));
                            let plays = g.2.iter().any(|i| i.state == Some(State::Playing));
                            let from = g.2.iter().any(|i| !i.inherited.is_empty());
                            let wash = format!("color-mix(in oklab, {} 18%, {MAIN})", g.1);
                            let name = g.0.clone();
                            rsx! {
                                button {
                                    key: "{g.0}",
                                    style: "position: relative; width: 100%; min-height: 46px; display: flex; align-items: center; justify-content: flex-start; gap: 8px; padding: 0 12px 0 16px; border: none; text-align: left; background: {pick(lit, wash.as_str(), CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        picked.set(Some(name.clone()));
                                        variations.set(true);
                                    },
                                    // The preset in use, brought into view.
                                    onmounted: move |e| if on { into_view(e) },
                                    if lit {
                                        span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {g.1};" }
                                    }
                                    span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{g.0}" }
                                    if over { OverrideIcon { key: "{g.1}", colour: g.1.clone(), size: 12 } }
                                    if plays { span { style: "width: 7px; height: 7px; border-radius: 999px; background: {LIVE};" } }
                                    if from && !over { InheritIcon { colour: INK_3.to_string(), size: 12 } }
                                    if narrow {
                                        svg { width: "7", height: "12", view_box: "0 0 7 12", style: "flex-shrink: 0;",
                                            path { d: "M1 1l5 5-5 5", fill: "none", stroke: INK_3, stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if show_variations {
                if let Some((name, colour, vars)) = preset.clone() {
                div { style: "position: relative; flex: 1; min-width: 0; overflow-y: auto;",
                    div { style: "position: sticky; top: 0; z-index: 1; display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 8px 16px 6px; background: {MAIN}; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                        if narrow {
                            button {
                                "aria-label": "All presets",
                                style: "width: 32px; height: 44px; margin-left: -8px; flex-shrink: 0; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
                                onclick: move |_| variations.set(false),
                                svg { width: "9", height: "15", view_box: "0 0 9 15",
                                    path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                                }
                            }
                        }
                        span { style: "width: 10px; height: 10px; border-radius: 3px; background: {colour}; flex-shrink: 0;" }
                        span { style: "flex: 1; min-width: 0; font-size: 17px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                    }
                    {
                        // The variation a routing pick points at, else the first in use.
                        let first = vars.iter().position(|v| !reveal_variation.is_empty() && v.name == reveal_variation).or_else(|| first_current(&vars));
                        rsx! {
                            for (n, v) in vars.into_iter().enumerate() {
                                Variation { key: "{v.id}", kind: kind.clone(), item: v.clone(), colour: colour.clone(), applies, scroll: first == Some(n) }
                            }
                        }
                    }
                }
            }
            }
        }
    }
}

/// One variation: a state glyph, its name, and what it loads or runs as
/// chips. The one in use sits in a wash of its effect's colour.
#[component]
fn Variation(kind: Kind, item: Thing, colour: String, applies: bool, scroll: bool) -> Element {
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
    let override_ink = format!("color-mix(in oklab, {colour} 45%, {INK_2})");
    let (k2, i2) = (kind.clone(), item.clone());
    let (ctx, build) = (use_context::<BrowserCtx>(), use_context::<BuildPick>());
    rsx! {
        button {
            disabled: !applies,
            style: "position: relative; width: 100%; display: flex; align-items: flex-start; justify-content: flex-start; gap: 12px; min-height: 52px; padding: 12px 16px; border: none; border-bottom: 1px solid {RULE}; text-align: left; background: {bg}; color: {INK}; font-family: {FONT}; cursor: {pick(applies, \"pointer\", \"default\")};",
            onclick: move |_| apply_now(rig.clone(), ctx, build, &k2, &i2),
            onmounted: move |e| if scroll { into_view(e) },
            if on {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {bar};" }
            }
            span { style: "width: 16px; height: 20px; flex-shrink: 0; display: flex; align-items: center; justify-content: center;",
                if swapped {
                    OverrideIcon { key: "{colour}", colour: colour.clone(), size: 14 }
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
                        span { style: "font-size: 12px; font-weight: 700; color: {override_ink}; white-space: nowrap;", "Override" }
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
/// right. With a part picked, a stack row makes the part play that stack
/// and a patch row that patch — borrowed when the profile isn't the
/// song's; a button gives the whole song the profile. Filling a stack
/// (Profile mode), a patch row puts the patch in or takes it out. Narrow,
/// the two become steps.
#[component]
fn ProfileColumns(d: Data, target: Target) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let song_profile = profile_of(&d, true);
    let mut picked = use_signal(|| song_profile.clone());
    let mut stacks = use_signal(|| false);
    let width = use_signal(|| None::<f64>);
    let narrow = width().is_some_and(|w| w < COLUMNS_NARROW_W);
    let current = picked();
    let profile = d.lib.profiles.iter().find(|p| p.name == current).cloned();
    let part = match &target {
        Target::Part(k) => d.perf.parts.get(*k).cloned(),
        _ => None,
    };
    let song = song_now(&d);
    // Not the song's profile: what a part picks from it is borrowed.
    let borrowed = (current != song_profile).then(|| current.clone());
    // Filling a stack (Profile mode): a patch row puts it in or takes it out.
    let fill = match &target {
        Target::Stack(i) => d.perf.stacks.get(*i).cloned(),
        _ => None,
    };
    // The profile's stacks that hold its own patches (a song's own, merged
    // in while it plays, are not the profile's).
    let defs: Vec<(String, Vec<String>)> = profile
        .as_ref()
        .map(|p| {
            p.stacks
                .iter()
                .map(|st| (st.clone(), p.patch_list.iter().filter(|x| x.song.is_empty() && &x.stack == st).map(|x| x.name.clone()).collect::<Vec<_>>()))
                .filter(|s| !s.1.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let show_profiles = !narrow || !stacks();
    let show_stacks = !narrow || stacks();
    rsx! {
        div {
            style: "height: 100%; display: flex; min-height: 0;",
            onmounted: move |e| measure(e.data(), width),
            if show_profiles {
                div { style: "width: {pick(narrow, \"100%\", \"38%\")}; max-width: {pick(narrow, \"none\", \"240px\")}; flex-shrink: 0; overflow-y: auto; border-right: {pick(narrow, \"none\", RULE_LINE)};",
                    for p in d.lib.profiles.iter().cloned() {
                        {
                            let on = p.name == current;
                            let lit = on && !narrow;
                            let name = p.name.clone();
                            let theirs = p.name == song_profile;
                            rsx! {
                                button {
                                    key: "{p.name}",
                                    style: "position: relative; width: 100%; min-height: 52px; display: flex; align-items: center; justify-content: flex-start; gap: 10px; padding: 6px 12px 6px 14px; border: none; text-align: left; background: {pick(lit, \"rgba(255,255,255,0.07)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        picked.set(name.clone());
                                        stacks.set(true);
                                    },
                                    if lit {
                                        span { style: "position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 0 2px 2px 0; background: {name_colour(&p.name)};" }
                                    }
                                    ProfileIcon { name: p.name.clone(), colour: name_colour(&p.name).to_string(), size: 16 }
                                    span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 600)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.name}" }
                                    if theirs {
                                        span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", if part.is_some() { "The song's" } else { "Playing" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if show_stacks {
                div { style: "flex: 1; min-width: 0; overflow-y: auto;",
                    div { style: "display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 8px 12px 8px 16px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                        if narrow {
                            button {
                                "aria-label": "All profiles",
                                style: "width: 32px; height: 44px; margin-left: -8px; flex-shrink: 0; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
                                onclick: move |_| stacks.set(false),
                                svg { width: "9", height: "15", view_box: "0 0 9 15",
                                    path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                                }
                            }
                        }
                        ProfileIcon { key: "{current}", name: current.clone(), colour: name_colour(&current).to_string(), size: 18 }
                        span { style: "flex: 1; min-width: 0; font-size: 17px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{current}" }
                        if part.is_some() && borrowed.is_some() {
                            button {
                                style: "height: 36px; padding: 0 12px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 13px; font-weight: 700; font-family: {FONT}; white-space: nowrap; cursor: pointer; box-sizing: border-box;",
                                onclick: {
                                    let (rig, song, name) = (rig.clone(), song.clone(), current.clone());
                                    move |_| {
                                        let (song, name) = (song.clone(), name.clone());
                                        call!(rig, |r| r.set_song_profile(song, name));
                                    }
                                },
                                "Play {song} on it"
                            }
                        }
                    }
                    for (st, patches) in defs {
                        {
                            let tape = tape_mark(&st);
                            let guide = format!("color-mix(in oklab, {tape} 45%, transparent)");
                            // The part plays this stack, on this profile.
                            let stack_on = part.as_ref().is_some_and(|pp| pp.stack == st && (if pp.profile.is_empty() { &song_profile } else { &pp.profile }) == &current);
                            let stack_bg = if stack_on { format!("color-mix(in oklab, {tape} 14%, transparent)") } else { CLEAR.to_string() };
                            let had_profile = part.as_ref().is_some_and(|pp| !pp.profile.is_empty());
                            let stack_click = {
                                let (rig, part_name, borrowed, st) = (rig.clone(), part.as_ref().map(|pp| pp.name.clone()), borrowed.clone(), st.clone());
                                move |_| {
                                    let Some(pn) = part_name.clone() else { return };
                                    let stack = st.clone();
                                    call!(rig, |r| r.set_part_stack(pn, stack));
                                    // Its profile: the one it is borrowed from, or
                                    // back to the song's.
                                    if borrowed.is_some() || had_profile {
                                        let (pn, b) = (part_name.clone().unwrap_or_default(), borrowed.clone().unwrap_or_default());
                                        call!(rig, |r| r.set_part_profile(pn, b));
                                    }
                                }
                            };
                            rsx! {
                                section { key: "{st}", style: "border-bottom: 1px solid {RULE};",
                                    button {
                                        disabled: part.is_none(),
                                        style: "position: relative; width: 100%; display: flex; align-items: center; justify-content: flex-start; gap: 10px; min-height: 48px; padding: 6px 16px; border: none; text-align: left; background: {stack_bg}; color: {INK}; font-family: {FONT}; cursor: {pick(part.is_some(), \"pointer\", \"default\")};",
                                        onclick: stack_click,
                                        if stack_on {
                                            span { style: "position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 0 2px 2px 0; background: {LIVE};" }
                                        }
                                        span { style: "width: 12px; height: 12px; border-radius: 3px; background: {tape}; flex-shrink: 0;" }
                                        span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 750;", "{st}" }
                                        if part.is_some() {
                                            span { style: "font-size: 12px; font-weight: 700; color: {pick(stack_on, LIVE, INK_3)};", if stack_on { "Plays the stack" } else { "Whole stack" } }
                                        }
                                    }
                                    for name in patches {
                                        {
                                            let in_fill = fill.as_ref().is_some_and(|f| f.patches.iter().any(|x| x.eq_ignore_ascii_case(&name)));
                                            let on = part.as_ref().is_some_and(|pp| pp.stack.is_empty() && pp.preset.is_empty() && pp.patch.eq_ignore_ascii_case(&name)) || in_fill;
                                            let can = part.is_some() || fill.is_some();
                                            let fill_stack = fill.as_ref().map(|f| f.name.clone());
                                            let dot = if on { tape.to_string() } else { CLEAR.to_string() };
                                            let (rig, part_name, patch, borrowed) = (rig.clone(), part.as_ref().map(|pp| pp.name.clone()).unwrap_or_default(), name.clone(), borrowed.clone());
                                            rsx! {
                                                button {
                                                    key: "{name}",
                                                    disabled: !can,
                                                    style: "position: relative; width: 100%; min-height: 44px; display: flex; align-items: center; justify-content: flex-start; gap: 10px; padding: 4px 16px 4px 38px; border: none; text-align: left; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)}; color: {INK}; font-family: {FONT}; cursor: {pick(can, \"pointer\", \"default\")};",
                                                    onclick: move |_| {
                                                        if let Some(stack) = fill_stack.clone() {
                                                            let pa = patch.clone();
                                                            call!(rig, |r| r.set_stack_patch(stack, pa, !in_fill));
                                                            return;
                                                        }
                                                        let (pn, pa) = (part_name.clone(), patch.clone());
                                                        call!(rig, |r| r.set_part_patch(pn, pa));
                                                        if borrowed.is_some() || had_profile {
                                                            let (pn, b) = (part_name.clone(), borrowed.clone().unwrap_or_default());
                                                            call!(rig, |r| r.set_part_profile(pn, b));
                                                        }
                                                    },
                                                    span { style: "position: absolute; left: 21px; top: 0; bottom: 0; width: 1.5px; background: {guide};" }
                                                    span { style: "width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; background: {dot}; border: 1.5px solid {tape};" }
                                                    span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 700, 540)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                                                    if on { span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", if fill.is_some() { "In" } else { "Playing" } } }
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
fn SearchResults(d: Data, target: Target, set_songs: Vec<String>, q: String, raw: String) -> Element {
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
    let first = found.iter().position(|(_, i)| i.state.is_some() || !i.inherited.is_empty());
    rsx! {
        if found.is_empty() {
            div { style: "padding: 32px 16px; font-size: 13.5px; color: {INK_3}; text-align: center; line-height: 1.4;", "Nothing called “{raw}”." }
        }
        for (n, (k, i)) in found.into_iter().enumerate() {
            {
                // A variation found by search says whose it is.
                let shown = if i.nested { Thing { name: i.id.clone(), ..i.clone() } } else { i.clone() };
                let sub = if i.from.is_empty() { k.label() } else { format!("{} · {}", k.label(), i.from) };
                let can = applies(&k, &target, &d);
                rsx! { Row { key: "{n}", kind: k.clone(), item: shown, sub, applies: can, scroll: first == Some(n) } }
            }
        }
    }
}

// ── Songs ──────────────────────────────────────────────────────────────────

/// A key's place in the circle's order; one it doesn't know comes first.
fn key_rank(k: &str) -> i32 {
    KEY_ORDER.iter().position(|x| *x == k).map_or(-1, |i| i as i32)
}

/// The colour a new collection gets: the next of the palette.
fn collection_colour(lib: &LibraryModel) -> String {
    COLLECTION_COLOURS[lib.collections.len() % COLLECTION_COLOURS.len()].to_string()
}

/// The song filters: a collection (All, or one of the player's) and
/// artist, key and genre — offering only what the library has.
#[component]
fn SongFilters(lib: LibraryModel, filter: Signal<SongFilter>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let f = filter();
    let mut artists: Vec<String> = lib.songs.iter().map(|s| s.artist.clone()).filter(|a| !a.is_empty()).collect();
    artists.sort();
    artists.dedup();
    let mut keys: Vec<String> = Vec::new();
    for k in lib.songs.iter().map(|s| &s.key).filter(|k| !k.is_empty()) {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    keys.sort_by_key(|k| key_rank(k));
    let mut genres: Vec<String> = lib.songs.iter().map(|s| s.genre.clone()).filter(|a| !a.is_empty()).collect();
    genres.sort();
    genres.dedup();
    let colour = collection_colour(&lib);
    let col = f.collection.clone().and_then(|c| lib.collections.iter().find(|x| x.name == c).cloned());
    let taken: Vec<String> = lib.collections.iter().map(|c| c.name.clone()).collect();
    let mut tabs: Vec<(Option<String>, String)> = vec![(None, String::new())];
    tabs.extend(lib.collections.iter().map(|c| (Some(c.name.clone()), c.colour.clone())));
    rsx! {
        div { style: "display: flex; align-items: center; gap: 4px; min-width: 0; flex: 1; overflow-x: auto; scrollbar-width: none;",
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
                        let colour = colour.clone();
                        move |e: MouseEvent| {
                            // Under the button, its right edge on the menu's.
                            let (c, el) = (e.client_coordinates(), e.element_coordinates());
                            let (rig, colour) = (rig.clone(), colour.clone());
                            open_naming(host, c.x - el.x + 40.0 - MENU_W, c.y - el.y + 44.0, MenuEntry::name("add", "New collection…", "", "Add", taken.clone()), EventHandler::new(move |p: Picked| {
                                let (name, colour) = (p.text.clone(), colour.clone());
                                call!(rig, |r| r.set_collection(name, colour, Vec::new()));
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
    let mut button_el = use_signal(|| None::<Rc<MountedData>>);
    rsx! {
        span { style: "display: flex; align-items: center; flex-shrink: 0; border-radius: {R}; background: {pick(on, FILL_ON, FILL)};",
            button {
                style: "height: 44px; display: flex; align-items: center; gap: 7px; padding: {pick(on, \"0 6px 0 12px\", \"0 10px 0 12px\")}; border: none; background: transparent; font-size: 14px; font-weight: {pick(on, 700, 600)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; font-family: {FONT}; cursor: pointer;",
                onmounted: move |e| button_el.set(Some(e.data())),
                onclick: move |e: MouseEvent| {
                    // Under the button, its right edge on the menu's.
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (x, y) = (c.x - el.x, c.y - el.y + 48.0);
                    let items = items.clone();
                    let mounted = button_el.peek().clone();
                    let picked = EventHandler::new(move |p: Picked| {
                        onpick.call(if p.id == "__any" { None } else { Some(p.id.clone()) });
                    });
                    spawn(async move {
                        let at = match mounted {
                            Some(m) => m.get_client_rect().await.ok().map(|r| (r.max_x() - MENU_W, r.max_y() + 4.0)),
                            None => None,
                        };
                        let (x, y) = at.unwrap_or((x, y));
                        open_menu(host, x, y, items, picked);
                    });
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
    // The first song in the set: the one the list opens on.
    let first = shown.iter().position(|t| t.state.is_some());
    rsx! {
        div { style: "padding-bottom: 16px;",
            for (n, t) in shown.iter().cloned().enumerate() {
                SongRow { key: "{t.id}", d: d.clone(), item: t.clone(), add, scroll: first == Some(n) }
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
fn SongRow(d: Data, item: Thing, add: bool, scroll: bool) -> Element {
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
    let colour = collection_colour(&d.lib);
    rsx! {
        div {
            style: "position: relative; display: flex; align-items: center; background: {pick(on, \"rgba(255,255,255,0.06)\", CLEAR)};",
            onmounted: move |e| if scroll { into_view(e) },
            if on {
                span { style: "position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: {LIVE};" }
            }
            button {
                disabled: !add,
                style: "flex: 1; min-width: 0; min-height: 56px; display: flex; align-items: center; justify-content: flex-start; gap: 12px; padding: 6px 4px 6px 16px; border: none; text-align: left; background: transparent; color: {INK}; font-family: {FONT}; cursor: {pick(add, \"pointer\", \"default\")};",
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
                            let (c, colour) = (p.text.clone(), colour.clone());
                            call!(rig, |r| r.set_collection(c, colour, vec![name]));
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

/// Every patch the rig knows, once, as `(name, stack, from)`: each
/// profile's, then the songs' own.
fn all_patches(d: &Data) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for p in &d.lib.profiles {
        for x in &p.patch_list {
            if !x.stack.is_empty() && !out.iter().any(|o| o.0.eq_ignore_ascii_case(&x.name)) {
                let from = if x.song.is_empty() { p.name.clone() } else { format!("{}'s own", x.song) };
                out.push((x.name.clone(), x.stack.clone(), from));
            }
        }
    }
    // The rest are the song's own, merged into the profile while it plays.
    let song = song_now(d);
    for x in &d.patches {
        if !out.iter().any(|o| o.0.eq_ignore_ascii_case(&x.name)) {
            let stack = if x.stack.is_empty() { "Special".to_string() } else { x.stack.clone() };
            out.push((x.name.clone(), stack, format!("{song}'s own")));
        }
    }
    out
}

/// A patch's source for `profile`: its own (`true`), borrowed from another
/// profile, or (neither) a song's own.
fn source_of(lib: &LibraryModel, profile: &str, patch: &str) -> (bool, Option<String>) {
    // A song's own patch is merged into the profile while it plays: not
    // the profile's own.
    let mine = lib.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(profile) && p.patch_list.iter().any(|x| x.song.is_empty() && x.name.eq_ignore_ascii_case(patch)));
    if mine {
        return (true, None);
    }
    (false, lib.profiles.iter().find(|p| p.patch_list.iter().any(|x| x.song.is_empty() && x.name.eq_ignore_ascii_case(patch))).map(|p| p.name.clone()))
}

/// "n overrides".
fn kinds_label(n: usize) -> String {
    format!("{n} override{}", if n == 1 { "" } else { "s" })
}

/// ‹ or ›: pick the part before or after; dim at the ends.
#[component]
fn StepButton(back: bool, to: Option<usize>, part: Signal<Option<usize>>) -> Element {
    let ink = if to.is_some() { INK_2 } else { DIM };
    rsx! {
        button {
            disabled: to.is_none(),
            "aria-label": if back { "Previous section" } else { "Next section" },
            style: "width: 40px; height: 44px; border: none; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer;",
            onclick: move |_| {
                if let Some(k) = to {
                    let mut p = part;
                    p.set(Some(k));
                }
            },
            svg { key: "{ink}", width: "9", height: "15", view_box: "0 0 9 15", style: "{pick(back, NOTHING, FLIPPED)}",
                path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: ink, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

const FLIPPED: &str = "transform: scaleX(-1);";

/// The rail's right-hand rule.
const RULE_LINE: &str = "1px solid #222228";

/// The picks put over the playing variation (a preset target).
fn variation_picks(d: &Data, target: &Target) -> Vec<signal_guitar_proto::PartPick> {
    let Target::Preset(p, v) = target else { return Vec::new() };
    d.comp
        .presets
        .iter()
        .find(|x| x.name == *p)
        .and_then(|x| x.snapshots.iter().find(|s| s.name == *v))
        .map(|s| s.picks.clone())
        .unwrap_or_default()
}

/// Where a routing pick points inside the browser: `(preset, variation)`.
#[derive(Clone, Copy)]
struct Reveal(Signal<Option<(String, String)>>);
