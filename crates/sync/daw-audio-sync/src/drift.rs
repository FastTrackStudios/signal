//! Drift correction — the REAPER actuator for the core's
//! [`daw_transport_sync::DriftController`]: when the local playhead
//! diverges from the elected leader's, nudge REAPER's playrate.
//!
//! # Control loop
//!
//! Runs as a tokio task at `CORRECTION_HZ` (~20Hz). Each tick:
//!
//! 1. Snapshot local position from the [`SnapshotCell`].
//! 2. Snapshot the peer table; elect the lowest-UUID peer that's
//!    actively playing as leader. If we are the leader (or the local
//!    transport is stopped), hold rate at 1.0 and forget the
//!    controller's state.
//! 3. Bring the leader's latest position frame into our clock — the
//!    core [`Position`] shifted by the peer's clock offset — and hand it,
//!    with our snapshot, to the core controller. The control law is the
//!    core's: proportional-integral on the playhead gap, a deadband of a
//!    couple of samples, the rate clamped to ±`max_rate_deviation` around
//!    the leader's.
//! 4. Hand the controller's rate to the host-supplied actuator when it
//!    differs from the last one applied. The actuator is responsible for
//!    getting onto REAPER's main thread (via `TaskSupport` typically) and
//!    calling `CSurf_OnPlayRateChange(new_rate)`.
//!
//! # Rate only — no locate, no start/stop
//!
//! The core controller can also ask for a scheduled locate (gap past its
//! threshold), a start, or a stop. REAPER's actuator here only moves the
//! rate, so this adapter configures the controller rate-only: the locate
//! threshold is infinite (every gap is closed by rate, however long that
//! takes at ±1% — the behaviour this corrector always had), and the
//! controller is only consulted while both sides play, so it never asks
//! to start or stop. Should a locate or stop come back anyway, it is
//! logged at debug and the rate is held.
//!
//! # Why proportional + deadband instead of bang-bang
//!
//! ReaBlink uses a bang-bang controller (toggle to 0.94 / 1.06 via
//! `Main_OnCommand 40524/40525`) because Ableton Link only needs
//! beat-level alignment where ±6% is inaudible during a beat. For
//! sample-accurate sync, we need sub-percent corrections so the
//! pitch shift is below ~5 cents (imperceptible). Proportional with
//! ±1% cap fits that constraint and lets us land within a few
//! samples of the leader in steady state; the integral term learns the
//! two sound cards' constant crystal mismatch so no standing gap is left.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use daw_transport_sync::{Correction, DriftController, Position};
use tokio::task::JoinHandle;
use tracing::{debug, trace};

use crate::clock_sync::{ClockSync, PeerId, PeerInfo};
use crate::{AudioSnapshot, SnapshotCell};

/// How often the corrector runs. 20Hz matches the position-broadcast
/// rate so we always have a fresh leader sample to compare against.
const CORRECTION_HZ: u64 = 20;

/// A rate within this of the last one applied is not re-sent to REAPER.
const RATE_EPSILON: f64 = 1e-7;

/// Control parameters. Tuned for sample-accurate convergence without
/// audible pitch artifacts. Mapped onto the core's
/// [`daw_transport_sync::DriftConfig`] by [`Self::controller_config`].
#[derive(Clone, Copy, Debug)]
pub struct DriftConfig {
    /// Drift below this (in seconds) gets no proportional correction —
    /// below the deadband, sample-rate drift between machines is the
    /// dominant noise source; correction would just chase noise.
    pub deadband_seconds: f64,
    /// Target convergence time. Larger = smoother, less audible
    /// pitch wobble, but slower correction. 1.0s is a good default
    /// for live playback where peers should sound aligned but the
    /// listener can't hear sub-percent rate changes.
    pub convergence_seconds: f64,
    /// Maximum deviation from the leader's rate in either direction.
    /// 0.01 = ±1% (≈ ±17 cents at worst). Anything larger risks
    /// audible pitch shift on sustained content.
    pub max_rate_deviation: f64,
    /// How long the integral term takes to learn a standing crystal
    /// mismatch (seconds; 0 turns it off → proportional only).
    pub integral_seconds: f64,
    /// Drop position frames older than this from consideration. Avoids
    /// correcting against stale data after a network hiccup or peer
    /// disconnect.
    pub max_position_age: Duration,
}

