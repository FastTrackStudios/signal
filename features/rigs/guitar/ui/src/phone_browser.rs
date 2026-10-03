//! The phone's browser: one full-screen picker for everything a player
//! chooses — profiles, patches, songs and setlists, the module presets (the
//! Core and Time; Drive, Amp, Delay, Reverb), one block's preset and
//! algorithm, one drive slot's pedal and capture.
//!
//! It opens on what was tapped — a name on the status line, a face's
//! preset name or algorithm — and follows Apple's guidance for this kind of
//! screen (the Human Interface Guidelines' sheets, lists, segmented
//! controls and accessibility pages):
//!
//! - a sidebar, not a strip of tabs: more than about five sections is
//!   navigation, and in landscape that is a list down the side (Settings'
//!   shape) — grouped (this page's blocks; what plays; the modules), each
//!   row 44 points tall and saying what it is on now;
//! - the section on the right: ‹ what plays › on 44-point steppers, then
//!   every choice, the chosen one marked with a checkmark (not inverted),
//!   a choice that leads somewhere (a profile to its patches, a setlist to
//!   its songs, a pedal to its captures) with a chevron;
//! - a pick applies at once, so there is no Save — Done, trailing, closes,
//!   and so does a swipe down from the head;
//! - no text under 11 points, targets no smaller than 44.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LibraryModel, LiveBlock, PatchInfo, PerformanceModel};
use signal_proto::block::BlockType;

use crate::state::RigViewState;

const BG: &str = "#0f1012";
const BAR_BG: &str = "#17181b";
const RAISED: &str = "#26292f";
const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";

/// What the browser is on.
#[derive(Clone, PartialEq, Debug)]
pub enum BrowseTab {
    Profiles,
    Patches,
    Songs,
    Setlists,
    /// A module's presets and their snapshots (`Core`, `Time`, `Drive`,
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

    fn label(&self) -> String {
        match self {
            Self::Profiles => "Profile".to_string(),
            Self::Patches => "Patch".to_string(),
            Self::Songs => "Song".to_string(),
            Self::Setlists => "Setlist".to_string(),
            Self::Module(m) => (*m).to_string(),
            Self::Block { name, .. } | Self::Pedal { name } => name.clone(),
        }
    }
}

/// The accent a choice that plays is marked with.
const ACCENT: &str = "#0a84ff";

/// One choice: a tile, its kind small over its name, a checkmark when it
/// is what plays and a chevron when it leads on.
#[component]
fn Tile(name: String, #[props(default)] sub: String, on: bool, #[props(default)] leads: bool, onpick: EventHandler<()>) -> Element {
    let look = if on {
        format!("background: rgba(10, 132, 255, 0.18); border: 1px solid {ACCENT};")
    } else {
        format!("background: {RAISED}; border: 1px solid {RULE};")
    };
    rsx! {
        div { style: "min-height: 56px; box-sizing: border-box; padding: 8px 12px; border-radius: 12px; display: flex; flex-direction: row; align-items: center; gap: 8px; cursor: pointer; overflow: hidden; color: {TEXT}; {look}",
            onclick: move |_| onpick.call(()),
            div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; justify-content: center; gap: 2px;",
                if !sub.is_empty() {
                    span { style: "font-size: 11px; font-weight: 700; color: {DIM}; white-space: nowrap; overflow: hidden;", "{sub}" }
                }
                span { style: "font-size: 15px; font-weight: 700; white-space: nowrap; overflow: hidden;", "{name}" }
            }
            if on {
                // Drawn: the checkmark glyph is not in Blitz's fonts.
                svg { width: "18", height: "18", view_box: "0 0 24 24", fill: "none", stroke: ACCENT, stroke_width: "3", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M5 12.5l4.5 4.5L19 7" }
                }
            } else if leads {
                span { style: "flex: 0 0 auto; font-size: 20px; color: {DIM};", "›" }
            }
        }
    }
}

/// A run of tiles under a heading.
#[component]
fn Section(title: String, children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 8px;",
            if !title.is_empty() {
                span { style: "font-size: 13px; font-weight: 700; color: {DIM}; padding-left: 4px;", "{title}" }
            }
            // A grid (not a wrapped flex row — Blitz places one in a
            // scroller above its top), 12 points between tiles.
            div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px;", {children} }
        }
    }
}

