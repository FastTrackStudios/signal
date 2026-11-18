//! The worship rig, expressed in the canonical node model.
//!
//! This crate's own [`ProfileDef`](crate::profiles::ProfileDef) is a flat,
//! rig-specific shape: a preset pool, patches pointing into it, drive presets
//! with options, and a bespoke override list. The domain expresses all of that
//! with one type. This module converts one into the other, which is how the
//! model gets proven against a real rig before anything is moved onto it.
//!
//! # How the pieces map
//!
//! | this crate | the domain |
//! |---|---|
//! | a `PresetDef` (one amp capture) | a leaf [`Node`] whose block is `BlockKind::Nam` |
//! | a `DrivePresetDef` (a pedal, several captures) | a container node with one variant per capture |
//! | a `PatchDef` | a [`Variant`] of the chain node |
//! | an `OverrideDef` | an [`Override`] on that variant |
//!
//! The last row is the interesting one. Twelve patches are not twelve chains
//! — they are **twelve variants of one chain**, because they share every
//! block and differ only in which amp sits in the Amp slot and which
//! parameters are bent. That is the collapse the domain buys: the structure
//! is stored once.
//!
//! # A pedal's captures are variants, an amp's are not
//!
//! A drive preset holds several captures of *the same pedal* at different
//! settings, so they become variants of one node, swapped with `ReplaceRef`.
//! The amp presets are captures of *different amps* and stay separate nodes —
//! you choose between them. See the README on where a capture becomes a
//! preset and where it becomes a variant.

use signal_proto::block::BlockType;
use signal_proto::block_kind::{BlockKind, NamRef};
use signal_proto::model::Block;
use signal_proto::node::{Combine, Content, Node, NodeId, NodeLibrary, Role, Selection, Variant};
use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

use sampler::gapless::{Bank, InstallError, Resident, VariantBank};

use crate::library::RigHome;
use crate::profiles::{DrivePresetDef, OverrideDef, ProfileDef};

/// The worship rig as nodes: every capture, every pedal, and the chain whose
/// variants are the patches.
pub struct RigNodes {
    pub library: NodeLibrary,
    /// The chain node. Resolve it with one of [`patches`](Self::patches).
    pub chain: NodeId,
    /// Patch name → the variant that recalls it.
    pub patches: Vec<(String, signal_proto::node::VariantId)>,
    /// Every drive preset's name, so a pedal node can be told from any other
    /// container.
    pub pedal_names: Vec<String>,
    /// Which drive slot each pedal node occupies, and which of its variants
    /// is which capture option.
    ///
    /// The way back from a node choice to the profile field that stores it.
    /// The library is *derived* from `ProfileDef`, so selecting a preset has
    /// to be written back to the def or it is gone on the next rebuild —
    /// until the library itself is what is stored, this is what makes a
    /// choice persist.
    pub drive_slots: Vec<DriveSlotNodes>,
}

/// One drive slot's node, and its captures in profile-option order.
pub struct DriveSlotNodes {
    /// The slot as the profile names it — "Drive 1".
    pub slot: String,
    /// The pedal node sitting in it.
    pub node: NodeId,
    /// Variant per capture, indexed the way `DriveSlotDef::option` counts.
    pub options: Vec<signal_proto::node::VariantId>,
}

impl RigNodes {
    /// The library as the [`RigProfile`] the live rig installs.
    ///
    /// This is the adoption seam. `ProfileRig::load_profile` takes a
    /// `RigProfile` — a flat chain per patch — and installs each one into the
    /// gapless bank. Producing that from the library instead of from
    /// `build_profile` makes the node library the thing the rig plays, with
    /// one call changed at each site rather than the live rig rebuilt.
    ///
    /// `def` supplies what is performance metadata rather than signal: the
    /// profile's name, each patch's trim, and the stacks a footswitch
    /// rotates through. Everything that makes sound comes from resolving the
    /// library.
    ///
    /// A patch whose variant does not resolve is left out rather than
    /// installed empty, and says so — a silent patch on a footswitch is
    /// worse than one that is missing from the list.
    #[must_use]
    pub fn to_profile(
        &self,
        def: &ProfileDef,
        drives: &[DrivePresetDef],
    ) -> sampler::rig_profile::RigProfile {
        use sampler::rig_profile::{RigPatch, RigProfile, RigStack};

        let mut profile = RigProfile::new(&def.name);
        for patch in &def.patches {
            let Some(variant) = self.patch(&patch.name) else {
                tracing::warn!(patch.name = %patch.name, "guitar: patch has no variant");
                continue;
            };
            let resolved = match signal_proto::node_resolve::resolve(
                &self.library,
                &self.chain,
                Some(variant),
            ) {
                Ok((resolved, report)) => {
                    if !report.is_clean() {
                        tracing::warn!(
                            patch.name = %patch.name,
                            missing = report.missing.len(),
                            unmatched = report.unmatched_overrides.len(),
                            "guitar: patch resolved with holes"
                        );
                    }
                    resolved
                }
                Err(e) => {
                    tracing::warn!(patch.name = %patch.name, error = %e, "guitar: patch did not resolve");
                    continue;
                }
            };

            let mut built = RigPatch::new(&patch.name);
            built.chain = sampler::from_node::to_chain(&resolved);
            settle_amp_stage(&mut built.chain, def, patch);
            settle_drives(&mut built.chain, def, patch, drives);
            #[cfg(not(target_arch = "wasm32"))]
            apply_block_levels(&mut built.chain, def, patch, drives);
            // On the trim block inside the chain, not the scene's output —
            // see `profiles::set_patch_trim` for why a level after the
            // reverbs cannot be changed without hearing it.
            crate::profiles::set_patch_trim(&mut built, patch.level_db + patch.trim_db);
            crate::profiles::assign_meters(&mut built);
            profile = profile.with_patch(built);
        }
        for stack in &def.stacks {
            profile = profile.with_stack(RigStack::new(&stack.name, stack.patches.clone()));
        }
        // Where the profile lands when it loads: its default scene, by name.
        if let Some(i) = profile.patches.iter().position(|p| {
            !def.default_patch.is_empty() && p.name.eq_ignore_ascii_case(&def.default_patch)
        }) {
            profile.default_patch = i;
        }
        profile
    }

