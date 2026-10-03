//! The phone's browser: one full-screen picker for everything a player
//! chooses — profiles, patches, songs and setlists, the module presets (the
//! Core and Time; Drive, Amp, Delay, Reverb), one block's preset and
//! algorithm, one drive slot's pedal and capture.
//!
//! It opens on what was tapped — a name on the status line, a face's
//! preset name — and a strip of tabs along the top reaches the rest. Each
//! tab is the same shape: ‹ what plays › on big steppers, then everything
//! there is to choose, as tiles a thumb hits without aiming. A pick plays
//! at once; one that leads somewhere (a profile to its patches, a setlist
//! to its songs, a pedal to its captures) keeps the browser open there.

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

/// A tile's look: lit when it is what plays.
fn tile(on: bool) -> String {
    if on {
        "background: #f4f4f5; color: #0a0b0d; border: 1px solid #f4f4f5;".to_string()
    } else {
        format!("background: {RAISED}; color: {TEXT}; border: 1px solid {RULE};")
    }
}

/// One choice: a big tile, its kind small over its name.
#[component]
fn Tile(name: String, #[props(default)] sub: String, on: bool, onpick: EventHandler<()>) -> Element {
    let look = tile(on);
    rsx! {
        div { style: "flex: 0 0 auto; width: 168px; min-height: 58px; box-sizing: border-box; padding: 9px 12px; border-radius: 10px; display: flex; flex-direction: column; justify-content: center; gap: 2px; cursor: pointer; overflow: hidden; {look}",
            onclick: move |_| onpick.call(()),
            if !sub.is_empty() {
                span { style: "font-size: 10px; font-weight: 800; letter-spacing: 0.05em; text-transform: uppercase; opacity: 0.6; white-space: nowrap; overflow: hidden;", "{sub}" }
            }
            span { style: "font-size: 14px; font-weight: 700; white-space: nowrap; overflow: hidden;", "{name}" }
        }
    }
}

/// A run of tiles under a heading.
#[component]
fn Section(title: String, children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 8px;",
            if !title.is_empty() {
                span { style: "font-size: 11px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {DIM};", "{title}" }
            }
            // Rows of tiles, laid out as fixed rows (a wrapped flex row in a
            // scroller is placed above its top in Blitz).
            div { style: "display: grid; grid-template-columns: repeat(auto-fill, 168px); gap: 8px;", {children} }
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
                    ),
                    None => (LibraryModel::default(), Vec::<PatchInfo>::new(), CompositionModel::default()),
                }
            }
        }
    });
    let (lib, patches, comp) = data.read().clone().unwrap_or_default();
    let blocks: Vec<LiveBlock> = (state.blocks)();
    let nodes = (state.nodes)();

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

    // What plays, and the step, per tab.
    let current = on();
    let (head_label, head_name, head_sub): (String, String, String) = match &current {
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
    };
    let step = {
        let current = current.clone();
        let (lib, patches, comp, blocks, nodes, model) = (lib.clone(), patches.clone(), comp.clone(), blocks.clone(), nodes.clone(), model.clone());
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
                    let Some(node) = nodes.iter().find(|n| n.name.eq_ignore_ascii_case(name) && !n.alternatives.is_empty()) else { return };
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

    // The strip: the block or pedal it was opened on, then what plays,
    // then the modules.
    let mut tabs: Vec<BrowseTab> = front.clone();
    tabs.extend([BrowseTab::Profiles, BrowseTab::Patches, BrowseTab::Songs, BrowseTab::Setlists]);
    tabs.extend(BrowseTab::MODULES.iter().map(|m| BrowseTab::Module(m)));
    let arrow = format!("flex: 0 0 64px; height: 48px; display: flex; align-items: center; justify-content: center; border-radius: 10px; background: {RAISED}; border: 1px solid {RULE}; color: {TEXT}; font-size: 26px; font-weight: 700; cursor: pointer;");
    let back = step.clone();

    rsx! {
        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 10; display: flex; flex-direction: column; box-sizing: border-box; padding: 0 {trail}px 0 {lead}px; background: {BG}; color: {TEXT};",
            // ── The tabs ──
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; gap: 4px; padding: 6px 8px; overflow-x: auto; overflow-y: hidden; background: {BAR_BG}; border-bottom: 1px solid {RULE};",
                for t in tabs {
                    {
                        let look = tile(t == current);
                        let label = t.label();
                        let key = format!("{t:?}");
                        rsx! {
                            div { key: "{key}", style: "flex: 1 0 auto; min-width: 64px; padding: 0 8px; box-sizing: border-box; height: 34px; display: flex; align-items: center; justify-content: center; border-radius: 8px; font-size: 12px; font-weight: 800; cursor: pointer; overflow: hidden; white-space: nowrap; {look}",
                                onclick: move |_| on.set(t.clone()),
                                "{label}"
                            }
                        }
                    }
                }
            }
            // ── What plays, stepped ──
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; align-items: center; gap: 10px; padding: 8px 10px; border-bottom: 1px solid {RULE};",
                div { style: "{arrow}", onclick: move |_| back(-1), "‹" }
                div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; align-items: center;",
                    span { style: "font-size: 10px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {DIM};", "{head_label}" }
                    span { style: "font-size: 18px; font-weight: 800; color: #f4f4f5; white-space: nowrap; overflow: hidden;", "{head_name}" }
                    if !head_sub.is_empty() {
                        span { style: "font-size: 12px; font-weight: 600; color: {DIM}; white-space: nowrap;", "{head_sub}" }
                    }
                }
                div { style: "{arrow}", onclick: move |_| step(1), "›" }
                div { style: "flex: 0 0 auto; padding: 12px 18px; border-radius: 10px; font-size: 15px; font-weight: 700; cursor: pointer; background: {RAISED}; border: 1px solid {RULE};",
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
                                Tile { key: "{p.name}", name: p.name.clone(), sub: format!("{} patches", p.patches), on: p.active,
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
                                Tile { key: "{i}-{s}", name: s.clone(), on: i as u32 == model.setlist_index,
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
                        let node = nodes.iter().find(|n| n.name.eq_ignore_ascii_case(&name) && !n.alternatives.is_empty()).cloned();
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
                                        Tile { key: "{a.id}", name: a.name.clone(), on: a.name.eq_ignore_ascii_case(&pedal),
                                            onpick: { let (id, with) = (n.id.clone(), a.id.clone()); move |()| { let (id, with) = (id.clone(), with.clone()); fire!(r => r.replace_node(id, with)); } },
                                        }
                                    }
                                }
                            } else {
                                span { style: "font-size: 14px; color: {DIM};", "This slot has no other pedals to choose." }
                            }
                        }
                    }
                }
            }
        }
    }
}
