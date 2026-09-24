//! GigPerformer-style profile/patch layer over [`GuitarRig`].
//!
//! A [`RigProfile`] (e.g. "Worship") is a list of [`RigPatch`]es (e.g. "Clean",
//! "Lead"), each a named tone realized as an ordered FX **chain** (drive → amp →
//! cab → …) plus patch-level input/output trims. [`ProfileRig`] loads a profile
//! by **pre-installing every patch's chain into the rig's resident bank**, so
//! switching patches mid-set is a single lock-free atomic — no reload, no
//! dropout.
//!
//! ## Level-matching across patches
//!
//! When [`ProfileRig::set_level_match`] is on, each patch's output trim is
//! auto-compensated from its amp model's NAM `loudness` metadata so "Clean" and
//! "Lead" land at a consistent perceived level. See [`ProfileRig::activate`].
//!
//! ## Relationship to the `signal-proto` Profile model
//!
//! Each NAM block is a [`signal_proto::block_kind::NamRef`] — the same reference
//! a proto `Block` with `BlockKind::Nam` carries. [`RigProfile::from_proto`]
//! converts a proto [`Profile`](signal_proto::profile::Profile) into a
//! `RigProfile` given a [`PatchResolver`] (which the `signal-live` resolve stack
//! implements); this layer stays repo-free so the standalone rig runs without
//! the storage stack.

use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::{collections::HashMap, sync::Arc};

use facet::Facet;

use crate::SamplerError;
use crate::rig::RigBlock;
#[cfg(not(target_arch = "wasm32"))]
use crate::rig::{GuitarRig, ModelId};
#[cfg(not(target_arch = "wasm32"))]
use crate::block_params::{BlockDelta, BlockWrite};
#[cfg(not(target_arch = "wasm32"))]
use crate::rig::{PreparedChain, prepare_chain};

/// One patch in a rig profile: a named tone whose chain is either inlined or
/// **referenced** from a [`RigPreset`](crate::rig_library::RigPreset) scene.
///
/// Per the Signal domain model, a Patch (a Profile entry) *points at* a Preset
/// Snapshot — here, a named `scene` of a named `preset`. When `preset`/`scene`
/// are set, the [`Library`](crate::rig_library::Library) resolves them into the
/// actual `chain` (folding the scene's trims into the patch's). When they're
/// empty, the inline `chain` is used directly — the standalone rig stays usable
/// without a library.
#[derive(Debug, Clone, Facet)]
pub struct RigPatch {
    /// Patch name shown on the switcher (e.g. "Clean", "Lead").
    pub name: String,
    /// Name of the [`RigPreset`](crate::rig_library::RigPreset) this patch points
    /// at. Empty = inline `chain`.
    #[facet(default)]
    pub preset: String,
    /// Scene (snapshot) name within `preset`. Empty with a non-empty `preset`
    /// uses the preset's default scene.
    #[facet(default)]
    pub scene: String,
    /// Ordered FX chain (drive → amp → cab → …). Each block is a NAM model or a
    /// cabinet IR; see [`RigBlock`]. Populated inline, or filled by the library
    /// when this patch references a preset scene.
    #[facet(default)]
    pub chain: Vec<RigBlock>,
    /// Patch-level trim before the chain (dB).
    #[facet(default)]
    pub input_trim_db: f32,
    /// Patch-level trim after the chain (dB).
    #[facet(default)]
    pub output_trim_db: f32,
}

impl RigPatch {
    /// A patch that is a single NAM amp (no cab / extra blocks).
    pub fn amp(name: impl Into<String>, model_path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            preset: String::new(),
            scene: String::new(),
            chain: vec![RigBlock::nam(model_path)],
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }

    /// An empty-chain patch to build up with [`with_block`](Self::with_block).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            preset: String::new(),
            scene: String::new(),
            chain: Vec::new(),
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }

    /// A patch that *references* a [`RigPreset`](crate::rig_library::RigPreset)
    /// scene (resolved by the [`Library`](crate::rig_library::Library)). An empty
    /// `scene` uses the preset's default scene.
    pub fn from_preset(
        name: impl Into<String>,
        preset: impl Into<String>,
        scene: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            preset: preset.into(),
            scene: scene.into(),
            chain: Vec::new(),
            input_trim_db: 0.0,
            output_trim_db: 0.0,
        }
    }

    #[must_use]
    pub fn with_block(mut self, block: RigBlock) -> Self {
        self.chain.push(block);
        self
    }

    #[must_use]
    pub fn with_trims(mut self, input_db: f32, output_db: f32) -> Self {
        self.input_trim_db = input_db;
        self.output_trim_db = output_db;
        self
    }
}

/// A **Stack** — a footswitch group holding an ordered *rotation* of patches.
///
/// Per the user's FM9-style model: a stack maps to one footswitch. Press it to
/// activate its current patch; press again while it's already active to rotate
/// to the next patch in the stack (wraps). Patches live on the owning
/// [`RigProfile`]'s `patches` pool — a stack just lists their names, so a patch
/// can appear in more than one stack. See the `stacks-footswitch-model` note.
#[derive(Debug, Clone, Facet)]
pub struct RigStack {
    /// Stack / footswitch name (e.g. "Clean", "Crunch", "Lead").
    pub name: String,
    /// Patch names — references into the profile's `patches` — in rotation order.
    pub patches: Vec<String>,
}

impl RigStack {
    pub fn new(
        name: impl Into<String>,
        patches: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            patches: patches.into_iter().map(Into::into).collect(),
        }
    }
}

/// A named collection of patches — the standalone-rig analogue of a `signal_proto::Profile`.
///
/// Patches can be activated directly (flat, by index) or grouped into [`RigStack`]s
/// (footswitch rotation); both views share the same `patches` pool.
#[derive(Debug, Clone, Facet)]
pub struct RigProfile {
    pub name: String,
    pub patches: Vec<RigPatch>,
    /// Index of the patch to make active on load. Defaults to 0.
    #[facet(default)]
    pub default_patch: usize,
    /// Footswitch stacks grouping the patches. Empty = flat (index-activated)
    /// profile; existing profiles without stacks still parse.
    #[facet(default)]
    pub stacks: Vec<RigStack>,
}

impl RigProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            patches: Vec::new(),
            default_patch: 0,
            stacks: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_patch(mut self, patch: RigPatch) -> Self {
        self.patches.push(patch);
        self
    }

    #[must_use]
    pub fn with_stack(mut self, stack: RigStack) -> Self {
        self.stacks.push(stack);
        self
    }

    /// Index of the patch named `name` (case-insensitive) in `patches`.
    #[must_use]
    pub fn patch_index(&self, name: &str) -> Option<usize> {
        self.patches
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Parse a profile from a `.styx` file (see `examples/worship.styx`).
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the content is invalid.
    pub fn from_styx_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_styx_str(&text)
    }

    /// Parse a profile from a string.
    ///
    /// # Errors
    /// Returns an error if the content is invalid.
    pub fn from_styx_str(text: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Convert a `signal_proto::Profile` into a `RigProfile` using a resolver.
    ///
    /// Uses `resolver` to turn each patch into an ordered chain of realized blocks.
    /// NAM blocks map to [`RigBlock::nam`]; cabinet blocks with an IR path map to
    /// [`RigBlock::cab_ir`]. Non-NAM/non-cab blocks (hosted plugins, native DSP without a path)
    /// are skipped with a log — the standalone rig can't host them yet. A patch that resolves
    /// to an empty chain is kept (it will be "unavailable" at load time).
    ///
    /// # Errors
    /// Returns an error if the resolver fails.
    pub fn from_proto<R: PatchResolver>(
        profile: &signal_proto::profile::Profile,
        resolver: &R,
    ) -> Result<Self, String> {
        use signal_proto::block_kind::BlockKind;

        let mut patches = Vec::with_capacity(profile.patches.len());
        let mut default_patch = 0;
        for (i, patch) in profile.patches.iter().enumerate() {
            if patch.id == profile.default_patch_id {
                default_patch = i;
            }
            let resolved = resolver.resolve_patch(patch)?;
            let mut chain = Vec::new();
            for rb in resolved {
                match rb.kind {
                    BlockKind::Nam { model } => chain.push(RigBlock::nam(model.model_path)),
                    BlockKind::HostedPlugin { plugin: href } => {
                        chain.push(RigBlock::plugin_with_state(href.path, href.state_b64));
                    }
                    BlockKind::Native => {
                        // A cabinet realized natively carries its IR path.
                        if let Some(ir) = rb.cab_ir_path {
                            chain.push(RigBlock::cab_ir(ir));
                        } else {
                            tracing::debug!(
                                patch = %patch.name,
                                "from_proto: skipping native block with no IR path"
                            );
                        }
                    }
                    other => {
                        tracing::warn!(
                            patch = %patch.name,
                            kind = other.tag(),
                            "from_proto: skipping block — not supported in the standalone rig yet"
                        );
                    }
                }
            }
            patches.push(RigPatch {
                name: patch.name.clone(),
                preset: String::new(),
                scene: String::new(),
                chain,
                input_trim_db: 0.0,
                output_trim_db: 0.0,
            });
        }
        Ok(Self {
            name: profile.name.clone(),
            patches,
            default_patch,
            stacks: Vec::new(),
        })
    }
}

/// Resolves a proto `Patch` into an ordered list of realized blocks.
///
/// The `signal-live` resolve stack (repos + `ResolveService`) implements this;
/// the standalone rig only needs this narrow surface, so it stays decoupled from storage.
/// Each [`ResolvedRigBlock`] is one block in chain order.
pub trait PatchResolver {
    /// Resolve a patch into blocks.
    ///
    /// # Errors
    /// Returns an error if the patch cannot be resolved.
    fn resolve_patch(
        &self,
        patch: &signal_proto::profile::Patch,
    ) -> Result<Vec<ResolvedRigBlock>, String>;
}