impl Default for DriftConfig {
    fn default() -> Self {
        let core = daw_transport_sync::DriftConfig::default();
        Self {
            deadband_seconds: core.deadband_seconds, // 50µs ≈ 2 samples at 48kHz
            convergence_seconds: core.convergence_seconds,
            max_rate_deviation: core.max_rate_deviation,
            integral_seconds: core.integral_seconds,
            max_position_age: Duration::from_millis(500),
        }
    }
}

impl DriftConfig {
    /// The core controller's configuration for REAPER's rate-only
    /// actuator: no locate (infinite threshold), so every gap is closed
    /// by rate.
    pub fn controller_config(&self) -> daw_transport_sync::DriftConfig {
        daw_transport_sync::DriftConfig {
            deadband_seconds: self.deadband_seconds,
            convergence_seconds: self.convergence_seconds,
            max_rate_deviation: self.max_rate_deviation,
            integral_seconds: self.integral_seconds,
            locate_threshold_seconds: f64::INFINITY,
            max_position_age_micros: self.max_position_age.as_secs_f64() * 1e6,
            ..daw_transport_sync::DriftConfig::default()
        }
    }
}

/// Latest correction decision. Exposed via [`DriftCorrector::last_decision`]
/// for diagnostics + tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriftDecision {
    pub sequence: u64,
    /// Leader currently tracked, or `None` if we're the leader or
    /// no peer is playing.
    pub leader: Option<PeerId>,
    /// Local minus leader projected, in seconds. `None` when
    /// `leader` is `None`.
    pub drift_seconds: Option<f64>,
    /// Rate we asked the actuator to apply. `1.0` when no
    /// correction is in flight.
    pub target_rate: f64,
}

/// The corrector's state between ticks: the core controller, whom it is
/// following, and the rate REAPER was last told. Pure — the tokio loop
/// feeds it and runs the actuator.
struct RateFollower {
    controller: DriftController,
    local_peer_id: PeerId,
    max_position_age: Duration,
    leader: Option<PeerId>,
    applied_rate: f64,
}

impl RateFollower {
    fn new(config: DriftConfig, local_peer_id: PeerId) -> Self {
        Self {
            controller: DriftController::new(config.controller_config()),
            local_peer_id,
            max_position_age: config.max_position_age,
            leader: None,
            applied_rate: 1.0,
        }
    }

    /// One tick. Returns the decision (for diagnostics) and the rate to
    /// hand the actuator, if it changed.
    fn decide(
        &mut self,
        sequence: u64,
        local: &AudioSnapshot,
        peers: &[PeerInfo],
    ) -> (DriftDecision, Option<f64>) {
        let leader = if local.is_playing {
            elect_leader(peers, self.local_peer_id, self.max_position_age)
        } else {
            // Stopped: reset so a stale correction is not inherited on
            // the next play.
            None
        };
        let Some((leader, position)) = leader.and_then(|l| {
            let position = l.positions.iter().find(|p| p.is_playing).copied()?;
            Some((l, position))
        }) else {
            // We lead (or nobody plays): nominal rate, nothing learned.
            self.controller.reset();
            self.leader = None;
            let actuate = self.apply(1.0);
            let decision = DriftDecision {
                sequence,
                leader: None,
                drift_seconds: None,
                target_rate: 1.0,
            };
            return (decision, actuate);
        };

        if self.leader != Some(leader.id) {
            // A new leader: what was learned about the old one's crystal
            // does not hold.
            self.controller.reset();
            self.leader = Some(leader.id);
        }

        // The leader's frame is stamped in its clock; offset_us is
        // remote − local, so shifting by −offset brings it into ours.
        let leader_here: Position = position.position().shifted(-(leader.offset_us as f64));
        match self.controller.step(local, &leader_here, local.host_micros) {
            Correction::Hold | Correction::Rate(_) => {}
            other => debug!(
                correction = ?other,
                "drift: rate-only actuator — locate/stop not applied, holding rate",
            ),
        }
        let target_rate = self.controller.rate();
        let actuate = self.apply(target_rate);
        let drift_seconds = self.controller.last_drift();
        if actuate.is_some() {
            trace!(
                drift_us = drift_seconds.map(|d| (d * 1e6) as i64),
                target_rate,
                leader = ?leader.id,
                "drift correction applied",
            );
        }
        let decision = DriftDecision {
            sequence,
            leader: Some(leader.id),
            drift_seconds,
            target_rate,
        };
        (decision, actuate)
    }

    /// `Some(rate)` when REAPER has to be told.
    fn apply(&mut self, rate: f64) -> Option<f64> {
        if (rate - self.applied_rate).abs() > RATE_EPSILON {
            self.applied_rate = rate;
            Some(rate)
        } else {
            None
        }
    }
}

