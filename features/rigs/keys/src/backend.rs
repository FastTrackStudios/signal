//! Headless keys-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`KeysRig`] (composition-tree instrument on the shared engine),
//! scans the Keyscape library for presets, loads a single-instrument program
//! per preset, and plays it from hardware MIDI or UI notes. Implements the
//! [`signal_keys_proto::keys::KeysRig`] service + its `#[subscribe]` stream;
//! mount `router()` (`architect::rig::RigBackend`) on a vox transport.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use daw_audio_io::AudioIoPrefs;
use midicore::MidiEvent;
use signal_keys_proto::keys::{KeysEvent, KeysRig as KeysRigSvc, KeysRigStreamSource};
use signal_keys_proto::{KeysNode, KeysPreset, KeysStatus};
use signal_sampler::rig_node::{RigNode, Role};
use signal_sampler::{Container, MidiInputHandle};

use crate::KeysRig;

/// Root of the local Keyscape extraction (per-instrument dirs each holding a
/// `library.styx`). Used as a fallback when no packs are present. Override with
/// `FTS_KEYSCAPE_ROOT`.
const KEYSCAPE_ROOT: &str = "/run/media/AudioHaven/Sampled/Keys/Keyscape";
/// Root of the built `.signalpack` library (one self-contained pack per
/// instrument). Preferred over the raw extraction. Override with
/// `FTS_KEYSCAPE_PACKS`.
const KEYSCAPE_PACKS_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs";
#[derive(Default)]
struct State {
    presets: Vec<KeysPreset>,
    /// Absolute `library.styx` spec path per preset (index-aligned).
    specs: Vec<PathBuf>,
    loaded: Option<usize>,
    /// The loaded composition tree (for the control-view structure).
    tree: Option<Container>,
    midi_port: Option<String>,
    midi_handle: Option<MidiInputHandle>,
}

struct Inner {
    rig: Mutex<Option<KeysRig>>,
    state: Mutex<State>,
    events: PubSub<KeysEvent>,
    pump_started: AtomicBool,
}

/// The keys-rig backend handle. Cheap to clone (all state shared).
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct KeysRigBackend {
    inner: Arc<Inner>,
}

impl Default for KeysRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl KeysRigBackend {
    /// Build the backend and scan the Keyscape library. Does not open audio.
    pub fn new() -> Self {
        let (presets, specs) = scan_keyscape();
        // Default patch: the LA Custom Rhodes if present, else the first found.
        let default_idx = presets
            .iter()
            .position(|p| p.name == "Rhodes - LA Custom")
            .or_else(|| {
                presets.iter().position(|p| {
                    let n = p.name.to_ascii_lowercase();
                    n.contains("rhodes") && n.contains("la custom")
                })
            });
        tracing::info!(
            presets = presets.len(),
            default = default_idx.and_then(|i| presets.get(i)).map(|p| p.name.as_str()).unwrap_or("<first>"),
            "keys rig: scanned library"
        );
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                state: Mutex::new(State { presets, specs, loaded: default_idx, ..State::default() }),
                events: architect::rig::events_hub(),
                pump_started: AtomicBool::new(false),
            }),
        };
        backend.spawn_meter_pump("keys-meter-pump");
        backend
    }

    fn program_for(&self, index: usize) -> Option<Container> {
        let s = self.inner.state.lock().ok()?;
        let spec = s.specs.get(index)?.to_string_lossy().into_owned();
        let name = s.presets.get(index)?.name.clone();
        Some(keys_program(&name, spec))
    }

    /// Open audio with the given (or first) preset, if not already open.
    fn ensure_open(&self) -> bool {
        {
            if self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false) {
                return true;
            }
        }
        let idx = self.inner.state.lock().ok().and_then(|s| s.loaded).unwrap_or(0);
        let Some(tree) = self.program_for(idx) else { return false };
        let prefs = AudioIoPrefs {
            output_device: String::new(),
            sample_rate: 0,
            buffer_size: 256,
            ..Default::default()
        };
        match KeysRig::open(&prefs, &tree) {
            Ok(r) => {
                {
                    let mut rig = self.inner.rig.lock().unwrap();
                    *rig = Some(r);
                }
                if let Ok(mut s) = self.inner.state.lock() {
                    if s.loaded.is_none() {
                        s.loaded = Some(idx);
                    }
                    for (i, p) in s.presets.iter_mut().enumerate() {
                        p.loaded = i == idx;
                    }
                    s.tree = Some(tree);
                }
                true
            }
            Err(e) => {
                tracing::error!("keys rig: audio open failed: {e}");
                false
            }
        }
    }

    fn do_load_preset(&self, index: usize) {
        let Some(tree) = self.program_for(index) else { return };
        if !self.ensure_open() {
            return;
        }
        if let Ok(mut rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_mut() {
                rig.load_preset(&tree);
            }
        }
        if let Ok(mut s) = self.inner.state.lock() {
            for (i, p) in s.presets.iter_mut().enumerate() {
                p.loaded = i == index;
            }
            s.loaded = Some(index);
            s.tree = Some(tree);
        }
        self.publish_all();
    }

    fn reattach_midi(&self) {
        let port = self.inner.state.lock().ok().and_then(|s| s.midi_port.clone());
        midicore::attach::reattach(
            "keys rig",
            port.as_deref(),
            || {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = None;
                }
            },
            |sel| {
                // KeysRig isn't Clone (owns the audio engine), so attach
                // under the lock.
                let rig = self.inner.rig.lock().unwrap();
                match rig.as_ref() {
                    Some(rig) => rig.attach_midi(sel).map(Some),
                    None => Ok(None),
                }
            },
            |h| {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
            },
        );
    }

    fn publish_all(&self) {
        self.inner.events.publish(KeysEvent::Library(KeysRigSvc::presets(self)));
        self.inner.events.publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

}

