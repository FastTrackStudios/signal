//! The drum rig on the iPad, in the guitar rig's frame: the same top bar,
//! sidebar, bottom dock and foot bar, with pads where the switches are.
//!
//!   top bar    over the sidebar its toggle and the rig's mode — Preset,
//!              Profile, Setlist, as every rig has; then the kit (and how
//!              far it has loaded), the MIDI and Audio lights (a press opens
//!              Setup), the voices playing and the OUT meter
//!   sidebar    the mode's list, on the screens every rig shares — Preset:
//!              the kits, each with its variations (its mixes); a pad hit
//!              turns it to that piece's
//!              instruments, a tap swapping one in (a kit piece is a module,
//!              its instruments the module's presets)
//!   main       Perform: the 4×4 pads, and docked at their foot the picked
//!              piece — its instrument, stepped through the ones like it;
//!              its level; mute and solo. Kit: the pieces by family — Kick,
//!              Snare, Hi-Hat, Toms, Cymbals, Perc — and the instruments the
//!              picked one can take
//!   foot bar   Perform · Kit as tabs; the piece dock's toggle; Setup (the
//!              audio, the MIDI input and drum map, and — two steps from the
//!              pads — the way back to the app's instruments)
//!
//! No footswitches and no routing: the drum rig is played from the pads (or
//! MIDI, or triggers on a real kit) and mixed in the session's mixer. The
//! pads are laid out as an MPC's, from the bottom left — kick, snare, side
//! stick, clap; the hats and the bell; the toms and the ride; the cymbals on
//! top. A pad plays as the finger lands (higher on the pad, harder: an
//! iPad's glass has no pressure) and lets go as it lifts; the pad hit is
//! the piece picked.
//!
//! The drum rig's own client when the app provides one; otherwise a demo
//! kit, so the surface is there before the engine is.

use dioxus::prelude::*;
use signal_drums_proto::drum::{DrumEvent, DrumRigClient, DrumRigStreamClient};
use signal_drums_proto::{
    DrumStatus, InputMap, KitInfo, KitSlot, LibraryPiece, MeterSnapshot, MixerStrip, PieceInfo,
    StripKind,
};

use signal_rig_shell::tokens::*;
use signal_rig_shell::{
    Count, Dock, FootBar, FootButton, Frame, Indicator, InstrumentsBack, Main, MeterWell, Middle,
    MiniMeter, ModeTabs, Rule, SetupButton, Sidebar, TopBar, has_host, ink,
};

/// A piece's family: its colour, its place on the pads, its group in the
/// list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
enum Family {
    Kick,
    Snare,
    HiHat,
    Toms,
    Cymbals,
    Perc,
}

impl Family {
    const ALL: [Self; 6] = [
        Self::Kick,
        Self::Snare,
        Self::HiHat,
        Self::Toms,
        Self::Cymbals,
        Self::Perc,
    ];

    fn of(id: &str, label: &str) -> Self {
        let s = format!("{id} {label}").to_lowercase();
        let has = |w: &[&str]| w.iter().any(|w| s.contains(w));
        if has(&["kick", "bass drum", "bd"]) {
            Self::Kick
        } else if has(&["hat", "hh"]) {
            Self::HiHat
        } else if has(&["snare", "stick", "rim", "sd"]) {
            Self::Snare
        } else if has(&["tom"]) {
            Self::Toms
        } else if has(&["crash", "ride", "china", "splash", "cym", "bell"]) {
            Self::Cymbals
        } else {
            Self::Perc
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Kick => "Kick",
            Self::Snare => "Snare",
            Self::HiHat => "Hi-Hat",
            Self::Toms => "Toms",
            Self::Cymbals => "Cymbals",
            Self::Perc => "Perc",
        }
    }

    /// The family's colour: an identity mark (a band, a swatch), never a
    /// fill.
    fn colour(self) -> &'static str {
        match self {
            Self::Kick => "#f97316",
            Self::Snare => "#38bdf8",
            Self::HiHat => "#facc15",
            Self::Toms => "#a78bfa",
            Self::Cymbals => "#2dd4bf",
            Self::Perc => "#f472b6",
        }
    }
}

/// One piece as the surface shows it.
#[derive(Clone, PartialEq, Debug)]
struct Piece {
    /// The engine id (`kick`, `rtom1`) — what a swap addresses.
    id: String,
    label: String,
    note: u32,
    /// The instrument in the slot.
    instrument: String,
    kind: String,
    family: Family,
}

/// The pads, bottom-left first (pad 1), as an MPC numbers them: where each
/// family's pieces go, in order.
const PAD_ORDER: [Family; 16] = [
    Family::Kick,
    Family::Snare,
    Family::Snare,
    Family::Perc,
    Family::HiHat,
    Family::HiHat,
    Family::HiHat,
    Family::Cymbals,
    Family::Toms,
    Family::Toms,
    Family::Toms,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
];

/// The kit on the 16 pads: each pad takes the next piece of its family; what
/// is left over fills the pads still empty, in kit order.
fn lay_out(pieces: &[Piece]) -> [Option<usize>; 16] {
    let mut pads = [None; 16];
    let mut used = vec![false; pieces.len()];
    for (pad, fam) in PAD_ORDER.iter().enumerate() {
        if let Some(i) = (0..pieces.len()).find(|&i| !used[i] && pieces[i].family == *fam) {
            used[i] = true;
            pads[pad] = Some(i);
        }
    }
    for pad in pads.iter_mut().filter(|p| p.is_none()) {
        if let Some(i) = (0..pieces.len()).find(|&i| !used[i]) {
            used[i] = true;
            *pad = Some(i);
        }
    }
    pads
}

