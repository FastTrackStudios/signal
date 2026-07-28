//! Live **sample-instrument** rig — the MIDI analog of [`GuitarRig`](crate::rig::GuitarRig),
//! running signal's [`SampleEngine`]s ON daw's realtime [`AudioEngine`](daw::standalone::audio_engine::AudioEngine).
//!
//! [`SamplerRig`] is the daw-backed replacement for the retired `SamplerPlayer`
//! (`player.rs`, deleted): same load / MIDI / drum-mixer / preload / stats surface,
//! but the cpal output stream + bespoke render callback are gone. Instead a
//! single **bank track** in a tiny daw project carries a `BankInstrument`
//! (a [`SamplerBank`] wrapped as a [`PluginInstance`]); daw's renderer runs
//! that instrument every block on daw's [`AudioEngine`](daw::standalone::audio_engine::AudioEngine), and the bank's
//! existing engine / preset / drum-mixer machinery is reused verbatim — no
//! mixer or voice engine is re-implemented here.
//!
//! ## Two modes
//!
//! - **Live** ([`SamplerRig::new`] / [`open`](SamplerRig::open) / device
//!   variants): opens daw's output-only [`AudioEngine`](daw::standalone::audio_engine::AudioEngine), seeds a one-track
//!   project, and installs the `BankInstrument`. MIDI is delivered to the
//!   bank by pushing it onto the bank track's live-MIDI queue
//!   ([`Standalone::push_note_on`] / `push_note_off` / `push_cc`); daw's
//!   renderer drains that queue each block and hands it to the instrument,
//!   which forwards each message to the bank.
//! - **Offline** ([`SamplerRig::new_offline`]): no device, no daw engine.
//!   [`render_offline`](SamplerRig::render_offline) drives the bank's
//!   [`render`](SamplerBank::render) directly (mirroring `SamplerPlayer`'s
//!   offline path) so tests / benches can pull blocks without an audio
//!   backend or daw renderer plumbing.
//!
//! ## Per-instrument-track model (alternate API)
//!
//! The lower-level [`add_instrument`](SamplerRig::add_instrument) /
//! [`from_mixer_layout`](SamplerRig::from_mixer_layout) surface maps one
//! [`SampleEngine`] onto one daw track with daw-native send/bus routing — the
//! richer "one instrument = one daw track" model. It is retained alongside
//! the bank-backed surface for the eventual full per-track migration.
//!
//! ## Drum mic / bus routing (the per-track mixer mapping)
//!
//! [`SamplerRig::from_mixer_layout`] maps a [`MixerLayout`] onto daw's native
//! send/bus primitives — **no new daw capability is needed**:
//!
//! - **Close mic / direct-to-master** → a plain instrument track.
//! - **Bus mic (overhead / room)** → the mic's instrument track gets a
//!   [`Routing::add_send`] to a shared **bus track** with
//!   [`SendMode::PostFx`], and its parent-send to master is disabled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use midicore::MidiMonitor;

use daw::service::{
    FxChainContext, FxChains, ProjectContext, RouteRef, Routing, SendMode, TrackRef, Tracks,
};
use daw::standalone::Standalone;
use daw::standalone::metering::{Meters, linear_to_db};
use daw_audio_io::AudioIoPrefs;
use signal_rig_host::{RigHost, RigProject};
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::bank::{PreloadProfile, SamplerBank};
use crate::engine::SampleEngine;
use crate::engine::cache::{EvictStats, PreloadStats};
use crate::instrument::SamplerInstrument;
use crate::mixer::{FX_PREPARE_BLOCK, MixerLayout};
use crate::stats::AudioStatsSnapshot;

/// Stable identifier for a loaded instrument within the rig (e.g. a piece id
/// like `"kick"`, or `"kick:Overhead"` for a per-mic instrument).
pub type InstrumentId = String;

/// Block size sampler instruments are prepared for.
const PREPARE_BLOCK: u32 = FX_PREPARE_BLOCK;

const SAMPLER_PROJECT_NAME: &str = "Signal Sampler Rig";
const BANK_TRACK_NAME: &str = "Sampler";

// ── BankInstrument ──────────────────────────────────────────────────────────

/// A whole [`SamplerBank`] presented to daw as one instrument
/// [`PluginInstance`]. The renderer runs it each block on the bank track:
/// it drains that block's MIDI into the bank (forwarding note-on / note-off /
/// CC) and renders the bank's stereo mix into the track bus, which sums to
/// master.
///
/// The bank is shared (`Arc<Mutex<…>>`) with the control side so loads /
/// drum-mixer tweaks / stats reads happen on the UI thread; the audio thread
/// `try_lock`s and outputs silence on contention (same policy as
/// `SamplerPlayer`'s callback), recording a lock-miss.
struct BankInstrument {
    bank: Arc<Mutex<SamplerBank>>,
    stats: Arc<BankStats>,
    /// Interleaved-stereo render scratch, reused across blocks.
    scratch: Vec<f32>,
    prepared: bool,
}

/// Audio-thread-written counters the control side reads for
/// [`AudioStatsSnapshot`]. Only the fields daw's renderer can source live
/// here; cache / voice fields are read straight off the bank under its lock.
#[derive(Debug, Default)]
struct BankStats {
    callbacks: AtomicU64,
    lock_misses: AtomicU64,
    last_render_us: AtomicU64,
    max_render_us: AtomicU64,
}

impl BankStats {
    fn record_render(&self, us: u64) {
        self.last_render_us.store(us, Ordering::Relaxed);
        let mut cur = self.max_render_us.load(Ordering::Relaxed);
        while us > cur {
            match self.max_render_us.compare_exchange_weak(
                cur,
                us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(n) => cur = n,
            }
        }
    }
    fn reset(&self) {
        self.callbacks.store(0, Ordering::Relaxed);
        self.lock_misses.store(0, Ordering::Relaxed);
        self.last_render_us.store(0, Ordering::Relaxed);
        self.max_render_us.store(0, Ordering::Relaxed);
    }
}

impl BankInstrument {
    fn new(bank: Arc<Mutex<SamplerBank>>, stats: Arc<BankStats>) -> Self {
        Self {
            bank,
            stats,
            scratch: Vec::new(),
            prepared: false,
        }
    }
}

