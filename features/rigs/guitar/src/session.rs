//! Headless guitar-rig session — the backend for the detachable GUI.
//!
//! [`GuitarRigBackend`] owns the live [`ProfileRig`] and implements the wire
//! services from `signal-guitar-proto` ([`Rig`], [`AudioSettings`]). It knows
//! nothing about any front-end: desktop serves it in-process over
//! `architect::LocalServer`, a headless box serves the same router over a
//! WebSocket, and every GUI is just a remote speaking the generated clients.
//!
//! Moved out of `apps/desktop/src/main.rs` — this is rig domain logic, not
//! app wiring.

use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use signal_guitar_proto::audio::AudioSettings;
use signal_guitar_proto::rig::{Rig, RigEvent, RigStreamSource};
use signal_guitar_proto::{
    AudioDevice, AudioDevices, AudioPrefs, BlockParam, HeadphoneState, LiveBlock, PatchInfo,
    PerfStack, PerformanceModel, PresetInfo, RigStatus, TunerReading,
};
use signal_proto::block::BlockType;
use signal_sampler::{DeviceInfo, GuitarRig, ProfileRig, RigBlock, RigManager};

use crate::library::RigLibrary;
use crate::profiles::{DrivePresetDef, ProfileDef, SetlistDef, SongDef, build_profile};

/// Rig whose audio prefs the settings service reads/writes (persisted to
/// `<config>/signal/rigs/guitar-rig.styx` by `RigManager`).
const AUDIO_RIG_NAME: &str = "Guitar Rig";

/// The boost pedal's cycle: first press engages +1 dB, then each press
/// advances — +2, +3, a −1 dB cut, and back around to +1.
const BOOST_LEVELS: &[f32] = &[1.0, 2.0, 3.0, -1.0];

/// Tempo shown before anyone taps.
const DEFAULT_BPM: f32 = 120.0;

/// Taps further apart than this start a new tap sequence instead of dragging
/// the average toward crawl tempos.
const TAP_RESET: std::time::Duration = std::time::Duration::from_millis(2500);

/// How many recent tap intervals the tempo is averaged over.
const TAP_WINDOW: usize = 4;

/// Tap-tempo tracker: recent intervals + the last tap instant.
#[derive(Default)]
struct TapTracker {
    last: Option<std::time::Instant>,
    intervals: Vec<f32>,
}

/// Meter-pump loop state, kept outside the tick so a caught panic in one
/// iteration doesn't reset gestures or drop the MIDI stream.
///
/// MIDI footswitch mapping: CC 101–105 = switches 1–5 (tap on release,
/// hold at 500 ms fires the hold-layer action); CC 106–110 = the
/// hold-layer functions directly. Momentary switches repeat while held —
/// edge-detect on value.
#[derive(Default)]
pub struct MeterPump {
    /// The open MIDI capture (all input ports merged); `None` until a port
    /// exists. Reopened by the hot-plug scan.
    midi: Option<midicore::midir::MidiStream>,
    /// Input-port names at the last hot-plug scan — a change reopens.
    midi_ports: Vec<String>,
    tick: u64,
    sw_down: [Option<std::time::Instant>; 5],
    sw_hold_fired: [bool; 5],
    cc_down: [bool; 10],
}

/// Shared live rig (a [`ProfileRig`] wrapping the [`GuitarRig`]).
type SharedRig = Arc<Mutex<Option<ProfileRig>>>;

use signal_rig_host::lock::{LockExt, panic_message};

