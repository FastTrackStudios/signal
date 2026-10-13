//! Master transport position heartbeat.
//!
//! When this peer is the transport master and playback is active, periodically
//! reads the local playhead position and broadcasts it to followers so they
//! can drift-correct. Non-master peers and stopped/paused states produce no
//! heartbeats — transport state transitions (play/stop/pause) are handled by
//! the normal subscription forwarder.
//!
//! The heartbeat runs regardless of whether Link is active. Link handles
//! beat-phase alignment (fractional sync within beats), but PeerMesh
//! heartbeats are needed for absolute timeline position sync (hard seeks
//! when positions diverge by more than 1s).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::{SyncDomain, SyncEvent};
use daw::rpc::Daw;
use tokio::sync::broadcast;
use tracing::{debug, trace};

/// Heartbeat interval — 30Hz matches the Link engine's tick rate, giving the
/// drift corrector enough feedback to converge smoothly.
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(33);

/// Spawn the master position heartbeat task.
///
/// The heartbeat always runs when this peer is master, regardless of Link
/// status. Link handles beat-phase alignment but not absolute timeline
/// position sync. PeerMesh heartbeats are needed so followers can detect
/// large position divergences (>1s) and hard-seek to catch up.
pub fn spawn(
    daw: Daw,
    peer_id: String,
    sequence: Arc<AtomicU64>,
    event_tx: broadcast::Sender<SyncEvent>,
    is_master: Arc<AtomicBool>,
    _link_active: Arc<AtomicBool>,
    sync_enabled: Arc<AtomicBool>,
) {
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);

        loop {
            interval.tick().await;

            // Skip heartbeats when peer sync is disabled
            if !sync_enabled.load(Ordering::Relaxed) {
                continue;
            }

            // Only the master sends position heartbeats
            if !is_master.load(Ordering::Relaxed) {
                continue;
            }

            // Read current transport state
            let project = match daw.current_project().await {
                Ok(p) => p,
                Err(_) => continue,
            };

            let transport = match project.transport().get_state().await {
                Ok(t) => t,
                Err(_) => continue,
            };
            // Timestamp when the master position was actually read (right after
            // get_state returns), so followers can measure the exact time gap
            // between the two position samples.
            let position_read_at = SyncEvent::now_ms();

            // Only send heartbeats while playing
            if transport.play_state != daw::service::PlayState::Playing {
                continue;
            }

            let seq = sequence.fetch_add(1, Ordering::Relaxed);
            let project_guid = match project.info().await {
                Ok(info) => info.guid,
                Err(_) => continue,
            };

            let event = SyncEvent {
                origin_peer: peer_id.clone(),
                sequence: seq,
                project_guid,
                domain: SyncDomain::Transport(transport),
                created_at_ms: position_read_at,
            };

            match event_tx.send(event) {
                Ok(n) => trace!("Master heartbeat sent (seq={seq}, {n} subscribers)"),
                Err(_) => debug!("No subscribers for heartbeat"),
            }
        }
    });
}
