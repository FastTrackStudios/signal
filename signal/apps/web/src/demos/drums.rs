//! Drum rig mock: a pad grid firing a pattern, with the mic faders
//! moving under it.
//!
//! The pads are on a 2-second loop with per-pad delays that spell out a
//! backbeat — kick on 1 and 3, snare on 2 and 4, hats through it. The
//! round-robin readout ticks alongside, because "the same hit twice does
//! not sound the same twice" is the thing worth showing about a drum rig
//! and a still picture cannot show it.

use dioxus::prelude::*;

/// (pad label, delay, whether it is the accent colour).
const PADS: &[(&str, &str, bool)] = &[
    ("KICK", "0s", true),
    ("SNARE", "0.5s", true),
    ("HAT", "0.25s", false),
    ("TOM 1", "1.25s", false),
    ("TOM 2", "1.5s", false),
    ("RIDE", "0.75s", false),
    ("CRASH", "0s", false),
    ("PERC", "1.75s", false),
];

/// Mic faders. Overheads and room sit lower than the close mics, which is
/// roughly where a kit actually balances.
const MICS: &[(&str, u8, &str)] = &[
    ("IN", 82, "0s"),
    ("OUT", 70, "0.12s"),
    ("SN T", 76, "0.24s"),
    ("SN B", 52, "0.36s"),
    ("OH", 64, "0.48s"),
    ("ROOM", 40, "0.6s"),
];

#[component]
pub fn DrumsDemo() -> Element {
    rsx! {
        div { class: "sg-demo sg-demo-drums", aria_hidden: "true",
            div { class: "sg-demo-bar",
                span { class: "sg-demo-title", "Drums" }
                span { class: "sg-demo-chip", "6 mics · RR 8" }
            }

            div { class: "sg-pads",
                for (name, delay, accent) in PADS.iter().copied() {
                    div {
                        key: "{name}",
                        class: if accent { "sg-pad sg-pad-accent" } else { "sg-pad" },
                        style: "animation-delay: {delay}",
                        span { class: "sg-pad-name", "{name}" }
                    }
                }
            }

            div { class: "sg-mics",
                for (name, height, delay) in MICS.iter().copied() {
                    div { key: "{name}", class: "sg-mic",
                        div { class: "sg-mic-track",
                            div {
                                class: "sg-mic-fill",
                                style: "height: {height}%; animation-delay: {delay}",
                            }
                        }
                        span { class: "sg-mic-label", "{name}" }
                    }
                }
            }
        }
    }
}
