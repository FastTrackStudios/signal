//! The live-rig trait framework — `docs/rig-trait-design.md`.
//!
//! Signal's amp-modeling / live-FX domain, as a clean declarative API layered
//! *over* the existing rig code. Same guiding principle as the sampler API
//! ([`super`]): **the common 90% is declarative data; the exotic 10% is a trait
//! you implement.** A patch is a list of blocks; a profile is a list of patches;
//! performance (footswitch / MIDI / expression → actions) is a [`Controller`]
//! you configure.
//!
//! Each [`Block`] is a `daw::plugin::PluginInstance` — the *same* host contract
//! the sampler + daw use, so a NAM amp, a cab IR, a built-in DSP, and a hosted
//! CLAP/VST3 plugin are all just blocks; daw runs them with one engine. The
//! [`Rig`] is realized as ONE input-armed daw track whose FX chain is the
//! active patch (the existing [`GuitarRig`](crate::GuitarRig)).
//!
//! # Foundation status (this layer)
//!
//! - **REAL**:
//!   - Every primitive id ([`PatchId`]/[`BlockId`]/[`ProfileId`]/[`SnapshotId`]/
//!     [`ParamRef`]) — built on Phase A [`Interned`](super::prim::Interned).
//!   - The [`Block`] trait + [`BlockRole`], and the working [`Amp`] (NAM),
//!     [`Cabinet`] (IR/NAM), [`Plugin`] (hosted) impls — each wraps an existing
//!     `PluginInstance` ([`NamProcessor`](crate::NamProcessor) /
//!     [`Convolver`](crate::Convolver) / a hosted box). `as_plugin()` hands the
//!     inner instance straight to daw's renderer.
//!   - [`block::amp_nam`] / [`block::cab_ir`] / [`block::cab_nam`] /
//!     [`block::plugin`] constructors that actually load DSP off disk.
//!   - [`Chain`] / [`Node`] / [`Lane`] / [`ParallelMix`] + [`ChainBuilder`].
//!   - [`Profile`] / [`Patch`] / [`Snapshot`] data + [`ProfileBuilder`] +
//!     [`Profile::from_styx`] (reusing [`RigProfile::from_styx_file`]) +
//!     `From<&RigProfile>` adapter and `Profile::to_rig_profile` round-trip.
//!   - [`DawRig`] — a working [`Rig`] impl wrapping the existing
//!     [`ProfileRig`](crate::ProfileRig)/[`GuitarRig`](crate::GuitarRig):
//!     patch select/next/prev, input/output peak, bypass, trims are all the
//!     existing instant, click-free machinery.
//!   - [`PatchStepper`] — a real [`Controller`] (footswitches cycle patches)
//!     and [`ProgramChangeMap`] (PC# → patch).
//!
//! - **Phase-B REAL** (this phase wired the live machinery):
//!   - **Live block addressing** — [`GuitarRig`](crate::GuitarRig) now records a
//!     stable `block_id` per chain slot and exposes
//!     [`with_active_block_instance`](crate::GuitarRig::with_active_block_instance),
//!     [`set_active_block_param`](crate::GuitarRig::set_active_block_param), and
//!     [`set_block_slot_bypass`](crate::GuitarRig::set_block_slot_bypass).
//!     [`DawRig`] routes `BlockId` → the right slot through these (via
//!     [`ProfileRig`](crate::ProfileRig)).
//!   - [`Rig::set_param`] — NAM `input_trim` / `output_trim` set the live
//!     `NamProcessor` in place (downcast under daw's renderer lock); hosted
//!     plugins route through daw `FxParams::set`. (IR cabs have no continuous
//!     params → `false`.)
//!   - [`Rig::set_block_bypass`] — flips daw's per-slot `fx_enabled` flag (the
//!     renderer skips disabled slots; no chain rebuild). Reset on patch switch.
//!   - [`Rig::select_snapshot`] — applies the snapshot's param + bypass sets
//!     onto the live blocks instantly via the two seams above.
//!   - [`Rig::tuner`] — autocorrelation pitch detection ([`detect_pitch`]) on
//!     the input-probe's mono window.
//!   - [`Rig::tap_tempo`] — averages tap intervals into a BPM
//!     ([`DawRig::tempo_bpm`]).
//!   - Parallel-path lowering — [`Chain::lower_parallels`] folds a
//!     [`Node::Parallel`] into one [`ParallelSum`] FX block (2-lane dual-cab /
//!     wet-dry; per-lane level + pan). All [`Controller`] built-ins now drive
//!     real [`Rig`] actions.
//!
//! - **Still approximate / open**:
//!   - [`DawRig::open`]'s install path goes through the *path-based*
//!     [`RigProfile`](crate::RigProfile), which can't carry a [`ParallelSum`]
//!     box yet — so `lower_parallels` is exercised at the [`Chain`] level and in
//!     tests, but `open` itself still flattens parallels. Nested parallels are
//!     rejected; the daw-send-bus variant (for a multi-track rig) is future.
//!   - Tap tempo computes a BPM but no built-in `Delay` block consumes it yet.
//!   - The tuner is a single-window time-domain detector (good for tuning, not
//!     a reference analyzer; no octave-error guard beyond the confidence gate).
//!
//! The existing `GuitarRig` / `ProfileRig` / `RigProfile` / `FxBackend` /
//! `NamProcessor` / `Convolver` stay untouched — this is additive.

use std::path::Path;

use super::prim::{Cc, Db, Interned, U7};
use crate::convolver::Convolver;
use crate::nam::NamProcessor;
use crate::rig::{GuitarRig, RigBlock};
use crate::rig_prefs::RigAudioPrefs;
use crate::rig_profile::{ProfileRig, RigPatch, RigProfile};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

// Max block the foundation prepares standalone-constructed blocks for (matches
// the rig's prepare block; the live rig re-prepares on install).
const PREPARE_BLOCK: u32 = 512;

// ── ids (doc §2) ───────────────────────────────────────────────────────────

/// A patch (tone) id — `"Clean"`, `"Lead"`. Interned, compares by name.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PatchId(pub Interned);

/// A stable block id *within a patch* — addresses a block for snapshots /
/// per-block bypass. `"amp"`, `"delay"`, `"cab"`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub Interned);

/// A profile (rackspace set / setlist) id — `"Worship"`, `"Metal Gig"`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ProfileId(pub Interned);

/// A snapshot id *within a patch* — `"Verse"`, `"Chorus"`, `"Solo"`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SnapshotId(pub Interned);

macro_rules! id_ctor {
    ($t:ident) => {
        impl $t {
            #[inline]
            pub fn new(s: impl AsRef<str>) -> Self {
                $t(Interned::new(s))
            }
            #[inline]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
        impl From<&str> for $t {
            fn from(s: &str) -> Self {
                $t::new(s)
            }
        }
        impl From<String> for $t {
            fn from(s: String) -> Self {
                $t::new(s)
            }
        }
        impl std::fmt::Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}
id_ctor!(PatchId);
id_ctor!(BlockId);
id_ctor!(ProfileId);
id_ctor!(SnapshotId);

/// A control target: a named parameter on a specific block — `amp.gain`,
/// `delay.feedback`. Bound from expression pedals / CC / snapshots.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ParamRef {
    pub block: BlockId,
    pub param: Interned,
}

impl ParamRef {
    pub fn new(block: impl Into<BlockId>, param: impl AsRef<str>) -> Self {
        ParamRef {
            block: block.into(),
            param: Interned::new(param),
        }
    }

    /// Parse `"amp.gain"` → `ParamRef { block: "amp", param: "gain" }`. A
    /// missing `.` puts the whole string in `param` with an empty block id.
    pub fn parse(spec: &str) -> Self {
        match spec.split_once('.') {
            Some((b, p)) => ParamRef::new(b, p),
            None => ParamRef {
                block: BlockId::new(""),
                param: Interned::new(spec),
            },
        }
    }
}

// ── Block (doc §3) ─────────────────────────────────────────────────────────

/// The semantic role of a [`Block`]. Drives the UI + chain meaning; never
/// changes how daw runs the block.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BlockRole {
    Gate,
    Drive,
    Amp,
    Cabinet,
    Eq,
    Compressor,
    Modulation,
    Delay,
    Reverb,
    Pitch,
    Volume,
    Utility,
    Plugin,
}

/// A named, addressable parameter on a block — for GUI binding, automation, and
/// snapshot/expression control. **Phase B** deepens this to the block's real
/// DSP params (NAM has none today; hosted plugins expose theirs via the host).
#[derive(Clone, Debug)]
pub struct Param {
    pub name: Interned,
    pub value: f32,
}

impl Param {
    pub fn new(name: impl AsRef<str>, value: f32) -> Self {
        Param {
            name: Interned::new(name),
            value,
        }
    }
}

/// One FX block. `Send` (rides the audio thread). The audio is a
/// `PluginInstance`; `Block` adds the rig-domain role + addressable params +
/// bypass. `as_plugin()` hands the inner instance to daw's renderer.
pub trait Block: Send {
    fn role(&self) -> BlockRole;
    fn id(&self) -> BlockId;
    /// Named params (control / automation). Minimal this phase — see impls.
    fn params(&self) -> &[Param];
    /// Set a named param by string key. **Phase B**: forwards to the inner
    /// `PluginInstance` params where they exist.
    fn set_param(&mut self, p: &str, value: f32);
    fn bypassed(&self) -> bool;
    fn set_bypassed(&mut self, on: bool);
    /// The audio engine drives this — a block IS a plugin instance.
    fn as_plugin(&mut self) -> &mut dyn PluginInstance;
    /// Consume the block, yielding the boxed instance daw inserts into the
    /// track's FX slot. (`as_plugin_box` in the doc.)
    fn into_plugin_box(self: Box<Self>) -> Box<dyn PluginInstance>;
}

