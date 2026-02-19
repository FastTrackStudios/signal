//! Module slot state machine and trait.
//!
//! A `ModuleSlot` manages N parallel plugin instances for one processing stage
//! (e.g. Drive, Amp, EQ). The lifecycle flows:
//!
//! ```text
//! Loading → Ready → Active → Tailing → Unloaded
//!                     ↑          │
//!                     └──────────┘  (re-activate if needed)
//! ```
//!
//! Gapless switching: `load()` → `activate()` → old Active becomes Tailing →
//! `cleanup_tails()` reclaims silent instances.

use signal_proto::module_type::ModuleType;
use signal_proto::ModuleSnapshot;

use super::error::EngineError;
use super::target::ModuleTarget;

/// Opaque handle to a plugin instance within a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceHandle(pub u64);

impl InstanceHandle {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Lifecycle state of a single plugin instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InstanceState {
    /// FX chain is being built / plugins are loading.
    Loading = 0,
    /// Fully loaded, output muted, waiting to be activated.
    Ready = 1,
    /// Live output — this instance is the active sound for this slot.
    Active = 2,
    /// Previous instance whose tail (reverb/delay) is still ringing out.
    /// Send is ramping to -inf.
    Tailing = 3,
    /// Resources freed, instance can be reclaimed.
    Unloaded = 4,
}

impl InstanceState {
    /// Instance has resources allocated (not yet freed).
    pub fn is_alive(self) -> bool {
        matches!(
            self,
            Self::Loading | Self::Ready | Self::Active | Self::Tailing
        )
    }

    /// Instance is producing audio output.
    pub fn is_audible(self) -> bool {
        matches!(self, Self::Active | Self::Tailing)
    }
}

/// Result of loading a new instance into a slot.
#[derive(Debug)]
pub enum LoadResult {
    /// A new instance was created and is now Loading/Ready.
    Loaded(InstanceHandle),
    /// The exact same preset+snapshot is already loaded — no work needed.
    AlreadyLoaded(InstanceHandle),
    /// Load failed.
    Failed(EngineError),
}

/// Result of activating a Ready instance.
#[derive(Debug)]
pub enum ActivateResult {
    /// The instance is now Active. Previous Active (if any) moved to Tailing.
    Activated {
        new_active: InstanceHandle,
        previous: Option<InstanceHandle>,
    },
    /// Activation failed.
    Failed(EngineError),
}

/// Manages parallel plugin instances for one processing stage.
///
/// Implementors handle the DAW-specific FX chain manipulation. The trait
/// defines the gapless switching protocol that `RigEngine` orchestrates.
#[allow(async_fn_in_trait)]
pub trait ModuleSlot: Send + Sync {
    /// Which processing stage this slot manages.
    fn module_type(&self) -> ModuleType;

    /// Load a new instance: Loading → Ready.
    ///
    /// Returns `AlreadyLoaded` if the exact preset+snapshot is already in
    /// Ready or Active state (avoids redundant plugin loads).
    async fn load(&self, target: &ModuleTarget) -> LoadResult;

    /// Promote a Ready instance to Active. Previous Active → Tailing.
    async fn activate(&self, handle: InstanceHandle) -> ActivateResult;

    /// Apply parameter changes to the Active instance without switching.
    async fn apply_snapshot(
        &self,
        handle: InstanceHandle,
        snapshot: &ModuleSnapshot,
    ) -> Result<(), EngineError>;

    /// Query the state of a specific instance.
    fn instance_state(&self, handle: InstanceHandle) -> Option<InstanceState>;

    /// Get the currently Active instance (if any).
    fn active_instance(&self) -> Option<InstanceHandle>;

    /// All live instances with their states.
    fn loaded_instances(&self) -> Vec<(InstanceHandle, InstanceState)>;

    /// Free an instance's resources: any state → Unloaded.
    async fn unload(&self, handle: InstanceHandle) -> Result<(), EngineError>;

    /// Called from `tick()` — reclaims Tailing instances that have gone silent.
    async fn cleanup_tails(&self);

    /// Mute all output from this slot.
    async fn disable(&self);

    /// Un-mute this slot.
    async fn enable(&self);

    /// Whether the slot is currently disabled (bypassed/muted).
    fn is_disabled(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_state_is_alive() {
        assert!(InstanceState::Loading.is_alive());
        assert!(InstanceState::Ready.is_alive());
        assert!(InstanceState::Active.is_alive());
        assert!(InstanceState::Tailing.is_alive());
        assert!(!InstanceState::Unloaded.is_alive());
    }

    #[test]
    fn instance_state_is_audible() {
        assert!(!InstanceState::Loading.is_audible());
        assert!(!InstanceState::Ready.is_audible());
        assert!(InstanceState::Active.is_audible());
        assert!(InstanceState::Tailing.is_audible());
        assert!(!InstanceState::Unloaded.is_audible());
    }

    #[test]
    fn instance_handle_equality() {
        let a = InstanceHandle::new(42);
        let b = InstanceHandle::new(42);
        let c = InstanceHandle::new(99);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
