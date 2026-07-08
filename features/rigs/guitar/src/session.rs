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
use architect::{HasDispatcher, Layer, LayerRouter, PubSub, Services, layers};
use signal_guitar_proto::audio::AudioSettings;
use signal_guitar_proto::rig::{Rig, RigEvent, RigStreamSource};
use signal_guitar_proto::{
    AudioDevice, AudioDevices, AudioPrefs, BlockParam, HeadphoneState, LiveBlock, PatchInfo,
    PerfStack, PerformanceModel, PresetInfo, RigStatus, TunerReading,
};
use signal_proto::block::BlockType;
use signal_sampler::{DeviceInfo, GuitarRig, ProfileRig, RigBlock, RigManager};

use crate::profiles::{
    ProfileDef, SetlistDef, SongDef, build_profile, default_setlists, drive_presets, song_library,
    worship_def,
};

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

/// Shared live rig (a [`ProfileRig`] wrapping the [`GuitarRig`]).
type SharedRig = Arc<Mutex<Option<ProfileRig>>>;

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
    section_index: Arc<Mutex<usize>>,
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
}

impl Default for GuitarRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Meter-stream publish interval (~30 Hz).
const METER_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

impl GuitarRigBackend {
    pub fn new() -> Self {
        let backend = Self {
            rig: Arc::new(Mutex::new(None)),
            boost_on: Arc::new(Mutex::new(false)),
            boost_level: Arc::new(Mutex::new(BOOST_LEVELS[0])),
            blocks: Arc::new(Mutex::new(Vec::new())),
            tempo: Arc::new(Mutex::new(None)),
            taps: Arc::new(Mutex::new(TapTracker::default())),
            profile_def: Arc::new(Mutex::new(worship_def())),
            songs_lib: Arc::new(Mutex::new(song_library())),
            setlists: Arc::new(Mutex::new(default_setlists())),
            setlist_index: Arc::new(Mutex::new(0)),
            song_index: Arc::new(Mutex::new(0)),
            section_index: Arc::new(Mutex::new(0)),
            headphone: Arc::new(Mutex::new(HeadphoneState::default())),
            master_trim: Arc::new(Mutex::new(0.0)),
            midi_log: Arc::new(Mutex::new(Vec::new())),
            revision: Arc::new(Mutex::new(0)),
            events: PubSub::sliding(64),
        };
        backend.spawn_meter_pump();
        backend.spawn_drive_calibration();
        backend
    }

    /// Publish `Status` events at meter rate while the rig runs (plus one
    /// final event when it stops, so remotes see `running: false`). A plain
    /// thread, not the audio callback — meters cross from the RT thread via
    /// atomics, and `PubSub::publish` takes a lock so it must never run RT.
    fn spawn_meter_pump(&self) {
        let backend = self.clone();
        std::thread::spawn(move || {
            let midi = midicore::midir::MidiStream::open_all();
            if midi.is_some() {
                tracing::info!("rig core: MIDI capture active");
            }
            let mut was_running = false;
            let mut tick = 0u64;
            loop {
                std::thread::sleep(METER_INTERVAL);
                tick += 1;
                // Update the GR estimate + drain MIDI regardless of publish.
                if let Some(stream) = &midi {
                    let mut log = backend.midi_log.lock().unwrap();
                    for msg in stream.drain() {
                        log.push(format!("{msg:?}"));
                        let len = log.len();
                        if len > 128 {
                            log.drain(0..len - 128);
                        }
                    }
                }
                let status = Rig::status(&backend);
                if status.running || was_running {
                    was_running = status.running;
                    backend.events.publish(RigEvent::Status(status));
                    // Spectrum + comp telemetry at full meter rate (~30 Hz)
                    // — the surface stays alive to the hand.
                    let _ = tick;
                    if let Some(bins) = backend.input_spectrum() {
                        backend.events.publish(RigEvent::Spectrum(bins));
                    }
                    let (wave_in, wave_gr) = signal_fx::comp_meter::wave_snapshot(3);
                    backend.events.publish(RigEvent::CompWave(wave_in, wave_gr));
                }
            }
        });
    }

