//! `impl Items for Standalone` — post-architect::rpc port.
//!
//! Backed by `ProjectState::items` (HashMap<item_guid, ItemEntry>) plus
//! `ProjectState::items_by_track` (ordering per track). The old
//! `StandaloneItem` async service struct and parallel item state were
//! retired with the port.

use crate::sync::EnvelopeKey;
use daw_proto::Items;
use daw_proto::event_bus::DawEvent;
use daw_proto::item::ItemEvent;
use daw_proto::{
    BeatAttachMode, DawError, DawResult, Duration, FadeShape, Item, ItemRef, ItemSpan,
    PositionInSeconds, ProjectContext, TrackRef,
};
use uuid::Uuid;

use crate::sync::{ItemEntry, ProjectState, Standalone};

/// Publish an item change onto the cross-domain event bus so subscribers
/// (the sync engine, inspectors) see it. Item events carry `project_guid`
/// per variant, so no separate envelope is needed.
fn publish_item_event(daw: &Standalone, event: ItemEvent) {
    daw.bus_events.publish(DawEvent::Item(event));
}

/// Mutate one item in place. The closure — given the project guid, item
/// guid, and a mutable item (old values readable before it writes) —
/// returns the [`ItemEvent`] describing the change, which is published on
/// the bus. Keeps old-value capture, mutation, and publish in one place.
fn mutate_item_evt<F>(
    daw: &Standalone,
    project: &ProjectContext,
    item: &ItemRef,
    f: F,
) -> DawResult<()>
where
    F: FnOnce(&str, &str, &mut Item) -> ItemEvent,
{
    let guid = resolve_project(daw, project).ok_or_else(no_project)?;
    let event = daw.with_project_mut(&guid, |p| {
        let item_guid = item_guid_from_ref(p, item)
            .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
        let entry = p
            .items
            .get_mut(&item_guid)
            .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
        let event = f(&guid, &item_guid, &mut entry.item);
        Ok::<ItemEvent, DawError>(event)
    })??;
    publish_item_event(daw, event);
    Ok(())
}

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn no_project() -> DawError {
    DawError::not_found("Project", "context")
}

fn resolve_track_guid(p: &ProjectState, track: &TrackRef) -> Option<String> {
    match track {
        TrackRef::Guid(g) => Some(g.clone()),
        TrackRef::Index(idx) => p.tracks.get(*idx as usize).map(|t| t.guid.clone()),
        TrackRef::Master => None,
    }
}

/// The item as stored, with the active take's `start_offset` filled in.
///
/// Derived on read rather than mirrored into `ItemEntry`, so there is
/// one place the offset lives and `set_start_offset` cannot leave the
/// item saying something the take disagrees with. Zero when the item
/// has no takes.
fn with_start_offset(p: &ProjectState, guid: &str, item: &Item) -> Item {
    let mut item = item.clone();
    item.start_offset = p
        .takes
        .get(guid)
        .and_then(|list| list.takes.get(list.active_idx as usize))
        .map_or(daw_proto::primitives::Duration::ZERO, |take| {
            take.start_offset
        });
    item
}

fn item_guid_from_ref(p: &ProjectState, item: &ItemRef) -> Option<String> {
    match item {
        ItemRef::Guid(g) => Some(g.clone()),
        ItemRef::ProjectIndex(idx) => {
            // Loose mapping: take the Nth item by insertion order.
            p.items.keys().nth(*idx as usize).cloned()
        }
        ItemRef::Index(_) => None,
    }
}

/// Mutate an item in place and publish [`ItemEvent::Changed`] — for the
/// properties no specific variant names, so every setter announces
/// itself and a subscriber that re-reads on item events misses nothing.
fn mutate_item_changed<F>(
    daw: &Standalone,
    project: &ProjectContext,
    item: &ItemRef,
    f: F,
) -> DawResult<()>
where
    F: FnOnce(&mut Item),
{
    mutate_item_evt(daw, project, item, |pg, ig, i| {
        f(i);
        ItemEvent::Changed {
            project_guid: pg.to_string(),
            item_guid: ig.to_string(),
        }
    })
}

