//! Transport sync — follow a remote backend's transport to the sample.
//!
//! [`TransportSync`] is the `daw_proto::TransportSync` service as a
//! handle: the server's sync clock, one-shot stamped positions, and the
//! positions stream. [`TransportLeader`] is what a follower runs against
//! it: a clock estimator fed by pinging the server's clock, and the
//! latest stamped position of one project — exactly what
//! [`daw_transport_sync::Follower::tick`] takes to keep a local
//! [`TransportBackend`] in step with the remote one.
//!
//! ```no_run
//! # async fn example(daw: daw_control::Daw, backend: &dyn daw_transport_sync::TransportBackend)
//! #     -> daw_control::Result<()> {
//! use daw_transport_sync::{Follower, clock::now_micros_f64};
//!
//! let project = daw.current_project().await?;
//! // `now_micros_f64` must be the clock `backend` stamps its snapshots in.
//! let leader = project.transport_sync().leader(now_micros_f64);
//! let mut follower = Follower::default();
//! loop {
//!     leader.tick(&mut follower, backend);
//!     // …every 10–30 ms.
//! #   break;
//! }
//! # Ok(())
//! # }
//! ```

use std::sync::{Arc, Mutex};
use std::time::Duration;

use daw_proto::{ProjectContext, StampedPosition};
use daw_transport_sync::{ClockEstimator, Correction, Follower, Position, TransportBackend};

use crate::lock::LockExt;
use crate::{DawClients, EventStream, Result};

/// How often a [`TransportLeader`] pings the server's clock.
pub const PING_INTERVAL: Duration = Duration::from_millis(100);

/// The `TransportSync` service of one DAW connection.
#[derive(Clone)]
pub struct TransportSync {
    clients: Arc<DawClients>,
}

impl TransportSync {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// The server's sync clock now, microseconds — the clock every
    /// [`StampedPosition::host_micros`] from this server is in.
    pub async fn clock_now(&self) -> Result<f64> {
        Ok(self.clients.transport_sync.clock_now().await?)
    }

    /// One clock exchange: `(t1, server, t4)` — `local_clock` read
    /// before sending and after the answer, the server's clock between.
    /// Feed it to a [`ClockEstimator`] as `record(t1, server, server,
    /// t4)`.
    pub async fn ping(&self, local_clock: fn() -> f64) -> Result<(f64, f64, f64)> {
        let t1 = local_clock();
        let server = self.clock_now().await?;
        let t4 = local_clock();
        Ok((t1, server, t4))
    }

    /// `project`'s latest stamped position; `None` before its first
    /// audio buffer.
    pub async fn snapshot(&self, project: ProjectContext) -> Result<Option<StampedPosition>> {
        Ok(self.clients.transport_sync.snapshot(project).await?)
    }

    /// Stamped positions for every project, as the server publishes
    /// them (every change, a keepalive at least every ~20 ms). Drop the
    /// stream to unsubscribe.
    pub fn positions(&self) -> EventStream<StampedPosition> {
        self.positions_where(Box::new(|_| true))
    }

    /// Stamped positions for one project.
    pub fn positions_of(&self, project_guid: impl Into<String>) -> EventStream<StampedPosition> {
        let want = project_guid.into();
        self.positions_where(Box::new(move |p| p.project_guid == want))
    }

    fn positions_where(
        &self,
        admit: Box<dyn Fn(&StampedPosition) -> bool + Send + Sync>,
    ) -> EventStream<StampedPosition> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.transport_sync_stream.clone();
        EventStream::spawn(
            async move {
                let _ = stream.positions(raw_tx).await;
            },
            raw_rx,
            admit,
        )
    }

    /// Follow `project_guid` on this server: start pinging its clock
    /// (every [`PING_INTERVAL`]) and listening to its positions.
    /// `local_clock` is the clock the local backend being kept in step
    /// stamps its snapshots in (µs). Stops when the leader is dropped.
    pub fn leader(
        &self,
        project_guid: impl Into<String>,
        local_clock: fn() -> f64,
    ) -> TransportLeader {
        TransportLeader::spawn(
            self.clone(),
            project_guid.into(),
            local_clock,
            PING_INTERVAL,
        )
    }
}

/// [`TransportSync`] for one project.
#[derive(Clone)]
pub struct ProjectTransportSync {
    guid: String,
    inner: TransportSync,
}

impl ProjectTransportSync {
    pub(crate) fn new(guid: String, clients: Arc<DawClients>) -> Self {
        Self {
            guid,
            inner: TransportSync::new(clients),
        }
    }

    /// The server's sync clock now, microseconds.
    pub async fn clock_now(&self) -> Result<f64> {
        self.inner.clock_now().await
    }

    /// This project's latest stamped position.
    pub async fn snapshot(&self) -> Result<Option<StampedPosition>> {
        self.inner
            .snapshot(ProjectContext::project(&self.guid))
            .await
    }

