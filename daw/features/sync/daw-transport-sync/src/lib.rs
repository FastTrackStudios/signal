//! Keep several transports playing as one — for any DAW backend.
//!
//! Two machines pressing play at "the same moment" are never in step: their
//! clocks disagree, their audio devices start at different times, and their
//! sample clocks run at slightly different speeds. Played together that is
//! phasing — a comb filter that wanders. This crate is what closes the gap,
//! down to a few samples, and keeps it closed.
//!
//! # The pieces
//!
//! - [`clock`] — a monotonic, microsecond (sub-microsecond in `f64`) clock
//!   for this process: the time domain every other number here is in.
//! - [`buffer_clock`] — when each buffer really started: a delay-locked
//!   loop over callback times, so scheduler jitter does not become sync
//!   error ([`BufferClock`]).
//! - [`snapshot`] — what the audio thread knows at the start of each
//!   buffer: where the playhead is *and when that was* ([`AudioSnapshot`]),
//!   published lock-free ([`SnapshotCell`]). A playhead without its time is
//!   useless for sync; a UI poll is milliseconds stale.
//! - [`estimate`] — how far another machine's clock is from this one:
//!   NTP's four timestamps per exchange, smoothed with an interquartile
//!   mean and the tightest round trips favoured ([`ClockEstimator`]).
//! - [`position`] — a playhead at a known instant in some clock, projected
//!   to any other instant and carried between clock domains
//!   ([`Position`]).
//! - [`drift`] — the controller: given this engine's snapshot and the
//!   leader's position (both in one clock), a [`Correction`] — nudge the
//!   rate a fraction of a percent, jump when too far out, or leave it.
//! - [`gate`] — which snapshots a carrier should send: every change, and
//!   a keepalive between ([`PublishGate`]).
//! - [`backend`] — what a DAW implements to be kept in step
//!   ([`TransportBackend`]), and [`Follower`], which drives one.
//!
//! # A shared clock
//!
//! Pick one machine's clock as the session's (the host's): everyone else
//! estimates their offset to it by pinging it, and every position anyone
//! publishes is stamped in *that* clock. Then any two peers compare
//! directly, whoever leads — offsets compose through the shared clock, so
//! nobody needs a clock relation with anybody but the host.
//!
//! No transport here: the numbers are plain, and whoever carries them
//! (UDP on a LAN, QUIC across the internet) calls [`ClockEstimator::record`]
//! with the four stamps of each exchange.

pub mod backend;
pub mod buffer_clock;
pub mod clock;
pub mod drift;
pub mod estimate;
pub mod gate;
pub mod position;
pub mod snapshot;

pub use backend::{Follower, TransportBackend};
pub use buffer_clock::BufferClock;
pub use drift::{Correction, DriftConfig, DriftController};
pub use estimate::{ClockEstimator, RollingWindow};
pub use gate::PublishGate;
pub use position::Position;
pub use snapshot::{AudioSnapshot, ProjectId, SnapshotCell};