/// A friendly name from an engine id (`rtom1` → `Rack Tom 1`).
fn label_of(id: &str) -> String {
    let words: Vec<String> = id
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect();
    words.join(" ")
}

/// The demo kit: sixteen GM pieces, shown when no drum rig is connected.
fn demo_kit() -> Vec<Piece> {
    let p = |id: &str, label: &str, note: u32, instrument: &str, kind: &str| Piece {
        id: id.into(),
        label: label.into(),
        note,
        instrument: instrument.into(),
        kind: kind.into(),
        family: Family::of(id, label),
    };
    vec![
        p("kick", "Kick", 36, "Maple 22×18", "kick"),
        p("snare", "Snare", 38, "Brass 14×6.5", "snare"),
        p("sidestick", "Side Stick", 37, "Brass 14×6.5", "snare"),
        p("clap", "Clap", 39, "Studio Clap", "perc"),
        p("hihat-closed", "Hat Closed", 42, "K 14\"", "hihat"),
        p("hihat-pedal", "Hat Pedal", 44, "K 14\"", "hihat"),
        p("hihat-open", "Hat Open", 46, "K 14\"", "hihat"),
        p("ride-bell", "Ride Bell", 53, "K Light 22\"", "ride"),
        p("rtom1", "Rack Tom 1", 48, "Maple 10\"", "tom"),
        p("rtom2", "Rack Tom 2", 45, "Maple 12\"", "tom"),
        p("ftom1", "Floor Tom", 43, "Maple 16\"", "tom"),
        p("ride", "Ride", 51, "K Light 22\"", "ride"),
        p("crash-l", "Crash L", 49, "A Custom 18\"", "crash"),
        p("crash-r", "Crash R", 57, "A Custom 19\"", "crash"),
        p("china", "China", 52, "Oriental 18\"", "china"),
        p("splash", "Splash", 55, "A 10\"", "splash"),
    ]
}

