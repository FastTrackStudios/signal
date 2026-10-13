//! MIDI input — the "is anything reaching the rig?" answer.
//!
//! The activity light is a status readout in the app bar (dark = nothing
//! arriving, blue = events landing, green = notes held); the monitor and port
//! picker are a right-rail panel. Before the chrome, this was a bar widget
//! with a popover hanging off it.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_keys_proto::keys::KeysRigClient;

use crate::state::held_notes;

/// The activity light's colour, flashing on every event that lands and going
/// solid while notes are held. A hook: the flash is a timer.
pub fn use_midi_glow(midi: &[MidiEvent]) -> (bool, &'static str) {
    let mut seen = use_signal(|| 0usize);
    let mut flash = use_signal(|| false);
    let count = midi.len();
    use_effect(move || {
        if count != seen() {
            seen.set(count);
            flash.set(true);
            spawn(async move {
                architect::platform::sleep(std::time::Duration::from_millis(160)).await;
                flash.set(false);
            });
        }
    });
    let solid = !held_notes(midi).is_empty();
    if solid {
        (true, "#4ade80")
    } else if flash() {
        (true, "#38bdf8")
    } else {
        (false, "#3f3f46")
    }
}

/// The MIDI panel: what the rig is listening to, and what is arriving.
#[component]
pub fn MidiPanel(
    /// Recent events (oldest first) — the monitor's contents.
    midi: Vec<MidiEvent>,
    /// The port the rig is attached to (`None` = omni / all inputs).
    port: Option<String>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let label = port.clone().unwrap_or_else(|| "all inputs".into());

    let ports = use_resource({
        let rig = rig.clone();
        move || {
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.midi_ports().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 12px; padding: 12px;",
            {
                rsx! {
                    div { style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                        span { style: "font-size: 11px; font-weight: 700; color: #e4e4e7;", "MIDI input" }
                        div { style: "flex: 1;" }
                        span { style: "font-size: 10px; color: #38bdf8;", "{label}" }
                    }
                    // Port picker — "all inputs" plus each hardware port.
                    div { style: "display: flex; flex-direction: column; gap: 2px;",
                        {
                            let rig = rig.clone();
                            rsx! {
                                button {
                                    style: port_style(port.is_none()),
                                    onclick: move |_| {
                                        let rig = rig.clone();
                                        spawn(async move {
                                            if let Some(r) = rig {
                                                let _ = r.set_midi_port(String::new()).await;
                                            }
                                        });
                                    },
                                    "All inputs (omni)"
                                }
                            }
                        }
                        for name in ports.read().clone().unwrap_or_default() {
                            {
                                let rig = rig.clone();
                                let selected = port.as_deref() == Some(name.as_str());
                                let pick = name.clone();
                                rsx! {
                                    button {
                                        key: "{name}",
                                        style: port_style(selected),
                                        onclick: move |_| {
                                            let (rig, pick) = (rig.clone(), pick.clone());
                                            spawn(async move {
                                                if let Some(r) = rig {
                                                    let _ = r.set_midi_port(pick).await;
                                                }
                                            });
                                        },
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                    if midi.is_empty() {
                        span { style: "font-size: 10px; color: #52525b; padding: 4px 2px;",
                            "No events yet — play a key, or pick the port your keyboard is on."
                        }
                    }
                    MidiMonitorPanel {
                        events: midi.clone(),
                        count: midi.len() as u64,
                        title: "Events".to_string(),
                    }
                }
            }
        }
    }
}

fn port_style(selected: bool) -> String {
    format!(
        "appearance: none; text-align: left; border: none; border-radius: 6px; padding: 5px 8px; \
         background: {}; color: {}; font-size: 11px;",
        if selected { "#101821" } else { "transparent" },
        if selected { "#7dd3fc" } else { "#a1a1aa" },
    )
}
