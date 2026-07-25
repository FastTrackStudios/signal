//! Zoom levels — the Control surface is one view at three depths.
//!
//! ```text
//! Mixer            every engine, every lane          (top-down)
//!   └ Engine       one engine, its lanes bigger      (double-click a card / ⤢)
//!       └ Layer    ONE lane, the whole Signal Engine (the play surface)
//! ```
//!
//! The layer zoom is where a patch actually lives: because every lane is the
//! same engine program (`signal_synth::signal_layer`), this one surface
//! controls a Keyscape piano, an Omnisphere soundsource and a wavetable
//! identically.

use dioxus::prelude::*;

/// Which depth the Control view is at.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Zoom {
    /// The whole mixer.
    #[default]
    Mixer,
    /// One engine, by name.
    Engine(String),
    /// One layer, by name (the play surface).
    Layer(String),
}

/// Read the zoom signal from context (provided by `KeysRigRemote`). Both
/// hooks run unconditionally — a hook behind an `unwrap_or_else` would shift
/// hook order between renders and panic.
pub fn use_zoom() -> Signal<Zoom> {
    let local = use_signal(Zoom::default);
    let shared = use_hook(|| try_consume_context::<Signal<Zoom>>());
    shared.unwrap_or(local)
}

/// The small ⤢ affordance that opens a deeper zoom. Deliberately quiet — it
/// sits in a card corner and only brightens on hover.
#[component]
pub fn OpenButton(
    /// Tooltip ("Open Keys A").
    title: String,
    #[props(default = 13)] size: u32,
    on_open: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            style: "appearance: none; border: none; background: transparent; color: #3f3f46; \
                    padding: 2px; line-height: 0; cursor: pointer;",
            title: "{title}",
            onclick: move |e| {
                e.stop_propagation();
                on_open.call(());
            },
            svg {
                width: "{size}", height: "{size}", view_box: "0 0 24 24", fill: "none",
                stroke: "currentColor", stroke_width: "2",
                stroke_linecap: "round", stroke_linejoin: "round",
                // Expand arrows — the standard "open fullscreen" glyph.
                path { d: "M9 3H3v6" }
                path { d: "M3 3l7 7" }
                path { d: "M15 21h6v-6" }
                path { d: "M21 21l-7-7" }
            }
        }
    }
}
