//! The iPad layout — the touch prototype (`prototypes/touch`) on the live
//! rig. One screen in four views, Perform · Build · Edit · Setup:
//!
//!   top bar    the sidebar toggle, the footswitch mode, then the rig's
//!              health and safety: MIDI, CPU, the meters, Mute and Panic
//!   sidebar    the setlist (see `setlist`)
//!   main       Perform: the macro bar on top, the footswitches at the foot
//!   foot bar   the views as tabs (a bar along the top edge when open);
//!              Perform's docks as toggles (a fill when on); Tuner; Setup
//!
//! Phase 1 of the port: the shell, the setlist, Perform. Build, Edit and
//! Setup follow (the prototype's Browser, Edit and Setup views).

mod colors;
mod setlist;
mod tokens;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::PerformanceModel;

use self::tokens::*;
use crate::state::RigViewState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Perform,
    Build,
    Edit,
    Setup,
}

/// The tablet remote: everything but the switches, which the caller builds
/// (they carry the rig's callbacks).
#[component]
pub fn TabletRemote(model: PerformanceModel, state: RigViewState, switches: Element) -> Element {
    let mut view = use_signal(|| View::Perform);
    let mut sidebar = use_signal(|| true);
    let mut macros = use_signal(|| true);
    let mut docked = use_signal(|| true);
    rsx! {
        div { style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; background: {DESK}; color: {INK}; font-family: {FONT}; overflow: hidden;",
            TopBar { model: model.clone(), state, sidebar: sidebar(), on_sidebar: move |()| sidebar.toggle() }
            div { style: "flex: 1; min-height: 0; display: flex;",
                if sidebar() {
                    aside { style: "width: {SIDEBAR_W}px; flex-shrink: 0; height: 100%; border-right: 1px solid {RULE}; box-sizing: border-box;",
                        setlist::TabletSetlist { state }
                    }
                }
                main { style: "flex: 1; min-width: 0; height: 100%; display: flex; flex-direction: column; background: {MAIN};",
                    match view() {
                        View::Perform => rsx! {
                            if macros() {
                                div { style: "flex-shrink: 0; border-bottom: 1px solid {RULE};",
                                    crate::macro_bar::MacroBar { macros: state.macros }
                                }
                            }
                            div { style: "flex: 1; min-height: 0;" }
                            if docked() {
                                div { style: "flex-shrink: 0; border-top: 1px solid {RULE};", {switches} }
                            }
                        },
                        View::Build => rsx! { Later { what: "Build — the browser, picking for a part, a stack or a variation" } },
                        View::Edit => rsx! { Later { what: "Edit — routing and the block's FX row" } },
                        View::Setup => rsx! { Later { what: "Setup — guitar, audio and MIDI" } },
                    }
                }
            }
            FootBar {
                view: view(),
                on_view: move |v| view.set(v),
                macros: macros(),
                on_macros: move |()| macros.toggle(),
                switches: docked(),
                on_switches: move |()| docked.toggle(),
            }
            signal_widgets::PopupLayer {}
        }
    }
}

/// A view still to be ported.
#[component]
fn Later(what: &'static str) -> Element {
    rsx! {
        div { style: "flex: 1; display: flex; align-items: center; justify-content: center; padding: 24px; font-size: 13px; color: {INK_3};",
            "{what}"
        }
    }
}

// ── Top bar ────────────────────────────────────────────────────────────────

const MODES: [(u32, &str); 3] = [(0, "Preset"), (1, "Profile"), (2, "Setlist")];
/// The green floor under the mode that is set.
const LIVE_FLOOR: &str = "inset 0 -2px 0 #22c55e";

