//! Headless drum-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`SamplerRig`], loads GGD-style `.signalpreset` kits, plays them
//! from hardware MIDI (through the drum-map converter) or UI pads, and exposes
//! the multi-mic drum mixer. Implements the [`signal_drums_proto::drum::DrumRig`]
//! service + its `#[subscribe]` event stream; mount `router()` (`architect::rig::RigBackend`)
//! on a vox transport (in-process, WebSocket, iroh).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use midicore::{Channel, DrumMap, DrumMapConverter, KeyNumber, MidiEvent, MidiMonitor, Velocity};
use signal_drums_proto::drum::{DrumEvent, DrumRig, DrumRigStreamSource};
use signal_drums_proto::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo, StripKind};
use signal_rig_host::mixer::db_to_linear;
use signal_sampler::{MidiInputHandle, PreloadProfile, PresetSpec, SamplerRig};

use crate::GM_DRUM_CHANNEL;

/// The single instrument-prefix the kit is loaded under.
const KIT: &str = "kit";
/// Default library root if `$SIGNAL_DRUM_LIBRARY` is unset.
const DEFAULT_LIBRARY: &str =
    "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2";

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
    /// The engine actually loaded in each slot `(slot_id, abs .signalengine
    /// path)` — the source of truth for the kit designer + kit view, so a swap
    /// is reflected consistently everywhere (re-reading the preset file would
    /// show the pre-swap engine).
    engines: Vec<(String, PathBuf)>,
    input_map: InputMap,
    midi_port: Option<String>,
    /// Per-strip mixer state over the kit's daw tracks (the backend owns the
    /// strip model; every fader/mute/solo lands on a daw track / send).
    mix: KitMix,
    midi_handle: Option<MidiInputHandle>,
}

/// One strip's fader/mute/solo.
#[derive(Clone, Copy, Debug, Default)]
struct StripState {
    gain_db: f32,
    muted: bool,
    soloed: bool,
}

/// A close-mic channel over its daw track.
#[derive(Clone, Debug)]
struct ChanRef {
    piece: usize,
    mic: String,
    track: String,
    meter: usize,
    state: StripState,
}

/// A bus-mic send over its daw track (one send, route index 0, into `bus`).
#[derive(Clone, Debug)]
struct SendRef {
    piece: usize,
    #[allow(dead_code)]
    mic: String,
    track: String,
    meter: usize,
    bus: usize,
    level_db: f32,
    muted: bool,
    soloed: bool,
}

/// A shared bus over its daw track.
#[derive(Clone, Debug)]
struct BusRef {
    label: String,
    track: String,
    meter: usize,
    state: StripState,
}

/// One piece strip (the folder-level fader over its mic tracks).
#[derive(Clone, Debug)]
struct PieceStrip {
    id: String,
    label: String,
    state: StripState,
}

/// The whole kit mixer as daw-track references — rebuilt on every kit load.
#[derive(Clone, Debug, Default)]
struct KitMix {
    pieces: Vec<PieceStrip>,
    channels: Vec<ChanRef>,
    sends: Vec<SendRef>,
    buses: Vec<BusRef>,
}

impl KitMix {
    /// Build the strip model from the loaded kit's track set.
    fn from_kit(kit: &signal_sampler::kit_tracks::KitState) -> Self {
        let mut mix = KitMix::default();
        let bus_index: std::collections::HashMap<&str, usize> = kit
            .buses
            .iter()
            .enumerate()
            .map(|(i, (id, _, _))| (id.as_str(), i))
            .collect();
        for (pi, piece) in kit.pieces.iter().enumerate() {
            mix.pieces.push(PieceStrip {
                id: piece.id.clone(),
                label: crate::library::slot_label(&piece.id),
                state: StripState { muted: piece.muted, ..StripState::default() },
            });
            for mic in &piece.mics {
                match &mic.bus {
                    None => mix.channels.push(ChanRef {
                        piece: pi,
                        mic: mic.mic.clone(),
                        track: mic.track_guid.clone(),
                        meter: mic.meter_index,
                        state: StripState::default(),
                    }),
                    Some(bus) => mix.sends.push(SendRef {
                        piece: pi,
                        mic: mic.mic.clone(),
                        track: mic.track_guid.clone(),
                        meter: mic.meter_index,
                        bus: bus_index.get(bus.as_str()).copied().unwrap_or(0),
                        level_db: 0.0,
                        muted: false,
                        soloed: false,
                    }),
                }
            }
        }
        for (label, track, meter) in &kit.buses {
            mix.buses.push(BusRef {
                label: label.clone(),
                track: track.clone(),
                meter: *meter,
                state: StripState::default(),
            });
        }
        mix
    }

