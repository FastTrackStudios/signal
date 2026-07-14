//! Headless synth-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`KeysRig`] (the shared composition-tree host on the engine),
//! scans an Omnisphere patch library for presets, imports the selected
//! `.prt_omn` / `.mlt_omn` into a composition tree (realizing its Soundsources
//! against the local extraction), and plays it from hardware MIDI or UI notes.
//! Implements the [`signal_synth_proto::synth::SynthRig`] service + its
//! `#[subscribe]` stream; mount [`router`](SynthRigBackend::router) on a vox
//! transport.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, LayerRouter, PubSub, Services, layers};
use daw_audio_io::AudioIoPrefs;
use midicore::{MidiEvent, PortSelector};
use signal_keys::KeysRig;
use signal_sampler::rig_node::{Container, RigNode, Role};
use signal_synth::omni_import::{SoundsourceIndex, load_patch_file};
use signal_synth_proto::synth::{
    SynthEvent, SynthRig as SynthRigSvc, SynthRigStreamSource,
};
use signal_synth_proto::{SynthNode, SynthPreset, SynthStatus};

/// Root of the Omnisphere patch library to browse. Defaults to the factory
/// **Live Keyboardist** bundle (where the default patch lives); override with
/// `FTS_OMNISPHERE_PATCHES`.
const PATCHES_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches/Factory/Live Keyboardist";

/// Substring of the preset selected on first open — "American Obesity" (a
/// Live Keyboardist pad built on the OB-8 PWM Big Strings + Prophet 5 Classic
/// soundsources). Falls back to the first preset if absent.
const DEFAULT_PATCH: &str = "American Obesity";
/// Meter-stream publish interval (~30 Hz).
const PUMP_MS: u64 = 33;

#[derive(Default)]
struct State {
    presets: Vec<SynthPreset>,
    /// Absolute patch path per preset (index-aligned).
    paths: Vec<PathBuf>,
    loaded: Option<usize>,
    /// The loaded composition tree (for the control-view structure).
    tree: Option<Container>,
    midi_port: Option<String>,
    midi_handle: Option<signal_sampler::MidiInputHandle>,
}

struct Inner {
    rig: Mutex<Option<KeysRig>>,
    state: Mutex<State>,
    /// The local soundsource extraction index — realizes Sample-mode patches.
    /// Scanned once at construction (may be empty on machines without it).
    index: SoundsourceIndex,
    events: PubSub<SynthEvent>,
    pump_started: AtomicBool,
}

/// The synth-rig backend handle. Cheap to clone (all state shared).
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct SynthRigBackend {
    inner: Arc<Inner>,
}

