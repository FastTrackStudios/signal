//! `impl StretchMarkers for Standalone`.
//!
//! Markers are stored per take guid and kept sorted by position, which
//! is the invariant the host maintains and every reader assumes: a
//! piecewise map is only a map if its knots are in order.

use daw_proto::{
    ItemRef, ProjectContext, StretchMarker, StretchMarkers, StretchMode, StretchTakeRef, TakeRef,
};

use crate::sync::Standalone;
use daw_proto::{DawError, DawResult};

/// The take guid a location names, resolved through the active-take
/// rules.
fn take_guid(
    daw: &Standalone,
    project: &ProjectContext,
    item: &ItemRef,
    take: &TakeRef,
) -> Option<String> {
    use daw_proto::Takes;
    let takes = daw.get_takes(project.clone(), item.clone());
    let index = match take {
        TakeRef::Active => takes.iter().position(|t| t.is_active).unwrap_or(0),
        TakeRef::Index(i) => *i as usize,
        TakeRef::Guid(g) => takes.iter().position(|t| t.guid == *g)?,
    };
    takes.get(index).map(|t| t.guid.clone())
}

fn resolve(daw: &Standalone, loc: &StretchTakeRef) -> DawResult<(String, String)> {
    let project = match &loc.project {
        ProjectContext::Project(g) => Some(g.clone()),
        ProjectContext::Current => daw
            .state
            .lock()
            .ok()
            .and_then(|s| s.current_project_guid.clone()),
    }
    .ok_or_else(|| DawError::not_found("Project", "context"))?;
    let take = take_guid(daw, &loc.project, &loc.item, &loc.take)
        .ok_or_else(|| DawError::not_found("Take", "location"))?;
    Ok((project, take))
}

fn sorted(mut markers: Vec<StretchMarker>) -> Vec<StretchMarker> {
    markers.sort_by(|a, b| a.position.total_cmp(&b.position));
    markers
}

impl StretchMarkers for Standalone {
    fn get_stretch_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<StretchMarker> {
        let loc = StretchTakeRef::new(project, item, take);
        let Ok((project, take)) = resolve(self, &loc) else {
            return Vec::new();
        };
        self.with_project(&project, |p| {
            p.stretch_markers.get(&take).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_stretch_marker(
        &self,
        location: StretchTakeRef,
        marker: StretchMarker,
    ) -> DawResult<u32> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            let list = p.stretch_markers.entry(take).or_default();
            list.push(marker);
            *list = sorted(core::mem::take(list));
            // Where it landed, not where it was pushed: the host keeps
            // markers ordered, so a caller cannot assume the last index.
            list.iter()
                .position(|m| m.position == marker.position)
                .unwrap_or(list.len() - 1) as u32
        })
    }

    fn set_stretch_marker(
        &self,
        location: StretchTakeRef,
        index: u32,
        marker: StretchMarker,
    ) -> DawResult<()> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            let list = p.stretch_markers.entry(take).or_default();
            match list.get_mut(index as usize) {
                Some(slot) => {
                    *slot = marker;
                    *list = sorted(core::mem::take(list));
                    Ok(())
                }
                None => Err(DawError::not_found("StretchMarker", &index.to_string())),
            }
        })?
    }

    fn delete_stretch_marker(&self, location: StretchTakeRef, index: u32) -> DawResult<()> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            let list = p.stretch_markers.entry(take).or_default();
            if (index as usize) < list.len() {
                list.remove(index as usize);
                Ok(())
            } else {
                Err(DawError::not_found("StretchMarker", &index.to_string()))
            }
        })?
    }

    fn clear_stretch_markers(&self, location: StretchTakeRef) -> DawResult<()> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            p.stretch_markers.remove(&take);
        })
    }

    fn set_stretch_markers(
        &self,
        location: StretchTakeRef,
        markers: Vec<StretchMarker>,
    ) -> DawResult<()> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            if markers.is_empty() {
                p.stretch_markers.remove(&take);
            } else {
                p.stretch_markers.insert(take, sorted(markers));
            }
        })
    }

    fn set_stretch_mode(&self, location: StretchTakeRef, mode: StretchMode) -> DawResult<()> {
        let (project, take) = resolve(self, &location)?;
        self.with_project_mut(&project, |p| {
            p.stretch_modes.insert(take, mode);
        })
    }
}