/// The headless rig session: live audio + profile/footswitch state, shared
/// behind `Arc`s so service calls can arrive from any thread. `ProfileRig` is
/// `Send` but not `Sync` (the pipewire backend owns a `*mut pw_thread_loop`),
/// so it lives in a `Mutex` and calls serialize through it.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct GuitarRigBackend {
    rig: SharedRig,
    /// Boost engaged (tap toggles; the level is remembered separately).
    boost_on: Arc<Mutex<bool>>,
    /// Boost pedal level in dB (hold rotates through [`BOOST_LEVELS`]).
    boost_level: Arc<Mutex<f32>>,
    /// The active patch's live FX chain, mirrored for clients. Rebuilt whenever
    /// the active patch changes (the rig has no per-block bypass/param getters).
    blocks: Arc<Mutex<Vec<LiveBlock>>>,
    /// Tapped tempo (BPM). `None` until the first complete tap sequence;
    /// once set it survives patch switches (re-applied to delay blocks).
    tempo: Arc<Mutex<Option<f32>>>,
    /// Recent tap history feeding [`Rig::tap_tempo`].
    taps: Arc<Mutex<TapTracker>>,
    /// The editable profile definition (preset pool + patch pointers) —
    /// the source the runtime profile is built from.
    profile_def: Arc<Mutex<ProfileDef>>,
    /// The song library (defaults) + the setlists (per-set overrides), the
    /// active setlist, and the current song/section position. Mutable until
    /// these become service-driven entities.
    songs_lib: Arc<Mutex<Vec<SongDef>>>,
    setlists: Arc<Mutex<Vec<SetlistDef>>>,
    setlist_index: Arc<Mutex<usize>>,
    song_index: Arc<Mutex<usize>>,
    part_index: Arc<Mutex<usize>>,
    /// Drive block presets (NAM option sets) — library-backed.
    drive_presets: Arc<Mutex<Vec<DrivePresetDef>>>,
    /// Fullscreen tuner overlay state (footswitch-driven, model-synced).
    tuner_visible: Arc<Mutex<bool>>,
    /// Perform-grid mode (0 Preset / 1 Profile / 2 Setlist).
    perform_mode: Arc<Mutex<u32>>,
    /// Deferred profile save (live-edit auto-save marks; pump flushes).
    library_dirty: Arc<std::sync::atomic::AtomicBool>,
    /// Deferred last-active-state save (`last-state.styx`) — marked on
    /// patch/song/part/setlist/tempo changes; the pump flushes it so a
    /// crash restart lands back where the set was.
    state_dirty: Arc<std::sync::atomic::AtomicBool>,
    /// [`Rig::start`] re-entrancy guard: one open at a time (concurrent
    /// opens race the drop/sleep/reopen dance into audible gaps).
    opening: Arc<std::sync::atomic::AtomicBool>,
    /// Debug-formatted audio prefs the live rig was opened with — a repeat
    /// `start` with unchanged prefs is a no-op instead of an audio gap.
    open_prefs: Arc<Mutex<Option<String>>>,
    /// Footswitch CC mapping (midi.styx — remappable).
    midi_map: Arc<Mutex<crate::profiles::MidiMapDef>>,
    /// Keyboard bindings (keymap.styx).
    keymap: Arc<Mutex<Vec<crate::profiles::KeyBindingDef>>>,
    /// Headphone-cue module state (volume/self-mix staged; mute applied).
    headphone: Arc<Mutex<HeadphoneState>>,
    /// Master output trim (dB) — applied with the patch base + mute.
    master_trim: Arc<Mutex<f32>>,
    /// Recent MIDI events (formatted), newest last, capped.
    midi_log: Arc<Mutex<Vec<String>>>,
    /// Monotonic state version, bumped on every mutation (see
    /// `PerformanceModel::revision`).
    revision: Arc<Mutex<u64>>,
    /// The `#[subscribe]` fan-out hub: every mutation publishes full-state
    /// [`RigEvent`]s here; the meter pump publishes `Status` at meter rate.
    events: PubSub<RigEvent>,
    /// Once-start guard for the shared meter pump (`architect::rig`).
    pump_started: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for GuitarRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GuitarRigBackend {
    pub fn new() -> Self {
        let lib = RigLibrary::load_or_bootstrap();
        let backend = Self {
            rig: Arc::new(Mutex::new(None)),
            boost_on: Arc::new(Mutex::new(false)),
            boost_level: Arc::new(Mutex::new(BOOST_LEVELS[0])),
            blocks: Arc::new(Mutex::new(Vec::new())),
            tempo: Arc::new(Mutex::new(None)),
            taps: Arc::new(Mutex::new(TapTracker::default())),
            profile_def: Arc::new(Mutex::new(lib.profile)),
            songs_lib: Arc::new(Mutex::new(lib.songs)),
            setlists: Arc::new(Mutex::new(lib.setlists)),
            setlist_index: Arc::new(Mutex::new(0)),
            song_index: Arc::new(Mutex::new(0)),
            part_index: Arc::new(Mutex::new(0)),
            drive_presets: Arc::new(Mutex::new(lib.drive_presets)),
            tuner_visible: Arc::new(Mutex::new(false)),
            perform_mode: Arc::new(Mutex::new(1)),
            library_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            state_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            opening: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            open_prefs: Arc::new(Mutex::new(None)),
            midi_map: Arc::new(Mutex::new(lib.midi_map)),
            keymap: Arc::new(Mutex::new(lib.keymap)),
            headphone: Arc::new(Mutex::new(HeadphoneState::default())),
            master_trim: Arc::new(Mutex::new(0.0)),
            midi_log: Arc::new(Mutex::new(Vec::new())),
            revision: Arc::new(Mutex::new(0)),
            events: architect::rig::events_hub(),
            pump_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        backend.spawn_meter_pump("rig-meter-pump");
        backend.spawn_drive_calibration();
        backend
    }

    /// One meter-pump iteration (the [`RigBackend::on_tick`] body): MIDI
    /// hot-plug + drain, footswitch gestures, debounced saves. Runs on the
    /// shared `architect::rig` pump — which supplies the interval, the
    /// once-start guard, and the per-tick `catch_unwind` survival — while
    /// the guitar-specific control heartbeat lives here.
    fn pump_tick(&self, pump: &mut MeterPump) {
        pump.tick += 1;
        // MIDI hot-plug: (re)open when the stream is missing or the set of
        // input ports changed (footswitch replugged / enumerated late) —
        // checked every ~2 s, so a mid-set replug comes back on its own.
        // `rescan_stream` drops the old stream's OS clients BEFORE opening
        // anew (the ALSA-seq queue-exhaustion invariant).
        if pump.tick == 1 || pump.tick.is_multiple_of(60) {
            midicore::attach::rescan_stream(&mut pump.midi, &mut pump.midi_ports);
        }
        // Flush pending auto-saves (live edits + position) about once a second.
        if pump.tick.is_multiple_of(30) {
            if self
                .library_dirty
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                let def = self.profile_def.lock_ok();
                RigLibrary::save_profile(&def);
                tracing::debug!("auto-saved live edits to profile.styx");
            }
            if self
                .state_dirty
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                RigLibrary::save_last_state(&self.snapshot_last_state());
                tracing::debug!("auto-saved position to last-state.styx");
            }
        }
        // Update the GR estimate + drain MIDI regardless of publish.
        if let Some(stream) = &pump.midi {
            let mut events: Vec<(u8, u8)> = Vec::new();
            {
                let mut log = self.midi_log.lock_ok();
                for msg in stream.drain() {
                    if let Some(midicore::MidiEvent::ControlChange {
                        controller,
                        value,
                        ..
                    }) = msg.to_event()
                    {
                        let (cc, val) = (u8::from(controller), u8::from(value));
                        tracing::debug!("midi cc {cc} = {val}");
                        events.push((cc, val));
                    }
                    log.push(format!("{msg:?}"));
                    let len = log.len();
                    if len > 128 {
                        log.drain(0..len - 128);
                    }
                }
            }
            for (cc, val) in events {
                // Remappable mapping (midi.styx): gesture switches
                // (tap/hold) + direct hold-layer slots.
                let map = self.midi_map.lock_ok().clone();
                let gesture = map.tap_ccs.iter().position(|c| *c == cc as u32);
                let direct = map.direct.iter().find(|d| d.cc == cc as u32).map(|d| d.slot);
                let Some(idx) = gesture.or_else(|| direct.map(|s| s as usize + 5)) else {
                    continue;
                };
                let idx = idx.min(pump.cc_down.len() - 1);
                let down = val > 0;
                if down == pump.cc_down[idx] {
                    continue; // momentary repeat — not an edge
                }
                pump.cc_down[idx] = down;
                match gesture {
                    // Gesture switches: tap on short release,
                    // hold-layer action at 500 ms (mirrors the UI).
                    Some(sw) if sw < 5 => {
                        if down {
                            pump.sw_down[sw] = Some(std::time::Instant::now());
                            pump.sw_hold_fired[sw] = false;
                        } else {
                            if !pump.sw_hold_fired[sw] {
                                tracing::info!("footswitch {} tap", sw + 1);
                                match sw {
                                    4 => Rig::tap_tempo(self),
                                    s => Rig::press_stack(self, s as u32),
                                }
                            }
                            pump.sw_down[sw] = None;
                        }
                    }
                    _ => {
                        if down {
                            if let Some(slot) = direct {
                                tracing::info!("footswitch direct cc {cc} → slot {slot}");
                                self.hold_layer_action(slot as usize);
                            }
                        }
                    }
                }
            }
            // Holds: fire the hold-layer action at 500 ms.
            for sw in 0..5 {
                if let Some(t) = pump.sw_down[sw] {
                    if !pump.sw_hold_fired[sw] && t.elapsed().as_millis() >= 500 {
                        pump.sw_hold_fired[sw] = true;
                        tracing::info!("footswitch {} hold", sw + 1);
                        self.hold_layer_action(sw);
                    }
                }
            }
        }
    }

    /// Log-binned input spectrum (dB, −90..0) from the rig's pre-amp tap.
    fn input_spectrum(&self) -> Option<Vec<f32>> {
        let (samples, rate) = {
            let guard = self.rig.lock_ok();
            let prig = guard.as_ref()?;
            (prig.input_samples(), prig.sample_rate() as f32)
        };
        if samples.len() < 2048 {
            return None;
        }
        Some(spectrum_bins(&samples[samples.len() - 2048..], rate, 96))
    }

    /// Real gain reduction from the running compressor's detector (the
    /// active chain's comp owns the global telemetry slot). Zero when no
    /// live comp block exists or it's bypassed.
    fn live_comp_gr(&self) -> f32 {
        let has_comp = self
            .blocks
            .lock_ok()
            .iter()
            .any(|b| b.block_type == BlockType::Compressor && !b.bypassed);
        if has_comp { signal_fx::comp_meter::gr_db().max(0.0) } else { 0.0 }
    }

    /// Load a rebuilt profile into the live rig, restore the active patch,
    /// and resync everything — the shared tail of every edit-time rebuild.
    fn reload_rebuilt(&self, rebuilt: signal_sampler::rig_profile::RigProfile) {
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Err(e) = prig.load_profile(rebuilt, None) {
                    tracing::error!("profile reload failed: {e}");
                }
                if let Some(name) = &active {
                    let idx = prig
                        .patches()
                        .iter()
                        .position(|p| p.name.eq_ignore_ascii_case(name));
                    if let Some(idx) = idx {
                        prig.activate(idx);
                    }
                }
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    /// The NAM capture behind a board block, if any: drive slots resolve
    /// through their drive preset; the amp resolves through the active
    /// patch's pool preset.
    fn nam_path_for_block(&self, block_name: &str) -> Option<String> {
        let def = self.profile_def.lock_ok();
        if let Some(slot) = def
            .drives
            .iter()
            .find(|d| d.block.eq_ignore_ascii_case(block_name))
        {
            let dps = self.drive_presets.lock_ok();
            return dps
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&slot.preset))
                .and_then(|p| p.options.get(slot.option).map(|o| o.nam.clone()));
        }
        if block_name.eq_ignore_ascii_case("Amp L") || block_name.eq_ignore_ascii_case("Amp R") {
            let active = {
                let guard = self.rig.lock_ok();
                guard
                    .as_ref()
                    .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))?
            };
            let preset = def
                .patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&active))
                .map(|p| p.preset.clone())?;
            return def
                .presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&preset))
                .map(|p| p.nam.clone());
        }
        None
    }

    /// Realise a drive position on a NAM block at constant perceived level:
    /// input trim pushes the capture, output trim compensates the measured
    /// loudness back to unity (see `nam_calibrate::drive_compensation`).
    fn apply_drive(&self, block_id: &str, block_name: &str, drive: f32) {
        let Some(path) = self.nam_path_for_block(block_name) else {
            return;
        };
        let sr = self
            .rig
            .lock_ok()
            .as_ref()
            .map(|p| p.sample_rate() as f64)
            .unwrap_or(48_000.0);
        let Some((in_db, out_db)) =
            signal_sampler::nam_calibrate::drive_compensation(std::path::Path::new(&path), sr, drive)
        else {
            tracing::warn!("drive compensation unavailable for {block_name} — leaving trims");
            return;
        };
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.rig().set_active_block_param(block_id, "input_trim", in_db);
                prig.rig().set_active_block_param(block_id, "output_trim", out_db);
            }
        }
        tracing::debug!("{block_name}: drive {drive:.2} → in {in_db:+.1} dB, out {out_db:+.1} dB");
    }

    /// Re-apply constant-loudness drive to every NAM board block of the
    /// active chain (after activation / reload — cached, so cheap).
    fn apply_all_drives(&self) {
        let blocks: Vec<(String, String, f32)> = self
            .blocks
            .lock_ok()
            .iter()
            .filter(|b| {
                matches!(b.block_type, BlockType::Drive | BlockType::Boost | BlockType::Amp)
            })
            .map(|b| {
                let drive = b
                    .params
                    .iter()
                    .find(|p| p.name == "drive")
                    .map(|p| p.value)
                    .unwrap_or(0.5);
                (b.id.clone(), b.name.clone(), drive)
            })
            .collect();
        for (id, name, drive) in blocks {
            self.apply_drive(&id, &name, drive);
        }
    }

    /// Pre-measure drive curves for every NAM the profile can reach (drive
    /// preset options + the pool presets) — the import-time offline test.
    /// Runs on its own thread; results land in the on-disk cache.
    fn spawn_drive_calibration(&self) {
        let backend = self.clone();
        std::thread::spawn(move || {
            let sr = backend
                .rig
                .lock_ok()
                .as_ref()
                .map(|p| p.sample_rate() as f64)
                .unwrap_or(48_000.0);
            let mut paths: Vec<String> = Vec::new();
            for p in backend.drive_presets.lock_ok().iter() {
                for o in &p.options {
                    paths.push(o.nam.clone());
                }
            }
            {
                let def = backend.profile_def.lock_ok();
                for p in &def.presets {
                    paths.push(p.nam.clone());
                }
            }
            for path in paths {
                let t = std::time::Instant::now();
                if signal_sampler::nam_calibrate::drive_curve(std::path::Path::new(&path), sr)
                    .is_some()
                    && t.elapsed().as_millis() > 50
                {
                    tracing::info!(
                        "drive calibration: {} measured in {:.1}s",
                        std::path::Path::new(&path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?"),
                        t.elapsed().as_secs_f32()
                    );
                }
            }
            // With curves in cache, snap the live chain to compensated trims.
            backend.apply_all_drives();
            tracing::info!("drive calibration: complete");
        });
    }

    /// The hold-layer functions, by hold-layer slot (0-based): Ambient
    /// stack, FX toggle, next song, boost toggle, tuner. Shared by held
    /// footswitches 1–5 and the direct CC 106–110 mapping.
    fn hold_layer_action(&self, slot: usize) {
        match slot {
            0 => Rig::press_stack(self, 4),
            1 => Rig::toggle_fx(self),
            2 => Rig::next_song(self),
            3 => Rig::toggle_boost(self),
            4 => Rig::toggle_tuner(self),
            _ => {}
        }
    }

    /// Mark the last-active position (setlist/song/part/patch/tempo) for
    /// the pump's debounced flush to `last-state.styx`.
    fn mark_state_dirty(&self) {
        self.state_dirty
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Snapshot the last-active position for persistence.
    fn snapshot_last_state(&self) -> crate::library::LastState {
        let active_patch = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
            .unwrap_or_default();
        crate::library::LastState {
            setlist_index: *self.setlist_index.lock_ok() as u32,
            song_index: *self.song_index.lock_ok() as u32,
            part_index: *self.part_index.lock_ok() as u32,
            active_patch,
            tempo_bpm: self.tempo.lock_ok().unwrap_or(0.0),
        }
    }

    /// Restore the position saved by the pump — after a crash restart the
    /// rig lands back on the same setlist/song/part/patch/tempo instead of
    /// song 1 at 120 BPM. Every index/name is re-validated against the
    /// (possibly hand-edited) library; stale entries clamp or fall back —
    /// this must never panic. Runs before `open_blocking`'s sync tail, so
    /// the tail applies tempo/boost/drives to the restored patch.
    fn restore_last_state(&self) {
        let Some(st) = RigLibrary::load_last_state() else {
            return;
        };
        {
            let sets = self.setlists.lock_ok();
            if !sets.is_empty() {
                *self.setlist_index.lock_ok() =
                    (st.setlist_index as usize).min(sets.len() - 1);
            }
        }
        let resolved = self.resolved_setlist();
        let song = (st.song_index as usize).min(resolved.len().saturating_sub(1));
        *self.song_index.lock_ok() = if resolved.is_empty() { 0 } else { song };
        let parts = resolved
            .get(song)
            .map(|(_, _, _, _, parts)| parts.len())
            .unwrap_or(0);
        *self.part_index.lock_ok() = (st.part_index as usize).min(parts.saturating_sub(1));
        if st.tempo_bpm > 0.0 {
            *self.tempo.lock_ok() = Some(st.tempo_bpm.clamp(40.0, 300.0));
        }
        if !st.active_patch.is_empty() {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if !activate_patch_by_name(prig, &st.active_patch) {
                    tracing::warn!("last-state: patch '{}' no longer exists", st.active_patch);
                }
            }
        }
        tracing::info!(
            "restored last state: setlist {} song {} part {} patch '{}' tempo {}",
            *self.setlist_index.lock_ok(),
            *self.song_index.lock_ok(),
            *self.part_index.lock_ok(),
            st.active_patch,
            st.tempo_bpm,
        );
    }

    /// Re-apply the main-output trim: patch base + master trim + mute.
    fn apply_main_mute(&self) {
        let mute = self.headphone.lock_ok().main_mute;
        let trim = *self.master_trim.lock_ok();
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                let base = prig.active_patch().map(|p| p.output_trim_db).unwrap_or(0.0);
                prig.rig()
                    .set_output_trim_db(base + trim + if mute { -96.0 } else { 0.0 });
            }
        }
    }

    /// Publish the full perf model + chain — call after every mutation.
    fn publish_state(&self) {
        *self.revision.lock_ok() += 1;
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
        self.events.publish(RigEvent::Chain(Rig::chain(self)));
    }

    /// The tempo shown/used right now (tapped, or the default).
    fn tempo_bpm(&self) -> f32 {
        self.tempo.lock_ok().unwrap_or(DEFAULT_BPM)
    }

    /// Push the tapped tempo onto every delay block in the active chain
    /// (quarter-note delay time in ms). Configured patch times apply until
    /// the first tap; after that, taps own the delay time — including across
    /// patch switches.
    fn apply_tempo_to_delays(&self) {
        let Some(bpm) = *self.tempo.lock_ok() else {
            return;
        };
        let quarter_ms = 60_000.0 / bpm;
        let delay_ids: Vec<String> = self
            .blocks
            .lock_ok()
            .iter()
            .filter(|b| b.block_type == BlockType::Delay)
            .map(|b| b.id.clone())
            .collect();
        if delay_ids.is_empty() {
            return;
        }
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                for id in &delay_ids {
                    // The MX engine tempo-syncs itself: with tempo set, each
                    // side's tap division derives its own time.
                    prig.rig().set_active_block_param(id, "tempo_bpm", bpm);
                }
            }
        }
        tracing::info!("tap tempo: {bpm:.1} BPM → delay time {quarter_ms:.0} ms");
    }

    /// The boost currently applied (0.0 while disengaged).
    fn current_boost_db(&self) -> f32 {
        if *self.boost_on.lock_ok() {
            *self.boost_level.lock_ok()
        } else {
            0.0
        }
    }

    /// Record a live edit into the ACTIVE patch's overrides — dialing in a
    /// sound on a stack auto-saves; returning to the patch restores exactly
    /// what it sounded like. `param: None` records a bypass override.
    fn record_patch_override(&self, block_id: &str, param: Option<&str>, value: f32) {
        let block = {
            let blocks = self.blocks.lock_ok();
            blocks
                .iter()
                .find(|b| b.id == block_id)
                .map(|b| (b.name.clone(), b.block_type))
        };
        let Some((block_name, bt)) = block else { return };
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        let Some(patch_name) = active else { return };
        let module = format!("{:?}", bt.category());
        {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def
                .patches
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&patch_name))
            else {
                return;
            };
            let (param_name, op) = match param {
                Some(n) => (n.to_string(), "set"),
                None => (String::new(), "bypass"),
            };
            match p.overrides.iter_mut().find(|o| {
                o.block.eq_ignore_ascii_case(&block_name) && o.op == op && o.param == param_name
            }) {
                Some(o) => o.value = value,
                None => p.overrides.push(crate::profiles::OverrideDef {
                    module,
                    block: block_name,
                    param: param_name,
                    op: op.to_string(),
                    value,
                    text: String::new(),
                }),
            }
        }
        // Debounced write: knob drags mark dirty; the meter pump flushes.
        self.library_dirty
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Recall the active patch's boost setting (PatchDef.boost_db): a patch
    /// like Lead carries its +3 dB engaged; 0 means boost off.
    fn recall_patch_boost(&self) {
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        let Some(name) = active else { return };
        let boost = self
            .profile_def
            .lock_ok()
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
            .map(|p| p.boost_db)
            .unwrap_or(0.0);
        *self.boost_on.lock_ok() = boost != 0.0;
        if boost != 0.0 {
            *self.boost_level.lock_ok() = boost;
        }
    }

    /// Push the boost pedal level onto the active chain's "Boost" gain block
    /// (a `Volume` block). Re-applied after patch switches (activation
    /// reinstalls the configured 0 dB).
    fn apply_boost_to_block(&self) {
        let db = self.current_boost_db();
        let boost_ids: Vec<String> = self
            .blocks
            .lock_ok()
            .iter()
            .filter(|b| b.block_type == BlockType::Volume && b.name.eq_ignore_ascii_case("Boost"))
            .map(|b| b.id.clone())
            .collect();
        if boost_ids.is_empty() {
            return;
        }
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                for id in &boost_ids {
                    prig.rig().set_active_block_param(id, "gain_db", db);
                }
            }
        }
    }

    /// Activate a footswitch stack and re-sync everything that activation
    /// resets: the block mirror + bypass defaults, the tapped tempo on the
    /// fresh delays, and the boost gain block.
    fn activate_stack_and_sync(&self, index: usize) {
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                prig.activate_stack(index);
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
        self.mark_state_dirty();
    }

    /// The active setlist's entries, resolved against the song library:
    /// `(name, key, bpm, stack, sections)` per slot — per-set overrides win
    /// over the song's defaults.
    fn resolved_setlist(&self) -> Vec<(String, String, u32, usize, Vec<String>)> {
        let lib = self.songs_lib.lock_ok();
        let setlists = self.setlists.lock_ok();
        let idx = *self.setlist_index.lock_ok();
        let Some(set) = setlists.get(idx) else {
            return Vec::new();
        };
        set.entries
            .iter()
            .map(|e| {
                let song = lib.iter().find(|s| s.name.eq_ignore_ascii_case(&e.song));
                let key = if e.key.is_empty() {
                    song.map(|s| s.key.clone()).unwrap_or_default()
                } else {
                    e.key.clone()
                };
                let bpm = if e.bpm == 0 {
                    song.map(|s| s.bpm).unwrap_or(120)
                } else {
                    e.bpm
                };
                let stack = song.map(|s| s.stack).unwrap_or(0);
                let parts = song.map(|s| s.parts.clone()).unwrap_or_default();
                (e.song.clone(), key, bpm, stack, parts)
            })
            .collect()
    }

    /// Recall setlist entry `idx`: activate its stack, set the song's tempo
    /// (which re-times the delays), and rewind to the first section.
    fn recall_song(&self, idx: usize) {
        let entry = self.resolved_setlist().get(idx).cloned();
        if let Some((name, key, bpm, stack, _)) = entry {
            *self.part_index.lock_ok() = 0;
            *self.tempo.lock_ok() = Some(bpm as f32);
            self.mark_state_dirty();
            tracing::info!("setlist → {name} ({key} · {bpm} BPM, stack {stack})");
            self.apply_tempo_to_delays();
            // Song switch tuning: reset every stack cursor, then point the
            // song's overridden stacks at their landing patches — the
            // switches are dialed for the song before anything activates.
            let defaults = self
                .songs_lib
                .lock_ok()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&name))
                .map(|s| s.stack_defaults.clone())
                .unwrap_or_default();
            {
                let mut guard = self.rig.lock_ok();
                if let Some(prig) = guard.as_mut() {
                    prig.reset_stack_positions();
                    for d in &defaults {
                        prig.point_stack_at(&d.stack, &d.patch);
                    }
                }
            }
            // Recall activates the song's stack only when it isn't already
            // the active one — a press on the active stack would *rotate*
            // it (FM9 semantics), silently changing the patch.
            let already_active = self
                .rig
                .lock_ok()
                .as_ref()
                .map(|prig| {
                    prig.stacks().get(stack).is_some_and(|st| {
                        st.patches.iter().any(|p| {
                            prig.active_patch().is_some_and(|a| a.name.eq_ignore_ascii_case(p))
                        })
                    })
                })
                .unwrap_or(false);
            if already_active {
                self.publish_state();
            } else {
                self.activate_stack_and_sync(stack);
            }
        }
    }

    /// Open (or re-open) the live rig, then load the Worship profile (which
    /// pre-installs every patch's chain for gapless footswitch switching).
    ///
    /// Blocking (device open + NAM loads) — call off the UI thread; the
    /// [`Rig::start`] service method spawns it.
    ///
    /// Drops any existing rig *first* so the audio device is released before we
    /// re-acquire it — otherwise the re-open races the teardown into "device
    /// busy".
    pub fn open_blocking(&self) {
        tracing::info!("rig open: begin");
        let had_previous = self.rig.lock_ok().take().is_some();
        if had_previous {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        let mut mgr = RigManager::load(AUDIO_RIG_NAME);
        // Monitor + play through a single duplex interface.
        if mgr.audio.output_device.is_empty() && !mgr.audio.input_device.is_empty() {
            mgr.audio.output_device = mgr.audio.input_device.clone();
            let _ = mgr.save();
        }

        tracing::info!("rig open: prefs loaded, opening audio device…");
        match GuitarRig::open(&mgr.audio) {
            Ok(g) => {
                tracing::info!(
                    "rig live: in {} ch{} → out {}",
                    if mgr.audio.input_device.is_empty() { "default" } else { &mgr.audio.input_device },
                    mgr.audio.input_channel + 1,
                    if mgr.audio.output_device.is_empty() { "default" } else { &mgr.audio.output_device },
                );
                let mut prig = ProfileRig::new(g);
                // One loudness authority: the per-block drive calibration
                // (unity-loudness blocks). The old patch-level match would
                // stack a second, different target on top.
                prig.set_level_match(false);
                let profile = {
                    let def = self.profile_def.lock_ok();
                    let dps = self.drive_presets.lock_ok();
                    build_profile(&def, &dps)
                };
                match prig.load_profile(profile, None) {
                    Ok(()) => tracing::info!("profile loaded ({} patches)", prig.patches().len()),
                    Err(e) => tracing::error!("profile load failed: {e}"),
                }
                {
                    let mut slot = self.rig.lock_ok();
                    *slot = Some(prig);
                }
                // Record what we opened with, for the start() no-op check.
                *self.open_prefs.lock_ok() = Some(format!("{:?}", mgr.audio));
            }
            Err(e) => {
                *self.open_prefs.lock_ok() = None;
                tracing::error!("rig open failed: {e:#}");
            }
        }
        // Land back where the last set was (crash-restart recovery).
        self.restore_last_state();
        // Mirror the (now active) patch's FX chain + apply bypass defaults,
        // and re-push any tapped tempo onto the fresh delays.
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    /// Rebuild the mirror of the active patch's FX chain and (re-)apply each
    /// block's initial bypass to the engine (activation re-enables all slots,
    /// so the off-by-default blocks must be re-bypassed here).
    fn resync_blocks(&self) {
        let mut out = Vec::new();
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                if let Some(patch) = prig.active_patch() {
                    let ids = prig.active_block_ids();
                    let reals: Vec<&RigBlock> =
                        patch.chain.iter().filter(|b| b.has_backend()).collect();
                    for (block, id) in reals.iter().zip(ids.iter()) {
                        if block.bypassed {
                            prig.rig().set_block_slot_bypass(id, true);
                        }
                        let (param_name, param_min, param_max, param_value) =
                            match primary_param(block.block_type) {
                                Some((n, mn, mx, dflt)) => {
                                    (Some(n.to_string()), mn, mx, block.param_f32(n).unwrap_or(dflt))
                                }
                                None => (None, 0.0, 0.0, 0.0),
                            };
                        let name = if block.name.trim().is_empty() {
                            format!("{:?}", block.block_type)
                        } else {
                            block.name.clone()
                        };
                        let params = param_specs(block.block_type)
                            .iter()
                            .map(|(pname, min, max, dflt)| BlockParam {
                                name: pname.clone(),
                                value: block.param_f32(pname).unwrap_or(*dflt),
                                min: *min,
                                max: *max,
                            })
                            .collect();
                        // Drive slots: surface the loaded drive preset and
                        // its NAM options for the board's quick switch.
                        let (preset, options, option) = {
                            let def = self.profile_def.lock_ok();
                            def.drives
                                .iter()
                                .find(|d| d.block.eq_ignore_ascii_case(&name))
                                .and_then(|d| {
                                    self.drive_presets
                                        .lock_ok()
                                        .iter()
                                        .find(|p| p.name.eq_ignore_ascii_case(&d.preset))
                                        .map(|p| {
                                            (
                                                p.name.clone(),
                                                p.options.iter().map(|o| o.name.clone()).collect(),
                                                d.option as u32,
                                            )
                                        })
                                })
                                .unwrap_or_default()
                        };
                        out.push(LiveBlock {
                            id: id.clone(),
                            block_type: block.block_type,
                            name,
                            bypassed: block.bypassed,
                            param_name,
                            param_value,
                            param_min,
                            param_max,
                            params,
                            preset,
                            options,
                            option,
                        });
                    }
                }
            }
        }
        *self.blocks.lock_ok() = out;
    }
}

