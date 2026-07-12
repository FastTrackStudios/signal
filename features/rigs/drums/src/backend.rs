//! Headless drum-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`SamplerRig`], loads GGD-style `.signalpreset` kits, plays them
//! from hardware MIDI (through the drum-map converter) or UI pads, and exposes
//! the multi-mic drum mixer. Implements the [`signal_drums_proto::drum::DrumRig`]
//! service + its `#[subscribe]` event stream; mount [`router`](DrumRigBackend::router)
//! on a vox transport (in-process, WebSocket, iroh).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, LayerRouter, PubSub, Services, layers};
use midicore::{DrumMap, PortSelector};
use signal_drums_proto::drum::{DrumEvent, DrumRig, DrumRigStreamSource};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo, StripKind};
use signal_sampler::{MidiInputHandle, PreloadProfile, PresetSpec, SamplerRig};

use crate::{GM_DRUM_CHANNEL, attach_converted_kit};

/// The single instrument-prefix the kit is loaded under.
const KIT: &str = "kit";
/// Default library root if `$SIGNAL_DRUM_LIBRARY` is unset.
const DEFAULT_LIBRARY: &str =
    "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2";

/// Meter-stream publish interval (~30 Hz).
const PUMP_MS: u64 = 33;

fn to_drum_map(m: InputMap) -> Option<DrumMap> {
    match m {
        InputMap::Direct => None,
        InputMap::StrataPrime => Some(DrumMap::StrataPrime),
        InputMap::Fts => Some(DrumMap::Fts),
        InputMap::Ggd => Some(DrumMap::Ggd),
    }
}

#[derive(Default)]
struct State {
    kits: Vec<KitInfo>,
    loaded: Option<usize>,
    pieces: Vec<PieceInfo>,
    /// Full engine instrument ids ("kit:kick", …) for preload polling.
    piece_ids: Vec<String>,
    input_map: InputMap,
    midi_port: Option<String>,
    midi_handle: Option<MidiInputHandle>,
    recent_midi: VecDeque<String>,
}

struct Inner {
    rig: Mutex<Option<SamplerRig>>,
    state: Mutex<State>,
    events: PubSub<DrumEvent>,
    library_dir: PathBuf,
}

/// The drum-rig backend handle. Cheap to clone (all state is shared); every
/// clone dispatches to the same core.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct DrumRigBackend {
    inner: Arc<Inner>,
}

