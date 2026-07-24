//! Keys-rig Dioxus components — the remote GUI half of the detachable keys
//! rig. Renders purely from `signal-keys-proto` via the generated vox clients
//! (provided in Dioxus context by the host). Inline styles only (Blitz-safe).
//!
//! Layout mirrors the guitar rig: a top bar (profile · patch lens · meters),
//! three pages behind mode tabs — **Routing** (the composition tree),
//! **Control** (the mixer: engines, layer faders, patch pickers) and
//! **Session** (keyboard + MIDI monitor) — and the **Perform strip** with the
//! profile's footswitch stacks pinned to the bottom of all of them.

use dioxus::prelude::*;
use midicore_ui::MidiMonitorPanel;
use signal_ui::components::Piano;

mod control;
mod engine_view;
mod fader;
mod knob;
mod layer_view;
mod midi_light;
mod perform;
mod routing;
mod state;
mod zoom;

pub use control::{ControlView, engine_color};
pub use engine_view::EngineView;
pub use knob::Knob;
pub use layer_view::LayerView;
pub use zoom::{OpenButton, Zoom, ZoomBar};
pub use fader::{Fader, fmt_db};
pub use midi_light::MidiLight;
pub use perform::{PerformStrip, stack_color};
pub use routing::RoutingView;
pub use state::{KeysViewState, held_notes, use_keys_state};

// The wire contract, re-exported for convenience.
pub use signal_keys_proto as proto;

/// Which page is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Wire the rig: the composition tree.
    Routing,
    /// Play & shape it: the mixer (default).
    Control,
    /// Keyboard, MIDI monitor, integration.
    Session,
}

/// The keys-rig remote view. Mount inside a host that has provided
/// `KeysRigClient` + `KeysRigStreamClient` in context.
#[component]
pub fn KeysRigRemote() -> Element {
    let (state, _rig) = use_keys_state();
    let mut mode = use_signal(|| Mode::Control);
    // Control-view depth: mixer → engine → layer. Shared through context so
    // the cards can open themselves.
    let zoom = use_context_provider(|| Signal::new(Zoom::Mixer));

    let status = state.status.read().clone();
    let mixer = state.mixer.read().clone();
    let perform = state.perform.read().clone();
    let presets = state.presets.read().clone();
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

    rsx! {
        div {
            style: "display: flex; flex-direction: column; flex: 1; min-height: 0; \
                    color: #e4e4e7; font-family: sans-serif; background: #08080a;",
            // ── top bar ──
            div {
                style: "display: flex; align-items: center; gap: 10px; padding: 7px 12px; \
                        border-bottom: 1px solid #1c1c1f;",
                span { style: "font-size: 13px; font-weight: 700;", "Keys" }
                span {
                    style: format!(
                        "padding: 3px 10px; border-radius: 999px; background: {lens_bg}; color: {lens_fg}; \
                         font-size: 11px; font-weight: 700; letter-spacing: 0.05em;",
                    ),
                    "{lens_label}"
                }
                span { style: "font-size: 10px; color: #52525b;",
                    {format!("{live_lanes} lane{} live", if live_lanes == 1 { "" } else { "s" })}
                }
                div { style: "flex: 1;" }
                // Mode tabs.
                for (m, label) in [
                    (Mode::Routing, "Routing"),
                    (Mode::Control, "Control"),
                    (Mode::Session, "Session"),
                ] {
                    button {
                        key: "{label}",
                        style: format!(
                            "appearance: none; border: none; border-radius: 7px; padding: 4px 12px; \
                             font-size: 11px; font-weight: 600; background: {}; color: {};",
                            if mode() == m { "#101821" } else { "transparent" },
                            if mode() == m { "#38bdf8" } else { "#52525b" },
                        ),
                        onclick: move |_| mode.set(m),
                        "{label}"
                    }
                }
                // MIDI activity light — click for the monitor + port picker.
                MidiLight { midi: midi.clone(), port: status.midi_port.clone() }
                div { style: "width: 4px;" }
                // Engine LED + master meter.
                span {
                    style: format!(
                        "width: 7px; height: 7px; border-radius: 999px; background: {}; box-shadow: 0 0 6px {};",
                        if running { "#22c55e" } else { "#3f3f46" },
                        if running { "#22c55e88" } else { "transparent" },
                    ),
                }
                div { style: "width: 80px; height: 8px; background: #18181b; border-radius: 2px; overflow: hidden;",
                    div { style: "height: 100%; width: {master_pct}%; background: #22c55e;" }
                }
            }
            // ── page ──
            match mode() {
                Mode::Routing => rsx! { RoutingView { tree: tree.clone() } },
                Mode::Control => match zoom() {
                    Zoom::Mixer => rsx! {
                        ControlView { mixer: mixer.clone(), presets: presets.clone() }
                    },
                    Zoom::Engine(name) => {
                        match mixer.engines.iter().find(|e| e.name == name) {
                            Some(engine) => rsx! {
                                EngineView {
                                    engine: engine.clone(),
                                    presets: presets.clone(),
                                    zoom,
                                }
                            },
                            None => rsx! {
                                ControlView { mixer: mixer.clone(), presets: presets.clone() }
                            },
                        }
                    }
                    Zoom::Layer(name) => rsx! { LayerView { layer: name, zoom } },
                },
                Mode::Session => rsx! { SessionView { state } },
            }
            // ── perform strip (every page) ──
            PerformStrip { perform: perform.clone() }
        }
    }
}

/// Session page: the playable keyboard + the MIDI monitor. (The setlist /
/// DAW-sync surface lands here next.)
#[component]
fn SessionView(state: KeysViewState) -> Element {
    let rig = use_hook(try_consume_context::<signal_keys_proto::keys::KeysRigClient>);
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;
    let lit = held_notes(&midi);

    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 12px; \
                    display: flex; flex-direction: column; gap: 12px;",
            MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor".to_string() }
            div {
                span {
                    style: "font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b;",
                    "Keyboard"
                }
                {
                    let rig_on = rig.clone();
                    let rig_off = rig.clone();
                    rsx! {
                        Piano {
                            start_note: 21,
                            end_note: 108,
                            active_notes: lit,
                            show_labels: false,
                            waterfall: false,
                            accent_color: "#a78bfa".to_string(),
                            height: "132px",
                            on_note_on: move |n: u8| {
                                let rig = rig_on.clone();
                                spawn(async move {
                                    if let Some(r) = rig { let _ = r.trigger(n as u32, 100).await; }
                                });
                            },
                            on_note_off: move |n: u8| {
                                let rig = rig_off.clone();
                                spawn(async move {
                                    if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; }
                                });
                            },
                        }
                    }
                }
            }
        }
    }
}
