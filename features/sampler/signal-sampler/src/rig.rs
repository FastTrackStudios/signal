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

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use signal_proto::block::{BlockCategory, BlockType};

use facet::Facet;

#[cfg(not(target_arch = "wasm32"))]
use daw::service::{FxChainContext, FxParams, ProjectContext, TrackRef, Tracks};
#[cfg(not(target_arch = "wasm32"))]
use daw::standalone::Standalone;
#[cfg(not(target_arch = "wasm32"))]
use daw::standalone::metering::{Meters, linear_to_db};
#[cfg(not(target_arch = "wasm32"))]
use daw_audio_io::duplex::EngineStats;
// The shared daw-backed host: project seeding, track/FX-slot reservation, the
// realtime engine (native duplex `pw_filter` on Linux with the `pipewire`
// feature, cpal fallback elsewhere), meters, transport.
use signal_plugin_host::PluginInstance;
#[cfg(not(target_arch = "wasm32"))]
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginParamInfo,
};
#[cfg(not(target_arch = "wasm32"))]
use signal_rig_host::{DuplexRigHost, RigProject};

use crate::convolver::Convolver;
use crate::mixer::FX_PREPARE_BLOCK;
use crate::nam::NamProcessor;
#[cfg(not(target_arch = "wasm32"))]
use crate::rig_prefs::RigAudioPrefs;

/// Max block size the rig prepares models / plugins for. daw's callback block is
/// normally 64–1024 frames; preparing for [`FX_PREPARE_BLOCK`] keeps us safe
/// against larger backend buffers without per-block re-preparation.
const MAX_BLOCK: usize = FX_PREPARE_BLOCK as usize;
/// A block gate's ramp scratch: blocks larger than this skip the ramp.
const GATE_BLOCK: usize = 2048;

/// Fixed number of FX slots reserved on the rig track. Slot 0 is the
/// `InputProbe` (input meter); slots `1..=MAX_CHAIN_SLOTS` carry the active
/// chain's blocks (identity pass-throughs fill unused ones). Reserving a
/// constant count keeps the project's `fx_chain` (guids) immutable, so patch
/// switches never rebuild the renderer's snapshot — the swap is pure box-insert.
const MAX_CHAIN_SLOTS: usize = 40;

/// Identifies a chain resident control-side. Assigned on install; opaque
/// elsewhere.
pub type ModelId = u32;

/// How a block's [`BlockType`] is **implemented** — the realization axis
/// (orthogonal to the semantic `block_type`).
///
/// Each block type can have several implementations; a [`RigBlock`] picks one by which
/// asset it carries. Mirrors `signal_proto::block_kind::BlockKind` and the runtime
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
    #[must_use]
    pub fn supports(self, block_type: BlockType) -> bool {
        match self {
            Self::Native | Self::Plugin => true,
            Self::Nam => matches!(
                block_type,
                BlockType::Amp | BlockType::Drive | BlockType::Saturator
            ),
            Self::Ir => block_type == BlockType::Cabinet,
            Self::Sample => block_type == BlockType::Sampler,
        }
    }

    /// The implementations valid for `block_type` (for UI pickers). `Native`
    /// always leads (the default), then the type-specific specials, then
    /// `Plugin`.
    #[must_use]
    pub fn allowed_for(block_type: BlockType) -> Vec<Self> {
        [
            Self::Native,
            Self::Nam,
            Self::Ir,
            Self::Sample,
            Self::Plugin,
        ]
        .into_iter()
        .filter(|i| i.supports(block_type))
        .collect()
    }

    /// Short identifier for logs / UI.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Nam => "nam",
            Self::Ir => "ir",
            Self::Plugin => "plugin",
            Self::Sample => "sample",
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
    /// Stable identity — a UUIDv7, minted once and persisted. The leaf half
    /// of the same contract as [`Container::id`](crate::rig_node::Container::id):
    /// an override reaching a single parameter addresses the block it lives
    /// on, and must keep working when the block is renamed.
    #[facet(default)]
    pub id: String,
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
    /// Per-block parameter values applied at build time — `(name, value)`
    /// matching the backend's parameter names, as decimal strings.
    ///
    /// **The value is in whatever units that backend's parameter uses**, and
    /// which those are is known only to the backend: `amp_attack` is
    /// milliseconds, `rate` is Hz, `cutoff` and `sustain` are already
    /// normalized. There is no rule here, only a convention per parameter —
    /// `native_osc::with_block_params` is the reference for the synth
    /// layer's.
    ///
    /// (This said "normalized values" until 2026-09. It was wrong: the rig's
    /// own param path takes plain dB/Hz/ms and normalizes against the
    /// backend's declared min/max, and the envelope times above are read as
    /// milliseconds. It matters because it is exactly what stops a
    /// `Container` being lifted into a `NodeLibrary` — a domain
    /// `BlockParameter` is a 0..=1 position plus its range, and without the
    /// range these values cannot cross. See
    /// `from_node::tests::an_unranged_parameter_cannot_hold_a_real_value`.)
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
    #[must_use]
    pub fn of_type(block_type: BlockType) -> Self {
        Self {
            id: crate::rig_node::new_node_id(),
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
    #[must_use]
    pub fn param_str(&self, name: &str) -> Option<String> {
        self.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value.clone())
    }

    /// A build-time parameter as `f32`, if present and numeric.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn is_native(&self) -> bool {
        self.implementation() == BlockImpl::Native
    }

    /// Whether the rig can build an audio backend for this block **today**. NAM,
    /// IR and Plugin implementations are always buildable; a `Native` block is
    /// buildable only for the block types whose built-in DSP is implemented (see
    /// [`native_dsp_available`]) — the rest are placeholders until their DSP lands.
    #[must_use]
    pub fn has_backend(&self) -> bool {
        match self.implementation() {
            BlockImpl::Native => native_dsp_available(self.block_type),
            _ => true,
        }
    }

    /// Check the block's implementation is valid for its type (e.g. a NAM model
    /// can't realize a `Delay`). Returns a human-readable error otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if the block's implementation is not valid for its `block_type`.
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
    #[must_use]
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        std::path::Path::new(self.asset_path())
            .file_stem()
            .and_then(|s| s.to_str())
            .map(ToString::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.block_type.display_name().to_string())
    }

    /// Whether the global "time bypass" footswitch should hit this block: its
    /// type is a Time effect (Delay / Reverb / Freeze) or it's tagged into a
    /// "Time" module. Deliberately narrow — drive, pitch (POG) and modulation
    /// are NOT killed by the time switch (Funk = clean with Time bypassed).
    #[must_use]
    pub fn is_time_fx(&self) -> bool {
        self.module.eq_ignore_ascii_case("time")
            || self.block_type.category() == BlockCategory::Time
    }

    /// In the Time module — what the global time/FX bypass acts on. An
    /// explicit module wins over the block type's category, so a reverb
    /// placed before the amp (module "Pre FX") is not killed by the FX
    /// switch meant for the delays and reverbs at the end of the chain.
    #[must_use]
    pub fn is_time_module(&self) -> bool {
        if self.module.is_empty() {
            self.block_type.category() == BlockCategory::Time
        } else {
            self.module.eq_ignore_ascii_case("time")
        }
    }

    #[must_use]
    pub fn is_nam(&self) -> bool {
        !self.nam.is_empty()
    }

    #[must_use]
    pub fn is_cab_ir(&self) -> bool {
        !self.ir.is_empty()
    }

    #[must_use]
    pub fn is_plugin(&self) -> bool {
        !self.plugin.is_empty()
    }

    #[must_use]
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
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
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
    /// A test signal that replaces the instrument while set — the DI reference
    /// looped through the live chain, for levelling patches by what they
    /// actually output (see [`GuitarRig::start_test_signal`]).
    inject: std::sync::Mutex<Option<Arc<Vec<f32>>>>,
    /// Bumped when `inject` changes; the probe re-reads it (and restarts the
    /// loop) when it sees a new value, so the audio thread only locks then.
    inject_gen: std::sync::atomic::AtomicU64,
}

/// Mono window the tuner runs autocorrelation over. At 48 kHz this covers
/// ~85 ms — long enough to resolve a low-E (~82 Hz, ~12 ms period) several
/// times over.
#[cfg(not(target_arch = "wasm32"))]
const TUNER_WINDOW: usize = 4096;

#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
fn load_fake_di() -> Option<Vec<f32>> {
    let path = std::env::var("SIGNAL_FAKE_DI")
        .ok()
        .filter(|p| !p.is_empty())?;
    let (mono, _) = fts_sample::load_mono_f32(
        std::path::Path::new(&path),
        None,
        fts_sample::ResampleQuality::default(),
    )
    .map_err(|e| tracing::warn!("fake DI: {e}"))
    .ok()?;
    (!mono.is_empty()).then(|| {
        tracing::info!(
            "fake DI active: {path} ({:.1}s loop)",
            mono.len() as f32 / 48_000.0
        );
        mono
    })
}

