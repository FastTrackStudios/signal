//! Keys-rig Dioxus components — the remote GUI half of the detachable keys
//! rig. Renders purely from `signal-keys-proto` via the generated vox clients
//! (provided in Dioxus context by the host). Inline styles only (Blitz-safe).
//!
//! The rig draws no bar: it publishes its readouts and panels into the app
//! chrome (`fts_chrome`) and renders one surface — the mixer at three zooms
//! (mixer → engine → layer). Routing, the MIDI monitor and the on-screen
//! keyboard are right-rail panels, so they open beside what you are playing
//! instead of replacing it. The **Perform strip** (the profile's footswitch
//! stacks) stays pinned to the bottom of every zoom.

use dioxus::prelude::*;

mod browser;
mod control;
mod engine_view;
mod fader;
mod graphs;
mod knob;
mod layer_macros;
mod layer_view;
mod macro_panel;
mod midi_light;
mod module_edit;
mod perform;
mod routing;
mod selection;
mod state;
mod zoom;

pub use browser::Browser;
pub use control::{ControlView, engine_color};
pub use engine_view::EngineView;
pub use knob::Knob;
pub use layer_view::LayerView;
pub use macro_panel::{MacroPanel, Shape, ShapeCard};
pub use module_edit::ModuleEdit;
pub use zoom::{OpenButton, Zoom};
pub use fader::{Fader, fmt_db};
pub use graphs::{Adsr, EnvelopeGraph, FilterCurve};
pub use midi_light::MidiPanel;
pub use perform::{PerformStrip, stack_color};
pub use routing::RoutingView;
pub use selection::{Selection, use_selection};
pub use state::{KeysViewState, held_notes, use_keys_state};

// The wire contract, re-exported for convenience.
pub use signal_keys_proto as proto;

/// The keys-rig remote view. Mount inside a host that has provided
/// `KeysRigClient` + `KeysRigStreamClient` in context.
///
/// The rig draws no bar of its own: it publishes its readouts and its panels
/// into the app chrome (`fts_chrome`), and the mixer/engine/layer zoom is the
/// whole surface. Routing, the MIDI monitor and the keyboard used to be page
/// modes — pages you had to leave the mixer to reach; they are right-rail
/// panels now, so they open *beside* what you are playing.
#[component]
pub fn KeysRigRemote() -> Element {
    let (state, _rig) = use_keys_state();
    // Control-view depth: mixer → engine → layer. Shared through context so
    // the cards can open themselves.
    let zoom = use_context_provider(|| Signal::new(Zoom::Mixer));
    // What the browser is pointed at — a path into the rig, set by clicking
    // anything in the mixer.
    let _selection = use_context_provider(|| Signal::new(selection::Selection::default()));
    // The live state, for the panels that re-pull their own detail when the
    // rig publishes (the mixer's macro band).
    use_context_provider(|| state);
    let level = fts_chrome::use_chrome_level(2);

    let status = state.status.read().clone();
    let mixer = state.mixer.read().clone();
    let perform = state.perform.read().clone();
    let tree = state.tree.read().clone();
    let midi = state.midi.read().clone();

    let master_pct = (status.master_peak.clamp(0.0, 1.0) * 100.0) as u32;
    let running = status.running;
    // The header lens: the active stack IS the sound, like the guitar rig's.
    let active_stack = perform.stacks.iter().find(|s| s.is_active);
    let (lens_bg, lens_fg) = active_stack
        .map(|s| stack_color(&s.name))
        .unwrap_or(("#1c1c20", "#a1a1aa"));
    let lens_label = active_stack
        .map(|s| s.name.to_uppercase())
        .or_else(|| status.loaded_preset.clone())
        .unwrap_or_else(|| "—".into());
    let live_lanes = mixer
        .engines
        .iter()
        .flat_map(|e| e.layers.iter())
        .filter(|l| l.live && !l.muted)
        .count();
    let (midi_lit, midi_color) = midi_light::use_midi_glow(&midi);

    // What the rig's bar used to say, said in the app bar instead.
    level.status(vec![
        fts_chrome::StatusItem::pill(lens_label.clone(), lens_fg, lens_bg),
        fts_chrome::StatusItem::text(format!(
            "{live_lanes} lane{} live",
            if live_lanes == 1 { "" } else { "s" }
        )),
        fts_chrome::StatusItem::dot(midi_lit, midi_color),
        fts_chrome::StatusItem::dot(running, "#22c55e"),
        fts_chrome::StatusItem::meter(status.master_peak, "#22c55e"),
    ]);
    // The old Routing and Session pages, as panels.
    level.panels(vec![
        fts_chrome::PanelSpec::new("keys.routing", "Routing", fts_chrome::Icon::Routing).width(360),
        fts_chrome::PanelSpec::new("keys.midi", "MIDI", fts_chrome::Icon::Midi).width(360),
    ]);
    let held = held_notes(&midi);
    let _ = master_pct;

    rsx! {
        div {
            style: "display: flex; flex-direction: column; flex: 1; min-height: 0; \
                    color: #e4e4e7; font-family: sans-serif; background: #08080a;",
            div { style: "display: flex; flex: 1; min-height: 0;",
                // The browser — one sidebar, pointed at the selection.
                Browser { state }
                div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0;",
                    match zoom() {
                        Zoom::Mixer => rsx! {
                            ControlView { mixer: mixer.clone(), held: held.clone() }
                        },
                        Zoom::Engine(name) => {
                            match mixer.engines.iter().find(|e| e.name == name) {
                                Some(engine) => rsx! {
                                    EngineView { engine: engine.clone(), zoom }
                                },
                                None => rsx! {
                                    ControlView { mixer: mixer.clone(), held: held.clone() }
                                },
                            }
                        }
                        Zoom::Layer(name) => rsx! {
                            LayerView { layer: name, zoom, mixer: mixer.clone() }
                        },
                    }
                }
                // The rig's panels, flush against the app's right rail.
                fts_chrome::PanelHost { id: "keys.routing".to_string(),
                    RoutingView { tree: tree.clone() }
                }
                fts_chrome::PanelHost { id: "keys.midi".to_string(),
                    midi_light::MidiPanel { midi: midi.clone(), port: status.midi_port.clone() }
                }
            }
            // ── perform strip (every zoom) ──
            PerformStrip { perform: perform.clone() }
        }
    }
}