    /// Every pedal node in the library, by name — the board's alternatives.
    ///
    /// `to_nodes` builds a node for *every* drive preset the library holds,
    /// not only the assigned ones, so the unassigned ones are already here
    /// waiting to be put in a slot.
    #[must_use]
    pub fn pedals(&self) -> Vec<(String, NodeId)> {
        self.library
            .nodes
            .iter()
            .filter(|n| self.pedal_names.iter().any(|name| name == &n.name))
            .map(|n| (n.name.clone(), n.id.clone()))
            .collect()
    }

    /// The drive slot a pedal node currently occupies, if it is on the board.
    #[must_use]
    pub fn slot_of_node(&self, node: &str) -> Option<String> {
        self.drive_slots
            .iter()
            .find(|s| s.node.as_str() == node)
            .map(|s| s.slot.clone())
    }

    /// The drive preset a node is, by name — what the profile stores in a
    /// slot.
    #[must_use]
    pub fn pedal_name(&self, node: &str) -> Option<String> {
        self.library
            .nodes
            .iter()
            .find(|n| n.id.as_str() == node && self.pedal_names.iter().any(|p| p == &n.name))
            .map(|n| n.name.clone())
    }

    /// The drive slot and option index a `(node, variant)` choice means, if
    /// the profile has a field that can hold it.
    ///
    /// `None` for anything else — a module's preset, a block whose variants
    /// exist only in the library. Those are real choices the domain can
    /// express and the profile cannot, and saying so is better than writing
    /// them somewhere they will not survive a rebuild.
    #[must_use]
    pub fn drive_option_for(&self, node: &str, variant: &str) -> Option<(String, usize)> {
        let slot = self.drive_slots.iter().find(|s| s.node.as_str() == node)?;
        let option = slot.options.iter().position(|v| v.as_str() == variant)?;
        Some((slot.slot.clone(), option))
    }

    /// The variant for a patch by name.
    #[must_use]
    pub fn patch(&self, name: &str) -> Option<&signal_proto::node::VariantId> {
        self.patches
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// Add a variant of the chain that recalls it with one block on a
    /// different capture — "this patch, but the Klon on its high-gain side".
    ///
    /// This is how a Block Preset's variants become *performable* rather than
    /// editable. `set_block_option` changes profile state, which is an edit
    /// and costs a reload; a chain variant selecting a child's variant is a
    /// performance action, and it is installed with every other patch, so
    /// reaching it is a pointer swap.
    ///
    /// Returns the variant to press.
    pub fn with_selection(
        &mut self,
        name: &str,
        node: &NodeId,
        variant: &signal_proto::node::VariantId,
    ) -> Option<signal_proto::node::VariantId> {
        let chain = self.library.get_mut(&self.chain)?;
        let selecting = Variant::new(name).selecting(node.clone(), variant.clone());
        let id = selecting.id.clone();
        chain.variants.push(selecting);
        self.patches.push((name.to_string(), id.clone()));
        Some(id)
    }

    /// A node in the library by name — a pedal, an amp capture.
    #[must_use]
    pub fn node_named(&self, name: &str) -> Option<&Node> {
        self.library
            .nodes
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(name))
    }

    /// Install every patch, so switching between them cannot cause a gap.
    ///
    /// The whole profile becomes resident at once — twelve chains for twelve
    /// patches — which is what `ProfileRig` already does by hand for gapless
    /// footswitching. The difference is that afterwards the switch is a
    /// [`Resident`] token rather than an integer, so it *cannot* be asked to
    /// load. See [`sampler::gapless`].
    ///
    /// # Errors
    ///
    /// Propagates the first patch whose chain will not load, named — at
    /// install time, which is before the set rather than during it.
    pub fn install<B: Bank>(&self, bank: &mut B) -> Result<PatchBank, InstallError> {
        let variants = VariantBank::install_all(bank, &self.library, &self.chain)?;
        Ok(PatchBank {
            variants,
            by_name: self.patches.clone(),
        })
    }
}