#[component]
fn TopBar(model: PerformanceModel, state: RigViewState, sidebar: bool, on_sidebar: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mode = model.perform_mode;
    let house = model.headphone.main_mute;
    let phones = model.headphone.phones_mute;
    let cpu = state.dsp.read().cpu;
    let (input, output) = (*state.in_level.read(), *state.out_level.read());
    let running = *state.running.read();
    let cpu_colour = if cpu >= 0.8 { VOID } else if cpu >= 0.5 { "#eab308" } else { LIVE };
    rsx! {
        header { style: "height: {TOP_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            BarIcon { label: "Sidebar", on: sidebar, onclick: move |_| on_sidebar.call(()),
                svg { width: "20", height: "20", view_box: "0 0 20 20", style: "display: block;",
                    rect { x: "2.5", y: "3.5", width: "15", height: "13", rx: "2.5", fill: "none", stroke: "currentColor", stroke_width: "1.5" }
                    path { d: "M8 3.5v13", stroke: "currentColor", stroke_width: "1.5" }
                }
            }
            // The footswitch mode: one control, three settings — flat, no box.
            div { style: "display: flex; align-items: stretch; gap: 2px; padding: 0 8px; border-left: 1px solid {RULE}; border-right: 1px solid {RULE};",
                for (m, label) in MODES {
                    button {
                        key: "{m}",
                        style: "padding: 0 14px; min-width: 44px; background: transparent; border: none; font: inherit; font-size: 15px; font-weight: {pick(mode == m, 700, 560)}; color: {pick(mode == m, INK, INK_3)}; box-shadow: {pick(mode == m, LIVE_FLOOR, NO_SHADOW)}; cursor: pointer;",
                        onclick: {
                            let rig = rig.clone();
                            move |_| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.set_perform_mode(m).await; });
                                }
                            }
                        },
                        "{label}"
                    }
                }
            }
            div { style: "flex: 1;" }
            // The rig's health: the engine, then its load.
            div { style: "display: flex; align-items: center; gap: 6px; padding: 0 10px; font-size: 12px; font-weight: 700; color: {pick(running, LIVE, VOID)};",
                span { style: "width: 8px; height: 8px; border-radius: 999px; background: currentColor;" }
                if running { "Audio" } else { "No audio" }
            }
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px; padding: 0 8px; font-size: 11px; font-weight: 750; color: {cpu_colour}; min-width: 44px;",
                span { "CPU" }
                span { style: "font-variant-numeric: tabular-nums;", "{pct(cpu.into())}%" }
            }
            Meters { input, output }
            // Safety, at the end: the house mute, then Panic, apart.
            div { style: "display: flex; align-items: stretch; border-left: 1px solid {RULE};",
                button {
                    style: "min-width: 72px; padding: 0 12px; border: none; font: inherit; font-size: 12px; font-weight: 750; letter-spacing: 0.04em; cursor: pointer; background: {pick(house, VOID, CLEAR)}; color: {pick(house, DANGER_INK, INK_2)};",
                    onclick: {
                        let rig = rig.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.set_mutes(!house, phones && !house).await; });
                            }
                        }
                    },
                    if house { "MUTED" } else { "MUTE" }
                }
                button {
                    style: "min-width: 64px; padding: 0 12px; border: none; border-left: 1px solid {RULE}; font: inherit; font-size: 12px; font-weight: 750; letter-spacing: 0.04em; cursor: pointer; background: transparent; color: #eab308;",
                    onclick: {
                        let rig = rig.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.panic().await; });
                            }
                        }
                    },
                    "PANIC"
                }
            }
        }
    }
}

/// IN and OUT, upright.
#[component]
fn Meters(input: f64, output: f64) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; padding: 0 12px;",
            for (label, level) in [("IN", input), ("OUT", output)] {
                div { key: "{label}", style: "display: flex; flex-direction: column; align-items: center; gap: 2px;",
                    div { style: "position: relative; width: 6px; height: 24px; border-radius: 2px; background: {WELL}; overflow: hidden;",
                        div { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {pct(level)}%; background: {pick(level > 0.9, VOID, LIVE)};" }
                    }
                    span { style: "font-size: 9px; font-weight: 700; color: {INK_3};", "{label}" }
                }
            }
        }
    }
}

