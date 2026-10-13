//! `impl Markers for Standalone` — post-architect::rpc port.
//!
//! The standalone backend is single-threaded and lives in-process, so
//! its dispatcher is the current thread. All state is held in the
//! existing per-project `ProjectState::markers` map keyed by
//! `next_marker_id`.
//!
//! The 500-line async `MarkerService` impl that used to live here
//! (event streaming, mock-data scaffolding, navigation goto, range
//! queries, lane methods) was retired alongside the sync trait
//! consolidation — see the architect refactor notes. Restoring any
//! of those surfaces happens on a sibling trait, not by re-bloating
//! the canonical `Markers`.

use architect::HasDispatcher;
use architect::dispatch::CurrentThreadDispatcher;
use daw_proto::Markers;
use daw_proto::marker::{MarkerEvent, MarkerStreamEvent};
use daw_proto::{DawError, DawResult, Marker, Position, PositionInSeconds, ProjectContext};

use crate::sync::Standalone;

impl HasDispatcher for Standalone {
    type Dispatcher = CurrentThreadDispatcher;

    fn dispatcher(&self) -> Self::Dispatcher {
        CurrentThreadDispatcher
    }
}

fn publish_marker_event(daw: &Standalone, project_guid: &str, event: MarkerEvent) {
    let event = MarkerStreamEvent {
        project_guid: project_guid.to_string(),
        event,
    };
    daw.bus_events
        .publish(daw_proto::event_bus::DawEvent::Marker(event.clone()));
    daw.marker_events.publish(event);
}

impl daw_proto::marker::MarkersStreamSource for Standalone {
    fn events_hub(&self) -> &architect::PubSub<MarkerStreamEvent> {
        &self.marker_events
    }
}

impl Markers for Standalone {
    fn all(&self, project: ProjectContext) -> Vec<Marker> {
        let guid = match resolve_project(self, &project) {
            Some(g) => g,
            None => return Vec::new(),
        };
        self.with_project(&guid, |p| p.markers.values().cloned().collect())
            .unwrap_or_default()
    }

    fn get(&self, project: ProjectContext, id: u32) -> Option<Marker> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| p.markers.get(&id).cloned())
            .ok()
            .flatten()
    }

    fn count(&self, project: ProjectContext) -> u32 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0;
        };
        self.with_project(&guid, |p| p.markers.len() as u32)
            .unwrap_or(0)
    }

    fn add(&self, project: ProjectContext, position: f64, name: &str) -> DawResult<u32> {
        insert_marker(self, &project, crate::new_braced_guid(), position, name)
    }

    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        position: f64,
        name: &str,
    ) -> DawResult<u32> {
        crate::check_new_guid("Marker", guid)?;
        insert_marker(self, &project, guid.to_string(), position, name)
    }

    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        self.with_project_mut(&guid, |p| {
            p.markers
                .remove(&id)
                .map(|_| ())
                .ok_or_else(|| DawError::not_found("Marker", &id.to_string()))
        })??;
        publish_marker_event(self, &guid, MarkerEvent::Removed(id));
        Ok(())
    }

    fn set_position(&self, project: ProjectContext, id: u32, position: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let marker = self.with_project_mut(&guid, |p| {
            let m = p
                .markers
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Marker", &id.to_string()))?;
            m.position = Position::from_time(PositionInSeconds::from_seconds(position));
            Ok::<_, DawError>(m.clone())
        })??;
        publish_marker_event(self, &guid, MarkerEvent::Changed(marker));
        Ok(())
    }

    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let marker = self.with_project_mut(&guid, |p| {
            let m = p
                .markers
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Marker", &id.to_string()))?;
            m.name = name.to_string();
            Ok::<_, DawError>(m.clone())
        })??;
        publish_marker_event(self, &guid, MarkerEvent::Changed(marker));
        Ok(())
    }

    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let marker = self.with_project_mut(&guid, |p| {
            let m = p
                .markers
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Marker", &id.to_string()))?;
            m.color = crate::color_from_service(color);
            Ok::<_, DawError>(m.clone())
        })??;
        publish_marker_event(self, &guid, MarkerEvent::Changed(marker));
        Ok(())
    }

    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()> {
        let guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "current"))?;
        let marker = self.with_project_mut(&guid, |p| {
            let m = p
                .markers
                .get_mut(&id)
                .ok_or_else(|| DawError::not_found("Marker", &id.to_string()))?;
            m.lane = lane;
            Ok::<_, DawError>(m.clone())
        })??;
        publish_marker_event(self, &guid, MarkerEvent::Changed(marker));
        Ok(())
    }
}

/// Make a marker carrying `guid` — the body of both `add` and
/// `add_with_guid`. A guid any marker or region of the project already
/// has is refused: the two share one list in the project file, where the
/// GUID is what a save matches them by.
fn insert_marker(
    daw: &Standalone,
    project: &ProjectContext,
    guid: String,
    position: f64,
    name: &str,
) -> DawResult<u32> {
    let project_guid =
        resolve_project(daw, project).ok_or_else(|| DawError::not_found("Project", "current"))?;
    let (id, marker) = daw.with_project_mut(&project_guid, |p| {
        if crate::sync::project::marker_or_region_guid_taken(p, &guid) {
            return Err(DawError::already_exists("Marker", &guid));
        }
        let id = p.next_marker_id;
        p.next_marker_id += 1;
        let marker = Marker {
            id: Some(id),
            guid: Some(guid),
            lane: crate::sync::project::default_lane(
                &p.ruler_lanes,
                crate::sync::RulerLane::DEFAULT_MARKER,
            ),
            ..Marker::new(
                Position::from_time(PositionInSeconds::from_seconds(position)),
                name.to_string(),
            )
        };
        p.markers.insert(id, marker.clone());
        Ok((id, marker))
    })??;
    publish_marker_event(daw, &project_guid, MarkerEvent::Added(marker));
    Ok(id)
}

/// Map a `ProjectContext` onto a concrete guid the standalone state
/// can index. `Current` resolves to the standalone's tracked current
/// project (or `None` if none was ever seeded).
fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}
