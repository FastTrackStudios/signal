//! Keys rig mock: a keybed with notes falling on it, and the layer mixer
//! beside it.
//!
//! Two animations that have to agree: a key lights, and the layer faders
//! answer. They are offset so the mixer reads as responding to the
//! playing rather than running on its own clock.

use dioxus::prelude::*;

/// (label, fader height %, animation delay) per layer. The heights differ
/// so the mixer looks like a balance someone set rather than a row of
/// identical bars.
const LAYERS: &[(&str, u8, &str)] = &[
    ("Rhodes", 78, "0s"),
    ("Pad", 46, "0.4s"),
    ("Strings", 62, "0.8s"),
    ("Bass", 34, "1.2s"),
];

/// Which white keys light, and when. Chosen to read as a played phrase
/// rather than a scale run.
const LIT: &[(usize, &str)] = &[(0, "0s"), (2, "0.5s"), (4, "1.0s"), (7, "1.5s")];

#[component]
pub fn KeysDemo() -> Element {
    // A black key sits after every white key except the 3rd and 7th —
    // the standard octave pattern, so the keybed reads as a keyboard.
    const NO_SHARP: [usize; 2] = [2, 6];

    rsx! {
        div { class: "sg-demo sg-demo-keys", aria_hidden: "true",
            div { class: "sg-demo-bar",
                span { class: "sg-demo-title", "Keys" }
                span { class: "sg-demo-chip", "4 layers · 12 RR" }
            }

            div { class: "sg-keys-body",
                div { class: "sg-mixer",
                    for (name, height, delay) in LAYERS.iter().copied() {
                        div { key: "{name}", class: "sg-mixer-strip",
                            div { class: "sg-fader-track",
                                div {
                                    class: "sg-fader-fill",
                                    style: "height: {height}%; animation-delay: {delay}",
                                }
                            }
                            span { class: "sg-mixer-label", "{name}" }
                        }
                    }
                }

                div { class: "sg-keybed",
                    for i in 0..10_usize {
                        div { key: "w{i}", class: "sg-key-white",
                            style: match LIT.iter().find(|(k, _)| *k == i) {
                                Some((_, delay)) => format!("animation-delay: {delay}"),
                                // No delay set means the key never lights;
                                // the class is shared, the animation is not.
                                None => "animation: none".to_string(),
                            },
                        }
                    }
                    for i in 0..9_usize {
                        if !NO_SHARP.contains(&(i % 7)) {
                            div {
                                key: "b{i}",
                                class: "sg-key-black",
                                // Positioned against the white keys rather
                                // than laid out, because a black key sits
                                // between two of them.
                                style: "left: calc({i + 1} * (100% / 10) - 0.9%)",
                            }
                        }
                    }
                }
            }
        }
    }
}
