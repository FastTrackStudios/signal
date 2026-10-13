//! **The controls either side of the keys** — the pedals and wheels a keys
//! player actually has under their hands and feet.
//!
//! A keyboard is not just notes. The sustain pedal is down for most of a
//! ballad, the expression pedal is what an organ swell *is*, and the mod
//! wheel is where a pad's movement comes from. None of that is visible in a
//! note display, so a rig that only draws keys is hiding half of what the
//! player is doing — and all of what a broken pedal is not doing.
//!
//! Laid out as the hardware is: pedals at the far left (feet), then the two
//! wheels (left hand), then the keys. Everything here is a **read-out** of
//! incoming MIDI, not a control: you play these on the keyboard.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;

/// The continuous controllers, folded out of the MIDI monitor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Controllers {
    /// −1 … 1, centre 0.
    pub bend: f32,
    /// CC1, 0 … 1.
    pub modulation: f32,
    /// CC64, 0 … 1 (a switch pedal reads 0 or 1; a continuous one, between).
    pub sustain: f32,
    /// CC11, 0 … 1.
    pub expression: f32,
}

impl Default for Controllers {
    fn default() -> Self {
        // Expression rests open: a rig with no expression pedal attached is
        // at full volume, not silent.
        Self {
            bend: 0.0,
            modulation: 0.0,
            sustain: 0.0,
            expression: 1.0,
        }
    }
}

impl Controllers {
    /// Fold the recent MIDI into the current controller positions. Later
    /// events win, so this is simply "where everything is now".
    pub fn from_midi(events: &[MidiEvent]) -> Self {
        let mut c = Self::default();
        for e in events {
            match e {
                MidiEvent::PitchBend { bend, .. } => {
                    // 14-bit, centre 8192.
                    c.bend = ((bend.get() as f32) - 8192.0) / 8192.0;
                }
                MidiEvent::ControlChange {
                    controller, value, ..
                } => {
                    let v = value.get() as f32 / 127.0;
                    match controller.get() {
                        1 => c.modulation = v,
                        11 => c.expression = v,
                        64 => c.sustain = v,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        c
    }
}

/// The pedals and wheels, in hardware order.
#[component]
pub fn ControllerStrip(controllers: Controllers, #[props(default = 96)] height_px: u32) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: flex-end; gap: 14px; flex-shrink: 0; \
                    padding-right: 4px;",
            Pedal {
                label: "SUS".to_string(),
                value: controllers.sustain,
                color: "#4ade80".to_string(),
                height_px,
                // A sustain pedal is usually a switch: half-way is down.
                latched: controllers.sustain >= 0.5,
            }
            Pedal {
                label: "EXP".to_string(),
                value: controllers.expression,
                color: "#fbbf24".to_string(),
                height_px,
                latched: false,
            }
            Wheel {
                label: "BEND".to_string(),
                // Bend rests at centre and swings both ways.
                position: (controllers.bend + 1.0) / 2.0,
                centred: true,
                color: "#38bdf8".to_string(),
                height_px,
            }
            Wheel {
                label: "MOD".to_string(),
                position: controllers.modulation,
                centred: false,
                color: "#a78bfa".to_string(),
                height_px,
            }
        }
    }
}

/// A wheel, drawn as the side-on cylinder it is: a track with a ribbed cap
/// that rides up it.
#[component]
fn Wheel(label: String, position: f32, centred: bool, color: String, height_px: u32) -> Element {
    let p = position.clamp(0.0, 1.0);
    let h = height_px as f32;
    // The cap's centre, measured from the bottom.
    let cap = (p * (h - 14.0)) + 7.0;
    let fill_from = if centred { 0.5f32.min(p) } else { 0.0 };
    let fill_to = if centred { 0.5f32.max(p) } else { p };
    let fill_bottom = fill_from * (h - 14.0) + 7.0;
    let fill_height = (fill_to - fill_from) * (h - 14.0);

    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 4px;",
            div {
                style: "position: relative; width: 20px; height: {height_px}px; border-radius: 10px; \
                        background: #0d0d10; border: 1px solid #26262b; overflow: hidden;",
                // Where it rests, so a stuck wheel is obvious.
                div {
                    style: format!(
                        "position: absolute; left: 2px; right: 2px; bottom: {:.1}px; height: 1px; \
                         background: #3f3f46;",
                        if centred { (h - 14.0) / 2.0 + 7.0 } else { 7.0 },
                    ),
                }
                div {
                    style: format!(
                        "position: absolute; left: 3px; right: 3px; bottom: {fill_bottom:.1}px; \
                         height: {fill_height:.1}px; background: {color}; opacity: 0.45;",
                    ),
                }
                // The cap.
                div {
                    style: format!(
                        "position: absolute; left: -1px; right: -1px; bottom: {:.1}px; height: 12px; \
                         border-radius: 3px; border: 1px solid {color}; \
                         background: linear-gradient(180deg, #3a3a42, #17171b);",
                        cap - 6.0,
                    ),
                }
            }
            span { style: "font-size: 8px; font-weight: 700; letter-spacing: 0.06em; color: #52525b;",
                "{label}"
            }
        }
    }
}

/// A pedal, drawn as a plate that tilts: the fill is how far it is down.
#[component]
fn Pedal(label: String, value: f32, color: String, height_px: u32, latched: bool) -> Element {
    let v = value.clamp(0.0, 1.0);
    let pct = (v * 100.0) as u32;

    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 4px;",
            div {
                style: format!(
                    "position: relative; width: 26px; height: {height_px}px; border-radius: 5px; \
                     background: #0d0d10; border: 1px solid {}; overflow: hidden;",
                    if latched { color.clone() } else { "#26262b".to_string() },
                ),
                div {
                    style: format!(
                        "position: absolute; left: 0; right: 0; bottom: 0; height: {pct}%; \
                         background: {color}; opacity: {};",
                        if latched { "0.55" } else { "0.35" },
                    ),
                }
                // Tread lines, so a pedal at rest still reads as a pedal.
                for i in 1..4 {
                    div {
                        key: "{i}",
                        style: format!(
                            "position: absolute; left: 4px; right: 4px; top: {}%; height: 1px; \
                             background: #ffffff; opacity: 0.05;",
                            i * 22,
                        ),
                    }
                }
            }
            span {
                style: format!(
                    "font-size: 8px; font-weight: 700; letter-spacing: 0.06em; color: {};",
                    if latched { color } else { "#52525b".to_string() },
                ),
                "{label}"
            }
        }
    }
}