#[cfg(not(target_arch = "wasm32"))]
struct InputProbe {
    shared: Arc<InputMeterShared>,
    /// Fake-DI loop + cursor (debug input; None in normal operation).
    fake: Option<Vec<f32>>,
    fake_pos: usize,
    /// The injected test signal (see `InputMeterShared::inject`) + cursor,
    /// and the generation it was read at.
    inject: Option<Arc<Vec<f32>>>,
    inject_pos: usize,
    inject_gen: u64,
    prepared: bool,
    /// Per-block mono scratch (reused) for the tuner window push.
    mono: Vec<f32>,
    /// Where the chain's input is kept for the output stage (a voice fading
    /// out goes on hearing the guitar — see `tail_stage`).
    share: Option<Arc<crate::tail_stage::InputShare>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl InputProbe {
    fn new(shared: Arc<InputMeterShared>) -> Self {
        Self {
            shared,
            prepared: true,
            mono: Vec::with_capacity(MAX_BLOCK),
            fake: load_fake_di(),
            fake_pos: 0,
            inject: None,
            inject_pos: 0,
            inject_gen: 0,
            share: None,
        }
    }

    fn with_share(mut self, share: Arc<crate::tail_stage::InputShare>) -> Self {
        self.share = Some(share);
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
        // A new test signal (or its removal): re-read it, from the top. Only
        // locks when the generation moved, and never waits for the lock.
        let generation = self.shared.inject_gen.load(Ordering::Acquire);
        if generation != self.inject_gen {
            if let Ok(slot) = self.shared.inject.try_lock() {
                self.inject = slot.clone();
                self.inject_pos = 0;
                self.inject_gen = generation;
            }
        }
        let (mut pk_l, mut pk_r) = (0.0f32, 0.0f32);
        // Reused mono scratch for the tuner window push (off the steady-state
        // alloc path after the first block).
        self.mono.clear();
        self.mono.reserve(frames);
        for i in 0..frames {
            // The test signal, else the fake-DI debug loop, replaces the
            // live input when armed.
            let (src_l, src_r) = if let Some(w) = self.inject.as_ref().filter(|w| !w.is_empty()) {
                let v = w[self.inject_pos];
                self.inject_pos = (self.inject_pos + 1) % w.len();
                (v, v)
            } else {
                match &self.fake {
                    Some(w) => {
                        let v = w[self.fake_pos];
                        self.fake_pos = (self.fake_pos + 1) % w.len();
                        (v, v)
                    }
                    None => (in_l[i], in_r[i]),
                }
            };
            // Muted: the chain gets silence (trails keep ringing) while
            // the meters and the tuner still see the instrument.
            out_l[i] = if muted { 0.0 } else { src_l };
            out_r[i] = if muted { 0.0 } else { src_r };
            pk_l = pk_l.max(src_l.abs());
            pk_r = pk_r.max(src_r.abs());
            self.mono.push((src_l + src_r) * 0.5);
        }
        if let Some(share) = &self.share {
            share.write(&out_l[..frames], &out_r[..frames]);
        }
        self.shared.store(pk_l.max(pk_r));
        self.shared.store_lr(pk_l, pk_r);
        // Feed the global DI sidechain — the gate keys off the clean guitar
        // even though it sits post-amp in the chain.
        fx_blocks::sidechain::set_peak(pk_l.max(pk_r));
        self.shared.push_samples(&self.mono);
        self.shared.push_capture(&self.mono);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// What the [`OutputTap`] shares with the control thread.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
struct OutputTapShared {
    /// Captured output, per channel — filled while `remaining > 0`.
    capture: std::sync::Mutex<(Vec<f32>, Vec<f32>)>,
    /// Frames still to capture (0 = disarmed).
    remaining: std::sync::atomic::AtomicUsize,
    /// Silence what leaves the chain (the capture still sees it) — so a
    /// levelling pass is not played through the speakers.
    muted: AtomicBool,
    /// Capture what is heard — after the patch level and the tails ringing
    /// under it — rather than the chain's own output (what levelling needs).
    heard: AtomicBool,
}

/// The last FX slot on the rig track, after every chain slot: the chain's
/// real output, captured on request — the measurement point for levelling
/// patches by what the rig actually plays, trims and all.
///
/// It is also where a switch lands: the patch's output level is applied
/// here (after the capture, so a measurement sees the chain untrimmed), the
/// incoming chain fades in, and the outgoing ones ring out — see
/// [`crate::tail_stage`].
#[cfg(not(target_arch = "wasm32"))]
struct OutputTap {
    shared: Arc<OutputTapShared>,
    prepared: bool,
    stage: crate::tail_stage::TailStage,
}

#[cfg(not(target_arch = "wasm32"))]
impl PluginInstance for OutputTap {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.rig.output_tap".into(),
            name: "Output".into(),
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
        let heard = self.shared.heard.load(Ordering::Relaxed);
        if !heard {
            self.capture(&in_l[..frames], &in_r[..frames]);
        }
        let tails = self.stage.process(
            &in_l[..frames],
            &in_r[..frames],
            &mut out_l[..frames],
            &mut out_r[..frames],
        );
        if tails {
            // Two separately limited chains can sum past full scale while
            // one rings out under the other.
            for s in out_l[..frames].iter_mut().chain(out_r[..frames].iter_mut()) {
                *s = soft_ceiling(*s);
            }
        }
        if heard {
            self.capture(&out_l[..frames], &out_r[..frames]);
        }
        if self.shared.muted.load(Ordering::Relaxed) {
            out_l[..frames].fill(0.0);
            out_r[..frames].fill(0.0);
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl OutputTap {
    fn capture(&self, l: &[f32], r: &[f32]) {
        let remaining = self.shared.remaining.load(Ordering::Relaxed);
        if remaining > 0 {
            let take = remaining.min(l.len());
            // Buffers were reserved when armed; never wait on the lock.
            if let Ok(mut cap) = self.shared.capture.try_lock() {
                cap.0.extend_from_slice(&l[..take]);
                cap.1.extend_from_slice(&r[..take]);
                self.shared.remaining.store(remaining - take, Ordering::Relaxed);
            }
        }
    }
}

/// Unity below −0.4 dBFS; above it, a smooth knee into full scale.
#[cfg(not(target_arch = "wasm32"))]
fn soft_ceiling(x: f32) -> f32 {
    const KNEE: f32 = 0.955;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

/// A unity pass-through [`PluginInstance`] used to fill the rig track's unused
/// FX slots (chains shorter than [`MAX_CHAIN_SLOTS`]). Copies input → output.
#[cfg(not(target_arch = "wasm32"))]
struct Identity {
    prepared: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Identity {
    fn new() -> Self {
        Self { prepared: true }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
#[must_use]
pub fn native_dsp_available(block_type: BlockType) -> bool {
    crate::native::native_dsp_available(block_type)
}

/// A built, prepared chain block plus the level metadata the rig needs to
/// level-match and input-stage it. Non-NAM blocks leave the NAM-only fields
/// `None`.
// The level-metadata fields are read only by the native chain installer
// (GuitarRig); on wasm32 `build_block` still fills them.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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

/// A chain whose blocks are built but not yet resident in a rig.
///
/// Building a chain is the expensive part — model loads, DSP allocation,
/// `prepare()` — and it needs nothing from the rig but the sample rate, so
/// it can happen on any thread. Installing is cheap. Keeping them apart is
/// what lets a profile build its patches concurrently and install them one
/// after another; see [`Rig::install_prepared`].
pub struct PreparedChain {
    boxes: Vec<Option<Box<dyn PluginInstance>>>,
    names: Vec<String>,
    ids: Vec<String>,
    prepare_on_arm: Vec<bool>,
    /// Each block's type, and whether it is a built-in effect (written live
    /// as param events), parallel to `boxes`.
    kinds: Vec<(BlockType, bool)>,
    /// Each block's bypass (see `block_gate`), parallel to `boxes`.
    gates: Vec<Arc<crate::block_gate::GateCtl>>,
    /// Where the Time section starts — what rings on after a switch.
    time_start: usize,
    primary_loudness: Option<f64>,
    primary_expected_sr: Option<f64>,
    primary_input_level_dbu: Option<f64>,
    primary_output_level_dbu: Option<f64>,
}

impl PreparedChain {
    /// The chain's display name — the block names joined by arrows.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.names.join(" → ")
    }

    /// The built blocks with their ids, in chain order — for a host that
    /// runs the chain itself rather than through [`GuitarRig`] (the browser
    /// worklet). A slot is `None` only while a rig has it armed.
    #[must_use]
    pub fn into_blocks(self) -> Vec<(String, Box<dyn PluginInstance>)> {
        self.ids
            .into_iter()
            .zip(self.boxes)
            .filter_map(|(id, b)| b.map(|b| (id, b)))
            .collect()
    }
}

/// Build every block of a chain, off the rig.
///
/// The counterpart to [`Rig::install_prepared`]. Takes the sample rate
/// rather than a rig so several chains can be prepared at once.
///
/// # Errors
///
/// Returns an error if the chain is empty or too long, or if any block
/// fails to load, validate, or prepare.
pub fn prepare_chain(
    blocks: &[RigBlock],
    block_ids: &[String],
    sample_rate: u32,
) -> Result<PreparedChain, String> {
    prepare_chain_with(blocks, block_ids, sample_rate, &mut |_, _| None)
}

/// [`prepare_chain`], letting the caller supply a block's processor itself.
///
/// `supply(index, block)` runs before each block is built; a `Some` is used
/// in place of building it. The browser host uses this to put a worker
/// proxy where a NAM model would run — before the dual-amp stage wraps the
/// chain, so a proxied Amp R still blends in parallel with Amp L.
///
/// # Errors
///
/// As [`prepare_chain`].
pub fn prepare_chain_with(
    blocks: &[RigBlock],
    block_ids: &[String],
    sample_rate: u32,
    supply: &mut dyn FnMut(usize, &RigBlock) -> Option<Box<dyn PluginInstance>>,
) -> Result<PreparedChain, String> {
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
    // Which slots hold a tail that must be cleared when the chain is armed.
    let mut prepare_on_arm = Vec::with_capacity(blocks.len());
    let mut kinds = Vec::with_capacity(blocks.len());
    let mut primary_loudness = None;
    let mut primary_expected_sr = None;
    let mut primary_input_level_dbu = None;
    let mut primary_output_level_dbu = None;
    let mut primary_captured = false;

    for (i, b) in blocks.iter().enumerate() {
        // `Instant` panics on wasm32-unknown-unknown (no clock).
        #[cfg(not(target_arch = "wasm32"))]
        let began = std::time::Instant::now();
        let built = match supply(i, b) {
            Some(boxed) => BuiltBlock::plain(boxed, b.name.clone()),
            None => build_block(b, sample_rate)?,
        };
        #[cfg(not(target_arch = "wasm32"))]
        tracing::trace!(
            block.name = %b.name,
            block.kind = ?b.block_type,
            block.build_ms = began.elapsed().as_secs_f64() * 1000.0,
            "prepare_chain: block built"
        );
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
        prepare_on_arm.push(b.is_time_fx());
        kinds.push((b.block_type, b.is_native()));
        boxes.push(Some(built.boxed));
    }

    // Each block carries its own bypass (`block_gate`). Delays and reverbs
    // innermost, so a time stage still splits its input when one is off and
    // a bypassed one rings out; everything else outermost, so a bypassed
    // block is skipped exactly as the dual-amp and time stages expect.
    let block_names: Vec<&str> = blocks.iter().map(|b| b.name.as_str()).collect();
    let gates: Vec<Arc<crate::block_gate::GateCtl>> = blocks
        .iter()
        .map(|b| crate::block_gate::GateCtl::new(b.bypassed))
        .collect();
    let stage_roles = crate::time_stage::roles(&block_names);
    for (i, b) in blocks.iter().enumerate() {
        if b.is_time_fx() {
            if let Some(inner) = boxes[i].take() {
                // The time stage's second block adds only its wet; the stage
                // carries the dry.
                let pass = if stage_roles[i] == Some(crate::time_stage::Role::Add) { 0.0 } else { 1.0 };
                boxes[i] = Some(Box::new(crate::block_gate::BlockGate::new(
                    inner,
                    gates[i].clone(),
                    crate::block_gate::GateMode::Trails,
                    pass,
                    GATE_BLOCK,
                )));
            }
        }
    }
    let time_start = blocks
        .iter()
        .position(RigBlock::is_time_module)
        .unwrap_or(blocks.len());

    // Two amps loaded: Amp L and Amp R blend in parallel rather than one
    // driving the other (see `amp_blend`).
    let r_loaded = blocks
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case(crate::amp_blend::AMP_R) && b.is_nam());
    let roles = crate::amp_blend::roles(&block_names, r_loaded);
    crate::amp_blend::wrap(&mut boxes, &roles, MAX_BLOCK);
    // The Time module: its two delays in parallel, and its two reverbs.
    crate::time_stage::wrap(&mut boxes, &block_names, MAX_BLOCK);
    for (i, b) in blocks.iter().enumerate() {
        if !b.is_time_fx() {
            if let Some(inner) = boxes[i].take() {
                boxes[i] = Some(Box::new(crate::block_gate::BlockGate::new(
                    inner,
                    gates[i].clone(),
                    crate::block_gate::GateMode::Hard,
                    1.0,
                    GATE_BLOCK,
                )));
            }
        }
    }

    Ok(PreparedChain {
        boxes,
        names,
        ids,
        prepare_on_arm,
        kinds,
        gates,
        time_start,
        primary_loudness,
        primary_expected_sr,
        primary_input_level_dbu,
        primary_output_level_dbu,
    })
}

/// Build a prepared box for one [`RigBlock`] at `sample_rate`.
pub(crate) fn build_block(block: &RigBlock, sample_rate: u32) -> Result<BuiltBlock, String> {
    // Reject implementations that don't fit the block type (e.g. NAM on a Delay)
    // before touching a loader.
    block.validate()?;
    if block.is_nam() {
        {
            // Installed bytes first (the browser has nothing else; a native
            // host may pre-load too), else the file the block names.
            let mut nam = match crate::assets::get(&block.nam) {
                Some(bytes) => NamProcessor::from_bytes(
                    &bytes,
                    block.nam.clone(),
                    sample_rate as f64,
                    MAX_BLOCK,
                )?,
                #[cfg(not(target_arch = "wasm32"))]
                None => NamProcessor::load(&block.nam, sample_rate as f64, MAX_BLOCK)?,
                #[cfg(target_arch = "wasm32")]
                None => return Err(format!("NAM model not loaded: {}", block.nam)),
            };
            let size = crate::nam::model_size();
            if size < 1.0 {
                nam.set_slimmable_size(size);
            }
            nam.input_gain_db = block.input_trim_db;
            nam.output_gain_db = block.output_trim_db;
            if let Some(iface) = crate::nam::interface_calibration_dbu() {
                let (cin, cout) =
                    crate::nam::calibration_for(nam.input_level(), nam.output_level(), iface);
                nam.calibration_in_db = cin;
                nam.calibration_out_db = cout;
            }
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
        }
    } else if block.is_cab_ir() {
        let conv = match crate::assets::get(&block.ir) {
            Some(bytes) => Convolver::from_bytes(&bytes, &block.ir)?,
            None => Convolver::load(&block.ir)?,
        };
        let dn = conv.display_name.clone();
        Ok(BuiltBlock::plain(Box::new(conv), format!("{dn} (cab)")))
    } else if block.is_plugin() {
        // wasm32: no dynamic plugin loading (libloading / native dylibs).
        #[cfg(target_arch = "wasm32")]
        return Err(format!(
            "hosted CLAP/VST3 plugins are not available in the browser build: {}",
            block.plugin
        ));
        #[cfg(not(target_arch = "wasm32"))]
        {
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
        }
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
        // (synth blocks + the built-in FX in `fx-blocks`). `native_dsp_available`
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
/// sampler TUI's loading path (`PlayerPatch::load/from_pack` +
/// `SampleEngine::new`) — plus its display name. The render tree holds this
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
    // In-memory packs win over the filesystem: a pack installed under this
    // spec-path key (see `crate::pack_registry`) supplies all audio — the
    // browser path (fetched bytes, no filesystem), also usable natively.
    let registered = crate::pack_registry::get(&block.sample);
    #[cfg(not(target_arch = "wasm32"))]
    let patch = if let Some(pack) = registered {
        crate::PlayerPatch::from_opened_pack(pack)
            .map_err(|e| format!("load in-memory sample pack {}: {e}", block.sample))?
    } else if is_pack {
        crate::PlayerPatch::from_pack(spec_path)
            .map_err(|e| format!("load sample pack {}: {e}", block.sample))?
    } else {
        crate::PlayerPatch::load(spec_path, &root)
            .map_err(|e| format!("load sample library {}: {e}", block.sample))?
    };
    #[cfg(target_arch = "wasm32")]
    let patch = {
        let _ = (is_pack, &root);
        let Some(pack) = registered else {
            return Err(format!(
                "no in-memory pack installed for {} — attach its bytes first (pack_registry::install)",
                block.sample
            ));
        };
        crate::PlayerPatch::from_opened_pack(pack)
            .map_err(|e| format!("load in-memory sample pack {}: {e}", block.sample))?
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
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(err) = std::thread::Builder::new()
        .name(format!("signal-preload:{label}"))
        .spawn(move || {
            let stats = cache.preload(paths.iter().map(std::path::PathBuf::as_path));
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
    // wasm32: no streamer thread, so this runs synchronously on the caller —
    // which in the AudioWorklet IS the audio thread. It stays because for a
    // STREAMING pack a "preload" is a head (~48 KB) plus an index, not a
    // decoded sample, and it is what puts the zone in the cache at all: the
    // render DROPS a voice whose sample is not loaded, so skipping this is
    // not a fast rig, it is a SILENT one (learned the hard way — W12).
    //
    // What must never happen here is a full decode. That is the decoder
    // worker's job (it replaces streamed entries with resident PCM
    // off-thread); this only opens them.
    #[cfg(target_arch = "wasm32")]
    {
        // BOUNDED, coverage-first. `paths` is already in playable order
        // (middle-out from middle C), and opening one zone costs a decoded
        // head — ~12k frames, a couple of hundred KB. Opening ALL of them
        // for a nine-lane rig measured 3.8 SECONDS on the audio thread and
        // 1.5 GB resident, i.e. the entire wasm PCM budget consumed before
        // a note was played. Neither is a browser-shaped cost.
        //
        // So open enough to be instantly playable around the middle of the
        // keyboard and let the rest open on demand: a note whose zone is
        // not open queues a warm request, which the streamer thread (W13)
        // services off the audio thread. First press of a far zone lands a
        // touch late; nothing stalls, and residency stays bounded.
        const WASM_EAGER_ZONES: usize = 2;
        let eager = paths.len().min(WASM_EAGER_ZONES);
        let stats = cache.preload(paths.iter().take(eager).map(|p| p.as_path()));
        tracing::debug!(
            library = %label,
            opened = stats.loaded,
            of_total = paths.len(),
            failed = stats.failed,
            "sample block opened (wasm: bounded head set; streamer opens the rest on demand)"
        );
    }
    // A first-class Sample Soundsource.
    Ok((crate::SamplerInstrument::new(engine), name))
}

/// A resident chain: its pre-built + prepared boxes (one per chain slot, in
/// order) plus its control-side [`SlotInfo`]. Switching to it inserts these
/// boxes into the track's slot guids.
#[cfg(not(target_arch = "wasm32"))]
struct ResidentChain {
    #[allow(dead_code)]
    info: SlotInfo,
    /// Boxes for chain slots `0..boxes.len()`. Taken out (`Option`) when active.
    boxes: Vec<Option<Box<dyn PluginInstance>>>,
    /// Stable per-block id for each chain slot, parallel to `boxes`. Lets the
    /// live-rig layer address a running block by id (snapshot / per-block
    /// bypass / per-block param). Defaults to the block's file stem.
    block_ids: Vec<String>,
    /// Which slots must be re-prepared when this chain is armed, parallel to
    /// `boxes` — the ones holding time-based state.
    ///
    /// Arming used to re-prepare every slot, which is how a switch came to
    /// cost 40 ms: three NAM blocks at ~9.7 ms each rebuild their networks,
    /// and a neural amp has no tail to clear. Preparing exists to stop a
    /// delay line or reverb tail from the chain's last activation dumping out
    /// as a burst on the switch, so it is the time blocks — ~0.5 ms each — that
    /// need it and nothing else.
    prepare_on_arm: Vec<bool>,
    /// Each block's type and built-in-ness, parallel to `boxes`.
    kinds: Vec<(BlockType, bool)>,
    /// Each block's bypass, parallel to `boxes` — the chain's own, so it is
    /// set before the chain plays and travels with it into a tail.
    gates: Vec<Arc<crate::block_gate::GateCtl>>,
    /// Where the Time section starts (`boxes.len()`: none).
    time_start: usize,
}

/// A chain taken out of the rig by [`GuitarRig::retire_chain`]: whatever of
/// its blocks were resident, freed when this drops.
#[cfg(not(target_arch = "wasm32"))]
pub struct RetiredChain {
    _chain: ResidentChain,
}

/// Mutable swap state shared behind a [`Mutex`] so the patch-switch surface
/// ([`set_active`](GuitarRig::set_active) / bypass / trims) can stay `&self`
/// (the original API), while installs (`&mut self`) lock it briefly. The
/// resident chains live here because both install (write) and activate (swap)
/// touch them; the actual box-swap goes through `daw.insert_plugin_instance`,
/// which is itself `&self`.
#[cfg(not(target_arch = "wasm32"))]
struct SwapState {
    /// Resident chains keyed by [`ModelId`].
    chains: std::collections::HashMap<ModelId, ResidentChain>,
    /// Currently-active chain id, or [`None`] (clean DI passthrough).
    active: Option<ModelId>,
    /// The chain whose blocks are in the engine now — `active`, unless the
    /// rig is bypassed (then identities play and `live` is `None`).
    live: Option<ModelId>,
    /// Patch-level trims (dB).
    input_trim_db: f32,
    output_trim_db: f32,
    /// The playing patch's output level (dB), applied in the output stage.
    patch_trim_db: f32,
    bypass: bool,
}

/// A live guitar rig: a single input-armed daw track whose FX chain is the
/// active patch, running on daw's realtime `AudioEngine`.
#[cfg(not(target_arch = "wasm32"))]
pub struct GuitarRig {
    daw: Standalone,
    // The shared daw host (project + realtime engine + transport); drop =
    // stop audio. `None` for an offline rig (see `open_offline`).
    _host: Option<DuplexRigHost>,
    /// The offline rig's renderer and playhead — the same `ProjectRenderer`
    /// the realtime callback drives, pulled here by `render_offline` instead
    /// of by an audio device. `None` for a live rig.
    offline: Option<std::sync::Mutex<(daw::standalone::audio_engine::render::ProjectRenderer, u64)>>,
    /// Live realtime metrics (render time / block size) from the duplex engine,
    /// driving the rig's DSP-load meter. `None` under the cpal fallback.
    engine_stats: Option<Arc<EngineStats>>,
    meters: Arc<Meters>,
    track_guid: String,
    /// Fixed fx-guids for the chain slots (constant for the project's life).
    slot_guids: Vec<String>,
    /// Input-meter atomic written by the `InputProbe` at slot 0.
    input_meter: Arc<InputMeterShared>,
    /// The chain's real output (the `OutputTap` after every chain slot).
    output_tap: Arc<OutputTapShared>,
    /// The output tap's fx guid — where the tail stage lives.
    output_guid: String,

    pub sample_rate: u32,
    /// Mutable swap state (resident chains + active selection + trims/bypass).
    swap: std::sync::Mutex<SwapState>,
    /// Control-side mirror of installed chains (for the UI), in install order.
    slots: Vec<SlotInfo>,
    next_id: ModelId,
    prefs: RigAudioPrefs,
}

/// Project/track names for the rig's tiny daw project.
#[cfg(not(target_arch = "wasm32"))]
const RIG_PROJECT_NAME: &str = "Signal Guitar Rig";
#[cfg(not(target_arch = "wasm32"))]
const RIG_TRACK_NAME: &str = "Guitar In";

#[cfg(not(target_arch = "wasm32"))]
impl GuitarRig {
    /// Open the system default input + output devices.
    ///
    /// # Errors
    ///
    /// Returns an error if the default audio devices cannot be opened or configured.
    pub fn new() -> eyre::Result<Self> {
        Self::open(&RigAudioPrefs::default())
    }

    /// Back-compat: open by device substring on input channel 0.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified audio devices cannot be opened or configured.
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
            allow_builtin_mic: false,
            ..RigAudioPrefs::default()
        })
    }

    /// Open the rig from [`RigAudioPrefs`]. Builds a one-track daw project (the
    /// track armed to monitor `prefs.input_channel`), starts daw's realtime
    /// `AudioEngine` with live input, reserves the track's FX slots, and
    /// begins transport so the renderer runs every block.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio engine cannot be started or the project cannot be set up.
    pub fn open(prefs: &RigAudioPrefs) -> eyre::Result<Self> {
        let (project, track_guid, slot_guids, output_tap_guid) =
            Self::seed(prefs.input_channel as u32)?;

        // 3. Open the duplex realtime engine (`prefs.into()` carries the
        //    device/routing config; the host forces `want_input` on) and the
        //    per-track meter bank (one cell, post-fader output peak).
        let host = project.start_duplex(&prefs.into())?;
        let sample_rate = host.sample_rate();
        let engine_stats = host.stats();
        let meters = host.install_meters(1);
        let daw = host.daw().clone();

        let (input_meter, output_tap, output_guid) =
            Self::populate(&daw, sample_rate, &slot_guids, output_tap_guid);

        // 5. Roll the transport so the renderer runs (and the live input flows
        //    through the chain to master) every block.
        host.play();

        // Tracing only — never println/eprintln here: the live-rig TUI owns the
        // terminal, and a stray write to stdout/stderr corrupts the render. A
        // re-open (device/latency change) calls this while the TUI is up.
        tracing::info!(
            input_channel = prefs.input_channel,
            sample_rate,
            project = %host.project_guid(),
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
            phones_mixer: prefs.phones_mixer,
            allow_builtin_mic: prefs.allow_builtin_mic,
            input_calibration_dbu: prefs.input_calibration_dbu,
            nam_calibration_off: prefs.nam_calibration_off,
        };

        Ok(Self::assemble(
            daw,
            Some(host),
            None,
            engine_stats,
            meters,
            track_guid,
            slot_guids,
            input_meter,
            output_tap,
            output_guid,
            sample_rate,
            effective,
        ))
    }

    /// The rig with no audio device: the identical project, track, slots,
    /// probe, output tap and — once chains are installed — chains as
    /// [`open`](Self::open), rendered by the same `ProjectRenderer` the
    /// realtime callback drives, only pulled by
    /// [`render_offline`](Self::render_offline) as fast as the CPU allows.
    ///
    /// This is how a patch is measured *offline*: not by a second chain
    /// runner that has to be kept in step with the live one (the old one
    /// drifted by up to 18 dB on a dimed amp through an IR), but by this one.
    /// Input comes from [`start_test_signal`](Self::start_test_signal),
    /// output from the output tap — both exactly where the live rig has them.
    ///
    /// # Errors
    ///
    /// If the project or its slots cannot be set up.
    pub fn open_offline(sample_rate: u32) -> eyre::Result<Self> {
        let (project, track_guid, slot_guids, output_tap_guid) = Self::seed(0)?;
        let daw = project.daw().clone();
        let meters = Meters::new(1);
        daw.set_meters(meters.clone());
        let (input_meter, output_tap, output_guid) =
            Self::populate(&daw, sample_rate, &slot_guids, output_tap_guid);
        let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(
            &daw,
            project.project_guid(),
            sample_rate,
        );
        let prefs = RigAudioPrefs {
            sample_rate,
            ..RigAudioPrefs::default()
        };
        Ok(Self::assemble(
            daw,
            None,
            Some(std::sync::Mutex::new((renderer, 0))),
            None,
            meters,
            track_guid,
            slot_guids,
            input_meter,
            output_tap,
            output_guid,
            sample_rate,
            prefs,
        ))
    }

    /// Steps 1–2, shared by both engines: the one-track project, armed, with
    /// its fixed FX slots.
    fn seed(input_channel: u32) -> eyre::Result<(RigProject, String, Vec<String>, String)> {
        // 1. Seed a one-track project (current, so the FX-chain service
        //    targets it); arm the track to monitor the hardware input channel.
        let project = RigProject::new(RIG_PROJECT_NAME);
        let track_guid = project.add_track(RIG_TRACK_NAME)?;
        project.arm_input(&track_guid, input_channel)?;

        // 2. Reserve the fixed FX slots on the track (constant guids). Slot 0
        //    is the input probe; the rest start as identity pass-throughs.
        let mut slot_guids = Vec::with_capacity(MAX_CHAIN_SLOTS + 1);
        for i in 0..=MAX_CHAIN_SLOTS {
            slot_guids.push(project.add_fx_slot(&track_guid, &format!("rig-slot-{i}"))?);
        }
        // After every chain slot, and kept out of `slot_guids` so nothing that
        // walks the chain slots ever sees it.
        let output_tap_guid = project.add_fx_slot(&track_guid, "rig-output-tap")?;
        Ok((project, track_guid, slot_guids, output_tap_guid))
    }

    /// Step 4, shared by both engines: input probe at 0, identities in the
    /// chain slots, the output tap after them.
    fn populate(
        daw: &Standalone,
        sample_rate: u32,
        slot_guids: &[String],
        output_tap_guid: String,
    ) -> (Arc<InputMeterShared>, Arc<OutputTapShared>, String) {
        let input_meter = Arc::new(InputMeterShared::default());
        let share = crate::tail_stage::InputShare::new(MAX_BLOCK);
        {
            let mut probe = InputProbe::new(input_meter.clone()).with_share(share.clone());
            let _ = probe.prepare(sample_rate as f64, FX_PREPARE_BLOCK);
            daw.insert_plugin_instance(slot_guids[0].clone(), Box::new(probe));
            for guid in &slot_guids[1..] {
                let mut id = Identity::new();
                let _ = id.prepare(sample_rate as f64, FX_PREPARE_BLOCK);
                daw.insert_plugin_instance(guid.clone(), Box::new(id));
            }
        }
        let output_tap = Arc::new(OutputTapShared::default());
        daw.insert_plugin_instance(
            output_tap_guid.clone(),
            Box::new(OutputTap {
                shared: output_tap.clone(),
                prepared: true,
                stage: crate::tail_stage::TailStage::new(share, f64::from(sample_rate), MAX_BLOCK),
            }),
        );
        (input_meter, output_tap, output_tap_guid)
    }

    #[expect(clippy::too_many_arguments, reason = "one constructor, two engines")]
    fn assemble(
        daw: Standalone,
        host: Option<DuplexRigHost>,
        offline: Option<std::sync::Mutex<(daw::standalone::audio_engine::render::ProjectRenderer, u64)>>,
        engine_stats: Option<Arc<EngineStats>>,
        meters: Arc<Meters>,
        track_guid: String,
        slot_guids: Vec<String>,
        input_meter: Arc<InputMeterShared>,
        output_tap: Arc<OutputTapShared>,
        output_guid: String,
        sample_rate: u32,
        prefs: RigAudioPrefs,
    ) -> Self {
        Self {
            daw,
            _host: host,
            offline,
            engine_stats,
            meters,
            track_guid,
            slot_guids,
            input_meter,
            output_tap,
            output_guid,
            sample_rate,
            swap: std::sync::Mutex::new(SwapState {
                chains: std::collections::HashMap::new(),
                active: None,
                live: None,
                input_trim_db: 0.0,
                output_trim_db: 0.0,
                patch_trim_db: 0.0,
                bypass: false,
            }),
            slots: Vec::new(),
            next_id: 0,
            prefs,
        }
    }

    /// Whether this rig renders offline (no audio device).
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.offline.is_some()
    }

    /// Render `frames` through the offline rig (no-op for a live one).
    pub fn render_offline(&self, frames: usize) {
        /// Well under the slots' prepared maximum (`FX_PREPARE_BLOCK`), and
        /// the size a realtime callback would plausibly use.
        const BLOCK: usize = 512;
        let Some(offline) = &self.offline else { return };
        let Ok(mut guard) = offline.lock() else { return };
        let (renderer, playhead) = &mut *guard;
        let mut left = frames;
        while left > 0 {
            let n = left.min(BLOCK);
            let _ = renderer.render_block(*playhead, n);
            *playhead += n as u64;
            left -= n;
        }
    }

    /// The next `frames` of the chain's real output, `(left, right)` — the
    /// one measurement both engines share. Offline, rendered on the spot;
    /// live, captured as the device plays (blocks until it is, or `timeout`).
    pub fn measure_output(&self, frames: usize, timeout: std::time::Duration) -> (Vec<f32>, Vec<f32>) {
        self.arm_output_capture(frames);
        if self.is_offline() {
            self.render_offline(frames);
        } else {
            let deadline = std::time::Instant::now() + timeout;
            while self.output_capture_remaining() > 0 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        self.take_output_capture()
    }

    /// List available input devices (name + channel count + native rate).
    #[must_use]
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
    #[must_use]
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
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        daw::standalone::audio_engine::PhonesBus::shared().set(volume, self_mix);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = (volume, self_mix); // cpal fallback: no routed phones bus yet
    }

    /// Whether the engine blends the monitor-mix inputs into the phones
    /// itself — off while the separate headphone mixer (`signal-phones`)
    /// plays the mix.
    pub fn set_phones_blend(on: bool) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        daw::standalone::audio_engine::PhonesBus::shared().set_blend_mix(on);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = on;
    }

    /// Mute the main output pair only (routed interfaces): the phones keep
    /// the signal.
    pub fn set_main_pair_mute(on: bool) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        daw::standalone::audio_engine::PhonesBus::shared().set_main_mute(on);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = on;
    }

    /// Build an FX chain on **this** thread (loading every `.nam` / `.wav` /
    /// plugin, prepared at the rig's sample rate) and store it resident.
    /// Returns its [`ModelId`]. Does **not** activate it — call
    /// [`set_active`](Self::set_active). A failed block load fails the whole
    /// install.
    ///
    /// # Errors
    ///
    /// Returns an error if any block fails to load, validate, or prepare.
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
    ///
    /// # Errors
    ///
    /// Returns an error if any block fails to load, validate, or prepare, or if the
    /// chain exceeds the maximum number of slots.
    pub fn install_chain_with_ids(
        &mut self,
        blocks: &[RigBlock],
        block_ids: &[String],
    ) -> Result<ModelId, String> {
        let prepared = prepare_chain(blocks, block_ids, self.sample_rate)?;
        Ok(self.install_prepared(prepared))
    }

    /// Install an already-[`prepare_chain`]d chain. Does not activate.
    ///
    /// This is the cheap half of installing: a handful of moves and one
    /// insert under the swap lock. All the expensive work — model loads,
    /// DSP allocation, `prepare()` — already happened in [`prepare_chain`],
    /// which is why the two are separable in the first place.
    pub fn install_prepared(&mut self, prepared: PreparedChain) -> ModelId {
        let PreparedChain {
            boxes,
            names,
            ids,
            prepare_on_arm,
            kinds,
            gates,
            time_start,
            primary_loudness,
            primary_expected_sr,
            primary_input_level_dbu,
            primary_output_level_dbu,
        } = prepared;

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
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .chains
            .insert(
                id,
                ResidentChain {
                    info,
                    boxes,
                    block_ids: ids,
                    prepare_on_arm,
                    kinds,
                    gates,
                    time_start,
                },
            );
        id
    }

    /// Convenience: install a single-NAM chain (amp only). Does not activate.
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded or prepared.
    pub fn install_model(&mut self, path: impl AsRef<Path>) -> Result<ModelId, String> {
        let p = path.as_ref().to_string_lossy().to_string();
        self.install_chain(&[RigBlock::nam(p)])
    }

    /// Remove a resident chain. If it was active, falls back to passthrough.
    pub fn uninstall_model(&mut self, id: ModelId) {
        if self.active() == Some(id) {
            self.set_active(None);
        }
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .chains
            .remove(&id);
        self.slots.retain(|s| s.id != id);
    }

    /// Take a chain out of the rig without a gap — the gapless counterpart
    /// of [`uninstall_model`](Self::uninstall_model), for a chain a reload no
    /// longer references.
    ///
    /// Refuses (returns `None`, keeps it) the chain in the engine now:
    /// switch away from it first, and it rings out as a tail. A chain still
    /// ringing is safe to retire — its blocks are not here but in the output
    /// stage's voice, which owns them until the tail is quiet; the next
    /// switch collects that voice on the control thread and, the chain being
    /// gone, drops it there. So nothing a tail voice is rendering is dropped,
    /// and nothing is dropped on the audio thread.
    ///
    /// The returned chain holds whatever blocks were resident; drop it
    /// wherever freeing them is cheapest (off any lock the caller holds).
    pub fn retire_chain(&mut self, id: ModelId) -> Option<RetiredChain> {
        let chain = {
            let mut swap = self
                .swap
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if swap.live == Some(id) {
                return None;
            }
            if swap.active == Some(id) {
                // Bypassed rig remembering this chain: forget it.
                swap.active = None;
            }
            swap.chains.remove(&id)?
        };
        self.slots.retain(|s| s.id != id);
        Some(RetiredChain { _chain: chain })
    }

    /// The chain in the engine now (`None`: passthrough or bypassed).
    #[must_use]
    pub fn live(&self) -> Option<ModelId> {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
    }

    /// Write param changes to chain `id`'s running blocks — no rebuild, no
    /// switch: `(slot, write)` with slots in chain order.
    ///
    /// Wherever the chain's blocks are: in the engine (under the renderer's
    /// lock, all writes in one hold, so no block renders half an edit),
    /// resident, or ringing out in a tail. A built-in effect gets its params
    /// as param events — exactly the `(id, value)` its build set from the
    /// same stored params (see [`crate::block_params`]) — a NAM block its
    /// trims. Everything is resolved before any lock the renderer takes.
    ///
    /// `false` if the rig holds no chain `id`.
    pub fn write_chain_blocks(
        &self,
        id: ModelId,
        writes: &[(usize, crate::block_params::BlockWrite)],
    ) -> bool {
        self.write_chain_blocks_with(id, writes, false)
    }

    /// [`write_chain_blocks`](Self::write_chain_blocks) to a chain that has
    /// never played (just built): each written block is prepared again after,
    /// so it starts *at* the written values rather than gliding to them from
    /// its build's — exactly as if it had been built with them. A chain that
    /// is playing or ringing is written as usual (a glide is what a playing
    /// block should do).
    pub fn write_new_chain_blocks(
        &self,
        id: ModelId,
        writes: &[(usize, crate::block_params::BlockWrite)],
    ) -> bool {
        self.write_chain_blocks_with(id, writes, true)
    }

    fn write_chain_blocks_with(
        &self,
        id: ModelId,
        writes: &[(usize, crate::block_params::BlockWrite)],
        settle: bool,
    ) -> bool {
        let sr = f64::from(self.sample_rate);
        let mut swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let live = swap.live == Some(id);
        let Some(chain) = swap.chains.get_mut(&id) else {
            return false;
        };
        let resolved: Vec<(usize, crate::block_params::ResolvedWrite)> = writes
            .iter()
            .filter_map(|(slot, w)| {
                let (bt, _) = *chain.kinds.get(*slot)?;
                crate::block_params::ResolvedWrite::resolve(bt, w).map(|sw| (*slot, sw))
            })
            .collect();
        if resolved.is_empty() {
            return true;
        }
        if live {
            let guids = &self.slot_guids;
            self.daw.with_plugin_instances(|map| {
                for (slot, sw) in &resolved {
                    if let Some(inst) = guids.get(slot + 1).and_then(|g| map.get_mut(g)) {
                        sw.apply(inst.as_mut());
                    }
                }
            });
        } else if chain.boxes.iter().any(Option::is_some) {
            for (slot, sw) in &resolved {
                if let Some(Some(inst)) = chain.boxes.get_mut(*slot) {
                    sw.apply(inst.as_mut());
                }
            }
            if settle {
                let mut slots: Vec<usize> = resolved.iter().map(|(s, _)| *s).collect();
                slots.dedup();
                for slot in slots {
                    if let Some(Some(inst)) = chain.boxes.get_mut(slot) {
                        let _ = inst.prepare(sr, FX_PREPARE_BLOCK);
                    }
                }
            }
        } else {
            // Ringing out: its blocks are the tail's.
            self.with_stage(|stage| {
                if let Some(boxes) = stage.voice_boxes_mut(id) {
                    for (slot, sw) in &resolved {
                        if let Some(Some(inst)) = boxes.get_mut(*slot) {
                            sw.apply(inst.as_mut());
                        }
                    }
                }
            });
        }
        true
    }

    /// Chain `id`'s block ids, in slot order (empty if the rig has no such
    /// chain).
    #[must_use]
    pub fn chain_block_ids(&self, id: ModelId) -> Vec<String> {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .chains
            .get(&id)
            .map(|c| c.block_ids.clone())
            .unwrap_or_default()
    }

    /// Select the active chain — swaps the chain's pre-prepared boxes into the
    /// track's fixed slot guids under daw's renderer per-block lock
    /// (glitch-free), filling unused slots with identity pass-throughs. `None`
    /// = clean DI passthrough (all chain slots → identity). The old boxes are
    /// dropped on this (control) thread, off the audio thread. `&self` (via an
    /// internal `Mutex`) so footswitch / UI paths needn't hold the rig `&mut`.
    /// # Panics
    ///
    /// Panics if a chain GUID recorded in `slot_guids` is missing from
    /// `swap.chains`, or if a slot marked present turns out to be empty —
    /// both are internal invariants, not input errors.
    pub fn set_active(&self, id: Option<ModelId>) {
        let mut swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let chain_guids = &self.slot_guids[1..]; // slot 0 is the input probe
        let sr = self.sample_rate as f64;
        let bypass = swap.bypass;
        let known = id.filter(|i| swap.chains.contains_key(i));
        let arming = known.filter(|_| !bypass);
        let trim_db = swap.patch_trim_db;
        swap.active = match arming {
            Some(cid) => Some(cid),
            // When bypassed, remember the requested (known) id so toggling
            // bypass off re-arms it; otherwise we're cleanly passthrough.
            None if bypass => known,
            None => None,
        };

        // Already what plays: nothing to swap — only the level.
        if swap.live == arming {
            drop(swap);
            self.with_stage(|stage| stage.set_trim_db(trim_db));
            return;
        }

        // ── Phase 1: allocate and prepare, touching nothing the audio thread
        // can see.
        //
        // `prepare` is where a switch spends its time — a reverb sizes its
        // delay lines, a NAM resets its network — and all of it used to happen
        // *between* the first slot being swapped and the last. Nothing here is
        // visible to the renderer, so it can take as long as it takes.

        // The incoming boxes, one per slot. A chain that is still ringing out
        // from an earlier switch has no boxes here — they are in its voice,
        // and come back out of it in phase 2 as they are (`None` below marks
        // those slots). Otherwise the time blocks are re-prepared, clearing
        // what they held from the chain's last activation.
        let mut resume = false;
        let mut incoming: Vec<Option<Box<dyn PluginInstance>>> = match arming {
            Some(cid) => {
                let chain = swap.chains.get_mut(&cid).expect("checked contains_key");
                resume = !chain.boxes.is_empty() && chain.boxes.iter().all(Option::is_none);
                (0..chain_guids.len())
                    .map(|slot| {
                        if slot >= chain.boxes.len() {
                            return Some(Self::fresh_identity(sr));
                        }
                        if resume {
                            return None;
                        }
                        let mut new_box = chain.boxes[slot]
                            .take()
                            .unwrap_or_else(|| Self::fresh_identity(sr));
                        // Only where a tail could survive; see `prepare_on_arm`.
                        if chain.prepare_on_arm.get(slot).copied().unwrap_or(true) {
                            let _ = new_box.prepare(sr, FX_PREPARE_BLOCK);
                            // Cleared, it would hear the guitar from the
                            // first sample at full level: its first repeat
                            // or reflection lands as a step after the
                            // crossfade is over. Ramp its input in instead.
                            if let Some(gate) = chain.gates.get(slot) {
                                gate.fade_in();
                            }
                        }
                        Some(new_box)
                    })
                    .collect()
            }
            // Bypassed, or an id the rig does not hold: clean passthrough.
            None => chain_guids
                .iter()
                .map(|_| Some(Self::fresh_identity(sr)))
                .collect(),
        };
        // Where the outgoing chain's blocks will go: its voice, and what the
        // stage hands back (finished tails, one made room for).
        let outgoing = swap.live.and_then(|prev| {
            swap.chains
                .get(&prev)
                .map(|c| (prev, c.boxes.len(), c.time_start))
        });
        let mut voice_boxes: Vec<Option<Box<dyn PluginInstance>>> =
            Vec::with_capacity(outgoing.map_or(0, |(_, n, _)| n));
        let mut displaced: Vec<Option<Box<dyn PluginInstance>>> =
            Vec::with_capacity(chain_guids.len());
        let mut returned: Vec<crate::tail_stage::Voice> =
            Vec::with_capacity(crate::tail_stage::MAX_VOICES + 1);
        let resume_id = arming.filter(|_| resume);
        let stage_guid = self.output_guid.as_str();

        // ── Phase 2: the swap — one hold of the renderer's lock, so no block
        // ever renders half of one chain and half of another. Inserts and
        // moves only; everything displaced is dropped after.
        let resumed_ok = self.daw.with_plugin_instances(|map| {
            // The stage's box out of the map while the slots change around
            // it, and back in after — no allocation: the key is re-used.
            let mut tap_box = map.remove(stage_guid);
            let stage = tap_box
                .as_mut()
                .and_then(|p| p.as_any_mut())
                .and_then(|a| a.downcast_mut::<OutputTap>())
                .map(|tap| &mut tap.stage);
            let Some(stage) = stage else {
                // No stage (should not happen): the old swap, no tails.
                for (guid, bx) in chain_guids.iter().zip(incoming.iter_mut()) {
                    if let Some(bx) = bx.take() {
                        displaced.push(map.insert(guid.clone(), bx));
                    }
                }
                if let Some(t) = tap_box {
                    map.insert(stage_guid.to_string(), t);
                }
                return false;
            };
            stage.collect_finished(&mut returned);
            let mut resumed = false;
            if let Some(cid) = resume_id {
                // Still ringing, or finished and just collected: either way
                // its blocks come back here, as they are.
                let finished = returned
                    .iter()
                    .position(|v| v.chain == cid)
                    .map(|i| returned.swap_remove(i));
                if let Some(v) = finished.or_else(|| stage.take(cid)) {
                    for (slot, bx) in v.boxes.into_iter().enumerate() {
                        if let Some(dst) = incoming.get_mut(slot) {
                            *dst = bx;
                        }
                    }
                    resumed = true;
                }
            }
            // A ringing chain whose voice was not there (finished and
            // collected above, say): its blocks came back to `returned`,
            // not here — play identities rather than leave the old chain in.
            for bx in incoming.iter_mut().filter(|b| b.is_none()) {
                *bx = Some(Self::fresh_identity(sr));
            }
            for (slot, (guid, bx)) in chain_guids.iter().zip(incoming.iter_mut()).enumerate() {
                let Some(bx) = bx.take() else { continue };
                let old = map.insert(guid.clone(), bx);
                match outgoing {
                    Some((_, n, _)) if slot < n => voice_boxes.push(old),
                    _ => displaced.push(old),
                }
            }
            let voice = outgoing.map(|(prev, _, ts)| {
                crate::tail_stage::Voice::new(prev, std::mem::take(&mut voice_boxes), ts)
            });
            if let Some(v) = stage.switch(voice, trim_db) {
                returned.push(v);
            }
            if let Some(t) = tap_box {
                map.insert(stage_guid.to_string(), t);
            }
            resumed
        });
        if resume && !resumed_ok {
            tracing::warn!(chain = ?arming, "rig switch: a ringing chain's blocks were not found; playing it dry");
        }

        // Blocks back to their chains; anything whose chain has gone, and
        // the identities, dropped here — off the audio thread.
        for v in returned {
            if let Some(chain) = swap.chains.get_mut(&v.chain) {
                for (slot, bx) in v.boxes.into_iter().enumerate() {
                    if let Some(dst) = chain.boxes.get_mut(slot) {
                        if dst.is_none() {
                            *dst = bx;
                        }
                    }
                }
            }
        }
        drop(displaced);
        swap.live = arming;
    }

    /// Run `f` on the output stage, under the renderer's lock.
    fn with_stage<R>(&self, f: impl FnOnce(&mut crate::tail_stage::TailStage) -> R) -> Option<R> {
        self.daw
            .with_plugin_instance(&self.output_guid, |p| {
                p.as_any_mut()
                    .and_then(|a| a.downcast_mut::<OutputTap>())
                    .map(|tap| f(&mut tap.stage))
            })
            .flatten()
    }

    /// Tails ringing on from earlier switches (diagnostics).
    #[must_use]
    pub fn tail_voices(&self) -> usize {
        self.with_stage(|s| s.ringing()).unwrap_or(0)
    }

    /// The playing patch's output level (dB). Applied in the output stage —
    /// smoothed, and captured by a switch so the outgoing patch rings out at
    /// its own level. Set it before [`set_active`](Self::set_active) and the
    /// switch lands it with the crossfade.
    pub fn set_patch_trim_db(&self, db: f32) {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .patch_trim_db = db;
    }

    /// Set chain `id`'s bypass mask (one flag per block, in chain order):
    /// at once when it is not playing, ramped when it is.
    pub fn set_chain_bypass(&self, id: ModelId, mask: &[bool]) {
        let swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let playing = swap.live == Some(id);
        if let Some(chain) = swap.chains.get(&id) {
            for (gate, &on) in chain.gates.iter().zip(mask) {
                // A chain ringing out keeps its gates as they were until it
                // plays again (its boxes are out).
                let ringing = chain.boxes.iter().all(Option::is_none) && !chain.boxes.is_empty();
                if playing || ringing {
                    gate.set(on);
                } else {
                    gate.set_now(on);
                }
            }
        }
    }

    fn fresh_identity(sr: f64) -> Box<dyn PluginInstance> {
        let mut id = Identity::new();
        let _ = id.prepare(sr, FX_PREPARE_BLOCK);
        Box::new(id)
    }

    pub fn active(&self) -> Option<ModelId> {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active
    }

    /// Convenience for the single-amp case: install a single-NAM chain + activate.
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded or prepared.
    ///
    /// # Panics
    ///
    /// Panics if the swap state mutex is poisoned or the just-installed slot cannot be retrieved.
    pub fn load_nam(&mut self, path: impl AsRef<Path>) -> Result<SlotInfo, String> {
        let id = self.install_model(path)?;
        self.set_active(Some(id));
        Ok(self.slots.last().cloned().expect("just installed"))
    }

    /// Remove every chain and fall back to passthrough.
    pub fn clear(&mut self) {
        self.set_active(None);
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .chains
            .clear();
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
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .input_trim_db = db;
        // The input trim is folded into each NAM block's per-block input gain at
        // build time (the resolver sets `RigBlock::input_trim_db`), so the
        // patch-level input trim is a no-op live; kept for API parity. The
        // common path (ProfileRig) sets trims *before* `set_active`, which is
        // where they take effect.
    }

    pub fn set_output_trim_db(&self, db: f32) {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .output_trim_db = db;
        // Output trim → the track's post-fader gain (linear).
        let out_lin = signal_rig_host::mixer::db_to_linear(db) as f64;
        let _ = <Standalone as Tracks>::set_volume(
            &self.daw,
            ProjectContext::Current,
            TrackRef::guid(self.track_guid.as_str()),
            out_lin,
        );
    }

    pub fn set_bypass(&self, bypass: bool) {
        {
            let mut swap = self
                .swap
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bypass
    }

    pub fn input_trim_db(&self) -> f32 {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .input_trim_db
    }

    pub fn output_trim_db(&self) -> f32 {
        self.swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .output_trim_db
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

    /// Replace the instrument with `samples` (mono, at the rig's rate),
    /// looped from the top, until [`stop_test_signal`](Self::stop_test_signal).
    /// The whole live chain — the patch as switched, every trim the backend
    /// applied — hears it exactly as it would the guitar.
    pub fn start_test_signal(&self, samples: Arc<Vec<f32>>) {
        if let Ok(mut slot) = self.input_meter.inject.lock() {
            *slot = Some(samples);
        }
        self.input_meter.inject_gen.fetch_add(1, Ordering::AcqRel);
    }

    /// Back to the instrument.
    pub fn stop_test_signal(&self) {
        if let Ok(mut slot) = self.input_meter.inject.lock() {
            *slot = None;
        }
        self.input_meter.inject_gen.fetch_add(1, Ordering::AcqRel);
    }

    /// Silence the rig's output (the output capture still hears the chain).
    pub fn set_output_muted(&self, muted: bool) {
        self.output_tap.muted.store(muted, Ordering::Relaxed);
    }

    /// Capture the next `frames` of the chain's output (per channel).
    pub fn arm_output_capture(&self, frames: usize) {
        self.arm_capture_at(frames, false);
    }

    /// Capture the next `frames` of what is heard: the patch at its level,
    /// with the tails of earlier patches ringing under it.
    pub fn arm_heard_capture(&self, frames: usize) {
        self.arm_capture_at(frames, true);
    }

    fn arm_capture_at(&self, frames: usize, heard: bool) {
        self.output_tap.heard.store(heard, Ordering::Relaxed);
        if let Ok(mut cap) = self.output_tap.capture.lock() {
            cap.0.clear();
            cap.1.clear();
            cap.0.reserve(frames);
            cap.1.reserve(frames);
        }
        self.output_tap.remaining.store(frames, Ordering::Relaxed);
    }

    /// Frames still to capture (0 once the capture is complete).
    pub fn output_capture_remaining(&self) -> usize {
        self.output_tap.remaining.load(Ordering::Relaxed)
    }

    /// Take the captured output, `(left, right)`.
    pub fn take_output_capture(&self) -> (Vec<f32>, Vec<f32>) {
        self.output_tap
            .capture
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default()
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
            .map_or(0.0, |c| c.peak(0).max(c.peak(1)))
    }

    /// Per-channel output peaks (linear) — stereo metering.
    pub fn output_peak_lr(&self) -> (f32, f32) {
        self.meters
            .cell(0)
            .map_or((0.0, 0.0), |c| (c.peak(0), c.peak(1)))
    }

    /// Realtime xruns. The duplex engine has no input ring (input and output are
    /// the same callback), so ring under/overruns don't exist; graph-deadline
    /// xruns aren't surfaced yet, so this reads 0.
    pub fn underruns(&self) -> u64 {
        self.engine_stats
            .as_ref()
            .map_or(0, |s| s.xruns.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn overruns(&self) -> u64 {
        0
    }

    /// Every dropout the engine has seen since `*seen` (a sequence number,
    /// 0 to start), oldest first — for a drop log. Empty for an offline rig.
    pub fn collect_drops(&self, seen: &mut u64) -> Vec<daw_audio_io::duplex::DropEvent> {
        self.engine_stats
            .as_ref()
            .map(|s| s.drops.collect(seen))
            .unwrap_or_default()
    }

    pub fn installs(&self) -> u64 {
        self.slots.len() as u64
    }

    /// Last block's render time in microseconds (the DSP-load numerator),
    /// measured by the duplex engine's realtime callback.
    pub fn render_us(&self) -> u32 {
        self.engine_stats.as_ref().map_or(0, |s| {
            (s.render_ns.load(std::sync::atomic::Ordering::Relaxed) / 1000) as u32
        })
    }

    pub fn peak_render_us(&self) -> u32 {
        self.engine_stats.as_ref().map_or(0, |s| {
            (s.peak_render_ns.load(std::sync::atomic::Ordering::Relaxed) / 1000) as u32
        })
    }

    /// Mean render time per block, microseconds — the stable measure.
    ///
    /// Peak answers "did we drop audio"; the mean answers "is this build
    /// faster than that one". One preempted block on a shared machine moves
    /// the peak by milliseconds and says nothing about the code, so a
    /// benchmark reads this and a dropout hunt reads the peak.
    pub fn mean_render_us(&self) -> u32 {
        self.engine_stats
            .as_ref()
            .map_or(0, |s| (s.mean_render_ms() * 1000.0) as u32)
    }

    /// Whether the audio device reported itself gone (unplugged, powered
    /// off): the engine is dead and only a reopen brings it back. Always
    /// false for an offline rig.
    pub fn stream_failed(&self) -> bool {
        /// `EngineStats::stream_state`'s error code (both backends).
        const STATE_ERROR: i32 = -1;
        self.engine_stats.as_ref().is_some_and(|s| {
            s.stream_state.load(std::sync::atomic::Ordering::Relaxed) == STATE_ERROR
        })
    }

    /// Blocks rendered since the device opened — the sample count behind
    /// [`mean_render_us`](Self::mean_render_us).
    pub fn blocks_rendered(&self) -> u64 {
        self.engine_stats
            .as_ref()
            .map_or(0, |s| s.calls.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Blocks whose render overran the block's own realtime budget.
    ///
    /// This is the dropout count that is actually ours: a callback that
    /// misses its deadline has already made the next one late, whether or not
    /// the graph gets around to calling it an xrun. Prefer it to
    /// [`underruns`](Self::underruns), which reads the driver's clock and can
    /// sit at zero while the graph drops audio.
    pub fn over_budget(&self) -> u64 {
        self.engine_stats.as_ref().map_or(0, |s| {
            s.over_budget.load(std::sync::atomic::Ordering::Relaxed)
        })
    }

    /// Fraction of the realtime budget the last block consumed (0..=1).
    pub fn dsp_load(&self) -> f32 {
        self.engine_stats
            .as_ref()
            .map_or(0.0, |s| s.load(self.sample_rate) as f32)
    }

    /// Fraction of the budget the MEAN block consumes (0..=1) — the number to
    /// compare across builds, since it does not move with one stalled block.
    pub fn mean_dsp_load(&self) -> f32 {
        let budget_us = f64::from(self.block_frames()) / f64::from(self.sample_rate.max(1)) * 1e6;
        if budget_us <= 0.0 {
            return 0.0;
        }
        ((f64::from(self.mean_render_us()) / budget_us) as f32).clamp(0.0, 1.0)
    }

    pub fn reset_render_peak(&self) {
        if let Some(s) = &self.engine_stats {
            s.reset_peak();
        }
    }

    /// Frames in the last block (the running buffer/quantum) — from the engine's
    /// live metrics, falling back to the configured buffer before the first block.
    pub fn block_frames(&self) -> u32 {
        let live = self.engine_stats.as_ref().map_or(0, |s| {
            s.block_frames.load(std::sync::atomic::Ordering::Relaxed)
        });
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
    /// chain maps to `slot_guids[1]`, the renderer `fx_idx` is `index + 1`).
    fn active_block_slot(&self, block_id: &str) -> Option<(usize, String)> {
        let swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let active = swap.active?;
        let chain = swap.chains.get(&active)?;
        let slot = chain.block_ids.iter().position(|b| b == block_id)?;
        // chain slot 0 → slot_guids[1] (slot_guids[0] is the input probe).
        let guid = self.slot_guids.get(slot + 1)?.clone();
        Some((slot, guid))
    }

    /// Block ids of the active chain, in order. Empty when nothing is active.
    pub fn active_block_ids(&self) -> Vec<String> {
        let swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let swap = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(chain) = swap.active.and_then(|a| swap.chains.get(&a)) else {
            return false;
        };
        let Some(slot) = chain.block_ids.iter().position(|b| b == block_id) else {
            return false;
        };
        // The block's own gate (`block_gate`): ramped, and a delay or reverb
        // rings out rather than being cut.
        match chain.gates.get(slot) {
            Some(gate) => {
                gate.set(on);
                true
            }
            None => false,
        }
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

        // 0. A built-in effect: its own param event, the value as it is —
        // the write its build makes from the same stored value (see
        // `block_params`). Once, to the block: not a value kept on the slot
        // and re-sent every block to whatever chain plays there next.
        let target = {
            let swap = self
                .swap
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            swap.active
                .and_then(|a| swap.chains.get(&a).map(|c| (a, c.kinds.get(slot).copied())))
        };
        if let Some((active, Some((bt, true)))) = target {
            if crate::block_params::is_known(bt, param_name) {
                return self.write_chain_blocks(
                    active,
                    &[(
                        slot,
                        crate::block_params::BlockWrite::Params(vec![(
                            param_name.to_string(),
                            f64::from(value),
                        )]),
                    )],
                );
            }
        }

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
#[must_use]
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
#[cfg(not(target_arch = "wasm32"))]
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| e.to_string())
}

/// Process-unique guid string — now provided by the shared rig host; kept
/// re-exported here for the existing `signal_sampler::rig::uuid_string` path.
pub use signal_rig_host::uuid_string;

#[cfg(test)]
mod tests {
    use super::*;

    /// Render `secs` offline, capturing what is heard.
    fn heard(rig: &GuitarRig, secs: f64) -> Vec<f32> {
        let frames = (secs * f64::from(rig.sample_rate)) as usize;
        rig.arm_heard_capture(frames);
        rig.render_offline(frames);
        rig.take_output_capture().0
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// A reverb's tail rings on after switching to a patch without one —
    /// and the tail is the old patch's, not silence.
    #[test]
    fn a_switch_lets_the_old_reverb_ring_out() {
        let rig = GuitarRig::open_offline(48_000).unwrap();
        let mut rig = rig;
        let verb = RigBlock::effect(BlockType::Reverb, "VERB 1")
            .with_param("mix", "1")
            .with_param("level", "0")
            .with_param("dry", "0")
            .with_param("algorithm", "1")
            .with_param("decay", "0.8");
        let a = rig.install_chain(&[verb]).unwrap();
        let b = rig
            .install_chain(&[RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "0")])
            .unwrap();
        // A 50 ms burst, then silence (the test signal loops: make it long).
        let mut sig = vec![0.0f32; 48_000 * 20];
        let mut seed = 7u32;
        for s in sig.iter_mut().take(2_400) {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5;
        }
        rig.set_active(Some(a));
        rig.start_test_signal(Arc::new(sig));
        let before = heard(&rig, 0.3);
        assert!(rms(&before[4_800..]) > 1e-3, "A's reverb rings before the switch");
        rig.set_active(Some(b));
        let after = heard(&rig, 1.0);
        let late = rms(&after[24_000..]);
        assert!(late > 1e-4, "A's tail still rings 0.5 s after switching to B: {late}");
        assert_eq!(rig.tail_voices(), 1);
    }

    /// Back to a patch whose tail has finished: it plays itself, not dry.
    #[test]
    fn switching_back_after_the_tail_has_ended_plays_the_patch() {
        let mut rig = GuitarRig::open_offline(48_000).unwrap();
        let loud = rig
            .install_chain(&[RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "6")])
            .unwrap();
        let quiet = rig
            .install_chain(&[RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "-6")])
            .unwrap();
        rig.start_test_signal(Arc::new(vec![0.25f32; 48_000]));
        rig.set_active(Some(loud));
        heard(&rig, 0.1);
        rig.set_active(Some(quiet));
        heard(&rig, 0.5);
        rig.set_active(Some(loud));
        let out = heard(&rig, 0.1);
        let last = out[out.len() - 1];
        assert!((last - 0.25 * 1.995).abs() < 0.01, "A at +6 dB again, not dry: {last}");
    }

    /// Two levels, one switch: no step between them.
    #[test]
    fn a_switch_crossfades_the_two_patches() {
        let mut rig = GuitarRig::open_offline(48_000).unwrap();
        let loud = rig
            .install_chain(&[RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "6")])
            .unwrap();
        let quiet = rig
            .install_chain(&[RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "-6")])
            .unwrap();
        // A slow sine: its own sample-to-sample change is tiny, so any step
        // is the switch's.
        let sig: Vec<f32> = (0..48_000 * 4)
            .map(|i| 0.25 * (std::f32::consts::TAU * 50.0 * i as f32 / 48_000.0).sin())
            .collect();
        rig.set_active(Some(loud));
        rig.start_test_signal(Arc::new(sig));
        let mut out = heard(&rig, 0.2);
        rig.set_active(Some(quiet));
        out.extend(heard(&rig, 0.2));
        let steady = 0.25 * 2.0 * std::f32::consts::TAU * 50.0 / 48_000.0;
        let max_jump = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_jump < steady * 1.5, "largest step {max_jump} vs a sine's own {steady}");
    }

    /// A 16-bit mono WAV of `samples` — an IR as the browser would fetch it.
    fn wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut b = Vec::with_capacity(44 + samples.len() * 2);
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&sample_rate.to_le_bytes());
        b.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            b.extend_from_slice(&s.to_le_bytes());
        }
        b
    }

    /// The browser has no filesystem: a chain whose model and IR are only
    /// *registered bytes* (keys that are not paths) still builds and plays.
    #[test]
    fn a_chain_builds_from_registered_bytes_alone() {
        let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rigs/guitar/default-config/models/King of Tone both sides.nam");
        let amp_key = "web-test://amp.nam";
        let cab_key = "web-test://cab.wav";
        crate::assets::install(amp_key, std::fs::read(model).unwrap());
        let mut ir = vec![0i16; 256];
        ir[0] = i16::MAX / 2;
        crate::assets::install(cab_key, wav_bytes(&ir, 48_000));

        let blocks = [
            RigBlock::nam(amp_key).named("Amp L"),
            RigBlock::cab_ir(cab_key).named("Cab L"),
        ];
        let ids = vec!["amp".to_string(), "cab".to_string()];
        let mut chain = prepare_chain(&blocks, &ids, 48_000).expect("builds from bytes");

        let events = signal_plugin_host::PluginEvents::default();
        let input: Vec<f32> = (0..128).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let mut buf = input.clone();
        for slot in chain.boxes.iter_mut().flatten() {
            let (il, ir) = (buf.clone(), buf.clone());
            let (mut ol, mut or) = (vec![0.0; 128], vec![0.0; 128]);
            slot.process_block(&il, &ir, &mut ol, &mut or, &events)
                .unwrap();
            buf = ol;
        }
        assert!(buf.iter().all(|s| s.is_finite()));
        assert!(buf.iter().any(|s| s.abs() > 1e-6), "the chain made sound");

        // An unregistered key that is not a file fails loudly, not silently.
        assert!(prepare_chain(&[RigBlock::nam("web-test://missing.nam")], &[], 48_000).is_err());
        crate::assets::clear();
    }

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

        // …while a Native type with no registry entry (Wah) has no backend
        // yet, so it's skipped at install.
        let pending = RigBlock::effect(BlockType::Wah, "Wah");
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
        use signal_rig_host::mixer::db_to_linear;
        assert_eq!(db_to_linear(0.0), 1.0);
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-4);
    }
}
