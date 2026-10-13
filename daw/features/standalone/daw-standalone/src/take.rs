//! `impl Takes for Standalone` — post-architect::rpc port.
//!
//! Backed by `ProjectState::takes: HashMap<item_guid, TakeList>`.
//! The old async `StandaloneTake` service + per-service locks retire
//! with the port; everything now goes through canonical project state.

use crate::sync::EnvelopeKey;
use daw_proto::Takes;
use daw_proto::event_bus::DawEvent;
use daw_proto::item::{ItemEvent, TakeEvent};
use daw_proto::{
    AddTakeMarkerAtPositionRequest, DawError, DawResult, Duration, ItemRef, ProjectContext,
    SourceType, Take, TakeMarker, TakeMarkerCreate, TakeMarkerUpdate, TakeRef,
};

use crate::sync::{Standalone, TakeList};

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn not_found_proj() -> DawError {
    DawError::not_found("Project", "context")
}

fn resolve_item_guid(item: &ItemRef) -> Option<String> {
    match item {
        ItemRef::Guid(g) => Some(g.clone()),
        _ => None,
    }
}

fn resolve_take_index(list: &TakeList, take: &TakeRef) -> Option<usize> {
    match take {
        TakeRef::Active => Some(list.active_idx as usize),
        TakeRef::Index(i) => Some(*i as usize),
        TakeRef::Guid(g) => list.takes.iter().position(|t| t.guid == *g),
    }
}

/// Where a marker call lands: the project, the key the take's markers
/// are kept under, and the take's guid (for the change event).
struct MarkerTarget {
    project: String,
    /// The take's source file — markers belong to the recording, so
    /// every item cut from it shares them — or, for a take with no file,
    /// the take's own guid.
    key: String,
    take: String,
}

/// The project and take a marker call addresses.
fn marker_target(
    daw: &Standalone,
    project: &ProjectContext,
    item: &ItemRef,
    take: &TakeRef,
) -> Option<MarkerTarget> {
    let proj = resolve_project(daw, project)?;
    let item_guid = resolve_item_guid(item)?;
    let list = daw
        .with_project(&proj, |p| p.takes.get(&item_guid).cloned())
        .ok()
        .flatten()?;
    let idx = resolve_take_index(&list, take)?;
    let t = list.takes.get(idx)?;
    Some(MarkerTarget {
        project: proj,
        key: t.source_file_path.clone().unwrap_or_else(|| t.guid.clone()),
        take: t.guid.clone(),
    })
}

/// A new marker id.
fn new_marker_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Keep markers in source order and renumber them.
///
/// The index is an enumeration position, not an identity — the proto
/// says so, and REAPER renumbers on every insert too. Sorting here is
/// what makes "the marker at index 3" mean the fourth one in time
/// rather than the fourth one added.
fn renumber(list: &mut [TakeMarker]) {
    list.sort_by(|a, b| {
        a.source_position_seconds
            .total_cmp(&b.source_position_seconds)
    });
    for (i, m) in list.iter_mut().enumerate() {
        m.index = i as u32;
    }
}