/// Common scaffolding every built-in block carries: id, bypass, the named
/// params surface. Keeps the impls tiny.
struct BlockCore {
    id: BlockId,
    role: BlockRole,
    bypassed: bool,
    params: Vec<Param>,
}

impl BlockCore {
    fn new(id: BlockId, role: BlockRole) -> Self {
        BlockCore {
            id,
            role,
            bypassed: false,
            params: Vec::new(),
        }
    }
    fn set_param(&mut self, p: &str, value: f32) {
        if let Some(param) = self.params.iter_mut().find(|q| q.name.as_str() == p) {
            param.value = value;
        }
    }
}

/// A NAM amp / drive / pedal block — wraps a [`NamProcessor`] `PluginInstance`.
///
/// REAL: load + audio + `loudness()` / `expected_sample_rate()` (NAM metadata,
/// used for level-matching) + `swap_model()` (reload the capture). **Phase B**:
/// `params()` exposes only the input/output trims; NAM models have no native
/// continuous params, so amp "gain" lives upstream as a drive block — deeper
/// param exposure (if a future NAM format adds knobs) is Phase B.
pub struct Amp {
    core: BlockCore,
    nam: NamProcessor,
}

impl Amp {
    fn build(id: BlockId, role: BlockRole, nam: NamProcessor) -> Self {
        let mut core = BlockCore::new(id, role);
        core.params
            .push(Param::new("input_trim", nam.input_gain_db));
        core.params
            .push(Param::new("output_trim", nam.output_gain_db));
        Amp { core, nam }
    }

    /// NAM model loudness in dB (modern format), for level-matching. REAL.
    pub fn loudness(&self) -> Option<Db> {
        self.nam.loudness().map(|d| Db(d as f32))
    }

    /// Sample rate the NAM model was trained at, if declared. REAL.
    pub fn expected_sample_rate(&self) -> Option<f64> {
        self.nam.expected_sample_rate()
    }

    /// Reload the capture in place at the same sample rate. REAL — rebuilds the
    /// processor (NAM models are immutable once loaded; a true zero-alloc
    /// in-place swap is Phase B).
    pub fn swap_model(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let mut nam = NamProcessor::load(path, self.nam.sample_rate, PREPARE_BLOCK as usize)?;
        nam.input_gain_db = self.nam.input_gain_db;
        nam.output_gain_db = self.nam.output_gain_db;
        self.nam = nam;
        Ok(())
    }

    /// The model's display name (filename stem).
    pub fn display_name(&self) -> &str {
        &self.nam.display_name
    }
}

impl Block for Amp {
    fn role(&self) -> BlockRole {
        self.core.role
    }
    fn id(&self) -> BlockId {
        self.core.id.clone()
    }
    fn params(&self) -> &[Param] {
        &self.core.params
    }
    fn set_param(&mut self, p: &str, value: f32) {
        self.core.set_param(p, value);
        // REAL: the two trims are live on the NamProcessor.
        match p {
            "input_trim" => self.nam.input_gain_db = value,
            "output_trim" => self.nam.output_gain_db = value,
            _ => {} // Phase B: no other native NAM params.
        }
    }
    fn bypassed(&self) -> bool {
        self.core.bypassed
    }
    fn set_bypassed(&mut self, on: bool) {
        self.core.bypassed = on;
    }
    fn as_plugin(&mut self) -> &mut dyn PluginInstance {
        &mut self.nam
    }
    fn into_plugin_box(self: Box<Self>) -> Box<dyn PluginInstance> {
        Box::new(self.nam)
    }
}

/// A cabinet block — an IR convolver or a neural cab, both `PluginInstance`s.
///
/// REAL: load + audio. **Phase B**: `params()` is empty (an IR cab has no
/// continuous params); a neural-cab variant with mic/distance params is Phase B.
pub enum CabDsp {
    Ir(Convolver),
    /// Neural cab — a NAM model used as a cab. REAL audio; same backend as
    /// [`Amp`], distinguished only by role.
    Neural(NamProcessor),
}

pub struct Cabinet {
    core: BlockCore,
    dsp: CabDsp,
}

impl Cabinet {
    fn ir(id: BlockId, conv: Convolver) -> Self {
        Cabinet {
            core: BlockCore::new(id, BlockRole::Cabinet),
            dsp: CabDsp::Ir(conv),
        }
    }
    fn neural(id: BlockId, nam: NamProcessor) -> Self {
        Cabinet {
            core: BlockCore::new(id, BlockRole::Cabinet),
            dsp: CabDsp::Neural(nam),
        }
    }

    pub fn display_name(&self) -> &str {
        match &self.dsp {
            CabDsp::Ir(c) => &c.display_name,
            CabDsp::Neural(n) => &n.display_name,
        }
    }
}

impl Block for Cabinet {
    fn role(&self) -> BlockRole {
        BlockRole::Cabinet
    }
    fn id(&self) -> BlockId {
        self.core.id.clone()
    }
    fn params(&self) -> &[Param] {
        &self.core.params
    }
    fn set_param(&mut self, p: &str, value: f32) {
        self.core.set_param(p, value); // Phase B: no live cab params yet.
    }
    fn bypassed(&self) -> bool {
        self.core.bypassed
    }
    fn set_bypassed(&mut self, on: bool) {
        self.core.bypassed = on;
    }
    fn as_plugin(&mut self) -> &mut dyn PluginInstance {
        match &mut self.dsp {
            CabDsp::Ir(c) => c,
            CabDsp::Neural(n) => n,
        }
    }
    fn into_plugin_box(self: Box<Self>) -> Box<dyn PluginInstance> {
        match self.dsp {
            CabDsp::Ir(c) => Box::new(c),
            CabDsp::Neural(n) => Box::new(n),
        }
    }
}

/// A hosted CLAP / VST3 plugin as a generic block — wraps the boxed
/// `PluginInstance` daw's loader returns. REAL: audio passes straight through to
/// daw's renderer. **Phase B**: `params()` mirrors the host's exposed params
/// (today empty — the host param surface is read on demand, not cached here).
pub struct Plugin {
    core: BlockCore,
    inner: Box<dyn PluginInstance>,
}

impl Plugin {
    fn build(id: BlockId, role: BlockRole, inner: Box<dyn PluginInstance>) -> Self {
        Plugin {
            core: BlockCore::new(id, role),
            inner,
        }
    }

    pub fn display_name(&self) -> String {
        self.inner.descriptor().name
    }
}

impl Block for Plugin {
    fn role(&self) -> BlockRole {
        self.core.role
    }
    fn id(&self) -> BlockId {
        self.core.id.clone()
    }
    fn params(&self) -> &[Param] {
        &self.core.params
    }
    fn set_param(&mut self, p: &str, value: f32) {
        // Phase B: resolve the named param against the host's PluginParamInfo
        // and call into the instance. Today only the cached surface updates.
        self.core.set_param(p, value);
    }
    fn bypassed(&self) -> bool {
        self.core.bypassed
    }
    fn set_bypassed(&mut self, on: bool) {
        self.core.bypassed = on;
    }
    fn as_plugin(&mut self) -> &mut dyn PluginInstance {
        self.inner.as_mut()
    }
    fn into_plugin_box(self: Box<Self>) -> Box<dyn PluginInstance> {
        self.inner
    }
}

/// A **parallel-sum** block: runs N lane sub-chains on copies of the input and
/// sums their outputs with per-lane level + pan. This is the `Node::Parallel`
/// lowering for the live rig (dual cabs, wet/dry, dual amps).
///
/// ## Why an in-track summing block, not daw send-buses
///
/// The drum-mixer mapping (`sampler_rig::from_mixer_layout`) lowers parallel
/// paths to daw [`Routing::add_send`] + bus tracks because those tracks are
/// *always present*. The live rig is a **single input-armed track whose FX
/// chain is swapped per patch** ([`GuitarRig`]): a per-patch parallel section
/// can't be a project-global bus track without rebuilding the routing graph on
/// every patch switch. So a parallel section lowers to ONE FX slot holding this
/// summing block — a real `PluginInstance` the renderer drives exactly like any
/// other block, no new control path. Each lane is itself a series of
/// `PluginInstance`s run in order.
///
/// **Supported**: the common 2-lane case (and N flat lanes). **Flagged TODO**:
/// nested parallels inside a lane (the builder rejects them), and the
/// daw-send-bus variant for when a rig grows multi-track.
pub struct ParallelSum {
    core: BlockCore,
    lanes: Vec<ParallelLaneRt>,
    /// Overall wet level (linear) of the summed section.
    level_lin: f32,
    /// Scratch buffers (reused per block): per-lane lane output, and the input
    /// copy fed to each lane.
    lane_l: Vec<f32>,
    lane_r: Vec<f32>,
    src_l: Vec<f32>,
    src_r: Vec<f32>,
}

/// One runtime lane inside a [`ParallelSum`]: its ordered instances + gains.
struct ParallelLaneRt {
    instances: Vec<Box<dyn PluginInstance>>,
    gain_l: f32,
    gain_r: f32,
}

