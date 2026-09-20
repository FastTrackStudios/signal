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
    AudioDevice, AudioDevices, AudioPrefs, BlockParam, HeadphoneState, LiveBlock, LiveNode,
    LivePreset, PatchInfo, PerfPart, PerfStack, PerformanceModel, PresetInfo, RigStatus,
    TunerReading,
};
use signal_proto::block::BlockType;
use signal_sampler::{DeviceInfo, GuitarRig, ProfileRig, RigBlock, RigManager};

use crate::library::RigLibrary;
use crate::nodes::profile_from_library;
use crate::profiles::{DriveImport, DrivePresetDef, ProfileDef, SetlistDef, SongDef};

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
pub struct MeterPump {
    /// The open MIDI capture (all input ports merged); `None` until a port
    /// exists. Reopened by the hot-plug scan.
    /// Drain-style subscription to the shared process-wide MIDI hub. The
    /// rig no longer opens its own 23 OS clients; the hub opens each port
    /// once for the whole app and this pump drains its share.
    midi: Option<signal_rig_host::midi_hub::MidiDrain>,
    tick: u64,
    /// Footswitch tap/hold/edge state (the shared gesture engine).
    switches: FootswitchEngine,
}

impl Default for MeterPump {
    fn default() -> Self {
        Self {
            midi: None,
            tick: 0,
            // 5 gesture switches + 5 direct slots, holds at the 500 ms
            // pedalboard convention (mirrors the UI's hold threshold).
            switches: FootswitchEngine::new(5, 5, std::time::Duration::from_millis(500)),
        }
    }
}

/// Shared live rig (a [`ProfileRig`] wrapping the [`GuitarRig`]).
type SharedRig = Arc<Mutex<Option<ProfileRig>>>;

use signal_rig_host::gestures::{FootswitchAction, FootswitchEngine, FootswitchMap};
use signal_rig_host::lock::{LockExt, panic_message};