/// Every patch of a profile, resident and switchable.
pub struct PatchBank {
    variants: VariantBank,
    by_name: Vec<(String, signal_proto::node::VariantId)>,
}

impl PatchBank {
    /// The token for a patch by name — what a footswitch presses.
    ///
    /// `None` only if the name is not a patch. A patch that exists is always
    /// resident, because [`RigNodes::install`] installs all of them.
    #[must_use]
    pub fn resident(&self, patch: &str) -> Option<Resident> {
        let variant = self
            .by_name
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(patch))
            .map(|(_, v)| v)?;
        self.variants.resident(variant)
    }

    /// Switch to a patch. Infallible and load-free — see [`VariantBank::activate`].
    pub fn press<B: Bank>(bank: &B, patch: Resident) {
        VariantBank::activate(bank, patch);
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.variants.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }
}

/// A leaf node realized by a `.nam` capture.
///
/// `block_type` is the role the capture plays — `Amp` for a preset in the
/// pool, `Drive` for a pedal's capture. Same `BlockKind::Nam` either way:
/// what a capture *is* and what it *does* are the two orthogonal axes.
fn nam_leaf(name: &str, path: &str, block_type: BlockType) -> Node {
    let mut block = Block::with_exact_parameters(Vec::new());
    block.kind = BlockKind::Nam {
        model: NamRef {
            model_path: path.to_string(),
            model_id: None,
        },
    };
    Node::leaf(name, block_type, block)
}

/// A node's id, derived from **what it is** rather than minted fresh.
///
/// `to_nodes` runs again on every profile edit — a rename, a trim, an
/// imported capture — so an id minted per build is not an identity: a stored
/// preset, an override addressed by id, a UI holding a selection would all
/// dangle on the next keystroke, silently.
///
/// UUIDv5 over a readable path (`worship/Time/DLY 1`), so the same profile
/// always names the same nodes, on this machine and any other. That is the
/// opposite tool from the v7 ids a *distributable* rig mints for nodes a
/// person creates — this is derived content, and derived content must agree
/// everywhere rather than be unique everywhere.
fn seeded(path: &str) -> NodeId {
    NodeId::from(signal_proto::seed_id(path).to_string())
}

/// A variant's id, seeded the same way: `<node path>@<variant name>`.
fn seeded_variant(path: &str, variant: &str) -> signal_proto::node::VariantId {
    signal_proto::node::VariantId::from(
        signal_proto::seed_id(&format!("{path}@{variant}")).to_string(),
    )
}

/// A variant with a stable id.
fn variant_at(path: &str, name: &str) -> Variant {
    let mut variant = Variant::new(name);
    variant.id = seeded_variant(path, name);
    variant
}

/// Give a node a seeded id, and seed its default variant with it.
fn at_path(mut node: Node, path: &str) -> Node {
    node.id = seeded(path);
    if let Some(default) = node.variants.first_mut() {
        let name = default.name.clone();
        default.id = seeded_variant(path, &name);
        node.default_variant = default.id.clone();
    }
    node
}

/// The module a block belongs to: what it says, or what its type implies.
///
/// `RigBlock::module` is the explicit answer and is usually empty; the block
/// type's category is the implicit one the rig has always used ("Empty =
/// grouped by the block type's category"). Either way every block has a
/// purpose, and in the node model a purpose is a container.
fn module_of(block: &sampler::RigBlock) -> String {
    if block.module.is_empty() {
        block.block_type.category().display_name().to_string()
    } else {
        block.module.clone()
    }
}

/// A pedal capture as a leaf, matching what the chain builder makes of the
/// same capture.
///
/// The drive board comes up **bypassed** — every pedal off until the control
/// surface engages it — and that is state, not decoration: a chain resolved
/// with three drives live is a different rig. The `drive` value rides along
/// as a setting because `BlockType::Drive` has no native DSP to range it
/// against; see `to_node` on why that is carried rather than guessed.
fn drive_leaf(name: &str, path: &str) -> Node {
    let mut node = nam_leaf(name, path, BlockType::Drive);
    node.bypassed = true;
    node.settings.push(signal_proto::node_routing::Setting {
        name: format!("{}drive", sampler::to_node::RAW_PARAM),
        value: "0.5".to_string(),
    });
    node
}