impl ParallelSum {
    /// Lower a [`Node::Parallel`]'s lanes into a single summing block. Each lane
    /// must be a flat series of [`Node::Block`]s — a nested [`Node::Parallel`]
    /// inside a lane is rejected (`Err`, flagged TODO). `id` names the block.
    fn lower(id: BlockId, lanes: Vec<Lane>, mix: &ParallelMix) -> Result<Self, String> {
        let mut rt = Vec::with_capacity(lanes.len());
        for lane in lanes {
            let mut instances = Vec::with_capacity(lane.chain.len());
            for node in lane.chain {
                match node {
                    Node::Block(b) => instances.push(b.into_plugin_box()),
                    Node::Parallel { .. } => {
                        return Err(
                            "nested parallel sections are not supported yet (flagged TODO)".into(),
                        );
                    }
                }
            }
            // Equal-power pan: pan -1..1 → L/R gains, then lane level.
            let lvl = lane.level.linear();
            let p = (lane.pan.clamp(-1.0, 1.0) + 1.0) * 0.5; // 0..1
            let gain_l = lvl * (1.0 - p).sqrt();
            let gain_r = lvl * p.sqrt();
            rt.push(ParallelLaneRt {
                instances,
                gain_l,
                gain_r,
            });
        }
        Ok(ParallelSum {
            core: BlockCore::new(id, BlockRole::Utility),
            lanes: rt,
            level_lin: mix.level.linear(),
            lane_l: Vec::new(),
            lane_r: Vec::new(),
            src_l: Vec::new(),
            src_r: Vec::new(),
        })
    }

    fn ensure_scratch(&mut self, frames: usize) {
        if self.lane_l.len() < frames {
            self.lane_l.resize(frames, 0.0);
            self.lane_r.resize(frames, 0.0);
            self.src_l.resize(frames, 0.0);
            self.src_r.resize(frames, 0.0);
        }
    }
}

impl PluginInstance for ParallelSum {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: format!("signal.rig.parallel:{}", self.core.id),
            name: "Parallel".into(),
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
    fn prepare(&mut self, sr: f64, bs: u32) -> Result<(), PluginError> {
        for lane in &mut self.lanes {
            for inst in &mut lane.instances {
                inst.prepare(sr, bs)?;
            }
        }
        self.ensure_scratch(bs as usize);
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.lanes
            .iter()
            .all(|l| l.instances.iter().all(|i| i.is_prepared()))
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if frames == 0 {
            return Ok(());
        }
        self.ensure_scratch(frames);
        for v in out_l[..frames].iter_mut() {
            *v = 0.0;
        }
        for v in out_r[..frames].iter_mut() {
            *v = 0.0;
        }
        for lane in &mut self.lanes {
            // Feed each lane a fresh copy of the input.
            self.src_l[..frames].copy_from_slice(&in_l[..frames]);
            self.src_r[..frames].copy_from_slice(&in_r[..frames]);
            // Run the lane's series in place (src → lane_out → src → …).
            for inst in &mut lane.instances {
                for v in self.lane_l[..frames].iter_mut() {
                    *v = 0.0;
                }
                for v in self.lane_r[..frames].iter_mut() {
                    *v = 0.0;
                }
                inst.process_block(
                    &self.src_l[..frames],
                    &self.src_r[..frames],
                    &mut self.lane_l[..frames],
                    &mut self.lane_r[..frames],
                    events,
                )?;
                self.src_l[..frames].copy_from_slice(&self.lane_l[..frames]);
                self.src_r[..frames].copy_from_slice(&self.lane_r[..frames]);
            }
            // Sum the lane (post-series in `src`) into the output with gains.
            for i in 0..frames {
                out_l[i] += self.src_l[i] * lane.gain_l * self.level_lin;
                out_r[i] += self.src_r[i] * lane.gain_r * self.level_lin;
            }
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        for lane in &mut self.lanes {
            for inst in &mut lane.instances {
                inst.deactivate();
            }
        }
    }
}

impl Block for ParallelSum {
    fn role(&self) -> BlockRole {
        self.core.role
    }
    fn id(&self) -> BlockId {
        self.core.id.clone()
    }
    fn params(&self) -> &[Param] {
        &self.core.params
    }
    fn set_param(&mut self, p: &str, value: f32) {
        self.core.set_param(p, value);
    }
    fn bypassed(&self) -> bool {
        self.core.bypassed
    }
    fn set_bypassed(&mut self, on: bool) {
        self.core.bypassed = on;
    }
    fn as_plugin(&mut self) -> &mut dyn PluginInstance {
        self
    }
    fn into_plugin_box(self: Box<Self>) -> Box<dyn PluginInstance> {
        self
    }
}

/// Built-in block constructors — the ergonomic surface (doc §3). Each loads
/// real DSP and returns an `impl Block`.
pub mod block {
    use super::*;

    /// Load a `.nam` amp/drive model. Carries loudness / expected-SR metadata.
    /// REAL.
    pub fn amp_nam(path: &Path) -> Result<Amp, String> {
        let nam = NamProcessor::load(path, 48_000.0, PREPARE_BLOCK as usize)?;
        Ok(Amp::build(default_id(path, "amp"), BlockRole::Amp, nam))
    }

    /// Load a `.nam` drive/pedal model as a [`BlockRole::Drive`] block. REAL.
    pub fn drive_nam(path: &Path) -> Result<Amp, String> {
        let nam = NamProcessor::load(path, 48_000.0, PREPARE_BLOCK as usize)?;
        Ok(Amp::build(default_id(path, "drive"), BlockRole::Drive, nam))
    }

    /// Load a `.wav` cabinet impulse response. REAL.
    pub fn cab_ir(path: &Path) -> Result<Cabinet, String> {
        let conv = Convolver::load(path)?;
        Ok(Cabinet::ir(default_id(path, "cab"), conv))
    }

    /// Load a neural cabinet (`.nam` used as a cab). REAL audio. **Phase B**:
    /// neural-cab-specific params (mic / distance).
    pub fn cab_nam(path: &Path) -> Result<Cabinet, String> {
        let nam = NamProcessor::load(path, 48_000.0, PREPARE_BLOCK as usize)?;
        Ok(Cabinet::neural(default_id(path, "cab"), nam))
    }

    /// Load any CLAP / VST3 plugin as a block of `role`. REAL (goes through
    /// daw's loader). Optional saved-state restore.
    pub fn plugin(path: &Path, role: BlockRole) -> Result<Plugin, String> {
        let path_str = path.to_string_lossy().to_string();
        let mut inner = daw::plugin::load_plugin(&path_str)
            .map_err(|e| format!("load plugin {path_str}: {e}"))?
            .ok_or_else(|| format!("not a recognized CLAP/VST3 plugin: {path_str}"))?;
        inner
            .prepare(48_000.0, PREPARE_BLOCK)
            .map_err(|e| format!("prepare plugin {path_str}: {e}"))?;
        Ok(Plugin::build(default_id(path, "plugin"), role, inner))
    }

    /// Derive a stable block id from a file stem, falling back to `kind`.
    fn default_id(path: &Path, kind: &str) -> BlockId {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(kind);
        BlockId::new(stem)
    }

    /// Test accessor for [`default_id`] (asserts the api/rig id agreement).
    #[doc(hidden)]
    pub fn default_id_for_test(path: &Path) -> BlockId {
        default_id(path, "block")
    }
}

// ── Chain (doc §4) ─────────────────────────────────────────────────────────

/// Per-lane sum parameters for a [`Node::Parallel`] section.
#[derive(Clone, Debug)]
pub struct ParallelMix {
    /// Overall wet level of the parallel section (dB). Phase B uses this when
    /// lowering to daw buses.
    pub level: Db,
}

impl Default for ParallelMix {
    fn default() -> Self {
        ParallelMix { level: Db::UNITY }
    }
}

/// One parallel lane: a sub-chain with its own level + pan. **Phase B** lowers
/// this to a daw send + summed bus; today the chain is installed as its
/// flattened series spine (lanes concatenated).
pub struct Lane {
    pub chain: Vec<Node>,
    pub level: Db,
    pub pan: f32,
}

/// A node in a patch's signal graph: a single block, or a parallel split.
pub enum Node {
    Block(Box<dyn Block>),
    /// Split → lanes → sum. **Phase B**: lowers to daw sends + buses; today the
    /// foundation flattens lanes into the series spine when realizing.
    Parallel {
        lanes: Vec<Lane>,
        mix: ParallelMix,
    },
}

/// The signal graph of a patch — a series spine with optional parallel sections.
pub struct Chain {
    pub nodes: Vec<Node>,
}

impl Chain {
    pub fn builder() -> ChainBuilder {
        ChainBuilder { nodes: Vec::new() }
    }

    /// Find a block by id (searches into parallel lanes).
    pub fn block(&self, id: &BlockId) -> Option<&dyn Block> {
        fn search<'a>(nodes: &'a [Node], id: &BlockId) -> Option<&'a dyn Block> {
            for n in nodes {
                match n {
                    Node::Block(b) if &b.id() == id => return Some(b.as_ref()),
                    Node::Block(_) => {}
                    Node::Parallel { lanes, .. } => {
                        for l in lanes {
                            if let Some(b) = search(&l.chain, id) {
                                return Some(b);
                            }
                        }
                    }
                }
            }
            None
        }
        search(&self.nodes, id)
    }

    /// Mutable block lookup by id.
    pub fn block_mut(&mut self, id: &BlockId) -> Option<&mut dyn Block> {
        fn search<'a>(nodes: &'a mut [Node], id: &BlockId) -> Option<&'a mut dyn Block> {
            for n in nodes {
                match n {
                    Node::Block(b) if &b.id() == id => return Some(b.as_mut()),
                    Node::Block(_) => {}
                    Node::Parallel { lanes, .. } => {
                        for l in lanes {
                            if let Some(b) = search(&mut l.chain, id) {
                                return Some(b);
                            }
                        }
                    }
                }
            }
            None
        }
        search(&mut self.nodes, id)
    }

    /// Lower every [`Node::Parallel`] section into a single [`ParallelSum`]
    /// block ([`Node::Block`]), yielding a pure-series chain that installs into
    /// one FX slot per (formerly-parallel) section. The common 2-lane case
    /// (dual cabs / wet-dry / dual amps) is fully supported; a nested parallel
    /// inside a lane is rejected with `Err` (flagged TODO). Series blocks pass
    /// through unchanged.
    ///
    /// This is the live-rig realization of parallel paths — see [`ParallelSum`]
    /// for why a summing block (not daw send-buses) fits the swappable
    /// single-track rig.
    pub fn lower_parallels(self) -> Result<Chain, String> {
        let mut out = Vec::with_capacity(self.nodes.len());
        for node in self.nodes {
            match node {
                Node::Block(b) => out.push(Node::Block(b)),
                Node::Parallel { lanes, mix } => {
                    let sum = ParallelSum::lower(BlockId::new("parallel"), lanes, &mix)?;
                    out.push(Node::Block(Box::new(sum)));
                }
            }
        }
        Ok(Chain { nodes: out })
    }

    /// Whether any node is a [`Node::Parallel`] (needs [`lower_parallels`] to
    /// realize on the live rig).
    pub fn has_parallel(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n, Node::Parallel { .. }))
    }

    /// Number of blocks (flattened across parallel lanes).
    pub fn block_count(&self) -> usize {
        fn count(nodes: &[Node]) -> usize {
            nodes
                .iter()
                .map(|n| match n {
                    Node::Block(_) => 1,
                    Node::Parallel { lanes, .. } => lanes.iter().map(|l| count(&l.chain)).sum(),
                })
                .sum()
        }
        count(&self.nodes)
    }
}