/// The headless rig session: live audio + profile/footswitch state, shared
/// behind `Arc`s so service calls can arrive from any thread.
///
/// `ProfileRig` is `Send` but not `Sync` (the pipewire backend owns a
/// `*mut pw_thread_loop`), so it lives in a `Mutex` and calls serialize through it.
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
    /// The active patch, when there is no engine holding one (design mode).
    ///
    /// Not a second source of truth: with a rig open, the engine's
    /// `active_patch()` is the answer and this is unread. It exists because
    /// design mode has no engine and a UI still has to know which patch is lit.
    design_patch: Arc<Mutex<String>>,
    /// The last patch-levelling pass — progress while it runs, results after.
    levelling: Arc<Mutex<signal_guitar_proto::LevelProgress>>,
    /// Set while a levelling pass is in flight, so a second press does not
    /// start a second pass over the same patches.
    levelling_busy: Arc<std::sync::atomic::AtomicBool>,
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
            design_patch: Arc::new(Mutex::new(String::new())),
            levelling: Arc::new(Mutex::new(signal_guitar_proto::LevelProgress::default())),
            levelling_busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        // Design mode opens no MIDI: the hub creates a graph node and claims
        // every input port, so two copies would fight over the footswitch and
        // clutter the patchbay of whatever rig is actually playing.
        if !crate::library::rig_is_design()
            && (pump.tick == 1 || pump.tick.is_multiple_of(60))
        {
            // Subscribe once, then let the hub own re-opening. The pump used
            // to call `rescan_stream`, which reopened all 23 ports whenever
            // the ordered port list differed — an unstable enumeration order
            // alone was enough to trigger it.
            let hub = signal_rig_host::midi_hub::hub();
            if pump.midi.is_none() {
                pump.midi = Some(hub.subscribe_drain("guitar", None));
            }
            hub.rescan();
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
                        controller, value, ..
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
            // Remappable mapping (midi.styx) → the shared gesture engine:
            // gesture switches (tap on short release, hold at 500 ms) +
            // direct hold-layer slots (fire on press).
            let map = {
                let m = self.midi_map.lock_ok();
                FootswitchMap {
                    tap_ccs: m.tap_ccs.clone(),
                    direct: m.direct.iter().map(|d| (d.cc, d.slot)).collect(),
                }
            };
            let mut actions: Vec<FootswitchAction> = events
                .into_iter()
                .filter_map(|(cc, val)| pump.switches.on_cc(&map, cc, val))
                .collect();
            actions.extend(pump.switches.poll_holds());
            for action in actions {
                match action {
                    FootswitchAction::Tap(4) => {
                        tracing::info!("footswitch 5 tap");
                        Rig::tap_tempo(self);
                    }
                    FootswitchAction::Tap(sw) => {
                        tracing::info!("footswitch {} tap", sw + 1);
                        Rig::press_stack(self, sw as u32);
                    }
                    FootswitchAction::Hold(sw) => {
                        tracing::info!("footswitch {} hold", sw + 1);
                        self.hold_layer_action(sw);
                    }
                    FootswitchAction::Direct(slot) => {
                        tracing::info!("footswitch direct → slot {slot}");
                        self.hold_layer_action(slot as usize);
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
        if has_comp {
            fx_blocks::comp_meter::gr_db().max(0.0)
        } else {
            0.0
        }
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

    /// Add a pedal capture to the drive library as an option of `group`,
    /// creating that preset the first time the tone is seen and claiming a
    /// free drive slot for it.
    ///
    /// Re-importing a file already in the preset is a no-op rather than a
    /// duplicate option — a download the user repeats is the same capture.
    fn import_drive_capture(&self, group: &str, option: &str, nam_path: &str, hash: &str) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let mut dps = self.drive_presets.lock_ok();
            let outcome = crate::profiles::import_drive_capture(
                &mut def, &mut dps, group, option, nam_path, hash,
            );
            match &outcome {
                // A file the library already holds is not a change, so
                // there is nothing to persist and nothing to rebuild for.
                DriveImport::AlreadyPresent => {
                    tracing::info!("import: '{group}' already holds this capture");
                    return;
                }
                DriveImport::Option { preset } => {
                    tracing::info!("import: '{option}' added to '{preset}'");
                }
                DriveImport::Slot { preset, block } => {
                    tracing::info!("import: '{preset}' → {block}");
                }
                DriveImport::NoFreeSlot { preset } => {
                    tracing::warn!("import: '{preset}' has no free drive slot to claim");
                }
            }
            RigLibrary::save_drive_presets(&dps);
            // Only a claimed slot changes the profile; the other outcomes
            // touch the drive library alone.
            if matches!(outcome, DriveImport::Slot { .. }) {
                RigLibrary::save_profile(&def);
            }
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
        self.spawn_drive_calibration();
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
            let preset = pool_preset_of(&def, &active)?;
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
            .map_or(48_000.0, |p| f64::from(p.sample_rate()));
        let Some((in_db, out_db)) = signal_sampler::nam_calibrate::drive_compensation(
            std::path::Path::new(&path),
            sr,
            drive,
        ) else {
            tracing::warn!("drive compensation unavailable for {block_name} — leaving trims");
            return;
        };
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.rig()
                    .set_active_block_param(block_id, "input_trim", in_db);
                prig.rig()
                    .set_active_block_param(block_id, "output_trim", out_db);
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
                matches!(
                    b.block_type,
                    BlockType::Drive | BlockType::Boost | BlockType::Amp
                )
            })
            .map(|b| {
                let drive = b
                    .params
                    .iter()
                    .find(|p| p.name == "drive")
                    .map_or(0.5, |p| p.value);
                (b.id.clone(), b.name.clone(), drive)
            })
            .collect();
        for (id, name, drive) in blocks {
            self.apply_drive(&id, &name, drive);
        }
    }

    /// Measure every patch through its whole chain and trim each one to a
    /// common loudness.
    ///
    /// Runs on its own thread — rendering a chain with three NAM blocks against
    /// ~2 seconds of DI is far from realtime, and there are as many patches as
    /// the profile holds.
    ///
    /// The trim goes into the patch definition rather than being applied live,
    /// for the same reason the drive calibration is cached: it is a measured
    /// property of that chain, it does not change until the chain does, and a
    /// player should be able to see it, edit it and keep it.
    fn run_levelling(&self) {
        let sample_rate = self
            .rig
            .lock_ok()
            .as_ref()
            .map_or(48_000, signal_sampler::rig_profile::ProfileRig::sample_rate);

        // The chains as built, paired with the patch definition they came from.
        // Taken from the live rig rather than the definition because the
        // installed chain is what is actually heard — overrides included.
        let chains: Vec<(String, Vec<signal_sampler::rig::RigBlock>)> = {
            let guard = self.rig.lock_ok();
            let Some(prig) = guard.as_ref() else {
                tracing::warn!("patch levelling needs an open rig");
                return;
            };
            prig.patches()
                .iter()
                .map(|p| {
                    let blocks = p
                        .chain
                        .iter()
                        .filter(|b| b.has_backend())
                        .cloned()
                        .collect();
                    (p.name.clone(), blocks)
                })
                .collect()
        };

        let total = chains.len() as u32;
        {
            let mut progress = self.levelling.lock_ok();
            *progress = signal_guitar_proto::LevelProgress {
                done: 0,
                total,
                patch: String::new(),
                complete: false,
                results: Vec::new(),
            };
        }
        self.publish_levelling();
        tracing::info!(patches = total, "patch levelling: begin");

        let mut measured: Vec<(String, f32, f32)> = Vec::with_capacity(chains.len());
        for (name, blocks) in chains {
            {
                let mut progress = self.levelling.lock_ok();
                progress.patch = name.clone();
            }
            self.publish_levelling();

            let measurement = signal_sampler::patch_level::level_of(&blocks, sample_rate)
                .filter(|lufs| lufs.is_finite());
            let Some(lufs) = measurement else {
                // No measurement means no trim. The tempting thing is to treat
                // silence as "very quiet" and apply the maximum makeup, which
                // is the worst possible answer: the patch is not quiet, it did
                // not render, and +24 dB on the one patch that *does* play
                // would be the loudest mistake the rig could make.
                tracing::warn!(
                    patch = %name,
                    "patch levelling: chain did not render — leaving its trim alone"
                );
                let mut progress = self.levelling.lock_ok();
                progress.done += 1;
                progress.results.push(signal_guitar_proto::PatchLevel {
                    patch: name,
                    lufs: f32::NEG_INFINITY,
                    trim_db: f32::NAN,
                });
                drop(progress);
                self.publish_levelling();
                continue;
            };
            let trim = (signal_sampler::patch_level::TARGET_LUFS - lufs) as f32;
            // A wide but finite range: a patch needing more than this is a
            // patch built wrong, and 30 dB of makeup would only amplify noise.
            let trim = trim.clamp(-24.0, 24.0);
            tracing::info!(patch = %name, lufs, trim_db = trim, "patch levelling: measured");
            measured.push((name.clone(), lufs as f32, trim));
            {
                let mut progress = self.levelling.lock_ok();
                progress.done += 1;
                progress.results.push(signal_guitar_proto::PatchLevel {
                    patch: name,
                    lufs: lufs as f32,
                    trim_db: trim,
                });
            }
            self.publish_levelling();
        }

        // Write the trims into the definition, then rebuild so they are live.
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            for (name, _, trim) in &measured {
                if let Some(patch) = def
                    .patches
                    .iter_mut()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                {
                    patch.trim_db = *trim;
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);

        {
            let mut progress = self.levelling.lock_ok();
            progress.complete = true;
            progress.patch.clear();
        }
        self.publish_levelling();
        tracing::info!(
            patches = measured.len(),
            target_lufs = signal_sampler::patch_level::TARGET_LUFS,
            "patch levelling: done"
        );
    }

    fn publish_levelling(&self) {
        let progress = self.levelling.lock_ok().clone();
        self.events
            .publish(RigEvent::Levelling(progress));
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
                .map_or(48_000.0, |p| f64::from(p.sample_rate()));
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
                *self.setlist_index.lock_ok() = (st.setlist_index as usize).min(sets.len() - 1);
            }
        }
        let resolved = self.resolved_setlist();
        let song = (st.song_index as usize).min(resolved.len().saturating_sub(1));
        *self.song_index.lock_ok() = if resolved.is_empty() { 0 } else { song };
        let parts = resolved
            .get(song)
            .map_or(0, |(_, _, _, _, parts)| parts.len());
        *self.part_index.lock_ok() = (st.part_index as usize).min(parts.saturating_sub(1));
        if st.tempo_bpm > 0.0 {
            *self.tempo.lock_ok() = Some(st.tempo_bpm.clamp(40.0, 300.0));
        }
        if !st.active_patch.is_empty() {
            // Recorded whether or not an engine exists: design mode reads it
            // from here, since it has nothing to activate.
            *self.design_patch.lock_ok() = st.active_patch.clone();
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
                let base = prig.active_patch().map_or(0.0, |p| p.output_trim_db);
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
    /// Drop the active patch's overrides of a block — one parameter, or all
    /// of them — and rebuild so the values return to what the chain builds.
    ///
    /// The other half of [`record_patch_override`](Self::record_patch_override),
    /// which every knob move calls. Without it a patch could only ever
    /// accumulate changes.
    fn clear_overrides(&self, block_id: &str, param: Option<&str>) {
        let block_name = {
            let blocks = self.blocks.lock_ok();
            blocks
                .iter()
                .find(|b| b.id == block_id)
                .map(|b| b.name.clone())
        };
        let Some(block_name) = block_name else { return };
        let active = {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        let Some(patch_name) = active else { return };

        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(patch) = def
                .patches
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&patch_name))
            else {
                return;
            };
            let before = patch.overrides.len();
            patch.overrides.retain(|o| {
                if !o.block.eq_ignore_ascii_case(&block_name) {
                    return true;
                }
                match param {
                    Some(name) => !(o.op == "set" && o.param.eq_ignore_ascii_case(name)),
                    None => false,
                }
            });
            if patch.overrides.len() == before {
                return;
            }
            tracing::info!(
                patch.name = %patch_name,
                block.name = %block_name,
                param = param.unwrap_or("*"),
                cleared = before - patch.overrides.len(),
                "guitar: patch override cleared"
            );
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    fn record_patch_override(&self, block_id: &str, param: Option<&str>, value: f32) {
        let block = {
            let blocks = self.blocks.lock_ok();
            blocks
                .iter()
                .find(|b| b.id == block_id)
                .map(|b| (b.name.clone(), b.block_type))
        };
        let Some((block_name, bt)) = block else {
            return;
        };
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

    /// Recall the active patch's boost setting (`PatchDef.boost_db)`: a patch
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
            .map_or(0.0, |p| p.boost_db);
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

    /// Step stack `index` the way `activate_stack` would: onto its current
    /// patch, or to the next one if that patch is already live.
    fn rotate_design_stack(&self, index: usize) {
        let next = {
            let def = self.profile_def.lock_ok();
            let Some(stack) = def.stacks.get(index) else {
                return;
            };
            if stack.patches.is_empty() {
                return;
            }
            let live = self.design_patch.lock_ok().clone();
            let at = stack
                .patches
                .iter()
                .position(|p| p.eq_ignore_ascii_case(&live));
            let pos = match at {
                // Already on this stack — rotate.
                Some(i) => (i + 1) % stack.patches.len(),
                // Coming from elsewhere — land on where the stack is pointing.
                None => 0,
            };
            stack.patches[pos].clone()
        };
        *self.design_patch.lock_ok() = next;
    }

    /// Seconds since the process started — the fake instrument's clock.
    ///
    /// Wall clock rather than a tick count so the instrument runs at the same
    /// speed whatever rate the pump happens to be publishing at.
    fn design_seconds(&self) -> f32 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
    }

    /// Load the profile and nothing else — see [`open_blocking`](Self::open_blocking).
    fn open_for_design(&self) {
        tracing::info!("design mode: no audio device, no MIDI, no DSP");
        *self.rig.lock_ok() = None;
        *self.open_prefs.lock_ok() = None;
        self.restore_last_state();
        self.resync_blocks();
        self.publish_state();
    }

    /// The active patch's chain, from the definition rather than the engine.
    ///
    /// What the rig would build, without building it. Used by design mode,
    /// where there is no engine to mirror — the block ids are the chain
    /// position rather than engine slot ids, which is enough for every UI that
    /// addresses a block by id, since every write in design mode lands in the
    /// definition too.
    fn design_blocks(&self) -> Vec<LiveBlock> {
        let wanted = self.design_patch.lock_ok().clone();
        let def = self.profile_def.lock_ok();
        let dps = self.drive_presets.lock_ok();
        let profile = profile_from_library(&def, &dps);
        let active = Some(wanted)
            .filter(|n| !n.is_empty())
            .and_then(|name| {
                profile
                    .patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                    .cloned()
            })
            .or_else(|| profile.patches.first().cloned());
        let Some(patch) = active else {
            return Vec::new();
        };
        patch
            .chain
            .iter()
            .filter(|b| b.has_backend())
            .enumerate()
            .map(|(i, block)| {
                let name = if block.name.trim().is_empty() {
                    format!("{:?}", block.block_type)
                } else {
                    block.name.clone()
                };
                let (param_name, param_min, param_max, param_value) =
                    match primary_param(block.block_type) {
                        Some((n, mn, mx, dflt)) => (
                            Some(n.to_string()),
                            mn,
                            mx,
                            block.param_f32(n).unwrap_or(dflt),
                        ),
                        None => (None, 0.0, 0.0, 0.0),
                    };
                let params: Vec<BlockParam> = param_specs(block.block_type)
                    .iter()
                    .map(|(pname, min, max, dflt)| BlockParam {
                        name: pname.clone(),
                        value: block.param_f32(pname).unwrap_or(*dflt),
                        min: *min,
                        max: *max,
                        overridden: false,
                    })
                    .collect();
                LiveBlock {
                    id: format!("design-{i}"),
                    block_type: block.block_type,
                    name,
                    // Everything engaged.
                    //
                    // A patch keeps most of its chain bypassed — that is what
                    // a patch IS — and a bypassed block draws unlit, so a
                    // design session spent looking at the rig would be a
                    // session looking at six grey panels. Design mode exists
                    // to see the interface, and you cannot design a surface
                    // you cannot see.
                    //
                    // Not a lie about the rig: nothing here is playing. The
                    // real chain reports its real bypass, because there the
                    // distinction is audible.
                    bypassed: false,
                    param_name,
                    param_value,
                    param_min,
                    param_max,
                    params,
                    preset: String::new(),
                    options: Vec::new(),
                    option: 0,
                    overridden: false,
                }
            })
            .collect()
    }

    /// Activate a footswitch stack and re-sync everything that activation
    /// resets: the block mirror + bypass defaults, the tapped tempo on the
    /// fresh delays, and the boost gain block.
    fn activate_stack_and_sync(&self, index: usize) {
        let t0 = std::time::Instant::now();
        {
            let mut guard = self.rig.lock_ok();
            match guard.as_mut() {
                Some(prig) => prig.activate_stack(index),
                // No engine: do to the definition what activation does to the
                // engine — land on the stack's patch, or rotate if already on
                // it. A footswitch in design mode has to move the UI, or the
                // performance surfaces cannot be designed at all.
                None => {
                    drop(guard);
                    self.rotate_design_stack(index);
                    false
                }
            };
        }
        let audible = t0.elapsed();
        self.sync_after_switch(audible, "stack");
    }

    /// Everything a switch does after the audio has already changed, timed.
    ///
    /// The activate above is the switch a player hears; this is the follow-up
    /// that makes the rest of the rig agree with it — the chain mirror the UI
    /// draws, the delays' tempo, the boost, the drive trims. It is measured
    /// because it is the part that can be slow, and a switch that takes a
    /// second to settle is not a switch a player can use: the trims land after
    /// the note, so the level shifts under them.
    fn sync_after_switch(&self, audible: std::time::Duration, via: &str) {
        let t = std::time::Instant::now();
        self.resync_blocks();
        let resync = t.elapsed();

        let t = std::time::Instant::now();
        self.apply_tempo_to_delays();
        let tempo = t.elapsed();

        let t = std::time::Instant::now();
        self.recall_patch_boost();
        self.apply_boost_to_block();
        let boost = t.elapsed();

        let t = std::time::Instant::now();
        self.apply_all_drives();
        let drives = t.elapsed();

        let t = std::time::Instant::now();
        self.publish_state();
        let publish = t.elapsed();

        self.mark_state_dirty();

        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        tracing::info!(
            switch.via = via,
            switch.audible_ms = ms(audible),
            switch.resync_ms = ms(resync),
            switch.tempo_ms = ms(tempo),
            switch.boost_ms = ms(boost),
            switch.drives_ms = ms(drives),
            switch.publish_ms = ms(publish),
            switch.total_ms = ms(audible) + ms(resync) + ms(tempo) + ms(boost) + ms(drives) + ms(publish),
            "patch switch"
        );
    }

    /// The active setlist's entries, resolved against the song library:
    /// `(name, key, bpm, stack, sections)` per slot — per-set overrides win
    /// over the song's defaults.
    fn resolved_setlist(&self) -> Vec<(String, String, u32, usize, Vec<PerfPart>)> {
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
                    song.map_or(120, |s| s.bpm)
                } else {
                    e.bpm
                };
                let stack = song.map_or(0, |s| s.stack);
                // Each section with what it recalls: the song's `part_recalls`
                // matched by name, empty for a section that is still a label.
                let parts: Vec<PerfPart> = song
                    .map(|s| {
                        s.parts_with_recalls()
                            .into_iter()
                            .map(|(name, patch)| PerfPart { name, patch })
                            .collect()
                    })
                    .unwrap_or_default();
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
            let already_active = self.rig.lock_ok().as_ref().is_some_and(|prig| {
                prig.stacks().get(stack).is_some_and(|st| {
                    st.patches.iter().any(|p| {
                        prig.active_patch()
                            .is_some_and(|a| a.name.eq_ignore_ascii_case(p))
                    })
                })
            });
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
        // Design mode opens nothing.
        //
        // No device, no MIDI node, no DSP — so two of these can run side by
        // side, which is the whole point: an interface is exclusive, and a
        // screen cannot be laid out against a rig that will not start because
        // another copy already holds the hardware.
        //
        // The profile still loads, because a UI with no patches, presets or
        // chain is not the UI. Everything downstream reads the definition
        // instead of the engine: `perf` already has a static path for a rig
        // that is not open, `chain` gets one here, and the meters are
        // synthesised by the pump.
        if crate::library::rig_is_design() {
            self.open_for_design();
            return;
        }
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
                    if mgr.audio.input_device.is_empty() {
                        "default"
                    } else {
                        &mgr.audio.input_device
                    },
                    mgr.audio.input_channel + 1,
                    if mgr.audio.output_device.is_empty() {
                        "default"
                    } else {
                        &mgr.audio.output_device
                    },
                );
                let mut prig = ProfileRig::new(g);
                // One loudness authority: the per-block drive calibration
                // (unity-loudness blocks). The old patch-level match would
                // stack a second, different target on top.
                prig.set_level_match(false);
                let profile = {
                    let def = self.profile_def.lock_ok();
                    let dps = self.drive_presets.lock_ok();
                    profile_from_library(&def, &dps)
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
        // A silent run processes everything and is heard by nobody: mute is a
        // −96 dB trim on the master, after the chain, so the DSP load a
        // benchmark measures is the load a player pays. Engaged here rather
        // than left to the caller so it is on before the first block, not a
        // few hundred milliseconds of full-volume audio later.
        if crate::library::rig_is_silent() {
            self.headphone.lock_ok().main_mute = true;
            tracing::info!("silent run — main output muted, chain still processing");
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
        // No engine to mirror — build the chain the definition describes.
        if self.rig.lock_ok().is_none() && crate::library::rig_is_design() {
            *self.blocks.lock_ok() = self.design_blocks();
            return;
        }
        let mut out = Vec::new();
        // The active patch's overrides, read once: what the player has moved
        // away from the chain as built.
        let active_overrides: Vec<crate::profiles::OverrideDef> = {
            let active = self
                .rig
                .lock_ok()
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
            active
                .and_then(|name| {
                    self.profile_def
                        .lock_ok()
                        .patches
                        .iter()
                        .find(|p| p.name.eq_ignore_ascii_case(&name))
                        .map(|p| p.overrides.clone())
                })
                .unwrap_or_default()
        };
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
                                Some((n, mn, mx, dflt)) => (
                                    Some(n.to_string()),
                                    mn,
                                    mx,
                                    block.param_f32(n).unwrap_or(dflt),
                                ),
                                None => (None, 0.0, 0.0, 0.0),
                            };
                        let name = if block.name.trim().is_empty() {
                            format!("{:?}", block.block_type)
                        } else {
                            block.name.clone()
                        };
                        // What the active patch has moved away from the
                        // built chain. Every knob move is recorded as an
                        // override the instant it happens, so this is the
                        // only way a player can see what they changed.
                        let overrides: Vec<(String, String)> = active_overrides
                            .iter()
                            .filter(|o| o.block.eq_ignore_ascii_case(&name))
                            .map(|o| (o.op.clone(), o.param.clone()))
                            .collect();
                        let is_overridden = |param: &str| {
                            overrides
                                .iter()
                                .any(|(op, p)| op == "set" && p.eq_ignore_ascii_case(param))
                        };
                        let params: Vec<BlockParam> = param_specs(block.block_type)
                            .iter()
                            .map(|(pname, min, max, dflt)| BlockParam {
                                name: pname.clone(),
                                value: block.param_f32(pname).unwrap_or(*dflt),
                                min: *min,
                                max: *max,
                                overridden: is_overridden(pname),
                            })
                            .collect();
                        let block_overridden = !overrides.is_empty();
                        // Drive slots: surface the loaded drive preset and
                        // its NAM options for the board's quick switch.
                        // Drive slots: surface the loaded drive preset and its
                        // NAM options. Amps: the pool preset the active patch
                        // points at, and the whole pool as the alternatives —
                        // an amp block has no preset of its own, its identity
                        // IS the patch's preset, so that is what the board
                        // must name. Without this an amp chunk had an empty
                        // preset and no options, which is why it could only
                        // fall back to printing its slot name.
                        let (preset, options, option) = {
                            let def = self.profile_def.lock_ok();
                            if let Some(d) = def
                                .drives
                                .iter()
                                .find(|d| d.block.eq_ignore_ascii_case(&name))
                            {
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
                                    .unwrap_or_default()
                            } else if block.block_type == BlockType::Amp {
                                let current =
                                    pool_preset_of(&def, &patch.name).unwrap_or_default();
                                let pool: Vec<String> =
                                    def.presets.iter().map(|p| p.name.clone()).collect();
                                let index = pool
                                    .iter()
                                    .position(|p| p.eq_ignore_ascii_case(&current))
                                    .unwrap_or(0) as u32;
                                (current, pool, index)
                            } else {
                                <(String, Vec<String>, u32)>::default()
                            }
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
                            overridden: block_overridden,
                        });
                    }
                }
            }
        }
        *self.blocks.lock_ok() = out;
    }
}

