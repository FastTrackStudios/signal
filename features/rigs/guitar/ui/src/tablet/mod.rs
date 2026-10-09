//! The iPad layout — the touch prototype (`prototypes/touch`) on the live
//! rig, ported one to one. One screen in four views, Perform · Build ·
//! Edit · Setup:
//!
//!   top bar    over the sidebar its toggle and the footswitch mode; then
//!              Panic, the MIDI and Audio lights, CPU, the meters, Mute
//!   sidebar    the setlist (see `setlist`)
//!   main       Perform: the macro bar along the top (its panels drop
//!              down), the browser on call in the middle, the switches at
//!              the foot
//!   foot bar   the views as tabs (a bar along the top edge when open);
//!              Perform's docks as toggles (a fill when on); Tuner; Setup
//!
//! Build is the browser (`browser`); Edit is `edit`; Setup is `setup`.

mod browser;
mod colors;
pub(crate) mod splash;
mod tones;
mod edit;
mod macros;
mod marks;
mod menu;
mod panels;
mod player;
mod routing;
mod routing_canvas;
mod fx_row;
mod setlist;
mod setup;
mod sidebar_views;
mod switches;
mod tokens;
mod topbar;

use dioxus::prelude::*;
use signal_guitar_proto::PerformanceModel;

use self::tokens::*;
use self::topbar::Rule;
use crate::state::RigViewState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Perform,
    Build,
    Edit,
    Setup,
}

/// The part picked in Build (Setlist mode): what the browser builds into.
/// Shared by the setlist (whose section rows pick it) and the browser.
#[derive(Clone, Copy, PartialEq)]
pub struct BuildPick {
    pub part: Signal<Option<usize>>,
    /// Whether a section row's tap picks (Build) or plays (Perform).
    pub building: Signal<bool>,
}