/// Make an item on `track` — the body of both `add_item` and
/// `add_item_with_guid`, so an item made with a caller's guid is the
/// same item `add_item` makes and announces itself the same way.
/// `guid: None` mints the engine's own; a guid an item of the project
/// already has is refused.
fn insert_item(
    daw: &Standalone,
    project: &ProjectContext,
    track: &TrackRef,
    guid: Option<String>,
    span: ItemSpan,
) -> DawResult<String> {
    let project_guid = resolve_project(daw, project).ok_or_else(no_project)?;
    let (item_guid, track_guid, item) = daw.with_project_mut(&project_guid, |p| {
        let track_guid = resolve_track_guid(p, track)
            .filter(|g| p.tracks.iter().any(|t| &t.guid == g))
            .ok_or_else(|| DawError::not_found("Track", &format!("{track:?}")))?;
        let item_guid = match guid {
            Some(g) if p.items.contains_key(&g) => {
                return Err(DawError::already_exists("Item", &g));
            }
            Some(g) => g,
            None => p.mint_item_guid(),
        };
        let order = p.items_by_track.entry(track_guid.clone()).or_default();
        let index = order.len() as u32;
        order.push(item_guid.clone());
        let mut item = Item::default();
        item.guid = item_guid.clone();
        item.track_guid = track_guid.clone();
        item.index = index;
        item.position = span.position;
        item.length = span.length;
        p.items
            .insert(item_guid.clone(), ItemEntry { item: item.clone() });
        Ok((item_guid, track_guid, item))
    })??;
    publish_item_event(
        daw,
        ItemEvent::Created {
            project_guid,
            track_guid,
            item,
        },
    );
    Ok(item_guid)
}

impl Items for Standalone {
    fn get_items(&self, project: ProjectContext, track: TrackRef) -> Vec<Item> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            let Some(track_guid) = resolve_track_guid(p, &track) else {
                return Vec::new();
            };
            p.items_by_track
                .get(&track_guid)
                .map(|guids| {
                    // With the take's offset, as `get_item` gives it: a
                    // track's items read without it all claimed to play
                    // their source from the start.
                    guids
                        .iter()
                        .filter_map(|g| p.items.get(g).map(|e| with_start_offset(p, g, &e.item)))
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn get_item(&self, project: ProjectContext, item: ItemRef) -> Option<Item> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| {
            let item_guid = item_guid_from_ref(p, &item)?;
            p.items
                .get(&item_guid)
                .map(|e| with_start_offset(p, &item_guid, &e.item))
        })
        .ok()
        .flatten()
    }

