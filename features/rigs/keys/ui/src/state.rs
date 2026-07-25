//! Live keys-rig view state — seeded once, then folded from the event
//! stream. Every view reads these signals; nothing polls.

use dioxus::prelude::*;
use midicore_proto::MidiEvent;
use signal_keys_proto::keys::{KeysEvent, KeysRigClient, KeysRigStreamClient};
use signal_keys_proto::{KeysMixer, KeysNode, KeysPerform, KeysPreset, KeysStatus};

/// The whole rig, as the remote sees it. `PartialEq` compares the signal
/// handles (cheap + stable), so it can ride as a component prop.
#[derive(Clone, Copy, PartialEq)]
pub struct KeysViewState {
    pub status: Signal<KeysStatus>,
    pub presets: Signal<Vec<KeysPreset>>,
    pub tree: Signal<KeysNode>,
    pub midi: Signal<Vec<MidiEvent>>,
    /// Engines → layers, faders, patches — the Control view's model.
    pub mixer: Signal<KeysMixer>,
    /// Stacks + grid mode — the Perform strip's model.
    pub perform: Signal<KeysPerform>,
}

/// Subscribe to the rig: seed state, then fold the event stream into it.
/// Returns the state and the command client (`None` when the host provided
/// no context — the views then render read-only).
pub fn use_keys_state() -> (KeysViewState, Option<KeysRigClient>) {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let stream = use_hook(try_consume_context::<KeysRigStreamClient>);

    let mut status = use_signal(KeysStatus::default);
    let mut presets = use_signal(Vec::<KeysPreset>::new);
    let mut tree = use_signal(KeysNode::default);
    let mut midi = use_signal(Vec::<MidiEvent>::new);
    let mut mixer = use_signal(KeysMixer::default);
    let mut perform = use_signal(KeysPerform::default);

    // Seed once — start the rig (idempotent), then pull current state.
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
                if let Ok(m) = rig.mixer().await {
                    mixer.set(m);
                }
                if let Ok(p) = rig.perform().await {
                    perform.set(p);
                }
                if let Ok(m) = rig.midi_recent().await {
                    midi.set(m);
                }
            }
        });
    }

    // Live updates. `use_stream` re-establishes the subscription itself when
    // it ends, so a restarted engine — or a burst that drops this subscriber —
    // heals without the view knowing.
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
            move |ev: KeysEvent| {
                let (mut status, mut presets, mut tree, mut midi) = (status, presets, tree, midi);
                let (mut mixer, mut perform) = (mixer, perform);
                match ev {
                    KeysEvent::Status(s) => status.set(s),
                    KeysEvent::Library(p) => presets.set(p),
                    KeysEvent::Tree(t) => tree.set(t),
                    KeysEvent::Midi(m) => midi.set(m),
                    KeysEvent::Mixer(m) => mixer.set(m),
                    KeysEvent::Perform(p) => perform.set(p),
                }
            },
        );
    }

    (
        KeysViewState { status, presets, tree, midi, mixer, perform },
        rig,
    )
}

/// Notes currently held, derived from the MIDI monitor — lights the piano.
pub fn held_notes(midi: &[MidiEvent]) -> Vec<u8> {
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
}
