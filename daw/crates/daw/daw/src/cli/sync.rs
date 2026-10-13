//! `daw sync` — manage the daw-bridge sync runtime across multiple
//! locally-spawned REAPER instances.
//!
//! daw-bridge ships the sync engine + TCP peer mesh + heartbeat behind the
//! `FTS_SYNC_ENABLED=1` env gate (so production users don't pay for it).
//! These commands let you spin up two-or-more REAPERs with the gate flipped
//! on, wire them into a direct-TCP mesh, watch their `FTS_SYNC_EXT/*`
//! ext-state beacons, and shut everything down — without going through the
//! reaper-test harness.

use std::path::PathBuf;
use std::time::Duration;

use crate::rpc::Daw;
use eyre::Result;
use serde_json::{Value, json};

use crate::cli::{
    connect, discover_all_sockets, kill_reaper, profile_by_id, spawn_reaper_with_env,
};

/// REAPER ext-state section the sync runtime writes its beacons under.
const EXT_SECTION: &str = "FTS_SYNC_EXT";

/// A spawned sync-enabled REAPER instance.
pub struct SpawnedInstance {
    pub pid: u32,
    pub socket_path: PathBuf,
}

/// Spawn `count` REAPER instances with `FTS_SYNC_ENABLED=1`. Waits for each
/// instance's Unix socket to appear before spawning the next, so the rest of
/// the harness sees a stable set of sockets.
///
/// Returns the spawned instances in launch order. Caller is responsible for
/// teardown (use [`stop_all`] or [`kill_reaper`] per pid).
pub async fn spawn_sync_instances(count: u32, profile_id: &str) -> Result<Vec<SpawnedInstance>> {
    let profile = profile_by_id(profile_id)
        .ok_or_else(|| eyre::eyre!("Unknown DAW profile: {profile_id}"))?;
    let mut instances = Vec::with_capacity(count as usize);

    for i in 0..count {
        let before: std::collections::BTreeSet<_> =
            discover_all_sockets().into_iter().map(|(_, p)| p).collect();
        let pid = spawn_reaper_with_env(&profile, &[("FTS_SYNC_ENABLED", "1")])?;
        eprintln!("  [{i}] spawned REAPER (PID {pid}), waiting for socket…");

        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let socket_path = loop {
            if let Some(path) = discover_all_sockets()
                .into_iter()
                .map(|(_, p)| p)
                .find(|p| !before.contains(p))
            {
                break path;
            }
            if std::time::Instant::now() > deadline {
                kill_reaper(pid);
                return Err(eyre::eyre!(
                    "[{i}] Timed out waiting for REAPER (PID {pid}) socket"
                ));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        eprintln!("  [{i}] socket: {}", socket_path.display());
        instances.push(SpawnedInstance { pid, socket_path });
    }

    Ok(instances)
}

/// Status snapshot for a single sync-enabled REAPER instance.
#[derive(Debug)]
pub struct InstanceStatus {
    pub pid: u32,
    pub socket: PathBuf,
    pub status: Option<String>,
    pub peer_id: Option<String>,
    pub mesh_port: Option<String>,
    pub peer_count: Option<String>,
}

impl InstanceStatus {
    fn to_json(&self) -> Value {
        json!({
            "pid": self.pid,
            "socket": self.socket.display().to_string(),
            "status": self.status,
            "peer_id": self.peer_id,
            "mesh_port": self.mesh_port,
            "peer_count": self.peer_count,
        })
    }
}

struct ReadyPeer<'a> {
    status: &'a InstanceStatus,
    peer_id: &'a str,
    mesh_port: &'a str,
}

async fn read_instance(pid: u32, socket: PathBuf) -> Result<InstanceStatus> {
    let daw = connect(Some(socket.clone())).await?;
    let status = read_key(&daw, "status").await;
    let peer_id = read_key(&daw, "peer_id").await;
    let mesh_port = read_key(&daw, "mesh_port").await;
    let peer_count = read_key(&daw, "peer_count").await;
    Ok(InstanceStatus {
        pid,
        socket,
        status,
        peer_id,
        mesh_port,
        peer_count,
    })
}

async fn read_key(daw: &Daw, key: &str) -> Option<String> {
    daw.ext_state().get(EXT_SECTION, key).await.ok().flatten()
}

/// Fetch sync status for every locally-discovered REAPER socket.
pub async fn status_all() -> Result<Vec<InstanceStatus>> {
    let sockets = discover_all_sockets();
    let mut out = Vec::with_capacity(sockets.len());
    for (pid, socket) in sockets {
        match read_instance(pid, socket.clone()).await {
            Ok(s) => out.push(s),
            Err(e) => {
                eprintln!("warning: failed to read instance pid={pid}: {e}");
            }
        }
    }
    Ok(out)
}

/// Direct-TCP-connect every discovered sync-enabled REAPER to every other
/// one by writing the `connect_peers` ext-state value. The daw-bridge sync
/// runtime polls that key every second and dials the listed peers.
pub async fn connect_all() -> Result<()> {
    let instances = status_all().await?;
    let ready: Vec<_> = instances
        .iter()
        .filter_map(|status| {
            if status.status.as_deref() == Some("ready") {
                Some(ReadyPeer {
                    status,
                    peer_id: status.peer_id.as_deref()?,
                    mesh_port: status.mesh_port.as_deref()?,
                })
            } else {
                None
            }
        })
        .collect();

    if ready.len() < 2 {
        eyre::bail!(
            "Need at least 2 sync-ready REAPERs to connect, found {}. Did you `daw sync spawn --count 2` first?",
            ready.len()
        );
    }

    let connect_str: String = ready
        .iter()
        .map(|peer| format!("{}@127.0.0.1:{}", peer.peer_id, peer.mesh_port))
        .collect::<Vec<_>>()
        .join(",");

    for peer in &ready {
        let daw = connect(Some(peer.status.socket.clone())).await?;
        daw.ext_state()
            .set(EXT_SECTION, "connect_peers", &connect_str, false)
            .await
            .map_err(|e| eyre::eyre!("write connect_peers on pid={}: {e}", peer.status.pid))?;
    }
    println!(
        "Wrote connect_peers to {} REAPER(s):\n  {connect_str}",
        ready.len()
    );
    Ok(())
}

/// Kill every locally-discovered sync-enabled REAPER instance. Detects
/// "sync-enabled" by presence of an `FTS_SYNC_EXT/status` ext-state value.
pub async fn stop_all() -> Result<u32> {
    let instances = status_all().await?;
    let mut killed = 0;
    for inst in instances {
        if inst.status.is_some() {
            if kill_reaper(inst.pid) {
                println!("  killed PID {}", inst.pid);
                let _ = std::fs::remove_file(&inst.socket);
                killed += 1;
            } else {
                eprintln!("  warning: failed to kill PID {}", inst.pid);
            }
        }
    }
    Ok(killed)
}

/// Pretty-print `status_all` output as a table.
pub fn print_status_table(instances: &[InstanceStatus]) {
    if instances.is_empty() {
        println!("No REAPER instances found.");
        return;
    }
    println!(
        "{:>8}  {:<12}  {:<24}  {:<10}  {:<6}  socket",
        "PID", "status", "peer_id", "mesh_port", "peers"
    );
    for s in instances {
        println!(
            "{:>8}  {:<12}  {:<24}  {:<10}  {:<6}  {}",
            s.pid,
            s.status.as_deref().unwrap_or("-"),
            s.peer_id.as_deref().unwrap_or("-"),
            s.mesh_port.as_deref().unwrap_or("-"),
            s.peer_count.as_deref().unwrap_or("-"),
            s.socket.display(),
        );
    }
}

/// JSON view of `status_all` output.
pub fn status_json(instances: &[InstanceStatus]) -> Value {
    Value::Array(instances.iter().map(InstanceStatus::to_json).collect())
}