/// One realized block from a [`PatchResolver`]: its `BlockKind` (Nam / Native /
/// `HostedPlugin` / …) plus, for a natively-realized cabinet, its IR path.
#[derive(Debug, Clone)]
pub struct ResolvedRigBlock {
    pub kind: signal_proto::block_kind::BlockKind,
    /// IR path for a native cabinet block, if any (extracted from the block's
    /// parameters by the resolver).
    pub cab_ir_path: Option<String>,
}

impl ResolvedRigBlock {
    pub fn nam(model_path: impl Into<String>) -> Self {
        Self {
            kind: signal_proto::block_kind::BlockKind::Nam {
                model: signal_proto::block_kind::NamRef {
                    model_path: model_path.into(),
                    model_id: None,
                },
            },
            cab_ir_path: None,
        }
    }

    pub fn cab_ir(ir_path: impl Into<String>) -> Self {
        Self {
            kind: signal_proto::block_kind::BlockKind::Native,
            cab_ir_path: Some(ir_path.into()),
        }
    }

    pub fn plugin(
        format: impl Into<String>,
        path: impl Into<String>,
        state_b64: Option<String>,
    ) -> Self {
        Self {
            kind: signal_proto::block_kind::BlockKind::HostedPlugin {
                plugin: signal_proto::block_kind::HostedPluginRef {
                    format: format.into(),
                    path: path.into(),
                    state_b64,
                },
            },
            cab_ir_path: None,
        }
    }
}

/// Resolve a block path against the profile file's directory when relative.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(path);
    match base_dir {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p,
    }
}

/// A [`GuitarRig`] plus the active [`RigProfile`] — the live switcher.
#[cfg(not(target_arch = "wasm32"))]
pub struct ProfileRig {
    rig: GuitarRig,
    profile: Option<RigProfile>,
    /// `ModelId` for each patch, parallel to `profile.patches`.
    patch_ids: Vec<ModelId>,
    active: Option<usize>,
    /// Per-stack rotation cursor, parallel to `profile.stacks`. Advancing a
    /// stack (re-pressing its footswitch) bumps its cursor (wrapping).
    stack_pos: Vec<usize>,
    /// A song's rotations: the profile's own list for each stack a song has
    /// replaced, kept to put back when the song goes (`None` = untouched).
    stack_base: Vec<Option<Vec<String>>>,
    /// Stacks whose switch always lands on its patch rather than rotating.
    no_rotate: Vec<bool>,
    /// The spec each installed chain was built from, so a reload rebuilds
    /// only the chains whose spec changed (see [`ReloadTicket::plan`]).
    chain_keys: HashMap<ModelId, ChainKey>,
    /// The newest reload ticket's generation; only it may commit.
    reload_gen: u64,
    /// Every value written to a running chain since it was built or
    /// retuned — what a reload puts back on that patch's next chain.
    /// Control-thread only.
    live_log: std::sync::Mutex<LiveLog>,
    /// Global "time bypass": when on, every time/fx block (the Time module) on
    /// the active patch is bypassed. Re-applied on each `activate`.
    fx_bypass: bool,
    /// Auto level-match patches from measured NAM loudness (LUFS).
    level_match: bool,
    /// Target loudness (dB) patches normalize toward when level-matching.
    target_loudness_db: f32,
    /// Feed each NAM model the analog input level (dBu) it was captured at, so
    /// its drive/tone is authentic. Off by default — needs a correct interface
    /// calibration value to help rather than hurt.
    calibrated_input: bool,
    /// The interface's input calibration: the analog level (dBu) that equals
    /// 0 dBFS at the DI input. Used with each model's `input_level` to compute
    /// the pre-model calibration gain.
    input_calibration_dbu: f32,
}

#[cfg(not(target_arch = "wasm32"))]
/// One patch resolved into the blocks and ids a chain is built from.
struct ChainSpec {
    blocks: Vec<RigBlock>,
    block_ids: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
/// Build every chain, concurrently where the platform has threads.
///
/// The result is parallel to `specs` — `None` where the patch had nothing to
/// build — so the caller can install them in patch order.
fn prepare_all(
    specs: &[Option<Arc<ChainSpec>>],
    sample_rate: u32,
) -> Vec<Option<Result<PreparedChain, String>>> {
    let one = |spec: &Option<Arc<ChainSpec>>| {
        spec.as_ref()
            .map(|s| prepare_chain(&s.blocks, &s.block_ids, sample_rate))
    };

    // No threads in the browser build.
    #[cfg(target_arch = "wasm32")]
    return specs.iter().map(one).collect();

    #[cfg(not(target_arch = "wasm32"))]
    {
        // One core left over: a build runs while the rig plays, and the
        // footswitches, the pump and the UI must still get a look in.
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .saturating_sub(1)
            .max(1)
            .min(specs.len());
        if threads <= 1 {
            return specs.iter().map(one).collect();
        }
        let chunk = specs.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let handles: Vec<_> = specs
                .chunks(chunk)
                .map(|c| scope.spawn(move || c.iter().map(one).collect::<Vec<_>>()))
                .collect();
            handles
                .into_iter()
                // A panic in a build thread is the caller's panic; losing it
                // would silently shorten the vec and misalign every patch
                // after it.
                .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        })
    }
}

// ── Gapless reload ───────────────────────────────────────────────────────
//
// An edit that changes a chain used to reload the whole profile: clear the
// rig (silence, tails cut), forget where every stack was, and build every
// patch again — seconds, under whatever lock guarded the rig. A reload is
// now three phases:
//
// 1. **ticket** (`ProfileRig::begin_reload`, under the lock, cheap): the
//    spec of every installed chain, a generation, and where the live-write
//    log stood;
// 2. **plan + prepare** (`ReloadTicket::plan`, `ReloadPlan::prepare`, no
//    lock, no rig): resolve the new profile's chains; keep every one whose
//    spec is unchanged, *retune* one that differs only in what can be
//    written to its running blocks (params, NAM trims — see
//    `block_params`), and build only the rest;
// 3. **commit** (`ProfileRig::commit_reload`, under the lock, fast): write
//    the retunes, install the builds with the patch's live state (knobs,
//    macros, boost, tempo: the live-write log, and the session's overlay)
//    already on them, remap by patch name, carry the switcher's state
//    across, switch a rebuilt playing chain in the way a footswitch does,
//    retire what is left.

/// Hands out reload generations, process-wide — so a plan made against one
/// rig can never be mistaken for current on another (a reopened device).
#[cfg(not(target_arch = "wasm32"))]
fn next_reload_gen() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// What a reload does with the switcher's state.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadMode {
    /// The same profile, edited: the playing patch plays on, and each stack
    /// keeps its cursor, a song's rotation and its no-rotate flag — all by
    /// name.
    Keep,
    /// Another profile (or the first): stacks start fresh and it lands on
    /// its default patch.
    Switch,
}

/// The installed chain behind a patch, and the spec it was built from.
#[cfg(not(target_arch = "wasm32"))]
struct ChainKey {
    patch: String,
    key: String,
    /// The part of `key` a retune cannot change: asset stamps, NAM build
    /// settings.
    stamps: String,
    spec: Arc<ChainSpec>,
}

/// One installed chain, as a ticket carries it.
#[cfg(not(target_arch = "wasm32"))]
struct Installed {
    id: ModelId,
    patch: String,
    key: String,
    stamps: String,
    spec: Arc<ChainSpec>,
}

/// A stack's state carried across a [`ReloadMode::Keep`] reload.
#[cfg(not(target_arch = "wasm32"))]
struct CarriedStack {
    name: String,
    /// The rotation playing (a song's, when `song`).
    live: Vec<String>,
    song: bool,
    cursor_patch: Option<String>,
    pos: usize,
    no_rotate: bool,
}

/// What becomes of one patch's chain in a reload.
#[cfg(not(target_arch = "wasm32"))]
enum Fate {
    /// Nothing to build (no blocks with a backend).
    Nothing,
    /// The installed chain, as it is.
    Keep(ModelId),
    /// The installed chain, with these writes to its running blocks.
    Retune(ModelId, Vec<(usize, BlockWrite)>),
    /// A new chain.
    Build,
}

