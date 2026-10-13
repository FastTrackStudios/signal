//! `impl Automation for Standalone` — envelope storage in
//! `ProjectState.envelopes`.
//!
//! Each envelope is keyed by `(track_guid, EnvelopeKey)` and stores
//! visibility, arm, automation mode, plus a vec of `EnvelopePoint`s
//! kept sorted by time. `value_at` reads the curve the engine plays:
//! each segment bent by its point's shape, automation items laid over
//! their windows ([`daw_proto::automation::played`]).

use daw_proto::TrackRef;
use daw_proto::automation::{
    AddAutomationItemParams, AddPointParams, Automation, AutomationItem, Envelope,
    EnvelopeLocation, EnvelopePoint, EnvelopeShape, LaneParams, SetAutomationItemParams,
    SetAutomationItemPointsParams, SetPointParams, TimeRangeParams,
};
use daw_proto::primitives::{AutomationMode, PositionInSeconds};
use daw_proto::project::ProjectContext;
use daw_proto::{DawError, DawResult};

use crate::sync::{EnvelopeData, EnvelopeKey, Standalone};

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn resolve_track_guid(daw: &Standalone, project_guid: &str, track: &TrackRef) -> Option<String> {
    daw.with_project(project_guid, |p| match track {
        TrackRef::Guid(g) => p
            .tracks
            .iter()
            .find(|t| t.guid == *g)
            .map(|t| t.guid.clone()),
        TrackRef::Index(i) => p.tracks.get(*i as usize).map(|t| t.guid.clone()),
        TrackRef::Master => Some("master".to_string()),
    })
    .ok()
    .flatten()
}

fn renumber(points: &mut [EnvelopePoint]) {
    for (i, p) in points.iter_mut().enumerate() {
        p.index = i as u32;
    }
}

