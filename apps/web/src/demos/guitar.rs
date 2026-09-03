//! Guitar rig mock: a pedalboard into an amp, with the signal moving
//! through it.
//!
//! The animation is one idea — a pulse travelling left to right along the
//! chain — expressed twice: the pedals light in sequence, and the amp's
//! meter answers a beat later. That lag is the point of the picture, and
//! it is done with `animation-delay` rather than anything that has to be
//! kept in sync.

use dioxus::prelude::*;

/// Pedals on the board, in signal order. The delay staggers each one's
/// light so the pulse reads as travelling rather than blinking together.
const PEDALS: &[(&str, &str)] = &[
    ("COMP", "0s"),
    ("DRIVE", "0.18s"),
    ("FUZZ", "0.36s"),
    ("MOD", "0.54s"),
    ("DLY", "0.72s"),
];

#[component]
pub fn GuitarDemo() -> Element {
    rsx! {
        div { class: "sg-demo sg-demo-guitar", aria_hidden: "true",
            div { class: "sg-demo-bar",
                span { class: "sg-demo-title", "Guitar" }
                span { class: "sg-demo-chip", "NAM · 48 kHz · 64" }
            }

            div { class: "sg-board",
                for (name, delay) in PEDALS.iter().copied() {
                    div { key: "{name}", class: "sg-pedal",
                        div { class: "sg-pedal-led", style: "animation-delay: {delay}" }
                        span { class: "sg-pedal-name", "{name}" }
                        div { class: "sg-pedal-knobs",
                            span { class: "sg-knob" }
                            span { class: "sg-knob sg-knob-b" }
                        }
                    }
                }
            }

            div { class: "sg-amp",
                div { class: "sg-amp-grille" }
                div { class: "sg-amp-meter",
                    // Six bars, each a little later than the last, so the
                    // meter swells rather than jumping as one block.
                    for i in 0..6 {
                        span {
                            key: "{i}",
                            class: "sg-meter-bar",
                            style: "animation-delay: {f64::from(i).mul_add(0.06, 0.9)}s",
                        }
                    }
                }
                span { class: "sg-amp-label", "1959 · IR: 4x12 V30" }
            }
        }
    }
}
