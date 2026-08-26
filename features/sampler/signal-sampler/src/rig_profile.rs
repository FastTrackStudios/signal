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

use facet::Facet;

use crate::rig::RigBlock;
#[cfg(not(target_arch = "wasm32"))]
use crate::rig::{GuitarRig, ModelId};
use crate::SamplerError;

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

/// A named collection of patches — the standalone-rig analogue of a
/// `signal_proto::Profile`. Patches can be activated directly (flat, by index)
/// or grouped into [`RigStack`]s (footswitch rotation); both views share the
/// same `patches` pool.
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
    pub fn patch_index(&self, name: &str) -> Option<usize> {
        self.patches
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Parse a profile from a `.styx` file (see `examples/worship.styx`).
    pub fn from_styx_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_styx_str(&text)
    }

    pub fn from_styx_str(text: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Convert a `signal_proto::Profile` into a `RigProfile`, using `resolver`
    /// to turn each patch into an ordered chain of realized blocks. NAM blocks
    /// map to [`RigBlock::nam`]; cabinet blocks with an IR path map to
    /// [`RigBlock::cab_ir`]. Non-NAM/non-cab blocks (hosted plugins, native DSP
    /// without a path) are skipped with a log — the standalone rig can't host
    /// them yet. A patch that resolves to an empty chain is kept (it will be
    /// "unavailable" at load time).
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
                    BlockKind::Nam(nam) => chain.push(RigBlock::nam(nam.model_path)),
                    BlockKind::HostedPlugin(href) => {
                        chain.push(RigBlock::plugin_with_state(href.path, href.state_b64))
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

/// Resolves a proto `Patch` into an ordered list of realized blocks. The
/// `signal-live` resolve stack (repos + `ResolveService`) implements this; the
/// standalone rig only needs this narrow surface, so it stays decoupled from
/// storage. Each [`ResolvedRigBlock`] is one block in chain order.
pub trait PatchResolver {
    fn resolve_patch(
        &self,
        patch: &signal_proto::profile::Patch,
    ) -> Result<Vec<ResolvedRigBlock>, String>;
}

/// One realized block from a [`PatchResolver`]: its `BlockKind` (Nam / Native /
/// HostedPlugin / …) plus, for a natively-realized cabinet, its IR path.
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
            kind: signal_proto::block_kind::BlockKind::Nam(signal_proto::block_kind::NamRef {
                model_path: model_path.into(),
                model_id: None,
            }),
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
            kind: signal_proto::block_kind::BlockKind::HostedPlugin(
                signal_proto::block_kind::HostedPluginRef {
                    format: format.into(),
                    path: path.into(),
                    state_b64,
                },
            ),
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
impl ProfileRig {
    pub fn new(rig: GuitarRig) -> Self {
        Self {
            rig,
            profile: None,
            patch_ids: Vec::new(),
            active: None,
            stack_pos: Vec::new(),
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

    /// Load a profile: uninstall any previous chains, pre-install every patch's
    /// chain into the bank, then activate the default patch. `base_dir` resolves
    /// relative block paths (pass the profile file's directory).
    pub fn load_profile(
        &mut self,
        profile: RigProfile,
        base_dir: Option<&Path>,
    ) -> Result<(), String> {
        self.rig.clear();
        self.patch_ids.clear();
        self.active = None;
        self.stack_pos = vec![0; profile.stacks.len()];

        let mut loaded = 0usize;
        let mut first_ok: Option<usize> = None;
        for (i, patch) in profile.patches.iter().enumerate() {
            // Resolve every buildable block's asset path against the base dir;
            // skip blocks with no audio backend yet — i.e. `Native` blocks, whose
            // built-in DSP isn't written (a not-yet-chosen Time-module effect).
            // They stay in the patch for display + bypass grouping and become
            // live once given a NAM/IR/plugin asset (or native DSP lands).
            let resolve = |p: &str| -> String {
                if p.is_empty() {
                    String::new()
                } else {
                    resolve_path(p, base_dir).to_string_lossy().to_string()
                }
            };
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
                self.patch_ids.push(MODEL_UNAVAILABLE);
                tracing::warn!(patch = %patch.name, "ProfileRig: patch has no blocks — skipping");
                continue;
            }
            // Stable, unique per-block ids (block name, deduped) so the UI can
            // address each block (bypass toggle, param edits) individually —
            // otherwise assetless native blocks all collapse to the id "block".
            let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
            let block_ids: Vec<String> = blocks
                .iter()
                .map(|b| {
                    let base = if b.name.trim().is_empty() {
                        format!("{:?}", b.block_type)
                    } else {
                        b.name.trim().to_string()
                    };
                    let n = seen.entry(base.clone()).or_insert(0);
                    let id = if *n == 0 {
                        base.clone()
                    } else {
                        format!("{base} {}", *n + 1)
                    };
                    *n += 1;
                    id
                })
                .collect();
            match self.rig.install_chain_with_ids(&blocks, &block_ids) {
                Ok(id) => {
                    self.patch_ids.push(id);
                    loaded += 1;
                    first_ok.get_or_insert(i);
                }
                Err(e) => {
                    self.patch_ids.push(MODEL_UNAVAILABLE);
                    tracing::warn!(
                        patch = %patch.name,
                        error = %e,
                        "ProfileRig: failed to build patch chain — skipping"
                    );
                }
            }
        }

        if loaded == 0 && !profile.patches.is_empty() {
            self.profile = Some(profile);
            return Err("no patch chains could be built".into());
        }

        let want = profile.default_patch;
        let start = if self
            .patch_ids
            .get(want)
            .copied()
            .unwrap_or(MODEL_UNAVAILABLE)
            != MODEL_UNAVAILABLE
        {
            want
        } else {
            first_ok.unwrap_or(0)
        };
        self.profile = Some(profile);
        self.activate(start);
        Ok(())
    }

    /// Convenience: load a profile from a `.styx` file.
    pub fn load_profile_file(&mut self, path: &Path) -> Result<(), String> {
        let profile = RigProfile::from_styx_file(path).map_err(|e| e.to_string())?;
        let base = path.parent();
        self.load_profile(profile, base)
    }

    /// Activate the patch at `index`. Returns false if out of range or its
    /// chain failed to build. Applies patch trims + (optional) level-match.
    pub fn activate(&mut self, index: usize) -> bool {
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
        self.rig.set_output_trim_db(output_trim);
        self.rig.set_active(Some(id));
        self.active = Some(index);
        self.apply_fx_bypass(index);
        true
    }

    /// Push the current global time-bypass state onto the active patch: bypass
    /// every installed time/fx block (the Time module) of patch `index`. The
    /// installed chain skips placeholder blocks, so we zip the live block ids
    /// against the patch's installed (has-backend) blocks to find which are time/fx.
    fn apply_fx_bypass(&self, index: usize) {
        let Some(profile) = &self.profile else {
            return;
        };
        let Some(patch) = profile.patches.get(index) else {
            return;
        };
        let live_ids = self.rig.active_block_ids();
        let real = patch.chain.iter().filter(|b| b.has_backend());
        for (block, id) in real.zip(live_ids.iter()) {
            if block.is_time_fx() {
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
            self.apply_fx_bypass(i);
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
        self.profile
            .as_ref()
            .map(|p| p.stacks.as_slice())
            .unwrap_or(&[])
    }

    /// The rotation cursor (index into the stack's patch list) for `stack_idx`.
    /// Reset every stack's rotation cursor to its first patch.
    pub fn reset_stack_positions(&mut self) {
        for p in self.stack_pos.iter_mut() {
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
        let target_pos = if already_active {
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
        self.profile
            .as_ref()
            .map(|p| p.patches.as_slice())
            .unwrap_or(&[])
    }

    pub fn is_patch_available(&self, index: usize) -> bool {
        self.patch_ids
            .get(index)
            .copied()
            .map(|id| id != MODEL_UNAVAILABLE)
            .unwrap_or(false)
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
    pub fn set_block_bypass(&self, block_id: &str, on: bool) -> bool {
        self.rig.set_block_slot_bypass(block_id, on)
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
    pub fn set_block_param(&self, block_id: &str, param_name: &str, value: f32) -> bool {
        self.rig.set_active_block_param(block_id, param_name, value)
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