impl Default for SynthRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SynthRigBackend {
    /// Build the backend, scan the patch library + soundsource extraction. Does
    /// not open audio.
    pub fn new() -> Self {
        let (presets, paths) = scan_patches();
        let index = SoundsourceIndex::scan_default();
        // Default patch: American Obesity if present, else the first found.
        let default_idx = presets
            .iter()
            .position(|p| p.name.contains(DEFAULT_PATCH))
            .or_else(|| (!presets.is_empty()).then_some(0));
        tracing::info!(
            presets = presets.len(),
            soundsources = index.len(),
            "synth rig: scanned library"
        );
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                state: Mutex::new(State { presets, paths, loaded: default_idx, ..State::default() }),
                index,
                events: PubSub::sliding(64),
                pump_started: AtomicBool::new(false),
            }),
        };
        backend.spawn_meter_pump();
        backend
    }

    /// The composed service router — mount this on a vox transport.
    pub fn router(&self) -> LayerRouter {
        self.clone().into_router()
    }

    /// Import preset `index` into a composition tree (realizing soundsources).
    fn program_for(&self, index: usize) -> Option<Container> {
        let path = {
            let s = self.inner.state.lock().ok()?;
            s.paths.get(index)?.clone()
        };
        match load_patch_file(&path, &self.inner.index) {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::error!(?path, "synth rig: patch import failed: {e}");
                None
            }
        }
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
                tracing::error!("synth rig: audio open failed: {e}");
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
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_handle = None;
        }
        let sel = match &port {
            Some(name) => PortSelector::NameContains(name.clone()),
            None => PortSelector::All,
        };
        // KeysRig isn't Clone (owns the audio engine), so attach under the lock.
        let handle = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            rig.attach_midi(sel).map_err(|e| e.to_string())
        };
        match handle {
            Ok(h) => {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
                tracing::info!(port = %port.as_deref().unwrap_or("omni (all inputs)"), "synth rig: MIDI attached");
            }
            Err(e) => tracing::error!("synth rig: MIDI attach failed: {e}"),
        }
    }

    fn publish_all(&self) {
        self.inner.events.publish(SynthEvent::Library(SynthRigSvc::presets(self)));
        self.inner.events.publish(SynthEvent::Tree(SynthRigSvc::tree(self)));
        self.inner.events.publish(SynthEvent::Status(SynthRigSvc::status(self)));
    }

    fn spawn_meter_pump(&self) {
        if self.inner.pump_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let backend = self.clone();
        let _ = std::thread::Builder::new()
            .name("synth-meter-pump".into())
            .spawn(move || {
                let mut last_running = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(PUMP_MS));
                    let running = backend.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
                    if running != last_running {
                        last_running = running;
                        backend.inner.events.publish(SynthEvent::Status(SynthRigSvc::status(&backend)));
                        backend.inner.events.publish(SynthEvent::Tree(SynthRigSvc::tree(&backend)));
                    }
                    if !running {
                        continue;
                    }
                    backend.inner.events.publish(SynthEvent::Status(SynthRigSvc::status(&backend)));
                    backend.inner.events.publish(SynthEvent::Midi(SynthRigSvc::midi_recent(&backend)));
                }
            });
    }
}

// ── service impl ─────────────────────────────────────────────────────────────

impl SynthRigSvc for SynthRigBackend {
    fn start(&self) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("synth-open".into())
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
        self.inner.events.publish(SynthEvent::Status(SynthRigSvc::status(self)));
    }

    fn status(&self) -> SynthStatus {
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        let loaded_preset = s.loaded.and_then(|i| s.presets.get(i)).map(|p| p.name.clone());
        let master_peak = if running {
            self.inner.rig.lock().ok().and_then(|r| r.as_ref().map(|r| r.output_peak())).unwrap_or(0.0)
        } else {
            0.0
        };
        SynthStatus { running, loaded_preset, master_peak, voices: 0, midi_port: s.midi_port.clone() }
    }

    fn presets(&self) -> Vec<SynthPreset> {
        self.inner.state.lock().map(|s| s.presets.clone()).unwrap_or_default()
    }

    fn load_preset(&self, index: u32) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("synth-load".into())
            .spawn(move || b.do_load_preset(index as usize));
    }

    fn tree(&self) -> SynthNode {
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
        self.inner.events.publish(SynthEvent::Status(SynthRigSvc::status(self)));
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

impl SynthRigStreamSource for SynthRigBackend {
    fn events_hub(&self) -> &PubSub<SynthEvent> {
        &self.inner.events
    }
}

impl Services for SynthRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_synth_proto::synth::Service,
            signal_synth_proto::synth::StreamService
        ]
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn note_ev(note: u8, velocity: u8) -> MidiEvent {
    use midicore::{Channel, KeyNumber, Velocity};
    MidiEvent::NoteOn { channel: Channel::new(0), key: KeyNumber::new(note), velocity: Velocity::new(velocity) }
}

