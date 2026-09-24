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
pub(crate) const AUDIO_RIG_NAME: &str = "Guitar Rig";

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
    /// The audio watchdog (see `audio_watchdog`): blocks rendered at the
    /// last look, since when they have not moved, and when to look for a
    /// lost device next.
    audio_calls: u64,
    audio_stalled_since: Option<std::time::Instant>,
    audio_retry_at: Option<std::time::Instant>,
    /// The pedal's LEDs as last sent, when, and until when an incoming note
    /// that matches them is the pedal echoing them back (see `sync_leds`).
    leds: Option<Vec<bool>>,
    leds_at: Option<std::time::Instant>,
    led_echo_until: Option<std::time::Instant>,
}

impl Default for MeterPump {
    fn default() -> Self {
        Self {
            midi: None,
            tick: 0,
            // 5 gesture switches + 5 direct slots, holds at the 500 ms
            // pedalboard convention (mirrors the UI's hold threshold).
            switches: {
                let mut e = FootswitchEngine::new(5, 5, std::time::Duration::from_millis(500));
                // Pressed together and held: 1 + 2 is back (previous song,
                // or profile), 4 + 5 is on (next) — see `step`; 3 + 4 is
                // the tuner.
                e.add_chord(0, 1);
                e.add_chord(3, 4);
                e.add_chord(2, 3);
                e
            },
            audio_calls: 0,
            audio_stalled_since: None,
            audio_retry_at: None,
            leds: None,
            leds_at: None,
            led_echo_until: None,
        }
    }
}

/// The footswitches' usual jobs: 1–4 their stacks, 5 tap tempo — or, in a
/// song that has sections, stepping through them (tap on, hold back).
fn default_switch_actions(song_has_parts: bool) -> Vec<String> {
    let five = if song_has_parts { "sections" } else { "tap_tempo" };
    ["stack", "stack", "stack", "stack", five]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Part `name`'s recall entry, made if it has none.
fn part_recall_mut<'a>(
    song: &'a mut crate::profiles::SongDef,
    name: &str,
) -> &'a mut crate::profiles::PartRecallDef {
    if let Some(i) = song
        .part_recalls
        .iter()
        .position(|r| r.part.eq_ignore_ascii_case(name))
    {
        return &mut song.part_recalls[i];
    }
    song.part_recalls.push(crate::profiles::PartRecallDef {
        part: name.to_string(),
        ..Default::default()
    });
    song.part_recalls.last_mut().expect("just pushed")
}

/// The patch the preset tab's audition plays under (see `choose_preset`).
/// Never saved, never listed.
const AUDITION_PATCH: &str = "\u{25B6} Audition";

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
    /// LOCK ORDER: `rig` before `profile_def`, always. A call that needs both
    /// at once takes `rig` first; one that only needs a fact from the rig
    /// (the active patch's name) takes it and lets go before touching the
    /// definition. `patches()` holds `rig` into `patch_infos`, which locks
    /// `profile_def`; `nodes()` and `perf()` used to take them the other way
    /// round, and the two requests a patch switch fires together deadlocked —
    /// every tokio worker then queued behind `rig`, and the engine stopped
    /// answering anything, `/health` included, at 0% CPU.
    rig: SharedRig,
    /// The preset tab's audition — `(preset, snapshot)` playing on its own,
    /// outside the profile (see `choose_preset`). `None` = a patch is playing.
    audition: Arc<Mutex<Option<(String, String)>>>,
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
    /// Every other profile in the library — not the active one, which is
    /// [`profile_def`](Self::profile_def) and nowhere else, so an edit to it
    /// cannot land in a stale copy.
    other_profiles: Arc<Mutex<Vec<ProfileDef>>>,
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
    /// The rig is meant to be playing (started, not stopped): the audio
    /// watchdog brings it back when its device drops out and returns.
    wants_audio: Arc<std::sync::atomic::AtomicBool>,
    /// Each stack switch's behaviour right now (the song's entry for it,
    /// else the profile's stack), in stack order — see `apply_song_stacks`.
    switch_modes: Arc<Mutex<Vec<crate::profiles::SwitchMode>>>,
    /// Each footswitch's job right now (1–5, a `SWITCH_ACTIONS` key) — the
    /// song's and the part's assignments over the defaults.
    switch_actions: Arc<Mutex<Vec<String>>>,
    /// Stacks the part that is up tunes, in stack order.
    part_tuned: Arc<Mutex<Vec<bool>>>,
    /// Where a held momentary switch goes back to on release.
    momentary_return: Arc<Mutex<Option<String>>>,
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
    /// The process CPU meter's last sample (see [`CpuMeter`]).
    cpu: Arc<Mutex<CpuMeter>>,
}

/// The process's CPU share, from its CPU time against the wall clock.
///
/// Every remote polls status at meter rate, so a delta per call would be a
/// delta over a few milliseconds — noise. It re-samples at most every half
/// second and hands out the last value in between.
#[derive(Default)]
struct CpuMeter {
    last: Option<(std::time::Instant, std::time::Duration)>,
    value: f32,
}

impl CpuMeter {
    const WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

    fn sample(&mut self, cores: u32) -> f32 {
        let now = std::time::Instant::now();
        let Some(cpu) = process_cpu_time() else {
            return 0.0;
        };
        match self.last {
            Some((at, _)) if now.duration_since(at) < Self::WINDOW => {}
            Some((at, before)) => {
                let wall = now.duration_since(at).as_secs_f32();
                let used = cpu.saturating_sub(before).as_secs_f32();
                self.value = (used / wall / cores.max(1) as f32).clamp(0.0, 1.0);
                self.last = Some((now, cpu));
            }
            None => self.last = Some((now, cpu)),
        }
        self.value
    }
}

/// User + system CPU time this process has used, all threads.
#[cfg(unix)]
fn process_cpu_time() -> Option<std::time::Duration> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage writes a whole `rusage` on success and we read it
    // only then.
    let usage = unsafe {
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) != 0 {
            return None;
        }
        usage.assume_init()
    };
    let tv = |t: libc::timeval| {
        std::time::Duration::from_secs(t.tv_sec as u64)
            + std::time::Duration::from_micros(t.tv_usec as u64)
    };
    Some(tv(usage.ru_utime) + tv(usage.ru_stime))
}

#[cfg(not(unix))]
fn process_cpu_time() -> Option<std::time::Duration> {
    None
}

