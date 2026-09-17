//! Switching that **cannot** cause a gap, because the types do not allow one.
//!
//! A gap happens for exactly one reason: something had to be loaded at the
//! moment of the switch. Avoiding that by being careful does not hold — the
//! rig's own `set_block_option` is careful and still rebuilds, and its comment
//! says so ("a full reload (brief gap)"). Care is not a guarantee.
//!
//! So the guarantee is structural. The two phases are separated in the type
//! system, and the switching half is given no way to express a load:
//!
//! | phase | when | may allocate | may fail |
//! |---|---|---|---|
//! | [`VariantBank::install_all`] | edit time | yes | yes |
//! | [`VariantBank::activate`] | on stage | **no** | **no** |
//!
//! Three things make that hold, and all three are load-bearing:
//!
//! 1. **[`Resident`] has no public constructor.** The only way to obtain one
//!    is to install a variant, so a token cannot name a chain that is not in
//!    memory. `ModelId` is a bare `u32` and `set_active(Some(999))` compiles;
//!    `activate` cannot be written that way.
//! 2. **[`activate`](VariantBank::activate) returns `()`.** There is no
//!    failure to report, because the only thing that could fail — finding and
//!    preparing the chain — already happened.
//! 3. **It takes `&B`, not `&mut B`.** It cannot install, uninstall, or
//!    mutate the bank, so it cannot be quietly extended into something that
//!    loads.
//!
//! If a call to `activate` compiles, the chain it names is already resident
//! and the switch is a pointer swap. That is the whole design.
//!
//! # What this is for
//!
//! A Block Preset's variants — every AC30 capture, every gain setting of a
//! pedal. The model says those are Snapshots, which must be free to recall
//! (see the README on Snapshot versus Scene). A NAM capture has no parameter
//! to move, so recalling one *is* a load — and the way to keep the Snapshot
//! guarantee is to move that load to load time. This is where that happens.

use signal_proto::node::{NodeId, NodeLibrary, VariantId};
use signal_proto::node_resolve::{ResolveError, resolve};

use crate::from_node::to_chain;
use crate::rig::{ModelId, RigBlock};

/// Somewhere chains can be made resident and switched between.
///
/// A trait so the guarantee can be tested without opening an audio device —
/// and so a future backend inherits it rather than reimplementing it.
pub trait Bank {
    /// Make a chain resident and prepared. Loads from disk, allocates.
    ///
    /// # Errors
    ///
    /// Returns an error if any block fails to load or prepare.
    fn install(&mut self, blocks: &[RigBlock]) -> Result<ModelId, String>;

    /// Make an already-resident chain the active one. A pointer swap.
    fn activate(&self, model: ModelId);

    /// Release a resident chain.
    fn uninstall(&mut self, model: ModelId);
}

#[cfg(not(target_arch = "wasm32"))]
impl Bank for crate::rig::GuitarRig {
    fn install(&mut self, blocks: &[RigBlock]) -> Result<ModelId, String> {
        self.install_chain(blocks)
    }

    fn activate(&self, model: ModelId) {
        self.set_active(Some(model));
    }

    fn uninstall(&mut self, model: ModelId) {
        self.uninstall_model(model);
    }
}

/// Proof that one variant is resident and ready to sound.
///
/// Deliberately opaque: the inner handle is private and there is no
/// constructor, so the only way to hold one is to have installed it. That is
/// what makes [`VariantBank::activate`] unable to ask for a load.
///
/// A token cannot be forged, and the compiler is the one enforcing it:
///
/// ```compile_fail
/// use signal_sampler::gapless::Resident;
/// // `model` is private and there is no constructor — so there is no way to
/// // name a chain that was never installed.
/// let forged = Resident { model: 999 };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resident {
    model: ModelId,
}