/// Which pool preset a patch points at — the amp, by name.
///
/// An amp block carries no preset of its own: the patch names one from the
/// pool, and that is the tone in the amp slot. Anything that shows or changes
/// "the current amp" resolves it here, so the board and the capture lookup
/// cannot disagree about what is loaded.
///
/// A free function over the definition, taking the patch by name, because the
/// callers do not agree about what they already hold. `resync_blocks` walks
/// the chain with the rig mutex held and the active patch in hand; a method
/// that re-locked the rig to find that same patch deadlocked on the spot —
/// `std::sync::Mutex` is not reentrant — and took the chain publish, the
/// delay tempo, the boost recall and the drive calibration down with it.
fn pool_preset_of(def: &crate::profiles::ProfileDef, patch: &str) -> Option<String> {
    def.patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(patch))
        .map(|p| p.preset.clone())
}

/// Every controllable param for a block type: `(name, min, max, default)` —
/// mirrors the native DSP's param specs (fx-blocks). The Control view's
/// panels render from this; values come from the patch's build-time params
/// (incl. overrides) and live edits.
fn param_specs(bt: BlockType) -> Vec<(String, f32, f32, f32)> {
    fn owned(t: &[(&str, f32, f32, f32)]) -> Vec<(String, f32, f32, f32)> {
        t.iter()
            .map(|(n, a, b, c)| (n.to_string(), *a, *b, *c))
            .collect()
    }
    match bt {
        // The full FTS-EQ param surface — one source of truth with the
        // DSP (bands + slope + dynamics + masters, from eq_param_range).
        BlockType::Eq => (0..fx_blocks::EQ_PARAM_COUNT)
            .filter_map(|id| {
                let name = fx_blocks::eq_param_name_of(id)?;
                let (min, max, default) = fx_blocks::eq_param_range(id);
                Some((name, min as f32, max as f32, default as f32))
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
        // The TimeLine-MX delay surface (fx-blocks DELAY_PARAMS subset the
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
const fn primary_param(bt: BlockType) -> Option<(&'static str, f32, f32, f32)> {
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
fn build_perf_model_static(def: &ProfileDef, live: &str) -> PerformanceModel {
    let stacks = def
        .stacks
        .iter()
        .map(|st| {
            // Where this stack is pointing. With a `live` patch — design mode,
            // which has a real active patch and no engine — the stack holding
            // it shows it and reads as active, so the footswitch grid responds
            // to a press. Without one (no interface plugged in yet) every stack
            // rests at its first patch, which is what it will land on.
            let at = st
                .patches
                .iter()
                .position(|p| !live.is_empty() && p.eq_ignore_ascii_case(live));
            let pos = at.unwrap_or(0);
            let cur = st.patches.get(pos).cloned().unwrap_or_default();
            let patch_def = def
                .patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&cur));
            PerfStack {
                name: st.name.clone(),
                current_patch: patch_display(&st.name, &cur),
                position: pos as u32,
                patch_count: st.patches.len() as u32,
                available: at.is_some(),
                is_active: at.is_some(),
                preset: patch_def.map(|p| p.preset.clone()).unwrap_or_default(),
                override_modules: patch_def
                    .map(super::profiles::PatchDef::override_modules)
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
                .is_some_and(|i| prig.is_patch_available(i));
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
                    .map(super::profiles::PatchDef::override_modules)
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
        songs: Vec::new(), // filled in by the service (setlist lives outside prig)
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

/// The resolved tree as the flat, depth-tagged list the wire carries.
///
/// A node's **presets are its variants**, and only where there is more than
/// one: a node with a single default has nothing to choose between, and
/// offering a picker with one entry is noise.
fn flatten_nodes(
    node: &signal_proto::node_resolve::Resolved,
    rig: &crate::nodes::RigNodes,
    depth: u32,
    out: &mut Vec<LiveNode>,
) {
    use signal_proto::node_resolve::ResolvedContent;

    let library_node = rig.library.get(&node.id);
    let presets: Vec<LivePreset> = library_node
        .filter(|n| n.variants.len() > 1)
        .map(|n| {
            n.variants
                .iter()
                .map(|v| LivePreset {
                    id: v.id.as_str().to_string(),
                    name: v.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let preset_id =
        library_node.map_or_else(String::new, |n| n.default_variant.as_str().to_string());

    let (is_block, block_type) = match &node.content {
        ResolvedContent::Leaf { block_type, .. } => (true, Some(*block_type)),
        ResolvedContent::Children(_) => (false, None),
    };

    // What else could sit here. A drive slot can hold any pedal in the
    // library — including ones not currently on the board — which is the
    // choice the profile has always had and no surface has ever offered.
    let alternatives: Vec<LivePreset> = if rig.slot_of_node(node.id.as_str()).is_some() {
        rig.pedals()
            .into_iter()
            .map(|(name, id)| LivePreset {
                id: id.as_str().to_string(),
                name,
            })
            .collect()
    } else {
        Vec::new()
    };

    out.push(LiveNode {
        id: node.id.as_str().to_string(),
        name: node.name.clone(),
        role: node.role.tag().to_lowercase(),
        depth,
        is_block,
        block_type,
        bypassed: node.bypassed,
        presets,
        preset_id,
        alternatives,
    });

    if let ResolvedContent::Children(children) = &node.content {
        for child in children {
            flatten_nodes(child, rig, depth + 1, out);
        }
    }
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
        // Design mode has no engine and still has to tick: the meters, the
        // analyser and the compressor trace are the surfaces being designed.
        self.rig.lock_ok().is_some() || crate::library::rig_is_design()
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
        if crate::library::rig_is_design() {
            let t = self.design_seconds();
            self.events
                .publish(RigEvent::Spectrum(crate::design::spectrum(t, 96)));
            // A rolling window of the same instrument, so the compressor's
            // traces sweep rather than sit still.
            let (wave_in, wave_gr) = crate::design::traces(t, 120);
            self.events.publish(RigEvent::CompWave(wave_in, wave_gr));
            return;
        }
        if let Some(bins) = self.input_spectrum() {
            self.events.publish(RigEvent::Spectrum(bins));
        }
        let (wave_in, wave_gr) = fx_blocks::comp_meter::wave_snapshot(3);
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
        // Design mode: the fake instrument, and the patch the definition says
        // is live. Everything else on this payload is genuinely absent — there
        // is no engine to report a block size or a render time, and inventing
        // those would make the DSP strip lie about a rig that is not running.
        if crate::library::rig_is_design() && self.rig.lock_ok().is_none() {
            let f = crate::design::frame(self.design_seconds());
            let patch = self.design_patch.lock_ok().clone();
            return RigStatus {
                running: true,
                input_peak: f.input,
                output_peak: f.output,
                active_patch: Some(patch).filter(|p| !p.is_empty()),
                comp_gr_db: f.gain_reduction_db,
                input_peak_l: f.input,
                input_peak_r: f.input,
                output_peak_l: f.output,
                output_peak_r: f.output,
                perf: signal_guitar_proto::RigPerf::default(),
            };
        }
        let guard = self.rig.lock_ok();
        let (input_peak, output_peak, in_lr, out_lr, active_patch, perf) = match guard.as_ref() {
            Some(prig) => {
                let rig = prig.rig();
                (
                    rig.input_peak(),
                    rig.output_peak(),
                    rig.input_peak_lr(),
                    rig.output_peak_lr(),
                    prig.active_patch().map(|p| p.name.clone()),
                    signal_guitar_proto::RigPerf {
                        block_frames: rig.block_frames(),
                        sample_rate: rig.sample_rate,
                        render_us: rig.render_us(),
                        peak_render_us: rig.peak_render_us(),
                        mean_render_us: rig.mean_render_us(),
                        load: rig.dsp_load(),
                        mean_load: rig.mean_dsp_load(),
                        over_budget: rig.over_budget(),
                        xruns: rig.underruns(),
                        blocks: rig.blocks_rendered(),
                    },
                )
            }
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
            perf,
        }
    }

    fn perf(&self) -> PerformanceModel {
        let mut m = {
            let def = self.profile_def.lock_ok();
            // Live model when the audio rig is open; otherwise the static
            // model from the profile def, so the footswitch stacks still
            // render before the device opens (iOS with no interface yet).
            let live = self.design_patch.lock_ok().clone();
            self.rig.lock_ok().as_ref().map_or_else(
                || build_perf_model_static(&def, &live),
                |prig| build_perf_model(prig, &def),
            )
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
            m.setlists = self
                .setlists
                .lock_ok()
                .iter()
                .map(|s| s.name.clone())
                .collect();
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

    fn nodes(&self) -> Vec<LiveNode> {
        let def = self.profile_def.lock_ok();
        let dps = self.drive_presets.lock_ok();
        let rig = crate::nodes::library_for(&def, &dps);

        // The tree as the *active patch* resolves it, so what the UI lists is
        // what is playing rather than the chain's unbent default.
        let active = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
        let variant = active.as_deref().and_then(|name| rig.patch(name));
        let Ok((resolved, _)) =
            signal_proto::node_resolve::resolve(&rig.library, &rig.chain, variant)
        else {
            return Vec::new();
        };

        let mut out = Vec::new();
        flatten_nodes(&resolved, &rig, 0, &mut out);
        out
    }

    fn clear_block_param(&self, id: String, param: String) {
        self.clear_overrides(&id, Some(&param));
    }

    fn clear_block_overrides(&self, id: String) {
        self.clear_overrides(&id, None);
    }

    fn set_part_patch(&self, part: String, patch: String) {
        let song_name = {
            let idx = *self.song_index.lock_ok();
            self.resolved_setlist()
                .get(idx)
                .map(|(name, ..)| name.clone())
        };
        let Some(song_name) = song_name else { return };
        {
            let mut songs = self.songs_lib.lock_ok();
            let Some(song) = songs
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&song_name))
            else {
                return;
            };
            song.part_recalls
                .retain(|r| !r.part.eq_ignore_ascii_case(&part));
            if !patch.is_empty() {
                song.part_recalls.push(crate::profiles::PartRecallDef {
                    part: part.clone(),
                    patch: patch.clone(),
                });
            }
            tracing::info!(
                song = %song_name,
                part = %part,
                patch = %patch,
                "guitar: section recall set"
            );
            RigLibrary::save_songs(&songs);
        }
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
    }

    fn replace_node(&self, node: String, with: String) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            let rig = crate::nodes::library_for(&def, &dps);

            // Which slot is being filled, and with which pedal. Both are
            // node ids on the wire; the profile stores a slot name and a
            // preset name, so this is where they meet.
            let (Some(slot), Some(pedal)) = (rig.slot_of_node(&node), rig.pedal_name(&with)) else {
                tracing::warn!(
                    node.id = %node,
                    with.id = %with,
                    "guitar: nothing replaceable there — only a drive slot takes another node today"
                );
                return;
            };
            let Some(assigned) = def
                .drives
                .iter_mut()
                .find(|d| d.block.eq_ignore_ascii_case(&slot))
            else {
                return;
            };
            if assigned.preset == pedal {
                return;
            }
            assigned.preset.clone_from(&pedal);
            // A different pedal has its own captures, so the option index
            // from the old one means nothing on it.
            assigned.option = 0;
            tracing::info!(slot = %slot, pedal = %pedal, "guitar: drive slot filled");
            RigLibrary::save_profile(&def);
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    fn save_preset(&self, node: String, name: String) {
        let def = self.profile_def.lock_ok();
        let dps = self.drive_presets.lock_ok();
        let rig = crate::nodes::library_for(&def, &dps);

        // What the node sounds like right now: the active patch's tree, with
        // every override the player has dialled in already applied.
        let active = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
        let variant = active.as_deref().and_then(|name| rig.patch(name));
        let Ok((current, _)) =
            signal_proto::node_resolve::resolve(&rig.library, &rig.chain, variant)
        else {
            tracing::warn!("guitar: preset not saved — the rig did not resolve");
            return;
        };

        let id = signal_proto::node::NodeId::from(node.clone());
        let Some(preset) = crate::node_store::capture(&rig, &current, &id, &name) else {
            tracing::warn!(node.id = %node, "guitar: preset not saved — no such node");
            return;
        };
        let changed = preset.overrides.len();

        let mut store = RigLibrary::load_node_store();
        store.presets.retain(|p| p.id != preset.id);
        store.presets.push(preset);
        RigLibrary::save_node_store(&store);
        tracing::info!(
            node.id = %node,
            preset.name = %name,
            preset.changed = changed,
            "guitar: preset saved"
        );
    }

    fn select_preset(&self, node: String, preset: String) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            let rig = crate::nodes::library_for(&def, &dps);

            // Which drive slot this node is, and which of its captures the
            // variant names. A drive slot's capture lives in the profile —
            // `DriveSlotDef::option` — so that choice is written there, and
            // nowhere else: one fact stored twice is a fact that can disagree
            // with itself.
            let Some((slot, option)) = rig.drive_option_for(&node, &preset) else {
                // Everything else — a preset on a module, a preset someone
                // saved — has no profile field, so it is recorded in the node
                // store and applied over the derived library on every build.
                let mut store = RigLibrary::load_node_store();
                store.select(&node, &preset);
                RigLibrary::save_node_store(&store);
                tracing::info!(node.id = %node, preset.id = %preset, "guitar: preset selected");
                return self.reload_rebuilt(profile_from_library(&def, &dps));
            };
            let Some(assigned) = def
                .drives
                .iter_mut()
                .find(|d| d.block.eq_ignore_ascii_case(&slot))
            else {
                return;
            };
            if assigned.option == option {
                return;
            }
            assigned.option = option;
            tracing::info!(slot = %slot, option, "guitar: drive slot preset selected");
            RigLibrary::save_profile(&def);
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
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
                .map_or(0, |(_, _, _, _, parts)| parts.len().saturating_sub(1));
            (i, last)
        };
        let idx = (index as usize).min(last);
        *self.part_index.lock_ok() = idx;
        self.mark_state_dirty();

        // A section recalls a patch, when it has been given one. That is what
        // makes a section part of the performance rather than a label on it:
        // stepping through a song's sections switches the rig with it.
        let recall = self
            .resolved_setlist()
            .get(song_idx)
            .and_then(|(_, _, _, _, parts)| parts.get(idx).cloned())
            .filter(|part| !part.patch.is_empty());
        if let Some(part) = recall {
            tracing::info!(part = %part.name, patch = %part.patch, "part → patch");
            let switched = {
                let mut guard = self.rig.lock_ok();
                match guard.as_mut() {
                    Some(prig) => activate_patch_by_name(prig, &part.patch),
                    // Design mode: the section still recalls its patch.
                    None => {
                        drop(guard);
                        let known = {
                            let def = self.profile_def.lock_ok();
                            def.patches
                                .iter()
                                .any(|p| p.name.eq_ignore_ascii_case(&part.patch))
                        };
                        if known {
                            *self.design_patch.lock_ok() = part.patch.clone();
                        }
                        known
                    }
                }
            };
            if switched {
                // The same follow-up a footswitch press does, measured the
                // same way — a section recall is a switch a player feels.
                self.sync_after_switch(std::time::Duration::ZERO, "section");
            } else {
                tracing::warn!(
                    part = %part.name,
                    patch = %part.patch,
                    "guitar: section names a patch the profile does not have"
                );
            }
        } else {
            tracing::info!("part → {idx} (song {song_idx})");
        }
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
            let Some(set) = setlists.get_mut(active) else {
                return;
            };
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
                    .is_some_and(|first| first.eq_ignore_ascii_case(&p.name));
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
        // One catalog read for the whole pool, not one per preset: this is
        // called on every performance-model change.
        let catalog = nam_catalog();
        def.presets
            .iter()
            .map(|preset| {
                let provenance = catalog
                    .as_ref()
                    .and_then(|c| c.get_entry(&preset.hash))
                    .and_then(|e| e.provenance.as_ref());
                PresetInfo {
                    name: preset.name.clone(),
                    active: active_preset.as_deref() == Some(preset.name.as_str()),
                    used_by: def
                        .patches
                        .iter()
                        .filter(|p| p.preset.eq_ignore_ascii_case(&preset.name))
                        .count() as u32,
                    creator: provenance
                        .and_then(|p| p.creator.clone())
                        .unwrap_or_default(),
                    license: provenance
                        .and_then(|p| p.license.clone())
                        .unwrap_or_default(),
                    tone_url: provenance
                        .and_then(|p| p.tone_url.clone())
                        .unwrap_or_default(),
                    gear: provenance.and_then(|p| p.gear.clone()).unwrap_or_default(),
                    has_artwork: provenance.is_some_and(|p| p.artwork_path.is_some()),
                }
            })
            .collect()
    }

    fn preset_artwork(&self, preset: String) -> signal_guitar_proto::Artwork {
        use signal_guitar_proto::Artwork;
        let hash = {
            let def = self.profile_def.lock_ok();
            let Some(found) = def
                .presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&preset))
            else {
                return Artwork {
                    error: format!("no preset '{preset}'"),
                    ..Artwork::default()
                };
            };
            found.hash.clone()
        };
        // A preset with no cover is ordinary, not a failure: an empty
        // Artwork says "nothing to draw", which is what a UI needs to know.
        let Some(relative) = nam_catalog()
            .as_ref()
            .and_then(|c| c.get_entry(&hash))
            .and_then(|e| e.provenance.as_ref())
            .and_then(|p| p.artwork_path.clone())
        else {
            return Artwork::default();
        };
        let path = nam_root().join(&relative);
        match std::fs::read(&path) {
            Ok(bytes) => Artwork {
                mime: mime_for_path(&relative).to_string(),
                bytes,
                error: String::new(),
            },
            Err(e) => Artwork {
                error: e.to_string(),
                ..Artwork::default()
            },
        }
    }

    fn level_progress(&self) -> signal_guitar_proto::LevelProgress {
        self.levelling.lock_ok().clone()
    }

    fn level_patches(&self) {
        use std::sync::atomic::Ordering;
        if self
            .levelling_busy
            .swap(true, Ordering::SeqCst)
        {
            tracing::info!("patch levelling already running");
            return;
        }
        let backend = self.clone();
        std::thread::spawn(move || {
            backend.run_levelling();
            backend.levelling_busy.store(false, Ordering::SeqCst);
        });
    }

    fn set_patch_preset(&self, patch: u32, preset: u32) {
        // Repoint in the definition…
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(preset_name) = def.presets.get(preset as usize).map(|p| p.name.clone()) else {
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
                profile_from_library(&def, &dps)
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

        // An amp block has no option list of its own — its alternatives are
        // the preset pool, and choosing one means repointing the patch. Same
        // operation the preset browser performs, so it goes to the same place
        // rather than growing a second way to load an amp.
        if block_name.eq_ignore_ascii_case("Amp L") || block_name.eq_ignore_ascii_case("Amp R") {
            let patch = {
                let def = self.profile_def.lock_ok();
                let guard = self.rig.lock_ok();
                let Some(active) = guard
                    .as_ref()
                    .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
                else {
                    return;
                };
                drop(guard);
                def.patches
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(&active))
            };
            if let Some(patch) = patch {
                self.set_patch_preset(patch as u32, option);
            }
            return;
        }

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
                .map_or(0, |p| p.options.len());
            if n_options == 0 {
                return;
            }
            slot.option = (option as usize).min(n_options - 1);
            tracing::info!("{} → {} option {}", slot.block, slot.preset, slot.option);
            RigLibrary::save_profile(&def);
            {
                let dps = self.drive_presets.lock_ok();
                profile_from_library(&def, &dps)
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
            if def
                .presets
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                tracing::warn!("add_preset: '{name}' already exists");
                return;
            }
            def.presets.push(crate::profiles::PresetDef {
                name: name.clone(),
                hash: capture_hash(&nam_path),
                nam: nam_path,
            });
            RigLibrary::save_profile(&def);
        }
        tracing::info!("preset added: {name}");
        self.spawn_drive_calibration();
        self.publish_state();
    }

    fn add_stack(&self, name: String) {
        {
            let mut def = self.profile_def.lock_ok();
            if def
                .stacks
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(&name))
            {
                return;
            }
            def.stacks.push(crate::profiles::StackDef {
                name: name.clone(),
                patches: Vec::new(),
            });
            RigLibrary::save_profile(&def);
        }
        tracing::info!("stack added: {name}");
        self.publish_state();
    }

    fn add_patch(&self, name: String, stack: String, preset: String) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            if def
                .patches
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                tracing::warn!("add_patch: '{name}' already exists");
                return;
            }
            if !def
                .presets
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&preset))
            {
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
            if let Some(st) = def
                .stacks
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&stack))
            {
                st.patches.push(name.clone());
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        tracing::info!("patch added: {name}");
        self.reload_rebuilt(rebuilt);
    }

    fn import_capture(&self, name: String, nam_path: String, gear: String, group: String) {
        if !std::path::Path::new(&nam_path).exists() {
            tracing::warn!("import_capture: {nam_path} does not exist");
            return;
        }
        // A tone with no title groups under the model's own name, which
        // degrades to one preset per capture — the same shape as before,
        // rather than every ungrouped capture colliding in one preset.
        let group = if group.trim().is_empty() {
            name.clone()
        } else {
            group
        };
        // Hashed once, here: the content hash is what the NAM catalog keys
        // by, so it is how this capture will later find its own creator,
        // licence and cover art.
        let hash = capture_hash(&nam_path);
        if gear.eq_ignore_ascii_case("pedal") {
            self.import_drive_capture(&group, &name, &nam_path, &hash);
        } else {
            // An amp tone joins the pool under the capture's own name;
            // `add_preset` already dedupes and persists.
            self.add_preset(name, nam_path);
        }
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
                    hash: capture_hash(&nam_path),
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
        tracing::info!(
            "reloading rig library from {}",
            crate::library::rig_dir().display()
        );
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
            profile_from_library(&def, &dps)
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
            let Some(p) = def
                .presets
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&old))
            else {
                return;
            };
            p.name = new_name.clone();
            for patch in &mut def.patches {
                if patch.preset.eq_ignore_ascii_case(&old) {
                    patch.preset = new_name.clone();
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        tracing::info!("preset renamed: {old} → {new_name}");
        self.reload_rebuilt(rebuilt);
    }

    fn delete_preset(&self, name: String) {
        {
            let mut def = self.profile_def.lock_ok();
            if def
                .patches
                .iter()
                .any(|p| p.preset.eq_ignore_ascii_case(&name))
            {
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
            let Some(p) = def
                .patches
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&old))
            else {
                return;
            };
            p.name = new_name.clone();
            for st in &mut def.stacks {
                for slot in &mut st.patches {
                    if slot.eq_ignore_ascii_case(&old) {
                        *slot = new_name.clone();
                    }
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
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
            for st in &mut def.stacks {
                st.patches.retain(|p| !p.eq_ignore_ascii_case(&name));
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
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
            let Some(st) = def
                .stacks
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&old))
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
                part_recalls: Vec::new(),
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
            sets.push(SetlistDef {
                name: name.clone(),
                entries: Vec::new(),
            });
            RigLibrary::save_setlists(&sets);
        }
        tracing::info!("setlist added: {name}");
        self.publish_state();
    }

    fn add_setlist_entry(&self, setlist: u32, song: String) {
        {
            let mut sets = self.setlists.lock_ok();
            let Some(set) = sets.get_mut(setlist as usize) else {
                return;
            };
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
            let Some(set) = sets.get_mut(setlist as usize) else {
                return;
            };
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
        let (Some(block_name), Some(patch_name)) = (block, active) else {
            return;
        };
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
                o.block.eq_ignore_ascii_case(&block_name)
                    && o.op == "set_text"
                    && o.param == "ir_path"
            }) {
                Some(o) => o.text = path.clone(),
                None => p.overrides.push(crate::profiles::OverrideDef::set_text(
                    "Time",
                    &block_name,
                    "ir_path",
                    &path,
                )),
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
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
            let Some(p) = def.presets.get_mut(index as usize) else {
                return;
            };
            tracing::info!("preset '{}' → {nam_path}", p.name);
            p.nam = nam_path;
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
        self.spawn_drive_calibration();
    }

    fn set_patch_trim(&self, patch: u32, db: f32) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.patches.get_mut(patch as usize) else {
                return;
            };
            p.trim_db = db.clamp(-24.0, 24.0);
            tracing::info!("patch '{}' trim {:+.1} dB", p.name, p.trim_db);
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
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
        let Some(preset_name) = preset_name else {
            return;
        };
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
            match guard.as_mut() {
                Some(prig) => {
                    prig.activate(idx);
                }
                // Design mode: the browser still changes what is live.
                None => {
                    drop(guard);
                    let name = self
                        .profile_def
                        .lock_ok()
                        .patches
                        .get(idx)
                        .map(|p| p.name.clone());
                    if let Some(name) = name {
                        *self.design_patch.lock_ok() = name;
                    }
                }
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
        tracing::info!(
            "tuner overlay {} (input {})",
            if shown { "shown" } else { "hidden" },
            if shown { "muted" } else { "live" }
        );
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
                        .is_none_or(|p| p.rig().di_capture_state().1 == 0)
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
            tracing::info!(
                "main output: {}",
                if hp.main_mute { "MUTED" } else { "live" }
            );
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
                    .map_or(BOOST_LEVELS[0], |i| {
                        BOOST_LEVELS[(i + 1) % BOOST_LEVELS.len()]
                    });
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
                TunerReading {
                    active: true,
                    freq_hz: freq,
                    note,
                    cents,
                }
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

// ── shared RigCore (mounted instance-scoped as "guitar") ─────────────────────
impl signal_rigs_proto::rig_core::RigCore for GuitarRigBackend {
    fn start(&self) {
        Rig::start(self);
    }
    fn stop(&self) {
        Rig::stop(self);
    }
    fn running(&self) -> bool {
        architect::rig::RigBackend::is_running(self)
    }
    fn presets(&self) -> Vec<signal_rigs_proto::RigPresetInfo> {
        let active = Rig::status(self).active_patch.unwrap_or_default();
        Rig::patches(self)
            .into_iter()
            .map(|p| signal_rigs_proto::RigPresetInfo {
                loaded: p.name == active,
                name: p.name,
            })
            .collect()
    }
    fn load_preset(&self, index: u32) {
        Rig::select_patch(self, index);
    }
    fn midi_ports(&self) -> Vec<String> {
        architect::rig::RigBackend::midi_ports(self)
    }
    fn set_midi_port(&self, _name: String) {
        // The guitar rig is omni (all footswitch ports merged) — no port
        // filter to set.
    }
    fn midi_recent(&self) -> Vec<String> {
        Rig::midi_recent(self)
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
            inputs: GuitarRig::input_devices()
                .into_iter()
                .map(map_device)
                .collect(),
            outputs: GuitarRig::output_devices()
                .into_iter()
                .map(map_device)
                .collect(),
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
    let denom = 2.0f32.mul_add(-y0, ym1) + yp1;
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
    let midi = 12.0f32.mul_add((freq / 440.0).log2(), 69.0);
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
            let w = 0.5f32.mul_add(
                -(2.0 * std::f32::consts::PI * i as f32 / n as f32).cos(),
                0.5,
            );
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
                let (tr, ti) = (
                    re[j].mul_add(wr, -(im[j] * wi)),
                    re[j].mul_add(wi, im[j] * wr),
                );
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
            let mag = re[k].hypot(im[k]) / (n as f32 / 4.0);
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

/// The content hash of a capture on disk — the key the NAM catalog indexes
/// by, and so the handle a preset keeps on its own provenance.
///
/// Empty when the file cannot be read: a preset without a hash simply shows
/// no attribution, which is the right failure. Refusing the import because a
/// picture might be missing would be the wrong one.
fn capture_hash(path: &str) -> String {
    std::fs::read(path).map_or_else(
        |e| {
            tracing::debug!(path, %e, "capture hash: unreadable");
            String::new()
        },
        |bytes| signal_nam::sha256_hex(&bytes),
    )
}

/// Root of the local NAM library — what a cover's recorded path resolves
/// against. The same resolution the engine and the `signal` CLI use, so all
/// three agree on one library.
fn nam_root() -> std::path::PathBuf {
    let config = signal_rig_host::store::signal_config_dir();
    signal_nam::nam_root_from_env(&config.join("nam"))
}

/// The NAM catalog, or `None` when there is not one yet.
///
/// A missing catalog is the ordinary state of a rig that has never imported
/// anything. Presets then show no provenance, which is correct rather than
/// broken — the seeded captures have none to show.
fn nam_catalog() -> Option<signal_nam::NamCatalog> {
    signal_nam::NamCatalog::load(&nam_root().join("catalog.json")).ok()
}

/// Media type from a cover's extension. The download writes that extension
/// from the type it was served, so this reads it straight back.
fn mime_for_path(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::pool_preset_of;
    use crate::profiles::{PatchDef, ProfileDef};

    fn patch(name: &str, preset: &str) -> PatchDef {
        PatchDef {
            name: name.to_string(),
            preset: preset.to_string(),
            trim_db: 0.0,
            boost_db: 0.0,
            overrides: Vec::new(),
        }
    }

    /// The amp a patch is on is the pool preset it names — resolved from the
    /// definition alone, taking the patch by name.
    ///
    /// The signature is the point. An earlier version was a method that found
    /// the active patch by locking the rig, and `resync_blocks` calls it while
    /// already holding that lock: a non-reentrant mutex re-locked on the same
    /// thread, which hung the open sequence at the chain publish and took the
    /// delay tempo, boost recall and drive calibration with it. Keeping the
    /// lookup pure means a caller can only pass in what it already has.
    #[test]
    fn a_patch_names_its_amp() {
        let def = ProfileDef {
            drives: Vec::new(),
            name: "test".to_string(),
            presets: Vec::new(),
            patches: vec![patch("Clean", "Fender Clean"), patch("Lead", "Arena Lead")],
            stacks: Vec::new(),
        };
        assert_eq!(
            pool_preset_of(&def, "Lead").as_deref(),
            Some("Arena Lead"),
            "the patch names the amp"
        );
        // Patch names are matched the way the rest of the profile matches them.
        assert_eq!(
            pool_preset_of(&def, "clean").as_deref(),
            Some("Fender Clean")
        );
        // A patch the profile no longer holds has no amp, rather than the
        // first one — the board would otherwise name a tone that is not loaded.
        assert_eq!(pool_preset_of(&def, "Crunch"), None);
    }
}