/// Give a resolved chain's amp stage what the patch definition says.
///
/// The node model has one swappable amp slot and seeds everything else from
/// the first patch's chain, so on its own it would name the amp after its
/// preset (the UI and `set_block_option` look for "Amp L"), keep the first
/// patch's cab on every patch, and play the first patch's Amp R everywhere.
/// The definition is the authority for all three, so they are settled here,
/// once, after resolving — keeping this path equal to `build_profile`.
fn settle_amp_stage(
    chain: &mut [sampler::RigBlock],
    def: &ProfileDef,
    patch: &crate::profiles::PatchDef,
) {
    let preset = |name: &str| {
        def.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    };
    let (cab_l, nam_r, cab_r) = (
        preset(&patch.preset)
            .map(|p| p.cab.clone())
            .unwrap_or_default(),
        preset(&patch.preset2)
            .map(|p| p.nam.clone())
            .unwrap_or_default(),
        preset(&patch.preset2)
            .map(|p| p.cab.clone())
            .unwrap_or_default(),
    );
    // Amp R's bypass: the patch's own override if it has one, else engaged
    // exactly when something is loaded.
    let r_bypassed = patch
        .overrides
        .iter()
        .rev()
        .find(|o| o.op.eq_ignore_ascii_case("bypass") && o.block.eq_ignore_ascii_case("Amp R"))
        .map_or(nam_r.is_empty(), |o| o.value >= 0.5 || nam_r.is_empty());

    if let Some(amp) = chain
        .iter_mut()
        .find(|b| b.block_type == BlockType::Amp && !b.name.eq_ignore_ascii_case("Amp R"))
    {
        amp.name = "Amp L".to_string();
        // Its settings — an amp snapshot's input trim (`drive`): the slot's
        // leaf is whichever capture the patch swapped in, which carries only
        // what its own preset set, so the patch's word on Amp L lands here.
        for ov in patch.overrides.iter().filter(|o| {
            o.block.eq_ignore_ascii_case("Amp L")
                && o.op.eq_ignore_ascii_case("set")
                && !o.param.is_empty()
        }) {
            match amp.params.iter_mut().find(|p| p.name == ov.param) {
                Some(p) => p.value = ov.value.to_string(),
                None => amp.params.push(sampler::rig_node::Param {
                    name: ov.param.clone(),
                    value: ov.value.to_string(),
                }),
            }
        }
    }
    for block in chain.iter_mut() {
        match block.name.as_str() {
            "Cab L" => block.ir = cab_l.clone(),
            "Amp R" => {
                block.nam = nam_r.clone();
                block.bypassed = r_bypassed;
            }
            "Cab R" => {
                block.ir = cab_r.clone();
                block.bypassed = r_bypassed;
            }
            _ => {}
        }
    }
}

/// A patch's own drive-slot picks (from a Drive module snapshot), over the
/// profile's board. The node model's pedal slots are profile-wide, so a
/// patch that runs different pedals gets them set here after resolving —
/// the same seam as the amp stage.
fn settle_drives(
    chain: &mut [sampler::RigBlock],
    def: &ProfileDef,
    patch: &crate::profiles::PatchDef,
    presets: &[DrivePresetDef],
) {
    if patch.drives.is_empty() {
        return;
    }
    for slot in crate::compose::drives_for(def, patch) {
        let nam = presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&slot.preset))
            .and_then(|p| p.options.get(slot.option))
            .map(|o| o.nam.clone())
            .unwrap_or_default();
        if let Some(block) = chain
            .iter_mut()
            .find(|b| b.name.eq_ignore_ascii_case(&slot.block))
        {
            block.nam = nam;
        }
    }
}

/// The rig's sample rate the drive curves are cached at — levels are set
/// at build time, before any rig says what rate it runs.

/// Every amp and drive block's Output Level, written into its trims as the
/// chain is built: the amp's from its module snapshot (via the pool preset),
/// a pedal's from its drive option — each levelled on its own by
/// `signal rig level-modules`. Its input trim is what its drive knob stands
/// for; a knob away from 0.5 adds the cached curve's correction to the
/// level, so the pedal stays as loud as it was levelled.
///
/// Built into the chain, not applied when a patch is switched in: a level
/// set after the chain was already playing was a gain stage only the live
/// rig had, and the first thing to drift.
#[cfg(not(target_arch = "wasm32"))]
fn apply_block_levels(
    chain: &mut [sampler::RigBlock],
    def: &ProfileDef,
    patch: &crate::profiles::PatchDef,
    drives: &[DrivePresetDef],
) {
    let pool_level = |name: &str| {
        def.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map_or(0.0, |p| p.level_db)
    };
    for block in chain.iter_mut() {
        if block.nam.is_empty() {
            continue;
        }
        let level = match block.block_type {
            BlockType::Amp if block.name.eq_ignore_ascii_case("Amp R") => {
                pool_level(&patch.preset2)
            }
            BlockType::Amp => pool_level(&patch.preset),
            BlockType::Drive | BlockType::Boost => drive_option_level(drives, &block.nam),
            _ => continue,
        };
        let drive = block.param_f32("drive").unwrap_or(0.5);
        let path = std::path::Path::new(&block.nam);
        (block.input_trim_db, block.output_trim_db) =
            sampler::nam_calibrate::drive_trims(path, level, drive);
    }
}

/// A drive capture's Output Level: the option whose capture it is.
pub(crate) fn drive_option_level(drives: &[DrivePresetDef], nam: &str) -> f32 {
    let file = |p: &str| {
        std::path::Path::new(p)
            .file_name()
            .map(std::ffi::OsStr::to_owned)
    };
    drives
        .iter()
        .flat_map(|p| &p.options)
        .find(|o| o.nam == nam || (file(&o.nam).is_some() && file(&o.nam) == file(nam)))
        .map_or(0.0, |o| o.level_db)
}