/// The pieces from the rig: its notes, its slots' labels and instruments.
fn pieces_of(pieces: &[PieceInfo], slots: &[KitSlot]) -> Vec<Piece> {
    pieces
        .iter()
        .map(|p| {
            let slot = slots.iter().find(|s| s.slot_id == p.id);
            let label = slot.map_or_else(|| label_of(&p.id), |s| s.label.clone());
            Piece {
                id: p.id.clone(),
                family: Family::of(&p.id, &label),
                label,
                note: p.note,
                instrument: slot.map(|s| s.current_name.clone()).unwrap_or_default(),
                kind: slot.map(|s| s.kind.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Perform,
    Kit,
    Setup,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetupTab {
    Audio,
    Midi,
}

/// A piece's mixer state with no engine to hold it.
#[derive(Clone, PartialEq, Debug)]
struct DemoStrip {
    id: String,
    muted: bool,
    soloed: bool,
    gain_db: f32,
}

/// The picked piece's strip: where it sits in the drum mixer, and how.
#[derive(Clone, PartialEq, Debug, Default)]
struct StripState {
    idx: Option<u32>,
    muted: bool,
    soloed: bool,
    gain_db: f32,
    peak: f32,
}

#[component]
pub fn DrumTablet() -> Element {
    // Its menus open over it.
    signal_widgets::PopupHost::provide();
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let stream = use_hook(try_consume_context::<DrumRigStreamClient>);
    let live = rig.is_some();
    let mut view = use_signal(|| View::Perform);
    let mut sidebar = use_signal(|| true);
    // The rig's performance surface (presets, profiles, setlists), shared
    // with every rig; its mode, kept here too so the tabs answer with no
    // engine behind them.
    let perf = signal_rig_perform_ui::use_perform();
    let perform = use_hook(signal_rig_perform_ui::perform_client);
    let has_perform = perform.is_some();
    let mut mode = use_signal(|| 0u32);
    use_effect(move || {
        let m = perf.perf.read().mode;
        if has_perform {
            mode.set(m);
        }
    });
    // The sidebar turned to the picked piece's instruments (a pad hit).
    let mut browsing = use_signal(|| false);
    // The piece docked under the pads.
    let mut docked = use_signal(|| true);
    let mut setup_tab = use_signal(|| SetupTab::Audio);
    let mut kits = use_signal(Vec::<KitInfo>::new);
    let mut raw = use_signal(Vec::<PieceInfo>::new);
    let mut slots = use_signal(Vec::<KitSlot>::new);
    let mut mixer = use_signal(Vec::<MixerStrip>::new);
    let mut status = use_signal(DrumStatus::default);
    let meters = use_signal(MeterSnapshot::default);
    // The piece picked (by its id), and the pads held down.
    let mut picked = use_signal(|| None::<String>);
    let held = use_signal(|| [false; 16]);
    // Demo mode: the mixer kept here.
    let demo = use_signal(Vec::<DemoStrip>::new);

    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(s) = rig.status().await {
                    status.set(s);
                }
                if let Ok(k) = rig.kits().await {
                    kits.set(k);
                }
                if let Ok(p) = rig.pieces().await {
                    raw.set(p);
                }
                if let Ok(s) = rig.kit_slots().await {
                    slots.set(s);
                }
                if let Ok(m) = rig.mixer().await {
                    mixer.set(m);
                }
            }
        });
    }
    architect::use_stream(
        move |sink| {
            let stream = stream.clone();
            async move {
                match stream {
                    Some(s) => s.events(sink).await.is_ok(),
                    None => false,
                }
            }
        },
        move |ev: DrumEvent| {
            let (mut kits, mut raw, mut slots, mut mixer, mut status, mut meters) =
                (kits, raw, slots, mixer, status, meters);
            match ev {
                DrumEvent::Status(s) => status.set(s),
                DrumEvent::Meters(m) => meters.set(m),
                DrumEvent::Library(k) => kits.set(k),
                DrumEvent::Kit(p) => raw.set(p),
                DrumEvent::Design(s) => slots.set(s),
                DrumEvent::Mixer(m) => mixer.set(m),
                DrumEvent::Midi(_) => {}
            }
        },
    );

    let pieces = if live {
        pieces_of(&raw.read(), &slots.read())
    } else {
        demo_kit()
    };
    let pads = lay_out(&pieces);
    let kit_name = if live {
        kits.read()
            .iter()
            .find(|k| k.loaded)
            .map_or_else(|| "No kit".to_string(), |k| k.name.clone())
    } else {
        "Demo Kit".to_string()
    };
    let sel = picked()
        .and_then(|id| pieces.iter().position(|p| p.id == id))
        .or_else(|| (!pieces.is_empty()).then_some(0));
    let sel_piece = sel.and_then(|i| pieces.get(i).cloned());
    // The picked piece's strip in the drum mixer (by its label, else its
    // place among the pieces), its peak from the meters.
    let strip =
        sel_piece.as_ref().map(|p| {
            let i = sel.unwrap_or(0);
            let m = mixer.read();
            let strips: Vec<(usize, &MixerStrip)> = m
                .iter()
                .enumerate()
                .filter(|(_, s)| s.kind == StripKind::Piece)
                .collect();
            let found = strips
                .iter()
                .find(|(_, s)| s.label.eq_ignore_ascii_case(&p.label))
                .or_else(|| strips.get(i));
            match found {
                Some((at, s)) => StripState {
                    idx: Some(s.idx),
                    muted: s.muted,
                    soloed: s.soloed,
                    gain_db: s.gain_db,
                    peak: meters.read().strips.get(*at).copied().unwrap_or(s.peak),
                },
                None => demo.read().iter().find(|d| d.id == p.id).map_or_else(
                    StripState::default,
                    |d| StripState {
                        idx: None,
                        muted: d.muted,
                        soloed: d.soloed,
                        gain_db: d.gain_db,
                        peak: 0.0,
                    },
                ),
            }
        });
    let st = status();
    let out = f64::from(meters.read().master.max(st.master_peak))
        .clamp(0.0, 1.0)
        .sqrt();
    let midi = if live && st.midi_port.is_some() {
        LIVE
    } else {
        INK_3
    };
    let audio = match (live, st.running) {
        (false, _) => INK_3,
        (true, true) => LIVE,
        (true, false) => VOID,
    };
    let loading = live && st.preload < 1.0;
    rsx! {
        Frame {
            TopBar {
                sidebar: sidebar(),
                on_sidebar: move |()| sidebar.toggle(),
                side: rsx! {
                    ModeTabs {
                        mode: mode(),
                        on_pick: move |m: u32| {
                            mode.set(m);
                            if let Some(c) = perform.clone() {
                                spawn(async move {
                                    let _ = c.set_perform_mode(m).await;
                                });
                            }
                            sidebar.set(true);
                            browsing.set(false);
                            if view() == View::Setup {
                                view.set(View::Perform);
                            }
                        },
                    }
                },
                // The kit in play, and how far its samples have loaded.
                div { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 0 18px;",
                    span { style: "font-size: 17px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{kit_name}" }
                    if loading {
                        span { style: "font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums; white-space: nowrap;", "Loading {pct(f64::from(st.preload))}%" }
                    }
                }
                if !live {
                    span { style: "align-self: center; font-size: 12px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_3}; padding: 0 12px; white-space: nowrap;", "No drum engine" }
                } else if !st.running {
                    StartAudio {}
                }
                Rule {}
                Indicator { kind: "midi", tone: midi, onclick: move |_| { setup_tab.set(SetupTab::Midi); view.set(View::Setup); } }
                Indicator { kind: "audio", tone: audio, onclick: move |_| { setup_tab.set(SetupTab::Audio); view.set(View::Setup); } }
                // The voices sounding, where the guitar shows its CPU.
                Count { n: st.voices, label: "voices" }
                Rule {}
                MeterWell {
                    MiniMeter { label: "O", level: out, muted: false, phones: false }
                }
                // Clear of the screen's rounded corner (the guitar ends on Mute).
                span { style: "width: 14px; flex-shrink: 0;" }
            }
            Middle {
                if view() == View::Setup {
                    DrumSetup { tab: setup_tab, status: st.clone(), live }
                } else {
                    if sidebar() {
                        Sidebar {
                            if let (true, Some(p)) = (browsing(), sel_piece.clone()) {
                                PieceBrowser { key: "{p.id}", piece: p, on_close: move |()| browsing.set(false) }
                            } else {
                                // The screens every rig shares.
                                match mode() {
                                    0 => rsx! { super::sidebar_views::PresetView { state: perf } },
                                    1 => rsx! { super::sidebar_views::ProfileView { state: perf } },
                                    _ => rsx! { super::setlist::TabletSetlist { state: perf } },
                                }
                            }
                        }
                    }
                    Main {
                        match view() {
                            View::Perform => rsx! {
                                div { style: "flex: 1; min-height: 0; display: flex; padding: 14px; box-sizing: border-box; background: {MAIN};",
                                    PadGrid {
                                        pieces: pieces.clone(),
                                        pads,
                                        selected: sel,
                                        held,
                                        on_hit: move |i: usize| {
                                            picked.set(pieces.get(i).map(|p| p.id.clone()));
                                            browsing.set(true);
                                            sidebar.set(true);
                                        },
                                    }
                                }
                                if docked() {
                                    if let (Some(p), Some(s)) = (sel_piece.clone(), strip.clone()) {
                                        Dock {
                                            PieceDock { key: "{p.id}", piece: p, strip: s, demo }
                                        }
                                    }
                                }
                            },
                            View::Kit => rsx! {
                                div { style: "flex: 1; min-height: 0; display: flex;",
                                    div { style: "width: {SIDEBAR_W}px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; border-right: 1px solid {RULE};",
                                        PieceList { pieces: pieces.clone(), selected: sel, on_pick: move |id: String| picked.set(Some(id)) }
                                    }
                                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0;",
                                        if let Some(p) = sel_piece.clone() {
                                            KitView { key: "{p.id}", piece: p }
                                        }
                                    }
                                }
                            },
                            View::Setup => rsx! {},
                        }
                    }
                }
            }
            FootBar {
                FootButton { label: "Perform", on: view() == View::Perform, pin: false, onclick: move |_| view.set(View::Perform),
                    path { d: "M5 3.5v11l9-5.5Z", fill: ink(view() == View::Perform) }
                }
                FootButton { label: "Kit", on: view() == View::Kit, pin: false, onclick: move |_| view.set(View::Kit),
                    ellipse { cx: "9", cy: "6", rx: "6.5", ry: "2.5", fill: "none", stroke: ink(view() == View::Kit), stroke_width: "1.4" }
                    path { d: "M2.5 6v6c0 1.4 2.9 2.5 6.5 2.5s6.5-1.1 6.5-2.5V6", fill: "none", stroke: ink(view() == View::Kit), stroke_width: "1.4" }
                }
                span { style: "flex: 1;" }
                // Perform's dock: the picked piece.
                if view() == View::Perform {
                    FootButton { label: "Piece", on: docked(), pin: true, onclick: move |_| docked.toggle(),
                        path { d: "M4 2.5v13M9 2.5v13M14 2.5v13", stroke: ink(docked()), stroke_width: "1.4", stroke_linecap: "round" }
                        rect { x: "2.3", y: "10", width: "3.4", height: "2.6", rx: "0.8", fill: ink(docked()) }
                        rect { x: "7.3", y: "5", width: "3.4", height: "2.6", rx: "0.8", fill: ink(docked()) }
                        rect { x: "12.3", y: "8", width: "3.4", height: "2.6", rx: "0.8", fill: ink(docked()) }
                    }
                }
                Rule {}
                SetupButton { on: view() == View::Setup, onclick: move |_| view.set(View::Setup) }
            }
            signal_widgets::PopupLayer {}
        }
    }
}