/// Every controllable param for a block type: `(name, min, max, default)` —
/// mirrors the native DSP's param specs (signal-fx). The Control view's
/// panels render from this; values come from the patch's build-time params
/// (incl. overrides) and live edits.
fn param_specs(bt: BlockType) -> Vec<(String, f32, f32, f32)> {
    fn owned(t: &[(&str, f32, f32, f32)]) -> Vec<(String, f32, f32, f32)> {
        t.iter().map(|(n, a, b, c)| (n.to_string(), *a, *b, *c)).collect()
    }
    match bt {
        // The full FTS-EQ band set — one source of truth with the DSP.
        BlockType::Eq => (0..signal_fx::EQ_BANDS * signal_fx::EQ_FIELDS)
            .map(|i| {
                let (band, field) = (i / signal_fx::EQ_FIELDS, i % signal_fx::EQ_FIELDS);
                let (min, max, default) = match field {
                    0 | 1 => (0.0, 1.0, 0.0),
                    2 => (10.0, 30000.0, 1000.0),
                    3 => (-30.0, 30.0, 0.0),
                    4 => (0.025, 40.0, 0.707),
                    _ => (0.0, 9.0, 0.0),
                };
                (signal_fx::eq_param_name(band, field), min, max, default)
            })
            .collect(),
        BlockType::Compressor => owned(&[
            ("threshold", -60.0, 0.0, -40.0),
            ("ratio", 1.0, 20.0, 4.0),
            ("attack", 0.1, 200.0, 10.0),
            ("release", 5.0, 1000.0, 120.0),
            ("knee", 0.0, 24.0, 6.0),
            ("range", 0.0, 60.0, 60.0),
            ("fold", 0.0, 1.0, 0.0),
            ("style", 0.0, 4.0, 0.0),
        ]),
        BlockType::Gate => owned(&[
            ("threshold", -90.0, 0.0, -50.0),
            ("attack", 0.1, 50.0, 1.0),
            ("release", 5.0, 500.0, 120.0),
        ]),
        BlockType::Volume => owned(&[("gain_db", -24.0, 24.0, 0.0)]),
        // Level-matched single drive control (DSP lands with drive-dsp).
        BlockType::Drive | BlockType::Boost => owned(&[("drive", 0.0, 1.0, 0.5)]),
        // NAM amp trims — input trim is "how hard the amp is pushed".
        BlockType::Amp => owned(&[
            ("drive", 0.0, 1.0, 0.5),
            ("input_trim", -12.0, 12.0, 0.0),
            ("output_trim", -12.0, 12.0, 0.0),
        ]),
        // The TimeLine-MX delay surface (signal-fx DELAY_PARAMS subset the
        // panel drives): style, per-side tempo divisions, high-pass,
        // repeat dynamics (ducking), mix, feedback.
        BlockType::Delay => owned(&[
            ("mix", 0.0, 1.0, 0.08),
            ("time", 20.0, 2500.0, 350.0),
            ("feedback", 0.0, 0.95, 0.3),
            ("style", 0.0, 12.0, 1.0),
            ("tap_div_l", 0.0, 7.0, 0.0),
            ("tap_div_r", 0.0, 7.0, 0.0),
            ("high_pass", 0.0, 900.0, 0.0),
            ("repeat_dyn", 0.0, 1.0, 0.0),
            ("pan", -1.0, 1.0, 0.0),
        ]),
        // Reverb surface — algorithm + mix/time/damping/tone/modulation +
        // wet pan (MX chain-A pan).
        BlockType::Reverb => owned(&[
            ("mix", 0.0, 1.0, 0.08),
            ("decay", 0.0, 1.0, 0.4),
            ("size", 0.0, 1.0, 0.5),
            ("algorithm", 0.0, 14.0, 1.0),
            ("modulation", 0.0, 1.0, 0.2),
            ("damping", 0.0, 1.0, 0.3),
            ("tone", -1.0, 1.0, 0.0),
            ("pan_a", -1.0, 1.0, 0.0),
        ]),
        BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => owned(&[
            ("mix", 0.0, 1.0, 0.4),
            ("depth", 0.0, 1.0, 0.5),
            ("rate", 0.05, 10.0, 1.0),
            ("engine", 0.0, 4.0, 0.0),
        ]),
        BlockType::Trem => owned(&[
            ("depth", 0.0, 1.0, 0.5),
            ("mix", 0.0, 1.0, 1.0),
            ("rate", 0.05, 12.0, 4.0),
            ("mode", 0.0, 2.0, 1.0),
        ]),
        _ => Vec::new(),
    }
}

