//! Blocks: what a lane is made of.
//!
//! Every block (EQ, comp, gate, drive, reverb, delay, NAM, the sampler) is
//! compiled in by default, on every platform, and each also builds as its
//! own CLAP (FastTrackStudios/plugins). Signal's chain holds a block as a
//! [`ChainBlock`], whatever runs it:
//!
//! ```text
//! ChainBlock { id, kind, params, state, backend }
//!                         │                  ├─ Linked(Box<dyn BlockProcessor>)   the default
//!                         │                  └─ Clap(HostedPlugin)                its CLAP, or a third-party plugin
//!                         └─ ParamBank: base values (plain) + modulation (normalized), by stable id
//! ```
//!
//! - **One schema.** A block's parameters are its spec's (`id`, range,
//!   unit), the same for the compiled-in block and its CLAP, so a preset
//!   doesn't change when a block is overridden. The values live in the
//!   [`ParamBank`], not in the backend: swapping backends replays them.
//! - **Macros and modulation work 100% on either backend.** They drive
//!   parameters by id, never a block's internals. A set moves the base
//!   value; a modulation offsets it in normalized units and keeps the base
//!   (`CLAP_EVENT_PARAM_MOD` on the CLAP backend). Changes land between
//!   pieces: the processor splits a host block at each change, so both
//!   backends see them at the same frame.
//! - **Modulation sources** (an envelope, gain reduction) come from the
//!   linked block directly, and from a CLAP block through Signal's
//!   extension (`com.fasttrackstudio.signal/1`).
//! - **Where it runs.** Signal's own blocks run everywhere; a CLAP backend
//!   only where a [`BlockFactory`] for it exists (`signal-plugin-host` on
//!   the desktop) and the plugin is installed. Where a block can't be
//!   built, its fallback is, or a visible bypass ([`BlockStatus`]).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

pub use plugin_processors::{BlockProcessor, Midi, ModSourceSpec};

use crate::host::{AssetRef, SignalHost};
use crate::param::ParamSpec;
use crate::platform::Platforms;
use crate::state::BlockSpec;

/// What a block is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockKind {
    /// One of Signal's own, by name (`eq`, `comp`, `nam`, `cab`, …),
    /// compiled in unless overridden by its CLAP.
    Signal(String),
    /// A third-party plugin hosted inside Signal: always the CLAP backend.
    Plugin(PluginRef),
}

/// A third-party plugin, as a preset names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginRef {
    pub format: PluginFormat,
    /// The plugin's own id (a CLAP id, a VST3 class id).
    pub id: String,
    /// What a person calls it: "Valhalla Supermassive".
    pub name: String,
    /// The versions the preset was made with ("2.x").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A plugin format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginFormat {
    Clap,
    Vst3,
    /// AUv3 on iOS and macOS: a later possibility.
    Auv3,
}

impl PluginFormat {
    /// Where the format can load at all.
    #[must_use]
    pub const fn platforms(self) -> Platforms {
        match self {
            Self::Clap | Self::Vst3 => Platforms::DESKTOP,
            Self::Auv3 => Platforms::only(crate::Platform::Ios).with(crate::Platform::Macos),
        }
    }
}

impl fmt::Display for BlockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signal(name) => f.write_str(name),
            Self::Plugin(p) => write!(f, "{} ({:?})", p.name, p.format),
        }
    }
}

// ── Parameters: one schema, either backend ──────────────────────────────

/// A change to one parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamChange {
    /// Set the base value (plain units).
    Set(f32),
    /// Set this source's modulation offset (normalized units). The base
    /// value is kept; offsets from several sources add.
    Modulate(f32),
}

/// A block's parameter values: the base (plain) and the summed modulation
/// (normalized) of each, by spec index. The values belong to the chain, so
/// they survive a backend swap.
pub struct ParamBank {
    specs: &'static [ParamSpec],
    base: Vec<f32>,
    modulation: Vec<f32>,
}

impl ParamBank {
    #[must_use]
    pub fn new(specs: &'static [ParamSpec]) -> Self {
        Self {
            specs,
            base: specs.iter().map(|s| s.default).collect(),
            modulation: alloc::vec![0.0; specs.len()],
        }
    }

