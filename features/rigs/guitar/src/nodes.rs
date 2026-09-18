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

use signal_sampler::gapless::{Bank, InstallError, Resident, VariantBank};

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
    pub fn to_profile(&self, def: &ProfileDef) -> signal_sampler::rig_profile::RigProfile {
        use signal_sampler::rig_profile::{RigPatch, RigProfile, RigStack};

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
            built.chain = signal_sampler::from_node::to_chain(&resolved);
            built.output_trim_db += patch.trim_db;
            profile = profile.with_patch(built);
        }
        for stack in &def.stacks {
            profile = profile.with_stack(RigStack::new(&stack.name, stack.patches.clone()));
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
    /// load. See [`signal_sampler::gapless`].
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
fn module_of(block: &signal_sampler::RigBlock) -> String {
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
        name: format!("{}drive", signal_sampler::to_node::RAW_PARAM),
        value: "0.5".to_string(),
    });
    node
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
#[must_use]
pub fn profile_from_library(
    def: &ProfileDef,
    drives: &[DrivePresetDef],
) -> signal_sampler::rig_profile::RigProfile {
    library_for(def, drives).to_profile(def)
}

/// The rig's node library: derived from the profile, then everything saved
/// about its nodes applied on top.
///
/// One function, so no caller can accidentally use the bare derivation and
/// silently lose a saved preset.
#[must_use]
pub fn library_for(def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    let mut rig = to_nodes(def, drives);
    crate::library::RigLibrary::load_node_store().apply(&mut rig);
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
    let base = crate::profiles::build_profile(&unbent, drives);
    let base_chain: &[signal_sampler::RigBlock] = base
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
    let mut push_into = |module: String, id: NodeId, modules: &mut Vec<(String, Vec<NodeId>)>| {
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
            let drive_slot = crate::profiles::DRIVE_SLOTS
                .iter()
                .find(|slot| block.display_name().eq_ignore_ascii_case(slot));
            if let Some(slot) = drive_slot {
                let assigned = def
                    .drives
                    .iter()
                    .find(|d| d.block.eq_ignore_ascii_case(slot))
                    .and_then(|d| {
                        pedals
                            .iter()
                            .find(|(name, _, _)| name.eq_ignore_ascii_case(&d.preset))
                            .map(|(_, id, options)| (id, options, d.option))
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
            // different one swaps it.
            if block.block_type == BlockType::Amp {
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
            let (leaf, _report) = signal_sampler::to_node::lift_one_block(block);
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
            .and_then(|b| signal_sampler::native::range_of(b.block_type, param))
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
            let Some(range) = signal_sampler::native::range_of(target.block_type, &ov.param) else {
                continue;
            };
            let default = signal_sampler::native::default_of(target.block_type, &ov.param)
                .unwrap_or(range.min);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{drive_presets, worship_def};
    use signal_proto::node_resolve::{ResolvedContent, resolve};

    /// The rig that actually plays: the shipped config, which is byte-identical
    /// to the one in `~/.config/signal/rig`.
    fn shipped() -> (ProfileDef, Vec<DrivePresetDef>) {
        #[derive(facet::Facet)]
        struct Presets {
            presets: Vec<DrivePresetDef>,
        }
        let def: ProfileDef = facet_styx::from_str(crate::library::DEFAULT_PROFILE)
            .expect("the shipped profile parses");
        let drives = facet_styx::from_str::<Presets>(crate::library::DEFAULT_DRIVE_PRESETS)
            .expect("the shipped drive presets parse")
            .presets;
        (def, drives)
    }

    /// The amp capture in a resolved chain, by block type — the chain runs
    /// on past it, so position says nothing.
    fn amp_leaf<'a>(
        leaves: &[&'a signal_proto::node_resolve::Resolved],
    ) -> Option<&'a signal_proto::node_resolve::Resolved> {
        leaves.iter().copied().find(|leaf| match &leaf.content {
            ResolvedContent::Leaf { block_type, .. } => *block_type == BlockType::Amp,
            ResolvedContent::Children(_) => false,
        })
    }

    fn nam_of(node: &signal_proto::node_resolve::Resolved) -> Option<&str> {
        match &node.content {
            ResolvedContent::Leaf { block, .. } => match &block.kind {
                BlockKind::Nam { model } => Some(model.model_path.as_str()),
                _ => None,
            },
            ResolvedContent::Children(_) => None,
        }
    }

    /// The twelve patches are twelve variants of ONE chain — the structure is
    /// stored once, which is the collapse the domain buys.
    #[test]
    fn the_worship_patches_are_variants_of_a_single_chain() {
        let rig = to_nodes(&worship_def(), &drive_presets());
        let chain = rig.library.get(&rig.chain).expect("chain");

        assert_eq!(rig.patches.len(), 12, "every patch became a variant");
        assert_eq!(
            chain.variants.len(),
            13,
            "twelve patches plus the node's own default"
        );
        // One chain, not twelve.
        let chains = rig
            .library
            .nodes
            .iter()
            .filter(|n| n.role == Role::Preset)
            .count();
        assert_eq!(chains, 1);
    }

    /// Patches pointing at different amps resolve to different captures,
    /// through `ReplaceRef` against the amp slot.
    #[test]
    fn a_patch_resolves_to_the_amp_it_points_at() {
        let rig = to_nodes(&worship_def(), &drive_presets());

        let clean = rig.patch("Clean").expect("Clean exists");
        let (resolved, report) = resolve(&rig.library, &rig.chain, Some(clean)).expect("resolves");
        assert!(report.missing.is_empty(), "no dangling references");
        // By block type, not by position: the chain continues past the amp
        // into the gate, the amp EQ and the time module.
        let leaves = resolved.leaves();
        let amp = amp_leaf(&leaves).expect("the chain has an amp");
        assert!(
            nam_of(amp).is_some_and(|p| p.contains("Fender")),
            "Clean points at the Fender capture, got {:?}",
            nam_of(amp)
        );

        let ambient = rig.patch("Ambient").expect("Ambient exists");
        let (resolved, _) = resolve(&rig.library, &rig.chain, Some(ambient)).expect("resolves");
        let leaves = resolved.leaves();
        let amp = amp_leaf(&leaves).expect("amp");
        assert!(
            nam_of(amp).is_some_and(|p| p.contains("AC30") || p.contains("TB30")),
            "Ambient points at the AC30 capture, got {:?}",
            nam_of(amp)
        );
    }

    /// A pedal's captures are variants of one node, not separate pedals.
    #[test]
    fn a_pedals_captures_are_variants_of_one_node() {
        let rig = to_nodes(&worship_def(), &drive_presets());
        let kot = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "King of Tone")
            .expect("the pedal is one node");

        // Two captures: its default plus one swap variant.
        assert_eq!(kot.variants.len(), 2, "both sides, and red-as-boost");
        assert!(
            kot.variants.iter().any(|v| v
                .overrides
                .iter()
                .any(|o| matches!(o.op, NodeOverrideOp::ReplaceRef { id: _ }))),
            "a later capture swaps the node in the slot"
        );
    }

    /// The end of the road: nodes to something the sampler plays.
    ///
    /// This is what the old `from_proto` could not do — it dropped names and
    /// parameters and skipped native blocks, which is why the rig never used
    /// it.
    #[test]
    fn a_patch_resolves_all_the_way_to_playable_blocks() {
        let rig = to_nodes(&worship_def(), &drive_presets());
        let ambient = rig.patch("Ambient").expect("Ambient");
        let (resolved, report) =
            resolve(&rig.library, &rig.chain, Some(ambient)).expect("resolves");
        assert!(report.missing.is_empty());

        let chain = signal_sampler::from_node::to_chain(&resolved);
        assert!(!chain.is_empty(), "the patch renders to a chain");

        let amp = chain
            .iter()
            .find(|b| b.block_type == signal_proto::block::BlockType::Amp)
            .expect("the chain has an amp");
        assert!(
            amp.nam.contains("AC30") || amp.nam.contains("TB30"),
            "and it is the AC30 this patch asked for: {}",
            amp.nam
        );
        // Names and identity survive, which the old bridge dropped outright.
        assert!(chain.iter().all(|b| !b.name.is_empty()));
        assert!(chain.iter().all(|b| !b.id.is_empty()));
    }

    /// The whole stack, end to end: the shipped profile becomes nodes,
    /// every patch is installed, and then a set's worth of footswitch
    /// presses loads nothing at all.
    #[test]
    fn a_whole_set_of_footswitch_presses_loads_nothing() {
        /// Counts installs so the test can assert switching does none.
        #[derive(Default)]
        struct Counting {
            installs: usize,
            next: u32,
        }
        impl Bank for Counting {
            fn install(&mut self, _blocks: &[signal_sampler::RigBlock]) -> Result<u32, String> {
                self.installs += 1;
                self.next += 1;
                Ok(self.next)
            }
            fn activate(&self, _model: u32) {}
            fn uninstall(&mut self, _model: u32) {}
        }

        let rig = to_nodes(&worship_def(), &drive_presets());
        let mut bank = Counting::default();
        let patches = rig.install(&mut bank).expect("every patch installs");

        let loaded_once = bank.installs;
        assert!(loaded_once >= rig.patches.len(), "all of them are resident");

        // Play the set: every patch, several times over, as a footswitch
        // would during a service.
        for _ in 0..5 {
            for (name, _) in &rig.patches {
                let token = patches
                    .resident(name)
                    .unwrap_or_else(|| panic!("{name} is resident"));
                PatchBank::press(&bank, token);
            }
        }

        assert_eq!(
            bank.installs, loaded_once,
            "sixty patch changes and not one load — the gap cannot happen"
        );
        assert!(patches.resident("Not A Patch").is_none());
    }

    /// The requirement that started this: switching between a Block Preset's
    /// captures — every gain setting of a pedal, every version of an AC30 —
    /// with no gap, inside the Snapshot system.
    ///
    /// It needs no new machinery. A chain variant selects which variant each
    /// child uses, and chain variants are what the bank installs, so a
    /// capture swap is reached by the same pointer swap as a patch change.
    #[test]
    fn switching_a_blocks_captures_is_as_gapless_as_switching_patches() {
        #[derive(Default)]
        struct Counting {
            installs: usize,
            next: u32,
        }
        impl Bank for Counting {
            fn install(&mut self, _b: &[signal_sampler::RigBlock]) -> Result<u32, String> {
                self.installs += 1;
                self.next += 1;
                Ok(self.next)
            }
            fn activate(&self, _model: u32) {}
            fn uninstall(&mut self, _model: u32) {}
        }

        let mut rig = to_nodes(&worship_def(), &drive_presets());

        // King of Tone ships two captures: both-sides, and red-as-boost.
        let kot = rig.node_named("King of Tone").expect("the pedal is a node");
        let kot_id = kot.id.clone();
        let captures: Vec<_> = kot
            .variants
            .iter()
            .map(|v| (v.name.clone(), v.id.clone()))
            .collect();
        assert!(captures.len() >= 2, "two settings of one pedal");

        // One chain variant per capture — the performable form.
        let pressable: Vec<_> = captures
            .iter()
            .map(|(name, variant)| {
                rig.with_selection(&format!("Clean · KoT {name}"), &kot_id, variant)
                    .expect("chain exists")
            })
            .collect();

        let mut bank = Counting::default();
        let patches = rig.install(&mut bank).expect("installs");
        let loaded_once = bank.installs;

        // Toggle between the pedal's captures, as a footswitch would.
        for _ in 0..10 {
            for (name, _) in &captures {
                let token = patches
                    .resident(&format!("Clean · KoT {name}"))
                    .expect("every capture is resident");
                PatchBank::press(&bank, token);
            }
        }

        assert_eq!(
            bank.installs, loaded_once,
            "twenty capture swaps and not one load"
        );
        assert_eq!(pressable.len(), captures.len());
    }

    /// The whole rig survives the format it is stored in.
    #[test]
    fn the_converted_rig_round_trips_through_styx() {
        let rig = to_nodes(&worship_def(), &drive_presets());
        let text = facet_styx::to_string(&rig.library).expect("serializes");
        let back: NodeLibrary = facet_styx::from_str(&text).expect("parses back");

        assert_eq!(back.nodes.len(), rig.library.nodes.len());
        let clean = rig.patch("Clean").expect("Clean");
        let (before, _) = resolve(&rig.library, &rig.chain, Some(clean)).expect("resolves");
        let (after, report) = resolve(&back, &rig.chain, Some(clean)).expect("resolves");
        assert!(report.is_clean());
        assert_eq!(
            before.leaves().len(),
            after.leaves().len(),
            "the same rig plays after a save and load"
        );
    }

    /// The library reproduces the rig, patch for patch and block for block.
    ///
    /// `build_profile` is the rig as it plays today; the node library is the
    /// rig as the domain holds it. This resolves every patch's variant, sends
    /// it through `from_node`, and compares the result against the chain
    /// `build_profile` builds for the same patch — names, types, capture
    /// paths and every parameter value.
    ///
    /// It is the check that makes adoption safe rather than hopeful: if this
    /// passes, the rig can be driven from the library without sounding any
    /// different.
    #[test]
    fn the_library_reproduces_every_patch_of_the_rig() {
        let def = worship_def();
        let drives = drive_presets();
        let rig = to_nodes(&def, &drives);
        let built = crate::profiles::build_profile(&def, &drives);

        for patch in &built.patches {
            let variant = rig.patch(&patch.name).unwrap_or_else(|| {
                panic!("{} has no variant", patch.name);
            });
            let (resolved, report) = resolve(&rig.library, &rig.chain, Some(variant))
                .unwrap_or_else(|e| panic!("{} did not resolve: {e}", patch.name));
            assert!(report.is_clean(), "{}: {report:?}", patch.name);

            let chain = signal_sampler::from_node::to_chain(&resolved);
            assert_eq!(
                chain.len(),
                patch.chain.len(),
                "{}: {} blocks from the library against {} from the builder\n  library: {:?}\n  builder: {:?}",
                patch.name,
                chain.len(),
                patch.chain.len(),
                chain
                    .iter()
                    .map(|b| b.display_name().to_string())
                    .collect::<Vec<_>>(),
                patch
                    .chain
                    .iter()
                    .map(|b| b.display_name().to_string())
                    .collect::<Vec<_>>(),
            );

            for (from_library, from_builder) in chain.iter().zip(patch.chain.iter()) {
                assert_eq!(
                    from_library.block_type,
                    from_builder.block_type,
                    "{}: block type at {}",
                    patch.name,
                    from_builder.display_name()
                );
                assert_eq!(
                    from_library.nam,
                    from_builder.nam,
                    "{}: capture for {}",
                    patch.name,
                    from_builder.display_name()
                );
                assert_eq!(
                    from_library.bypassed,
                    from_builder.bypassed,
                    "{}: bypass state of {} — the drive board comes up off",
                    patch.name,
                    from_builder.display_name()
                );
                for param in &from_builder.params {
                    let want: f32 = param.value.trim().parse().unwrap_or(0.0);
                    let got = from_library.param_f32(&param.name).unwrap_or_else(|| {
                        panic!(
                            "{}: {} lost parameter {}",
                            patch.name,
                            from_builder.display_name(),
                            param.name
                        )
                    });
                    assert!(
                        (got - want).abs() < want.abs() * 0.01 + 0.01,
                        "{}: {}.{} was {want} and came back {got}",
                        patch.name,
                        from_builder.display_name(),
                        param.name
                    );
                }
            }
        }
    }

    /// And the library understands the rig's parameters rather than carrying
    /// them as opaque strings — the measure of how much of the guitar rig the
    /// domain actually models.
    #[test]
    fn the_rigs_parameters_are_all_ranged() {
        let def = worship_def();
        let drives = drive_presets();
        let built = crate::profiles::build_profile(&def, &drives);

        let mut unranged = Vec::new();
        for patch in &built.patches {
            for block in &patch.chain {
                let (_, report) = signal_sampler::to_node::lift_one_block(block);
                unranged.extend(report.unranged);
            }
        }
        unranged.sort();
        unranged.dedup();

        // Exactly four, and they are the same finding four times: the empty
        // drive-board slots are `BlockType::Boost` / `BlockType::Drive`
        // placeholders carrying `("drive", "0.5")`, and neither type has any
        // native DSP — both build as `NativePassthrough`, which declares no
        // parameters at all.
        //
        // So there is no range to declare, because there is nothing for the
        // value to mean yet. Worth knowing on its own: that `drive` reaches
        // no DSP today, and has not since the placeholders were added. The
        // lift carries it verbatim, so it is there for whoever writes the
        // pedal, and this list is how we will notice when they do.
        assert_eq!(
            unranged,
            vec![
                ("Boost".to_string(), "drive".to_string()),
                ("Drive 1".to_string(), "drive".to_string()),
                ("Drive 2".to_string(), "drive".to_string()),
                ("Drive 3".to_string(), "drive".to_string()),
            ],
            "the only unrangeable values should be the placeholder pedals'"
        );
    }

    /// The rig that actually plays, reproduced from the node library.
    ///
    /// `worship_def()` is the hard-coded fallback for a machine with no
    /// config. What the rig loads is `default-config/profile.styx` — thirteen
    /// patches whose override values were captured by playing them, with
    /// rig-relative capture paths. It is a different and much less forgiving
    /// shape than the fallback: real dB and Hz values on real blocks, drive
    /// slots pointed at capture *option 1* rather than the first, and a
    /// `module Utility` label that matches no block category.
    ///
    /// If this passes, the node library is the guitar rig rather than a model
    /// of it.
    #[test]
    fn the_library_reproduces_the_shipped_rig() {
        let def: ProfileDef = facet_styx::from_str(crate::library::DEFAULT_PROFILE)
            .expect("the shipped profile parses");
        let drives: Vec<DrivePresetDef> = {
            #[derive(facet::Facet)]
            struct Presets {
                presets: Vec<DrivePresetDef>,
            }
            let parsed: Presets = facet_styx::from_str(crate::library::DEFAULT_DRIVE_PRESETS)
                .expect("the shipped drive presets parse");
            parsed.presets
        };

        assert_eq!(
            def.patches.len(),
            13,
            "the shipped rig has thirteen patches"
        );

        let rig = to_nodes(&def, &drives);
        let built = crate::profiles::build_profile(&def, &drives);

        for patch in &built.patches {
            let variant = rig
                .patch(&patch.name)
                .unwrap_or_else(|| panic!("{} has no variant", patch.name));
            let (resolved, report) = resolve(&rig.library, &rig.chain, Some(variant))
                .unwrap_or_else(|e| panic!("{} did not resolve: {e}", patch.name));
            assert!(report.is_clean(), "{}: {report:?}", patch.name);

            let chain = signal_sampler::from_node::to_chain(&resolved);
            assert_eq!(
                chain.len(),
                patch.chain.len(),
                "{}: block count from the library",
                patch.name
            );

            for (from_library, from_builder) in chain.iter().zip(patch.chain.iter()) {
                assert_eq!(
                    from_library.block_type,
                    from_builder.block_type,
                    "{}: block type at {}",
                    patch.name,
                    from_builder.display_name()
                );
                assert_eq!(
                    from_library.nam,
                    from_builder.nam,
                    "{}: capture for {}",
                    patch.name,
                    from_builder.display_name()
                );
                assert_eq!(
                    from_library.bypassed,
                    from_builder.bypassed,
                    "{}: bypass of {}",
                    patch.name,
                    from_builder.display_name()
                );
                for param in &from_builder.params {
                    let want: f32 = param.value.trim().parse().unwrap_or(0.0);
                    let got = from_library.param_f32(&param.name).unwrap_or_else(|| {
                        panic!(
                            "{}: {} lost parameter {}",
                            patch.name,
                            from_builder.display_name(),
                            param.name
                        )
                    });
                    assert!(
                        (got - want).abs() < want.abs().mul_add(0.01, 0.01),
                        "{}: {}.{} was {want} and came back {got}",
                        patch.name,
                        from_builder.display_name(),
                        param.name
                    );
                }
            }
        }
    }

    /// And the shipped rig's parameters are all ones the domain can range,
    /// bar the placeholder pedals whose block types have no DSP.
    #[test]
    fn the_shipped_rigs_parameters_are_ranged() {
        let (def, drives) = shipped();
        let built = crate::profiles::build_profile(&def, &drives);
        let mut unranged = Vec::new();
        for patch in &built.patches {
            for block in &patch.chain {
                let (_, report) = signal_sampler::to_node::lift_one_block(block);
                unranged.extend(report.unranged.into_iter().map(|(_, param)| param));
            }
        }
        unranged.sort();
        unranged.dedup();
        assert_eq!(
            unranged,
            vec!["drive".to_string()],
            "only the placeholder pedals' `drive` should be unrangeable"
        );
    }

    /// The adoption seam: what the library installs is what `build_profile`
    /// installs, for the rig that actually plays.
    ///
    /// `to_profile` is the one call a site swaps to make the node library the
    /// source of what sounds. This compares the two `RigProfile`s the live rig
    /// would receive — patch for patch, block for block, parameter for
    /// parameter, plus the trims and stacks a footswitch depends on.
    #[test]
    fn to_profile_matches_the_builder_for_the_shipped_rig() {
        let (def, drives) = shipped();
        let rig = to_nodes(&def, &drives);

        let from_nodes = rig.to_profile(&def);
        let from_builder = crate::profiles::build_profile(&def, &drives);

        assert_eq!(from_nodes.name, from_builder.name);
        assert_eq!(
            from_nodes.patches.len(),
            from_builder.patches.len(),
            "every patch installs"
        );

        for (nodes_patch, builder_patch) in from_nodes.patches.iter().zip(&from_builder.patches) {
            assert_eq!(nodes_patch.name, builder_patch.name);
            assert!(
                (nodes_patch.output_trim_db - builder_patch.output_trim_db).abs() < f32::EPSILON,
                "{}: output trim",
                nodes_patch.name
            );
            assert_eq!(
                nodes_patch.chain.len(),
                builder_patch.chain.len(),
                "{}: block count",
                nodes_patch.name
            );
            for (a, b) in nodes_patch.chain.iter().zip(&builder_patch.chain) {
                assert_eq!(a.block_type, b.block_type, "{}", nodes_patch.name);
                assert_eq!(a.nam, b.nam, "{}: capture", nodes_patch.name);
                assert_eq!(
                    a.bypassed,
                    b.bypassed,
                    "{}: bypass of {}",
                    nodes_patch.name,
                    b.display_name()
                );
                for param in &b.params {
                    let want: f32 = param.value.trim().parse().unwrap_or(0.0);
                    let got = a.param_f32(&param.name).unwrap_or_else(|| {
                        panic!(
                            "{}: {} lost {}",
                            nodes_patch.name,
                            b.display_name(),
                            param.name
                        )
                    });
                    assert!(
                        (got - want).abs() < want.abs().mul_add(0.01, 0.01),
                        "{}: {}.{} {want} vs {got}",
                        nodes_patch.name,
                        b.display_name(),
                        param.name
                    );
                }
            }
        }

        // The stacks a footswitch rotates through come across untouched.
        let names = |p: &signal_sampler::rig_profile::RigProfile| -> Vec<String> {
            p.stacks.iter().map(|s| s.name.clone()).collect()
        };
        assert_eq!(names(&from_nodes), names(&from_builder));
    }

    /// A pedal's captures are addressable as presets, and the choice maps
    /// back to the profile field that stores it.
    ///
    /// The library is derived from `ProfileDef`, so a selection that has no
    /// field to live in would be lost on the next rebuild. This is the lookup
    /// that decides which choices are real.
    #[test]
    fn a_pedals_captures_map_back_to_their_profile_slot() {
        let (def, drives) = shipped();
        let rig = to_nodes(&def, &drives);

        // The shipped rig wires King of Tone into Drive 1 at option 1.
        let kot = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "King of Tone")
            .expect("the pedal is a node");
        assert!(kot.variants.len() >= 2, "two captures to choose between");

        for (index, variant) in kot.variants.iter().enumerate() {
            let (slot, option) = rig
                .drive_option_for(kot.id.as_str(), variant.id.as_str())
                .unwrap_or_else(|| panic!("{} has no profile field", variant.name));
            assert_eq!(slot, "Drive 1", "the slot the profile assigns it to");
            assert_eq!(option, index, "option order follows the pedal's captures");
        }
    }

    /// A choice the profile cannot hold says so rather than being written
    /// somewhere it will not survive.
    #[test]
    fn a_choice_with_no_profile_field_is_refused() {
        let (def, drives) = shipped();
        let rig = to_nodes(&def, &drives);

        // The chain itself has variants — the patches — but they are not a
        // drive slot's captures, so there is no slot field for them.
        let chain = rig.library.get(&rig.chain).expect("the chain");
        let patch = chain.variants.last().expect("a patch variant");
        assert_eq!(
            rig.drive_option_for(chain.id.as_str(), patch.id.as_str()),
            None
        );

        // And an unknown pair is not guessed at.
        assert_eq!(rig.drive_option_for("nope", "nope"), None);
    }

    /// Node ids must survive a rebuild, and until this test they did not.
    ///
    /// Everything the node model promises rests on a node keeping its
    /// identity: a stored preset, an override addressed by id, a UI that
    /// names what to change. `to_nodes` is called afresh on every profile
    /// edit — a patch rename, a trim, a capture import — so an id minted per
    /// build is not an identity at all. A saved reference would dangle on the
    /// very next keystroke, silently.
    #[test]
    fn node_ids_survive_a_rebuild() {
        let (def, drives) = shipped();
        let first = to_nodes(&def, &drives);
        let second = to_nodes(&def, &drives);

        let ids = |rig: &RigNodes| -> Vec<(String, String)> {
            let mut out: Vec<(String, String)> = rig
                .library
                .nodes
                .iter()
                .map(|n| (n.name.clone(), n.id.as_str().to_string()))
                .collect();
            out.sort();
            out
        };
        assert_eq!(
            ids(&first),
            ids(&second),
            "the same profile built twice must name the same nodes"
        );

        // And the variants a preset is chosen by, for the same reason.
        let variants = |rig: &RigNodes| -> Vec<String> {
            let mut out: Vec<String> = rig
                .library
                .nodes
                .iter()
                .flat_map(|n| n.variants.iter().map(|v| v.id.as_str().to_string()))
                .collect();
            out.sort();
            out
        };
        assert_eq!(variants(&first), variants(&second), "variant ids too");
    }

    /// A drive slot can be filled with a different pedal — the choice the
    /// profile has always had (`DriveSlotDef::preset`) and no surface ever
    /// offered. Distinct from choosing a capture, which is a variant of the
    /// pedal already there.
    #[test]
    fn a_slot_offers_every_pedal_in_the_library() {
        let (def, drives) = shipped();
        let rig = to_nodes(&def, &drives);

        let pedals = rig.pedals();
        assert!(
            pedals.len() >= 2,
            "the library holds more pedals than the board uses: {pedals:?}"
        );

        // The pedal on Drive 1, and the slot it reports.
        let (_, on_board) = pedals
            .iter()
            .find(|(name, _)| name == "King of Tone")
            .expect("King of Tone is in the library");
        assert_eq!(
            rig.slot_of_node(on_board.as_str()).as_deref(),
            Some("Drive 1")
        );
        assert_eq!(
            rig.pedal_name(on_board.as_str()).as_deref(),
            Some("King of Tone"),
            "and the node maps back to the name the profile stores"
        );

        // A pedal the board is not using has no slot — it is an alternative,
        // not a fixture.
        let unassigned = pedals
            .iter()
            .find(|(_, id)| rig.slot_of_node(id.as_str()).is_none());
        if let Some((name, id)) = unassigned {
            assert!(
                rig.pedal_name(id.as_str()).is_some(),
                "{name} is still a pedal, just not on the board"
            );
        }
    }
}
