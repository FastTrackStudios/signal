//! The phone's browser: one full-screen picker for everything a player
//! chooses, in columns that narrow left to right — the family, then its
//! variations, then the one that plays:
//!
//! - a set: **Setlist › Song › Part**;
//! - a profile: **Profile › Stack › Patch**;
//! - a module (Core, Time; Drive, Amp, Delay, Reverb): **Preset › Variation**;
//! - a block (a delay, a reverb): **Group › Preset**, its algorithm first;
//! - a drive slot: **Pedal › Capture**.
//!
//! It opens on what was tapped — the status line's names (what plays: the
//! set in Setlist mode, the profile in Profile mode; a module), a face's
//! preset name or algorithm — on what plays now, every column showing the
//! path to it. Across the top, that path, and ‹ › stepping through its last
//! step (the next part, patch, variation, preset, capture).
//!
//! Apple's guidance for this kind of screen, as followed: lists in columns
//! (drill-down, Settings' and Music's shape) rather than a strip of tabs;
//! rows 44 points tall or more; what plays marked with a checkmark in an
//! accent, a row that leads on with a chevron; picks apply at once, so
//! Done (trailing) or a swipe down from the head closes; no text under 11
//! points. A row that is something to play (a setlist, a song, a profile)
//! plays as it opens its column; a row that only groups (a stack, a preset
//! family) just opens it.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LibraryModel, LiveBlock, PatchInfo, PerformanceModel};
use signal_proto::block::BlockType;
use signal_proto::live_node::LiveNode;

use crate::state::RigViewState;

const BG: &str = "#0f1012";
const COL_BG: &str = "#141518";
const BAR_BG: &str = "#17181b";
const RAISED: &str = "#26292f";
const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
/// The accent what plays is marked with.
const ACCENT: &str = "#0a84ff";
/// The algorithm's row key in a block's first column (not a preset group).
const ALGORITHM: &str = "\u{1}algorithm";

/// What the browser is opened on.
#[derive(Clone, PartialEq, Debug)]
pub enum BrowseTab {
    Profiles,
    Patches,
    Songs,
    Setlists,
    /// A module's presets and their variations (`Core`, `Time`, `Drive`,
    /// `Amp`, `Delay`, `Reverb`).
    Module(&'static str),
    /// One block's presets (and, for a delay or reverb, its algorithm) —
    /// by the block's chain name (`DLY 1`) and preset type (`delay`).
    Block { name: String, block_type: String },
    /// One drive slot's pedal and capture, by its chain name (`Drive 1`).
    Pedal { name: String },
}

impl BrowseTab {
    /// The modules a tab of their own is kept for.
    pub const MODULES: [&'static str; 6] = ["Core", "Time", "Drive", "Amp", "Delay", "Reverb"];

    /// A face's block selection, as a tab: a drive slot is a pedal.
    #[must_use]
    pub fn for_block(name: &str, block_type: &str) -> Self {
        if block_type.eq_ignore_ascii_case("drive") || block_type.eq_ignore_ascii_case("boost") {
            Self::Pedal { name: name.to_string() }
        } else {
            Self::Block { name: name.to_string(), block_type: block_type.to_string() }
        }
    }

    /// A module by name, as a tab (its own name kept `'static`).
    #[must_use]
    pub fn for_module(module: &str) -> Option<Self> {
        Self::MODULES.iter().find(|m| m.eq_ignore_ascii_case(module)).map(|m| Self::Module(m))
    }
}

/// One row of a column.
#[derive(Clone, PartialEq, Debug)]
struct Row {
    /// Unique within its column (what the column's selection holds).
    key: String,
    name: String,
    sub: String,
    /// It plays now (a checkmark).
    live: bool,
    /// It opens the next column (a chevron).
    leads: bool,
}

/// A column: its heading and its rows.
#[derive(Clone, PartialEq, Debug)]
struct Column {
    title: String,
    rows: Vec<Row>,
}

/// One row: 48 points tall, a checkmark when it plays, a chevron when it
/// leads on; the open one raised.
#[component]
fn ListRow(row: Row, open: bool, onpick: EventHandler<()>) -> Element {
    let bg = if open { RAISED } else { "transparent" };
    let name_color = if row.live { ACCENT } else { TEXT };
    rsx! {
        div { style: "min-height: 48px; flex: 0 0 auto; box-sizing: border-box; padding: 6px 10px 6px 12px; border-radius: 10px; display: flex; flex-direction: row; align-items: center; gap: 8px; cursor: pointer; background: {bg};",
            onclick: move |_| onpick.call(()),
            div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; justify-content: center; gap: 1px;",
                span { style: "font-size: 15px; font-weight: 700; color: {name_color}; white-space: nowrap; overflow: hidden;", "{row.name}" }
                if !row.sub.is_empty() {
                    span { style: "font-size: 11px; font-weight: 600; color: {DIM}; white-space: nowrap; overflow: hidden;", "{row.sub}" }
                }
            }
            if row.live {
                // Drawn: the checkmark glyph is not in Blitz's fonts.
                svg { width: "18", height: "18", view_box: "0 0 24 24", fill: "none", stroke: ACCENT, stroke_width: "3", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M5 12.5l4.5 4.5L19 7" }
                }
            }
            if row.leads {
                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: DIM, stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M9 6l6 6-6 6" }
                }
            }
        }
    }
}

