//! Live guitar-rig audio path — the standalone amp-modeler rig, running ON
//! daw's realtime audio engine.
//!
//! The rig is **one input-armed track in a tiny daw project**: a single track
//! whose record input is a hardware channel (`RecordInput::Audio { channel }`),
//! whose FX chain is the active patch's chain ([NAM amp, cab IR, optional hosted
//! CLAP/VST3]), routed to master. signal builds and drives this project;
//! daw's `AudioEngine` opens the output device + a live input stream and, every
//! block, mixes the armed input channel into the track's bus, runs its FX chain,
//! and outputs the result. daw's engine carries no rig concept — it is a pure
//! realtime processor; the project/track/slot/meter wiring below is signal's own
//! assembly out of daw's project + FX primitives + that engine.
//!
//! ## Instant, GigPerformer-style patch switching
//!
//! Each FX block is a [`PluginInstance`]: [`NamProcessor`] / [`Convolver`]
//! (native) implement it directly; hosted plugins go through daw's own loader.
//! daw's renderer pulls the boxed instance backing each fx-guid out of
//! `Standalone`'s `plugin_instances` map **per block, under a lock**. So
//! swapping a *pre-prepared* box in under that lock
//! ([`Standalone::insert_plugin_instance`]) is glitch-free.
//!
//! The track reserves a fixed number of fx slots up front (constant guids, so
//! the project's `fx_chain` never changes → the renderer never rebuilds its
//! snapshot). A "chain" install pre-builds + prepares its boxes and stores them
//! control-side. [`set_active`](GuitarRig::set_active) swaps the active chain's
//! boxes into the slot guids (and identity pass-throughs into the unused slots)
//! — a handful of map inserts under the renderer's lock. The displaced old boxes
//! are dropped off the audio thread.
//!
//! ## Metering
//!
//! daw's renderer writes one post-fader peak per track into a [`Meters`] bank;
//! that is the **output** meter. The first reserved fx slot is an
//! `InputProbe` pass-through that records the **input** peak it sees
//! (post-input-trim, pre-amp) into a shared atomic.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use signal_proto::block::{BlockCategory, BlockType};

use facet::Facet;

use daw::service::{
    FxChainContext, FxChains, FxParams, ProjectContext, ProjectInfo, RecordInput, TrackRef, Tracks,
};
// The rig's realtime engine. On Linux with the `pipewire` feature it's the
// native duplex `pw_filter` engine (one callback, no ring, low latency); the
// cpal `AudioEngine` is the fallback. Both share `with_project_prefs` +
// `sample_rate`, so the open path below is engine-agnostic.
use daw::standalone::Standalone;
#[cfg(not(all(feature = "pipewire", target_os = "linux")))]
use daw::standalone::audio_engine::AudioEngine as RigEngine;
#[cfg(all(feature = "pipewire", target_os = "linux"))]
use daw::standalone::audio_engine::DuplexAudioEngine as RigEngine;
use daw::standalone::metering::{Meters, linear_to_db};
use daw::standalone::transport_engine::{PlayStateRepr, TransportShared};
use daw_audio_io::duplex::EngineStats;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::convolver::Convolver;
use crate::mixer::FX_PREPARE_BLOCK;
use crate::nam::NamProcessor;
use crate::rig_prefs::RigAudioPrefs;

/// Max block size the rig prepares models / plugins for. daw's callback block is
/// normally 64–1024 frames; preparing for [`FX_PREPARE_BLOCK`] keeps us safe
/// against larger backend buffers without per-block re-preparation.
const MAX_BLOCK: usize = FX_PREPARE_BLOCK as usize;

/// Fixed number of FX slots reserved on the rig track. Slot 0 is the
/// `InputProbe` (input meter); slots `1..=MAX_CHAIN_SLOTS` carry the active
/// chain's blocks (identity pass-throughs fill unused ones). Reserving a
/// constant count keeps the project's `fx_chain` (guids) immutable, so patch
/// switches never rebuild the renderer's snapshot — the swap is pure box-insert.
const MAX_CHAIN_SLOTS: usize = 24;

/// Identifies a chain resident control-side. Assigned on install; opaque
/// elsewhere.
pub type ModelId = u32;

fn db_to_lin(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// How a block's [`BlockType`] is **implemented** — the realization axis
/// (orthogonal to the semantic `block_type`). Each block type can have several
/// implementations; a [`RigBlock`] picks one by which asset it carries.
///
/// Mirrors `signal_proto::block_kind::BlockKind` and the runtime
/// [`FxBackend`](crate::mixer::FxBackend).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum BlockImpl {
    /// Signal's built-in DSP for this block type — our own "DSP plugin". The
    /// default implementation. (Not yet written for most types; see
    /// [`RigBlock::has_backend`].)
    Native,
    /// A Neural Amp Modeler `.nam` model — the special implementation for
    /// nonlinear/amp-shaped blocks (Amp, Drive, even a neural Cabinet).
    Nam,
    /// A cabinet impulse response convolved by the built-in convolver.
    Ir,
    /// An external CLAP / VST3 plugin loaded for this block (e.g. a third-party
    /// delay or reverb).
    Plugin,
    /// A sample library played by the built-in [`SampleEngine`](crate::SampleEngine)
    /// — the implementation for `Sampler` blocks (keys/piano layers, drum kits,
    /// orchestral instruments). The block carries the library spec path.
    Sample,
}

impl BlockImpl {
    /// Whether this implementation is valid for `block_type`. The realization
    /// axis is constrained by the semantic axis — not every backend fits every
    /// block:
    /// - **Nam** only fits amp-shaped (nonlinear) blocks: `Amp`, `Drive`,
    ///   `Saturator`. A NAM model can't be a delay or a reverb.
    /// - **Ir** only fits a `Cabinet` (impulse-response convolution).
    /// - **Plugin** and **Native** fit any block type (an external plugin or our
    ///   built-in DSP can implement anything).
    pub fn supports(self, block_type: BlockType) -> bool {
        match self {
            BlockImpl::Native | BlockImpl::Plugin => true,
            BlockImpl::Nam => matches!(
                block_type,
                BlockType::Amp | BlockType::Drive | BlockType::Saturator
            ),
            BlockImpl::Ir => block_type == BlockType::Cabinet,
            BlockImpl::Sample => block_type == BlockType::Sampler,
        }
    }

    /// The implementations valid for `block_type` (for UI pickers). `Native`
    /// always leads (the default), then the type-specific specials, then
    /// `Plugin`.
    pub fn allowed_for(block_type: BlockType) -> Vec<BlockImpl> {
        [
            BlockImpl::Native,
            BlockImpl::Nam,
            BlockImpl::Ir,
            BlockImpl::Sample,
            BlockImpl::Plugin,
        ]
        .into_iter()
        .filter(|i| i.supports(block_type))
        .collect()
    }

    /// Short identifier for logs / UI.
    pub const fn tag(self) -> &'static str {
        match self {
            BlockImpl::Native => "native",
            BlockImpl::Nam => "nam",
            BlockImpl::Ir => "ir",
            BlockImpl::Plugin => "plugin",
            BlockImpl::Sample => "sample",
        }
    }
}

/// One block in a patch's FX chain, modeled on the two orthogonal axes from
/// `DOMAIN.md`:
///
/// - **`block_type`** — *what the block does* (proto [`BlockType`]: `Amp`,
///   `Cabinet`, `Drive`, `Reverb`, `Delay`, `Pitch`, …). This is the block's
///   identity in the module system.
/// - **implementation** — *how that type is realized* ([`BlockImpl`]), chosen by
///   which asset field is set: [`nam`](Self::nam) (a NAM model), [`ir`](Self::ir)
///   (a cabinet impulse response), [`plugin`](Self::plugin) (a hosted CLAP/VST3).
///   No asset ⇒ [`Native`](BlockImpl::Native) — Signal's built-in DSP for that
///   type. Native DSP isn't written yet, so a Native block has no backend
///   ([`has_backend`](Self::has_backend) = false) and is skipped at install
///   while still occupying the chain (structure + bypass grouping).
///
/// So an amp is just `{ block_type @Amp, nam "…/amp.nam" }` — the NAM model is
/// the amp block's implementation, not a special block "kind"; a `{ block_type
/// @Delay, plugin "Echo.clap" }` is a Delay realized by an external plugin. The
/// global time-bypass acts on every block whose type is in
/// [`BlockCategory::Time`] (`Delay` / `Reverb` / `Freeze`).
#[derive(Clone, Debug, Facet)]
pub struct RigBlock {
    /// What the block is — its semantic type (Amp, Cabinet, Reverb, Delay, …).
    pub block_type: BlockType,
    /// Realization: path to a `.nam` model. Set ⇒ realized by Neural Amp
    /// Modeler. (Works for any nonlinear block — amp, drive, even a neural cab.)
    #[facet(default)]
    pub nam: String,
    /// Realization: path to a cabinet impulse response `.wav` (convolved).
    #[facet(default)]
    pub ir: String,
    /// Realization: path to a hosted CLAP / VST3 plugin (format auto-detected).
    #[facet(default)]
    pub plugin: String,
    /// For a `plugin` realization: optional base64 state chunk restored on load.
    #[facet(default)]
    pub state_b64: Option<String>,
    /// Realization: path to a sample-library spec (`library.styx`). Set ⇒ the
    /// block is a sample-playback instrument driven by the built-in
    /// [`SampleEngine`](crate::SampleEngine) — keys/piano layers, drum kits,
    /// orchestral sections.
    #[facet(default)]
    pub sample: String,
    /// Samples root dir for `sample` (WAV/zone paths resolve relative to it).
    /// Empty ⇒ the spec file's parent directory.
    #[facet(default)]
    pub samples_root: String,
    /// Section id inside the library (e.g. `"1v"`). Empty ⇒ the spec's first
    /// section (fine for single-instrument libraries).
    #[facet(default)]
    pub sample_section: String,
    /// Mic position id (e.g. `"Mix"`). Empty ⇒ the spec's first mic.
    #[facet(default)]
    pub sample_mic: String,
    /// Per-block parameter values applied at build time (`(name, value)`
    /// matching the backend's parameter names; normalized values as decimal
    /// strings — e.g. `("cutoff", "0.42")`). Imported presets set these.
    #[facet(default)]
    pub params: Vec<crate::rig_node::Param>,
    /// Display name (e.g. "Big Hall", "Dotted Delay"). Falls back to the asset
    /// file stem, then the block type. Useful for placeholder blocks.
    #[facet(default)]
    pub name: String,
    /// Optional explicit module grouping (e.g. "Time"). Empty = grouped by the
    /// block type's category. The time-bypass hits this module or the Time
    /// category.
    #[facet(default)]
    pub module: String,
    /// Initial bypass state when the chain is installed.
    #[facet(default)]
    pub bypassed: bool,
    /// Per-block input trim (dB) before this block. NAM only.
    #[facet(default)]
    pub input_trim_db: f32,
    /// Per-block output trim (dB) after this block. NAM only.
    #[facet(default)]
    pub output_trim_db: f32,
}