/// The playable profile for a rig definition — **the path the live rig
/// takes**.
///
/// Builds the node library and resolves every patch out of it, rather than
/// building chains directly. One function so the whole session has one seam:
/// everything that installs a profile goes through here, and what the rig
/// plays is what the domain resolved.
///
/// `build_profile` is still the builder underneath — [`to_nodes`] uses it to
/// get the chain's shape before any patch bends it — but nothing downstream
/// of this sees its output. The two are pinned equal for the shipped rig by
/// `to_profile_matches_the_builder_for_the_shipped_rig`, patch for patch and
/// parameter for parameter.
///
/// This reads the process's own library ([`RigHome::process_default`]); a
/// rig with a home of its own builds through [`profile_from_library_in`].
#[must_use]
pub fn profile_from_library(
    def: &ProfileDef,
    drives: &[DrivePresetDef],
) -> sampler::rig_profile::RigProfile {
    profile_from_library_in(&RigHome::process_default(), def, drives)
}

/// [`profile_from_library`] out of `home`'s library: its compositions, its
/// saved nodes, its instrument.
#[must_use]
pub fn profile_from_library_in(
    home: &RigHome,
    def: &ProfileDef,
    drives: &[DrivePresetDef],
) -> sampler::rig_profile::RigProfile {
    let flat = crate::compose::flatten(def, &home.load_compositions());
    to_nodes_with_store_in(home, &flat, drives).to_profile(&flat, drives)
}

/// The rig's node library: derived from the profile, then everything saved
/// about its nodes applied on top.
///
/// One function, so no caller can accidentally use the bare derivation and
/// silently lose a saved preset. The process's own library; a rig with a
/// home of its own reads through [`library_for_in`].
#[must_use]
pub fn library_for(def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    library_for_in(&RigHome::process_default(), def, drives)
}

/// [`library_for`] out of `home`'s library.
#[must_use]
pub fn library_for_in(home: &RigHome, def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    let flat = crate::compose::flatten(def, &home.load_compositions());
    to_nodes_with_store_in(home, &flat, drives)
}

/// [`to_nodes`] plus the saved node overlay, for a profile already
/// flattened — the process's own library.
pub(crate) fn to_nodes_with_store(def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    to_nodes_with_store_in(&RigHome::process_default(), def, drives)
}

/// [`to_nodes_with_store`] out of `home`'s library, fitted to its
/// instrument.
pub(crate) fn to_nodes_with_store_in(
    home: &RigHome,
    def: &ProfileDef,
    drives: &[DrivePresetDef],
) -> RigNodes {
    let mut rig = to_nodes_for(def, drives, home.instrument());
    home.load_node_store().apply(&mut rig);
    rig
}

/// Convert this crate's profile into the domain's node model.
///
/// The **whole** chain, not an outline of it: the compressor, the volume
/// pedal, the drive board, the amp, the gate, the amp EQ and the time module
/// all become nodes, with their parameter values ranged against the DSP that
/// reads them. It was structure-only until the audio-side bridge stopped
/// dropping parameters and `native::range_of` gave the values somewhere to
/// land.
///
/// One thing is deliberately *not* a copy of the built chain. A drive slot
/// points at that pedal's container node — whose variants are its captures —
/// rather than at a flat block named after the slot. So the resolved chain
/// names a drive by its capture ("Both Sides") where the builder names it by
/// its slot ("Drive 1"). That is the model earning its keep: the pedal is one
/// node with several settings, and swapping between them is a variant switch
/// rather than a rebuild.
#[must_use]
pub fn to_nodes(def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    to_nodes_for(def, drives, crate::instrument::current())
}

