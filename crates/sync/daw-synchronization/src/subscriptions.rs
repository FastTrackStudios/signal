//! Per-project subscriptions that forward local DAW events as `SyncEvent`s.
//!
//! Subscribes to the daw facade's cross-domain architect `#[subscribe]`
//! event bus (`Daw::events()`) and wraps every event in a [`SyncEvent`]
//! envelope with this peer's identity. This is backend-agnostic — it works
//! against any daw backend that publishes to the bus (REAPER, daw-standalone,
//! …), replacing the old per-domain `daw_reaper::event_hub()` broadcast
//! forwarders that only worked in-process with the REAPER bridge.
//!
//! One forwarder task per project; the [`BusFilter`] (built from
//! [`SyncConfig`]) drops disabled domains and other projects client-side.
//! Cancellation is via a single [`CancellationToken`] held by the returned
//! [`ProjectSubscriptions`].

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use daw::service::event_bus::{BusFilter, DawEvent};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::suppression::SuppressionSet;
use crate::{SyncConfig, SyncDomain, SyncEvent};

/// Handle to a set of subscriptions for a single project.
///
/// Dropping cancels the forwarder task via the shared cancellation token.
pub struct ProjectSubscriptions {
    project_guid: String,
    cancel: CancellationToken,
}

impl ProjectSubscriptions {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn project_guid(&self) -> &str {
        &self.project_guid
    }
}

