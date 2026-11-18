//! Rig engine trait — orchestrates all module slots for scene transitions.
//!
//! The `RigEngine` is the top-level coordinator: it takes a scene reference,
//! resolves what each slot needs, computes diffs, and executes transitions
//! across all slots. It also manages preloading and periodic maintenance.

use signal_proto::ModuleSnapshot;
use signal_proto::module_type::ModuleType;

use super::error::EngineError;

/// Handle to track a multi-slot preload operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PresetLoadHandle(pub u64);

/// Progress of a multi-slot preload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetReadiness {
    /// All slots are Ready.
    Ready,
    /// Some slots are still Loading.
    Loading { loaded: u16, total: u16 },
    /// At least one slot failed to load.
    Failed,
}

/// Priority for background preloading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PreloadPriority {
    /// Next scene in current song — must be near-instant.
    Critical = 0,
    /// Previous + other scenes in current song.
    High = 1,
    /// Next/prev song's first scene.
    Medium = 2,
    /// Browsing presets in profile.
    Low = 3,
}

/// Outcome of a scene transition attempt.
#[derive(Debug)]
pub enum SwitchOutcome {
    /// All slots transitioned successfully.
    Completed,
    /// Some slots are still loading — poll with `check_readiness`.
    Pending {
        handle: PresetLoadHandle,
        readiness: PresetReadiness,
    },
    /// Transition failed entirely.
    Failed { reason: String },
}

/// Result of a `load_scene` operation, including per-slot errors.
#[derive(Debug)]
pub struct TransitionResult {
    /// Overall outcome of the transition.
    pub outcome: SwitchOutcome,
    /// Non-fatal errors from individual slots (the transition may still succeed
    /// even if some slots have issues).
    pub slot_errors: Vec<(ModuleType, EngineError)>,
}

impl TransitionResult {
    #[must_use]
    pub const fn completed() -> Self {
        Self {
            outcome: SwitchOutcome::Completed,
            slot_errors: Vec::new(),
        }
    }

    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            outcome: SwitchOutcome::Failed {
                reason: reason.into(),
            },
            slot_errors: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self.outcome, SwitchOutcome::Completed)
    }

    #[must_use]
    pub const fn has_errors(&self) -> bool {
        !self.slot_errors.is_empty()
    }
}

/// Top-level engine that orchestrates all module slots for a rig.
///
/// Scene transitions flow through: resolve → diff → execute per-slot.
/// The engine also manages preloading and periodic tail cleanup.
#[expect(async_fn_in_trait)]
pub trait RigEngine: Send + Sync {
    /// Apply parameter changes to a specific slot (no instance switch).
    async fn apply_snapshot(
        &self,
        module_type: ModuleType,
        snapshot: &ModuleSnapshot,
    ) -> Result<(), EngineError>;

    /// Check the readiness of a preload operation.
    fn check_readiness(&self, handle: PresetLoadHandle) -> PresetReadiness;

    /// Wait until a preload operation completes.
    async fn wait_ready(&self, handle: PresetLoadHandle);

    /// Query the number of active slots.
    fn slot_count(&self) -> usize;

    /// Query which module types have active slots.
    fn active_module_types(&self) -> Vec<ModuleType>;

    /// Periodic maintenance tick (~60Hz): process preload queue, cleanup tails.
    async fn tick(&self);

    /// Shut down all slots and free resources.
    async fn shutdown(&self);
}

/// Time-based animation driver for morph transitions.
///
/// Advances elapsed time each frame and produces an eased `t ∈ [0, 1]`.
#[derive(Debug, Clone)]
pub struct SnapshotTween {
    pub duration_ms: f64,
    pub curve: signal_proto::easing::EasingCurve,
    elapsed_ms: f64,
    state: TweenState,
}

/// State of a tween animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweenState {
    /// Not started or reset.
    Idle,
    /// Actively animating.
    Running,
    /// Animation finished (t = 1.0).
    Complete,
}

impl SnapshotTween {
    #[must_use]
    pub const fn new(duration_ms: f64, curve: signal_proto::easing::EasingCurve) -> Self {
        Self {
            duration_ms,
            curve,
            elapsed_ms: 0.0,
            state: TweenState::Idle,
        }
    }

    /// Start or restart the tween.
    pub const fn start(&mut self) {
        self.elapsed_ms = 0.0;
        self.state = TweenState::Running;
    }

    /// Advance by `delta_ms` and return the eased value.
    pub fn advance(&mut self, delta_ms: f64) -> f64 {
        if self.state != TweenState::Running {
            return if self.state == TweenState::Complete {
                1.0
            } else {
                0.0
            };
        }

        self.elapsed_ms += delta_ms;

        if self.elapsed_ms >= self.duration_ms {
            self.elapsed_ms = self.duration_ms;
            self.state = TweenState::Complete;
        }

        let t = if self.duration_ms > 0.0 {
            self.elapsed_ms / self.duration_ms
        } else {
            1.0
        };

        self.curve.apply(t)
    }

    #[must_use]
    pub const fn state(&self) -> TweenState {
        self.state
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state == TweenState::Running
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.state == TweenState::Complete
    }

    /// Reset to idle state.
    pub const fn reset(&mut self) {
        self.elapsed_ms = 0.0;
        self.state = TweenState::Idle;
    }
}
