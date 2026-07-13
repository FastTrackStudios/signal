//! Headless drum-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`SamplerRig`], loads GGD-style `.signalpreset` kits, plays them
//! from hardware MIDI (through the drum-map converter) or UI pads, and exposes
//! the multi-mic drum mixer. Implements the [`signal_drums_proto::drum::DrumRig`]
//! service + its `#[subscribe]` event stream; mount [`router`](DrumRigBackend::router)
//! on a vox transport (in-process, WebSocket, iroh).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, LayerRouter, PubSub, Services, layers};
use midicore::{Channel, DrumMap, DrumMapConverter, KeyNumber, MidiEvent, MidiMonitor, PortSelector,
    Velocity};
use signal_drums_proto::drum::{DrumEvent, DrumRig, DrumRigStreamSource};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo, StripKind};
use signal_sampler::{FxTarget, MidiInputHandle, PreloadProfile, PresetSpec, SamplerRig};

use crate::GM_DRUM_CHANNEL;

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
}

struct Inner {
    rig: Mutex<Option<SamplerRig>>,
    state: Mutex<State>,
    events: PubSub<DrumEvent>,
    library_dir: PathBuf,
    /// Rolling MIDI monitor (midicore's shared type) — the live MIDI callback
    /// records raw events; the UI renders them via `midicore-ui`.
    monitor: MidiMonitor,
    /// Optional Komplete Kontrol Light Guide mirroring the kit onto the keybed
    /// LEDs. `None` when no keyboard is attached / hidraw isn't accessible.
    light: Mutex<Option<crate::DrumLightGuide>>,
    /// One-shot guard so the meter pump is spawned exactly once, however the
    /// rig first becomes active (start / load_kit / open).
    pump_started: std::sync::atomic::AtomicBool,
    /// The sample-library catalog (every swappable `.signalengine`, grouped by
    /// kind on the client). Scanned once at construction.
    library: Vec<signal_drums_proto::LibraryPiece>,
    /// Available MM2 (Cradle) mix presets — `(name, path)` of `.preset` files
    /// under the library's `Mixes/` dir — whose per-strip level + FX we can
    /// import onto the loaded kit.
    mixes: Vec<(String, PathBuf)>,
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
        let library = crate::library::scan_engines(&crate::library::engines_dir(&library_dir));
        let mixes = scan_mixes(&library_dir.join("Mixes"));
        tracing::info!(pieces = library.len(), mixes = mixes.len(), "drum rig: scanned library");
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                // Default the hardware input to the FTS drum map (our e-kit
                // layout → the loaded kit), not raw Direct.
                state: Mutex::new(State { input_map: InputMap::Fts, ..State::default() }),
                events: PubSub::sliding(64),
                library_dir,
                monitor: MidiMonitor::new(),
                light: Mutex::new(None),
                pump_started: std::sync::atomic::AtomicBool::new(false),
                library,
                mixes,
            }),
        };
        backend.rescan_library();
        backend.spawn_meter_pump();
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
        self.paint_light_guide(); // opens the keyboard's Light Guide, paints the kit
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
        self.paint_light_guide();
        self.publish_all();
    }

    /// The `.signalpreset` path of the currently-loaded kit, if any.
    fn current_preset_path(&self) -> Option<PathBuf> {
        let s = self.inner.state.lock().ok()?;
        s.loaded.and_then(|i| s.kits.get(i)).map(|k| PathBuf::from(&k.path))
    }

    /// Swap one slot's engine to `engine_path` and reload the kit. The preset's
    /// note routing + slot ids are unchanged — only the sampled instrument in
    /// that slot differs — so pads/lights keep working; the mic set (hence the
    /// mixer strips) may change with the new engine, so we republish the mixer.
    fn do_swap_piece(&self, slot_id: String, engine_path: String) {
        let Some(path) = self.current_preset_path() else { return };
        let Ok(mut spec) = PresetSpec::from_file(&path) else { return };
        let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
        let mut found = false;
        for e in spec.engines.iter_mut() {
            if e.id == slot_id {
                e.engine = engine_path.clone();
                found = true;
            }
        }
        if !found {
            tracing::warn!(slot_id, "drum swap: no such slot");
            return;
        }
        {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            rig.set_preload_profile(PreloadProfile::DrumKit);
            match rig.load_preset_spec(KIT, &spec, &dir) {
                Ok(ids) => {
                    rig.set_midi_channel(KIT, GM_DRUM_CHANNEL);
                    rig.set_default_instrument(KIT);
                    if let Ok(mut s) = self.inner.state.lock() {
                        s.piece_ids = ids;
                    }
                }
                Err(e) => {
                    tracing::error!("drum swap: reload failed: {e}");
                    return;
                }
            }
        }
        self.paint_light_guide();
        self.publish_all();
    }

    /// Import an MM2 (Cradle) mix preset onto the loaded kit: reload the kit
    /// clean, then apply each MM2 strip's fader level + FX chain to the matching
    /// channel/bus (and the Master Bus chain to master). Strips match our mixer
    /// by name (Kick In 1, Snare Top, Overheads, Room Far, …).
    fn do_import_mix(&self, kit_index: usize, mix_path: PathBuf) {
        let mixer = match std::fs::read_to_string(&mix_path)
            .ok()
            .and_then(|t| crate::cradle::parse_mixer(&t).ok())
        {
            Some(m) => m,
            None => {
                tracing::error!("mm2 import: read/parse {}", mix_path.display());
                return;
            }
        };
        let path = {
            let s = self.inner.state.lock().unwrap();
            s.kits.get(kit_index).map(|k| PathBuf::from(&k.path))
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
        // Load the kit clean, then apply the mix onto it.
        let (piece_ids, pieces) = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            match crate::load_preset_kit(rig, KIT, &path) {
                Ok(ids) => {
                    let pieces = pieces_from_preset(&path, &ids);
                    self.apply_mix(rig, &mixer);
                    (ids, pieces)
                }
                Err(e) => {
                    tracing::error!("mm2 import: kit load failed: {e}");
                    return;
                }
            }
        };
        if let Ok(mut s) = self.inner.state.lock() {
            for (i, k) in s.kits.iter_mut().enumerate() {
                k.loaded = i == kit_index;
            }
            s.loaded = Some(kit_index);
            s.piece_ids = piece_ids;
            s.pieces = pieces;
        }
        self.reattach_midi();
        self.paint_light_guide();
        self.publish_all();
    }

    /// Apply a parsed MM2 mix onto the already-loaded kit (caller holds the rig
    /// lock; the kit was just reloaded clean). Sets each MM2 strip's fader level
    /// and installs its FX chain on the matching channel/bus/master.
    fn apply_mix(&self, rig: &SamplerRig, mixer: &crate::cradle::Mixer) {
        let sr = rig.sample_rate() as f64;
        let Some(layout) = rig.drum_mixer_layout(KIT) else { return };
        let mut fx_applied = 0usize;
        for eng in &layout.engines {
            for ch in &eng.channels {
                let piece = crate::library::slot_label(&eng.label);
                let target = if ch.mic_label.is_empty() {
                    piece
                } else {
                    format!("{} {}", piece, ch.mic_label)
                };
                let Some(strip) = crate::mm2fx::match_strip(mixer, &target) else { continue };
                rig.set_mixer_channel_gain_db(KIT, ch.channel_idx, crate::mm2fx::level_to_db(strip.level));
                for fx in strip.fx_slots() {
                    if let Some(p) = crate::mm2fx::build_processor(&fx, sr) {
                        if rig.install_mixer_plugin(KIT, FxTarget::Channel(ch.channel_idx), p).is_ok() {
                            fx_applied += 1;
                        }
                    }
                }
            }
        }
        for bus in &layout.buses {
            let Some(strip) = crate::mm2fx::match_strip(mixer, &bus.label) else { continue };
            rig.set_mixer_bus_gain_db(KIT, bus.bus_idx, crate::mm2fx::level_to_db(strip.level));
            for fx in strip.fx_slots() {
                if let Some(p) = crate::mm2fx::build_processor(&fx, sr) {
                    if rig.install_mixer_plugin(KIT, FxTarget::Bus(bus.bus_idx), p).is_ok() {
                        fx_applied += 1;
                    }
                }
            }
        }
        if let Some(strip) = crate::mm2fx::match_strip(mixer, "Master Bus") {
            for fx in strip.fx_slots() {
                if let Some(p) = crate::mm2fx::build_processor(&fx, sr) {
                    if rig.install_mixer_plugin(KIT, FxTarget::Master, p).is_ok() {
                        fx_applied += 1;
                    }
                }
            }
        }
        tracing::info!(fx_applied, strips = mixer.strips.len(), "mm2 import: applied mix");
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
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return };
        // Default to omni: no named port → merge *all* MIDI inputs (PipeWire
        // fans every device into one stream). A named port narrows to it.
        let sel = match &port {
            Some(name) => PortSelector::NameContains(name.clone()),
            None => PortSelector::All,
        };
        // One transform closure: record the raw event into the monitor, then
        // (optionally) run the drum-map converter. Recording the pre-conversion
        // event shows what the hardware actually sent.
        let inner = self.inner.clone();
        let mut conv = to_drum_map(map).map(|from| DrumMapConverter::new(from, DrumMap::Mm2));
        let handle = rig
            .attach_midi_transformed(sel, move |ev| {
                inner.monitor.record(&ev);
                // Flash the played key on the Light Guide (the raw/physical key
                // the player pressed — for a Direct-mapped keyboard that's the
                // kit note; for a converted kit it's still the key they touched).
                if let MidiEvent::NoteOn { key, velocity, .. } = &ev {
                    if velocity.get() > 0 {
                        if let Ok(mut l) = inner.light.lock() {
                            if let Some(lg) = l.as_mut() {
                                lg.note_on(key.get());
                            }
                        }
                    }
                }
                match conv.as_mut() {
                    Some(c) => c.convert(ev),
                    None => vec![ev],
                }
            })
            .map_err(|e| e.to_string());
        match handle {
            Ok(h) => {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
                let which = port.as_deref().unwrap_or("omni (all inputs)");
                tracing::info!(port = %which, "drum rig: MIDI attached");
            }
            Err(e) => tracing::error!("drum rig: MIDI attach failed: {e}"),
        }
    }

    // ── event publishing ──
    fn publish_all(&self) {
        self.inner.events.publish(DrumEvent::Library(DrumRig::kits(self)));
        self.inner.events.publish(DrumEvent::Kit(DrumRig::pieces(self)));
        self.inner.events.publish(DrumEvent::Design(DrumRig::kit_slots(self)));
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    /// Open the Light Guide (if a keyboard is attached) and paint the current
    /// kit's per-piece colours onto it. Safe no-op when no keyboard.
    fn paint_light_guide(&self) {
        let pieces: Vec<(u8, String)> = {
            let s = self.inner.state.lock().unwrap();
            s.pieces.iter().map(|p| (p.note as u8, p.id.clone())).collect()
        };
        if let Ok(mut l) = self.inner.light.lock() {
            if l.is_none() {
                *l = crate::DrumLightGuide::open();
            }
            if let Some(lg) = l.as_mut() {
                lg.set_kit(&pieces);
            }
        }
    }

    /// Spawn the meter pump — idempotent (only the first call wins), so the
    /// rig streams meters + MIDI however it first becomes active. The pump
    /// publishes only high-rate data (`Meters`, `Midi`); the control surface
    /// (`Mixer`) and transport (`Status`) are published on mutation instead,
    /// so a fader the user is dragging is never overwritten at meter rate.
    fn spawn_meter_pump(&self) {
        use std::sync::atomic::Ordering;
        if self.inner.pump_started.swap(true, Ordering::SeqCst) {
            return; // already running
        }
        let backend = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-meter-pump".into())
            .spawn(move || {
                let mut last_running = false;
                // MIDI hot-plug watch: snapshot the port set and re-attach when
                // it changes, so a device plugged in after the rig started
                // (e.g. the mioXM) is picked up without touching the UI.
                let sorted_ports = || {
                    let mut p = SamplerRig::midi_input_ports();
                    p.sort();
                    p
                };
                let mut known_ports = sorted_ports();
                let mut tick: u32 = 0;
                // ~every 2 s (60 * 33 ms).
                const PORT_SCAN_TICKS: u32 = 60;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(PUMP_MS));
                    // Decay Light Guide flashes even while stopped (cheap no-op
                    // if no keyboard).
                    if let Ok(mut l) = backend.inner.light.lock() {
                        if let Some(lg) = l.as_mut() {
                            lg.tick();
                        }
                    }
                    let running = backend.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
                    // Re-scan MIDI ports periodically; on a change, re-attach so
                    // hot-plugged interfaces are merged into the omni stream.
                    tick = tick.wrapping_add(1);
                    if tick % PORT_SCAN_TICKS == 0 {
                        let now = sorted_ports();
                        if now != known_ports {
                            tracing::info!(ports = ?now, "drum rig: MIDI ports changed — re-attaching");
                            known_ports = now;
                            if running {
                                backend.reattach_midi();
                                backend.inner.events.publish(DrumEvent::Status(DrumRig::status(&backend)));
                            }
                        }
                    }
                    // Transport transitions are rare — publish Status only on the
                    // edge, not every tick.
                    if running != last_running {
                        last_running = running;
                        backend.inner.events.publish(DrumEvent::Status(DrumRig::status(&backend)));
                        backend.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(&backend)));
                    }
                    if !running {
                        continue;
                    }
                    backend.inner.events.publish(DrumEvent::Meters(DrumRig::meters(&backend)));
                    backend.inner.events.publish(DrumEvent::Midi(DrumRig::midi_recent(&backend)));
                }
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

    fn kit_slots(&self) -> Vec<signal_drums_proto::KitSlot> {
        let Some(path) = self.current_preset_path() else { return Vec::new() };
        let Ok(spec) = PresetSpec::from_file(&path) else { return Vec::new() };
        let dir = path.parent().unwrap_or(Path::new(""));
        crate::library::preset_slots(&spec, dir)
            .into_iter()
            .map(|(id, abs)| {
                let abs_str = abs.display().to_string();
                let lib = self.inner.library.iter().find(|p| p.path == abs_str);
                let current_name = lib
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| abs.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string());
                let kind = lib
                    .map(|p| p.kind.clone())
                    .filter(|k| !k.is_empty())
                    .unwrap_or_else(|| crate::library::kind_from_slot(&id).to_string());
                signal_drums_proto::KitSlot {
                    label: crate::library::slot_label(&id),
                    kind,
                    current_name,
                    current_path: abs_str,
                    slot_id: id,
                }
            })
            .collect()
    }

    fn library(&self) -> Vec<signal_drums_proto::LibraryPiece> {
        self.inner.library.clone()
    }

    fn swap_piece(&self, slot_id: String, engine_path: String) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-swap".into())
            .spawn(move || b.do_swap_piece(slot_id, engine_path));
    }

    fn mm2_mixes(&self) -> Vec<String> {
        self.inner.mixes.iter().map(|(n, _)| n.clone()).collect()
    }

    fn import_mm2_mix(&self, name: String) {
        let mix_path = self.inner.mixes.iter().find(|(n, _)| *n == name).map(|(_, p)| p.clone());
        let Some(mix_path) = mix_path else {
            tracing::warn!(name, "mm2 import: no such mix");
            return;
        };
        // A preset = kit + mix under one name: pick the kit whose name matches
        // this mix (Metal monster → the Metal Monster kit); "Default" and any
        // unmatched mix keep the current kit (else the first available).
        let kit_index = {
            let s = self.inner.state.lock().unwrap();
            // Normalize away punctuation/spacing so "80's Meet Now-ies" (mix)
            // matches "80s Meet Now-ies" (kit).
            let want = norm_name(&name);
            let by_name = s.kits.iter().position(|k| norm_name(&k.name) == want);
            by_name
                .filter(|_| !name.eq_ignore_ascii_case("Default"))
                .or(s.loaded)
                .or(if s.kits.is_empty() { None } else { Some(0) })
        };
        let Some(kit_index) = kit_index else {
            tracing::warn!("mm2 import: no kit to load");
            return;
        };
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("drum-mm2-import".into())
            .spawn(move || b.do_import_mix(kit_index, mix_path));
    }

    fn trigger(&self, note: u32, velocity: u32) {
        let (note, velocity) = (note as u8, velocity as u8);
        // Inject as live MIDI on the GM percussion channel — the same
        // `push_live_midi` path hardware uses — so a UI click/pad is a
        // first-class event in the system (plays, and any live-MIDI consumer
        // sees it). Velocity 0 is a note-off (running-status convention).
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.midi_message(GM_DRUM_CHANNEL, 0x90, note, velocity);
            }
        }
        let channel = Channel::new(GM_DRUM_CHANNEL);
        let key = KeyNumber::new(note);
        let ev = if velocity > 0 {
            MidiEvent::NoteOn { channel, key, velocity: Velocity::new(velocity) }
        } else {
            MidiEvent::NoteOff { channel, key, velocity: Velocity::new(0) }
        };
        self.inner.monitor.record(&ev);
        if velocity > 0 {
            if let Ok(mut l) = self.inner.light.lock() {
                if let Some(lg) = l.as_mut() {
                    lg.note_on(note);
                }
            }
        }
    }

    fn mixer(&self) -> Vec<MixerStrip> {
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return Vec::new() };
        let Some(layout) = rig.drum_mixer_layout(KIT) else { return Vec::new() };
        let meters = rig.drum_mixer_meters(KIT);
        let mut strips = Vec::new();
        // Hierarchy: each piece is a folder (its fader sums the mics) followed
        // by its mic channels (Kick In 1/2/Sub …); then the shared OH/room
        // buses. Same order as `meters()` so peak indices line up.
        for eng in &layout.engines {
            let sends = eng
                .sends
                .iter()
                .map(|s| signal_drums_proto::SendInfo {
                    idx: s.send_idx as u32,
                    bus_label: s.bus_label.clone(),
                    level_db: s.level_db,
                })
                .collect();
            strips.push(MixerStrip {
                kind: StripKind::Piece,
                idx: eng.engine_idx as u32,
                label: eng.label.clone(),
                group: String::new(),
                gain_db: eng.piece_gain_db,
                muted: eng.piece_muted,
                soloed: eng.piece_soloed,
                peak: meters.as_ref().map(|m| m.piece_peak(eng.engine_idx)).unwrap_or(0.0),
                sends,
            });
            for ch in &eng.channels {
                strips.push(MixerStrip {
                    kind: StripKind::Channel,
                    idx: ch.channel_idx as u32,
                    label: if ch.mic_label.is_empty() { eng.label.clone() } else { ch.mic_label.clone() },
                    group: eng.label.clone(),
                    gain_db: ch.gain_db,
                    muted: ch.muted,
                    soloed: ch.soloed,
                    peak: meters.as_ref().map(|m| m.channel_peak(ch.channel_idx)).unwrap_or(0.0),
                    sends: Vec::new(),
                });
            }
        }
        for bus in &layout.buses {
            strips.push(MixerStrip {
                kind: StripKind::Bus,
                idx: bus.bus_idx as u32,
                label: bus.label.clone(),
                group: String::new(),
                gain_db: bus.gain_db,
                muted: bus.muted,
                soloed: bus.soloed,
                peak: meters.as_ref().map(|m| m.bus_peak(bus.bus_idx)).unwrap_or(0.0),
                sends: Vec::new(),
            });
        }
        strips
    }

    fn meters(&self) -> signal_drums_proto::MeterSnapshot {
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return Default::default() };
        let Some(m) = rig.drum_mixer_meters(KIT) else { return Default::default() };
        // Positionally aligned with `mixer()`: per engine [piece, channels…],
        // then buses.
        let mut strips = Vec::new();
        if let Some(layout) = rig.drum_mixer_layout(KIT) {
            for eng in &layout.engines {
                strips.push(m.piece_peak(eng.engine_idx));
                for ch in &eng.channels {
                    strips.push(m.channel_peak(ch.channel_idx));
                }
            }
            for bus in &layout.buses {
                strips.push(m.bus_peak(bus.bus_idx));
            }
        }
        signal_drums_proto::MeterSnapshot {
            master: m.master_peak(),
            strips,
            voices: rig.active_voices(KIT) as u32,
        }
    }

    fn set_piece_gain(&self, idx: u32, db: f32) {
        // Continuous control — no Mixer echo (the UI tracks it optimistically);
        // echoing at drag rate is what made the fader jitter.
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_piece_gain_db(KIT, idx as usize, db);
            }
        }
    }
    fn set_piece_mute(&self, idx: u32, muted: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_piece_mute(KIT, idx as usize, muted);
            }
        }
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_piece_solo(&self, idx: u32, soloed: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_piece_solo(KIT, idx as usize, soloed);
            }
        }
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_bus_solo(&self, idx: u32, soloed: bool) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_bus_solo(KIT, idx as usize, soloed);
            }
        }
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_send_level(&self, idx: u32, db: f32) {
        // Continuous — no Mixer echo (see set_piece_gain).
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.set_mixer_send_level_db(KIT, idx as usize, db);
            }
        }
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

    fn midi_recent(&self) -> Vec<MidiEvent> {
        self.inner.monitor.recent()
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

/// Normalize a preset/kit name for matching: lowercase, alphanumerics only
/// (drops apostrophes, hyphens, spaces) so "80's Meet Now-ies" == "80s Meet
/// Now-ies".
fn norm_name(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// Collect `.preset` (MM2 Cradle) mix files under `dir` as `(name, path)`.
fn scan_mixes(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("preset") {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("mix").to_string();
                out.push((name, path));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
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
