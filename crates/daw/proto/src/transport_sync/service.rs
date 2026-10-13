//! The `TransportSync` service: a clock to ping and a stream of
//! clock-stamped playhead positions.

use super::StampedPosition;
use crate::ProjectContext;

/// Follow a backend's transport to the sample from another process.
///
/// A remote follower needs two things: how far the server's sync clock
/// is from its own (ping [`clock_now`](Self::clock_now) — NTP's four
/// stamps with the server's reading as both t2 and t3, fed to a
/// `daw_transport_sync::ClockEstimator`), and where the leader's
/// playhead is at a known instant of that clock (the
/// [`positions`](Self::positions) stream). `daw_control`'s
/// `TransportLeader` does both and hands a `daw_transport_sync::Follower`
/// what it needs.
///
/// `clock_now` is `async` so it is answered on the connection's own
/// task, not marshalled onto a backend's main thread (REAPER's is a
/// ~30 Hz timer — a ping answered there would stamp a random moment
/// inside the round trip and the offset would be off by up to half of
/// it).
#[architect::rpc]
pub trait TransportSync {
    /// The server's sync clock now, microseconds — the clock every
    /// [`StampedPosition::host_micros`] is in (daw-transport-sync's
    /// process clock for daw-standalone, REAPER's `time_precise` for
    /// REAPER).
    async fn clock_now(&self) -> f64;

    /// The project's latest stamped position, or `None` when no audio
    /// buffer has run for it yet (no device, no engine, unknown
    /// project).
    fn snapshot(&self, project: ProjectContext) -> Option<StampedPosition>;

    /// Stamped positions for every project with a running transport,
    /// as they happen: on every change (play, stop, locate, rate) and
    /// otherwise at least every ~20 ms while anyone subscribes.
    /// Subscribers filter by `project_guid` client-side.
    #[subscribe]
    fn positions(&self) -> StampedPosition;
}
