//! Headless bass-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`ProfileRig`] over the shared duplex DI engine
//! ([`GuitarRig`] — `r[signal.soundsource.audio]`): bass in → NAM amp / IR
//! chain → out. The preset library ("Bass", "Synth Bass", …) is loaded from
//! styx ([`crate::library`]) and **pre-installed as one profile** so preset
//! switches are gapless. Implements the [`signal_bass_proto::bass::BassRig`]
//! service + its `#[subscribe]` stream; mount `router()`
//! (`architect::rig::RigBackend`) on a vox transport.
//!
//! Born on the shared primitives — zero hand-rolled plumbing:
//! - `architect::rig::RigBackend` supplies the meter pump (interval,
//!   once-guard, running-edge detection, hot-plug rescan, per-tick panic
//!   containment).
//! - `midicore::attach` supplies the MIDI lifecycle (drop-before-open
//!   reattach + the monitor-tap sink). Hardware switches presets via
//!   program change / footswitch CCs (`midi.styx`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{layers, HasDispatcher, Layer, PubSub, Services};
use midicore::{MidiEvent, MidiMonitor};
use signal_bass_proto::bass::{BassEvent, BassRig as BassRigSvc, BassRigStreamSource};
use signal_bass_proto::{BassBlock, BassPreset, BassStatus, PresetKind};
use signal_proto::block::BlockType;
use signal_sampler::rig::RigBlock;
use signal_sampler::{GuitarRig, ProfileRig, RigManager, RigPatch, RigProfile};

use crate::library::{BassLastState, BassLibrary, BassMidiMapDef, BassPresetDef};

/// Rig whose audio prefs are read/written (persisted to
/// `<config>/signal/rigs/bass-rig.styx` by `RigManager`).
const AUDIO_RIG_NAME: &str = "Bass Rig";

use signal_rig_host::lock::LockExt;

/// Meter-pump loop state, kept outside the tick so a caught panic in one
/// iteration doesn't reset CC edge detection.
#[derive(Default)]
pub struct BassTick {
    tick: u64,
    /// Last seen value per control CC (edge detection — momentary switches
    /// repeat while held).
    prev_down: bool,
    next_down: bool,
}

struct Inner {
    /// The live rig (`ProfileRig` over the duplex `GuitarRig`). `Send` but
    /// not `Sync` (pipewire owns a `*mut pw_thread_loop`) — serialize
    /// through the mutex.
    rig: Mutex<Option<ProfileRig>>,
    /// The editable preset pool (the styx library).
    presets: Mutex<Vec<BassPresetDef>>,
    /// Per-preset "chain installed and ready" flag (index-aligned).
    available: Mutex<Vec<bool>>,
    /// Mirror of the active preset's chain, for clients.
    blocks: Mutex<Vec<BassBlock>>,
    /// MIDI switching map (`midi.styx`).
    midi_map: Mutex<BassMidiMapDef>,
    /// Everything the hardware sent, for the UI monitor.
    monitor: MidiMonitor,
    /// The open MIDI input (reattached by `midicore::attach`).
    midi_handle: Mutex<Option<midicore::midir::MidiInput>>,
    /// Control events queued by the MIDI sink, drained by the pump tick
    /// (service calls must not run on the midir callback thread).
    control_q: Mutex<Vec<MidiEvent>>,
    /// Master output trim (dB) — applied on top of the preset's trim.
    master_trim: Mutex<f32>,
    /// `start` re-entrancy guard: one open at a time.
    opening: AtomicBool,
    /// Debug-formatted audio prefs the rig was opened with — a repeat
    /// `start` with unchanged prefs is a no-op instead of an audio gap.
    open_prefs: Mutex<Option<String>>,
    /// Deferred last-state save (`last-state.styx`) — pump flushes.
    state_dirty: AtomicBool,
    /// The `#[subscribe]` fan-out hub.
    events: PubSub<BassEvent>,
    /// Once-start guard for the shared meter pump (`architect::rig`).
    pump_started: AtomicBool,
}