// r[impl primitives.architect.rig-backend]
impl RigBackend for KeysRigBackend {
    type Event = KeysEvent;
    type Tick = ();

    fn events_hub(&self) -> &PubSub<KeysEvent> {
        &self.inner.events
    }

    fn is_running(&self) -> bool {
        self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false)
    }

    fn pump_started(&self) -> &AtomicBool {
        &self.inner.pump_started
    }

    fn on_running_edge(&self, _running: bool) {
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner.events.publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
    }

    fn on_running_tick(&self) {
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner.events.publish(KeysEvent::Midi(KeysRigSvc::midi_recent(self)));
    }

    fn midi_ports(&self) -> Vec<String> {
        KeysRig::midi_input_ports()
    }

    fn on_midi_ports_changed(&self, ports: &[String]) {
        // A keyboard plugged in after the rig started is merged into the
        // omni stream without touching the UI.
        tracing::info!(?ports, "keys rig: MIDI ports changed — re-attaching");
        self.reattach_midi();
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }
}

// ── service impl ─────────────────────────────────────────────────────────────

impl KeysRigSvc for KeysRigBackend {
    fn start(&self) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-open".into())
            .spawn(move || {
                if b.ensure_open() {
                    b.reattach_midi();
                }
                b.publish_all();
            });
    }

    fn stop(&self) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_handle = None;
        }
        if let Ok(mut rig) = self.inner.rig.lock() {
            *rig = None;
        }
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

    fn status(&self) -> KeysStatus {
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        let loaded_preset = s.loaded.and_then(|i| s.presets.get(i)).map(|p| p.name.clone());
        let master_peak = if running {
            self.inner.rig.lock().ok().and_then(|r| r.as_ref().map(|r| r.output_peak())).unwrap_or(0.0)
        } else {
            0.0
        };
        KeysStatus { running, loaded_preset, master_peak, voices: 0, midi_port: s.midi_port.clone() }
    }

    fn presets(&self) -> Vec<KeysPreset> {
        self.inner.state.lock().map(|s| s.presets.clone()).unwrap_or_default()
    }

    fn load_preset(&self, index: u32) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-load".into())
            .spawn(move || b.do_load_preset(index as usize));
    }

    fn tree(&self) -> KeysNode {
        self.inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.tree.as_ref().map(|t| node_of(t, "")))
            .unwrap_or_default()
    }

    fn trigger(&self, note: u32, velocity: u32) {
        let (note, velocity) = (note as u8, velocity as u8);
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                if velocity > 0 {
                    rig.note_on(note, velocity);
                    rig.midi_monitor().record(&note_ev(note, velocity));
                } else {
                    rig.note_off(note);
                }
            }
        }
    }

    fn midi_ports(&self) -> Vec<String> {
        KeysRig::midi_input_ports()
    }

    fn set_midi_port(&self, name: String) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_port = if name.is_empty() { None } else { Some(name) };
        }
        self.reattach_midi();
        self.inner.events.publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

    fn midi_recent(&self) -> Vec<MidiEvent> {
        self.inner
            .rig
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.midi_monitor().recent()))
            .unwrap_or_default()
    }
}

