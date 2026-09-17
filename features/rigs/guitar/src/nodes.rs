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
use signal_proto::node::{Combine, Content, Node, NodeId, NodeLibrary, Role, Variant};
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
}

impl RigNodes {
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
    let mut block = Block::from_parameters(Vec::new());
    block.kind = BlockKind::Nam(NamRef {
        model_path: path.to_string(),
        model_id: None,
    });
    Node::leaf(name, block_type, block)
}

/// Convert this crate's profile into the domain's node model.
///
/// Structure only — the native FX blocks the chain builder adds (compressor,
/// EQ, delays, reverbs) are not carried yet, because the audio-side bridge
/// still drops their parameters. What is carried is everything the patches
/// actually differ by: the amp, the drive board, and the overrides.
#[must_use]
pub fn to_nodes(def: &ProfileDef, drives: &[DrivePresetDef]) -> RigNodes {
    let mut library = NodeLibrary::new();

    // ── Amp captures: one node each, because they are different amps ─────
    let mut amps = Vec::new();
    for preset in &def.presets {
        let node = nam_leaf(&preset.name, &preset.nam, BlockType::Amp);
        amps.push((preset.name.clone(), node.id.clone()));
        library.insert(node);
    }

    // ── Pedals: one node each, its captures as variants ──────────────────
    let mut pedals = Vec::new();
    for pedal in drives {
        let Some(first) = pedal.options.first() else {
            continue;
        };
        let default_leaf = nam_leaf(&first.name, &first.nam, BlockType::Drive);
        let default_id = default_leaf.id.clone();
        library.insert(default_leaf);

        let mut node = Node::container(&pedal.name, Role::Module, Combine::Serial);
        if let Content::Children { nodes } = &mut node.content {
            nodes.push(default_id.clone());
        }
        // Its own default variant is the first capture; each later capture
        // swaps the node in that slot. One pedal, several settings.
        for option in pedal.options.iter().skip(1) {
            let leaf = nam_leaf(&option.name, &option.nam, BlockType::Drive);
            let mut variant = Variant::new(&option.name);
            variant.overrides.push(Override {
                path: NodePath::new(vec![NodePathSegment::Block {
                    id: default_id.as_str().to_string(),
                }]),
                op: NodeOverrideOp::ReplaceRef { id: leaf.id.as_str().to_string() },
            });
            library.insert(leaf);
            node.variants.push(variant);
        }
        pedals.push((pedal.name.clone(), node.id.clone()));
        library.insert(node);
    }

    // ── The chain: one node, with a slot per drive and one for the amp ───
    let mut chain = Node::container(&def.name, Role::Preset, Combine::Serial);
    let mut amp_slot = None;
    if let Content::Children { nodes } = &mut chain.content {
        for slot in crate::profiles::DRIVE_SLOTS {
            let assigned = def
                .drives
                .iter()
                .find(|d| d.block.eq_ignore_ascii_case(slot))
                .and_then(|d| {
                    pedals
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(&d.preset))
                });
            if let Some((_, id)) = assigned {
                nodes.push(id.clone());
            }
        }
        // The amp slot holds the first preset; every patch that wants a
        // different one swaps it.
        if let Some((_, id)) = amps.first() {
            nodes.push(id.clone());
            amp_slot = Some(id.clone());
        }
    }

    // ── Patches: variants of that one chain ──────────────────────────────
    let mut patches = Vec::new();
    for patch in &def.patches {
        let mut variant = Variant::new(&patch.name);
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
        variant
            .overrides
            .extend(patch.overrides.iter().filter_map(to_override));
        patches.push((patch.name.clone(), variant.id.clone()));
        chain.variants.push(variant);
    }

    let chain_id = chain.id.clone();
    library.insert(chain);
    RigNodes {
        library,
        chain: chain_id,
        patches,
    }
}

/// This crate's flat override into the domain's path-and-op form.
///
/// Returns `None` for an op this rig writes but the domain expresses another
/// way — `set_text` swaps an asset, which is a `ReplaceRef` against a node,
/// not a parameter write.
fn to_override(ov: &OverrideDef) -> Option<Override> {
    let mut segments = Vec::new();
    if !ov.module.is_empty() {
        segments.push(NodePathSegment::Module {
            id: ov.module.clone(),
        });
    }
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
            Some(Override::set(NodePath::new(segments), ov.value))
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

    fn nam_of(node: &signal_proto::node_resolve::Resolved) -> Option<&str> {
        match &node.content {
            ResolvedContent::Leaf { block, .. } => match &block.kind {
                BlockKind::Nam(nam) => Some(nam.model_path.as_str()),
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
        let leaves = resolved.leaves();
        let amp = leaves.last().expect("the chain ends in the amp");
        assert!(
            nam_of(amp).is_some_and(|p| p.contains("Fender")),
            "Clean points at the Fender capture, got {:?}",
            nam_of(amp)
        );

        let ambient = rig.patch("Ambient").expect("Ambient exists");
        let (resolved, _) = resolve(&rig.library, &rig.chain, Some(ambient)).expect("resolves");
        let amp = resolved.leaves().last().copied().expect("amp");
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

        let amp = chain.last().expect("the chain ends in the amp");
        assert_eq!(amp.block_type, signal_proto::block::BlockType::Amp);
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
            fn install(
                &mut self,
                _blocks: &[signal_sampler::RigBlock],
            ) -> Result<u32, String> {
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
}
