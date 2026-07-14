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
use signal_synth_proto::{
    SynthArticulation, SynthEnvelope, SynthFilter, SynthLayer, SynthMapping, SynthMic,
    SynthModRoute, SynthNode, SynthPreset, SynthStatus, SynthZone,
};

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
/// Default master output gain — a −12 dB pad, because summed multi-layer
/// soundsource patches (two full-level layers + the loop) run hot and clip at
/// unity. The UI volume slider adjusts it live.
const DEFAULT_VOLUME: f32 = 0.25;

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
    /// Master output gain (linear), applied to the rig on open + live changes.
    volume: f32,
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
                state: Mutex::new(State {
                    presets,
                    paths,
                    loaded: default_idx,
                    volume: DEFAULT_VOLUME,
                    ..State::default()
                }),
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
        let volume = self.inner.state.lock().ok().map(|s| s.volume).unwrap_or(DEFAULT_VOLUME);
        match KeysRig::open(&prefs, &tree) {
            Ok(r) => {
                r.set_output_gain(volume); // pad the hot summed output
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
        SynthStatus {
            running,
            loaded_preset,
            master_peak,
            voices: 0,
            midi_port: s.midi_port.clone(),
            volume: s.volume,
        }
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

    fn set_volume(&self, gain_milli: u32) {
        let gain = gain_milli as f32 / 1000.0;
        if let Ok(mut s) = self.inner.state.lock() {
            s.volume = gain;
        }
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_output_gain(gain);
            }
        }
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

    fn soundsources(&self) -> Vec<String> {
        let mut blocks = Vec::new();
        if let Ok(s) = self.inner.state.lock() {
            if let Some(tree) = s.tree.as_ref() {
                sample_blocks(tree, &mut blocks);
            }
        }
        let mut names: Vec<String> = blocks.into_iter().map(|(n, _)| n).collect();
        names.dedup();
        names
    }

    fn mapping(&self, soundsource: String) -> SynthMapping {
        // Resolve the soundsource → its spec path: prefer the exact pack the
        // loaded tree references; fall back to the library index by name.
        let path = {
            let s = match self.inner.state.lock() {
                Ok(s) => s,
                Err(_) => return SynthMapping::default(),
            };
            let mut blocks = Vec::new();
            if let Some(tree) = s.tree.as_ref() {
                sample_blocks(tree, &mut blocks);
            }
            if soundsource.is_empty() {
                blocks.into_iter().next().map(|(_, p)| p)
            } else {
                blocks
                    .into_iter()
                    .find(|(n, _)| n == &soundsource)
                    .map(|(_, p)| p)
                    .or_else(|| self.inner.index.find(&soundsource).map(|p| p.to_path_buf()))
            }
        };
        let Some(path) = path else { return SynthMapping::default() };
        match spec_of(&path) {
            Some(spec) => project_mapping(&spec, &soundsource),
            None => SynthMapping::default(),
        }
    }

    fn layers(&self) -> Vec<SynthLayer> {
        let path = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.loaded.and_then(|i| s.paths.get(i).cloned()));
        let Some(path) = path else { return Vec::new() };
        // Single-part patches for now (a Multi's parts come later).
        if path.extension().is_some_and(|e| e == "mlt_omn") {
            return Vec::new();
        }
        let Ok(xml) = std::fs::read_to_string(&path) else { return Vec::new() };
        match signal_synth::omni_import::parse_patch(&xml) {
            Ok(patch) => project_layers(&patch),
            Err(_) => Vec::new(),
        }
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

/// Project a parsed patch's layers into the wire [`SynthLayer`]s for the Edit
/// page. Filter cutoff is the calibrated Hz; routes are scoped to the layer by
/// the `A/B/C/D ` target prefix.
fn project_layers(patch: &signal_synth::omni_import::OmniPatch) -> Vec<SynthLayer> {
    use signal_synth::omni_import::{classify_filter_full, omni_cutoff_hz};
    const NAMES: [&str; 4] = ["Layer A", "Layer B", "Layer C", "Layer D"];
    const LETTERS: [&str; 4] = ["A", "B", "C", "D"];

    let env = |e: Option<(f32, f32, f32, f32)>| {
        let (a, d, s, r) = e.unwrap_or((0.0, 0.0, 1.0, 0.0));
        SynthEnvelope { attack: a, decay: d, sustain: s, release: r }
    };

    patch
        .layers
        .iter()
        .take(4)
        .enumerate()
        .map(|(i, l)| {
            let (mode, poles, _) = classify_filter_full(&l.filter_name);
            let mut filters = vec![SynthFilter {
                name: l.filter_name.clone(),
                cutoff_hz: omni_cutoff_hz(l.filter_freq),
                resonance: l.filter_res,
                env_depth: l.filter_env_depth,
                mode: mode.to_string(),
                poles,
            }];
            if let Some((f2, r2)) = l.filter2 {
                filters.push(SynthFilter {
                    name: "Filter 2".to_string(),
                    cutoff_hz: omni_cutoff_hz(f2),
                    resonance: r2,
                    env_depth: 0.0,
                    mode: "lowpass".to_string(),
                    poles: 2,
                });
            }
            let prefix = format!("{} ", LETTERS[i]);
            let routes = patch
                .mod_routes
                .iter()
                .filter(|r| r.target.starts_with(&prefix))
                .map(|r| SynthModRoute {
                    source: r.source.clone(),
                    target: r.target.clone(),
                    depth: r.depth,
                })
                .collect();
            SynthLayer {
                name: NAMES[i].to_string(),
                active: !l.soundsource.is_empty() || l.filter_active || l.amp_env.is_some(),
                source: l.soundsource.clone(),
                level: l.level,
                filters,
                amp_env: env(l.amp_env),
                filter_env: env(l.filter_env),
                routes,
            }
        })
        .collect()
}

/// Collect `(soundsource name, spec path)` for every sample block in the tree.
fn sample_blocks(c: &Container, out: &mut Vec<(String, PathBuf)>) {
    for n in &c.children {
        match n {
            RigNode::Block { block } if !block.sample.is_empty() => {
                out.push((block.display_name().to_string(), PathBuf::from(&block.sample)));
            }
            RigNode::Container { container } => sample_blocks(container, out),
            _ => {}
        }
    }
}

/// Read a soundsource's `LibrarySpec` from its `.signalpack` (header only, no
/// audio decode) or raw `library.styx`.
fn spec_of(path: &std::path::Path) -> Option<signal_sampler::LibrarySpec> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("signalpack")) {
        signal_sampler::read_pack_header(path).ok().map(|h| h.spec)
    } else {
        signal_sampler::LibrarySpec::from_file(path).ok()
    }
}