    fn any_solo(&self) -> bool {
        self.pieces.iter().any(|p| p.state.soloed)
            || self.channels.iter().any(|c| c.state.soloed)
            || self.sends.iter().any(|s| s.soloed)
            || self.buses.iter().any(|b| b.state.soloed)
    }
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
                events: architect::rig::events_hub(),
                library_dir,
                monitor: MidiMonitor::new(),
                light: Mutex::new(None),
                pump_started: std::sync::atomic::AtomicBool::new(false),
                library,
                mixes,
            }),
        };
        backend.rescan_library();
        backend.spawn_meter_pump("drum-meter-pump");
        backend
    }

    /// Start the meter pump + open audio in the background.
    pub fn start_background(&self) {
        self.spawn_meter_pump("drum-meter-pump");
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
            match crate::load_kit_tracks(rig, KIT, &path) {
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
        self.rebuild_mix_state();

        let engines = engines_from_preset(&path);
        if let Ok(mut s) = self.inner.state.lock() {
            for (i, k) in s.kits.iter_mut().enumerate() {
                k.loaded = i == index;
            }
            s.loaded = Some(index);
            s.piece_ids = piece_ids;
            s.pieces = pieces;
            s.engines = engines;
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
            match crate::load_kit_tracks_spec(rig, KIT, &spec, &dir) {
                Ok(ids) => {
                    let abs = {
                        let p = PathBuf::from(&engine_path);
                        if p.is_absolute() { p } else { dir.join(p) }
                    };
                    if let Ok(mut s) = self.inner.state.lock() {
                        s.piece_ids = ids;
                        // Reflect the swap in the loaded-engine map (so the kit
                        // designer + kit view show the new instrument).
                        if let Some(e) = s.engines.iter_mut().find(|(id, _)| *id == slot_id) {
                            e.1 = abs;
                        }
                    }
                    self.rebuild_mix_state();
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
            match crate::load_kit_tracks(rig, KIT, &path) {
                Ok(ids) => {
                    let pieces = pieces_from_preset(&path, &ids);
                    (ids, pieces)
                }
                Err(e) => {
                    tracing::error!("mm2 import: kit load failed: {e}");
                    return;
                }
            }
        };
        self.rebuild_mix_state();
        {
            let rig = self.inner.rig.lock().unwrap();
            if let Some(rig) = rig.as_ref() {
                self.apply_mix(rig, &mixer);
            }
        }
        let engines = engines_from_preset(&path);
        if let Ok(mut s) = self.inner.state.lock() {
            for (i, k) in s.kits.iter_mut().enumerate() {
                k.loaded = i == kit_index;
            }
            s.loaded = Some(kit_index);
            s.piece_ids = piece_ids;
            s.pieces = pieces;
            s.engines = engines;
        }
        self.reattach_midi();
        self.paint_light_guide();
        self.publish_all();
    }

    /// Apply a parsed MM2 mix onto the already-loaded per-track kit: each
    /// matching strip's fader level lands in the strip model (then on its daw
    /// track), and its FX chain is inserted as extra fx slots on that track.
    /// The Master Bus chain has no daw master fx chain yet — logged, skipped.
    fn apply_mix(&self, rig: &SamplerRig, mixer: &crate::cradle::Mixer) {
        use daw::service::handle::DawHandle as _;
        let sr = rig.sample_rate() as f64;
        let Some(daw) = rig.daw_handle() else { return };
        let project = daw.current();
        let mut fx_applied = 0usize;
        let (channels, buses, piece_labels) = {
            let s = self.inner.state.lock().unwrap();
            (
                s.mix.channels.clone(),
                s.mix.buses.clone(),
                s.mix.pieces.iter().map(|p| p.label.clone()).collect::<Vec<_>>(),
            )
        };
        let add_fx = |track: &str, label: &str, plugin: Box<dyn signal_plugin_host::PluginInstance>| {
            let Ok(slot) = project.track(track).add_fx_slot(label) else {
                return false;
            };
            let mut boxed = plugin;
            let _ = signal_plugin_host::PluginInstance::prepare(boxed.as_mut(), sr, 1024);
            daw.insert_plugin_instance(slot.into_guid(), boxed);
            true
        };
        for (ci, ch) in channels.iter().enumerate() {
            let piece = piece_labels.get(ch.piece).cloned().unwrap_or_default();
            let target = if ch.mic.is_empty() {
                piece
            } else {
                format!("{} {}", piece, ch.mic)
            };
            let Some(strip) = crate::mm2fx::match_strip(mixer, &target) else { continue };
            let db = crate::mm2fx::level_to_db(strip.level);
            if let Ok(mut st) = self.inner.state.lock() {
                if let Some(c) = st.mix.channels.get_mut(ci) {
                    c.state.gain_db = db;
                }
            }
            for fx in strip.fx_slots() {
                if let Some(p) = crate::mm2fx::build_instance(&fx, sr) {
                    if add_fx(&ch.track, &format!("mm2-{target}"), p) {
                        fx_applied += 1;
                    }
                }
            }
        }
        for (bi, bus) in buses.iter().enumerate() {
            let Some(strip) = crate::mm2fx::match_strip(mixer, &bus.label) else { continue };
            let db = crate::mm2fx::level_to_db(strip.level);
            if let Ok(mut st) = self.inner.state.lock() {
                if let Some(b) = st.mix.buses.get_mut(bi) {
                    b.state.gain_db = db;
                }
            }
            for fx in strip.fx_slots() {
                if let Some(p) = crate::mm2fx::build_instance(&fx, sr) {
                    if add_fx(&bus.track, &format!("mm2-{}", bus.label), p) {
                        fx_applied += 1;
                    }
                }
            }
        }
        if crate::mm2fx::match_strip(mixer, "Master Bus").is_some() {
            tracing::warn!("mm2 import: Master Bus FX not applied (no daw master fx chain yet)");
        }
        self.apply_kit_mixer();
        tracing::info!(fx_applied, strips = mixer.strips.len(), "mm2 import: applied mix");
    }

    /// Rebuild the strip model from the freshly loaded kit's tracks.
    fn rebuild_mix_state(&self) {
        let mix = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            rig.with_kit(KitMix::from_kit)
        };
        if let (Some(mix), Ok(mut s)) = (mix, self.inner.state.lock()) {
            s.mix = mix;
        }
        self.apply_kit_mixer();
    }

    /// Push the whole strip model onto the daw tracks: close mics get
    /// piece+channel fader/mute/solo folded; bus mics carry the piece fader
    /// on the track and the send level on the route; buses get their own
    /// strip. Buses are solo-passed whenever one of their senders is soloed
    /// (a send destination is not an ancestor, so the renderer's solo mask
    /// alone would silence it).
    fn apply_kit_mixer(&self) {
        use daw::service::handle::DawHandle as _;
        use daw::service::{RouteLocation, RouteRef, Routing, TrackRef};
        use daw::standalone::Standalone;
        let daw = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            let Some(daw) = rig.daw_handle() else { return };
            daw
        };
        let project = daw.current();
        let s = self.inner.state.lock().unwrap();
        let mix = &s.mix;
        let vol = |db: f32| db_to_linear(db) as f64;
        for ch in &mix.channels {
            let piece = mix.pieces.get(ch.piece).map(|p| p.state).unwrap_or_default();
            let track = project.track(&ch.track);
            let _ = track.set_volume(vol(piece.gain_db + ch.state.gain_db));
            let _ = track.mute(piece.muted || ch.state.muted);
            let _ = track.solo(piece.soloed || ch.state.soloed);
        }
        for snd in &mix.sends {
            let piece = mix.pieces.get(snd.piece).map(|p| p.state).unwrap_or_default();
            let track = project.track(&snd.track);
            let _ = track.set_volume(vol(piece.gain_db));
            let _ = track.mute(piece.muted || snd.muted);
            let _ = track.solo(piece.soloed || snd.soloed);
            // The send level rides the route, not the track fader.
            let _ = <Standalone as Routing>::set_volume(
                &daw,
                project.context(),
                RouteLocation::send(TrackRef::guid(&snd.track), RouteRef::Index(0)),
                vol(snd.level_db),
            );
        }
        let any_solo = mix.any_solo();
        for (bi, bus) in mix.buses.iter().enumerate() {
            let sender_solo = mix.sends.iter().any(|snd| {
                snd.bus == bi
                    && (snd.soloed
                        || mix.pieces.get(snd.piece).map(|p| p.state.soloed).unwrap_or(false))
            });
            let track = project.track(&bus.track);
            let _ = track.set_volume(vol(bus.state.gain_db));
            let _ = track.mute(bus.state.muted);
            let _ = track.solo(bus.state.soloed || (any_solo && sender_solo));
        }
    }

    fn reattach_midi(&self) {
        let (port, map) = {
            let s = self.inner.state.lock().unwrap();
            (s.midi_port.clone(), s.input_map)
        };
        midicore::attach::reattach(
            "drum rig",
            port.as_deref(),
            || {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = None;
                }
            },
            |sel| {
                // Clone the rig handle out of the lock (cheap Arc clone)
                // BEFORE opening MIDI ports: a multi-port interface like the
                // mioXM takes ~2 s to open all its ports, and holding the rig
                // lock that long would freeze every RPC handler
                // (status/kit_slots/mixer) → the remote UI times out and
                // shows "no kit". Cloning lets those keep serving during the
                // attach.
                let rig = {
                    let g = self.inner.rig.lock().unwrap();
                    match g.as_ref() {
                        Some(r) => r.clone(),
                        None => return Ok(None),
                    }
                };
                // One transform closure: record the raw event into the
                // monitor, then (optionally) run the drum-map converter.
                // Recording the pre-conversion event shows what the hardware
                // actually sent.
                let inner = self.inner.clone();
                let mut conv =
                    to_drum_map(map).map(|from| DrumMapConverter::new(from, DrumMap::Mm2));
                rig.attach_midi_kit(sel, move |ev| {
                    inner.monitor.record(&ev);
                    // Flash the played key on the Light Guide (the raw/physical
                    // key the player pressed — for a Direct-mapped keyboard
                    // that's the kit note; for a converted kit it's still the
                    // key they touched).
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
                .map(Some)
            },
            |h| {
                if let Ok(mut s) = self.inner.state.lock() {
                    s.midi_handle = Some(h);
                }
            },
        );
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
}

// r[impl primitives.architect.rig-backend]
impl RigBackend for DrumRigBackend {
    type Event = DrumEvent;
    type Tick = ();

    fn events_hub(&self) -> &PubSub<DrumEvent> {
        &self.inner.events
    }

    fn is_running(&self) -> bool {
        self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false)
    }

    fn pump_started(&self) -> &std::sync::atomic::AtomicBool {
        &self.inner.pump_started
    }

    fn on_tick(&self, _tick: &mut ()) {
        // Decay Light Guide flashes even while stopped (cheap no-op if no
        // keyboard).
        if let Ok(mut l) = self.inner.light.lock() {
            if let Some(lg) = l.as_mut() {
                lg.tick();
            }
        }
    }

    fn midi_ports(&self) -> Vec<String> {
        SamplerRig::midi_input_ports()
    }

    fn on_midi_ports_changed(&self, ports: &[String]) {
        // A device plugged in after the rig started (e.g. the mioXM) is merged
        // into the omni stream without touching the UI.
        tracing::info!(?ports, "drum rig: MIDI ports changed — re-attaching");
        self.reattach_midi();
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
    }

    fn on_running_edge(&self, _running: bool) {
        // Transport transitions are rare — full Status + Mixer only on the edge.
        self.inner.events.publish(DrumEvent::Status(DrumRig::status(self)));
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }

    fn on_running_tick(&self) {
        self.inner.events.publish(DrumEvent::Meters(DrumRig::meters(self)));
        self.inner.events.publish(DrumEvent::Midi(DrumRig::midi_recent(self)));
    }
}

// ── DrumRig service impl ────────────────────────────────────────────────────

impl DrumRig for DrumRigBackend {
    fn start(&self) {
        self.spawn_meter_pump("drum-meter-pump");
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
                // Master ≈ the loudest strip (close mics + buses); the daw
                // master has no meter cell of its own yet.
                let meters = rig.meters_bank();
                let peak = |idx: usize| {
                    meters.cell(idx).map(|c| c.peak(0).max(c.peak(1))).unwrap_or(0.0)
                };
                for ch in &s.mix.channels {
                    master_peak = master_peak.max(peak(ch.meter));
                }
                for bus in &s.mix.buses {
                    master_peak = master_peak.max(peak(bus.meter));
                }
                voices = rig.kit_voices() as u32;
                for piece in &s.mix.pieces {
                    let (l, t) = rig.kit_piece_progress(&format!("{KIT}:{}", piece.id));
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
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                for p in pieces.iter_mut() {
                    let (l, t) = rig.kit_piece_progress(&p.id);
                    p.loaded_samples = l as u32;
                    p.total_samples = t as u32;
                }
            }
        }
        pieces
    }

    fn kit_slots(&self) -> Vec<signal_drums_proto::KitSlot> {
        // Source of truth = the engines actually loaded (reflects swaps), not
        // the on-disk preset file.
        let engines = self.inner.state.lock().map(|s| s.engines.clone()).unwrap_or_default();
        engines
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
                rig.kit_note(note, velocity);
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
        let meters = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return Vec::new() };
            rig.meters_bank()
        };
        let peak =
            |idx: usize| meters.cell(idx).map(|c| c.peak(0).max(c.peak(1))).unwrap_or(0.0);
        let s = self.inner.state.lock().unwrap();
        let mix = &s.mix;
        let mut strips = Vec::new();
        // Hierarchy: each piece is a folder-level strip (its fader rides all
        // its mic tracks) followed by its close-mic channels; then the shared
        // OH/room buses. Same order as `meters()` so peak indices line up.
        for (pi, piece) in mix.pieces.iter().enumerate() {
            let sends = mix
                .sends
                .iter()
                .enumerate()
                .filter(|(_, snd)| snd.piece == pi)
                .map(|(si, snd)| signal_drums_proto::SendInfo {
                    idx: si as u32,
                    bus_label: mix
                        .buses
                        .get(snd.bus)
                        .map(|b| b.label.clone())
                        .unwrap_or_default(),
                    level_db: snd.level_db,
                })
                .collect();
            // Piece peak = the loudest of its mic tracks this block.
            let piece_peak = mix
                .channels
                .iter()
                .filter(|c| c.piece == pi)
                .map(|c| c.meter)
                .chain(mix.sends.iter().filter(|sd| sd.piece == pi).map(|sd| sd.meter))
                .map(peak)
                .fold(0.0f32, f32::max);
            strips.push(MixerStrip {
                kind: StripKind::Piece,
                idx: pi as u32,
                label: piece.label.clone(),
                group: String::new(),
                gain_db: piece.state.gain_db,
                muted: piece.state.muted,
                soloed: piece.state.soloed,
                peak: piece_peak,
                sends,
            });
            for (ci, ch) in mix.channels.iter().enumerate().filter(|(_, c)| c.piece == pi) {
                strips.push(MixerStrip {
                    kind: StripKind::Channel,
                    idx: ci as u32,
                    label: if ch.mic.is_empty() { piece.label.clone() } else { ch.mic.clone() },
                    group: piece.label.clone(),
                    gain_db: ch.state.gain_db,
                    muted: ch.state.muted,
                    soloed: ch.state.soloed,
                    peak: peak(ch.meter),
                    sends: Vec::new(),
                });
            }
        }
        for (bi, bus) in mix.buses.iter().enumerate() {
            strips.push(MixerStrip {
                kind: StripKind::Bus,
                idx: bi as u32,
                label: bus.label.clone(),
                group: String::new(),
                gain_db: bus.state.gain_db,
                muted: bus.state.muted,
                soloed: bus.state.soloed,
                peak: peak(bus.meter),
                sends: Vec::new(),
            });
        }
        strips
    }

    fn meters(&self) -> signal_drums_proto::MeterSnapshot {
        let (meters, voices) = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return Default::default() };
            (rig.meters_bank(), rig.kit_voices() as u32)
        };
        let peak =
            |idx: usize| meters.cell(idx).map(|c| c.peak(0).max(c.peak(1))).unwrap_or(0.0);
        let s = self.inner.state.lock().unwrap();
        let mix = &s.mix;
        // Positionally aligned with `mixer()`: per piece [piece, channels…],
        // then buses.
        let mut strips = Vec::new();
        let mut master = 0.0f32;
        for (pi, _) in mix.pieces.iter().enumerate() {
            let piece_peak = mix
                .channels
                .iter()
                .filter(|c| c.piece == pi)
                .map(|c| c.meter)
                .chain(mix.sends.iter().filter(|sd| sd.piece == pi).map(|sd| sd.meter))
                .map(peak)
                .fold(0.0f32, f32::max);
            strips.push(piece_peak);
            for ch in mix.channels.iter().filter(|c| c.piece == pi) {
                let p = peak(ch.meter);
                master = master.max(p);
                strips.push(p);
            }
        }
        for bus in &mix.buses {
            let p = peak(bus.meter);
            master = master.max(p);
            strips.push(p);
        }
        signal_drums_proto::MeterSnapshot { master, strips, voices }
    }

    fn set_piece_gain(&self, idx: u32, db: f32) {
        // Continuous control — no Mixer echo (the UI tracks it optimistically);
        // echoing at drag rate is what made the fader jitter.
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(p) = s.mix.pieces.get_mut(idx as usize) {
                p.state.gain_db = db;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_piece_mute(&self, idx: u32, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(p) = s.mix.pieces.get_mut(idx as usize) {
                p.state.muted = muted;
            }
        }
        self.apply_kit_mixer();
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_piece_solo(&self, idx: u32, soloed: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(p) = s.mix.pieces.get_mut(idx as usize) {
                p.state.soloed = soloed;
            }
        }
        self.apply_kit_mixer();
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_bus_solo(&self, idx: u32, soloed: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(b) = s.mix.buses.get_mut(idx as usize) {
                b.state.soloed = soloed;
            }
        }
        self.apply_kit_mixer();
        self.inner.events.publish(DrumEvent::Mixer(DrumRig::mixer(self)));
    }
    fn set_send_level(&self, idx: u32, db: f32) {
        // Continuous — no Mixer echo (see set_piece_gain).
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(snd) = s.mix.sends.get_mut(idx as usize) {
                snd.level_db = db;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_channel_gain(&self, idx: u32, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(c) = s.mix.channels.get_mut(idx as usize) {
                c.state.gain_db = db;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_channel_mute(&self, idx: u32, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(c) = s.mix.channels.get_mut(idx as usize) {
                c.state.muted = muted;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_channel_solo(&self, idx: u32, soloed: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(c) = s.mix.channels.get_mut(idx as usize) {
                c.state.soloed = soloed;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_bus_gain(&self, idx: u32, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(b) = s.mix.buses.get_mut(idx as usize) {
                b.state.gain_db = db;
            }
        }
        self.apply_kit_mixer();
    }
    fn set_bus_mute(&self, idx: u32, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(b) = s.mix.buses.get_mut(idx as usize) {
                b.state.muted = muted;
            }
        }
        self.apply_kit_mixer();
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

// ── shared RigCore (mounted instance-scoped as "drums") ──────────────────────
impl signal_rigs_proto::rig_core::RigCore for DrumRigBackend {
    fn start(&self) {
        DrumRig::start(self);
    }
    fn stop(&self) {
        DrumRig::stop(self);
    }
    fn running(&self) -> bool {
        architect::rig::RigBackend::is_running(self)
    }
    fn presets(&self) -> Vec<signal_rigs_proto::RigPresetInfo> {
        DrumRig::kits(self)
            .into_iter()
            .map(|k| signal_rigs_proto::RigPresetInfo { name: k.name, loaded: k.loaded })
            .collect()
    }
    fn load_preset(&self, index: u32) {
        DrumRig::load_kit(self, index);
    }
    fn midi_ports(&self) -> Vec<String> {
        DrumRig::midi_ports(self)
    }
    fn set_midi_port(&self, name: String) {
        DrumRig::set_midi_port(self, name);
    }
    fn midi_recent(&self) -> Vec<String> {
        DrumRig::midi_recent(self).iter().map(|e| format!("{e:?}")).collect()
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

/// Read `(slot_id, abs engine path)` for each engine in a preset file.
fn engines_from_preset(path: &Path) -> Vec<(String, PathBuf)> {
    let Ok(spec) = PresetSpec::from_file(path) else { return Vec::new() };
    let dir = path.parent().unwrap_or(Path::new(""));
    crate::library::preset_slots(&spec, dir)
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