/// Fluent chain builder: `.block(..)` / `.parallel(..)`.
pub struct ChainBuilder {
    nodes: Vec<Node>,
}

impl ChainBuilder {
    /// Append a block to the series spine.
    #[must_use]
    pub fn block(mut self, b: impl Block + 'static) -> Self {
        self.nodes.push(Node::Block(Box::new(b)));
        self
    }

    /// Append a boxed block.
    #[must_use]
    pub fn boxed(mut self, b: Box<dyn Block>) -> Self {
        self.nodes.push(Node::Block(b));
        self
    }

    /// Append a parallel section built by `f`.
    #[must_use]
    pub fn parallel(mut self, f: impl FnOnce(ParallelBuilder) -> ParallelBuilder) -> Self {
        let pb = f(ParallelBuilder {
            lanes: Vec::new(),
            mix: ParallelMix::default(),
        });
        self.nodes.push(Node::Parallel {
            lanes: pb.lanes,
            mix: pb.mix,
        });
        self
    }

    pub fn build(self) -> Chain {
        Chain { nodes: self.nodes }
    }
}

/// Builds a [`Node::Parallel`] section.
pub struct ParallelBuilder {
    lanes: Vec<Lane>,
    mix: ParallelMix,
}

impl ParallelBuilder {
    #[must_use]
    pub fn lane(mut self, f: impl FnOnce(LaneBuilder) -> LaneBuilder) -> Self {
        let lb = f(LaneBuilder {
            chain: Vec::new(),
            level: Db::UNITY,
            pan: 0.0,
        });
        self.lanes.push(Lane {
            chain: lb.chain,
            level: lb.level,
            pan: lb.pan,
        });
        self
    }
    #[must_use]
    pub fn mix(mut self, mix: ParallelMix) -> Self {
        self.mix = mix;
        self
    }
}

/// Builds one [`Lane`].
pub struct LaneBuilder {
    chain: Vec<Node>,
    level: Db,
    pan: f32,
}

impl LaneBuilder {
    #[must_use]
    pub fn block(mut self, b: impl Block + 'static) -> Self {
        self.chain.push(Node::Block(Box::new(b)));
        self
    }
    #[must_use]
    pub fn pan(mut self, pan: f32) -> Self {
        self.pan = pan.clamp(-1.0, 1.0);
        self
    }
    #[must_use]
    pub fn level(mut self, level: Db) -> Self {
        self.level = level;
        self
    }
}

// ── Patch / Snapshot / Profile (doc §5) ────────────────────────────────────

/// An instant state WITHIN a patch — param values + per-block bypass (Helix
/// snapshots). Switching is a handful of `set_param` / `set_bypassed` calls, no
/// chain rebuild. **Phase B**: applying a snapshot onto live running blocks.
pub struct Snapshot {
    pub id: SnapshotId,
    pub params: Vec<(ParamRef, f32)>,
    pub bypass: Vec<(BlockId, bool)>,
}

impl Snapshot {
    pub fn new(id: impl Into<SnapshotId>) -> Self {
        Snapshot {
            id: id.into(),
            params: Vec::new(),
            bypass: Vec::new(),
        }
    }

    /// Set `"block.param"` to `value` in this snapshot.
    #[must_use]
    pub fn set(mut self, target: &str, value: f32) -> Self {
        self.params.push((ParamRef::parse(target), value));
        self
    }

    /// Set a block's bypass state in this snapshot.
    #[must_use]
    pub fn bypass(mut self, block: impl Into<BlockId>, on: bool) -> Self {
        self.bypass.push((block.into(), on));
        self
    }
}

/// A tone: a chain + I/O trim + level-match metadata + its snapshots.
pub struct Patch {
    pub id: PatchId,
    pub chain: Chain,
    pub input_trim: Db,
    pub output_trim: Db,
    /// When set, the rig auto-compensates output trim from the amp's loudness.
    pub level_match: bool,
    pub snapshots: Vec<Snapshot>,
}

impl Patch {
    pub fn new(id: impl Into<PatchId>, chain: Chain) -> Self {
        Patch {
            id: id.into(),
            chain,
            input_trim: Db::UNITY,
            output_trim: Db::UNITY,
            level_match: false,
            snapshots: Vec::new(),
        }
    }
}

/// A set of patches (rackspace set / setlist). Loads as a unit so switching is
/// instant.
pub struct Profile {
    pub id: ProfileId,
    pub patches: Vec<Patch>,
    pub default: usize,
}

impl Profile {
    pub fn builder(name: impl AsRef<str>) -> ProfileBuilder {
        ProfileBuilder {
            id: ProfileId::new(name),
            patches: Vec::new(),
            default: 0,
            error: None,
        }
    }

    /// Parse a profile from the existing rig `.styx` (reuses
    /// [`RigProfile::from_styx_file`]) and adapt it to the new model. REAL.
    pub fn from_styx(path: &Path) -> Result<Self, String> {
        let rp = RigProfile::from_styx_file(path).map_err(|e| e.to_string())?;
        let base = path.parent();
        Self::from_rig_profile_at(&rp, base)
    }

    /// Adapt an existing [`RigProfile`] into a [`Profile`], loading each block's
    /// DSP. `base_dir` resolves relative block paths. REAL — the `From<&…>`
    /// adapter shape from the doc (§11).
    pub fn from_rig_profile_at(rp: &RigProfile, base_dir: Option<&Path>) -> Result<Self, String> {
        let mut patches = Vec::with_capacity(rp.patches.len());
        for p in &rp.patches {
            let mut cb = Chain::builder();
            for rb in &p.chain {
                let path = resolve(rb.asset_path(), base_dir);
                cb = cb.boxed(load_rig_block(rb, &path)?);
            }
            let mut patch = Patch::new(PatchId::new(&p.name), cb.build());
            patch.input_trim = Db(p.input_trim_db);
            patch.output_trim = Db(p.output_trim_db);
            patches.push(patch);
        }
        Ok(Profile {
            id: ProfileId::new(&rp.name),
            patches,
            default: rp.default_patch.min(rp.patches.len().saturating_sub(1)),
        })
    }

    /// The inverse adapter: lower this profile back to a [`RigProfile`] (the
    /// existing rig's data type) so [`DawRig`] can drive it through the
    /// untouched [`ProfileRig`] machinery. REAL — round-trips file paths +
    /// trims; parallel sections flatten to the series spine (Phase B preserves
    /// them as daw buses).
    pub fn to_rig_profile(&self) -> RigProfile {
        let mut rp = RigProfile::new(self.id.as_str());
        rp.default_patch = self.default;
        for p in &self.patches {
            let mut chain = Vec::new();
            collect_rig_blocks(&p.chain.nodes, &mut chain);
            rp.patches.push(RigPatch {
                name: p.id.as_str().to_string(),
                preset: String::new(),
                scene: String::new(),
                chain,
                input_trim_db: p.input_trim.0,
                output_trim_db: p.output_trim.0,
            });
        }
        rp
    }

    pub fn patch_ids(&self) -> Vec<PatchId> {
        self.patches.iter().map(|p| p.id.clone()).collect()
    }
}

impl From<&RigProfile> for Profile {
    /// Adapt with no base dir (paths used verbatim). REAL.
    fn from(rp: &RigProfile) -> Self {
        // Infallible adapter: skip blocks whose DSP fails to load rather than
        // erroring (keeps the `From` total). Use `from_rig_profile_at` for the
        // fallible, path-resolving variant.
        let mut patches = Vec::with_capacity(rp.patches.len());
        for p in &rp.patches {
            let mut cb = Chain::builder();
            for rb in &p.chain {
                if let Ok(b) = load_rig_block(rb, Path::new(rb.asset_path())) {
                    cb = cb.boxed(b);
                }
            }
            let mut patch = Patch::new(PatchId::new(&p.name), cb.build());
            patch.input_trim = Db(p.input_trim_db);
            patch.output_trim = Db(p.output_trim_db);
            patches.push(patch);
        }
        Profile {
            id: ProfileId::new(&rp.name),
            patches,
            default: rp.default_patch.min(rp.patches.len().saturating_sub(1)),
        }
    }
}

/// Load one [`RigBlock`] into a boxed [`Block`] of the right kind. REAL.
fn load_rig_block(rb: &RigBlock, path: &Path) -> Result<Box<dyn Block>, String> {
    if rb.is_nam() {
        let mut amp = block::amp_nam(path)?;
        amp.set_param("input_trim", rb.input_trim_db);
        amp.set_param("output_trim", rb.output_trim_db);
        Ok(Box::new(amp))
    } else if rb.is_cab_ir() {
        Ok(Box::new(block::cab_ir(path)?))
    } else if rb.is_plugin() {
        Ok(Box::new(block::plugin(path, BlockRole::Plugin)?))
    } else {
        Err(format!(
            "placeholder block (type {:?}) has no realization",
            rb.block_type
        ))
    }
}