/// The bass-rig backend handle. Cheap to clone (all state shared).
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct BassRigBackend {
    inner: Arc<Inner>,
}

impl Default for BassRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl BassRigBackend {
    /// Build the backend and load the styx library. Does not open audio.
    pub fn new() -> Self {
        let lib = BassLibrary::load_or_bootstrap();
        let n = lib.presets.len();
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                presets: Mutex::new(lib.presets),
                available: Mutex::new(vec![false; n]),
                blocks: Mutex::new(Vec::new()),
                midi_map: Mutex::new(lib.midi_map),
                monitor: MidiMonitor::new(),
                midi_handle: Mutex::new(None),
                control_q: Mutex::new(Vec::new()),
                master_trim: Mutex::new(0.0),
                opening: AtomicBool::new(false),
                open_prefs: Mutex::new(None),
                state_dirty: AtomicBool::new(false),
                events: architect::rig::events_hub(),
                pump_started: AtomicBool::new(false),
            }),
        };
        backend.spawn_meter_pump("bass-meter-pump");
        backend.reattach_midi();
        backend
    }

    // ── MIDI (midicore::attach — the one lifecycle) ─────────────────────

    /// (Re-)attach the hardware MIDI input per the stored port filter:
    /// drop-before-open via [`midicore::attach::reattach`], monitor tap +
    /// control-queue forward via [`midicore::attach::tap_sink`]. Preset
    /// switching works even while audio is stopped (selection is state).
    fn reattach_midi(&self) {
        let port = {
            let map = self.inner.midi_map.lock_ok();
            if map.port.is_empty() {
                None
            } else {
                Some(map.port.clone())
            }
        };
        let inner = self.inner.clone();
        midicore::attach::reattach(
            "bass rig",
            port.as_deref(),
            || {
                *self.inner.midi_handle.lock_ok() = None;
            },
            |sel| {
                let sink = midicore::attach::tap_sink(self.inner.monitor.clone(), move |ev| {
                    if matches!(
                        ev,
                        MidiEvent::ProgramChange { .. } | MidiEvent::ControlChange { .. }
                    ) {
                        inner.control_q.lock_ok().push(ev);
                    }
                });
                midicore::midir::MidiInput::open(sel, sink).map(Some)
            },
            |h| {
                *self.inner.midi_handle.lock_ok() = Some(h);
            },
        );
    }

    /// One pump iteration ([`RigBackend::on_tick`]): drain queued control
    /// events (program change / footswitch CCs), keep the MIDI input alive,
    /// flush debounced saves.
    fn pump_tick(&self, tick: &mut BassTick) {
        tick.tick += 1;
        // Late/replugged MIDI while stopped: the scaffold's hot-plug hook
        // only fires while running, so re-try a missing handle here.
        if tick.tick.is_multiple_of(60)
            && self.inner.midi_handle.lock_ok().is_none()
            && !midicore::midir::input_ports().is_empty()
        {
            self.reattach_midi();
        }
        // Flush the pending last-state save about once a second.
        if tick.tick.is_multiple_of(30) && self.inner.state_dirty.swap(false, Ordering::Relaxed) {
            BassLibrary::save_last_state(&self.snapshot_last_state());
        }
        let queued: Vec<MidiEvent> = std::mem::take(&mut *self.inner.control_q.lock_ok());
        for ev in queued {
            let map = self.inner.midi_map.lock_ok().clone();
            match ev {
                MidiEvent::ProgramChange { program, .. } if map.program_change => {
                    tracing::info!("bass rig: program change → preset {}", u8::from(program));
                    BassRigSvc::select_preset(self, u8::from(program) as u32);
                }
                MidiEvent::ControlChange {
                    controller, value, ..
                } => {
                    let (cc, down) = (u8::from(controller) as u32, u8::from(value) > 0);
                    if cc == map.prev_cc {
                        if down && !tick.prev_down {
                            BassRigSvc::prev_preset(self);
                        }
                        tick.prev_down = down;
                    } else if cc == map.next_cc {
                        if down && !tick.next_down {
                            BassRigSvc::next_preset(self);
                        }
                        tick.next_down = down;
                    }
                }
                _ => {}
            }
        }
    }

    // ── profile building ────────────────────────────────────────────────

    /// Mark the last-active state (preset/trim) for the pump's debounced
    /// flush to `last-state.styx`.
    fn mark_state_dirty(&self) {
        self.inner.state_dirty.store(true, Ordering::Relaxed);
    }

    fn snapshot_last_state(&self) -> BassLastState {
        BassLastState {
            active_preset: self.active_preset_name().unwrap_or_default(),
            master_trim_db: *self.inner.master_trim.lock_ok(),
        }
    }

    fn active_preset_name(&self) -> Option<String> {
        self.inner
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
    }

    /// Open (or re-open) the live rig and pre-install every available
    /// preset's chain (gapless switching). Blocking (device open + NAM
    /// loads) — the `start` service method spawns it. Drops any existing
    /// rig FIRST so the audio device is released before re-acquiring.
    pub fn open_blocking(&self) {
        tracing::info!("bass rig open: begin");
        let had_previous = self.inner.rig.lock_ok().take().is_some();
        if had_previous {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        let mut mgr = RigManager::load(AUDIO_RIG_NAME);
        // Monitor + play through a single duplex interface by default.
        if mgr.audio.output_device.is_empty() && !mgr.audio.input_device.is_empty() {
            mgr.audio.output_device = mgr.audio.input_device.clone();
            let _ = mgr.save();
        }

        match GuitarRig::open(&mgr.audio) {
            Ok(g) => {
                tracing::info!(
                    "bass rig live: in {} ch{} → out {}",
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
                // One loudness authority: preset trims from the library.
                prig.set_level_match(false);
                let (profile, available) = {
                    let defs = self.inner.presets.lock_ok();
                    build_profile(&defs)
                };
                match prig.load_profile(profile, None) {
                    Ok(()) => {
                        tracing::info!(
                            "bass profile loaded ({} presets installed)",
                            prig.patches().len()
                        )
                    }
                    Err(e) => tracing::error!("bass profile load failed: {e}"),
                }
                *self.inner.available.lock_ok() = available;
                *self.inner.rig.lock_ok() = Some(prig);
                *self.inner.open_prefs.lock_ok() = Some(format!("{:?}", mgr.audio));
            }
            Err(e) => {
                *self.inner.open_prefs.lock_ok() = None;
                tracing::error!("bass rig open failed: {e:#}");
            }
        }
        // Land back on the last tone (crash-restart recovery), else the
        // first available preset.
        self.restore_last_state();
        self.resync_blocks();
        self.apply_master_trim();
        self.publish_all();
    }

    /// Restore the position saved by the pump. Names are re-validated
    /// against the (possibly hand-edited) library — never panics.
    fn restore_last_state(&self) {
        let st = BassLibrary::load_last_state();
        if let Some(st) = &st {
            *self.inner.master_trim.lock_ok() = st.master_trim_db.clamp(-60.0, 12.0);
        }
        let wanted = st.map(|s| s.active_preset).filter(|s| !s.is_empty());
        let mut guard = self.inner.rig.lock_ok();
        let Some(prig) = guard.as_mut() else { return };
        let restored = wanted
            .as_deref()
            .map(|name| prig.activate_named(name))
            .unwrap_or(false);
        if !restored {
            if let Some(w) = &wanted {
                tracing::warn!("bass last-state: preset '{w}' no longer loadable");
            }
            // First available preset (profile order = library order).
            prig.activate(0);
        }
    }

    /// Re-apply the main-output trim: active preset base + master trim.
    fn apply_master_trim(&self) {
        let trim = *self.inner.master_trim.lock_ok();
        let guard = self.inner.rig.lock_ok();
        if let Some(prig) = guard.as_ref() {
            let base = prig.active_patch().map(|p| p.output_trim_db).unwrap_or(0.0);
            prig.rig().set_output_trim_db(base + trim);
        }
    }

    /// Rebuild the mirror of the active preset's chain and re-apply each
    /// block's initial bypass (activation re-enables all slots).
    fn resync_blocks(&self) {
        let mut out = Vec::new();
        {
            let guard = self.inner.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                if let Some(patch) = prig.active_patch() {
                    let ids = prig.active_block_ids();
                    let reals: Vec<&RigBlock> =
                        patch.chain.iter().filter(|b| b.has_backend()).collect();
                    for (block, id) in reals.iter().zip(ids.iter()) {
                        if block.bypassed {
                            prig.set_block_bypass(id, true);
                        }
                        let name = if block.name.trim().is_empty() {
                            format!("{:?}", block.block_type)
                        } else {
                            block.name.clone()
                        };
                        out.push(BassBlock {
                            id: id.clone(),
                            block_type: block.block_type,
                            name,
                            bypassed: block.bypassed,
                        });
                    }
                }
            }
        }
        *self.inner.blocks.lock_ok() = out;
    }

    /// Activate a preset by library index and resync everything activation
    /// resets. Shared by the service, program change, and footswitch steps.
    fn activate_index(&self, index: usize) {
        let (name, ok) = {
            let defs = self.inner.presets.lock_ok();
            let avail = self.inner.available.lock_ok();
            match defs.get(index) {
                Some(d) => (d.name.clone(), avail.get(index).copied().unwrap_or(false)),
                None => return,
            }
        };
        if !ok {
            tracing::info!(
                "bass rig: preset '{name}' not available (missing assets / sampled stub)"
            );
            return;
        }
        {
            let mut guard = self.inner.rig.lock_ok();
            match guard.as_mut() {
                Some(prig) => {
                    if !prig.activate_named(&name) {
                        tracing::warn!("bass rig: preset '{name}' not installed");
                        return;
                    }
                }
                None => {
                    tracing::info!("bass rig: not running — start it to switch presets");
                    return;
                }
            }
        }
        tracing::info!("bass rig: preset → {name}");
        self.resync_blocks();
        self.apply_master_trim();
        self.mark_state_dirty();
        self.publish_all();
    }

    /// Step the active preset by `delta` through the available presets.
    fn step_preset(&self, delta: i32) {
        let active = self.active_preset_name();
        let order: Vec<usize> = {
            let avail = self.inner.available.lock_ok();
            (0..avail.len()).filter(|i| avail[*i]).collect()
        };
        if order.is_empty() {
            return;
        }
        let cur = {
            let defs = self.inner.presets.lock_ok();
            active
                .and_then(|name| {
                    order.iter().position(|i| {
                        defs.get(*i)
                            .is_some_and(|d| d.name.eq_ignore_ascii_case(&name))
                    })
                })
                .unwrap_or(0) as i32
        };
        let next = (cur + delta).rem_euclid(order.len() as i32) as usize;
        self.activate_index(order[next]);
    }

    /// Publish full state — call after every mutation.
    fn publish_all(&self) {
        self.inner
            .events
            .publish(BassEvent::Status(BassRigSvc::status(self)));
        self.inner
            .events
            .publish(BassEvent::Library(BassRigSvc::presets(self)));
        self.inner
            .events
            .publish(BassEvent::Chain(BassRigSvc::chain(self)));
    }
}