/// What went wrong while installing — all of it at edit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The node could not be resolved at all (absent, or a reference cycle).
    Unresolvable(ResolveError),
    /// One variant's chain would not load. Names the variant, because the
    /// player needs to know which capture is the problem.
    Variant { variant: VariantId, reason: String },
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unresolvable(e) => write!(f, "{e}"),
            Self::Variant { variant, reason } => {
                write!(f, "variant {}: {reason}", variant.as_str())
            }
        }
    }
}

impl std::error::Error for InstallError {}

/// Every variant of one node, resident together.
///
/// Built once, switched between many times. Holding this is what makes a
/// Snapshot free to recall.
#[derive(Debug)]
pub struct VariantBank {
    node: NodeId,
    resident: Vec<(VariantId, Resident)>,
}

impl VariantBank {
    /// Resolve and install **every** variant of `node`.
    ///
    /// All of them, not the ones expected to be used: a variant left out is a
    /// variant that would have to load later, which is the gap this exists to
    /// prevent. The cost is memory, paid knowingly and up front.
    ///
    /// # Errors
    ///
    /// [`InstallError::Unresolvable`] if the node cannot be resolved at all,
    /// or [`InstallError::Variant`] naming the first variant whose chain will
    /// not load.
    pub fn install_all<B: Bank>(
        bank: &mut B,
        library: &NodeLibrary,
        node: &NodeId,
    ) -> Result<Self, InstallError> {
        let variants: Vec<VariantId> = library
            .get(node)
            .map(|n| n.variants.iter().map(|v| v.id.clone()).collect())
            .unwrap_or_default();

        let mut resident = Vec::with_capacity(variants.len());
        for variant in variants {
            let (tree, _report) =
                resolve(library, node, Some(&variant)).map_err(InstallError::Unresolvable)?;
            let model = bank
                .install(&to_chain(&tree))
                .map_err(|reason| InstallError::Variant {
                    variant: variant.clone(),
                    reason,
                })?;
            resident.push((variant, Resident { model }));
        }
        Ok(Self {
            node: node.clone(),
            resident,
        })
    }

    /// The token for a variant, or `None` if it was never installed.
    ///
    /// The lookup that can fail happens **here**, off the switch path. By the
    /// time anything holds a [`Resident`], the question "is it loaded?" has
    /// already been answered yes.
    #[must_use]
    pub fn resident(&self, variant: &VariantId) -> Option<Resident> {
        self.resident
            .iter()
            .find(|(id, _)| id == variant)
            .map(|(_, r)| *r)
    }

    /// Switch to a resident variant.
    ///
    /// Infallible and load-free *by type*. The only argument naming a chain is
    /// a [`Resident`], which nothing outside [`install_all`](Self::install_all)
    /// can mint; the receiver is shared, so nothing here can install. There is
    /// no way to spell this call that requires a load, and therefore no way to
    /// spell one that drops audio.
    pub fn activate<B: Bank>(bank: &B, resident: Resident) {
        bank.activate(resident.model);
    }

    /// The node these variants belong to.
    #[must_use]
    pub const fn node(&self) -> &NodeId {
        &self.node
    }

