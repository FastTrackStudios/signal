//! The rig on a phone, held sideways: the chain a page at a time.
//!
//! A phone in landscape (an iPhone 16 Pro is 874 × 402 points) has room
//! for one or two units at full height, not a row of them. So the chain is
//! cut into pages, in the order a player thinks of it — the front of the
//! board, the Core, the effects after it — and the page that is up fills
//! the screen:
//!
//! - along the top, the status: the profile (or song) and patch (‹ name ›;
//!   tap the name for every profile and patch), the page's module presets
//!   (the top-level module — the Core, Time — then the page's own — Drive,
//!   Amp, Delay, Reverb), and the audio;
//! - the page itself, its units laid out for the room;
//! - along the bottom edge, the chain: a thin segment a page, coloured by
//!   where it sits (Core white, Time blue and purple, the rest grey), the
//!   page that is up lit. Tap one to go there — or the rail's Chain for a
//!   tall one over the page, easier to hit, that drops back once a page is
//!   picked (and scrolls sideways when a chain has more pages than fit).

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LiveBlock, PerformanceModel};
use signal_proto::block::BlockType;

use crate::control::{
    DelayPanel, LiveComp, LiveEq, ModGroupPanel, ModuleControls, ReverbPanel, ZoomPanel, find_block, MOD_KINDS,
    MOTION_KINDS,
};
use crate::library::Kind;
use crate::rig_faces::PrePart;
use crate::state::RigViewState;

/// A page of the chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Input,
    Pedals,
    Pitch,
    Filter,
    PreModTrem,
    PreDelayVerb,
    PreComp,
    Drives,
    Amps,
    Eq,
    GatePostComp,
    ModMotion,
    Delays,
    Reverbs,
}

/// Where a page sits: the colour of its segment, and the module presets
/// its bar shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Home {
    Front,
    Core,
    Post,
    Time,
}

impl Page {
    pub const ALL: [Self; 14] = [
        Self::Input,
        Self::Pedals,
        Self::Pitch,
        Self::Filter,
        Self::PreModTrem,
        Self::PreDelayVerb,
        Self::PreComp,
        Self::Drives,
        Self::Amps,
        Self::Eq,
        Self::GatePostComp,
        Self::ModMotion,
        Self::Delays,
        Self::Reverbs,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Input => "Input · Transpose · Doubler",
            Self::Pedals => "Pedals · Wah · Dive · Volume",
            Self::Pitch => "Pitch · Octaver · Harmonizer",
            Self::Filter => "Filter",
            Self::PreModTrem => "Pre Mod · Pre Trem",
            Self::PreDelayVerb => "Pre Delay · Pre Verb",
            Self::PreComp => "Compressor",
            Self::Drives => "Drives",
            Self::Amps => "Amps",
            Self::Eq => "EQ",
            Self::GatePostComp => "Gate · Post Comp",
            Self::ModMotion => "Modulation · Motion",
            Self::Delays => "Delays",
            Self::Reverbs => "Reverbs",
        }
    }

    /// Its name in a file or a setting (`drives`, `gate-post-comp`).
    pub fn slug(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Pedals => "pedals",
            Self::Pitch => "pitch",
            Self::Filter => "filter",
            Self::PreModTrem => "pre-mod-trem",
            Self::PreDelayVerb => "pre-delay-verb",
            Self::PreComp => "compressor",
            Self::Drives => "drives",
            Self::Amps => "amps",
            Self::Eq => "eq",
            Self::GatePostComp => "gate-post-comp",
            Self::ModMotion => "mod-motion",
            Self::Delays => "delays",
            Self::Reverbs => "reverbs",
        }
    }

    /// Its segment's label on the chain rail.
    fn short(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Pedals => "Pedals",
            Self::Pitch => "Pitch",
            Self::Filter => "Filter",
            Self::PreModTrem => "P.Mod",
            Self::PreDelayVerb => "P.Dly",
            Self::PreComp => "Comp",
            Self::Drives => "Drive",
            Self::Amps => "Amp",
            Self::Eq => "EQ",
            Self::GatePostComp => "Gate",
            Self::ModMotion => "Mod",
            Self::Delays => "Delay",
            Self::Reverbs => "Verb",
        }
    }

    fn home(self) -> Home {
        match self {
            Self::Input | Self::Pedals | Self::Pitch | Self::Filter | Self::PreModTrem | Self::PreDelayVerb => Home::Front,
            Self::PreComp | Self::Drives | Self::Amps | Self::Eq | Self::GatePostComp => Home::Core,
            Self::ModMotion => Home::Post,
            Self::Delays | Self::Reverbs => Home::Time,
        }
    }

    /// Its segment's colour (the modules' colours on the separators).
    fn color(self) -> &'static str {
        match self {
            Self::Drives => "#fbbf24",
            Self::Amps => "#f87171",
            Self::Delays => "#60a5fa",
            Self::Reverbs => "#c4b5fd",
            _ => match self.home() {
                Home::Core => "#e4e4e7",
                Home::Time => "#93c5fd",
                Home::Front | Home::Post => "#a1a1aa",
            },
        }
    }
}

