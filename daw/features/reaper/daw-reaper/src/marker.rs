//! `Reaper` — the REAPER backend singleton.
//!
//! After the `#[architect::rpc]` port, backends are stateless: they
//! don't hold a project guid, they don't hold a TaskSupport handle,
//! they don't allocate at construction. The full mount surface is:
//!
//! ```ignore
//! use daw_proto::marker;
//! router.mount(marker::serve(Reaper));
//! ```
//!
//! `Reaper` carries:
//!
//! - The dispatcher (via [`architect::HasDispatcher`]) — every sync
//!   method goes through [`ReaperMainThreadDispatcher`] to land on
//!   the REAPER main thread before touching the C API.
//! - Per-service trait impls (`Markers`, eventually `Items`, `Tracks`,
//!   …). Each method takes `ProjectContext` so one `Reaper` instance
//!   serves every project the binary touches.
//!
//! Tests / alternate-runtime scenarios that want a different dispatcher
//! (e.g. `CurrentThreadDispatcher` for unit tests without REAPER
//! running) use a newtype wrapper with its own `HasDispatcher` impl.

use daw_control::lock::LockExt;
use std::any::Any;
use std::ffi::CString;
use std::future::Future;
use std::pin::Pin;

use architect::HasDispatcher;
use architect::dispatch::{BoxedAny, DispatchError, Dispatcher};
use daw_proto::Markers;
use daw_proto::{DawError, DawResult, Marker, Position, ProjectContext, TimePosition};
use reaper_high::Reaper as ReaperHigh;
use reaper_medium::{MarkerOrRegionPosition, PositionInSeconds};

use crate::main_thread;
use crate::project_context::resolve_project_context;
use crate::safe_wrappers::markers as sw;
use crate::safe_wrappers::ruler_lanes;

// ── Dispatcher ─────────────────────────────────────────────────────────

/// Marshals sync closures onto REAPER's main thread via
/// [`main_thread::query`]. Use this with any architect-rpc host whose
/// backend touches REAPER state.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReaperMainThreadDispatcher;

impl Dispatcher for ReaperMainThreadDispatcher {
    fn dispatch(
        &self,
        f: Box<dyn FnOnce() -> BoxedAny + Send + 'static>,
    ) -> Pin<Box<dyn Future<Output = Result<BoxedAny, DispatchError>> + Send + 'static>> {
        Box::pin(async move {
            // `main_thread::query` returns `None` when TaskSupport
            // isn't installed (extension not yet bootstrapped or
            // already torn down). Map that to `ShutDown` so callers
            // see a clean error instead of a silent panic.
            match main_thread::query(f).await {
                Some(any) => Ok(any),
                None => Err(DispatchError::ShutDown),
            }
        })
    }
}

// ── Backend singleton ──────────────────────────────────────────────────

/// REAPER backend identity. Stateless — project context flows through
/// each method call. Mount with `marker::serve(Reaper)` (and the
/// per-service `serve` of every other trait `Reaper` impls).
#[derive(Debug, Default, Clone, Copy)]
pub struct Reaper;

impl HasDispatcher for Reaper {
    type Dispatcher = ReaperMainThreadDispatcher;

    fn dispatcher(&self) -> Self::Dispatcher {
        ReaperMainThreadDispatcher
    }
}

// ── Markers impl ───────────────────────────────────────────────────────

