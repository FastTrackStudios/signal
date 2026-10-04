//! The rig on a phone, held sideways: the chain a page at a time.
//!
//! A phone in landscape (an iPhone 16 Pro is 874 × 402 points) has room
//! for one or two units at full height, not a row of them. So the chain is
//! cut into pages, in the order a player thinks of it — the front of the
//! board, the Core, the effects after it — and the page that is up fills
//! the screen:
//!
//! - along the top, the status — names only, each a tap from its picker
//!   (‹ › steppers and everything to choose from): the way back, the
//!   profile (or song) and patch, the page's module presets (the top-level
//!   module — the Core, Time — then the page's own — Drive, Amp, Delay,
//!   Reverb), and the audio;
//! - under it, the chain: a thin segment a page, coloured by where it
//!   sits (Core white, Time blue and purple, the rest grey), the page that
//!   is up lit. Tap one to go there — or the page's button on the status
//!   line to drop a tall one down over the page, easier to hit, that goes back up
//!   once a page is picked (and scrolls sideways when a chain has more
//!   pages than fit);
//! - the page itself, its units laid out for the room — swipe in from its
//!   right edge for the next page, from its left for the one before.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LiveBlock, PerformanceModel};
use signal_proto::block::BlockType;

use crate::control::{
    DelayPanel, LiveComp, LiveEq, ModGroupPanel, ReverbPanel, ZoomPanel, find_block, MOD_KINDS,
    MOTION_KINDS,
};
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

impl Page {
    /// The page's icon (24-unit strokes), in the middle of its raised tile.
    fn icon(self) -> &'static [&'static str] {
        match self {
            // A jack plug.
            Self::Input => &["M12 2v6", "M8 8h8v5a4 4 0 0 1-8 0z", "M12 17v5"],
            // An expression pedal, rocked.
            Self::Pedals => &["M4 20h16", "M6 20L9 6l10 3-3 11"],
            // A note, raised.
            Self::Pitch => &["M9 18V6l6-2", "M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0z", "M18 14V4", "M15 7l3-3 3 3"],
            // A low-pass curve.
            Self::Filter => &["M3 8h8c4 0 5 10 10 10"],
            // One wave.
            Self::PreModTrem => &["M2 12c2.5-6 5-6 7.5 0s5 6 7.5 0 3.5-3 5-3"],
            // Repeats dying away.
            Self::PreDelayVerb => &["M4 6v12", "M10 9v6", "M15 10.5v3", "M19 11.5v1"],
            // Squeezed in from both sides.
            Self::PreComp => &["M12 3v6", "M9 6l3 3 3-3", "M12 21v-6", "M9 18l3-3 3 3", "M4 12h16"],
            // A flame.
            Self::Drives => &["M12 3c1 4 5 5 5 10a5 5 0 0 1-10 0c0-3 2-4 2-7 1 1 2 2 3 4"],
            // A combo: its panel and its speaker.
            Self::Amps => &["M4 5h16v14H4z", "M4 9h16", "M9 14a3 3 0 1 0 6 0 3 3 0 1 0-6 0"],
            // Three bands.
            Self::Eq => &["M6 4v16", "M12 4v16", "M18 4v16", "M4 14h4", "M10 8h4", "M16 16h4"],
            // A gate, open.
            Self::GatePostComp => &["M3 18h5V6h8v12h5"],
            // Two waves.
            Self::ModMotion => &["M2 9c2.5-4 5-4 7.5 0s5 4 7.5 0 3.5-2 5-2", "M2 15c2.5-4 5-4 7.5 0s5 4 7.5 0 3.5-2 5-2"],
            // Echoes, each smaller.
            Self::Delays => &[
                "M3 12a2.5 2.5 0 1 0 5 0 2.5 2.5 0 1 0-5 0",
                "M10.5 12a1.8 1.8 0 1 0 3.6 0 1.8 1.8 0 1 0-3.6 0",
                "M16.5 12a1.1 1.1 0 1 0 2.2 0 1.1 1.1 0 1 0-2.2 0",
                "M21 12h.01",
            ],
            // A room ringing out.
            Self::Reverbs => &["M12 12h.01", "M8.5 8.5a5 5 0 0 0 0 7", "M15.5 8.5a5 5 0 0 1 0 7", "M5.5 5.5a9 9 0 0 0 0 13", "M18.5 5.5a9 9 0 0 1 0 13"],
        }
    }
}