impl RigBlock {
    /// An **Amp** block realized by a NAM model — the common case.
    pub fn nam(model_path: impl Into<String>) -> Self {
        Self::of_type(BlockType::Amp).with_nam(model_path)
    }

    /// A **Cabinet** block realized by an impulse response.
    pub fn cab_ir(ir_path: impl Into<String>) -> Self {
        Self::of_type(BlockType::Cabinet).with_ir(ir_path)
    }

    /// A hosted CLAP / VST3 plugin block (no saved state). Untyped — defaults to
    /// `Custom`; prefer [`effect`](Self::effect) to give it a real type.
    pub fn plugin(path: impl Into<String>) -> Self {
        Self::of_type(BlockType::Custom).with_plugin(path)
    }

    /// A hosted plugin block that restores a saved state chunk.
    pub fn plugin_with_state(path: impl Into<String>, state_b64: Option<String>) -> Self {
        Self {
            state_b64,
            ..Self::of_type(BlockType::Custom).with_plugin(path)
        }
    }

    /// A typed effect placeholder: a block with a real `block_type` and `name`
    /// but no realization yet (drop a plugin/model in later). Kept in the chain
    /// for structure + bypass grouping; skipped at install.
    pub fn effect(block_type: BlockType, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::of_type(block_type)
        }
    }

    /// A bare block of `block_type` with no realization (a placeholder).
    pub fn of_type(block_type: BlockType) -> Self {
        Self {
            block_type,
            nam: String::new(),
            ir: String::new(),
            plugin: String::new(),
            state_b64: None,
            sample: String::new(),
            samples_root: String::new(),
            sample_section: String::new(),
            sample_mic: String::new(),
            params: Vec::new(),
            name: String::new(),
            module: String::new(),
            bypassed: false,
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }

    /// A **Sampler** block realized by a sample library spec — the keys/piano,
    /// drum-kit and orchestral instrument case.
    pub fn sample_lib(spec_path: impl Into<String>) -> Self {
        Self::of_type(BlockType::Sampler).with_sample(spec_path)
    }

    #[must_use]
    pub fn with_nam(mut self, path: impl Into<String>) -> Self {
        self.nam = path.into();
        self
    }

    #[must_use]
    pub fn with_ir(mut self, path: impl Into<String>) -> Self {
        self.ir = path.into();
        self
    }

    #[must_use]
    pub fn with_plugin(mut self, path: impl Into<String>) -> Self {
        self.plugin = path.into();
        self
    }

    #[must_use]
    pub fn with_sample(mut self, spec_path: impl Into<String>) -> Self {
        self.sample = spec_path.into();
        self
    }

    #[must_use]
    pub fn with_samples_root(mut self, root: impl Into<String>) -> Self {
        self.samples_root = root.into();
        self
    }

    #[must_use]
    pub fn with_sample_section(mut self, section: impl Into<String>) -> Self {
        self.sample_section = section.into();
        self
    }

    #[must_use]
    pub fn with_sample_mic(mut self, mic: impl Into<String>) -> Self {
        self.sample_mic = mic.into();
        self
    }

    /// Set one build-time parameter value (see the `params` field).
    #[must_use]
    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push(crate::rig_node::Param {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// A build-time parameter's raw string value, if present.
    pub fn param_str(&self, name: &str) -> Option<String> {
        self.params.iter().find(|p| p.name == name).map(|p| p.value.clone())
    }

    /// A build-time parameter as `f32`, if present and numeric.
    pub fn param_f32(&self, name: &str) -> Option<f32> {
        self.params
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.value.trim().parse().ok())
    }

    #[must_use]
    pub fn with_module(mut self, module: impl Into<String>) -> Self {
        self.module = module.into();
        self
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The realization's asset path — the `.nam`, `.wav` IR, sample-library
    /// spec, or plugin path, whichever is set (empty for a placeholder).
    pub fn asset_path(&self) -> &str {
        if !self.nam.is_empty() {
            &self.nam
        } else if !self.ir.is_empty() {
            &self.ir
        } else if !self.sample.is_empty() {
            &self.sample
        } else {
            &self.plugin
        }
    }

    /// Which **implementation** (backend) realizes this block's type. Derived
    /// from which asset is set; no asset ⇒ [`BlockImpl::Native`] (our built-in
    /// DSP). See [`BlockImpl`].
    pub fn implementation(&self) -> BlockImpl {
        if !self.nam.trim().is_empty() {
            BlockImpl::Nam
        } else if !self.ir.trim().is_empty() {
            BlockImpl::Ir
        } else if !self.sample.trim().is_empty() {
            BlockImpl::Sample
        } else if !self.plugin.trim().is_empty() {
            BlockImpl::Plugin
        } else {
            BlockImpl::Native
        }
    }

    /// True when this block is realized by the built-in [`Native`](BlockImpl::Native)
    /// DSP for its type (no NAM / IR / plugin asset chosen).
    pub fn is_native(&self) -> bool {
        self.implementation() == BlockImpl::Native
    }

    /// Whether the rig can build an audio backend for this block **today**. NAM,
    /// IR and Plugin implementations are always buildable; a `Native` block is
    /// buildable only for the block types whose built-in DSP is implemented (see
    /// [`native_dsp_available`]) — the rest are placeholders until their DSP lands.
    pub fn has_backend(&self) -> bool {
        match self.implementation() {
            BlockImpl::Native => native_dsp_available(self.block_type),
            _ => true,
        }
    }

    /// Check the block's implementation is valid for its type (e.g. a NAM model
    /// can't realize a `Delay`). Returns a human-readable error otherwise.
    pub fn validate(&self) -> Result<(), String> {
        let imp = self.implementation();
        if imp.supports(self.block_type) {
            Ok(())
        } else {
            Err(format!(
                "a {} implementation is not valid for a {:?} block (allowed: {:?})",
                imp.tag(),
                self.block_type,
                BlockImpl::allowed_for(self.block_type)
                    .iter()
                    .map(|i| i.tag())
                    .collect::<Vec<_>>(),
            ))
        }
    }

    /// Display label: explicit `name`, else the asset file stem, else the block
    /// type's display name.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        std::path::Path::new(self.asset_path())
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.block_type.display_name().to_string())
    }

    /// Whether the global "time bypass" footswitch should hit this block: its
    /// type is a Time effect (Delay / Reverb / Freeze) or it's tagged into a
    /// "Time" module. Deliberately narrow — drive, pitch (POG) and modulation
    /// are NOT killed by the time switch (Funk = clean with Time bypassed).
    pub fn is_time_fx(&self) -> bool {
        self.module.eq_ignore_ascii_case("time")
            || self.block_type.category() == BlockCategory::Time
    }

    pub fn is_nam(&self) -> bool {
        !self.nam.is_empty()
    }

    pub fn is_cab_ir(&self) -> bool {
        !self.ir.is_empty()
    }

    pub fn is_plugin(&self) -> bool {
        !self.plugin.is_empty()
    }

    pub fn is_sample(&self) -> bool {
        !self.sample.is_empty()
    }
}

/// Control-side description of a chain resident in the rig, for the UI.
#[derive(Clone, Debug)]
pub struct SlotInfo {
    pub id: ModelId,
    /// Chain summary, e.g. `"Drive → AC30 → V30 (cab)"`.
    pub display_name: String,
    /// Per-block display names, in order.
    pub blocks: Vec<String>,
    /// Loudness (LUFS) of the chain's first NAM block — the measured value from
    /// the DI calibration pass when available, else declared metadata. Used to
    /// level-match patches to a common target loudness.
    pub primary_loudness: Option<f64>,
    /// Expected sample rate of the chain's first NAM block, if declared.
    pub primary_expected_sr: Option<f64>,
    /// Captured analog input level (dBu) of the chain's first NAM block, if
    /// declared — drives input-staging calibration.
    pub primary_input_level_dbu: Option<f64>,
    /// Captured analog output level (dBu) of the chain's first NAM block.
    pub primary_output_level_dbu: Option<f64>,
}