/// Build the runtime profile from the library: one patch per loadable
/// `audio` preset (chain: optional drive NAM → amp NAM → optional cab IR;
/// an all-empty chain is the clean DI passthrough). Returns the profile +
/// the per-preset availability flags (index-aligned with the library).
/// Sampled presets are declared surface only for now — listed, never
/// installed.
fn build_profile(defs: &[BassPresetDef]) -> (RigProfile, Vec<bool>) {
    let mut profile = RigProfile::new("Bass");
    let mut available = Vec::with_capacity(defs.len());
    for def in defs {
        if !def.is_audio() {
            available.push(false);
            continue;
        }
        // Every referenced asset must exist, or the preset stays listed but
        // unavailable (the library can name captures the user hasn't
        // dropped into models/ yet).
        let assets = [&def.drive_nam, &def.nam, &def.ir];
        let missing = assets
            .iter()
            .any(|p| !p.is_empty() && !std::path::Path::new(p.as_str()).is_file());
        if missing {
            available.push(false);
            continue;
        }
        let mut patch = RigPatch::new(&def.name).with_trims(def.input_trim_db, def.output_trim_db);
        if !def.drive_nam.is_empty() {
            patch = patch.with_block(
                RigBlock::of_type(BlockType::Drive)
                    .with_nam(&def.drive_nam)
                    .named("Drive"),
            );
        }
        if !def.nam.is_empty() {
            patch = patch.with_block(RigBlock::nam(&def.nam).named("Amp"));
        }
        if !def.ir.is_empty() {
            patch = patch.with_block(RigBlock::cab_ir(&def.ir).named("Cab"));
        }
        profile = profile.with_patch(patch);
        available.push(true);
    }
    (profile, available)
}