impl Markers for Reaper {
    fn all(&self, project: ProjectContext) -> Vec<Marker> {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();
        let ctx = resolve_project_context(&project);
        let mut markers = Vec::new();

        let total_count = medium.count_project_markers(ctx).total_count;
        for idx in 0..total_count {
            medium.enum_project_markers_3(ctx, idx, |result| {
                if let Some(info) = result
                    && info.region_end_position.is_none()
                {
                    let id = info.id.get();
                    let lane = ruler_lanes::assigned_lane(low, ctx, false, id)
                        .or_else(|| ruler_lanes::get_marker_lane(low, ctx, idx));
                    markers.push(Marker {
                        id: Some(id),
                        position: Position::from_time(TimePosition::from_seconds(
                            info.position.get(),
                        )),
                        name: info.name.to_string(),
                        color: {
                            let c = info.color.to_raw();
                            if c != 0 { Some(c as u32) } else { None }
                        },
                        guid: None,
                        lane,
                    });
                }
            });
        }

        markers.sort_by(|a, b| {
            a.position_seconds()
                .partial_cmp(&b.position_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        markers
    }

    fn get(&self, project: ProjectContext, id: u32) -> Option<Marker> {
        // Fully-qualified — `Reaper` impls multiple service traits
        // (`Markers`, `Tracks`, …) and `self.all(...)` would be
        // ambiguous between them.
        <Self as Markers>::all(self, project)
            .into_iter()
            .find(|m| m.id == Some(id))
    }

    fn count(&self, project: ProjectContext) -> u32 {
        let medium = ReaperHigh::get().medium_reaper();
        let ctx = resolve_project_context(&project);
        let total = medium.count_project_markers(ctx).total_count;
        let mut n = 0u32;
        for idx in 0..total {
            medium.enum_project_markers_3(ctx, idx, |result| {
                if let Some(info) = result
                    && info.region_end_position.is_none()
                {
                    n += 1;
                }
            });
        }
        n
    }

    fn add(&self, project: ProjectContext, position: f64, name: &str) -> DawResult<u32> {
        let ctx = resolve_project_context(&project);
        let medium = ReaperHigh::get().medium_reaper();
        let pos = PositionInSeconds::new(position)
            .map_err(|e| DawError::operation_failed(format!("invalid position: {e:?}")))?;
        let id = medium
            .add_project_marker_2(ctx, MarkerOrRegionPosition::Marker(pos), name, None, None)
            .map_err(|e| DawError::operation_failed(format!("add marker failed: {e:?}")))?;
        Ok(id)
    }

    fn add_with_guid(
        &self,
        _project: ProjectContext,
        _guid: &str,
        _position: f64,
        _name: &str,
    ) -> DawResult<u32> {
        // `MARKER_GUID:X` is read-only in `GetSetProjectInfo_String`, and
        // no other API call takes a marker GUID: REAPER mints its own.
        Err(DawError::not_supported(
            "REAPER cannot set a marker's GUID through its API",
        ))
    }

    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()> {
        let ctx = resolve_project_context(&project);
        let low = ReaperHigh::get().medium_reaper().low();
        sw::delete_project_marker(low, ctx, id as i32, false);
        Ok(())
    }

    fn set_position(&self, _project: ProjectContext, id: u32, position: f64) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        sw::set_project_marker(low, id as i32, false, position, 0.0, None);
        Ok(())
    }

    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        // REAPER's SetProjectMarker ignores -1.0 sentinels — passing -1.0 for
        // `pos` actually moves the marker to -1.0s. Read the current position
        // first so we only update the name.
        let cur = <Self as Markers>::get(self, project, id)
            .ok_or_else(|| DawError::operation_failed(format!("marker {id} not found")))?;
        let pos = cur.position.time.map(|t| t.as_seconds()).unwrap_or(0.0);
        let low = ReaperHigh::get().medium_reaper().low();
        let cname = CString::new(name)
            .map_err(|e| DawError::operation_failed(format!("invalid name: {e}")))?;
        sw::set_project_marker(low, id as i32, false, pos, 0.0, Some(&cname));
        Ok(())
    }

    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()> {
        let ctx = resolve_project_context(&project);
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let total_count = medium.count_project_markers(ctx).total_count;
        let reaper_color = (color | 0x01000000) as i32;

        let mut found = false;
        for idx in 0..total_count {
            medium.enum_project_markers_3(ctx, idx, |result| {
                if let Some(info) = result
                    && info.region_end_position.is_none()
                    && info.id.get() == id
                    && let Ok(name) = CString::new(info.name.to_string())
                {
                    sw::set_project_marker_by_index2(
                        low,
                        ctx,
                        idx as i32,
                        false,
                        info.position.get(),
                        0.0,
                        id as i32,
                        Some(&name),
                        reaper_color,
                        0,
                    );
                    found = true;
                }
            });
            if found {
                break;
            }
        }
        if !found {
            return Err(DawError::not_found("Marker", &id.to_string()));
        }
        Ok(())
    }

    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        let ctx = resolve_project_context(&project);
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let total_count = medium.count_project_markers(ctx).total_count;
        let lane = lane.unwrap_or(0);

        for idx in 0..total_count {
            let mut found = false;
            medium.enum_project_markers_3(ctx, idx, |result| {
                found = result
                    .as_ref()
                    .is_some_and(|info| info.region_end_position.is_none() && info.id.get() == id);
            });
            if found {
                if !ruler_lanes::set_marker_lane(low, ctx, idx, lane) {
                    return Err(DawError::operation_failed(
                        "REAPER ruler lane API is unavailable",
                    ));
                }
                ruler_lanes::remember_assigned_lane(low, ctx, false, id, lane);
                return Ok(());
            }
        }

        Err(DawError::not_found("Marker", &id.to_string()))
    }
}

