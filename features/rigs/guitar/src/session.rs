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
    AudioDevice, AudioDevices, AudioPrefs, LiveBlock, PerfStack, PerformanceModel, RigStatus,
};
use signal_proto::block::BlockType;
use signal_sampler::{DeviceInfo, GuitarRig, ProfileRig, RigBlock, RigManager};

use crate::profiles::worship_profile;

/// Rig whose audio prefs the settings service reads/writes (persisted to
/// `<config>/signal/rigs/guitar-rig.styx` by `RigManager`).
const AUDIO_RIG_NAME: &str = "Guitar Rig";

/// Extra output gain (dB) applied when the volume boost is engaged.
const BOOST_DB: f32 = 6.0;

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
    /// Volume-boost engaged (adds output trim on top of the active patch).
    boost: Arc<Mutex<bool>>,
    /// The active patch's live FX chain, mirrored for clients. Rebuilt whenever
    /// the active patch changes (the rig has no per-block bypass/param getters).
    blocks: Arc<Mutex<Vec<LiveBlock>>>,
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
            boost: Arc::new(Mutex::new(false)),
            blocks: Arc::new(Mutex::new(Vec::new())),
            events: PubSub::sliding(64),
        };
        backend.spawn_meter_pump();
        backend
    }

    /// Publish `Status` events at meter rate while the rig runs (plus one
    /// final event when it stops, so remotes see `running: false`). A plain
    /// thread, not the audio callback — meters cross from the RT thread via
    /// atomics, and `PubSub::publish` takes a lock so it must never run RT.
    fn spawn_meter_pump(&self) {
        let backend = self.clone();
        std::thread::spawn(move || {
            let mut was_running = false;
            loop {
                std::thread::sleep(METER_INTERVAL);
                let status = Rig::status(&backend);
                if status.running || was_running {
                    was_running = status.running;
                    backend.events.publish(RigEvent::Status(status));
                }
            }
        });
    }

    /// Publish the full perf model + chain — call after every mutation.
    fn publish_state(&self) {
        self.events.publish(RigEvent::Perf(Rig::perf(self)));
        self.events.publish(RigEvent::Chain(Rig::chain(self)));
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
                match prig.load_profile(worship_profile(), None) {
                    Ok(()) => tracing::info!("worship profile loaded ({} patches)", prig.patches().len()),
                    Err(e) => tracing::error!("profile load failed: {e}"),
                }
                if let Ok(mut slot) = self.rig.lock() {
                    *slot = Some(prig);
                }
            }
            Err(e) => tracing::error!("rig open failed: {e:#}"),
        }
        // Mirror the (now active) patch's FX chain + apply bypass defaults.
        self.resync_blocks();
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
                        out.push(LiveBlock {
                            id: id.clone(),
                            block_type: block.block_type,
                            name,
                            bypassed: block.bypassed,
                            param_name,
                            param_value,
                            param_min,
                            param_max,
                        });
                    }
                }
            }
        }
        *self.blocks.lock().unwrap() = out;
    }
}

/// The primary dialable param for a block type: `(name, min, max, default)`.
fn primary_param(bt: BlockType) -> Option<(&'static str, f32, f32, f32)> {
    match bt {
        BlockType::Reverb | BlockType::Delay => Some(("mix", 0.0, 0.10, 0.08)),
        BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => Some(("mix", 0.0, 1.0, 0.4)),
        BlockType::Trem => Some(("depth", 0.0, 1.0, 0.5)),
        _ => None,
    }
}

/// Re-apply the output trim = active patch base trim (+ boost if engaged). Call
/// after any patch activation, since `activate` resets the trim per patch.
fn apply_boost(prig: &ProfileRig, boost: bool) {
    let base = prig.active_patch().map(|p| p.output_trim_db).unwrap_or(0.0);
    prig.rig()
        .set_output_trim_db(base + if boost { BOOST_DB } else { 0.0 });
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

/// Snapshot the performance model (folders + live cursor/active state).
fn build_perf_model(prig: &ProfileRig) -> PerformanceModel {
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
            PerfStack {
                name: st.name.clone(),
                current_patch: patch_display(&st.name, &cur),
                position: pos as u32,
                patch_count: st.patches.len() as u32,
                available,
                is_active: active_stack == Some(si),
            }
        })
        .collect();
    PerformanceModel {
        profile_name: prig.profile_name().unwrap_or_default().to_string(),
        stacks,
        fx_bypass: prig.fx_bypass(),
        boost: false, // overwritten by the service (boost lives outside prig)
        tempo_bpm: 120,
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
        match guard.as_ref() {
            Some(prig) => RigStatus {
                running: true,
                input_peak: prig.rig().input_peak(),
                output_peak: prig.rig().output_peak(),
                active_patch: prig.active_patch().map(|p| p.name.clone()),
            },
            None => RigStatus::default(),
        }
    }

    fn perf(&self) -> PerformanceModel {
        let mut m = self
            .rig
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(build_perf_model))
            .unwrap_or_default();
        m.boost = *self.boost.lock().unwrap();
        m
    }

    fn chain(&self) -> Vec<LiveBlock> {
        self.blocks.lock().map(|b| b.clone()).unwrap_or_default()
    }

    fn press_stack(&self, index: u32) {
        let boost = *self.boost.lock().unwrap();
        if let Ok(mut guard) = self.rig.lock() {
            if let Some(prig) = guard.as_mut() {
                prig.activate_stack(index as usize);
                apply_boost(prig, boost);
            }
        }
        // Rebuild the block mirror + re-apply bypass defaults (drop the rig
        // lock first — resync_blocks re-locks it).
        self.resync_blocks();
        self.publish_state();
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
        let boost = {
            let mut b = self.boost.lock().unwrap();
            *b = !*b;
            *b
        };
        if let Ok(guard) = self.rig.lock() {
            if let Some(prig) = guard.as_ref() {
                apply_boost(prig, boost);
            }
        }
        tracing::info!("volume boost: {}", if boost { "ON" } else { "off" });
        self.publish_state();
    }

    fn tap_tempo(&self) {
        tracing::info!("tap tempo (not yet wired to a delay block)");
    }

    fn toggle_block_bypass(&self, id: String) {
        let new_bypass = {
            let mut blocks = self.blocks.lock().unwrap();
            blocks.iter_mut().find(|b| b.id == id).map(|b| {
                b.bypassed = !b.bypassed;
                b.bypassed
            })
        };
        if let Some(byp) = new_bypass {
            if let Ok(guard) = self.rig.lock() {
                if let Some(prig) = guard.as_ref() {
                    prig.rig().set_block_slot_bypass(&id, byp);
                }
            }
        }
        self.publish_state();
    }

    fn set_block_param(&self, id: String, param: String, value: f32) {
        if let Ok(mut blocks) = self.blocks.lock() {
            if let Some(b) = blocks.iter_mut().find(|b| b.id == id) {
                b.param_value = value;
            }
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