    /// Log-binned input spectrum (dB, −90..0) from the rig's pre-amp tap.
    fn input_spectrum(&self) -> Option<Vec<f32>> {
        let (samples, rate) = {
            let guard = self.rig.lock().ok()?;
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
            .lock()
            .map(|b| b.iter().any(|b| b.block_type == BlockType::Compressor && !b.bypassed))
            .unwrap_or(false);
        if has_comp { signal_fx::comp_meter::gr_db().max(0.0) } else { 0.0 }
    }

    /// The NAM capture behind a board block, if any: drive slots resolve
    /// through their drive preset; the amp resolves through the active
    /// patch's pool preset.
    fn nam_path_for_block(&self, block_name: &str) -> Option<String> {
        let def = self.profile_def.lock().unwrap();
        if let Some(slot) = def
            .drives
            .iter()
            .find(|d| d.block.eq_ignore_ascii_case(block_name))
        {
            return drive_presets()
                .into_iter()
                .find(|p| p.name.eq_ignore_ascii_case(&slot.preset))
                .and_then(|p| p.options.get(slot.option).map(|o| o.nam.clone()));
        }
        if block_name.eq_ignore_ascii_case("Amp L") || block_name.eq_ignore_ascii_case("Amp R") {
            let active = {
                let guard = self.rig.lock().ok()?;
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
        let sr = {
            let guard = self.rig.lock().ok();
            guard
                .as_ref()
                .and_then(|g| g.as_ref().map(|p| p.sample_rate() as f64))
                .unwrap_or(48_000.0)
        };
        let Some((in_db, out_db)) =
            signal_sampler::nam_calibrate::drive_compensation(std::path::Path::new(&path), sr, drive)
        else {
            tracing::warn!("drive compensation unavailable for {block_name} — leaving trims");
            return;
        };
        if let Ok(guard) = self.rig.lock() {
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
            .lock()
            .unwrap()
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
            let sr = {
                let guard = backend.rig.lock().ok();
                guard
                    .as_ref()
                    .and_then(|g| g.as_ref().map(|p| p.sample_rate() as f64))
                    .unwrap_or(48_000.0)
            };
            let mut paths: Vec<String> = Vec::new();
            for p in drive_presets() {
                for o in &p.options {
                    paths.push(o.nam.clone());
                }
            }
            {
                let def = backend.profile_def.lock().unwrap();
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

    /// Re-apply the main-output trim: patch base + master trim + mute.
    fn apply_main_mute(&self) {
        let mute = self.headphone.lock().unwrap().main_mute;
        let trim = *self.master_trim.lock().unwrap();
        if let Ok(guard) = self.rig.lock() {
            if let Some(prig) = guard.as_ref() {
                let base = prig.active_patch().map(|p| p.output_trim_db).unwrap_or(0.0);
                prig.rig()
                    .set_output_trim_db(base + trim + if mute { -96.0 } else { 0.0 });
            }
        }
    }

    /// Publish the full perf model + chain — call after every mutation.
    fn publish_state(&self) {
        *self.revision.lock().unwrap() += 1;
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
        self.events.publish(RigEvent::Chain(Rig::chain(self)));
    }

    /// The tempo shown/used right now (tapped, or the default).
    fn tempo_bpm(&self) -> f32 {
        self.tempo.lock().unwrap().unwrap_or(DEFAULT_BPM)
    }

    /// Push the tapped tempo onto every delay block in the active chain
    /// (quarter-note delay time in ms). Configured patch times apply until
    /// the first tap; after that, taps own the delay time — including across
    /// patch switches.
    fn apply_tempo_to_delays(&self) {
        let Some(bpm) = *self.tempo.lock().unwrap() else {
            return;
        };
        let quarter_ms = 60_000.0 / bpm;
        let delay_ids: Vec<String> = self
            .blocks
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.block_type == BlockType::Delay)
            .map(|b| b.id.clone())
            .collect();
        if delay_ids.is_empty() {
            return;
        }
        if let Ok(guard) = self.rig.lock() {
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
        if *self.boost_on.lock().unwrap() {
            *self.boost_level.lock().unwrap()
        } else {
            0.0
        }
    }

    /// Push the boost pedal level onto the active chain's "Boost" gain block
    /// (a `Volume` block). Re-applied after patch switches (activation
    /// reinstalls the configured 0 dB).
    fn apply_boost_to_block(&self) {
        let db = self.current_boost_db();
        let boost_ids: Vec<String> = self
            .blocks
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.block_type == BlockType::Volume && b.name.eq_ignore_ascii_case("Boost"))
            .map(|b| b.id.clone())
            .collect();
        if boost_ids.is_empty() {
            return;
        }
        if let Ok(guard) = self.rig.lock() {
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
        if let Ok(mut guard) = self.rig.lock() {
            if let Some(prig) = guard.as_mut() {
                prig.activate_stack(index);
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    /// The active setlist's entries, resolved against the song library:
    /// `(name, key, bpm, stack, sections)` per slot — per-set overrides win
    /// over the song's defaults.
    fn resolved_setlist(&self) -> Vec<(String, String, u32, usize, Vec<String>)> {
        let lib = self.songs_lib.lock().unwrap();
        let setlists = self.setlists.lock().unwrap();
        let idx = *self.setlist_index.lock().unwrap();
        let Some(set) = setlists.get(idx) else {
            return Vec::new();
        };
        set.entries
            .iter()
            .map(|e| {
                let song = lib.iter().find(|s| s.name.eq_ignore_ascii_case(&e.song));
                let key = e
                    .key
                    .clone()
                    .or_else(|| song.map(|s| s.key.clone()))
                    .unwrap_or_default();
                let bpm = e.bpm.or_else(|| song.map(|s| s.bpm)).unwrap_or(120);
                let stack = song.map(|s| s.stack).unwrap_or(0);
                let sections = song.map(|s| s.sections.clone()).unwrap_or_default();
                (e.song.clone(), key, bpm, stack, sections)
            })
            .collect()
    }

    /// Recall setlist entry `idx`: activate its stack, set the song's tempo
    /// (which re-times the delays), and rewind to the first section.
    fn recall_song(&self, idx: usize) {
        let entry = self.resolved_setlist().get(idx).cloned();
        if let Some((name, key, bpm, stack, _)) = entry {
            *self.section_index.lock().unwrap() = 0;
            *self.tempo.lock().unwrap() = Some(bpm as f32);
            tracing::info!("setlist → {name} ({key} · {bpm} BPM, stack {stack})");
            self.apply_tempo_to_delays();
            // Recall activates the song's stack only when it isn't already
            // the active one — a press on the active stack would *rotate*
            // it (FM9 semantics), silently changing the patch.
            let already_active = self
                .rig
                .lock()
                .ok()
                .and_then(|g| {
                    g.as_ref().map(|prig| {
                        prig.stacks().get(stack).is_some_and(|st| {
                            st.patches.iter().any(|p| {
                                prig.active_patch().is_some_and(|a| a.name.eq_ignore_ascii_case(p))
                            })
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

    /// Build the vox router serving this session's whole service surface.
    /// Mount it on any transport — `architect::LocalServer` in-process,
    /// `architect::axum_ws` for remote GUIs.
    pub fn router(&self) -> LayerRouter {
        self.clone().into_router()
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
        let had_previous = if let Ok(mut g) = self.rig.lock() {
            g.take().is_some()
        } else {
            false
        };
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
                let profile = build_profile(&self.profile_def.lock().unwrap());
                match prig.load_profile(profile, None) {
                    Ok(()) => tracing::info!("profile loaded ({} patches)", prig.patches().len()),
                    Err(e) => tracing::error!("profile load failed: {e}"),
                }
                if let Ok(mut slot) = self.rig.lock() {
                    *slot = Some(prig);
                }
            }
            Err(e) => tracing::error!("rig open failed: {e:#}"),
        }
        // Mirror the (now active) patch's FX chain + apply bypass defaults,
        // and re-push any tapped tempo onto the fresh delays.
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    /// Rebuild the mirror of the active patch's FX chain and (re-)apply each
    /// block's initial bypass to the engine (activation re-enables all slots,
    /// so the off-by-default blocks must be re-bypassed here).
    fn resync_blocks(&self) {
        let mut out = Vec::new();
        if let Ok(guard) = self.rig.lock() {
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
                            let def = self.profile_def.lock().unwrap();
                            def.drives
                                .iter()
                                .find(|d| d.block.eq_ignore_ascii_case(&name))
                                .and_then(|d| {
                                    drive_presets()
                                        .into_iter()
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
        *self.blocks.lock().unwrap() = out;
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
            ("threshold", -60.0, 0.0, -18.0),
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
        sections: Vec::new(),
        section_index: 0,
        headphone: HeadphoneState::default(),
        master_trim_db: 0.0,
        revision: 0,
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

impl Rig for GuitarRigBackend {
    fn start(&self) {
        let backend = self.clone();
        std::thread::spawn(move || backend.open_blocking());
    }

    fn stop(&self) {
        *self.rig.lock().unwrap() = None;
        tracing::info!("rig stopped");
        self.publish_state();
    }

    fn status(&self) -> RigStatus {
        let guard = match self.rig.lock() {
            Ok(g) => g,
            Err(_) => return RigStatus::default(),
        };
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
            let def = self.profile_def.lock().unwrap();
            self.rig
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|prig| build_perf_model(prig, &def)))
                .unwrap_or_default()
        };
        m.boost_db = self.current_boost_db();
        m.tempo_bpm = self.tempo_bpm().round() as u32;
        {
            let resolved = self.resolved_setlist();
            let song_idx = *self.song_index.lock().unwrap();
            m.songs = resolved
                .iter()
                .map(|(name, key, bpm, _, _)| signal_guitar_proto::SongSlot {
                    name: name.clone(),
                    key: key.clone(),
                    bpm: *bpm,
                })
                .collect();
            m.song_index = song_idx as u32;
            m.sections = resolved
                .get(song_idx)
                .map(|(_, _, _, _, sections)| sections.clone())
                .unwrap_or_default();
            m.setlists = self.setlists.lock().unwrap().iter().map(|s| s.name.clone()).collect();
            m.setlist_index = *self.setlist_index.lock().unwrap() as u32;
        }
        m.section_index = *self.section_index.lock().unwrap() as u32;
        m.headphone = self.headphone.lock().unwrap().clone();
        m.master_trim_db = *self.master_trim.lock().unwrap();
        m.revision = *self.revision.lock().unwrap();
        m
    }

    fn chain(&self) -> Vec<LiveBlock> {
        self.blocks.lock().map(|b| b.clone()).unwrap_or_default()
    }

    fn press_stack(&self, index: u32) {
        self.activate_stack_and_sync(index as usize);
    }

    fn next_song(&self) {
        let last = self.resolved_setlist().len().saturating_sub(1);
        let idx = {
            let mut i = self.song_index.lock().unwrap();
            *i = (*i + 1).min(last);
            *i
        };
        self.recall_song(idx);
    }

    fn prev_song(&self) {
        let idx = {
            let mut i = self.song_index.lock().unwrap();
            *i = i.saturating_sub(1);
            *i
        };
        self.recall_song(idx);
    }

    fn select_song(&self, index: u32) {
        let last = self.resolved_setlist().len().saturating_sub(1);
        let idx = (index as usize).min(last);
        *self.song_index.lock().unwrap() = idx;
        self.recall_song(idx);
    }

    fn select_section(&self, index: u32) {
        let (song_idx, last) = {
            let i = *self.song_index.lock().unwrap();
            let last = self
                .resolved_setlist()
                .get(i)
                .map(|(_, _, _, _, sections)| sections.len().saturating_sub(1))
                .unwrap_or(0);
            (i, last)
        };
        let idx = (index as usize).min(last);
        *self.section_index.lock().unwrap() = idx;
        tracing::info!("section → {idx} (song {song_idx})");
        // Sections don't drive audio yet (scenes later) — publish so every
        // remote follows the section highlight.
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
    }

    fn select_setlist(&self, index: u32) {
        {
            let mut idx = self.setlist_index.lock().unwrap();
            let last = self.setlists.lock().unwrap().len().saturating_sub(1);
            *idx = (index as usize).min(last);
        }
        *self.song_index.lock().unwrap() = 0;
        tracing::info!(
            "setlist switched → {}",
            self.setlists.lock().unwrap()[*self.setlist_index.lock().unwrap()].name
        );
        self.recall_song(0);
        self.publish_state();
    }

    fn move_song(&self, from: u32, to: u32) {
        {
            let mut setlists = self.setlists.lock().unwrap();
            let active = *self.setlist_index.lock().unwrap();
            let Some(set) = setlists.get_mut(active) else { return };
            let entries = &mut set.entries;
            let (from, to) = (from as usize, to as usize);
            if from >= entries.len() || to >= entries.len() {
                return;
            }
            let song = entries.remove(from);
            entries.insert(to, song);
            // Keep the current-song pointer on the same song.
            let mut cur = self.song_index.lock().unwrap();
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
        let guard = match self.rig.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
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
                    let def = self.profile_def.lock().unwrap();
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
        if let Ok(mut guard) = self.rig.lock() {
            if let Some(prig) = guard.as_mut() {
                // If the patch lives in a stack, jump that stack's rotation
                // to it — the footswitch grid stays consistent with what's
                // audible (one stack always lit). Loose patches activate
                // directly.
                let name = prig.patches().get(index as usize).map(|p| p.name.clone());
                let in_stack = name.as_ref().and_then(|n| {
                    prig.stacks().iter().enumerate().find_map(|(si, st)| {
                        st.patches
                            .iter()
                            .position(|p| p.eq_ignore_ascii_case(n))
                            .map(|pos| (si, pos))
                    })
                });
                match in_stack {
                    Some((stack, pos)) => {
                        prig.activate_stack_at(stack, pos);
                    }
                    None => {
                        prig.activate(index as usize);
                    }
                }
            }
        }
        self.resync_blocks();
        self.apply_tempo_to_delays();
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    fn presets(&self) -> Vec<PresetInfo> {
        let def = self.profile_def.lock().unwrap();
        let active_patch = self
            .rig
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|p| p.active_patch().map(|p| p.name.clone())));
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
            let mut def = self.profile_def.lock().unwrap();
            let Some(preset_name) = def.presets.get(preset as usize).map(|p| p.name.clone())
            else {
                return;
            };
            let Some(p) = def.patches.get_mut(patch as usize) else {
                return;
            };
            tracing::info!("patch '{}' → preset '{preset_name}'", p.name);
            p.preset = preset_name;
            build_profile(&def)
        };
        // …then rebuild the live chains. A full reload (brief gap) — this is
        // an edit-time operation, and it keeps every patch preinstalled for
        // gapless footswitching afterward.
        let active = {
            let guard = self.rig.lock().unwrap();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        if let Ok(mut guard) = self.rig.lock() {
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
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone());
        let Some(block_name) = block_name else { return };
        let rebuilt = {
            let mut def = self.profile_def.lock().unwrap();
            let Some(slot) = def
                .drives
                .iter_mut()
                .find(|d| d.block.eq_ignore_ascii_case(&block_name))
            else {
                return;
            };
            let n_options = drive_presets()
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&slot.preset))
                .map(|p| p.options.len())
                .unwrap_or(0);
            if n_options == 0 {
                return;
            }
            slot.option = (option as usize).min(n_options - 1);
            tracing::info!("{} → {} option {}", slot.block, slot.preset, slot.option);
            build_profile(&def)
        };
        let active = {
            let guard = self.rig.lock().unwrap();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        };
        if let Ok(mut guard) = self.rig.lock() {
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
        self.apply_boost_to_block();
        self.apply_all_drives();
        self.publish_state();
    }

    fn capture_di_reference(&self, seconds: u32) {
        let secs = seconds.clamp(5, 60) as usize;
        let (sr, armed) = {
            let guard = self.rig.lock().unwrap();
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
                    let guard = backend.rig.lock().unwrap();
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
                let guard = backend.rig.lock().unwrap();
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
            let mut hp = self.headphone.lock().unwrap();
            hp.volume = volume.clamp(0.0, 1.0);
            hp.self_mix = self_mix.clamp(0.0, 1.0);
        }
        self.publish_state();
    }

    fn toggle_main_mute(&self) {
        {
            let mut hp = self.headphone.lock().unwrap();
            hp.main_mute = !hp.main_mute;
            tracing::info!("main output: {}", if hp.main_mute { "MUTED" } else { "live" });
        }
        self.apply_main_mute();
        self.publish_state();
    }

    fn set_master_trim(&self, db: f32) {
        *self.master_trim.lock().unwrap() = db.clamp(-24.0, 12.0);
        self.apply_main_mute();
        self.publish_state();
    }

    fn midi_recent(&self) -> Vec<String> {
        self.midi_log.lock().unwrap().clone()
    }

    fn toggle_fx(&self) {
        if let Ok(mut guard) = self.rig.lock() {
            if let Some(prig) = guard.as_mut() {
                let on = prig.toggle_fx_bypass();
                tracing::info!("FX bypass: {}", if on { "ON" } else { "off" });
            }
        }
        self.publish_state();
    }

    fn toggle_boost(&self) {
        let on = {
            let mut b = self.boost_on.lock().unwrap();
            *b = !*b;
            *b
        };
        self.apply_boost_to_block();
        tracing::info!(
            "boost pedal: {}",
            if on {
                format!("{:+.0} dB", *self.boost_level.lock().unwrap())
            } else {
                "off".to_string()
            }
        );
        self.publish_state();
    }

    fn cycle_boost(&self) {
        {
            let mut on = self.boost_on.lock().unwrap();
            let mut level = self.boost_level.lock().unwrap();
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
        let (samples, rate) = match self.rig.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(prig) => (prig.input_samples(), prig.sample_rate() as f32),
                None => return TunerReading::default(),
            },
            Err(_) => return TunerReading::default(),
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
        let now = std::time::Instant::now();
        let new_tempo = {
            let mut taps = self.taps.lock().unwrap();
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
            *self.tempo.lock().unwrap() = Some(bpm);
            self.apply_tempo_to_delays();
            self.events.publish(RigEvent::Perf(Rig::perf(self)));
        }
    }

    fn set_block_bypass(&self, id: String, bypassed: bool) {
        let current = self
            .blocks
            .lock()
            .unwrap()
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
            let mut blocks = self.blocks.lock().unwrap();
            blocks.iter_mut().find(|b| b.id == id).map(|b| {
                b.bypassed = !b.bypassed;
                (b.bypassed, b.block_type)
            })
        };
        if let Some((byp, bt)) = new_bypass {
            if let Ok(mut guard) = self.rig.lock() {
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
        }
        self.publish_state();
    }

    fn set_block_param(&self, id: String, param: String, value: f32) {
        // Constant-loudness drive: on NAM board blocks the drive knob is
        // realised as a compensated input/output trim pair.
        if param == "drive" {
            let block = self
                .blocks
                .lock()
                .unwrap()
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
        if let Ok(mut blocks) = self.blocks.lock() {
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
        if let Ok(guard) = self.rig.lock() {
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
    for lag in min_lag..max_lag {
        let v = acf(lag);
        norms[lag] = v;
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
    for b in 0..bins {
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
        out[b] = if peak > 0.0 {
            (20.0 * peak.log10()).clamp(-90.0, 0.0)
        } else {
            -90.0
        };
    }
    out
}
