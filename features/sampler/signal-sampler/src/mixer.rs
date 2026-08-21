//! Drum-kit mixer overlay for a [`PresetRuntime`](crate::runtime::PresetRuntime).
//!
//! Models the send-based routing of a real drum console:
//!
//! - **Close mics** (e.g. `In 1`, `In 2`) of each piece go **direct** to the
//!   master — one [`DirectChannel`] each (fader + mute + solo + meter).
//! - **Overhead / Room** mics of each piece do NOT go out the close track;
//!   they are **sent** (with a per-piece level) to a shared bus track — one
//!   [`Send`] each (level + mute + solo + meter).
//! - Each shared **bus** (`Overhead`, `Room Close`, …) sums every piece's send
//!   and goes to master — one [`Bus`] each (fader + mute + solo + meter).
//! - A single **master** fader + mute scales the whole kit, with a master meter.
//!
//! The render loop lives on `PresetRuntime` (which owns the engines' per-mic
//! scratch buffers); this module holds the routing config, the bus + master
//! accumulators, and the lock-free peak meters.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use signal_plugin_host::HostedPlugin;

use crate::convolver::Convolver;
#[cfg(not(target_arch = "wasm32"))]
use crate::nam::NamProcessor;

/// Per-block peak-meter decay (peak-hold). Applied once per audio block so the
/// UI sees a smooth fall even when it polls slowly.
const METER_DECAY: f32 = 0.85;

/// Max block size (in frames) we prepare hosted plugins for at install time.
/// The cpal callback's actual block is normally 256–1024 frames; preparing
/// for 8192 keeps us safe against larger upstream buffers + future variable-
/// block-size paths without re-preparing per block.
pub const FX_PREPARE_BLOCK: u32 = 8192;

// ── FX chain (REAPER-style track FX) ────────────────────────────────────────

/// One slot's DSP backend — what's actually filling the slot. Maps 1:1 to
/// [`signal_proto::block_kind::BlockKind`]'s non-Native variants. Adding a
/// new backend (e.g. built-in convolution reverb) is a new variant here +
/// a matching `BlockKind` arm.
pub enum FxBackend {
    /// Third-party CLAP / VST3 plugin loaded via `signal-plugin-host`.
    Hosted(HostedPlugin),
    /// Built-in Neural Amp Modeler — works for any block role that wants
    /// neural-net amp/pedal modeling (Amp, Drive, Cabinet, …).
    /// Native-only: the NAM C++ core doesn't build for wasm32.
    #[cfg(not(target_arch = "wasm32"))]
    Nam(NamProcessor),
    /// Built-in cabinet impulse-response convolution (a `BlockKind::Native`
    /// realization of a Cabinet block). Mono FIR; see [`crate::convolver`].
    CabIr(Convolver),
}

impl std::fmt::Debug for FxBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hosted(h) => f.debug_tuple("Hosted").field(h).finish(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => f.debug_tuple("Nam").field(n).finish(),
            Self::CabIr(c) => f.debug_tuple("CabIr").field(c).finish(),
        }
    }
}

impl FxBackend {
    /// Short tag for UI labels / log lines — matches `BlockKind::tag()`.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Hosted(_) => "plugin",
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(_) => "nam",
            Self::CabIr(_) => "cabir",
        }
    }

    /// Cached display name (plugin descriptor, NAM filename, or IR filename).
    pub fn display_name(&self) -> &str {
        match self {
            Self::Hosted(h) => &h.descriptor().name,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => &n.display_name,
            Self::CabIr(c) => &c.display_name,
        }
    }

    /// Render one interleaved-stereo block in place. Each backend handles
    /// its own buffer-shape conversion (NAM does stereo↔mono internally).
    pub fn process_interleaved(&mut self, inout: &mut [f32]) {
        match self {
            Self::Hosted(h) => {
                let _ = h.process_interleaved(inout, &[], &[]);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => n.process_interleaved(inout),
            Self::CabIr(c) => c.process_interleaved(inout),
        }
    }

    /// Release any audio-thread resources before the slot is dropped.
    pub fn deactivate(&mut self) {
        if let Self::Hosted(h) = self {
            h.deactivate();
        }
        // NamProcessor releases its model in Drop.
    }
}

