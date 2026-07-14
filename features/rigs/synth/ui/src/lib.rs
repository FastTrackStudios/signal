//! Synth-rig Dioxus components — the remote GUI half of the detachable synth
//! rig. Renders purely from `signal-synth-proto` via the generated vox clients
//! (provided in Dioxus context by the host). Inline styles only (Blitz-safe).
//!
//! Sibling of `signal-keys-ui`: a **preset browser** on the left (the imported
//! Omnisphere patches), the **control view** in the middle (boxes for the
//! Quadzone + its layers, from the composition tree), and a **performance**
//! strip (the piano) at the bottom.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use midicore_ui::MidiMonitorPanel;
use signal_synth_proto::synth::{SynthEvent, SynthRigClient, SynthRigStreamClient};
use signal_synth_proto::{SynthNode, SynthPreset, SynthStatus};
use signal_ui::components::Piano;

/// Live synth-rig view-state: seeded once, then folded from the event stream.
#[derive(Clone, Copy)]
struct SynthState {
    status: Signal<SynthStatus>,
    presets: Signal<Vec<SynthPreset>>,
    tree: Signal<SynthNode>,
    midi: Signal<Vec<MidiEvent>>,
}

fn use_synth_state() -> (SynthState, Option<SynthRigClient>) {
    let rig = use_hook(try_consume_context::<SynthRigClient>);
    let stream = use_hook(try_consume_context::<SynthRigStreamClient>);

    let mut status = use_signal(SynthStatus::default);
    let mut presets = use_signal(Vec::<SynthPreset>::new);
    let mut tree = use_signal(SynthNode::default);
    let mut midi = use_signal(Vec::<MidiEvent>::new);

    // Seed once — start the rig, then pull the current state.
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                let _ = rig.start().await;
                if let Ok(s) = rig.status().await {
                    status.set(s);
                }
                if let Ok(p) = rig.presets().await {
                    presets.set(p);
                }
                if let Ok(t) = rig.tree().await {
                    tree.set(t);
                }
                if let Ok(m) = rig.midi_recent().await {
                    midi.set(m);
                }
            }
        });
    }

    // Live updates.
    {
        let stream = stream.clone();
        architect::use_stream(
            move |sink| {
                let stream = stream.clone();
                async move {
                    match stream {
                        Some(s) => s.events(sink).await.is_ok(),
                        None => false,
                    }
                }
            },
            move |ev: SynthEvent| {
                let (mut status, mut presets, mut tree, mut midi) = (status, presets, tree, midi);
                match ev {
                    SynthEvent::Status(s) => status.set(s),
                    SynthEvent::Library(p) => presets.set(p),
                    SynthEvent::Tree(t) => tree.set(t),
                    SynthEvent::Midi(m) => midi.set(m),
                }
            },
        );
    }

    (SynthState { status, presets, tree, midi }, rig)
}