/// Drift corrector. Holds the spawned task; drop to stop.
pub struct DriftCorrector {
    task: Option<JoinHandle<()>>,
    last_decision_bits: Arc<DecisionCell>,
}

impl DriftCorrector {
    /// Spawn the corrector loop on the current tokio runtime. The
    /// `actuator` closure is called whenever the controller decides
    /// to change the playrate; it's expected to dispatch the change
    /// to REAPER's main thread (typically via `TaskSupport`) and
    /// call `MediumReaper::csurf_on_play_rate_change`.
    ///
    /// The controller call rate is bounded by `CORRECTION_HZ`, so
    /// the actuator runs at most 20× per second under normal
    /// operation — but it MAY be called every tick when the system
    /// is actively converging, so the actuator should be cheap
    /// (just enqueue a main-thread task).
    pub fn spawn<F>(
        cell: Arc<SnapshotCell>,
        clock_sync: Arc<ClockSync>,
        config: DriftConfig,
        actuator: F,
    ) -> Self
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        let last_decision_bits = Arc::new(DecisionCell::default());
        let decision_cell = last_decision_bits.clone();
        let mut follower = RateFollower::new(config, clock_sync.peer_id);

        let task = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(1000 / CORRECTION_HZ));
            let mut seq = 0u64;

            loop {
                tick.tick().await;
                seq = seq.wrapping_add(1);

                let Some(local) = cell.load() else { continue };
                // Only read the peer table when there is something to
                // follow with.
                let peers = if local.is_playing {
                    clock_sync.peers.peers_snapshot().await
                } else {
                    Vec::new()
                };
                let (decision, actuate) = follower.decide(seq, &local, &peers);
                if let Some(rate) = actuate {
                    actuator(rate);
                }
                decision_cell.store(decision);
            }
        });

        Self {
            task: Some(task),
            last_decision_bits,
        }
    }

    /// Latest controller decision. `None` until the loop has run at
    /// least once.
    pub fn last_decision(&self) -> Option<DriftDecision> {
        self.last_decision_bits.load()
    }
}

impl Drop for DriftCorrector {
    fn drop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

/// Pick the leader from a peer table. Strategy: smallest-UUID peer
/// that's currently playing and has a recent enough position frame.
/// Returns `None` when no other peer qualifies OR when we (the local
/// peer) win the election — caller treats both as "no correction".
fn elect_leader(peers: &[PeerInfo], local_peer_id: PeerId, max_age: Duration) -> Option<PeerInfo> {
    let mut best: Option<&PeerInfo> = None;
    let mut best_id: Option<uuid::Uuid> = None;
    for peer in peers {
        // A peer is eligible if it has at least one playing,
        // recently-broadcast position frame.
        let has_active = peer
            .positions
            .iter()
            .any(|pos| pos.is_playing && pos.received_at.elapsed() <= max_age);
        if !has_active {
            continue;
        }
        if peer.id == local_peer_id {
            continue;
        }
        match best_id {
            None => {
                best = Some(peer);
                best_id = Some(peer.id.0);
            }
            Some(b) if peer.id.0 < b => {
                best = Some(peer);
                best_id = Some(peer.id.0);
            }
            _ => {}
        }
    }
    // If our id is smaller than the best remote we found, WE are the
    // leader — return None so the controller holds rate at 1.0.
    if let Some(b) = best_id
        && local_peer_id.0 < b
    {
        return None;
    }
    best.cloned()
}

/// Lock-free single-writer / multi-reader cell for the latest
/// [`DriftDecision`]. Same seqlock pattern as
/// [`crate::SnapshotCell`].
#[derive(Default)]
struct DecisionCell {
    seq: AtomicU64,
    sequence: AtomicU64,
    leader_hi: AtomicU64,
    leader_lo: AtomicU64,
    has_leader: AtomicU64,
    drift_bits: AtomicU64,
    has_drift: AtomicU64,
    rate_bits: AtomicU64,
}

impl DecisionCell {
    fn store(&self, d: DriftDecision) {
        let prev = self.seq.load(Ordering::Relaxed);
        self.seq.store(prev.wrapping_add(1), Ordering::Release);
        self.sequence.store(d.sequence, Ordering::Relaxed);
        match d.leader {
            Some(id) => {
                let bytes = id.0.as_u128();
                self.leader_hi
                    .store((bytes >> 64) as u64, Ordering::Relaxed);
                self.leader_lo.store(bytes as u64, Ordering::Relaxed);
                self.has_leader.store(1, Ordering::Relaxed);
            }
            None => self.has_leader.store(0, Ordering::Relaxed),
        }
        match d.drift_seconds {
            Some(v) => {
                self.drift_bits.store(v.to_bits(), Ordering::Relaxed);
                self.has_drift.store(1, Ordering::Relaxed);
            }
            None => self.has_drift.store(0, Ordering::Relaxed),
        }
        self.rate_bits
            .store(d.target_rate.to_bits(), Ordering::Relaxed);
        self.seq.store(prev.wrapping_add(2), Ordering::Release);
    }

