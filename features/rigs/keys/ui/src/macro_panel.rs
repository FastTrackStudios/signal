//! **The macro band** — the Global Controls of whatever is selected, led by
//! the three shapes those controls move.
//!
//! The rig is a tree of the same idea three times over: a module has its
//! controls, a layer has a macro panel over its modules, an engine has one
//! over its layers — and the rig has one over every engine. Each level's knobs
//! are **offsets into the level beneath** — absolute and 1:1 while everything
//! under them agrees, a bipolar offset around the patch's own settings once it
//! doesn't (the backend's `drive_global`). So the band is the same component
//! at every level; only the macros handed to it change.
//!
//! **Filter response, filter envelope and amp envelope are cards, not knob
//! rows**: one card per shape, the shape drawn wide and shallow with the knobs
//! that move it directly underneath. Every sound in the rig is on those axes
//! at once — the ones the selection reaches at full strength, the rest behind
//! them — so turning a knob shows a whole family moving together.

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

/// Which shape a card draws — and, with it, which knobs sit under it.
#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    /// The filter's response across the spectrum.
    FilterResponse,
    /// The filter envelope — how that response moves while a note is held.
    FilterEnv,
    /// The amplitude envelope.
    AmpEnv,
}

impl Shape {
    /// `(card title, macro group)`.
    fn parts(self) -> (&'static str, &'static str) {
        match self {
            Shape::FilterResponse => ("Filter", "Filter"),
            Shape::FilterEnv => ("Filter Envelope", "Filter Env"),
            Shape::AmpEnv => ("Amp Envelope", "Amp Env"),
        }
    }
}

/// **One shape and the knobs that move it.** The graph is the card's subject,
/// so it takes the width and stays shallow; the controls sit under it.
///
/// Every sound in scope is drawn on the same axes — that is the point of the
/// stack: you turn one knob and watch the whole family move together, keeping
/// their spacing.
#[component]
pub fn ShapeCard(
    shape: Shape,
    /// The scope's macros (any level) — the card picks its own group out.
    macros: Vec<KeysMacro>,
    /// Every sound being drawn, in its engine's colour.
    curves: Vec<ModuleCurve>,
    accent: String,
    /// Graph height. Wide and shallow in the mixer's band; taller in the zoom.
    #[props(default = 108)] height_px: u32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let (title, group_name) = shape.parts();
    let items = group(&macros, group_name);
    let spread = spread(&macros, group_name);

    rsx! {
        Panel {
            title: title.to_string(),
            accent: accent.clone(),
            lit: items.iter().any(|m| m.live),
            trailing: offset_badge(varies(&macros, group_name)),
            div { style: "display: flex; flex-direction: column; gap: 10px;",
                match shape {
                    Shape::FilterResponse => rsx! {
                        StackedFilters { curves: curves.clone(), height_px, flat: true }
                    },
                    Shape::FilterEnv => rsx! {
                        StackedEnvelopes { curves: curves.clone(), height_px, amp: false, flat: true }
                    },
                    Shape::AmpEnv => rsx! {
                        StackedEnvelopes { curves: curves.clone(), height_px, amp: true, flat: true }
                    },
                }
                if !items.is_empty() {
                    div {
                        style: "display: flex; flex-direction: column; gap: 6px; \
                                padding-top: 8px; border-top: 1px solid #1c1c21;",
                        KnobRow {
                            macros: items.clone(),
                            accent: accent.clone(),
                            on_change: move |(id, v)| on_change.call((id, v)),
                        }
                        if let Some(s) = spread.clone() {
                            span { style: "font-size: 9px; color: #52525b;", "{s}" }
                        }
                    }
                }
            }
        }
    }
}

/// One scope's Global Controls: the three shapes across the top, the knob-row
/// groups under them.
///
/// The shapes get the width because they are what you look at rather than
/// read; the small groups scroll sideways beneath, in the mixer's own
/// left-to-right language.
#[component]
pub fn MacroPanel(
    macros: Vec<KeysMacro>,
    /// Every sound in the rig, in its engine's colour, focused where the
    /// selection reaches.
    #[props(default)]
    curves: Vec<ModuleCurve>,
    accent: String,
    /// Graph height inside the shape cards.
    #[props(default = 108)] height_px: u32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    const SHAPES: [Shape; 3] = [Shape::FilterResponse, Shape::FilterEnv, Shape::AmpEnv];
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px; min-width: 0;",
            div {
                style: "display: grid; gap: 12px; align-items: start; \
                        grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));",
                for (i, shape) in SHAPES.into_iter().enumerate() {
                    ShapeCard {
                        key: "{i}",
                        shape,
                        macros: macros.clone(),
                        curves: curves.clone(),
                        accent: accent.clone(),
                        height_px,
                        on_change: move |(id, v)| on_change.call((id, v)),
                    }
                }
            }
            div {
                style: "display: flex; gap: 12px; align-items: stretch; overflow-x: auto; \
                        padding-bottom: 4px;",
                for (name, hint) in GROUPS.iter() {
                    {
                        let items = group(&macros, name);
                        if items.is_empty() {
                            rsx! {}
                        } else {
                            let spread = spread(&macros, name);
                            rsx! {
                                div { key: "{name}", style: "flex: 0 0 auto;",
                                    Panel {
                                        title: name.to_string(),
                                        accent: accent.clone(),
                                        lit: items.iter().any(|m| m.live),
                                        trailing: offset_badge(varies(&macros, name)),
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
}