    fn get_all_items(&self, project: ProjectContext) -> Vec<Item> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            p.items
                .iter()
                .map(|(guid, e)| with_start_offset(p, guid, &e.item))
                .collect()
        })
        .unwrap_or_default()
    }

    fn get_selected_items(&self, project: ProjectContext) -> Vec<Item> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            p.items
                .values()
                .filter(|e| e.item.selected)
                .map(|e| e.item.clone())
                .collect()
        })
        .unwrap_or_default()
    }

    fn item_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0;
        };
        self.with_project(&guid, |p| {
            let Some(track_guid) = resolve_track_guid(p, &track) else {
                return 0;
            };
            p.items_by_track
                .get(&track_guid)
                .map(|v| v.len() as u32)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn add_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        position: PositionInSeconds,
        length: Duration,
    ) -> Option<String> {
        insert_item(
            self,
            &project,
            &track,
            None,
            ItemSpan::new(position, length),
        )
        .ok()
    }

    fn add_item_with_guid(
        &self,
        project: ProjectContext,
        track: TrackRef,
        guid: &str,
        span: ItemSpan,
    ) -> DawResult<String> {
        crate::check_new_guid("Item", guid)?;
        insert_item(self, &project, &track, Some(guid.to_string()), span)
    }

    fn delete_item(&self, project: ProjectContext, item: ItemRef) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        let (item_guid, track_guid) = self.with_project_mut(&guid, |p| {
            let item_guid = item_guid_from_ref(p, &item)
                .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
            let entry = p
                .items
                .remove(&item_guid)
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let track_guid = entry.item.track_guid.clone();
            if let Some(order) = p.items_by_track.get_mut(&entry.item.track_guid) {
                order.retain(|g| g != &item_guid);
                for (i, g) in order.iter().enumerate() {
                    if let Some(e) = p.items.get_mut(g) {
                        e.item.index = i as u32;
                    }
                }
            }
            p.takes.remove(&item_guid);
            Ok::<(String, String), DawError>((item_guid, track_guid))
        })??;
        publish_item_event(
            self,
            ItemEvent::Deleted {
                project_guid: guid,
                track_guid,
                item_guid,
            },
        );
        Ok(())
    }

    fn duplicate_item(&self, project: ProjectContext, item: ItemRef) -> Option<String> {
        let guid = resolve_project(self, &project)?;
        let created = self
            .with_project_mut(&guid, |p| {
                let item_guid = item_guid_from_ref(p, &item)?;
                let src = p.items.get(&item_guid)?.clone();
                let new_guid = p.mint_item_guid();
                let order = p
                    .items_by_track
                    .entry(src.item.track_guid.clone())
                    .or_default();
                let new_index = order.len() as u32;
                order.push(new_guid.clone());
                let mut new_item = src.item.clone();
                new_item.guid = new_guid.clone();
                new_item.index = new_index;
                p.items.insert(
                    new_guid.clone(),
                    ItemEntry {
                        item: new_item.clone(),
                    },
                );

                // The takes come with it. An item without them is not a
                // duplicate of anything: `item.take_count` was copied
                // and says 1, so the copy *claims* a take, and every
                // take call on it then fails with "item not found" —
                // which reads as the item being missing rather than as
                // its takes never having been made.
                //
                // Take guids are minted fresh. Two takes sharing a guid
                // means `TakeRef::Guid` resolves to whichever comes
                // first, so an edit aimed at the copy lands on the
                // original.
                if let Some(src_takes) = p.takes.get(&item_guid).cloned() {
                    let mut takes = src_takes;
                    for take in &mut takes.takes {
                        let old_guid = take.guid.clone();
                        // Its envelopes are keyed by item and take: the
                        // copy's are copies, or a split's right half plays
                        // without the left's volume curve.
                        let curves: Vec<_> = p
                            .envelopes
                            .iter()
                            .filter_map(|((owner, key), data)| match key {
                                EnvelopeKey::Take {
                                    item_guid: of_item,
                                    take_guid: of_take,
                                    kind,
                                } if *of_item == item_guid && *of_take == old_guid => {
                                    Some((owner.clone(), *kind, data.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        let new_take = Uuid::new_v4().to_string();
                        for (owner, kind, data) in curves {
                            p.envelopes.insert(
                                (
                                    owner,
                                    EnvelopeKey::Take {
                                        item_guid: new_guid.clone(),
                                        take_guid: new_take.clone(),
                                        kind,
                                    },
                                ),
                                data,
                            );
                        }
                        take.guid = new_take;
                        take.item_guid = new_guid.clone();
                        // Everything keyed by the take guid comes with
                        // it, or the duplicate is silent and unwarped
                        // where the original plays: the materialized
                        // source (an `Arc`, so this is a refcount, not a
                        // copy), the stretch markers, and the mode.
                        #[cfg(any(feature = "audio", feature = "decode"))]
                        if let Some(audio) = p.audio_sources.get(&old_guid).cloned() {
                            p.audio_sources.insert(take.guid.clone(), audio);
                        }
                        if let Some(markers) = p.stretch_markers.get(&old_guid).cloned() {
                            p.stretch_markers.insert(take.guid.clone(), markers);
                        }
                        if let Some(mode) = p.stretch_modes.get(&old_guid).copied() {
                            p.stretch_modes.insert(take.guid.clone(), mode);
                        }
                        // A MIDI take's events are keyed by it too: without
                        // them the copy is an empty MIDI item, and a split —
                        // a duplicate trimmed to the right half — loses
                        // every note after the cut.
                        copy_keyed(&mut p.midi_notes, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_ccs, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_pitch_bends, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_program_changes, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_sysex, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_channel_pressures, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_poly_pressures, &old_guid, &take.guid);
                        copy_keyed(&mut p.midi_note_expressions, &old_guid, &take.guid);
                    }
                    p.takes.insert(new_guid.clone(), takes);
                }

                Some((new_guid, new_item))
            })
            .ok()
            .flatten();
        let (new_guid, new_item) = created?;
        publish_item_event(
            self,
            ItemEvent::Created {
                project_guid: guid,
                track_guid: new_item.track_guid.clone(),
                item: new_item,
            },
        );
        Some(new_guid)
    }

    fn set_position(
        &self,
        project: ProjectContext,
        item: ItemRef,
        position: PositionInSeconds,
    ) -> DawResult<()> {
        mutate_item_evt(self, &project, &item, |pg, ig, i| {
            let old = i.position.as_seconds();
            i.position = position;
            ItemEvent::PositionChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                old_position: old,
                new_position: position.as_seconds(),
            }
        })
    }

    fn set_length(
        &self,
        project: ProjectContext,
        item: ItemRef,
        length: Duration,
    ) -> DawResult<()> {
        mutate_item_evt(self, &project, &item, |pg, ig, i| {
            let old = i.length.as_seconds();
            i.length = length;
            ItemEvent::LengthChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                old_length: old,
                new_length: length.as_seconds(),
            }
        })
    }

    fn move_to_track(
        &self,
        project: ProjectContext,
        item: ItemRef,
        track: TrackRef,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        let (item_guid, old_track, new_track) = self.with_project_mut(&guid, |p| {
            let item_guid = item_guid_from_ref(p, &item)
                .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
            let track_guid = resolve_track_guid(p, &track)
                .ok_or_else(|| DawError::not_found("Track", &format!("{track:?}")))?;
            // Remove from current track's ordering.
            let entry = p
                .items
                .get(&item_guid)
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let prev_track = entry.item.track_guid.clone();
            if let Some(order) = p.items_by_track.get_mut(&prev_track) {
                order.retain(|g| g != &item_guid);
            }
            // Insert into new track.
            let order = p.items_by_track.entry(track_guid.clone()).or_default();
            let new_index = order.len() as u32;
            order.push(item_guid.clone());
            if let Some(e) = p.items.get_mut(&item_guid) {
                e.item.track_guid = track_guid.clone();
                e.item.index = new_index;
            }
            Ok::<(String, String, String), DawError>((item_guid, prev_track, track_guid))
        })??;
        publish_item_event(
            self,
            ItemEvent::MovedToTrack {
                project_guid: guid,
                item_guid,
                old_track_guid: old_track,
                new_track_guid: new_track,
            },
        );
        Ok(())
    }

    fn set_snap_offset(
        &self,
        project: ProjectContext,
        item: ItemRef,
        offset: Duration,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.snap_offset = offset)
    }

    fn set_muted(&self, project: ProjectContext, item: ItemRef, muted: bool) -> DawResult<()> {
        mutate_item_evt(self, &project, &item, |pg, ig, i| {
            i.muted = muted;
            ItemEvent::MuteChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                muted,
            }
        })
    }

    fn set_selected(
        &self,
        project: ProjectContext,
        item: ItemRef,
        selected: bool,
    ) -> DawResult<()> {
        mutate_item_evt(self, &project, &item, |pg, ig, i| {
            i.selected = selected;
            ItemEvent::SelectionChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                selected,
            }
        })
    }

    fn set_locked(&self, project: ProjectContext, item: ItemRef, locked: bool) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.locked = locked)
    }

    fn select_all_items(&self, project: ProjectContext, selected: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        // One event per item whose selection actually flipped: the ones
        // already in that state did not change.
        let flipped = self.with_project_mut(&guid, |p| {
            let mut flipped = Vec::new();
            for (item_guid, entry) in p.items.iter_mut() {
                if entry.item.selected != selected {
                    entry.item.selected = selected;
                    flipped.push(item_guid.clone());
                }
            }
            flipped
        })?;
        for item_guid in flipped {
            publish_item_event(
                self,
                ItemEvent::SelectionChanged {
                    project_guid: guid.clone(),
                    item_guid,
                    selected,
                },
            );
        }
        Ok(())
    }

    fn set_volume(&self, project: ProjectContext, item: ItemRef, volume: f64) -> DawResult<()> {
        mutate_item_evt(self, &project, &item, |pg, ig, i| {
            i.volume = volume;
            ItemEvent::VolumeChanged {
                project_guid: pg.to_string(),
                item_guid: ig.to_string(),
                volume,
            }
        })
    }

    fn set_fade_in(
        &self,
        project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| {
            i.fade_in_length = length;
            i.fade_in_shape = shape;
        })
    }

    fn set_fade_out(
        &self,
        project: ProjectContext,
        item: ItemRef,
        length: Duration,
        shape: FadeShape,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| {
            i.fade_out_length = length;
            i.fade_out_shape = shape;
        })
    }

    fn set_loop_source(
        &self,
        project: ProjectContext,
        item: ItemRef,
        loop_source: bool,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.loop_source = loop_source)
    }

    fn set_beat_attach_mode(
        &self,
        project: ProjectContext,
        item: ItemRef,
        mode: BeatAttachMode,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.beat_attach_mode = mode)
    }

    fn set_auto_stretch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        auto_stretch: bool,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.auto_stretch = auto_stretch)
    }

    fn set_color(
        &self,
        project: ProjectContext,
        item: ItemRef,
        color: Option<u32>,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.color = color)
    }

    fn label(&self, project: ProjectContext, item: ItemRef) -> Option<String> {
        Items::get_item(self, project, item).and_then(|i| i.label)
    }

    fn set_label(&self, project: ProjectContext, item: ItemRef, label: &str) -> DawResult<()> {
        let label = label.to_string();
        mutate_item_changed(self, &project, &item, |i| i.label = Some(label))
    }

    fn set_group_id(
        &self,
        project: ProjectContext,
        item: ItemRef,
        group_id: Option<u32>,
    ) -> DawResult<()> {
        mutate_item_changed(self, &project, &item, |i| i.group_id = group_id)
    }

    fn set_fixed_lane(&self, project: ProjectContext, item: ItemRef, lane: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        let item_guid = self.with_project_mut(&guid, |p| {
            let item_guid = item_guid_from_ref(p, &item)
                .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
            let track_guid = p
                .items
                .get(&item_guid)
                .map(|e| e.item.track_guid.clone())
                .ok_or_else(|| DawError::not_found("Item", &item_guid))?;
            let lane_count = p
                .tracks
                .iter()
                .find(|t| t.guid == track_guid)
                .map(|t| t.lane_count)
                .unwrap_or(0);
            crate::track::check_lane(lane, lane_count)?;
            if let Some(entry) = p.items.get_mut(&item_guid) {
                entry.item.fixed_lane = Some(lane);
            }
            Ok::<String, DawError>(item_guid)
        })??;
        publish_item_event(
            self,
            ItemEvent::Changed {
                project_guid: guid,
                item_guid,
            },
        );
        Ok(())
    }
}

/// `map[to]` as a copy of `map[from]`, when there is one.
fn copy_keyed<T: Clone>(map: &mut std::collections::HashMap<String, T>, from: &str, to: &str) {
    if let Some(value) = map.get(from).cloned() {
        map.insert(to.to_owned(), value);
    }
}
