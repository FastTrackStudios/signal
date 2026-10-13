//! `impl Regions for Standalone` — post-architect::rpc port.
//!
//! Backed by `ProjectState::regions: BTreeMap<u32, Region>` in the
//! existing in-memory state. The old async `StandaloneRegion` impl
//! (~500 LOC with parallel `RegionState` mock-data scaffolding +
//! event streams) retired in favor of operating directly on the
//! canonical state.

use daw_proto::Regions;
use daw_proto::region::{RegionEvent, RegionStreamEvent};
use daw_proto::{DawError, DawResult, ProjectContext, Region, TimeRange};

use crate::sync::Standalone;

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn publish_region_event(daw: &Standalone, project_guid: &str, event: RegionEvent) {
    let event = RegionStreamEvent {
        project_guid: project_guid.to_string(),
        event,
    };
    daw.bus_events
        .publish(daw_proto::event_bus::DawEvent::Region(event.clone()));
    daw.region_events.publish(event);
}

impl daw_proto::region::RegionsStreamSource for Standalone {
    fn events_hub(&self) -> &architect::PubSub<RegionStreamEvent> {
        &self.region_events
    }
}

/// Make a region carrying `guid` — the body of both `add` and
/// `add_with_guid`. A guid any marker or region of the project already
/// has is refused (they share one list in the project file).
fn insert_region(
    daw: &Standalone,
    project: &ProjectContext,
    guid: String,
    range: TimeRange,
    name: &str,
) -> DawResult<u32> {
    let project_guid =
        resolve_project(daw, project).ok_or_else(|| DawError::not_found("Project", "current"))?;
    let (id, region) = daw.with_project_mut(&project_guid, |p| {
        if crate::sync::project::marker_or_region_guid_taken(p, &guid) {
            return Err(DawError::already_exists("Region", &guid));
        }
        let id = p.next_region_id;
        p.next_region_id += 1;
        let mut region = Region::new(range, name.to_string());
        region.id = Some(id);
        region.guid = Some(guid);
        region.lane = crate::sync::project::default_lane(
            &p.ruler_lanes,
            crate::sync::RulerLane::DEFAULT_REGION,
        );
        p.regions.insert(id, region.clone());
        Ok((id, region))
    })??;
    publish_region_event(daw, &project_guid, RegionEvent::Added(region));
    Ok(id)
}

impl Regions for Standalone {
    fn all(&self, project: ProjectContext) -> Vec<Region> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| p.regions.values().cloned().collect())
            .unwrap_or_default()
    }

    fn get(&self, project: ProjectContext, id: u32) -> Option<Region> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| p.regions.get(&id).cloned())
            .ok()
            .flatten()
    }

    fn count(&self, project: ProjectContext) -> u32 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0;
        };
        self.with_project(&guid, |p| p.regions.len() as u32)
            .unwrap_or(0)
    }

    fn add(&self, project: ProjectContext, start: f64, end: f64, name: &str) -> DawResult<u32> {
        insert_region(
            self,
            &project,
            crate::new_braced_guid(),
            TimeRange::from_seconds(start, end),
            name,
        )
    }

    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        range: TimeRange,
        name: &str,
    ) -> DawResult<u32> {
        crate::check_new_guid("Region", guid)?;
        insert_region(self, &project, guid.to_string(), range, name)
    }

    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        self.with_project_mut(&guid, |p| {
            p.regions
                .remove(&id)
                .map(|_| ())
                .ok_or_else(|| DawError::not_found("Region", &id.to_string()))
        })??;
        publish_region_event(self, &guid, RegionEvent::Removed(id));
        Ok(())
    }

    fn set_bounds(&self, project: ProjectContext, id: u32, start: f64, end: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let region = self.with_project_mut(&guid, |p| {
            let r = p
                .regions
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Region", &id.to_string()))?;
            r.time_range = daw_proto::TimeRange::from_seconds(start, end);
            Ok::<_, DawError>(r.clone())
        })??;
        publish_region_event(self, &guid, RegionEvent::Changed(region));
        Ok(())
    }

    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let region = self.with_project_mut(&guid, |p| {
            let r = p
                .regions
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Region", &id.to_string()))?;
            r.name = name.to_string();
            Ok::<_, DawError>(r.clone())
        })??;
        publish_region_event(self, &guid, RegionEvent::Changed(region));
        Ok(())
    }

    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let region = self.with_project_mut(&guid, |p| {
            let r = p
                .regions
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Region", &id.to_string()))?;
            r.color = crate::color_from_service(color);
            Ok::<_, DawError>(r.clone())
        })??;
        publish_region_event(self, &guid, RegionEvent::Changed(region));
        Ok(())
    }

    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let region = self.with_project_mut(&guid, |p| {
            let r = p
                .regions
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Region", &id.to_string()))?;
            r.lane = lane;
            Ok::<_, DawError>(r.clone())
        })??;
        publish_region_event(self, &guid, RegionEvent::Changed(region));
        Ok(())
    }
}
