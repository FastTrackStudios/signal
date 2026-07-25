//! **The macro band** — one row of Global Controls for whatever is selected.
//!
//! The rig is a tree of the same idea three times over: a module has its
//! controls, a layer has a macro panel over its modules, an engine has one
//! over its layers. Each level's knobs are **offsets into the level beneath**
//! — absolute and 1:1 while everything under them agrees, a bipolar offset
//! around the patch's own settings once it doesn't (the backend's
//! `drive_global`). So the band is the same component at every level; only
//! the macros handed to it change.
//!
//! The mixer renders it for the selection: click the Keys card and these are
//! the Keys engine's, click a lane and they are the lane's. Anything deeper
//! than a lane belongs to the zoom.

use dioxus::prelude::*;
use signal_keys_proto::KeysMacro;

use crate::module_edit::{KnobRow, Panel};

/// Panel order for a scope's Global Controls, with what each says when its
/// knobs have nothing to report.
const GROUPS: &[(&str, &str)] = &[
    ("Filter", "cutoff · resonance · env amount"),
    ("Amp Env", "attack · decay · sustain · release"),
    ("Filter Env", "the filter's own shape"),
    ("Vibrato", "pitch pulse"),
    ("Unison", "voices · detune"),
    ("Ambience", "reverb amount · length"),
    ("Tone", "EQ — centred is bypassed"),
    ("Effects", "output"),
];

/// One scope's Global Controls, grouped into panels.
///
/// Laid out as a single scrolling row: the mixer above it is read left to
/// right, and so is this. A panel whose macros are all offsets says so, and
/// the spread underneath every group says what the level beneath currently
/// holds — a centred knob is never a mystery.
#[component]
pub fn MacroPanel(
    macros: Vec<KeysMacro>,
    accent: String,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let group = |name: &str| -> Vec<KeysMacro> {
        macros.iter().filter(|m| m.group == name).cloned().collect()
    };
    // One spread line per panel: the first macro with something to say speaks
    // for it.
    let spread = |name: &str| -> Option<String> {
        macros
            .iter()
            .find(|m| m.group == name && !m.spread.is_empty())
            .map(|m| m.spread.clone())
    };
    let varies = |name: &str| macros.iter().any(|m| m.group == name && m.bipolar);

    rsx! {
        div {
            style: "display: flex; gap: 12px; align-items: stretch; overflow-x: auto; \
                    padding: 2px 0 4px;",
            for (name, hint) in GROUPS.iter() {
                {
                    let items = group(name);
                    if items.is_empty() {
                        rsx! {}
                    } else {
                        let spread = spread(name);
                        let bipolar = varies(name);
                        rsx! {
                            div { key: "{name}", style: "flex: 0 0 auto;",
                                Panel {
                                    title: name.to_string(),
                                    accent: accent.clone(),
                                    lit: items.iter().any(|m| m.live),
                                    trailing: rsx! {
                                        if bipolar {
                                            span { style: "font-size: 9px; color: #fbbf24;", "offset" }
                                        }
                                    },
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