impl PluginInstance for BankInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.sampler.bank".into(),
            name: "Signal Sampler Bank".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _v: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _t: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, _sr: f64, block_size: u32) -> Result<(), PluginError> {
        self.scratch.resize(block_size as usize * 2, 0.0);
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let start = std::time::Instant::now();
        self.stats.callbacks.fetch_add(1, Ordering::Relaxed);
        let frames = out_l.len().min(out_r.len());

        // Never block the audio thread on the control side's bank lock.
        let mut bank = match self.bank.try_lock() {
            Ok(b) => b,
            Err(_) => {
                self.stats.lock_misses.fetch_add(1, Ordering::Relaxed);
                out_l[..frames].fill(0.0);
                out_r[..frames].fill(0.0);
                return Ok(());
            }
        };

        // Apply this block's live / clip MIDI to the bank (channel 0; the
        // bank routes by its own MIDI-channel table when set).
        for ev in events.midi {
            apply_midi(&mut bank, &ev.message);
        }

        let want = frames * 2;
        if self.scratch.len() < want {
            self.scratch.resize(want, 0.0);
        }
        self.scratch[..want].fill(0.0);
        bank.render(&mut self.scratch[..want]);
        for f in 0..frames {
            out_l[f] = self.scratch[f * 2];
            out_r[f] = self.scratch[f * 2 + 1];
        }
        drop(bank);

        self.stats.record_render(start.elapsed().as_micros() as u64);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// Forward one daw MIDI message into the bank. Channel is honoured via the
/// bank's `midi_message` routing (so `set_midi_channel` works); plain
/// note/cc with no channel routing fall through to the bank's default id.
fn apply_midi(bank: &mut SamplerBank, message: &midicore::MidiEvent) {
    use midicore::MidiEvent;
    match message {
        MidiEvent::NoteOn {
            channel,
            key,
            velocity,
        } => bank.midi_message(channel.index(), 0x90, key.get(), velocity.get()),
        MidiEvent::NoteOff {
            channel,
            key,
            velocity,
        } => bank.midi_message(channel.index(), 0x80, key.get(), velocity.get()),
        MidiEvent::ControlChange {
            channel,
            controller,
            value,
        } => bank.midi_message(channel.index(), 0xB0, controller.get(), value.get()),
        MidiEvent::ChannelPressure { channel, pressure } => {
            bank.midi_message(channel.index(), 0xD0, pressure.get(), 0)
        }
        MidiEvent::PolyAftertouch {
            channel,
            key,
            pressure,
        } => bank.midi_message(channel.index(), 0xA0, key.get(), pressure.get()),
        _ => {}
    }
}

/// Build a `MidiEvent::NoteOn` from raw channel/note/velocity bytes.
fn ev_note_on(ch: u8, note: u8, vel: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOn {
        channel: Channel::new(ch),
        key: KeyNumber::new(note),
        velocity: Velocity::new(vel),
    }
}

/// Build a `MidiEvent::NoteOff` from raw channel/note/velocity bytes.
fn ev_note_off(ch: u8, note: u8, vel: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOff {
        channel: Channel::new(ch),
        key: KeyNumber::new(note),
        velocity: Velocity::new(vel),
    }
}

/// Build a `MidiEvent::ControlChange` from raw channel/controller/value bytes.
fn ev_cc(ch: u8, controller: u8, value: u8) -> midicore::MidiEvent {
    use midicore::{Channel, ControllerNumber, ControllerValue, MidiEvent};
    MidiEvent::ControlChange {
        channel: Channel::new(ch),
        controller: ControllerNumber::new(controller),
        value: ControllerValue::new(value),
    }
}

// ── Per-instrument-track routing tables (alternate per-track API) ────────────

/// One instrument's place in the daw project (per-track API): which track
/// carries it, which FX slot guid backs its [`SamplerInstrument`], and which
/// [`Meters`] cell holds its post-fader peak.
#[derive(Clone, Debug)]
pub struct InstrumentTrack {
    /// daw track guid — the target of `push_note_on` / `push_cc`.
    pub track_guid: String,
    /// FX-slot guid the instrument box was inserted under.
    pub fx_guid: String,
    /// Meter cell index (project track order) for this track's output peak.
    pub meter_index: usize,
    /// Optional grouping piece id (e.g. `"kick"`) when this instrument is one
    /// mic of a multi-mic piece. `None` = standalone instrument.
    pub piece: Option<String>,
}

/// One bus track (overhead / room) that sums its mic sends → master.
#[derive(Clone, Debug)]
pub struct BusTrack {
    /// The bus mic id this track collects (e.g. `"Overhead"`).
    pub id: String,
    pub track_guid: String,
    pub meter_index: usize,
}

// ── MIDI monitor ─────────────────────────────────────────────────────────────

// The rolling MIDI monitor now lives in `midicore` (a cross-cutting MIDI
// concern, not sampler-specific) and is re-exported below for consumers that
// reach it via `signal_sampler::MidiMonitor`.

// ── SamplerRig ───────────────────────────────────────────────────────────────

/// Shared inner state, behind an `Arc` so [`SamplerRig`] is cheap to
/// [`Clone`] (the audio engine + bank are shared, like `SamplerPlayer`).
struct Inner {
    /// `None` in offline mode (no device opened).
    daw: Option<Standalone>,
    /// The shared daw host (project + output engine + transport); drop =
    /// stop audio. `None` offline.
    _host: Option<RigHost>,
    meters: Mutex<Arc<Meters>>,
    #[allow(dead_code)]
    project_guid: String,

    /// The signal-domain engine/preset/drum-mixer/preload/stats machinery,
    /// shared with the bank track's `BankInstrument` (live) or driven
    /// directly (offline). This is the bank-backed surface's home.
    bank: Arc<Mutex<SamplerBank>>,
    /// daw track guid carrying the `BankInstrument` (live mode). `None`
    /// offline.
    bank_track: Option<String>,
    stats: Arc<BankStats>,

    /// Per-instrument-track routing table (alternate per-track API).
    tracks: Mutex<TrackTables>,

    /// Tap on the live MIDI stream for UI monitoring (populated by
    /// [`SamplerRig::attach_midi`]).
    midi_monitor: MidiMonitor,

    /// Per-track kit hosting (the fully daw-based drum mixer), when a kit
    /// was loaded via [`SamplerRig::load_kit_tracks`].
    kit: Mutex<Option<crate::kit_tracks::KitState>>,

    /// Stem routing: articulation class → output bus id for
    /// [`SamplerRig::render_offline_document_buses`]. Default: every class →
    /// `"main"` (single bus, bit-identical to the plain document render).
    class_routing: Mutex<std::collections::BTreeMap<crate::engine::ArticClass, String>>,

    pub sample_rate: u32,
}

fn default_class_routing() -> std::collections::BTreeMap<crate::engine::ArticClass, String> {
    use crate::engine::ArticClass;
    [
        (ArticClass::Longs, "main".to_string()),
        (ArticClass::Shorts, "main".to_string()),
    ]
    .into_iter()
    .collect()
}

#[derive(Default)]
struct TrackTables {
    instruments: HashMap<InstrumentId, InstrumentTrack>,
    order: Vec<InstrumentId>,
    buses: HashMap<String, BusTrack>,
}

/// A live sample-instrument rig backed by daw's [`AudioEngine`](daw::standalone::audio_engine::AudioEngine) — the
/// daw-native replacement for the retired `SamplerPlayer`. Cheap
/// to clone; all clones share one audio engine + bank.
#[derive(Clone)]
pub struct SamplerRig {
    inner: Arc<Inner>,
}

impl SamplerRig {
    // ── Constructors (SamplerPlayer-equivalent) ──────────────────────────────

    /// Open the system default output device (replaces the retired
    /// `SamplerPlayer::new`).
    pub fn new() -> eyre::Result<Self> {
        Self::open(&AudioIoPrefs {
            sample_rate: 0,
            buffer_size: 256,
            ..Default::default()
        })
    }

    /// Open a specific output device by substring + optional rate / buffer /
    /// cache budget (replaces the retired
    /// `SamplerPlayer::with_device_config_and_cache_budget`).
    /// `device_name` empty / `None` = system default.
    pub fn with_device_config_and_cache_budget(
        device_name: Option<&str>,
        sample_rate: Option<u32>,
        buffer_size: Option<u32>,
        cache_budget_bytes: Option<usize>,
    ) -> eyre::Result<Self> {
        let prefs = AudioIoPrefs {
            output_device: device_name.unwrap_or("").to_string(),
            sample_rate: sample_rate.unwrap_or(0),
            buffer_size: buffer_size.unwrap_or(256),
            ..Default::default()
        };
        Self::open_with_cache_budget(&prefs, cache_budget_bytes)
    }

    /// Open an output-only sampler project on daw's engine, transport rolling,
    /// carrying an (empty) [`SamplerBank`] on a single bank track.
    pub fn open(prefs: &AudioIoPrefs) -> eyre::Result<Self> {
        Self::open_with_cache_budget(prefs, None)
    }

    /// [`open`](Self::open) with an explicit decoded-sample cache budget.
    pub fn open_with_cache_budget(
        prefs: &AudioIoPrefs,
        cache_budget_bytes: Option<usize>,
    ) -> eyre::Result<Self> {
        // Seed the project and reserve one bank track + fx slot for the
        // BankInstrument.
        let project = RigProject::new(SAMPLER_PROJECT_NAME);
        let bank_track = project.add_track(BANK_TRACK_NAME)?;
        let bank_fx_guid = project.add_fx_slot(&bank_track, "sampler-bank")?;

        // Output-only engine (a sampler generates, never records) + one meter
        // cell for the bank track.
        let host = project.start_output(prefs)?;
        let sample_rate = host.sample_rate();
        let meters = host.install_meters(1);
        let daw = host.daw().clone();
        let project_guid = host.project_guid().to_string();

        // Build + install the bank instrument.
        let bank = Arc::new(Mutex::new(SamplerBank::with_cache_budget(
            sample_rate,
            cache_budget_bytes,
        )));
        let stats = Arc::new(BankStats::default());
        let mut inst = BankInstrument::new(bank.clone(), stats.clone());
        let _ = inst.prepare(sample_rate as f64, PREPARE_BLOCK);
        daw.insert_plugin_instance(bank_fx_guid, Box::new(inst));

        host.play();

        tracing::info!(
            sample_rate,
            project = %project_guid,
            "signal-sampler: sampler rig started on daw engine (bank-backed)"
        );

        Ok(Self {
            inner: Arc::new(Inner {
                daw: Some(daw),
                _host: Some(host),
                meters: Mutex::new(meters),
                project_guid,
                bank,
                bank_track: Some(bank_track),
                stats,
                tracks: Mutex::new(TrackTables::default()),
                midi_monitor: MidiMonitor::default(),
                kit: Mutex::new(None),
                class_routing: Mutex::new(default_class_routing()),
                sample_rate,
            }),
        })
    }

    /// Create an offline rig (no device, no daw engine). Use the normal load /
    /// MIDI APIs, then [`render_offline`](Self::render_offline) /
    /// [`render_offline_buses`](Self::render_offline_buses) to pull blocks.
    /// Replaces the retired `SamplerPlayer::new_offline`.
    pub fn new_offline(sample_rate: u32) -> Self {
        Self::new_offline_with_cache_budget(sample_rate, None)
    }

    pub fn new_offline_with_cache_budget(
        sample_rate: u32,
        cache_budget_bytes: Option<usize>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                daw: None,
                _host: None,
                meters: Mutex::new(Meters::new(0)),
                project_guid: String::new(),
                bank: Arc::new(Mutex::new(SamplerBank::with_cache_budget(
                    sample_rate,
                    cache_budget_bytes,
                ))),
                bank_track: None,
                stats: Arc::new(BankStats::default()),
                tracks: Mutex::new(TrackTables::default()),
                midi_monitor: MidiMonitor::default(),
                kit: Mutex::new(None),
                class_routing: Mutex::new(default_class_routing()),
                sample_rate,
            }),
        }
    }

    /// True if this rig has no audio device (offline mode).
    pub fn is_offline(&self) -> bool {
        self.inner.daw.is_none()
    }

    fn bank(&self) -> &Arc<Mutex<SamplerBank>> {
        &self.inner.bank
    }

    // ── Instrument loading (bank-backed; SamplerPlayer-equivalent) ────────────

    pub fn load_instrument(
        &self,
        id: impl Into<InstrumentId>,
        spec_path: &Path,
        samples_root: Option<&Path>,
        section: impl Into<String>,
        mic: impl Into<String>,
    ) -> eyre::Result<()> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_instrument(id, spec_path, samples_root, section, mic)
    }

    /// Load an instrument whose samples come from `zones_path` and whose engine
    /// config (articulations / keyswitch / CC58 / legato / dynamics) comes from
    /// a separate descriptive spec — required for articulation + keyswitch
    /// switching on Cinematic Studio libraries (zone specs carry no config).
    pub fn load_instrument_with_config(
        &self,
        id: impl Into<InstrumentId>,
        config_path: &Path,
        zones_path: &Path,
        samples_root: &Path,
        section: impl Into<String>,
        mic: impl Into<String>,
    ) -> eyre::Result<()> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_instrument_with_config(id, config_path, zones_path, samples_root, section, mic)
    }

    /// Load a `.signalpack`.
    pub fn load_pack(&self, id: impl Into<InstrumentId>, pack_path: &Path) -> eyre::Result<()> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_pack(id, pack_path)
    }

    /// Load a `.signalblock`.
    pub fn load_block(&self, id: impl Into<InstrumentId>, block_path: &Path) -> eyre::Result<()> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_block(id, block_path)
    }

    /// Load a `.signalengine`.
    pub fn load_engine(&self, id: impl Into<InstrumentId>, engine_path: &Path) -> eyre::Result<()> {
        let spec = crate::engine_spec::EngineSpec::from_file(engine_path)?;
        let dir = engine_path.parent().unwrap_or(std::path::Path::new(""));
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_engine_spec(id, &spec, dir)
    }

    /// Load a `.signalpreset` — returns the per-engine ids registered under
    /// `<id_prefix>:<engine_id>`.
    pub fn load_preset(
        &self,
        id_prefix: &str,
        preset_path: &Path,
    ) -> eyre::Result<Vec<InstrumentId>> {
        let preset = crate::preset_spec::PresetSpec::from_file(preset_path)?;
        let dir = preset_path.parent().unwrap_or(std::path::Path::new(""));
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_preset_spec(id_prefix, &preset, dir)
    }

    /// Load a preset from an in-memory `PresetSpec` (engine paths resolve
    /// against `preset_dir`; absolute paths pass through). Lets callers swap a
    /// single engine ref and reload — the kit-designer's per-piece swap.
    pub fn load_preset_spec(
        &self,
        id_prefix: &str,
        preset: &crate::preset_spec::PresetSpec,
        preset_dir: &Path,
    ) -> eyre::Result<Vec<InstrumentId>> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .load_preset_spec(id_prefix, preset, preset_dir)
    }

    pub fn unload_instrument(&self, id: &str) {
        match self.bank().lock() {
            Ok(mut bank) => bank.unload_instrument(id),
            Err(_) => tracing::warn!("signal-sampler: sampler bank lock poisoned; unload skipped"),
        }
    }

    pub fn set_midi_channel(&self, id: impl Into<InstrumentId>, channel: u8) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_midi_channel(id, channel),
            Err(_) => {
                tracing::warn!("signal-sampler: sampler bank lock poisoned; MIDI channel skipped")
            }
        }
    }

    /// Route live MIDI on any unmapped channel to `id` — makes a single kit /
    /// instrument react to all MIDI without per-channel setup.
    pub fn set_default_instrument(&self, id: impl Into<InstrumentId>) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_default_instrument(id),
            Err(_) => {
                tracing::warn!("signal-sampler: sampler bank lock poisoned; default instr skipped")
            }
        }
    }

    pub fn set_muted(&self, id: &str, muted: bool) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_muted(id, muted),
            Err(_) => tracing::warn!("signal-sampler: sampler bank lock poisoned; mute skipped"),
        }
    }

    /// Pin an instrument to a single articulation (e.g. `"Leg"` legato);
    /// `None` clears the pin. Without a pin, a multi-articulation zone set fires
    /// every articulation matching the note — pin one to play just sustains.
    pub fn pin_articulation(&self, id: &str, artic: Option<String>) {
        match self.bank().lock() {
            Ok(mut bank) => bank.pin_articulation(id, artic),
            Err(_) => tracing::warn!("signal-sampler: bank lock poisoned; pin skipped"),
        }
    }

    /// Select an instrument's live articulation (the keyswitch / CC58
    /// equivalent) — only zones of this articulation fire. Unlike
    /// [`pin_articulation`](Self::pin_articulation) this is the normal melodic
    /// selection (key range still applies); pinning is for percussion / routing.
    pub fn set_articulation(&self, id: &str, artic: impl Into<String>) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_articulation(id, artic),
            Err(_) => {
                tracing::warn!("signal-sampler: bank lock poisoned; set_articulation skipped")
            }
        }
    }

    /// An instrument's current live articulation — reflects keyswitch / CC58
    /// changes coming from the keyboard, so a UI can display what's selected.
    pub fn articulation(&self, id: &str) -> Option<String> {
        self.bank().lock().ok().and_then(|b| b.articulation(id))
    }

    /// Set an instrument's sustain attack envelope in ms (CSS attack
    /// parameter). 0 = the sample's natural attack.
    pub fn set_attack_ms(&self, id: &str, ms: u32) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_attack_ms(id, ms);
        }
    }

    /// Set an instrument's sustain release fade in ms (CSS release parameter);
    /// the recorded release sample plays underneath on note-off.
    pub fn set_release_ms(&self, id: &str, ms: u32) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_release_ms(id, ms);
        }
    }

    /// Pin an instrument's RR-bearing triggers (shorts, legato, releases) to a
    /// specific round-robin slot; `None` restores normal CC59 / cycle / random.
    /// The A/B null harness sweeps this to align our RR ordering with a
    /// deterministic CSS render (CC59 cycle, per the v1.7 manual).
    pub fn set_forced_rr(&self, id: &str, slot: Option<u32>) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_forced_rr(id, slot);
        }
    }

    /// Clone an instrument's loaded [`LibrarySpec`](crate::spec::LibrarySpec).
    pub fn instrument_spec(&self, id: &str) -> Option<crate::spec::LibrarySpec> {
        self.bank().lock().ok().and_then(|b| b.instrument_spec(id))
    }

    /// Explicitly set an instrument's legato mode (`enabled`, `expressive`).
    pub fn set_legato_mode(&self, id: &str, enabled: bool, expressive: bool) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_legato_mode(id, enabled, expressive);
        }
    }

    /// Explicitly set an instrument's play-mode policy — see
    /// [`PlayMode`](crate::engine::PlayMode). Mode selection is normally
    /// automatic (StrictLive everywhere; the document renderer forces
    /// Lookahead for its duration); this is the explicit override.
    pub fn set_play_mode(&self, id: &str, mode: crate::engine::PlayMode) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_play_mode(id, mode);
        }
    }

    /// An instrument's current play-mode policy.
    pub fn play_mode(&self, id: &str) -> Option<crate::engine::PlayMode> {
        self.bank().lock().ok().and_then(|b| b.play_mode(id))
    }

    /// Enable/disable an instrument's legato transition fire log (tests /
    /// offline analysis).
    pub fn set_legato_fire_log_enabled(&self, id: &str, enabled: bool) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_legato_fire_log_enabled(id, enabled);
        }
    }

    /// Recorded legato transition firings for an instrument.
    pub fn legato_fire_log(&self, id: &str) -> Vec<crate::engine::LegatoFireEvent> {
        self.bank()
            .lock()
            .map(|b| b.legato_fire_log(id))
            .unwrap_or_default()
    }

    /// Enable/disable an instrument's structured render trace — which files
    /// play, when, loop points, gains, transitions. Enable BEFORE the render;
    /// read back with [`render_trace`](Self::render_trace) after.
    pub fn set_trace_enabled(&self, id: &str, enabled: bool) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_trace_enabled(id, enabled);
        }
    }

    /// Per-note solo filter (offline analysis) — only the given notes are
    /// audible; muted voices still advance so legato timing is unchanged.
    /// `None` = full mix.
    pub fn set_solo_notes(&self, id: &str, notes: Option<std::collections::BTreeSet<u8>>) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_solo_notes(id, notes);
        }
    }

    /// Pure sample-playback mode: one looped sample per note at a straight
    /// gain — no CC1 layer crossfade, ENV_FLEX, or legato trim/bloom. A clean
    /// Kontakt-CSS baseline for validating raw sample playback.
    pub fn set_pure_playback(&self, id: &str, on: bool) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_pure_playback(id, on);
        }
    }

    /// The structured render trace for an instrument (frames are engine-local).
    pub fn render_trace(&self, id: &str) -> crate::engine::RenderTrace {
        self.bank()
            .lock()
            .map(|b| b.render_trace(id))
            .unwrap_or_default()
    }

    /// Document-mode legato prefire, addressed by instrument id — see
    /// [`SampleEngine::legato_prefire`](crate::engine::SampleEngine::legato_prefire).
    pub fn legato_prefire(&self, id: &str, note: u8, velocity: u8) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.legato_prefire(id, note, velocity);
        }
    }

    /// Line-addressed legato prefire — see
    /// [`SampleEngine::legato_prefire_line`](crate::engine::SampleEngine::legato_prefire_line).
    pub fn legato_prefire_line(
        &self,
        id: &str,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
    ) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.legato_prefire_line(id, line, note, velocity);
        }
    }

    /// Engine frames rendered so far by an instrument — the clock the fire
    /// log's `frame` field is measured on (tests / offline analysis).
    pub fn engine_frames_rendered(&self, id: &str) -> Option<u64> {
        self.bank()
            .lock()
            .ok()
            .and_then(|b| b.engine_frames_rendered(id))
    }

    /// REACTIVE legato-path trigger count since an instrument's fire log was
    /// last enabled — see
    /// [`SampleEngine::reactive_legato_fires`](crate::engine::SampleEngine::reactive_legato_fires).
    pub fn reactive_legato_fires(&self, id: &str) -> u64 {
        self.bank()
            .lock()
            .map(|b| b.reactive_legato_fires(id))
            .unwrap_or(0)
    }

    // ── Document mode (offline; see docs/plan/document-mode.md) ───────────────

    /// Render a [`TrackDocument`](crate::document::TrackDocument) offline
    /// through instrument `id`: annotate against the instrument's spec (legato
    /// prefires land transition arrivals ON the grid; shorts pre-roll), then
    /// walk the schedule sample-accurately. Deterministic: same document +
    /// seed + sample rate ⇒ byte-identical audio (round-robin is pinned per
    /// note from a stable hash of the seed — see `document::stable_rr_slot`).
    /// Only available on an offline rig.
    pub fn render_offline_document(
        &self,
        id: &str,
        doc: &crate::document::TrackDocument,
        opts: &crate::document::DocumentRenderOptions,
    ) -> eyre::Result<crate::document::DocumentRenderResult> {
        if !self.is_offline() {
            return Err(eyre::eyre!(
                "render_offline_document is only available on SamplerRig::new_offline"
            ));
        }
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?;
        let spec = bank
            .instrument_spec(id)
            .ok_or_else(|| eyre::eyre!("no instrument loaded under '{id}'"))?;
        let schedule = crate::document::annotate(doc, &spec, self.inner.sample_rate);
        warm_document(&mut bank, id, doc, &spec);
        Ok(crate::document::render_schedule(
            &mut bank, id, &schedule, opts,
        ))
    }

    /// Route an articulation class (stem) to an output bus for
    /// [`render_offline_document_buses`](Self::render_offline_document_buses).
    /// Default: every class → `"main"`. Routing never perturbs voice order,
    /// round-robin, or timing — it only decides which bus each voice's audio
    /// lands in, so split buses sum back to the main render.
    pub fn set_class_bus(&self, class: crate::engine::ArticClass, bus: impl Into<String>) {
        if let Ok(mut map) = self.inner.class_routing.lock() {
            map.insert(class, bus.into());
        }
    }

    /// Current stem routing (class → bus id).
    pub fn class_routing(&self) -> std::collections::BTreeMap<crate::engine::ArticClass, String> {
        self.inner
            .class_routing
            .lock()
            .map(|m| m.clone())
            .unwrap_or_else(|_| default_class_routing())
    }

    /// [`render_offline_document`](Self::render_offline_document), split into
    /// articulation-class output buses per the routing set with
    /// [`set_class_bus`](Self::set_class_bus) (stem export: Longs / Shorts).
    /// With the default routing the single `"main"` bus is bit-identical to
    /// the plain document render. Only available on an offline rig.
    pub fn render_offline_document_buses(
        &self,
        id: &str,
        doc: &crate::document::TrackDocument,
        opts: &crate::document::DocumentRenderOptions,
    ) -> eyre::Result<crate::document::DocumentBusRenderResult> {
        if !self.is_offline() {
            return Err(eyre::eyre!(
                "render_offline_document_buses is only available on SamplerRig::new_offline"
            ));
        }
        let routing = self.class_routing();
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?;
        let spec = bank
            .instrument_spec(id)
            .ok_or_else(|| eyre::eyre!("no instrument loaded under '{id}'"))?;
        let schedule = crate::document::annotate(doc, &spec, self.inner.sample_rate);
        warm_document(&mut bank, id, doc, &spec);
        Ok(crate::document::render_schedule_buses(
            &mut bank, id, &schedule, opts, &routing,
        ))
    }

    /// Switch an instrument's active microphone position (e.g. `"Mix"`).
    pub fn set_mic(&self, id: &str, mic_id: impl Into<String>) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_mic(id, mic_id),
            Err(_) => tracing::warn!("signal-sampler: bank lock poisoned; set_mic skipped"),
        }
    }

    /// Restrict an instrument's zoned playback to a single mic; `None` plays
    /// all. Needed for multi-mic libraries (CSS ships Main + Mix in one zone
    /// set with no `mics` block, so all mics otherwise sound at once).
    pub fn set_solo_mic(&self, id: &str, mic_id: Option<String>) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_solo_mic(id, mic_id),
            Err(_) => tracing::warn!("signal-sampler: bank lock poisoned; set_solo_mic skipped"),
        }
    }

    /// Warm (decode into cache) the samples `note` would trigger for `id` under
    /// its current articulation pin + solo mic, so the first hit isn't silent.
    /// Background-warm a playable range at startup, like the guitar rig prewarms
    /// its NAM models. Returns how many samples were decoded.
    pub fn warm_note(&self, id: &str, note: u8) -> PreloadStats {
        match self.bank().lock() {
            Ok(bank) => bank.warm_note(id, note),
            Err(_) => PreloadStats::default(),
        }
    }

    pub fn preload_instrument(&self, id: &str) -> eyre::Result<PreloadStats> {
        self.bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?
            .preload_instrument(id)
    }

    pub fn set_preload_profile(&self, profile: PreloadProfile) {
        match self.bank().lock() {
            Ok(mut bank) => bank.set_preload_profile(profile),
            Err(_) => {
                tracing::warn!(
                    "signal-sampler: sampler bank lock poisoned; preload profile skipped"
                )
            }
        }
    }

    // ── MIDI driving — ONE path for live and offline ─────────────────────────
    //
    // Every note/CC funnels through [`dispatch`](Self::dispatch). Live: pushed
    // full-fidelity into daw's live-MIDI ring, drained by the renderer →
    // `BankInstrument::process_block` → `apply_midi` → `bank.midi_message`.
    // Offline: the SAME `apply_midi` directly on the bank. So both modes hit the
    // identical bank routing + engine code — no behaviour can differ between
    // "tested offline" and "played live". The `id` args are kept for API
    // compatibility; the bank-track rig is monotimbral and routes by channel /
    // its default instrument (per-instrument addressing uses the per-track API).

    /// The single MIDI entry point shared by live + offline.
    fn dispatch(&self, msg: midicore::MidiEvent) {
        if let Some((daw, track)) = self.bank_io() {
            daw.push_live_midi(&track, msg);
        } else if let Ok(mut bank) = self.bank().lock() {
            apply_midi(&mut bank, &msg);
        }
    }

    pub fn note_on(&self, _id: &str, note: u8, velocity: u8) {
        self.dispatch(ev_note_on(0, note, velocity));
    }

    pub fn note_off(&self, _id: &str, note: u8) {
        self.dispatch(ev_note_off(0, note, 0));
    }

    pub fn note_off_with_velocity(&self, _id: &str, note: u8, velocity: u8) {
        self.dispatch(ev_note_off(0, note, velocity));
    }

    pub fn cc(&self, _id: &str, controller: u8, value: u8) {
        self.dispatch(ev_cc(0, controller, value));
    }

    /// All Notes Off (CC 123) — release every held note.
    pub fn all_notes_off(&self, _id: &str) {
        self.dispatch(ev_cc(0, 123, 0));
    }

    /// Panic — All Sound Off (CC 120): immediate silence.
    pub fn panic(&self, _id: &str) {
        self.dispatch(ev_cc(0, 120, 0));
    }

    /// Dispatch a raw MIDI message (status + 2 data bytes) — same path as
    /// hardware MIDI. Channel is preserved (full fidelity).
    pub fn midi_message(&self, channel: u8, status: u8, data1: u8, data2: u8) {
        use midicore::{Channel, MidiEvent, PitchBend, ProgramNumber};
        let msg = match status & 0xF0 {
            0x90 if data2 > 0 => ev_note_on(channel, data1, data2),
            0x90 | 0x80 => ev_note_off(channel, data1, data2),
            0xB0 => ev_cc(channel, data1, data2),
            0xC0 => MidiEvent::ProgramChange {
                channel: Channel::new(channel),
                program: ProgramNumber::new(data1),
            },
            0xE0 => MidiEvent::PitchBend {
                channel: Channel::new(channel),
                bend: PitchBend::from_halves(midicore::U7::new(data1), midicore::U7::new(data2)),
            },
            _ => return,
        };
        self.dispatch(msg);
    }

    /// `(daw, bank_track_guid)` when running live with a bank track.
    fn bank_io(&self) -> Option<(&Standalone, String)> {
        match (self.inner.daw.as_ref(), self.inner.bank_track.as_ref()) {
            (Some(d), Some(t)) => Some((d, t.clone())),
            _ => None,
        }
    }

    // ── Hardware MIDI input (daw-owned primitive) ─────────────────────────────

    /// List available hardware MIDI input port names — for a device picker.
    /// All device enumeration lives in `midicore::midir`; signal only forwards.
    pub fn midi_input_ports() -> Vec<String> {
        midicore::midir::input_ports()
    }

    /// Open a hardware MIDI keyboard and forward its events into this rig's bank
    /// track (live mode only). `selection` chooses one named device, **all**
    /// devices merged, or a REAPER-style **virtual** port.
    ///
    /// The returned [`midicore::midir::MidiInput`] owns the open connection(s) —
    /// hold it for as long as you want MIDI, drop it to stop. All MIDI primitive
    /// logic (enumeration, selection, byte parsing) lives in `midicore`; signal
    /// just wires the source to daw's live-MIDI ring with full fidelity
    /// (channel / velocity / pitch-bend preserved via `push_live_midi`).
    pub fn attach_midi(
        &self,
        selection: midicore::PortSelector,
    ) -> eyre::Result<midicore::midir::MidiInput> {
        let (daw, track) = match (self.inner.daw.as_ref(), self.inner.bank_track.as_ref()) {
            (Some(d), Some(t)) => (d.clone(), t.clone()),
            _ => eyre::bail!("attach_midi requires a live rig with a bank track (not offline)"),
        };
        // Tap for the UI monitor, then forward full-fidelity to the engine —
        // the monitor-tap → live-MIDI-sink wiring is midicore's.
        let sink = midicore::attach::tap_sink(self.inner.midi_monitor.clone(), move |ev| {
            daw.push_live_midi(&track, ev);
        });
        midicore::midir::MidiInput::open(selection, sink)
    }

    /// Like [`attach_midi`](Self::attach_midi), but runs every incoming event
    /// through `transform` first, forwarding each produced event to the engine.
    /// This is where a drum-map converter (e.g. Strata Prime → MM2) inserts:
    /// pass a closure that owns a `midicore::DrumMapConverter` and returns
    /// `conv.convert(ev)`. The monitor taps the *original* (pre-transform)
    /// event so a UI shows what the hardware actually sent.
    ///
    /// `transform` is shared behind a mutex (the midir sink is `Fn + Clone`),
    /// and runs in the MIDI callback — keep it allocation-light and lock-free
    /// of the audio thread.
    pub fn attach_midi_transformed<F>(
        &self,
        selection: midicore::PortSelector,
        transform: F,
    ) -> eyre::Result<midicore::midir::MidiInput>
    where
        F: FnMut(midicore::MidiEvent) -> Vec<midicore::MidiEvent> + Send + 'static,
    {
        let (daw, track) = match (self.inner.daw.as_ref(), self.inner.bank_track.as_ref()) {
            (Some(d), Some(t)) => (d.clone(), t.clone()),
            _ => eyre::bail!(
                "attach_midi_transformed requires a live rig with a bank track (not offline)"
            ),
        };
        let sink = midicore::attach::tap_sink_transformed(
            self.inner.midi_monitor.clone(),
            transform,
            move |ev| {
                daw.push_live_midi(&track, ev);
            },
        );
        midicore::midir::MidiInput::open(selection, sink)
    }

    /// The live MIDI monitor — a rolling log + total count of messages reaching
    /// the rig, so a UI can confirm MIDI is arriving (and from which device).
    pub fn midi_monitor(&self) -> MidiMonitor {
        self.inner.midi_monitor.clone()
    }

    // ── Drum mixer (bank-backed; SamplerPlayer-equivalent) ────────────────────

    pub fn drum_mixer_layout(&self, id: &str) -> Option<MixerLayout> {
        self.bank().lock().ok()?.preset_mixer_layout(id)
    }

    pub fn drum_mixer_meters(&self, id: &str) -> Option<Arc<crate::mixer::MixerMeters>> {
        self.bank().lock().ok()?.preset_mixer_meters(id)
    }

    pub fn set_mixer_piece_gain_db(&self, id: &str, idx: usize, db: f32) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_piece_gain_db(id, idx, db);
        }
    }
    pub fn set_mixer_piece_mute(&self, id: &str, idx: usize, muted: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_piece_mute(id, idx, muted);
        }
    }
    pub fn set_mixer_piece_solo(&self, id: &str, idx: usize, soloed: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_piece_solo(id, idx, soloed);
        }
    }
    pub fn set_mixer_channel_gain_db(&self, id: &str, idx: usize, db: f32) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_channel_gain_db(id, idx, db);
        }
    }
    pub fn set_mixer_channel_mute(&self, id: &str, idx: usize, muted: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_channel_mute(id, idx, muted);
        }
    }
    pub fn set_mixer_channel_solo(&self, id: &str, idx: usize, soloed: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_channel_solo(id, idx, soloed);
        }
    }
    pub fn set_mixer_send_level_db(&self, id: &str, idx: usize, db: f32) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_send_level_db(id, idx, db);
        }
    }
    pub fn set_mixer_send_mute(&self, id: &str, idx: usize, muted: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_send_mute(id, idx, muted);
        }
    }
    pub fn set_mixer_send_solo(&self, id: &str, idx: usize, soloed: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_send_solo(id, idx, soloed);
        }
    }
    pub fn set_mixer_bus_gain_db(&self, id: &str, idx: usize, db: f32) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_bus_gain_db(id, idx, db);
        }
    }
    pub fn set_mixer_bus_mute(&self, id: &str, idx: usize, muted: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_bus_mute(id, idx, muted);
        }
    }
    pub fn set_mixer_bus_solo(&self, id: &str, idx: usize, soloed: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_bus_solo(id, idx, soloed);
        }
    }
    pub fn set_mixer_master_gain_db(&self, id: &str, db: f32) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_master_gain_db(id, db);
        }
    }
    pub fn set_mixer_master_mute(&self, id: &str, muted: bool) {
        if let Ok(mut b) = self.bank().lock() {
            b.set_mixer_master_mute(id, muted);
        }
    }

    // ── Hosted FX plugins (CLAP / VST3) on the bank's drum mixer ──────────────

    pub fn load_mixer_plugin(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Option<usize>, signal_plugin_host::PluginError> {
        let plugin = match signal_plugin_host::HostedPlugin::load(path)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut bank = self.bank().lock().map_err(|_| {
            signal_plugin_host::PluginError::LoadFailed("bank mutex poisoned".into())
        })?;
        let slot = bank.install_mixer_plugin(id, target, plugin)?;
        Ok(Some(slot))
    }

    /// Install an already-built `HostedPlugin` (e.g. a built-in signal-fx
    /// processor) into a drum-mixer channel/bus/master FX chain.
    pub fn install_mixer_plugin(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        plugin: signal_plugin_host::HostedPlugin,
    ) -> Result<usize, signal_plugin_host::PluginError> {
        let mut bank = self.bank().lock().map_err(|_| {
            signal_plugin_host::PluginError::LoadFailed("bank mutex poisoned".into())
        })?;
        bank.install_mixer_plugin(id, target, plugin)
    }

    pub fn load_preset_master_plugin(
        &self,
        id: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Option<usize>, signal_plugin_host::PluginError> {
        let plugin = match signal_plugin_host::HostedPlugin::load(path)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut bank = self.bank().lock().map_err(|_| {
            signal_plugin_host::PluginError::LoadFailed("bank mutex poisoned".into())
        })?;
        let slot = bank.install_preset_master_plugin(id, plugin)?;
        Ok(Some(slot))
    }

    pub fn remove_mixer_plugin(&self, id: &str, target: crate::mixer::FxTarget, slot_idx: usize) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.remove_mixer_plugin(id, target, slot_idx);
        }
    }

    pub fn remove_preset_master_plugin(&self, id: &str, slot_idx: usize) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.remove_preset_master_plugin(id, slot_idx);
        }
    }

    pub fn set_mixer_slot_bypass(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        bypassed: bool,
    ) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_mixer_slot_bypass(id, target, slot_idx, bypassed);
        }
    }

    pub fn set_mixer_slot_param(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        param_id: u32,
        value: f64,
    ) {
        if let Ok(bank) = self.bank().lock() {
            bank.set_mixer_slot_param(id, target, slot_idx, param_id, value);
        }
    }

    pub fn mixer_slot_params(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
    ) -> Option<Vec<signal_plugin_host::PluginParamInfo>> {
        self.bank()
            .lock()
            .ok()?
            .mixer_slot_params(id, target, slot_idx)
    }

    pub fn load_mixer_nam(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let path = path.as_ref().to_path_buf();
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| String::from("bank mutex poisoned"))?;
        bank.install_mixer_nam(id, target, &path)
    }

    pub fn load_preset_master_nam(
        &self,
        id: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let path = path.as_ref().to_path_buf();
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| String::from("bank mutex poisoned"))?;
        bank.install_preset_master_nam(id, &path)
    }

    pub fn set_mixer_nam_gain(
        &self,
        id: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        input: bool,
        gain_db: f32,
    ) {
        if let Ok(mut bank) = self.bank().lock() {
            bank.set_mixer_nam_gain(id, target, slot_idx, input, gain_db);
        }
    }

    // ── Stats / introspection (bank-backed; SamplerPlayer-equivalent) ─────────

    /// `(loaded, total)` background-preload progress for an instrument.
    pub fn preload_progress(&self, id: &str) -> (usize, usize) {
        self.bank()
            .try_lock()
            .map(|bank| bank.preload_progress(id))
            .unwrap_or((0, 0))
    }

    pub fn active_voices(&self, id: &str) -> usize {
        self.bank()
            .try_lock()
            .map(|bank| bank.active_voices(id))
            .unwrap_or(0)
    }

    pub fn stolen_voices(&self, id: &str) -> usize {
        self.bank()
            .try_lock()
            .map(|bank| bank.stolen_voices(id))
            .unwrap_or(0)
    }

    pub fn evict_cache_over_budget(&self) -> EvictStats {
        self.bank()
            .try_lock()
            .map(|bank| bank.evict_cache_over_budget())
            .unwrap_or_default()
    }

    pub fn reset_audio_stats(&self) {
        self.inner.stats.reset();
    }

    /// An [`AudioStatsSnapshot`] sourced from daw's render counters (callbacks,
    /// render-us, lock-misses) + the bank's cache / voice telemetry. Counters
    /// daw's renderer doesn't surface (stream errors, callback intervals,
    /// MIDI-to-callback latency) stay 0 — the daw engine owns them and they
    /// aren't exposed yet.
    pub fn audio_stats(&self) -> AudioStatsSnapshot {
        let s = &self.inner.stats;
        let mut snap = AudioStatsSnapshot {
            callbacks: s.callbacks.load(Ordering::Relaxed),
            lock_misses: s.lock_misses.load(Ordering::Relaxed),
            last_render_us: s.last_render_us.load(Ordering::Relaxed),
            max_render_us: s.max_render_us.load(Ordering::Relaxed),
            ..Default::default()
        };
        if let Ok(bank) = self.bank().try_lock() {
            snap.stolen_voices = bank.total_stolen_voices();
            snap.cache_misses = bank.total_cache_misses();
            snap.sample_misses = bank.total_sample_misses();
            snap.loaded_sample_bytes = bank.total_loaded_sample_bytes();
            snap.cache_budget_bytes = bank.cache_budget_bytes();
            snap.cache_over_budget_bytes = bank.cache_over_budget_bytes();
            snap.recent_cache_misses = bank.recent_cache_misses();
            snap.recent_sample_misses = bank.recent_sample_misses();
            snap.resize_events = bank.resize_events();
        }
        snap
    }

    // ── Offline rendering (bank-backed; SamplerPlayer-equivalent) ─────────────

    /// Render one offline stereo block. `output` is interleaved L/R and is
    /// cleared before rendering. Only available on an offline rig.
    pub fn render_offline(&self, output: &mut [f32]) -> eyre::Result<()> {
        if !self.is_offline() {
            return Err(eyre::eyre!(
                "render_offline is only available on SamplerRig::new_offline"
            ));
        }
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?;
        output.fill(0.0);
        bank.render(output);
        Ok(())
    }

    /// Offline render of a loaded preset into per-mic buses, keyed by mic id.
    /// Only available on an offline rig.
    pub fn render_offline_buses(
        &self,
        prefix: &str,
        block_frames: usize,
    ) -> eyre::Result<std::collections::BTreeMap<String, Vec<f32>>> {
        if !self.is_offline() {
            return Err(eyre::eyre!(
                "render_offline_buses is only available on SamplerRig::new_offline"
            ));
        }
        let mut bank = self
            .bank()
            .lock()
            .map_err(|_| eyre::eyre!("sampler bank lock poisoned"))?;
        bank.render_preset_buses(prefix, block_frames)
            .ok_or_else(|| eyre::eyre!("no preset loaded under '{prefix}'"))
    }

    // ── Per-instrument-track API (alternate "one engine = one daw track") ─────

    /// Total tracks in the per-track project (instruments + buses).
    fn track_count(tables: &TrackTables) -> usize {
        tables.instruments.len() + tables.buses.len()
    }

    /// Re-install the meter bank sized for the current per-track count
    /// (instruments + buses), plus the bank track at cell 0.
    fn resize_meters(&self, tables: &TrackTables) {
        let n = 1 + Self::track_count(tables);
        let meters = Meters::new(n);
        if let Some(daw) = self.inner.daw.as_ref() {
            daw.set_meters(meters.clone());
        }
        if let Ok(mut m) = self.inner.meters.lock() {
            *m = meters;
        }
    }

    fn add_instrument_track(
        &self,
        id: InstrumentId,
        name: &str,
        engine: SampleEngine,
        piece: Option<String>,
    ) -> eyre::Result<InstrumentTrack> {
        let daw = self
            .inner
            .daw
            .as_ref()
            .ok_or_else(|| eyre::eyre!("add_instrument requires a live (device-backed) rig"))?;
        let mut tables = self
            .inner
            .tracks
            .lock()
            .map_err(|_| eyre::eyre!("track table poisoned"))?;
        // Per-track tracks sit after the bank track (cell 0).
        let meter_index = 1 + Self::track_count(&tables);
        let track_guid = <Standalone as Tracks>::add(daw, ProjectContext::Current, name, None)
            .map_err(|e| eyre::eyre!("sampler rig: add track {name:?} failed: {e}"))?;

        let fx_ctx = FxChainContext::track(track_guid.clone());
        let idx = <Standalone as FxChains>::add(daw, fx_ctx.clone(), &format!("inst-{id}"))
            .map_err(|e| eyre::eyre!("sampler rig: reserve fx slot for {id:?} failed: {e}"))?;
        let fx_guid = <Standalone as FxChains>::get(daw, fx_ctx, idx)
            .map(|fx| fx.guid)
            .ok_or_else(|| eyre::eyre!("sampler rig: fx slot for {id:?} vanished after add"))?;

        // A first-class Sample Soundsource, hosted through the generic leaf.
        let mut inst = crate::SoundsourceLeaf::new(SamplerInstrument::new(engine));
        let _ = daw::plugin::PluginInstance::prepare(
            &mut inst,
            self.inner.sample_rate as f64,
            PREPARE_BLOCK,
        );
        daw.insert_plugin_instance(fx_guid.clone(), Box::new(inst));

        let track = InstrumentTrack {
            track_guid,
            fx_guid,
            meter_index,
            piece,
        };
        tables.instruments.insert(id.clone(), track.clone());
        tables.order.push(id);
        drop(tables);
        let tables = self.inner.tracks.lock().expect("track table poisoned");
        self.resize_meters(&tables);
        Ok(track)
    }

    /// Add a standalone sample instrument on its own daw track (per-track API).
    /// The engine must already be at this rig's [`sample_rate`](Self::sample_rate).
    pub fn add_instrument(&self, id: InstrumentId, engine: SampleEngine) -> eyre::Result<()> {
        {
            let tables = self
                .inner
                .tracks
                .lock()
                .map_err(|_| eyre::eyre!("track table poisoned"))?;
            if tables.instruments.contains_key(&id) {
                return Err(eyre::eyre!("sampler rig: instrument {id:?} already added"));
            }
        }
        let name = id.clone();
        self.add_instrument_track(id, &name, engine, None)?;
        Ok(())
    }

    fn add_bus_track(&self, bus_id: &str) -> eyre::Result<BusTrack> {
        let daw = self
            .inner
            .daw
            .as_ref()
            .ok_or_else(|| eyre::eyre!("bus tracks require a live rig"))?;
        let mut tables = self
            .inner
            .tracks
            .lock()
            .map_err(|_| eyre::eyre!("track table poisoned"))?;
        let meter_index = 1 + Self::track_count(&tables);
        let name = format!("{bus_id} (bus)");
        let track_guid = <Standalone as Tracks>::add(daw, ProjectContext::Current, &name, None)
            .map_err(|e| eyre::eyre!("sampler rig: add bus track {bus_id:?} failed: {e}"))?;
        let bus = BusTrack {
            id: bus_id.to_string(),
            track_guid,
            meter_index,
        };
        tables.buses.insert(bus_id.to_string(), bus.clone());
        drop(tables);
        let tables = self.inner.tracks.lock().expect("track table poisoned");
        self.resize_meters(&tables);
        Ok(bus)
    }

    fn route_to_bus(&self, source_track: &str, bus_track: &str) -> eyre::Result<()> {
        let daw = self
            .inner
            .daw
            .as_ref()
            .ok_or_else(|| eyre::eyre!("routing requires a live rig"))?;
        let ctx = ProjectContext::Current;
        let idx = <Standalone as Routing>::add_send(
            daw,
            ctx.clone(),
            TrackRef::guid(source_track),
            TrackRef::guid(bus_track),
        )
        .ok_or_else(|| eyre::eyre!("sampler rig: add_send {source_track} → {bus_track} failed"))?;
        <Standalone as Routing>::set_send_mode(
            daw,
            ctx.clone(),
            TrackRef::guid(source_track),
            RouteRef::Index(idx),
            SendMode::PostFx,
        )
        .map_err(|e| eyre::eyre!("sampler rig: set_send_mode failed: {e}"))?;
        <Standalone as Routing>::set_parent_send_enabled(
            daw,
            ctx,
            TrackRef::guid(source_track),
            false,
        )
        .map_err(|e| eyre::eyre!("sampler rig: disable parent send failed: {e}"))?;
        Ok(())
    }

    /// Build a per-track sampler rig from a [`MixerLayout`] + a per-strip
    /// engine map (per-track API). Each close mic → an instrument track direct
    /// to master; each bus mic → an instrument track sending into a shared bus.
    pub fn from_mixer_layout(
        prefs: &AudioIoPrefs,
        layout: &MixerLayout,
        mut engines: HashMap<(usize, String), SampleEngine>,
    ) -> eyre::Result<Self> {
        let rig = Self::open(prefs)?;

        for eng in &layout.engines {
            let piece = eng.label.clone();

            for ch in &eng.channels {
                let key = (eng.engine_idx, ch.mic_label.clone());
                let Some(engine) = engines.remove(&key) else {
                    continue;
                };
                let id = format!("{}:{}", eng.label, ch.mic_label);
                let name = format!("{} {}", eng.label, ch.mic_label);
                rig.add_instrument_track(id, &name, engine, Some(piece.clone()))?;
            }

            for snd in &eng.sends {
                let key = (eng.engine_idx, snd.mic_label.clone());
                let Some(engine) = engines.remove(&key) else {
                    continue;
                };

                let bus_id = if snd.bus_label.is_empty() {
                    snd.mic_label.clone()
                } else {
                    snd.bus_label.clone()
                };
                let bus_guid = {
                    let existing = rig
                        .inner
                        .tracks
                        .lock()
                        .ok()
                        .and_then(|t| t.buses.get(&bus_id).map(|b| b.track_guid.clone()));
                    match existing {
                        Some(g) => g,
                        None => rig.add_bus_track(&bus_id)?.track_guid,
                    }
                };

                let id = format!("{}:{}", eng.label, snd.mic_label);
                let name = format!("{} {}", eng.label, snd.mic_label);
                let track = rig.add_instrument_track(id, &name, engine, Some(piece.clone()))?;
                rig.route_to_bus(&track.track_guid, &bus_guid)?;
            }
        }

        Ok(rig)
    }

    /// Note-on to a per-track instrument (or piece) `id` — fans the MIDI to
    /// every mic track of a multi-mic piece. Per-track API; no-op if no such
    /// track (use [`note_on`](Self::note_on) for the bank-backed instrument).
    pub fn track_note_on(&self, id: &str, note: u8, vel: u8) {
        for guid in self.tracks_for(id) {
            if let Some(daw) = self.inner.daw.as_ref() {
                daw.push_note_on(&guid, note, vel);
            }
        }
    }

    pub fn track_note_off(&self, id: &str, note: u8) {
        for guid in self.tracks_for(id) {
            if let Some(daw) = self.inner.daw.as_ref() {
                daw.push_note_off(&guid, note);
            }
        }
    }

    pub fn track_cc(&self, id: &str, controller: u8, value: u8) {
        for guid in self.tracks_for(id) {
            if let Some(daw) = self.inner.daw.as_ref() {
                daw.push_cc(&guid, controller, value);
            }
        }
    }

    // ── Per-track kit hosting (the fully daw-based drum mixer) ───────────────

    /// The daw service handle (live mode) — the backend drives strip
    /// fader/mute/solo directly via the `Tracks` service by guid.
    pub fn daw_handle(&self) -> Option<Standalone> {
        self.inner.daw.clone()
    }

    /// Load a `.signalpreset` kit as per-track daw tracks: each piece's close
    /// mics become instrument tracks direct to master; bus mics become
    /// instrument tracks sending into shared bus tracks. Replaces any
    /// previously loaded per-track kit. Returns the piece ids
    /// (`"<prefix>:<engine id>"`), preset order.
    pub fn load_kit_tracks(
        &self,
        id_prefix: &str,
        preset: &crate::preset_spec::PresetSpec,
        preset_dir: &Path,
    ) -> eyre::Result<Vec<InstrumentId>> {
        use crate::kit_tracks::{KitMic, KitPiece, KitRouting, KitState};

        if self.inner.daw.is_none() {
            eyre::bail!("load_kit_tracks requires a live (device-backed) rig");
        }
        self.unload_kit_tracks()?;

        let sample_rate = self.inner.sample_rate;
        let mut pieces: Vec<KitPiece> = Vec::new();
        let mut piece_ids = Vec::new();
        let mut preload: Vec<(String, crate::engine::cache::SampleCache, Vec<PathBuf>, u8)> =
            Vec::new();

        for er in &preset.engines {
            let engine_path = crate::bank::resolve_relative(&er.engine, preset_dir);
            let engine_dir = engine_path.parent().unwrap_or(Path::new(""));
            let engine_spec = crate::engine_spec::EngineSpec::from_file(&engine_path)?;
            let pack_path = crate::bank::resolve_relative(&engine_spec.block.pack, engine_dir);
            let patch = crate::PlayerPatch::from_pack(&pack_path)?;
            let section = patch
                .spec
                .sections
                .first()
                .map(|s| s.id.clone())
                .unwrap_or_default();
            let mut mic_ids: Vec<String> =
                patch.spec.mics.iter().map(|m| m.id.clone()).collect();
            if mic_ids.is_empty() {
                mic_ids.push(String::new());
            }

            // Overrides: gain/pan land on the mic tracks (daw faders);
            // transpose folds into the piece's dispatch offset.
            let mut gain_db = 0.0f32;
            let mut pan = 0.0f32;
            let mut transpose = er.transpose as i16;
            for ov in engine_spec.block.overrides.iter().chain(er.overrides.iter()) {
                match ov.param.as_str() {
                    "gain_db" | "gain" => gain_db += ov.value,
                    "pan" => pan = ov.value,
                    "transpose" => transpose += ov.value.round() as i16,
                    other => {
                        tracing::warn!(piece = %er.id, param = other, "kit tracks: unmapped override ignored");
                    }
                }
            }

            let label = if engine_spec.name.is_empty() {
                er.id.clone()
            } else {
                engine_spec.name.clone()
            };
            let piece_id: InstrumentId = format!("{id_prefix}:{}", er.id);
            piece_ids.push(piece_id.clone());

            let mut mics = Vec::new();
            for mic_id in mic_ids {
                let mut engine =
                    SampleEngine::new(patch.clone(), sample_rate, section.clone(), mic_id.clone());
                // Restrict this engine to its mic's zones — one engine per
                // mic, RR-correlated (deterministic selection, identical
                // trigger streams).
                engine.set_solo_mic(Some(mic_id.clone()));
                if !er.articulation.is_empty() {
                    engine.pin_articulation(Some(er.articulation.clone()));
                }
                if !er.choke_group.is_empty() {
                    engine.set_choke_group(Some(&er.choke_group), &er.choke_on);
                }
                // Mic-filtered preload set (zone paths whose zone is this
                // mic's), so N mic engines cost the same RAM as one
                // multi-mic engine.
                let paths: Vec<PathBuf> = if patch.zone_paths.is_empty() {
                    engine.sample_paths_owned()
                } else {
                    patch
                        .spec
                        .zones
                        .iter()
                        .zip(patch.zone_paths.iter())
                        .filter(|(z, _)| z.mic == mic_id)
                        .map(|(_, p)| p.clone())
                        .collect()
                };
                let prio = PreloadProfile::default().engine_priority(&engine_spec.engine_type);
                preload.push((
                    format!("{piece_id}:{mic_id}"),
                    engine.cache_handle(),
                    paths,
                    prio,
                ));

                let shared: crate::kit_tracks::SharedEngine = Arc::new(Mutex::new(engine));
                let instrument_id = format!("{piece_id}:{mic_id}");
                let display = if mic_id.is_empty() {
                    label.clone()
                } else {
                    format!("{label} {mic_id}")
                };
                let bus = crate::mixer::mic_is_bus(&mic_id).then(|| mic_id.clone());
                let track =
                    self.add_kit_mic_track(&instrument_id, &display, shared.clone(), &er.id)?;
                if let Some(bus_id) = &bus {
                    let bus_guid = self.ensure_bus_track(bus_id)?;
                    self.route_to_bus(&track.track_guid, &bus_guid)?;
                }
                if let Some(daw) = self.inner.daw.as_ref() {
                    if gain_db != 0.0 {
                        let _ = <Standalone as Tracks>::set_volume(
                            daw,
                            ProjectContext::Current,
                            TrackRef::guid(&track.track_guid),
                            10f64.powf(gain_db as f64 / 20.0),
                        );
                    }
                    if pan != 0.0 {
                        let _ = <Standalone as Tracks>::set_pan(
                            daw,
                            ProjectContext::Current,
                            TrackRef::guid(&track.track_guid),
                            pan as f64,
                        );
                    }
                    if er.mute {
                        let _ = <Standalone as Tracks>::set_muted(
                            daw,
                            ProjectContext::Current,
                            TrackRef::guid(&track.track_guid),
                            true,
                        );
                    }
                }
                mics.push(KitMic {
                    mic: mic_id,
                    instrument_id,
                    track_guid: track.track_guid,
                    fx_guid: track.fx_guid,
                    meter_index: track.meter_index,
                    bus,
                    engine: shared,
                });
            }
            pieces.push(KitPiece {
                id: er.id.clone(),
                label,
                transpose: transpose.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
                muted: er.mute,
                mics,
            });
        }

        // Bus list for the snapshot.
        let buses = {
            let tables = self
                .inner
                .tracks
                .lock()
                .map_err(|_| eyre::eyre!("track table poisoned"))?;
            let mut buses: Vec<(String, String, usize)> = tables
                .buses
                .values()
                .map(|b| (b.id.clone(), b.track_guid.clone(), b.meter_index))
                .collect();
            buses.sort_by_key(|(_, _, m)| *m);
            buses
        };

        // Priority-ordered background preload (kick/snare first), one pass.
        preload.sort_by_key(|(_, _, _, p)| *p);
        let kit_label = preset.name.clone();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("signal-preload-kit:{kit_label}"))
            .spawn(move || {
                for (id, cache, paths, _) in preload {
                    let stats = cache.preload(paths.iter().map(|p| p.as_path()));
                    tracing::debug!(mic = %id, loaded = stats.loaded, skipped = stats.skipped, "kit mic preload done");
                }
                tracing::info!(kit = %kit_label, "kit preload complete");
            })
        {
            tracing::warn!(err = %err, "failed to spawn kit preload thread");
        }

        let state = KitState {
            prefix: id_prefix.to_string(),
            name: preset.name.clone(),
            routing: KitRouting::from_preset(preset),
            pieces,
            buses,
        };
        if let Ok(mut kit) = self.inner.kit.lock() {
            *kit = Some(state);
        }
        Ok(piece_ids)
    }

    /// Remove the current per-track kit's tracks (mics + buses), if any.
    pub fn unload_kit_tracks(&self) -> eyre::Result<()> {
        let Some(state) = self.inner.kit.lock().ok().and_then(|mut k| k.take()) else {
            return Ok(());
        };
        let Some(daw) = self.inner.daw.as_ref() else { return Ok(()) };
        let mut tables = self
            .inner
            .tracks
            .lock()
            .map_err(|_| eyre::eyre!("track table poisoned"))?;
        for piece in &state.pieces {
            for mic in &piece.mics {
                let _ = <Standalone as Tracks>::remove(
                    daw,
                    ProjectContext::Current,
                    TrackRef::guid(&mic.track_guid),
                );
                tables.instruments.remove(&mic.instrument_id);
                tables.order.retain(|id| id != &mic.instrument_id);
            }
        }
        for (bus_id, guid, _) in &state.buses {
            let _ = <Standalone as Tracks>::remove(
                daw,
                ProjectContext::Current,
                TrackRef::guid(guid),
            );
            tables.buses.remove(bus_id);
        }
        // Meter indices are compacted on the next load (resize_meters).
        drop(tables);
        let tables = self.inner.tracks.lock().expect("track table poisoned");
        self.resize_meters(&tables);
        Ok(())
    }

    fn add_kit_mic_track(
        &self,
        id: &str,
        name: &str,
        engine: crate::kit_tracks::SharedEngine,
        piece: &str,
    ) -> eyre::Result<InstrumentTrack> {
        let daw = self
            .inner
            .daw
            .as_ref()
            .ok_or_else(|| eyre::eyre!("kit tracks require a live rig"))?;
        let mut tables = self
            .inner
            .tracks
            .lock()
            .map_err(|_| eyre::eyre!("track table poisoned"))?;
        let meter_index = 1 + Self::track_count(&tables);
        let track_guid = <Standalone as Tracks>::add(daw, ProjectContext::Current, name, None)
            .map_err(|e| eyre::eyre!("sampler rig: add kit track {name:?} failed: {e}"))?;
        let fx_ctx = FxChainContext::track(track_guid.clone());
        let idx = <Standalone as FxChains>::add(daw, fx_ctx.clone(), &format!("kit-{id}"))
            .map_err(|e| eyre::eyre!("sampler rig: reserve kit fx slot failed: {e}"))?;
        let fx_guid = <Standalone as FxChains>::get(daw, fx_ctx, idx)
            .map(|fx| fx.guid)
            .ok_or_else(|| eyre::eyre!("sampler rig: kit fx slot vanished after add"))?;

        let mut inst = crate::kit_tracks::KitMicInstrument::new(engine);
        let _ = inst.prepare(self.inner.sample_rate as f64, PREPARE_BLOCK);
        daw.insert_plugin_instance(fx_guid.clone(), Box::new(inst));

        let track = InstrumentTrack {
            track_guid,
            fx_guid,
            meter_index,
            piece: Some(piece.to_string()),
        };
        tables.instruments.insert(id.to_string(), track.clone());
        tables.order.push(id.to_string());
        drop(tables);
        let tables = self.inner.tracks.lock().expect("track table poisoned");
        self.resize_meters(&tables);
        Ok(track)
    }

    fn ensure_bus_track(&self, bus_id: &str) -> eyre::Result<String> {
        let existing = self
            .inner
            .tracks
            .lock()
            .ok()
            .and_then(|t| t.buses.get(bus_id).map(|b| b.track_guid.clone()));
        match existing {
            Some(g) => Ok(g),
            None => Ok(self.add_bus_track(bus_id)?.track_guid),
        }
    }

    /// Dispatch one MIDI event through the loaded kit's routing table
    /// (direct engine calls — mic RR selection stays correlated).
    pub fn kit_dispatch(&self, ev: &midicore::MidiEvent) {
        if let Ok(kit) = self.inner.kit.lock() {
            if let Some(kit) = kit.as_ref() {
                kit.dispatch(ev);
            }
        }
    }

    /// Route one note through the loaded kit (UI pads / triggers).
    pub fn kit_note(&self, note: u8, velocity: u8) {
        if let Ok(kit) = self.inner.kit.lock() {
            if let Some(kit) = kit.as_ref() {
                kit.dispatch_note(note, velocity);
            }
        }
    }

    /// True when a per-track kit is loaded.
    pub fn kit_active(&self) -> bool {
        self.inner.kit.lock().map(|k| k.is_some()).unwrap_or(false)
    }

    /// Run `f` over the loaded kit state (strip guids, meter indices,
    /// engines) — the backend's read/inspect seam.
    pub fn with_kit<R>(&self, f: impl FnOnce(&crate::kit_tracks::KitState) -> R) -> Option<R> {
        self.inner
            .kit
            .lock()
            .ok()
            .and_then(|k| k.as_ref().map(f))
    }

    /// The current per-track meter bank (cell indices = the meter indices in
    /// the kit/instrument track tables).
    pub fn meters_bank(&self) -> Arc<Meters> {
        self.inner
            .meters
            .lock()
            .map(|m| m.clone())
            .unwrap_or_else(|_| Meters::new(0))
    }

    /// A kit piece's preload progress `(loaded, total)`, summed over its mic
    /// engines.
    pub fn kit_piece_progress(&self, piece: &str) -> (usize, usize) {
        self.with_kit(|kit| {
            let Some(p) = kit.piece(piece) else { return (0, 0) };
            let mut loaded = 0;
            let mut total = 0;
            for mic in &p.mics {
                if let Ok(e) = mic.engine.try_lock() {
                    loaded += e.loaded_sample_count();
                    total += e.total_sample_count();
                }
            }
            (loaded, total)
        })
        .unwrap_or((0, 0))
    }

    /// Active voices across the loaded kit's mic engines.
    pub fn kit_voices(&self) -> usize {
        self.with_kit(|kit| {
            kit.pieces
                .iter()
                .flat_map(|p| p.mics.iter())
                .filter_map(|m| m.engine.try_lock().ok().map(|e| e.active_voices()))
                .sum()
        })
        .unwrap_or(0)
    }

    /// Open a hardware MIDI input whose events are transformed then routed
    /// through the loaded kit's dispatch table (per-track kit analog of
    /// [`attach_midi_transformed`](Self::attach_midi_transformed)).
    pub fn attach_midi_kit<F>(
        &self,
        selection: midicore::PortSelector,
        transform: F,
    ) -> eyre::Result<midicore::midir::MidiInput>
    where
        F: FnMut(midicore::MidiEvent) -> Vec<midicore::MidiEvent> + Send + 'static,
    {
        let rig = self.clone();
        let sink = midicore::attach::tap_sink_transformed(
            self.inner.midi_monitor.clone(),
            transform,
            move |ev| rig.kit_dispatch(&ev),
        );
        midicore::midir::MidiInput::open(selection, sink)
    }

    fn tracks_for(&self, id: &str) -> Vec<String> {
        let Ok(tables) = self.inner.tracks.lock() else {
            return Vec::new();
        };
        if let Some(t) = tables.instruments.get(id) {
            return vec![t.track_guid.clone()];
        }
        tables
            .instruments
            .values()
            .filter(|t| t.piece.as_deref() == Some(id))
            .map(|t| t.track_guid.clone())
            .collect()
    }

    // ── Introspection / metering ──────────────────────────────────────────────

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    /// Number of per-track instruments (per-track API).
    pub fn instrument_count(&self) -> usize {
        self.inner
            .tracks
            .lock()
            .map(|t| t.instruments.len())
            .unwrap_or(0)
    }

    /// The per-track routing entry for an instrument id (per-track API).
    pub fn instrument(&self, id: &str) -> Option<InstrumentTrack> {
        self.inner.tracks.lock().ok()?.instruments.get(id).cloned()
    }

    /// Per-track bus tracks, keyed by bus id (per-track API).
    pub fn buses(&self) -> HashMap<String, BusTrack> {
        self.inner
            .tracks
            .lock()
            .map(|t| t.buses.clone())
            .unwrap_or_default()
    }

    fn meters(&self) -> Arc<Meters> {
        self.inner
            .meters
            .lock()
            .map(|m| m.clone())
            .unwrap_or_else(|_| Meters::new(0))
    }

    /// Per-track output peak (linear) for a per-track instrument `id`.
    pub fn instrument_peak(&self, id: &str) -> f32 {
        let meters = self.meters();
        self.inner
            .tracks
            .lock()
            .ok()
            .and_then(|t| t.instruments.get(id).map(|i| i.meter_index))
            .and_then(|i| meters.cell(i))
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }

    /// Per-track output peak (linear) for a per-track bus `id`.
    pub fn bus_peak(&self, id: &str) -> f32 {
        let meters = self.meters();
        self.inner
            .tracks
            .lock()
            .ok()
            .and_then(|t| t.buses.get(id).map(|b| b.meter_index))
            .and_then(|i| meters.cell(i))
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }

    /// Master / overall output peak (linear) — the loudest track cell.
    pub fn output_peak(&self) -> f32 {
        let meters = self.meters();
        let mut pk = 0.0f32;
        for i in 0..meters.len() {
            if let Some(c) = meters.cell(i) {
                pk = pk.max(c.peak(0)).max(c.peak(1));
            }
        }
        pk
    }

    pub fn output_peak_db(&self) -> f64 {
        linear_to_db(self.output_peak())
    }
}