/// One slot in a track-style FX chain — a [`FxBackend`] instance plus a
/// per-slot bypass and a cached display name for the UI. The backend is
/// owned by the slot and deactivated when the slot is removed.
pub struct FxSlot {
    pub backend: FxBackend,
    pub bypassed: bool,
    /// Cached display name — read once at install time so the UI doesn't
    /// have to lock the audio thread to label the slot.
    pub display_name: String,
}

impl std::fmt::Debug for FxSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxSlot")
            .field("display_name", &self.display_name)
            .field("bypassed", &self.bypassed)
            .field("backend", &self.backend)
            .finish()
    }
}

/// A serial FX chain that processes its slots in order, in place on an
/// interleaved-stereo buffer. Modeled after a REAPER track FX chain: each
/// slot is one [`FxBackend`], bypassable per-slot. An empty chain is a
/// no-op.
#[derive(Debug, Default)]
pub struct FxChain {
    pub slots: Vec<FxSlot>,
}

impl FxChain {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Process `inout` (interleaved stereo) through each non-bypassed slot
    /// in order. Errors / failures inside a backend are swallowed — the UI
    /// surface reports load/prepare failures separately; the audio thread
    /// must never panic on a misbehaving plugin.
    pub fn process(&mut self, inout: &mut [f32]) {
        for slot in &mut self.slots {
            if slot.bypassed {
                continue;
            }
            slot.backend.process_interleaved(inout);
        }
    }
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A drum mic id routes to a shared **bus** (overhead / room) rather than
/// straight out a close-mic channel when its name looks like an overhead or
/// room/ambient mic. Everything else is a direct close mic.
pub fn mic_is_bus(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    l == "oh"
        || l.contains("overhead")
        || l.starts_with("room")
        || l.contains("ambien")
        || l.contains("ambc")
}

/// Lock-free peak meters: the audio thread stores per-block peaks; the UI reads
/// them via a cloned `Arc` without touching the bank lock. Values are linear
/// peak amplitude (~0..1+); the UI converts to dB.
#[derive(Debug)]
pub struct MixerMeters {
    pieces: Vec<AtomicU32>,
    channels: Vec<AtomicU32>,
    sends: Vec<AtomicU32>,
    buses: Vec<AtomicU32>,
    master: AtomicU32,
}

impl MixerMeters {
    fn new(pieces: usize, channels: usize, sends: usize, buses: usize) -> Self {
        let mk = |n| (0..n).map(|_| AtomicU32::new(0)).collect();
        Self {
            pieces: mk(pieces),
            channels: mk(channels),
            sends: mk(sends),
            buses: mk(buses),
            master: AtomicU32::new(0),
        }
    }

    fn store_peak(slot: &AtomicU32, block_peak: f32) {
        let prev = f32::from_bits(slot.load(Ordering::Relaxed));
        let held = (prev * METER_DECAY).max(block_peak);
        slot.store(held.to_bits(), Ordering::Relaxed);
    }

    fn read(slot: &AtomicU32) -> f32 {
        f32::from_bits(slot.load(Ordering::Relaxed))
    }

    /// Current peak (linear) of piece (engine) `i`.
    pub fn piece_peak(&self, i: usize) -> f32 {
        self.pieces.get(i).map(Self::read).unwrap_or(0.0)
    }
    /// Current peak (linear) of direct channel `i`.
    pub fn channel_peak(&self, i: usize) -> f32 {
        self.channels.get(i).map(Self::read).unwrap_or(0.0)
    }
    /// Current peak (linear) of send `i`.
    pub fn send_peak(&self, i: usize) -> f32 {
        self.sends.get(i).map(Self::read).unwrap_or(0.0)
    }
    /// Current peak (linear) of bus `i`.
    pub fn bus_peak(&self, i: usize) -> f32 {
        self.buses.get(i).map(Self::read).unwrap_or(0.0)
    }
    /// Current peak (linear) of the master output.
    pub fn master_peak(&self) -> f32 {
        Self::read(&self.master)
    }

