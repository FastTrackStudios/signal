//! `TransportSync` for the REAPER backend: a remote client follows
//! REAPER's transport to the sample.
//!
//! The numbers are daw-audio-sync's: its audio hooks stamp one
//! [`AudioSnapshot`](daw_audio_sync::AudioSnapshot) per buffer in
//! REAPER's `time_precise` clock — per project in the multi-project
//! [`registry`](daw_audio_sync::registry) when the bridge registered it,
//! otherwise the current project in the single global cell. This module
//! serves them:
//!
//! - `clock_now` reads that same clock ([`daw_audio_sync::clock_micros`])
//!   on the connection's task — never the main thread, whose ~30 Hz
//!   timer would put the stamp anywhere in the round trip.
//! - `snapshot` resolves the project on the main thread (project
//!   pointers and GUIDs are main-thread-only) and reads its cell.
//! - the `positions` stream is fed by a pump off the main thread: the
//!   cells are lock-free, so it reads them every few milliseconds and
//!   publishes what a follower needs ([`PublishGate`]: every change, a
//!   keepalive between). Which slot is which project GUID is refreshed
//!   from the main thread a few times a second.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use daw_audio_sync::registry::MAX_PROJECTS;
use daw_audio_sync::{AudioSnapshot, ProjectId};
use daw_proto::{ProjectContext, StampedPosition, TransportSync, TransportSyncStreamSource};
use daw_transport_sync::PublishGate;
use reaper_high::{Project, Reaper as ReaperHigh};

use crate::project_context::{find_project_by_guid, project_guid};

/// How often the pump reads the cells while anyone subscribes.
const PUMP_TICK: Duration = Duration::from_millis(5);
/// How often the pump checks for a first subscriber while there is none.
const PUMP_IDLE: Duration = Duration::from_millis(50);
/// How often the slot → project GUID map is refreshed (main thread).
const MAP_REFRESH: Duration = Duration::from_millis(250);

static PUMP_STARTED: AtomicBool = AtomicBool::new(false);

impl TransportSync for crate::Reaper {
    async fn clock_now(&self) -> f64 {
        daw_audio_sync::clock_micros(ReaperHigh::get().medium_reaper().low())
    }

    // Main thread (the dispatcher): project lookup is main-thread-only.
    fn snapshot(&self, project: ProjectContext) -> Option<StampedPosition> {
        let reaper = ReaperHigh::get();
        let project = match &project {
            ProjectContext::Current => reaper.current_project(),
            ProjectContext::Project(guid) => find_project_by_guid(guid)?,
        };
        let guid = project_guid(&project);
        let from_registry = daw_audio_sync::registry::global_registry().and_then(|reg| {
            let slot = reg.slot(reg.find_slot(project.raw())?)?;
            let (_, id) = slot.current()?;
            slot.cell.load().filter(|snap| snap.project_id == id)
        });
        let snap = match from_registry {
            Some(snap) => snap,
            // The single-cell hook samples the current project only.
            None if project == reaper.current_project() => daw_audio_sync::global_snapshot()?,
            None => return None,
        };
        Some(StampedPosition::from_snapshot(guid, &snap))
    }
}

impl TransportSyncStreamSource for crate::Reaper {
    // Called from the stream host's async attach path (a tokio task),
    // so the pump can be spawned here, on the first subscription.
    fn positions_hub(&self) -> &architect::PubSub<StampedPosition> {
        spawn_pump();
        crate::event_hub::hub().sync_positions_hub()
    }
}

/// Where the pump reads each project: registry slots by index (with the
/// id the slot held when mapped, so a reassigned slot is not misread),
/// and the current project's GUID for the single global cell.
#[derive(Default)]
struct ProjectMap {
    slots: Vec<(usize, ProjectId, String)>,
    current: Option<String>,
}

/// Build the map. **Main thread only.**
fn map_projects() -> ProjectMap {
    let reaper = ReaperHigh::get();
    let slots = daw_audio_sync::registry::global_registry()
        .map(|reg| {
            (0..MAX_PROJECTS)
                .filter_map(|i| {
                    let (project, id) = reg.slot(i)?.current()?;
                    Some((i, id, project_guid(&Project::new(project))))
                })
                .collect()
        })
        .unwrap_or_default();
    ProjectMap {
        slots,
        current: Some(project_guid(&reaper.current_project())),
    }
}

/// Every project's latest snapshot, by GUID.
fn read_snapshots(map: &ProjectMap) -> Vec<(String, AudioSnapshot)> {
    let mut out = Vec::new();
    if let Some(reg) = daw_audio_sync::registry::global_registry() {
        for (index, id, guid) in &map.slots {
            if let Some(snap) = reg
                .slot(*index)
                .and_then(|slot| slot.cell.load())
                .filter(|snap| snap.project_id == *id)
            {
                out.push((guid.clone(), snap));
            }
        }
    }
    if out.is_empty()
        && let (Some(guid), Some(snap)) = (&map.current, daw_audio_sync::global_snapshot())
    {
        out.push((guid.clone(), snap));
    }
    out
}

/// Spawn the sync-position pump (once per process).
fn spawn_pump() {
    if PUMP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::task::spawn(async move {
        let hub = crate::event_hub::hub().sync_positions_hub();
        let mut gates: HashMap<String, PublishGate> = HashMap::new();
        let mut map = ProjectMap::default();
        let mut mapped_at: Option<tokio::time::Instant> = None;
        loop {
            if hub.subscriber_count() == 0 {
                gates.clear();
                mapped_at = None;
                tokio::time::sleep(PUMP_IDLE).await;
                continue;
            }
            if mapped_at.is_none_or(|at| at.elapsed() >= MAP_REFRESH) {
                // `None`: the extension is going away — keep what we had.
                if let Some(fresh) = crate::main_thread::query(map_projects).await {
                    map = fresh;
                }
                mapped_at = Some(tokio::time::Instant::now());
            }
            let snapshots = read_snapshots(&map);
            gates.retain(|guid, _| snapshots.iter().any(|(g, _)| g == guid));
            for (guid, snap) in snapshots {
                if gates.entry(guid.clone()).or_default().offer(&snap) {
                    hub.publish(StampedPosition::from_snapshot(guid, &snap));
                }
            }
            tokio::time::sleep(PUMP_TICK).await;
        }
    });
}
