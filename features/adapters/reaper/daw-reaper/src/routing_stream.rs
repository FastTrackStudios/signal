//! Routing (sends / receives / hardware outputs) polling + broadcasting.
//!
//! Mirrors `item.rs`'s own-broadcaster pattern. Called from the
//! daw-bridge timer on the main thread.
//!
//! Diff strategy:
//! - For each open project, for each track, snapshot sends + receives
//!   + hardware_outputs.
//! - Cache keyed by `(project_guid, track_guid, route_type)`.
//! - Emit `RouteCreated`, `RouteDeleted`, `VolumeChanged`,
//!   `PanChanged`, `MuteChanged` per the field-level diff.

use daw_control::lock::LockExt;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use daw_proto::{ProjectContext, RouteType, Routing, RoutingEvent, TrackRef, TrackRoute, Tracks};
use reaper_high::{Project, Reaper as ReaperHigh};
use reaper_medium::ProjectRef;
use tokio::sync::broadcast;

use crate::project_context::{MAX_PROJECT_TABS, project_guid};

#[derive(Clone, Debug)]
struct CachedRoute {
    /// Snapshot index within the per-track route list of this type.
    index: u32,
    /// Destination signature — track GUID for track sends/receives,
    /// "hw:N" for hardware outputs.
    dest_key: String,
    volume: f64,
    pan: f64,
    muted: bool,
}

impl CachedRoute {
    fn from_route(r: &TrackRoute) -> Self {
        let dest_key = if let Some(guid) = &r.dest_track_guid {
            format!("track:{guid}")
        } else if let Some(idx) = r.hw_output_index {
            format!("hw:{idx}")
        } else {
            String::new()
        };
        Self {
            index: r.index,
            dest_key,
            volume: r.volume,
            pan: r.pan,
            muted: r.muted,
        }
    }
}

/// Cache key: (project_guid, source_track_guid, route_type).
type RouteKey = (String, String, RouteType);

static ROUTING_BROADCASTER: OnceLock<broadcast::Sender<RoutingEvent>> = OnceLock::new();
static ROUTING_CACHE: OnceLock<Mutex<HashMap<RouteKey, Vec<CachedRoute>>>> = OnceLock::new();

const VOLUME_THRESHOLD: f64 = 0.0001;
const PAN_THRESHOLD: f64 = 0.001;

/// Initialize the routing event broadcaster. Idempotent.
pub fn init_routing_broadcaster() {
    let (tx, _rx) = broadcast::channel::<RoutingEvent>(1024);
    let _ = ROUTING_BROADCASTER.set(tx);
    let _ = ROUTING_CACHE.set(Mutex::new(HashMap::new()));
}

/// Subscribe to routing events. Returns `None` until [`init_routing_broadcaster`] runs.
pub fn subscribe_routing() -> Option<broadcast::Receiver<RoutingEvent>> {
    ROUTING_BROADCASTER.get().map(|tx| tx.subscribe())
}

/// Push-published routing event from the control-surface callback path.
/// Skips publish if the affected route isn't in our cache yet — the
/// follower hasn't seen the RouteCreated event for it, so emitting field
/// changes ahead of the create just floods the apply pipeline with
/// "route not found" lookups and starves the create event. The next
/// poll tick picks up both the route + its initial state in order.
/// Send a routing event to the in-process broadcaster **and** the bus.
///
/// No project guid to resolve: unlike an `FxEvent`, every `RoutingEvent`
/// already carries its own, so the bus copy is the event itself.
fn emit(tx: &broadcast::Sender<RoutingEvent>, event: RoutingEvent) {
    let _ = tx.send(event.clone());
    crate::event_hub::hub().publish_routing(event);
}

pub(crate) fn publish_from_callback(event: RoutingEvent) {
    let Some(tx) = ROUTING_BROADCASTER.get() else {
        return;
    };
    let Some(cache_cell) = ROUTING_CACHE.get() else {
        return;
    };
    let cached = match cache_cell.lock() {
        Ok(mut cache) => apply_callback_to_cache(&mut cache, &event),
        Err(_) => false,
    };
    if !cached {
        return;
    }
    emit(tx, event);
}

/// Returns true if the cache had the affected route (and the patch
/// was applied), false otherwise. Callers use this as a signal to skip
/// publishing — see `publish_from_callback`.
fn apply_callback_to_cache(
    cache: &mut HashMap<RouteKey, Vec<CachedRoute>>,
    event: &RoutingEvent,
) -> bool {
    match event {
        RoutingEvent::VolumeChanged {
            project_guid,
            source_track_guid,
            route_type,
            route_index,
            volume,
        } => {
            let Some(routes) =
                cache.get_mut(&(project_guid.clone(), source_track_guid.clone(), *route_type))
            else {
                return false;
            };
            let Some(r) = routes.iter_mut().find(|r| r.index == *route_index) else {
                return false;
            };
            r.volume = *volume;
            true
        }
        RoutingEvent::PanChanged {
            project_guid,
            source_track_guid,
            route_type,
            route_index,
            pan,
        } => {
            let Some(routes) =
                cache.get_mut(&(project_guid.clone(), source_track_guid.clone(), *route_type))
            else {
                return false;
            };
            let Some(r) = routes.iter_mut().find(|r| r.index == *route_index) else {
                return false;
            };
            r.pan = *pan;
            true
        }
        RoutingEvent::MuteChanged {
            project_guid,
            source_track_guid,
            route_type,
            route_index,
            muted,
        } => {
            let Some(routes) =
                cache.get_mut(&(project_guid.clone(), source_track_guid.clone(), *route_type))
            else {
                return false;
            };
            let Some(r) = routes.iter_mut().find(|r| r.index == *route_index) else {
                return false;
            };
            r.muted = *muted;
            true
        }
        _ => true,
    }
}