/// [`to_nodes`] with the chain fitted to `instrument` (a rig's own, not
/// the process's).
#[must_use]
pub fn to_nodes_for(
    def: &ProfileDef,
    drives: &[DrivePresetDef],
    instrument: crate::instrument::Instrument,
) -> RigNodes {
    let mut library = NodeLibrary::new();

    // ── Amp captures: one node each, because they are different amps ─────
    let mut amps = Vec::new();
    for preset in &def.presets {
        let node = at_path(
            nam_leaf(&preset.name, &preset.nam, BlockType::Amp),
            &format!("{}/amp/{}", def.name, preset.name),
        );
        amps.push((preset.name.clone(), node.id.clone()));
        library.insert(node);
    }

    // ── Pedals: one node each, its captures as variants ──────────────────
    //
    // A pedal's leaves are named after the **slot** the profile assigns it
    // to, because that is how a patch addresses them; see `drive_leaf`. A
    // pedal the profile does not use keeps the capture's name, since there is
    // no slot to name it after.
    let slot_of = |preset: &str| {
        def.drives
            .iter()
            .find(|d| d.preset.eq_ignore_ascii_case(preset))
            .map(|d| d.block.clone())
    };
    let mut pedals = Vec::new();
    let mut drive_slots: Vec<DriveSlotNodes> = Vec::new();
    for pedal in drives {
        let Some(first) = pedal.options.first() else {
            continue;
        };
        let slot = slot_of(&pedal.name);
        let leaf_name = |capture: &str| slot.clone().unwrap_or_else(|| capture.to_string());
        let pedal_path = format!("{}/pedal/{}", def.name, pedal.name);
        let default_leaf = at_path(
            drive_leaf(&leaf_name(&first.name), &first.nam),
            &format!("{pedal_path}/{}", first.name),
        );
        let default_id = default_leaf.id.clone();
        library.insert(default_leaf);

        let mut node = at_path(
            Node::container(&pedal.name, Role::Module, Combine::Serial),
            &pedal_path,
        );
        if let Content::Children { nodes } = &mut node.content {
            nodes.push(default_id.clone());
        }
        // Option index → the variant that recalls it. Index 0 is the node's
        // own default; the rest are variants that swap the capture in the
        // slot. A profile assigns a slot *an option*, so the chain has to be
        // able to name one.
        let mut options = vec![node.default_variant.clone()];
        // Its own default variant is the first capture; each later capture
        // swaps the node in that slot. One pedal, several settings.
        for option in pedal.options.iter().skip(1) {
            let leaf = at_path(
                drive_leaf(&leaf_name(&option.name), &option.nam),
                &format!("{pedal_path}/{}", option.name),
            );
            let mut variant = variant_at(&pedal_path, &option.name);
            variant.overrides.push(Override {
                path: NodePath::new(vec![NodePathSegment::Block {
                    id: default_id.as_str().to_string(),
                }]),
                op: NodeOverrideOp::ReplaceRef {
                    id: leaf.id.as_str().to_string(),
                },
            });
            library.insert(leaf);
            options.push(variant.id.clone());
            node.variants.push(variant);
        }
        if let Some(slot) = slot.clone() {
            drive_slots.push(DriveSlotNodes {
                slot,
                node: node.id.clone(),
                options: options.clone(),
            });
        }
        pedals.push((pedal.name.clone(), node.id.clone(), options));
        library.insert(node);
    }

    // ── The chain: the real chain, slot for slot ─────────────────────────
    //
    // Built by walking the chain `build_profile` produces — the compressor,
    // the volume pedal, the gate, the amp EQ, the time module, all of it —
    // and lifting each block into a leaf node, except at two kinds of slot:
    //
    // - a **drive slot** points at that pedal's container node, so its
    //   captures stay variants of one pedal;
    // - the **amp slot** points at an amp capture node, which patches swap.
    //
    // This used to hold only the drives and the amp, which made the node
    // library a sketch of the rig rather than the rig. Lifting the rest is
    // what lets the library be the source of truth: what resolves out of it
    // is the chain, not an outline of it.
    // The chain as the builder makes it, *before* any patch bends it.
    //
    // `build_profile` applies each patch's overrides to that patch's own
    // chain, so `patches[0]` is not the base — it is the first patch. Lifting
    // that as the base baked Clean's gate threshold, EQ and delay mix into
    // every other patch's starting point, and only a patch that happened to
    // override the same parameter corrected it. Silent, and exactly the kind
    // of thing you would chase as "the Dry patch sounds gated".
    let unbent = ProfileDef {
        default_patch: String::new(),
        patches: def
            .patches
            .iter()
            .map(|patch| crate::profiles::PatchDef {
                overrides: Vec::new(),
                ..patch.clone()
            })
            .collect(),
        ..def.clone()
    };
    let base = crate::profiles::build_profile_for(&unbent, drives, instrument);
    let base_chain: &[sampler::RigBlock] = base
        .patches
        .first()
        .map_or(&[], |patch| patch.chain.as_slice());

    let chain_path = format!("{}/chain", def.name);
    let mut chain = at_path(
        Node::container(&def.name, Role::Preset, Combine::Serial),
        &chain_path,
    );
    let mut amp_slot = None;
    let mut slot_options: Vec<Selection> = Vec::new();
    // Blocks grouped into Module containers, because that is what a Module
    // *is* — "a collection of blocks with a purpose". The rig already says
    // which purpose each block belongs to: its explicit `module`, or its
    // block type's category. A patch override addressed as
    // `module Time -> block VERB 1 -> param mix` needs the Time module to
    // exist as a node, and this is where it comes from.
    let mut modules: Vec<(String, Vec<NodeId>)> = Vec::new();
    // How many blocks of each `(module, name)` the chain has held so far.
    let mut occurrences: std::collections::HashMap<(String, String), u32> =
        std::collections::HashMap::new();
    let push_into = |module: String, id: NodeId, modules: &mut Vec<(String, Vec<NodeId>)>| {
        // Consecutive blocks of one purpose are one module. Non-consecutive
        // ones are separate modules of the same name — the chain's order is
        // the signal order and must not be rearranged to tidy the grouping.
        match modules.last_mut() {
            Some((name, ids)) if *name == module => ids.push(id),
            _ => modules.push((module, vec![id])),
        }
    };
    {
        for block in base_chain {
            // A drive slot: the block's name is one of the board's slots.
            // (A board pedal, by type: the post-amp gain block is also
            // called "Boost".)
            let drive_slot = crate::profiles::BOARD_SLOTS
                .iter()
                .find(|slot| block.display_name().eq_ignore_ascii_case(slot))
                .filter(|_| matches!(block.block_type, BlockType::Drive | BlockType::Boost));
            if let Some(slot) = drive_slot {
                // The boost slot unassigned plays the library's boost
                // capture (see `profiles::boost_block`) — the same pedal here.
                let assignment = def
                    .drives
                    .iter()
                    .find(|d| d.block.eq_ignore_ascii_case(slot))
                    .map(|d| (d.preset.clone(), d.option))
                    .or_else(|| {
                        slot.eq_ignore_ascii_case(crate::profiles::BOOST_SLOT)
                            .then(|| crate::profiles::default_boost(drives))
                            .flatten()
                    });
                let assigned = assignment.and_then(|(preset, option)| {
                    pedals
                        .iter()
                        .find(|(name, _, _)| name.eq_ignore_ascii_case(&preset))
                        .map(|(_, id, options)| (id, options, option))
                });
                if let Some((id, options, option)) = assigned {
                    push_into(module_of(block), id.clone(), &mut modules);
                    // Which of the pedal's captures this slot runs. Without
                    // this the chain resolved every pedal to its first
                    // capture, so a slot assigned the Medium Gain setting
                    // played the Low Gain one — the same pedal, the wrong
                    // sound.
                    if let Some(variant) = options.get(option) {
                        slot_options.push(Selection {
                            node: id.clone(),
                            variant: variant.clone(),
                        });
                    }
                    continue;
                }
                // A slot the profile has not assigned is still a slot: the
                // builder puts a transparent placeholder there so every slot
                // stays addressable, and dropping it here would make the
                // library's chain a block shorter than the rig's.
            }
            // The amp slot holds the first capture; every patch that wants a
            // different one swaps it. Only "Amp L" — the pool-preset swap
            // this whole node model was built around. "Amp R" (a second amp,
            // in parallel — see `profiles::PatchDef::preset2`) has no swap
            // variant of its own yet in this node model, so it falls through
            // to the generic leaf path below like any other block: one leaf,
            // seeded from the first patch's chain, the same as every patch
            // until a variant-swap is added for it too.
            if block.block_type == BlockType::Amp
                && block.display_name().eq_ignore_ascii_case("Amp L")
            {
                if let Some((_, id)) = amps.first() {
                    push_into(module_of(block), id.clone(), &mut modules);
                    amp_slot = Some(id.clone());
                }
                continue;
            }
            // Everything else is itself, at a path built from where it sits:
            // `<profile>/<module>/<name>`, plus an occurrence number when a
            // module holds two blocks of the same name. The chain has two
            // called "Boost" — the board's and the post-amp one — and without
            // the discriminator they would seed to the same id and the second
            // would silently replace the first in the library.
            let (leaf, _report) = sampler::to_node::lift_one_block(block);
            let module = module_of(block);
            let key = (module.clone(), block.display_name());
            let seen = occurrences.entry(key).or_insert(0);
            *seen += 1;
            let path = if *seen == 1 {
                format!("{}/{module}/{}", def.name, block.display_name())
            } else {
                format!("{}/{module}/{}#{seen}", def.name, block.display_name())
            };
            let leaf = at_path(leaf, &path);
            push_into(module, leaf.id.clone(), &mut modules);
            library.insert(leaf);
        }
    }

    // One container per module, in chain order, under the chain node.
    if let Content::Children { nodes } = &mut chain.content {
        // A module name can appear more than once: the chain groups
        // *consecutive* blocks of one purpose, and Dynamics occurs twice —
        // the compressor at the front, the gate after the amp. They are two
        // different modules that happen to share a word, so they need two
        // ids; seeding both from the name alone made the second replace the
        // first in the library and the chain lost its compressor.
        let mut module_seen: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for (name, children) in modules {
            let seen = module_seen.entry(name.clone()).or_insert(0);
            *seen += 1;
            let module_path = if *seen == 1 {
                format!("{}/module/{name}", def.name)
            } else {
                format!("{}/module/{name}#{seen}", def.name)
            };
            let mut module = at_path(
                Node::container(&name, Role::Module, Combine::Serial),
                &module_path,
            );
            if let Content::Children { nodes: inner } = &mut module.content {
                *inner = children;
            }
            nodes.push(module.id.clone());
            library.insert(module);
        }
    }

    // What a parameter of a named block means — the base chain knows which
    // block type each name is, and the block type's DSP knows the range.
    let ranges = |block: &str, param: &str| {
        base_chain
            .iter()
            .find(|b| b.display_name().eq_ignore_ascii_case(block))
            .and_then(|b| sampler::native::range_of(b.block_type, param))
    };

    // The pedal settings the board is wired to, on the chain's own default.
    if let Some(default) = chain.variants.first_mut() {
        default.selections.extend(slot_options.iter().cloned());
    }

    // ── Patches: variants of that one chain ──────────────────────────────
    let mut patches = Vec::new();
    for patch in &def.patches {
        let mut variant = variant_at(&chain_path, &patch.name);
        // Each variant carries the board's pedal settings, because a child
        // a variant does not name falls back to its *own* default — not to
        // whatever the node's default variant selected. A patch that said
        // nothing about the drives would otherwise quietly play a different
        // capture from the one the board is wired to.
        variant.selections.extend(slot_options.iter().cloned());
        // A patch wanting the amp already in the slot needs no override;
        // every other patch swaps it.
        if let Some(slot) = amp_slot.as_ref()
            && let Some((_, wanted)) = amps
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(&patch.preset))
            && wanted != slot
        {
            variant.overrides.push(Override {
                path: NodePath::new(vec![NodePathSegment::Block {
                    id: slot.as_str().to_string(),
                }]),
                op: NodeOverrideOp::ReplaceRef {
                    id: wanted.as_str().to_string(),
                },
            });
        }
        variant.overrides.extend(
            patch
                .overrides
                .iter()
                .filter_map(|ov| to_override(ov, &ranges)),
        );
        patches.push((patch.name.clone(), variant.id.clone()));
        chain.variants.push(variant);
    }

    // A patch may bend a parameter the chain never set — the Ambient patch
    // picks a reverb algorithm, and the built chain only sets mix, decay and
    // size. The rig's own applier pushes such a parameter onto the block when
    // it is missing; the domain cannot, because an override names a parameter
    // and does not carry its range.
    //
    // So seed it here, at the DSP's own default, and let the override move it
    // from there. Only the parameters some patch actually touches exist,
    // which keeps the library the size of the rig rather than the size of
    // every knob the DSP has.
    for patch in &def.patches {
        for ov in &patch.overrides {
            if ov.op.as_str() != "set" || ov.param.is_empty() || ov.block.is_empty() {
                continue;
            }
            let Some(target) = base_chain
                .iter()
                .find(|b| b.display_name().eq_ignore_ascii_case(&ov.block))
            else {
                continue;
            };
            let Some(range) = sampler::native::range_of(target.block_type, &ov.param) else {
                continue;
            };
            // A capture's drive sits at 0.5 — the capture at unity — not at
            // whatever a native block of its type would start from: a patch
            // trimming Amp R would otherwise turn every other patch's down
            // 12 dB.
            let default = if ov.param == "drive" && !target.nam.is_empty() {
                0.5
            } else {
                sampler::native::default_of(target.block_type, &ov.param).unwrap_or(range.min)
            };
            let Some(node) = library
                .nodes
                .iter_mut()
                .find(|n| n.name.eq_ignore_ascii_case(&ov.block) && n.is_leaf())
            else {
                continue;
            };
            if let Content::Leaf { block, .. } = &mut node.content
                && !block.parameters().iter().any(|p| p.id() == ov.param)
            {
                block.push_parameter(signal_proto::BlockParameter::ranged(
                    &ov.param, &ov.param, default, range,
                ));
            }
        }
    }

    let chain_id = chain.id.clone();
    library.insert(chain);
    RigNodes {
        library,
        chain: chain_id,
        patches,
        pedal_names: drives.iter().map(|p| p.name.clone()).collect(),
        drive_slots,
    }
}

