//! `impl Regions for Reaper` — sync trait + REAPER C API.
//!
//! Mounting goes through `daw_proto::region::serve(Reaper)`. The
//! dispatcher (REAPER's main thread queue) is pulled off the backend
//! via `HasDispatcher` on `Reaper`. Each method assumes it's running
//! on the main thread — the bridge enforces that contract.
//!
//! Helper `get_regions_on_main_thread` stays public for callers that
//! already hold a main-thread proof (no need to go through the
//! singleton trait).

use daw_control::lock::LockExt;
use std::ffi::CString;

use daw_proto::Regions;
use daw_proto::{DawError, DawResult, ProjectContext, Region, TimeRange};
use reaper_high::Reaper as ReaperHigh;
use reaper_medium::{
    MarkerOrRegionPosition, PositionInSeconds, ProjectContext as ReaperProjectContext,
};

use crate::project_context::resolve_project_context;
use crate::safe_wrappers::markers as sw;
use crate::safe_wrappers::ruler_lanes;

// ── Public sync helper ────────────────────────────────────────────────

/// Read all regions from the current project, sorted by start position.
/// Must be called from the main thread.
pub fn get_regions_on_main_thread() -> Vec<Region> {
    read_regions(ReaperProjectContext::CurrentProject)
}

fn read_regions(ctx: ReaperProjectContext) -> Vec<Region> {
    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    let low = medium.low();
    let mut regions = Vec::new();

    let total_count = medium.count_project_markers(ctx).total_count;
    for idx in 0..total_count {
        medium.enum_project_markers_3(ctx, idx, |result| {
            if let Some(info) = result
                && let Some(end_pos) = info.region_end_position
            {
                let id = info.id.get();
                let lane = ruler_lanes::assigned_lane(low, ctx, true, id)
                    .or_else(|| ruler_lanes::get_marker_lane(low, ctx, idx));
                regions.push(Region {
                    id: Some(id),
                    time_range: TimeRange::from_seconds(info.position.get(), end_pos.get()),
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

    regions.sort_by(|a, b| {
        a.start_seconds()
            .partial_cmp(&b.start_seconds())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    regions
}

// ── Tracks impl ───────────────────────────────────────────────────────

fn not_found_region() -> DawError {
    DawError::not_found("Region", "")
}

impl Regions for crate::Reaper {
    fn all(&self, project: ProjectContext) -> Vec<Region> {
        read_regions(resolve_project_context(&project))
    }

    fn get(&self, project: ProjectContext, id: u32) -> Option<Region> {
        read_regions(resolve_project_context(&project))
            .into_iter()
            .find(|r| r.id == Some(id))
    }

    fn count(&self, project: ProjectContext) -> u32 {
        let medium = ReaperHigh::get().medium_reaper();
        let ctx = resolve_project_context(&project);
        let total = medium.count_project_markers(ctx).total_count;
        let mut n = 0u32;
        for idx in 0..total {
            medium.enum_project_markers_3(ctx, idx, |result| {
                if let Some(info) = result
                    && info.region_end_position.is_some()
                {
                    n += 1;
                }
            });
        }
        n
    }

    fn add(&self, project: ProjectContext, start: f64, end: f64, name: &str) -> DawResult<u32> {
        let ctx = resolve_project_context(&project);
        let medium = ReaperHigh::get().medium_reaper();
        let start_pos = PositionInSeconds::new(start)
            .map_err(|e| DawError::operation_failed(format!("invalid start position: {e:?}")))?;
        let end_pos = PositionInSeconds::new(end)
            .map_err(|e| DawError::operation_failed(format!("invalid end position: {e:?}")))?;
        let id = medium
            .add_project_marker_2(
                ctx,
                MarkerOrRegionPosition::Region(start_pos, end_pos),
                name,
                None,
                None,
            )
            .map_err(|e| DawError::operation_failed(format!("add region failed: {e:?}")))?;
        Ok(id)
    }

    fn add_with_guid(
        &self,
        _project: ProjectContext,
        _guid: &str,
        _range: daw_proto::TimeRange,
        _name: &str,
    ) -> DawResult<u32> {
        // `MARKER_GUID:X` is read-only in `GetSetProjectInfo_String`, and
        // no other API call takes a region GUID: REAPER mints its own.
        Err(DawError::not_supported(
            "REAPER cannot set a region's GUID through its API",
        ))
    }

    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()> {
        let ctx = resolve_project_context(&project);
        let low = ReaperHigh::get().medium_reaper().low();
        sw::delete_project_marker(low, ctx, id as i32, true);
        Ok(())
    }

    fn set_bounds(&self, _project: ProjectContext, id: u32, start: f64, end: f64) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        sw::set_project_marker(low, id as i32, true, start, end, None);
        Ok(())
    }

    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        let ctx = resolve_project_context(&project);
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();
        let total_count = medium.count_project_markers(ctx).total_count;
        let cname = CString::new(name)
            .map_err(|e| DawError::operation_failed(format!("invalid name: {e}")))?;

        let mut found = false;
        for idx in 0..total_count {
            medium.enum_project_markers_3(ctx, idx, |result| {
                if let Some(info) = result
                    && let Some(end_pos) = info.region_end_position
                    && info.id.get() == id
                {
                    sw::set_project_marker(
                        low,
                        id as i32,
                        true,
                        info.position.get(),
                        end_pos.get(),
                        Some(&cname),
                    );
                    found = true;
                }
            });
            if found {
                break;
            }
        }
        if !found {
            return Err(not_found_region());
        }
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
                    && let Some(end_pos) = info.region_end_position
                    && info.id.get() == id
                    && let Ok(name) = CString::new(info.name.to_string())
                {
                    sw::set_project_marker_by_index2(
                        low,
                        ctx,
                        idx as i32,
                        true,
                        info.position.get(),
                        end_pos.get(),
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
            return Err(not_found_region());
        }
        Ok(())
    }

    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        // Mirror of Markers::set_lane: enum-index lookup filtered to
        // region entries (`region_end_position.is_some()`), then the
        // shared `ruler_lanes::set_marker_lane` wrapper (which takes
        // either markers or regions — REAPER's API treats them as one
        // indexed list).
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
                    .is_some_and(|info| info.region_end_position.is_some() && info.id.get() == id);
            });
            if found {
                if !ruler_lanes::set_marker_lane(low, ctx, idx, lane) {
                    return Err(DawError::operation_failed(
                        "REAPER ruler lane API is unavailable",
                    ));
                }
                ruler_lanes::remember_assigned_lane(low, ctx, true, id, lane);
                return Ok(());
            }
        }

        Err(not_found_region())
    }
}

impl daw_proto::region::RegionsStreamSource for crate::Reaper {
    fn events_hub(&self) -> &architect::PubSub<RegionStreamEvent> {
        crate::event_hub::hub().regions_hub()
    }
}

// ── Streaming: poll + broadcast regions ────────────────────────────────
//
// Same pattern as markers — per-project HashMap<u32, Region> cache,
// diff per tick, emit Added/Changed/Removed events through the hub's
// regions channel. Driven from the bridge's 30Hz timer.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use daw_proto::RegionEvent;
use daw_proto::region::RegionStreamEvent;
use reaper_medium::ProjectRef;

use crate::project_context::{MAX_PROJECT_TABS, project_guid as project_guid_from};

static REGION_CACHE: OnceLock<Mutex<HashMap<String, HashMap<u32, Region>>>> = OnceLock::new();

fn region_cache() -> &'static Mutex<HashMap<String, HashMap<u32, Region>>> {
    REGION_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Poll REAPER region state for every open project. **Main thread only.**
/// Is this the same region, wearing a different number?
///
/// Everything a user can see, compared — the id deliberately not.
fn same_but_for_the_number(old: &Region, new: &Region) -> bool {
    old.name == new.name
        && old.time_range == new.time_range
        && old.color == new.color
        && old.lane == new.lane
}

pub fn poll_and_broadcast_regions() {
    let hub = crate::event_hub::hub();
    if hub.regions_subscriber_count() == 0 {
        return;
    }

    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    let mut cache = region_cache().lock_recoverable("region::cache");

    let mut seen_projects: Vec<String> = Vec::new();

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = reaper_high::Project::new(result.project);
        let project_guid = project_guid_from(&project);
        seen_projects.push(project_guid.clone());

        let project_ctx = ProjectContext::Project(project_guid.clone());
        let fresh: Vec<Region> = daw_proto::Regions::all(&crate::Reaper, project_ctx);
        let fresh_by_id: HashMap<u32, Region> = fresh
            .into_iter()
            .filter_map(|r| r.id.map(|id| (id, r)))
            .collect();

        let prev = cache.entry(project_guid.clone()).or_default();

        // See the matching note in `marker.rs`: markers and regions are
        // one REAPER list wearing one set of numbers, and the renumber
        // action reassigns them together.
        let gone: Vec<u32> = prev
            .keys()
            .copied()
            .filter(|id| !fresh_by_id.contains_key(id))
            .collect();
        let renumbered = crate::renumber::pair(prev, &fresh_by_id, same_but_for_the_number);

        for (id, region) in &fresh_by_id {
            if renumbered.values().any(|new_id| new_id == id) {
                continue;
            }
            match prev.get(id) {
                None => hub.publish_region(RegionStreamEvent {
                    project_guid: project_guid.clone(),
                    event: RegionEvent::Added(region.clone()),
                }),
                Some(old) if old != region => {
                    hub.publish_region(RegionStreamEvent {
                        project_guid: project_guid.clone(),
                        event: RegionEvent::Changed(region.clone()),
                    });
                }
                Some(_) => {}
            }
        }
        for (old_id, new_id) in &renumbered {
            let Some(region) = fresh_by_id.get(new_id) else {
                continue;
            };
            // The lane shadow is filed under the number, so it has to
            // move with it. The SECTIONS lane is what makes a region a
            // song section — a lane lost in a renumber is a verse that
            // stops being a verse.
            crate::safe_wrappers::ruler_lanes::carry_assigned_lane(
                medium.low(),
                reaper_medium::ProjectContext::Proj(result.project),
                true,
                *old_id,
                *new_id,
            );
            hub.publish_region(RegionStreamEvent {
                project_guid: project_guid.clone(),
                event: RegionEvent::Renumbered {
                    from: *old_id,
                    region: region.clone(),
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
                true,
                *id,
            );
            hub.publish_region(RegionStreamEvent {
                project_guid: project_guid.clone(),
                event: RegionEvent::Removed(*id),
            });
        }

        *prev = fresh_by_id;
    }

    cache.retain(|guid, _| seen_projects.iter().any(|seen| seen == guid));
}