/// The primary dialable param for a block type: `(name, min, max, default)`.
fn primary_param(bt: BlockType) -> Option<(&'static str, f32, f32, f32)> {
    match bt {
        BlockType::Reverb | BlockType::Delay => Some(("mix", 0.0, 0.10, 0.08)),
        BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => Some(("mix", 0.0, 1.0, 0.4)),
        BlockType::Trem => Some(("depth", 0.0, 1.0, 0.5)),
        BlockType::Volume => Some(("gain_db", -12.0, 12.0, 0.0)),
        BlockType::Gate => Some(("threshold", -90.0, 0.0, -50.0)),
        _ => None,
    }
}

/// Folder-relative display label for a patch: the folder default shows as
/// "Default"; others drop the folder prefix ("Clean Verb" → "Verb").
fn patch_display(stack_name: &str, patch_name: &str) -> String {
    if patch_name.eq_ignore_ascii_case(stack_name) {
        "Default".to_string()
    } else {
        patch_name
            .strip_prefix(&format!("{stack_name} "))
            .unwrap_or(patch_name)
            .to_string()
    }
}

/// Snapshot the performance model (folders + live cursor/active state),
/// decorated from the profile definition (preset pointers + override badges).
/// A perf model built from the profile DEFINITION alone — the footswitch
/// stacks and their patch rotations — with default live state. Used before
/// the audio rig opens (e.g. iOS with no interface plugged in yet) so the
/// perform grid shows the stacks instead of an empty screen; the live model
/// ([`build_perf_model`]) takes over once the rig is open.
fn build_perf_model_static(def: &ProfileDef) -> PerformanceModel {
    let stacks = def
        .stacks
        .iter()
        .map(|st| {
            let cur = st.patches.first().cloned().unwrap_or_default();
            let patch_def = def
                .patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&cur));
            PerfStack {
                name: st.name.clone(),
                current_patch: patch_display(&st.name, &cur),
                position: 0,
                patch_count: st.patches.len() as u32,
                available: false,
                is_active: false,
                preset: patch_def.map(|p| p.preset.clone()).unwrap_or_default(),
                override_modules: patch_def
                    .map(|p| p.override_modules())
                    .unwrap_or_default(),
            }
        })
        .collect();
    PerformanceModel {
        profile_name: def.name.clone(),
        stacks,
        tempo_bpm: 120,
        ..Default::default()
    }
}

fn build_perf_model(prig: &ProfileRig, def: &ProfileDef) -> PerformanceModel {
    let active_stack = prig.active_stack();
    let patches = prig.patches();
    let stacks = prig
        .stacks()
        .iter()
        .enumerate()
        .map(|(si, st)| {
            let len = st.patches.len().max(1);
            let pos = prig.stack_position(si) % len;
            let cur = st.patches.get(pos).cloned().unwrap_or_default();
            let available = patches
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&cur))
                .map(|i| prig.is_patch_available(i))
                .unwrap_or(false);
            let patch_def = def
                .patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&cur));
            PerfStack {
                name: st.name.clone(),
                current_patch: patch_display(&st.name, &cur),
                position: pos as u32,
                patch_count: st.patches.len() as u32,
                available,
                is_active: active_stack == Some(si),
                preset: patch_def.map(|p| p.preset.clone()).unwrap_or_default(),
                override_modules: patch_def
                    .map(|p| p.override_modules())
                    .unwrap_or_default(),
            }
        })
        .collect();
    PerformanceModel {
        profile_name: prig.profile_name().unwrap_or_default().to_string(),
        stacks,
        fx_bypass: prig.fx_bypass(),
        boost_db: 0.0, // overwritten by the service (the pedal lives outside prig)
        tempo_bpm: 120,
        songs: Vec::new(),   // filled in by the service (setlist lives outside prig)
        song_index: 0,
        setlists: Vec::new(),
        setlist_index: 0,
        library_songs: Vec::new(),
        tuner_visible: false,
        perform_mode: 1,
        key_bindings: Vec::new(),
        parts: Vec::new(),
        part_index: 0,
        headphone: HeadphoneState::default(),
        master_trim_db: 0.0,
        revision: 0,
    }
}