impl Takes for Standalone {
    fn get_takes(&self, project: ProjectContext, item: ItemRef) -> Vec<Take> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        let Some(item_guid) = resolve_item_guid(&item) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            p.takes
                .get(&item_guid)
                .map(|tl| tl.takes.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn get_take(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> Option<Take> {
        let guid = resolve_project(self, &project)?;
        let item_guid = resolve_item_guid(&item)?;
        self.with_project(&guid, |p| {
            let tl = p.takes.get(&item_guid)?;
            let idx = resolve_take_index(tl, &take)?;
            tl.takes.get(idx).cloned()
        })
        .ok()
        .flatten()
    }

    fn get_active_take(&self, project: ProjectContext, item: ItemRef) -> Option<Take> {
        let guid = resolve_project(self, &project)?;
        let item_guid = resolve_item_guid(&item)?;
        self.with_project(&guid, |p| {
            let tl = p.takes.get(&item_guid)?;
            tl.takes.get(tl.active_idx as usize).cloned()
        })
        .ok()
        .flatten()
    }

    fn take_count(&self, project: ProjectContext, item: ItemRef) -> u32 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0;
        };
        let Some(item_guid) = resolve_item_guid(&item) else {
            return 0;
        };
        self.with_project(&guid, |p| {
            p.takes
                .get(&item_guid)
                .map(|tl| tl.takes.len() as u32)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn add_take(&self, project: ProjectContext, item: ItemRef) -> Option<String> {
        let guid = resolve_project(self, &project)?;
        let item_guid = resolve_item_guid(&item)?;
        let created = self
            .with_project_mut(&guid, |p| {
                let take_guid = p.mint_take_guid();
                let tl = p.takes.entry(item_guid.clone()).or_default();
                let mut take = Take::default();
                take.guid = take_guid.clone();
                take.item_guid = item_guid.clone();
                take.index = tl.takes.len() as u32;
                tl.takes.push(take.clone());
                (take_guid, take)
            })
            .ok()?;
        let (take_guid, take) = created;
        self.bus_events.publish(DawEvent::Take(TakeEvent::Created {
            project_guid: guid,
            item_guid,
            take,
        }));
        Some(take_guid)
    }

    fn delete_take(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let item_guid = resolve_item_guid(&item)
            .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
        let take_guid = self.with_project_mut(&guid, |p| {
            let tl = p
                .takes
                .get_mut(&item_guid)
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let idx = resolve_take_index(tl, &take)
                .ok_or_else(|| DawError::not_found("Take", &format!("{take:?}")))?;
            if idx >= tl.takes.len() {
                return Err(DawError::not_found("Take", &format!("{take:?}")));
            }
            let removed = tl.takes.remove(idx);
            if tl.active_idx as usize >= tl.takes.len() && !tl.takes.is_empty() {
                tl.active_idx = tl.takes.len() as u32 - 1;
            }
            Ok::<String, DawError>(removed.guid)
        })??;
        self.bus_events.publish(DawEvent::Take(TakeEvent::Deleted {
            project_guid: guid,
            item_guid,
            take_guid,
        }));
        Ok(())
    }

    fn set_active_take(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let item_guid = resolve_item_guid(&item)
            .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
        let (old_idx, new_idx) = self.with_project_mut(&guid, |p| {
            let tl = p
                .takes
                .get_mut(&item_guid)
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let idx = resolve_take_index(tl, &take)
                .ok_or_else(|| DawError::not_found("Take", &format!("{take:?}")))?;
            if idx >= tl.takes.len() {
                return Err(DawError::not_found("Take", &format!("{take:?}")));
            }
            let old = tl.active_idx;
            tl.active_idx = idx as u32;
            Ok::<(u32, u32), DawError>((old, idx as u32))
        })??;
        // The active-take cursor is a property of the item.
        self.bus_events
            .publish(DawEvent::Item(ItemEvent::ActiveTakeChanged {
                project_guid: guid,
                item_guid,
                old_take_index: old_idx,
                new_take_index: new_idx,
            }));
        Ok(())
    }

    fn set_name(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        name: String,
    ) -> DawResult<()> {
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            t.name = name.clone();
            TakeEvent::NameChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
                name,
            }
        })
    }

    fn set_color(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        color: Option<u32>,
    ) -> DawResult<()> {
        self.mutate_take_changed(&project, &item, &take, |t| t.color = color)
    }