/// Warm every pitch a document plays under EVERY articulation state the
/// document passes through — its CC58 keyswitch values and, when the spec
/// configures a latched-CC selector (UACC), the selector's values too.
///
/// Warming resolves zones from the engine's CURRENT articulation, so a
/// document that keyswitches (e.g. CC58 → staccato) would otherwise
/// cache-miss the switched-to zones into silence during the offline walk
/// (the walker never decodes on the render path). States are applied
/// in REVERSE chronological order so the engine is left in the document's
/// FIRST keyswitch state — the state any pre-keyswitch notes render under.
fn warm_document(
    bank: &mut crate::bank::SamplerBank,
    id: &str,
    doc: &crate::document::TrackDocument,
    spec: &crate::spec::LibrarySpec,
) {
    let mut pitches: Vec<u8> = doc.notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable();
    pitches.dedup();

    let mut ks_ccs = vec![58u8];
    if let Some(sel) = spec.latched_cc_selector() {
        ks_ccs.push(sel.cc);
    }
    // Chronological keyswitch states, deduped by (cc, val).
    let mut states: Vec<(u8, u8, u8)> = doc
        .ccs
        .iter()
        .filter(|c| ks_ccs.contains(&c.cc))
        .map(|c| (c.chan, c.cc, c.val))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    states.retain(|s| seen.insert((s.1, s.2)));

    if states.is_empty() {
        for &p in &pitches {
            let _ = bank.warm_note(id, p);
        }
        return;
    }
    for &(chan, cc, val) in states.iter().rev() {
        bank.cc_instrument_line(id, crate::document::line_for_chan(chan), cc, val);
        for &p in &pitches {
            let _ = bank.warm_note(id, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::mic_is_bus;

    /// A 2-piece kit (kick, snare), each with a close mic + an Overhead bus mic.
    fn kit_layout() -> MixerLayout {
        let mics = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let engine_mics = vec![
            ("kick".to_string(), mics(&["In 1", "Overhead"])),
            ("snare".to_string(), mics(&["In 1", "Overhead"])),
        ];
        crate::mixer::DrumMixer::build(&engine_mics, 48_000).layout()
    }

    #[test]
    fn classifies_close_and_bus_mics() {
        assert!(!mic_is_bus("In 1"));
        assert!(mic_is_bus("Overhead"));
    }

    #[test]
    fn layout_yields_expected_track_shape() {
        let layout = kit_layout();
        let total_channels: usize = layout.engines.iter().map(|e| e.channels.len()).sum();
        let total_sends: usize = layout.engines.iter().map(|e| e.sends.len()).sum();
        assert_eq!(total_channels, 2, "two close mics → two direct tracks");
        assert_eq!(total_sends, 2, "two overhead sends");
        assert_eq!(layout.buses.len(), 1, "one shared Overhead bus");
        let bus_labels: std::collections::HashSet<_> = layout
            .engines
            .iter()
            .flat_map(|e| e.sends.iter().map(|s| s.bus_label.clone()))
            .collect();
        assert_eq!(bus_labels.len(), 1);
        assert_eq!(bus_labels.into_iter().next().unwrap(), "Overhead");
    }

    // ── Offline (headless, no device) ─────────────────────────────────────────

    #[test]
    fn offline_rig_renders_silence_for_missing_instrument() {
        let rig = SamplerRig::new_offline(48_000);
        assert!(rig.is_offline());
        rig.note_on("missing", 60, 100);
        let mut block = vec![1.0f32; 128 * 2];
        rig.render_offline(&mut block).expect("offline render");
        assert!(
            block.iter().all(|s| *s == 0.0),
            "missing instrument → silence"
        );
    }

    #[test]
    fn offline_rig_is_cloneable_and_shares_bank() {
        let rig = SamplerRig::new_offline(48_000);
        let clone = rig.clone();
        // Both clones see the same (empty) bank — voice count is 0.
        assert_eq!(rig.active_voices("missing"), 0);
        assert_eq!(clone.active_voices("missing"), 0);
    }

    #[test]
    fn offline_render_buses_requires_preset() {
        let rig = SamplerRig::new_offline(48_000);
        assert!(rig.render_offline_buses("none", 128).is_err());
    }

    #[test]
    fn audio_stats_default_on_fresh_rig() {
        let rig = SamplerRig::new_offline(48_000);
        let s = rig.audio_stats();
        assert_eq!(s.callbacks, 0);
        assert_eq!(s.cache_misses, 0);
        rig.reset_audio_stats();
        assert_eq!(rig.audio_stats().callbacks, 0);
    }

    #[test]
    fn loading_a_zone_pack_plays_offline() {
        // Build a minimal zone instrument + render a note offline through the
        // bank — proves the bank-backed load + render path works headlessly.
        let dir = std::env::temp_dir().join(format!("signal-rig-offline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let wav = dir.join("note.wav");
        write_sine_wav(&wav, 48_000);
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");

        let rig = SamplerRig::new_offline(48_000);
        rig.load_instrument("piano", &spec_path, Some(&dir), "", "")
            .expect("load instrument");
        rig.preload_instrument("piano").expect("preload");
        rig.note_on("piano", 60, 110);

        let mut energy = 0.0f64;
        let mut block = vec![0.0f32; 512 * 2];
        for _ in 0..8 {
            rig.render_offline(&mut block).expect("render");
            for &s in &block {
                energy += (s as f64) * (s as f64);
            }
        }
        assert!(energy > 1e-6, "loaded zone instrument should produce audio");
        assert!(
            rig.active_voices("piano") > 0,
            "note-on should allocate a voice"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Device-backed tests need an audio backend; under `--features jack` they
    // need a running JACK/PipeWire server. Gated behind an env var.
    fn audio_available() -> bool {
        std::env::var_os("SIGNAL_SAMPLER_RIG_AUDIO").is_some()
    }

    fn write_sine_wav(path: &std::path::Path, frames: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("create wav");
        for i in 0..frames {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.8;
            w.write_sample(s).expect("write sample");
        }
        w.finalize().expect("finalize wav");
    }

    fn minimal_engine(dir: &std::path::Path) -> SampleEngine {
        let wav = dir.join("note.wav");
        write_sine_wav(&wav, 48_000);
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");
        let patch = crate::PlayerPatch::load(&spec_path, dir).expect("load patch");
        let mut engine = SampleEngine::new(patch, 48_000, "", "");
        engine.preload_samples();
        engine
    }

    #[test]
    fn open_loads_and_plays_through_bank_track() {
        if !audio_available() {
            eprintln!("skip: set SIGNAL_SAMPLER_RIG_AUDIO=1 with an audio backend to run");
            return;
        }
        let dir = std::env::temp_dir().join(format!("signal-rig-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let wav = dir.join("note.wav");
        write_sine_wav(&wav, 48_000);
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");

        let prefs = AudioIoPrefs {
            sample_rate: 48_000,
            buffer_size: 256,
            ..Default::default()
        };
        let rig = SamplerRig::open(&prefs).expect("open rig");
        assert!(!rig.is_offline());
        rig.load_instrument("piano", &spec_path, Some(&dir), "", "")
            .expect("load");
        rig.preload_instrument("piano").expect("preload");
        rig.note_on("piano", 60, 110);
        // Let the renderer run a few blocks, then check stats moved.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(rig.audio_stats().callbacks > 0, "renderer must have run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_instrument_and_note_on_reaches_track() {
        if !audio_available() {
            eprintln!("skip: set SIGNAL_SAMPLER_RIG_AUDIO=1 with an audio backend to run");
            return;
        }
        let dir = std::env::temp_dir().join(format!("signal-sampler-rig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let prefs = AudioIoPrefs {
            sample_rate: 48_000,
            buffer_size: 256,
            ..Default::default()
        };
        let rig = SamplerRig::open(&prefs).expect("open rig");
        rig.add_instrument("piano".into(), minimal_engine(&dir))
            .expect("add instrument");
        assert_eq!(rig.instrument_count(), 1);
        assert!(rig.instrument("piano").is_some());
        rig.track_note_on("piano", 60, 100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_mixer_layout_builds_tracks_buses_and_sends() {
        if !audio_available() {
            eprintln!("skip: set SIGNAL_SAMPLER_RIG_AUDIO=1 with an audio backend to run");
            return;
        }
        let dir = std::env::temp_dir().join(format!("signal-sampler-mix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let layout = kit_layout();
        let prefs = AudioIoPrefs {
            sample_rate: 48_000,
            buffer_size: 256,
            ..Default::default()
        };

        let mut engines: HashMap<(usize, String), SampleEngine> = HashMap::new();
        for eng in &layout.engines {
            for ch in &eng.channels {
                engines.insert((eng.engine_idx, ch.mic_label.clone()), minimal_engine(&dir));
            }
            for snd in &eng.sends {
                engines.insert(
                    (eng.engine_idx, snd.mic_label.clone()),
                    minimal_engine(&dir),
                );
            }
        }

        let rig = SamplerRig::from_mixer_layout(&prefs, &layout, engines).expect("build rig");
        assert_eq!(rig.instrument_count(), 4);
        assert_eq!(rig.buses().len(), 1);
        assert!(rig.buses().contains_key("Overhead"));

        let bus_guid = rig.buses()["Overhead"].track_guid.clone();
        let daw = rig.inner.daw.as_ref().unwrap();
        for (id, t) in rig.inner.tracks.lock().unwrap().instruments.iter() {
            if id.ends_with(":Overhead") {
                let sends = <Standalone as Routing>::sends(
                    daw,
                    ProjectContext::Current,
                    TrackRef::guid(&t.track_guid),
                );
                assert!(
                    sends
                        .iter()
                        .any(|s| s.dest_track_guid.as_deref() == Some(bus_guid.as_str())),
                    "overhead mic {id} must send to the Overhead bus"
                );
                assert!(
                    !<Standalone as Routing>::parent_send_enabled(
                        daw,
                        ProjectContext::Current,
                        TrackRef::guid(&t.track_guid)
                    ),
                    "overhead mic {id} parent send must be disabled"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