/// Chain summary for the preset browser row (e.g. "DI → Amp → Cab").
fn summary_of(def: &BassPresetDef) -> String {
    if !def.is_audio() {
        return "sampled (coming)".to_string();
    }
    let mut parts = vec!["DI"];
    if !def.drive_nam.is_empty() {
        parts.push("Drive");
    }
    if !def.nam.is_empty() {
        parts.push("Amp");
    }
    if !def.ir.is_empty() {
        parts.push("Cab");
    }
    parts.push("Out");
    parts.join(" → ")
}

// r[impl primitives.architect.rig-backend]
impl RigBackend for BassRigBackend {
    type Event = BassEvent;
    type Tick = BassTick;

    fn events_hub(&self) -> &PubSub<BassEvent> {
        &self.inner.events
    }

    fn is_running(&self) -> bool {
        self.inner.rig.lock_ok().is_some()
    }

    fn pump_started(&self) -> &AtomicBool {
        &self.inner.pump_started
    }

    fn on_tick(&self, tick: &mut BassTick) {
        self.pump_tick(tick);
    }

    /// Publish `Status` on the transport edge — including the final
    /// `running: false` event when the rig stops, so remotes see it.
    fn on_running_edge(&self, _running: bool) {
        self.inner
            .events
            .publish(BassEvent::Status(BassRigSvc::status(self)));
    }