    fn set_volume(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        volume: f64,
    ) -> DawResult<()> {
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            t.volume = volume;
            TakeEvent::VolumeChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
                volume,
            }
        })
    }

    fn set_play_rate(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        rate: f64,
    ) -> DawResult<()> {
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            t.play_rate = rate;
            TakeEvent::PlayRateChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
                play_rate: rate,
            }
        })
    }

    fn set_pitch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        semitones: f64,
    ) -> DawResult<()> {
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            t.pitch = semitones;
            TakeEvent::PitchChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
                pitch: semitones,
            }
        })
    }

    fn set_preserve_pitch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        preserve: bool,
    ) -> DawResult<()> {
        self.mutate_take_changed(&project, &item, &take, |t| t.preserve_pitch = preserve)
    }

    fn set_start_offset(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        offset: Duration,
    ) -> DawResult<()> {
        let mut slipped: Option<(String, String, String, f64)> = None;
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            let by = offset.as_seconds() - t.start_offset.as_seconds();
            t.start_offset = offset;
            slipped = Some((pg.to_string(), ig.to_string(), t.guid.clone(), by));
            TakeEvent::Changed {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
            }
        })?;
        // The take's envelopes stay on its audio: slipped `by` further
        // into the source, a point that was at a sound is at it still,
        // `by` earlier in the item.
        if let Some((project_guid, item_guid, take_guid, by)) = slipped
            && by != 0.0
        {
            self.with_project_mut(&project_guid, |p| {
                let length = p.items.get(&item_guid).map(|e| e.item.length.as_seconds());
                for ((_, key), data) in &mut p.envelopes {
                    let EnvelopeKey::Take {
                        item_guid: of_item,
                        take_guid: of_take,
                        ..
                    } = key
                    else {
                        continue;
                    };
                    if *of_item == item_guid && *of_take == take_guid {
                        for point in &mut data.points {
                            point.time = daw_proto::PositionInSeconds::from_seconds(
                                point.time.as_seconds() - by,
                            );
                        }
                        // A slice keeps the curve over itself: a take cut
                        // into a thousand slices is not a thousand copies
                        // of the whole song's curve.
                        if let Some(length) = length {
                            keep_over(&mut data.points, length);
                        }
                    }
                }
            })?;
        }
        Ok(())
    }

    fn set_source_file(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        path: String,
    ) -> DawResult<()> {
        let mut changed_take: Option<(String, String)> = None;
        self.mutate_take_evt(&project, &item, &take, |pg, ig, t| {
            t.source_file_path = Some(path.clone());
            t.source_type = SourceType::Audio;
            changed_take = Some((pg.to_string(), t.guid.clone()));
            TakeEvent::SourceChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
                source_path: Some(path),
            }
        })?;
        // The materialized audio belongs to the *old* source; keeping it
        // would make every placement-aware reader (accessor, peaks, the
        // renderer) play the file the take no longer points at.
        #[cfg(any(feature = "audio", feature = "decode"))]
        if let Some((pg, tg)) = changed_take {
            let _ = self.with_project_mut(&pg, |p| {
                p.audio_sources.remove(&tg);
            });
        }
        #[cfg(not(any(feature = "audio", feature = "decode")))]
        let _ = changed_take;
        Ok(())
    }

    fn get_source_type(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> SourceType {
        let Some(guid) = resolve_project(self, &project) else {
            return SourceType::Unknown;
        };
        let Some(item_guid) = resolve_item_guid(&item) else {
            return SourceType::Unknown;
        };
        self.with_project(&guid, |p| {
            p.takes
                .get(&item_guid)
                .and_then(|tl| {
                    let idx = resolve_take_index(tl, &take)?;
                    tl.takes.get(idx).map(|t| t.source_type)
                })
                .unwrap_or(SourceType::Empty)
        })
        .unwrap_or(SourceType::Unknown)
    }

    // Take markers — kept per source (see `MarkerTarget`), with ids and
    // a drum hit's properties. Ratings are not modelled yet.

    fn get_take_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<TakeMarker> {
        let Some(target) = marker_target(self, &project, &item, &take) else {
            return Vec::new();
        };
        self.with_project(&target.project, |p| {
            p.take_markers.get(&target.key).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        marker: TakeMarkerCreate,
    ) -> Option<u32> {
        let target = marker_target(self, &project, &item, &take)?;
        let id = marker.id.unwrap_or_else(new_marker_id);
        let index = self
            .with_project_mut(&target.project, |p| {
                let list = p.take_markers.entry(target.key.clone()).or_default();
                list.push(TakeMarker {
                    index: 0,
                    id: id.clone(),
                    name: marker.name,
                    source_position_seconds: marker.source_position_seconds,
                    color: marker.color,
                    hit: marker.hit,
                });
                renumber(list);
                // The index it landed at, not the one it was pushed to:
                // markers are kept in source order, so an early marker
                // added late lands in the middle.
                list.iter()
                    .position(|m| m.id == id)
                    .unwrap_or(list.len() - 1) as u32
            })
            .ok()?;
        self.publish_take_changed(&target.project, &item, &target.take);
        Some(index)
    }

    fn set_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        update: TakeMarkerUpdate,
    ) -> DawResult<()> {
        let target = marker_target(self, &project, &item, &take)
            .ok_or_else(|| DawError::not_found("Take", "location"))?;
        self.with_project_mut(&target.project, |p| {
            let list = p.take_markers.entry(target.key.clone()).or_default();
            let Some(m) = list.get_mut(update.index as usize) else {
                return Err(DawError::not_found("TakeMarker", &update.index.to_string()));
            };
            if let Some(name) = update.name {
                m.name = name;
            }
            if let Some(pos) = update.source_position_seconds {
                m.source_position_seconds = pos;
            }
            // `Some(None)` clears, `None` leaves alone — the partial
            // update convention the proto documents.
            if let Some(color) = update.color {
                m.color = color;
            }
            if let Some(hit) = update.hit {
                m.hit = hit;
            }
            renumber(list);
            Ok(())
        })??;
        self.publish_take_changed(&target.project, &item, &target.take);
        Ok(())
    }

    fn delete_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        index: u32,
    ) -> DawResult<()> {
        let target = marker_target(self, &project, &item, &take)
            .ok_or_else(|| DawError::not_found("Take", "location"))?;
        self.with_project_mut(&target.project, |p| {
            let list = p.take_markers.entry(target.key.clone()).or_default();
            if (index as usize) >= list.len() {
                return Err(DawError::not_found("TakeMarker", &index.to_string()));
            }
            list.remove(index as usize);
            renumber(list);
            Ok(())
        })??;
        self.publish_take_changed(&target.project, &item, &target.take);
        Ok(())
    }

    fn set_take_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        markers: Vec<TakeMarker>,
    ) -> DawResult<()> {
        let target = marker_target(self, &project, &item, &take)
            .ok_or_else(|| DawError::not_found("Take", "location"))?;
        self.with_project_mut(&target.project, |p| {
            let mut list: Vec<TakeMarker> = markers
                .into_iter()
                .map(|mut m| {
                    if m.id.is_empty() {
                        m.id = new_marker_id();
                    }
                    m
                })
                .collect();
            renumber(&mut list);
            if list.is_empty() {
                p.take_markers.remove(&target.key);
            } else {
                p.take_markers.insert(target.key.clone(), list);
            }
        })?;
        self.publish_take_changed(&target.project, &item, &target.take);
        Ok(())
    }

    fn add_take_marker_at_position(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        request: AddTakeMarkerAtPositionRequest,
    ) -> Option<u32> {
        // Taken as a source position directly. Standalone models no
        // take offset or play rate on this path, so a conversion here
        // would be inventing one — and inventing one silently is worse
        // than treating the two as the same number and saying so.
        let seconds = request.position.seconds().unwrap_or(0.0);
        self.add_take_marker(
            project,
            item,
            take,
            TakeMarkerCreate {
                name: request.name,
                source_position_seconds: seconds,
                color: request.color,
                ..TakeMarkerCreate::default()
            },
        )
    }

    fn run_take_rating_action(
        &self,
        _project: ProjectContext,
        _item: ItemRef,
        _take: TakeRef,
        _command_id: u32,
    ) -> DawResult<()> {
        Ok(())
    }
}