/// Activate a patch by name. If the patch lives in a stack, jump that
/// stack's rotation to it — the footswitch grid stays consistent with
/// what's audible (one stack always lit). Loose patches activate directly.
/// Returns `false` when no patch matches (stale name — never panics).
fn activate_patch_by_name(prig: &mut ProfileRig, name: &str) -> bool {
    let Some(index) = prig
        .patches()
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name))
    else {
        return false;
    };
    let in_stack = prig.stacks().iter().enumerate().find_map(|(si, st)| {
        st.patches
            .iter()
            .position(|p| p.eq_ignore_ascii_case(name))
            .map(|pos| (si, pos))
    });
    match in_stack {
        Some((stack, pos)) => {
            prig.activate_stack_at(stack, pos);
        }
        None => {
            prig.activate(index);
        }
    }
    true
}

fn map_device(d: DeviceInfo) -> AudioDevice {
    AudioDevice {
        name: d.name,
        channels: d.channels,
        default_sample_rate: d.default_sample_rate,
    }
}

// ── Service impls ─────────────────────────────────────────────────────────

// r[impl primitives.architect.rig-backend]
//
// The shared scaffold supplies the pump loop (one interval, once-guard,
// running-edge detection, per-tick catch_unwind); the guitar keeps its rich
// control heartbeat — footswitch gestures, tap tempo, debounced auto-saves,
// footswitch-MIDI hot-plug — in `pump_tick` as `on_tick` business logic.
// (Its MIDI runs in `on_tick`, not the scaffold's `midi_ports` hooks,
// because footswitches must work even while audio is stopped.)
impl RigBackend for GuitarRigBackend {
    type Event = RigEvent;
    type Tick = MeterPump;

    fn events_hub(&self) -> &PubSub<RigEvent> {
        &self.events
    }

    fn is_running(&self) -> bool {
        self.rig.lock_ok().is_some()
    }

    fn pump_started(&self) -> &std::sync::atomic::AtomicBool {
        &self.pump_started
    }

    fn on_tick(&self, pump: &mut MeterPump) {
        self.pump_tick(pump);
    }

    /// Publish `Status` on the transport edge — including the final
    /// `running: false` event when the rig stops, so remotes see it.
    fn on_running_edge(&self, _running: bool) {
        self.events.publish(RigEvent::Status(Rig::status(self)));
    }

    /// Status + spectrum + comp telemetry at full meter rate (~30 Hz) — the
    /// surface stays alive to the hand. A plain thread, not the audio
    /// callback: meters cross from the RT thread via atomics, and
    /// `PubSub::publish` takes a lock so it must never run RT.
    fn on_running_tick(&self) {
        self.events.publish(RigEvent::Status(Rig::status(self)));
        if let Some(bins) = self.input_spectrum() {
            self.events.publish(RigEvent::Spectrum(bins));
        }
        let (wave_in, wave_gr) = signal_fx::comp_meter::wave_snapshot(3);
        self.events.publish(RigEvent::CompWave(wave_in, wave_gr));
    }
}