impl Default for GuitarRigBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GuitarRigBackend {
    pub fn new() -> Self {
        let lib = RigLibrary::load_or_bootstrap();
        let others = others_of(&lib.profiles, &lib.profile.name);
        let backend = Self {
            rig: Arc::new(Mutex::new(None)),
            audition: Arc::new(Mutex::new(None)),
            boost_on: Arc::new(Mutex::new(false)),
            boost_level: Arc::new(Mutex::new(BOOST_LEVELS[0])),
            blocks: Arc::new(Mutex::new(Vec::new())),
            tempo: Arc::new(Mutex::new(None)),
            taps: Arc::new(Mutex::new(TapTracker::default())),
            profile_def: Arc::new(Mutex::new(lib.profile)),
            other_profiles: Arc::new(Mutex::new(others)),
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
            wants_audio: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            switch_modes: Arc::new(Mutex::new(Vec::new())),
            switch_actions: Arc::new(Mutex::new(default_switch_actions(false))),
            part_tuned: Arc::new(Mutex::new(Vec::new())),
            momentary_return: Arc::new(Mutex::new(None)),
            open_prefs: Arc::new(Mutex::new(None)),
            midi_map: Arc::new(Mutex::new(lib.midi_map)),
            keymap: Arc::new(Mutex::new(lib.keymap)),
            headphone: Arc::new(Mutex::new(HeadphoneState::default())),
            master_trim: Arc::new(Mutex::new(0.0)),
            midi_log: Arc::new(Mutex::new(Vec::new())),
            revision: Arc::new(Mutex::new(0)),
            events: architect::rig::events_hub(),
            pump_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cpu: Arc::new(Mutex::new(CpuMeter::default())),
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
    /// Bring the rig back after its audio device drops out.
    ///
    /// An interface unplugged or powered off kills the engine — the device
    /// reports itself gone, or simply stops calling back — and nothing used to
    /// reopen it: the rig stayed silent until the app restarted. Here a dead
    /// engine is released, and once the configured device is enumerable again
    /// the rig reopens through the normal start path, landing back on the
    /// same patch (`open_blocking` restores the last state). The same retry
    /// covers a rig whose device was not there when the app launched.
    fn audio_watchdog(&self, pump: &mut MeterPump) {
        use std::sync::atomic::Ordering;
        /// No blocks for this long on a live engine is a dead one.
        const STALL: std::time::Duration = std::time::Duration::from_secs(2);
        /// How often to look for a lost device.
        const RETRY: std::time::Duration = std::time::Duration::from_secs(2);
        if !self.wants_audio.load(Ordering::Relaxed) || self.opening.load(Ordering::Relaxed) {
            pump.audio_stalled_since = None;
            return;
        }
        let now = std::time::Instant::now();
        let health = self
            .rig
            .lock_ok()
            .as_ref()
            .map(|p| (p.rig().stream_failed(), p.rig().blocks_rendered()));
        match health {
            Some((failed, calls)) => {
                let stalled = if calls == pump.audio_calls {
                    now.duration_since(*pump.audio_stalled_since.get_or_insert(now)) >= STALL
                } else {
                    pump.audio_calls = calls;
                    pump.audio_stalled_since = None;
                    false
                };
                if failed || stalled {
                    tracing::warn!(
                        audio.device_gone = failed,
                        audio.stalled = stalled,
                        "audio device lost — releasing it; the rig reopens when it returns"
                    );
                    // Dropping the dead engine releases the device; the
                    // retry below reopens it.
                    let dead = self.rig.lock_ok().take();
                    drop(dead);
                    *self.open_prefs.lock_ok() = None;
                    pump.audio_stalled_since = None;
                    pump.audio_calls = 0;
                    pump.audio_retry_at = Some(now + RETRY);
                    self.publish_state();
                }
            }
            None => {
                if pump.audio_retry_at.is_some_and(|t| now < t) {
                    return;
                }
                pump.audio_retry_at = Some(now + RETRY);
                if audio_device_present() {
                    tracing::info!("audio device present — reopening the rig");
                    Rig::start(self);
                }
            }
        }
    }

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
        if !crate::library::rig_is_design() && (pump.tick == 1 || pump.tick.is_multiple_of(60)) {
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
        // The pedal's LEDs follow the rig (sent only on change, and a
        // refresh every few seconds).
        if !crate::library::rig_is_design() {
            self.sync_leds(pump);
        }
        // Audio device drop-outs: twice a second.
        if !crate::library::rig_is_design() && pump.tick.is_multiple_of(15) {
            self.audio_watchdog(pump);
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
            // (note, down): a note pedal's switches — `None` for a Note On
            // at velocity 0, resolved against the switch's state below.
            let mut notes: Vec<(u8, Option<bool>)> = Vec::new();
            {
                let mut log = self.midi_log.lock_ok();
                for msg in stream.drain() {
                    match msg.to_event() {
                        Some(midicore::MidiEvent::ControlChange {
                            controller, value, ..
                        }) => {
                            let (cc, val) = (u8::from(controller), u8::from(value));
                            tracing::debug!("midi cc {cc} = {val}");
                            events.push((cc, val));
                        }
                        // Notes are read from the raw status, not the decoded
                        // event: a Note On at velocity 0 decodes to Note Off,
                        // and some pedals send exactly that as their *press*
                        // (see `FootswitchEngine::note_switch_is_down`).
                        Some(
                            midicore::MidiEvent::NoteOn { .. }
                            | midicore::MidiEvent::NoteOff { .. },
                        ) => {
                            let [status, key, velocity] = msg.bytes();
                            let down = match (status & 0xF0, velocity) {
                                (0x90, v) if v > 0 => Some(true),
                                (0x90, _) => None,
                                _ => Some(false),
                            };
                            tracing::info!(
                                midi.note = key,
                                midi.velocity = velocity,
                                midi.status = status,
                                "midi note"
                            );
                            notes.push((key, down));
                        }
                        _ => {}
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
                    tap_notes: m.tap_notes.clone(),
                    direct: m.direct.iter().map(|d| (d.cc, d.slot)).collect(),
                }
            };
            // Momentary switches (the stack switches, not tap tempo on 5).
            {
                let modes = self.switch_modes.lock_ok();
                let jobs = self.switch_actions.lock_ok();
                let job = |i: usize| jobs.get(i).map_or("", String::as_str);
                // Only a switch doing its stack's job can be momentary.
                let flags: Vec<bool> = (0..4)
                    .map(|i| job(i) == "stack" && modes.get(i).is_some_and(|m| m.momentary))
                    .collect();
                pump.switches.set_momentary(&flags);
            }
            let mut actions: Vec<FootswitchAction> = events
                .into_iter()
                .filter_map(|(cc, val)| pump.switches.on_cc(&map, cc, val))
                .collect();
            let echo_window = pump
                .led_echo_until
                .is_some_and(|t| std::time::Instant::now() < t);
            for (note, down) in notes {
                // The pedal repeating an LED state we just sent is not a press.
                if echo_window {
                    if let (Some(i), Some(leds)) = (
                        map.tap_notes.iter().position(|n| *n == u32::from(note)),
                        pump.leds.as_ref(),
                    ) {
                        if down == Some(leds.get(i).copied().unwrap_or(false)) {
                            continue;
                        }
                    }
                }
                let down = down.unwrap_or_else(|| !pump.switches.note_switch_is_down(&map, note));
                actions.extend(pump.switches.on_note(&map, note, down));
            }
            actions.extend(pump.switches.poll_holds());
            // LEDs after this tick's switching (below the action loop).
            for action in actions {
                match action {
                    FootswitchAction::Tap(sw) => {
                        tracing::info!("footswitch {} tap", sw + 1);
                        self.switch_tap(sw);
                    }
                    FootswitchAction::Hold(sw) => {
                        tracing::info!("footswitch {} hold", sw + 1);
                        self.switch_hold(sw);
                    }
                    FootswitchAction::Direct(slot) => {
                        tracing::info!("footswitch direct → slot {slot}");
                        self.hold_layer_action(slot as usize);
                    }
                    FootswitchAction::Press(sw) => {
                        tracing::info!("footswitch {} down (momentary)", sw + 1);
                        Rig::press_stack(self, sw as u32);
                    }
                    FootswitchAction::LongHold(sw) => {
                        tracing::info!("footswitch {} long hold", sw + 1);
                        // A stepping switch's usual hold moved out here.
                        self.hold_layer_action(sw);
                    }
                    FootswitchAction::Chord(a, b) => {
                        tracing::info!("footswitches {} + {} together", a + 1, b + 1);
                        match (a, b) {
                            (0, 1) => self.step(-1),
                            (3, 4) => self.step(1),
                            (2, 3) => Rig::toggle_tuner(self),
                            _ => {}
                        }
                    }
                    FootswitchAction::Release(sw) => {
                        tracing::info!("footswitch {} up (momentary)", sw + 1);
                        Rig::release_stack(self, sw as u32);
                    }
                }
            }
        }
    }

    /// Light the pedal's switches from the rig: the one whose stack is
    /// playing on, the rest off — sent when that changes, and again every few
    /// seconds so a pedal that reconnected (or toggled a light itself)
    /// comes back into step.
    fn sync_leds(&self, pump: &mut MeterPump) {
        const REFRESH: std::time::Duration = std::time::Duration::from_secs(3);
        let (device, notes) = {
            let m = self.midi_map.lock_ok();
            (m.led_output.clone(), m.tap_notes.clone())
        };
        if device.is_empty() || notes.is_empty() {
            return;
        }
        let active = self.rig.lock_ok().as_ref().and_then(ProfileRig::active_stack);
        // The stack switches (tap tempo on the last switch keeps its own).
        let jobs = self.switch_actions.lock_ok().clone();
        let want: Vec<bool> = (0..notes.len().min(4))
            .map(|i| active == Some(i) && jobs.get(i).is_none_or(|j| j == "stack"))
            .collect();
        let now = std::time::Instant::now();
        let due = pump.leds.as_ref() != Some(&want)
            || pump.leds_at.is_none_or(|t| now.duration_since(t) >= REFRESH);
        if !due {
            return;
        }
        let mut bytes = Vec::with_capacity(want.len() * 3);
        for (i, &lit) in want.iter().enumerate() {
            let note = u8::try_from(notes[i]).unwrap_or(0).min(127);
            if lit {
                bytes.extend_from_slice(&[0x90, note, 127]);
            } else {
                bytes.extend_from_slice(&[0x80, note, 0]);
            }
        }
        let reached = signal_rig_host::midi_hub::send_to(&device, &bytes);
        if pump.leds.as_ref() != Some(&want) {
            tracing::info!(midi.leds = ?want, midi.device = %device, midi.reached = reached, "pedal LEDs");
        }
        pump.leds = Some(want);
        pump.leds_at = Some(now);
        pump.led_echo_until = Some(now + std::time::Duration::from_millis(60));
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
    /// Status without the CPU meter — [`Rig::status`] adds it.
    fn raw_status(&self) -> RigStatus {
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
                        // The process meter is added by `Rig::status`.
                        cpu: 0.0,
                        cores: 0,
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

    /// Every profile's name, the active one first.
    fn profile_names(&self) -> Vec<String> {
        let mut names = vec![self.profile_def.lock_ok().name.clone()];
        names.extend(self.other_profiles.lock_ok().iter().map(|p| p.name.clone()));
        names
    }

    /// The active profile's default patch by name — its default scene, or
    /// the first patch when it names none (or one it no longer has).
    fn default_patch_name(&self) -> String {
        let def = self.profile_def.lock_ok();
        def.patches
            .iter()
            .find(|p| {
                !def.default_patch.is_empty() && p.name.eq_ignore_ascii_case(&def.default_patch)
            })
            .or_else(|| def.patches.first())
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    /// Make `name` the playing profile, if it is not already. `false` when
    /// nothing changed — already playing, or no such profile.
    fn ensure_profile(&self, name: &str) -> bool {
        if name.is_empty() || self.profile_def.lock_ok().name.eq_ignore_ascii_case(name) {
            return false;
        }
        let def = self
            .other_profiles
            .lock_ok()
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .cloned();
        match def {
            Some(def) => {
                self.switch_profile(def);
                true
            }
            None => {
                tracing::warn!("no profile named '{name}' — staying on the one loaded");
                false
            }
        }
    }

    /// Activate a patch of the playing profile by name, in the engine or,
    /// with none, in design mode. `false` if the profile has no such patch.
    fn activate_named(&self, name: &str) -> bool {
        let mut guard = self.rig.lock_ok();
        match guard.as_mut() {
            Some(prig) => activate_patch_by_name(prig, name),
            None => {
                drop(guard);
                let known = self
                    .profile_def
                    .lock_ok()
                    .patches
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(name));
                if known {
                    *self.design_patch.lock_ok() = name.to_string();
                }
                known
            }
        }
    }

    /// Play `def` instead of the active profile, which goes back on the
    /// shelf. Rebuilds the rig and lands on the new profile's default patch.
    fn switch_profile(&self, def: ProfileDef) {
        let name = def.name.clone();
        {
            let mut current = self.profile_def.lock_ok();
            let old = std::mem::replace(&mut *current, def);
            let mut others = self.other_profiles.lock_ok();
            others.retain(|p| !p.name.eq_ignore_ascii_case(&name));
            others.push(old);
            others.sort_by_key(|p| p.name.to_lowercase());
        }
        // The patch that was up belongs to the other profile: land on the
        // new one's default scene (design mode has no engine to do it).
        *self.design_patch.lock_ok() = self.default_patch_name();
        let rebuilt = {
            let def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Err(e) = prig.load_profile(rebuilt, None) {
                    tracing::error!("profile switch: load failed: {e}");
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
        self.spawn_drive_calibration();
        tracing::info!("profile switched → {name}");
    }

    fn reload_rebuilt(&self, rebuilt: signal_sampler::rig_profile::RigProfile) {
        self.reload_rebuilt_activating(rebuilt, None);
    }

    /// [`reload_rebuilt`](Self::reload_rebuilt), landing on `activate` (a
    /// patch name) instead of whatever was playing.
    fn reload_rebuilt_activating(
        &self,
        rebuilt: signal_sampler::rig_profile::RigProfile,
        activate: Option<&str>,
    ) {
        let active = activate.map(str::to_string).or_else(|| {
            let guard = self.rig.lock_ok();
            guard
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
        });
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
        // Rebuilt from the profile: the song's rotations went with the old
        // stacks.
        self.apply_song_stacks();
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
        let flat = self.flat_def();
        let def = &flat;
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
            let preset = if block_name.eq_ignore_ascii_case("Amp R") {
                def.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&active))
                    .map(|p| p.preset2.clone())
                    .filter(|p| !p.is_empty())?
            } else {
                pool_preset_of(&def, &active)?
            };
            return def
                .presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&preset))
                .map(|p| p.nam.clone());
        }
        None
    }

    /// Move a NAM block's drive knob live, holding its loudness: the input
    /// trim is what the new position stands for, and the output is the
    /// block's built Output Level (see `nodes::apply_block_levels`) moved by
    /// the cached curve's difference between the built position and the new
    /// one. Relative to the level the block was built with, never a fresh
    /// absolute gain.
    fn apply_drive(&self, block_id: &str, block_name: &str, drive: f32) {
        let (sr, built) = {
            let guard = self.rig.lock_ok();
            let Some(prig) = guard.as_ref() else { return };
            let block = prig.active_patch().and_then(|p| {
                p.chain
                    .iter()
                    .find(|b| b.name.eq_ignore_ascii_case(block_name) && !b.nam.is_empty())
                    .cloned()
            });
            (f64::from(prig.sample_rate()), block)
        };
        let Some(block) = built else { return };
        let path = std::path::Path::new(&block.nam);
        let built_drive = block.param_f32("drive").unwrap_or(0.5);
        if signal_sampler::nam_calibrate::drive_curve_cached(path, sr).is_none() {
            self.measure_drive_later(block.nam.clone(), sr);
        }
        let delta = |d: f32| signal_sampler::nam_calibrate::drive_output_delta_cached(path, sr, d);
        let in_db = signal_sampler::nam_calibrate::drive_input_db(drive);
        let out_db = block.output_trim_db - delta(built_drive) + delta(drive);
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

    /// Measure `path`'s drive curve off the switch path, then re-apply the
    /// live chain's drives. At most one measurement per model in flight.
    fn measure_drive_later(&self, path: String, sample_rate: f64) {
        static PENDING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        {
            let mut pending = PENDING.lock_ok();
            if pending.contains(&path) {
                return;
            }
            pending.push(path.clone());
        }
        tracing::info!(model = %path, "drive calibration: not measured yet — measuring in the background");
        let backend = self.clone();
        std::thread::spawn(move || {
            let _ = signal_sampler::nam_calibrate::drive_curve(std::path::Path::new(&path), sample_rate);
            PENDING.lock_ok().retain(|p| p != &path);
            let _ = backend;
        });
    }

    /// Re-apply constant-loudness drive to every NAM board block of the
    /// active chain (after activation / reload — cached, so cheap).
    /// Levels are built into the chain (`nodes::apply_block_levels`), so a
    /// switch applies no gain of its own. Kept as the one name the switch
    /// paths call, so a future switch-time step has a single home.
    fn apply_all_drives(&self) {}

    /// Level every patch to a common loudness, measured where it is heard:
    /// at the rig's output, through the rig's own engine (see
    /// `crate::measure` — an offline `GuitarRig`, the live one's code path
    /// without the audio device, so this runs in the background and in
    /// seconds, and the live audio is never touched).
    ///
    /// Each patch's error moves the *snapshot* it plays (a composed patch's
    /// level is its snapshot's — `flatten` copies it over the patch's own, so
    /// writing the patch level did nothing), or the patch itself when it
    /// plays a legacy pool preset. A second pass re-measures what is still
    /// more than 0.3 dB out: the chain ends in the rig's limiter, so a patch
    /// that was far too hot reads quieter than it is and one step undershoots.
    fn run_levelling(&self) {
        const PASSES: usize = 3;
        const GOOD_ENOUGH_DB: f32 = 0.3;
        let sample_rate = self
            .rig
            .lock_ok()
            .as_ref()
            .map_or(48_000, signal_sampler::rig_profile::ProfileRig::sample_rate);
        // Each patch's own target: the rig's, plus its snapshot's gain bias.
        let comp = RigLibrary::load_compositions();
        let target_of = |name: &str| {
            let def = self.profile_def.lock_ok();
            def.patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map_or(signal_sampler::patch_level::TARGET_LUFS as f32, |p| {
                    crate::compose::loudness_target(&comp, &p.rig_preset, &p.snapshot)
                })
        };
        tracing::info!(sample_rate, "patch levelling: begin");

        for pass in 0..PASSES {
            // The profile as the live rig builds it, from what is saved now.
            let profile = {
                let def = self.profile_def.lock_ok();
                let dps = self.drive_presets.lock_ok();
                profile_from_library(&def, &dps)
            };
            let patches = profile.patches;
            {
                let mut progress = self.levelling.lock_ok();
                *progress = signal_guitar_proto::LevelProgress {
                    done: 0,
                    total: patches.len() as u32,
                    patch: String::new(),
                    complete: false,
                    results: Vec::new(),
                };
            }
            self.publish_levelling();

            let measured: Vec<Option<f32>> =
                crate::levelling::par_map(&patches, 0, |patch| {
                    let lufs = crate::measure::patch_lufs(patch, sample_rate);
                    tracing::info!(patch = %patch.name, pass, lufs, "patch levelling: measured");
                    {
                        let mut progress = self.levelling.lock_ok();
                        progress.done += 1;
                        progress.patch.clone_from(&patch.name);
                        progress.results.push(signal_guitar_proto::PatchLevel {
                            patch: patch.name.clone(),
                            lufs: lufs.unwrap_or(f32::NEG_INFINITY),
                            trim_db: lufs.map_or(f32::NAN, |l| target_of(&patch.name) - l),
                        });
                    }
                    self.publish_levelling();
                    lufs
                });

            let errors: Vec<(String, f32)> = patches
                .iter()
                .zip(&measured)
                .filter_map(|(p, l)| {
                    l.filter(|l| *l > signal_sampler::loudness::SILENCE_LUFS as f32)
                        .map(|l| (p.name.clone(), target_of(&p.name) - l))
                })
                .collect();
            if errors.iter().all(|(_, e)| e.abs() <= GOOD_ENOUGH_DB) {
                break;
            }
            self.apply_level_errors(&errors);
        }

        {
            let mut progress = self.levelling.lock_ok();
            progress.complete = true;
            progress.patch.clear();
        }
        self.publish_levelling();
        tracing::info!("patch levelling: done");
    }

    /// Move each patch's level by its measured error (dB). A patch's
    /// `level_db` is an offset on top of its snapshot's (see
    /// `compose::flatten`), and the snapshots are levelled on their own
    /// (`signal rig level-presets`), so what is left to correct belongs to
    /// the patch — its overrides — and moving a shared snapshot for it would
    /// knock every other patch that plays it out of level. Saves the profile
    /// and rebuilds the live chains.
    fn apply_level_errors(&self, errors: &[(String, f32)]) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            for (name, error) in errors {
                if let Some(patch) = def
                    .patches
                    .iter_mut()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                {
                    patch.level_db = (patch.level_db + error).clamp(-40.0, 40.0);
                    tracing::info!(patch = %name, correction_db = error, level_db = patch.level_db, "patch levelling: patch offset moved");
                }
            }
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    fn publish_levelling(&self) {
        let progress = self.levelling.lock_ok().clone();
        self.events.publish(RigEvent::Levelling(progress));
    }

    /// Pre-measure drive curves for every NAM the profile can reach (drive
    /// preset options + the pool presets) — the import-time offline test.
    /// Runs on its own thread; results land in the on-disk cache.
    fn spawn_drive_calibration(&self) {
        let backend = self.clone();
        std::thread::spawn(move || backend.run_drive_calibration());
    }

    /// [`spawn_drive_calibration`](Self::spawn_drive_calibration), on the
    /// calling thread — for a caller that has more to do once the trims have
    /// settled.
    fn run_drive_calibration(&self) {
        let backend = self;
        {
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
            // Every capture any patch actually plays — the composed patches'
            // amps come from the module library, not the pool, and a model
            // missing here was first measured by the footswitch that reached
            // it (see `apply_drive`).
            if let Some(prig) = backend.rig.lock_ok().as_ref() {
                for patch in prig.patches() {
                    for block in &patch.chain {
                        if !block.nam.is_empty() {
                            paths.push(block.nam.clone());
                        }
                    }
                }
            }
            paths.retain(|p| !p.is_empty());
            paths.sort();
            paths.dedup();
            // Side by side, leaving two cores to the audio thread and the UI.
            let threads = crate::levelling::cores().saturating_sub(2).max(1);
            crate::levelling::par_map(&paths, threads, |path| {
                let path = path.clone();
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
            });
            // With curves in cache, snap the live chain to compensated trims.
            backend.apply_all_drives();
            tracing::info!("drive calibration: complete");
        }
    }

    /// The hold-layer functions, by hold-layer slot (0-based): Ambient
    /// stack, FX toggle, next song, boost toggle, tuner. Shared by held
    /// footswitches 1–5 and the direct CC 106–110 mapping.
    fn hold_layer_action(&self, slot: usize) {
        match slot {
            0 => Rig::press_stack(self, 4),
            1 => Rig::toggle_fx(self),
            2 => self.toggle_song_mode(),
            3 => Rig::toggle_boost(self),
            4 => Rig::toggle_tuner(self),
            _ => {}
        }
    }

    /// Footswitch `sw` (0-based) tapped: whatever job it has right now.
    fn switch_tap(&self, sw: usize) {
        let job = self.switch_job(sw);
        match job.as_str() {
            "stack" if sw < 4 => Rig::press_stack(self, sw as u32),
            "tap_tempo" => Rig::tap_tempo(self),
            "parts" => self.step_part_impl(1, false),
            "sections" => self.step_part_impl(1, true),
            "songs" => Rig::next_song(self),
            "tuner" => Rig::toggle_tuner(self),
            "boost" => Rig::toggle_boost(self),
            "fx" => Rig::toggle_fx(self),
            _ => tracing::info!(switch = sw + 1, %job, "footswitch has nothing to do"),
        }
    }

    /// Footswitch `sw` held: a stepping switch goes back; any other does
    /// its hold-layer function.
    fn switch_hold(&self, sw: usize) {
        match self.switch_job(sw).as_str() {
            "parts" => self.step_part_impl(-1, false),
            "sections" => self.step_part_impl(-1, true),
            "songs" => Rig::prev_song(self),
            // Switch 5's hold is free: the tuner is switches 3 + 4 together.
            "tap_tempo" if sw == 4 => tracing::info!("footswitch 5 hold: unassigned"),
            _ => self.hold_layer_action(sw),
        }
    }

    fn switch_job(&self, sw: usize) -> String {
        self.switch_actions
            .lock_ok()
            .get(sw)
            .cloned()
            .unwrap_or_default()
    }

    /// Step through the song that is up: to the next (`dir` > 0) or previous
    /// part, or — `sections` — to the first part of the next section, or back
    /// to the start of this one (the previous one's, from its start). Stops
    /// at either end rather than leaving the song.
    fn step_part_impl(&self, dir: i32, sections: bool) {
        let Some(song) = self.current_song_def() else {
            tracing::info!("part step: no song is up (Setlist mode only)");
            return;
        };
        let n = song.parts.len();
        if n == 0 {
            return;
        }
        let cur = (*self.part_index.lock_ok()).min(n - 1);
        let target = if sections {
            let starts = song.section_starts();
            let at = starts.iter().rposition(|&st| st <= cur).unwrap_or(0);
            if dir > 0 {
                starts.get(at + 1).copied().unwrap_or(cur)
            } else if cur > starts[at] {
                starts[at]
            } else {
                starts[at.saturating_sub(1)]
            }
        } else {
            (cur as i32 + dir).clamp(0, n as i32 - 1) as usize
        };
        if target == cur {
            tracing::info!(part = cur, "part step: at the end of the song");
            return;
        }
        tracing::info!(from = cur, to = target, sections, "part step");
        Rig::select_part(self, target as u32);
    }

    /// The song that is up, as the library has it (Setlist mode only).
    fn current_song_def(&self) -> Option<crate::profiles::SongDef> {
        let name = self.current_song_name()?;
        self.songs_lib
            .lock_ok()
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&name))
            .cloned()
    }

    /// The part that is up and its recall entry, when it has one.
    fn current_part_def(&self) -> Option<(String, crate::profiles::PartRecallDef)> {
        let song = self.current_song_def()?;
        let name = song.parts.get(*self.part_index.lock_ok())?.clone();
        let recall = song.part_recall(&name).cloned().unwrap_or_default();
        Some((name, recall))
    }

    /// A stack switch landed on a patch a later part of this section
    /// recalls: the song has moved on to that part (the bridge's build
    /// played on switches 1–4) — follow it, so the next step goes on from
    /// there rather than back into the build.
    fn follow_part(&self) {
        let (Some(song), Some(active)) = (self.current_song_def(), self.active_patch_name()) else {
            return;
        };
        let cur = *self.part_index.lock_ok();
        let section = song.section_of(cur);
        let hit = (0..song.parts.len()).find(|&i| {
            i != cur
                && song.section_of(i).eq_ignore_ascii_case(&section)
                && song
                    .part_recall(&song.parts[i])
                    .is_some_and(|r| r.patch.eq_ignore_ascii_case(&active))
        });
        // Only within one run of the section (a later "Chorus" is its own).
        let starts = song.section_starts();
        let run = |i: usize| starts.iter().rposition(|&st| st <= i);
        if let Some(i) = hit.filter(|&i| run(i) == run(cur)) {
            tracing::info!(part = %song.parts[i], "part follows the switch");
            *self.part_index.lock_ok() = i;
            self.mark_state_dirty();
        }
    }

    /// Switch 8 (hold switch 3): Setlist mode ↔ Profile mode. Into the
    /// setlist, the song that is up is recalled — switches tuned for it.
    fn toggle_song_mode(&self) {
        if *self.perform_mode.lock_ok() == PERFORM_SETLIST {
            Rig::set_perform_mode(self, 1);
        } else {
            Rig::set_perform_mode(self, PERFORM_SETLIST);
            let idx = *self.song_index.lock_ok();
            self.recall_song(idx);
        }
    }

    /// The two chords: forward (`+1`, switches 4 + 5) or back (`−1`,
    /// switches 1 + 2) — through the setlist's songs in Setlist mode,
    /// through the profiles (by name, wrapping) otherwise.
    fn step(&self, dir: i32) {
        if *self.perform_mode.lock_ok() == PERFORM_SETLIST {
            if dir > 0 {
                Rig::next_song(self);
            } else {
                Rig::prev_song(self);
            }
            return;
        }
        let current = self.profile_def.lock_ok().name.clone();
        let mut names = self.profile_names();
        names.sort_by_key(|n| n.to_lowercase());
        let Some(at) = names.iter().position(|n| n.eq_ignore_ascii_case(&current)) else {
            return;
        };
        let len = names.len() as i32;
        let next = &names[(at as i32 + dir).rem_euclid(len) as usize];
        tracing::info!(profile.from = %current, profile.to = %next, "profile step");
        Rig::select_profile(self, next.clone());
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
            profile: self.profile_def.lock_ok().name.clone(),
            perform_mode: *self.perform_mode.lock_ok(),
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
        *self.perform_mode.lock_ok() = st.perform_mode.min(2);
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

    /// Re-apply the main-output fader: master trim + mute. (The patch's own
    /// level is applied in the rig's output stage, where a switch crossfades
    /// it and the outgoing patch's tail keeps its own.)
    fn apply_main_mute(&self) {
        let mute = self.headphone.lock_ok().main_mute;
        let trim = *self.master_trim.lock_ok();
        {
            let guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_ref() {
                prig.rig()
                    .set_output_trim_db(trim + if mute { -96.0 } else { 0.0 });
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
    /// The profile as the live rig would load it — pure data, no engine.
    /// Design mode's source for patches, stacks and chains.
    fn design_profile(&self) -> signal_sampler::rig_profile::RigProfile {
        let def = self.profile_def.lock_ok();
        let dps = self.drive_presets.lock_ok();
        profile_from_library(&def, &dps)
    }

    /// The profile with every composed patch resolved — what actually plays.
    fn flat_def(&self) -> crate::profiles::ProfileDef {
        let def = self.profile_def.lock_ok().clone();
        crate::compose::flatten(&def, &RigLibrary::load_compositions())
    }

    /// The patch playing now: the rig's active one, or design mode's.
    fn live_patch_name(&self) -> Option<String> {
        let live = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
        live.or_else(|| {
            let profile = self.design_profile();
            self.design_active_patch(&profile)
        })
    }

    /// The live profile, plus the audition patch when one is playing — kept
    /// last, so every real patch keeps its index.
    fn profile_with_audition(&self) -> signal_sampler::rig_profile::RigProfile {
        let mut def = self.profile_def.lock_ok().clone();
        if let Some((preset, snapshot)) = self.audition.lock_ok().clone() {
            if let Some(patch) = crate::compose::snapshot_patch(&def, &preset, &snapshot, AUDITION_PATCH) {
                def.patches.push(patch);
            }
        }
        let dps = self.drive_presets.lock_ok();
        profile_from_library(&def, &dps)
    }

    /// Stop auditioning (a real patch is being played).
    fn end_audition(&self) {
        self.audition.lock_ok().take();
    }

    /// Change the live patch's definition, save it, and rebuild so it is heard.
    fn edit_live_patch(&self, edit: impl FnOnce(&mut crate::profiles::PatchDef)) {
        let Some(name) = self.live_patch_name() else {
            return;
        };
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(patch) = def
                .patches
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
            else {
                return;
            };
            edit(patch);
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    /// The live patch's effective pick for one module.
    fn live_pick(
        &self,
        comp: &crate::compose::Compositions,
        module: &str,
    ) -> Option<crate::profiles::ModuleChoiceDef> {
        let name = self.live_patch_name()?;
        let def = self.profile_def.lock_ok();
        let patch = def
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))?;
        crate::compose::module_picks(comp, patch)
            .into_iter()
            .find(|m| m.module.eq_ignore_ascii_case(module))
    }

    /// Design mode's active patch: the restored/selected `design_patch`, or
    /// the first patch when that is empty or no longer in the profile — the
    /// same fallback `design_blocks` builds the chain from.
    fn design_active_patch(
        &self,
        profile: &signal_sampler::rig_profile::RigProfile,
    ) -> Option<String> {
        let chosen = self.design_patch.lock_ok().clone();
        profile
            .patches
            .iter()
            .find(|p| !chosen.is_empty() && p.name.eq_ignore_ascii_case(&chosen))
            .or_else(|| profile.patches.get(profile.default_patch))
            .or_else(|| profile.patches.first())
            .map(|p| p.name.clone())
    }

    /// The sidebar's view of `patches`, grouped by `stacks`. One mapping
    /// for the live rig and design mode, so the two cannot drift apart.
    fn patch_infos(
        &self,
        patches: &[signal_sampler::rig_profile::RigPatch],
        stacks: &[signal_sampler::rig_profile::RigStack],
        active: Option<&str>,
        available: impl Fn(usize) -> bool,
    ) -> Vec<PatchInfo> {
        // A song's own patches are listed while that song is up, not as the
        // profile's (see `PatchDef::song`).
        let song = self.current_song_name().unwrap_or_default();
        let def = self.profile_def.lock_ok();
        let hidden = |name: &str| {
            def.patches.iter().any(|d| {
                d.name.eq_ignore_ascii_case(name)
                    && !d.song.is_empty()
                    && !d.song.eq_ignore_ascii_case(&song)
            })
        };
        patches
            .iter()
            .enumerate()
            // The audition is not one of the profile's patches.
            .filter(|(_, p)| p.name != AUDITION_PATCH && !hidden(&p.name))
            .map(|(i, p)| {
                // Grouped by the profile's own stacks — not the live
                // rotations, which a song re-tunes (WASHED's switch 1 is not
                // Worship's Clean stack). A song's patches group under the
                // song.
                let _ = stacks;
                let song_of = def
                    .patches
                    .iter()
                    .find(|d| d.name.eq_ignore_ascii_case(&p.name))
                    .map(|d| d.song.clone())
                    .unwrap_or_default();
                let stack_entry = def
                    .stacks
                    .iter()
                    .find(|st| st.patches.iter().any(|n| n.eq_ignore_ascii_case(&p.name)));
                let stack = if song_of.is_empty() {
                    stack_entry.map(|st| st.name.clone()).unwrap_or_default()
                } else {
                    song_of
                };
                let default_in_stack = stack_entry
                    .and_then(|st| st.patches.first())
                    .is_some_and(|first| first.eq_ignore_ascii_case(&p.name));
                let (preset, rig_preset, variation, override_modules) = def
                    .patches
                    .iter()
                    .find(|d| d.name.eq_ignore_ascii_case(&p.name))
                    // What the patch plays: a composed patch points at a
                    // preset *snapshot* (`rig_preset` · `snapshot`) and leaves
                    // the legacy pool `preset` empty — reading only the latter
                    // blanked every label once the profile was recomposed.
                    .map(|d| {
                        let points_at = if d.rig_preset.is_empty() {
                            d.preset.clone()
                        } else if d.snapshot.is_empty() {
                            d.rig_preset.clone()
                        } else {
                            format!("{} · {}", d.rig_preset, d.snapshot)
                        };
                        let rig_preset = if d.rig_preset.is_empty() {
                            d.preset.clone()
                        } else {
                            d.rig_preset.clone()
                        };
                        (points_at, rig_preset, d.snapshot.clone(), d.override_modules())
                    })
                    .unwrap_or_default();
                PatchInfo {
                    preset,
                    rig_preset,
                    variation,
                    override_modules,
                    default_in_stack,
                    name: p.name.clone(),
                    stack,
                    available: available(i),
                    active: active == Some(p.name.as_str()),
                }
            })
            .collect()
    }

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
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f32()
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
    /// A capture or IR path as the name a player reads.
    ///
    /// The file stem, without its extension: a NAM capture named
    /// `Fender Deluxe Clean.nam` is "Fender Deluxe Clean" on the panel.
    fn asset_stem(path: &str) -> String {
        std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

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
                    // Except an amp slot with nothing loaded: lit, it reads
                    // as a second amp that is playing.
                    bypassed: block.block_type == BlockType::Amp && block.asset_path().is_empty(),
                    param_name,
                    param_value,
                    param_min,
                    param_max,
                    params,
                    // The tone this block is loaded with, from its own
                    // asset. Blank here meant the amp chunk on the drive
                    // board had no name to show and read as an empty slot —
                    // "the amp is missing" — when the block was there all
                    // along with a capture in it.
                    preset: Self::asset_stem(block.asset_path()),
                    options: Vec::new(),
                    option: 0,
                    output_level_db: None,
                    overridden: false,
                }
            })
            .collect()
    }

    /// Activate a footswitch stack and re-sync everything that activation
    /// resets: the block mirror + bypass defaults, the tapped tempo on the
    /// fresh delays, and the boost gain block.
    /// How stack switch `index` behaves right now.
    fn stack_name(&self, index: usize) -> Option<String> {
        self.profile_def
            .lock_ok()
            .stacks
            .get(index)
            .map(|st| st.name.clone())
    }

    /// Tune stack switch `index`: its rotation (`None` leaves it) and mode —
    /// for the part that is up (`part`), the song, or with no song the
    /// profile's stack itself.
    fn tune_switch_impl(
        &self,
        index: usize,
        patches: Option<Vec<String>>,
        momentary: bool,
        no_rotate: bool,
        part: bool,
    ) {
        let Some(stack) = self.stack_name(index) else {
            return;
        };
        let put = |list: &mut Vec<crate::profiles::StackDefaultDef>| {
            match list.iter_mut().find(|d| d.stack.eq_ignore_ascii_case(&stack)) {
                Some(d) => {
                    d.momentary = momentary;
                    d.no_rotate = no_rotate;
                    if let Some(p) = &patches {
                        d.patches = p.clone();
                    }
                }
                None => list.push(crate::profiles::StackDefaultDef {
                    stack: stack.clone(),
                    patch: String::new(),
                    patches: patches.clone().unwrap_or_default(),
                    momentary,
                    no_rotate,
                }),
            }
            true
        };
        match (self.current_song_name(), part) {
            (Some(_), true) => {
                let Some((part_name, _)) = self.current_part_def() else {
                    tracing::warn!("tune_switch: the song has no part up");
                    return;
                };
                self.edit_current_song("part switch", |song| {
                    put(&mut part_recall_mut(song, &part_name).stack_defaults)
                });
                tracing::info!(part = %part_name, %stack, momentary, no_rotate, ?patches, "switch tuned for the part");
            }
            (Some(song), false) => {
                self.edit_current_song("song switch", |s| put(&mut s.stack_defaults));
                tracing::info!(%song, %stack, momentary, no_rotate, ?patches, "switch tuned for the song");
            }
            (None, _) => {
                let rebuilt = {
                    let mut def = self.profile_def.lock_ok();
                    if let Some(st) = def.stacks.get_mut(index) {
                        st.momentary = momentary;
                        st.no_rotate = no_rotate;
                        if let Some(p) = &patches {
                            st.patches = p.clone();
                        }
                    }
                    // A profile rotation is the stack itself: rebuild.
                    patches.is_some().then(|| {
                        RigLibrary::save_profile(&def);
                        let dps = self.drive_presets.lock_ok();
                        profile_from_library(&def, &dps)
                    })
                };
                self.library_dirty
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                tracing::info!(%stack, momentary, no_rotate, ?patches, "switch tuned for the profile");
                if let Some(rebuilt) = rebuilt {
                    self.reload_rebuilt(rebuilt);
                }
            }
        }
        self.reapply_part_switches();
        self.publish_state();
    }

    /// Re-tune the switches after an edit, and put a stack whose rotation
    /// changed back at its landing patch (a rotation it no longer has no
    /// cursor into).
    fn reapply_part_switches(&self) {
        let tuned = self.apply_song_stacks();
        if let Some(prig) = self.rig.lock_ok().as_mut() {
            for d in tuned.iter().filter(|d| !d.patch.is_empty()) {
                prig.point_stack_at(&d.stack, &d.patch);
            }
        }
    }

    fn switch_mode(&self, index: usize) -> crate::profiles::SwitchMode {
        self.switch_modes.lock_ok().get(index).copied().unwrap_or_default()
    }

    /// The song that is up — only in Setlist mode: in Profile (or Preset)
    /// mode the profile plays as itself, with no song's switch tuning or
    /// patches in play.
    fn current_song_name(&self) -> Option<String> {
        if *self.perform_mode.lock_ok() != PERFORM_SETLIST {
            return None;
        }
        let idx = *self.song_index.lock_ok();
        self.resolved_setlist().get(idx).map(|(name, ..)| name.clone())
    }

    /// The patch playing now.
    fn active_patch_name(&self) -> Option<String> {
        self.rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
    }

    /// Put the song's switch tuning on the rig: the rotations it gives its
    /// switches (in place of the profile's, which come back when it goes),
    /// and every switch's mode — the song's entry for that stack when it has
    /// one, else the profile's stack.
    fn apply_song_stacks(&self) -> Vec<crate::profiles::StackDefaultDef> {
        let song = self.current_song_def();
        let part = self.current_part_def().map(|(_, r)| r);
        let (part_defaults, profile_switches) = part
            .as_ref()
            .map(|p| (p.stack_defaults.clone(), p.profile_switches))
            .unwrap_or_default();
        // The song's tuning (unless the part plays the profile's switches),
        // then the part's entries in place of the song's for those stacks.
        let mut defaults: Vec<crate::profiles::StackDefaultDef> = if profile_switches {
            Vec::new()
        } else {
            song.as_ref().map(|s| s.stack_defaults.clone()).unwrap_or_default()
        };
        for d in &part_defaults {
            defaults.retain(|x| !x.stack.eq_ignore_ascii_case(&d.stack));
            defaults.push(d.clone());
        }
        let (modes, tuned): (Vec<crate::profiles::SwitchMode>, Vec<bool>) = self
            .profile_def
            .lock_ok()
            .stacks
            .iter()
            .map(|st| {
                let mode = defaults
                    .iter()
                    .find(|d| d.stack.eq_ignore_ascii_case(&st.name))
                    .map_or(
                        crate::profiles::SwitchMode {
                            momentary: st.momentary,
                            no_rotate: st.no_rotate,
                        },
                        crate::profiles::StackDefaultDef::mode,
                    );
                let tuned = part_defaults
                    .iter()
                    .any(|d| d.stack.eq_ignore_ascii_case(&st.name));
                (mode, tuned)
            })
            .unzip();
        if let Some(prig) = self.rig.lock_ok().as_mut() {
            prig.restore_stack_rotations();
            for d in defaults.iter().filter(|d| !d.patches.is_empty()) {
                prig.set_stack_rotation(&d.stack, d.patches.clone());
            }
            let no_rotate: Vec<bool> = modes.iter().map(|m| m.no_rotate).collect();
            prig.set_no_rotate(&no_rotate);
        }
        *self.switch_modes.lock_ok() = modes;
        *self.part_tuned.lock_ok() = tuned;
        // The switches' jobs: the defaults, the song's, then the part's.
        let mut jobs = default_switch_actions(song.as_ref().is_some_and(|s| !s.parts.is_empty()));
        let assigned = song
            .iter()
            .flat_map(|s| s.switch_actions.iter())
            .chain(part.iter().flat_map(|p| p.switch_actions.iter()));
        for a in assigned {
            if let Some(slot) = (a.switch as usize).checked_sub(1).and_then(|i| jobs.get_mut(i)) {
                if !a.action.is_empty() {
                    *slot = a.action.clone();
                }
            }
        }
        *self.switch_actions.lock_ok() = jobs;
        defaults
    }

    fn activate_stack_and_sync(&self, index: usize) {
        self.end_audition();
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
    /// Apply a section's overrides to the live chain.
    ///
    /// Through `set_block_param`, which is the same path a knob takes — so a
    /// section change behaves exactly like someone reaching over and turning
    /// the control, including the drive compensation and delay re-timing
    /// that hang off particular parameters. A separate path would be a
    /// second implementation of "what does this parameter mean", and the two
    /// would drift.
    ///
    /// Blocks are matched by name against the live chain. A section naming a
    /// block the current patch does not have is a warning, not a failure:
    /// songs outlive profile edits, and losing the section is better than
    /// losing the song.
    fn apply_section_overrides(&self, overrides: &[crate::profiles::OverrideDef]) {
        if overrides.is_empty() {
            return;
        }
        for ov in overrides {
            let id = {
                let blocks = self.blocks.lock_ok();
                blocks
                    .iter()
                    .find(|b| b.name.eq_ignore_ascii_case(&ov.block))
                    .map(|b| b.id.clone())
            };
            let Some(id) = id else {
                tracing::warn!(
                    block = %ov.block,
                    param = %ov.param,
                    "section override names a block this patch does not have"
                );
                continue;
            };
            match ov.op.as_str() {
                "bypass" => self.set_block_bypass(id, ov.value >= 0.5),
                _ if !ov.param.is_empty() => {
                    self.set_block_param(id, ov.param.clone(), ov.value);
                }
                _ => tracing::warn!(op = %ov.op, "section override has no parameter"),
            }
        }
        tracing::info!(count = overrides.len(), "section overrides applied");
    }

    /// Edit the song that is currently up, save, and publish.
    ///
    /// Every section edit is the same three steps against the same song, and
    /// writing them out per call is how one of them ends up not saving.
    /// `edit` returns whether anything actually changed — a no-op must not
    /// rewrite the library or fire a perf event.
    fn edit_current_song<F>(&self, what: &str, edit: F)
    where
        F: FnOnce(&mut crate::profiles::SongDef) -> bool,
    {
        let song_name = {
            let i = *self.song_index.lock_ok();
            self.resolved_setlist()
                .get(i)
                .map(|(name, ..)| name.clone())
        };
        let Some(song_name) = song_name else {
            tracing::warn!(what, "no song is up");
            return;
        };
        let changed = {
            let mut songs = self.songs_lib.lock_ok();
            let Some(song) = songs
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&song_name))
            else {
                return;
            };
            let changed = edit(song);
            if changed {
                RigLibrary::save_songs(&songs);
            }
            changed
        };
        if changed {
            tracing::info!(song = %song_name, what, "guitar: song edited");
            self.events.publish(RigEvent::Perf(Rig::perf(self)));
        }
    }