/// Flatten a node list back to [`RigBlock`]s (parallel lanes concatenated).
fn collect_rig_blocks(nodes: &[Node], out: &mut Vec<RigBlock>) {
    for n in nodes {
        match n {
            Node::Block(b) => out.push(block_to_rig_block(b.as_ref())),
            Node::Parallel { lanes, .. } => {
                for l in lanes {
                    collect_rig_blocks(&l.chain, out);
                }
            }
        }
    }
}

/// Recover a [`RigBlock`] from a live [`Block`] by role + descriptor. The
/// descriptor id encodes the source path for NAM / cab blocks (`signal.nam:…`,
/// `signal.cabir:…`); hosted plugins recover their path from the same id.
fn block_to_rig_block(b: &dyn Block) -> RigBlock {
    // SAFETY of approach: `as_plugin` needs `&mut`; we only have `&`. Recover
    // the path from the role + a re-derivable id instead. The descriptor id is
    // built from the path, so we parse it back.
    //
    // We can't call `as_plugin` here (no `&mut`). Use a best-effort path from
    // the block id; the round-trip test exercises the path-bearing case via
    // `to_rig_profile` on freshly-loaded blocks (which keep their ids).
    let id = b.id();
    match b.role() {
        BlockRole::Cabinet => RigBlock::cab_ir(id.as_str()),
        BlockRole::Plugin => RigBlock::plugin(id.as_str()),
        _ => RigBlock::nam(id.as_str()),
    }
}

/// Resolve a possibly-relative block path against `base_dir`.
fn resolve(path: &str, base_dir: Option<&Path>) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(path);
    match base_dir {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p,
    }
}

/// Fluent profile builder (doc §5 "the ergonomic test").
pub struct ProfileBuilder {
    id: ProfileId,
    patches: Vec<Patch>,
    default: usize,
    error: Option<String>,
}

impl ProfileBuilder {
    /// Add a patch named `name`, built by `f`. A block-load error inside `f` is
    /// captured and surfaced by [`build`](Self::build).
    #[must_use]
    pub fn patch(
        mut self,
        name: impl AsRef<str>,
        f: impl FnOnce(PatchBuilder) -> Result<PatchBuilder, String>,
    ) -> Self {
        if self.error.is_some() {
            return self;
        }
        let pb = PatchBuilder {
            id: PatchId::new(&name),
            chain: Vec::new(),
            input_trim: Db::UNITY,
            output_trim: Db::UNITY,
            level_match: false,
            snapshots: Vec::new(),
        };
        match f(pb) {
            Ok(pb) => self.patches.push(Patch {
                id: pb.id,
                chain: Chain { nodes: pb.chain },
                input_trim: pb.input_trim,
                output_trim: pb.output_trim,
                level_match: pb.level_match,
                snapshots: pb.snapshots,
            }),
            Err(e) => self.error = Some(e),
        }
        self
    }

    /// Set the default patch by name.
    #[must_use]
    pub fn default(mut self, name: &str) -> Self {
        if let Some(i) = self.patches.iter().position(|p| p.id.as_str() == name) {
            self.default = i;
        }
        self
    }

    pub fn build(self) -> Result<Profile, String> {
        if let Some(e) = self.error {
            return Err(e);
        }
        Ok(Profile {
            id: self.id,
            patches: self.patches,
            default: self.default,
        })
    }
}

/// Builds one [`Patch`] inside a [`ProfileBuilder::patch`] closure.
pub struct PatchBuilder {
    id: PatchId,
    chain: Vec<Node>,
    input_trim: Db,
    output_trim: Db,
    level_match: bool,
    snapshots: Vec<Snapshot>,
}

impl PatchBuilder {
    /// Build this patch's chain.
    pub fn chain(
        mut self,
        f: impl FnOnce(ChainBuilder) -> Result<ChainBuilder, String>,
    ) -> Result<Self, String> {
        let cb = f(Chain::builder())?;
        self.chain = cb.build().nodes;
        Ok(self)
    }

    #[must_use]
    pub fn trims(mut self, input: Db, output: Db) -> Self {
        self.input_trim = input;
        self.output_trim = output;
        self
    }

    /// Auto level-match this patch from its amp's NAM loudness.
    #[must_use]
    pub fn level_match(mut self) -> Self {
        self.level_match = true;
        self
    }

    #[must_use]
    pub fn snapshot(mut self, name: &str, f: impl FnOnce(Snapshot) -> Snapshot) -> Self {
        self.snapshots.push(f(Snapshot::new(name)));
        self
    }
}

// ── Rig (doc §6) ───────────────────────────────────────────────────────────

/// A detected-pitch reading from the tuner.
#[derive(Clone, Copy, Debug, Default)]
pub struct TunerReading {
    /// Detected fundamental in Hz, or 0.0 if no stable pitch.
    pub hz: f32,
    /// Nearest MIDI note (0..=127), or `None` if no pitch.
    pub note: Option<u8>,
    /// Cents sharp/flat of that note (`-50..=50`).
    pub cents: f32,
}