/// This crate's flat override into the domain's path-and-op form.
///
/// Returns `None` for an op this rig writes but the domain expresses another
/// way — `set_text` swaps an asset, which is a `ReplaceRef` against a node,
/// not a parameter write.
/// One of this crate's flat override defs as a domain [`Override`].
///
/// `ranges` answers what a block's parameter means, by block name — the
/// override's value is a **real** value in the DSP's units (a delay feedback
/// of 0.4 on a 0..0.95 control, 800 Hz on a filter) while the domain stores a
/// normalized position, so it has to be converted here. Without that, a
/// feedback of 0.4 was stored as the position 0.4 and came back out as 0.38:
/// a real difference, silently, on every patch that bends a parameter.
///
/// A parameter `ranges` cannot answer for keeps the value as written, which
/// is the same trade the lift makes for an unrangeable parameter.
fn to_override(
    ov: &OverrideDef,
    ranges: &impl Fn(&str, &str) -> Option<signal_proto::ParameterRange>,
) -> Option<Override> {
    let mut segments = Vec::new();
    // `OverrideDef::module` is deliberately NOT part of the path.
    //
    // It reads like an address and is not one: this crate's own applier
    // (`profiles::apply_overrides`) matches on the block name alone and never
    // looks at it, and `ProfileDef::override_modules` collects it "for the
    // UI's" benefit. It is a display grouping, and the names it uses are the
    // player's ("Utility"), not the block types' categories ("Dynamics").
    //
    // Emitting it as a path segment made the domain stricter than the rig has
    // ever been, and the override then matched nothing.
    if !ov.block.is_empty() {
        segments.push(NodePathSegment::Block {
            id: ov.block.clone(),
        });
    }
    match ov.op.as_str() {
        "set" if !ov.param.is_empty() => {
            segments.push(NodePathSegment::Parameter {
                id: ov.param.clone(),
            });
            let position =
                ranges(&ov.block, &ov.param).map_or(ov.value, |range| range.normalize(ov.value));
            Some(Override::set(NodePath::new(segments), position))
        }
        "bypass" => Some(Override::bypass(NodePath::new(segments), ov.value >= 0.5)),
        _ => None,
    }
}