/// Audio not running: a badge that starts it.
#[component]
fn StartAudio() -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    rsx! {
        div { style: "align-self: stretch; display: flex; align-items: center; padding: 0 10px; flex-shrink: 0;",
            button {
                style: "height: {HIT}px; padding: 0 14px; border-radius: 999px; border: 1.5px solid {MODIFIED}; background: transparent; color: {MODIFIED}; font-family: {FONT}; font-size: 13px; font-weight: 750; white-space: nowrap; cursor: pointer;",
                onclick: move |_| {
                    if let Some(r) = rig.clone() {
                        let _ = dioxus_core::spawn_forever(async move { let _ = r.start().await; });
                    }
                },
                "Start audio"
            }
        }
    }
}

/// The sidebar on one piece (a pad hit): its instruments, the one in its
/// slot marked; a tap swaps one in. ‹ goes back to the mode's list.
#[component]
fn PieceBrowser(piece: Piece, on_close: EventHandler<()>) -> Element {
    let options = use_options(&piece);
    let list = options.read().clone();
    rsx! {
        div { style: "flex-shrink: 0; height: 52px; display: flex; align-items: center; gap: 4px; padding: 0 6px; border-bottom: 1px solid {RULE}; background: {SHEET};",
            button {
                "aria-label": "Back",
                style: "width: {HIT}px; height: {HIT}px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                onclick: move |_| on_close.call(()),
                svg { width: "9", height: "15", view_box: "0 0 9 15",
                    path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
            span { style: "width: 10px; height: 10px; border-radius: 2px; flex-shrink: 0; background: {piece.family.colour()};" }
            span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{piece.label}" }
            if !list.is_empty() {
                span { style: "padding: 0 10px; font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{list.len()}" }
            }
        }
        div { style: "flex: 1; min-height: 0; overflow-y: auto;",
            if list.is_empty() {
                // With no engine: the one in the slot, so the list is not bare.
                div { style: "min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 18px; border-bottom: 1px solid {RULE}; background: {ROW_ON}; font-size: 16px; font-weight: 750;",
                    span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(true, DIM)}" }
                    if piece.instrument.is_empty() { "—" } else { "{piece.instrument}" }
                }
            }
            for o in list {
                {
                    let on = o.name == piece.instrument;
                    let (slot, to) = (piece.id.clone(), o.clone());
                    rsx! {
                        button {
                            key: "{o.path}",
                            "aria-pressed": "{on}",
                            style: "width: 100%; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 18px; border: none; border-bottom: 1px solid {RULE}; background: {pick(on, ROW_ON, CLEAR)}; color: {INK}; text-align: left; font-family: {FONT}; font-size: 16px; font-weight: {pick(on, 750, 560)}; cursor: pointer;",
                            onclick: move |_| {
                                if !on {
                                    swap(slot.clone(), to.clone());
                                }
                            },
                            span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(on, DIM)} box-sizing: border-box;" }
                            span { style: "flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{o.name}" }
                        }
                    }
                }
            }
        }
    }
}

