//! **The macro band** — one row of Global Controls for whatever is selected,
//! and the two visual cards at its head.
//!
//! The rig is a tree of the same idea three times over: a module has its
//! controls, a layer has a macro panel over its modules, an engine has one
//! over its layers — and the rig has one over every engine. Each level's knobs
//! are **offsets into the level beneath** — absolute and 1:1 while everything
//! under them agrees, a bipolar offset around the patch's own settings once it
//! doesn't (the backend's `drive_global`). So the band is the same component
//! at every level; only the macros handed to it change.
//!
//! **Filter and Amp are cards, not knob rows**: the shapes every sound in the
//! rig is currently making, drawn on one pair of axes with the controls that
//! move them directly underneath. Curves the selection reaches are drawn at
//! full strength, everything else sits behind them — turn a knob and you watch
//! the whole family move together.

use dioxus::prelude::*;
use signal_keys_proto::KeysMacro;

use crate::graphs::{ModuleCurve, StackedEnvelopes, StackedFilters};
use crate::module_edit::{KnobRow, Panel};

/// Panel order for the *small* groups — everything that is a knob row rather
/// than a shape, with what each says when its knobs have nothing to report.
const GROUPS: &[(&str, &str)] = &[
    ("Vibrato", "pitch pulse"),
    ("Unison", "voices · detune"),
    ("Ambience", "reverb amount · length"),
    ("Tone", "EQ — centred is bypassed"),
    ("Effects", "output"),
];

/// The macros of one group.
fn group(macros: &[KeysMacro], name: &str) -> Vec<KeysMacro> {
    macros.iter().filter(|m| m.group == name).cloned().collect()
}

/// One spread line per group: the first macro with something to say speaks for
/// it ("0.4 – 2.1 kHz").
fn spread(macros: &[KeysMacro], name: &str) -> Option<String> {
    macros
        .iter()
        .find(|m| m.group == name && !m.spread.is_empty())
        .map(|m| m.spread.clone())
}

fn varies(macros: &[KeysMacro], name: &str) -> bool {
    macros.iter().any(|m| m.group == name && m.bipolar)
}

/// The badge a panel wears when its knobs are offsets rather than values.
fn offset_badge(on: bool) -> Element {
    rsx! {
        if on {
            span { style: "font-size: 9px; color: #fbbf24;", "offset" }
        }
    }
}

/// A caption over a graph or a knob row — the small label that says which of
/// the card's two halves you are looking at.
#[component]
fn Caption(text: String) -> Element {
    rsx! {
        span {
            style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; \
                    text-transform: uppercase; color: #71717a;",
            "{text}"
        }
    }
}