    /// Status + recent MIDI at meter rate while running.
    fn on_running_tick(&self) {
        self.inner
            .events
            .publish(BassEvent::Status(BassRigSvc::status(self)));
        self.inner
            .events
            .publish(BassEvent::Midi(self.inner.monitor.recent()));
    }

    fn midi_ports(&self) -> Vec<String> {
        midicore::midir::input_ports()
    }

    fn on_midi_ports_changed(&self, ports: &[String]) {
        tracing::info!(?ports, "bass rig: MIDI ports changed — re-attaching");
        self.reattach_midi();
    }
}

// ── service impl ────────────────────────────────────────────────────────

impl BassRigSvc for BassRigBackend {
    fn start(&self) {
        // One open at a time: concurrent starts race the drop → sleep →
        // reopen dance into audible gaps.
        if self.inner.opening.swap(true, Ordering::SeqCst) {
            tracing::info!("bass rig start: open already in progress — ignored");
            return;
        }
        let backend = self.clone();
        let _ = std::thread::Builder::new()
            .name("bass-open".into())
            .spawn(move || {
                let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Already live with unchanged prefs? No-op instead of a gap.
                    let prefs = format!("{:?}", RigManager::load(AUDIO_RIG_NAME).audio);
                    let live = backend.inner.rig.lock_ok().is_some();
                    if live && backend.inner.open_prefs.lock_ok().as_deref() == Some(prefs.as_str())
                    {
                        tracing::info!("bass rig start: already live with unchanged prefs — no-op");
                        return;
                    }
                    backend.open_blocking();
                }));
                backend.inner.opening.store(false, Ordering::SeqCst);
                if opened.is_err() {
                    tracing::error!("bass rig open panicked — see the panic logger");
                }
            });
    }

    fn stop(&self) {
        *self.inner.rig.lock_ok() = None;
        *self.inner.open_prefs.lock_ok() = None;
        tracing::info!("bass rig stopped");
        self.publish_all();
    }

    fn status(&self) -> BassStatus {
        let guard = self.inner.rig.lock_ok();
        let Some(prig) = guard.as_ref() else {
            return BassStatus {
                master_trim_db: *self.inner.master_trim.lock_ok(),
                midi_port: self.inner.midi_map.lock_ok().port.clone(),
                ..BassStatus::default()
            };
        };
        let (in_l, in_r) = prig.rig().input_peak_lr();
        let (out_l, out_r) = prig.rig().output_peak_lr();
        BassStatus {
            running: true,
            input_peak: prig.rig().input_peak(),
            output_peak: prig.rig().output_peak(),
            input_peak_l: in_l,
            input_peak_r: in_r,
            output_peak_l: out_l,
            output_peak_r: out_r,
            active_preset: prig.active_patch().map(|p| p.name.clone()),
            master_trim_db: *self.inner.master_trim.lock_ok(),
            midi_port: self.inner.midi_map.lock_ok().port.clone(),
        }
    }

    fn presets(&self) -> Vec<BassPreset> {
        let active = self.active_preset_name();
        let defs = self.inner.presets.lock_ok();
        let avail = self.inner.available.lock_ok();
        defs.iter()
            .enumerate()
            .map(|(i, d)| BassPreset {
                name: d.name.clone(),
                kind: if d.is_audio() {
                    PresetKind::Audio
                } else {
                    PresetKind::Sample
                },
                available: avail.get(i).copied().unwrap_or(false),
                active: active
                    .as_deref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(&d.name)),
                summary: summary_of(d),
            })
            .collect()
    }

    fn select_preset(&self, index: u32) {
        self.activate_index(index as usize);
    }

    fn next_preset(&self) {
        self.step_preset(1);
    }

    fn prev_preset(&self) {
        self.step_preset(-1);
    }

    fn chain(&self) -> Vec<BassBlock> {
        self.inner.blocks.lock_ok().clone()
    }

    fn toggle_block_bypass(&self, id: String) {
        let now = {
            let mut blocks = self.inner.blocks.lock_ok();
            let Some(b) = blocks.iter_mut().find(|b| b.id == id) else {
                return;
            };
            b.bypassed = !b.bypassed;
            b.bypassed
        };
        {
            let guard = self.inner.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.set_block_bypass(&id, now);
            }
        }
        self.inner
            .events
            .publish(BassEvent::Chain(BassRigSvc::chain(self)));
    }

    fn set_master_trim(&self, db: f32) {
        *self.inner.master_trim.lock_ok() = db.clamp(-60.0, 12.0);
        self.apply_master_trim();
        self.mark_state_dirty();
        self.inner
            .events
            .publish(BassEvent::Status(BassRigSvc::status(self)));
    }

    fn midi_ports(&self) -> Vec<String> {
        midicore::midir::input_ports()
    }

    fn set_midi_port(&self, name: String) {
        {
            let mut map = self.inner.midi_map.lock_ok();
            map.port = name;
            BassLibrary::save_midi_map(&map);
        }
        self.reattach_midi();
        self.inner
            .events
            .publish(BassEvent::Status(BassRigSvc::status(self)));
    }

    fn midi_recent(&self) -> Vec<MidiEvent> {
        self.inner.monitor.recent()
    }

    fn reload_library(&self) {
        let lib = BassLibrary::load_or_bootstrap();
        *self.inner.midi_map.lock_ok() = lib.midi_map;
        let n = lib.presets.len();
        *self.inner.presets.lock_ok() = lib.presets;
        *self.inner.available.lock_ok() = vec![false; n];
        // Rebuild the live rig if running (the guitar-rig reload idiom).
        let (profile, available) = {
            let defs = self.inner.presets.lock_ok();
            build_profile(&defs)
        };
        {
            let mut guard = self.inner.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                let active = prig.active_patch().map(|p| p.name.clone());
                if let Err(e) = prig.load_profile(profile, None) {
                    tracing::error!("bass library reload failed: {e}");
                }
                match active {
                    Some(name) if prig.activate_named(&name) => {}
                    _ => {
                        prig.activate(0);
                    }
                }
                *self.inner.available.lock_ok() = available;
            }
        }
        self.reattach_midi();
        self.resync_blocks();
        self.apply_master_trim();
        self.publish_all();
        tracing::info!("bass library reloaded");
    }
}