impl daw_proto::marker::MarkersStreamSource for Reaper {
    fn events_hub(&self) -> &architect::PubSub<MarkerStreamEvent> {
        crate::event_hub::hub().markers_hub()
    }
}

// ── Compile-time tripwires ─────────────────────────────────────────────
//
// Fail the build (here, with an obvious diagnostic) if the wiring
// contract slips — these are cheaper to maintain than catching the
// same problem at every `marker::serve(Reaper)` callsite.

#[allow(dead_code)]
fn _assert_reaper_is_arc_safe() {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<Reaper>();
}

#[allow(dead_code)]
fn _assert_dispatcher_impls_trait() {
    fn assert_dispatcher<T: Dispatcher>() {}
    assert_dispatcher::<ReaperMainThreadDispatcher>();
}

#[allow(dead_code)]
fn _assert_reaper_has_dispatcher() {
    fn assert_has_dispatcher<T: HasDispatcher>() {}
    assert_has_dispatcher::<Reaper>();
}

#[allow(dead_code)]
fn _erase_unused() {
    // `Any` is referenced by `BoxedAny` indirectly; quiet the lint
    // without a top-of-file `#[allow]`.
    let _: Option<&dyn Any> = None;
}

// ── Streaming: poll + broadcast markers ────────────────────────────────

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use daw_proto::MarkerEvent;
use daw_proto::marker::MarkerStreamEvent;
use reaper_medium::ProjectRef;

use crate::project_context::{MAX_PROJECT_TABS, project_guid as project_guid_from};

static MARKER_CACHE: OnceLock<Mutex<HashMap<String, HashMap<u32, Marker>>>> = OnceLock::new();

fn marker_cache() -> &'static Mutex<HashMap<String, HashMap<u32, Marker>>> {
    MARKER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Is this the same marker, wearing a different number?
///
/// Everything a user can see, compared. The id is deliberately not:
/// the whole question is whether the id changed underneath something
/// that did not.
fn same_but_for_the_number(old: &Marker, new: &Marker) -> bool {
    old.name == new.name
        && old.position == new.position
        && old.color == new.color
        && old.lane == new.lane
}