    fn load(&self) -> Option<DriftDecision> {
        for _ in 0..4 {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 == 0 {
                return None;
            }
            if s1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }
            let sequence = self.sequence.load(Ordering::Relaxed);
            let leader = if self.has_leader.load(Ordering::Relaxed) != 0 {
                let hi = self.leader_hi.load(Ordering::Relaxed) as u128;
                let lo = self.leader_lo.load(Ordering::Relaxed) as u128;
                Some(PeerId(uuid::Uuid::from_u128((hi << 64) | lo)))
            } else {
                None
            };
            let drift = if self.has_drift.load(Ordering::Relaxed) != 0 {
                Some(f64::from_bits(self.drift_bits.load(Ordering::Relaxed)))
            } else {
                None
            };
            let target_rate = f64::from_bits(self.rate_bits.load(Ordering::Relaxed));
            let s2 = self.seq.load(Ordering::Acquire);
            if s1 == s2 {
                return Some(DriftDecision {
                    sequence,
                    leader,
                    drift_seconds: drift,
                    target_rate,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock_sync::{PeerInfo, RemotePosition};
    use std::net::SocketAddr;
    use std::time::Instant;
    use uuid::Uuid;

    fn make_peer(id: u128, playing: bool, playhead: f64, host_us: i64) -> PeerInfo {
        PeerInfo {
            id: PeerId(Uuid::from_u128(id)),
            addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            offset_us: 0,
            delay_us: 0,
            last_rtt_at: Instant::now(),
            last_announce_at: Instant::now(),
            positions: vec![RemotePosition {
                project_id: [0u8; 16],
                host_micros: host_us,
                playhead_seconds: playhead,
                sample_rate: 48_000.0,
                playrate: 1.0,
                is_playing: playing,
                received_at: Instant::now(),
            }],
        }
    }

    #[test]
    fn election_picks_smallest_uuid_when_we_are_larger() {
        let me = PeerId(Uuid::from_u128(200));
        let peers = vec![make_peer(100, true, 1.0, 0), make_peer(150, true, 1.0, 0)];
        let leader = elect_leader(&peers, me, Duration::from_secs(1)).unwrap();
        assert_eq!(leader.id.0.as_u128(), 100);
    }

    #[test]
    fn election_returns_none_when_we_are_smallest() {
        let me = PeerId(Uuid::from_u128(50));
        let peers = vec![make_peer(100, true, 1.0, 0)];
        assert!(elect_leader(&peers, me, Duration::from_secs(1)).is_none());
    }

    #[test]
    fn election_skips_stopped_peers() {
        let me = PeerId(Uuid::from_u128(200));
        let peers = vec![make_peer(50, false, 0.0, 0), make_peer(100, true, 1.0, 0)];
        let leader = elect_leader(&peers, me, Duration::from_secs(1)).unwrap();
        assert_eq!(leader.id.0.as_u128(), 100);
    }

    fn playing_snapshot(host_micros: f64, playhead_seconds: f64) -> AudioSnapshot {
        AudioSnapshot {
            sequence: 1,
            host_micros,
            playhead_seconds,
            buffer_len: 256,
            is_playing: true,
            ..AudioSnapshot::default()
        }
    }

    /// A leader (id 100, lower than ours) whose clock reads `offset_us`
    /// later than ours, playing from `playhead` at its `host_us`.
    fn leader_peer(offset_us: i64, playhead: f64, host_us: i64) -> PeerInfo {
        let mut peer = make_peer(100, true, playhead, host_us);
        peer.offset_us = offset_us;
        peer
    }

    const ME: u128 = 200;

    #[test]
    fn decisions_match_the_core_controller() {
        // The adapter's decisions are the core controller's, fed the
        // leader's frame shifted into our clock: tick by tick, the same
        // gap and the same rate. Closed loop: our playhead moves at the
        // rate we last applied, on a device 80ppm fast, starting 3ms
        // ahead; the leader's clock reads 7ms later than ours.
        let config = DriftConfig::default();
        let mut follower = RateFollower::new(config, PeerId(Uuid::from_u128(ME)));
        let mut core = DriftController::new(config.controller_config());
        let offset = 7_000i64;
        let dt = 50_000.0;

        let mut applied = 1.0;
        let mut local_playhead = 10.003;
        for tick in 0..400u64 {
            let t = 5_000_000.0 + tick as f64 * dt;
            // The leader's fresh frame (20Hz), stamped in its clock.
            let leader_playhead = 10.0 + (t - 5_000_000.0) * 1e-6;
            let peers = vec![leader_peer(offset, leader_playhead, t as i64 + offset)];
            let leader_here = peers[0].positions[0].position().shifted(-(offset as f64));

            let local = playing_snapshot(t, local_playhead);
            let (decision, actuate) = follower.decide(tick, &local, &peers);
            core.step(&local, &leader_here, t);
            assert_eq!(decision.leader, Some(PeerId(Uuid::from_u128(100))));
            assert_eq!(decision.drift_seconds, core.last_drift());
            assert_eq!(decision.target_rate, core.rate());
            if let Some(rate) = actuate {
                applied = rate;
            }
            assert!((applied - core.rate()).abs() <= RATE_EPSILON);
            local_playhead += dt * 1e-6 * applied * (1.0 + 80e-6);
        }
        // 20 s in: the gap is closed and the integral has learned this
        // device runs fast, so it keeps running a touch slow.
        let drift = core.last_drift().unwrap();
        assert!(drift.abs() < 100e-6, "drift {drift}");
        assert!(core.rate() < 1.0);
        assert!(
            core.learned_mismatch() < -40e-6,
            "{}",
            core.learned_mismatch()
        );
    }

    #[test]
    fn a_large_gap_is_closed_by_rate_not_locate() {
        // REAPER's actuator is rate-only: a gap far past the core's
        // default locate threshold is still a clamped rate, as it always
        // was here.
        let mut follower = RateFollower::new(DriftConfig::default(), PeerId(Uuid::from_u128(ME)));
        let peers = vec![leader_peer(0, 10.0, 1_000_000)];
        let (decision, actuate) = follower.decide(1, &playing_snapshot(1_000_000.0, 10.5), &peers);
        assert_eq!(actuate, Some(0.99));
        assert!((decision.drift_seconds.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn stopping_puts_the_rate_back() {
        let mut follower = RateFollower::new(DriftConfig::default(), PeerId(Uuid::from_u128(ME)));
        let peers = vec![leader_peer(0, 10.0, 1_000_000)];
        let (_, actuate) = follower.decide(1, &playing_snapshot(1_000_000.0, 10.01), &peers);
        assert!(actuate.is_some_and(|r| r < 1.0));

        let mut stopped = playing_snapshot(1_050_000.0, 10.06);
        stopped.is_playing = false;
        let (decision, actuate) = follower.decide(2, &stopped, &peers);
        assert_eq!(actuate, Some(1.0));
        assert!(decision.leader.is_none() && decision.drift_seconds.is_none());
        // And nothing more to say while stopped.
        assert_eq!(follower.decide(3, &stopped, &peers).1, None);
    }

    #[test]
    fn leading_holds_nominal_rate() {
        // Our id is lowest: we lead, whatever the others are doing.
        let mut follower = RateFollower::new(DriftConfig::default(), PeerId(Uuid::from_u128(1)));
        let peers = vec![leader_peer(0, 10.0, 1_000_000)];
        let (decision, actuate) = follower.decide(1, &playing_snapshot(1_000_000.0, 12.0), &peers);
        assert_eq!(actuate, None);
        assert!(decision.leader.is_none());
        assert_eq!(decision.target_rate, 1.0);
    }

    #[test]
    fn decision_cell_round_trip() {
        let cell = DecisionCell::default();
        assert!(cell.load().is_none());
        let d = DriftDecision {
            sequence: 42,
            leader: Some(PeerId(Uuid::from_u128(0xabc))),
            drift_seconds: Some(0.0012),
            target_rate: 0.9985,
        };
        cell.store(d);
        let got = cell.load().unwrap();
        assert_eq!(got.sequence, 42);
        assert_eq!(got.leader.unwrap().0.as_u128(), 0xabc);
        assert!((got.drift_seconds.unwrap() - 0.0012).abs() < 1e-12);
        assert!((got.target_rate - 0.9985).abs() < 1e-12);
    }
}