    // Peak setters used by the audio render (via the `meters` field, so they
    // don't borrow the whole mixer while its scratch buffers are borrowed mut).
    pub(crate) fn set_channel_peak(&self, i: usize, peak: f32) {
        if let Some(s) = self.channels.get(i) {
            Self::store_peak(s, peak);
        }
    }
    pub(crate) fn set_send_peak(&self, i: usize, peak: f32) {
        if let Some(s) = self.sends.get(i) {
            Self::store_peak(s, peak);
        }
    }
    pub(crate) fn set_bus_peak(&self, i: usize, peak: f32) {
        if let Some(s) = self.buses.get(i) {
            Self::store_peak(s, peak);
        }
    }
    pub(crate) fn set_piece_peak(&self, i: usize, peak: f32) {
        if let Some(s) = self.pieces.get(i) {
            Self::store_peak(s, peak);
        }
    }
    pub(crate) fn set_master_peak(&self, peak: f32) {
        Self::store_peak(&self.master, peak);
    }
}

/// A close-mic channel that goes direct to master.
#[derive(Debug)]
pub struct DirectChannel {
    pub engine_idx: usize,
    pub mic_idx: usize,
    pub engine_label: String,
    pub mic_label: String,
    pub gain_db: f32,
    pub gain_lin: f32,
    pub muted: bool,
    pub soloed: bool,
    /// Pre-fader FX chain (REAPER convention): mic source → FX → gain → master.
    pub fx: FxChain,
}

/// A bus-mic send from one piece into a shared bus.
#[derive(Debug)]
pub struct Send {
    pub engine_idx: usize,
    pub mic_idx: usize,
    pub bus_idx: usize,
    pub engine_label: String,
    pub mic_label: String,
    pub level_db: f32,
    pub level_lin: f32,
    pub muted: bool,
    pub soloed: bool,
}

/// A shared bus track (sum of all pieces' sends of one mic id) → master.
#[derive(Debug)]
pub struct Bus {
    pub id: String,
    pub label: String,
    pub gain_db: f32,
    pub gain_lin: f32,
    pub muted: bool,
    pub soloed: bool,
    /// Pre-fader FX chain: bus sum → FX → bus gain → master.
    pub fx: FxChain,
    /// Interleaved-stereo accumulator, re-sized to the current block.
    pub(crate) acc: Vec<f32>,
}

/// One kit *piece* (an engine: kick, snare, …) — the primary mixer surface.
/// A piece owns all of an engine's close-mic channels **and** its bus sends, so
/// its gain/mute/solo apply uniformly across every mic of that drum regardless
/// of routing (the piece fader is a true per-drum level). Indexed by
/// `engine_idx` (its position in this vec).
#[derive(Debug)]
pub struct Piece {
    pub engine_idx: usize,
    pub label: String,
    pub gain_db: f32,
    pub gain_lin: f32,
    pub muted: bool,
    pub soloed: bool,
}

/// Routing config + bus/master accumulators + meters for a drum preset.
#[derive(Debug)]
pub struct DrumMixer {
    /// Per-engine piece strips (kick, snare, …) — the primary control surface.
    pub pieces: Vec<Piece>,
    pub channels: Vec<DirectChannel>,
    pub sends: Vec<Send>,
    pub buses: Vec<Bus>,
    pub master_gain_db: f32,
    pub master_gain_lin: f32,
    pub master_muted: bool,
    /// FX chain on the master sum (master_scratch → FX → master gain → output).
    pub master_fx: FxChain,
    pub meters: Arc<MixerMeters>,
    /// Sample rate this mixer is running at — passed at construction so
    /// `install_*_plugin` can `prepare` the hosted plugin without the audio
    /// thread blocking on a UI-side lookup.
    pub(crate) sample_rate: u32,
    /// Scratch the whole kit mix is summed into (so master gain + meter apply
    /// to this preset's output alone, then it's added to the host buffer).
    pub(crate) master_scratch: Vec<f32>,
    /// Per-block scratch used to run each direct channel's FX chain in
    /// isolation before mixing into `master_scratch`. Re-sized to the
    /// current block size on first render of each block.
    pub(crate) channel_scratch: Vec<f32>,
    /// Per-piece block-peak accumulator (`pieces.len()`), so a piece meter is
    /// the max over all its channels' + sends' contributions in the block.
    pub(crate) piece_peak_scratch: Vec<f32>,
}

impl DrumMixer {
    /// Build a mixer from each engine's `(label, mic_ids)` at the host's
    /// `sample_rate`. Close mics become direct channels; overhead/room mics
    /// become sends into shared buses (one bus per distinct bus-mic id, in
    /// first-seen order). `sample_rate` is stored so installed FX slots can
    /// be `prepare`d without a callback to the audio backend.
    pub fn build(engine_mics: &[(String, Vec<String>)], sample_rate: u32) -> Self {
        let mut channels = Vec::new();
        let mut sends = Vec::new();
        let mut buses: Vec<Bus> = Vec::new();
        let mut bus_idx_of: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        let mut pieces = Vec::with_capacity(engine_mics.len());
        for (engine_idx, (engine_label, mics)) in engine_mics.iter().enumerate() {
            pieces.push(Piece {
                engine_idx,
                label: engine_label.clone(),
                gain_db: 0.0,
                gain_lin: 1.0,
                muted: false,
                soloed: false,
            });
            for (mic_idx, mic_id) in mics.iter().enumerate() {
                if mic_is_bus(mic_id) {
                    let bus_idx = *bus_idx_of.entry(mic_id.clone()).or_insert_with(|| {
                        buses.push(Bus {
                            id: mic_id.clone(),
                            label: mic_id.clone(),
                            gain_db: 0.0,
                            gain_lin: 1.0,
                            muted: false,
                            soloed: false,
                            fx: FxChain::default(),
                            acc: Vec::new(),
                        });
                        buses.len() - 1
                    });
                    sends.push(Send {
                        engine_idx,
                        mic_idx,
                        bus_idx,
                        engine_label: engine_label.clone(),
                        mic_label: mic_id.clone(),
                        level_db: 0.0,
                        level_lin: 1.0,
                        muted: false,
                        soloed: false,
                    });
                } else {
                    channels.push(DirectChannel {
                        engine_idx,
                        mic_idx,
                        engine_label: engine_label.clone(),
                        mic_label: mic_id.clone(),
                        gain_db: 0.0,
                        gain_lin: 1.0,
                        muted: false,
                        soloed: false,
                        fx: FxChain::default(),
                    });
                }
            }
        }

        let meters =
            Arc::new(MixerMeters::new(pieces.len(), channels.len(), sends.len(), buses.len()));
        Self {
            pieces,
            channels,
            sends,
            buses,
            master_gain_db: 0.0,
            master_gain_lin: 1.0,
            master_muted: false,
            master_fx: FxChain::default(),
            meters,
            sample_rate,
            master_scratch: Vec::new(),
            channel_scratch: Vec::new(),
            piece_peak_scratch: Vec::new(),
        }
    }

