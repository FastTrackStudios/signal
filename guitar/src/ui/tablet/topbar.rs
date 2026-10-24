//! The top bar — the prototype's `TopBar`, `ModeButton`, `Safety`,
//! `Settings` indicators, `Cpu` and `Meters`, on the rig.
//!
//!   over the sidebar   its toggle, and the footswitch mode (one button, its
//!                      menu picks — a green line along its foot)
//!   health & safety    Panic (with the health it fixes, far from Mute), the
//!                      MIDI and Audio lights (a press opens Setup), CPU, the
//!                      IN · OUT · PHONES meters, and Mute: a tap mutes the
//!                      house, its ⋯ offers muting fully

use std::time::Duration;

use crate::proto::PerformanceModel;
use crate::proto::rig::RigClient;
use dioxus::prelude::*;
use signal_widgets::PopupHost;

use super::menu::{Item, MENU_W, Picked, open_menu};
use super::tokens::*;
use crate::ui::state::RigViewState;
pub use signal_rig_shell::Rule;
use signal_rig_shell::{Cpu, Indicator, Meters, TopBar as SignalTopBar};

#[component]
pub fn TopBar(
    model: PerformanceModel,
    state: RigViewState,
    sidebar: bool,
    on_sidebar: EventHandler<()>,
    on_setup: EventHandler<&'static str>,
) -> Element {
    // Panic resetting: the meters go flat until it is done.
    let resetting = use_signal(|| false);
    let midi = use_midi_health();
    let hp = &model.headphone;
    let output = *state.out_level.read();
    // What the phones hear: the guitar at its own level, the band's mix at
    // its, under the phones fader.
    let (mix_l, mix_r) = *state.mix_db.read();
    let mix = crate::ui::meters::meter_level(10f32.powf(mix_l.max(mix_r) / 20.0));
    let phones =
        (output * f64::from(hp.self_mix)).max(mix * f64::from(hp.mix_level)) * f64::from(hp.volume);
    // Resetting here, or anywhere: the rig says so to every remote.
    let flat = resetting() || model.panicking;
    // Audio struggling: amber for a few seconds after a dropout.
    let mut struggling = use_signal(|| 0u32);
    use_hook(move || {
        spawn(async move {
            let mut seen: Option<u64> = None;
            loop {
                architect::platform::sleep(Duration::from_secs(1)).await;
                let perf = state.dsp.peek().clone();
                let drops = perf.over_budget + perf.xruns;
                if seen.is_some_and(|s| drops > s) {
                    struggling.set(5);
                } else if struggling() > 0 {
                    struggling.set(struggling() - 1);
                }
                seen = Some(drops);
            }
        })
    });
    rsx! {
        SignalTopBar {
            sidebar,
            on_sidebar: move |()| on_sidebar.call(()),
            side: rsx! { ModeTabs { mode: model.perform_mode } },
            span { style: "flex: 1;" }
            // Audio not running: a badge that starts it (or, after a
            // failure, restarts it).
            if !*state.running.read() {
                AudioBadge { failed: !state.audio_error.read().is_empty() }
            }
            Rule {}
            super::player::Player {}
            Rule {}
            PanicButton { busy: resetting }
            Rule {}
            Indicator { kind: "midi", tone: midi, onclick: move |_| on_setup.call("midi") }
            Indicator { kind: "audio", tone: if !*state.running.read() { VOID } else if struggling() > 0 { WARN } else { LIVE }, onclick: move |_| on_setup.call("audio") }
            Cpu { cpu: f64::from(state.dsp.read().cpu) }
            Rule {}
            Meters {
                input: if flat { 0.0 } else { *state.in_level.read() },
                output: if flat { 0.0 } else { output },
                phones_level: if flat { 0.0 } else { phones },
                house: model.headphone.main_mute,
                phones: model.headphone.phones_mute,
            }
            Rule {}
            MuteButton { house: model.headphone.main_mute, phones: model.headphone.phones_mute }
        }
    }
}