impl Rig for GuitarRigBackend {
    fn start(&self) {
        // One open at a time: concurrent starts would race the
        // drop → 250 ms sleep → reopen dance into audible gaps.
        if self.opening.swap(true, std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("rig start: open already in progress — ignored");
            return;
        }
        let backend = self.clone();
        // Opening starts the transport engine, which lazily spawns pump
        // tasks via `architect::platform::spawn` (→ `tokio::spawn`) — that needs an
        // ambient runtime. This open runs on a fresh OS thread, which does
        // NOT inherit the caller's runtime, so carry the caller's handle
        // across and enter it on the new thread. (On the pipewire engine
        // the duplex path never spawned these, which is why it worked
        // without this; the cpal path on iOS/macOS does.)
        let rt_handle = tokio::runtime::Handle::try_current().ok();
        std::thread::spawn(move || {
            let _rt_guard = rt_handle.as_ref().map(|h| h.enter());
            let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Already live with unchanged prefs? Reopening would drop
                // the device mid-note for nothing — no-op.
                let prefs = format!("{:?}", RigManager::load(AUDIO_RIG_NAME).audio);
                let live = backend.rig.lock_ok().is_some();
                if live && backend.open_prefs.lock_ok().as_deref() == Some(prefs.as_str()) {
                    tracing::info!("rig start: already live with unchanged prefs — no-op");
                    return;
                }
                backend.open_blocking();
            }));
            backend
                .opening
                .store(false, std::sync::atomic::Ordering::SeqCst);
            if let Err(panic) = opened {
                tracing::error!("rig open panicked ({})", panic_message(&*panic));
            }
        });
    }

    fn stop(&self) {
        *self.rig.lock_ok() = None;
        *self.open_prefs.lock_ok() = None;
        tracing::info!("rig stopped");
        self.publish_state();
    }

    fn status(&self) -> RigStatus {
        let guard = self.rig.lock_ok();
        let (input_peak, output_peak, in_lr, out_lr, active_patch) = match guard.as_ref() {
            Some(prig) => (
                prig.rig().input_peak(),
                prig.rig().output_peak(),
                prig.rig().input_peak_lr(),
                prig.rig().output_peak_lr(),
                prig.active_patch().map(|p| p.name.clone()),
            ),
            None => return RigStatus::default(),
        };
        drop(guard);
        let in_db = if input_peak > 0.0 {
            (20.0 * input_peak.log10()).max(-90.0)
        } else {
            -90.0
        };
        let _ = in_db;
        RigStatus {
            running: true,
            input_peak,
            output_peak,
            active_patch,
            comp_gr_db: self.live_comp_gr(),
            input_peak_l: in_lr.0,
            input_peak_r: in_lr.1,
            output_peak_l: out_lr.0,
            output_peak_r: out_lr.1,
        }
    }

    fn perf(&self) -> PerformanceModel {
        let mut m = {
            let def = self.profile_def.lock_ok();
            // Live model when the audio rig is open; otherwise the static
            // model from the profile def, so the footswitch stacks still
            // render before the device opens (iOS with no interface yet).
            self.rig
                .lock_ok()
                .as_ref()
                .map(|prig| build_perf_model(prig, &def))
                .unwrap_or_else(|| build_perf_model_static(&def))
        };
        m.boost_db = self.current_boost_db();
        m.tempo_bpm = self.tempo_bpm().round() as u32;
        {
            let resolved = self.resolved_setlist();
            let song_idx = *self.song_index.lock_ok();
            m.songs = resolved
                .iter()
                .map(|(name, key, bpm, _, _)| signal_guitar_proto::SongSlot {
                    name: name.clone(),
                    key: key.clone(),
                    bpm: *bpm,
                })
                .collect();
            m.song_index = song_idx as u32;
            m.parts = resolved
                .get(song_idx)
                .map(|(_, _, _, _, parts)| parts.clone())
                .unwrap_or_default();
            m.setlists = self.setlists.lock_ok().iter().map(|s| s.name.clone()).collect();
            m.setlist_index = *self.setlist_index.lock_ok() as u32;
            m.tuner_visible = *self.tuner_visible.lock_ok();
            m.perform_mode = *self.perform_mode.lock_ok();
            m.key_bindings = self
                .keymap
                .lock_ok()
                .iter()
                .map(|b| signal_guitar_proto::KeyBinding {
                    keys: b.keys.clone(),
                    action: b.action.clone(),
                })
                .collect();
            m.library_songs = self
                .songs_lib
                .lock_ok()
                .iter()
                .map(|s| signal_guitar_proto::SongSlot {
                    name: s.name.clone(),
                    key: s.key.clone(),
                    bpm: s.bpm,
                })
                .collect();
        }
        m.part_index = *self.part_index.lock_ok() as u32;
        m.headphone = self.headphone.lock_ok().clone();
        m.master_trim_db = *self.master_trim.lock_ok();
        m.revision = *self.revision.lock_ok();
        m
    }

    fn chain(&self) -> Vec<LiveBlock> {
        self.blocks.lock_ok().clone()
    }

    fn press_stack(&self, index: u32) {
        self.activate_stack_and_sync(index as usize);
    }

    fn next_song(&self) {
        let last = self.resolved_setlist().len().saturating_sub(1);
        let idx = {
            let mut i = self.song_index.lock_ok();
            *i = (*i + 1).min(last);
            *i
        };
        self.recall_song(idx);
    }

    fn prev_song(&self) {
        let idx = {
            let mut i = self.song_index.lock_ok();
            *i = i.saturating_sub(1);
            *i
        };
        self.recall_song(idx);
    }

    fn select_song(&self, index: u32) {
        let last = self.resolved_setlist().len().saturating_sub(1);
        let idx = (index as usize).min(last);
        *self.song_index.lock_ok() = idx;
        self.recall_song(idx);
    }

    fn select_part(&self, index: u32) {
        let (song_idx, last) = {
            let i = *self.song_index.lock_ok();
            let last = self
                .resolved_setlist()
                .get(i)
                .map(|(_, _, _, _, parts)| parts.len().saturating_sub(1))
                .unwrap_or(0);
            (i, last)
        };
        let idx = (index as usize).min(last);
        *self.part_index.lock_ok() = idx;
        self.mark_state_dirty();
        tracing::info!("part → {idx} (song {song_idx})");
        // Sections don't drive audio yet (scenes later) — publish so every
        // remote follows the section highlight.
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
    }

    fn select_setlist(&self, index: u32) {
        // setlists.styx is user-edited — it may be empty. Guard, don't index.
        let name = {
            let sets = self.setlists.lock_ok();
            if sets.is_empty() {
                tracing::warn!("select_setlist: no setlists in the library");
                return;
            }
            let idx = (index as usize).min(sets.len() - 1);
            *self.setlist_index.lock_ok() = idx;
            sets[idx].name.clone()
        };
        *self.song_index.lock_ok() = 0;
        tracing::info!("setlist switched → {name}");
        self.recall_song(0);
        self.publish_state();
        self.mark_state_dirty();
    }

    fn move_song(&self, from: u32, to: u32) {
        {
            let mut setlists = self.setlists.lock_ok();
            let active = *self.setlist_index.lock_ok();
            let Some(set) = setlists.get_mut(active) else { return };
            let entries = &mut set.entries;
            let (from, to) = (from as usize, to as usize);
            if from >= entries.len() || to >= entries.len() {
                return;
            }
            let song = entries.remove(from);
            entries.insert(to, song);
            RigLibrary::save_setlists(&setlists);
            // Keep the current-song pointer on the same song.
            let mut cur = self.song_index.lock_ok();
            if *cur == from {
                *cur = to;
            } else if from < *cur && to >= *cur {
                *cur -= 1;
            } else if from > *cur && to <= *cur {
                *cur += 1;
            }
        }
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
    }

    fn patches(&self) -> Vec<PatchInfo> {
        let guard = self.rig.lock_ok();
        let Some(prig) = guard.as_ref() else {
            return Vec::new();
        };
        let active = prig.active_patch().map(|p| p.name.clone());
        let stacks = prig.stacks().to_vec();
        prig.patches()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let stack_entry = stacks
                    .iter()
                    .find(|st| st.patches.iter().any(|n| n.eq_ignore_ascii_case(&p.name)));
                let stack = stack_entry.map(|st| st.name.clone()).unwrap_or_default();
                let default_in_stack = stack_entry
                    .and_then(|st| st.patches.first())
                    .map(|first| first.eq_ignore_ascii_case(&p.name))
                    .unwrap_or(false);
                let (preset, override_modules) = {
                    let def = self.profile_def.lock_ok();
                    def.patches
                        .iter()
                        .find(|d| d.name.eq_ignore_ascii_case(&p.name))
                        .map(|d| (d.preset.clone(), d.override_modules()))
                        .unwrap_or_default()
                };
                PatchInfo {
                    preset,
                    override_modules,
                    default_in_stack,
                    name: p.name.clone(),
                    stack,
                    available: prig.is_patch_available(i),
                    active: active.as_deref() == Some(p.name.as_str()),
                }
            })
            .collect()
    }

    fn select_patch(&self, index: u32) {
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Some(name) = prig.patches().get(index as usize).map(|p| p.name.clone()) {
                    activate_patch_by_name(prig, &name);
                }
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
        self.mark_state_dirty();
    }

    fn presets(&self) -> Vec<PresetInfo> {
        let def = self.profile_def.lock_ok();
        let active_patch = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|p| p.active_patch().map(|p| p.name.clone()));
        let active_preset = active_patch.and_then(|ap| {
            def.patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&ap))
                .map(|p| p.preset.clone())
        });
        def.presets
            .iter()
            .map(|preset| PresetInfo {
                name: preset.name.clone(),
                active: active_preset.as_deref() == Some(preset.name.as_str()),
                used_by: def
                    .patches
                    .iter()
                    .filter(|p| p.preset.eq_ignore_ascii_case(&preset.name))
                    .count() as u32,
            })
            .collect()
    }

    fn set_patch_preset(&self, patch: u32, preset: u32) {
        // Repoint in the definition…
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(preset_name) = def.presets.get(preset as usize).map(|p| p.name.clone())
            else {
                return;
            };
            let Some(p) = def.patches.get_mut(patch as usize) else {
                return;
            };
            tracing::info!("patch '{}' → preset '{preset_name}'", p.name);
            p.preset = preset_name;
            RigLibrary::save_profile(&def);
            {
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        }
        };
        // …then rebuild the live chains. A full reload (brief gap) — this is
        // an edit-time operation, and it keeps every patch preinstalled for
        // gapless footswitching afterward.
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Err(e) = prig.load_profile(rebuilt, None) {
                    tracing::error!("profile reload failed: {e}");
                }
                // Restore the patch that was live before the reload.
                if let Some(name) = &active {
                    let idx = prig
                        .patches()
                        .iter()
                        .position(|p| p.name.eq_ignore_ascii_case(name));
                    if let Some(idx) = idx {
                        prig.activate(idx);
                    }
                }
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    fn set_block_option(&self, id: String, option: u32) {
        // The id addresses a live block; resolve its name, flip the option
        // in the definition, rebuild + reload (edit-time gap, patches stay
        // preinstalled for gapless switching afterward).
        let block_name = self
            .blocks
            .lock_ok()
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone());
        let Some(block_name) = block_name else { return };
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(slot) = def
                .drives
                .iter_mut()
                .find(|d| d.block.eq_ignore_ascii_case(&block_name))
            else {
                return;
            };
            let n_options = self
                .drive_presets
                .lock_ok()
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&slot.preset))
                .map(|p| p.options.len())
                .unwrap_or(0);
            if n_options == 0 {
                return;
            }
            slot.option = (option as usize).min(n_options - 1);
            tracing::info!("{} → {} option {}", slot.block, slot.preset, slot.option);
            RigLibrary::save_profile(&def);
            {
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        }
        };
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Err(e) = prig.load_profile(rebuilt, None) {
                    tracing::error!("profile reload failed: {e}");
                }
                if let Some(name) = &active {
                    let idx = prig
                        .patches()
                        .iter()
                        .position(|p| p.name.eq_ignore_ascii_case(name));
                    if let Some(idx) = idx {
                        prig.activate(idx);
                    }
                }
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    fn add_preset(&self, name: String, nam_path: String) {
        if !std::path::Path::new(&nam_path).exists() {
            tracing::warn!("add_preset: {nam_path} does not exist");
            return;
        }
        {
            let mut def = self.profile_def.lock_ok();
            if def.presets.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                tracing::warn!("add_preset: '{name}' already exists");
                return;
            }
            def.presets.push(crate::profiles::PresetDef { name: name.clone(), nam: nam_path });
            RigLibrary::save_profile(&def);
        }
        tracing::info!("preset added: {name}");
        self.spawn_drive_calibration();
        self.publish_state();
    }

    fn add_stack(&self, name: String) {
        {
            let mut def = self.profile_def.lock_ok();
            if def.stacks.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
                return;
            }
            def.stacks.push(crate::profiles::StackDef { name: name.clone(), patches: Vec::new() });
            RigLibrary::save_profile(&def);
        }
        tracing::info!("stack added: {name}");
        self.publish_state();
    }

    fn add_patch(&self, name: String, stack: String, preset: String) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            if def.patches.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                tracing::warn!("add_patch: '{name}' already exists");
                return;
            }
            if !def.presets.iter().any(|p| p.name.eq_ignore_ascii_case(&preset)) {
                tracing::warn!("add_patch: preset '{preset}' not found");
                return;
            }
            def.patches.push(crate::profiles::PatchDef {
                name: name.clone(),
                preset,
                trim_db: 0.0,
                boost_db: 0.0,
                overrides: Vec::new(),
            });
            if let Some(st) = def.stacks.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&stack)) {
                st.patches.push(name.clone());
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        tracing::info!("patch added: {name}");
        self.reload_rebuilt(rebuilt);
    }

    fn add_drive_preset(&self, name: String, nam_path: String) {
        if !std::path::Path::new(&nam_path).exists() {
            tracing::warn!("add_drive_preset: {nam_path} does not exist");
            return;
        }
        {
            let mut dps = self.drive_presets.lock_ok();
            if dps.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                return;
            }
            dps.push(crate::profiles::DrivePresetDef {
                name: name.clone(),
                options: vec![crate::profiles::DriveOptionDef {
                    name: "Default".to_string(),
                    nam: nam_path,
                }],
            });
            RigLibrary::save_drive_presets(&dps);
        }
        tracing::info!("drive preset added: {name}");
        self.spawn_drive_calibration();
        self.publish_state();
    }

    fn reload_library(&self) {
        tracing::info!("reloading rig library from {}", crate::library::rig_dir().display());
        let lib = RigLibrary::load_or_bootstrap();
        *self.profile_def.lock_ok() = lib.profile;
        *self.drive_presets.lock_ok() = lib.drive_presets;
        *self.songs_lib.lock_ok() = lib.songs;
        *self.setlists.lock_ok() = lib.setlists;
        *self.midi_map.lock_ok() = lib.midi_map;
        *self.keymap.lock_ok() = lib.keymap;
        let rebuilt = {
            let def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
        self.spawn_drive_calibration();
    }

    fn rename_preset(&self, old: String, new_name: String) {
        if new_name.trim().is_empty() {
            return;
        }
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&old))
            else {
                return;
            };
            p.name = new_name.clone();
            for patch in def.patches.iter_mut() {
                if patch.preset.eq_ignore_ascii_case(&old) {
                    patch.preset = new_name.clone();
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        tracing::info!("preset renamed: {old} → {new_name}");
        self.reload_rebuilt(rebuilt);
    }

    fn delete_preset(&self, name: String) {
        {
            let mut def = self.profile_def.lock_ok();
            if def.patches.iter().any(|p| p.preset.eq_ignore_ascii_case(&name)) {
                tracing::warn!("delete_preset: '{name}' is in use by a patch — repoint first");
                return;
            }
            def.presets.retain(|p| !p.name.eq_ignore_ascii_case(&name));
            RigLibrary::save_profile(&def);
        }
        tracing::info!("preset deleted: {name}");
        self.publish_state();
    }

    fn rename_patch(&self, old: String, new_name: String) {
        if new_name.trim().is_empty() {
            return;
        }
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.patches.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&old))
            else {
                return;
            };
            p.name = new_name.clone();
            for st in def.stacks.iter_mut() {
                for slot in st.patches.iter_mut() {
                    if slot.eq_ignore_ascii_case(&old) {
                        *slot = new_name.clone();
                    }
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        tracing::info!("patch renamed: {old} → {new_name}");
        self.reload_rebuilt(rebuilt);
    }

    fn delete_patch(&self, name: String) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let before = def.patches.len();
            def.patches.retain(|p| !p.name.eq_ignore_ascii_case(&name));
            if def.patches.len() == before {
                return;
            }
            for st in def.stacks.iter_mut() {
                st.patches.retain(|p| !p.eq_ignore_ascii_case(&name));
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        tracing::info!("patch deleted: {name}");
        self.reload_rebuilt(rebuilt);
    }

    fn rename_stack(&self, old: String, new_name: String) {
        if new_name.trim().is_empty() {
            return;
        }
        {
            let mut def = self.profile_def.lock_ok();
            let Some(st) = def.stacks.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&old))
            else {
                return;
            };
            st.name = new_name.clone();
            RigLibrary::save_profile(&def);
        }
        tracing::info!("stack renamed: {old} → {new_name}");
        self.publish_state();
    }

    fn delete_stack(&self, name: String) {
        {
            let mut def = self.profile_def.lock_ok();
            def.stacks.retain(|s| !s.name.eq_ignore_ascii_case(&name));
            RigLibrary::save_profile(&def);
        }
        tracing::info!("stack deleted: {name}");
        self.publish_state();
    }

    fn add_song(&self, name: String, key: String, bpm: u32) {
        if name.trim().is_empty() {
            return;
        }
        {
            let mut songs = self.songs_lib.lock_ok();
            if songs.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
                return;
            }
            songs.push(SongDef {
                name: name.clone(),
                key: if key.is_empty() { "C".to_string() } else { key },
                bpm: if bpm == 0 { 120 } else { bpm },
                stack: 0,
                parts: Vec::new(),
                stack_defaults: Vec::new(),
            });
            RigLibrary::save_songs(&songs);
        }
        tracing::info!("song added: {name}");
        self.publish_state();
    }

    fn add_setlist(&self, name: String) {
        if name.trim().is_empty() {
            return;
        }
        {
            let mut sets = self.setlists.lock_ok();
            if sets.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
                return;
            }
            sets.push(SetlistDef { name: name.clone(), entries: Vec::new() });
            RigLibrary::save_setlists(&sets);
        }
        tracing::info!("setlist added: {name}");
        self.publish_state();
    }

    fn add_setlist_entry(&self, setlist: u32, song: String) {
        {
            let mut sets = self.setlists.lock_ok();
            let Some(set) = sets.get_mut(setlist as usize) else { return };
            set.entries.push(crate::profiles::SetlistEntryDef {
                song: song.clone(),
                key: String::new(),
                bpm: 0,
            });
            RigLibrary::save_setlists(&sets);
        }
        tracing::info!("setlist {setlist}: added {song}");
        self.publish_state();
    }

    fn remove_setlist_entry(&self, setlist: u32, entry: u32) {
        {
            let mut sets = self.setlists.lock_ok();
            let Some(set) = sets.get_mut(setlist as usize) else { return };
            if (entry as usize) < set.entries.len() {
                set.entries.remove(entry as usize);
            }
            RigLibrary::save_setlists(&sets);
        }
        self.publish_state();
    }

    fn set_setlist_entry(&self, entry: u32, key: String, bpm: u32) {
        {
            let active = *self.setlist_index.lock_ok();
            let mut sets = self.setlists.lock_ok();
            let Some(e) = sets
                .get_mut(active)
                .and_then(|s| s.entries.get_mut(entry as usize))
            else {
                return;
            };
            e.key = key;
            e.bpm = bpm;
            RigLibrary::save_setlists(&sets);
        }
        // Re-recall if the edited entry is the current song (tempo change).
        if *self.song_index.lock_ok() == entry as usize {
            self.recall_song(entry as usize);
        }
        self.publish_state();
    }

    fn set_block_ir(&self, id: String, path: String) {
        if !std::path::Path::new(&path).exists() {
            tracing::warn!("set_block_ir: {path} does not exist");
            return;
        }
        // Record as a set_text override on the active patch, then rebuild —
        // the registry loads the IR at chain build time.
        let block = {
            let blocks = self.blocks.lock_ok();
            blocks.iter().find(|b| b.id == id).map(|b| b.name.clone())
        };
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        let (Some(block_name), Some(patch_name)) = (block, active) else { return };
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def
                .patches
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&patch_name))
            else {
                return;
            };
            match p.overrides.iter_mut().find(|o| {
                o.block.eq_ignore_ascii_case(&block_name) && o.op == "set_text" && o.param == "ir_path"
            }) {
                Some(o) => o.text = path.clone(),
                None => p
                    .overrides
                    .push(crate::profiles::OverrideDef::set_text("Time", &block_name, "ir_path", &path)),
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        tracing::info!("{block_name}: custom IR {path}");
        self.reload_rebuilt(rebuilt);
    }

    fn set_preset_nam(&self, index: u32, nam_path: String) {
        if !std::path::Path::new(&nam_path).exists() {
            tracing::warn!("set_preset_nam: {nam_path} does not exist");
            return;
        }
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.presets.get_mut(index as usize) else { return };
            tracing::info!("preset '{}' → {nam_path}", p.name);
            p.nam = nam_path;
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
        self.spawn_drive_calibration();
    }

    fn set_patch_trim(&self, patch: u32, db: f32) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.patches.get_mut(patch as usize) else { return };
            p.trim_db = db.clamp(-24.0, 24.0);
            tracing::info!("patch '{}' trim {:+.1} dB", p.name, p.trim_db);
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            build_profile(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    fn set_perform_mode(&self, mode: u32) {
        *self.perform_mode.lock_ok() = mode.min(2);
        tracing::info!(
            "perform mode → {}",
            ["preset", "profile", "setlist"][mode.min(2) as usize]
        );
        self.publish_state();
    }

    fn play_preset(&self, index: u32) {
        // Resolve the pool preset → its first patch → activate directly.
        let preset_name = {
            let def = self.profile_def.lock_ok();
            def.presets.get(index as usize).map(|p| p.name.clone())
        };
        let Some(preset_name) = preset_name else { return };
        let patch_idx = {
            let def = self.profile_def.lock_ok();
            def.patches
                .iter()
                .position(|p| p.preset.eq_ignore_ascii_case(&preset_name))
        };
        let Some(idx) = patch_idx else {
            tracing::warn!("play_preset: no patch uses '{preset_name}'");
            return;
        };
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                prig.activate(idx);
            }
        }
        tracing::info!("preset mode → {preset_name}");
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        self.publish_state();
        self.mark_state_dirty();
    }

    fn toggle_tuner(&self) {
        let shown = {
            let mut t = self.tuner_visible.lock_ok();
            *t = !*t;
            *t
        };
        // Tuning is silent: mute the instrument INTO the chain (not the
        // output) so delay/reverb trails ring out naturally underneath.
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.rig().set_input_mute(shown);
            }
        }
        tracing::info!("tuner overlay {} (input {})", if shown { "shown" } else { "hidden" },
            if shown { "muted" } else { "live" });
        self.publish_state();
    }

    fn capture_di_reference(&self, seconds: u32) {
        let secs = seconds.clamp(5, 60) as usize;
        let (sr, armed) = {
            let guard = self.rig.lock_ok();
            match guard.as_ref() {
                Some(prig) => {
                    let sr = prig.sample_rate() as usize;
                    prig.rig().arm_di_capture(secs * sr);
                    (sr, true)
                }
                None => (48_000, false),
            }
        };
        if !armed {
            tracing::warn!("DI capture: rig not running");
            return;
        }
        tracing::info!("DI capture armed: play for {secs} s…");
        let backend = self.clone();
        std::thread::spawn(move || {
            // Wait for the probe to fill the buffer (+1 s headroom).
            for _ in 0..(secs + 2) * 10 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let done = {
                    let guard = backend.rig.lock_ok();
                    guard
                        .as_ref()
                        .map(|p| p.rig().di_capture_state().1 == 0)
                        .unwrap_or(true)
                };
                if done {
                    break;
                }
            }
            let samples = {
                let guard = backend.rig.lock_ok();
                guard
                    .as_ref()
                    .map(|p| p.rig().take_di_capture())
                    .unwrap_or_default()
            };
            match signal_sampler::nam_calibrate::install_di_reference(&samples, sr as u32) {
                Ok(()) => {
                    tracing::info!("DI captured — re-measuring the library");
                    backend.spawn_drive_calibration();
                }
                Err(e) => tracing::warn!("DI capture failed: {e}"),
            }
        });
    }

    fn set_headphone(&self, volume: f32, self_mix: f32) {
        {
            let mut hp = self.headphone.lock_ok();
            hp.volume = volume.clamp(0.0, 1.0);
            hp.self_mix = self_mix.clamp(0.0, 1.0);
            // Feed the engine's phones bus (routed interfaces blend the
            // external monitor mix + self signal there, lock-free).
            signal_sampler::rig::GuitarRig::set_phones_levels(hp.volume, hp.self_mix);
        }
        self.publish_state();
    }

    fn toggle_main_mute(&self) {
        {
            let mut hp = self.headphone.lock_ok();
            hp.main_mute = !hp.main_mute;
            tracing::info!("main output: {}", if hp.main_mute { "MUTED" } else { "live" });
        }
        self.apply_main_mute();
        self.publish_state();
    }

    fn set_master_trim(&self, db: f32) {
        *self.master_trim.lock_ok() = db.clamp(-24.0, 12.0);
        self.apply_main_mute();
        self.publish_state();
    }

    fn midi_recent(&self) -> Vec<String> {
        self.midi_log.lock_ok().clone()
    }

    fn toggle_fx(&self) {
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                let on = prig.toggle_fx_bypass();
                tracing::info!("FX bypass: {}", if on { "ON" } else { "off" });
            }
        }
        self.publish_state();
    }

    fn toggle_boost(&self) {
        let on = {
            let mut b = self.boost_on.lock_ok();
            *b = !*b;
            *b
        };
        self.apply_boost_to_block();
        tracing::info!(
            "boost pedal: {}",
            if on {
                format!("{:+.0} dB", *self.boost_level.lock_ok())
            } else {
                "off".to_string()
            }
        );
        self.publish_state();
    }

    fn cycle_boost(&self) {
        {
            let mut on = self.boost_on.lock_ok();
            let mut level = self.boost_level.lock_ok();
            if *on {
                // Already engaged: rotate to the next level.
                let next = BOOST_LEVELS
                    .iter()
                    .position(|l| (*l - *level).abs() < 0.01)
                    .map(|i| BOOST_LEVELS[(i + 1) % BOOST_LEVELS.len()])
                    .unwrap_or(BOOST_LEVELS[0]);
                *level = next;
            } else {
                // Engage at the remembered level.
                *on = true;
            }
        }
        self.apply_boost_to_block();
        tracing::info!("boost pedal: {:+.0} dB", self.current_boost_db());
        self.publish_state();
    }

    fn tuner(&self) -> TunerReading {
        let (samples, rate) = {
            let guard = self.rig.lock_ok();
            match guard.as_ref() {
                Some(prig) => (prig.input_samples(), prig.sample_rate() as f32),
                None => return TunerReading::default(),
            }
        };
        match detect_pitch(&samples, rate) {
            Some(freq) => {
                let (note, cents) = note_and_cents(freq);
                TunerReading { active: true, freq_hz: freq, note, cents }
            }
            None => TunerReading::default(),
        }
    }

    fn tap_tempo(&self) {
        // Tuner up? The tap-tempo switch doubles as its dismiss.
        if *self.tuner_visible.lock_ok() {
            Rig::toggle_tuner(self);
            return;
        }
        let now = std::time::Instant::now();
        let new_tempo = {
            let mut taps = self.taps.lock_ok();
            let interval = taps.last.map(|t| now - t);
            taps.last = Some(now);
            match interval {
                // First tap, or a stale one — starts a new sequence.
                None => {
                    taps.intervals.clear();
                    None
                }
                Some(dt) if dt > TAP_RESET => {
                    taps.intervals.clear();
                    None
                }
                Some(dt) => {
                    taps.intervals.push(dt.as_secs_f32());
                    if taps.intervals.len() > TAP_WINDOW {
                        taps.intervals.remove(0);
                    }
                    let avg = taps.intervals.iter().sum::<f32>() / taps.intervals.len() as f32;
                    // 40–300 BPM keeps stray double-taps from producing
                    // nonsense tempos.
                    Some((60.0 / avg).clamp(40.0, 300.0))
                }
            }
        };
        if let Some(bpm) = new_tempo {
            *self.tempo.lock_ok() = Some(bpm);
            self.mark_state_dirty();
            self.apply_tempo_to_delays();
            self.events.publish(RigEvent::Perf(Rig::perf(self)));
        }
    }

    fn set_block_bypass(&self, id: String, bypassed: bool) {
        let current = self
            .blocks
            .lock_ok()
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.bypassed);
        if current == Some(bypassed) {
            return;
        }
        Rig::toggle_block_bypass(self, id);
    }

    fn toggle_block_bypass(&self, id: String) {
        let new_bypass = {
            let mut blocks = self.blocks.lock_ok();
            blocks.iter_mut().find(|b| b.id == id).map(|b| {
                b.bypassed = !b.bypassed;
                (b.bypassed, b.block_type)
            })
        };
        if let Some((byp, bt)) = new_bypass {
            {
                let mut guard = self.rig.lock_ok();
                if let Some(prig) = guard.as_mut() {
                    // Persist into the patch config so FX-bypass cycles and
                    // re-activation restore this toggle, not the build-time
                    // default.
                    prig.set_block_config_bypass(&id, byp);
                    // While the global FX (time) bypass is engaged, un-bypassing
                    // a time block only updates its configured state — the
                    // engine slot stays muted until FX comes back.
                    let is_time = bt.category() == signal_proto::block::BlockCategory::Time;
                    let effective = byp || (is_time && prig.fx_bypass());
                    prig.rig().set_block_slot_bypass(&id, effective);
                }
            }
            self.record_patch_override(&id, None, if byp { 1.0 } else { 0.0 });
        }
        self.publish_state();
    }

    fn set_block_param(&self, id: String, param: String, value: f32) {
        self.record_patch_override(&id, Some(&param), value);
        // Constant-loudness drive: on NAM board blocks the drive knob is
        // realised as a compensated input/output trim pair.
        if param == "drive" {
            let block = self
                .blocks
                .lock_ok()
                .iter()
                .find(|b| b.id == id)
                .map(|b| (b.name.clone(), b.block_type));
            if let Some((name, bt)) = block {
                if matches!(bt, BlockType::Drive | BlockType::Boost | BlockType::Amp) {
                    self.apply_drive(&id, &name, value);
                }
            }
        }
        let mut retime_delay = false;
        {
            let mut blocks = self.blocks.lock_ok();
            if let Some(b) = blocks.iter_mut().find(|b| b.id == id) {
                if b.param_name.as_deref() == Some(param.as_str()) {
                    b.param_value = value;
                }
                if let Some(p) = b.params.iter_mut().find(|p| p.name == param) {
                    p.value = value;
                }
                retime_delay = b.block_type == BlockType::Delay && param.starts_with("tap_div");
            }
        }
        if retime_delay {
            // A note-division change re-times the delay from the tempo.
            self.apply_tempo_to_delays();
        }
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.rig().set_active_block_param(&id, &param, value);
            }
        }
        // Chain state only — param drags shouldn't re-publish the perf model.
        self.events.publish(RigEvent::Chain(Rig::chain(self)));
    }
}