/// Recursively scan the Omnisphere patch library for `.prt_omn` / `.mlt_omn`
/// files. The patch's library folder becomes its `kind` (category); macOS
/// AppleDouble sidecars (`._*`) are skipped. Env `FTS_OMNISPHERE_PATCHES`
/// overrides the root.
fn scan_patches() -> (Vec<SynthPreset>, Vec<PathBuf>) {
    let root = std::env::var("FTS_OMNISPHERE_PATCHES").unwrap_or_else(|_| PATCHES_ROOT.into());
    let mut found: Vec<(String, String, PathBuf)> = Vec::new();
    let mut stack = vec![PathBuf::from(&root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with("._") {
                continue; // AppleDouble resource-fork sidecar
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "prt_omn" || x == "mlt_omn") {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if stem.is_empty() {
                    continue;
                }
                let kind = p
                    .parent()
                    .and_then(|d| d.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("Patches")
                    .to_string();
                found.push((stem, kind, p));
            }
        }
    }
    found.sort_by(|a, b| a.2.cmp(&b.2));
    let mut presets = Vec::with_capacity(found.len());
    let mut paths = Vec::with_capacity(found.len());
    for (name, kind, path) in found {
        presets.push(SynthPreset { name, kind, loaded: false });
        paths.push(path);
    }
    (presets, paths)
}

fn slug(s: &str) -> String {
    s.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Convert a composition [`Container`] into a wire [`SynthNode`] tree.
fn node_of(c: &Container, parent: &str) -> SynthNode {
    let id = if parent.is_empty() { slug(&c.name) } else { format!("{parent}/{}", slug(&c.name)) };
    let mut children = Vec::new();
    let mut any_live = false;
    for n in &c.children {
        let child = match n {
            RigNode::Container { container } => node_of(container, &id),
            RigNode::Block { block } => SynthNode {
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
    SynthNode { id, label: c.name.clone(), role: role_tag(c.role), live: any_live, children }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Recursively find a block by display name anywhere in the tree.
    fn find_block(c: &Container, name: &str) -> Option<signal_sampler::rig::RigBlock> {
        for n in &c.children {
            match n {
                RigNode::Block { block } if block.display_name() == name => return Some(block.clone()),
                RigNode::Container { container } => {
                    if let Some(b) = find_block(container, name) {
                        return Some(b);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Machine-local: the default patch (American Obesity, Live Keyboardist)
    /// imports, realizes its **OB-8 PWM Big Strings** soundsource against the
    /// local extraction as a live sample block, and renders audibly.
    /// `cargo test -p signal-synth-rig -- --ignored --nocapture`
    #[test]
    #[ignore = "requires the factory Omnisphere library + soundsource extraction"]
    fn default_patch_pulls_ob8_soundsource_and_sounds() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};

        let (presets, paths) = scan_patches();
        let Some(idx) = presets.iter().position(|p| p.name.contains(DEFAULT_PATCH)) else {
            eprintln!("skipping: {DEFAULT_PATCH} not present in the patch library");
            return;
        };
        let index = SoundsourceIndex::scan_default();
        if index.is_empty() {
            eprintln!("skipping: Omnisphere soundsource extraction not present");
            return;
        }
        let tree = load_patch_file(&paths[idx], &index).expect("import American Obesity");

        // The named soundsource is present AND realized (live sample backend).
        let ob8 = find_block(&tree, "OB-8 PWM Big Strings")
            .expect("American Obesity references the OB-8 PWM Big Strings soundsource");
        assert!(
            ob8.has_backend(),
            "OB-8 PWM Big Strings should realize as a live sample block against the extraction"
        );

        // It sounds.
        let mut rn = signal_sampler::node_render::RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiEvent::NoteOn {
                channel: daw::service::Channel::new(0),
                key: daw::service::KeyNumber::new(60),
                velocity: daw::service::Velocity::new(100),
            },
        }];
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents { params: &[], midi: &midi, note_expressions: &[] };
            rn.render(&mut l, &mut r, &ev);
            heard = heard.max((l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt());
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(heard > 1e-3, "American Obesity should be audible, rms={heard}");
    }
}
