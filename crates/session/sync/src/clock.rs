//! The session's shared clock, over the network.
//!
//! Playing together to the sample needs one clock everyone's positions are
//! stamped in. It is the host's: the host serves [`SessionClock`] beside
//! the docs, and every joiner pings it ten times a second, feeding a
//! [`ClockEstimator`] — the offset between its own monotonic clock and the
//! host's, accurate to tens of microseconds on a LAN (the fastest round
//! trips, smoothed). [`SharedClock`] is that estimate as the rest of the
//! app sees it: "the shared clock now", and the offset to carry a local
//! stamp into it.
//!
//! One vox call per ping, answered with the host's clock at the moment it
//! handled it: the two middle stamps of NTP's four are the same instant
//! (a handler that only reads a clock takes no measurable time).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use daw_transport_sync::ClockEstimator;
use daw_transport_sync::clock::now_micros_f64;

/// The host's clock, as a service.
#[vox::service]
pub trait SessionClock {
    /// The host's monotonic clock now, microseconds.
    async fn now(&self) -> f64;
}

/// The host's side: its own clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClockHost;

impl SessionClock for ClockHost {
    async fn now(&self) -> f64 {
        now_micros_f64()
    }
}

/// How often a joiner pings the host.
const PING: Duration = Duration::from_millis(100);

/// No estimate yet.
const UNKNOWN: u64 = u64::MAX;

/// This machine's view of the shared clock.
#[derive(Clone)]
pub struct SharedClock {
    /// shared − local, µs, as f64 bits; [`UNKNOWN`] before the first ping.
    offset: Arc<AtomicU64>,
    /// Round trip, µs, as f64 bits.
    round_trip: Arc<AtomicU64>,
    pinger: Option<Arc<architect::platform::JoinHandle<()>>>,
}

impl std::fmt::Debug for SharedClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedClock")
            .field("offset_micros", &self.offset_micros())
            .finish()
    }
}

impl SharedClock {
    /// The host's: the shared clock is this one.
    #[must_use]
    pub fn owned() -> Self {
        Self {
            offset: Arc::new(AtomicU64::new(0f64.to_bits())),
            round_trip: Arc::new(AtomicU64::new(0f64.to_bits())),
            pinger: None,
        }
    }

    /// A joiner's: estimated by pinging the host through `client` until
    /// this (and every clone) is dropped.
    #[must_use]
    pub fn follow(client: SessionClockClient) -> Self {
        Self::follow_with(move || {
            let client = client.clone();
            async move { client.now().await.ok() }
        })
    }

    /// A joiner's, following any clock: `now` asks it the time
    /// (microseconds; `None` when it did not answer) — the host's
    /// [`SessionClock`], or Task's (`live_proto::LiveSessions::now`) for a
    /// set Task keeps.
    #[must_use]
    pub fn follow_with<F, Fut>(now: F) -> Self
    where
        F: Fn() -> Fut + architect::platform::MaybeSend + 'static,
        Fut: std::future::Future<Output = Option<f64>> + architect::platform::MaybeSend,
    {
        let offset = Arc::new(AtomicU64::new(UNKNOWN));
        let round_trip = Arc::new(AtomicU64::new(UNKNOWN));
        let (o, r) = (Arc::clone(&offset), Arc::clone(&round_trip));
        // `architect::platform`: tokio natively, the page's event loop in a
        // browser.
        let pinger = architect::platform::spawn(async move {
            let mut estimator = ClockEstimator::default();
            loop {
                let t1 = now_micros_f64();
                let Some(host) = now().await else {
                    architect::platform::sleep(PING).await;
                    continue;
                };
                let t4 = now_micros_f64();
                estimator.record(t1, host, host, t4);
                if let (Some(off), Some(rtt)) =
                    (estimator.offset_micros(), estimator.round_trip_micros())
                {
                    o.store(off.to_bits(), Ordering::Relaxed);
                    r.store(rtt.to_bits(), Ordering::Relaxed);
                }
                architect::platform::sleep(PING).await;
            }
        });
        Self {
            offset,
            round_trip,
            pinger: Some(Arc::new(pinger)),
        }
    }

    /// Shared minus local, µs — `None` until the first ping is back.
    #[must_use]
    pub fn offset_micros(&self) -> Option<f64> {
        let bits = self.offset.load(Ordering::Relaxed);
        (bits != UNKNOWN).then(|| f64::from_bits(bits))
    }

    /// The typical round trip to the host, µs (the offset's error is at
    /// most half its asymmetry).
    #[must_use]
    pub fn round_trip_micros(&self) -> Option<f64> {
        let bits = self.round_trip.load(Ordering::Relaxed);
        (bits != UNKNOWN).then(|| f64::from_bits(bits))
    }

    /// The shared clock now, µs.
    #[must_use]
    pub fn now(&self) -> Option<f64> {
        self.offset_micros().map(|o| now_micros_f64() + o)
    }

    /// A local stamp, in the shared clock.
    #[must_use]
    pub fn to_shared(&self, local_micros: f64) -> Option<f64> {
        self.offset_micros().map(|o| local_micros + o)
    }
}

impl Drop for SharedClock {
    fn drop(&mut self) {
        // The last clone stops the pinger.
        if let Some(pinger) = self.pinger.take()
            && let Ok(pinger) = Arc::try_unwrap(pinger)
        {
            pinger.abort();
        }
    }
}