/// An enumerated audio device — name, channel count, native sample rate.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub channels: u16,
    pub default_sample_rate: u32,
}

// ── Shared meter state written by audio-thread plugin instances ──────────────

/// Shared atomic the `InputProbe` writes the per-block input peak into, read
/// by the UI input meter. Held by both the rig (reader) and the probe instance
/// (audio-thread writer).
#[derive(Debug, Default)]
struct InputMeterShared {
    /// Latest block input peak (linear), as `f32` bits.
    peak: AtomicU32,
    /// Per-channel peaks (linear, `f32` bits) — stereo metering.
    peak_l: AtomicU32,
    peak_r: AtomicU32,
    /// When set, the probe feeds silence into the chain (tuner engaged):
    /// meters + tuner window still see the instrument, trails ring out.
    input_muted: AtomicBool,
    /// Most-recent mono input samples (L+R averaged), newest-last, for the
    /// tuner's pitch detection. Written once per block by the probe; read
    /// (cloned) by the control thread. Capacity is fixed at `TUNER_WINDOW`.
    samples: std::sync::Mutex<Vec<f32>>,
    /// DI capture: while armed (`capture_remaining > 0`) the probe appends
    /// every input sample here — the source for a *real* calibration DI
    /// recorded from the player's own guitar.
    capture: std::sync::Mutex<Vec<f32>>,
    /// Samples still to capture (0 = disarmed). Relaxed atomics — the probe
    /// is the only writer of `capture` while armed.
    capture_remaining: std::sync::atomic::AtomicUsize,
}

/// Mono window the tuner runs autocorrelation over. At 48 kHz this covers
/// ~85 ms — long enough to resolve a low-E (~82 Hz, ~12 ms period) several
/// times over.
const TUNER_WINDOW: usize = 4096;

impl InputMeterShared {
    fn store(&self, peak: f32) {
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
    }

    fn store_lr(&self, l: f32, r: f32) {
        self.peak_l.store(l.to_bits(), Ordering::Relaxed);
        self.peak_r.store(r.to_bits(), Ordering::Relaxed);
    }

    fn load_lr(&self) -> (f32, f32) {
        (
            f32::from_bits(self.peak_l.load(Ordering::Relaxed)),
            f32::from_bits(self.peak_r.load(Ordering::Relaxed)),
        )
    }
    fn load(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }
    /// Push this block's mono samples into the rolling tuner window.
    fn push_samples(&self, mono: &[f32]) {
        if let Ok(mut buf) = self.samples.try_lock() {
            buf.extend_from_slice(mono);
            let len = buf.len();
            if len > TUNER_WINDOW {
                buf.drain(0..len - TUNER_WINDOW);
            }
        }
    }
    /// Snapshot the current tuner window (cloned for the control thread).
    fn snapshot_samples(&self) -> Vec<f32> {
        self.samples.lock().map(|b| b.clone()).unwrap_or_default()
    }

    fn arm_capture(&self, samples: usize) {
        if let Ok(mut buf) = self.capture.lock() {
            buf.clear();
            buf.reserve(samples);
        }
        self.capture_remaining
            .store(samples, std::sync::atomic::Ordering::Relaxed);
    }

    fn push_capture(&self, mono: &[f32]) {
        let remaining = self
            .capture_remaining
            .load(std::sync::atomic::Ordering::Relaxed);
        if remaining == 0 {
            return;
        }
        let take = remaining.min(mono.len());
        if let Ok(mut buf) = self.capture.try_lock() {
            buf.extend_from_slice(&mono[..take]);
        }
        self.capture_remaining
            .store(remaining - take, std::sync::atomic::Ordering::Relaxed);
    }

    fn capture_state(&self) -> (usize, usize) {
        let remaining = self
            .capture_remaining
            .load(std::sync::atomic::Ordering::Relaxed);
        let len = self.capture.lock().map(|b| b.len()).unwrap_or(0);
        (len, remaining)
    }

    fn take_capture(&self) -> Vec<f32> {
        self.capture
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default()
    }
}

/// A unity pass-through [`PluginInstance`] that records the input peak it sees.
/// Sits at slot 0 of the rig track so the UI gets an input meter (daw's renderer
/// only meters the post-fader *output* per track). Pure copy + max — cheap.
/// Load a mono f32 loop from a wav for the fake-DI debug input
/// (`SIGNAL_FAKE_DI=/path/to.wav`) — screenshots and demos with the meters
/// alive, no instrument plugged in.
fn load_fake_di() -> Option<Vec<f32>> {
    let path = std::env::var("SIGNAL_FAKE_DI").ok().filter(|p| !p.is_empty())?;
    let mut reader = hound::WavReader::open(&path)
        .map_err(|e| tracing::warn!("fake DI: {path}: {e}"))
        .ok()?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 * scale).collect()
        }
    };
    let ch = spec.channels.max(1) as usize;
    let mono: Vec<f32> = samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    (!mono.is_empty()).then(|| {
        tracing::info!("fake DI active: {path} ({:.1}s loop)", mono.len() as f32 / 48_000.0);
        mono
    })
}

struct InputProbe {
    shared: Arc<InputMeterShared>,
    /// Fake-DI loop + cursor (debug input; None in normal operation).
    fake: Option<Vec<f32>>,
    fake_pos: usize,
    prepared: bool,
    /// Per-block mono scratch (reused) for the tuner window push.
    mono: Vec<f32>,
}

impl InputProbe {
    fn new(shared: Arc<InputMeterShared>) -> Self {
        Self {
            shared,
            prepared: true,
            mono: Vec::with_capacity(MAX_BLOCK),
            fake: load_fake_di(),
            fake_pos: 0,
        }
    }
}