/// **The Filter card** — one block, not two: the response and the filter
/// envelope of every sound above, the knobs that move them below.
#[component]
pub fn FilterCard(
    /// The scope's macros (any level) — the card picks its own groups out.
    macros: Vec<KeysMacro>,
    /// Every sound being drawn, in its engine's colour.
    curves: Vec<ModuleCurve>,
    accent: String,
    /// Graph height. The mixer's band is short; the layer zoom is tall.
    #[props(default = 140)] height_px: u32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let cutoff = group(&macros, "Filter");
    let env = group(&macros, "Filter Env");
    let lit = cutoff.iter().chain(env.iter()).any(|m| m.live);
    let bipolar = varies(&macros, "Filter") || varies(&macros, "Filter Env");
    let (spread_f, spread_e) = (spread(&macros, "Filter"), spread(&macros, "Filter Env"));

    rsx! {
        Panel {
            title: "Filter".to_string(),
            accent: accent.clone(),
            lit,
            trailing: offset_badge(bipolar),
            div { style: "display: flex; flex-direction: column; gap: 12px;",
                // The shapes, side by side: what the filter does to the sound,
                // and how it moves while a note is held.
                div {
                    style: "display: grid; gap: 12px; \
                            grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));",
                    div { style: "display: flex; flex-direction: column; gap: 6px; min-width: 0;",
                        Caption { text: "Response".to_string() }
                        StackedFilters { curves: curves.clone(), height_px, flat: true }
                    }
                    div { style: "display: flex; flex-direction: column; gap: 6px; min-width: 0;",
                        Caption { text: "Envelope".to_string() }
                        StackedEnvelopes { curves: curves.clone(), height_px, amp: false, flat: true }
                    }
                }
                // …and the controls for both, under the shapes they move.
                div {
                    style: "display: flex; gap: 20px; flex-wrap: wrap; padding-top: 8px; \
                            border-top: 1px solid #1c1c21;",
                    if !cutoff.is_empty() {
                        div { style: "display: flex; flex-direction: column; gap: 8px;",
                            KnobRow {
                                macros: cutoff.clone(),
                                accent: accent.clone(),
                                on_change: move |(id, v)| on_change.call((id, v)),
                            }
                            if let Some(s) = spread_f.clone() {
                                span { style: "font-size: 9px; color: #52525b;", "{s}" }
                            }
                        }
                    }
                    if !env.is_empty() {
                        div { style: "display: flex; flex-direction: column; gap: 8px;",
                            KnobRow {
                                macros: env.clone(),
                                accent: accent.clone(),
                                on_change: move |(id, v)| on_change.call((id, v)),
                            }
                            if let Some(s) = spread_e.clone() {
                                span { style: "font-size: 9px; color: #52525b;", "{s}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// **The Amp card** — every sound's amplitude envelope, and the ADSR that
/// moves them.
#[component]
pub fn AmpCard(
    macros: Vec<KeysMacro>,
    curves: Vec<ModuleCurve>,
    accent: String,
    #[props(default = 140)] height_px: u32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let env = group(&macros, "Amp Env");
    let lit = env.iter().any(|m| m.live);
    let spread_a = spread(&macros, "Amp Env");

    rsx! {
        Panel {
            title: "Amp".to_string(),
            accent: accent.clone(),
            lit,
            trailing: offset_badge(varies(&macros, "Amp Env")),
            div { style: "display: flex; flex-direction: column; gap: 12px;",
                div { style: "display: flex; flex-direction: column; gap: 6px;",
                    Caption { text: "Envelope".to_string() }
                    StackedEnvelopes { curves: curves.clone(), height_px, amp: true, flat: true }
                }
                div {
                    style: "display: flex; flex-direction: column; gap: 8px; padding-top: 8px; \
                            border-top: 1px solid #1c1c21;",
                    KnobRow {
                        macros: env.clone(),
                        accent: accent.clone(),
                        on_change: move |(id, v)| on_change.call((id, v)),
                    }
                    if let Some(s) = spread_a.clone() {
                        span { style: "font-size: 9px; color: #52525b;", "{s}" }
                    }
                }
            }
        }
    }
}

/// One scope's Global Controls: the Filter and Amp cards, then the knob-row
/// groups.
///
/// Laid out as a single scrolling row — the mixer above it is read left to
/// right, and so is this. The two cards lead because they are the two things
/// you look at rather than read.
#[component]
pub fn MacroPanel(
    macros: Vec<KeysMacro>,
    /// Every sound in the rig, in its engine's colour, focused where the
    /// selection reaches.
    #[props(default)]
    curves: Vec<ModuleCurve>,
    accent: String,
    /// Graph height inside the two cards.
    #[props(default = 130)] height_px: u32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    rsx! {
        div {
            style: "display: flex; gap: 12px; align-items: stretch; overflow-x: auto; \
                    padding: 2px 0 4px;",
            div { style: "flex: 0 0 auto; width: min(760px, 62vw);",
                FilterCard {
                    macros: macros.clone(),
                    curves: curves.clone(),
                    accent: accent.clone(),
                    height_px,
                    on_change: move |(id, v)| on_change.call((id, v)),
                }
            }
            div { style: "flex: 0 0 auto; width: min(420px, 34vw);",
                AmpCard {
                    macros: macros.clone(),
                    curves: curves.clone(),
                    accent: accent.clone(),
                    height_px,
                    on_change: move |(id, v)| on_change.call((id, v)),
                }
            }
            for (name, hint) in GROUPS.iter() {
                {
                    let items = group(&macros, name);
                    if items.is_empty() {
                        rsx! {}
                    } else {
                        let spread = spread(&macros, name);
                        let bipolar = varies(&macros, name);
                        rsx! {
                            div { key: "{name}", style: "flex: 0 0 auto;",
                                Panel {
                                    title: name.to_string(),
                                    accent: accent.clone(),
                                    lit: items.iter().any(|m| m.live),
                                    trailing: offset_badge(bipolar),
                                    div { style: "display: flex; flex-direction: column; gap: 8px;",
                                        KnobRow {
                                            macros: items.clone(),
                                            accent: accent.clone(),
                                            on_change: move |(id, v)| on_change.call((id, v)),
                                        }
                                        span {
                                            style: "font-size: 9px; color: #52525b; line-height: 1.4; \
                                                    white-space: nowrap;",
                                            if let Some(s) = spread.clone() { "{s}" } else { "{hint}" }
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