impl RigStreamSource for GuitarRigBackend {
    fn events_hub(&self) -> &PubSub<RigEvent> {
        &self.events
    }
}

impl AudioSettings for GuitarRigBackend {
    fn devices(&self) -> AudioDevices {
        AudioDevices {
            inputs: GuitarRig::input_devices().into_iter().map(map_device).collect(),
            outputs: GuitarRig::output_devices().into_iter().map(map_device).collect(),
        }
    }

    fn prefs(&self) -> AudioPrefs {
        let mgr = RigManager::load(AUDIO_RIG_NAME);
        let a = &mgr.audio;
        AudioPrefs {
            input_device: a.input_device.clone(),
            input_channel: a.input_channel as u32,
            output_device: a.output_device.clone(),
            sample_rate: a.sample_rate,
            buffer_size: a.buffer_size,
        }
    }

    fn save_prefs(&self, prefs: AudioPrefs) {
        let mut mgr = RigManager::load(AUDIO_RIG_NAME);
        mgr.audio.input_device = prefs.input_device;
        mgr.audio.input_channel = prefs.input_channel as usize;
        mgr.audio.output_device = prefs.output_device;
        mgr.audio.sample_rate = prefs.sample_rate;
        mgr.audio.buffer_size = prefs.buffer_size;
        if let Err(e) = mgr.save() {
            tracing::error!("failed to save audio prefs: {e}");
        }
    }
}

