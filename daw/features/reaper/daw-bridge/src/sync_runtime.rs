//! Sync runtime — hosts the synchronization engine + TCP peer mesh inside
//! the daw-bridge REAPER extension.
//!
//! Spawned from `plugin_main` when `FTS_SYNC_ENABLED=1` is set (default
//! production daw-bridge users don't pay for sync). Writes the
//! `FTS_SYNC_EXT/*` ext_state beacons the multi-instance integration tests
//! poll for orchestration:
//!
//! | Key            | Direction | Purpose                                   |
//! |----------------|-----------|-------------------------------------------|
//! | `status`       | out       | `"ready"` once engine + mesh are live.    |
//! | `pid`          | out       | This process's PID.                       |
//! | `peer_id`      | out       | Mesh peer id (= `reaper-<pid>`).          |
//! | `mesh_port`    | out       | TCP port the mesh listener bound to.      |
//! | `peer_count`   | out       | # of connected remote peers (excl self).  |
//! | `connect_peers`| in        | `peer@host:port,...` → trigger direct TCP |
//!
//! No mDNS yet — the avahi dep would bloat daw-bridge. Tests connect via
//! `connect_sync_peers_direct` which writes `connect_peers` ext_state.

use std::sync::Arc;
use std::time::Duration;

use daw::rpc::Daw;
use daw_network::{MeshConfig, MeshPeerEvent, PeerMesh};
use daw_synchronization::{SyncConfig, SyncSession, SynchronizationEngine, heartbeat};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

const EXT_SECTION: &str = "FTS_SYNC_EXT";

/// Spawn the sync runtime. Returns once the engine + mesh are bound and
/// the `status=ready` beacon has been written. Background tasks for
/// inbound-event application, direct-connect polling, and peer-count
/// reporting keep running on the daw-bridge tokio runtime.
pub async fn start(daw: Daw) -> eyre::Result<()> {
    let pid = std::process::id();
    info!("[sync:{pid}] starting sync runtime");

    let session = SyncSession {
        peer_id: format!("reaper-{pid}"),
        session_id: "default".to_string(),
        display_name: format!("REAPER ({pid})"),
    };
    let engine = Arc::new(SynchronizationEngine::new(
        daw.clone(),
        session.clone(),
        SyncConfig::all(),
    ));
    engine
        .start()
        .await
        .map_err(|e| eyre::eyre!("engine start: {e}"))?;
    info!("[sync:{pid}] engine started");

    let mesh_config = MeshConfig {
        peer_id: session.peer_id.clone(),
        session_id: session.session_id.clone(),
    };
    let (mesh, incoming_rx, peer_events_rx) = PeerMesh::bind(
        mesh_config,
        engine.subscribe(),
        Arc::clone(engine.suppression()),
    )
    .await?;
    let mesh = Arc::new(mesh);
    let mesh_port = mesh.local_port();
    info!("[sync:{pid}] peer mesh bound on port {mesh_port}");

    // Beacons for test orchestration.
    let beacons: [(&str, String); 4] = [
        ("status", "ready".to_string()),
        ("pid", pid.to_string()),
        ("peer_id", session.peer_id.clone()),
        ("mesh_port", mesh_port.to_string()),
    ];
    for (key, value) in &beacons {
        daw.ext_state()
            .set(EXT_SECTION, key, value, false)
            .await
            .map_err(|e| eyre::eyre!("ext_state set {key}: {e}"))?;
    }
    info!("[sync:{pid}] health beacons written");

    // Background loops — keep them detached on the daw-bridge runtime.
    tokio::task::spawn(handle_incoming(Arc::clone(&engine), incoming_rx));
    tokio::task::spawn(handle_peer_events(Arc::clone(&engine), peer_events_rx));
    tokio::task::spawn(poll_connect_peers(
        daw.clone(),
        Arc::clone(&mesh),
        session.peer_id.clone(),
    ));
    tokio::task::spawn(report_peer_count(daw.clone(), Arc::clone(&mesh)));

    // Master position heartbeat — drives the drift corrector on followers.
    heartbeat::spawn(
        daw,
        session.peer_id.clone(),
        engine.sequence(),
        engine.event_sender(),
        engine.is_master_flag(),
        engine.link_active_flag(),
        engine.sync_enabled_flag(),
    );

    Ok(())
}

/// Pipe mesh connect/disconnect events into engine peer tracking so
/// master election (lowest peer_id wins) reflects the current set.
async fn handle_peer_events(
    engine: Arc<SynchronizationEngine>,
    mut rx: tokio::sync::mpsc::Receiver<MeshPeerEvent>,
) {
    while let Some(event) = rx.recv().await {
        match event {
            MeshPeerEvent::Connected(peer_id) => {
                engine.add_peer(peer_id).await;
            }
            MeshPeerEvent::Disconnected(peer_id) => {
                engine.remove_peer(&peer_id).await;
            }
        }
    }
}

/// Pipe inbound peer events into `engine.apply_remote`.
async fn handle_incoming(
    engine: Arc<SynchronizationEngine>,
    mut rx: tokio::sync::mpsc::Receiver<daw_synchronization::SyncEvent>,
) {
    let pid = std::process::id();
    while let Some(event) = rx.recv().await {
        debug!(
            "[sync:{pid}] inbound event from {} seq={}",
            event.origin_peer, event.sequence
        );
        engine.apply_remote(&event).await;
    }
    info!("[sync:{pid}] inbound event stream closed");
}

/// Poll `FTS_SYNC_EXT/connect_peers` once per second; connect any new
/// addresses listed. Format: `peer@host:port,peer@host:port`.
async fn poll_connect_peers(daw: Daw, mesh: Arc<PeerMesh>, local_peer_id: String) {
    let pid = std::process::id();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let last = Arc::new(Mutex::new(String::new()));
    loop {
        interval.tick().await;
        let value = match daw.ext_state().get(EXT_SECTION, "connect_peers").await {
            Ok(Some(v)) if !v.is_empty() => v,
            _ => continue,
        };
        {
            let mut guard = last.lock().await;
            if value == *guard {
                continue;
            }
            *guard = value.clone();
        }
        for entry in value.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let Some((peer_id, addr_str)) = entry.split_once('@') else {
                debug!("[sync:{pid}] bad connect_peers entry: {entry}");
                continue;
            };
            if peer_id == local_peer_id {
                continue;
            }
            match addr_str.parse() {
                Ok(addr) => {
                    info!("[sync:{pid}] connect_peers: dialing {peer_id} at {addr}");
                    mesh.connect_peer(addr, peer_id.to_string()).await;
                }
                Err(e) => {
                    warn!("[sync:{pid}] bad address '{addr_str}': {e}");
                }
            }
        }
    }
}

/// Publish the current peer count to ext_state every 500 ms.
async fn report_peer_count(daw: Daw, mesh: Arc<PeerMesh>) {
    let pid = std::process::id();
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let mut last: Option<usize> = None;
    loop {
        interval.tick().await;
        let count = mesh.peer_count().await;
        if last == Some(count) {
            continue;
        }
        last = Some(count);
        if let Err(e) = daw
            .ext_state()
            .set(EXT_SECTION, "peer_count", &count.to_string(), false)
            .await
        {
            debug!("[sync:{pid}] failed to write peer_count: {e}");
        }
    }
}