impl Standalone {
    /// Mutate a take in place and publish [`TakeEvent::Changed`] — for the
    /// properties no specific variant names, so every setter announces
    /// itself and a subscriber that re-reads on take events misses nothing.
    fn mutate_take_changed<F>(
        &self,
        project: &ProjectContext,
        item: &ItemRef,
        take: &TakeRef,
        f: F,
    ) -> DawResult<()>
    where
        F: FnOnce(&mut Take),
    {
        self.mutate_take_evt(project, item, take, |pg, ig, t| {
            f(t);
            TakeEvent::Changed {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                take_guid: t.guid.clone(),
            }
        })
    }

    /// Announce a change to take `take_guid`'s markers (read back with
    /// `get_take_markers`) as [`TakeEvent::Changed`].
    fn publish_take_changed(&self, project_guid: &str, item: &ItemRef, take_guid: &str) {
        let Some(item_guid) = resolve_item_guid(item) else {
            return;
        };
        self.bus_events.publish(DawEvent::Take(TakeEvent::Changed {
            project_guid: project_guid.to_string(),
            item_guid,
            take_guid: take_guid.to_string(),
        }));
    }

    /// Mutate one take in place. The closure — given the project guid, item
    /// guid, and mutable take — returns the [`TakeEvent`] to publish on the
    /// cross-domain bus (so the sync engine replicates the change).
    fn mutate_take_evt<F>(
        &self,
        project: &ProjectContext,
        item: &ItemRef,
        take: &TakeRef,
        f: F,
    ) -> DawResult<()>
    where
        F: FnOnce(&str, &str, &mut Take) -> TakeEvent,
    {
        let guid = resolve_project(self, project).ok_or_else(not_found_proj)?;
        let item_guid = resolve_item_guid(item)
            .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
        let event = self.with_project_mut(&guid, |p| {
            let tl = p
                .takes
                .get_mut(&item_guid)
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let idx = resolve_take_index(tl, take)
                .ok_or_else(|| DawError::not_found("Take", &format!("{take:?}")))?;
            let t = tl
                .takes
                .get_mut(idx)
                .ok_or_else(|| DawError::not_found("Take", &format!("{take:?}")))?;
            let event = f(&guid, &item_guid, t);
            Ok::<TakeEvent, DawError>(event)
        })??;
        self.bus_events.publish(DawEvent::Take(event));
        Ok(())
    }
}

/// How far past a take's own span its envelope's points are kept,
/// seconds: room for a slip back and a reach over a gap.
const ENVELOPE_PAD: f64 = 1.0;

/// `points` (by time) cut to those over `0..length` and [`ENVELOPE_PAD`]
/// either side, with the nearest beyond each end kept so the curve is
/// what it was at the edges.
fn keep_over(points: &mut Vec<daw_proto::automation::EnvelopePoint>, length: f64) {
    let (from, to) = (-ENVELOPE_PAD, length + ENVELOPE_PAD);
    let first = points
        .iter()
        .position(|p| p.time.as_seconds() >= from)
        .unwrap_or(points.len());
    let last = points
        .iter()
        .rposition(|p| p.time.as_seconds() <= to)
        .map_or(0, |i| i + 1);
    let keep_from = first.saturating_sub(1);
    let keep_to = (last + 1).min(points.len());
    if keep_from >= keep_to {
        points.truncate(0);
        return;
    }
    points.truncate(keep_to);
    points.drain(..keep_from);
    for (i, p) in points.iter_mut().enumerate() {
        p.index = u32::try_from(i).unwrap_or(u32::MAX);
    }
}