    /// `SIGNAL_SWITCH_PROBE=1`: record the rig's input and output for the
    /// 4 s after a switch to `logs/switch-<patch>-<time>-{in,out}.wav`, so a
    /// transition that sounds wrong can be re-rendered offline from the very
    /// input that was played and compared with what the live rig produced.
    /// Diagnostic only.
    fn probe_switch_gain(&self, patch: String) {
        if std::env::var_os("SIGNAL_SWITCH_PROBE").is_none() {
            return;
        }
        let backend = self.clone();
        std::thread::spawn(move || {
            let Some(sr) = backend.rig.lock_ok().as_ref().map(|p| p.sample_rate()) else {
                return;
            };
            let frames = sr as usize * 4;
            if let Some(p) = backend.rig.lock_ok().as_ref() {
                p.rig().arm_di_capture(frames);
                p.rig().arm_output_capture(frames);
            }
            std::thread::sleep(std::time::Duration::from_millis(4300));
            let (inp, (l, r)) = match backend.rig.lock_ok().as_ref() {
                Some(p) => (p.rig().take_di_capture(), p.rig().take_output_capture()),
                None => return,
            };
            let dir = std::env::var("SIGNAL_SWITCH_PROBE_DIR").unwrap_or_else(|_| "/Volumes/dev-drive/logs".into());
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let base = format!("{dir}/switch-{}-{stamp}", patch.replace([' ', '/'], "_"));
            let write = |path: String, channels: u16, frames: &mut dyn Iterator<Item = f32>| {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate: sr,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                if let Ok(mut w) = hound::WavWriter::create(&path, spec) {
                    for s in frames {
                        let _ = w.write_sample(s);
                    }
                    let _ = w.finalize();
                }
            };
            write(format!("{base}-in.wav"), 1, &mut inp.iter().copied());
            write(
                format!("{base}-out.wav"),
                2,
                &mut l.iter().zip(&r).flat_map(|(&a, &b)| [a, b]),
            );
            tracing::info!(%patch, files = %base, frames_in = inp.len(), frames_out = l.len(), "switch probe: recorded");
        });
    }