impl Default for DrumRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DrumRigBackend {
    /// Build the backend and scan the kit library. Does not open audio — call
    /// [`DrumRig::start`] (which spawns the open off-thread) or `load_kit`.
    pub fn new() -> Self {
        let library_dir = std::env::var("SIGNAL_DRUM_LIBRARY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_LIBRARY));
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                state: Mutex::new(State::default()),
                events: PubSub::sliding(64),
                library_dir,
            }),
        };
        backend.rescan_library();
        backend
    }

    /// The composed service router — mount this on a vox transport.
    pub fn router(&self) -> LayerRouter {
        self.clone().into_router()
    }

    /// Start the meter pump + open audio in the background.
    pub fn start_background(&self) {
        self.spawn_meter_pump();
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-rig-open".into())
            .spawn(move || b.open_blocking());
    }

    // ── library ──
    fn rescan_library(&self) {
        let mut kits = Vec::new();
        collect_presets(&self.inner.library_dir, &mut kits);
        kits.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let loaded = self.inner.state.lock().ok().and_then(|s| s.loaded);
        if let Some(li) = loaded {
            if let Some(k) = kits.get_mut(li) {
                k.loaded = true;
            }
        }
        if let Ok(mut s) = self.inner.state.lock() {
            s.kits = kits;
        }
    }

    fn open_blocking(&self) {
        {
            let mut rig = self.inner.rig.lock().unwrap();
            if rig.is_none() {
                match SamplerRig::new() {
                    Ok(r) => {
                        r.set_preload_profile(PreloadProfile::DrumKit);
                        *rig = Some(r);
                    }
                    Err(e) => {
                        tracing::error!("drum rig: audio open failed: {e}");
                        return;
                    }
                }
            }
        }
        // Reload the previously-selected kit (if any) into the fresh rig.
        let idx = self.inner.state.lock().ok().and_then(|s| s.loaded);
        if let Some(idx) = idx {
            self.do_load_kit(idx);
        }
        self.reattach_midi();
        self.publish_all();
    }

    fn do_load_kit(&self, index: usize) {
        let path = {
            let s = self.inner.state.lock().unwrap();
            s.kits.get(index).map(|k| PathBuf::from(&k.path))
        };
        let Some(path) = path else { return };

        // Ensure audio is open.
        {
            let mut rig = self.inner.rig.lock().unwrap();
            if rig.is_none() {
                match SamplerRig::new() {
                    Ok(r) => {
                        r.set_preload_profile(PreloadProfile::DrumKit);
                        *rig = Some(r);
                    }
                    Err(e) => {
                        tracing::error!("drum rig: audio open failed: {e}");
                        return;
                    }
                }
            }
        }

        let (piece_ids, pieces) = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            match crate::load_preset_kit(rig, KIT, &path) {
                Ok(ids) => {
                    let pieces = pieces_from_preset(&path, &ids);
                    (ids, pieces)
                }
                Err(e) => {
                    tracing::error!("drum rig: load_kit failed: {e}");
                    return;
                }
            }
        };

        if let Ok(mut s) = self.inner.state.lock() {
            for (i, k) in s.kits.iter_mut().enumerate() {
                k.loaded = i == index;
            }
            s.loaded = Some(index);
            s.piece_ids = piece_ids;
            s.pieces = pieces;
        }
        self.reattach_midi();
        self.publish_all();
    }

    fn reattach_midi(&self) {
        let (port, map) = {
            let s = self.inner.state.lock().unwrap();
            (s.midi_port.clone(), s.input_map)
        };
        // Drop any existing connection first.
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_handle = None;
        }
        let Some(port) = port else { return };
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return };
        let sel = PortSelector::NameContains(port.clone());
        let handle = match to_drum_map(map) {
            None => rig.attach_midi(sel).map_err(|e| e.to_string()),
            Some(from) => attach_converted_kit(rig, sel, from, DrumMap::Mm2),
        };
        match handle {
            Ok(h) => {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
                tracing::info!(port = %port, "drum rig: MIDI attached");
            }
            Err(e) => tracing::error!("drum rig: MIDI attach failed: {e}"),
        }
    }

    // ── event publishing ──
    fn publish_all(&self) {
        self.inner.events.publish(DrumEvent::Library(DrumRig::kits(self)));
        self.inner.events.publish(DrumEvent::Kit(DrumRig::pieces(self)));
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    fn spawn_meter_pump(&self) {
        let backend = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-meter-pump".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(PUMP_MS));
                let running = backend.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
                if !running {
                    continue;
                }
                backend.inner.events.publish(DrumEvent::Status(DrumRig::status(&backend)));
                backend.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(&backend)));
            });
    }
}

// ── DrumRig service impl ────────────────────────────────────────────────────

impl DrumRig for DrumRigBackend {
    fn start(&self) {
        self.spawn_meter_pump();
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-rig-open".into())
            .spawn(move || b.open_blocking());
    }