    /// True if there is anything to route (avoids engaging on empty presets).
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty() && self.sends.is_empty()
    }

    /// Any piece / channel / send / bus currently soloed → solo-in-place active.
    pub(crate) fn any_solo(&self) -> bool {
        self.pieces.iter().any(|p| p.soloed)
            || self.channels.iter().any(|c| c.soloed)
            || self.sends.iter().any(|s| s.soloed)
            || self.buses.iter().any(|b| b.soloed)
    }

    pub fn set_piece_gain_db(&mut self, i: usize, db: f32) {
        if let Some(p) = self.pieces.get_mut(i) {
            p.gain_db = db;
            p.gain_lin = db_to_lin(db);
        }
    }
    pub fn set_piece_mute(&mut self, i: usize, muted: bool) {
        if let Some(p) = self.pieces.get_mut(i) {
            p.muted = muted;
        }
    }
    pub fn set_piece_solo(&mut self, i: usize, soloed: bool) {
        if let Some(p) = self.pieces.get_mut(i) {
            p.soloed = soloed;
        }
    }

    pub fn set_channel_gain_db(&mut self, i: usize, db: f32) {
        if let Some(ch) = self.channels.get_mut(i) {
            ch.gain_db = db;
            ch.gain_lin = db_to_lin(db);
        }
    }
    pub fn set_channel_mute(&mut self, i: usize, muted: bool) {
        if let Some(ch) = self.channels.get_mut(i) {
            ch.muted = muted;
        }
    }
    pub fn set_channel_solo(&mut self, i: usize, soloed: bool) {
        if let Some(ch) = self.channels.get_mut(i) {
            ch.soloed = soloed;
        }
    }
    pub fn set_send_level_db(&mut self, i: usize, db: f32) {
        if let Some(s) = self.sends.get_mut(i) {
            s.level_db = db;
            s.level_lin = db_to_lin(db);
        }
    }
    pub fn set_send_mute(&mut self, i: usize, muted: bool) {
        if let Some(s) = self.sends.get_mut(i) {
            s.muted = muted;
        }
    }
    pub fn set_send_solo(&mut self, i: usize, soloed: bool) {
        if let Some(s) = self.sends.get_mut(i) {
            s.soloed = soloed;
        }
    }
    pub fn set_bus_gain_db(&mut self, i: usize, db: f32) {
        if let Some(b) = self.buses.get_mut(i) {
            b.gain_db = db;
            b.gain_lin = db_to_lin(db);
        }
    }
    pub fn set_bus_mute(&mut self, i: usize, muted: bool) {
        if let Some(b) = self.buses.get_mut(i) {
            b.muted = muted;
        }
    }
    pub fn set_bus_solo(&mut self, i: usize, soloed: bool) {
        if let Some(b) = self.buses.get_mut(i) {
            b.soloed = soloed;
        }
    }
    pub fn set_master_gain_db(&mut self, db: f32) {
        self.master_gain_db = db;
        self.master_gain_lin = db_to_lin(db);
    }
    pub fn set_master_mute(&mut self, muted: bool) {
        self.master_muted = muted;
    }

