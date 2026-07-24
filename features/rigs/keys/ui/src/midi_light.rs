//! MIDI activity light — the "is anything reaching the rig?" answer, always
//! visible in the top bar.
//!
//! Dark = nothing arriving. Flashing = events landing. Solid = notes held.
//! Click it for the full monitor: the port picker (what the rig is listening
//! to) and the live event list.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_keys_proto::keys::KeysRigClient;

use crate::state::held_notes;

/// The light + its popover.
#[component]
pub fn MidiLight(
    /// Recent events (oldest first) — the monitor's contents.
    midi: Vec<MidiEvent>,
    /// The port the rig is attached to (`None` = omni / all inputs).
    port: Option<String>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut open = use_signal(|| false);

    // Flash on any new event: remember how many we'd seen, light briefly
    // whenever that count moves.
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

    let held = held_notes(&midi);
    let solid = !held.is_empty();
    let lit = solid || flash();
    let (dot, glow) = if solid {
        ("#4ade80", "#4ade8099")
    } else if lit {
        ("#38bdf8", "#38bdf899")
    } else {
        ("#3f3f46", "transparent")
    };
    let label = port.clone().unwrap_or_else(|| "all inputs".into());

    // Ports, fetched when the popover opens.
    let ports = use_resource({
        let rig = rig.clone();
        move || {
            let rig = rig.clone();
            async move {
                if !open() {
                    return Vec::new();
                }
                match rig {
                    Some(r) => r.midi_ports().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });

    rsx! {
        div { style: "position: relative;",
            button {
                style: "appearance: none; display: flex; align-items: center; gap: 6px; \
                        border: 1px solid #26262b; border-radius: 999px; padding: 3px 9px 3px 7px; \
                        background: #101013; color: #71717a; font-size: 10px; font-weight: 600;",
                title: "MIDI — click for the monitor",
                onclick: move |_| open.toggle(),
                span {
                    style: format!(
                        "width: 8px; height: 8px; border-radius: 999px; background: {dot}; \
                         box-shadow: 0 0 8px {glow};",
                    ),
                }
                "MIDI"
            }
            if open() {
                div {
                    style: "position: absolute; top: calc(100% + 6px); right: 0; z-index: 60; width: 340px; \
                            display: flex; flex-direction: column; gap: 8px; padding: 10px; \
                            background: #0b0b0d; border: 1px solid #2b2b31; border-radius: 12px; \
                            box-shadow: 0 16px 40px #000d;",
                    div { style: "display: flex; align-items: center; gap: 8px;",
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