/// What the rail picks: the chain's pages, the footswitches, or audio.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Control,
    Switch,
    Audio,
}

/// The bars' colours (the Sessions app's, so the two read as one family).
const BAR_BG: &str = "#17181b";
const RULE: &str = "#2a2c31";
const RAISED: &str = "#26292f";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
/// The status line's height (along the top).
const LINE_H: u32 = 48;
/// The chain under the status line, small (the page's button drops it down).
const CHAIN_H: u32 = 24;
/// The raised chain, as a share of the view's height.
const CHAIN_TALL: &str = "31%";
/// One page of the raised chain at its narrowest: past this the chain
/// scrolls sideways instead of squeezing.
const CHAIN_TILE_MIN: u32 = 46;
/// The strips along the page's sides that a swipe to the next or previous
/// page starts in, and how far it must travel to turn the page.
const EDGE_W: u32 = 22;
const SWIPE_TURN: f64 = 40.0;
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

/// Present under the phone's surface: a face's algorithm or preset press
/// opens the browser (through `SelectedModule`) instead of a popup.
#[derive(Clone, Copy)]
pub struct PhoneBrowses;

/// The screen's corner radius in points, for what sits in its corners to
/// keep clear of the curve. Provided by the iOS shell; without it (the
/// Android remote, the shot tool) the corners are taken to be square.
#[derive(Clone, Copy)]
pub struct ScreenCorners(pub Signal<f64>);

/// How far in from one edge content must start to clear a screen corner
/// rounded at `r`, when it comes to within `e` of the other edge (a circle's
/// chord: nothing at `e >= r`, all of `r` at `e = 0`).
pub(crate) fn corner_clear(r: f64, e: f64) -> f64 {
    if r <= 0.0 || e >= r {
        return 0.0;
    }
    r - (r * r - (r - e) * (r - e)).sqrt()
}

/// The screen's corner radius from context (0 without the iOS shell).
pub(crate) fn screen_radius() -> f64 {
    try_use_context::<ScreenCorners>().map_or(0.0, |c| (c.0)())
}