/// The kit's pieces by family; the picked one lifted.
#[component]
fn PieceList(
    pieces: Vec<Piece>,
    selected: Option<usize>,
    on_pick: EventHandler<String>,
) -> Element {
    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto;",
            for fam in Family::ALL {
                if pieces.iter().any(|p| p.family == fam) {
                    div { key: "{fam.label()}",
                        div { style: "display: flex; align-items: center; gap: 8px; padding: 14px 18px 6px;",
                            span { style: "width: 8px; height: 8px; border-radius: 2px; background: {fam.colour()};" }
                            span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_2};", "{fam.label()}" }
                        }
                        for (i, p) in pieces.iter().enumerate().filter(|(_, p)| p.family == fam) {
                            {
                                let on = selected == Some(i);
                                let id = p.id.clone();
                                rsx! {
                                    button {
                                        key: "{p.id}",
                                        style: "width: 100%; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 18px; border: none; border-bottom: 1px solid {RULE}; background: {pick(on, UP, CLEAR)}; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer;",
                                        onclick: move |_| on_pick.call(id.clone()),
                                        span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                            span { style: "font-size: 16px; font-weight: {pick(on, 750, 560)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.label}" }
                                            if !p.instrument.is_empty() {
                                                span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.instrument}" }
                                            }
                                        }
                                        span { style: "font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{p.note}" }
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

/// The instruments a piece can take: the rig's similarity space for it,
/// else the library's of its kind.
fn use_options(piece: &Piece) -> Signal<Vec<LibraryPiece>> {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let mut options = use_signal(Vec::<LibraryPiece>::new);
    let (id, kind) = (piece.id.clone(), piece.kind.clone());
    use_future(move || {
        let (rig, id, kind) = (rig.clone(), id.clone(), kind.clone());
        async move {
            let Some(r) = rig else { return };
            let mut list = r.similar_pieces(id).await.unwrap_or_default();
            if list.is_empty() {
                list = r
                    .library()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|l| l.kind.eq_ignore_ascii_case(&kind))
                    .collect();
            }
            options.set(list);
        }
    });
    options
}

/// Puts an instrument in a piece's slot.
fn swap(slot: String, to: LibraryPiece) {
    if let Some(r) = try_consume_context::<DrumRigClient>() {
        spawn(async move {
            let _ = r.swap_piece(slot, to.path).await;
        });
    }
}

/// The picked piece, docked under the pads: what it is, its instrument
/// stepped through the ones like it, its level, mute and solo.
#[component]
fn PieceDock(piece: Piece, strip: StripState, demo: Signal<Vec<DemoStrip>>) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let options = use_options(&piece);
    let at = options
        .read()
        .iter()
        .position(|o| o.name == piece.instrument);
    let count = options.read().len();
    let step = {
        let id = piece.id.clone();
        move |d: i32| {
            let list = options.read().clone();
            if list.is_empty() {
                return;
            }
            let n = list.len() as i32;
            let next = ((at.map_or(-1, |a| a as i32) + d).rem_euclid(n)) as usize;
            if let Some(o) = list.get(next).cloned() {
                swap(id.clone(), o);
            }
        }
    };
    // Mute, solo or level: to the rig's strip, else kept here.
    let set = {
        let id = piece.id.clone();
        move |what: Edit| match (rig.clone(), strip.idx) {
            (Some(r), Some(idx)) => {
                spawn(async move {
                    let _ = match what {
                        Edit::Mute(on) => r.set_piece_mute(idx, on).await,
                        Edit::Solo(on) => r.set_piece_solo(idx, on).await,
                        Edit::Gain(db) => r.set_piece_gain(idx, db).await,
                    };
                });
            }
            _ => {
                let mut demo = demo;
                let mut d = demo.write();
                if !d.iter().any(|s| s.id == id) {
                    d.push(DemoStrip {
                        id: id.clone(),
                        muted: false,
                        soloed: false,
                        gain_db: 0.0,
                    });
                }
                if let Some(s) = d.iter_mut().find(|s| s.id == id) {
                    match what {
                        Edit::Mute(on) => s.muted = on,
                        Edit::Solo(on) => s.soloed = on,
                        Edit::Gain(db) => s.gain_db = db,
                    }
                }
            }
        }
    };
    let (s1, s2) = (step.clone(), step);
    let (e1, e2, e3, e4) = (set.clone(), set.clone(), set.clone(), set);
    let gain = strip.gain_db;
    let (muted, soloed) = (strip.muted, strip.soloed);
    let toggle = |on: bool, colour: &str| {
        format!(
            "width: 76px; height: {HIT}px; flex-shrink: 0; border-radius: {R}; border: 1px solid {}; background: {}; color: {}; font-size: 15px; font-weight: 700; font-family: {FONT}; cursor: pointer;",
            if on { colour } else { RULE_STRONG },
            if on { colour } else { CLEAR },
            if on { "#0a0a0c" } else { INK_2 }
        )
    };
    let arrow = format!(
        "width: {HIT}px; height: {HIT}px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;"
    );
    let level = f64::from(strip.peak).clamp(0.0, 1.0).sqrt();
    rsx! {
        div { style: "display: flex; align-items: center; gap: 14px; padding: 12px 14px; border-top: 1px solid {RULE}; background: {SHEET};",
            // What it is.
            div { style: "width: 220px; flex-shrink: 0; min-width: 0; display: flex; align-items: center; gap: 10px;",
                span { style: "width: 10px; height: 10px; border-radius: 2px; flex-shrink: 0; background: {piece.family.colour()};" }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                    span { style: "font-size: 19px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{piece.label}" }
                    span { style: "font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "Note {piece.note}" }
                }
            }
            // Its instrument, stepped through its like.
            div { style: "flex: 1; min-width: 0; display: flex; align-items: center; height: {HIT}px; border-radius: {R}; background: {FILL};",
                button { "aria-label": "Previous instrument", disabled: count == 0, style: "{arrow}", onclick: move |_| s1(-1),
                    super::fx_row::Chevron { left: true }
                }
                span { style: "flex: 1; min-width: 0; text-align: center; font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: {pick(piece.instrument.is_empty(), INK_3, INK)};",
                    if piece.instrument.is_empty() { "—" } else { "{piece.instrument}" }
                }
                button { "aria-label": "Next instrument", disabled: count == 0, style: "{arrow}", onclick: move |_| s2(1),
                    super::fx_row::Chevron { left: false }
                }
            }
            // Its level, a dB a press, and its meter.
            div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 2px; height: {HIT}px; border-radius: {R}; background: {FILL};",
                button { "aria-label": "Quieter", style: "{arrow} color: {INK_2}; font-size: 20px; font-weight: 600; font-family: {FONT};", onclick: move |_| e3(Edit::Gain((gain - 1.0).max(-60.0))), "−" }
                span { style: "position: relative; width: 84px; height: 100%; display: flex; align-items: center; justify-content: center; font-size: 15px; font-weight: 700; font-variant-numeric: tabular-nums;",
                    "{gain:+.1} dB"
                    span { style: "position: absolute; left: 6px; right: 6px; bottom: 5px; height: 3px; border-radius: 2px; background: {WELL}; overflow: hidden;",
                        span { style: "display: block; width: {pct(level)}%; height: 100%; background: {LIVE};" }
                    }
                }
                button { "aria-label": "Louder", style: "{arrow} color: {INK_2}; font-size: 20px; font-weight: 600; font-family: {FONT};", onclick: move |_| e4(Edit::Gain((gain + 1.0).min(12.0))), "+" }
            }
            button { "aria-pressed": "{muted}", style: "{toggle(muted, VOID)}", onclick: move |_| e1(Edit::Mute(!muted)), "Mute" }
            button { "aria-pressed": "{soloed}", style: "{toggle(soloed, \"#facc15\")}", onclick: move |_| e2(Edit::Solo(!soloed)), "Solo" }
        }
    }
}

/// A change to a piece's strip.
#[derive(Clone, Copy)]
enum Edit {
    Mute(bool),
    Solo(bool),
    Gain(f32),
}

/// Kit: the instruments the picked piece can take, the one in its slot
/// marked; a tap swaps one in.
#[component]
fn KitView(piece: Piece) -> Element {
    let options = use_options(&piece);
    let list = options.read().clone();
    rsx! {
        div { style: "flex-shrink: 0; height: {HEADER_H}px; display: flex; align-items: center; gap: 12px; padding: 0 22px; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            span { style: "width: 12px; height: 12px; border-radius: 3px; background: {piece.family.colour()};" }
            span { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_3};", "{piece.family.label()}" }
                span { style: "font-size: 22px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{piece.label}" }
            }
            span { style: "flex: 1;" }
            span { style: "font-size: 14px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{list.len()} instruments" }
        }
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 14px; box-sizing: border-box; background: {MAIN};",
            if list.is_empty() {
                div { style: "padding: 24px 8px; font-size: 15px; color: {INK_3};", "No instruments" }
            }
            div { style: "display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px;",
                for o in list {
                    {
                        let on = o.name == piece.instrument;
                        let (slot, to) = (piece.id.clone(), o.clone());
                        rsx! {
                            button {
                                key: "{o.path}",
                                "aria-pressed": "{on}",
                                style: "min-height: 72px; display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-radius: {R_MD}; border: {ring(on, 2.0)}; background: {pick(on, UP, SHEET)}; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer; box-sizing: border-box;",
                                onclick: move |_| {
                                    if !on {
                                        swap(slot.clone(), to.clone());
                                    }
                                },
                                span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(on, DIM)} box-sizing: border-box;" }
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: {pick(on, 750, 600)}; overflow: hidden; text-overflow: ellipsis;", "{o.name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Setup ──────────────────────────────────────────────────────────────────

/// Setup: the way back to the instruments, then Audio and MIDI as tabs
/// across the top, the open one's choices below.
#[component]
fn DrumSetup(tab: Signal<SetupTab>, status: DrumStatus, live: bool) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let mut ports = use_signal(Vec::<String>::new);
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                if let Some(r) = rig {
                    ports.set(r.midi_ports().await.unwrap_or_default());
                }
            }
        });
    }
    let columns = if has_host() {
        "auto repeat(2, minmax(0, 1fr))"
    } else {
        "repeat(2, minmax(0, 1fr))"
    };
    let audio_name = match (live, status.running) {
        (false, _) => "No drum engine",
        (true, true) => "Running",
        (true, false) => "Stopped",
    };
    let audio_sub = if live {
        format!("{} voices", status.voices)
    } else {
        String::new()
    };
    let midi_name = status
        .midi_port
        .clone()
        .unwrap_or_else(|| "No input".to_string());
    let tabs = [
        (SetupTab::Audio, "Audio", audio_name.to_string(), audio_sub),
        (
            SetupTab::Midi,
            "MIDI",
            midi_name,
            map_name(status.input_map).to_string(),
        ),
    ];
    let open = tab();
    let row = |on: bool| {
        format!(
            "width: 100%; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 22px; border: none; border-bottom: 1px solid {RULE}; background: {}; color: {INK}; text-align: left; font-family: {FONT}; font-size: 16px; font-weight: {}; cursor: pointer;",
            pick(on, ROW_ON, CLEAR),
            pick(on, 750, 560)
        )
    };
    let head = format!(
        "padding: 18px 22px 8px; font-size: 12px; font-weight: 750; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_3};"
    );
    rsx! {
        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0;",
            div { style: "flex-shrink: 0; display: grid; grid-template-columns: {columns}; height: {HEADER_H}px; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
                InstrumentsBack {}
                for (i, (id, label, name, sub)) in tabs.into_iter().enumerate() {
                    {
                        let on = id == open;
                        let border = if i > 0 { format!("border-left: 1px solid {RULE};") } else { String::new() };
                        rsx! {
                            button {
                                key: "{label}",
                                style: "position: relative; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; justify-content: center; gap: 2px; padding: 0 18px; text-align: left; border: none; {border} background: {pick(on, ROW_ON, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| tab.set(id),
                                if on {
                                    span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: 3px; background: {INK};" }
                                }
                                span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.08em; text-transform: uppercase; color: {pick(on, INK_2, INK_3)};", "{label}" }
                                span { style: "font-size: 17px; font-weight: 750; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                                if !sub.is_empty() {
                                    span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{sub}" }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; background: {DESK};",
                match open {
                    SetupTab::Audio => rsx! {
                        div { style: "{head}", "Audio" }
                        {
                            let rig = rig.clone();
                            let running = status.running;
                            rsx! {
                                button {
                                    disabled: !live,
                                    style: "{row(running)}",
                                    onclick: move |_| {
                                        if let Some(r) = rig.clone() {
                                            let _ = dioxus_core::spawn_forever(async move {
                                                let _ = if running { r.stop().await } else { r.start().await };
                                            });
                                        }
                                    },
                                    span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(running, DIM)} box-sizing: border-box;" }
                                    span { style: "flex: 1;", if running { "Audio running" } else { "Start audio" } }
                                    if running {
                                        span { style: "font-size: 14px; font-weight: 650; color: {INK_3};", "Stop" }
                                    }
                                }
                            }
                        }
                        if let Some(kit) = status.loaded_kit.clone() {
                            div { style: "{row(false)} cursor: default;",
                                span { style: "flex: 1;", "Kit" }
                                span { style: "font-size: 15px; color: {INK_2};", "{kit}" }
                            }
                        }
                    },
                    SetupTab::Midi => rsx! {
                        div { style: "{head}", "Input" }
                        if ports.read().is_empty() {
                            div { style: "padding: 8px 22px 16px; font-size: 15px; color: {INK_3};", "No MIDI inputs" }
                        }
                        for p in ports.read().clone() {
                            {
                                let on = status.midi_port.as_deref() == Some(p.as_str());
                                let rig = rig.clone();
                                let name = p.clone();
                                rsx! {
                                    button {
                                        key: "{p}",
                                        style: "{row(on)}",
                                        onclick: move |_| {
                                            if let Some(r) = rig.clone() {
                                                let name = name.clone();
                                                spawn(async move { let _ = r.set_midi_port(name).await; });
                                            }
                                        },
                                        span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(on, DIM)} box-sizing: border-box;" }
                                        span { style: "flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{p}" }
                                    }
                                }
                            }
                        }
                        div { style: "{head}", "Drum map" }
                        for m in [InputMap::Direct, InputMap::StrataPrime, InputMap::Fts, InputMap::Ggd] {
                            {
                                let on = status.input_map == m;
                                let rig = rig.clone();
                                rsx! {
                                    button {
                                        key: "{map_name(m)}",
                                        disabled: !live,
                                        style: "{row(on)}",
                                        onclick: move |_| {
                                            if let Some(r) = rig.clone() {
                                                spawn(async move { let _ = r.set_input_map(m).await; });
                                            }
                                        },
                                        span { style: "width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; {state_dot(on, DIM)} box-sizing: border-box;" }
                                        span { style: "flex: 1;", "{map_name(m)}" }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// A hardware drum map's name.
fn map_name(m: InputMap) -> &'static str {
    match m {
        InputMap::Direct => "Direct (GM)",
        InputMap::StrataPrime => "Alesis Strata Prime",
        InputMap::Fts => "FTS",
        InputMap::Ggd => "GGD",
    }
}

/// The 4×4 pads, pad 1 at the bottom left. A pad plays as a finger lands —
/// harder the higher on the pad — and lets go as it lifts.
#[component]
fn PadGrid(
    pieces: Vec<Piece>,
    pads: [Option<usize>; 16],
    selected: Option<usize>,
    held: Signal<[bool; 16]>,
    on_hit: EventHandler<usize>,
) -> Element {
    // Rows top to bottom: pads 13–16, 9–12, 5–8, 1–4.
    let order: Vec<usize> = (0..4)
        .rev()
        .flat_map(|r| (0..4).map(move |c| r * 4 + c))
        .collect();
    rsx! {
        div { style: "flex: 1; min-width: 0; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); grid-template-rows: repeat(4, minmax(0, 1fr)); gap: 10px;",
            for pad in order {
                {
                    let piece = pads[pad].and_then(|i| pieces.get(i).cloned().map(|p| (i, p)));
                    rsx! {
                        Pad { key: "{pad}", no: pad as u32 + 1, piece, selected, held, on_hit }
                    }
                }
            }
        }
    }
}

#[component]
fn Pad(
    no: u32,
    piece: Option<(usize, Piece)>,
    selected: Option<usize>,
    held: Signal<[bool; 16]>,
    on_hit: EventHandler<usize>,
) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    // The pad's height, measured: where on it a finger lands is how hard.
    let mut pad_h = use_signal(|| 160.0_f64);
    let slot = (no - 1) as usize;
    let Some((i, p)) = piece else {
        return rsx! {
            div { style: "border-radius: {R_MD}; border: 1px dashed {RULE_STRONG}; display: flex; align-items: flex-start; padding: 10px 12px; box-sizing: border-box;",
                span { style: "font-size: 12px; font-weight: 650; color: {DIM};", "{no}" }
            }
        };
    };
    let down = held.read()[slot];
    let on = selected == Some(i);
    let colour = p.family.colour();
    let note = p.note;
    let (r_down, r_up) = (rig.clone(), rig);
    let release = move || {
        let mut held = held;
        if held.peek()[slot] {
            held.write()[slot] = false;
            if let Some(r) = r_up.clone() {
                spawn(async move {
                    let _ = r.trigger(note, 0).await;
                });
            }
        }
    };
    let (rel1, rel2, rel3) = (release.clone(), release.clone(), release);
    let bg = if down {
        tint(colour, 55)
    } else {
        tint(colour, 12)
    };
    let edge = if on {
        format!("2px solid {FOCUS_FG}")
    } else {
        format!("1px solid {RULE_STRONG}")
    };
    rsx! {
        div {
            "aria-label": "Pad {no}: {p.label}",
            role: "button",
            onmounted: move |e| {
                let el = e.data();
                spawn(async move {
                    if let Ok(r) = el.get_client_rect().await
                        && r.height() > 1.0
                    {
                        pad_h.set(r.height());
                    }
                });
            },
            style: "position: relative; border-radius: {R_MD}; border: {edge}; background: {bg}; overflow: hidden; display: flex; flex-direction: column; justify-content: space-between; padding: 12px 14px; box-sizing: border-box; touch-action: none; user-select: none; cursor: pointer;",
            onpointerdown: move |e: PointerEvent| {
                e.prevent_default();
                // Higher on the pad, harder: 127 at the top, 40 at the foot.
                let y = e.element_coordinates().y;
                let h = pad_h.peek().max(1.0);
                let vel = (127.0 - (y / h).clamp(0.0, 1.0) * 87.0).round() as u32;
                let mut held = held;
                held.write()[slot] = true;
                on_hit.call(i);
                if let Some(r) = r_down.clone() {
                    spawn(async move {
                        let _ = r.trigger(note, vel.clamp(1, 127)).await;
                    });
                }
            },
            onpointerup: move |_| rel1(),
            onpointercancel: move |_| rel2(),
            onpointerleave: move |_| rel3(),
            // The family's band along the top.
            span { style: "position: absolute; left: 0; right: 0; top: 0; height: 6px; background: {colour};" }
            div { style: "display: flex; justify-content: space-between; align-items: baseline; margin-top: 4px;",
                span { style: "font-size: 12px; font-weight: 700; color: {INK_3}; font-variant-numeric: tabular-nums;", "{no}" }
                span { style: "font-size: 12px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{p.note}" }
            }
            div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                span { style: "font-size: 19px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.label}" }
                if !p.instrument.is_empty() {
                    span { style: "font-size: 13px; color: {INK_2}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.instrument}" }
                }
            }
        }
    }
}