/// Project a `LibrarySpec` into the wire [`SynthMapping`].
fn project_mapping(spec: &signal_sampler::LibrarySpec, name: &str) -> SynthMapping {
    let zones = spec
        .zones
        .iter()
        .map(|z| SynthZone {
            file: z.file.clone(),
            key_min: z.key_min,
            key_max: z.key_max,
            root_key: z.root_key,
            vel_min: z.vel_min,
            vel_max: z.vel_max,
            rr_index: z.rr_index,
            rr_mode: z.rr_mode.clone(),
            gain_db: z.gain_db,
            pan: z.pan,
            tune_cents: z.tune_cents,
            loop_start: z.loop_start,
            loop_end: z.loop_end,
            trigger_mode: z.trigger_mode.clone(),
            mic: z.mic.clone(),
            articulation: z.articulation.clone(),
            dynamic: z.dynamic.clone(),
            group: z.group.clone(),
            variant: z.variant.clone(),
        })
        .collect();
    let mics = spec
        .mics
        .iter()
        .map(|m| SynthMic {
            id: m.id.clone(),
            label: m.label.clone(),
            kind: m.kind.clone(),
            default: m.default,
        })
        .collect();
    let articulations = spec
        .articulations
        .iter()
        .map(|a| SynthArticulation {
            id: a.id.clone(),
            label: a.label.clone(),
            kind: format!("{:?}", a.kind),
            rr: a.rr as u32,
        })
        .collect();
    let mut groups: Vec<String> = spec
        .zones
        .iter()
        .map(|z| z.group.clone())
        .filter(|g| !g.is_empty())
        .collect();
    groups.sort();
    groups.dedup();
    SynthMapping {
        name: if name.is_empty() { spec.name.clone() } else { name.to_string() },
        vendor: spec.vendor.clone(),
        zones,
        mics,
        articulations,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Machine-local: the OB-8 PWM Big Strings **pack** plays its real samples
    /// — held past the ~2.8 s recording it SUSTAINS via the baked-in STINFO loop
    /// (not a piano decay), and at note 60 (a root-key zone, playback rate 1.0)
    /// the rendered tone's fundamental matches C4 (~261.6 Hz), proving it's the
    /// actual soundsource audio and not a fallback oscillator.
    /// `cargo test -p signal-synth-rig -- --ignored ob8_pack`
    #[test]
    #[ignore = "requires the built OB-8 PWM Big Strings pack"]
    fn ob8_pack_sustains_at_pitch() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};

        let pack = std::env::var("FTS_OMNISPHERE_PACKS")
            .unwrap_or_else(|_| {
                "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs".into()
            })
            + "/Synth Classic/OB-8 PWM Big Strings.signalpack";
        if !std::path::Path::new(&pack).exists() {
            eprintln!("skipping: {pack} not built");
            return;
        }

        // Bare soundsource — no filter/amp/FX, just the sample block.
        let tree = Container::preset("probe").add(
            Container::engine("e").add(
                Container::layer("A").add(Container::module("src").sample_block("Source", pack)),
            ),
        );
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

        let per_sec = 48_000 / 512;
        let warm = per_sec * 2; // retrigger while the pack preloads
        let blocks = per_sec * 7;
        let capture_from = per_sec * 6; // last ~1 s, well past retrigger + sample len
        let mut late = 0.0f32;
        let mut wave: Vec<f32> = Vec::new();
        for b in 0..blocks {
            let ev = PluginEvents {
                params: &[],
                midi: if b < warm { &midi } else { &[] },
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            if b < warm {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            if b >= capture_from {
                let rms = (l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
                late = late.max(rms);
                wave.extend_from_slice(&l);
            }
        }
        assert!(
            late > 1e-2,
            "OB-8 pack should SUSTAIN via the loop past the sample length, late rms={late}"
        );

        // Autocorrelation pitch estimate over the sustained window.
        let sr = 48_000.0f32;
        let (fmin, fmax) = (180.0f32, 360.0f32); // bracket C4 (261.6 Hz)
        let (lag_min, lag_max) = ((sr / fmax) as usize, (sr / fmin) as usize);
        let mut best_lag = lag_min;
        let mut best = f32::MIN;
        for lag in lag_min..=lag_max {
            let mut acc = 0.0f32;
            for i in 0..(wave.len() - lag) {
                acc += wave[i] * wave[i + lag];
            }
            if acc > best {
                best = acc;
                best_lag = lag;
            }
        }
        let f0 = sr / best_lag as f32;
        assert!(
            (f0 - 261.6).abs() < 12.0,
            "OB-8 at note 60 should sound ~C4 (261.6 Hz); got {f0:.1} Hz — wrong sample or pitch"
        );
        eprintln!("ob8_pack: sustained late_rms={late:.3}, fundamental={f0:.1} Hz");
    }

    /// Machine-local: the loaded default patch exposes its soundsources and a
    /// real keymap (85 one-per-key zones for OB-8 PWM Big Strings) over the
    /// Mapping wire types — the data source for the Mapping editor UI.
    /// `cargo test -p signal-synth-rig -- --ignored mapping`
    #[test]
    #[ignore = "requires the built OB-8 pack + factory library"]
    fn mapping_exposes_real_zones() {
        use signal_synth_proto::synth::SynthRig as _;

        let b = SynthRigBackend::new();
        // Seed the loaded tree without opening audio.
        let idx = b.inner.state.lock().unwrap().loaded;
        let Some(idx) = idx else {
            eprintln!("skipping: no default preset");
            return;
        };
        let Some(tree) = b.program_for(idx) else {
            eprintln!("skipping: default patch not importable");
            return;
        };
        b.inner.state.lock().unwrap().tree = Some(tree);

        let sources = b.soundsources();
        eprintln!("soundsources: {sources:?}");
        assert!(
            sources.iter().any(|s| s.contains("OB-8 PWM Big Strings")),
            "American Obesity should expose its OB-8 soundsource, got {sources:?}"
        );

        let m = b.mapping("OB-8 PWM Big Strings".into());
        eprintln!(
            "mapping {}: {} zones, {} mics, {} artics",
            m.name,
            m.zones.len(),
            m.mics.len(),
            m.articulations.len()
        );
        assert_eq!(m.zones.len(), 85, "OB-8 has one zone per key (85)");
        assert!(
            m.zones.iter().all(|z| z.key_min <= z.key_max && z.root_key >= z.key_min),
            "zones have sane key ranges"
        );
        // Loops are library-state-dependent (only packs rebuilt with the STINFO
        // fix carry them), so this is informational — the projection itself is
        // what's asserted above.
        let looped = m.zones.iter().filter(|z| z.loop_end > z.loop_start).count();
        eprintln!("{looped}/85 zones carry loops (0 = pack predates the STINFO fix)");
    }

    /// Machine-local: the default patch exposes its layers with filter +
    /// envelope + modulation over the wire — the Edit page's data source.
    /// `cargo test -p signal-synth-rig -- --ignored layers`
    #[test]
    #[ignore = "requires the factory Omnisphere library"]
    fn layers_expose_filter_env_and_routes() {
        use signal_synth_proto::synth::SynthRig as _;

        let b = SynthRigBackend::new();
        let layers = b.layers();
        eprintln!("{} layers", layers.len());
        let a = layers.iter().find(|l| l.name == "Layer A").expect("Layer A");
        eprintln!(
            "Layer A: src={:?} level={:.2} filter[0]={} {:.0}Hz res={:.2} envd={:.2} | amp {:?} filt {:?} | {} routes",
            a.source, a.level, a.filters[0].name, a.filters[0].cutoff_hz, a.filters[0].resonance,
            a.filters[0].env_depth, a.amp_env, a.filter_env, a.routes.len()
        );
        assert!(a.active, "Layer A active");
        assert!(!a.filters.is_empty(), "Layer A has a filter");
        assert!(a.filters[0].cutoff_hz > 0.0, "filter cutoff in Hz");
        // American Obesity: Filter Env + Wheel route the filter cutoff.
        assert!(
            a.routes.iter().any(|r| r.source.contains("Wheel") || r.source.contains("Env")),
            "Layer A carries its mod routes, got {:?}",
            a.routes
        );
    }

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

        // It sounds — AND it sustains. The OB-8 source is a ~2.8 s recording;
        // holding a note past that must keep sounding via the STINFO sustain
        // loop (baked into the pack), not decay to silence like a piano. We
        // render ~5 s holding note 60 and require the LAST second to still be
        // audible.
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
        // The sampler drops a note-on whose zone isn't cached yet, so retrigger
        // every block for the first ~2 s while the background preload decodes
        // the pack; then hold silently. The OB-8 source is a ~2.8 s recording —
        // the LAST second (~7 s in, long past retrigger + sample length) must
        // still sound, proving the STINFO sustain loop holds it (not a piano
        // decay). Give the preload wall-time during the warm phase.
        let per_sec = 48_000 / 512;
        let warm = per_sec * 2;
        let blocks = per_sec * 8;
        let late_from = blocks - per_sec; // last ~1 s
        let mut early = 0.0f32;
        let mut late = 0.0f32;
        for b in 0..blocks {
            let ev = PluginEvents {
                params: &[],
                midi: if b < warm { &midi } else { &[] },
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            let rms = (l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
            early = early.max(rms);
            if b >= late_from {
                late = late.max(rms);
            }
            if b < warm {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
        }
        assert!(
            early > 1e-3,
            "American Obesity should play its OB-8 soundsource, rms={early}"
        );
        // The held note SUSTAINS: the soundsource loops and the inactive
        // oscillator sub-modules (Harmonia/Dfs/Waveshaper) are no longer emitted
        // as live blocks that would mask it. Regression guard for the
        // "every patch sounds like the modal piano" bug.
        assert!(
            late > 1e-2,
            "American Obesity should SUSTAIN its soundsource past the sample length, late rms={late}"
        );
    }
}
