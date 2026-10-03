//! Audio on a phone: a rail mode (beside the rail, like Control and
//! Switch), its three tabs made for a thumb.
//!
//! - **Status** — is it playing (and if not, why), the levels in and out,
//!   and the button that starts or restarts it;
//! - **Interface** — which interface and which of its inputs the guitar is
//!   in, where it plays, how short the buffer is;
//! - **DI player** — a recorded guitar looped through the rig, to dial tones
//!   in with nothing plugged in.
//!
//! Every choice applies at once (saved, and the device reopened): there is
//! no form to fill and then save.
//!
//! Choices are big rows; a row with more than a couple of options opens a
//! full-screen list over the page rather than a dropdown a finger misses.
//! The desktop's routing and headphone-mixer cards are not here — a phone
//! interface is two channels each way, and the mixer is its own process,
//! which iOS does not run.

use dioxus::prelude::*;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{AudioDevices, AudioPrefs, RigStatus};

use crate::state::RigViewState;

const BG: &str = "#0f1012";
const CARD: &str = "#17181b";
const RAISED: &str = "#26292f";
const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
const GREEN: &str = "#22c55e";
const AMBER: &str = "#f59e0b";
const RED: &str = "#ef4444";
/// A row a finger hits without aiming.
const ROW_H: u32 = 56;

/// The buffer sizes offered: iOS's audio unit takes 256 frames and up.
const BUFFERS: &[u32] = if cfg!(target_os = "ios") { &[256, 512, 1024] } else { &[64, 128, 256, 512] };

/// The page's tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Status,
    Interface,
    Di,
}

/// Which list is open over the page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum List {
    Input,
    Output,
}

