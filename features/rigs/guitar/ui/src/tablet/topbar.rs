//! The top bar — the prototype's `TopBar`, `ModeButton`, `Safety`,
//! `Settings` indicators, `Cpu` and `Meters`, on the rig.
//!
//!   over the sidebar   its toggle, and the footswitch mode (one button, its
//!                      menu picks — a green line along its foot)
//!   health & safety    Panic (with the health it fixes, far from Mute), the
//!                      MIDI and Audio lights (a press opens Setup), CPU, the
//!                      IN · OUT · PHONES meters, and Mute: a tap mutes the
//!                      house, a hold offers muting fully

use std::time::Duration;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::PerformanceModel;
use signal_widgets::PopupHost;

use super::menu::{open_menu, Item, Picked, MENU_W};
use super::tokens::*;
use crate::state::RigViewState;

const MODES: [(u32, &str, &str); 3] = [(0, "Preset", "presets"), (1, "Profile", "its stacks"), (2, "Setlist", "the set")];

#[component]
pub fn TopBar(model: PerformanceModel, state: RigViewState, sidebar: bool, on_sidebar: EventHandler<()>, on_setup: EventHandler<&'static str>) -> Element {
    let sidebar_ink = if sidebar { INK } else { INK_3 };
    // Panic resetting: the meters go flat until it is done.
    let resetting = use_signal(|| false);
    let midi = use_midi_health();
    let hp = &model.headphone;
    let output = *state.out_level.read();
    // What the phones hear: the guitar at its own level, the band's mix at
    // its, under the phones fader.
    let (mix_l, mix_r) = *state.mix_db.read();
    let mix = crate::meters::meter_level(10f32.powf(mix_l.max(mix_r) / 20.0));
    let phones = (output * f64::from(hp.self_mix)).max(mix * f64::from(hp.mix_level)) * f64::from(hp.volume);
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
        header { style: "height: {TOP_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box; font-family: {FONT};",
            // Over the sidebar: its toggle and the mode the footswitches are in.
            div { style: "width: {SIDEBAR_W}px; flex-shrink: 0; display: flex; align-items: stretch; border-right: 1px solid {RULE}; box-sizing: border-box;",
                button {
                    "aria-label": if sidebar { "Hide the setlist" } else { "Show the setlist" },
                    style: "width: 52px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                    onclick: move |_| on_sidebar.call(()),
                    svg { key: "{sidebar}", width: "20", height: "20", view_box: "0 0 20 20",
                        rect { x: "2.5", y: "3.5", width: "15", height: "13", rx: "2.5", fill: "none", stroke: sidebar_ink, stroke_width: "1.5" }
                        path { d: "M8 4v12", stroke: sidebar_ink, stroke_width: "1.5" }
                    }
                }
                Rule {}
                ModeTabs { mode: model.perform_mode }
            }
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

#[component]
pub fn Rule() -> Element {
    rsx! { span { style: "width: 1px; align-self: center; height: 22px; background: {RULE}; flex-shrink: 0;" } }
}

/// The footswitch mode as three tabs — Preset, Profile, Setlist — what the
/// sidebar shows and the switches play: a tap goes there, the live green
/// under the one in play.
#[component]
fn ModeTabs(mode: u32) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    // 0 Preset, 1 Profile, anything else Setlist — as the sidebar reads it.
    let on = mode.min(2);
    rsx! {
        div { style: "flex: 1; min-width: 0; display: flex; align-items: stretch;",
            for (m, label, _) in MODES {
                {
                    let rig = rig.clone();
                    let here = m == on;
                    rsx! {
                        button {
                            key: "{m}",
                            "aria-label": "Footswitches play the {label}",
                            style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; font-size: 15px; font-weight: {pick(here, 750, 600)}; color: {pick(here, INK, INK_3)}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| {
                                if here { return; }
                                if let Some(r) = rig.clone() {
                                    let _ = dioxus_core::spawn_forever(async move { let _ = r.set_perform_mode(m).await; });
                                }
                            },
                            span { style: "display: flex; align-items: center; gap: 7px;",
                                ModeIcon { mode: m, ink: pick(here, INK, INK_3).to_string() }
                                "{label}"
                            }
                            if here {
                                span { style: "position: absolute; left: 10px; right: 10px; bottom: 0; height: 2px; border-radius: 1px; background: {LIVE};" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A mode's mark: a preset's layers, a profile's stacks of patches, a
/// setlist's lines.
#[component]
fn ModeIcon(mode: u32, ink: String) -> Element {
    let d = match mode {
        0 => "M9 2.5 15.5 6 9 9.5 2.5 6ZM2.5 9 9 12.5 15.5 9M2.5 12 9 15.5 15.5 12",
        1 => "M3 3.5h4.5v4.5H3ZM10.5 3.5H15v4.5h-4.5ZM3 10h4.5v4.5H3ZM10.5 10H15v4.5h-4.5Z",
        _ => "M6 4.5h9M6 9h9M6 13.5h9M3 4.5h.01M3 9h.01M3 13.5h.01",
    };
    rsx! {
        svg { key: "{ink}", width: "18", height: "18", view_box: "0 0 18 18", style: "flex-shrink: 0; display: block;",
            path { d: "{d}", fill: "none", stroke: "{ink}", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
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
    let (ink, bg) = if busy() { ("#1a1205", "#fbbf24") } else { ("#fbbf24", CLEAR) };
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

/// The MIDI or Audio light: its icon coloured by how the link is; a press
/// opens Setup on its tab.
#[component]
fn Indicator(kind: &'static str, tone: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    let label = if kind == "midi" { "MIDI" } else { "Audio" };
    let says = if tone == VOID { "down" } else { "connected" };
    rsx! {
        button {
            "aria-label": "{label} {says} — open {label} in Setup",
            style: "align-self: stretch; min-width: 44px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            svg { key: "{tone}", width: "17", height: "17", view_box: "0 0 16 16",
                if kind == "midi" {
                    circle { cx: "8", cy: "8", r: "6.2", fill: "none", stroke: tone, stroke_width: "1.4" }
                    for (x, y) in [(4.6, 8.0), (5.6, 5.4), (8.0, 4.4), (10.4, 5.4), (11.4, 8.0)] {
                        circle { key: "{x}", cx: "{x}", cy: "{y}", r: "0.9", fill: tone }
                    }
                } else {
                    path { d: "M1.5 8h2l1.5-4 2.5 8 2-6 1.5 4 1-2h2.5", fill: "none", stroke: tone, stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
        }
    }
}

/// CPU: a chip, its load under it — quiet while easy, amber from 60%, red
/// from 85% (where xruns start).
#[component]
fn Cpu(cpu: f64) -> Element {
    let pct = (cpu * 100.0).round().clamp(0.0, 100.0) as i32;
    let tone = if pct >= 85 { VOID } else if pct >= 60 { MODIFIED } else { INK_3 };
    rsx! {
        span { title: "DSP load on the rig: {pct}%", style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px; padding: 0 6px; font-size: 12px; font-weight: 750; line-height: 1; color: {tone};",
            svg { key: "{tone}", width: "14", height: "14", view_box: "0 0 16 16",
                rect { x: "3.5", y: "3.5", width: "9", height: "9", rx: "1.5", fill: "none", stroke: tone, stroke_width: "1.4" }
                rect { x: "6", y: "6", width: "4", height: "4", rx: "0.5", fill: tone }
                path { d: "M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2", stroke: tone, stroke_width: "1.2", stroke_linecap: "round" }
            }
            span { style: "font-variant-numeric: tabular-nums;", "{pct}%" }
        }
    }
}

/// IN, OUT and PHONES as three slim upright bars; a muted output goes red
/// and still.
#[component]
fn Meters(input: f64, output: f64, phones_level: f64, house: bool, phones: bool) -> Element {
    rsx! {
        div { title: "IN · OUT · PHONES", style: "align-self: stretch; flex-shrink: 0; display: flex; gap: 5px; padding: 6px 8px 4px; box-sizing: border-box;",
            MiniMeter { label: "I", level: input, muted: false, phones: false }
            MiniMeter { label: "O", level: output, muted: house, phones: false }
            MiniMeter { label: "", level: phones_level, muted: phones, phones: true }
        }
    }
}

#[component]
fn MiniMeter(label: &'static str, level: f64, muted: bool, phones: bool) -> Element {
    let l = level.clamp(0.0, 1.0);
    // The gradient is the bar's full height, shown up to the level.
    let inner = 100.0 / l.max(0.01);
    let ground = if muted { "color-mix(in oklab, #f87171 55%, #000)" } else { WELL };
    let ink = if muted { VOID } else { INK_3 };
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 2px; min-height: 0; min-width: 10px;",
            span { style: "position: relative; flex: 1; width: 6px; min-height: 0; border-radius: 3px; overflow: hidden; background: {ground};",
                if !muted {
                    span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {pct(l)}%; overflow: hidden;",
                        span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {inner}%; background: linear-gradient(0deg, #15803d 0%, #22c55e 60%, #eab308 82%, #f87171 100%);" }
                    }
                }
            }
            span { style: "height: 12px; display: flex; align-items: center; font-size: 12px; font-weight: 800; line-height: 1; color: {ink};",
                if phones {
                    svg { key: "{muted}", width: "12", height: "11", view_box: "0 0 16 14",
                        path { d: "M2.5 9V7.5a5.5 5.5 0 0 1 11 0V9", fill: "none", stroke: ink, stroke_width: "2" }
                        rect { x: "1.2", y: "8.2", width: "3.6", height: "5", rx: "1.2", fill: ink }
                        rect { x: "11.2", y: "8.2", width: "3.6", height: "5", rx: "1.2", fill: ink }
                    }
                } else {
                    "{label}"
                }
            }
        }
    }
}

/// Mute: a tap mutes the house (you still hear yourself in the phones);
/// held, a menu offers muting fully. Lit red while muted; a tap while lit
/// unmutes.
#[component]
fn MuteButton(house: bool, phones: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let mut held = use_signal(|| false);
    let mut fired = use_signal(|| false);
    // Each press its own: a hold timer from an earlier press never fires.
    let mut press = use_signal(|| 0u32);
    let full = house && phones;
    let any = house || phones;
    let items = vec![
        Item::head("Mute"),
        Item::run("house", "Mute house").detail("you still hear yourself").checked(house && !full),
        Item::run("full", "Mute fully").detail("your guitar out of the phones too").checked(full),
        Item::Sep,
        Item::run("none", "Unmute").unless((!any).then(|| "Nothing is muted".to_string())),
    ];
    let set = set_mutes;
    let (ink, bg) = if any { (DANGER_INK, VOID) } else { (INK_2, CLEAR) };
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
        "Fully muted — tap to unmute, hold for more"
    } else if any {
        "House muted — tap to unmute, hold for more"
    } else {
        "Mute house — tap to mute the house, hold to mute fully"
    };
    let shade = if any { (1.0 - (phase() * std::f64::consts::TAU).cos()) / 2.0 * 0.18 } else { 0.0 };
    rsx! {
        button {
            "aria-label": "{aria}",
            "aria-pressed": "{any}",
            style: "align-self: stretch; min-width: 48px; flex-shrink: 0; padding: 0 12px; display: flex; align-items: center; justify-content: center; border: none; background: {bg}; touch-action: none; cursor: pointer; position: relative;",
            onpointerdown: {
                let rig = rig.clone();
                let items = items.clone();
                move |e: PointerEvent| {
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (left, bottom) = (c.x - el.x, c.y - el.y + f64::from(TOP_H));
                    held.set(true);
                    fired.set(false);
                    let mine = press() + 1;
                    press.set(mine);
                    let rig = rig.clone();
                    let items = items.clone();
                    spawn(async move {
                        architect::platform::sleep(Duration::from_millis(500)).await;
                        if held() && press() == mine {
                            fired.set(true);
                            open_menu(host, left - MENU_W, bottom + 4.0, items, mute_pick(rig));
                        }
                    });
                }
            },
            onpointerup: {
                let rig = rig.clone();
                move |_| {
                    let was = held();
                    held.set(false);
                    if was && !fired() {
                        if any { set(rig.clone(), false, false) } else { set(rig.clone(), true, phones) }
                    }
                }
            },
            onpointerleave: move |_| held.set(false),
            onpointercancel: move |_| held.set(false),
            oncontextmenu: {
                let rig = rig.clone();
                let items = items.clone();
                move |e: MouseEvent| {
                    e.prevent_default();
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (left, bottom) = (c.x - el.x, c.y - el.y + f64::from(TOP_H));
                    held.set(false);
                    open_menu(host, left - MENU_W, bottom + 4.0, items.clone(), mute_pick(rig.clone()));
                }
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
    }
}

fn set_mutes(rig: Option<RigClient>, house: bool, phones: bool) {
    if let Some(r) = rig {
        let _ = dioxus_core::spawn_forever(async move { let _ = r.set_mutes(house, phones).await; });
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
                    s.controllers.get(s.controller_index as usize).map(|c| c.device.clone())
                });
                let next = match wanted.filter(|d| !d.is_empty()) {
                    Some(d) if ports.iter().any(|p| p.eq_ignore_ascii_case(&d) || p.contains(&d)) => LIVE,
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
    let (ink, bg, edge) = if failed && !starting() { (DANGER_INK, VOID, VOID) } else { (AMBER, CLEAR, AMBER) };
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