#[component]
pub fn TabletRemote(model: PerformanceModel, state: RigViewState) -> Element {
    let mut view = use_signal(|| View::Perform);
    let mut sidebar = use_signal(|| true);
    let mut macros = use_signal(|| true);
    let mut docked = use_signal(|| true);
    // Perform's middle: the routing grid, unless it's switched off.
    let mut routing_on = use_signal(|| true);
    // Perform's FX row: the selected block's controls, in the switches'
    // place — a block picked in the grid brings it up, nothing picked (a
    // tap on the bare grid) puts the switches back.
    let mut fx_on = use_signal(|| false);
    let mut browser = use_signal(|| false);
    // Edit's left pane: the browser, or the sidebar a tap away.
    let mut left_browser = use_signal(|| true);
    let setup_tab = use_signal(|| setup::SetupTab::Guitar);
    let pick = use_context_provider(|| BuildPick { part: Signal::new(None), building: Signal::new(false) });
    // Edit's routing selection, and where it points the browser.
    let routing_sel = use_context_provider(|| routing::RoutingSel(Signal::new(None))).0;
    use_effect(move || {
        let picked = routing_sel().is_some();
        fx_on.set(picked);
        docked.set(!picked);
    });
    let browser_focus = use_context_provider(|| routing::BrowserFocus(Signal::new(None)));
    // The faces' presets (their names, the FX row's bar).
    let face_comp = sidebar_views::use_compositions(state);
    use_context_provider(|| crate::face_chrome::FacePresets(face_comp));
    // The sidebar turned into a browser on one block's (or module's)
    // presets — the FX row's bar opens it.
    let mut sidebar_browser = use_signal(|| false);
    // Its slot, for TONE3000 (an amp, a drive slot), and which source shows.
    let mut tone_slot = use_signal(|| None::<String>);
    let mut tone_tab = use_signal(|| false);
    // With the sidebar on a block's presets, picking another block in the
    // routing turns it to that block's.
    use_effect(move || {
        let picked = routing_sel();
        if !*sidebar_browser.peek() {
            return;
        }
        let kind = match picked {
            Some(routing::Selected::Block(id)) => state.blocks.peek().iter().find(|b| b.id == id).map(|b| {
                tone_slot.set(tones::slot_of(&b.name, b.block_type.as_str()));
                browser_kind(b.block_type.as_str())
            }),
            Some(routing::Selected::Module(m, _)) => {
                tone_slot.set(tones::slot_of(&m, &format!("module:{m}")));
                Some(format!("module:{m}"))
            }
            None => None,
        };
        if let Some(kind) = kind {
            let mut f = browser_focus.0;
            f.set(Some(routing::Focus { kind, preset: String::new(), variation: String::new(), search: false }));
        }
    });
    // A face's preset name: the browser, on that block's presets.
    use_context_provider(|| {
        crate::face_chrome::OpenPresets(Callback::new(move |(block, block_type): (String, String)| {
            tone_slot.set(tones::slot_of(&block, &block_type));
            tone_tab.set(false);
            let mut f = browser_focus.0;
            let kind = browser_kind(&block_type);
            f.set(Some(routing::Focus { kind, preset: String::new(), variation: String::new(), search: false }));
            // In the sidebar, the routing and the FX row kept in view.
            match view() {
                View::Edit => left_browser.set(true),
                _ => {
                    sidebar.set(true);
                    sidebar_browser.set(true);
                }
            }
        }))
    });
    // "Patch…" on a section or part, or the presets' search: bring the
    // browser up (in Edit it takes the sidebar; in Build it is already there).
    use_context_provider(|| setlist::PickPart {
        open: Callback::new(move |()| {
            match view() {
                View::Perform => {
                    browser.set(true);
                    routing_on.set(false);
                }
                View::Edit => left_browser.set(true),
                _ => {}
            }
        }),
    });
    // A press anywhere outside the macro bar and its panel closes the panel
    // (the press still lands where it was meant).
    let away = use_context_provider(macros::MacroAway::new);
    use_effect(move || {
        let mut b = pick.building;
        b.set(view() == View::Build);
    });
    rsx! {
        div {
            style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; background: {DESK}; color: {INK}; font-family: {FONT}; font-weight: 500; overflow: hidden;",
            onpointerdown: move |_| away.press(),
            // Every button answers a press (the prototype's `.pressable`).
            document::Style { {PRESSABLE} }
            topbar::TopBar {
                model: model.clone(),
                state,
                sidebar: sidebar(),
                on_sidebar: move |()| sidebar.toggle(),
                on_setup: move |tab: &'static str| {
                    let mut t = setup_tab;
                    t.set(setup::SetupTab::from_id(tab));
                    view.set(View::Setup);
                },
            }
            div { style: "flex: 1; min-height: 0; display: flex;",
                if view() == View::Setup {
                    setup::SetupView { state, tab: setup_tab }
                } else {
                    if sidebar() {
                        aside { style: "width: {SIDEBAR_W}px; flex-shrink: 0; height: 100%; border-right: 1px solid {RULE}; box-sizing: border-box; display: flex; flex-direction: column; min-height: 0;",
                            // In Edit, where routing and the FX row hold the
                            // main area: the browser, the sidebar a tap away.
                            if view() == View::Edit {
                                LeftSwitch { browser: left_browser(), mode: model.perform_mode, on_change: move |b| left_browser.set(b) }
                            }
                            div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
                                if view() == View::Edit && left_browser() {
                                    browser::Browser { state, narrow: true }
                                } else if sidebar_browser() {
                                    // An amp or a drive slot: its presets, and at their foot
                                    // the way to TONE3000's; there, the way back.
                                    if let Some(slot) = tone_slot().filter(|_| tones::available()) {
                                        if tone_tab() {
                                            div { style: "flex-shrink: 0; height: 52px; display: flex; align-items: center; gap: 4px; padding: 0 6px; border-bottom: 1px solid {RULE}; background: {SHEET};",
                                                button {
                                                    style: "height: 44px; display: flex; align-items: center; gap: 8px; padding: 0 10px; border: none; background: transparent; color: {INK_2}; font-size: 15px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
                                                    onclick: move |_| tone_tab.set(false),
                                                    svg { width: "9", height: "15", view_box: "0 0 9 15",
                                                        path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                                                    }
                                                    "{slot}"
                                                }
                                                span { style: "flex: 1; text-align: center; font-size: 14px; font-weight: 800; letter-spacing: 0.06em; color: {INK};", "TONE3000" }
                                                span { style: "width: 80px;" }
                                            }
                                            tones::TonesPanel { slot }
                                        } else {
                                            browser::Browser { state, narrow: true, on_close: move |()| sidebar_browser.set(false) }
                                            div { style: "flex-shrink: 0; padding: 12px 14px; border-top: 1px solid {RULE};",
                                                button {
                                                    style: "width: 100%; height: 44px; display: flex; align-items: center; justify-content: center; gap: 8px; border: none; border-radius: {R}; background: {FILL}; color: {INK}; font-size: 15px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
                                                    onclick: move |_| tone_tab.set(true),
                                                    "More on TONE3000"
                                                }
                                            }
                                        }
                                    } else {
                                        browser::Browser { state, narrow: true, on_close: move |()| sidebar_browser.set(false) }
                                    }
                                } else {
                                    // The sidebar for the footswitch mode: the set,
                                    // the profile's stacks, or the presets.
                                    match model.perform_mode {
                                        0 => rsx! { sidebar_views::PresetView { state } },
                                        1 => rsx! { sidebar_views::ProfileView { state } },
                                        _ => rsx! { setlist::TabletSetlist { state } },
                                    }
                                }
                            }
                        }
                    }
                    main { style: "flex: 1; min-width: 0; height: 100%; display: flex; flex-direction: column; background: {DESK};",
                        match view() {
                            View::Perform => rsx! {
                                // The macros along the top: what you turn while playing.
                                if macros() {
                                    div { style: "flex-shrink: 0; position: relative; z-index: 4; border-bottom: 1px solid #000;",
                                        macros::TouchMacroBar { state }
                                    }
                                }
                                // The middle: the browser when it's asked for, else the
                                // routing grid.
                                div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
                                    if browser() {
                                        browser::Browser { state, on_close: move |()| {
                            browser.set(false);
                            routing_on.set(true);
                        } }
                                    } else if routing_on() {
                                        div { style: "flex: 1; min-height: 0;", routing::Routing { state } }
                                    }
                                }
                                if fx_on() {
                                    fx_row::FxRow { state }
                                } else if docked() {
                                    div { style: "flex-shrink: 0;", switches::TouchSwitches { perf: model.clone() } }
                                }
                            },
                            View::Build => rsx! {
                                div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
                                    browser::Browser { state }
                                }
                            },
                            View::Edit => rsx! { edit::EditView { state, perf: model.clone() } },
                            View::Setup => rsx! {},
                        }
                    }
                }
            }
            FootBar {
                view: view(),
                on_view: move |v| view.set(v),
                macros: macros(),
                on_macros: move |()| macros.toggle(),
                switches: docked(),
                on_switches: move |()| {
                    docked.toggle();
                    if docked() {
                        fx_on.set(false);
                    }
                },
                routing: routing_on(),
                // The middle holds one at a time: the routing, or the browser.
                on_routing: move |()| {
                    routing_on.toggle();
                    if routing_on() {
                        browser.set(false);
                    }
                },
                fx: fx_on(),
                on_fx: move |()| {
                    fx_on.toggle();
                    if fx_on() {
                        docked.set(false);
                    }
                },
                browser: browser(),
                state,
                on_browser: move |()| {
                    browser.toggle();
                    routing_on.set(!browser());
                },
            }
            signal_widgets::PopupLayer {}
        }
    }
}