#[component]
fn BarIcon(label: &'static str, on: bool, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        button {
            title: "{label}",
            "aria-label": "{label}",
            style: "width: 52px; display: flex; align-items: center; justify-content: center; background: transparent; border: none; color: {pick(on, INK, INK_3)}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            {children}
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
) -> Element {
    rsx! {
        footer { style: "height: {FOOT_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {SHEET}; padding: 0 6px; box-sizing: border-box;",
            FootButton { label: "Perform", on: view == View::Perform, pin: false, onclick: move |_| on_view.call(View::Perform),
                path { d: "M5 3.5v11l9-5.5Z", fill: "currentColor" }
            }
            FootButton { label: "Build", on: view == View::Build, pin: false, onclick: move |_| on_view.call(View::Build),
                rect { x: "2.5", y: "2.5", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                rect { x: "10", y: "2.5", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                rect { x: "2.5", y: "10", width: "5.5", height: "5.5", rx: "1.2", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                path { d: "M12.75 10v5.5M10 12.75h5.5", stroke: "currentColor", stroke_width: "1.4", stroke_linecap: "round" }
            }
            FootButton { label: "Edit", on: view == View::Edit, pin: false, onclick: move |_| on_view.call(View::Edit),
                path { d: "M4 2.5v13M9 2.5v13M14 2.5v13", stroke: "currentColor", stroke_width: "1.4", stroke_linecap: "round" }
                rect { x: "2.3", y: "10", width: "3.4", height: "2.6", rx: "0.8", fill: "currentColor" }
                rect { x: "7.3", y: "5", width: "3.4", height: "2.6", rx: "0.8", fill: "currentColor" }
                rect { x: "12.3", y: "8", width: "3.4", height: "2.6", rx: "0.8", fill: "currentColor" }
            }
            if view == View::Perform {
                Rule {}
                FootButton { label: "Macros", on: macros, pin: true, onclick: move |_| on_macros.call(()),
                    circle { cx: "4.5", cy: "9", r: "2.6", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                    circle { cx: "13.5", cy: "9", r: "2.6", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                    path { d: "M4.5 9 6 7.4M13.5 9l1.5-1.6M8 4.5h2M8 13.5h2", stroke: "currentColor", stroke_width: "1.4", stroke_linecap: "round" }
                }
                FootButton { label: "Switches", on: switches, pin: true, onclick: move |_| on_switches.call(()),
                    rect { x: "2", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                    rect { x: "7", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                    rect { x: "12", y: "5", width: "4", height: "8", rx: "1", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
                }
            }
            div { style: "flex: 1;" }
            Rule {}
            FootButton { label: "Setup", on: view == View::Setup, pin: false, onclick: move |_| on_view.call(View::Setup),
                Gear {}
            }
        }
    }
}

#[component]
fn Rule() -> Element {
    rsx! { span { style: "width: 1px; align-self: center; height: 22px; background: {RULE}; flex-shrink: 0; margin: 0 4px;" } }
}

/// A foot-bar button. A view is a tab: the one open is bright with a bar
/// along its top edge. A dock (`pin`) is a toggle: on, it sits in a fill.
#[component]
fn FootButton(label: &'static str, on: bool, pin: bool, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    // Explicit ink: Blitz does not re-resolve currentColor inside an SVG on
    // restyle, so the icon takes its colour as a value.
    let ink = if on { INK } else { INK_3 };
    let bg = if pin && on { FILL_ON } else { "transparent" };
    rsx! {
        button {
            title: "{label}",
            style: "position: relative; min-width: 64px; margin: {pick(pin, PIN_MARGIN, TAB_MARGIN)}; padding: 0 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; border: none; border-radius: {pick(pin, 8, 0)}px; background: {bg}; color: {ink}; font: inherit; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if !pin && on {
                span { style: "position: absolute; top: 0; left: 12px; right: 12px; height: 3px; border-radius: 0 0 3px 3px; background: {INK};" }
            }
            // Keyed by its state: rebuilt, not restyled, when it changes.
            svg { key: "{on}", width: "20", height: "20", view_box: "0 0 18 18", color: "{ink}", style: "display: block; color: {ink};", {children} }
            span { style: "font-size: 11px; font-weight: {pick(on, 700, 600)};", "{label}" }
        }
    }
}

/// A gear: teeth around a hub.
#[component]
fn Gear() -> Element {
    let teeth = 8;
    let pts: Vec<String> = (0..teeth * 4)
        .map(|i| {
            let a = (f64::from(i) / f64::from(teeth * 4)) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
            let r = if i % 4 == 1 || i % 4 == 2 { 7.6 } else { 5.9 };
            format!("{:.2},{:.2}", 9.0 + r * a.cos(), 9.0 + r * a.sin())
        })
        .collect();
    let points = pts.join(" ");
    rsx! {
        polygon { points: "{points}", fill: "none", stroke: "currentColor", stroke_width: "1.4", stroke_linejoin: "round" }
        circle { cx: "9", cy: "9", r: "2.4", fill: "none", stroke: "currentColor", stroke_width: "1.4" }
    }
}