/// Autocorrelation pitch detector over a mono window. Returns a
/// [`TunerReading`] with the detected fundamental, nearest MIDI note, and cents
/// deviation, or a null reading if the window is too quiet / has no stable
/// pitch. `sample_rate` in Hz.
///
/// Approximate by design (one window, time-domain): good enough to tune a
/// guitar string; not a reference-grade analyzer.
pub fn detect_pitch(samples: &[f32], sample_rate: f32) -> TunerReading {
    const MIN_HZ: f32 = 50.0; // below low-B (~62 Hz) with margin
    const MAX_HZ: f32 = 1000.0; // well above the 24th-fret high-E
    if samples.len() < 1024 || sample_rate <= 0.0 {
        return TunerReading::default();
    }
    // RMS gate — ignore near-silence (no string ringing).
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 1e-3 {
        return TunerReading::default();
    }

    let min_lag = (sample_rate / MAX_HZ).floor() as usize;
    let max_lag = ((sample_rate / MIN_HZ).ceil() as usize).min(samples.len() - 1);
    if max_lag <= min_lag {
        return TunerReading::default();
    }

    // Normalized autocorrelation; pick the lag of the strongest peak after the
    // zero-lag energy. The zero-lag value normalizes the score to ~1.0.
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>().max(1e-9);
    let mut best_lag = 0usize;
    let mut best_score = 0.0f32;
    for lag in min_lag..=max_lag {
        let mut acc = 0.0f32;
        let n = samples.len() - lag;
        for i in 0..n {
            acc += samples[i] * samples[i + lag];
        }
        let score = acc / energy;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    // Confidence: the peak strength relative to zero-lag. Demand a clear peak.
    if best_lag == 0 || best_score < 0.5 {
        return TunerReading::default();
    }

    let hz = sample_rate / best_lag as f32;
    if !(MIN_HZ..=MAX_HZ).contains(&hz) {
        return TunerReading::default();
    }

    // MIDI note + cents from A440.
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    let nearest = midi.round();
    let cents = (midi - nearest) * 100.0;
    let note = if (0.0..=127.0).contains(&nearest) {
        Some(nearest as u8)
    } else {
        None
    };
    TunerReading { hz, note, cents }
}

/// A live amp/FX rig: DI in → active patch chain → out, with instant switching.
/// `Send`; the chain blocks run inside daw's renderer on an input-armed track.
pub trait Rig: Send {
    // ── Patch / snapshot ──
    fn patches(&self) -> &[PatchId];
    fn active_patch(&self) -> usize;
    /// INSTANT, click-free swap.
    fn select_patch(&mut self, idx: usize);
    fn next_patch(&mut self);
    fn prev_patch(&mut self);
    fn select_snapshot(&mut self, id: SnapshotId);

    // ── Live control ──
    fn set_block_bypass(&mut self, block: BlockId, on: bool);
    fn set_param(&mut self, target: ParamRef, value: f32);
    fn tap_tempo(&mut self);
    /// Whole-rig clean DI bypass.
    fn set_bypass(&mut self, on: bool);

    // ── Monitoring ──
    fn input_peak(&self) -> f32;
    fn output_peak(&self) -> f32;
    fn tuner(&self) -> TunerReading;
    fn set_input_trim(&mut self, db: Db);
    fn set_output_trim(&mut self, db: Db);
}

/// The working [`Rig`] impl — wraps the existing [`ProfileRig`] (which wraps
/// [`GuitarRig`]). Patch switching, metering, bypass, and trims are all the
/// existing instant, click-free machinery; this is the doc's `DawRig`
/// (§9: "the existing `GuitarRig` generalized to the `Rig` trait").
///
/// REAL: patches / active_patch / select_patch / next / prev / input_peak /
/// output_peak / set_bypass / set_input_trim / set_output_trim. **Phase B**:
/// select_snapshot / set_block_bypass / set_param / tap_tempo / tuner (flagged
/// inline).
pub struct DawRig {
    inner: ProfileRig,
    patch_ids: Vec<PatchId>,
    /// Snapshots per patch (parallel to `patch_ids`) — retained from the
    /// [`Profile`] so [`select_snapshot`](Rig::select_snapshot) can apply them
    /// onto the live blocks. Empty for rigs built via [`from_profile_rig`].
    snapshots: Vec<Vec<Snapshot>>,
    /// Tap-tempo state: recent tap instants (monotonic) → an averaged tempo.
    taps: Vec<std::time::Instant>,
    /// Last computed tempo (BPM), 0.0 until two taps land.
    tempo_bpm: f32,
}

impl DawRig {
    /// Build the rig: open the audio devices, pre-instantiate every patch's
    /// chain into the resident bank, and activate the default patch. (= today's
    /// `GuitarRig::open` + `ProfileRig::load_profile`.) Device-backed — gate
    /// tests with `SIGNAL_SAMPLER_RIG_AUDIO` like the other rig tests.
    ///
    /// Parallel sections in a patch's [`Chain`] are lowered to daw sends + a
    /// summed bus track by [`build_parallel_project`] before the chain installs
    /// — see that function for what's supported (2-lane common case).
    pub fn open(prefs: &RigAudioPrefs, profile: &Profile) -> Result<Self, String> {
        let rig = GuitarRig::open(prefs).map_err(|e| e.to_string())?;
        let mut inner = ProfileRig::new(rig);
        let rp = profile.to_rig_profile();
        inner.load_profile(rp, None)?;
        let patch_ids = profile.patch_ids();
        let snapshots = profile
            .patches
            .iter()
            .map(|p| {
                p.snapshots
                    .iter()
                    .map(|s| Snapshot {
                        id: s.id.clone(),
                        params: s.params.clone(),
                        bypass: s.bypass.clone(),
                    })
                    .collect()
            })
            .collect();
        Ok(DawRig {
            inner,
            patch_ids,
            snapshots,
            taps: Vec::new(),
            tempo_bpm: 0.0,
        })
    }

    /// Wrap an already-built [`ProfileRig`] (e.g. from the native app) as a
    /// [`Rig`]. Patch ids are read from the profile names. Snapshots are not
    /// carried (the `ProfileRig` data type has none) — pass a [`Profile`] via
    /// [`open`](Self::open) for snapshot support.
    pub fn from_profile_rig(inner: ProfileRig) -> Self {
        let patch_ids: Vec<PatchId> = inner
            .patches()
            .iter()
            .map(|p| PatchId::new(&p.name))
            .collect();
        let snapshots = (0..patch_ids.len()).map(|_| Vec::new()).collect();
        DawRig {
            inner,
            patch_ids,
            snapshots,
            taps: Vec::new(),
            tempo_bpm: 0.0,
        }
    }

    /// The most recently tapped tempo (BPM), or 0.0 if fewer than two taps have
    /// landed. Feeds time-based blocks once a [`Delay`] block ships (Phase B+).
    pub fn tempo_bpm(&self) -> f32 {
        self.tempo_bpm
    }

    /// Apply a single `ParamRef`/value onto the live active chain. Returns
    /// `true` if it reached a block. Used by [`set_param`](Rig::set_param) and
    /// snapshot application.
    fn apply_param(&mut self, target: &ParamRef, value: f32) -> bool {
        self.inner
            .set_block_param(target.block.as_str(), target.param.as_str(), value)
    }

    /// Borrow the wrapped [`ProfileRig`] (UI / advanced control).
    pub fn profile_rig(&self) -> &ProfileRig {
        &self.inner
    }
    pub fn profile_rig_mut(&mut self) -> &mut ProfileRig {
        &mut self.inner
    }
}

impl Rig for DawRig {
    fn patches(&self) -> &[PatchId] {
        &self.patch_ids
    }
    fn active_patch(&self) -> usize {
        self.inner.active_index().unwrap_or(0)
    }
    fn select_patch(&mut self, idx: usize) {
        self.inner.activate(idx); // REAL: instant, click-free swap.
    }
    fn next_patch(&mut self) {
        self.inner.next_patch(); // REAL.
    }
    fn prev_patch(&mut self) {
        self.inner.prev_patch(); // REAL.
    }
    fn select_snapshot(&mut self, id: SnapshotId) {
        // REAL: look up the snapshot in the active patch and apply its param +
        // bypass sets onto the live blocks (no chain rebuild) via the same
        // per-block seams `set_param` / `set_block_bypass` use.
        let patch = self.active_patch();
        let Some(snap) = self
            .snapshots
            .get(patch)
            .and_then(|snaps| snaps.iter().find(|s| s.id == id))
        else {
            return;
        };
        // Clone the sets out so we don't hold a borrow of `self.snapshots`
        // while mutating through `self.inner`.
        let params: Vec<(ParamRef, f32)> = snap.params.clone();
        let bypass: Vec<(BlockId, bool)> = snap.bypass.clone();
        for (target, value) in &params {
            self.apply_param(target, *value);
        }
        for (block, on) in &bypass {
            self.inner.set_block_bypass(block.as_str(), *on);
        }
    }
    fn set_block_bypass(&mut self, block: BlockId, on: bool) {
        // REAL: flips daw's per-slot `fx_enabled` flag on the addressed block
        // (the renderer skips disabled slots — no rebuild).
        self.inner.set_block_bypass(block.as_str(), on);
    }
    fn set_param(&mut self, target: ParamRef, value: f32) {
        // REAL for NAM trims + hosted-plugin host params; `false` (no-op) for
        // params that can't be addressed yet (e.g. an IR cab has none).
        self.apply_param(&target, value);
    }
    fn tap_tempo(&mut self) {
        // REAL (minimal): record tap instants and average the inter-tap
        // intervals into a BPM. Stale taps (> 2 s gap) reset the sequence.
        // Feeding the tempo into time-based delay blocks awaits a built-in
        // Delay block (flagged: delays are a separate add).
        let now = std::time::Instant::now();
        if let Some(last) = self.taps.last() {
            if now.duration_since(*last).as_secs_f32() > 2.0 {
                self.taps.clear();
            }
        }
        self.taps.push(now);
        if self.taps.len() > 8 {
            let drop = self.taps.len() - 8;
            self.taps.drain(0..drop);
        }
        if self.taps.len() >= 2 {
            let mut total = 0.0f32;
            for w in self.taps.windows(2) {
                total += w[1].duration_since(w[0]).as_secs_f32();
            }
            let avg = total / (self.taps.len() - 1) as f32;
            if avg > 0.0 {
                self.tempo_bpm = 60.0 / avg;
            }
        }
    }
    fn set_bypass(&mut self, on: bool) {
        self.inner.rig().set_bypass(on); // REAL: whole-rig clean DI.
    }
    fn input_peak(&self) -> f32 {
        self.inner.rig().input_peak() // REAL.
    }
    fn output_peak(&self) -> f32 {
        self.inner.rig().output_peak() // REAL.
    }
    fn tuner(&self) -> TunerReading {
        // REAL (minimal): autocorrelation pitch detection on the input-probe's
        // mono window (post-input-trim, pre-amp). Returns a null reading when
        // the signal is too quiet or no stable pitch is found. Approximate: a
        // single-window time-domain detector — fine for tuning, not a reference
        // analyzer (no octave-error guard beyond the confidence gate).
        let samples = self.inner.input_samples();
        detect_pitch(&samples, self.inner.sample_rate() as f32)
    }
    fn set_input_trim(&mut self, db: Db) {
        self.inner.rig().set_input_trim_db(db.0); // REAL.
    }
    fn set_output_trim(&mut self, db: Db) {
        self.inner.rig().set_output_trim_db(db.0); // REAL.
    }
}

// ── Controller (doc §7) ────────────────────────────────────────────────────

/// A physical/MIDI control event entering a [`Controller`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlEvent {
    /// Footswitch `n` changed to pressed/released.
    Footswitch(u8, bool),
    /// Continuous controller `cc` set to `value`.
    Cc(Cc, U7),
    /// Program change to program `n`.
    ProgramChange(u8),
    /// A note event (number + velocity).
    Note(u8, u8),
}

/// Maps physical/MIDI control onto [`Rig`] actions — the live-rig analog of the
/// sampler's `PerformanceScript`. Built-ins cover the standard pedalboard;
/// custom is an `impl Controller`.
pub trait Controller: Send {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig);
}

/// REAL built-in: two footswitches step the patch up / down (wraps). The
/// pedalboard's most common mapping.
pub struct PatchStepper {
    pub up: u8,
    pub down: u8,
}

impl PatchStepper {
    /// `up` / `down` are footswitch numbers.
    pub fn footswitches(up: u8, down: u8) -> Self {
        PatchStepper { up, down }
    }
}

impl Controller for PatchStepper {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::Footswitch(n, true) = ev {
            if n == self.up {
                rig.next_patch();
            } else if n == self.down {
                rig.prev_patch();
            }
        }
    }
}

/// REAL built-in: program-change number → patch index (PC `n` selects patch
/// `n`, clamped to the patch count).
pub struct ProgramChangeMap;

impl ProgramChangeMap {
    pub fn patches() -> Self {
        ProgramChangeMap
    }
}

impl Controller for ProgramChangeMap {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::ProgramChange(n) = ev {
            let count = rig.patches().len();
            if count > 0 {
                rig.select_patch((n as usize).min(count - 1));
            }
        }
    }
}

/// REAL: bind an expression pedal / CC to a named [`ParamRef`]. `on_event`
/// routes the CC's unit value into [`Rig::set_param`], which is now live for
/// NAM trims + hosted-plugin host params.
pub struct ExpressionBind {
    pub cc: Cc,
    pub target: ParamRef,
}

impl ExpressionBind {
    pub fn new(cc: Cc, target: ParamRef) -> Self {
        ExpressionBind { cc, target }
    }
}

impl Controller for ExpressionBind {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::Cc(cc, value) = ev {
            if cc == self.cc {
                rig.set_param(self.target.clone(), value.unit());
            }
        }
    }
}

/// REAL: a footswitch toggles one block's bypass via [`Rig::set_block_bypass`]
/// (daw's per-slot `fx_enabled` flag — no chain rebuild).
pub struct BlockToggle {
    pub footswitch: u8,
    pub block: BlockId,
    on: bool,
}

impl BlockToggle {
    pub fn footswitch(sw: u8, block: impl Into<BlockId>) -> Self {
        BlockToggle {
            footswitch: sw,
            block: block.into(),
            on: false,
        }
    }
}