/// What the rail picks: the chain's pages, the footswitches, or editing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Control,
    Switch,
    Edit,
}

/// The bars' colours (the Sessions app's, so the two read as one family).
const BAR_BG: &str = "#17181b";
const RULE: &str = "#2a2c31";
const RAISED: &str = "#26292f";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
/// The status line's height (along the top).
const LINE_H: u32 = 40;
/// The chain along the bottom edge, small (the rail's Chain raises it).
const CHAIN_H: u32 = 22;
/// The raised chain, as a share of the view's height.
const CHAIN_TALL: &str = "62%";
/// One page of the raised chain at its narrowest: past this the chain
/// scrolls sideways instead of squeezing.
const CHAIN_TILE_MIN: u32 = 46;
/// How far the bars keep clear of the screen's rounded corners.
const CORNER: u32 = 16;
/// The rail down the left.
const RAIL_W: u32 = 60;
/// How far the page keeps clear of the camera housing on the right (the
/// island: 37 points wide, 11 in from the edge).
const HOUSING: u32 = 54;

/// What the app around the rig gives it on a phone: the way back to the
/// instrument menu (the iPhone app's front door). Absent where the rig is
/// the whole app (the Android remote, the desktop shot tool).
#[derive(Clone, Copy)]
pub struct PhoneHost {
    pub on_home: Callback<()>,
}

/// Which side the camera housing is on, for a phone on its side: `true`
/// the left. The page keeps [`HOUSING`] clear on that side. Provided by the
/// iOS shell (it reads the window scene's orientation); without it the
/// housing is taken to be on the right.
#[derive(Clone, Copy)]
pub struct IslandLeft(pub Signal<bool>);