/// Poll REAPER routing state for every open project. **Main thread only.**
pub fn poll_and_broadcast_routing() {
    let Some(tx) = ROUTING_BROADCASTER.get() else {
        return;
    };
    // Either audience: the in-process broadcaster's subscribers, or anyone
    // on the cross-domain bus. Same gate as the FX poller, and the same
    // reason — a bus subscriber alone used to see nothing.
    if tx.receiver_count() == 0 && crate::event_hub::hub().routing_subscriber_count() == 0 {
        return;
    }
    let Some(cache_cell) = ROUTING_CACHE.get() else {
        return;
    };
    let mut cache = cache_cell.lock_recoverable("routing_stream::cache");

    let medium = ReaperHigh::get().medium_reaper();

    let mut seen_keys: Vec<RouteKey> = Vec::new();

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = Project::new(result.project);
        let project_guid_str = project_guid(&project);
        let project_ctx = ProjectContext::Project(project_guid_str.clone());

        let tracks = Tracks::all(&crate::Reaper, project_ctx.clone());

        for track in &tracks {
            let track_ref = TrackRef::Guid(track.guid.clone());

            for route_type in [
                RouteType::Send,
                RouteType::Receive,
                RouteType::HardwareOutput,
            ] {
                let fresh: Vec<TrackRoute> = match route_type {
                    RouteType::Send => {
                        Routing::sends(&crate::Reaper, project_ctx.clone(), track_ref.clone())
                    }
                    RouteType::Receive => {
                        Routing::receives(&crate::Reaper, project_ctx.clone(), track_ref.clone())
                    }
                    RouteType::HardwareOutput => Routing::hardware_outputs(
                        &crate::Reaper,
                        project_ctx.clone(),
                        track_ref.clone(),
                    ),
                };
                let key = (project_guid_str.clone(), track.guid.clone(), route_type);
                seen_keys.push(key.clone());

                let fresh_cached: Vec<CachedRoute> =
                    fresh.iter().map(CachedRoute::from_route).collect();

                match cache.get(&key) {
                    None => {
                        // First poll for this slot — seed only.
                    }
                    Some(prev) => {
                        diff_and_emit(
                            tx,
                            &project_guid_str,
                            &track.guid,
                            route_type,
                            prev,
                            &fresh_cached,
                            &fresh,
                        );
                    }
                }
                cache.insert(key, fresh_cached);
            }
        }
    }

    cache.retain(|k, _| seen_keys.contains(k));
}

fn diff_and_emit(
    tx: &broadcast::Sender<RoutingEvent>,
    project_guid: &str,
    source_track_guid: &str,
    route_type: RouteType,
    prev: &[CachedRoute],
    curr: &[CachedRoute],
    curr_full: &[TrackRoute],
) {
    use std::collections::HashMap;

    let prev_by_dest: HashMap<&str, &CachedRoute> =
        prev.iter().map(|r| (r.dest_key.as_str(), r)).collect();
    let curr_by_dest: HashMap<&str, &CachedRoute> =
        curr.iter().map(|r| (r.dest_key.as_str(), r)).collect();

    // Deleted: in prev but not in curr.
    for p in prev {
        if !p.dest_key.is_empty() && !curr_by_dest.contains_key(p.dest_key.as_str()) {
            emit(
                tx,
                RoutingEvent::RouteDeleted {
                    project_guid: project_guid.to_string(),
                    source_track_guid: source_track_guid.to_string(),
                    route_type,
                    route_index: p.index,
                },
            );
        }
    }

    // Created / changed.
    for (i, c) in curr.iter().enumerate() {
        if c.dest_key.is_empty() {
            continue;
        }
        match prev_by_dest.get(c.dest_key.as_str()) {
            None => {
                if let Some(route) = curr_full.get(i) {
                    emit(
                        tx,
                        RoutingEvent::RouteCreated {
                            project_guid: project_guid.to_string(),
                            source_track_guid: source_track_guid.to_string(),
                            route: route.clone(),
                        },
                    );
                }
            }
            Some(prev) => {
                if (prev.volume - c.volume).abs() > VOLUME_THRESHOLD {
                    emit(
                        tx,
                        RoutingEvent::VolumeChanged {
                            project_guid: project_guid.to_string(),
                            source_track_guid: source_track_guid.to_string(),
                            route_type,
                            route_index: c.index,
                            volume: c.volume,
                        },
                    );
                }
                if (prev.pan - c.pan).abs() > PAN_THRESHOLD {
                    emit(
                        tx,
                        RoutingEvent::PanChanged {
                            project_guid: project_guid.to_string(),
                            source_track_guid: source_track_guid.to_string(),
                            route_type,
                            route_index: c.index,
                            pan: c.pan,
                        },
                    );
                }
                if prev.muted != c.muted {
                    emit(
                        tx,
                        RoutingEvent::MuteChanged {
                            project_guid: project_guid.to_string(),
                            source_track_guid: source_track_guid.to_string(),
                            route_type,
                            route_index: c.index,
                            muted: c.muted,
                        },
                    );
                }
            }
        }
    }
}