    fn sync_after_switch(&self, audible: std::time::Duration, via: &str) {
        if let Some(name) = self.live_patch_name() {
            self.probe_switch_gain(name);
        }
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
            switch.total_ms =
                ms(audible) + ms(resync) + ms(tempo) + ms(boost) + ms(drives) + ms(publish),
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
                        s.parts_with_changes()
                            .into_iter()
                            .enumerate()
                            .map(|(pi, (name, patch, overrides))| PerfPart {
                                profile: s
                                    .part_recall(&name)
                                    .map(|r| r.profile.clone())
                                    .unwrap_or_default(),
                                section: s.section_of(pi),
                                switch_count: s.part_recall(&name).map_or(0, |r| {
                                    (r.stack_defaults.len() + r.switch_actions.len()) as u32
                                }),
                                profile_switches: s
                                    .part_recall(&name)
                                    .is_some_and(|r| r.profile_switches),
                                name,
                                patch,
                                overrides: overrides
                                    .iter()
                                    .map(|o| signal_guitar_proto::PartOverride {
                                        block: o.block.clone(),
                                        param: o.param.clone(),
                                        op: o.op.clone(),
                                        value: o.value,
                                    })
                                    .collect(),
                            })
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
        let Some((name, key, bpm, _, parts)) = self.resolved_setlist().get(idx).cloned() else {
            return;
        };
        *self.part_index.lock_ok() = 0;
        *self.tempo.lock_ok() = Some(bpm as f32);
        self.mark_state_dirty();
        let (profile, start_part, defaults) = self
            .songs_lib
            .lock_ok()
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&name))
            .map(|s| {
                (
                    s.profile.clone(),
                    s.start_part.clone(),
                    s.stack_defaults.clone(),
                )
            })
            .unwrap_or_default();
        tracing::info!(
            "setlist → {name} ({key} · {bpm} BPM, profile '{profile}', starts '{start_part}')"
        );

        // The song's profile first: everything after it — the stack
        // tuning, the landing patch, the part — is in that profile's terms.
        self.ensure_profile(&profile);
        self.apply_tempo_to_delays();

        // Song switch tuning: the song's rotations and switch modes, then
        // reset every stack cursor and point the song's overridden stacks at
        // their landing patches — the switches are dialed for the song
        // before anything activates.
        self.apply_song_stacks();
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                prig.reset_stack_positions();
                for d in &defaults {
                    prig.point_stack_at(&d.stack, &d.patch);
                }
            }
        }

        // Where the song starts: its start part (an intro lead, say), which
        // brings its own profile, patch and changes…
        if let Some(pi) = parts
            .iter()
            .position(|p| !start_part.is_empty() && p.name.eq_ignore_ascii_case(&start_part))
        {
            Rig::select_part(self, pi as u32);
            return;
        }
        // …or the profile's default scene — through the song's tuning of
        // that stack, when it has one (Clean → "Clean Verb" for this song).
        let landing = {
            let default = self.default_patch_name();
            let def = self.profile_def.lock_ok();
            def.stacks
                .iter()
                .find(|st| st.patches.iter().any(|p| p.eq_ignore_ascii_case(&default)))
                .and_then(|st| {
                    defaults
                        .iter()
                        .find(|d| d.stack.eq_ignore_ascii_case(&st.name))
                })
                .map_or(default.clone(), |d| {
                    if d.patch.is_empty() {
                        d.patches.first().cloned().unwrap_or(default)
                    } else {
                        d.patch.clone()
                    }
                })
        };
        if self.activate_named(&landing) {
            self.sync_after_switch(std::time::Duration::ZERO, "song");
        }
        self.publish_state();
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
        let cal = mgr.audio.nam_calibration();
        signal_sampler::nam::set_interface_calibration_dbu(cal);
        tracing::info!(nam.calibration_dbu = ?cal, "rig open: NAM level calibration");
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
        // The song that is up tunes the switches.
        self.apply_song_stacks();
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
    /// Where a NAM block's Output Level is stored, for the active patch.
    fn level_home(
        comp: &crate::compose::Compositions,
        patch: Option<&crate::profiles::PatchDef>,
        block: &RigBlock,
    ) -> Option<LevelHome> {
        if block.nam.is_empty() {
            return None;
        }
        match block.block_type {
            BlockType::Drive | BlockType::Boost => Some(LevelHome::Drive {
                nam: block.nam.clone(),
            }),
            BlockType::Amp => {
                let patch = patch?;
                let second = block.name.eq_ignore_ascii_case("Amp R");
                if patch.rig_preset.is_empty() {
                    let pool = if second { &patch.preset2 } else { &patch.preset };
                    return Some(LevelHome::Pool { name: pool.clone() });
                }
                crate::compose::module_picks(comp, patch)
                    .into_iter()
                    .find(|m| m.module.eq_ignore_ascii_case("Amp"))
                    .map(|m| LevelHome::Amp {
                        preset: m.preset,
                        snapshot: m.snapshot,
                        second,
                    })
            }
            _ => None,
        }
    }

    /// The Output Level stored at `home`.
    fn stored_level(
        comp: &crate::compose::Compositions,
        drives: &[crate::profiles::DrivePresetDef],
        pool: &[crate::profiles::PresetDef],
        home: &LevelHome,
    ) -> Option<f32> {
        match home {
            LevelHome::Amp { preset, snapshot, second } => comp
                .module("Amp", preset)?
                .snapshots
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(snapshot))
                .map(|s| if *second { s.level2_db } else { s.level_db }),
            LevelHome::Pool { name } => pool
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map(|p| p.level_db),
            LevelHome::Drive { nam } => Some(crate::nodes::drive_option_level(drives, nam)),
        }
    }

    fn resync_blocks(&self) {
        let comp = RigLibrary::load_compositions();
        // No engine to mirror — build the chain the definition describes.
        if self.rig.lock_ok().is_none() && crate::library::rig_is_design() {
            *self.blocks.lock_ok() = self.design_blocks();
            return;
        }
        let mut out = Vec::new();
        // The active patch's overrides, read once: what the player has moved
        // away from the chain as built.
        // What the Output Levels are read from — taken before the rig lock
        // below (LOCK ORDER: rig before profile_def; never both at once here).
        let (active_def, level_drives, level_pool) = {
            let active = self
                .rig
                .lock_ok()
                .as_ref()
                .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
            let def = self.profile_def.lock_ok();
            let patch = active.and_then(|name| {
                def.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                    .cloned()
            });
            (patch, self.drive_presets.lock_ok().clone(), def.presets.clone())
        };
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
                    // The shared switch-in step — the offline measurement
                    // runs the same one (see `crate::measure`).
                    crate::measure::apply_chain_bypass(prig);
                    for (block, id) in reals.iter().zip(ids.iter()) {
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
                            } else if let Some((label, snaps, idx, has_r)) = (block.block_type
                                == BlockType::Amp)
                                .then(|| {
                                    let own = def
                                        .patches
                                        .iter()
                                        .find(|p| p.name.eq_ignore_ascii_case(&patch.name))?;
                                    let pick = crate::compose::module_picks(&comp, own)
                                        .into_iter()
                                        .find(|m| m.module.eq_ignore_ascii_case("Amp"))?;
                                    let module = comp.module("Amp", &pick.preset)?;
                                    let snaps: Vec<String> =
                                        module.snapshots.iter().map(|s| s.name.clone()).collect();
                                    let idx = snaps
                                        .iter()
                                        .position(|s| s.eq_ignore_ascii_case(&pick.snapshot))
                                        .unwrap_or(0);
                                    let has_r = module
                                        .snapshots
                                        .get(idx)
                                        .is_some_and(|s| !s.nam2.is_empty());
                                    let label = format!(
                                        "{} · {}",
                                        module.name,
                                        snaps.get(idx).cloned().unwrap_or_default()
                                    );
                                    Some((label, snaps, idx as u32, has_r))
                                })
                                .flatten()
                            {
                                // A composed patch's amp is its Amp module
                                // pick: name it that way, and let the chunk
                                // step that amp's snapshots.
                                if name.eq_ignore_ascii_case("Amp R") && !has_r {
                                    <(String, Vec<String>, u32)>::default()
                                } else {
                                    (label, snaps, idx)
                                }
                            } else if block.block_type == BlockType::Amp {
                                // "Amp L" names the patch's first preset;
                                // "Amp R" its second — a second amp is just
                                // another board pedal, its own pool slot.
                                let current = if name.eq_ignore_ascii_case("Amp R") {
                                    def.patches
                                        .iter()
                                        .find(|p| p.name.eq_ignore_ascii_case(&patch.name))
                                        .map(|p| p.preset2.clone())
                                        .unwrap_or_default()
                                } else {
                                    pool_preset_of(&def, &patch.name).unwrap_or_default()
                                };
                                let pool: Vec<String> =
                                    def.presets.iter().map(|p| p.name.clone()).collect();
                                let index =
                                    pool.iter()
                                        .position(|p| p.eq_ignore_ascii_case(&current))
                                        .unwrap_or(0) as u32;
                                (current, pool, index)
                            } else if block.block_type == BlockType::Cabinet {
                                // A cab is named by its IR, so the board says
                                // which cab rather than "Cab L".
                                (Self::asset_stem(&block.ir), Vec::new(), 0)
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
                            output_level_db: Self::level_home(&comp, active_def.as_ref(), block)
                                .and_then(|h| {
                                    Self::stored_level(&comp, &level_drives, &level_pool, &h)
                                }),
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
/// Every profile but `active`, by name.
fn others_of(all: &[ProfileDef], active: &str) -> Vec<ProfileDef> {
    all.iter()
        .filter(|p| !p.name.eq_ignore_ascii_case(active))
        .cloned()
        .collect()
}

/// A name that is not blank and not already taken (case-insensitively).
fn free_name<'a>(name: &str, mut taken: impl Iterator<Item = &'a str>) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && !taken.any(|t| t.eq_ignore_ascii_case(name))).then(|| name.to_string())
}

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
        BlockType::Volume => owned(&[("gain_db", -24.0, 24.0, 0.0), ("pan", -1.0, 1.0, 0.0)]),
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
            // Fully wet, always (`mix` pinned at 1 — see
            // `profiles::pin_parallel_mix`): how loud the delay sits is this.
            ("level", -60.0, 12.0, -16.0),
            ("time", 20.0, 2500.0, 350.0),
            ("feedback", 0.0, 0.95, 0.3),
            ("style", 0.0, 12.0, 1.0),
            ("tap_div_l", 0.0, 10.0, 0.0),
            ("tap_div_r", 0.0, 10.0, 0.0),
            ("high_pass", 0.0, 900.0, 0.0),
            ("repeat_dyn", 0.0, 1.0, 0.0),
            ("pan", -1.0, 1.0, 0.0),
            // The dry guitar through the block; the delay is added in
            // parallel at `level`.
            ("dry", 0.0, 1.0, 1.0),
            // Each repeat darker than the last (in the loop), 20 kHz open.
            ("high_cut", 500.0, 20000.0, 8000.0),
            // Ducking: repeats drop by up to 18 dB while you play.
            ("duck_sens", 0.0, 18.0, 0.0),
            ("duck_release", 0.05, 1.0, 0.2),
            ("mod_rate", 0.05, 8.0, 0.6),
            ("mod_depth", 0.0, 1.0, 0.0),
        ]),
        // Reverb surface — algorithm + mix/time/damping/tone/modulation +
        // wet pan (MX chain-A pan).
        BlockType::Reverb => owned(&[
            // Fully wet, always, as the delay: the reverb's level.
            ("level", -60.0, 12.0, -16.0),
            ("decay", 0.0, 1.0, 0.4),
            ("size", 0.0, 1.0, 0.5),
            ("algorithm", 0.0, 14.0, 1.0),
            ("modulation", 0.0, 1.0, 0.2),
            ("damping", 0.0, 1.0, 0.3),
            ("tone", -1.0, 1.0, 0.0),
            ("pan_a", -1.0, 1.0, 0.0),
            // The dry guitar through the block; the reverb is added in
            // parallel at `level`.
            ("dry", 0.0, 1.0, 1.0),
            ("predelay", 0.0, 200.0, 0.0),
            // The wet's band (20 Hz / 20 kHz open).
            ("low_cut", 20.0, 2000.0, 20.0),
            ("high_cut", 1000.0, 20000.0, 20000.0),
            // Ducking: the tail drops while you play, blooms in the gaps.
            ("duck", 0.0, 1.0, 0.0),
            ("duck_threshold", -60.0, 0.0, -20.0),
            ("duck_release", 20.0, 2000.0, 120.0),
        ]),
        BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => owned(&[
            ("mix", 0.0, 1.0, 0.4),
            ("depth", 0.0, 1.0, 0.5),
            ("rate", 0.05, 10.0, 1.0),
            // `chorus::EngineType` order (persisted, append-only): Cubic,
            // BBD, Tape, Orbit, Juno, CE-2, Dimension, Clone, Tri-Chorus,
            // SCF, Julia.
            ("engine", 0.0, 10.0, 0.0),
            // The engine's own colour — the wet's tone on most (0.5 = the
            // unit as built), pre-delay on the SCF, Lag on the Julia.
            ("color", 0.0, 1.0, 0.5),
            ("feedback", 0.0, 1.0, 0.0),
            // 0 = the engine's mono output, 1 = its full stereo spread.
            ("width", 0.0, 1.0, 1.0),
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

/// `perform_mode` for Setlist mode — the only one in which songs are live.
const PERFORM_SETLIST: u32 = 2;

/// The primary dialable param for a block type: `(name, min, max, default)`.
const fn primary_param(bt: BlockType) -> Option<(&'static str, f32, f32, f32)> {
    match bt {
        // Fully wet in parallel: how much is the level (dB against the dry).
        BlockType::Reverb | BlockType::Delay => Some(("level", -60.0, 12.0, -16.0)),
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
                // The profile's; the service lays the song's over them.
                momentary: false,
                no_rotate: false,
                part_tuned: false,
                patches: st.patches.clone(),
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
                // The profile's; the service lays the song's over them.
                momentary: false,
                no_rotate: false,
                part_tuned: false,
                patches: st.patches.clone(),
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
        song_profile: String::new(),
        start_part: String::new(),
        switch_actions: Vec::new(),
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
            self.events.publish(RigEvent::CompWave(signal_guitar_proto::CompTrace {
                block: crate::profiles::PRE_COMP.to_string(),
                input: wave_in,
                gr: wave_gr,
                gr_db: 0.0,
            }));
            return;
        }
        if let Some(bins) = self.input_spectrum() {
            self.events.publish(RigEvent::Spectrum(bins));
        }
        // One trace per compressor block, each from its own meter channel
        // (`profiles::meter_channel`). A bypassed compressor does not run, so
        // its channel is cleared rather than left frozen at its last reading.
        let comps: Vec<(String, bool)> = self
            .blocks
            .lock_ok()
            .iter()
            .filter(|b| b.block_type == BlockType::Compressor)
            .map(|b| (b.name.clone(), b.bypassed))
            .collect();
        for (name, bypassed) in comps {
            let ch = crate::profiles::meter_channel(&name);
            if ch == 0 {
                continue;
            }
            if bypassed {
                fx_blocks::comp_meter::clear(ch);
            }
            let (input, gr) = fx_blocks::comp_meter::wave_snapshot_of(ch, 3);
            self.events.publish(RigEvent::CompWave(signal_guitar_proto::CompTrace {
                block: name,
                input,
                gr,
                gr_db: fx_blocks::comp_meter::gr_db_of(ch),
            }));
        }
    }
}

impl Rig for GuitarRigBackend {
    fn start(&self) {
        self.wants_audio
            .store(true, std::sync::atomic::Ordering::Relaxed);
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
        self.wants_audio
            .store(false, std::sync::atomic::Ordering::Relaxed);
        *self.rig.lock_ok() = None;
        *self.open_prefs.lock_ok() = None;
        tracing::info!("rig stopped");
        self.publish_state();
    }

    fn status(&self) -> RigStatus {
        let mut status = self.raw_status();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
        status.perf.cpu = self.cpu.lock_ok().sample(cores);
        status.perf.cores = cores;
        status
    }

    fn perf(&self) -> PerformanceModel {
        let mut m = {
            // Live model when the audio rig is open; otherwise the static
            // model from the profile def, so the footswitch stacks still
            // render before the device opens (iOS with no interface yet).
            // Both are needed at once, so the rig is taken first (`LOCK
            // ORDER` on `rig`).
            let live = self.design_patch.lock_ok().clone();
            let rig = self.rig.lock_ok();
            let def = self.profile_def.lock_ok();
            rig.as_ref().map_or_else(
                || build_perf_model_static(&def, &live),
                |prig| build_perf_model(prig, &def),
            )
        };
        m.boost_db = self.current_boost_db();
        m.tempo_bpm = self.tempo_bpm().round() as u32;
        {
            let modes = self.switch_modes.lock_ok();
            for (st, mode) in m.stacks.iter_mut().zip(modes.iter()) {
                st.momentary = mode.momentary;
                st.no_rotate = mode.no_rotate;
            }
            let tuned = self.part_tuned.lock_ok();
            for (st, t) in m.stacks.iter_mut().zip(tuned.iter()) {
                st.part_tuned = *t;
            }
            m.switch_actions = self.switch_actions.lock_ok().clone();
        }
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
        if let Some(song) = m.songs.get(m.song_index as usize) {
            if let Some(def) = self
                .songs_lib
                .lock_ok()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&song.name))
            {
                m.song_profile = def.profile.clone();
                m.start_part = def.start_part.clone();
            }
        }
        m.headphone = self.headphone.lock_ok().clone();
        m.master_trim_db = *self.master_trim.lock_ok();
        m.revision = *self.revision.lock_ok();
        m
    }

    fn chain(&self) -> Vec<LiveBlock> {
        self.blocks.lock_ok().clone()
    }

    fn nodes(&self) -> Vec<LiveNode> {
        // The tree as the *active patch* resolves it, so what the UI lists is
        // what is playing rather than the chain's unbent default. Asked of
        // the rig first and released: see `LOCK ORDER` on `rig`.
        let active = self
            .rig
            .lock_ok()
            .as_ref()
            .and_then(|prig| prig.active_patch().map(|p| p.name.clone()));
        let def = self.profile_def.lock_ok();
        let dps = self.drive_presets.lock_ok();
        let rig = crate::nodes::library_for(&def, &dps);
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
            // Keep whatever the section already overrides: this call sets
            // the PATCH a section recalls, and clearing it should not throw
            // away the parameter changes that are the section's real
            // content.
            let (kept, profile) = song
                .part_recalls
                .iter()
                .find(|r| r.part.eq_ignore_ascii_case(&part))
                .map(|r| (r.overrides.clone(), r.profile.clone()))
                .unwrap_or_default();
            song.part_recalls
                .retain(|r| !r.part.eq_ignore_ascii_case(&part));
            if !patch.is_empty() || !kept.is_empty() || !profile.is_empty() {
                song.part_recalls.push(crate::profiles::PartRecallDef {
                    profile,
                    part: part.clone(),
                    patch: patch.clone(),
                    overrides: kept,
                    ..Default::default()
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

    fn add_part(&self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.edit_current_song("section added", |song| song.add_part(&name));
    }

    fn rename_part(&self, old: String, new_name: String) {
        self.edit_current_song("section renamed", |song| song.rename_part(&old, &new_name));
    }

    fn remove_part(&self, name: String) {
        self.edit_current_song("section removed", |song| song.remove_part(&name));
        // The cursor may now be past the end.
        let last = self
            .resolved_setlist()
            .get(*self.song_index.lock_ok())
            .map_or(0, |(_, _, _, _, parts)| parts.len().saturating_sub(1));
        let mut idx = self.part_index.lock_ok();
        if *idx > last {
            *idx = last;
        }
    }

    fn move_part(&self, from: u32, to: u32) {
        self.edit_current_song("section moved", |song| {
            song.move_part(from as usize, to as usize)
        });
    }

    fn set_part_overrides(&self, part: String, overrides: Vec<signal_guitar_proto::PartOverride>) {
        let song_name = {
            let i = *self.song_index.lock_ok();
            self.resolved_setlist()
                .get(i)
                .map(|(name, ..)| name.clone())
        };
        let Some(song_name) = song_name else {
            tracing::warn!(%part, "no song is up — nothing to set a section on");
            return;
        };
        {
            let mut songs = self.songs_lib.lock_ok();
            let Some(song) = songs
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&song_name))
            else {
                return;
            };
            let defs: Vec<crate::profiles::OverrideDef> = overrides
                .iter()
                .map(|o| crate::profiles::OverrideDef {
                    module: String::new(),
                    block: o.block.clone(),
                    param: o.param.clone(),
                    op: if o.op.is_empty() {
                        "set".to_string()
                    } else {
                        o.op.clone()
                    },
                    value: o.value,
                    text: String::new(),
                })
                .collect();

            // Keep the patch this section already recalls: this call sets
            // what it CHANGES, and the two are independent halves of the
            // same section.
            let (patch, profile) = song
                .part_recalls
                .iter()
                .find(|r| r.part.eq_ignore_ascii_case(&part))
                .map(|r| (r.patch.clone(), r.profile.clone()))
                .unwrap_or_default();
            song.part_recalls
                .retain(|r| !r.part.eq_ignore_ascii_case(&part));
            if !patch.is_empty() || !defs.is_empty() || !profile.is_empty() {
                song.part_recalls.push(crate::profiles::PartRecallDef {
                    profile,
                    part: part.clone(),
                    patch,
                    overrides: defs,
                    ..Default::default()
                });
            }
            tracing::info!(
                song = %song_name,
                %part,
                count = overrides.len(),
                "guitar: section overrides set"
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
        if self.switch_mode(index as usize).momentary {
            // Remember where we are — once: a second momentary pressed
            // while one is held still returns to where the first began.
            {
                let mut ret = self.momentary_return.lock_ok();
                if ret.is_none() {
                    *ret = self.active_patch_name();
                }
            }
        }
        self.activate_stack_and_sync(index as usize);
        self.follow_part();
    }

    fn release_stack(&self, index: u32) {
        if !self.switch_mode(index as usize).momentary {
            return;
        }
        let Some(back) = self.momentary_return.lock_ok().take() else {
            return;
        };
        if self.activate_named(&back) {
            self.sync_after_switch(std::time::Duration::ZERO, "momentary");
        }
        self.publish_state();
    }

    fn set_stack_mode(&self, index: u32, momentary: bool, no_rotate: bool) {
        self.tune_switch_impl(index as usize, None, momentary, no_rotate, false);
    }

    fn tune_switch(&self, t: signal_guitar_proto::SwitchTuning) {
        let patches = (!t.patches.is_empty()).then_some(t.patches);
        self.tune_switch_impl(t.index as usize, patches, t.momentary, t.no_rotate, t.part);
    }

    fn reset_switch(&self, index: u32, part: bool) {
        let Some(stack) = self.stack_name(index as usize) else {
            return;
        };
        if part {
            let Some((part_name, _)) = self.current_part_def() else {
                return;
            };
            self.edit_current_song("part switch reset", |song| {
                let Some(r) = song
                    .part_recalls
                    .iter_mut()
                    .find(|r| r.part.eq_ignore_ascii_case(&part_name))
                else {
                    return false;
                };
                let before = r.stack_defaults.len();
                r.stack_defaults.retain(|d| !d.stack.eq_ignore_ascii_case(&stack));
                r.stack_defaults.len() != before
            });
        } else if self.current_song_name().is_some() {
            self.edit_current_song("song switch reset", |song| {
                let before = song.stack_defaults.len();
                song.stack_defaults.retain(|d| !d.stack.eq_ignore_ascii_case(&stack));
                song.stack_defaults.len() != before
            });
        } else {
            return;
        }
        self.reapply_part_switches();
        self.publish_state();
    }

    fn set_switch_action(&self, switch: u32, action: String, part: bool) {
        let action = action.trim().to_string();
        if !action.is_empty()
            && !crate::profiles::SWITCH_ACTIONS.iter().any(|(k, _)| *k == action)
        {
            tracing::warn!(%action, "set_switch_action: no such action");
            return;
        }
        let sw = switch + 1;
        let put = |list: &mut Vec<crate::profiles::SwitchActionDef>| {
            list.retain(|a| a.switch != sw);
            if !action.is_empty() {
                list.push(crate::profiles::SwitchActionDef {
                    switch: sw,
                    action: action.clone(),
                });
            }
            true
        };
        if self.current_song_name().is_none() {
            tracing::warn!("set_switch_action: switch jobs are per song — Setlist mode only");
            return;
        }
        if part {
            let Some((part_name, _)) = self.current_part_def() else {
                return;
            };
            self.edit_current_song("part switch action", |song| {
                put(&mut part_recall_mut(song, &part_name).switch_actions)
            });
        } else {
            self.edit_current_song("song switch action", |song| put(&mut song.switch_actions));
        }
        tracing::info!(switch = sw, %action, part, "switch action set");
        self.apply_song_stacks();
        self.publish_state();
    }

    fn tap_switch(&self, switch: u32) {
        self.switch_tap(switch as usize);
    }

    fn hold_switch(&self, switch: u32) {
        self.switch_hold(switch as usize);
    }

    fn step_part(&self, dir: i32, sections: bool) {
        self.step_part_impl(dir, sections);
    }

    fn set_part_section(&self, part: String, section: String) {
        let section = section.trim().to_string();
        self.edit_current_song("part section", |song| {
            if !song.parts.iter().any(|p| p.eq_ignore_ascii_case(&part)) {
                return false;
            }
            part_recall_mut(song, &part).section = section.clone();
            true
        });
    }

    fn set_part_profile_switches(&self, part: String, on: bool) {
        self.edit_current_song("part profile switches", |song| {
            if !song.parts.iter().any(|p| p.eq_ignore_ascii_case(&part)) {
                return false;
            }
            part_recall_mut(song, &part).profile_switches = on;
            true
        });
        self.reapply_part_switches();
        self.publish_state();
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
        let section = self
            .resolved_setlist()
            .get(song_idx)
            .and_then(|(_, _, _, _, parts)| parts.get(idx).cloned());
        // What this section changes on top of its patch, from the library
        // rather than the perf model — the model carries what a remote needs
        // to DISPLAY, and these are what the rig has to APPLY.
        let overrides = {
            let song_name = self
                .resolved_setlist()
                .get(song_idx)
                .map(|(name, ..)| name.clone())
                .unwrap_or_default();
            match (&section, song_name.is_empty()) {
                (Some(part), false) => {
                    let songs = self.songs_lib.lock_ok();
                    songs
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(&song_name))
                        .and_then(|s| {
                            s.part_recalls
                                .iter()
                                .find(|r| r.part.eq_ignore_ascii_case(&part.name))
                                .map(|r| r.overrides.clone())
                        })
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            }
        };

        // A part is a base profile first — its own, else the song's — then a
        // patch in it, then changes on top.
        let song_profile = {
            let song_name = self
                .resolved_setlist()
                .get(song_idx)
                .map(|(name, ..)| name.clone())
                .unwrap_or_default();
            self.songs_lib
                .lock_ok()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&song_name))
                .map(|s| s.profile.clone())
                .unwrap_or_default()
        };
        let want_profile = section
            .as_ref()
            .map(|p| p.profile.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or(song_profile);
        let switched_profile = self.ensure_profile(&want_profile);
        // The part's switch tuning, and every stack back at its landing patch
        // — the switches are dialed for the part before its patch plays.
        let tuned = self.apply_song_stacks();
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                prig.reset_stack_positions();
                for d in tuned.iter().filter(|d| !d.patch.is_empty()) {
                    prig.point_stack_at(&d.stack, &d.patch);
                }
            }
        }

        let recall = section.filter(|part| !part.patch.is_empty());
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
        } else if !overrides.is_empty() {
            // A section with overrides and no patch of its own still needs a
            // baseline, or it inherits whatever the previous section left
            // behind. Re-establishing the current patch is what makes a
            // section the same sound every time it comes round.
            let current = {
                let guard = self.rig.lock_ok();
                guard
                    .as_ref()
                    .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
                    .unwrap_or_else(|| self.design_patch.lock_ok().clone())
            };
            if !current.is_empty() {
                let reset = {
                    let mut guard = self.rig.lock_ok();
                    match guard.as_mut() {
                        Some(prig) => activate_patch_by_name(prig, &current),
                        None => true,
                    }
                };
                if reset {
                    self.sync_after_switch(std::time::Duration::ZERO, "section");
                }
            }
        } else if switched_profile {
            // A part that only changes the profile lands on its default.
            tracing::info!("part → {idx} (song {song_idx}) on '{want_profile}'");
        } else {
            tracing::info!("part → {idx} (song {song_idx})");
        }

        self.apply_section_overrides(&overrides);
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
            drop(guard);
            // Design mode builds no engine, but the profile is plain data:
            // list its patches exactly as the live rig would (same order,
            // same stacks), so the sidebar shows the real profile.
            if !crate::library::rig_is_design() {
                return Vec::new();
            }
            let profile = self.design_profile();
            let active = self.design_active_patch(&profile);
            return self.patch_infos(&profile.patches, &profile.stacks, active.as_deref(), |_| {
                true
            });
        };
        let active = prig.active_patch().map(|p| p.name.clone());
        self.patch_infos(prig.patches(), prig.stacks(), active.as_deref(), |i| {
            prig.is_patch_available(i)
        })
    }

    fn select_patch(&self, index: u32) {
        self.end_audition();
        {
            let mut guard = self.rig.lock_ok();
            if let Some(prig) = guard.as_mut() {
                if let Some(name) = prig.patches().get(index as usize).map(|p| p.name.clone()) {
                    activate_patch_by_name(prig, &name);
                }
            } else if crate::library::rig_is_design() {
                drop(guard);
                // Same indices as `patches()` above; `resync_blocks` below
                // rebuilds the design chain from `design_patch`.
                if let Some(p) = self.design_profile().patches.get(index as usize) {
                    *self.design_patch.lock_ok() = p.name.clone();
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
        let design_active = crate::library::rig_is_design() && self.rig.lock_ok().is_none();
        let active_patch = if design_active {
            let profile = self.design_profile();
            self.design_active_patch(&profile)
        } else {
            self.rig
                .lock_ok()
                .as_ref()
                .and_then(|p| p.active_patch().map(|p| p.name.clone()))
        };
        let def = self.profile_def.lock_ok();
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
                    cab: preset.cab.clone(),
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
        if self.levelling_busy.swap(true, Ordering::SeqCst) {
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
            // Choosing a capture directly replaces the patch's Amp preset
            // pick — otherwise the pick would keep winning and this would
            // silently do nothing.
            p.modules.retain(|m| !m.module.eq_ignore_ascii_case("Amp"));
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

    /// Load a second amp (Amp R) into the patch, blended in parallel with the
    /// first — independently bypassable. See
    /// [`set_patch_preset`](Self::set_patch_preset).
    fn set_patch_preset2(&self, patch: u32, preset: u32) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(preset_name) = def.presets.get(preset as usize).map(|p| p.name.clone()) else {
                return;
            };
            let Some(p) = def.patches.get_mut(patch as usize) else {
                return;
            };
            tracing::info!("patch '{}' → second amp '{preset_name}'", p.name);
            p.preset2 = preset_name;
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
    }

    /// Unload Amp R — the slot goes back to an empty, bypassed passthrough.
    fn clear_patch_preset2(&self, patch: u32) {
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.patches.get_mut(patch as usize) else {
                return;
            };
            tracing::info!("patch '{}' → second amp cleared", p.name);
            p.preset2.clear();
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
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
                let Some(active) = self
                    .rig
                    .lock_ok()
                    .as_ref()
                    .and_then(|prig| prig.active_patch().map(|p| p.name.clone()))
                else {
                    return;
                };
                let def = self.profile_def.lock_ok();
                def.patches
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(&active))
            };
            // A composed patch: the chunk's options are its Amp module's
            // snapshots, so choosing one is a module pick.
            let comp = RigLibrary::load_compositions();
            if let Some(pick) = self.live_pick(&comp, "Amp") {
                if let Some(snap) = comp
                    .module("Amp", &pick.preset)
                    .and_then(|m| m.snapshots.get(option as usize))
                    .map(|s| s.name.clone())
                {
                    self.choose_module("Amp".into(), pick.preset, snap);
                    return;
                }
            }
            if let Some(patch) = patch {
                if block_name.eq_ignore_ascii_case("Amp R") {
                    self.set_patch_preset2(patch as u32, option);
                } else {
                    self.set_patch_preset(patch as u32, option);
                }
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
                cab: String::new(),
                cab_hash: String::new(),
                level_db: 0.0,
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
                momentary: false,
                no_rotate: false,
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
                song: String::new(),
                name: name.clone(),
                preset,
                preset2: String::new(),
                rig_preset: String::new(),
                snapshot: String::new(),
                modules: Vec::new(),
                blocks: Vec::new(),
                drives: Vec::new(),
                trim_db: 0.0,
                level_db: 0.0,
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
                    level_db: 0.0,
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
        // Keep playing the profile that was up, if it is still there — the
        // file on disk names the active one only as of the last flush.
        let current = self.profile_def.lock_ok().name.clone();
        let profile = lib
            .profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&current))
            .cloned()
            .unwrap_or(lib.profile);
        *self.other_profiles.lock_ok() = others_of(&lib.profiles, &profile.name);
        *self.profile_def.lock_ok() = profile;
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
                patches: Vec::new(),
                profile: String::new(),
                start_part: String::new(),
                name: name.clone(),
                key: if key.is_empty() { "C".to_string() } else { key },
                bpm: if bpm == 0 { 120 } else { bpm },
                stack: 0,
                parts: Vec::new(),
                stack_defaults: Vec::new(),
                part_recalls: Vec::new(),
                switch_actions: Vec::new(),
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

    fn set_preset_cab(&self, index: u32, ir_path: String) {
        if !ir_path.is_empty() && !std::path::Path::new(&ir_path).exists() {
            tracing::warn!("set_preset_cab: {ir_path} does not exist");
            return;
        }
        let rebuilt = {
            let mut def = self.profile_def.lock_ok();
            let Some(p) = def.presets.get_mut(index as usize) else {
                return;
            };
            tracing::info!("preset '{}' cab → {ir_path}", p.name);
            p.cab_hash = if ir_path.is_empty() {
                String::new()
            } else {
                capture_hash(&ir_path)
            };
            p.cab = ir_path;
            RigLibrary::save_profile(&def);
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
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
        self.mark_state_dirty();
        tracing::info!(
            "perform mode → {}",
            ["preset", "profile", "setlist"][mode.min(2) as usize]
        );
        // Songs are live only in Setlist mode: entering it tunes the
        // switches for the song that is up, leaving it gives the profile its
        // own back — and a song's patch playing then gives way to the
        // profile's default.
        self.apply_song_stacks();
        if mode.min(2) != PERFORM_SETLIST {
            let on_song_patch = self.active_patch_name().is_some_and(|name| {
                self.profile_def
                    .lock_ok()
                    .patches
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(&name) && !p.song.is_empty())
            });
            if on_song_patch {
                let default = self.default_patch_name();
                if self.activate_named(&default) {
                    self.sync_after_switch(std::time::Duration::ZERO, "mode");
                }
            }
        }
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
                    // Both measurements were taken against the old DI: the
                    // drive curves first (levelling hears the trims they
                    // set), then every patch through its whole chain. The
                    // drive curves alone left the patches levelled for
                    // someone else's guitar — a crunch that breaks up with
                    // the player's pickups came out louder than a drive that
                    // was already saturated.
                    tracing::info!("DI captured — re-measuring the library");
                    backend.run_drive_calibration();
                    backend.level_patches();
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

    fn compositions(&self) -> signal_guitar_proto::CompositionModel {
        use signal_guitar_proto::{
            CompositionModel, ModulePick, ModulePresetEntry, PresetEntry, PresetSnapshotEntry,
        };
        let comp = RigLibrary::load_compositions();
        let pick = |c: &crate::profiles::ModuleChoiceDef| ModulePick {
            module: c.module.clone(),
            preset: c.preset.clone(),
            snapshot: c.snapshot.clone(),
        };
        // An audition is what is playing, so it is what the preset tab marks.
        let auditioning = self
            .audition
            .lock_ok()
            .clone()
            .filter(|_| self.live_patch_name().as_deref() == Some(AUDITION_PATCH));
        let active_blocks: Vec<signal_guitar_proto::BlockPick> = self
            .live_patch_name()
            .and_then(|name| {
                let def = self.profile_def.lock_ok();
                def.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                    .map(|p| crate::compose::block_picks(&comp, p))
            })
            .unwrap_or_default()
            .into_iter()
            .map(|c| signal_guitar_proto::BlockPick { block: c.block, preset: c.preset })
            .collect();
        let (active_preset, active_snapshot, active_modules) = auditioning
            .map(|(preset, snapshot)| (preset, snapshot, Vec::new()))
            .or_else(|| self.live_patch_name().and_then(|name| {
                let def = self.profile_def.lock_ok();
                def.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                    .map(|p| {
                        let picks = crate::compose::module_picks(&comp, p);
                        (
                            p.rig_preset.clone(),
                            p.snapshot.clone(),
                            picks.iter().map(pick).collect(),
                        )
                    })
            }))
            .unwrap_or_default();
        CompositionModel {
            modules: comp
                .modules
                .iter()
                .map(|m| ModulePresetEntry {
                    module: m.module.clone(),
                    name: m.name.clone(),
                    snapshots: m.snapshots.iter().map(|s| s.name.clone()).collect(),
                })
                .collect(),
            presets: comp
                .presets
                .iter()
                .map(|p| PresetEntry {
                    name: p.name.clone(),
                    snapshots: p
                        .snapshots
                        .iter()
                        .map(|s| PresetSnapshotEntry {
                            name: s.name.clone(),
                            modules: s.modules.iter().map(pick).collect(),
                            overrides: s.overrides.len() as u32,
                        })
                        .collect(),
                })
                .collect(),
            active_preset,
            active_snapshot,
            active_modules,
            block_presets: comp
                .blocks
                .iter()
                .map(|b| signal_guitar_proto::BlockPresetEntry {
                    block_type: b.block_type.clone(),
                    name: b.name.clone(),
                    bypass: b.bypass,
                })
                .collect(),
            active_blocks,
        }
    }

    fn choose_block(&self, block: String, preset: String) {
        let comp = RigLibrary::load_compositions();
        let Some(found) = comp.block_preset(&preset) else {
            tracing::warn!(%block, %preset, "choose_block: no such block preset");
            return;
        };
        let choice = crate::compose::BlockChoiceDef {
            block,
            preset: found.name.clone(),
        };
        self.edit_live_patch(move |patch| {
            // Loading a preset onto a block replaces what was dialled on it
            // by hand: the patch's own overrides of that block go, or they
            // would keep winning over the preset just picked (an old
            // tap-division edit kept a "Dotted Eighth" playing eighths).
            patch
                .overrides
                .retain(|o| !o.block.eq_ignore_ascii_case(&choice.block));
            match patch
                .blocks
                .iter_mut()
                .find(|b| b.block.eq_ignore_ascii_case(&choice.block))
            {
                Some(b) => *b = choice,
                None => patch.blocks.push(choice),
            }
        });
    }

    fn choose_module(&self, module: String, preset: String, snapshot: String) {
        let comp = RigLibrary::load_compositions();
        let Some(found) = comp.module(&module, &preset) else {
            tracing::warn!(%module, %preset, "choose_module: no such module preset");
            return;
        };
        let snapshot = if snapshot.is_empty() {
            found
                .snapshots
                .first()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        } else {
            snapshot
        };
        // The modules this snapshot plays (a Time snapshot's Delay and
        // Reverb): the patch's own picks of those go, so the pick takes.
        let subs: Vec<String> = found
            .snapshots
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&snapshot))
            .map(|s| s.modules.iter().map(|m| m.module.clone()).collect())
            .unwrap_or_default();
        // Every block it sets (through the modules it references too): the
        // patch's own block presets and hand edits on those blocks go, or
        // they would keep winning over the module just picked — a delay's
        // old time edit kept playing through every Time preset.
        let blocks = crate::compose::blocks_set_by(&comp, &found.module, &found.name, &snapshot);
        let choice = crate::profiles::ModuleChoiceDef {
            module: found.module.clone(),
            preset: found.name.clone(),
            snapshot,
        };
        self.edit_live_patch(move |patch| {
            let set_here = |b: &str| blocks.iter().any(|x| x.eq_ignore_ascii_case(b));
            patch.overrides.retain(|o| !set_here(&o.block));
            patch.blocks.retain(|c| !set_here(&c.block));
            patch
                .modules
                .retain(|m| !subs.iter().any(|s| s.eq_ignore_ascii_case(&m.module)));
            match patch
                .modules
                .iter_mut()
                .find(|m| m.module.eq_ignore_ascii_case(&choice.module))
            {
                Some(m) => *m = choice,
                None => patch.modules.push(choice),
            }
        });
    }

    fn step_module(&self, module: String, delta: i32) {
        let comp = RigLibrary::load_compositions();
        let current = self
            .live_pick(&comp, &module)
            .and_then(|c| comp.module(&c.module, &c.preset).map(|m| (m, c)));
        let (preset, snapshot) = match current {
            Some((m, c)) => {
                let n = m.snapshots.len().max(1) as i32;
                let at = m
                    .snapshots
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case(&c.snapshot))
                    .unwrap_or(0) as i32;
                let next = (at + delta).rem_euclid(n) as usize;
                (
                    m.name.clone(),
                    m.snapshots
                        .get(next)
                        .map(|s| s.name.clone())
                        .unwrap_or_default(),
                )
            }
            None => match comp.modules_of(&module).next() {
                Some(m) => (m.name.clone(), String::new()),
                None => return,
            },
        };
        self.choose_module(module, preset, snapshot);
    }

    fn choose_preset(&self, preset: String, snapshot: String) {
        let comp = RigLibrary::load_compositions();
        let Some(found) = comp.preset(&preset) else {
            tracing::warn!(%preset, "choose_preset: no such preset");
            return;
        };
        let name = found.name.clone();
        let snapshot = if snapshot.is_empty() {
            found
                .snapshots
                .first()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        } else {
            snapshot
        };
        // An audition: the snapshot plays on its own, exactly as it was
        // levelled (`compose::snapshot_patch`), and the profile is not touched
        // — it used to repoint and *save* the live patch on every click, so
        // browsing presets rewrote the profile. Putting a preset in a patch
        // is the library's job.
        *self.audition.lock_ok() = Some((name, snapshot));
        let rebuilt = self.profile_with_audition();
        self.reload_rebuilt_activating(rebuilt, Some(AUDITION_PATCH));
    }

    fn step_preset_snapshot(&self, delta: i32) {
        let comp = RigLibrary::load_compositions();
        // Stepping from an audition steps the audition.
        let auditioning = self.audition.lock_ok().clone();
        let Some((preset, snapshot)) = auditioning.or_else(|| {
            self.live_patch_name().and_then(|name| {
                let def = self.profile_def.lock_ok();
                def.patches
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name))
                    .map(|p| (p.rig_preset.clone(), p.snapshot.clone()))
            })
        }) else {
            return;
        };
        let Some(found) = comp.preset(&preset) else {
            return;
        };
        let n = found.snapshots.len().max(1) as i32;
        let at = found
            .snapshots
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(&snapshot))
            .unwrap_or(0) as i32;
        let next = (at + delta).rem_euclid(n) as usize;
        if let Some(s) = found.snapshots.get(next) {
            self.choose_preset(found.name.clone(), s.name.clone());
        }
    }

    fn library(&self) -> signal_guitar_proto::LibraryModel {
        use signal_guitar_proto::{
            DriveEntry, LibraryModel, ProfileEntry, SetlistEntry, SongEntry, SongSlot,
        };
        let entry = |p: &ProfileDef, active: bool| ProfileEntry {
            name: p.name.clone(),
            active,
            stacks: p.stacks.iter().map(|s| s.name.clone()).collect(),
            patches: p.patches.len() as u32,
            presets: p.presets.iter().map(|p| p.name.clone()).collect(),
            patch_list: p
                .patches
                .iter()
                .map(|patch| signal_guitar_proto::ProfilePatch {
                    name: patch.name.clone(),
                    stack: p
                        .stacks
                        .iter()
                        .find(|st| {
                            st.patches
                                .iter()
                                .any(|n| n.eq_ignore_ascii_case(&patch.name))
                        })
                        .map(|st| st.name.clone())
                        .unwrap_or_default(),
                })
                .collect(),
            default_patch: p.default_patch.clone(),
        };
        let (mut profiles, slots) = {
            let active = self.profile_def.lock_ok();
            let mut profiles = vec![entry(&active, true)];
            profiles.extend(
                self.other_profiles
                    .lock_ok()
                    .iter()
                    .map(|p| entry(p, false)),
            );
            let slots: Vec<(String, String)> = active
                .drives
                .iter()
                .map(|d| (d.preset.clone(), d.block.clone()))
                .collect();
            (profiles, slots)
        };
        profiles.sort_by_key(|p| p.name.to_lowercase());

        let songs_lib = self.songs_lib.lock_ok().clone();
        let sets = self.setlists.lock_ok().clone();
        let active_set = *self.setlist_index.lock_ok();
        let songs = songs_lib
            .iter()
            .map(|s| SongEntry {
                name: s.name.clone(),
                key: s.key.clone(),
                bpm: s.bpm,
                parts: s.parts.clone(),
                setlists: sets
                    .iter()
                    .filter(|set| {
                        set.entries
                            .iter()
                            .any(|e| e.song.eq_ignore_ascii_case(&s.name))
                    })
                    .map(|set| set.name.clone())
                    .collect(),
                profile: s.profile.clone(),
                start_part: s.start_part.clone(),
            })
            .collect();
        let setlists = sets
            .iter()
            .enumerate()
            .map(|(i, set)| SetlistEntry {
                name: set.name.clone(),
                active: i == active_set,
                songs: set
                    .entries
                    .iter()
                    .map(|e| {
                        let song = songs_lib
                            .iter()
                            .find(|s| s.name.eq_ignore_ascii_case(&e.song));
                        SongSlot {
                            name: e.song.clone(),
                            key: if e.key.is_empty() {
                                song.map(|s| s.key.clone()).unwrap_or_default()
                            } else {
                                e.key.clone()
                            },
                            bpm: if e.bpm == 0 {
                                song.map_or(0, |s| s.bpm)
                            } else {
                                e.bpm
                            },
                        }
                    })
                    .collect(),
            })
            .collect();
        let drives = self
            .drive_presets
            .lock_ok()
            .iter()
            .map(|d| DriveEntry {
                name: d.name.clone(),
                options: d.options.iter().map(|o| o.name.clone()).collect(),
                slots: slots
                    .iter()
                    .filter(|(preset, _)| preset.eq_ignore_ascii_case(&d.name))
                    .map(|(_, block)| block.clone())
                    .collect(),
            })
            .collect();
        LibraryModel {
            profiles,
            songs,
            setlists,
            drives,
        }
    }

    fn select_profile(&self, name: String) {
        if self.profile_def.lock_ok().name.eq_ignore_ascii_case(&name) {
            return;
        }
        let def = self
            .other_profiles
            .lock_ok()
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
            .cloned();
        match def {
            Some(def) => self.switch_profile(def),
            None => tracing::warn!("select_profile: no profile named '{name}'"),
        }
    }

    fn add_profile(&self, name: String, from: String) {
        let Some(name) = free_name(&name, self.profile_names().iter().map(String::as_str)) else {
            tracing::warn!("add_profile: '{name}' is blank or taken");
            return;
        };
        let mut def = if from.trim().is_empty() {
            // A starter that plays: the amps and pedals already on hand, one
            // stack holding one patch on the first amp.
            let active = self.profile_def.lock_ok();
            let first = active
                .presets
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_default();
            ProfileDef {
                default_patch: String::new(),
                name: String::new(),
                drives: active.drives.clone(),
                presets: active.presets.clone(),
                patches: vec![crate::profiles::PatchDef {
                    song: String::new(),
                    name: "Clean".to_string(),
                    preset: first,
                    preset2: String::new(),
                    rig_preset: String::new(),
                    snapshot: String::new(),
                    modules: Vec::new(),
                    blocks: Vec::new(),
                    drives: Vec::new(),
                    trim_db: 0.0,
                    level_db: 0.0,
                    boost_db: 0.0,
                    overrides: Vec::new(),
                }],
                stacks: vec![crate::profiles::StackDef {
                    name: "Clean".to_string(),
                    patches: vec!["Clean".to_string()],
                    momentary: false,
                    no_rotate: false,
                }],
            }
        } else {
            let active = self.profile_def.lock_ok();
            if active.name.eq_ignore_ascii_case(&from) {
                active.clone()
            } else if let Some(p) = self
                .other_profiles
                .lock_ok()
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&from))
            {
                p.clone()
            } else {
                tracing::warn!("add_profile: no profile named '{from}' to copy");
                return;
            }
        };
        def.name = name.clone();
        RigLibrary::save_profile(&def);
        {
            let mut others = self.other_profiles.lock_ok();
            others.push(def);
            others.sort_by_key(|p| p.name.to_lowercase());
        }
        tracing::info!("profile added: {name}");
        self.publish_state();
    }

    fn rename_profile(&self, old: String, new_name: String) {
        let taken: Vec<String> = self
            .profile_names()
            .into_iter()
            .filter(|n| !n.eq_ignore_ascii_case(&old))
            .collect();
        let Some(new_name) = free_name(&new_name, taken.iter().map(String::as_str)) else {
            tracing::warn!("rename_profile: '{new_name}' is blank or taken");
            return;
        };
        let rename = |def: &mut ProfileDef| {
            RigLibrary::delete_profile(&def.name);
            def.name = new_name.clone();
            RigLibrary::save_profile(def);
        };
        {
            let mut active = self.profile_def.lock_ok();
            if active.name.eq_ignore_ascii_case(&old) {
                rename(&mut active);
            } else if let Some(p) = self
                .other_profiles
                .lock_ok()
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&old))
            {
                rename(p);
            } else {
                return;
            }
        }
        tracing::info!("profile renamed: {old} → {new_name}");
        self.publish_state();
        self.mark_state_dirty();
    }

    fn delete_profile(&self, name: String) {
        if self.profile_def.lock_ok().name.eq_ignore_ascii_case(&name) {
            tracing::warn!("delete_profile: '{name}' is playing — switch away first");
            return;
        }
        {
            let mut others = self.other_profiles.lock_ok();
            let Some(i) = others
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&name))
            else {
                return;
            };
            let gone = others.remove(i);
            RigLibrary::delete_profile(&gone.name);
        }
        tracing::info!("profile deleted: {name}");
        self.publish_state();
    }

    fn edit_song(&self, old: String, name: String, key: String, bpm: u32) {
        {
            let mut songs = self.songs_lib.lock_ok();
            let taken: Vec<String> = songs
                .iter()
                .filter(|s| !s.name.eq_ignore_ascii_case(&old))
                .map(|s| s.name.clone())
                .collect();
            let Some(name) = free_name(&name, taken.iter().map(String::as_str)) else {
                tracing::warn!("edit_song: '{name}' is blank or taken");
                return;
            };
            let Some(song) = songs.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&old)) else {
                return;
            };
            song.name = name.clone();
            if !key.trim().is_empty() {
                song.key = key.trim().to_string();
            }
            if bpm > 0 {
                song.bpm = bpm.clamp(20, 400);
            }
            RigLibrary::save_songs(&songs);
            if !name.eq(&old) {
                let mut sets = self.setlists.lock_ok();
                for e in sets.iter_mut().flat_map(|s| s.entries.iter_mut()) {
                    if e.song.eq_ignore_ascii_case(&old) {
                        e.song = name.clone();
                    }
                }
                RigLibrary::save_setlists(&sets);
            }
        }
        tracing::info!("song edited: {old}");
        self.publish_state();
    }

    fn delete_song(&self, name: String) {
        if self
            .setlists
            .lock_ok()
            .iter()
            .any(|s| s.entries.iter().any(|e| e.song.eq_ignore_ascii_case(&name)))
        {
            tracing::warn!("delete_song: '{name}' is in a setlist — remove it there first");
            return;
        }
        {
            let mut songs = self.songs_lib.lock_ok();
            songs.retain(|s| !s.name.eq_ignore_ascii_case(&name));
            RigLibrary::save_songs(&songs);
        }
        tracing::info!("song deleted: {name}");
        self.publish_state();
    }

    fn rename_setlist(&self, index: u32, new_name: String) {
        {
            let mut sets = self.setlists.lock_ok();
            let taken: Vec<String> = sets
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index as usize)
                .map(|(_, s)| s.name.clone())
                .collect();
            let Some(new_name) = free_name(&new_name, taken.iter().map(String::as_str)) else {
                tracing::warn!("rename_setlist: '{new_name}' is blank or taken");
                return;
            };
            let Some(set) = sets.get_mut(index as usize) else {
                return;
            };
            set.name = new_name;
            RigLibrary::save_setlists(&sets);
        }
        self.publish_state();
    }

    fn duplicate_setlist(&self, index: u32, new_name: String) {
        {
            let mut sets = self.setlists.lock_ok();
            let Some(new_name) = free_name(&new_name, sets.iter().map(|s| s.name.as_str())) else {
                tracing::warn!("duplicate_setlist: '{new_name}' is blank or taken");
                return;
            };
            let Some(mut copy) = sets.get(index as usize).cloned() else {
                return;
            };
            copy.name = new_name;
            sets.push(copy);
            RigLibrary::save_setlists(&sets);
        }
        self.publish_state();
    }

    fn delete_setlist(&self, index: u32) {
        let index = index as usize;
        let recall = {
            let mut sets = self.setlists.lock_ok();
            if sets.len() <= 1 || index >= sets.len() {
                tracing::warn!("delete_setlist: refusing — the last setlist, or no such set");
                return;
            }
            sets.remove(index);
            RigLibrary::save_setlists(&sets);
            let mut active = self.setlist_index.lock_ok();
            if *active == index {
                // The set that was playing is gone: start the one that took
                // its place from the top.
                *active = index.min(sets.len() - 1);
                true
            } else {
                if *active > index {
                    *active -= 1;
                }
                false
            }
        };
        if recall {
            *self.song_index.lock_ok() = 0;
            self.recall_song(0);
            self.mark_state_dirty();
        }
        self.publish_state();
    }

    fn set_song_profile(&self, song: String, profile: String) {
        {
            let mut songs = self.songs_lib.lock_ok();
            let Some(s) = songs
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&song))
            else {
                return;
            };
            s.profile = profile.trim().to_string();
            RigLibrary::save_songs(&songs);
        }
        tracing::info!("song '{song}' → profile '{profile}'");
        // The song that is up is re-recalled, so the choice is heard now
        // rather than the next time the song comes round.
        let current = *self.song_index.lock_ok();
        let is_current = self
            .resolved_setlist()
            .get(current)
            .is_some_and(|(name, ..)| name.eq_ignore_ascii_case(&song));
        if is_current {
            self.recall_song(current);
        }
        self.publish_state();
    }

    fn set_song_start_part(&self, song: String, part: String) {
        {
            let mut songs = self.songs_lib.lock_ok();
            let Some(s) = songs
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&song))
            else {
                return;
            };
            let part = part.trim();
            if !part.is_empty() && !s.parts.iter().any(|p| p.eq_ignore_ascii_case(part)) {
                tracing::warn!("set_song_start_part: '{song}' has no part '{part}'");
                return;
            }
            s.start_part = part.to_string();
            RigLibrary::save_songs(&songs);
        }
        self.publish_state();
    }

    fn set_part_profile(&self, part: String, profile: String) {
        let profile = profile.trim().to_string();
        self.edit_current_song("set_part_profile", |song| {
            if !song.parts.iter().any(|p| p.eq_ignore_ascii_case(&part)) {
                return false;
            }
            match song
                .part_recalls
                .iter_mut()
                .find(|r| r.part.eq_ignore_ascii_case(&part))
            {
                Some(r) => r.profile.clone_from(&profile),
                None => song.part_recalls.push(crate::profiles::PartRecallDef {
                    part: part.clone(),
                    profile: profile.clone(),
                    patch: String::new(),
                    overrides: Vec::new(),
                    ..Default::default()
                }),
            }
            true
        });
        // The part that is up is re-recalled, so the choice is heard now.
        let (song_idx, part_idx) = (*self.song_index.lock_ok(), *self.part_index.lock_ok());
        let is_current = self
            .resolved_setlist()
            .get(song_idx)
            .and_then(|(.., parts)| {
                parts
                    .get(part_idx)
                    .map(|p| p.name.eq_ignore_ascii_case(&part))
            })
            .unwrap_or(false);
        if is_current {
            Rig::select_part(self, part_idx as u32);
        }
    }

    fn set_profile_default(&self, profile: String, patch: String) {
        let patch = patch.trim().to_string();
        let set = |def: &mut ProfileDef| -> bool {
            if !patch.is_empty()
                && !def
                    .patches
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(&patch))
            {
                tracing::warn!("set_profile_default: '{}' has no patch '{patch}'", def.name);
                return false;
            }
            def.default_patch = patch.clone();
            RigLibrary::save_profile(def);
            true
        };
        let done = {
            let mut active = self.profile_def.lock_ok();
            if active.name.eq_ignore_ascii_case(&profile) {
                set(&mut active)
            } else {
                self.other_profiles
                    .lock_ok()
                    .iter_mut()
                    .find(|p| p.name.eq_ignore_ascii_case(&profile))
                    .is_some_and(|p| set(p))
            }
        };
        if done {
            tracing::info!("profile '{profile}' lands on '{patch}'");
            self.publish_state();
        }
    }

    fn set_block_param(&self, id: String, param: String, value: f32) {
        // A delay or reverb runs fully wet: a `mix` from an older surface
        // is its `level` (see `profiles::PARALLEL_FX`).
        let (param, value) = {
            let blocks = self.blocks.lock_ok();
            match blocks.iter().find(|b| b.id == id) {
                Some(b)
                    if param == "mix"
                        && matches!(b.block_type, BlockType::Delay | BlockType::Reverb)
                        && crate::profiles::is_parallel_fx(&b.name) =>
                {
                    ("level".to_string(), crate::profiles::mix_to_level_db(value))
                }
                _ => (param, value),
            }
        };
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

    fn set_block_level(&self, id: String, level_db: f32, commit: bool) {
        let level_db = level_db.clamp(-60.0, 24.0);
        let comp = RigLibrary::load_compositions();
        // The block as built, and the patch it is in.
        let (built, patch_name, sr) = {
            let guard = self.rig.lock_ok();
            let Some(prig) = guard.as_ref() else { return };
            let Some(patch) = prig.active_patch() else { return };
            let ids = prig.active_block_ids();
            let built = patch
                .chain
                .iter()
                .filter(|b| b.has_backend())
                .zip(ids.iter())
                .find(|(_, bid)| **bid == id)
                .map(|(b, _)| b.clone());
            (built, patch.name.clone(), f64::from(prig.sample_rate()))
        };
        let Some(block) = built else { return };
        let (patch_def, drives, pool) = {
            let def = self.profile_def.lock_ok();
            let patch = def
                .patches
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&patch_name))
                .cloned();
            (patch, self.drive_presets.lock_ok().clone(), def.presets.clone())
        };
        let Some(home) = Self::level_home(&comp, patch_def.as_ref(), &block) else {
            tracing::warn!(block = %block.name, "output level: block has no stored level");
            return;
        };
        let Some(stored) = Self::stored_level(&comp, &drives, &pool, &home) else { return };

        if !commit {
            // Live: the built trim, corrected for a drive knob moved since
            // the build (as `apply_drive` does), plus the level change.
            let path = std::path::Path::new(&block.nam);
            let delta = |d: f32| signal_sampler::nam_calibrate::drive_output_delta_cached(path, sr, d);
            let built_drive = block.param_f32("drive").unwrap_or(0.5);
            let drive_now = self
                .blocks
                .lock_ok()
                .iter()
                .find(|b| b.id == id)
                .and_then(|b| b.params.iter().find(|p| p.name == "drive").map(|p| p.value))
                .unwrap_or(built_drive);
            let out = block.output_trim_db - delta(built_drive) + delta(drive_now) + (level_db - stored);
            {
                let guard = self.rig.lock_ok();
                if let Some(prig) = guard.as_ref() {
                    prig.rig().set_active_block_param(&id, "output_trim", out);
                }
            }
            if let Some(b) = self.blocks.lock_ok().iter_mut().find(|b| b.id == id) {
                b.output_level_db = Some(level_db);
            }
            self.events.publish(RigEvent::Chain(Rig::chain(self)));
            return;
        }

        // Commit: store it with the gear, then rebuild so every patch playing
        // that amp or pedal is built with it.
        match &home {
            LevelHome::Amp { preset, snapshot, second } => {
                let mut comp = comp;
                let Some(snap) = comp
                    .modules
                    .iter_mut()
                    .filter(|m| {
                        m.module.eq_ignore_ascii_case("Amp") && m.name.eq_ignore_ascii_case(preset)
                    })
                    .flat_map(|m| m.snapshots.iter_mut())
                    .find(|s| s.name.eq_ignore_ascii_case(snapshot))
                else {
                    return;
                };
                if *second {
                    snap.level2_db = level_db;
                } else {
                    snap.level_db = level_db;
                }
                RigLibrary::save_compositions(&comp);
            }
            LevelHome::Pool { name } => {
                let mut def = self.profile_def.lock_ok();
                if let Some(p) = def.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
                    p.level_db = level_db;
                }
                RigLibrary::save_profile(&def);
            }
            LevelHome::Drive { nam } => {
                let mut dps = self.drive_presets.lock_ok();
                let file = |p: &str| std::path::Path::new(p).file_name().map(std::ffi::OsStr::to_owned);
                if let Some(o) = dps
                    .iter_mut()
                    .flat_map(|p| p.options.iter_mut())
                    .find(|o| o.nam == *nam || (file(&o.nam).is_some() && file(&o.nam) == file(nam)))
                {
                    o.level_db = level_db;
                }
                RigLibrary::save_drive_presets(&dps);
            }
        }
        tracing::info!(block = %block.name, from_db = stored, to_db = level_db, "output level: stored");
        let rebuilt = {
            let def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt(rebuilt);
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
            song: String::new(),
            name: name.to_string(),
            preset: preset.to_string(),
            preset2: String::new(),
            rig_preset: String::new(),
            snapshot: String::new(),
            modules: Vec::new(),
            blocks: Vec::new(),
            drives: Vec::new(),
            trim_db: 0.0,
            level_db: 0.0,
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
            default_patch: String::new(),
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

/// Where a NAM block's Output Level is stored: with the gear, so every patch
/// playing that amp or pedal has it.
#[derive(Clone, Debug)]
enum LevelHome {
    /// An amp module snapshot's `level_db` (`level2_db` for Amp R).
    Amp { preset: String, snapshot: String, second: bool },
    /// A legacy pool preset's `level_db`.
    Pool { name: String },
    /// The drive option playing this capture.
    Drive { nam: String },
}

/// Whether the rig's configured interface can be opened right now: its input
/// and output devices are both enumerable (an empty name is the system
/// default, present whenever any device is).
fn audio_device_present() -> bool {
    let prefs = RigManager::load(AUDIO_RIG_NAME).audio;
    let has = |want: &str, list: Vec<signal_sampler::DeviceInfo>| {
        if want.is_empty() {
            !list.is_empty()
        } else {
            list.iter().any(|d| d.name == want)
        }
    };
    has(&prefs.input_device, GuitarRig::input_devices())
        && has(&prefs.output_device, GuitarRig::output_devices())
}