/// The phone's rig surface: the patch along the top, the rail down the
/// left (Control, Switch, Edit; Profile or Song), and the view — in
/// Control, a page of the chain over the chain itself.
#[component]
pub fn PhoneControl(
    model: PerformanceModel,
    state: RigViewState,
    /// The footswitches, for Switch.
    #[props(default = VNode::empty())]
    switches: Element,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = try_use_context::<PhoneHost>();
    let island_left = try_use_context::<IslandLeft>().is_some_and(|s| (s.0)());
    // The housing's clearance: the rail moves out from under it on the
    // left; on the right the view keeps clear of it itself.
    let (lead, trail) = if island_left { (HOUSING, 0) } else { (0, HOUSING) };
    // `FTS_PHONE_PAGE=<slug>`: open on that page (the shot tool renders
    // every page at once, an app each).
    let mut page = use_signal(|| {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = std::env::var("FTS_PHONE_PAGE").ok().and_then(|s| Page::ALL.into_iter().find(|p| p.slug() == s)) {
            return p;
        }
        Page::Amps
    });
    let mut mode = use_signal(|| Mode::Control);
    // The chain raised over the page, for picking one (`FTS_PHONE_CHAIN=tall`
    // opens it so, for the shot tool).
    let mut chain_tall = use_signal(|| {
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var("FTS_PHONE_CHAIN").is_ok_and(|v| v == "tall") {
            return true;
        }
        false
    });
    // The profile and patch picker, over the whole screen.
    let mut picker = use_signal(|| false);
    // `FTS_PHONE_TOUR=<secs>`: step through every page on a timer — to look
    // at each without touching the screen.
    #[cfg(not(target_arch = "wasm32"))]
    use_future(move || async move {
        let Some(secs) = std::env::var("FTS_PHONE_TOUR").ok().and_then(|s| s.parse::<u64>().ok()) else { return };
        for p in Page::ALL.iter().cycle() {
            page.set(*p);
            tracing::info!(page = ?p, "phone tour");
            architect::platform::sleep(std::time::Duration::from_secs(secs)).await;
        }
    });
    let blocks = state.blocks.cloned();
    // The libraries and what plays, for the faces' preset steppers and the
    // module bars (as the Control view provides them).
    let mut comp_rev = use_signal(|| model.revision);
    if *comp_rev.peek() != model.revision {
        comp_rev.set(model.revision);
    }
    let compositions = use_resource({
        let rig = rig.clone();
        move || {
            let _ = comp_rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.compositions().await.unwrap_or_default(),
                    None => signal_guitar_proto::CompositionModel::default(),
                }
            }
        }
    });
    let face_presets = use_context_provider(|| crate::face_chrome::FacePresets(Signal::new(Default::default())));
    use_context_provider(|| crate::face_chrome::VizMode(Signal::new(std::collections::HashSet::new())));
    use_effect(move || {
        if let Some(c) = compositions.read().as_ref() {
            let mut s = face_presets.0;
            if *s.peek() != *c {
                s.set(c.clone());
            }
        }
    });
    let pick_of = |module: &str| {
        compositions
            .read()
            .as_ref()
            .and_then(|c| c.active_modules.iter().find(|m| m.module.eq_ignore_ascii_case(module)).cloned())
    };
    let all_modules: Vec<signal_guitar_proto::ModulePresetEntry> = compositions.read().as_ref().map(|c| c.modules.clone()).unwrap_or_default();
    let edited: Vec<String> = blocks.iter().filter(|b| b.overridden).map(|b| b.name.clone()).collect();
    // The page's module presets, for the top line.
    let modules: Vec<(Kind, &'static str)> = match (mode(), page()) {
        (Mode::Control, Page::Drives) => vec![(Kind::Compositions, "Core"), (Kind::DriveModules, "Drive")],
        (Mode::Control, Page::Amps) => vec![(Kind::Compositions, "Core"), (Kind::AmpModules, "Amp")],
        (Mode::Control, p) if p.home() == Home::Core => vec![(Kind::Compositions, "Core")],
        (Mode::Control, Page::Delays) => vec![(Kind::TimeModules, "Time"), (Kind::DelayModules, "Delay")],
        (Mode::Control, Page::Reverbs) => vec![(Kind::TimeModules, "Time"), (Kind::ReverbModules, "Reverb")],
        _ => Vec::new(),
    };
    // Profile or Song: the perform mode, shared with every remote.
    let song = model.perform_mode == 2;
    let set_play = {
        let rig = rig.clone();
        move |m: u32| {
            let rig = rig.clone();
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.set_perform_mode(m).await;
                }
            });
        }
    };
    let play_song = set_play.clone();

    // What plays: the song in Song, else the profile.
    let playing = if song {
        model.songs.get(model.song_index as usize).map_or_else(|| model.profile_name.clone(), |s| s.name.clone())
    } else {
        model.profile_name.clone()
    };
    // One chain segment: thin along the bottom, a tall tile when raised.
    let segment = move |p: Page, tall: bool| {
        let on = page() == p;
        let color = p.color();
        let look = if on { format!("background: {color}; color: #0a0b0d;") } else { format!("background: {RAISED}; color: {color};") };
        let size = if tall {
            format!("flex: 1 0 {CHAIN_TILE_MIN}px; flex-direction: column; gap: 6px; border-radius: 6px; font-size: 13px;")
        } else {
            "flex: 1 1 0%; border-radius: 3px; font-size: 9px;".to_string()
        };
        rsx! {
            div { key: "{p.short()}",
                style: "min-width: 0; display: flex; align-items: center; justify-content: center; font-weight: 800; letter-spacing: 0.04em; cursor: pointer; overflow: hidden; {size} {look}",
                onclick: move |_| {
                    page.set(p);
                    chain_tall.set(false);
                },
                span { "{p.short()}" }
                if tall {
                    span { style: "font-size: 8px; font-weight: 700; opacity: 0.7; text-align: center; padding: 0 3px;", "{p.title()}" }
                }
            }
        }
    };

    rsx! {
        // The whole screen: the view runs under the camera housing on
        // whichever side it is, and what sits beside it keeps clear itself.
        div { style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; min-height: 0; box-sizing: border-box; padding-left: {lead}px; background: #0f1012; color: {TEXT};",
            // ── The status, along the top: what plays (tap for every profile
            // and patch), the page's module presets — the top-level module,
            // then the page's own — and the audio ──
            div { style: "flex: 0 0 {LINE_H}px; display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; padding: 0 {CORNER}px 0 8px; background: {BAR_BG}; border-bottom: 1px solid {RULE}; min-width: 0;",
                PatchStepper { revision: model.revision, profile: playing, on_pick: move |()| picker.set(true) }
                for (kind, module) in modules {
                    div { key: "{module}", style: "flex: 0 1 200px; min-width: 0; height: 30px; display: flex;",
                        ModuleControls { kind, pick: pick_of(module), modules: all_modules.clone(), edited: edited.clone(), style: "width: 100%; height: 100%;" }
                    }
                }
                div { style: "flex: 1 1 0%;" }
                if mode() == Mode::Control && page().home() == Home::Core {
                    div { style: "flex: 0 0 110px;", crate::face_chrome::CoreFreeze {} }
                }
                AudioBadge { running: (state.running)(), error: (state.audio_error)() }
            }
            div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: row;",
                // ── The rail ──
                div { style: "flex: 0 0 {RAIL_W}px; display: flex; flex-direction: column; justify-content: center; gap: 2px; background: {BAR_BG}; border-right: 1px solid {RULE};",
                    if let Some(host) = host {
                        RailButton { on: false, label: "Rigs", icon: "Rigs", onclick: move |()| host.on_home.call(()) }
                        div { style: "height: 1px; margin: 6px 12px; background: {RULE};" }
                    }
                    for (m, label) in [(Mode::Control, "Control"), (Mode::Switch, "Switch"), (Mode::Edit, "Edit")] {
                        RailButton { key: "{label}", on: mode() == m, label, icon: label, onclick: move |()| mode.set(m) }
                    }
                    // The chain, raised over the page for picking one.
                    RailButton { on: chain_tall(), label: "Chain", icon: "Chain",
                        onclick: move |()| {
                            if mode() == Mode::Control {
                                chain_tall.toggle();
                            } else {
                                mode.set(Mode::Control);
                                chain_tall.set(true);
                            }
                        },
                    }
                    div { style: "height: 1px; margin: 6px 12px; background: {RULE};" }
                    RailButton { on: !song, label: "Profile", icon: "Profile", onclick: move |()| set_play(1) }
                    RailButton { on: song, label: "Song", icon: "Song", onclick: move |()| play_song(2) }
                    div { style: "height: 1px; margin: 6px 12px; background: {RULE};" }
                    RailButton { on: false, label: "Audio", icon: "Audio", onclick: move |()| crate::settings::open_audio_settings() }
                }
                // ── The view ──
                div { style: "position: relative; flex: 1 1 0%; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                    match mode() {
                        Mode::Control => rsx! {
                            div { style: "flex: 1 1 0%; min-height: 0; display: flex; overflow: hidden; box-sizing: border-box; padding-right: {trail}px;",
                                PageView { page: page(), blocks: blocks.clone(), state, tempo_bpm: model.tempo_bpm }
                            }
                            // The chain, thin, on the screen's bottom edge.
                            div { style: "flex: 0 0 {CHAIN_H}px; display: flex; flex-direction: row; gap: 2px; box-sizing: border-box; padding: 3px {CORNER}px 3px 3px; background: {BAR_BG}; border-top: 1px solid {RULE};",
                                for p in Page::ALL {
                                    {segment(p, false)}
                                }
                            }
                            // Raised: over the page's lower part, the page
                            // above it dimmed (tap there to drop it back).
                            // Mounted only while up — a hidden layer still
                            // takes presses in Blitz.
                            if chain_tall() {
                                div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 5; display: flex; flex-direction: column;",
                                    div { style: "flex: 1 1 0%; background: rgba(0, 0, 0, 0.55);", onclick: move |_| chain_tall.set(false) }
                                    div { style: "flex: 0 0 {CHAIN_TALL}; display: flex; flex-direction: row; gap: 4px; box-sizing: border-box; padding: 8px {CORNER}px 8px 8px; overflow-x: auto; overflow-y: hidden; background: {BAR_BG}; border-top: 1px solid {RULE};",
                                        for p in Page::ALL {
                                            {segment(p, true)}
                                        }
                                    }
                                }
                            }
                        },
                        Mode::Switch => rsx! {
                            div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: column; box-sizing: border-box; padding: 6px {trail}px 6px 6px;",
                                {switches}
                            }
                        },
                        Mode::Edit => rsx! {
                            div { style: "flex: 1 1 0%; display: flex; align-items: center; justify-content: center; color: {DIM}; font-size: 12px; padding-right: {trail}px;",
                                "Edit — coming next"
                            }
                        },
                    }
                }
            }
            if picker() {
                PatchPicker { revision: model.revision, lead, trail, on_close: move |()| picker.set(false) }
            }
        }
    }
}