impl Drop for ProjectSubscriptions {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Build the bus filter for a project from the sync config. Transport
/// *state* changes replicate; the 30 Hz position firehose does not (sync
/// forwards discrete state, not the continuous playhead). `Project` events
/// are not forwarded (no project-domain sync yet). Takes follow `items`.
fn bus_filter(config: &SyncConfig, project_guid: &str) -> BusFilter {
    BusFilter {
        tracks: config.tracks,
        fx: config.fx,
        markers: config.markers,
        regions: config.regions,
        tempo_map: config.tempo_map,
        transport_state: config.transport,
        transport_position: false,
        projects: false,
        items: config.items,
        takes: config.items,
        routing: config.routing,
        project_guid: Some(project_guid.to_string()),
    }
}

/// Spawn the forwarder task for the given project.
///
/// Reads the daw event bus, wraps each event in a `SyncEvent`, and
/// broadcasts on `event_tx`. Echo suppression happens at broadcast time
/// inside the engine (`SynchronizationEngine::broadcast_local`); this
/// forwarder is intentionally suppression-free so the engine sees every
/// local event.
#[allow(clippy::too_many_arguments)]
pub async fn subscribe_project(
    daw: &daw::rpc::Daw,
    _project: &daw::rpc::Project,
    project_guid: String,
    peer_id: String,
    sequence: Arc<AtomicU64>,
    event_tx: broadcast::Sender<SyncEvent>,
    _suppression: Arc<Mutex<SuppressionSet>>,
    config: &SyncConfig,
) -> Result<ProjectSubscriptions, daw::rpc::Error> {
    let cancel = CancellationToken::new();
    let ctx = ForwarderCtx {
        project_guid: project_guid.clone(),
        peer_id,
        sequence,
        event_tx,
        cancel: cancel.clone(),
    };

    spawn_bus_forwarder(ctx, daw.clone(), bus_filter(config, &project_guid));

    Ok(ProjectSubscriptions {
        project_guid,
        cancel,
    })
}

/// Spawn a watcher that auto-subscribes new projects as they open.
///
/// Currently a no-op stub. Project open/close events aren't yet wired
/// through the streaming surface; once wired, this should subscribe and
/// call `subscribe_project` for new projects.
#[allow(clippy::too_many_arguments)]
pub fn watch_projects(
    _daw: daw::rpc::Daw,
    _peer_id: String,
    _sequence: Arc<AtomicU64>,
    _event_tx: broadcast::Sender<SyncEvent>,
    _suppression: Arc<Mutex<SuppressionSet>>,
    _config: SyncConfig,
    _project_subs: Arc<Mutex<Vec<ProjectSubscriptions>>>,
) -> CancellationToken {
    CancellationToken::new()
}

// ── Forwarder plumbing ──────────────────────────────────────────────

#[derive(Clone)]
struct ForwarderCtx {
    project_guid: String,
    peer_id: String,
    sequence: Arc<AtomicU64>,
    event_tx: broadcast::Sender<SyncEvent>,
    cancel: CancellationToken,
}

impl ForwarderCtx {
    fn wrap(&self, project_guid: String, domain: SyncDomain) -> SyncEvent {
        SyncEvent {
            origin_peer: self.peer_id.clone(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            project_guid,
            domain,
            created_at_ms: SyncEvent::now_ms(),
        }
    }
}

/// One task per project: consume the daw event bus and forward each event
/// as a `SyncEvent`. The `BusFilter` already scopes to this project and
/// drops disabled domains, so the mapping is a straight variant dispatch.
fn spawn_bus_forwarder(ctx: ForwarderCtx, daw: daw::rpc::Daw, filter: BusFilter) {
    tokio::task::spawn(async move {
        let mut stream = match daw.events().subscribe(filter).await {
            Ok(s) => s,
            Err(e) => {
                debug!("sync bus subscribe failed: {e:?}");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = ctx.cancel.cancelled() => break,
                recv = stream.recv() => match recv {
                    Ok(Some(sr)) => {
                        let Some((guid, domain)) = to_sync_domain(&daw, sr.get()).await else {
                            continue;
                        };
                        let _ = ctx.event_tx.send(ctx.wrap(guid, domain));
                    }
                    Ok(None) => break,
                    Err(e) => {
                        debug!("sync bus stream error: {e:?}");
                        break;
                    }
                }
            }
        }
    });
}

/// Map one `DawEvent` to its `(project_guid, SyncDomain)`. Returns `None`
/// for events the sync layer doesn't replicate (position ticks, project
/// events).
async fn to_sync_domain(daw: &daw::rpc::Daw, ev: &DawEvent) -> Option<(String, SyncDomain)> {
    use daw::service::{ItemEvent, RoutingEvent, TakeEvent, TransportEvent};
    Some(match ev {
        // Not a session change at all — the subscription saying it is
        // live. `daw_control::Events` consumes it, so this is only
        // reachable by something reading the raw stream.
        DawEvent::Attached => return None,
        DawEvent::Track(e) => (e.project_guid.clone(), SyncDomain::Track(e.event.clone())),
        DawEvent::Fx(e) => (e.project_guid.clone(), SyncDomain::Fx(e.event.clone())),
        DawEvent::Marker(e) => (e.project_guid.clone(), SyncDomain::Marker(e.event.clone())),
        DawEvent::Region(e) => (e.project_guid.clone(), SyncDomain::Region(e.event.clone())),
        DawEvent::TempoMap(e) => (
            e.project_guid.clone(),
            SyncDomain::TempoMap(e.event.clone()),
        ),
        DawEvent::TransportState(e) => {
            let guid = transport_guid(e).to_string();
            // SyncDomain::Transport carries a full snapshot. For non-Snapshot
            // deltas, re-query current state and forward that (snapshot-style
            // sync — every change publishes the full post-change state).
            let snapshot = match e {
                TransportEvent::Snapshot { state, .. } => state.clone(),
                _ => current_transport(daw).await?,
            };
            (guid, SyncDomain::Transport(snapshot))
        }
        DawEvent::Item(e) => (e.project_guid().to_string(), SyncDomain::Item(e.clone())),
        DawEvent::Take(e) => (e.project_guid().to_string(), SyncDomain::Take(e.clone())),
        DawEvent::Routing(e) => (routing_guid(e).to_string(), SyncDomain::Routing(e.clone())),
        // Not replicated: 30 Hz position firehose, project open/close.
        DawEvent::TransportPosition(_) | DawEvent::Project(_) => return None,
    })
}

async fn current_transport(daw: &daw::rpc::Daw) -> Option<daw::service::Transport> {
    let project = daw.current_project().await.ok()?;
    project.transport().get_state().await.ok()
}

fn transport_guid(e: &daw::service::TransportEvent) -> &str {
    use daw::service::TransportEvent::*;
    match e {
        Snapshot { project_guid, .. }
        | PlayStateChanged { project_guid, .. }
        | RecordModeChanged { project_guid, .. }
        | LoopingChanged { project_guid, .. }
        | MetronomeChanged { project_guid, .. }
        | LoopRegionChanged { project_guid, .. }
        | TimeSelectionChanged { project_guid, .. }
        | TempoChanged { project_guid, .. }
        | PlayrateChanged { project_guid, .. } => project_guid,
    }
}

fn routing_guid(e: &daw::service::RoutingEvent) -> &str {
    use daw::service::RoutingEvent::*;
    match e {
        RouteCreated { project_guid, .. }
        | RouteDeleted { project_guid, .. }
        | VolumeChanged { project_guid, .. }
        | PanChanged { project_guid, .. }
        | MuteChanged { project_guid, .. }
        | ParentSendChanged { project_guid, .. } => project_guid,
    }
}