/// A value written to a running block since it was built — a knob, a macro,
/// the boost, the tempo on a delay — kept so a chain built or retuned for the
/// same patch comes in with it.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq)]
pub enum LiveWrite {
    /// A param, in the value the live path takes (plain, as the knob).
    Param(String, f32),
    Bypass(bool),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct LogEntry {
    block: String,
    write: LiveWrite,
    seq: u64,
}

/// Every chain's live writes, newest last, one per block and param.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct LiveLog {
    seq: u64,
    chains: HashMap<ModelId, Vec<LogEntry>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LiveLog {
    fn record(&mut self, chain: ModelId, block: &str, write: LiveWrite) {
        self.seq += 1;
        let entries = self.chains.entry(chain).or_default();
        entries.retain(|e| !(e.block == block && same_target(&e.write, &write)));
        entries.push(LogEntry { block: block.to_string(), write, seq: self.seq });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn same_target(a: &LiveWrite, b: &LiveWrite) -> bool {
    match (a, b) {
        (LiveWrite::Param(x, _), LiveWrite::Param(y, _)) => x == y,
        (LiveWrite::Bypass(_), LiveWrite::Bypass(_)) => true,
        _ => false,
    }
}

/// Phase one of a reload — see [`ProfileRig::begin_reload`].
#[cfg(not(target_arch = "wasm32"))]
pub struct ReloadTicket {
    generation: u64,
    sample_rate: u32,
    mode: ReloadMode,
    installed: Vec<Installed>,
    /// Where the live-write log stood: a write after this is *late* — made
    /// while this reload built — and wins over the reload's own values.
    log_seq: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReloadTicket {
    /// Resolve `profile`'s chains and diff them against the installed ones.
    ///
    /// A patch whose chain spec — blocks with their params, assets (and the
    /// assets' file stamps), block ids — matches an installed chain keeps
    /// that chain (its own first, then any other free one). A patch whose
    /// own chain differs only in what can be written to its running blocks
    /// (params of a built-in effect, a NAM block's trims — see
    /// [`block_delta`](crate::block_params::block_delta)) keeps it too,
    /// retuned. The rest are listed to build. Needs no rig: run it off the
    /// lock.
    #[must_use]
    pub fn plan(self, profile: RigProfile, base_dir: Option<&Path>) -> ReloadPlan {
        let specs: Vec<Option<Arc<ChainSpec>>> = resolve_specs(&profile, base_dir)
            .into_iter()
            .map(|s| s.map(Arc::new))
            .collect();
        let keys: Vec<Option<(String, String)>> =
            specs.iter().map(|s| s.as_deref().map(chain_key)).collect();
        let mut claimed = vec![false; self.installed.len()];
        let mut fate: Vec<Fate> = specs
            .iter()
            .map(|s| if s.is_some() { Fate::Build } else { Fate::Nothing })
            .collect();
        // Unchanged: its own chain first, so a patch keeps the chain it had
        // (and any tail it is ringing), then any identical free one.
        for own in [true, false] {
            for (i, patch) in profile.patches.iter().enumerate() {
                let Some((key, _)) = &keys[i] else { continue };
                if !matches!(fate[i], Fate::Build) {
                    continue;
                }
                let hit = self.installed.iter().enumerate().position(|(j, c)| {
                    !claimed[j] && c.key == *key && (!own || c.patch.eq_ignore_ascii_case(&patch.name))
                });
                if let Some(j) = hit {
                    claimed[j] = true;
                    fate[i] = Fate::Keep(self.installed[j].id);
                }
            }
        }
        // Changed only in settings: its own chain, retuned.
        for (i, patch) in profile.patches.iter().enumerate() {
            let (Some(spec), Some((_, stamps))) = (&specs[i], &keys[i]) else { continue };
            if !matches!(fate[i], Fate::Build) {
                continue;
            }
            let Some(j) = self.installed.iter().enumerate().position(|(j, c)| {
                !claimed[j] && c.patch.eq_ignore_ascii_case(&patch.name)
            }) else {
                continue;
            };
            let old = &self.installed[j];
            if old.stamps != *stamps || old.spec.block_ids != spec.block_ids {
                continue;
            }
            let mut writes = Vec::new();
            let mut live = true;
            for (slot, (a, b)) in old.spec.blocks.iter().zip(&spec.blocks).enumerate() {
                match crate::block_params::block_delta(a, b) {
                    BlockDelta::Same => {}
                    BlockDelta::Live(w) => writes.push((slot, w)),
                    BlockDelta::Structural => {
                        live = false;
                        break;
                    }
                }
            }
            if live {
                claimed[j] = true;
                fate[i] = Fate::Retune(old.id, writes);
            }
        }
        ReloadPlan {
            generation: self.generation,
            sample_rate: self.sample_rate,
            mode: self.mode,
            log_seq: self.log_seq,
            profile,
            keys,
            specs,
            fate,
        }
    }
}

/// Phase two of a reload: which chains to keep, retune and build.
#[cfg(not(target_arch = "wasm32"))]
pub struct ReloadPlan {
    generation: u64,
    sample_rate: u32,
    mode: ReloadMode,
    log_seq: u64,
    profile: RigProfile,
    keys: Vec<Option<(String, String)>>,
    specs: Vec<Option<Arc<ChainSpec>>>,
    fate: Vec<Fate>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReloadPlan {
    /// Chains this plan builds.
    #[must_use]
    pub fn builds(&self) -> usize {
        self.fate.iter().filter(|f| matches!(f, Fate::Build)).count()
    }

    /// Chains this plan keeps as they are.
    #[must_use]
    pub fn reuses(&self) -> usize {
        self.fate.iter().filter(|f| matches!(f, Fate::Keep(_))).count()
    }

    /// Chains this plan keeps and writes new settings to.
    #[must_use]
    pub fn retunes(&self) -> usize {
        self.fate.iter().filter(|f| matches!(f, Fate::Retune(..))).count()
    }

    /// Build the chains the plan lists — concurrently, and touching no rig,
    /// so the lock guarding it can be released for however long this takes.
    #[must_use]
    pub fn prepare(self) -> PreparedReload {
        let began = std::time::Instant::now();
        // Only what needs building goes to the builders, so one changed
        // chain is one chain's work, not a pass over every patch.
        let todo: Vec<usize> = (0..self.fate.len())
            .filter(|&i| matches!(self.fate[i], Fate::Build))
            .collect();
        let mut built: Vec<Option<Result<PreparedChain, String>>> =
            (0..self.fate.len()).map(|_| None).collect();
        if !todo.is_empty() {
            let specs: Vec<Option<Arc<ChainSpec>>> =
                todo.iter().map(|&i| self.specs[i].clone()).collect();
            for (&i, outcome) in todo.iter().zip(prepare_all(&specs, self.sample_rate)) {
                built[i] = outcome;
            }
        }
        PreparedReload {
            generation: self.generation,
            sample_rate: self.sample_rate,
            mode: self.mode,
            log_seq: self.log_seq,
            profile: self.profile,
            keys: self.keys,
            specs: self.specs,
            fate: self.fate,
            built,
            overlay: HashMap::new(),
            build_ms: began.elapsed().as_secs_f64() * 1000.0,
        }
    }
}

/// A reload whose chains are built, ready to commit — see
/// [`ProfileRig::commit_reload`].
#[cfg(not(target_arch = "wasm32"))]
pub struct PreparedReload {
    generation: u64,
    sample_rate: u32,
    mode: ReloadMode,
    log_seq: u64,
    profile: RigProfile,
    keys: Vec<Option<(String, String)>>,
    specs: Vec<Option<Arc<ChainSpec>>>,
    fate: Vec<Fate>,
    built: Vec<Option<Result<PreparedChain, String>>>,
    /// Per patch (lowercase name): writes to put on its new or retuned chain
    /// before it plays — the session's macro positions over the new
    /// baseline.
    overlay: HashMap<String, Vec<(String, LiveWrite)>>,
    build_ms: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl PreparedReload {
    /// The patch named `name` in the profile this reload commits.
    #[must_use]
    pub fn patch(&self, name: &str) -> Option<&RigPatch> {
        let i = self.profile.patch_index(name)?;
        self.profile.patches.get(i)
    }

    /// Whether this reload gives `name` a new chain or writes new settings
    /// to its chain — so its live state has to be put back on.
    #[must_use]
    pub fn changes(&self, name: &str) -> bool {
        self.profile
            .patch_index(name)
            .is_some_and(|i| matches!(self.fate[i], Fate::Build | Fate::Retune(..)))
    }

    /// Put `writes` — `(block id, write)` — on `patch`'s chain before it
    /// plays, if the reload changes it: after the logged live writes, before
    /// the late ones (see [`ProfileRig::commit_reload`]).
    pub fn set_overlay(&mut self, patch: &str, writes: Vec<(String, LiveWrite)>) {
        self.overlay.insert(patch.to_ascii_lowercase(), writes);
    }
}

/// How a commit went.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitStatus {
    Committed,
    /// A newer reload (or load) began after this one: nothing changed.
    Stale,
    /// Committed, but no patch had a chain to play.
    NothingBuilt,
}

/// Chains a commit let go of, freed when this drops.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Garbage {
    prepared: Vec<PreparedChain>,
    retired: Vec<crate::rig::RetiredChain>,
}

/// What a [`ProfileRig::commit_reload`] did. Holds the chains it let go of —
/// drop it after releasing the rig's lock.
#[cfg(not(target_arch = "wasm32"))]
pub struct ReloadCommit {
    pub status: CommitStatus,
    /// Chains built and installed.
    pub built: usize,
    /// Chains kept as they were.
    pub reused: usize,
    /// Chains kept, with new settings written to their running blocks.
    pub retuned: usize,
    /// Chains taken out.
    pub retired: usize,
    /// Chains that failed to build.
    pub failed: usize,
    /// Live writes put back on new and retuned chains before they played.
    pub carried: usize,
    /// Whether the chain playing changed (a gapless switch happened).
    pub switched: bool,
    /// How long phase two took, off the lock.
    pub build_ms: f64,
    /// How long this commit took, under it.
    pub commit_us: u64,
    garbage: Garbage,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReloadCommit {
    #[must_use]
    pub fn is_committed(&self) -> bool {
        self.status != CommitStatus::Stale
    }
}

/// The ids a patch's chain gives its blocks, in chain order — one per block
/// with a backend (the blocks a chain is built from).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn chain_block_ids(patch: &RigPatch) -> Vec<String> {
    let blocks: Vec<&RigBlock> = patch.chain.iter().filter(|b| b.has_backend()).collect();
    block_ids_for(&blocks)
}

#[cfg(not(target_arch = "wasm32"))]
fn block_ids_for(blocks: &[&RigBlock]) -> Vec<String> {
    // Stable, unique per-block ids (block name, deduped) so the UI can
    // address each block (bypass toggle, param edits) individually —
    // otherwise assetless native blocks all collapse to the id "block".
    let mut seen: HashMap<String, u32> = HashMap::new();
    blocks
        .iter()
        .map(|b| {
            let base = if b.name.trim().is_empty() {
                format!("{:?}", b.block_type)
            } else {
                b.name.trim().to_string()
            };
            let n = seen.entry(base.clone()).or_insert(0);
            let id = if *n == 0 { base.clone() } else { format!("{base} {}", *n + 1) };
            *n += 1;
            id
        })
        .collect()
}

/// Resolve every patch into the blocks and ids its chain is built from.
///
/// Resolves every buildable block's asset path against `base_dir` and skips
/// blocks with no audio backend yet — `Native` blocks whose built-in DSP
/// isn't written (a not-yet-chosen Time-module effect). They stay in the
/// patch for display and bypass grouping, and become live once given a
/// NAM/IR/plugin asset (or native DSP lands).
#[cfg(not(target_arch = "wasm32"))]
fn resolve_specs(profile: &RigProfile, base_dir: Option<&Path>) -> Vec<Option<ChainSpec>> {
    let resolve = |p: &str| -> String {
        if p.is_empty() {
            String::new()
        } else {
            resolve_path(p, base_dir).to_string_lossy().to_string()
        }
    };
    profile
        .patches
        .iter()
        .map(|patch| {
            let blocks: Vec<RigBlock> = patch
                .chain
                .iter()
                .filter(|b| b.has_backend())
                .map(|b| {
                    let mut rb = b.clone();
                    rb.nam = resolve(&b.nam);
                    rb.ir = resolve(&b.ir);
                    rb.plugin = resolve(&b.plugin);
                    rb
                })
                .collect();
            if blocks.is_empty() {
                tracing::warn!(patch = %patch.name, "ProfileRig: patch has no blocks — skipping");
                return None;
            }
            let block_ids = block_ids_for(&blocks.iter().collect::<Vec<_>>());
            Some(ChainSpec { blocks, block_ids })
        })
        .collect()
}

/// What a chain is built from, as keys: `(everything, stamps)`. Equal
/// `everything` builds the same chain; equal `stamps` is what a retune
/// needs besides equal shapes.
///
/// `everything` is every block field that reaches the build — type, assets,
/// params, trims, names, the chain's block ids — plus `stamps`: each asset
/// file's modification stamp (a capture re-recorded to the same path is a
/// different chain) and the global NAM build settings. A block's `bypassed`
/// is left out on purpose: it only seeds the block's gate, and
/// [`ProfileRig::activate`] sets every gate from the patch anyway — so
/// toggling a bypass in the definition reuses the chain. So is its `id`, a
/// UUID the build never reads.
#[cfg(not(target_arch = "wasm32"))]
fn chain_key(spec: &ChainSpec) -> (String, String) {
    use std::fmt::Write as _;
    fn stamp(path: &str) -> String {
        if path.is_empty() {
            return String::new();
        }
        std::fs::metadata(path)
            .ok()
            .map(|m| {
                let t = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                format!("{}@{t}", m.len())
            })
            .unwrap_or_else(|| "-".into())
    }
    let mut stamps = format!(
        "size={:?};cal={:?};",
        crate::nam::model_size(),
        crate::nam::interface_calibration_dbu()
    );
    let mut key = String::new();
    for (block, id) in spec.blocks.iter().zip(&spec.block_ids) {
        let mut b = block.clone();
        b.bypassed = false;
        // Minted afresh each time a definition is realized, and never read
        // by the build: two realizations of one definition are one chain.
        b.id = String::new();
        let _ = write!(
            stamps,
            "[{}|{}|{}|{}]",
            stamp(&b.nam),
            stamp(&b.ir),
            stamp(&b.plugin),
            stamp(&b.sample)
        );
        let _ = write!(key, "[{id}|{b:?}]");
    }
    (format!("{stamps}{key}"), stamps)
}

#[cfg(not(target_arch = "wasm32"))]
/// A patch's live state, for its new or retuned chain: the old chain's
/// logged writes the edit left alone, then `overlay`, then the writes
/// made after the ticket (`seq > log_seq`) whatever the edit did. Returns
/// the writes to put on the chain, its new log, and the bypasses to land
/// it with.
fn carry_live_state(
    old: Option<&ChainSpec>,
    new: &ChainSpec,
    entries: &[LogEntry],
    overlay: &[(String, LiveWrite)],
    log_seq: u64,
) -> (Vec<(usize, BlockWrite)>, Vec<LogEntry>, Vec<(String, bool)>) {
    let edited = |block: &str, w: &LiveWrite| -> bool {
        let (Some(old), Some(bi)) = (old, new.block_ids.iter().position(|b| b == block)) else {
            return true;
        };
        let Some(oi) = old.block_ids.iter().position(|b| b == block) else {
            return true;
        };
        let (a, b) = (&old.blocks[oi], &new.blocks[bi]);
        match w {
            LiveWrite::Bypass(_) => a.bypassed != b.bypassed,
            LiveWrite::Param(p, _) if p == "input_trim" => a.input_trim_db != b.input_trim_db,
            LiveWrite::Param(p, _) if p == "output_trim" => a.output_trim_db != b.output_trim_db,
            LiveWrite::Param(p, _) => a.param_str(p) != b.param_str(p),
        }
    };
    let mut order: Vec<LogEntry> = entries
        .iter()
        .filter(|e| e.seq <= log_seq && !edited(&e.block, &e.write))
        .cloned()
        .collect();
    order.extend(overlay.iter().map(|(block, write)| LogEntry {
        block: block.clone(),
        write: write.clone(),
        seq: 0,
    }));
    order.extend(entries.iter().filter(|e| e.seq > log_seq).cloned());

    let mut writes: Vec<(usize, BlockWrite)> = Vec::new();
    let mut bypass: Vec<(String, bool)> = Vec::new();
    let mut kept: Vec<LogEntry> = Vec::new();
    for e in order {
        let Some(slot) = new.block_ids.iter().position(|b| *b == e.block) else {
            continue;
        };
        let block = &new.blocks[slot];
        match &e.write {
            LiveWrite::Bypass(on) => {
                bypass.retain(|(b, _)| *b != e.block);
                bypass.push((e.block.clone(), *on));
            }
            LiveWrite::Param(p, v) => {
                let w = if block.is_nam() && p == "input_trim" {
                    BlockWrite::Nam { input_db: Some(*v), output_db: None }
                } else if block.is_nam() && p == "output_trim" {
                    BlockWrite::Nam { input_db: None, output_db: Some(*v) }
                } else if crate::block_params::is_known(block.block_type, p) && block.is_native() {
                    BlockWrite::Params(vec![(p.clone(), f64::from(*v))])
                } else {
                    // A hosted plugin's param lives on its slot, not
                    // its chain; nothing to carry.
                    continue;
                };
                writes.push((slot, w));
            }
        }
        kept.retain(|k| !(k.block == e.block && same_target(&k.write, &e.write)));
        kept.push(e);
    }
    (writes, kept, bypass)
}

#[cfg(not(target_arch = "wasm32"))]
impl ProfileRig {
    pub fn new(rig: GuitarRig) -> Self {
        Self {
            rig,
            profile: None,
            patch_ids: Vec::new(),
            active: None,
            stack_pos: Vec::new(),
            stack_base: Vec::new(),
            no_rotate: Vec::new(),
            chain_keys: HashMap::new(),
            reload_gen: 0,
            live_log: std::sync::Mutex::new(LiveLog::default()),
            fx_bypass: false,
            level_match: true,
            target_loudness_db: -18.0,
            calibrated_input: false,
            // 12.0 dBu ≈ 0 dBFS is a common interface reference; the user should
            // set their measured value (see NAM's calibration tutorial).
            input_calibration_dbu: 12.0,
        }
    }

    /// Open the default audio devices and wrap a fresh rig.
    ///
    /// # Errors
    /// Returns an error if audio initialization fails.
    pub fn open_default() -> eyre::Result<Self> {
        Ok(Self::new(GuitarRig::new()?))
    }

    /// Enable/disable loudness-based level matching across patches. Re-applies
    /// to the active patch immediately.
    pub fn set_level_match(&mut self, on: bool) {
        self.level_match = on;
        if let Some(i) = self.active {
            self.activate(i);
        }
    }

    pub fn is_level_matching(&self) -> bool {
        self.level_match
    }

    /// Target loudness (dB) patches normalize toward when level-matching.
    pub fn set_target_loudness_db(&mut self, db: f32) {
        self.target_loudness_db = db;
        if let Some(i) = self.active {
            self.activate(i);
        }
    }

    /// Enable/disable calibrated input staging (feed each model its captured
    /// dBu input level). Re-applies to the active patch immediately.
    pub fn set_calibrated_input(&mut self, on: bool) {
        self.calibrated_input = on;
        if let Some(i) = self.active {
            self.activate(i);
        }
    }

    pub fn is_calibrated_input(&self) -> bool {
        self.calibrated_input
    }

    /// Set the interface's input calibration: the analog level (dBu) that equals
    /// 0 dBFS at the DI input. Re-applies when calibrated input is on.
    pub fn set_input_calibration_dbu(&mut self, dbu: f32) {
        self.input_calibration_dbu = dbu;
        if self.calibrated_input {
            if let Some(i) = self.active {
                self.activate(i);
            }
        }
    }

    pub fn input_calibration_dbu(&self) -> f32 {
        self.input_calibration_dbu
    }

    /// Load a profile — the first one, or another in place of the playing
    /// one — and land on its default patch. `base_dir` resolves relative
    /// block paths (pass the profile file's directory).
    ///
    /// Gapless even with a profile already playing: this is
    /// [`begin_reload`](Self::begin_reload) in [`ReloadMode::Switch`], planned,
    /// prepared and committed in one go. Nothing is cleared first — the new
    /// chains are built beside the old, the landing patch comes in through
    /// the same crossfade a footswitch uses, and the old patch's tail rings
    /// out under it. A caller that can release its lock while the chains
    /// build should run the three phases itself.
    ///
    /// # Errors
    /// Returns an error if no patches could be built.
    pub fn load_profile(
        &mut self,
        profile: RigProfile,
        base_dir: Option<&Path>,
    ) -> Result<(), String> {
        let prepared = self
            .begin_reload(ReloadMode::Switch)
            .plan(profile, base_dir)
            .prepare();
        let commit = self.commit_reload(prepared, None);
        match commit.status {
            CommitStatus::Committed => Ok(()),
            CommitStatus::NothingBuilt => Err("no patch chains could be built".into()),
            CommitStatus::Stale => Err("profile load superseded".into()),
        }
    }

    /// [`load_profile`](Self::load_profile) for the *same* profile, edited:
    /// only the chains whose spec changed are built, the playing patch,
    /// stack cursors and a song's rotations stay where they were, and a
    /// change to the playing patch's chain crossfades in with its tail
    /// ringing on. All three phases under `&mut self`; see
    /// [`begin_reload`](Self::begin_reload) to build without holding a lock.
    pub fn reload_profile(
        &mut self,
        profile: RigProfile,
        base_dir: Option<&Path>,
    ) -> ReloadCommit {
        let prepared = self
            .begin_reload(ReloadMode::Keep)
            .plan(profile, base_dir)
            .prepare();
        self.commit_reload(prepared, None)
    }

    /// Phase one of a gapless reload: a ticket carrying what the plan diffs
    /// against — every installed chain's spec — a generation, and where the
    /// live-write log stands.
    ///
    /// Cheap (a few string clones and reference counts), so take it under
    /// whatever lock guards this rig, then release the lock for
    /// [`ReloadTicket::plan`] and [`ReloadPlan::prepare`], which never touch
    /// the rig. Taking a ticket supersedes every earlier one: only the newest
    /// commits (see [`commit_reload`](Self::commit_reload)).
    pub fn begin_reload(&mut self, mode: ReloadMode) -> ReloadTicket {
        self.reload_gen = next_reload_gen();
        ReloadTicket {
            generation: self.reload_gen,
            sample_rate: self.rig.sample_rate,
            mode,
            installed: self
                .chain_keys
                .iter()
                .map(|(&id, k)| Installed {
                    id,
                    patch: k.patch.clone(),
                    key: k.key.clone(),
                    stamps: k.stamps.clone(),
                    spec: k.spec.clone(),
                })
                .collect(),
            log_seq: self.log().seq,
        }
    }

    fn log(&self) -> std::sync::MutexGuard<'_, LiveLog> {
        self.live_log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Phase three: write the retunes, install what was built, keep what
    /// was reused, switch the playing patch over gaplessly and retire what
    /// nothing references.
    ///
    /// Fast — param writes, moves, map inserts and at most one switch (the
    /// same one a footswitch makes). Under `&mut self`, but nothing here
    /// waits on a build.
    ///
    /// - A reload whose ticket is not the newest (another reload, or a
    ///   [`load_profile`](Self::load_profile), began since) is **stale**: it
    ///   changes nothing, and its chains come back in the result to be
    ///   dropped.
    /// - A **retuned** chain gets the new settings written to its running
    ///   blocks (see [`crate::block_params`]): no new chain, no switch, no
    ///   crossfade. Written as a param event exactly as its build would have
    ///   set it.
    /// - A patch's **live state** — every value written to its old chain
    ///   since it was built (knob, macro, boost, tempo; see
    ///   [`set_block_param`](Self::set_block_param)) — goes onto its new or
    ///   retuned chain *before it plays*: first what the edit did not change
    ///   (the edit wins over an older live value), then the overlay
    ///   ([`PreparedReload::set_overlay`]), then every write made after the
    ///   ticket, edit or no (a knob turned while the chains built is the
    ///   newest word).
    /// - Patches are matched **by name**: the patch that was playing plays
    ///   on ([`ReloadMode::Keep`]) unless `activate` names another, and each
    ///   stack keeps its cursor on the patch it pointed at, its song rotation
    ///   and its no-rotate flag.
    /// - The playing patch whose chain was rebuilt switches through
    ///   [`GuitarRig::set_active`]: its old chain becomes a tail (only the
    ///   dry path crossfades) and the new one fades in. One kept or retuned
    ///   does not switch at all.
    /// - A chain no longer referenced is retired only after that switch, so
    ///   it is never the one playing; one still ringing stays owned by its
    ///   tail until the tail is done (see [`GuitarRig::retire_chain`]).
    ///
    /// Drop the result after releasing any lock: it holds the retired
    /// chains, and freeing them is not free.
    pub fn commit_reload(
        &mut self,
        prepared: PreparedReload,
        activate: Option<&str>,
    ) -> ReloadCommit {
        let began = std::time::Instant::now();
        let PreparedReload {
            generation,
            sample_rate,
            mode,
            log_seq,
            profile,
            keys,
            specs,
            fate,
            built,
            overlay,
            build_ms,
        } = prepared;
        let mut garbage = Garbage::default();
        let mut report = ReloadCommit {
            status: CommitStatus::Stale,
            built: 0,
            reused: 0,
            retuned: 0,
            retired: 0,
            failed: 0,
            carried: 0,
            switched: false,
            build_ms,
            commit_us: 0,
            garbage: Garbage::default(),
        };
        let installed_ok = fate.iter().all(|f| match f {
            Fate::Keep(id) | Fate::Retune(id, _) => self.chain_keys.contains_key(id),
            _ => true,
        });
        if generation != self.reload_gen || sample_rate != self.rig.sample_rate || !installed_ok {
            garbage
                .prepared
                .extend(built.into_iter().flatten().filter_map(Result::ok));
            report.garbage = garbage;
            report.commit_us = began.elapsed().as_micros() as u64;
            return report;
        }

        // What the switcher was doing, by name — read now, not at plan time:
        // a footswitch pressed while the chains built counts.
        let was_active = self.active_patch().map(|p| p.name.clone());
        let was_live = self.rig.live();
        let carried: Vec<CarriedStack> = match (mode, self.profile.as_ref()) {
            (ReloadMode::Keep, Some(old)) => old
                .stacks
                .iter()
                .enumerate()
                .map(|(si, st)| {
                    let pos = self.stack_position(si);
                    CarriedStack {
                        name: st.name.clone(),
                        cursor_patch: st.patches.get(pos).cloned(),
                        pos,
                        live: st.patches.clone(),
                        song: self.stack_base.get(si).is_some_and(Option::is_some),
                        no_rotate: self.no_rotate.get(si).copied().unwrap_or(false),
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        // Each patch's chain before this reload, by name — whose live state
        // a new or retuned chain for that patch takes on.
        let old_chain: HashMap<String, ModelId> = self
            .profile
            .as_ref()
            .map(|p| {
                p.patches
                    .iter()
                    .zip(&self.patch_ids)
                    .filter(|(_, id)| **id != MODEL_UNAVAILABLE)
                    .map(|(p, id)| (p.name.to_ascii_lowercase(), *id))
                    .collect()
            })
            .unwrap_or_default();

        // Install in patch order, keeping and retuning what the plan
        // matched; each new or retuned chain gets its patch's live state.
        let mut patch_ids = Vec::with_capacity(profile.patches.len());
        let mut chain_keys = HashMap::with_capacity(profile.patches.len());
        let mut new_log: HashMap<ModelId, Vec<LogEntry>> = HashMap::new();
        let mut landing_bypass: HashMap<String, Vec<(String, bool)>> = HashMap::new();
        let mut first_ok = None;
        let old_log = std::mem::take(&mut self.log().chains);
        for (i, ((patch, (fate, outcome)), (key, spec))) in profile
            .patches
            .iter()
            .zip(fate.into_iter().zip(built))
            .zip(keys.into_iter().zip(specs))
            .enumerate()
        {
            let name = patch.name.to_ascii_lowercase();
            let mut retune: Vec<(usize, BlockWrite)> = Vec::new();
            let fresh = matches!(fate, Fate::Build);
            let (id, changed) = match (fate, outcome) {
                (Fate::Keep(id), _) => {
                    report.reused += 1;
                    (id, false)
                }
                (Fate::Retune(id, writes), _) => {
                    report.retuned += 1;
                    retune = writes;
                    (id, true)
                }
                (Fate::Build, Some(Ok(chain))) => {
                    report.built += 1;
                    (self.rig.install_prepared(chain), true)
                }
                (Fate::Build, Some(Err(e))) => {
                    report.failed += 1;
                    tracing::warn!(
                        patch = %patch.name,
                        error = %e,
                        "ProfileRig: failed to build patch chain — skipping"
                    );
                    (MODEL_UNAVAILABLE, false)
                }
                _ => (MODEL_UNAVAILABLE, false),
            };
            if id != MODEL_UNAVAILABLE {
                first_ok.get_or_insert(i);
                if let (Some((key, stamps)), Some(spec)) = (key, spec) {
                    if changed {
                        // Another profile's patch of the same name is not
                        // this one: its live state stays behind.
                        let before = old_chain
                            .get(&name)
                            .copied()
                            .filter(|_| mode == ReloadMode::Keep);
                        let old_spec = before.and_then(|b| self.chain_keys.get(&b)).map(|k| k.spec.clone());
                        let entries = before.and_then(|b| old_log.get(&b)).cloned().unwrap_or_default();
                        let (writes, kept, bypass) = carry_live_state(
                            old_spec.as_deref(),
                            &spec,
                            &entries,
                            overlay.get(&name).map(Vec::as_slice).unwrap_or_default(),
                            log_seq,
                        );
                        report.carried += writes.len() + bypass.len();
                        // The settings and the live state in one write — a
                        // retuned chain may be playing, and no block of it
                        // may render between the two.
                        if fresh {
                            // A new chain: it starts at its live state, as
                            // if built with it.
                            if !writes.is_empty() {
                                self.rig.write_new_chain_blocks(id, &writes);
                            }
                        } else {
                            retune.extend(writes);
                            if !retune.is_empty() {
                                self.rig.write_chain_blocks(id, &retune);
                            }
                        }
                        new_log.insert(id, kept);
                        landing_bypass.insert(name.clone(), bypass);
                    } else if let Some(entries) = old_log.get(&id) {
                        new_log.insert(id, entries.clone());
                    }
                    chain_keys.insert(
                        id,
                        ChainKey {
                            patch: patch.name.clone(),
                            key,
                            stamps,
                            spec,
                        },
                    );
                }
            }
            patch_ids.push(id);
        }
        self.log().chains = new_log;

        // Stacks: fresh for another profile; for the same one, each stack
        // found by name picks up where it was.
        let n = profile.stacks.len();
        let mut stack_pos = vec![0; n];
        let mut stack_base = vec![None; n];
        let mut no_rotate = vec![false; n];
        let mut profile = profile;
        for (si, st) in profile.stacks.iter_mut().enumerate() {
            let Some(old) = carried.iter().find(|c| c.name.eq_ignore_ascii_case(&st.name)) else {
                continue;
            };
            if old.song {
                // The song's rotation stays up; the profile's (new) own one
                // waits under it.
                stack_base[si] = Some(std::mem::replace(&mut st.patches, old.live.clone()));
            }
            stack_pos[si] = old
                .cursor_patch
                .as_ref()
                .and_then(|name| st.patches.iter().position(|p| p.eq_ignore_ascii_case(name)))
                .unwrap_or(if old.pos < st.patches.len() { old.pos } else { 0 });
            no_rotate[si] = old.no_rotate;
        }

        let default_patch = profile.default_patch;
        let available = |ids: &[ModelId], i: usize| {
            ids.get(i).copied().unwrap_or(MODEL_UNAVAILABLE) != MODEL_UNAVAILABLE
        };
        let by_name = |name: &str| profile.patch_index(name);
        let target = activate
            .and_then(by_name)
            .filter(|&i| available(&patch_ids, i))
            .or_else(|| {
                (mode == ReloadMode::Keep)
                    .then_some(was_active.as_deref())
                    .flatten()
                    .and_then(by_name)
                    .filter(|&i| available(&patch_ids, i))
            })
            .or_else(|| available(&patch_ids, default_patch).then_some(default_patch))
            .or(first_ok);
        let target_bypass = target
            .and_then(|i| profile.patches.get(i))
            .and_then(|p| landing_bypass.remove(&p.name.to_ascii_lowercase()))
            .unwrap_or_default();

        self.profile = Some(profile);
        self.patch_ids = patch_ids;
        self.chain_keys = chain_keys;
        self.stack_pos = stack_pos;
        self.stack_base = stack_base;
        self.no_rotate = no_rotate;
        self.active = None;

        // Land — the footswitch's own path: a rebuilt chain crossfades in
        // (already carrying its live state and bypasses) and the old one
        // rings out as a tail; one kept or retuned does not switch.
        let landed = target.is_some_and(|i| self.activate_with(i, &target_bypass));
        if !landed {
            // Nothing to play: out through the same switch, so what was
            // playing still rings out rather than stopping.
            self.rig.set_active(None);
        }
        report.switched = self.rig.live() != was_live;

        // Retire every chain the new set does not reference — after the
        // switch, so none of them is the one playing.
        let keep: std::collections::HashSet<ModelId> = self.patch_ids.iter().copied().collect();
        let stale: Vec<ModelId> = self
            .rig
            .slots()
            .iter()
            .map(|s| s.id)
            .filter(|id| !keep.contains(id))
            .collect();
        for id in stale {
            match self.rig.retire_chain(id) {
                Some(r) => {
                    garbage.retired.push(r);
                    report.retired += 1;
                }
                None => tracing::warn!(chain = id, "ProfileRig: a retired chain is still playing — kept"),
            }
        }

        report.status = if landed || self.patch_ids.is_empty() {
            CommitStatus::Committed
        } else {
            CommitStatus::NothingBuilt
        };
        report.garbage = garbage;
        report.commit_us = began.elapsed().as_micros() as u64;
        tracing::info!(
            reload.mode = ?mode,
            reload.built = report.built,
            reload.reused = report.reused,
            reload.retuned = report.retuned,
            reload.retired = report.retired,
            reload.failed = report.failed,
            reload.carried = report.carried,
            reload.switched = report.switched,
            reload.build_ms = report.build_ms,
            reload.commit_us = report.commit_us,
            "ProfileRig: reload committed"
        );
        report
    }

    /// Convenience: load a profile from a `.styx` file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or loaded.
    pub fn load_profile_file(&mut self, path: &Path) -> Result<(), String> {
        let profile = RigProfile::from_styx_file(path).map_err(|e| e.to_string())?;
        let base = path.parent();
        self.load_profile(profile, base)
    }

    /// Activate the patch at `index`. Returns false if out of range or its
    /// chain failed to build. Applies patch trims + (optional) level-match.
    pub fn activate(&mut self, index: usize) -> bool {
        self.activate_with(index, &[])
    }

    /// [`activate`](Self::activate), with `bypass` — `(block id, on)` — in
    /// place of the patch's own for those blocks: a reload landing on a new
    /// chain brings the bypasses the old one had been given live (a macro's,
    /// a stomp's) in with it rather than after it.
    fn activate_with(&mut self, index: usize, bypass: &[(String, bool)]) -> bool {
        let Some(profile) = &self.profile else {
            return false;
        };
        let Some(patch) = profile.patches.get(index) else {
            return false;
        };
        let Some(&id) = self.patch_ids.get(index) else {
            return false;
        };
        if id == MODEL_UNAVAILABLE {
            return false;
        }

        let slot = self.rig.slot_info(id);
        let mut output_trim = patch.output_trim_db;
        if self.level_match {
            if let Some(loud) = slot.as_ref().and_then(|s| s.primary_loudness) {
                // Measured LUFS → makeup toward the common target so every amp,
                // clean or high-gain, lands at the same average volume.
                output_trim += self.target_loudness_db - loud as f32;
            }
        }
        let mut input_trim = patch.input_trim_db;
        if self.calibrated_input {
            // Pre-model gain so the model sees the analog level it was captured
            // at (authentic drive); orthogonal to the output level-match.
            let model_in = slot.as_ref().and_then(|s| s.primary_input_level_dbu);
            input_trim += crate::nam_calibrate::input_calibration_db(
                model_in,
                self.input_calibration_dbu as f64,
            );
        }
        self.rig.set_input_trim_db(input_trim);
        // The patch's level rides the switch (output stage, crossfaded, and
        // kept by the outgoing patch's tail); the fader is the master's.
        self.rig.set_patch_trim_db(output_trim);
        // The chain arrives with its bypass already set — the patch's own,
        // plus the global time bypass — rather than being corrected after it
        // starts playing.
        let mut mask: Vec<bool> = patch
            .chain
            .iter()
            .filter(|b| b.has_backend())
            .map(|b| b.bypassed || (self.fx_bypass && b.is_time_module()))
            .collect();
        let held: Vec<usize> = if bypass.is_empty() {
            Vec::new()
        } else {
            let ids = self.rig.chain_block_ids(id);
            bypass
                .iter()
                .filter_map(|(b, on)| {
                    let at = ids.iter().position(|i| i == b)?;
                    *mask.get_mut(at)? = *on;
                    Some(at)
                })
                .collect()
        };
        self.rig.set_chain_bypass(id, &mask);
        self.rig.set_active(Some(id));
        self.active = Some(index);
        self.apply_fx_bypass(index, &held);
        true
    }

    /// Push the current global time-bypass state onto the active patch: bypass
    /// every installed time/fx block (the Time module) of patch `index`. The
    /// installed chain skips placeholder blocks, so we zip the live block ids
    /// against the patch's installed (has-backend) blocks to find which are time/fx.
    fn apply_fx_bypass(&self, index: usize, held: &[usize]) {
        let Some(profile) = &self.profile else {
            return;
        };
        let Some(patch) = profile.patches.get(index) else {
            return;
        };
        let live_ids = self.rig.active_block_ids();
        let real = patch.chain.iter().filter(|b| b.has_backend());
        for (at, (block, id)) in real.zip(live_ids.iter()).enumerate() {
            if block.is_time_module() && !held.contains(&at) {
                // Releasing the global bypass must not resurrect blocks the
                // patch keeps bypassed by configuration (e.g. an "extreme"
                // DLY 2 / VERB 2 pair) — OR with the block's own state.
                self.rig
                    .set_block_slot_bypass(id, self.fx_bypass || block.bypassed);
            }
        }
    }

    // ── Global time / FX bypass ──────────────────────────────────────────

    /// Set the global time-bypass (kills the Time module — delay/reverb/mod —
    /// across the active patch). Re-applies immediately.
    pub fn set_fx_bypass(&mut self, on: bool) {
        self.fx_bypass = on;
        if let Some(i) = self.active {
            self.apply_fx_bypass(i, &[]);
        }
    }

    /// Toggle the global time-bypass; returns the new state.
    pub fn toggle_fx_bypass(&mut self) -> bool {
        self.set_fx_bypass(!self.fx_bypass);
        self.fx_bypass
    }

    pub fn fx_bypass(&self) -> bool {
        self.fx_bypass
    }

    // ── Footswitch stacks ────────────────────────────────────────────────

    /// The active profile's stacks (footswitch groups), or empty.
    pub fn stacks(&self) -> &[RigStack] {
        self.profile.as_ref().map_or(&[], |p| p.stacks.as_slice())
    }

    /// The rotation cursor (index into the stack's patch list) for `stack_idx`.
    /// Reset every stack's rotation cursor to its first patch.
    /// Replace stack `stack`'s rotation with `patches` — a song's switch
    /// tuning. The profile's own rotation is kept, and comes back with
    /// [`restore_stack_rotations`](Self::restore_stack_rotations).
    pub fn set_stack_rotation(&mut self, stack: &str, patches: Vec<String>) -> bool {
        let Some(profile) = self.profile.as_mut() else {
            return false;
        };
        let Some((si, st)) = profile
            .stacks
            .iter_mut()
            .enumerate()
            .find(|(_, st)| st.name.eq_ignore_ascii_case(stack))
        else {
            return false;
        };
        let original = std::mem::replace(&mut st.patches, patches);
        if let Some(base) = self.stack_base.get_mut(si) {
            base.get_or_insert(original);
        }
        if let Some(pos) = self.stack_pos.get_mut(si) {
            *pos = 0;
        }
        true
    }

    /// Put every stack's own rotation back (the song has gone).
    pub fn restore_stack_rotations(&mut self) {
        let Some(profile) = self.profile.as_mut() else {
            return;
        };
        for (st, base) in profile.stacks.iter_mut().zip(self.stack_base.iter_mut()) {
            if let Some(original) = base.take() {
                st.patches = original;
            }
        }
    }

    /// Put each stack's rotation to what `tuned` gives it — a song's
    /// `(stack, patches)` — or back to the profile's own for the stacks it
    /// does not name: [`restore_stack_rotations`](Self::restore_stack_rotations)
    /// then [`set_stack_rotation`](Self::set_stack_rotation) for each, except
    /// that a stack whose rotation comes out the same as it was keeps its
    /// cursor. Re-applying the same tuning (after a reload, say) moves no
    /// switch; a new rotation starts at its first patch as before.
    pub fn retune_stacks(&mut self, tuned: &[(String, Vec<String>)]) {
        let Some(profile) = self.profile.as_mut() else {
            return;
        };
        for (si, st) in profile.stacks.iter_mut().enumerate() {
            let base = self.stack_base.get_mut(si).and_then(Option::take);
            let own = base.clone().unwrap_or_else(|| st.patches.clone());
            let want = tuned
                .iter()
                .rev()
                .find(|(name, p)| !p.is_empty() && name.eq_ignore_ascii_case(&st.name))
                .map(|(_, p)| p.clone());
            match want {
                Some(song) => {
                    if song != st.patches {
                        st.patches = song;
                        if let Some(pos) = self.stack_pos.get_mut(si) {
                            *pos = 0;
                        }
                    }
                    if let Some(slot) = self.stack_base.get_mut(si) {
                        *slot = Some(own);
                    }
                }
                None => {
                    if let Some(original) = base {
                        st.patches = original;
                    }
                }
            }
        }
    }

    /// Whether each stack's switch rotates (`false`) or always lands on its
    /// patch (`true`), in stack order.
    pub fn set_no_rotate(&mut self, flags: &[bool]) {
        for (slot, &f) in self.no_rotate.iter_mut().zip(flags) {
            *slot = f;
        }
    }

    pub fn reset_stack_positions(&mut self) {
        for p in &mut self.stack_pos {
            *p = 0;
        }
    }

    /// Point a stack's rotation cursor at a named patch WITHOUT activating —
    /// the next press (or activation) of that stack lands there. Song-level
    /// switch tuning. No-op when the stack or patch isn't found.
    pub fn point_stack_at(&mut self, stack: &str, patch: &str) -> bool {
        let Some(profile) = self.profile.as_ref() else {
            return false;
        };
        let Some((si, st)) = profile
            .stacks
            .iter()
            .enumerate()
            .find(|(_, st)| st.name.eq_ignore_ascii_case(stack))
        else {
            return false;
        };
        let Some(pos) = st
            .patches
            .iter()
            .position(|p| p.eq_ignore_ascii_case(patch))
        else {
            return false;
        };
        if let Some(slot) = self.stack_pos.get_mut(si) {
            *slot = pos;
            true
        } else {
            false
        }
    }

    pub fn stack_position(&self, stack_idx: usize) -> usize {
        self.stack_pos.get(stack_idx).copied().unwrap_or(0)
    }

    /// The stack the currently-active patch belongs to at its current cursor, if
    /// any — for highlighting the active footswitch.
    pub fn active_stack(&self) -> Option<usize> {
        let profile = self.profile.as_ref()?;
        let active = self.active?;
        for (si, stack) in profile.stacks.iter().enumerate() {
            let pos = self.stack_position(si);
            if let Some(name) = stack.patches.get(pos) {
                if profile.patch_index(name) == Some(active) {
                    return Some(si);
                }
            }
        }
        None
    }

    /// Press stack `stack_idx` (footswitch). FM9-style rotation: if the stack's
    /// current patch is **not** already active, activate it; if it **is** active,
    /// rotate to the next patch in the stack (wrapping) and activate that.
    /// Returns true if a patch was activated.
    pub fn activate_stack(&mut self, stack_idx: usize) -> bool {
        // Pull what we need out from under the immutable profile borrow.
        let (patches, cur_pos, cur_active_idx) = {
            let Some(profile) = self.profile.as_ref() else {
                return false;
            };
            let Some(stack) = profile.stacks.get(stack_idx) else {
                return false;
            };
            if stack.patches.is_empty() {
                return false;
            }
            let pos = self.stack_position(stack_idx) % stack.patches.len();
            let cur_idx = profile.patch_index(&stack.patches[pos]);
            (stack.patches.clone(), pos, cur_idx)
        };

        let already_active = cur_active_idx.is_some() && self.active == cur_active_idx;
        let rotates = !self.no_rotate.get(stack_idx).copied().unwrap_or(false);
        let target_pos = if already_active && rotates {
            (cur_pos + 1) % patches.len()
        } else {
            cur_pos
        };
        if let Some(slot) = self.stack_pos.get_mut(stack_idx) {
            *slot = target_pos;
        }
        self.activate_named(&patches[target_pos])
    }

    /// Jump stack `stack_idx`'s rotation cursor straight to `pos` and
    /// activate that patch — the "pick a patch from a browser" path, keeping
    /// the footswitch state consistent with what's audible.
    pub fn activate_stack_at(&mut self, stack_idx: usize, pos: usize) -> bool {
        let patch_name = {
            let Some(profile) = self.profile.as_ref() else {
                return false;
            };
            let Some(stack) = profile.stacks.get(stack_idx) else {
                return false;
            };
            let Some(name) = stack.patches.get(pos) else {
                return false;
            };
            name.clone()
        };
        if let Some(slot) = self.stack_pos.get_mut(stack_idx) {
            *slot = pos;
        }
        self.activate_named(&patch_name)
    }

    /// Activate a patch by name (case-insensitive).
    pub fn activate_named(&mut self, name: &str) -> bool {
        let Some(idx) = self.profile.as_ref().and_then(|p| {
            p.patches
                .iter()
                .position(|q| q.name.eq_ignore_ascii_case(name))
        }) else {
            return false;
        };
        self.activate(idx)
    }

    /// Step to the next loadable patch (wraps). Footswitch-style.
    pub fn next_patch(&mut self) -> bool {
        self.step(1)
    }

    pub fn prev_patch(&mut self) -> bool {
        self.step(-1)
    }

    fn step(&mut self, dir: i32) -> bool {
        let n = self.patch_ids.len();
        if n == 0 {
            return false;
        }
        let start = self.active.unwrap_or(0) as i32;
        for k in 1..=n as i32 {
            let idx = (start + dir * k).rem_euclid(n as i32) as usize;
            if self.patch_ids[idx] != MODEL_UNAVAILABLE && self.activate(idx) {
                return true;
            }
        }
        false
    }

    // ── Read-side accessors ──────────────────────────────────────────────

    pub fn profile_name(&self) -> Option<&str> {
        self.profile.as_ref().map(|p| p.name.as_str())
    }

    pub fn patches(&self) -> &[RigPatch] {
        self.profile.as_ref().map_or(&[], |p| p.patches.as_slice())
    }

    pub fn is_patch_available(&self, index: usize) -> bool {
        self.patch_ids
            .get(index)
            .copied()
            .is_some_and(|id| id != MODEL_UNAVAILABLE)
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active_patch(&self) -> Option<&RigPatch> {
        let i = self.active?;
        self.profile.as_ref()?.patches.get(i)
    }

    pub fn rig(&self) -> &GuitarRig {
        &self.rig
    }

    pub fn rig_mut(&mut self) -> &mut GuitarRig {
        &mut self.rig
    }

    // ── Live block addressing (delegates to the underlying GuitarRig) ────────

    /// Block ids of the active patch's chain, in order. See
    /// [`GuitarRig::active_block_ids`].
    pub fn active_block_ids(&self) -> Vec<String> {
        self.rig.active_block_ids()
    }

    /// Run `f` against the live instance backing the active patch's block
    /// `block_id`. See [`GuitarRig::with_active_block_instance`].
    pub fn with_active_block_instance<R>(
        &self,
        block_id: &str,
        f: impl FnOnce(&mut dyn signal_plugin_host::PluginInstance) -> R,
    ) -> Option<R> {
        self.rig.with_active_block_instance(block_id, f)
    }

    /// Per-block bypass on the active patch's chain. See
    /// [`GuitarRig::set_block_slot_bypass`].
    ///
    /// Kept as the chain's live state: a reload that gives this patch a new
    /// chain, or retunes this one, puts it back on before the chain plays.
    pub fn set_block_bypass(&self, block_id: &str, on: bool) -> bool {
        let ok = self.rig.set_block_slot_bypass(block_id, on);
        if ok {
            if let Some(chain) = self.rig.active() {
                self.log().record(chain, block_id, LiveWrite::Bypass(on));
            }
        }
        ok
    }

    /// Update the *configured* bypass of the active patch's block addressed by
    /// live slot id — so the global FX-bypass cycle (`apply_fx_bypass`) and
    /// re-activation restore the user's runtime toggles, not the profile's
    /// build-time defaults. Engine state is untouched; pair with
    /// [`set_block_bypass`](Self::set_block_bypass).
    pub fn set_block_config_bypass(&mut self, block_id: &str, on: bool) {
        let Some(active) = self.active else { return };
        let Some(pos) = self
            .rig
            .active_block_ids()
            .iter()
            .position(|i| i == block_id)
        else {
            return;
        };
        if let Some(profile) = &mut self.profile {
            if let Some(patch) = profile.patches.get_mut(active) {
                if let Some(block) = patch.chain.iter_mut().filter(|b| b.has_backend()).nth(pos) {
                    block.bypassed = on;
                }
            }
        }
    }

    /// Set a named param on the active patch's block `block_id`. See
    /// [`GuitarRig::set_active_block_param`].
    ///
    /// Every live write should come through here — a knob, a macro, the
    /// boost, a delay's tempo, a NAM block's `input_trim` / `output_trim`
    /// (a drive knob's compensation): each is kept as the chain's live state,
    /// and a reload that gives this patch a new chain, or retunes this one,
    /// puts it back on before the chain plays (see
    /// [`commit_reload`](Self::commit_reload)).
    pub fn set_block_param(&self, block_id: &str, param_name: &str, value: f32) -> bool {
        let ok = self.rig.set_active_block_param(block_id, param_name, value);
        if ok {
            if let Some(chain) = self.rig.active() {
                self.log().record(
                    chain,
                    block_id,
                    LiveWrite::Param(param_name.to_string(), value),
                );
            }
        }
        ok
    }

    /// Mono input samples for pitch detection. See [`GuitarRig::input_samples`].
    pub fn input_samples(&self) -> Vec<f32> {
        self.rig.input_samples()
    }

    /// The rig's running sample rate (Hz).
    pub fn sample_rate(&self) -> u32 {
        self.rig.sample_rate
    }
}

/// Sentinel stored in `patch_ids` for a patch whose chain failed to build.
#[cfg(not(target_arch = "wasm32"))]
const MODEL_UNAVAILABLE: ModelId = ModelId::MAX;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_worship_profile_programmatically() {
        let profile = RigProfile::new("Worship")
            .with_patch(RigPatch::amp("Clean", "clean.nam"))
            .with_patch(
                RigPatch::new("Lead")
                    .with_block(RigBlock::nam("lead.nam"))
                    .with_block(RigBlock::cab_ir("v30.wav"))
                    .with_trims(3.0, -2.0),
            );
        assert_eq!(profile.patches.len(), 2);
        assert_eq!(profile.patches[0].chain.len(), 1);
        assert_eq!(profile.patches[1].chain.len(), 2);
        assert!(profile.patches[1].chain[1].is_cab_ir());
        assert_eq!(profile.patches[1].input_trim_db, 3.0);
    }

    #[test]
    fn parses_chain_profile_from_styx() {
        let text = r#"
            name Worship
            default_patch 0
            patches (
                {
                    name Clean
                    chain ( { block_type @Amp, nam amps/clean.nam } )
                }
                {
                    name Lead
                    chain (
                        { block_type @Drive, nam amps/drive.nam }
                        { block_type @Amp, nam amps/lead.nam }
                        { block_type @Cabinet, ir cabs/v30.wav }
                    )
                    input_trim_db 3.0
                    output_trim_db -2.0
                }
            )
        "#;
        let profile = RigProfile::from_styx_str(text).expect("parse");
        assert_eq!(profile.patches.len(), 2);
        assert_eq!(profile.patches[1].chain.len(), 3);
        assert!(profile.patches[1].chain[2].is_cab_ir());
        assert_eq!(profile.patches[1].chain[0].nam, "amps/drive.nam");
        assert_eq!(profile.patches[1].output_trim_db, -2.0);
    }

    #[test]
    fn parses_plugin_block_from_styx() {
        let text = r#"
            name Rig
            patches (
                {
                    name Lead
                    chain (
                        { block_type @Amp, nam amps/amp.nam }
                        { block_type @Cabinet, ir cabs/v30.wav }
                        { block_type @Delay, plugin /usr/lib/clap/ValhallaDelay.clap }
                    )
                }
            )
        "#;
        let profile = RigProfile::from_styx_str(text).expect("parse");
        let chain = &profile.patches[0].chain;
        assert_eq!(chain.len(), 3);
        assert!(chain[2].is_plugin());
        assert_eq!(chain[2].plugin, "/usr/lib/clap/ValhallaDelay.clap");
        assert!(chain[2].state_b64.is_none());
    }

    #[test]
    fn from_proto_maps_hosted_plugin_blocks() {
        use signal_proto::profile::{Patch, PatchId, Profile, ProfileId};
        use signal_proto::rig::{RigId, RigSceneId};

        struct PluginResolver;
        impl PatchResolver for PluginResolver {
            fn resolve_patch(&self, _p: &Patch) -> Result<Vec<ResolvedRigBlock>, String> {
                Ok(vec![
                    ResolvedRigBlock::nam("amp.nam"),
                    ResolvedRigBlock::plugin(
                        "Clap",
                        "/plugins/Reverb.clap",
                        Some("c3RhdGU=".into()),
                    ),
                ])
            }
        }

        let patch = Patch::from_rig_scene(PatchId::new(), "Lead", RigId::new(), RigSceneId::new());
        let profile = Profile::new(ProfileId::new(), "Rig", patch);
        let rig_profile = RigProfile::from_proto(&profile, &PluginResolver).expect("convert");
        let chain = &rig_profile.patches[0].chain;
        assert_eq!(chain.len(), 2);
        assert!(chain[0].is_nam());
        assert!(chain[1].is_plugin());
        assert_eq!(chain[1].plugin, "/plugins/Reverb.clap");
        assert_eq!(chain[1].state_b64.as_deref(), Some("c3RhdGU="));
    }

    #[test]
    fn shipped_worship_example_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/worship.styx");
        let profile = RigProfile::from_styx_file(&path).expect("worship.styx should parse");
        assert_eq!(profile.name, "Worship");
        assert!(profile.patches.len() >= 2);
        assert_eq!(profile.patches[0].name, "Clean");
    }

    #[test]
    fn resolves_relative_paths_against_base_dir() {
        let base = Path::new("/profiles/worship");
        assert_eq!(
            resolve_path("amps/clean.nam", Some(base)),
            Path::new("/profiles/worship/amps/clean.nam")
        );
        assert_eq!(
            resolve_path("/models/clean.nam", Some(base)),
            Path::new("/models/clean.nam")
        );
    }

    // ── proto bridge ────────────────────────────────────────────────────

    /// A trivial in-memory resolver: maps each patch to a fixed chain by name.
    struct StubResolver;
    impl PatchResolver for StubResolver {
        fn resolve_patch(
            &self,
            patch: &signal_proto::profile::Patch,
        ) -> Result<Vec<ResolvedRigBlock>, String> {
            // Pretend every patch is amp + cab; name the model after the patch.
            Ok(vec![
                ResolvedRigBlock::nam(format!("{}.nam", patch.name.to_lowercase())),
                ResolvedRigBlock::cab_ir("v30.wav"),
            ])
        }
    }

    #[test]
    fn from_proto_maps_patches_to_chains() {
        use signal_proto::profile::PatchId;
        use signal_proto::profile::{Patch, Profile};
        use signal_proto::rig::{RigId, RigSceneId};

        let clean = Patch::from_rig_scene(PatchId::new(), "Clean", RigId::new(), RigSceneId::new());
        let lead_id = PatchId::new();
        let mut profile = Profile::new(signal_proto::profile::ProfileId::new(), "Worship", clean);
        profile.add_patch(Patch::from_rig_scene(
            lead_id.clone(),
            "Lead",
            RigId::new(),
            RigSceneId::new(),
        ));

        let rig_profile = RigProfile::from_proto(&profile, &StubResolver).expect("convert");
        assert_eq!(rig_profile.name, "Worship");
        assert_eq!(rig_profile.patches.len(), 2);
        // Default patch ("Clean") is index 0.
        assert_eq!(rig_profile.default_patch, 0);
        // Each patch resolved to amp(NAM) + cab(IR).
        let lead = &rig_profile.patches[1];
        assert_eq!(lead.name, "Lead");
        assert_eq!(lead.chain.len(), 2);
        assert!(lead.chain[0].is_nam());
        assert_eq!(lead.chain[0].nam, "lead.nam");
        assert!(lead.chain[1].is_cab_ir());
    }
}
