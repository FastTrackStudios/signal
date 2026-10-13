//! `Diagnostics` impl for the REAPER backend.
//!
//! Runs the entire latency probe loop inside one architect-dispatched
//! main-thread closure. No per-sample RPC marshaling, no IPC, no
//! tokio scheduling. The only remaining cost is the csurf callback +
//! tokio `broadcast` send/recv.

use daw_proto::ProjectContext;
use daw_proto::diagnostics::{
    AudioSyncSnapshot, Diagnostics, DriftDecisionSummary, LocalProjectSnapshot,
    PeerProjectPosition, PeerSummary,
};
use daw_proto::track::{TrackEvent, TrackStreamEvent};
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::TryRecvError;

fn hex_id(bytes: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn audio_snapshot_to_wire(s: daw_audio_sync::AudioSnapshot) -> AudioSyncSnapshot {
    AudioSyncSnapshot {
        sequence: s.sequence,
        // The snapshot carries f64 µs (REAPER's time_precise, never
        // negative); the wire keeps its whole-µs u64.
        host_micros: s.host_micros.max(0.0).round() as u64,
        playhead_seconds: s.playhead_seconds,
        sample_rate: s.sample_rate,
        buffer_len: s.buffer_len,
        is_playing: s.is_playing,
    }
}

impl Diagnostics for crate::Reaper {
    fn hub_publish_latency_us(&self, project: ProjectContext, samples: u32) -> Vec<u64> {
        // Project resolution is just to keep the call shape close
        // to other diagnostic methods; the probe itself doesn't
        // touch REAPER state.
        let _ = project; // probe doesn't touch project state
        let project_guid = String::new();
        let guid = "probe-guid".to_string();

        let mut rx = crate::event_hub::hub().subscribe_tracks();
        while rx.try_recv().is_ok() {}

        let mut results = Vec::with_capacity(samples as usize);
        for i in 0..samples {
            let target = 0.2 + (i as f64) * 0.05;
            let event = TrackStreamEvent {
                project_guid: project_guid.clone(),
                event: TrackEvent::VolumeChanged {
                    guid: guid.clone(),
                    volume: target,
                },
            };
            let t0 = Instant::now();
            crate::event_hub::hub().publish_track(event);

            let deadline = t0 + std::time::Duration::from_millis(10);
            loop {
                match rx.try_recv() {
                    Ok(envelope) => {
                        if let TrackEvent::VolumeChanged { guid: g, volume } = &envelope.event
                            && g == &guid
                            && (*volume - target).abs() < 1e-9
                        {
                            results.push(t0.elapsed().as_micros() as u64);
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        if Instant::now() > deadline {
                            results.push(t0.elapsed().as_micros() as u64);
                            break;
                        }
                        std::hint::spin_loop();
                    }
                    Err(TryRecvError::Lagged(_)) => continue,
                    Err(TryRecvError::Closed) => return results,
                }
            }
        }

        results
    }

    fn audio_sync_snapshot(&self) -> Option<AudioSyncSnapshot> {
        daw_audio_sync::global_snapshot().map(audio_snapshot_to_wire)
    }

    fn audio_sync_peers(&self) -> Vec<PeerSummary> {
        // Runs on the main thread — peer table snapshot needs the
        // tokio runtime to acquire its RwLock. Use the ClockSync's
        // stored runtime handle instead of try_current (architect's
        // main-thread dispatch isn't inside a tokio context).
        let Some(cs) = daw_audio_sync::global_clock_sync() else {
            return Vec::new();
        };
        let peers = cs.runtime.block_on(cs.peers.peers_snapshot());
        let now = Instant::now();
        peers
            .into_iter()
            .map(|p| {
                // Summarise the FIRST broadcast position. Full
                // per-project visibility is available via the
                // separate audio_sync_peer_projects RPC (next).
                let (playhead, playing) = p
                    .positions
                    .first()
                    .map(|pos| (pos.playhead_seconds, pos.is_playing))
                    .unwrap_or((f64::NAN, false));
                PeerSummary {
                    id: p.id.0.to_string(),
                    addr: p.addr.to_string(),
                    offset_us: p.offset_us,
                    delay_us: p.delay_us,
                    rtt_age_ms: now.duration_since(p.last_rtt_at).as_millis() as u64,
                    announce_age_ms: now.duration_since(p.last_announce_at).as_millis() as u64,
                    remote_playhead_seconds: playhead,
                    remote_is_playing: playing,
                }
            })
            .collect()
    }

    fn audio_sync_self_peer_id(&self) -> String {
        daw_audio_sync::global_clock_sync()
            .map(|cs| cs.peer_id.0.to_string())
            .unwrap_or_default()
    }

    fn audio_sync_seed_peer(&self, peer_id: &str, addr: &str) -> daw_proto::DawResult<()> {
        use daw_proto::DawError;
        let Some(cs) = daw_audio_sync::global_clock_sync() else {
            return Err(DawError::operation_failed("audio-sync not running"));
        };
        let uuid = uuid::Uuid::parse_str(peer_id)
            .map_err(|e| DawError::operation_failed(format!("bad peer_id: {e}")))?;
        let addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| DawError::operation_failed(format!("bad addr: {e}")))?;
        cs.seed_peer_sync(daw_audio_sync::clock_sync::PeerId(uuid), addr);
        Ok(())
    }

    fn audio_sync_peer_projects(&self, peer_id: &str) -> Vec<PeerProjectPosition> {
        let Some(cs) = daw_audio_sync::global_clock_sync() else {
            return Vec::new();
        };
        let Ok(uuid) = uuid::Uuid::parse_str(peer_id) else {
            return Vec::new();
        };
        let target = daw_audio_sync::clock_sync::PeerId(uuid);
        let peers = cs.runtime.block_on(cs.peers.peers_snapshot());
        let Some(peer) = peers.into_iter().find(|p| p.id == target) else {
            return Vec::new();
        };
        let now = Instant::now();
        peer.positions
            .into_iter()
            .map(|pos| PeerProjectPosition {
                project_id_hex: hex_id(&pos.project_id),
                host_micros: pos.host_micros,
                playhead_seconds: pos.playhead_seconds,
                sample_rate: pos.sample_rate,
                playrate: pos.playrate,
                is_playing: pos.is_playing,
                received_age_ms: now.duration_since(pos.received_at).as_millis() as u64,
            })
            .collect()
    }

    fn audio_sync_local_projects(&self) -> Vec<LocalProjectSnapshot> {
        let Some(registry) = daw_audio_sync::registry::global_registry() else {
            return Vec::new();
        };
        registry
            .snapshots()
            .into_iter()
            .map(|(_idx, snap)| LocalProjectSnapshot {
                project_id_hex: hex_id(&snap.project_id),
                snapshot: audio_snapshot_to_wire(snap),
            })
            .collect()
    }

    fn audio_sync_drift_decision(&self) -> DriftDecisionSummary {
        let Some(corrector) = daw_audio_sync::global_drift_corrector() else {
            return DriftDecisionSummary {
                drift_seconds: f64::NAN,
                ..Default::default()
            };
        };
        let Some(d) = corrector.last_decision() else {
            return DriftDecisionSummary {
                drift_seconds: f64::NAN,
                ..Default::default()
            };
        };
        DriftDecisionSummary {
            sequence: d.sequence,
            leader_peer_id: d.leader.map(|p| p.0.to_string()).unwrap_or_default(),
            drift_seconds: d.drift_seconds.unwrap_or(f64::NAN),
            target_rate: d.target_rate,
        }
    }

    fn audio_sync_observe(&self, count: u32, interval_us: u64) -> Vec<AudioSyncSnapshot> {
        // Runs on the main thread (architect dispatcher). Spin-sleep
        // between samples — `interval_us` is the poll cadence (caller
        // should set it to ~1/4 of the expected buffer period so we
        // catch every distinct sequence). Total budget is generous:
        // 8× nominal so OS scheduler hiccups don't truncate the
        // window prematurely. Holds the main thread for the duration
        // — keep `count` reasonable (< 50).
        let mut out = Vec::with_capacity(count as usize);
        let interval = Duration::from_micros(interval_us.max(50));
        let mut last_seq = 0u64;
        let deadline = Instant::now() + interval * count.max(1) * 8;
        while out.len() < count as usize && Instant::now() < deadline {
            if let Some(snap) = daw_audio_sync::global_snapshot()
                && snap.sequence != last_seq
            {
                last_seq = snap.sequence;
                out.push(audio_snapshot_to_wire(snap));
            }
            std::thread::sleep(interval);
        }
        out
    }
}
