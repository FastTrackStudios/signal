//! Wire types for [`TransportSync`](super::TransportSync).

use facet::Facet;

/// Where a project's playhead was, and *when* — in the serving
/// backend's sync clock (the clock [`clock_now`] reads).
///
/// One per audio buffer the backend observed (not every one is
/// published). A position without its time is useless for sync; with
/// it, [`position`](Self::position) projects the playhead to any other
/// instant of that clock, and a client that knows its offset to that
/// clock (by pinging [`clock_now`]) carries it into its own.
///
/// [`clock_now`]: super::TransportSync::clock_now
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct StampedPosition {
    /// The project this position is for (subscribers filter on it —
    /// the stream is argless on the wire).
    pub project_guid: String,
    /// The instant, in the server's sync clock, microseconds.
    pub host_micros: f64,
    /// The playhead at that instant, seconds.
    pub playhead_seconds: f64,
    /// Playhead seconds per clock second (1.0 = nominal).
    pub playrate: f64,
    pub is_playing: bool,
    /// The backend's buffer counter: increases by one per audio buffer,
    /// so a gap tells a subscriber how many buffers it did not see.
    pub sequence: u64,
    /// The device sample rate, Hz (to express a gap in samples).
    pub sample_rate: f64,
}

impl StampedPosition {
    /// A backend's per-buffer snapshot, for `project_guid`.
    #[must_use]
    pub fn from_snapshot(
        project_guid: impl Into<String>,
        snapshot: &daw_transport_sync::AudioSnapshot,
    ) -> Self {
        Self {
            project_guid: project_guid.into(),
            host_micros: snapshot.host_micros,
            playhead_seconds: snapshot.playhead_seconds,
            playrate: snapshot.playrate,
            is_playing: snapshot.is_playing,
            sequence: snapshot.sequence,
            sample_rate: snapshot.sample_rate,
        }
    }

    /// This position as the sync core's [`daw_transport_sync::Position`],
    /// in the server's clock.
    #[must_use]
    pub const fn position(&self) -> daw_transport_sync::Position {
        daw_transport_sync::Position {
            host_micros: self.host_micros,
            playhead_seconds: self.playhead_seconds,
            playrate: self.playrate,
            is_playing: self.is_playing,
        }
    }
}

// Trivial Reborrow impl for the owned stream type — lets
// `SelfRef<StampedPosition>::get()` hand subscribers `&StampedPosition`.
// Safe because the type has no borrowed lifetimes.
#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::StampedPosition;
    unsafe impl vox_types::Reborrow for StampedPosition {
        type Ref<'a> = StampedPosition;
    }
}
