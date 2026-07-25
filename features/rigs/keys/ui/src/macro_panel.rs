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
//! **Every group that can be drawn is a card, not a knob row**: unison,
//! vibrato, the filter's response and envelope, the amp envelope — one card
//! per shape, drawn wide and shallow with the knobs that move it directly
//! underneath. Every sound in the rig is on those axes at once (the ones the
//! selection reaches at full strength, the rest behind them), so turning a
//! knob shows a whole family moving together.
//!
//! Every shape is the same height, and the groups still waiting for one hold
//! that height open — the knob rows across the band sit on one line.

use dioxus::prelude::*;
use signal_keys_proto::KeysMacro;

use crate::graphs::{
    ModuleCurve, StackedEnvelopes, StackedFilters, StackedUnison, StackedVibrato,
};
use crate::knob::Knob;
use crate::module_edit::{KnobRow, Panel};

/// One card in the band.
enum Cell {
    /// A knob group stacked **vertically** in a thin column: two or three
    /// knobs that need no width and no picture, read bottom-up (Low under
    /// Mid under High, the way an EQ is drawn everywhere else). The `bool`
    /// reverses the macro order so the table's Low-first listing comes out
    /// High-first on screen.
    Column(&'static str, &'static str, bool),
    /// A shape and the knobs that move it.
    Shape(Shape),
}

/// **The band, left to right.** Signal order, near enough: what makes the
/// voice (Unison, Vibrato), what shapes it (the filter's envelope, the filter,
/// the amp's envelope), then what it goes out through (Ambience, Tone).
///
/// The shapes sit in the middle so the two envelopes are never far from the
/// filter between them — you read the whole voice across one row.
const ROW: &[Cell] = &[
    Cell::Shape(Shape::Unison),
    Cell::Shape(Shape::Vibrato),
    Cell::Shape(Shape::FilterEnv),
    Cell::Shape(Shape::FilterResponse),
    Cell::Shape(Shape::AmpEnv),
    Cell::Column("Ambience", "reverb amount · length", false),
    Cell::Column("Tone", "EQ — centred is bypassed", true),
];

/// **The band's one card height**, from the graph height inside the shape
/// cards: panel padding and header, the graph, the divider, a knob row and its
/// readout. Every card is pinned to it — a column of three knobs is otherwise
/// taller than a graph card, and one tall card drags the whole row (and the
/// mixer above it) down with it.
fn card_height_px(graph_px: u32) -> u32 {
    graph_px + 160
}

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
    /// The unison stack: one line per voice, spread by detune.
    Unison,
    /// The vibrato: the line the pitch walks.
    Vibrato,
    /// The filter's response across the spectrum.
    FilterResponse,
    /// The filter envelope — how that response moves while a note is held.
    FilterEnv,
    /// The amplitude envelope.
    AmpEnv,
}

impl Shape {
    /// How wide this card is allowed to get. A shape with two knobs under it
    /// has no business taking the room a four-knob envelope needs — capped,
    /// they sit as narrow columns and hand the slack to the shapes that use
    /// it. `0` means no ceiling.
    fn max_width_px(self) -> u32 {
        match self {
            Shape::Unison | Shape::Vibrato => 210,
            _ => 0,
        }
    }

    /// `(card title, macro group)`.
    fn parts(self) -> (&'static str, &'static str) {
        match self {
            Shape::Unison => ("Unison", "Unison"),
            Shape::Vibrato => ("Vibrato", "Vibrato"),
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
            div {
                style: "display: flex; flex-direction: column; gap: 10px; \
                        min-height: 0; overflow: hidden;",
                match shape {
                    Shape::Unison => rsx! {
                        StackedUnison { curves: curves.clone(), height_px, flat: true }
                    },
                    Shape::Vibrato => rsx! {
                        StackedVibrato { curves: curves.clone(), height_px, flat: true }
                    },
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

/// One scope's Global Controls — **one row, in [`ROW`] order**.
///
/// Every card is only as wide as the knobs under it: a shape's graph has no
/// intrinsic width, so it takes whatever its knob row asks for and no more.
/// That is what makes seven cards fit a row instead of three cards eating it.
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
    rsx! {
        div {
            // `flex: 0 1 auto` per card: sized to its own knobs, giving width
            // back only when the window is too narrow for all seven. The
            // height is fixed for every card, so the row never grows because
            // one card has more to say.
            style: format!(
                "display: flex; gap: 12px; align-items: stretch; min-width: 0; height: {}px;",
                card_height_px(height_px),
            ),
            for (i, cell) in ROW.iter().enumerate() {
                {
                    match cell {
                        Cell::Shape(shape) => rsx! {
                            div {
                                key: "{i}",
                                style: format!(
                                    "flex: 0 1 auto; min-width: 0; display: flex;{}",
                                    match shape.max_width_px() {
                                        0 => String::new(),
                                        px => format!(" max-width: {px}px;"),
                                    },
                                ),
                                ShapeCard {
                                    shape: *shape,
                                    macros: macros.clone(),
                                    curves: curves.clone(),
                                    accent: accent.clone(),
                                    height_px,
                                    on_change: move |(id, v)| on_change.call((id, v)),
                                }
                            }
                        },
                        Cell::Column(name, hint, top_first) => {
                            let mut items = group(&macros, name);
                            if *top_first {
                                items.reverse();
                            }
                            if items.is_empty() {
                                rsx! {}
                            } else {
                                rsx! {
                                    // One knob wide. These are set-and-forget
                                    // trims, not shapes — they earn a column,
                                    // not a card's worth of the row.
                                    div {
                                        key: "{i}",
                                        style: "flex: 0 0 auto; max-width: 116px; display: flex;",
                                        Panel {
                                            title: name.to_string(),
                                            accent: accent.clone(),
                                            lit: items.iter().any(|m| m.live),
                                            trailing: offset_badge(varies(&macros, name)),
                                            div {
                                                // `space-between` over the full
                                                // height, with a gap that keeps
                                                // the knobs apart even when the
                                                // card is short: the column ends
                                                // where the shape cards' knob
                                                // rows do instead of bunching at
                                                // the top.
                                                style: "display: flex; flex-direction: column; gap: 6px; \
                                                        align-items: center; justify-content: space-between; \
                                                        flex: 1; min-height: 0; overflow: hidden;",
                                                title: "{hint}",
                                                for m in items.iter() {
                                                    Knob {
                                                        key: "{m.id}",
                                                        label: m.name.clone(),
                                                        value: m.value,
                                                        min: m.min,
                                                        max: m.max,
                                                        unit: m.unit.clone(),
                                                        live: m.live,
                                                        bipolar: m.bipolar,
                                                        accent: accent.clone(),
                                                        on_change: {
                                                            let id = m.id.clone();
                                                            move |v: f32| on_change.call((id.clone(), v))
                                                        },
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