    #[must_use]
    pub const fn specs(&self) -> &'static [ParamSpec] {
        self.specs
    }

    /// The index of the parameter with stable id `id`.
    #[must_use]
    pub fn index(&self, id: &str) -> Option<usize> {
        self.specs.iter().position(|s| s.id == id)
    }

    /// Its base value (plain).
    #[must_use]
    pub fn base(&self, index: usize) -> Option<f32> {
        self.base.get(index).copied()
    }

    /// Its summed modulation (normalized).
    #[must_use]
    pub fn modulation(&self, index: usize) -> Option<f32> {
        self.modulation.get(index).copied()
    }

    /// The value it runs at: the base, modulated.
    #[must_use]
    pub fn effective(&self, index: usize) -> Option<f32> {
        let spec = self.specs.get(index)?;
        Some(spec.modulated(self.base(index)?, self.modulation(index)?))
    }

    fn set_base(&mut self, index: usize, value: f32) -> Option<f32> {
        let spec = self.specs.get(index)?;
        let v = spec.clamp(value);
        *self.base.get_mut(index)? = v;
        Some(v)
    }

    fn add_modulation(&mut self, index: usize, delta: f32) {
        if let Some(m) = self.modulation.get_mut(index) {
            *m += delta;
        }
    }
}

// ── Backends ────────────────────────────────────────────────────────────

/// A parameter event as a CLAP backend takes it: the CLAP form's own
/// parameter id and a normalized value, at a frame of the next piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClapParamEvent {
    pub offset: u32,
    /// The CLAP parameter id the block's spec index maps to.
    pub param_id: u32,
    pub kind: ClapParamKind,
}

/// `CLAP_EVENT_PARAM_VALUE` or `CLAP_EVENT_PARAM_MOD`, both normalized
/// (nice-plug's parameter domain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClapParamKind {
    Value(f64),
    Mod(f64),
}

/// A running CLAP: implemented on the desktop over `signal-plugin-host`
/// (`daw::plugin`), and by the null-test harness's loopback. The core
/// never loads a library itself, so it builds where none can be loaded.
pub trait ClapInstance: Send {
    /// One piece, with the parameter events that land in it.
    fn process(&mut self, left: &mut [f32], right: &mut [f32], events: &[ClapParamEvent]);
    fn reset(&mut self);
    fn latency(&self) -> u32;
    /// The CLAP's state, for a third-party block's [`ChainBlock`] `state`
    /// and for passing a block across a hot reload.
    fn save_state(&mut self) -> Option<Vec<u8>>;
    fn load_state(&mut self, state: &[u8]) -> bool;
    /// Modulation source `index`'s value after the last piece, as the
    /// plugin publishes it over `com.fasttrackstudio.signal/1`. `None` from
    /// a plugin without the extension.
    fn mod_source(&self, index: usize) -> Option<f32> {
        let _ = index;
        None
    }
    fn midi(&mut self, event: Midi) {
        let _ = event;
    }
    /// The CLAP's own account of what it runs (its `signal/1` chain, as
    /// JSON): its algorithm and each control's running value, for the views.
    fn describe(&self) -> Option<String> {
        None
    }
}

/// A block's CLAP backend: the instance, and the CLAP parameter id of each
/// of the block's spec indices (resolved once, when it's loaded: the
/// CLAP form's ids are derived from the same stable string ids).
pub struct HostedPlugin {
    pub instance: Box<dyn ClapInstance>,
    pub param_ids: Vec<u32>,
    /// What was loaded, for the UI and the usage log.
    pub path: String,
    /// The events for the next piece, preallocated.
    pending: Vec<ClapParamEvent>,
}

/// Events a CLAP backend queues between two pieces before it drops any
/// (one per parameter per change point is far below this).
pub const CLAP_EVENT_CAPACITY: usize = 512;

impl HostedPlugin {
    #[must_use]
    pub fn new(
        instance: Box<dyn ClapInstance>,
        param_ids: Vec<u32>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            instance,
            param_ids,
            path: path.into(),
            pending: Vec::with_capacity(CLAP_EVENT_CAPACITY),
        }
    }

    fn queue(&mut self, index: usize, kind: ClapParamKind) {
        let Some(&param_id) = self.param_ids.get(index) else {
            return;
        };
        // Within capacity only: pushing past it would allocate on the
        // audio thread.
        if self.pending.len() < self.pending.capacity() {
            self.pending.push(ClapParamEvent {
                offset: 0,
                param_id,
                kind,
            });
        }
    }
}

/// What runs a block.
pub enum Backend {
    /// Compiled in: the default, everywhere.
    Linked(Box<dyn BlockProcessor>),
    /// Its CLAP (a development override, or a third-party plugin).
    Clap(HostedPlugin),
}

impl Backend {
    #[must_use]
    pub const fn is_linked(&self) -> bool {
        matches!(self, Self::Linked(_))
    }
}

/// Passes audio through: what a block that can't be built becomes when it
/// has no fallback.
pub struct Passthrough;

impl BlockProcessor for Passthrough {
    fn params(&self) -> &'static [ParamSpec] {
        &[]
    }
    fn set_param(&mut self, _index: usize, _value: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}
}