impl PluginInstance for InputProbe {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.rig.input_probe".into(),
            name: "Input".into(),
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
    fn prepare(&mut self, _sr: f64, _bs: u32) -> Result<(), PluginError> {
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        let muted = self.shared.input_muted.load(Ordering::Relaxed);
        let (mut pk_l, mut pk_r) = (0.0f32, 0.0f32);
        // Reused mono scratch for the tuner window push (off the steady-state
        // alloc path after the first block).
        self.mono.clear();
        self.mono.reserve(frames);
        for i in 0..frames {
            // Fake-DI debug loop replaces the live input when armed.
            let (src_l, src_r) = match &self.fake {
                Some(w) => {
                    let v = w[self.fake_pos];
                    self.fake_pos = (self.fake_pos + 1) % w.len();
                    (v, v)
                }
                None => (in_l[i], in_r[i]),
            };
            // Muted: the chain gets silence (trails keep ringing) while
            // the meters and the tuner still see the instrument.
            out_l[i] = if muted { 0.0 } else { src_l };
            out_r[i] = if muted { 0.0 } else { src_r };
            pk_l = pk_l.max(src_l.abs());
            pk_r = pk_r.max(src_r.abs());
            self.mono.push((src_l + src_r) * 0.5);
        }
        self.shared.store(pk_l.max(pk_r));
        self.shared.store_lr(pk_l, pk_r);
        // Feed the global DI sidechain — the gate keys off the clean guitar
        // even though it sits post-amp in the chain.
        signal_fx::sidechain::set_peak(pk_l.max(pk_r));
        self.shared.push_samples(&self.mono);
        self.shared.push_capture(&self.mono);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// A unity pass-through [`PluginInstance`] used to fill the rig track's unused
/// FX slots (chains shorter than [`MAX_CHAIN_SLOTS`]). Copies input → output.
struct Identity {
    prepared: bool,
}

impl Identity {
    fn new() -> Self {
        Self { prepared: true }
    }
}

impl PluginInstance for Identity {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.rig.identity".into(),
            name: "—".into(),
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
    fn prepare(&mut self, _sr: f64, _bs: u32) -> Result<(), PluginError> {
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        out_l[..frames].copy_from_slice(&in_l[..frames]);
        out_r[..frames].copy_from_slice(&in_r[..frames]);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// Block types whose built-in `Native` DSP is implemented today — delegated
/// to the [`native` registry](crate::native::native_dsp_available), the one
/// place a new block type's DSP is declared.
pub fn native_dsp_available(block_type: BlockType) -> bool {
    crate::native::native_dsp_available(block_type)
}

/// A built, prepared chain block plus the level metadata the rig needs to
/// level-match and input-stage it. Non-NAM blocks leave the NAM-only fields
/// `None`.
pub(crate) struct BuiltBlock {
    pub boxed: Box<dyn PluginInstance>,
    pub display_name: String,
    /// Loudness (LUFS) used for level-matching: the **measured** value from the
    /// DI calibration pass when available, else the model's declared metadata.
    pub loudness: Option<f64>,
    pub expected_sr: Option<f64>,
    /// Captured analog input level (dBu) for input-staging calibration.
    pub input_level_dbu: Option<f64>,
    /// Captured analog output level (dBu). Informational.
    pub output_level_dbu: Option<f64>,
}

impl BuiltBlock {
    /// A block with no level metadata (cab IR / plugin / sample / native).
    fn plain(boxed: Box<dyn PluginInstance>, display_name: String) -> Self {
        Self {
            boxed,
            display_name,
            loudness: None,
            expected_sr: None,
            input_level_dbu: None,
            output_level_dbu: None,
        }
    }
}

/// Build a prepared box for one [`RigBlock`] at `sample_rate`.
pub(crate) fn build_block(block: &RigBlock, sample_rate: u32) -> Result<BuiltBlock, String> {
    // Reject implementations that don't fit the block type (e.g. NAM on a Delay)
    // before touching a loader.
    block.validate()?;
    if block.is_nam() {
        let mut nam = NamProcessor::load(&block.nam, sample_rate as f64, MAX_BLOCK)?;
        nam.input_gain_db = block.input_trim_db;
        nam.output_gain_db = block.output_trim_db;
        if let Some(exp) = nam.expected_sample_rate() {
            if (exp - sample_rate as f64).abs() > 1.0 {
                tracing::warn!(
                    model = %nam.display_name,
                    expected_sample_rate = exp,
                    rig_sample_rate = sample_rate,
                    "NAM model trained at a different sample rate — voicing/pitch will be off"
                );
            }
        }
        // Measure loudness ourselves (cache-first) so level-matching is reliable
        // even when the model has no/incorrect `loudness` metadata; fall back to
        // the declared value only if measurement produced silence.
        let loud = nam.measured_loudness(MAX_BLOCK).or_else(|| nam.loudness());
        let exp_sr = nam.expected_sample_rate();
        let input_level_dbu = nam.input_level();
        let output_level_dbu = nam.output_level();
        let dn = nam.display_name.clone();
        Ok(BuiltBlock {
            boxed: Box::new(nam),
            display_name: dn,
            loudness: loud,
            expected_sr: exp_sr,
            input_level_dbu,
            output_level_dbu,
        })
    } else if block.is_cab_ir() {
        let conv = Convolver::load(&block.ir)?;
        let dn = conv.display_name.clone();
        Ok(BuiltBlock::plain(Box::new(conv), format!("{dn} (cab)")))
    } else if block.is_plugin() {
        // Hosted CLAP/VST3: go through daw's own plugin loader, which returns a
        // `Box<dyn PluginInstance>` ready to drop straight into the FX chain —
        // no signal-side re-wrapping needed (the renderer drives it directly).
        let mut plugin = daw::plugin::load_plugin(&block.plugin)
            .map_err(|e| format!("load plugin {}: {e}", block.plugin))?
            .ok_or_else(|| format!("not a recognized CLAP/VST3 plugin: {}", block.plugin))?;
        plugin
            .prepare(sample_rate as f64, FX_PREPARE_BLOCK)
            .map_err(|e| format!("prepare plugin {}: {e}", block.plugin))?;
        if let Some(state) = &block.state_b64 {
            match base64_decode(state) {
                Ok(bytes) => {
                    if let Err(e) = plugin.load_state(&bytes) {
                        tracing::warn!(plugin = %block.plugin, error = %e, "failed to restore plugin state");
                    }
                }
                Err(e) => {
                    tracing::warn!(plugin = %block.plugin, error = %e, "invalid base64 plugin state");
                }
            }
        }
        let dn = plugin.descriptor().name;
        Ok(BuiltBlock::plain(plugin, format!("{dn} (plugin)")))
    } else if block.is_sample() {
        // Sample library → the Sample Soundsource, wrapped in the generic
        // leaf: `build_block`'s callers are graph boundaries that need a
        // true `PluginInstance` (FX chains); the render tree holds the bare
        // Soundsource via `build_sample_source` instead.
        let (inst, name) = build_sample_source(block, sample_rate)?;
        let leaf = crate::SoundsourceLeaf::new(inst);
        Ok(BuiltBlock::plain(
            Box::new(leaf),
            format!("{name} (sample)"),
        ))
    } else {
        // Native implementation: built-in DSP from the native registry
        // (synth blocks + the built-in FX in `signal-fx`). `native_dsp_available`
        // gates which block types resolve here; the rest are filtered before
        // install with a helpful "give it a NAM/IR/plugin" message.
        let mut inst = crate::native::build_native(block, sample_rate).ok_or_else(|| {
            format!(
                "no built-in DSP for a Native {:?} block yet — give it a NAM/IR/plugin",
                block.block_type
            )
        })?;
        inst.prepare(sample_rate as f64, FX_PREPARE_BLOCK)
            .map_err(|e| format!("prepare native {:?}: {e}", block.block_type))?;
        let dn = inst.descriptor().name;
        Ok(BuiltBlock::plain(inst, format!("{dn} (native)")))
    }
}

/// Build a sample block's **Sample Soundsource** — the `SampleEngine`
/// wrapped as a [`SamplerInstrument`](crate::SamplerInstrument), same as the
/// sampler TUI's loading path (PlayerPatch::load/from_pack +
/// SampleEngine::new) — plus its display name. The render tree holds this
/// directly as a `Box<dyn Soundsource>`; [`build_block`] wraps it in the
/// generic leaf for FX-chain hosts.
pub(crate) fn build_sample_source(
    block: &RigBlock,
    sample_rate: u32,
) -> Result<(crate::SamplerInstrument, String), String> {
    let spec_path = std::path::Path::new(&block.sample);
    // A `.signalpack` is self-contained (embedded spec + samples); a bare
    // `library.styx` needs its sample dir scanned alongside.
    let is_pack = spec_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"));
    let root = if block.samples_root.trim().is_empty() {
        spec_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .to_path_buf()
    } else {
        std::path::PathBuf::from(&block.samples_root)
    };
    let patch = if is_pack {
        crate::PlayerPatch::from_pack(spec_path)
            .map_err(|e| format!("load sample pack {}: {e}", block.sample))?
    } else {
        crate::PlayerPatch::load(spec_path, &root)
            .map_err(|e| format!("load sample library {}: {e}", block.sample))?
    };
    let section = if block.sample_section.trim().is_empty() {
        patch
            .spec
            .sections
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_default()
    } else {
        block.sample_section.clone()
    };
    let mic = if block.sample_mic.trim().is_empty() {
        patch
            .spec
            .mics
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_default()
    } else {
        block.sample_mic.clone()
    };
    let name = patch.spec.name.clone();
    let mut engine = crate::SampleEngine::new(patch, sample_rate, section, mic);
    // Imported per-block settings: unison + amplitude attack/release.
    if let Some(v) = block.param_f32("unison_voices") {
        engine.set_unison(
            v.round() as u8,
            block
                .param_f32("unison_detune")
                .unwrap_or(0.1)
                .clamp(0.0, 2.0)
                * 100.0,
            block.param_f32("unison_width").unwrap_or(0.7),
        );
    }
    if let Some(v) = block.param_f32("amp_attack") {
        engine.set_attack_frames((v.max(0.0) * sample_rate as f32) as usize);
    }
    if let Some(v) = block.param_f32("amp_release") {
        engine.set_release_frames((v.max(0.0) * sample_rate as f32) as usize);
    }
    // Decode in the background, middle-out from middle C, so the block is
    // playable almost immediately and never blocks the caller (same
    // pattern as `SamplerBank::load_block`). `FTS_PRELOAD_PROFILE` caps
    // the eager set (phones: a full piano decoded is more RAM than the
    // device has); everything past the cap decodes on first note-on.
    let cache = engine.cache_handle();
    let mut paths = engine.sample_paths_playable(60);
    // A STREAMING pack preloads every zone, because a preload is now a head
    // (~48 KB) and an index, not a decoded sample. That is the whole point of
    // Kontakt's preload buffers: every zone is ready, nothing is resident.
    // Capping a streaming pack is how you get a piano where only the middle
    // two octaves sound — a cache miss does not decode, it drops the voice.
    //
    // Everything else stays bounded by `FTS_PRELOAD_PROFILE` (default
    // FastAudition): decoding a whole multi-GB library as f32 is not an
    // option, and the coverage-first order buys a playable keyboard rather
    // than every velocity layer of a handful of keys.
    if !cache.is_streamable() {
        if let Some(cap) = std::env::var("FTS_PRELOAD_PROFILE")
            .ok()
            .and_then(|s| crate::bank::PreloadProfile::from_name(&s))
            .unwrap_or_default()
            .preload_cap()
        {
            paths.truncate(cap);
        }
    }
    let label = name.clone();
    if let Err(err) = std::thread::Builder::new()
        .name(format!("signal-preload:{label}"))
        .spawn(move || {
            let stats = cache.preload(paths.iter().map(|p| p.as_path()));
            tracing::info!(
                library = %label,
                loaded = stats.loaded,
                failed = stats.failed,
                // Left on disk because the process hit its RAM ceiling —
                // those notes stream when played. See `engine::budget`.
                skipped = stats.skipped,
                resident_mb = crate::engine::budget::used_bytes() / (1024 * 1024),
                "sample block preload complete"
            );
        })
    {
        tracing::warn!(err = %err, "failed to spawn sample block preload thread");
    }
    // A first-class Sample Soundsource.
    Ok((crate::SamplerInstrument::new(engine), name))
}

/// A resident chain: its pre-built + prepared boxes (one per chain slot, in
/// order) plus its control-side [`SlotInfo`]. Switching to it inserts these
/// boxes into the track's slot guids.
struct ResidentChain {
    #[allow(dead_code)]
    info: SlotInfo,
    /// Boxes for chain slots `0..boxes.len()`. Taken out (`Option`) when active.
    boxes: Vec<Option<Box<dyn PluginInstance>>>,
    /// Stable per-block id for each chain slot, parallel to `boxes`. Lets the
    /// live-rig layer address a running block by id (snapshot / per-block
    /// bypass / per-block param). Defaults to the block's file stem.
    block_ids: Vec<String>,
}

/// Mutable swap state shared behind a [`Mutex`] so the patch-switch surface
/// ([`set_active`](GuitarRig::set_active) / bypass / trims) can stay `&self`
/// (the original API), while installs (`&mut self`) lock it briefly. The
/// resident chains live here because both install (write) and activate (swap)
/// touch them; the actual box-swap goes through `daw.insert_plugin_instance`,
/// which is itself `&self`.
struct SwapState {
    /// Resident chains keyed by [`ModelId`].
    chains: std::collections::HashMap<ModelId, ResidentChain>,
    /// Currently-active chain id, or [`None`] (clean DI passthrough).
    active: Option<ModelId>,
    /// Patch-level trims (dB).
    input_trim_db: f32,
    output_trim_db: f32,
    bypass: bool,
}

/// A live guitar rig: a single input-armed daw track whose FX chain is the
/// active patch, running on daw's realtime `AudioEngine`.
pub struct GuitarRig {
    daw: Standalone,
    // The realtime engine (duplex pw_filter or cpal); drop = stop audio.
    _engine: RigEngine,
    /// Live realtime metrics (render time / block size) from the duplex engine,
    /// driving the rig's DSP-load meter. `None` under the cpal fallback.
    engine_stats: Option<Arc<EngineStats>>,
    #[allow(dead_code)]
    shared: Arc<TransportShared>,
    meters: Arc<Meters>,
    #[allow(dead_code)]
    project_guid: String,
    track_guid: String,
    /// Fixed fx-guids for the chain slots (constant for the project's life).
    slot_guids: Vec<String>,
    /// Input-meter atomic written by the `InputProbe` at slot 0.
    input_meter: Arc<InputMeterShared>,

    pub sample_rate: u32,
    /// Mutable swap state (resident chains + active selection + trims/bypass).
    swap: std::sync::Mutex<SwapState>,
    /// Control-side mirror of installed chains (for the UI), in install order.
    slots: Vec<SlotInfo>,
    next_id: ModelId,
    prefs: RigAudioPrefs,
}

/// Project/track names for the rig's tiny daw project.
const RIG_PROJECT_NAME: &str = "Signal Guitar Rig";
const RIG_TRACK_NAME: &str = "Guitar In";

impl GuitarRig {
    /// Open the system default input + output devices.
    pub fn new() -> eyre::Result<Self> {
        Self::open(&RigAudioPrefs::default())
    }

    /// Back-compat: open by device substring on input channel 0.
    pub fn with_devices(
        input_name: Option<&str>,
        output_name: Option<&str>,
        sample_rate: Option<u32>,
        buffer_size: Option<u32>,
    ) -> eyre::Result<Self> {
        Self::open(&RigAudioPrefs {
            input_device: input_name.unwrap_or("").to_string(),
            input_channel: 0,
            output_device: output_name.unwrap_or("").to_string(),
            sample_rate: sample_rate.unwrap_or(0),
            buffer_size: buffer_size.unwrap_or(0),
            phones_routing: false,
            main_out_l: 0,
            main_out_r: 0,
            phones_out_l: 0,
            phones_out_r: 0,
            phones_mix_in_l: 0,
            phones_mix_in_r: 0,
        })
    }

    /// Open the rig from [`RigAudioPrefs`]. Builds a one-track daw project (the
    /// track armed to monitor `prefs.input_channel`), starts daw's realtime
    /// `AudioEngine` with live input, reserves the track's FX slots, and
    /// begins transport so the renderer runs every block.
    pub fn open(prefs: &RigAudioPrefs) -> eyre::Result<Self> {
        // Low latency under JACK/PipeWire: ask PipeWire for the quantum before
        // the JACK client connects (the client can't set the buffer there).
        #[cfg(feature = "jack")]
        {
            if let Some(buf) = prefs.buffer_size_opt() {
                if std::env::var_os("PIPEWIRE_LATENCY").is_none() {
                    let rate = prefs.sample_rate_opt().unwrap_or(48_000);
                    unsafe { std::env::set_var("PIPEWIRE_LATENCY", format!("{buf}/{rate}")) };
                    tracing::info!(quantum = %format!("{buf}/{rate}"), "rig: requesting PipeWire low-latency quantum");
                }
            }
        }

        let daw = Standalone::new();

        // 1. Seed a one-track project; make it current so the FX-chain service
        //    (which resolves the current project) targets it.
        let project_guid = uuid::new_v4_string();
        daw.seed_project(ProjectInfo {
            guid: project_guid.clone(),
            name: RIG_PROJECT_NAME.to_string(),
            path: String::new(),
        });
        daw.set_current_project(&project_guid);

        let track_guid =
            <Standalone as Tracks>::add(&daw, ProjectContext::Current, RIG_TRACK_NAME, None)
                .map_err(|e| eyre::eyre!("rig: add track failed: {e}"))?;

        // 2. Arm the track to monitor the hardware input channel — this is what
        //    makes daw's engine open a live input stream and feed the bus.
        <Standalone as Tracks>::set_record_input(
            &daw,
            ProjectContext::Current,
            TrackRef::guid(track_guid.as_str()),
            RecordInput::Audio {
                channel: prefs.input_channel as u32,
            },
        )
        .map_err(|e| eyre::eyre!("rig: set record input failed: {e}"))?;

        // 3. Reserve the fixed FX slots on the track (constant guids). Slot 0 is
        //    the input probe; the rest start as identity pass-throughs.
        let fx_ctx = FxChainContext::track(track_guid.clone());
        let mut slot_guids = Vec::with_capacity(MAX_CHAIN_SLOTS + 1);
        for i in 0..=MAX_CHAIN_SLOTS {
            let idx = <Standalone as FxChains>::add(&daw, fx_ctx.clone(), &format!("rig-slot-{i}"))
                .map_err(|e| eyre::eyre!("rig: reserve fx slot {i} failed: {e}"))?;
            // The standalone FxChains service assigns the entry's guid; fetch it.
            let guid = <Standalone as FxChains>::get(&daw, fx_ctx.clone(), idx)
                .map(|fx| fx.guid)
                .ok_or_else(|| eyre::eyre!("rig: fx slot {i} vanished after add"))?;
            slot_guids.push(guid);
        }

        // 4. Build the shared transport + realtime audio engine with live input.
        //    `prefs.into()` forces `want_input` so the engine opens the input
        //    stream even before a track is read as armed.
        let req_rate = prefs.sample_rate_opt().unwrap_or(48_000);
        let shared = Arc::new(TransportShared::new(req_rate, 120.0));
        let engine = RigEngine::with_project_prefs(
            daw.clone(),
            project_guid.clone(),
            shared.clone(),
            &prefs.into(),
        )
        .map_err(|e| eyre::eyre!("rig: audio engine failed: {e}"))?;
        let sample_rate = engine.sample_rate();
        // Duplex engine surfaces live render-time metrics; cpal fallback doesn't.
        #[cfg(all(feature = "pipewire", target_os = "linux"))]
        let engine_stats = Some(engine.stats());
        #[cfg(not(all(feature = "pipewire", target_os = "linux")))]
        let engine_stats: Option<Arc<EngineStats>> = None;

        // Per-track meter bank (one cell, post-fader output peak). Install AFTER
        // the engine opens so it's sized for the device; the renderer reads
        // `daw.meters()` fresh each block.
        let meters = Meters::new(1);
        daw.set_meters(meters.clone());

        // 5. Populate the reserved slots: input probe at 0, identities elsewhere.
        let input_meter = Arc::new(InputMeterShared::default());
        {
            let mut probe = InputProbe::new(input_meter.clone());
            let _ = probe.prepare(sample_rate as f64, FX_PREPARE_BLOCK);
            daw.insert_plugin_instance(slot_guids[0].clone(), Box::new(probe));
            for guid in &slot_guids[1..] {
                let mut id = Identity::new();
                let _ = id.prepare(sample_rate as f64, FX_PREPARE_BLOCK);
                daw.insert_plugin_instance(guid.clone(), Box::new(id));
            }
        }

        // 6. Roll the transport so the renderer runs (and the live input flows
        //    through the chain to master) every block.
        shared.set_play_state(PlayStateRepr::Playing);

        // Tracing only — never println/eprintln here: the live-rig TUI owns the
        // terminal, and a stray write to stdout/stderr corrupts the render. A
        // re-open (device/latency change) calls this while the TUI is up.
        tracing::info!(
            input_channel = prefs.input_channel,
            sample_rate,
            project = %project_guid,
            "signal-sampler: guitar rig started on daw engine (in ch{} → FX chain → master @ {} Hz)",
            prefs.input_channel,
            sample_rate,
        );

        let effective = RigAudioPrefs {
            input_device: prefs.input_device.clone(),
            input_channel: prefs.input_channel,
            output_device: prefs.output_device.clone(),
            sample_rate,
            buffer_size: prefs.buffer_size,
            phones_routing: false,
            main_out_l: 0,
            main_out_r: 0,
            phones_out_l: 0,
            phones_out_r: 0,
            phones_mix_in_l: 0,
            phones_mix_in_r: 0,
        };

        Ok(Self {
            daw,
            _engine: engine,
            engine_stats,
            shared,
            meters,
            project_guid,
            track_guid,
            slot_guids,
            input_meter,
            sample_rate,
            swap: std::sync::Mutex::new(SwapState {
                chains: std::collections::HashMap::new(),
                active: None,
                input_trim_db: 0.0,
                output_trim_db: 0.0,
                bypass: false,
            }),
            slots: Vec::new(),
            next_id: 0,
            prefs: effective,
        })
    }

    /// List available input devices (name + channel count + native rate).
    pub fn input_devices() -> Vec<DeviceInfo> {
        let host = daw_audio_io::audio_host();
        daw_audio_io::input_devices(&host)
            .into_iter()
            .map(|d| DeviceInfo {
                name: d.name,
                channels: d.channels,
                default_sample_rate: d.default_sample_rate,
            })
            .collect()
    }

    /// List available output devices.
    pub fn output_devices() -> Vec<DeviceInfo> {
        let host = daw_audio_io::audio_host();
        daw_audio_io::output_devices(&host)
            .into_iter()
            .map(|d| DeviceInfo {
                name: d.name,
                channels: d.channels,
                default_sample_rate: d.default_sample_rate,
            })
            .collect()
    }

    /// The prefs the rig actually opened with (resolved sample rate).
    pub fn prefs(&self) -> &RigAudioPrefs {
        &self.prefs
    }

    /// Live phones-bus levels (headphone volume + self-mix) — forwarded to
    /// the duplex engine's lock-free bus for routed interfaces.
    pub fn set_phones_levels(volume: f32, self_mix: f32) {
        #[cfg(all(feature = "pipewire", target_os = "linux"))]
        daw::standalone::audio_engine::PhonesBus::shared().set(volume, self_mix);
        #[cfg(not(all(feature = "pipewire", target_os = "linux")))]
        let _ = (volume, self_mix); // cpal fallback: no routed phones bus yet
    }

    /// Build an FX chain on **this** thread (loading every `.nam` / `.wav` /
    /// plugin, prepared at the rig's sample rate) and store it resident.
    /// Returns its [`ModelId`]. Does **not** activate it — call
    /// [`set_active`](Self::set_active). A failed block load fails the whole
    /// install.
    pub fn install_chain(&mut self, blocks: &[RigBlock]) -> Result<ModelId, String> {
        let ids: Vec<String> = blocks
            .iter()
            .map(|b| default_block_id(b.asset_path()))
            .collect();
        self.install_chain_with_ids(blocks, &ids)
    }

    /// Like [`install_chain`](Self::install_chain) but records an explicit
    /// stable id for each block (parallel to `blocks`), so the live-rig layer
    /// can address a running block by id (`with_active_block_instance`,
    /// `set_block_slot_bypass`). `block_ids` shorter than `blocks` falls back
    /// to file-stem ids for the remainder.
    pub fn install_chain_with_ids(
        &mut self,
        blocks: &[RigBlock],
        block_ids: &[String],
    ) -> Result<ModelId, String> {
        if blocks.is_empty() {
            return Err("chain has no blocks".into());
        }
        if blocks.len() > MAX_CHAIN_SLOTS {
            return Err(format!(
                "chain has {} blocks; the rig supports at most {MAX_CHAIN_SLOTS}",
                blocks.len()
            ));
        }
        let mut boxes: Vec<Option<Box<dyn PluginInstance>>> = Vec::with_capacity(blocks.len());
        let mut names = Vec::with_capacity(blocks.len());
        let mut ids = Vec::with_capacity(blocks.len());
        let mut primary_loudness = None;
        let mut primary_expected_sr = None;
        let mut primary_input_level_dbu = None;
        let mut primary_output_level_dbu = None;
        // The first NAM block in the chain is the amp — its levels drive
        // per-patch level-match + input staging, even if its loudness is unknown.
        let mut primary_captured = false;

        for (i, b) in blocks.iter().enumerate() {
            let built = build_block(b, self.sample_rate)?;
            if !primary_captured && b.is_nam() {
                primary_captured = true;
                primary_loudness = built.loudness;
                primary_expected_sr = built.expected_sr;
                primary_input_level_dbu = built.input_level_dbu;
                primary_output_level_dbu = built.output_level_dbu;
            }
            names.push(built.display_name);
            ids.push(
                block_ids
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| default_block_id(b.asset_path())),
            );
            boxes.push(Some(built.boxed));
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let info = SlotInfo {
            id,
            display_name: names.join(" → "),
            blocks: names,
            primary_loudness,
            primary_expected_sr,
            primary_input_level_dbu,
            primary_output_level_dbu,
        };
        self.slots.push(info.clone());
        self.swap.lock().unwrap().chains.insert(
            id,
            ResidentChain {
                info,
                boxes,
                block_ids: ids,
            },
        );
        Ok(id)
    }

    /// Convenience: install a single-NAM chain (amp only). Does not activate.
    pub fn install_model(&mut self, path: impl AsRef<Path>) -> Result<ModelId, String> {
        let p = path.as_ref().to_string_lossy().to_string();
        self.install_chain(&[RigBlock::nam(p)])
    }

    /// Remove a resident chain. If it was active, falls back to passthrough.
    pub fn uninstall_model(&mut self, id: ModelId) {
        if self.active() == Some(id) {
            self.set_active(None);
        }
        self.swap.lock().unwrap().chains.remove(&id);
        self.slots.retain(|s| s.id != id);
    }

    /// Select the active chain — swaps the chain's pre-prepared boxes into the
    /// track's fixed slot guids under daw's renderer per-block lock
    /// (glitch-free), filling unused slots with identity pass-throughs. `None`
    /// = clean DI passthrough (all chain slots → identity). The old boxes are
    /// dropped on this (control) thread, off the audio thread. `&self` (via an
    /// internal `Mutex`) so footswitch / UI paths needn't hold the rig `&mut`.
    pub fn set_active(&self, id: Option<ModelId>) {
        let mut swap = self.swap.lock().unwrap();
        let chain_guids = &self.slot_guids[1..]; // slot 0 is the input probe
        let sr = self.sample_rate as f64;
        let bypass = swap.bypass;

        // Clear any per-block bypass (daw `fx_enabled`) from the previous patch:
        // re-enable every chain slot so a new chain starts with all blocks live.
        // (Per-block bypass via `set_block_slot_bypass` flips these flags; they
        // must not leak across a patch switch.)
        let fx_ctx = FxChainContext::track(self.track_guid.clone());
        for slot in 1..self.slot_guids.len() {
            let _ =
                <Standalone as FxChains>::set_enabled(&self.daw, fx_ctx.clone(), slot as u32, true);
        }

        // 1. Reclaim the previously-active chain's boxes back into the resident
        //    map so it can be re-armed (replace each live box with an identity).
        if let Some(prev) = swap.active.take() {
            if let Some(chain) = swap.chains.get_mut(&prev) {
                for (slot, guid) in chain_guids.iter().enumerate() {
                    if slot < chain.boxes.len() {
                        let reclaimed = self
                            .daw
                            .insert_plugin_instance(guid.clone(), Self::fresh_identity(sr));
                        chain.boxes[slot] = reclaimed;
                    }
                }
            }
        }

        // 2. Arm the requested chain — unless bypassed, or the id is unknown.
        let known = id.filter(|i| swap.chains.contains_key(i));
        match known.filter(|_| !bypass) {
            Some(cid) => {
                let chain = swap.chains.get_mut(&cid).expect("checked contains_key");
                for (slot, guid) in chain_guids.iter().enumerate() {
                    let mut new_box: Box<dyn PluginInstance> = match chain.boxes.get_mut(slot) {
                        Some(slot_box) if slot_box.is_some() => slot_box.take().unwrap(),
                        _ => Self::fresh_identity(sr),
                    };
                    // Re-prepare on arm: clears delay lines and reverb tails
                    // left from this chain's last activation — otherwise the
                    // stale tail dumps out as a burst on the patch switch.
                    // (Control thread, not the audio callback.)
                    let _ = new_box.prepare(sr, FX_PREPARE_BLOCK);
                    drop(self.daw.insert_plugin_instance(guid.clone(), new_box));
                }
                swap.active = Some(cid);
            }
            None => {
                for guid in chain_guids {
                    drop(
                        self.daw
                            .insert_plugin_instance(guid.clone(), Self::fresh_identity(sr)),
                    );
                }
                // When bypassed, remember the requested (known) id so toggling
                // bypass off re-arms it; otherwise we're cleanly passthrough.
                swap.active = if bypass { known } else { None };
            }
        }
    }

    fn fresh_identity(sr: f64) -> Box<dyn PluginInstance> {
        let mut id = Identity::new();
        let _ = id.prepare(sr, FX_PREPARE_BLOCK);
        Box::new(id)
    }

    pub fn active(&self) -> Option<ModelId> {
        self.swap.lock().unwrap().active
    }

    /// Convenience for the single-amp case: install a single-NAM chain + activate.
    pub fn load_nam(&mut self, path: impl AsRef<Path>) -> Result<SlotInfo, String> {
        let id = self.install_model(path)?;
        self.set_active(Some(id));
        Ok(self.slots.last().cloned().expect("just installed"))
    }

    /// Remove every chain and fall back to passthrough.
    pub fn clear(&mut self) {
        self.set_active(None);
        self.swap.lock().unwrap().chains.clear();
        self.slots.clear();
    }

    /// Chains currently resident (control-side mirror, for the UI).
    pub fn slots(&self) -> &[SlotInfo] {
        &self.slots
    }

    /// Look up a slot by id.
    pub fn slot_info(&self, id: ModelId) -> Option<&SlotInfo> {
        self.slots.iter().find(|s| s.id == id)
    }

    pub fn set_input_trim_db(&self, db: f32) {
        self.swap.lock().unwrap().input_trim_db = db;
        // The input trim is folded into each NAM block's per-block input gain at
        // build time (the resolver sets `RigBlock::input_trim_db`), so the
        // patch-level input trim is a no-op live; kept for API parity. The
        // common path (ProfileRig) sets trims *before* `set_active`, which is
        // where they take effect.
    }

    pub fn set_output_trim_db(&self, db: f32) {
        self.swap.lock().unwrap().output_trim_db = db;
        // Output trim → the track's post-fader gain (linear).
        let out_lin = db_to_lin(db) as f64;
        let _ = <Standalone as Tracks>::set_volume(
            &self.daw,
            ProjectContext::Current,
            TrackRef::guid(self.track_guid.as_str()),
            out_lin,
        );
    }

    pub fn set_bypass(&self, bypass: bool) {
        {
            let mut swap = self.swap.lock().unwrap();
            if swap.bypass == bypass {
                return;
            }
            swap.bypass = bypass;
        }
        // Re-apply the active selection through the (now changed) bypass gate:
        // bypassed → chain slots become identities; unbypassed → real chain.
        let active = self.active();
        self.set_active(active);
    }

    pub fn is_bypassed(&self) -> bool {
        self.swap.lock().unwrap().bypass
    }

    pub fn input_trim_db(&self) -> f32 {
        self.swap.lock().unwrap().input_trim_db
    }

    pub fn output_trim_db(&self) -> f32 {
        self.swap.lock().unwrap().output_trim_db
    }

    /// Post-input peak (linear) — from the `InputProbe` at slot 0.
    pub fn input_peak(&self) -> f32 {
        self.input_meter.load()
    }

    /// Per-channel input peaks (linear) — stereo metering.
    pub fn input_peak_lr(&self) -> (f32, f32) {
        self.input_meter.load_lr()
    }

    /// Mute the instrument into the chain (tuner engaged). Metering and
    /// pitch detection keep running; effect tails ring out.
    pub fn set_input_mute(&self, muted: bool) {
        self.input_meter.input_muted.store(muted, Ordering::Relaxed);
    }

    /// Arm a DI capture: the input probe records the next `samples` mono
    /// input samples (for recording a real calibration DI reference).
    pub fn arm_di_capture(&self, samples: usize) {
        self.input_meter.arm_capture(samples);
    }

    /// `(captured_so_far, remaining)` of the armed DI capture.
    pub fn di_capture_state(&self) -> (usize, usize) {
        self.input_meter.capture_state()
    }

    /// Take the finished capture buffer (empties it).
    pub fn take_di_capture(&self) -> Vec<f32> {
        self.input_meter.take_capture()
    }

    /// A snapshot of the most-recent mono input samples (post-input-trim,
    /// pre-amp), newest-last, for pitch detection (the tuner). Up to
    /// `TUNER_WINDOW` frames. Empty until the first block runs.
    pub fn input_samples(&self) -> Vec<f32> {
        self.input_meter.snapshot_samples()
    }

    /// Output peak (linear) — the track's post-fader meter cell.
    pub fn output_peak(&self) -> f32 {
        self.meters
            .cell(0)
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }

    /// Per-channel output peaks (linear) — stereo metering.
    pub fn output_peak_lr(&self) -> (f32, f32) {
        self.meters
            .cell(0)
            .map(|c| (c.peak(0), c.peak(1)))
            .unwrap_or((0.0, 0.0))
    }

    /// Realtime xruns. The duplex engine has no input ring (input and output are
    /// the same callback), so ring under/overruns don't exist; graph-deadline
    /// xruns aren't surfaced yet, so this reads 0.
    pub fn underruns(&self) -> u64 {
        self.engine_stats
            .as_ref()
            .map(|s| s.xruns.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn overruns(&self) -> u64 {
        0
    }

    pub fn installs(&self) -> u64 {
        self.slots.len() as u64
    }

    /// Last block's render time in microseconds (the DSP-load numerator),
    /// measured by the duplex engine's realtime callback.
    pub fn render_us(&self) -> u32 {
        self.engine_stats
            .as_ref()
            .map(|s| (s.render_ns.load(std::sync::atomic::Ordering::Relaxed) / 1000) as u32)
            .unwrap_or(0)
    }

    pub fn peak_render_us(&self) -> u32 {
        self.engine_stats
            .as_ref()
            .map(|s| (s.peak_render_ns.load(std::sync::atomic::Ordering::Relaxed) / 1000) as u32)
            .unwrap_or(0)
    }

    pub fn reset_render_peak(&self) {
        if let Some(s) = &self.engine_stats {
            s.reset_peak();
        }
    }

    /// Frames in the last block (the running buffer/quantum) — from the engine's
    /// live metrics, falling back to the configured buffer before the first block.
    pub fn block_frames(&self) -> u32 {
        let live = self
            .engine_stats
            .as_ref()
            .map(|s| s.block_frames.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        if live > 0 {
            live
        } else {
            self.prefs.buffer_size
        }
    }

    /// Input→output bridge latency. The duplex engine processes input and output
    /// in one callback (no ring), so this is genuinely 0.
    pub fn ring_frames(&self) -> u32 {
        0
    }

    /// dB of the current output peak — convenience over [`linear_to_db`].
    pub fn output_peak_db(&self) -> f64 {
        linear_to_db(self.output_peak())
    }

    // ── Live block addressing (Phase B) ─────────────────────────────────────
    //
    // The active chain's blocks live in `daw`'s `plugin_instances` map under the
    // track's fixed slot guids (`slot_guids[1..]`, slot 0 = input probe). These
    // methods resolve a block by its stable id to the right slot, then reach the
    // live instance (param edits) or flip the daw `fx_enabled` flag on that slot
    // (per-block bypass) — both honored by the renderer with no chain rebuild.

    /// Resolve, for the currently-active chain, a block id → `(chain_slot_index,
    /// fx_guid)`. `chain_slot_index` is 0-based within the chain (slot 0 of the
    /// chain maps to `slot_guids[1]`, the renderer fx_idx is `index + 1`).
    fn active_block_slot(&self, block_id: &str) -> Option<(usize, String)> {
        let swap = self.swap.lock().unwrap();
        let active = swap.active?;
        let chain = swap.chains.get(&active)?;
        let slot = chain.block_ids.iter().position(|b| b == block_id)?;
        // chain slot 0 → slot_guids[1] (slot_guids[0] is the input probe).
        let guid = self.slot_guids.get(slot + 1)?.clone();
        Some((slot, guid))
    }

    /// Block ids of the active chain, in order. Empty when nothing is active.
    pub fn active_block_ids(&self) -> Vec<String> {
        let swap = self.swap.lock().unwrap();
        swap.active
            .and_then(|a| swap.chains.get(&a))
            .map(|c| c.block_ids.clone())
            .unwrap_or_default()
    }

    /// Run `f` against the live [`PluginInstance`] backing the active chain's
    /// block `block_id`, under daw's renderer lock. `None` if no chain is active
    /// or the id isn't in it. The control seam for per-block params (e.g. NAM
    /// amp trims via [`PluginInstance::as_any_mut`]).
    pub fn with_active_block_instance<R>(
        &self,
        block_id: &str,
        f: impl FnOnce(&mut dyn PluginInstance) -> R,
    ) -> Option<R> {
        let (_slot, guid) = self.active_block_slot(block_id)?;
        self.daw.with_plugin_instance(&guid, f)
    }

    /// Per-block bypass on the active chain: flips daw's `fx_enabled` flag on the
    /// block's slot (the renderer skips disabled slots — no chain rebuild, no
    /// box swap). `on = true` bypasses the block. Returns `true` if the block was
    /// found and the flag was set.
    pub fn set_block_slot_bypass(&self, block_id: &str, on: bool) -> bool {
        let Some((slot, _guid)) = self.active_block_slot(block_id) else {
            return false;
        };
        // Renderer fx_idx = chain slot + 1 (slot 0 is the input probe). Bypass =
        // disabled; enabled = !on.
        let fx_ctx = FxChainContext::track(self.track_guid.clone());
        <Standalone as FxChains>::set_enabled(&self.daw, fx_ctx, (slot + 1) as u32, !on).is_ok()
    }

    /// Set a named parameter on the active chain's block `block_id` to `value`.
    ///
    /// Routing, by block backend:
    /// - **NAM** (amp/drive/neural-cab): `"input_trim"` / `"output_trim"` set the
    ///   live `NamProcessor`'s gain (dB) in place under the renderer lock.
    /// - **Hosted CLAP/VST3**: matches `param_name` against the plugin's reported
    ///   parameter names and pushes the value through daw's `FxParams::set`
    ///   (stored in project `fx_params`, forwarded to `process_block` each block).
    ///   `value` is treated as already-normalized 0..1 for the param's range.
    ///
    /// Returns `true` if the param was found and applied. A param that can't be
    /// addressed yet (e.g. an IR cab, which has no continuous params) returns
    /// `false`.
    pub fn set_active_block_param(&self, block_id: &str, param_name: &str, value: f32) -> bool {
        let Some((slot, guid)) = self.active_block_slot(block_id) else {
            return false;
        };

        // 1. NAM trims — mutate the live instance directly via downcast.
        let nam_applied = self
            .daw
            .with_plugin_instance(&guid, |inst| {
                let any = inst.as_any_mut()?;
                if let Some(nam) = any.downcast_mut::<NamProcessor>() {
                    match param_name {
                        "input_trim" => {
                            nam.input_gain_db = value;
                            return Some(true);
                        }
                        "output_trim" => {
                            nam.output_gain_db = value;
                            return Some(true);
                        }
                        _ => return Some(false),
                    }
                }
                None
            })
            .flatten();
        match nam_applied {
            // NAM trim applied.
            Some(true) => return true,
            // NAM block, but the name isn't a trim — no other NAM params exist.
            Some(false) => return false,
            // Not a NAM block — fall through to the hosted-plugin path.
            None => {}
        }

        // 2. Hosted plugin — resolve param name → slot index + range, then
        // FxParams::set. The FxParams contract is NORMALIZED 0..1 (the daw
        // denormalizes via the param's range), while callers hand us plain
        // values (dB, Hz, ms) — normalize here or a "+1 dB" boost lands as
        // full-scale (+24 dB).
        let param_slot = self.daw.with_plugin_instance(&guid, |inst| {
            inst.params()
                .into_iter()
                .enumerate()
                .find(|(_, p)| p.name == param_name)
                .map(|(i, p)| (i, p.min, p.max))
        });
        let Some(Some((param_idx, min, max))) = param_slot else {
            return false;
        };
        let normalized = if max > min {
            ((value as f64 - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fx_ctx = FxChainContext::track(self.track_guid.clone());
        <Standalone as FxParams>::set(
            &self.daw,
            fx_ctx,
            (slot + 1) as u32,
            param_idx as u32,
            normalized,
        )
        .is_ok()
    }
}

/// Test accessor for [`default_block_id`] (cross-module test in `api::rig`).
#[doc(hidden)]
pub fn default_block_id_for_test(path: &str) -> String {
    default_block_id(path)
}

/// Derive a stable block id from a file path's stem (matches the api layer's
/// `default_id`). Empty/unknown stems fall back to `"block"`.
fn default_block_id(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("block")
        .to_string()
}

/// Decode a standard-base64 plugin state chunk.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| e.to_string())
}

/// Process-unique guid string — shared by [`GuitarRig`] and
/// [`SamplerRig`](crate::sampler_rig::SamplerRig) for their project guids.
pub fn uuid_string() -> String {
    uuid::new_v4_string()
}

mod uuid {
    //! Minimal UUIDv4-ish string generator — avoids pulling the `uuid` crate just
    //! for the rig's project guid. Not cryptographically strong; only needs to
    //! be unique within this process.
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub fn new_v4_string() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id() as u64;
        format!("signal-rig-{nanos:x}-{pid:x}-{c:x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_block_kind_predicates() {
        assert!(RigBlock::nam("a.nam").is_nam());
        assert!(RigBlock::cab_ir("v30.wav").is_cab_ir());
        assert!(RigBlock::plugin("/x/Reverb.clap").is_plugin());
        assert!(!RigBlock::nam("a.nam").is_cab_ir());
    }

    #[test]
    fn impl_is_constrained_by_block_type() {
        // NAM is only for amp-shaped blocks.
        assert!(BlockImpl::Nam.supports(BlockType::Amp));
        assert!(BlockImpl::Nam.supports(BlockType::Drive));
        assert!(BlockImpl::Nam.supports(BlockType::Saturator));
        assert!(!BlockImpl::Nam.supports(BlockType::Delay));
        assert!(!BlockImpl::Nam.supports(BlockType::Cabinet));
        // IR is only for a cabinet.
        assert!(BlockImpl::Ir.supports(BlockType::Cabinet));
        assert!(!BlockImpl::Ir.supports(BlockType::Amp));
        // Plugin + Native fit anything.
        assert!(BlockImpl::Plugin.supports(BlockType::Reverb));
        assert!(BlockImpl::Native.supports(BlockType::Pitch));

        // A NAM asset on a Delay block is rejected by validate().
        let bad = RigBlock::of_type(BlockType::Delay).with_nam("echo.nam");
        assert!(bad.validate().is_err());
        // A NAM amp is fine.
        assert!(RigBlock::nam("amp.nam").validate().is_ok());
        // allowed_for a Delay excludes Nam/Ir.
        let allowed = BlockImpl::allowed_for(BlockType::Delay);
        assert!(allowed.contains(&BlockImpl::Native) && allowed.contains(&BlockImpl::Plugin));
        assert!(!allowed.contains(&BlockImpl::Nam));
    }

    #[test]
    fn block_implementation_is_derived_from_the_asset() {
        // type + implementation are independent: an Amp can be NAM…
        let amp = RigBlock::nam("amp.nam");
        assert_eq!(amp.block_type, BlockType::Amp);
        assert_eq!(amp.implementation(), BlockImpl::Nam);
        assert!(amp.has_backend());

        // …a Cabinet can be an IR…
        assert_eq!(RigBlock::cab_ir("v30.wav").implementation(), BlockImpl::Ir);

        // …a Delay can be an external plugin…
        let delay = RigBlock::effect(BlockType::Delay, "Echo").with_plugin("Echo.clap");
        assert_eq!(delay.block_type, BlockType::Delay);
        assert_eq!(delay.implementation(), BlockImpl::Plugin);

        // …and a typed effect with no asset is Native (built-in DSP). Reverb
        // has a registered native backend today…
        let native = RigBlock::effect(BlockType::Reverb, "Hall");
        assert_eq!(native.implementation(), BlockImpl::Native);
        assert!(native.is_native());
        assert!(native.has_backend());

        // …while a Native type with no registry entry (Pitch) has no backend
        // yet, so it's skipped at install.
        let pending = RigBlock::effect(BlockType::Pitch, "Shifter");
        assert_eq!(pending.implementation(), BlockImpl::Native);
        assert!(!pending.has_backend());
    }

    #[test]
    fn rig_block_plugin_with_state_roundtrips() {
        let b = RigBlock::plugin_with_state("/x/Delay.vst3", Some("c3RhdGU=".into()));
        assert!(b.is_plugin());
        assert_eq!(b.state_b64.as_deref(), Some("c3RhdGU="));
    }

    /// Building a NAM block produces a prepared, non-silent `PluginInstance`.
    #[test]
    fn build_block_nam_produces_audio() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/amp_a.nam");
        let Ok(built) = build_block(
            &RigBlock::nam(fixture.to_string_lossy().to_string()),
            48_000,
        ) else {
            eprintln!("skip: amp_a.nam fixture failed to load");
            return;
        };
        let (mut boxed, name) = (built.boxed, built.display_name);
        assert!(!name.is_empty());
        assert!(boxed.is_prepared());
        const N: usize = 128;
        let sig: Vec<f32> = (0..N).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let (mut ol, mut or_) = (vec![0.0f32; N], vec![0.0f32; N]);
        boxed
            .process_block(&sig, &sig, &mut ol, &mut or_, &PluginEvents::default())
            .unwrap();
        let energy: f64 = ol.iter().map(|x| (*x as f64).powi(2)).sum();
        assert!(energy > 1e-9, "NAM block should produce audio");
    }

    #[test]
    fn identity_passthrough_copies_input() {
        let mut id = Identity::new();
        let il = [0.1, -0.2, 0.3];
        let ir = [0.4, -0.5, 0.6];
        let (mut ol, mut or_) = ([0.0f32; 3], [0.0f32; 3]);
        id.process_block(&il, &ir, &mut ol, &mut or_, &PluginEvents::default())
            .unwrap();
        assert_eq!(ol, il);
        assert_eq!(or_, ir);
    }

    #[test]
    fn input_probe_records_peak() {
        let shared = Arc::new(InputMeterShared::default());
        let mut probe = InputProbe::new(shared.clone());
        let il = [0.1, -0.8, 0.3];
        let ir = [0.2, 0.4, -0.5];
        let (mut ol, mut or_) = ([0.0f32; 3], [0.0f32; 3]);
        probe
            .process_block(&il, &ir, &mut ol, &mut or_, &PluginEvents::default())
            .unwrap();
        assert!(
            (shared.load() - 0.8).abs() < 1e-6,
            "probe records the block peak"
        );
        assert_eq!(ol, il, "probe passes input through unchanged");
    }

    #[test]
    fn db_to_lin_is_unity_at_zero() {
        assert_eq!(db_to_lin(0.0), 1.0);
        assert!((db_to_lin(-6.0206) - 0.5).abs() < 1e-4);
    }
}