    // ── FX-chain mutation ───────────────────────────────────────────────

    /// Install a hosted CLAP/VST3 plugin at the tail of an FX chain.
    /// Prepares the plugin for this mixer's sample rate + a generous max
    /// block size, then appends. Returns the new slot index, or the
    /// underlying plugin error.
    pub fn install_plugin(
        &mut self,
        target: FxTarget,
        mut plugin: HostedPlugin,
    ) -> Result<usize, signal_plugin_host::PluginError> {
        plugin.prepare(self.sample_rate as f64, FX_PREPARE_BLOCK)?;
        let display_name = plugin.descriptor().name.clone();
        self.push_slot(target, FxBackend::Hosted(plugin), display_name)
            .map_err(signal_plugin_host::PluginError::LoadFailed)
    }

    /// Install a Neural Amp Modeler at the tail of an FX chain. Loads the
    /// `.nam` model file from disk and prepares it for this mixer's
    /// sample rate. The load is fast (file IO + on-stack network parse);
    /// no further activation step is needed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install_nam(
        &mut self,
        target: FxTarget,
        model_path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let nam = NamProcessor::load(
            model_path,
            self.sample_rate as f64,
            FX_PREPARE_BLOCK as usize,
        )?;
        let display_name = nam.display_name.clone();
        self.push_slot(target, FxBackend::Nam(nam), display_name)
    }

    fn push_slot(
        &mut self,
        target: FxTarget,
        backend: FxBackend,
        display_name: String,
    ) -> Result<usize, String> {
        let chain = match self.chain_mut(target) {
            Some(c) => c,
            None => return Err("FX target not found".into()),
        };
        chain.slots.push(FxSlot {
            backend,
            bypassed: false,
            display_name,
        });
        Ok(chain.slots.len() - 1)
    }

    /// Remove a slot and deactivate the backend it held.
    pub fn remove_plugin(&mut self, target: FxTarget, slot_idx: usize) {
        if let Some(chain) = self.chain_mut(target) {
            if slot_idx < chain.slots.len() {
                let mut slot = chain.slots.remove(slot_idx);
                slot.backend.deactivate();
            }
        }
    }

    pub fn set_slot_bypass(&mut self, target: FxTarget, slot_idx: usize, bypassed: bool) {
        if let Some(chain) = self.chain_mut(target) {
            if let Some(slot) = chain.slots.get_mut(slot_idx) {
                slot.bypassed = bypassed;
            }
        }
    }

    /// Queue a param write to a hosted plugin (no-op for non-Hosted
    /// backends — NAM slots use `set_nam_gain` instead).
    pub fn set_slot_param(&self, target: FxTarget, slot_idx: usize, param_id: u32, value: f64) {
        if let Some(chain) = self.chain_ref(target) {
            if let Some(slot) = chain.slots.get(slot_idx) {
                if let FxBackend::Hosted(h) = &slot.backend {
                    h.set_param(param_id, value);
                }
            }
        }
    }

    /// Set NAM input / output gain (dB) for the slot. No-op for non-NAM
    /// backends. `which = true` ⇒ input gain, `false` ⇒ output gain.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_nam_gain(&mut self, target: FxTarget, slot_idx: usize, input: bool, gain_db: f32) {
        if let Some(chain) = self.chain_mut(target) {
            if let Some(slot) = chain.slots.get_mut(slot_idx) {
                if let FxBackend::Nam(n) = &mut slot.backend {
                    if input {
                        n.input_gain_db = gain_db;
                    } else {
                        n.output_gain_db = gain_db;
                    }
                }
            }
        }
    }

    /// Snapshot a hosted plugin's parameter list (for slider grids).
    /// `&mut self` because the underlying `PluginInstance::params` takes
    /// `&mut self`. NAM slots return `None` — they expose input/output
    /// gain trims through `set_nam_gain` instead.
    pub fn slot_params(
        &mut self,
        target: FxTarget,
        slot_idx: usize,
    ) -> Option<Vec<signal_plugin_host::PluginParamInfo>> {
        let chain = self.chain_mut(target)?;
        let slot = chain.slots.get_mut(slot_idx)?;
        if let FxBackend::Hosted(h) = &mut slot.backend {
            Some(h.params())
        } else {
            None
        }
    }

    fn chain_mut(&mut self, target: FxTarget) -> Option<&mut FxChain> {
        match target {
            FxTarget::Channel(i) => self.channels.get_mut(i).map(|c| &mut c.fx),
            FxTarget::Bus(i) => self.buses.get_mut(i).map(|b| &mut b.fx),
            FxTarget::Master => Some(&mut self.master_fx),
        }
    }

    fn chain_ref(&self, target: FxTarget) -> Option<&FxChain> {
        match target {
            FxTarget::Channel(i) => self.channels.get(i).map(|c| &c.fx),
            FxTarget::Bus(i) => self.buses.get(i).map(|b| &b.fx),
            FxTarget::Master => Some(&self.master_fx),
        }
    }
}