// ── The chain's block ───────────────────────────────────────────────────

/// One block in a chain.
pub struct ChainBlock {
    pub id: String,
    pub kind: BlockKind,
    params: ParamBank,
    sources: &'static [ModSourceSpec],
    /// State that isn't parameters: a third-party plugin's own blob.
    /// Signal's blocks keep everything in their parameters (and their
    /// asset), so theirs is `None`.
    pub state: Option<Vec<u8>>,
    backend: Backend,
    pub bypass: bool,
    status: BlockStatus,
}

impl ChainBlock {
    /// A block over `backend`, with parameter values from `params`.
    #[must_use]
    pub fn new(
        id: String,
        kind: BlockKind,
        params: ParamBank,
        sources: &'static [ModSourceSpec],
        backend: Backend,
        status: BlockStatus,
    ) -> Self {
        let mut block = Self {
            id,
            kind,
            params,
            sources,
            state: None,
            backend,
            bypass: false,
            status,
        };
        block.replay_params();
        block
    }

    #[must_use]
    pub const fn status(&self) -> &BlockStatus {
        &self.status
    }

    #[must_use]
    pub const fn params(&self) -> &ParamBank {
        &self.params
    }

    #[must_use]
    pub const fn backend(&self) -> &Backend {
        &self.backend
    }

    /// What a CLAP backend says it runs (see [`ClapInstance::describe`]).
    #[must_use]
    pub fn describe_clap(&self) -> Option<String> {
        match &self.backend {
            Backend::Clap(h) => h.instance.describe(),
            Backend::Linked(_) => None,
        }
    }

    #[must_use]
    pub const fn mod_sources(&self) -> &'static [ModSourceSpec] {
        self.sources
    }

    /// Modulation source `index`'s value after the last piece: `None` from
    /// a CLAP that doesn't publish it.
    #[must_use]
    pub fn mod_source(&self, index: usize) -> Option<f32> {
        match &self.backend {
            Backend::Linked(p) => (index < self.sources.len()).then(|| p.mod_source(index)),
            Backend::Clap(h) => h.instance.mod_source(index),
        }
    }

    /// Change a parameter. Lands before the next piece, on either backend.
    /// `Modulate` takes the *delta* of one source's offset; the caller
    /// keeps each source's contribution (see the graph's macros).
    pub fn apply(&mut self, index: usize, change: ParamChange) {
        match change {
            ParamChange::Set(v) => {
                if self.params.set_base(index, v).is_none() {
                    return;
                }
            }
            ParamChange::Modulate(delta) => self.params.add_modulation(index, delta),
        }
        self.push(index);
    }

    /// Hand parameter `index`'s current values to the backend.
    fn push(&mut self, index: usize) {
        match &mut self.backend {
            Backend::Linked(p) => {
                if let Some(v) = self.params.effective(index) {
                    p.set_param(index, v);
                }
            }
            Backend::Clap(h) => {
                let Some(spec) = self.params.specs.get(index) else {
                    return;
                };
                if let Some(base) = self.params.base(index) {
                    h.queue(index, ClapParamKind::Value(f64::from(spec.normalize(base))));
                }
                if let Some(m) = self.params.modulation(index) {
                    h.queue(index, ClapParamKind::Mod(f64::from(m)));
                }
            }
        }
    }

    /// Give the backend every parameter: after it's built or swapped in.
    fn replay_params(&mut self) {
        for i in 0..self.params.specs.len() {
            self.push(i);
        }
    }

    /// Swap in another backend at a block boundary (the hot-reload
    /// override, or ending it), handing it every parameter so the preset
    /// is unchanged. Returns the old one, to be dropped off the audio
    /// thread.
    pub fn swap_backend(&mut self, backend: Backend) -> Backend {
        let old = core::mem::replace(&mut self.backend, backend);
        self.replay_params();
        old
    }

    pub fn reset(&mut self) {
        match &mut self.backend {
            Backend::Linked(p) => p.reset(),
            Backend::Clap(h) => h.instance.reset(),
        }
    }

    #[must_use]
    pub fn latency(&self) -> u32 {
        match &self.backend {
            Backend::Linked(p) => p.latency(),
            Backend::Clap(h) => h.instance.latency(),
        }
    }

    /// The key for the next piece (a gate's sidechain: the piece's Trig).
    /// A CLAP backend doesn't take one yet (it would go to its sidechain
    /// port).
    pub fn set_key(&mut self, left: &[f32], right: &[f32]) {
        if let Backend::Linked(p) = &mut self.backend {
            p.set_key(left, right);
        }
    }

    /// A drum hit in the next piece. A CLAP backend doesn't take one yet
    /// (it would go as a note event).
    pub fn hit(&mut self, offset: u32, velocity: f32) {
        if let Backend::Linked(p) = &mut self.backend {
            p.hit(offset, velocity);
        }
    }

    pub fn midi(&mut self, event: Midi) {
        match &mut self.backend {
            Backend::Linked(p) => p.midi(event),
            Backend::Clap(h) => h.instance.midi(event),
        }
    }

    /// Process one piece in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.bypass {
            return;
        }
        match &mut self.backend {
            Backend::Linked(p) => p.process(left, right),
            Backend::Clap(h) => {
                h.instance.process(left, right, &h.pending);
                h.pending.clear();
            }
        }
    }
}

