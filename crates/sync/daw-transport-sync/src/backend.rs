//! What a DAW implements to be kept in step, and the loop that keeps it.

use crate::drift::{Correction, DriftController};
use crate::{AudioSnapshot, Position};

/// A transport that can be kept in step with another — REAPER, the
/// standalone engine, anything with an audio callback.
///
/// Times are in the backend's own sync clock ([`crate::clock`] for an
/// in-process engine; REAPER's `time_precise` for REAPER), microseconds.
pub trait TransportBackend {
    /// The audio thread's latest snapshot; `None` until the device runs.
    fn snapshot(&self) -> Option<AudioSnapshot>;

    /// Run at `rate` (1.0 = nominal) — resampled, so the pitch moves by
    /// the same tiny fraction, never a jump in the audio.
    fn set_rate(&self, rate: f64);

    /// Be at `position` (seconds) at `at_micros`, playing or not, at
    /// `rate`. A backend that can schedule lands on the exact sample; one
    /// that cannot should apply it at once at `position` moved on by the
    /// time until `at_micros` (negative time: already late).
    fn locate_at(&self, at_micros: f64, position: f64, playing: bool, rate: f64);

    /// Stop, resting at `position`.
    fn stop(&self, position: f64);
}

/// Keeps one backend on a leader.
///
/// Every position the leader publishes is stamped in a *shared* clock;
/// `offset_micros` is that clock minus this backend's (from a
/// [`crate::ClockEstimator`] against whoever owns the shared clock — zero
/// if it is this one).
#[derive(Clone, Debug, Default)]
pub struct Follower {
    controller: DriftController,
}

impl Follower {
    #[must_use]
    pub const fn new(controller: DriftController) -> Self {
        Self { controller }
    }

    #[must_use]
    pub const fn controller(&self) -> &DriftController {
        &self.controller
    }

    /// Start over (a new leader, a new song, leading ourselves now).
    pub fn reset(&mut self) {
        self.controller.reset();
    }

    /// One step: read the backend, compare with `leader` (shared clock),
    /// act. `now_local_micros` is the backend's clock now. Returns what
    /// was done, for diagnostics.
    pub fn tick(
        &mut self,
        backend: &dyn TransportBackend,
        leader: &Position,
        offset_micros: f64,
        now_local_micros: f64,
    ) -> Correction {
        let Some(mut local) = backend.snapshot() else {
            return Correction::Hold;
        };
        local.host_micros += offset_micros;
        let correction = self
            .controller
            .step(&local, leader, now_local_micros + offset_micros);
        match correction {
            Correction::Hold => {}
            Correction::Rate(rate) => backend.set_rate(rate),
            Correction::Locate {
                at_micros,
                position,
                playing,
                rate,
            } => {
                backend.locate_at(at_micros - offset_micros, position, playing, rate);
            }
            Correction::Stop { position } => backend.stop(position),
        }
        correction
    }
}