/// Addresses an FX chain on a drum mixer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxTarget {
    Channel(usize),
    Bus(usize),
    Master,
}

// ── Cloneable layout snapshot for the UI ────────────────────────────────────

/// A flat, cloneable description of the mixer for rendering. Meter indices line
/// up with [`MixerMeters`] (`channel_idx`/`send_idx`/`bus_idx`); setters on
/// the player take the same indices.
#[derive(Clone, Debug, Default)]
pub struct MixerLayout {
    pub engines: Vec<EngineStrip>,
    pub buses: Vec<BusStrip>,
    pub master_gain_db: f32,
    pub master_muted: bool,
    pub master_fx: Vec<FxSlotStrip>,
}

#[derive(Clone, Debug)]
pub struct EngineStrip {
    pub engine_idx: usize,
    pub label: String,
    /// Piece-level (whole-drum) fader/mute/solo — applies across all this
    /// engine's channels + sends. Addressed by `engine_idx`.
    pub piece_gain_db: f32,
    pub piece_muted: bool,
    pub piece_soloed: bool,
    pub channels: Vec<ChannelStrip>,
    pub sends: Vec<SendStrip>,
}

#[derive(Clone, Debug)]
pub struct ChannelStrip {
    pub channel_idx: usize,
    pub mic_label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    pub fx: Vec<FxSlotStrip>,
}