// ── Build outcomes ──────────────────────────────────────────────────────

/// What became of a block when its lane was built: what the UI shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum BlockStatus {
    /// Running as the preset has it.
    Active,
    /// Its fallback runs in its place.
    Fallback {
        reason: Unavailable,
        fallback: BlockKind,
    },
    /// Passing audio through untouched.
    Bypassed { reason: Unavailable },
}

impl BlockStatus {
    /// The reason it isn't running as the preset has it.
    #[must_use]
    pub const fn reason(&self) -> Option<&Unavailable> {
        match self {
            Self::Active => None,
            Self::Fallback { reason, .. } | Self::Bypassed { reason } => Some(reason),
        }
    }
}

/// Why a block couldn't be built here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Unavailable {
    /// It runs on `needs`, and this is not one of them (a CLAP on an iPad).
    Platform { needs: Platforms },
    /// Nothing in this build makes this kind (a newer preset's block, a
    /// plugin with no host for its format).
    UnknownKind,
    /// The plugin, or the file the block needs, isn't on this machine.
    Missing { what: String },
    /// It failed to load.
    Failed { why: String },
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Platform { needs } => {
                f.write_str("runs on ")?;
                for (i, p) in needs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{p:?}")?;
                }
                Ok(())
            }
            Self::UnknownKind => f.write_str("not in this build"),
            Self::Missing { what } => write!(f, "missing: {what}"),
            Self::Failed { why } => write!(f, "failed: {why}"),
        }
    }
}

// ── Factories ───────────────────────────────────────────────────────────

/// What a factory gets to build a block with.
pub struct BuildContext<'a> {
    pub sample_rate: f32,
    pub max_block: usize,
    pub host: &'a dyn SignalHost,
}

impl BuildContext<'_> {
    /// The bytes of the file `spec` needs.
    ///
    /// # Errors
    ///
    /// [`Unavailable::Missing`] if the spec names none or the host has none.
    pub fn asset_bytes(&self, spec: &BlockSpec) -> Result<Vec<u8>, Unavailable> {
        let asset: &AssetRef = spec.asset.as_ref().ok_or_else(|| Unavailable::Missing {
            what: alloc::format!("{}: no file named", spec.id),
        })?;
        self.host
            .load_file(asset)
            .map_err(|e| Unavailable::Missing {
                what: alloc::format!("{}: {e}", asset.path),
            })
    }
}

/// A built backend and the parameter schema it runs.
pub struct Built {
    pub backend: Backend,
    pub params: &'static [ParamSpec],
    pub sources: &'static [ModSourceSpec],
}

/// Makes blocks of the kinds it knows.
///
/// The core's [`BuiltinFactory`] makes Signal's own, linked. A desktop
/// host adds a CLAP factory beside it (over `signal-plugin-host`), which
/// makes third-party plugins and, for a block under development, that
/// block's CLAP form, with the compiled-in block's schema. The graph asks
/// each in turn.
pub trait BlockFactory: Sync {
    /// Where blocks of `kind` can run, if this factory makes them.
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms>;

    /// Build `spec`. Called only for a kind [`runs_on`](Self::runs_on)
    /// claimed, on a platform it named.
    ///
    /// # Errors
    ///
    /// Why it couldn't be built here.
    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable>;
}

/// Signal's own blocks, compiled in.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinFactory;

impl BlockFactory for BuiltinFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        match kind {
            BlockKind::Signal(name) => crate::blocks::exists(name).then_some(Platforms::ALL),
            BlockKind::Plugin(_) => None,
        }
    }

    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        match &spec.kind {
            BlockKind::Signal(name) => {
                let p = crate::blocks::build(name, spec, cx)?;
                Ok(Built {
                    params: p.params(),
                    sources: p.mod_sources(),
                    backend: Backend::Linked(p),
                })
            }
            BlockKind::Plugin(_) => Err(Unavailable::UnknownKind),
        }
    }
}