impl KeysRigStreamSource for KeysRigBackend {
    fn events_hub(&self) -> &PubSub<KeysEvent> {
        &self.inner.events
    }
}

impl Services for KeysRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_keys_proto::keys::Service,
            signal_keys_proto::keys::StreamService
        ]
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn note_ev(note: u8, velocity: u8) -> MidiEvent {
    use midicore::{Channel, KeyNumber, Velocity};
    MidiEvent::NoteOn { channel: Channel::new(0), key: KeyNumber::new(note), velocity: Velocity::new(velocity) }
}

/// Build a single-instrument keys program: Preset → Keys engine → Layer A →
/// Piano Source (Sampler block realized by `spec`).
fn keys_program(name: &str, spec: String) -> Container {
    Container::preset(name).add(
        Container::engine("Keys").add(
            Container::layer("Layer A").add(
                Container::module("Piano Source").sample_block("Piano", spec),
            ),
        ),
    )
}

/// Discover Keyscape instruments to load. Prefers the `.signalpack` library
/// (self-contained packs — faster load, the intended distribution format) and
/// falls back to the raw `library.styx` extraction if no packs are found.
/// The stored spec path (`.signalpack` or `library.styx`) is handed to the
/// sample block; `rig.rs` picks the loader by extension.
fn scan_keyscape() -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let packs_root =
        std::env::var("FTS_KEYSCAPE_PACKS").unwrap_or_else(|_| KEYSCAPE_PACKS_ROOT.into());
    let (packs, pack_specs) = scan_packs(&packs_root);
    if !packs.is_empty() {
        return (packs, pack_specs);
    }
    let root = std::env::var("FTS_KEYSCAPE_ROOT").unwrap_or_else(|_| KEYSCAPE_ROOT.into());
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            let styx = dir.join("library.styx");
            if styx.exists() {
                let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                presets.push(KeysPreset { kind: kind_of(&name), name, loaded: false });
                specs.push(styx);
            }
        }
    }
    (presets, specs)
}

/// Enumerate `*.signalpack` files in the packs root; the file stem is the
/// instrument name.
fn scan_packs(root: &str) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut packs: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("signalpack")))
            .collect();
        packs.sort();
        for pack in packs {
            let name = pack.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            presets.push(KeysPreset { kind: kind_of(&name), name, loaded: false });
            specs.push(pack);
        }
    }
    (presets, specs)
}

/// Broad category for grouping in the browser.
fn kind_of(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.contains("grand") || (n.contains("piano") && !n.contains("e piano")) { "Grand".into() }
    else if n.contains("rhodes") { "Rhodes".into() }
    else if n.contains("wurl") { "Wurlitzer".into() }
    else if n.contains("clav") { "Clav".into() }
    else if n.contains("mks") || n.contains("mk-80") || n.contains("e piano") || n.contains("electric") { "Electric".into() }
    else if n.contains("toy") || n.contains("celeste") || n.contains("glock") || n.contains("bell") { "Toy/Bell".into() }
    else { "Other".into() }
}

fn slug(s: &str) -> String {
    s.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Convert a composition [`Container`] into a wire [`KeysNode`] tree.
fn node_of(c: &Container, parent: &str) -> KeysNode {
    let id = if parent.is_empty() { slug(&c.name) } else { format!("{parent}/{}", slug(&c.name)) };
    let mut children = Vec::new();
    let mut any_live = false;
    for n in &c.children {
        let child = match n {
            RigNode::Container { container } => node_of(container, &id),
            RigNode::Block { block } => KeysNode {
                id: format!("{id}/{}", slug(&block.name)),
                label: block.name.clone(),
                role: "block".into(),
                live: block.has_backend(),
                children: Vec::new(),
            },
        };
        any_live |= child.live;
        children.push(child);
    }
    KeysNode { id, label: c.name.clone(), role: role_tag(c.role), live: any_live, children }
}

fn role_tag(role: Role) -> String {
    match role {
        Role::Preset => "preset",
        Role::Engine => "engine",
        Role::Layer => "layer",
        Role::Module => "module",
    }
    .to_string()
}