/// The performer screen's app bar (48 tall; mounts spec §2.2): the mark,
/// the set up, the rig crumb with its own ⋯ (Edit switches, MIDI learn,
/// Switch rig — the foot bar's Switches menu, moved here; no "Hidden",
/// the dock never hides), the app's `middle` (its sync pill), the MIDI
/// and Audio lights, Mute and Panic (one tap each, never under a menu:
/// the menus open below the bar), and the Views ⋯ (Build, Edit, Setup).
#[component]
pub(super) fn PerformerBar(middle: Element) -> Element {
    let rig = use_context::<super::PerformerRig>();
    let model = rig.model.read().clone();
    let state = rig.state;
    let resetting = use_signal(|| false);
    let midi = use_midi_health();
    let mut menu = use_signal(|| None::<BarMenu>);
    let account = try_use_context::<super::BarAccountItem>();
    let set = model
        .setlists
        .get(model.setlist_index as usize)
        .cloned()
        .unwrap_or_default();
    let crumb = if model.profile_name.is_empty() {
        "Rig".to_string()
    } else {
        cut(&model.profile_name, 12)
    };
    let audio = if *state.running.read() { LIVE } else { VOID };
    rsx! {
        div { style: "position: relative; display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; height: 48px; padding: 0 0 0 12px; box-sizing: border-box; font-family: {FONT};",
            span { style: "flex-shrink: 0; font-size: 15px; font-weight: 800; letter-spacing: 0.04em; color: {INK};", "Signal" }
            if !set.is_empty() {
                span { "aria-label": "Set: {set}", style: "flex-shrink: 0; padding: 0 10px; font-size: 15px; font-weight: 650; color: {INK_2}; white-space: nowrap;", "{cut(&set, 14)}" }
            }
            // The rig crumb and its ⋯.
            div { style: "position: relative; display: flex; align-items: center; flex-shrink: 0;",
                span { style: "padding: 0 4px 0 10px; font-size: 15px; font-weight: 700; color: {INK}; white-space: nowrap;", "{crumb}" }
                button {
                    "aria-label": "Rig options",
                    "aria-expanded": "{menu() == Some(BarMenu::Rig)}",
                    style: "{BAR_BTN} width: 48px;",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        flip(menu, BarMenu::Rig);
                    },
                    {dots()}
                }
                if menu() == Some(BarMenu::Rig) {
                    div { role: "menu", "aria-label": "Rig options", style: "{BAR_MENU} left: 0;",
                        for (id, label) in RIG_ITEMS {
                            button {
                                key: "{id}",
                                role: "menuitem",
                                "aria-label": "{label}",
                                style: BAR_ITEM,
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    menu.set(None);
                                    match id {
                                        "edit" => rig.go(super::View::Edit),
                                        "learn" => rig.setup("midi"),
                                        _ => rig.setup("guitar"),
                                    }
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
            span { style: "flex: 1;" }
            if !*state.running.read() {
                AudioBadge { failed: !state.audio_error.read().is_empty() }
            }
            {middle}
            Indicator { kind: "midi", tone: midi, onclick: move |_| rig.setup("midi") }
            Indicator { kind: "audio", tone: audio, onclick: move |_| rig.setup("audio") }
            div { style: "display: flex; align-items: stretch; align-self: stretch; flex-shrink: 0;",
                MuteButton { house: model.headphone.main_mute, phones: model.headphone.phones_mute }
            }
            div { style: "display: flex; align-items: stretch; align-self: stretch; flex-shrink: 0; margin-left: 8px;",
                PanicButton { busy: resetting }
            }
            div { style: "position: relative; display: flex; align-items: center; flex-shrink: 0;",
                button {
                    "aria-label": "Views and settings",
                    "aria-expanded": "{menu() == Some(BarMenu::Views)}",
                    style: "{BAR_BTN} width: 48px;",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        flip(menu, BarMenu::Views);
                    },
                    {dots()}
                }
                if menu() == Some(BarMenu::Views) {
                    div { role: "menu", "aria-label": "Views", style: "{BAR_MENU} right: 0;",
                        // The app's Account item first, above a hairline.
                        if let Some(account) = account {
                            {(account.0)(Callback::new(move |()| menu.set(None)))}
                            div { style: "height: 1px; margin: 2px 6px; background: #2b2b31;" }
                        }
                        for (id, label) in VIEW_ITEMS {
                            button {
                                key: "{id}",
                                role: "menuitem",
                                "aria-label": "{label}",
                                style: BAR_ITEM,
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    menu.set(None);
                                    match id {
                                        "build" => rig.go(super::View::Build),
                                        "edit" => rig.go(super::View::Edit),
                                        _ => rig.setup("guitar"),
                                    }
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Open a bar menu, or close it when it is the one open.
fn flip(mut menu: Signal<Option<BarMenu>>, m: BarMenu) {
    let now = *menu.peek();
    menu.set(if now == Some(m) { None } else { Some(m) });
}

/// The performer bar's two menus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BarMenu {
    Rig,
    Views,
}

/// The rig crumb's ⋯: what the foot bar's Switches menu was, less
/// "Hidden" (the performer screen's dock never hides; spec §8.4).
pub(super) const RIG_ITEMS: [(&str, &str); 3] = [
    ("edit", "Edit switches"),
    ("learn", "MIDI learn"),
    ("switch", "Switch rig"),
];

/// The app bar's ⋯: the rig's other views.
pub(super) const VIEW_ITEMS: [(&str, &str); 3] =
    [("build", "Build"), ("edit", "Edit"), ("setup", "Setup")];

const BAR_BTN: &str = "height: 48px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                       border: none; border-radius: 8px; background: transparent; padding: 0; cursor: pointer;";
/// A bar menu: under the bar (y = 48), so Mute and Panic stay uncovered.
const BAR_MENU: &str = "position: absolute; top: 48px; z-index: 60; display: flex; flex-direction: column; gap: 2px; \
                        padding: 4px; min-width: 200px; border-radius: 10px; background: #141418; border: 1px solid #2b2b31;";
const BAR_ITEM: &str = "height: 52px; padding: 0 14px; text-align: left; border: none; border-radius: 8px; \
                        background: transparent; color: #e4e4e7; font-size: 16px; font-weight: 650; cursor: pointer;";

fn dots() -> Element {
    rsx! {
        svg { width: "18", height: "4", view_box: "0 0 18 4", style: "display: block;",
            circle { cx: "2", cy: "2", r: "1.8", fill: INK_2 }
            circle { cx: "9", cy: "2", r: "1.8", fill: INK_2 }
            circle { cx: "16", cy: "2", r: "1.8", fill: INK_2 }
        }
    }
}

/// `s` cut to `max` characters with "…" (no `text-overflow` in Blitz).
fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut c: String = s.chars().take(max.saturating_sub(1)).collect();
        c.push('…');
        c
    }
}

/// The footswitch mode as the rig frame's three tabs — Preset, Profile,
/// Setlist — what the sidebar shows and the switches play.
#[component]
fn ModeTabs(mode: u32) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        signal_rig_shell::ModeTabs {
            mode,
            aria: "Footswitches play the %",
            on_pick: move |m: u32| {
                if let Some(r) = rig.clone() {
                    let _ = dioxus_core::spawn_forever(async move { let _ = r.set_perform_mode(m).await; });
                }
            },
        }
    }
}

/// Panic: everything silent at once, the mutes back once the tails die.
/// One tap — it's for emergencies — and it shows it's working.
#[component]
fn PanicButton(busy: Signal<bool>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut busy = busy;
    // The reset arrow turns while it works.
    let mut turn = use_signal(|| 0u32);
    let (ink, bg) = if busy() {
        ("#1a1205", "#fbbf24")
    } else {
        ("#fbbf24", CLEAR)
    };
    rsx! {
        button {
            "aria-label": "Panic — all notes off, reset audio and MIDI",
            style: "align-self: stretch; min-width: 48px; flex-shrink: 0; padding: 0 12px; display: flex; align-items: center; justify-content: center; border: none; background: {bg}; cursor: pointer;",
            onclick: move |_| {
                if busy() { return; }
                busy.set(true);
                if let Some(r) = rig.clone() {
                    let _ = dioxus_core::spawn_forever(async move { let _ = r.panic().await; });
                }
                spawn(async move {
                    // 1.2 s, the arrow turning once every 0.8 s.
                    for _ in 0..24 {
                        architect::platform::sleep(Duration::from_millis(50)).await;
                        turn.set((turn() + 22) % 360);
                    }
                    busy.set(false);
                    turn.set(0);
                });
            },
            svg { key: "{busy()}", width: "16", height: "16", view_box: "0 0 16 16", style: "transform: rotate({turn()}deg);",
                if busy() {
                    path { d: "M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2.5v3.2h-3.2", fill: "none", stroke: ink, stroke_width: "1.7", stroke_linecap: "round", stroke_linejoin: "round" }
                } else {
                    path { d: "M5.2 1.5h5.6l3.7 3.7v5.6l-3.7 3.7H5.2l-3.7-3.7V5.2Z", fill: "none", stroke: ink, stroke_width: "1.5", stroke_linejoin: "round" }
                    path { d: "M8 4.8v4", stroke: ink, stroke_width: "1.7", stroke_linecap: "round" }
                    circle { cx: "8", cy: "11.2", r: "1", fill: ink }
                }
            }
        }
    }
}

/// Mute: a tap mutes the house (you still hear yourself in the phones);
/// its ⋯ offers muting fully. Lit red while muted; a tap while lit
/// unmutes.
#[component]
fn MuteButton(house: bool, phones: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let full = house && phones;
    let any = house || phones;
    let items = vec![
        Item::head("Mute"),
        Item::run("house", "Mute house")
            .detail("you still hear yourself")
            .checked(house && !full),
        Item::run("full", "Mute fully")
            .detail("your guitar out of the phones too")
            .checked(full),
        Item::Sep,
        Item::run("none", "Unmute").unless((!any).then(|| "Nothing is muted".to_string())),
    ];
    let set = set_mutes;
    let (ink, bg) = if any {
        (DANGER_INK, VOID)
    } else {
        (INK_2, CLEAR)
    };
    // Lit, it breathes: a shade over it rising and falling every 1.6 s.
    let mut lit = use_signal(|| any);
    if lit() != any {
        lit.set(any);
    }
    let mut phase = use_signal(|| 0.0_f64);
    use_hook(move || {
        spawn(async move {
            loop {
                architect::platform::sleep(Duration::from_millis(80)).await;
                // Less motion asked for: lit, and still.
                if lit() && !super::splash::reduce_motion() {
                    phase.set((phase() + 0.08 / 1.6) % 1.0);
                } else if phase() != 0.0 {
                    phase.set(0.0);
                }
            }
        })
    });
    let aria = if full {
        "Fully muted — tap to unmute"
    } else if any {
        "House muted — tap to unmute"
    } else {
        "Mute house — tap to mute the house; ⋯ to mute fully"
    };
    let shade = if any {
        (1.0 - (phase() * std::f64::consts::TAU).cos()) / 2.0 * 0.18
    } else {
        0.0
    };
    rsx! {
        div { style: "align-self: stretch; flex-shrink: 0; display: flex; align-items: stretch;",
            button {
                "aria-label": "{aria}",
                "aria-pressed": "{any}",
                style: "align-self: stretch; min-width: 48px; flex-shrink: 0; padding: 0 12px; display: flex; align-items: center; justify-content: center; border: none; background: {bg}; cursor: pointer; position: relative;",
                // A tap: mute the house, or unmute.
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        if any { set(rig.clone(), false, false) } else { set(rig.clone(), true, phones) }
                    }
                },
                // Right-click: the menu, as its ⋯ opens it.
                oncontextmenu: {
                    let rig = rig.clone();
                    let items = items.clone();
                    signal_widgets::right_click(move |e: MouseEvent| {
                        e.prevent_default();
                        let (c, el) = (e.client_coordinates(), e.element_coordinates());
                        let (left, bottom) = (c.x - el.x, c.y - el.y + f64::from(TOP_H));
                        open_menu(host, left - MENU_W, bottom + 4.0, items.clone(), mute_pick(rig.clone()));
                    })
                },
                if shade > 0.0 {
                    span { style: "position: absolute; left: 0; right: 0; top: 0; bottom: 0; background: rgba(0,0,0,{shade});" }
                }
                svg { key: "{any}", width: "17", height: "17", view_box: "0 0 16 16",
                    path { d: "M2 6h2.5L8 3v10L4.5 10H2Z", fill: ink }
                    if any {
                        path { d: "M10.5 6l4 4M14.5 6l-4 4", stroke: ink, stroke_width: "1.6", stroke_linecap: "round" }
                    } else {
                        path { d: "M10.5 5.5a3.5 3.5 0 0 1 0 5", fill: "none", stroke: ink, stroke_width: "1.4", stroke_linecap: "round" }
                        path { d: "M12.5 3.8a6 6 0 0 1 0 8.4", fill: "none", stroke: ink, stroke_width: "1.4", stroke_linecap: "round", opacity: "0.7" }
                    }
                }
            }
            // More: mute fully, or unmute — no press-and-hold
            // (`signal_widgets::touch`).
            button {
                "aria-label": "Mute options",
                title: "Mute options",
                style: "align-self: stretch; width: 26px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; background: {bg}; padding: 0; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    let items = items.clone();
                    move |e: MouseEvent| {
                        let (c, el) = (e.client_coordinates(), e.element_coordinates());
                        let (right, bottom) = (c.x - el.x + 26.0, c.y - el.y + f64::from(TOP_H));
                        open_menu(host, right - MENU_W, bottom + 4.0, items.clone(), mute_pick(rig.clone()));
                    }
                },
                svg { width: "12", height: "4", view_box: "0 0 12 4", style: "display: block;",
                    circle { cx: "2", cy: "2", r: "1.5", fill: ink }
                    circle { cx: "6", cy: "2", r: "1.5", fill: ink }
                    circle { cx: "10", cy: "2", r: "1.5", fill: ink }
                }
            }
        }
    }
}

fn set_mutes(rig: Option<RigClient>, house: bool, phones: bool) {
    if let Some(r) = rig {
        let _ = dioxus_core::spawn_forever(async move {
            let _ = r.set_mutes(house, phones).await;
        });
    }
}

/// The mute menu's picks.
fn mute_pick(rig: Option<RigClient>) -> EventHandler<Picked> {
    EventHandler::new(move |p: Picked| match p.id.as_str() {
        "house" => set_mutes(rig.clone(), true, false),
        "full" => set_mutes(rig.clone(), true, true),
        "none" => set_mutes(rig.clone(), false, false),
        _ => {}
    })
}

/// The MIDI light: green while the controller chosen in Setup is plugged in
/// (or with none chosen), red when the chosen one is missing.
fn use_midi_health() -> &'static str {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut tone = use_signal(|| LIVE);
    use_hook(move || {
        spawn(async move {
            let Some(r) = rig else { return };
            loop {
                let ports = r.midi_ports().await.unwrap_or_default();
                let wanted = r.setup().await.ok().and_then(|s| {
                    s.controllers
                        .get(s.controller_index as usize)
                        .map(|c| c.device.clone())
                });
                let next = match wanted.filter(|d| !d.is_empty()) {
                    Some(d)
                        if ports
                            .iter()
                            .any(|p| p.eq_ignore_ascii_case(&d) || p.contains(&d)) =>
                    {
                        LIVE
                    }
                    Some(_) => VOID,
                    None => LIVE,
                };
                if tone() != next {
                    tone.set(next);
                }
                architect::platform::sleep(Duration::from_secs(3)).await;
            }
        })
    });
    tone()
}

/// Audio is not running: tap to start it — or, once it has failed, restart
/// it. Says it is working until the rig answers.
#[component]
fn AudioBadge(failed: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut starting = use_signal(|| false);
    let label = if starting() {
        "Starting audio…"
    } else if failed {
        "Restart audio"
    } else {
        "Start audio"
    };
    let (ink, bg, edge) = if failed && !starting() {
        (DANGER_INK, VOID, VOID)
    } else {
        (AMBER, CLEAR, AMBER)
    };
    rsx! {
        div { style: "align-self: stretch; display: flex; align-items: center; padding: 0 10px; flex-shrink: 0;",
            button {
                disabled: starting(),
                style: "height: {HIT}px; padding: 0 12px; display: flex; align-items: center; gap: 8px; border-radius: 999px; border: 1.5px solid {edge}; background: {bg}; color: {ink}; font-family: {FONT}; font-size: 13px; font-weight: 750; white-space: nowrap; cursor: pointer; box-sizing: border-box;",
                onclick: move |_| {
                    starting.set(true);
                    if let Some(r) = rig.clone() {
                        let _ = dioxus_core::spawn_forever(async move {
                            let _ = if failed { r.restart().await } else { r.start().await };
                        });
                    }
                    // The rig answers through `running`; the badge goes with it.
                    spawn(async move {
                        architect::platform::sleep(Duration::from_secs(6)).await;
                        starting.set(false);
                    });
                },
                svg { key: "{ink}", width: "14", height: "14", view_box: "0 0 16 16",
                    path { d: "M1.5 8h2l1.5-4 2.5 8 2-6 1.5 4 1-2h2.5", fill: "none", stroke: ink, stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                }
                "{label}"
            }
        }
    }
}

const AMBER: &str = "#fbbf24";

/// A link that is up but struggling (the prototype's "warn").
const WARN: &str = "#eab308";