/// Edit's left-pane tabs: the browser, or the sidebar named for the mode.
#[component]
fn LeftSwitch(browser: bool, mode: u32, on_change: EventHandler<bool>) -> Element {
    let name = match mode {
        0 => "Presets",
        1 => "Profile",
        _ => "Setlist",
    };
    rsx! {
        div { role: "tablist", style: "flex-shrink: 0; display: flex; border-bottom: 1px solid {RULE}; background: {SHEET};",
            for (is_browser, label) in [(true, "Browser"), (false, name)] {
                button {
                    key: "{label}",
                    role: "tab",
                    style: "position: relative; flex: 1; height: 44px; border: none; background: transparent; font-family: {FONT}; font-size: 14px; font-weight: {pick(browser == is_browser, 700, 560)}; color: {pick(browser == is_browser, INK, INK_3)}; cursor: pointer;",
                    onclick: move |_| on_change.call(is_browser),
                    "{label}"
                    if browser == is_browser {
                        span { style: "position: absolute; left: 16px; right: 16px; bottom: 0; height: 2px; border-radius: 1px; background: {INK_2};" }
                    }
                }
            }
        }
    }
}

// ── Foot bar ───────────────────────────────────────────────────────────────

#[component]
fn FootBar(
    view: View,
    on_view: EventHandler<View>,
    macros: bool,
    on_macros: EventHandler<()>,
    switches: bool,
    on_switches: EventHandler<()>,
    routing: bool,
    on_routing: EventHandler<()>,
    fx: bool,
    on_fx: EventHandler<()>,
    browser: bool,
    on_browser: EventHandler<()>,
    state: RigViewState,
) -> Element {
    let (perform, build, edit, setup) = (view == View::Perform, view == View::Build, view == View::Edit, view == View::Setup);
    rsx! {
        footer { style: "height: {FOOT_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {SHEET}; padding: 0 6px; box-sizing: border-box;",
            FootButton { label: "Perform", on: perform, pin: false, onclick: move |_| on_view.call(View::Perform),
                path { d: "M5 3.5v11l9-5.5Z", fill: ink(perform) }
            }
            FootButton { label: "Build", on: build, pin: false, onclick: move |_| on_view.call(View::Build),
                rect { x: "2.5", y: "2.5", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: ink(build), stroke_width: "1.4" }
                rect { x: "10", y: "2.5", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: ink(build), stroke_width: "1.4" }
                rect { x: "2.5", y: "10", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: ink(build), stroke_width: "1.4" }
                path { d: "M12.75 10v5.5M10 12.75h5.5", stroke: ink(build), stroke_width: "1.4", stroke_linecap: "round" }
            }
            FootButton { label: "Edit", on: edit, pin: false, onclick: move |_| on_view.call(View::Edit),
                path { d: "M4 2.5v13M9 2.5v13M14 2.5v13", stroke: ink(edit), stroke_width: "1.4", stroke_linecap: "round" }
                rect { x: "2.3", y: "10", width: "3.4", height: "2.6", rx: "0.8", fill: ink(edit) }
                rect { x: "7.3", y: "5", width: "3.4", height: "2.6", rx: "0.8", fill: ink(edit) }
                rect { x: "12.3", y: "8", width: "3.4", height: "2.6", rx: "0.8", fill: ink(edit) }
            }
            // Perform: what the middle holds — the routing or the browser —
            // and the macros along the top.
            if perform {
                Rule {}
                Group {
                    FootButton { label: "Routing", on: routing, pin: true, grouped: true, onclick: move |_| on_routing.call(()),
                        circle { cx: "3.5", cy: "9", r: "1.8", fill: "none", stroke: ink(routing), stroke_width: "1.4" }
                        circle { cx: "14.5", cy: "4.5", r: "1.8", fill: "none", stroke: ink(routing), stroke_width: "1.4" }
                        circle { cx: "14.5", cy: "13.5", r: "1.8", fill: "none", stroke: ink(routing), stroke_width: "1.4" }
                        path { d: "M5.3 9H8m0 0c2 0 2-4.5 4.7-4.5M8 9c2 0 2 4.5 4.7 4.5", fill: "none", stroke: ink(routing), stroke_width: "1.4", stroke_linecap: "round" }
                    }
                    FootButton { label: "Browser", on: browser, pin: true, grouped: true, onclick: move |_| on_browser.call(()),
                        path { d: "M3 3.5h3v11H3ZM7.5 3.5h3v11h-3ZM12 4l2.8-.8 2 10.6-2.8.8Z", fill: "none", stroke: ink(browser), stroke_width: "1.4", stroke_linejoin: "round" }
                    }
                }
                FootButton { label: "Macros", on: macros, pin: true, onclick: move |_| on_macros.call(()),
                    circle { cx: "4.5", cy: "9", r: "2.6", fill: "none", stroke: ink(macros), stroke_width: "1.4" }
                    circle { cx: "13.5", cy: "9", r: "2.6", fill: "none", stroke: ink(macros), stroke_width: "1.4" }
                    path { d: "M4.5 9 6 7.4M13.5 9l1.5-1.6", stroke: ink(macros), stroke_width: "1.4", stroke_linecap: "round" }
                    path { d: "M8 4.5h2M8 13.5h2", stroke: ink(macros), stroke_width: "1.4", stroke_linecap: "round" }
                }
            }
            // The FX row's block and its preset, while the row is up.
            if (perform && fx) || edit {
                fx_row::FxPresetBar { state }
            } else {
                span { style: "flex: 1;" }
            }
            // Perform's dock, at the right: the switches or the FX row, one
            // at a time.
            if perform {
                Group {
                    FootButton { label: "Switches", on: switches, pin: true, grouped: true, onclick: move |_| on_switches.call(()),
                        rect { x: "2", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: ink(switches), stroke_width: "1.4" }
                        rect { x: "7", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: ink(switches), stroke_width: "1.4" }
                        rect { x: "12", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: ink(switches), stroke_width: "1.4" }
                    }
                    FootButton { label: "FX", on: fx, pin: true, grouped: true, onclick: move |_| on_fx.call(()),
                        circle { cx: "5", cy: "6", r: "2.2", fill: "none", stroke: ink(fx), stroke_width: "1.4" }
                        circle { cx: "13", cy: "12", r: "2.2", fill: "none", stroke: ink(fx), stroke_width: "1.4" }
                        path { d: "M2 6h0.8M7.2 6H16M2 12h8.8M15.2 12H16", stroke: ink(fx), stroke_width: "1.4", stroke_linecap: "round" }
                    }
                }
            }
            Rule {}
            FootButton { label: "Setup", on: setup, pin: false, onclick: move |_| on_view.call(View::Setup),
                Gear { colour: ink(setup) }
            }
        }
    }
}