fn sort_points(points: &mut [EnvelopePoint]) {
    points.sort_by(|a, b| {
        a.time
            .as_seconds()
            .partial_cmp(&b.time.as_seconds())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Resolve the envelope at `loc` to its `(owner_id, key)` identity.
/// `owner_id` is the track GUID for track / FxParam / Send / Named
/// envelopes, or an empty string for `Take` envelopes (the item +
/// take GUIDs are carried inside the `EnvelopeKey::Take` variant so
/// the storage key is fully self-identifying).
fn resolve_envelope_id(
    daw: &Standalone,
    project_guid: &str,
    loc: &EnvelopeLocation,
) -> Option<(String, EnvelopeKey)> {
    let key = EnvelopeKey::from_ref(&loc.envelope);
    if matches!(key, EnvelopeKey::Take { .. }) {
        // Take envelopes don't have a track owner — the (item, take)
        // identity carries everything needed.
        return Some((String::new(), key));
    }
    let track_guid = resolve_track_guid(daw, project_guid, &loc.track)?;
    Some((track_guid, key))
}

/// Resolve a service-side envelope location to the touchable param
/// it addresses (track refs become guids via the project state).
fn touchable(
    daw: &Standalone,
    project: &ProjectContext,
    location: &EnvelopeLocation,
) -> DawResult<crate::automation_touch::TouchableParam> {
    let guid =
        resolve_project(daw, project).ok_or_else(|| DawError::not_found("Project", "context"))?;
    let resolve = |track: &daw_proto::TrackRef| -> Option<String> {
        daw.with_project(&guid, |p| {
            let idx = match track {
                daw_proto::TrackRef::Guid(g) => p.tracks.iter().position(|t| &t.guid == g),
                daw_proto::TrackRef::Index(i) => {
                    let i = *i as usize;
                    (i < p.tracks.len()).then_some(i)
                }
                daw_proto::TrackRef::Master => None,
            };
            idx.map(|i| p.tracks[i].guid.clone())
        })
        .ok()
        .flatten()
    };
    crate::automation_touch::TouchableParam::from_location(location, resolve)
        .ok_or_else(|| DawError::operation_failed("envelope location is not touchable"))
}

impl Automation for Standalone {
    fn envelopes(&self, project: ProjectContext, track: TrackRef) -> Vec<Envelope> {
        let Some(project_guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        let Some(track_guid) = resolve_track_guid(self, &project_guid, &track) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.envelopes
                .iter()
                .filter_map(|((tg, key), data)| {
                    if tg == &track_guid {
                        Some(data.to_proto(tg, key))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn envelope(&self, project: ProjectContext, location: EnvelopeLocation) -> Option<Envelope> {
        let project_guid = resolve_project(self, &project)?;
        let (track_guid, key) = resolve_envelope_id(self, &project_guid, &location)?;
        self.with_project(&project_guid, |p| {
            p.envelopes
                .get(&(track_guid.clone(), key.clone()))
                .map(|d| d.to_proto(&track_guid, &key))
        })
        .ok()
        .flatten()
    }

    fn set_visible(&self, project: ProjectContext, location: EnvelopeLocation, visible: bool) {
        mutate_envelope(self, project, location, |e| e.visible = visible);
    }

    fn set_lane(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        lane: LaneParams,
    ) -> DawResult<()> {
        // The floor is the theme's `envcp_min_height`; a caller that
        // asks for less gets the minimum rather than an error, which is
        // what a drag past the stop should do.
        let height = if lane.in_own_lane {
            lane.height.max(ENVCP_MIN_HEIGHT)
        } else {
            0
        };
        mutate_envelope(self, project, location, |e| {
            e.in_own_lane = lane.in_own_lane;
            e.lane_height = height;
        });
        Ok(())
    }

    fn automation_items(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
    ) -> Vec<AutomationItem> {
        read_envelope(self, &project, &location)
            .map(|e| {
                e.automation_items
                    .iter()
                    .map(|(ai, _)| ai.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn automation_item_points(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        index: u32,
    ) -> Vec<EnvelopePoint> {
        read_envelope(self, &project, &location)
            .and_then(|e| {
                e.automation_items
                    .get(index as usize)
                    .map(|(_, p)| p.clone())
            })
            .unwrap_or_default()
    }

    fn add_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: AddAutomationItemParams,
    ) -> DawResult<u32> {
        let project_guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "context"))?;
        let id = resolve_envelope_id(self, &project_guid, &location)
            .ok_or_else(|| DawError::not_found("envelope", "the location does not resolve"))?;
        let mut index = 0u32;
        self.with_project_mut(&project_guid, |p| {
            // Pools are the project's, as REAPER's are: an instance of a
            // pool on another track's envelope is the same curve.
            let all = || p.envelopes.values().flat_map(|e| e.automation_items.iter());
            // A pooled instance copies its pool-mate's curve, so the two
            // start identical — which is what pooling means before an edit.
            let seeded = (params.pool_id >= 0)
                .then(|| {
                    all()
                        .find(|(ai, _)| ai.pool_id == params.pool_id)
                        .map(|(_, points)| points.clone())
                })
                .flatten()
                .unwrap_or_default();
            let pool_id = if params.pool_id >= 0 {
                params.pool_id
            } else {
                // A fresh pool: one past the highest in the project.
                all().map(|(ai, _)| ai.pool_id).max().unwrap_or(-1) + 1
            };
            let e = p.envelopes.entry(id).or_insert_with(EnvelopeData::new);
            index = e.automation_items.len() as u32;
            e.automation_items.push((
                AutomationItem {
                    index,
                    pool_id,
                    position: params.position,
                    length: params.length,
                    ..Default::default()
                },
                seeded,
            ));
        })?;
        Ok(index)
    }

    fn set_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetAutomationItemParams,
    ) -> DawResult<()> {
        mutate_envelope(self, project, location, |e| {
            let Some((item, _)) = e.automation_items.get_mut(params.index as usize) else {
                return;
            };
            if let Some(v) = params.position {
                item.position = v;
            }
            if let Some(v) = params.length {
                item.length = v;
            }
            if let Some(v) = params.start_offset {
                item.start_offset = v;
            }
            if let Some(v) = params.play_rate {
                item.play_rate = v;
            }
            if let Some(v) = params.baseline {
                item.baseline = v;
            }
            if let Some(v) = params.amplitude {
                item.amplitude = v;
            }
            if let Some(v) = params.loop_source {
                item.loop_source = v;
            }
            if let Some(v) = params.selected {
                item.selected = v;
            }
        });
        Ok(())
    }

    fn set_automation_item_points(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetAutomationItemPointsParams,
    ) -> DawResult<()> {
        let mut points = params.points;
        points.sort_by(|a, b| a.time.as_seconds().total_cmp(&b.time.as_seconds()));
        for (i, p) in points.iter_mut().enumerate() {
            p.index = i as u32;
        }
        let project_guid = resolve_project(self, &project)
            .ok_or_else(|| DawError::not_found("Project", "context"))?;
        let id = resolve_envelope_id(self, &project_guid, &location)
            .ok_or_else(|| DawError::not_found("envelope", "the location does not resolve"))?;
        let mut found = false;
        let _ = self.with_project_mut(&project_guid, |p| {
            let Some(pool) = p
                .envelopes
                .get(&id)
                .and_then(|e| e.automation_items.get(params.index as usize))
                .map(|(item, _)| item.pool_id)
            else {
                return;
            };
            found = true;
            // Every instance of the pool carries the curve — on any
            // envelope of the project.
            for e in p.envelopes.values_mut() {
                for (item, curve) in &mut e.automation_items {
                    if item.pool_id == pool {
                        curve.clone_from(&points);
                    }
                }
            }
        });
        if found {
            Ok(())
        } else {
            Err(DawError::not_found(
                "automation item",
                &params.index.to_string(),
            ))
        }
    }

    fn delete_automation_item(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        index: u32,
    ) -> DawResult<()> {
        mutate_envelope(self, project, location, |e| {
            if (index as usize) < e.automation_items.len() {
                e.automation_items.remove(index as usize);
                // Indices are positions, so everything after shifts.
                for (i, (item, _)) in e.automation_items.iter_mut().enumerate() {
                    item.index = i as u32;
                }
            }
        });
        Ok(())
    }

    fn touch_param(&self, project: ProjectContext, location: EnvelopeLocation) -> DawResult<()> {
        let param = touchable(self, &project, &location)?;
        Standalone::touch_param(self, param);
        Ok(())
    }

    fn release_param(&self, project: ProjectContext, location: EnvelopeLocation) -> DawResult<()> {
        let param = touchable(self, &project, &location)?;
        Standalone::release_param(self, param);
        Ok(())
    }

    fn write_param(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        value: f64,
    ) -> DawResult<()> {
        let param = touchable(self, &project, &location)?;
        Standalone::write_param(self, project, param, value)
    }

    fn set_armed(&self, project: ProjectContext, location: EnvelopeLocation, armed: bool) {
        mutate_envelope(self, project, location, |e| e.armed = armed);
    }

    fn set_automation_mode(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        mode: AutomationMode,
    ) {
        mutate_envelope(self, project, location, |e| e.automation_mode = mode);
    }

    fn points(&self, project: ProjectContext, location: EnvelopeLocation) -> Vec<EnvelopePoint> {
        let Some(project_guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        let Some((tg, key)) = resolve_envelope_id(self, &project_guid, &location) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.envelopes
                .get(&(tg, key))
                .map(|d| d.points.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn points_in_range(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        range: TimeRangeParams,
    ) -> Vec<EnvelopePoint> {
        let (start, end) = (range.start.as_seconds(), range.end.as_seconds());
        Automation::points(self, project, location)
            .into_iter()
            .filter(|p| {
                let t = p.time.as_seconds();
                t >= start && t <= end
            })
            .collect()
    }

    fn value_at(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        time: PositionInSeconds,
    ) -> f64 {
        // What plays there: the points, bent by their shapes, with the
        // automation items over them — the engine's own curve.
        read_envelope(self, &project, &location)
            .and_then(|e| {
                daw_proto::automation::value_at(
                    &daw_proto::automation::played(&e.points, &e.automation_items),
                    time.as_seconds(),
                )
            })
            .unwrap_or(0.0)
    }

    fn add_point(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: AddPointParams,
    ) -> u32 {
        let Some(project_guid) = resolve_project(self, &project) else {
            return u32::MAX;
        };
        let Some((tg, key)) = resolve_envelope_id(self, &project_guid, &location) else {
            return u32::MAX;
        };
        self.with_project_mut(&project_guid, |p| {
            let data = p
                .envelopes
                .entry((tg, key))
                .or_insert_with(EnvelopeData::new);
            // Note: value is stored unclamped. Different envelope
            // kinds use different ranges (Volume/Pan/Mute = 0..=1,
            // Pitch = semitones, FxParam = whatever the plugin
            // exposes). The renderer / consumer clamps when it
            // applies the value.
            data.points.push(EnvelopePoint {
                index: 0, // rewritten by renumber()
                time: params.time,
                value: params.value,
                shape: params.shape,
                tension: 0.0,
                selected: false,
            });
            sort_points(&mut data.points);
            renumber(&mut data.points);
            // Return the new point's index (post-sort).
            data.points
                .iter()
                .position(|pt| {
                    (pt.time.as_seconds() - params.time.as_seconds()).abs() < 1e-12
                        && (pt.value - params.value).abs() < 1e-12
                })
                .map(|i| i as u32)
                .unwrap_or(0)
        })
        .unwrap_or(u32::MAX)
    }

    fn delete_point(&self, project: ProjectContext, location: EnvelopeLocation, index: u32) {
        let Some(project_guid) = resolve_project(self, &project) else {
            return;
        };
        let Some((tg, key)) = resolve_envelope_id(self, &project_guid, &location) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(data) = p.envelopes.get_mut(&(tg, key)) {
                let i = index as usize;
                if i < data.points.len() {
                    data.points.remove(i);
                    renumber(&mut data.points);
                }
            }
        });
    }

    fn set_point(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        params: SetPointParams,
    ) {
        let Some(project_guid) = resolve_project(self, &project) else {
            return;
        };
        let Some((tg, key)) = resolve_envelope_id(self, &project_guid, &location) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(data) = p.envelopes.get_mut(&(tg, key)) {
                let i = params.index as usize;
                if let Some(pt) = data.points.get_mut(i) {
                    pt.time = params.time;
                    pt.value = params.value;
                    pt.shape = params.shape;
                    sort_points(&mut data.points);
                    renumber(&mut data.points);
                }
            }
        });
    }

    fn delete_points_in_range(
        &self,
        project: ProjectContext,
        location: EnvelopeLocation,
        range: TimeRangeParams,
    ) {
        let Some(project_guid) = resolve_project(self, &project) else {
            return;
        };
        let Some((tg, key)) = resolve_envelope_id(self, &project_guid, &location) else {
            return;
        };
        let (start, end) = (range.start.as_seconds(), range.end.as_seconds());
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(data) = p.envelopes.get_mut(&(tg, key)) {
                data.points.retain(|pt| {
                    let t = pt.time.as_seconds();
                    !(t >= start && t <= end)
                });
                renumber(&mut data.points);
            }
        });
    }

    fn global_automation_override(&self, project: ProjectContext) -> Option<AutomationMode> {
        let project_guid = resolve_project(self, &project)?;
        self.with_project(&project_guid, |p| p.global_automation_override)
            .ok()
            .flatten()
    }

    fn set_global_automation_override(
        &self,
        project: ProjectContext,
        mode: Option<AutomationMode>,
    ) {
        let Some(project_guid) = resolve_project(self, &project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            p.global_automation_override = mode;
        });
    }
}

/// The theme's `envcp_min_height` — a lane cannot be shorter.
const ENVCP_MIN_HEIGHT: u32 = 27;

/// Read one envelope's state, if the project and location resolve.
fn read_envelope(
    daw: &Standalone,
    project: &ProjectContext,
    location: &EnvelopeLocation,
) -> Option<EnvelopeData> {
    let project_guid = resolve_project(daw, project)?;
    let (tg, key) = resolve_envelope_id(daw, &project_guid, location)?;
    daw.with_project(&project_guid, |p| p.envelopes.get(&(tg, key)).cloned())
        .ok()
        .flatten()
}

fn mutate_envelope(
    daw: &Standalone,
    project: ProjectContext,
    location: EnvelopeLocation,
    f: impl FnOnce(&mut EnvelopeData),
) {
    let Some(project_guid) = resolve_project(daw, &project) else {
        return;
    };
    let Some((tg, key)) = resolve_envelope_id(daw, &project_guid, &location) else {
        return;
    };
    let _ = daw.with_project_mut(&project_guid, |p| {
        let data = p
            .envelopes
            .entry((tg, key))
            .or_insert_with(EnvelopeData::new);
        f(data);
    });
}
