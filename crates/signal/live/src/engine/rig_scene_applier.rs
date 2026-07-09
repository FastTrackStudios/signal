//! Trait for switching between preloaded rig scene hierarchies in a DAW.
//!
//! The controller calls [`RigSceneApplier::switch_scene`] when a patch
//! targets a `PatchTarget::RigScene`. Implementations handle the actual
//! track hierarchy muting/unmuting for gapless scene switching.

use std::future::Future;
use std::pin::Pin;

/// Error type for rig scene switching failures.
#[derive(Debug)]
pub enum RigSceneApplyError {
    /// No target rig configured or input track not found.
    NoTarget(String),
    /// DAW communication failed.
    DawError(String),
    /// Failed to load a scene hierarchy.
    LoadError(String),
}

impl std::fmt::Display for RigSceneApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RigSceneApplyError::NoTarget(msg) => write!(f, "no target: {msg}"),
            RigSceneApplyError::DawError(msg) => write!(f, "DAW error: {msg}"),
            RigSceneApplyError::LoadError(msg) => write!(f, "load error: {msg}"),
        }
    }
}

impl std::error::Error for RigSceneApplyError {}

/// Abstracts switching between preloaded rig scene track hierarchies.
///
/// Implementations should:
/// 1. Mute the send from the input track to the current scene's rig folder
/// 2. Unmute the preloaded scene's rig folder and its send
/// 3. Schedule a delayed folder mute on the old scene for reverb tail ring-out
///
/// Returns `true` if the switch was performed, `false` if the scene
/// wasn't ready (e.g., still preloading).
pub trait RigSceneApplier: Send + Sync {
    fn switch_scene<'a>(
        &'a self,
        rig_id: &'a str,
        scene_id: &'a str,
        scene_name: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<bool, RigSceneApplyError>> + Send + 'a>>;
}