#[derive(Clone, Debug)]
pub struct SendStrip {
    pub send_idx: usize,
    pub mic_label: String,
    pub bus_idx: usize,
    pub bus_label: String,
    pub level_db: f32,
    pub muted: bool,
    pub soloed: bool,
}

#[derive(Clone, Debug)]
pub struct BusStrip {
    pub bus_idx: usize,
    pub label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    pub fx: Vec<FxSlotStrip>,
}

/// UI snapshot of one FX-chain slot. `slot_idx` matches the index taken by
/// `SamplerBank::set_mixer_slot_bypass` / `set_mixer_slot_param`.
#[derive(Clone, Debug)]
pub struct FxSlotStrip {
    pub slot_idx: usize,
    pub display_name: String,
    pub bypassed: bool,
}

impl FxChain {
    /// Snapshot the chain for the UI.
    pub fn strip(&self) -> Vec<FxSlotStrip> {
        self.slots
            .iter()
            .enumerate()
            .map(|(slot_idx, s)| FxSlotStrip {
                slot_idx,
                display_name: s.display_name.clone(),
                bypassed: s.bypassed,
            })
            .collect()
    }
}

impl DrumMixer {
    /// Snapshot the current structure + fader/mute/solo state, grouped by engine.
    pub fn layout(&self) -> MixerLayout {
        let mut engines: Vec<EngineStrip> = Vec::new();
        let mut idx_of_engine: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let pieces = &self.pieces;
        let engine_slot = |engines: &mut Vec<EngineStrip>,
                           idx_of: &mut std::collections::HashMap<usize, usize>,
                           engine_idx: usize,
                           label: &str|
         -> usize {
            *idx_of.entry(engine_idx).or_insert_with(|| {
                let p = pieces.get(engine_idx);
                engines.push(EngineStrip {
                    engine_idx,
                    label: label.to_string(),
                    piece_gain_db: p.map(|p| p.gain_db).unwrap_or(0.0),
                    piece_muted: p.map(|p| p.muted).unwrap_or(false),
                    piece_soloed: p.map(|p| p.soloed).unwrap_or(false),
                    channels: Vec::new(),
                    sends: Vec::new(),
                });
                engines.len() - 1
            })
        };

        for (channel_idx, ch) in self.channels.iter().enumerate() {
            let slot = engine_slot(
                &mut engines,
                &mut idx_of_engine,
                ch.engine_idx,
                &ch.engine_label,
            );
            engines[slot].channels.push(ChannelStrip {
                channel_idx,
                mic_label: ch.mic_label.clone(),
                gain_db: ch.gain_db,
                muted: ch.muted,
                soloed: ch.soloed,
                fx: ch.fx.strip(),
            });
        }
        for (send_idx, s) in self.sends.iter().enumerate() {
            let slot = engine_slot(
                &mut engines,
                &mut idx_of_engine,
                s.engine_idx,
                &s.engine_label,
            );
            let bus_label = self
                .buses
                .get(s.bus_idx)
                .map(|b| b.label.clone())
                .unwrap_or_default();
            engines[slot].sends.push(SendStrip {
                send_idx,
                mic_label: s.mic_label.clone(),
                bus_idx: s.bus_idx,
                bus_label,
                level_db: s.level_db,
                muted: s.muted,
                soloed: s.soloed,
            });
        }

        let buses = self
            .buses
            .iter()
            .enumerate()
            .map(|(bus_idx, b)| BusStrip {
                bus_idx,
                label: b.label.clone(),
                gain_db: b.gain_db,
                muted: b.muted,
                soloed: b.soloed,
                fx: b.fx.strip(),
            })
            .collect();

        MixerLayout {
            engines,
            buses,
            master_gain_db: self.master_gain_db,
            master_muted: self.master_muted,
            master_fx: self.master_fx.strip(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm2_like() -> Vec<(String, Vec<String>)> {
        let mics = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        vec![
            (
                "kick".into(),
                mics(&["In 1", "In 2", "Overhead", "Room Close", "Room Far"]),
            ),
            (
                "snare".into(),
                mics(&["In 1", "In 2", "Overhead", "Room Close", "Room Far"]),
            ),
        ]
    }

    #[test]
    fn classifies_close_vs_bus() {
        assert!(!mic_is_bus("In 1"));
        assert!(!mic_is_bus("Top"));
        assert!(mic_is_bus("Overhead"));
        assert!(mic_is_bus("OH"));
        assert!(mic_is_bus("Room Close"));
        assert!(mic_is_bus("Room Far"));
        assert!(mic_is_bus("Room Mono"));
    }

    #[test]
    fn builds_direct_channels_sends_and_shared_buses() {
        let m = DrumMixer::build(&mm2_like(), 48_000);
        assert_eq!(m.channels.len(), 4);
        assert_eq!(m.sends.len(), 6);
        assert_eq!(m.buses.len(), 3);
        assert_eq!(m.buses[0].label, "Overhead");
        let oh_sends: Vec<_> = m
            .sends
            .iter()
            .filter(|s| s.mic_label == "Overhead")
            .collect();
        assert_eq!(oh_sends.len(), 2);
        assert_eq!(oh_sends[0].bus_idx, oh_sends[1].bus_idx);
    }

    #[test]
    fn layout_groups_by_engine() {
        let m = DrumMixer::build(&mm2_like(), 48_000);
        let layout = m.layout();
        assert_eq!(layout.engines.len(), 2);
        assert_eq!(layout.engines[0].label, "kick");
        assert_eq!(layout.engines[0].channels.len(), 2);
        assert_eq!(layout.engines[0].sends.len(), 3);
        assert_eq!(layout.buses.len(), 3);
    }

    #[test]
    fn setters_update_gain_mute_solo_master() {
        let mut m = DrumMixer::build(&mm2_like(), 48_000);
        m.set_channel_gain_db(0, -6.0);
        assert!((m.channels[0].gain_lin - db_to_lin(-6.0)).abs() < 1e-6);
        m.set_send_mute(0, true);
        assert!(m.sends[0].muted);
        m.set_bus_solo(0, true);
        assert!(m.buses[0].soloed);
        assert!(m.any_solo());
        m.set_bus_solo(0, false);
        assert!(!m.any_solo());
        m.set_master_gain_db(3.0);
        assert!((m.master_gain_lin - db_to_lin(3.0)).abs() < 1e-6);
    }
}