/// An icon's ink, by its button's state — explicit, since Blitz does not
/// re-resolve `currentColor` inside an SVG on restyle.
fn ink(on: bool) -> &'static str {
    if on { INK } else { INK_3 }
}

/// A foot-bar button. A view (Perform, Build, Edit, Setup) is a tab: the
/// one open is bright with a bar along its top edge. A dock (`pin`) is a
/// toggle: on, it sits in a fill.
#[component]
fn FootButton(label: &'static str, on: bool, pin: bool, #[props(default)] grouped: bool, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    let colour = ink(on);
    let bg = if pin && on { FILL_ON } else { CLEAR };
    let margin = if grouped { "0" } else { pick(pin, PIN_MARGIN, TAB_MARGIN) };
    rsx! {
        button {
            title: "{label}",
            "aria-pressed": "{on}",
            style: "position: relative; min-width: 64px; margin: {margin}; padding: 0 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; border: none; border-radius: {pick(pin, 8, 0)}px; background: {bg}; color: {colour}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if !pin && on {
                span { style: "position: absolute; top: 0; left: 12px; right: 12px; height: 3px; border-radius: 0 0 3px 3px; background: {INK};" }
            }
            // Keyed by its state: rebuilt, not restyled, when it changes.
            svg { key: "{on}", width: "20", height: "20", view_box: "0 0 18 18", style: "display: block;", {children} }
            span { style: "font-size: 11px; font-weight: {pick(on, 700, 600)}; letter-spacing: 0.01em; color: {colour};", "{label}" }
        }
    }
}