    fn stop(&self) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_handle = None;
        }
        if let Ok(mut rig) = self.inner.rig.lock() {
            *rig = None;
        }
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    fn status(&self) -> DrumStatus {
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        let loaded_kit = s.loaded.and_then(|i| s.kits.get(i)).map(|k| k.name.clone());
        let (mut master_peak, mut voices) = (0.0f32, 0u32);
        let (mut loaded_n, mut total_n) = (0usize, 0usize);
        if running {
            let rig = self.inner.rig.lock().unwrap();
            if let Some(rig) = rig.as_ref() {
                if let Some(m) = rig.drum_mixer_meters(KIT) {
                    master_peak = m.master_peak();
                }
                voices = rig.active_voices(KIT) as u32;
                for id in &s.piece_ids {
                    let (l, t) = rig.preload_progress(id);
                    loaded_n += l;
                    total_n += t;
                }
            }
        }
        let preload = if total_n > 0 { loaded_n as f32 / total_n as f32 } else { 1.0 };
        DrumStatus {
            running,
            loaded_kit,
            master_peak,
            voices,
            midi_port: s.midi_port.clone(),
            input_map: s.input_map,
            preload,
        }
    }

    fn kits(&self) -> Vec<KitInfo> {
        self.inner.state.lock().map(|s| s.kits.clone()).unwrap_or_default()
    }

    fn load_kit(&self, index: u32) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-load-kit".into())
            .spawn(move || b.do_load_kit(index as usize));
    }

    fn pieces(&self) -> Vec<PieceInfo> {
        let mut pieces = self.inner.state.lock().map(|s| s.pieces.clone()).unwrap_or_default();
        // Freshen preload counts.
        let s = self.inner.state.lock().unwrap();
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                for (p, id) in pieces.iter_mut().zip(s.piece_ids.iter()) {
                    let (l, t) = rig.preload_progress(id);
                    p.loaded_samples = l as u32;
                    p.total_samples = t as u32;
                }
            }
        }
        pieces
    }

    fn trigger(&self, note: u32, velocity: u32) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.midi_message(GM_DRUM_CHANNEL, 0x90, note as u8, velocity as u8);
            }
        }
        if let Ok(mut s) = self.inner.state.lock() {
            push_recent(&mut s.recent_midi, format!("pad note {note} vel {velocity}"));
        }
    }

    fn mixer(&self) -> Vec<MixerStrip> {
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return Vec::new() };
        let Some(layout) = rig.drum_mixer_layout(KIT) else { return Vec::new() };
        let meters = rig.drum_mixer_meters(KIT);
        let mut strips = Vec::new();
        for eng in &layout.engines {
            for ch in &eng.channels {
                strips.push(MixerStrip {
                    kind: StripKind::Channel,
                    idx: ch.channel_idx as u32,
                    label: format!("{} {}", eng.label, ch.mic_label),
                    gain_db: ch.gain_db,
                    muted: ch.muted,
                    soloed: ch.soloed,
                    peak: meters.as_ref().map(|m| m.channel_peak(ch.channel_idx)).unwrap_or(0.0),
                });
            }
        }
        for bus in &layout.buses {
            strips.push(MixerStrip {
                kind: StripKind::Bus,
                idx: bus.bus_idx as u32,
                label: bus.label.clone(),
                gain_db: bus.gain_db,
                muted: bus.muted,
                soloed: bus.soloed,
                peak: meters.as_ref().map(|m| m.bus_peak(bus.bus_idx)).unwrap_or(0.0),
            });
        }
        strips
    }

    fn set_channel_gain(&self, idx: u32, db: f32) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_channel_gain_db(KIT, idx as usize, db);
            }
        }
    }
    fn set_channel_mute(&self, idx: u32, muted: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_channel_mute(KIT, idx as usize, muted);
            }
        }
    }
    fn set_channel_solo(&self, idx: u32, soloed: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_channel_solo(KIT, idx as usize, soloed);
            }
        }
    }
    fn set_bus_gain(&self, idx: u32, db: f32) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_bus_gain_db(KIT, idx as usize, db);
            }
        }
    }
    fn set_bus_mute(&self, idx: u32, muted: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_bus_mute(KIT, idx as usize, muted);
            }
        }
    }

    fn midi_ports(&self) -> Vec<String> {
        SamplerRig::midi_input_ports()
    }

    fn set_midi_port(&self, name: String) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_port = if name.is_empty() { None } else { Some(name) };
        }
        self.reattach_midi();
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    fn set_input_map(&self, map: InputMap) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.input_map = map;
        }
        self.reattach_midi();
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    fn midi_recent(&self) -> Vec<String> {
        self.inner
            .state
            .lock()
            .map(|s| s.recent_midi.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl DrumRigStreamSource for DrumRigBackend {
    fn events_hub(&self) -> &PubSub<DrumEvent> {
        &self.inner.events
    }
}

impl Services for DrumRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_drums_proto::drum::Service,
            signal_drums_proto::drum::StreamService
        ]
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn push_recent(q: &mut VecDeque<String>, s: String) {
    q.push_back(s);
    while q.len() > 64 {
        q.pop_front();
    }
}

/// Recursively collect `.signalpreset` files as [`KitInfo`].
fn collect_presets(dir: &Path, out: &mut Vec<KitInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_presets(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("signalpreset") {
            let name = PresetSpec::from_file(&path)
                .map(|p| p.name)
                .ok()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("kit").to_string()
                });
            out.push(KitInfo { name, path: path.display().to_string(), loaded: false });
        }
    }
}

/// Build [`PieceInfo`] from a preset's engines + note routing.
fn pieces_from_preset(path: &Path, ids: &[String]) -> Vec<PieceInfo> {
    let Ok(spec) = PresetSpec::from_file(path) else {
        return ids
            .iter()
            .map(|id| PieceInfo { id: id.clone(), ..Default::default() })
            .collect();
    };
    // engine id → first routed note.
    let note_of = |engine_id: &str| -> u32 {
        spec.note_routing
            .iter()
            .find(|nr| nr.targets.iter().any(|t| t == engine_id))
            .map(|nr| nr.note as u32)
            .unwrap_or(0)
    };
    spec.engines
        .iter()
        .map(|e| PieceInfo {
            id: e.id.clone(),
            note: note_of(&e.id),
            loaded_samples: 0,
            total_samples: 0,
        })
        .collect()
}
