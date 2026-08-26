//! **The algorithm picker** — the guitar rig's machine selector, in the keys
//! band.
//!
//! A delay or a reverb is not one effect with a knob for character: it is a
//! family of machines, and which one you are on changes what every other
//! control means. So the algorithm is not a knob — it reads as the section's
//! title, and clicking it opens the whole grid at once. Same shape as
//! `signal-guitar-ui`'s `AlgoPicker`, same tables, so a player moving between
//! the two rigs is choosing from the same list by the same gesture.

use dioxus::prelude::*;

/// `delay::DelayStyle` order — the TimeLine MX machines.
pub const DELAY_ALGOS: [&str; 13] = [
    "Tape", "Digital", "dBucket", "Lo-Fi", "Shimmer", "Reverse", "Ice", "Rhythm", "Drum",
    "Oil Can", "MultiTap", "Spectral", "Filter",
];

/// `reverb::AlgorithmType::ALL` order.
pub const VERB_ALGOS: [&str; 15] = [
    "Room",
    "Hall",
    "Plate",
    "Spring",
    "Cloud",
    "Bloom",
    "Shimmer",
    "Chorale",
    "Magneto",
    "NonLinear",
    "Swell",
    "Reflections",
    "Velvet",
    "FreeVerb",
    "Convolution",
];

/// The current algorithm, as a title that opens a grid.
#[component]
pub fn AlgoPicker(
    /// Macro id this writes (`"dly.algo"`, `"amb.algo"`), already scoped by
    /// the caller's level.
    id: String,
    value: f32,
    options: Vec<&'static str>,
    accent: String,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let mut open = use_signal(|| false);
    let index = (value.max(0.0) as usize).min(options.len().saturating_sub(1));
    let current = options.get(index).copied().unwrap_or("—");

    rsx! {
        button {
            style: format!(
                "appearance: none; display: flex; align-items: center; gap: 5px; cursor: pointer; \
                 border: 1px solid #26262b; border-radius: 6px; padding: 2px 7px; \
                 background: #101216; color: {accent}; font-size: 10px; font-weight: 700; \
                 letter-spacing: 0.04em;",
            ),
            title: "Choose a machine",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                open.set(true);
            },
            "{current}"
            span { style: "font-size: 7px; color: #52525b;", "▼" }
        }
        if open() {
            // The whole family at once: which machine you are on changes what
            // every other control means, so it is worth the full screen.
            div {
                style: "position: fixed; inset: 0; z-index: 400; display: flex; \
                        align-items: center; justify-content: center; background: #000000cc;",
                onclick: move |_| open.set(false),
                div {
                    style: "display: grid; grid-template-columns: repeat(4, minmax(96px, 1fr)); \
                            gap: 6px; padding: 14px; border: 1px solid #2b2b31; \
                            border-radius: 14px; background: #0c0c0f; box-shadow: 0 24px 64px #000d;",
                    for (i, name) in options.iter().enumerate() {
                        {
                            let id = id.clone();
                            let accent = accent.clone();
                            let here = i == index;
                            rsx! {
                                button {
                                    key: "{i}",
                                    style: format!(
                                        "appearance: none; border-radius: 8px; cursor: pointer; \
                                         padding: 9px 12px; font-size: 11px; font-weight: 700; \
                                         border: 1px solid {}; background: {}; color: {};",
                                        if here { accent.clone() } else { "#26262b".to_string() },
                                        if here { accent.clone() } else { "#131316".to_string() },
                                        if here { "#08080a".to_string() } else { "#a1a1aa".to_string() },
                                    ),
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        on_change.call((id.clone(), i as f32));
                                        open.set(false);
                                    },
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