/// The browser's kind for a block type: a drive slot's pedals are the
/// Drive module's presets; a module's its own; any other block its type's.
fn browser_kind(block_type: &str) -> String {
    let t = block_type.to_lowercase();
    if t.starts_with("module:") {
        block_type.to_string()
    } else if t == "drive" || t == "boost" {
        "module:Drive".to_string()
    } else {
        format!("block:{t}")
    }
}

/// Toggles that share one place, side by side in a well: one of them on
/// at a time.
#[component]
fn Group(children: Element) -> Element {
    rsx! {
        div { style: "display: flex; align-items: stretch; gap: 2px; margin: 5px 4px; padding: 2px; border-radius: 10px; background: rgba(255,255,255,0.04);", {children} }
    }
}

/// A gear: teeth around a hub.
#[component]
fn Gear(colour: &'static str) -> Element {
    let teeth = 8;
    let points: String = (0..teeth * 4)
        .map(|i| {
            let a = (f64::from(i) / f64::from(teeth * 4)) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
            let r = if i % 4 == 1 || i % 4 == 2 { 7.6 } else { 5.9 };
            format!("{:.2},{:.2}", 9.0 + r * a.cos(), 9.0 + r * a.sin())
        })
        .collect::<Vec<_>>()
        .join(" ");
    rsx! {
        polygon { points: "{points}", fill: "none", stroke: colour, stroke_width: "1.4", stroke_linejoin: "round" }
        circle { cx: "9", cy: "9", r: "2.4", fill: "none", stroke: colour, stroke_width: "1.4" }
    }
}

/// A pressed button sinks a little and lightens — over its own background,
/// which is inline, hence `!important`.
const PRESSABLE: &str = "button:not(:disabled):active { transform: scale(0.985); background-image: linear-gradient(rgba(255,255,255,0.07), rgba(255,255,255,0.07)) !important; }";
