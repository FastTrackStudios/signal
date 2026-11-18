//! Activating a patch that targets a node — the controller's half of the
//! migration off the five-level hierarchy.
//!
//! [`ProfileOps::activate_patch`](super::profiles::ProfileOps::activate_patch)
//! resolves through `resolve_service` into a
//! [`ResolvedGraph`](signal_proto::resolve::ResolvedGraph) and hands that to a
//! DAW applier. A node tree is not a `ResolvedGraph` and should not be bent
//! into one, so a node target activates here instead.
//!
//! # The node library arrives as an argument
//!
//! `SignalApi` bundles nine service traits, and every implementor would break
//! if it grew a tenth. That is not the only reason though: the node library is
//! a genuinely different store. The live rig keeps it as styx and never opens
//! a database at all, so it cannot be a service the controller is constructed
//! with. Passing the repository per call keeps both callers honest.
//!
//! # The DAW half
//!
//! This used to stop before the applier, because `apply_graph` takes a
//! `&ResolvedGraph` and handing it a fabricated one would report success for
//! a rig REAPER never received.
//!
//! `DawPatchApplier::apply_node` exists now, so a node target is applied the
//! same way a graph target is, and `applied_to_daw` means what it says. An
//! applier that has not been taught nodes refuses rather than doing nothing
//! quietly, and that refusal lands here as `applied_to_daw: false` — which is
//! still the truth.

use signal_live::node_service::{NodeResolveError, resolve_node};
use signal_proto::node_resolve::{Report, Resolved};
use signal_proto::profile::{PatchId, PatchTarget, ProfileId};
use signal_storage::node_repo::NodeRepo;

use super::error::OpsError;
use crate::events;
use crate::{SignalApi, SignalController};

/// Handle for node-targeted operations.
pub struct NodeOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> NodeOps<S> {
    /// Activate a patch whose target is a node.
    ///
    /// Returns the resolved tree and its [`Report`] — a rig missing one block
    /// still plays, and the caller shows what is missing rather than being
    /// handed an error for the whole thing.
    ///
    /// # Errors
    ///
    /// [`OpsError::NotFound`] / [`OpsError::VariantNotFound`] if the profile
    /// or patch is absent, or [`OpsError::Node`] if the library cannot be read
    /// or the patch does not target a node — a caller mistake, named rather
    /// than silently doing nothing.
    pub async fn activate_patch<R: NodeRepo + ?Sized>(
        &self,
        repo: &R,
        profile_id: &ProfileId,
        patch_id: &PatchId,
    ) -> Result<(Resolved, Report), OpsError> {
        let profile = self
            .0
            .service
            .load_profile(profile_id.clone())
            .await
            .map_err(OpsError::Storage)?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "profile",
                id: profile_id.to_string(),
            })?;

        let patch = profile
            .patches
            .iter()
            .find(|p| &p.id == patch_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "profile",
                parent_id: profile_id.to_string(),
                variant_id: patch_id.to_string(),
            })?;

        let PatchTarget::Node { node, variant } = &patch.target else {
            return Err(OpsError::Node(format!(
                "patch '{}' does not target a node; activate it through \
                 ProfileOps::activate_patch",
                patch.name
            )));
        };

        let resolved = resolve_node(repo, node, Some(variant))
            .await
            .map_err(|e: NodeResolveError| OpsError::Node(e.to_string()))?;

        // Clone out of the lock in its own statement (guard-across-await).
        let applier = self
            .0
            .daw_applier
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let applied_to_daw = if let Some(applier) = applier {
            match applier
                .apply_node(&resolved.0, Some(patch.name.as_str()))
                .await
            {
                Ok(applied) => applied,
                Err(e) => {
                    // One warn line for a refusal, and the event carries the
                    // outcome — the rig keeps playing whatever it has.
                    tracing::warn!(
                        patch.name = %patch.name,
                        error = %e,
                        "signal: node patch not applied to the DAW"
                    );
                    false
                }
            }
        } else {
            false
        };

        self.0.event_bus.emit(events::SignalEvent::PatchActivated {
            profile_id: profile_id.to_string(),
            patch_id: patch_id.to_string(),
            applied_to_daw,
        });

        Ok(resolved)
    }

    /// Resolve a node directly, without a profile — what a browser or an
    /// editor previewing a preset wants.
    ///
    /// # Errors
    ///
    /// [`OpsError::Node`] if the library cannot be read, or the node is
    /// absent or self-referencing.
    pub async fn resolve<R: NodeRepo + ?Sized>(
        &self,
        repo: &R,
        node: &signal_proto::node::NodeId,
        variant: Option<&signal_proto::node::VariantId>,
    ) -> Result<(Resolved, Report), OpsError> {
        resolve_node(repo, node, variant)
            .await
            .map_err(|e| OpsError::Node(e.to_string()))
    }
}