/// Whether the audio is running, small, in the bottom line: the surface works
/// without it (a phone with no interface plugged in plays nothing, but every
/// patch and page is there to edit). Tap for Audio settings.
#[component]
fn AudioBadge(running: bool, error: String) -> Element {
    let (dot, label, color) = if running {
        ("#22c55e", "", DIM)
    } else if !error.is_empty() {
        ("#ef4444", "Audio error", "#fca5a5")
    } else {
        ("#f59e0b", "No audio", "#fcd34d")
    };
    rsx! {
        div { style: "flex: 0 0 auto; height: 26px; display: flex; align-items: center; gap: 6px; padding: 0 9px; border-radius: 13px; background: {RAISED}; cursor: pointer;",
            title: if error.is_empty() { "Audio" } else { "{error}" },
            onclick: move |_| crate::settings::open_audio_settings(),
            span { style: "width: 7px; height: 7px; border-radius: 4px; background: {dot};" }
            if !label.is_empty() {
                span { style: "font-size: 10px; font-weight: 700; letter-spacing: 0.04em; color: {color}; white-space: nowrap;", "{label}" }
            }
        }
    }
}

/// A rail button: its icon, its name small under it; the one picked
/// raised.
#[component]
fn RailButton(on: bool, label: &'static str, icon: &'static str, onclick: EventHandler<()>) -> Element {
    let (fg, bg) = if on { (TEXT, RAISED) } else { (DIM, "transparent") };
    rsx! {
        div { style: "height: 48px; margin: 0 4px; border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; background: {bg}; cursor: pointer;",
            onclick: move |_| onclick.call(()),
            RailIcon { name: icon, color: fg }
            span { style: "font-size: 9px; font-weight: 600; color: {fg};", "{label}" }
        }
    }
}