impl Services for GuitarRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_guitar_proto::rig::Service,
            signal_guitar_proto::rig::StreamService,
            signal_guitar_proto::audio::Service
        ]
    }
}

// ── Tuner pitch detection ────────────────────────────────────────────────────

/// Detect the fundamental of `samples` (mono, `rate` Hz) via normalized
/// autocorrelation over the guitar range (60–500 Hz), with an
/// octave-error guard (prefer the shortest strong lag) and parabolic
/// interpolation for sub-sample precision. `None` = too quiet / no lock.
fn detect_pitch(samples: &[f32], rate: f32) -> Option<f32> {
    let n = samples.len();
    if n < 2048 || rate <= 0.0 {
        return None;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    if rms < 0.003 {
        return None;
    }
    let mean = samples.iter().sum::<f32>() / n as f32;
    let buf: Vec<f32> = samples.iter().map(|s| s - mean).collect();
    let e0: f32 = buf.iter().map(|s| s * s).sum();
    if e0 <= f32::EPSILON {
        return None;
    }

    let min_lag = (rate / 500.0).floor().max(2.0) as usize;
    let max_lag = ((rate / 60.0).ceil() as usize).min(n / 2);
    let acf = |lag: usize| -> f32 {
        let mut ac = 0.0f32;
        for i in 0..(n - lag) {
            ac += buf[i] * buf[i + lag];
        }
        ac / e0
    };

    let mut best_lag = 0usize;
    let mut best = 0.0f32;
    let mut norms = vec![0.0f32; max_lag + 2];
    for (lag, slot) in norms.iter_mut().enumerate().take(max_lag).skip(min_lag) {
        let v = acf(lag);
        *slot = v;
        if v > best {
            best = v;
            best_lag = lag;
        }
    }
    if best < 0.5 || best_lag == 0 {
        return None;
    }
    // Octave guard: the true period is the SHORTEST lag nearly as strong as
    // the best (ACF peaks repeat at every multiple of the period).
    let mut lag = best_lag;
    for l in min_lag..best_lag {
        // Only consider local peaks.
        if norms[l] >= 0.85 * best && norms[l] >= norms[l - 1] && norms[l] >= norms[l + 1] {
            lag = l;
            break;
        }
    }
    // Parabolic interpolation around the chosen lag.
    let (ym1, y0, yp1) = (acf(lag - 1), norms[lag].max(acf(lag)), acf(lag + 1));
    let denom = ym1 - 2.0 * y0 + yp1;
    let delta = if denom.abs() > f32::EPSILON {
        (0.5 * (ym1 - yp1) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some(rate / (lag as f32 + delta))
}

/// Nearest note name (with octave) + distance in cents.
fn note_and_cents(freq: f32) -> (String, f32) {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let midi = 69.0 + 12.0 * (freq / 440.0).log2();
    let nearest = midi.round();
    let cents = ((midi - nearest) * 100.0).clamp(-50.0, 50.0);
    let n = nearest as i32;
    let name = NAMES[(n.rem_euclid(12)) as usize];
    let octave = n / 12 - 1;
    (format!("{name}{octave}"), cents)
}

// ── Spectrum analysis ────────────────────────────────────────────────────────

/// Hann-windowed radix-2 FFT → `bins` log-spaced magnitude bins (dB,
/// −90..0) over 20 Hz–20 kHz. Small N (2048) at ~15 Hz on the control
/// thread — no RT involvement.
fn spectrum_bins(samples: &[f32], rate: f32, bins: usize) -> Vec<f32> {
    let n = samples.len().next_power_of_two() / 2 * 2;
    let n = n.min(samples.len());
    // Hann window into complex buffer.
    let mut re: Vec<f32> = (0..n)
        .map(|i| {
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
            samples[i] * w
        })
        .collect();
    let mut im = vec![0.0f32; n];
    // Iterative radix-2 Cooley–Tukey.
    let levels = n.trailing_zeros();
    // Bit-reversal permutation.
    for i in 0..n {
        let j = (i as u32).reverse_bits() >> (32 - levels);
        let j = j as usize;
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let step = -2.0 * std::f32::consts::PI / size as f32;
        for start in (0..n).step_by(size) {
            for k in 0..half {
                let ang = step * k as f32;
                let (wr, wi) = (ang.cos(), ang.sin());
                let (i, j) = (start + k, start + k + half);
                let (tr, ti) = (re[j] * wr - im[j] * wi, re[j] * wi + im[j] * wr);
                re[j] = re[i] - tr;
                im[j] = im[i] - ti;
                re[i] += tr;
                im[i] += ti;
            }
        }
        size *= 2;
    }
    // Log-spaced bins 20 Hz .. 20 kHz, peak magnitude per bin.
    let mut out = vec![-90.0f32; bins];
    let hz_per = rate / n as f32;
    for (b, slot) in out.iter_mut().enumerate() {
        let f0 = 20.0 * (1000.0f32).powf(b as f32 / bins as f32);
        let f1 = 20.0 * (1000.0f32).powf((b + 1) as f32 / bins as f32);
        let (k0, k1) = (
            ((f0 / hz_per) as usize).clamp(1, n / 2 - 1),
            ((f1 / hz_per) as usize).clamp(1, n / 2 - 1),
        );
        let mut peak = 0.0f32;
        for k in k0..=k1 {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / (n as f32 / 4.0);
            peak = peak.max(mag);
        }
        *slot = if peak > 0.0 {
            (20.0 * peak.log10()).clamp(-90.0, 0.0)
        } else {
            -90.0
        };
    }
    out
}