    /// How many variants are resident.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.resident.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.resident.is_empty()
    }

    /// Release every chain. Edit time, like installing.
    pub fn uninstall_all<B: Bank>(self, bank: &mut B) {
        for (_, r) in self.resident {
            bank.uninstall(r.model);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::block::BlockType;
    use signal_proto::block_kind::{BlockKind, NamRef};
    use signal_proto::model::Block;
    use signal_proto::node::{Combine, Node, Role, Variant};
    use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

    /// A bank that counts what it was asked to do, so a test can assert that
    /// switching loads nothing.
    #[derive(Default)]
    struct Counting {
        installs: usize,
        activations: usize,
        active: Option<ModelId>,
        next: ModelId,
        fail_on: Option<usize>,
    }

    impl Bank for Counting {
        fn install(&mut self, _blocks: &[RigBlock]) -> Result<ModelId, String> {
            if self.fail_on == Some(self.installs) {
                return Err("model would not load".into());
            }
            self.installs += 1;
            self.next += 1;
            Ok(self.next)
        }

        fn activate(&self, _model: ModelId) {
            // Nothing to record. `&self` is the point: activation cannot
            // mutate the bank, which is half of why it cannot load.
        }

        fn uninstall(&mut self, _model: ModelId) {}
    }

    fn capture(name: &str, path: &str) -> Node {
        let mut block = Block::from_parameters(Vec::new());
        block.kind = BlockKind::Nam(NamRef {
            model_path: path.into(),
            model_id: None,
        });
        Node::leaf(name, BlockType::Amp, block)
    }

    /// An AC30 block preset with three captures, as a Block Preset's variants.
    fn ac30_with_three_captures() -> (NodeLibrary, NodeId, Vec<VariantId>) {
        let mut lib = NodeLibrary::new();
        let edge = capture("Edge of Breakup", "models/edge.nam");
        let full = capture("Full", "models/full.nam");
        let edge_id = edge.id.clone();

        let mut node = Node::container("AC30", Role::Module, Combine::Serial).with_child(&edge);
        let mut swap = Variant::new("Full");
        swap.overrides.push(Override {
            path: NodePath::new(vec![NodePathSegment::Block {
                id: edge_id.as_str().to_string(),
            }]),
            op: NodeOverrideOp::ReplaceRef {
                id: full.id.as_str().to_string(),
            },
        });
        node.variants.push(swap);

        let ids: Vec<VariantId> = node.variants.iter().map(|v| v.id.clone()).collect();
        let node_id = node.id.clone();
        lib.insert(edge);
        lib.insert(full);
        lib.insert(node);
        (lib, node_id, ids)
    }

    #[test]
    fn every_variant_is_installed_up_front() {
        let (lib, node, variants) = ac30_with_three_captures();
        let mut bank = Counting::default();
        let installed = VariantBank::install_all(&mut bank, &lib, &node).expect("installs");

        assert_eq!(installed.len(), variants.len());
        assert_eq!(
            bank.installs,
            variants.len(),
            "all of them, not the ones expected to be used"
        );
        for v in &variants {
            assert!(installed.resident(v).is_some(), "every variant has a token");
        }
    }

    /// The property the whole module exists for: switching loads nothing.
    #[test]
    fn switching_installs_nothing() {
        let (lib, node, variants) = ac30_with_three_captures();
        let mut bank = Counting::default();
        let installed = VariantBank::install_all(&mut bank, &lib, &node).expect("installs");
        let after_install = bank.installs;

        // Switch back and forth as a footswitch would.
        for _ in 0..10 {
            for v in &variants {
                let token = installed.resident(v).expect("resident");
                VariantBank::activate(&bank, token);
            }
        }

        assert_eq!(
            bank.installs, after_install,
            "twenty switches loaded nothing — the load happened once, at install"
        );
    }

    /// A variant that was never installed cannot be switched to, because the
    /// lookup that would name it happens off the switch path and returns
    /// `None` rather than a token.
    #[test]
    fn an_uninstalled_variant_yields_no_token() {
        let (lib, node, _) = ac30_with_three_captures();
        let mut bank = Counting::default();
        let installed = VariantBank::install_all(&mut bank, &lib, &node).expect("installs");

        assert!(
            installed.resident(&VariantId::new()).is_none(),
            "there is no token for something never installed, so `activate` \
             cannot be called for it"
        );
    }

    /// Failure is an edit-time event, and it names the variant so a player
    /// knows which capture is the problem.
    #[test]
    fn a_capture_that_will_not_load_fails_at_install_time() {
        let (lib, node, _) = ac30_with_three_captures();
        let mut bank = Counting {
            fail_on: Some(1),
            ..Counting::default()
        };
        let err = VariantBank::install_all(&mut bank, &lib, &node).expect_err("should fail");
        assert!(
            matches!(err, InstallError::Variant { .. }),
            "and it says which one: {err}"
        );
    }
}