// ── shared RigCore (mounted instance-scoped as "bass") ───────────────────────
impl signal_rigs_proto::rig_core::RigCore for BassRigBackend {
    fn start(&self) {
        BassRigSvc::start(self);
    }
    fn stop(&self) {
        BassRigSvc::stop(self);
    }
    fn running(&self) -> bool {
        architect::rig::RigBackend::is_running(self)
    }
    fn presets(&self) -> Vec<signal_rigs_proto::RigPresetInfo> {
        BassRigSvc::presets(self)
            .into_iter()
            .map(|p| signal_rigs_proto::RigPresetInfo {
                loaded: p.active,
                name: p.name,
            })
            .collect()
    }
    fn load_preset(&self, index: u32) {
        BassRigSvc::select_preset(self, index);
    }
    fn midi_ports(&self) -> Vec<String> {
        BassRigSvc::midi_ports(self)
    }
    fn set_midi_port(&self, name: String) {
        BassRigSvc::set_midi_port(self, name);
    }
    fn midi_recent(&self) -> Vec<String> {
        BassRigSvc::midi_recent(self)
            .iter()
            .map(|e| format!("{e:?}"))
            .collect()
    }
}

impl BassRigStreamSource for BassRigBackend {
    fn events_hub(&self) -> &PubSub<BassEvent> {
        &self.inner.events
    }
}