/// The rail's glyphs, stroked on a 24 × 24 box (presentation attributes:
/// Blitz styles SVG geometry there, not in CSS).
#[component]
fn RailIcon(name: &'static str, color: &'static str) -> Element {
    let paths: &[&str] = match name {
        // Faders.
        "Control" => &["M6 4v16", "M12 4v16", "M18 4v16", "M4 9h4", "M10 15h4", "M16 7h4"],
        // The footswitches: two rows of three.
        "Switch" => &["M3 6h5v5H3z", "M10 6h4v5h-4z", "M16 6h5v5h-5z", "M3 14h5v5H3z", "M10 14h4v5h-4z", "M16 14h5v5h-5z"],
        // A pencil.
        "Edit" => &["M4 20l4-1 11-11-3-3L5 16z", "M14 7l3 3"],
        // A person.
        "Profile" => &["M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8z", "M4 21c1-4 4-6 8-6s7 2 8 6"],
        // Back to the instrument menu.
        "Rigs" => &["M14 6l-6 6 6 6"],
        // The chain: a row of segments, raised.
        "Chain" => &["M3 15h4v5H3z", "M10 15h4v5h-4z", "M17 15h4v5h-4z", "M8 9l4-4 4 4"],
        // A speaker.
        "Audio" => &["M4 9h3l4-3.5v13L7 15H4z", "M15 9.5a4 4 0 0 1 0 5", "M17.5 7a7.5 7.5 0 0 1 0 10"],
        // A note.
        _ => &["M9 18V5l11-2v13", "M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0z", "M20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z"],
    };
    rsx! {
        svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none", stroke: color, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
            for d in paths.iter() {
                path { d: *d }
            }
        }
    }
}