/// The browser, over the whole screen, on `tab`. `lead`/`trail` keep its
/// sides clear of the camera housing and the screen's corners.
#[component]
pub fn PhoneBrowser(
    tab: BrowseTab,
    model: PerformanceModel,
    state: RigViewState,
    lead: u32,
    trail: u32,
    /// The page's own blocks, as tabs at the front of the strip (a drive
    /// page's slots, a delay page's delays).
    #[props(default)]
    context: Vec<BrowseTab>,
    on_close: EventHandler<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut on = use_signal(|| tab.clone());
    // The page's blocks first, and a block or pedal opened from a face
    // that is not one of them.
    let front = use_hook(|| {
        let mut front = context.clone();
        if matches!(tab, BrowseTab::Block { .. } | BrowseTab::Pedal { .. }) && !front.contains(&tab) {
            front.insert(0, tab.clone());
        }
        front
    });
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
                    None => (LibraryModel::default(), Vec::<PatchInfo>::new(), CompositionModel::default(), Vec::new()),
                }
            }
        }
    });
    let (lib, patches, comp, fetched_nodes) = data.read().clone().unwrap_or_default();
    let blocks: Vec<LiveBlock> = (state.blocks)();
    // The rig's nodes, as fetched with the rest (the live view's own copy
    // can still be empty while nothing has changed the chain yet).
    let nodes = if fetched_nodes.is_empty() { (state.nodes)() } else { fetched_nodes };
    // A drive slot's node: by its name or its pedal's (a slot's node is
    // named for the pedal in it), else the slot's place among the board's
    // slots — the nodes that offer pedals, in chain order.
    let slot_node = {
        let (blocks, nodes) = (blocks.clone(), nodes.clone());
        move |slot: &str| -> Option<signal_proto::live_node::LiveNode> {
            let slots: Vec<_> = nodes.iter().filter(|n| !n.alternatives.is_empty()).cloned().collect();
            let block = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(slot));
            slots
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(slot) || block.is_some_and(|b| !b.preset.is_empty() && n.name.eq_ignore_ascii_case(&b.preset)))
                .cloned()
                .or_else(|| {
                    let at = blocks
                        .iter()
                        .filter(|b| matches!(b.block_type, BlockType::Drive | BlockType::Boost))
                        .position(|b| b.name.eq_ignore_ascii_case(slot))?;
                    slots.get(at).cloned()
                })
        }
    };

    // Every RPC a pick or a step makes, fired and forgotten. The client in
    // a signal (it is `Copy`), so every tile's closure can hold it.
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

    // What plays, per tab: (its heading, the name, a line under it).
    let now = |t: &BrowseTab| -> (String, String, String) { match t {
        BrowseTab::Profiles => ("Profile".into(), model.profile_name.clone(), String::new()),
        BrowseTab::Patches => (
            model.profile_name.clone(),
            patches.iter().find(|p| p.active).map(|p| p.name.clone()).unwrap_or_default(),
            patches.iter().find(|p| p.active).map(|p| p.stack.clone()).unwrap_or_default(),
        ),
        BrowseTab::Songs => (
            model.setlists.get(model.setlist_index as usize).cloned().unwrap_or_else(|| "Song".into()),
            model.songs.get(model.song_index as usize).map(|s| s.name.clone()).unwrap_or_default(),
            model.songs.get(model.song_index as usize).map(|s| format!("{} · {} bpm", s.key, s.bpm)).unwrap_or_default(),
        ),
        BrowseTab::Setlists => (
            "Setlist".into(),
            model.setlists.get(model.setlist_index as usize).cloned().unwrap_or_default(),
            String::new(),
        ),
        BrowseTab::Module(m) => {
            let pick = comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m));
            ((*m).to_string(), pick.map(|p| p.preset.clone()).unwrap_or_default(), pick.map(|p| p.snapshot.clone()).unwrap_or_default())
        }
        BrowseTab::Block { name, .. } => (
            name.clone(),
            comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(name)).map(|b| b.preset.clone()).unwrap_or_default(),
            String::new(),
        ),
        BrowseTab::Pedal { name } => {
            let b = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name));
            (name.clone(), b.map(|b| b.preset.clone()).unwrap_or_default(), b.map(|b| b.detail.clone()).unwrap_or_default())
        }
    } };
    let current = on();
    let (head_label, head_name, head_sub) = now(&current);
    let step = {
        let current = current.clone();
        let slot_node = slot_node.clone();
        let (lib, patches, comp, blocks, model) = (lib.clone(), patches.clone(), comp.clone(), blocks.clone(), model.clone());
        move |d: i32| {
            let wrap = |at: Option<usize>, n: usize| -> Option<usize> {
                (n > 0).then(|| at.map_or(0, |i| (i as i64 + i64::from(d)).rem_euclid(n as i64) as usize))
            };
            match &current {
                BrowseTab::Profiles => {
                    let at = lib.profiles.iter().position(|p| p.active);
                    if let Some(i) = wrap(at, lib.profiles.len()) {
                        let name = lib.profiles[i].name.clone();
                        fire!(r => r.select_profile(name));
                    }
                }
                BrowseTab::Patches => {
                    if let Some(i) = wrap(patches.iter().position(|p| p.active), patches.len()) {
                        fire!(r => r.select_patch(i as u32));
                    }
                }
                BrowseTab::Songs => {
                    if d > 0 { fire!(r => r.next_song()) } else { fire!(r => r.prev_song()) }
                }
                BrowseTab::Setlists => {
                    if let Some(i) = wrap(Some(model.setlist_index as usize), model.setlists.len()) {
                        fire!(r => r.select_setlist(i as u32));
                    }
                }
                BrowseTab::Module(m) => {
                    let m = (*m).to_string();
                    fire!(r => r.step_module(m, d));
                }
                BrowseTab::Block { name, block_type } => {
                    let names: Vec<String> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(block_type)).map(|p| p.name.clone()).collect();
                    let playing = comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(name)).map(|b| b.preset.clone());
                    let at = playing.and_then(|p| names.iter().position(|n| *n == p));
                    if let Some(i) = wrap(at, names.len()) {
                        let (block, preset) = (name.clone(), names[i].clone());
                        fire!(r => r.choose_block(block, preset));
                    }
                }
                BrowseTab::Pedal { name } => {
                    let Some(node) = slot_node(name) else { return };
                    let node = &node;
                    let pedal = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name)).map(|b| b.preset.clone()).unwrap_or_default();
                    let at = node.alternatives.iter().position(|a| a.name.eq_ignore_ascii_case(&pedal));
                    if let Some(i) = wrap(at, node.alternatives.len()) {
                        let (id, with) = (node.id.clone(), node.alternatives[i].id.clone());
                        fire!(r => r.replace_node(id, with));
                    }
                }
            }
        }
    };

    // The sidebar's groups: this page's blocks (and the block or pedal it
    // was opened on), what plays, the modules.
    let groups: Vec<(&str, Vec<BrowseTab>)> = vec![
        ("This page", front.clone()),
        ("Playing", vec![BrowseTab::Profiles, BrowseTab::Patches, BrowseTab::Songs, BrowseTab::Setlists]),
        ("Modules", BrowseTab::MODULES.iter().map(|m| BrowseTab::Module(m)).collect()),
    ];
    let rows: Vec<(String, Vec<(BrowseTab, String, String)>)> = groups
        .into_iter()
        .filter(|(_, tabs)| !tabs.is_empty())
        .map(|(g, tabs)| (g.to_string(), tabs.into_iter().map(|t| { let (_, n, _) = now(&t); let l = t.label(); (t, l, n) }).collect()))
        .collect();
    let arrow = format!("flex: 0 0 56px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; background: {RAISED}; border: 1px solid {RULE}; color: {TEXT}; font-size: 24px; font-weight: 700; cursor: pointer;");
    let back = step.clone();
    // A swipe down from the head closes it (where a sheet's grabber is).
    let mut drag = use_signal(|| None::<f64>);

    rsx! {
        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 10; display: flex; flex-direction: row; box-sizing: border-box; padding: 0 {trail}px 0 {lead}px; background: {BG}; color: {TEXT};",
            onpointermove: move |e: PointerEvent| {
                if let Some(from) = drag() && e.client_coordinates().y - from > 70.0 {
                    drag.set(None);
                    on_close.call(());
                }
            },
            onpointerup: move |_| drag.set(None),
            // ── The sidebar ──
            div { style: "flex: 0 0 216px; height: 100%; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 4px; box-sizing: border-box; padding: 10px 8px; background: {BAR_BG}; border-right: 1px solid {RULE};",
                for (group, tabs) in rows {
                    span { key: "g-{group}", style: "font-size: 12px; font-weight: 700; color: {DIM}; padding: 10px 10px 4px;", "{group}" }
                    for (t, label, value) in tabs {
                        {
                            let selected = t == current;
                            let look = if selected { format!("background: {RAISED};") } else { String::new() };
                            let key = format!("{t:?}");
                            rsx! {
                                div { key: "{key}", style: "min-height: 44px; flex: 0 0 auto; box-sizing: border-box; padding: 5px 10px; border-radius: 10px; display: flex; flex-direction: column; justify-content: center; cursor: pointer; {look}",
                                    onclick: move |_| on.set(t.clone()),
                                    span { style: "font-size: 15px; font-weight: 700; color: {TEXT}; white-space: nowrap; overflow: hidden;", "{label}" }
                                    if !value.is_empty() {
                                        span { style: "font-size: 11px; font-weight: 600; color: {DIM}; white-space: nowrap; overflow: hidden;", "{value}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // ── The section ──
            div { style: "flex: 1 1 0%; min-width: 0; height: 100%; display: flex; flex-direction: column;",
            // What plays, stepped — and the head a swipe down closes from.
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; align-items: center; gap: 12px; padding: 8px 12px; border-bottom: 1px solid {RULE};",
                onpointerdown: move |e: PointerEvent| drag.set(Some(e.client_coordinates().y)),
                div { style: "{arrow}", onclick: move |_| back(-1), "‹" }
                div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; align-items: center;",
                    span { style: "font-size: 12px; font-weight: 700; color: {DIM};", "{head_label}" }
                    span { style: "font-size: 19px; font-weight: 800; color: #f4f4f5; white-space: nowrap; overflow: hidden;", "{head_name}" }
                    if !head_sub.is_empty() {
                        span { style: "font-size: 12px; font-weight: 600; color: {DIM}; white-space: nowrap;", "{head_sub}" }
                    }
                }
                div { style: "{arrow}", onclick: move |_| step(1), "›" }
                div { style: "flex: 0 0 auto; height: 44px; display: flex; align-items: center; padding: 0 18px; border-radius: 12px; font-size: 16px; font-weight: 700; cursor: pointer; color: {ACCENT};",
                    onclick: move |_| on_close.call(()),
                    "Done"
                }
            }
            // ── Everything there is to choose ──
            div { style: "flex: 1 1 0%; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 12px;",
                match current.clone() {
                    BrowseTab::Profiles => rsx! {
                        Section { title: String::new(),
                            for p in lib.profiles.clone() {
                                Tile { key: "{p.name}", name: p.name.clone(), sub: format!("{} patches", p.patches), on: p.active, leads: true,
                                    // A profile leads to its patches.
                                    onpick: { let name = p.name.clone(); move |()| { let name = name.clone(); fire!(r => r.select_profile(name)); on.set(BrowseTab::Patches); } },
                                }
                            }
                        }
                    },
                    BrowseTab::Patches => {
                        let mut stacks: Vec<(String, Vec<(usize, PatchInfo)>)> = Vec::new();
                        for (i, p) in patches.iter().cloned().enumerate() {
                            match stacks.iter_mut().find(|(s, _)| *s == p.stack) {
                                Some((_, list)) => list.push((i, p)),
                                None => stacks.push((p.stack.clone(), vec![(i, p)])),
                            }
                        }
                        rsx! {
                            for (stack, list) in stacks {
                                Section { key: "{stack}", title: stack.clone(),
                                    for (i, p) in list {
                                        Tile { key: "{i}", name: p.name.clone(), on: p.active,
                                            onpick: move |()| { fire!(r => r.select_patch(i as u32)); on_close.call(()); },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    BrowseTab::Songs => rsx! {
                        Section { title: String::new(),
                            for (i, s) in model.songs.iter().cloned().enumerate() {
                                Tile { key: "{i}-{s.name}", name: s.name.clone(), sub: format!("{} · {} bpm", s.key, s.bpm), on: i as u32 == model.song_index,
                                    onpick: move |()| { fire!(r => r.select_song(i as u32)); on_close.call(()); },
                                }
                            }
                        }
                    },
                    BrowseTab::Setlists => rsx! {
                        Section { title: String::new(),
                            for (i, s) in model.setlists.iter().cloned().enumerate() {
                                Tile { key: "{i}-{s}", name: s.clone(), on: i as u32 == model.setlist_index, leads: true,
                                    // A setlist leads to its songs.
                                    onpick: move |()| { fire!(r => r.select_setlist(i as u32)); on.set(BrowseTab::Songs); },
                                }
                            }
                        }
                    },
                    BrowseTab::Module(m) => {
                        let mine: Vec<_> = comp.modules.iter().filter(|p| p.module.eq_ignore_ascii_case(m)).cloned().collect();
                        let pick = comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m)).cloned();
                        rsx! {
                            if mine.is_empty() {
                                span { style: "font-size: 14px; color: {DIM};", "No {m} presets yet." }
                            }
                            for p in mine {
                                Section { key: "{p.name}", title: p.name.clone(),
                                    for (i, s) in p.snapshots.iter().cloned().enumerate() {
                                        {
                                            let live = pick.as_ref().is_some_and(|k| k.preset.eq_ignore_ascii_case(&p.name) && (k.snapshot.eq_ignore_ascii_case(&s) || (k.snapshot.is_empty() && i == 0)));
                                            let (preset, snap) = (p.name.clone(), s.clone());
                                            rsx! {
                                                Tile { key: "{s}", name: s.clone(), on: live,
                                                    onpick: move |()| { let (m, preset, snap) = (m.to_string(), preset.clone(), snap.clone()); fire!(r => r.choose_module(m, preset, snap)); on_close.call(()); },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    BrowseTab::Block { name, block_type } => {
                        let block = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&name)).cloned();
                        // The algorithm: a delay's style, a reverb's algorithm.
                        let algos: Option<(&'static str, &'static [&'static str])> = match block.as_ref().map(|b| b.block_type) {
                            Some(BlockType::Delay) => Some(("style", &crate::control::DELAY_ALGOS)),
                            Some(BlockType::Reverb) => Some(("algorithm", &crate::control::VERB_ALGOS)),
                            _ => None,
                        };
                        let presets: Vec<String> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(&block_type)).map(|p| p.name.clone()).collect();
                        let playing = comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(&name)).map(|b| b.preset.clone()).unwrap_or_default();
                        rsx! {
                            if let (Some((param, names)), Some(b)) = (algos, block.clone()) {
                                {
                                    let now = b.params.iter().find(|p| p.name == param).map_or(-1.0, |p| p.value);
                                    rsx! {
                                        Section { title: "Algorithm".to_string(),
                                            for (i, a) in names.iter().enumerate() {
                                                Tile { key: "{a}", name: (*a).to_string(), on: (now - i as f32).abs() < 0.5,
                                                    onpick: { let id = b.id.clone(); move |()| crate::control::send_param(&client.peek(), &id, param, i as f32) },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Section { title: "Presets".to_string(),
                                for p in presets {
                                    Tile { key: "{p}", name: p.clone(), on: p == playing,
                                        onpick: { let (block, preset) = (name.clone(), p.clone()); move |()| { let (block, preset) = (block.clone(), preset.clone()); fire!(r => r.choose_block(block, preset)); on_close.call(()); } },
                                    }
                                }
                            }
                        }
                    }
                    BrowseTab::Pedal { name } => {
                        let block = blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&name)).cloned();
                        let node = slot_node(&name);
                        let pedal = block.as_ref().map(|b| b.preset.clone()).unwrap_or_default();
                        rsx! {
                            if let Some(b) = block.clone() {
                                if b.options.len() > 1 {
                                    Section { title: format!("{pedal} — capture"),
                                        for (i, o) in b.options.iter().cloned().enumerate() {
                                            Tile { key: "{i}-{o}", name: o.clone(), on: i as u32 == b.option,
                                                onpick: { let id = b.id.clone(); move |()| { let id = id.clone(); fire!(r => r.set_block_option(id, i as u32)); on_close.call(()); } },
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(n) = node {
                                Section { title: "Pedal".to_string(),
                                    for a in n.alternatives.clone() {
                                        // A pedal leads to its captures: the
                                        // browser stays open on them.
                                        Tile { key: "{a.id}", name: a.name.clone(), on: a.name.eq_ignore_ascii_case(&pedal), leads: true,
                                            onpick: { let (id, with) = (n.id.clone(), a.id.clone()); move |()| { let (id, with) = (id.clone(), with.clone()); fire!(r => r.replace_node(id, with)); } },
                                        }
                                    }
                                }
                            } else {
                                // The slot's pedal is the Drive module's to say
                                // (a module scene names its slots): it changes
                                // there.
                                Section { title: "This slot's pedal comes from the Drive module".to_string(),
                                    Tile { name: "Drive module".to_string(), sub: comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case("Drive")).map(|p| format!("{} · {}", p.preset, p.snapshot)).unwrap_or_default(), on: false, leads: true,
                                        onpick: move |()| on.set(BrowseTab::Module("Drive")),
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