/// The phone's rig surface: the status along the top, the rail down the
/// left (bottom up: Control, Profile/Setlist, Switch, Audio), and the view — in
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
    // What sits in the screen's rounded corners keeps clear of their curve
    // (the bars themselves run into them): how far in, for each corner's
    // content, from how close it comes to the other edge. Rounded up.
    let r = screen_radius();
    let clear = |e: f64| (corner_clear(r, e) + 2.0).ceil() as u32;
    // The status line's ends: the back arrow (8 down) and the audio badge
    // (5 down).
    let back_left = lead.max(clear(f64::from(LINE_H - 20) / 2.0));
    let status_right = CORNER.max(clear(f64::from(LINE_H - 26) / 2.0));
    // The rail's lowest button, 4 in from its side (past the housing, when
    // that is the rail's side).
    let rail_foot = 6u32.max(clear(f64::from(lead) + 4.0));
    // The pickers over the screen: their heads' buttons, 8 down.
    let sheet_left = lead.max(clear(8.0));
    let sheet_right = trail.max(clear(8.0));
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
    // The Audio tab up, shared by the top bar (where the tabs are, in Audio)
    // and the page.
    use_context_provider(|| crate::phone_audio::AudioTab(Signal::new(crate::phone_audio::AudioTab::initial())));
    // `FTS_PHONE_OPEN=audio|switch`: open on that mode, for the shot tool.
    #[cfg(not(target_arch = "wasm32"))]
    use_hook(|| match std::env::var("FTS_PHONE_OPEN").as_deref() {
        Ok("audio") => crate::settings::open_audio_settings(),
        Ok("switch") => mode.set(Mode::Switch),
        _ => {}
    });
    // Whatever asks for the audio settings (the badge, a banner, the shot
    // tool) gets the Audio mode: on a phone they live beside the rail.
    use_effect(move || {
        if *crate::settings::AUDIO_SETTINGS_OPEN.read() {
            *crate::settings::AUDIO_SETTINGS_OPEN.write() = false;
            mode.set(Mode::Audio);
        }
    });
    // An edge swipe under way: where it started, and which way it turns
    // (+1 from the right edge, the next page; -1 from the left, the one
    // before).
    let mut swipe = use_signal(|| None::<(f64, i32)>);
    // The chain raised over the page, for picking one (`FTS_PHONE_CHAIN=tall`
    // opens it so, for the shot tool).
    let mut chain_tall = use_signal(|| {
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var("FTS_PHONE_CHAIN").is_ok_and(|v| v == "tall") {
            return true;
        }
        false
    });
    use_context_provider(|| PhoneBrowses);
    // The browser over the screen, on a tab (closed: `None`).
    // `FTS_PHONE_BROWSE=patches|songs|module:Delay|block:DLY 1:delay|pedal:Drive 1`:
    // open on that tab, for the shot tool.
    let mut browse = use_signal(|| {
        use crate::phone_browser::BrowseTab;
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(v) = std::env::var("FTS_PHONE_BROWSE") {
            let mut parts = v.splitn(3, ':');
            return match (parts.next(), parts.next(), parts.next()) {
                (Some("patches"), ..) => Some(BrowseTab::Patches),
                (Some("profiles"), ..) => Some(BrowseTab::Profiles),
                (Some("songs"), ..) => Some(BrowseTab::Songs),
                (Some("setlists"), ..) => Some(BrowseTab::Setlists),
                (Some("module"), Some(m), _) => BrowseTab::for_module(m),
                (Some("block"), Some(n), Some(t)) => Some(BrowseTab::for_block(n, t)),
                (Some("pedal"), Some(n), _) => Some(BrowseTab::Pedal { name: n.to_string() }),
                _ => None,
            };
        }
        None::<BrowseTab>
    });
    // A face's preset name, its nameplate's browse, a panel's module: they
    // ask the desktop's sidebar and library for the presets, by these two
    // contexts — on a phone the browser answers them.
    let selected = try_use_context::<crate::module_sidebar::SelectedModule>();
    let library_open = try_use_context::<crate::library::OpenLibrary>();
    use_effect(move || {
        use crate::module_sidebar::Selection;
        if let Some(sel) = selected {
            let mut sig = sel.0;
            let asked = sig.read().clone();
            if let Some(asked) = asked {
                sig.set(None);
                let tab = match asked {
                    Selection::Module(m) => crate::phone_browser::BrowseTab::for_module(&m),
                    Selection::Block { name, block_type } => Some(crate::phone_browser::BrowseTab::for_block(&name, &block_type)),
                };
                if tab.is_some() {
                    browse.set(tab);
                }
            }
        }
    });
    use_effect(move || {
        use crate::library::Kind;
        use crate::phone_browser::BrowseTab;
        if let Some(lib) = library_open {
            let mut sig = lib.0;
            let asked = *sig.read();
            if let Some(kind) = asked {
                sig.set(None);
                let tab = match kind {
                    Kind::Profiles => Some(BrowseTab::Profiles),
                    Kind::Patches => Some(BrowseTab::Patches),
                    Kind::Songs => Some(BrowseTab::Songs),
                    Kind::Setlists => Some(BrowseTab::Setlists),
                    k => k.module().and_then(BrowseTab::for_module),
                };
                if tab.is_some() {
                    browse.set(tab);
                }
            }
        }
    });
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
    // The patch playing, for the status line.
    let patches = use_resource({
        let rig = rig.clone();
        move || {
            let _ = comp_rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.patches().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let patch_name = patches
        .read()
        .as_ref()
        .and_then(|l| l.iter().find(|p| p.active).map(|p| p.name.clone()))
        .unwrap_or_else(|| "—".to_string());
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
    // The page's module presets, for the top line.
    let modules: Vec<&'static str> = match (mode(), page()) {
        (Mode::Control, Page::Drives) => vec!["Core", "Drive"],
        (Mode::Control, Page::Amps) => vec!["Core", "Amp"],
        (Mode::Control, p) if p.home() == Home::Core => vec!["Core"],
        (Mode::Control, Page::Delays) => vec!["Time", "Delay"],
        (Mode::Control, Page::Reverbs) => vec!["Time", "Reverb"],
        _ => Vec::new(),
    };
    let (top_module, page_module) = (modules.first().copied(), modules.get(1).copied());
    // A module with edits on any block it owns (a pedal swapped into one of
    // its slots included): its name carries a `*`.
    let module_edited = |module: &str| {
        pick_of(module).is_some_and(|p| p.blocks.iter().any(|b| blocks.iter().any(|x| x.overridden && x.name.eq_ignore_ascii_case(b))))
    };
    // A module's preset as the status line names it.
    let name_of = |module: &str| {
        let (preset, snapshot) = pick_of(module).map(|p| (p.preset, p.snapshot)).unwrap_or_default();
        if snapshot.is_empty() { preset } else { format!("{preset} · {snapshot}") }
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

    // The rail's Profile/Setlist button: which one plays.
    let plays: &'static str = if song { "Setlist" } else { "Profile" };
    // What plays: the song in Song, else the profile.
    let playing = if song {
        model.songs.get(model.song_index as usize).map_or_else(|| model.profile_name.clone(), |s| s.name.clone())
    } else {
        model.profile_name.clone()
    };
    // The page with the chain down: dimmed, its presses its parent's.
    let page_veil = if chain_tall() { "opacity: 0.4; pointer-events: none;" } else { "" };
    // One chain segment: thin under the status line, a tall tile dropped down.
    let segment = move |p: Page, tall: bool| {
        let on = page() == p;
        let color = p.color();
        let look = if on { format!("background: {color}; color: #0a0b0d;") } else { format!("background: {RAISED}; color: {color};") };
        // Raised, the names sit along the tiles' tops, level with each other.
        // Under it, the page's icon in the middle of the tile.
        let ink = if on { "#0a0b0d" } else { color };
        let size = if tall {
            format!("flex: 1 0 {CHAIN_TILE_MIN}px; flex-direction: column; align-items: center; padding-top: 10px; border-radius: 6px; font-size: 13px;")
        } else {
            "flex: 1 1 0%; align-items: center; justify-content: center; border-radius: 3px; font-size: 11px;".to_string()
        };
        rsx! {
            div { key: "{p.short()}",
                style: "min-width: 0; display: flex; box-sizing: border-box; font-weight: 800; letter-spacing: 0.04em; cursor: pointer; overflow: hidden; {size} {look}",
                onclick: move |_| {
                    page.set(p);
                    chain_tall.set(false);
                },
                span { "{p.short()}" }
                if tall {
                    div { style: "flex: 1 1 0%; display: flex; align-items: center; justify-content: center;",
                        svg { width: "26", height: "26", view_box: "0 0 24 24", fill: "none", stroke: ink, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                            for d in p.icon().iter() {
                                path { d: *d }
                            }
                        }
                    }
                }
            }
        }
    };

    // The bar's items: centred in the line, but stretched in Switch so the
    // macro bar's own height (taller than the line) is the bar's.
    let bar_align = if mode() == Mode::Switch { "stretch" } else { "center" };

    rsx! {
        // The whole screen: the view runs under the camera housing on
        // whichever side it is, and what sits beside it keeps clear itself.
        // Every bar runs to the screen's edges, into its rounded corners —
        // the app takes the phone's shape. Only what sits in them keeps clear
        // of the camera housing: the rail's and the corner block's contents
        // on the housing's side when it is on the left, the page's on the
        // right.
        div { style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; min-height: 0; box-sizing: border-box; background: #0f1012; color: {TEXT};",
            // An edge swipe, followed here: a touch keeps going to what it
            // first landed on (the edge strip), and its moves bubble up to
            // the root — a layer mounted over the screen mid-touch never
            // hears them.
            onpointermove: move |e: PointerEvent| {
                let Some((from, way)) = swipe() else { return };
                let dx = e.client_coordinates().x - from;
                if dx * f64::from(-way) > SWIPE_TURN {
                    let at = Page::ALL.iter().position(|p| *p == page()).unwrap_or(0) as i32;
                    if let Some(p) = Page::ALL.get((at + way).clamp(0, Page::ALL.len() as i32 - 1) as usize) {
                        page.set(*p);
                    }
                    swipe.set(None);
                }
            },
            onpointerup: move |_| swipe.set(None),
            // ── The status, along the top: what plays (tap for every profile
            // and patch), the page, and the page's module presets ──
            // The mode's own bar: Control's presets, Audio's tabs, Switch's
            // macros — the last as tall as its knobs need (the switches
            // under it take the rest).
            div { style: "position: relative; z-index: 3; flex: 0 0 auto; min-height: {LINE_H}px; display: flex; flex-direction: row; align-items: {bar_align}; gap: 6px; box-sizing: border-box; padding: 0 {status_right}px 0 0; background: {BAR_BG}; border-bottom: 1px solid {RULE}; min-width: 0;",
                // The way back, over the rail and as wide as it: the corner
                // the two make reads as one column.
                div { style: "flex: 0 0 {RAIL_W + lead}px; align-self: stretch; box-sizing: border-box; padding-left: {back_left}px; display: flex; align-items: center; justify-content: center; border-right: 1px solid {RULE}; cursor: pointer;",
                    onclick: move |_| {
                        if let Some(host) = host {
                            host.on_home.call(());
                        }
                    },
                    if host.is_some() {
                        RailIcon { name: "Rigs", color: TEXT }
                    }
                }
                match mode() {
                Mode::Audio => rsx! {
                    div { style: "flex: 1 1 0%; min-width: 0; display: flex; padding: 4px 0;",
                        crate::phone_audio::AudioTabBar {}
                    }
                },
                Mode::Switch => rsx! {
                    div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; justify-content: center;",
                        crate::macro_bar::MacroBar { macros: state.macros }
                    }
                },
                Mode::Control => rsx! {
                // Three slots, each where it always is, whatever the page: what
                // plays, then the page's two modules — the top-level one
                // (Core, Time) and the page's own. A slot a page has no module
                // for stays, dimmed, so nothing slides into its place; a long
                // name is cut inside its slot rather than pushing the others.
                // (The page is picked on the chain under the line.)
                TopSlot { grow: 4, label: playing, name: patch_name.clone(),
                    on_open: move |()| browse.set(Some(if song { crate::phone_browser::BrowseTab::Songs } else { crate::phone_browser::BrowseTab::Patches })),
                }

                for (slot, module) in [(0, top_module), (1, page_module)] {
                    if let Some(module) = module {
                        TopSlot { key: "{slot}", grow: 3, label: module.to_string(), name: name_of(module), modified: module_edited(module),
                            on_open: move |()| browse.set(crate::phone_browser::BrowseTab::for_module(module)),
                        }
                    } else {
                        TopSlot { key: "{slot}", grow: 3, label: "Module".to_string(), name: String::new() }
                    }
                }
                },
                }
            }
            div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: row;",
                // ── The rail ──
                // Bottom-aligned, the most used lowest, under the thumb:
                // Audio, Switch, Profile/Setlist, Control.
                // Over the page (z-index): a page wider than its box — the
                // Amps row of three — runs on under the rail, clipped from
                // sight but not from taps (Blitz hit-tests clipped content),
                // and drawn after the rail it took the rail's taps: on some
                // pages, held one way round, the rail looked frozen.
                div { style: "position: relative; z-index: 3; flex: 0 0 {RAIL_W + lead}px; display: flex; flex-direction: column; justify-content: flex-end; gap: 2px; box-sizing: border-box; padding: 0 0 {rail_foot}px {lead}px; background: {BAR_BG}; border-right: 1px solid {RULE};",
                    // Audio, with how it is: a dot on its speaker, and its
                    // name saying Off or Error when it is not running.
                    AudioRailButton { on: mode() == Mode::Audio, running: (state.running)(), error: (state.audio_error)(),
                        onclick: move |()| {
                            chain_tall.set(false);
                            mode.set(Mode::Audio);
                        },
                    }
                    RailButton { on: mode() == Mode::Switch, label: "Switch", icon: "Switch",
                        onclick: move |()| {
                            chain_tall.set(false);
                            mode.set(Mode::Switch);
                        },
                    }
                    // Profile or Setlist, one button that flips between
                    // them: it shows which one plays.
                    RailButton { on: true, label: plays, icon: plays, onclick: move |()| set_play(if song { 1 } else { 2 }) }
                    RailButton { on: mode() == Mode::Control, label: "Control", icon: "Control", onclick: move |()| mode.set(Mode::Control) }
                }
                // ── The view ──
                // An explicit height: Blitz lays absolutely placed content out
                // against a stretched flex item's pre-stretch height.
                div { style: "position: relative; flex: 1 1 0%; height: 100%; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                    match mode() {
                        Mode::Control => rsx! {
                            // The chain, under the status line: thin, or
                            // dropped down as tall tiles. Laid out in the
                            // column (not over the page) so its tiles are hit
                            // where they are drawn.
                            if chain_tall() {
                                div { style: "flex: 0 0 {CHAIN_TALL}; display: flex; flex-direction: row; gap: 4px; box-sizing: border-box; padding: 8px {CORNER}px 8px 8px; overflow-x: auto; overflow-y: hidden; background: {BAR_BG}; border-bottom: 1px solid {RULE};",
                                    for p in Page::ALL {
                                        {segment(p, true)}
                                    }
                                }
                            } else {
                                div { style: "position: relative; z-index: 2; flex: 0 0 {CHAIN_H}px; display: flex; flex-direction: row; gap: 2px; box-sizing: border-box; padding: 3px {CORNER}px 3px 3px; background: {BAR_BG}; border-bottom: 1px solid {RULE};",
                                    for p in Page::ALL {
                                        {segment(p, false)}
                                    }
                                }
                            }
                            // The page. With the chain down it dims and takes
                            // no presses itself, and a tap on it puts the chain
                            // back up — no layer over it: an absolute layer here
                            // was laid out against the whole view, over the
                            // chain's tiles, and took their presses.
                            div { style: "position: relative; flex: 1 1 0%; min-height: 0; display: flex; overflow: hidden; box-sizing: border-box; padding-right: {trail}px;",
                                onclick: move |_| {
                                    if chain_tall() {
                                        chain_tall.set(false);
                                    }
                                },
                                div { style: "flex: 1 1 0%; min-width: 0; min-height: 0; display: flex; {page_veil}",
                                    PageView { page: page(), blocks: blocks.clone(), state, tempo_bpm: model.tempo_bpm }
                                }
                                // Swipe in from the right edge for the next
                                // page, from the left for the one before.
                                if !chain_tall() {
                                    div { style: "position: absolute; top: 0; left: 0; width: {EDGE_W}px; height: 100%; z-index: 4;",
                                        onpointerdown: move |e: PointerEvent| swipe.set(Some((e.client_coordinates().x, -1))),
                                    }
                                    div { style: "position: absolute; top: 0; right: 0; width: {EDGE_W + trail}px; height: 100%; z-index: 4;",
                                        onpointerdown: move |e: PointerEvent| swipe.set(Some((e.client_coordinates().x, 1))),
                                    }
                                }
                            }
                        },
                        Mode::Audio => rsx! {
                            div { style: "flex: 1 1 0%; min-height: 0; display: flex; box-sizing: border-box; padding-right: {trail}px;",
                                crate::phone_audio::PhoneAudio { state }
                            }
                        },
                        Mode::Switch => rsx! {
                            div { style: "flex: 1 1 0%; min-height: 0; display: flex; flex-direction: column; box-sizing: border-box; padding: 6px {trail}px 6px 6px;",
                                {switches}
                            }
                        },
                    }
                }
            }
            if let Some(tab) = browse() {
                crate::phone_browser::PhoneBrowser { key: "{tab:?}", tab, model: model.clone(), state, lead: sheet_left, trail: sheet_right,
                    on_close: move |()| browse.set(None),
                }
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
            span { style: "font-size: 11px; font-weight: 600; color: {fg};", "{label}" }
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
        // A person.
        "Profile" => &["M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8z", "M4 21c1-4 4-6 8-6s7 2 8 6"],
        // Back to the instrument menu.
        "Rigs" => &["M14 6l-6 6 6 6"],
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

/// The audio's state, as the rail's Audio button shows it: the dot on its
/// speaker, and its name (Audio while it runs, else why not).
fn audio_state(running: bool, error: &str) -> (&'static str, &'static str, &'static str) {
    if running {
        ("#22c55e", "Audio", "")
    } else if !error.is_empty() && error != "Audio stopped" {
        ("#ef4444", "Error", "#fca5a5")
    } else {
        ("#f59e0b", "Off", "#fcd34d")
    }
}

/// The rail's Audio button: a rail button whose speaker carries the audio's
/// state as a dot, and whose name says Off or Error when it is not running.
#[component]
fn AudioRailButton(on: bool, running: bool, error: String, onclick: EventHandler<()>) -> Element {
    let (fg, bg) = if on { (TEXT, RAISED) } else { (DIM, "transparent") };
    let (dot, label, warn) = audio_state(running, &error);
    let label_color = if warn.is_empty() { fg } else { warn };
    rsx! {
        div { style: "height: 48px; margin: 0 4px; border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; background: {bg}; cursor: pointer;",
            onclick: move |_| onclick.call(()),
            div { style: "position: relative; width: 20px; height: 20px;",
                RailIcon { name: "Audio", color: fg }
                span { style: "position: absolute; top: -2px; right: -4px; width: 8px; height: 8px; border-radius: 4px; background: {dot}; border: 2px solid {BAR_BG};" }
            }
            span { style: "font-size: 11px; font-weight: 600; color: {label_color};", "{label}" }
        }
    }
}

/// The slots' shared box: 40 points tall in the 48-point line (a full-size
/// target), its kind small over its name.
const SLOT: &str = "height: 40px; min-width: 0; box-sizing: border-box; display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 0 10px 0 12px; border-radius: 10px; overflow: hidden;";

/// A slot on the status line, `grow` shares of the line wide whatever it
/// holds: its kind over its name, a chevron for the browser it opens. With
/// no name it is a placeholder — dimmed, a dash, not a button — so a page
/// with fewer modules keeps the same slots.
#[component]
fn TopSlot(
    grow: u32,
    label: String,
    name: String,
    /// Changed from how it was saved: a `*` after the name.
    #[props(default)]
    modified: bool,
    #[props(default)]
    on_open: Option<EventHandler<()>>,
) -> Element {
    let empty = name.is_empty();
    let (bg, cursor, name_color, label_color) =
        if empty { ("transparent", "default", DIM, "#4b5058") } else { (RAISED, "pointer", "#f4f4f5", DIM) };
    let shown = if empty { "—".to_string() } else { name };
    rsx! {
        div { style: "flex: {grow} 1 0%; {SLOT} background: {bg}; border: 1px solid {RAISED}; cursor: {cursor};",
            onclick: move |_| {
                if let Some(open) = on_open {
                    open.call(());
                }
            },
            div { style: "flex: 1 1 0%; min-width: 0; display: flex; flex-direction: column; justify-content: center;",
                span { style: "font-size: 10px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {label_color}; white-space: nowrap; overflow: hidden;", "{label}" }
                span { style: "font-size: 14px; font-weight: 700; color: {name_color}; white-space: nowrap; overflow: hidden;",
                    "{shown}"
                    if modified {
                        span { style: "color: #f59e0b;", " *" }
                    }
                }
            }
            if !empty {
                svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: DIM, stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M6 9l6 6 6-6" }
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
        Page::Amps => {
            // The amps fitted to the page (the screen less the rail, the
            // housing's clearance and the bars), as the drives are.
            let (w, h) = try_use_context::<crate::control::WindowSize>().map_or((874.0, 381.0), |s| (s.0)());
            let page_h = h - f64::from(LINE_H) - f64::from(CHAIN_H) - 4.0;
            let page_w = w - f64::from(RAIL_W + HOUSING) - f64::from(CORNER);
            rsx! { crate::rig_faces::AmpRow { blocks, amps_only: true, fit: Some((page_w, page_h)) } }
        }
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