/// The phone's Audio page, in the view beside the rail.
#[component]
pub fn PhoneAudio(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);
    let mut prefs = use_signal(AudioPrefs::default);
    let mut devices = use_signal(AudioDevices::default);
    let mut status = use_signal(RigStatus::default);
    let mut list = use_signal(|| None::<List>);
    // `FTS_PHONE_AUDIO_TAB=interface|di`: open on that tab, for the shot tool.
    let mut tab = use_signal(|| {
        #[cfg(not(target_arch = "wasm32"))]
        match std::env::var("FTS_PHONE_AUDIO_TAB").as_deref() {
            Ok("interface") => return Tab::Interface,
            Ok("di") => return Tab::Di,
            _ => {}
        }
        Tab::Status
    });

    // The prefs once, then the devices and the rig's status on a short
    // timer while the page is up: an interface plugged in shows up, and the
    // status line follows a restart through.
    use_future({
        let settings = settings.clone();
        let rig = rig.clone();
        move || {
            let settings = settings.clone();
            let rig = rig.clone();
            async move {
                if let Some(s) = settings.as_ref()
                    && let Ok(p) = s.prefs().await
                {
                    prefs.set(p);
                }
                loop {
                    if let Some(s) = settings.as_ref()
                        && let Ok(d) = s.devices().await
                        && *devices.peek() != d
                    {
                        devices.set(d);
                    }
                    if let Some(r) = rig.as_ref()
                        && let Ok(st) = r.status().await
                    {
                        status.set(st);
                    }
                    architect::platform::sleep(std::time::Duration::from_millis(700)).await;
                }
            }
        }
    });

    // Apply a change at once: save it, and reopen the device on it.
    let apply = {
        let settings = settings.clone();
        let rig = rig.clone();
        move |p: AudioPrefs| {
            prefs.set(p.clone());
            let settings = settings.clone();
            let rig = rig.clone();
            spawn(async move {
                if let Some(s) = settings {
                    let _ = s.save_prefs(p).await;
                }
                if let Some(r) = rig {
                    let _ = r.restart().await;
                }
            });
        }
    };
    let call = {
        let rig = rig.clone();
        move |what: &'static str| {
            let rig = rig.clone();
            spawn(async move {
                let Some(r) = rig else { return };
                let _ = match what {
                    "start" => r.start().await,
                    "restart" => r.restart().await,
                    _ => r.stop().await,
                };
            });
        }
    };
    // The DI player: a clip on (it plays, in place of the guitar) or off.
    let di = {
        let rig = rig.clone();
        move |clip: u32, on: bool| {
            let rig = rig.clone();
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.play_di(clip, on).await;
                }
            });
        }
    };

    let p = prefs();
    let d = devices();
    let st = status();
    let running = (state.running)();
    let error = (state.audio_error)();
    // "Audio stopped" is a state, not a fault.
    let fault = !error.is_empty() && error != "Audio stopped";
    let (dot, headline) = if running {
        (GREEN, if st.di_playing { "Playing the DI" } else { "Playing" })
    } else if fault {
        (RED, "Audio error")
    } else {
        (AMBER, "No audio")
    };
    let in_name = if p.input_device.is_empty() { "Automatic".to_string() } else { p.input_device.clone() };
    let out_names: Vec<String> = d.outputs.iter().map(|o| o.name.clone()).collect();
    let out_name = if !p.output_device.is_empty() {
        p.output_device.clone()
    } else if out_names.is_empty() {
        "—".to_string()
    } else {
        out_names.join(", ")
    };
    let detail = if running && st.perf.sample_rate == 0 {
        format!("{in_name} → {out_name}")
    } else if running {
        let rate = st.perf.sample_rate;
        let frames = st.perf.block_frames;
        let ms = if rate > 0 { f64::from(frames) * 1000.0 / f64::from(rate) } else { 0.0 };
        format!("{in_name} → {out_name} · {:.1} kHz · {frames} frames ({ms:.1} ms)", f64::from(rate) / 1000.0)
    } else if fault {
        error.clone()
    } else if d.inputs.is_empty() {
        "Plug in a USB audio interface — or play the DI player".to_string()
    } else {
        "Start it — or play the DI player".to_string()
    };
    // The guitar input's choices: the chosen input's channels.
    let channels = d
        .inputs
        .iter()
        .find(|i| !p.input_device.is_empty() && i.name.contains(&p.input_device))
        .or_else(|| d.inputs.first())
        .map_or(2, |i| i.channels.max(1));
    // Outputs are the system's to choose on iOS (it routes to the
    // interface itself); elsewhere, a list.
    let pick_output = !cfg!(target_os = "ios") && d.outputs.len() > 1;
    let rate = if st.perf.sample_rate > 0 { st.perf.sample_rate } else { 48_000 };

    let row = format!("min-height: {ROW_H}px; display: flex; flex-direction: row; align-items: center; gap: 12px; padding: 0 16px; border-top: 1px solid {RULE};");
    let chip = |on: bool| {
        if on {
            "background: #f4f4f5; color: #0a0b0d; border: 1px solid #f4f4f5;".to_string()
        } else {
            format!("background: {RAISED}; color: {TEXT}; border: 1px solid {RULE};")
        }
    };
    let button = |tone: &str| {
        format!("flex: 1 1 0%; height: 48px; display: flex; align-items: center; justify-content: center; border-radius: 10px; font-size: 15px; font-weight: 800; cursor: pointer; background: {tone}; color: #0a0b0d;")
    };
    let (go, light, quiet, halt) = (button(GREEN), button("#f4f4f5"), format!("{} color: {TEXT};", button("#3f3f46")), button(RED));
    let meter = |level: f64, color: &str| {
        let pct = (level.clamp(0.0, 1.0) * 100.0).round();
        rsx! {
            div { style: "flex: 1 1 0%; height: 10px; border-radius: 5px; background: {RAISED}; overflow: hidden;",
                div { style: "width: {pct}%; height: 100%; background: {color};" }
            }
        }
    };

    let card = format!("display: flex; flex-direction: column; border-radius: 14px; background: {CARD}; border: 1px solid {RULE};");
    rsx! {
        div { style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; background: {BG}; color: {TEXT};",
            // ── The tabs ──
            div { style: "flex: 0 0 auto; display: flex; flex-direction: row; gap: 6px; padding: 8px 12px; border-bottom: 1px solid {RULE};",
                for (t, label) in [(Tab::Status, "Status"), (Tab::Interface, "Interface"), (Tab::Di, "DI player")] {
                    {
                        let look = chip(tab() == t);
                        rsx! {
                            div { key: "{label}", style: "flex: 1 1 0%; height: 40px; display: flex; align-items: center; justify-content: center; border-radius: 10px; font-size: 15px; font-weight: 800; cursor: pointer; {look}",
                                onclick: move |_| tab.set(t),
                                "{label}"
                            }
                        }
                    }
                }
            }
            div { style: "flex: 1 1 0%; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 12px; padding: 12px;",
                match tab() {
                    // ── Status ──
                    Tab::Status => rsx! {
                        div { style: "{card} gap: 14px; padding: 16px;",
                            div { style: "display: flex; flex-direction: row; align-items: center; gap: 12px;",
                                span { style: "width: 16px; height: 16px; border-radius: 8px; background: {dot};" }
                                div { style: "display: flex; flex-direction: column; min-width: 0;",
                                    span { style: "font-size: 20px; font-weight: 800;", "{headline}" }
                                    span { style: "font-size: 13px; color: {DIM};", "{detail}" }
                                }
                            }
                            div { style: "display: flex; flex-direction: row; align-items: center; gap: 10px;",
                                span { style: "flex: 0 0 34px; font-size: 11px; font-weight: 800; color: {DIM};", "IN" }
                                {meter((state.in_level)(), GREEN)}
                            }
                            div { style: "display: flex; flex-direction: row; align-items: center; gap: 10px;",
                                span { style: "flex: 0 0 34px; font-size: 11px; font-weight: 800; color: {DIM};", "OUT" }
                                {meter((state.out_level)(), "#60a5fa")}
                            }
                            div { style: "display: flex; flex-direction: row; gap: 10px;",
                                if running {
                                    div { style: "{light}", onclick: { let call = call.clone(); move |_| call("restart") }, "Restart" }
                                    div { style: "{quiet}", onclick: { let call = call.clone(); move |_| call("stop") }, "Stop" }
                                } else {
                                    div { style: "{go}", onclick: { let call = call.clone(); move |_| call("start") }, "Start audio" }
                                }
                            }
                        }
                    },
                    // ── Interface ──
                    Tab::Interface => rsx! {
                        div { style: "{card} overflow: hidden;",
                            div { style: "{row} border-top: none; cursor: pointer;", onclick: move |_| list.set(Some(List::Input)),
                                span { style: "flex: 0 0 120px; font-size: 14px; color: {DIM};", "Input" }
                                span { style: "flex: 1 1 0%; font-size: 15px; font-weight: 700; white-space: nowrap; overflow: hidden;", "{in_name}" }
                                span { style: "font-size: 20px; color: {DIM};", "›" }
                            }
                            div { style: "{row} padding-top: 8px; padding-bottom: 8px;",
                                span { style: "flex: 0 0 120px; font-size: 14px; color: {DIM};", "Guitar input" }
                                div { style: "flex: 1 1 0%; display: flex; flex-direction: row; gap: 8px;",
                                    for ch in 0..u32::from(channels) {
                                        {
                                            let mut apply = apply.clone();
                                            let p = p.clone();
                                            let look = chip(p.input_channel == ch);
                                            rsx! {
                                                div { key: "{ch}", style: "width: 52px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 10px; font-size: 16px; font-weight: 800; cursor: pointer; {look}",
                                                    onclick: move |_| apply(AudioPrefs { input_channel: ch, ..p.clone() }),
                                                    "{ch + 1}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { style: "{row} cursor: pointer;",
                                onclick: move |_| if pick_output { list.set(Some(List::Output)) },
                                span { style: "flex: 0 0 120px; font-size: 14px; color: {DIM};", "Output" }
                                span { style: "flex: 1 1 0%; font-size: 15px; font-weight: 700; white-space: nowrap; overflow: hidden;", "{out_name}" }
                                if pick_output {
                                    span { style: "font-size: 20px; color: {DIM};", "›" }
                                } else {
                                    span { style: "font-size: 11px; color: {DIM};", "follows the interface" }
                                }
                            }
                            div { style: "{row} padding-top: 8px; padding-bottom: 8px;",
                                span { style: "flex: 0 0 120px; font-size: 14px; color: {DIM};", "Buffer" }
                                div { style: "flex: 1 1 0%; display: flex; flex-direction: row; gap: 8px;",
                                    for &frames in BUFFERS {
                                        {
                                            let mut apply = apply.clone();
                                            let p = p.clone();
                                            let look = chip(p.buffer_size == frames);
                                            let ms = f64::from(frames) * 1000.0 / f64::from(rate);
                                            rsx! {
                                                div { key: "{frames}", style: "min-width: 76px; height: 44px; padding: 0 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; border-radius: 10px; cursor: pointer; {look}",
                                                    onclick: move |_| apply(AudioPrefs { buffer_size: frames, ..p.clone() }),
                                                    span { style: "font-size: 15px; font-weight: 800;", "{frames}" }
                                                    span { style: "font-size: 11px; font-weight: 600; opacity: 0.7;", "{ms:.1} ms" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    // ── The DI player ──
                    Tab::Di => rsx! {
                        div { style: "{card} gap: 12px; padding: 16px;",
                            span { style: "font-size: 13px; color: {DIM};",
                                "Your guitar, recorded dry, looped through the rig in place of the instrument — dial tones in with nothing plugged in. With no interface it plays through the phone."
                            }
                            // A button a recording: tap one to play it (another,
                            // to switch to it).
                            div { style: "display: flex; flex-direction: row; gap: 10px;",
                                for (i, name) in signal_guitar_proto::DI_CLIPS.iter().enumerate() {
                                    {
                                        let di = di.clone();
                                        let playing = st.di_playing && st.di_clip == i as u32;
                                        let look = if playing {
                                            format!("background: {GREEN}; color: #0a0b0d; border: 1px solid {GREEN};")
                                        } else {
                                            chip(false)
                                        };
                                        rsx! {
                                            div { key: "{name}", style: "flex: 1 1 0%; min-width: 0; height: 84px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; border-radius: 12px; cursor: pointer; {look}",
                                                onclick: move |_| di(i as u32, true),
                                                span { style: "font-size: 22px;", if playing { "♪" } else { "▶" } }
                                                span { style: "font-size: 15px; font-weight: 800;", "{name}" }
                                            }
                                        }
                                    }
                                }
                            }
                            if st.di_playing {
                                div { style: "{halt}", onclick: { let di = di.clone(); let clip = st.di_clip; move |_| di(clip, false) }, "■  Stop — back to the guitar" }
                            }
                        }
                    },
                }
            }
            // ── A list over the page ──
            if let Some(which) = list() {
                {
                    let (title, items, current): (&str, Vec<(String, String)>, String) = match which {
                        List::Input => (
                            "Input",
                            std::iter::once(("".to_string(), "Automatic — the connected interface".to_string()))
                                .chain(d.inputs.iter().map(|i| (i.name.clone(), format!("{} · {} in", i.name, i.channels))))
                                .collect(),
                            p.input_device.clone(),
                        ),
                        List::Output => (
                            "Output",
                            std::iter::once(("".to_string(), "Automatic".to_string()))
                                .chain(d.outputs.iter().map(|o| (o.name.clone(), format!("{} · {} out", o.name, o.channels))))
                                .collect(),
                            p.output_device.clone(),
                        ),
                    };
                    rsx! {
                        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 5; display: flex; flex-direction: column; background: {BG};",
                            div { style: "flex: 0 0 52px; display: flex; flex-direction: row; align-items: center; padding: 0 8px; border-bottom: 1px solid {RULE};",
                                span { style: "flex: 1 1 0%; font-size: 20px; font-weight: 800; padding-left: 8px;", "{title}" }
                                div { style: "padding: 10px 18px; border-radius: 10px; font-size: 15px; font-weight: 700; cursor: pointer; background: {RAISED}; border: 1px solid {RULE};",
                                    onclick: move |_| list.set(None),
                                    "Done"
                                }
                            }
                            div { style: "flex: 1 1 0%; min-height: 0; overflow: auto; display: flex; flex-direction: column; padding: 8px 12px;",
                                if items.len() <= 1 {
                                    div { style: "padding: 24px 8px; font-size: 14px; color: {DIM};",
                                        "Nothing connected. Plug in a USB audio interface — it shows up here."
                                    }
                                }
                                for (name, label) in items {
                                    {
                                        let on = name == current;
                                        let bg = if on { RAISED } else { "transparent" };
                                        let mut apply = apply.clone();
                                        let p = p.clone();
                                        rsx! {
                                            div { key: "{label}",
                                                style: "min-height: {ROW_H}px; display: flex; flex-direction: row; align-items: center; gap: 12px; padding: 0 12px; border-radius: 10px; cursor: pointer; background: {bg};",
                                                onclick: move |_| {
                                                    let next = match which {
                                                        List::Input => AudioPrefs { input_device: name.clone(), input_channel: 0, ..p.clone() },
                                                        List::Output => AudioPrefs { output_device: name.clone(), ..p.clone() },
                                                    };
                                                    apply(next);
                                                    list.set(None);
                                                },
                                                span { style: "flex: 1 1 0%; font-size: 16px; font-weight: 700;", "{label}" }
                                                if on {
                                                    svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none", stroke: GREEN, stroke_width: "3", stroke_linecap: "round", stroke_linejoin: "round",
                                                        path { d: "M5 12.5l4.5 4.5L19 7" }
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
    }
}