impl Services for BassRigBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_bass_proto::bass::Service,
            signal_bass_proto::bass::StreamService
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, kind: &str, nam: &str) -> BassPresetDef {
        BassPresetDef {
            name: name.into(),
            kind: kind.into(),
            drive_nam: String::new(),
            nam: nam.into(),
            ir: String::new(),
            sample: String::new(),
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }

    /// DI passthrough (empty chain) installs; missing captures and sampled
    /// presets stay listed but unavailable.
    #[test]
    fn build_profile_availability() {
        let defs = vec![
            def("Bass DI", "audio", ""),
            def("Bass", "audio", "/nonexistent/amp.nam"),
            def("Upright", "sample", ""),
        ];
        let (profile, available) = build_profile(&defs);
        assert_eq!(available, vec![true, false, false]);
        assert_eq!(profile.patches.len(), 1);
        assert_eq!(profile.patches[0].name, "Bass DI");
        assert!(profile.patches[0].chain.is_empty());
    }

    /// Synth Bass is the drive → amp shape — a preset of the same rig.
    #[test]
    fn build_profile_synth_bass_chain() {
        let dir = std::env::temp_dir().join("fts-bass-backend-test");
        std::fs::create_dir_all(&dir).unwrap();
        let nam = dir.join("amp.nam");
        let drive = dir.join("drive.nam");
        std::fs::write(&nam, b"stub").unwrap();
        std::fs::write(&drive, b"stub").unwrap();
        let mut d = def("Synth Bass", "audio", nam.to_str().unwrap());
        d.drive_nam = drive.to_string_lossy().into_owned();
        let (profile, available) = build_profile(&[d]);
        assert_eq!(available, vec![true]);
        let chain = &profile.patches[0].chain;
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].block_type, BlockType::Drive);
        assert_eq!(chain[1].block_type, BlockType::Amp);
    }
}