/// The browser, over the whole screen, opened on `tab`. `lead`/`trail`
/// keep its sides clear of the camera housing and the screen's corners.
#[component]
pub fn PhoneBrowser(
    tab: BrowseTab,
    model: PerformanceModel,
    state: RigViewState,
    lead: u32,
    trail: u32,
    on_close: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let revision = model.revision;
    let data = use_resource({
        let rig = rig.clone();
        move || {
            let _ = revision;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => (
                        r.library().await.unwrap_or_default(),
                        r.patches().await.unwrap_or_default(),
                        r.compositions().await.unwrap_or_default(),
                        r.nodes().await.unwrap_or_default(),
                    ),
                    None => (LibraryModel::default(), Vec::<PatchInfo>::new(), CompositionModel::default(), Vec::<LiveNode>::new()),
                }
            }
        }
    });
    let (lib, patches, comp, nodes) = data.read().clone().unwrap_or_default();
    let blocks: Vec<LiveBlock> = (state.blocks)();

    // The client in a signal (it is `Copy`), so every row's closure holds it.
    let client = use_signal(|| rig.clone());
    macro_rules! fire {
        ($r:ident => $body:expr) => {{
            if let Some($r) = client.peek().clone() {
                spawn(async move {
                    let _ = $body.await;
                });
            }
        }};
    }

    // The kind: what plays opens on the set or the profile, by mode.
    let kind = match &tab {
        BrowseTab::Profiles | BrowseTab::Patches | BrowseTab::Songs | BrowseTab::Setlists => {
            if model.perform_mode == 2 { BrowseTab::Setlists } else { BrowseTab::Profiles }
        }
        t => t.clone(),
    };
    // The open row of the first two columns (by key; `None`: the one on the
    // path to what plays).
    let mut open1 = use_signal(|| None::<String>);
    let mut open2 = use_signal(|| None::<String>);
    let live_module = |m: &str| comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m)).cloned();
    let block = |name: &str| blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name)).cloned();

    // ── The columns, per kind, and the path to what plays ──
    let (columns, path): (Vec<Column>, Vec<String>) = match &kind {
        BrowseTab::Setlists => {
            let live_set = model.setlists.get(model.setlist_index as usize).cloned().unwrap_or_default();
            let set = open1().unwrap_or_else(|| live_set.clone());
            let songs = lib.setlists.iter().find(|s| s.name == set).map(|s| s.songs.clone()).unwrap_or_default();
            let live_song = model.songs.get(model.song_index as usize).map(|s| s.name.clone()).unwrap_or_default();
            let song = open2().unwrap_or_else(|| {
                if set == live_set { live_song.clone() } else { songs.first().map(|s| s.name.clone()).unwrap_or_default() }
            });
            let parts = lib.songs.iter().find(|s| s.name == song).map(|s| s.parts.clone()).unwrap_or_default();
            let playing_song = set == live_set && song == live_song;
            let live_part = model.parts.get(model.part_index as usize).map(|p| p.name.clone()).unwrap_or_default();
            (
                vec![
                    Column {
                        title: "Setlist".into(),
                        rows: lib.setlists.iter().map(|s| Row { key: s.name.clone(), name: s.name.clone(), sub: format!("{} songs", s.songs.len()), live: s.name == live_set, leads: true }).collect(),
                    },
                    Column {
                        title: "Song".into(),
                        rows: songs.iter().map(|s| Row { key: s.name.clone(), name: s.name.clone(), sub: format!("{} · {} bpm", s.key, s.bpm), live: set == live_set && s.name == live_song, leads: true }).collect(),
                    },
                    Column {
                        title: "Part".into(),
                        rows: parts.iter().enumerate().map(|(i, p)| Row { key: format!("{i}"), name: p.clone(), sub: String::new(), live: playing_song && *p == live_part, leads: false }).collect(),
                    },
                ],
                vec![live_set, live_song, live_part],
            )
        }
        BrowseTab::Profiles => {
            let live_profile = model.profile_name.clone();
            let profile = open1().unwrap_or_else(|| live_profile.clone());
            let entry = lib.profiles.iter().find(|p| p.name == profile).cloned().unwrap_or_default();
            let live_patch = patches.iter().find(|p| p.active).cloned();
            let mut stacks: Vec<String> = entry.stacks.clone();
            if entry.patch_list.iter().any(|p| p.stack.is_empty()) {
                stacks.push(String::new());
            }
            let live_stack = if profile == live_profile { live_patch.as_ref().map(|p| p.stack.clone()) } else { None };
            let stack = open2().unwrap_or_else(|| live_stack.clone().unwrap_or_else(|| stacks.first().cloned().unwrap_or_default()));
            let stack_name = |s: &str| if s.is_empty() { "Patches".to_string() } else { s.to_string() };
            (
                vec![
                    Column {
                        title: "Profile".into(),
                        rows: lib.profiles.iter().map(|p| Row { key: p.name.clone(), name: p.name.clone(), sub: format!("{} patches", p.patches), live: p.name == live_profile, leads: true }).collect(),
                    },
                    Column {
                        title: "Stack".into(),
                        rows: stacks
                            .iter()
                            .map(|s| Row { key: s.clone(), name: stack_name(s), sub: format!("{} patches", entry.patch_list.iter().filter(|p| p.stack == *s).count()), live: live_stack.as_deref() == Some(s.as_str()), leads: true })
                            .collect(),
                    },
                    Column {
                        title: "Patch".into(),
                        rows: entry
                            .patch_list
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| p.stack == stack)
                            .map(|(i, p)| Row { key: format!("{i}"), name: p.name.clone(), sub: String::new(), live: profile == live_profile && live_patch.as_ref().is_some_and(|l| l.name == p.name), leads: false })
                            .collect(),
                    },
                ],
                vec![live_profile, live_stack.map(|s| stack_name(&s)).unwrap_or_default(), live_patch.map(|p| p.name).unwrap_or_default()],
            )
        }
        BrowseTab::Module(m) => {
            let mine: Vec<_> = comp.modules.iter().filter(|p| p.module.eq_ignore_ascii_case(m)).cloned().collect();
            let pick = live_module(m);
            let live_preset = pick.as_ref().map(|p| p.preset.clone()).unwrap_or_default();
            let preset = open1().unwrap_or_else(|| live_preset.clone());
            let snaps = mine.iter().find(|p| p.name == preset).map(|p| p.snapshots.clone()).unwrap_or_default();
            let live_snaps = mine.iter().find(|p| p.name == live_preset).map(|p| p.snapshots.clone()).unwrap_or_default();
            let live_snap = pick
                .as_ref()
                .map(|p| if p.snapshot.is_empty() { live_snaps.first().cloned().unwrap_or_default() } else { p.snapshot.clone() })
                .unwrap_or_default();
            (
                vec![
                    Column {
                        title: format!("{m} preset"),
                        rows: mine.iter().map(|p| Row { key: p.name.clone(), name: p.name.clone(), sub: format!("{} variations", p.snapshots.len()), live: p.name == live_preset, leads: true }).collect(),
                    },
                    Column {
                        title: "Variation".into(),
                        rows: snaps.iter().map(|s| Row { key: s.clone(), name: s.clone(), sub: String::new(), live: preset == live_preset && *s == live_snap, leads: false }).collect(),
                    },
                ],
                vec![live_preset, live_snap],
            )
        }
        BrowseTab::Block { name, block_type } => {
            let b = block(name);
            let algos: Option<(&'static str, &'static [&'static str])> = match b.as_ref().map(|b| b.block_type) {
                Some(BlockType::Delay) => Some(("style", &crate::control::DELAY_ALGOS)),
                Some(BlockType::Reverb) => Some(("algorithm", &crate::control::VERB_ALGOS)),
                _ => None,
            };
            let presets: Vec<_> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(block_type)).cloned().collect();
            let groups = crate::preset_look::grouped(&presets);
            let playing = comp.active_blocks.iter().find(|x| x.block.eq_ignore_ascii_case(name)).map(|x| x.preset.clone()).unwrap_or_default();
            let live_group = groups.iter().find(|(_, v)| v.iter().any(|(p, _)| p.name == playing)).map(|(g, _)| g.clone()).unwrap_or_default();
            let algo_now = b
                .as_ref()
                .zip(algos)
                .and_then(|(b, (param, names))| b.params.iter().find(|p| p.name == param).and_then(|p| names.get(p.value.round().max(0.0) as usize)).map(|s| (*s).to_string()));
            let mut first: Vec<Row> = Vec::new();
            if algos.is_some() {
                first.push(Row { key: ALGORITHM.into(), name: "Algorithm".into(), sub: algo_now.clone().unwrap_or_default(), live: false, leads: true });
            }
            first.extend(groups.iter().map(|(g, v)| Row {
                key: g.clone(),
                name: if g.is_empty() { "Presets".into() } else { g.clone() },
                sub: format!("{} presets", v.len()),
                live: *g == live_group,
                leads: true,
            }));
            let open = open1().unwrap_or_else(|| live_group.clone());
            let (second_title, second) = if open == ALGORITHM {
                (
                    "Algorithm".to_string(),
                    algos
                        .map(|(_, names)| names.iter().enumerate().map(|(i, a)| Row { key: format!("{i}"), name: (*a).to_string(), sub: String::new(), live: algo_now.as_deref() == Some(*a), leads: false }).collect())
                        .unwrap_or_default(),
                )
            } else {
                (
                    "Preset".to_string(),
                    groups
                        .iter()
                        .find(|(g, _)| *g == open)
                        .map(|(_, v)| v.iter().map(|(p, _)| Row { key: p.name.clone(), name: p.name.clone(), sub: String::new(), live: p.name == playing, leads: false }).collect())
                        .unwrap_or_default(),
                )
            };
            (
                vec![Column { title: name.clone(), rows: first }, Column { title: second_title, rows: second }],
                vec![algo_now.unwrap_or_default(), playing],
            )
        }
        BrowseTab::Pedal { name } => {
            let b = block(name);
            let pedal = b.as_ref().map(|b| b.preset.clone()).unwrap_or_default();
            let open = open1().unwrap_or_else(|| pedal.clone());
            // The captures: the playing pedal's from the chain, another's
            // from the library.
            let captures: Vec<String> = if open == pedal {
                b.as_ref().map(|b| b.options.clone()).unwrap_or_default()
            } else {
                lib.drives.iter().find(|d| d.name == open).map(|d| d.options.clone()).unwrap_or_default()
            };
            let live_capture = b.as_ref().map(|b| b.detail.clone()).unwrap_or_default();
            (
                vec![
                    Column {
                        title: "Pedal".into(),
                        rows: lib.drives.iter().map(|d| Row { key: d.name.clone(), name: d.name.clone(), sub: d.slots.join(", "), live: d.name == pedal, leads: true }).collect(),
                    },
                    Column {
                        title: "Capture".into(),
                        rows: captures.iter().enumerate().map(|(i, c)| Row { key: format!("{i}"), name: c.clone(), sub: String::new(), live: open == pedal && b.as_ref().is_some_and(|b| b.option as usize == i), leads: false }).collect(),
                    },
                ],
                vec![pedal, live_capture],
            )
        }
        _ => (Vec::new(), Vec::new()),
    };

    // ── A pick in a column ──
    let pick = {
        let (kind, model, lib, nodes, blocks, comp) = (kind.clone(), model.clone(), lib.clone(), nodes.clone(), blocks.clone(), comp.clone());
        move |col: usize, key: String| match (&kind, col) {
            // A set and a song play as they open.
            (BrowseTab::Setlists, 0) => {
                if let Some(i) = lib.setlists.iter().position(|s| s.name == key) {
                    fire!(r => r.select_setlist(i as u32));
                }
                open1.set(Some(key));
                open2.set(None);
            }
            (BrowseTab::Setlists, 1) => {
                let set = open1.peek().clone().or_else(|| model.setlists.get(model.setlist_index as usize).cloned()).unwrap_or_default();
                let set_i = lib.setlists.iter().position(|s| s.name == set);
                let song_i = lib.setlists.iter().find(|s| s.name == set).and_then(|s| s.songs.iter().position(|x| x.name == key));
                if let (Some(set_i), Some(song_i)) = (set_i, song_i) {
                    let same = set_i as u32 == model.setlist_index;
                    fire!(r => async move {
                        if !same {
                            let _ = r.select_setlist(set_i as u32).await;
                        }
                        r.select_song(song_i as u32).await
                    });
                }
                open2.set(Some(key));
            }
            (BrowseTab::Setlists, _) => {
                if let Ok(i) = key.parse::<u32>() {
                    fire!(r => r.select_part(i));
                }
            }
            // A profile plays as it opens; a stack only opens.
            (BrowseTab::Profiles, 0) => {
                let name = key.clone();
                fire!(r => r.select_profile(name));
                open1.set(Some(key));
                open2.set(None);
            }
            (BrowseTab::Profiles, 1) => open2.set(Some(key)),
            (BrowseTab::Profiles, _) => {
                let profile = open1.peek().clone().unwrap_or_else(|| model.profile_name.clone());
                if let Ok(i) = key.parse::<u32>() {
                    let switch = profile != model.profile_name;
                    fire!(r => async move {
                        if switch {
                            let _ = r.select_profile(profile).await;
                        }
                        r.select_patch(i).await
                    });
                }
            }
            // A module's preset opens; a variation plays.
            (BrowseTab::Module(_), 0) => open1.set(Some(key)),
            (BrowseTab::Module(m), _) => {
                let preset = open1
                    .peek()
                    .clone()
                    .or_else(|| comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m)).map(|p| p.preset.clone()))
                    .unwrap_or_default();
                let m = (*m).to_string();
                fire!(r => r.choose_module(m, preset, key));
            }
            // A block's group opens; an algorithm or preset plays.
            (BrowseTab::Block { .. }, 0) => open1.set(Some(key)),
            (BrowseTab::Block { name, .. }, _) => {
                if open1.peek().as_deref() == Some(ALGORITHM) {
                    if let (Some(b), Ok(i)) = (blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name)), key.parse::<usize>()) {
                        let param = if b.block_type == BlockType::Reverb { "algorithm" } else { "style" };
                        crate::control::send_param(&client.peek(), &b.id, param, i as f32);
                    }
                } else {
                    let block = name.clone();
                    fire!(r => r.choose_block(block, key));
                }
            }
            // A pedal goes into the slot (where the slot is the profile's to
            // fill) and opens its captures; a capture plays.
            (BrowseTab::Pedal { name }, 0) => {
                if let Some(node) = slot_node(&nodes, &blocks, name)
                    && let Some(alt) = node.alternatives.iter().find(|a| a.name == key)
                {
                    let (id, with) = (node.id.clone(), alt.id.clone());
                    fire!(r => r.replace_node(id, with));
                }
                open1.set(Some(key));
            }
            (BrowseTab::Pedal { name }, _) => {
                if let (Some(b), Ok(i)) = (blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name)), key.parse::<u32>()) {
                    let id = b.id.clone();
                    fire!(r => r.set_block_option(id, i));
                }
            }
            _ => {}
        }
    };

    // ── ‹ › through the path's last step ──
    let step = {
        let (kind, patches, comp, blocks) = (kind.clone(), patches.clone(), comp.clone(), blocks.clone());
        move |d: i32| {
            let wrap = |at: Option<usize>, n: usize| -> Option<usize> {
                (n > 0).then(|| at.map_or(0, |i| (i as i64 + i64::from(d)).rem_euclid(n as i64) as usize))
            };
            match &kind {
                BrowseTab::Setlists => fire!(r => r.step_part(d, false)),
                BrowseTab::Profiles => {
                    if let Some(i) = wrap(patches.iter().position(|p| p.active), patches.len()) {
                        fire!(r => r.select_patch(i as u32));
                    }
                }
                BrowseTab::Module(m) => {
                    let m = (*m).to_string();
                    fire!(r => r.step_module(m, d));
                }
                BrowseTab::Block { name, block_type } => {
                    let names: Vec<String> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(block_type)).map(|p| p.name.clone()).collect();
                    let playing = comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(name)).map(|b| b.preset.clone());
                    if let Some(i) = wrap(playing.and_then(|p| names.iter().position(|n| *n == p)), names.len()) {
                        let (block, preset) = (name.clone(), names[i].clone());
                        fire!(r => r.choose_block(block, preset));
                    }
                }
                BrowseTab::Pedal { name } => {
                    if let Some(b) = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
                        && let Some(i) = wrap(Some(b.option as usize), b.options.len())
                    {
                        let id = b.id.clone();
                        fire!(r => r.set_block_option(id, i as u32));
                    }
                }
                _ => {}
            }
        }
    };

    let crumbs: Vec<String> = path.into_iter().filter(|p| !p.is_empty()).collect();
    let title = match &kind {
        BrowseTab::Setlists => "Setlist".to_string(),
        BrowseTab::Profiles => "Profile".to_string(),
        BrowseTab::Module(m) => (*m).to_string(),
        BrowseTab::Block { name, .. } | BrowseTab::Pedal { name } => name.clone(),
        _ => String::new(),
    };
    let last = crumbs.last().cloned().unwrap_or_default();
    let before = crumbs[..crumbs.len().saturating_sub(1)].join("  ›  ");
    let heading = if before.is_empty() { title } else { format!("{title}  ·  {before}") };
    let arrow = format!("flex: 0 0 56px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; background: {RAISED}; border: 1px solid {RULE}; color: {TEXT}; font-size: 24px; font-weight: 700; cursor: pointer;");
    let back = step.clone();
    // A swipe down from the head closes it (where a sheet's grabber is).
    let mut drag = use_signal(|| None::<f64>);
    let cols = columns.len();
    let open_keys = [open1(), open2()];

    rsx! {
        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 10; display: flex; flex-direction: column; box-sizing: border-box; padding: 0 {trail}px 0 {lead}px; background: {BG}; color: {TEXT};",
            onpointermove: move |e: PointerEvent| {
                if let Some(from) = drag() && e.client_coordinates().y - from > 70.0 {
                    drag.set(None);
                    on_close.call(());
                }
            },
            onpointerup: move |_| drag.set(None),
            // ── What plays, and ‹ › through it ──
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; align-items: center; gap: 12px; padding: 8px 12px; background: {BAR_BG}; border-bottom: 1px solid {RULE};",
                onpointerdown: move |e: PointerEvent| drag.set(Some(e.client_coordinates().y)),
                div { style: "{arrow}", onclick: move |_| back(-1), "‹" }
                div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; align-items: center;",
                    span { style: "font-size: 12px; font-weight: 700; color: {DIM}; white-space: nowrap; overflow: hidden;", "{heading}" }
                    span { style: "font-size: 19px; font-weight: 800; color: #f4f4f5; white-space: nowrap; overflow: hidden;", "{last}" }
                }
                div { style: "{arrow}", onclick: move |_| step(1), "›" }
                div { style: "flex: 0 0 auto; height: 44px; display: flex; align-items: center; padding: 0 18px; border-radius: 12px; font-size: 16px; font-weight: 700; cursor: pointer; color: {ACCENT};",
                    onclick: move |_| on_close.call(()),
                    "Done"
                }
            }
            // ── The columns ──
            div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: row;",
                for (i, col) in columns.into_iter().enumerate() {
                    {
                        // A column that leads on shows its open row: the one
                        // picked, else the one on the path to what plays.
                        let open_key = open_keys.get(i).cloned().flatten().or_else(|| col.rows.iter().find(|r| r.live).map(|r| r.key.clone()));
                        let last_col = i + 1 == cols;
                        let border = if last_col { String::new() } else { format!("border-right: 1px solid {RULE};") };
                        let bg = if i == 0 { COL_BG } else { BG };
                        let pick = pick.clone();
                        rsx! {
                            div { key: "{i}-{col.title}", style: "flex: 1 1 0%; min-width: 0; height: 100%; display: flex; flex-direction: column; background: {bg}; {border}",
                                span { style: "flex: 0 0 auto; font-size: 12px; font-weight: 700; color: {DIM}; padding: 12px 16px 6px;", "{col.title}" }
                                div { style: "flex: 1 1 0%; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 2px; padding: 0 8px 12px;",
                                    if col.rows.is_empty() {
                                        span { style: "font-size: 13px; color: {DIM}; padding: 12px 8px;", "Nothing here." }
                                    }
                                    for row in col.rows {
                                        {
                                            let open = !last_col && open_key.as_deref() == Some(row.key.as_str());
                                            let key = row.key.clone();
                                            let mut pick = pick.clone();
                                            rsx! {
                                                ListRow { key: "{row.key}", row, open, onpick: move |()| pick(i, key.clone()) }
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

/// A drive slot's node: by its name or its pedal's (a slot's node is named
/// for the pedal in it), else the slot's place among the board's slots —
/// the nodes that offer pedals, in chain order.
fn slot_node(nodes: &[LiveNode], blocks: &[LiveBlock], slot: &str) -> Option<LiveNode> {
    let slots: Vec<&LiveNode> = nodes.iter().filter(|n| !n.alternatives.is_empty()).collect();
    let block = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(slot));
    slots
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case(slot) || block.is_some_and(|b| !b.preset.is_empty() && n.name.eq_ignore_ascii_case(&b.preset)))
        .map(|n| (*n).clone())
        .or_else(|| {
            let at = blocks.iter().filter(|b| matches!(b.block_type, BlockType::Drive | BlockType::Boost)).position(|b| b.name.eq_ignore_ascii_case(slot))?;
            slots.get(at).map(|n| (*n).clone())
        })
}