    /// This project's stamped positions.
    pub fn positions(&self) -> EventStream<StampedPosition> {
        self.inner.positions_of(self.guid.clone())
    }

    /// Follow this project — see [`TransportSync::leader`].
    pub fn leader(&self, local_clock: fn() -> f64) -> TransportLeader {
        self.inner.leader(self.guid.clone(), local_clock)
    }
}

#[derive(Default)]
struct LeaderState {
    clock: ClockEstimator,
    latest: Option<StampedPosition>,
}

/// A remote project's transport, as a follower needs it: the offset
/// from the local clock to the server's, and the project's latest
/// stamped position (in the server's clock).
///
/// Two background activities, stopped when this is dropped: pinging the
/// server's clock (t1/t4 on the local clock, the server's reading as
/// t2 = t3) into a [`ClockEstimator`], and keeping the latest position
/// off the positions stream.
pub struct TransportLeader {
    project_guid: String,
    local_clock: fn() -> f64,
    state: Arc<Mutex<LeaderState>>,
    /// Dropping this ends the ping loop and the stream subscription.
    _stop: futures::channel::oneshot::Sender<()>,
}

impl TransportLeader {
    fn spawn(
        sync: TransportSync,
        project_guid: String,
        local_clock: fn() -> f64,
        ping_interval: Duration,
    ) -> Self {
        let state = Arc::new(Mutex::new(LeaderState::default()));
        let mut positions = sync.positions_of(project_guid.clone());

        let pinger_state = state.clone();
        let pinger = async move {
            loop {
                if let Ok((t1, server, t4)) = sync.ping(local_clock).await {
                    pinger_state
                        .lock_recoverable("transport_sync::leader")
                        .clock
                        .record(t1, server, server, t4);
                }
                architect::platform::sleep(ping_interval).await;
            }
        };
        let listener_state = state.clone();
        let listener = async move {
            while let Ok(Some(position)) = positions.recv().await {
                listener_state
                    .lock_recoverable("transport_sync::leader")
                    .latest = Some(position.get().clone());
            }
        };

        let (stop_tx, stop_rx) = futures::channel::oneshot::channel::<()>();
        architect::platform::spawn(async move {
            futures::pin_mut!(pinger, listener);
            // Ends when the leader is dropped or the stream ends (the
            // connection is gone) — dropping both loops, which
            // unsubscribes.
            let _ =
                futures::future::select(futures::future::select(pinger, listener), stop_rx).await;
        });

        Self {
            project_guid,
            local_clock,
            state,
            _stop: stop_tx,
        }
    }

    /// The project followed.
    #[must_use]
    pub fn project_guid(&self) -> &str {
        &self.project_guid
    }

    /// The local clock now (the one handed to [`TransportSync::leader`]).
    #[must_use]
    pub fn local_now(&self) -> f64 {
        (self.local_clock)()
    }

    /// The server's clock minus the local one, microseconds; `None`
    /// before the first ping comes back.
    #[must_use]
    pub fn offset_micros(&self) -> Option<f64> {
        self.state().clock.offset_micros()
    }

    /// The typical ping round trip, microseconds (the offset's error is
    /// at most half its asymmetry).
    #[must_use]
    pub fn round_trip_micros(&self) -> Option<f64> {
        self.state().clock.round_trip_micros()
    }

    /// How many pings the offset rests on.
    #[must_use]
    pub fn clock_samples(&self) -> usize {
        self.state().clock.samples()
    }

    /// The latest stamped position received; `None` before the first.
    #[must_use]
    pub fn latest(&self) -> Option<StampedPosition> {
        self.state().latest.clone()
    }

    /// The latest position, in the server's clock — the `leader` a
    /// [`Follower`] takes.
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        self.state().latest.as_ref().map(StampedPosition::position)
    }

    /// The latest position stamped in the local clock instead.
    #[must_use]
    pub fn local_position(&self) -> Option<Position> {
        let state = self.state();
        let offset = state.clock.offset_micros()?;
        state.latest.as_ref().map(|p| p.position().shifted(-offset))
    }

    /// One follower step: keep `backend` (stamped in the local clock)
    /// on the leader. `None` until both the clock offset and a position
    /// are known — nothing was done.
    pub fn tick(
        &self,
        follower: &mut Follower,
        backend: &dyn TransportBackend,
    ) -> Option<Correction> {
        let (leader, offset) = {
            let state = self.state();
            (
                state.latest.as_ref()?.position(),
                state.clock.offset_micros()?,
            )
        };
        Some(follower.tick(backend, &leader, offset, self.local_now()))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LeaderState> {
        self.state.lock_recoverable("transport_sync::leader")
    }
}

impl std::fmt::Debug for TransportLeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportLeader")
            .field("project_guid", &self.project_guid)
            .finish_non_exhaustive()
    }
}