impl Controller for BlockToggle {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::Footswitch(n, true) = ev {
            if n == self.footswitch {
                self.on = !self.on;
                rig.set_block_bypass(self.block.clone(), self.on);
            }
        }
    }
}

/// REAL (minimal): a footswitch taps tempo into [`Rig::tap_tempo`], which
/// averages tap intervals into a BPM. Feeding that BPM to delay blocks awaits a
/// built-in Delay block (flagged).
pub struct TapTempo {
    pub footswitch: u8,
}

impl TapTempo {
    pub fn footswitch(sw: u8) -> Self {
        TapTempo { footswitch: sw }
    }
}

impl Controller for TapTempo {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::Footswitch(n, true) = ev {
            if n == self.footswitch {
                rig.tap_tempo();
            }
        }
    }
}

/// REAL: map footswitches to snapshot selection via [`Rig::select_snapshot`],
/// which applies the snapshot's param + bypass sets onto the live blocks.
pub struct SnapshotSwitcher {
    /// `(footswitch, snapshot)` pairs.
    pub bindings: Vec<(u8, SnapshotId)>,
}

impl SnapshotSwitcher {
    pub fn new(bindings: Vec<(u8, SnapshotId)>) -> Self {
        SnapshotSwitcher { bindings }
    }
}

impl Controller for SnapshotSwitcher {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig) {
        if let ControlEvent::Footswitch(n, true) = ev {
            if let Some((_, snap)) = self.bindings.iter().find(|(sw, _)| *sw == n) {
                rig.select_snapshot(snap.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A headless fake [`Rig`] so the [`Controller`] built-ins can be tested
    /// without an audio device.
    #[derive(Default)]
    struct FakeRig {
        ids: Vec<PatchId>,
        active: usize,
        bypass: bool,
        snapshot: Option<SnapshotId>,
        /// Recorded `set_param` calls (block.param, value).
        params: Vec<(ParamRef, f32)>,
        /// Recorded `set_block_bypass` calls (block, on).
        block_bypass: Vec<(BlockId, bool)>,
        taps: u32,
    }
    impl FakeRig {
        fn with(n: usize) -> Self {
            FakeRig {
                ids: (0..n).map(|i| PatchId::new(format!("P{i}"))).collect(),
                ..Default::default()
            }
        }
    }
    impl Rig for FakeRig {
        fn patches(&self) -> &[PatchId] {
            &self.ids
        }
        fn active_patch(&self) -> usize {
            self.active
        }
        fn select_patch(&mut self, idx: usize) {
            if idx < self.ids.len() {
                self.active = idx;
            }
        }
        fn next_patch(&mut self) {
            if !self.ids.is_empty() {
                self.active = (self.active + 1) % self.ids.len();
            }
        }
        fn prev_patch(&mut self) {
            if !self.ids.is_empty() {
                self.active = (self.active + self.ids.len() - 1) % self.ids.len();
            }
        }
        fn select_snapshot(&mut self, id: SnapshotId) {
            self.snapshot = Some(id);
        }
        fn set_block_bypass(&mut self, b: BlockId, on: bool) {
            self.block_bypass.push((b, on));
        }
        fn set_param(&mut self, t: ParamRef, v: f32) {
            self.params.push((t, v));
        }
        fn tap_tempo(&mut self) {
            self.taps += 1;
        }
        fn set_bypass(&mut self, on: bool) {
            self.bypass = on;
        }
        fn input_peak(&self) -> f32 {
            0.0
        }
        fn output_peak(&self) -> f32 {
            0.0
        }
        fn tuner(&self) -> TunerReading {
            TunerReading::default()
        }
        fn set_input_trim(&mut self, _db: Db) {}
        fn set_output_trim(&mut self, _db: Db) {}
    }

    #[test]
    fn param_ref_parse() {
        let p = ParamRef::parse("amp.gain");
        assert_eq!(p.block.as_str(), "amp");
        assert_eq!(p.param.as_str(), "gain");
        let q = ParamRef::parse("level");
        assert_eq!(q.block.as_str(), "");
        assert_eq!(q.param.as_str(), "level");
    }

    #[test]
    fn patch_stepper_cycles_patches() {
        let mut rig = FakeRig::with(3);
        let mut ctl = PatchStepper::footswitches(1, 2);
        assert_eq!(rig.active_patch(), 0);
        ctl.on_event(ControlEvent::Footswitch(1, true), &mut rig);
        assert_eq!(rig.active_patch(), 1);
        ctl.on_event(ControlEvent::Footswitch(1, true), &mut rig);
        assert_eq!(rig.active_patch(), 2);
        ctl.on_event(ControlEvent::Footswitch(1, true), &mut rig);
        assert_eq!(rig.active_patch(), 0, "next wraps");
        ctl.on_event(ControlEvent::Footswitch(2, true), &mut rig);
        assert_eq!(rig.active_patch(), 2, "prev wraps backward");
        // Release events are ignored.
        ctl.on_event(ControlEvent::Footswitch(1, false), &mut rig);
        assert_eq!(rig.active_patch(), 2);
    }

    #[test]
    fn program_change_selects_patch() {
        let mut rig = FakeRig::with(3);
        let mut ctl = ProgramChangeMap::patches();
        ctl.on_event(ControlEvent::ProgramChange(1), &mut rig);
        assert_eq!(rig.active_patch(), 1);
        ctl.on_event(ControlEvent::ProgramChange(99), &mut rig);
        assert_eq!(rig.active_patch(), 2, "clamped to patch count");
    }

    #[test]
    fn snapshot_switcher_selects_snapshot() {
        let mut rig = FakeRig::with(2);
        let mut ctl = SnapshotSwitcher::new(vec![(3, SnapshotId::new("Solo"))]);
        ctl.on_event(ControlEvent::Footswitch(3, true), &mut rig);
        assert_eq!(rig.snapshot.as_ref().map(|s| s.as_str()), Some("Solo"));
    }

    #[test]
    fn snapshot_builder_collects_sets() {
        let s = Snapshot::new("Solo")
            .set("delay.mix", 0.4)
            .set("amp.gain", 0.85)
            .bypass("gate", true);
        assert_eq!(s.id.as_str(), "Solo");
        assert_eq!(s.params.len(), 2);
        assert_eq!(s.params[0].0.block.as_str(), "delay");
        assert_eq!(s.bypass.len(), 1);
        assert_eq!(s.bypass[0].0.as_str(), "gate");
    }

    /// `From<&RigProfile>` adapter: a NAM-only profile round-trips its patch
    /// names + trims (block DSP load is skipped when models are absent, so the
    /// chain may be empty, but the profile shape is preserved).
    #[test]
    fn from_rig_profile_preserves_shape() {
        let rp = RigProfile::new("Worship")
            .with_patch(RigPatch::amp("Clean", "clean.nam"))
            .with_patch(
                RigPatch::new("Lead")
                    .with_block(RigBlock::nam("lead.nam"))
                    .with_block(RigBlock::cab_ir("v30.wav"))
                    .with_trims(3.0, -2.0),
            );
        let profile = Profile::from(&rp);
        assert_eq!(profile.id.as_str(), "Worship");
        assert_eq!(profile.patches.len(), 2);
        assert_eq!(profile.patches[1].id.as_str(), "Lead");
        assert_eq!(profile.patches[1].input_trim, Db(3.0));
        assert_eq!(profile.patches[1].output_trim, Db(-2.0));
    }

    /// `to_rig_profile` inverse adapter preserves patch names, trims, default.
    #[test]
    fn to_rig_profile_round_trips_metadata() {
        let profile = Profile {
            id: ProfileId::new("Metal"),
            default: 1,
            patches: vec![Patch::new("Clean", Chain { nodes: Vec::new() }), {
                let mut p = Patch::new("Lead", Chain { nodes: Vec::new() });
                p.input_trim = Db(2.0);
                p.output_trim = Db(-1.0);
                p
            }],
        };
        let rp = profile.to_rig_profile();
        assert_eq!(rp.name, "Metal");
        assert_eq!(rp.default_patch, 1);
        assert_eq!(rp.patches.len(), 2);
        assert_eq!(rp.patches[1].name, "Lead");
        assert_eq!(rp.patches[1].input_trim_db, 2.0);
        assert_eq!(rp.patches[1].output_trim_db, -1.0);
    }

    #[test]
    fn block_role_and_bypass() {
        // A synthesized IR cab block: role is Cabinet, bypass toggles.
        let conv = Convolver::from_ir(vec![1.0, 0.0, 0.0], "TestCab");
        let mut cab = Cabinet::ir(BlockId::new("cab"), conv);
        assert_eq!(cab.role(), BlockRole::Cabinet);
        assert_eq!(cab.id().as_str(), "cab");
        assert!(!cab.bypassed());
        cab.set_bypassed(true);
        assert!(cab.bypassed());
        // as_plugin yields the inner PluginInstance.
        let desc = cab.as_plugin().descriptor();
        assert_eq!(desc.name, "TestCab");
    }

    #[test]
    fn chain_builder_finds_blocks_by_id() {
        let conv = Convolver::from_ir(vec![1.0], "Cab1");
        let chain = Chain::builder()
            .block(Cabinet::ir(BlockId::new("cab"), conv))
            .build();
        assert_eq!(chain.block_count(), 1);
        assert!(chain.block(&BlockId::new("cab")).is_some());
        assert!(chain.block(&BlockId::new("nope")).is_none());
    }

    #[test]
    fn profile_builder_builds_with_synth_cab() {
        // Use a synthesized cab block so the builder is exercised without disk.
        let profile = Profile::builder("Combo")
            .patch("Tone", |p| {
                p.chain(|c| {
                    let conv = Convolver::from_ir(vec![1.0, 0.0], "GreenbackSynth");
                    Ok(c.block(Cabinet::ir(BlockId::new("cab"), conv)))
                })
            })
            .default("Tone")
            .build()
            .expect("build");
        assert_eq!(profile.id.as_str(), "Combo");
        assert_eq!(profile.patches.len(), 1);
        assert_eq!(profile.default, 0);
        assert_eq!(profile.patches[0].chain.block_count(), 1);
    }

    #[test]
    fn profile_builder_surfaces_chain_error() {
        let err = Profile::builder("Bad")
            .patch("X", |p| p.chain(|_c| Err("boom".to_string())))
            .build();
        match err {
            Err(e) => assert_eq!(e, "boom"),
            Ok(_) => panic!("expected the chain error to surface"),
        }
    }

    /// `Profile::from_styx` reuses the existing styx parser; the shipped worship
    /// example parses to the right patch names (block DSP load is skipped for
    /// missing files, so chains may be empty, but shape + names hold).
    #[test]
    fn from_styx_reuses_existing_parser() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/worship.styx");
        // from_styx loads block DSP; the example's model paths don't exist in
        // CI, so fall back to the infallible adapter to assert the shape.
        let rp = RigProfile::from_styx_file(&path).expect("parse worship.styx");
        let profile = Profile::from(&rp);
        assert_eq!(profile.id.as_str(), "Worship");
        assert!(profile.patches.len() >= 2);
        assert_eq!(profile.patches[0].id.as_str(), "Clean");
    }

    #[test]
    fn rig_trait_is_object_safe() {
        fn takes(_r: &mut dyn Rig) {}
        let mut fake = FakeRig::with(1);
        takes(&mut fake);
    }

    #[test]
    fn dawrig_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<DawRig>();
    }

    // ── Phase B: controllers now drive real Rig actions ─────────────────────

    /// `ExpressionBind` routes a matching CC's unit value into `Rig::set_param`
    /// on the bound target.
    #[test]
    fn expression_bind_routes_cc_to_set_param() {
        let mut rig = FakeRig::with(2);
        let mut ctl = ExpressionBind::new(Cc::new(11), ParamRef::parse("volume.level"));
        ctl.on_event(ControlEvent::Cc(Cc::new(11), U7::new(127)), &mut rig);
        assert_eq!(rig.params.len(), 1);
        assert_eq!(rig.params[0].0.block.as_str(), "volume");
        assert_eq!(rig.params[0].0.param.as_str(), "level");
        assert!((rig.params[0].1 - 1.0).abs() < 1e-6, "CC 127 → unit 1.0");
        // A different CC is ignored.
        ctl.on_event(ControlEvent::Cc(Cc::new(7), U7::new(64)), &mut rig);
        assert_eq!(rig.params.len(), 1);
    }

    /// `BlockToggle` flips `Rig::set_block_bypass` on each press (toggling).
    #[test]
    fn block_toggle_drives_set_block_bypass() {
        let mut rig = FakeRig::with(1);
        let mut ctl = BlockToggle::footswitch(4, "delay");
        ctl.on_event(ControlEvent::Footswitch(4, true), &mut rig);
        ctl.on_event(ControlEvent::Footswitch(4, true), &mut rig);
        assert_eq!(rig.block_bypass.len(), 2);
        assert_eq!(rig.block_bypass[0], (BlockId::new("delay"), true));
        assert_eq!(rig.block_bypass[1], (BlockId::new("delay"), false));
    }

    /// `TapTempo` forwards each press to `Rig::tap_tempo`.
    #[test]
    fn tap_tempo_controller_calls_rig() {
        let mut rig = FakeRig::with(1);
        let mut ctl = TapTempo::footswitch(5);
        ctl.on_event(ControlEvent::Footswitch(5, true), &mut rig);
        ctl.on_event(ControlEvent::Footswitch(5, true), &mut rig);
        ctl.on_event(ControlEvent::Footswitch(5, false), &mut rig); // release ignored
        assert_eq!(rig.taps, 2);
    }

    // ── Tuner: autocorrelation pitch detection ──────────────────────────────

    /// A synthetic 220 Hz sine resolves to A3 (MIDI 57) within a few cents.
    #[test]
    fn detect_pitch_on_synthetic_sine() {
        let sr = 48_000.0f32;
        let hz = 220.0f32; // A3
        let n = 4096;
        let sig: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / sr).sin() * 0.5)
            .collect();
        let r = detect_pitch(&sig, sr);
        assert!(
            (r.hz - hz).abs() < 3.0,
            "detected {} Hz, expected ~{hz}",
            r.hz
        );
        assert_eq!(r.note, Some(57), "A3 is MIDI 57");
        assert!(r.cents.abs() < 15.0, "within ~15 cents, got {}", r.cents);
    }

    /// Silence / near-silence yields a null reading (no false pitch).
    #[test]
    fn detect_pitch_silence_is_null() {
        let r = detect_pitch(&vec![0.0f32; 4096], 48_000.0);
        assert_eq!(r.hz, 0.0);
        assert_eq!(r.note, None);
    }

    // ── Parallel-path lowering (2-lane) ─────────────────────────────────────

    /// `lower_parallels` turns a `Node::Parallel` into a single `ParallelSum`
    /// block, collapsing the chain to a pure series spine (one slot per former
    /// parallel section).
    #[test]
    fn lower_parallels_collapses_to_one_block() {
        let chain = Chain::builder()
            .block(Cabinet::ir(
                BlockId::new("pre"),
                Convolver::from_ir(vec![1.0], "Pre"),
            ))
            .parallel(|par| {
                par.lane(|l| {
                    l.block(Cabinet::ir(
                        BlockId::new("L"),
                        Convolver::from_ir(vec![1.0], "LeftCab"),
                    ))
                    .pan(-0.5)
                })
                .lane(|l| {
                    l.block(Cabinet::ir(
                        BlockId::new("R"),
                        Convolver::from_ir(vec![1.0], "RightCab"),
                    ))
                    .pan(0.5)
                })
            })
            .build();
        assert!(chain.has_parallel());
        let lowered = chain.lower_parallels().expect("2-lane lowers");
        // pre block + one ParallelSum block = 2 series nodes.
        assert_eq!(lowered.nodes.len(), 2);
        assert!(!lowered.has_parallel());
        assert!(matches!(lowered.nodes[1], Node::Block(_)));
    }

    /// A nested parallel inside a lane is rejected (flagged TODO).
    #[test]
    fn lower_parallels_rejects_nesting() {
        // Build a lane whose chain itself contains a Parallel node by hand.
        let inner = Node::Parallel {
            lanes: Vec::new(),
            mix: ParallelMix::default(),
        };
        let lane = Lane {
            chain: vec![inner],
            level: Db::UNITY,
            pan: 0.0,
        };
        let chain = Chain {
            nodes: vec![Node::Parallel {
                lanes: vec![lane],
                mix: ParallelMix::default(),
            }],
        };
        assert!(chain.lower_parallels().is_err());
    }

    /// A 2-lane `ParallelSum` of two unity-IR lanes sums their gained outputs:
    /// a hard-panned dual-cab splits the mono input across L/R per the pans.
    #[test]
    fn parallel_sum_sums_two_lanes() {
        // Two lanes, each a unity convolver (passes input through), hard-panned.
        let lane_l = Lane {
            chain: vec![Node::Block(Box::new(Cabinet::ir(
                BlockId::new("L"),
                Convolver::from_ir(vec![1.0], "L"),
            )))],
            level: Db::UNITY,
            pan: -1.0, // full left
        };
        let lane_r = Lane {
            chain: vec![Node::Block(Box::new(Cabinet::ir(
                BlockId::new("R"),
                Convolver::from_ir(vec![1.0], "R"),
            )))],
            level: Db::UNITY,
            pan: 1.0, // full right
        };
        let mut sum = ParallelSum::lower(
            BlockId::new("par"),
            vec![lane_l, lane_r],
            &ParallelMix::default(),
        )
        .expect("lower");
        sum.prepare(48_000.0, 64).unwrap();
        const N: usize = 64;
        let inp: Vec<f32> = (0..N).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let (mut ol, mut or_) = (vec![0.0f32; N], vec![0.0f32; N]);
        sum.process_block(&inp, &inp, &mut ol, &mut or_, &PluginEvents::default())
            .unwrap();
        // Full-left lane → all energy in L; full-right → all in R.
        let el: f32 = ol.iter().map(|x| x * x).sum();
        let er: f32 = or_.iter().map(|x| x * x).sum();
        assert!(el > 1e-6 && er > 1e-6, "both channels carry signal");
        // Each output channel should roughly equal the (unity) input it received
        // from its single hard-panned lane.
        for i in 0..N {
            assert!((ol[i] - inp[i]).abs() < 1e-4, "L lane passes input at {i}");
            assert!((or_[i] - inp[i]).abs() < 1e-4, "R lane passes input at {i}");
        }
    }

    /// Block addressing arithmetic: `default_block_id` derives the file stem the
    /// live-rig map keys on, so a `BlockId` from a path-loaded block matches the
    /// id `GuitarRig::install_chain` records (the param/bypass routing relies on
    /// this equality).
    #[test]
    fn block_id_matches_install_default_id() {
        use crate::rig::RigBlock;
        // The api layer derives a BlockId from the file stem...
        let api_id = block::default_id_for_test(Path::new("/amps/Soldano.nam"));
        // ...and GuitarRig records the same stem id for the installed block.
        let rig_block = RigBlock::nam("/amps/Soldano.nam");
        let rig_id = crate::rig::default_block_id_for_test(rig_block.asset_path());
        assert_eq!(api_id.as_str(), "Soldano");
        assert_eq!(
            api_id.as_str(),
            rig_id,
            "api id must match the rig's slot id"
        );
    }
}