/// Poll REAPER marker state for every open project and emit
/// `MarkerEvent`s for additions, removals, and modifications.
///
/// **MUST be called from the REAPER main thread.**
pub fn poll_and_broadcast_markers() {
    let hub = crate::event_hub::hub();
    if hub.markers_subscriber_count() == 0 {
        return;
    }

    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    let mut cache = marker_cache().lock_recoverable("marker::cache");

    let mut seen_projects: Vec<String> = Vec::new();

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = reaper_high::Project::new(result.project);
        let project_guid = project_guid_from(&project);
        seen_projects.push(project_guid.clone());

        let project_ctx = ProjectContext::Project(project_guid.clone());
        let fresh: Vec<Marker> = Markers::all(&Reaper, project_ctx);
        // Drop markers without a stable id — they can't participate
        // in diffing. REAPER assigns one to every real marker, so in
        // practice this is a no-op.
        let fresh_by_id: HashMap<u32, Marker> = fresh
            .into_iter()
            .filter_map(|m| m.id.map(|id| (id, m)))
            .collect();

        let prev = cache.entry(project_guid.clone()).or_default();

        // Pair the numbers that vanished with the ones that appeared,
        // before calling either a removal or an addition. REAPER's
        // renumber action reassigns every id at once, and diffed by id
        // that looks like the whole list being deleted and a set of
        // strangers arriving in its place — which costs a client its
        // selection, and costs a client mid-drag the right target.
        //
        // A pair is a removal and an addition identical in everything
        // but the number. Two markers alike in name, position, colour
        // and lane are indistinguishable to a user as well, so picking
        // the wrong one of those cannot produce a wrong answer.
        let gone: Vec<u32> = prev
            .keys()
            .copied()
            .filter(|id| !fresh_by_id.contains_key(id))
            .collect();
        let mut arrived: Vec<u32> = fresh_by_id
            .keys()
            .copied()
            .filter(|id| !prev.contains_key(id))
            .collect();
        let mut renumbered: HashMap<u32, u32> = HashMap::new();
        for old_id in &gone {
            let Some(old) = prev.get(old_id) else {
                continue;
            };
            let found = arrived.iter().position(|new_id| {
                fresh_by_id
                    .get(new_id)
                    .is_some_and(|new| same_but_for_the_number(old, new))
            });
            if let Some(at) = found {
                renumbered.insert(*old_id, arrived.remove(at));
            }
        }

        for (id, marker) in &fresh_by_id {
            // Its arrival was already reported as a renumbering.
            if renumbered.values().any(|new_id| new_id == id) {
                continue;
            }
            match prev.get(id) {
                None => hub.publish_marker(MarkerStreamEvent {
                    project_guid: project_guid.clone(),
                    event: MarkerEvent::Added(marker.clone()),
                }),
                Some(old) if old != marker => {
                    hub.publish_marker(MarkerStreamEvent {
                        project_guid: project_guid.clone(),
                        event: MarkerEvent::Changed(marker.clone()),
                    });
                }
                Some(_) => {}
            }
        }
        for (old_id, new_id) in &renumbered {
            let Some(marker) = fresh_by_id.get(new_id) else {
                continue;
            };
            // The lane shadow is filed under the number, so it has to
            // move with it. The SECTIONS lane is what makes a region a
            // song section — a lane lost in a renumber is a verse that
            // stops being a verse.
            crate::safe_wrappers::ruler_lanes::carry_assigned_lane(
                medium.low(),
                reaper_medium::ProjectContext::Proj(result.project),
                false,
                *old_id,
                *new_id,
            );
            hub.publish_marker(MarkerStreamEvent {
                project_guid: project_guid.clone(),
                event: MarkerEvent::Renumbered {
                    from: *old_id,
                    marker: marker.clone(),
                },
            });
        }
        for id in &gone {
            if renumbered.contains_key(id) {
                continue;
            }
            // Gone for good: drop its lane so the next thing to take
            // that number does not inherit it.
            crate::safe_wrappers::ruler_lanes::forget_assigned_lane(
                medium.low(),
                reaper_medium::ProjectContext::Proj(result.project),
                false,
                *id,
            );
            hub.publish_marker(MarkerStreamEvent {
                project_guid: project_guid.clone(),
                event: MarkerEvent::Removed(*id),
            });
        }

        *prev = fresh_by_id;
    }

    cache.retain(|guid, _| seen_projects.iter().any(|seen| seen == guid));
}