/// The patch playing, stepped ‹ › through the profile's patches.
#[component]
fn PatchStepper(revision: u64, profile: String, on_pick: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let patches = use_resource({
        let rig = rig.clone();
        move || {
            let _ = revision;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.patches().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let list = patches.read().clone().unwrap_or_default();
    let at = list.iter().position(|p| p.active);
    let name = at.and_then(|i| list.get(i)).map_or_else(|| "—".to_string(), |p| p.name.clone());
    let n = list.len();
    let step = move |d: i64| {
        if n == 0 {
            return;
        }
        let next = at.map_or(0, |i| (i as i64 + d).rem_euclid(n as i64) as u32);
        let rig = rig.clone();
        spawn(async move {
            if let Some(r) = rig {
                let _ = r.select_patch(next).await;
            }
        });
    };
    let arrow = "font-size: 15px; line-height: 1; color: #a1a1aa; padding: 0 6px; cursor: pointer;";
    let back = step.clone();
    rsx! {
        div { style: "display: flex; align-items: center; gap: 2px; min-width: 0;",
            span { style: "{arrow}", onclick: move |_| back(-1), "‹" }
            // The profile over the patch; tap for every profile and patch.
            div { style: "display: flex; flex-direction: column; justify-content: center; min-width: 0; max-width: 160px; cursor: pointer;",
                onclick: move |_| on_pick.call(()),
                span { style: "font-size: 9px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {DIM}; white-space: nowrap; overflow: hidden;", "{profile}" }
                span { style: "font-size: 13px; font-weight: 700; color: #f4f4f5; white-space: nowrap; overflow: hidden;", "{name}" }
            }
            span { style: "{arrow}", onclick: move |_| step(1), "›" }
        }
    }
}

/// Every profile, and the patches of the one that plays: tap a profile to
/// load it, a patch to play it (and close). Over the whole screen — a
/// phone has no room beside it.
#[component]
fn PatchPicker(revision: u64, lead: u32, trail: u32, on_close: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let data = use_resource({
        let rig = rig.clone();
        move || {
            let _ = revision;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => (r.library().await.unwrap_or_default().profiles, r.patches().await.unwrap_or_default()),
                    None => (Vec::new(), Vec::new()),
                }
            }
        }
    });
    let (profiles, patches) = data.read().clone().unwrap_or_default();
    let load = {
        let rig = rig.clone();
        move |name: String| {
            let rig = rig.clone();
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.select_profile(name).await;
                }
            });
        }
    };
    let play = move |index: u32| {
        let rig = rig.clone();
        spawn(async move {
            if let Some(r) = rig {
                let _ = r.select_patch(index).await;
            }
        });
        on_close.call(());
    };
    let chip = |on: bool| {
        if on {
            "background: #f4f4f5; color: #0a0b0d; border: 1px solid #f4f4f5;".to_string()
        } else {
            format!("background: {RAISED}; color: {TEXT}; border: 1px solid {RULE};")
        }
    };
    rsx! {
        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 10; display: flex; flex-direction: column; box-sizing: border-box; padding: 0 {trail}px 0 {lead}px; background: #0f1012; color: {TEXT};",
            // Profiles, along the top.
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 8px 10px; background: {BAR_BG}; border-bottom: 1px solid {RULE}; overflow: hidden;",
                span { style: "flex: 0 0 auto; font-size: 10px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {DIM}; margin-right: 4px;", "Profile" }
                for p in profiles {
                    {
                        let name = p.name.clone();
                        let load = load.clone();
                        let look = chip(p.active);
                        rsx! {
                            div { key: "{p.name}",
                                style: "flex: 0 0 auto; padding: 6px 14px; border-radius: 5px; font-size: 13px; font-weight: 700; white-space: nowrap; cursor: pointer; {look}",
                                onclick: move |_| load(name.clone()),
                                "{p.name}"
                            }
                        }
                    }
                }
                div { style: "flex: 1 1 0%;" }
                div { style: "flex: 0 0 auto; padding: 6px 14px; border-radius: 5px; font-size: 13px; font-weight: 700; cursor: pointer; background: {RAISED}; color: {TEXT}; border: 1px solid {RULE};",
                    onclick: move |_| on_close.call(()),
                    "Done"
                }
            }
            // Its patches.
            div { style: "flex: 1 1 0%; min-height: 0; overflow: auto; padding: 10px; display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start; gap: 6px;",
                for (i, p) in patches.into_iter().enumerate() {
                    {
                        let play = play.clone();
                        let look = chip(p.active);
                        rsx! {
                            div { key: "{i}-{p.name}",
                                style: "flex: 0 0 auto; min-width: 120px; box-sizing: border-box; padding: 8px 12px; border-radius: 5px; display: flex; flex-direction: column; gap: 2px; cursor: pointer; {look}",
                                onclick: move |_| play(i as u32),
                                if !p.stack.is_empty() {
                                    span { style: "font-size: 9px; font-weight: 800; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.6; white-space: nowrap;", "{p.stack}" }
                                }
                                span { style: "font-size: 13px; font-weight: 700; white-space: nowrap;", "{p.name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One page of the chain, filling its box.
#[component]
fn PageView(page: Page, blocks: Vec<LiveBlock>, state: RigViewState, tempo_bpm: u32) -> Element {
    let only = |kinds: &[&str]| kinds.iter().map(|k| (*k).to_string()).collect::<Vec<_>>();
    match page {
        Page::Input => rsx! { crate::rig_faces::InputRow { blocks, only: only(&["transpose", "doubler"]) } },
        Page::Pedals => rsx! { crate::rig_faces::InputRow { blocks, only: only(&["wah", "dive", "volume"]) } },
        Page::Pitch => rsx! { crate::rig_faces::InputRow { blocks, only: only(&["pitch", "harmony"]) } },
        Page::Filter => rsx! { crate::rig_faces::InputRow { blocks, only: only(&["filter"]) } },
        Page::PreModTrem => rsx! { crate::rig_faces::PreFxRow { blocks, tempo_bpm, part: PrePart::Units } },
        Page::PreDelayVerb => rsx! { crate::rig_faces::PreFxRow { blocks, tempo_bpm, part: PrePart::Lanes } },
        Page::PreComp => {
            let comp = find_block(&blocks, BlockType::Compressor, "Pre Comp");
            rsx! {
                ZoomPanel { title: "Compressor".to_string(), left_power_on: comp.as_ref().map(|b| !b.bypassed),
                    if let Some(c) = comp { LiveComp { block: c, state } }
                }
            }
        }
        Page::Drives => {
            // Every pedal across the page's width (the screen less the rail
            // and the housing's clearance).
            let (w, h) = try_use_context::<crate::control::WindowSize>().map_or((874.0, 381.0), |s| (s.0)());
            // The page's height less the bars and the board's rails.
            let page_h = h - f64::from(LINE_H) - f64::from(CHAIN_H) - 24.0;
            rsx! { crate::rig_faces::DrivesRow { blocks, fit_width: Some(w - f64::from(RAIL_W + HOUSING)), fit_height: Some(page_h) } }
        }
        Page::Amps => rsx! { crate::rig_faces::AmpRow { blocks, amps_only: true } },
        Page::Eq => {
            let eq = find_block(&blocks, BlockType::Eq, "Amp EQ");
            rsx! {
                ZoomPanel { title: "EQ".to_string(), left_power_on: eq.as_ref().map(|b| !b.bypassed),
                    if let Some(e) = eq {
                        div { style: "position: relative; width: 100%; height: 100%;",
                            LiveEq { block: e.clone(), state }
                            crate::rig_faces::PresetCorner { block: e.name.clone(), block_type: "eq".to_string() }
                        }
                    }
                }
            }
        }
        Page::GatePostComp => {
            // Stacked, as the time pages: the gate over the post
            // compressor, each its face's phone lane.
            let tier = crate::control::use_tier();
            let faces = crate::rig_faces::use_faces();
            let gate = blocks.iter().find(|b| b.block_type == BlockType::Gate).cloned();
            let post = find_block(&blocks, BlockType::Compressor, "Post Comp");
            rsx! {
                div { style: "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0;",
                    if let (Some(g), Some(f)) = (gate, faces.gate.clone()) {
                        div { style: "flex: 1 1 0%; min-height: 0; display: flex;",
                            crate::rig_faces::GateFace { block: g, face: f.at(tier), level: state.in_peak_db, fill: true }
                        }
                    }
                    div { style: "flex: 1 1 0%; min-height: 0; display: flex; position: relative;",
                        if let (Some(c), Some(f)) = (post.clone(), faces.post_comp.clone()) {
                            crate::rig_faces::BlockFace { block: c, face: f.at(tier), fill: true, preset_type: Some("compressor".to_string()) }
                        } else if let Some(c) = post {
                            LiveComp { block: c, state }
                        }
                    }
                }
            }
        }
        Page::ModMotion => rsx! {
            // Stacked, as the delays and reverbs: Modulation over Motion,
            // each its face's lane (no panel chrome over the nameplate).
            div { style: "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0;",
                div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: column;",
                    ModGroupPanel { title: "Mod", kinds: MOD_KINDS.to_vec(), blocks: blocks.clone(), tempo_bpm }
                }
                div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: column;",
                    ModGroupPanel { title: "Motion", kinds: MOTION_KINDS.to_vec(), blocks: blocks.clone(), tempo_bpm, tempo_divisions: true }
                }
            }
        },
        Page::Delays => rsx! {
            ZoomPanel { title: "Delay".to_string(), flush: true, module: Some("Delay"),
                DelayPanel { blocks, tempo_bpm }
            }
        },
        Page::Reverbs => rsx! {
            ZoomPanel { title: "Reverb".to_string(), flush: true, module: Some("Reverb"),
                ReverbPanel { blocks, tempo_bpm }
            }
        },
    }
}