/// The synth-rig remote view. Mount inside a host that has provided
/// `SynthRigClient` + `SynthRigStreamClient` in context.
#[component]
pub fn SynthRigRemote() -> Element {
    let (state, rig) = use_synth_state();
    let status = state.status.read().clone();
    let presets = state.presets.read().clone();
    let tree = state.tree.read().clone();
    let midi = state.midi.read().clone();
    let midi_count = midi.len() as u64;

    // Currently-held notes light the piano.
    let lit: Vec<u8> = {
        let mut held = std::collections::BTreeSet::<u8>::new();
        for e in midi.iter() {
            match e {
                MidiEvent::NoteOn { key, velocity, .. } if velocity.get() > 0 => {
                    held.insert(key.get());
                }
                MidiEvent::NoteOn { key, .. } | MidiEvent::NoteOff { key, .. } => {
                    held.remove(&key.get());
                }
                _ => {}
            }
        }
        held.into_iter().collect()
    };

    let master_pct = (status.master_peak.clamp(0.0, 1.0) * 100.0) as u32;

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:0; flex:1; min-height:0; color:#e4e4e7; font-family:sans-serif;",
            // ── top bar ──
            div { style: "display:flex; align-items:center; gap:10px; padding:6px 12px; border-bottom:1px solid #1c1c1f;",
                span { style: "font-weight:700; font-size:13px;", "Synth" }
                span { style: "font-size:11px; color:#a1a1aa;", {status.loaded_preset.clone().unwrap_or_else(|| "—".into())} }
                div { style: "flex:1;" }
                // master meter
                div { style: "width:80px; height:8px; background:#18181b; border-radius:2px; overflow:hidden;",
                    div { style: "height:100%; width:{master_pct}%; background:#22c55e;" }
                }
            }
            div { style: "display:flex; gap:12px; flex:1; min-height:0;",
                // ── preset browser (left) ──
                div { style: "display:flex; flex-direction:column; gap:4px; width:220px; min-width:220px; overflow:auto; border-right:1px solid #1c1c1f; padding:8px;",
                    span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Presets ({presets.len()})" }
                    for (i, preset) in presets.iter().enumerate() {
                        {
                            let rig = rig.clone();
                            rsx!{ button {
                                key: "{preset.name}",
                                style: preset_btn(preset.loaded),
                                onclick: move |_| { let rig = rig.clone(); spawn(async move { if let Some(r) = rig { let _ = r.load_preset(i as u32).await; } }); },
                                span { style: "font-size:12px; font-weight:600;", "{preset.name}" }
                                span { style: "font-size:9px; color:#71717a;", "{preset.kind}" }
                            } }
                        }
                    }
                }
                // ── control view + performance ──
                div { style: "display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:auto; padding:10px;",
                    // control view: the layer tree as boxes
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Control" }
                        div { style: "margin-top:6px;",
                            NodeBoxes { node: tree }
                        }
                    }
                    MidiMonitorPanel { events: midi, count: midi_count, title: "MIDI monitor".to_string() }
                    // performance: piano
                    div {
                        span { style: "font-size:11px; color:#71717a; text-transform:uppercase; letter-spacing:0.05em;", "Keyboard" }
                        {
                            let rig_on = rig.clone();
                            let rig_off = rig.clone();
                            rsx!{ Piano {
                                start_note: 21,
                                end_note: 108,
                                active_notes: lit,
                                show_labels: false,
                                waterfall: false,
                                accent_color: "#38bdf8".to_string(),
                                height: "132px",
                                on_note_on: move |n: u8| { let rig = rig_on.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 100).await; } }); },
                                on_note_off: move |n: u8| { let rig = rig_off.clone(); spawn(async move { if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; } }); },
                            } }
                        }
                    }
                }
            }
        }
    }
}

/// Recursively render the composition tree as nested selectable boxes — the
/// Quadzone → layers → blocks structure (placeholder control surface for now).
#[component]
fn NodeBoxes(node: SynthNode) -> Element {
    if node.id.is_empty() {
        return rsx! { span { style: "font-size:11px; color:#52525b;", "no preset loaded" } };
    }
    let border = match node.role.as_str() {
        "engine" => "#4c3f6b",
        "layer" => "#3f5178",
        "block" => {
            if node.live { "#166534" } else { "#3f3f46" }
        }
        _ => "#27272a",
    };
    let bg = if node.role == "block" && node.live { "#14321e" } else { "#111113" };
    rsx! {
        div { style: format!("display:flex; flex-direction:column; gap:4px; padding:6px 8px; margin:3px 0; border-radius:8px; background:{bg}; border:1px solid {border};"),
            div { style: "display:flex; align-items:center; gap:6px;",
                span { style: "font-size:9px; color:#71717a; text-transform:uppercase;", "{node.role}" }
                span { style: "font-size:12px; font-weight:600;", "{node.label}" }
                if node.role == "block" && !node.live {
                    span { style: "font-size:9px; color:#71717a;", "(silent)" }
                }
            }
            if !node.children.is_empty() {
                div { style: "padding-left:10px; border-left:1px solid #27272a;",
                    for child in node.children.iter() {
                        NodeBoxes { key: "{child.id}", node: child.clone() }
                    }
                }
            }
        }
    }
}

fn preset_btn(loaded: bool) -> String {
    let (bg, br, fg) = if loaded { ("#0c2733", "#0ea5e9", "#e4e4e7") } else { ("#111113", "#27272a", "#a1a1aa") };
    format!("display:flex; flex-direction:column; text-align:left; padding:6px 8px; border-radius:6px; background:{bg}; color:{fg}; border:1px solid {br}; font-size:12px; cursor:pointer;")
}
