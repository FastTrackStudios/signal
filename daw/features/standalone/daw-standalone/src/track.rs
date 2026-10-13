//! `impl Tracks for Standalone` — post-architect::rpc port.
//!
//! Backed by `ProjectState::tracks: Vec<Track>` in the existing
//! in-memory state. Master track is synthesized on demand. The old
//! 400+-line async `StandaloneTrack` impl with parallel `TrackState`
//! storage was retired in favor of operating directly on the
//! canonical state — fewer places for state to drift.

use daw_proto::Tracks;
use daw_proto::track::{
    Comp, CompArea, GROUP_SLOTS, GroupFamily, GroupFlagChange, GroupModifierChange, LaneComping,
    ReorderTracksBehavior, TrackEvent, TrackGrouping, TrackStreamEvent, check_group_slot,
    group_slot_bit,
};
use daw_proto::{DawError, DawResult, ProjectContext, RecordInput, Track, TrackRef};
use uuid::Uuid;

use crate::sync::{Standalone, TrackExt};

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn find_track_index(tracks: &[Track], r: &TrackRef) -> Option<usize> {
    match r {
        TrackRef::Guid(guid) => tracks.iter().position(|t| t.guid == *guid),
        TrackRef::Index(idx) => {
            let i = *idx as usize;
            if i < tracks.len() { Some(i) } else { None }
        }
        TrackRef::Master => None,
    }
}

/// Tracks ganged to a control gesture on `origin`: every other track
/// whose FOLLOW mask for the parameter intersects the origin's LEAD
/// mask (REAPER's grouping matrix — touching a lead's control moves
/// the followers' controls).
fn group_followers(
    tracks: &[Track],
    origin: usize,
    lead_of: impl Fn(&daw_proto::track::TrackGrouping) -> u128,
    follow_of: impl Fn(&daw_proto::track::TrackGrouping) -> u128,
) -> Vec<usize> {
    let lead_mask = lead_of(&tracks[origin].grouping);
    if lead_mask == 0 || !crate::grouping::enabled() {
        return Vec::new();
    }
    tracks
        .iter()
        .enumerate()
        .filter(|(j, t)| *j != origin && follow_of(&t.grouping) & lead_mask != 0)
        .map(|(j, _)| j)
        .collect()
}

fn not_found_proj() -> DawError {
    DawError::not_found("Project", "context")
}

fn not_found_track() -> DawError {
    DawError::not_found("Track", "")
}

fn reconcile_track_structure(tracks: &mut [Track]) {
    let mut folder_stack: Vec<String> = Vec::new();
    for (index, track) in tracks.iter_mut().enumerate() {
        track.index = index as u32;
        track.parent_guid = folder_stack.last().cloned();
        track.is_folder = track.folder_depth > 0;

        if track.folder_depth > 0 {
            folder_stack.push(track.guid.clone());
        } else if track.folder_depth < 0 {
            for _ in 0..track.folder_depth.unsigned_abs() {
                folder_stack.pop();
            }
        }
    }
}

fn selected_reorder_insert_index(tracks: &[Track], index: u32) -> usize {
    let target = (index as usize).min(tracks.len());
    let selected_before_target = tracks
        .iter()
        .take(target)
        .filter(|track| track.selected)
        .count();
    target.saturating_sub(selected_before_target)
}

fn moved_events(before: &[Track], after: &[Track]) -> Vec<TrackEvent> {
    let old_indices = before
        .iter()
        .map(|track| (track.guid.as_str(), track.index))
        .collect::<std::collections::HashMap<_, _>>();

    after
        .iter()
        .filter_map(|track| {
            let old_index = old_indices.get(track.guid.as_str()).copied()?;
            (old_index != track.index).then(|| TrackEvent::Moved {
                guid: track.guid.clone(),
                old_index,
                new_index: track.index,
            })
        })
        .collect()
}

/// The lanes event for a track, as it stands.
///
/// Built from the track rather than from the change, because the four
/// fields are read together and drawn together: a comp view told the
/// count without the names would draw the right number of empty
/// labels, which is worse than not redrawing at all.
fn lanes_event(track: &daw_proto::Track) -> TrackEvent {
    TrackEvent::LanesChanged {
        guid: track.guid.clone(),
        lane_count: track.lane_count,
        lane_play_mask: track.lane_play_mask,
        lane_names: track.lane_names.clone(),
        lane_display: track.lane_display,
    }
}

/// Fill each track's send and receive counts from the routing maps.
///
/// Derived at read time rather than stored on the track, because the
/// routing maps are the authority and a stored count would go stale the
/// first time anything added a send without remembering to update it.
/// The maps are keyed by guid, so this is a lookup per track and not a
/// scan per track.
fn fill_route_counts(p: &crate::sync::ProjectState, tracks: &mut [daw_proto::Track]) {
    for track in tracks.iter_mut() {
        track.send_count = p
            .sends
            .get(&track.guid)
            .map_or(0, |routes: &Vec<daw_proto::TrackRoute>| {
                u32::try_from(routes.len()).unwrap_or(u32::MAX)
            });
        track.receive_count = p
            .receives
            .get(&track.guid)
            .map_or(0, |routes: &Vec<daw_proto::TrackRoute>| {
                u32::try_from(routes.len()).unwrap_or(u32::MAX)
            });
    }
}

/// Insert a track named `name` with guid `new_guid` at `at_index`
/// (`None` = append) — the body of both `add` and `add_with_guid`, so a
/// track made with a caller's guid is the same track `add` makes and
/// announces itself the same way. A guid a track already has is refused.
fn insert_track(
    daw: &Standalone,
    project: &ProjectContext,
    new_guid: String,
    name: &str,
    at_index: Option<u32>,
) -> DawResult<String> {
    let guid = resolve_project(daw, project).ok_or_else(not_found_proj)?;
    let events = daw.with_project_mut(&guid, |p| {
        if p.tracks.iter().any(|t| t.guid == new_guid) {
            return Err(DawError::already_exists("Track", &new_guid));
        }
        let before = p.tracks.clone();
        let pos = at_index
            .map(|i| (i as usize).min(p.tracks.len()))
            .unwrap_or(p.tracks.len());
        let track = Track {
            guid: new_guid.clone(),
            index: pos as u32,
            name: name.to_string(),
            ..Default::default()
        };
        p.tracks.insert(pos, track);
        reconcile_track_structure(&mut p.tracks);
        let added = p.tracks[pos].clone();
        let mut events = vec![TrackEvent::Added(added)];
        events.extend(moved_events(&before, &p.tracks));
        Ok(events)
    })??;
    publish_track_events(daw, &guid, events);
    Ok(new_guid)
}

fn publish_track_events(daw: &Standalone, project_guid: &str, events: Vec<TrackEvent>) {
    for event in events {
        let event = TrackStreamEvent {
            project_guid: project_guid.to_string(),
            event,
        };
        daw.bus_events
            .publish(daw_proto::event_bus::DawEvent::Track(event.clone()));
        daw.track_events.publish(event);
    }
}

impl daw_proto::track::TracksStreamSource for Standalone {
    fn events_hub(&self) -> &architect::PubSub<TrackStreamEvent> {
        &self.track_events
    }
}

impl Tracks for Standalone {
    fn all(&self, project: ProjectContext) -> Vec<Track> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            let mut tracks = p.tracks.clone();
            fill_route_counts(p, &mut tracks);
            tracks
        })
        .unwrap_or_default()
    }

    fn get(&self, project: ProjectContext, track: TrackRef) -> Option<Track> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| {
            find_track_index(&p.tracks, &track).map(|i| {
                let mut one = [p.tracks[i].clone()];
                fill_route_counts(p, &mut one);
                let [track] = one;
                track
            })
        })
        .ok()
        .flatten()
    }

    fn count(&self, project: ProjectContext) -> u32 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0;
        };
        self.with_project(&guid, |p| p.tracks.len() as u32)
            .unwrap_or(0)
    }

    fn selected(&self, project: ProjectContext) -> Vec<Track> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            p.tracks.iter().filter(|t| t.selected).cloned().collect()
        })
        .unwrap_or_default()
    }

    fn master(&self, project: ProjectContext) -> Option<Track> {
        // Standalone synthesizes a master track on demand — there's no
        // persistent master row in `ProjectState::tracks`.
        let _ = project;
        Some(Track {
            guid: "master".to_string(),
            index: 0,
            name: "MASTER".to_string(),
            ..Default::default()
        })
    }

    fn set_automation_mode(
        &self,
        project: ProjectContext,
        track: TrackRef,
        mode: daw_proto::primitives::AutomationMode,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let event = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            p.tracks[i].automation_mode = mode;
            let track_guid = p.tracks[i].guid.clone();
            // The track mode IS the envelopes' mode — propagate so the
            // existing touch/latch/write machinery records (or stops
            // recording) immediately. REAPER's I_AUTOMODE semantics.
            for (key, data) in p.envelopes.iter_mut() {
                if key.0 == track_guid {
                    data.automation_mode = mode;
                }
            }
            Ok::<_, DawError>(TrackEvent::AutomationModeChanged {
                guid: track_guid,
                mode,
            })
        })??;
        publish_track_events(self, &guid, vec![event]);
        Ok(())
    }

    fn set_input_monitor(
        &self,
        project: ProjectContext,
        track: TrackRef,
        monitor: daw_proto::track::InputMonitoringMode,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let event = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            p.tracks[i].input_monitor = monitor;
            Ok::<_, DawError>(TrackEvent::InputMonitorChanged {
                guid: p.tracks[i].guid.clone(),
                monitor,
            })
        })??;
        publish_track_events(self, &guid, vec![event]);
        Ok(())
    }

    fn set_phase_inverted(
        &self,
        project: ProjectContext,
        track: TrackRef,
        inverted: bool,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let mut events = Vec::new();
            // Polarity group: gang the flip.
            for j in group_followers(&p.tracks, i, |g| g.polarity_lead, |g| g.polarity_follow) {
                p.tracks[j].phase_inverted = inverted;
                events.push(TrackEvent::PhaseInvertedChanged {
                    guid: p.tracks[j].guid.clone(),
                    inverted,
                });
            }
            p.tracks[i].phase_inverted = inverted;
            events.push(TrackEvent::PhaseInvertedChanged {
                guid: p.tracks[i].guid.clone(),
                inverted,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_muted(&self, project: ProjectContext, track: TrackRef, muted: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let mut events = Vec::new();
            // Mute group: the gesture gangs to followers.
            for j in group_followers(&p.tracks, i, |g| g.mute_lead, |g| g.mute_follow) {
                p.tracks[j].muted = muted;
                events.push(TrackEvent::MuteChanged {
                    guid: p.tracks[j].guid.clone(),
                    muted,
                });
            }
            p.tracks[i].muted = muted;
            events.push(TrackEvent::MuteChanged {
                guid: p.tracks[i].guid.clone(),
                muted,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_soloed(&self, project: ProjectContext, track: TrackRef, soloed: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let mut events = Vec::new();
            for j in group_followers(&p.tracks, i, |g| g.solo_lead, |g| g.solo_follow) {
                p.tracks[j].soloed = soloed;
                events.push(TrackEvent::SoloChanged {
                    guid: p.tracks[j].guid.clone(),
                    soloed,
                });
            }
            p.tracks[i].soloed = soloed;
            events.push(TrackEvent::SoloChanged {
                guid: p.tracks[i].guid.clone(),
                soloed,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_solo_exclusive(&self, project: ProjectContext, track: TrackRef) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let target = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let target_guid = p.tracks[target].guid.clone();
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                let next = t.guid == target_guid;
                if t.soloed != next {
                    t.soloed = next;
                    events.push(TrackEvent::SoloChanged {
                        guid: t.guid.clone(),
                        soloed: next,
                    });
                }
            }
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn clear_all_solo(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                if t.soloed {
                    t.soloed = false;
                    events.push(TrackEvent::SoloChanged {
                        guid: t.guid.clone(),
                        soloed: false,
                    });
                }
            }
            events
        })?;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_armed(&self, project: ProjectContext, track: TrackRef, armed: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let mut events = Vec::new();
            // Record-arm group: arming the lead arms every follower
            // (the showcase session's "BAND RECORD VCA" arms the band).
            for j in group_followers(&p.tracks, i, |g| g.recarm_lead, |g| g.recarm_follow) {
                p.tracks[j].armed = armed;
                events.push(TrackEvent::ArmChanged {
                    guid: p.tracks[j].guid.clone(),
                    armed,
                });
            }
            p.tracks[i].armed = armed;
            events.push(TrackEvent::ArmChanged {
                guid: p.tracks[i].guid.clone(),
                armed,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_volume(&self, project: ProjectContext, track: TrackRef, volume: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let mut events = Vec::new();
            // Volume group: gestures gang RELATIVELY (a dB offset =
            // a linear ratio), matching REAPER's fader linking.
            let old = p.tracks[i].volume;
            let ratio = if old.abs() > 1e-12 { volume / old } else { 0.0 };
            for j in group_followers(&p.tracks, i, |g| g.volume_lead, |g| g.volume_follow) {
                let next = if ratio > 0.0 {
                    p.tracks[j].volume * ratio
                } else {
                    volume
                };
                p.tracks[j].volume = next;
                events.push(TrackEvent::VolumeChanged {
                    guid: p.tracks[j].guid.clone(),
                    volume: next,
                });
            }
            p.tracks[i].volume = volume;
            events.push(TrackEvent::VolumeChanged {
                guid: p.tracks[i].guid.clone(),
                volume,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_pan(&self, project: ProjectContext, track: TrackRef, pan: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let pan = pan.clamp(-1.0, 1.0);
            let mut events = Vec::new();
            // Pan group: gang the gesture as an additive offset.
            let delta = pan - p.tracks[i].pan;
            for j in group_followers(&p.tracks, i, |g| g.pan_lead, |g| g.pan_follow) {
                let next = (p.tracks[j].pan + delta).clamp(-1.0, 1.0);
                p.tracks[j].pan = next;
                events.push(TrackEvent::PanChanged {
                    guid: p.tracks[j].guid.clone(),
                    pan: next,
                });
            }
            p.tracks[i].pan = pan;
            events.push(TrackEvent::PanChanged {
                guid: p.tracks[i].guid.clone(),
                pan,
            });
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    // Track-group slots: names live in the project-info store under the
    // key REAPER uses, membership on `Track::grouping`.
    fn set_group_name(&self, project: ProjectContext, slot: u32, name: &str) -> DawResult<()> {
        check_group_slot(slot)?;
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            p.project_ext_state.insert(
                (String::new(), format!("TRACK_GROUP_NAME:{slot}")),
                name.to_string(),
            );
        })?;
        Ok(())
    }

    fn first_free_group_slot(
        &self,
        project: ProjectContext,
        band_start: u32,
        band_end: u32,
    ) -> Option<u32> {
        let guid = resolve_project(self, &project)?;
        let used = self.read_project(&guid, |p| {
            p.tracks
                .iter()
                .fold(0u128, |acc, t| acc | t.grouping.member_mask())
        })?;
        (band_start..=band_end.min(GROUP_SLOTS)).find(|slot| used & group_slot_bit(*slot) == 0)
    }

    fn set_group_membership(
        &self,
        project: ProjectContext,
        track: TrackRef,
        slot: u32,
        member: bool,
    ) -> DawResult<()> {
        check_group_slot(slot)?;
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let changed = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            for fam in GroupFamily::ALL {
                p.tracks[i].grouping.set_member(fam, slot, member);
            }
            Ok::<_, DawError>((p.tracks[i].guid.clone(), p.tracks[i].grouping.clone()))
        })??;
        // Standalone emits from its setters, the way it does for every
        // other field — so a client hears the same event whichever
        // backend it is attached to.
        let (guid_of_track, grouping) = changed;
        publish_track_events(
            self,
            &guid,
            vec![TrackEvent::GroupingChanged {
                guid: guid_of_track,
                grouping,
            }],
        );
        Ok(())
    }

    fn set_group_flags(
        &self,
        project: ProjectContext,
        track: TrackRef,
        change: GroupFlagChange,
    ) -> DawResult<()> {
        check_group_slot(change.slot)?;
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let changed = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            p.tracks[i]
                .grouping
                .set_role(change.family, change.slot, change.role);
            Ok::<_, DawError>((p.tracks[i].guid.clone(), p.tracks[i].grouping.clone()))
        })??;
        // Standalone emits from its setters, the way it does for every
        // other field — so a client hears the same event whichever
        // backend it is attached to.
        let (guid_of_track, grouping) = changed;
        publish_track_events(
            self,
            &guid,
            vec![TrackEvent::GroupingChanged {
                guid: guid_of_track,
                grouping,
            }],
        );
        Ok(())
    }

    fn set_group_modifier(
        &self,
        project: ProjectContext,
        track: TrackRef,
        change: GroupModifierChange,
    ) -> DawResult<()> {
        check_group_slot(change.slot)?;
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let changed = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            p.tracks[i]
                .grouping
                .set_modifier(change.modifier, change.slot, change.enabled);
            Ok::<_, DawError>((p.tracks[i].guid.clone(), p.tracks[i].grouping.clone()))
        })??;
        // Standalone emits from its setters, the way it does for every
        // other field — so a client hears the same event whichever
        // backend it is attached to.
        let (guid_of_track, grouping) = changed;
        publish_track_events(
            self,
            &guid,
            vec![TrackEvent::GroupingChanged {
                guid: guid_of_track,
                grouping,
            }],
        );
        Ok(())
    }

    fn group_flags(&self, project: ProjectContext, track: TrackRef) -> DawResult<TrackGrouping> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.read_project(&guid, |p| {
            find_track_index(&p.tracks, &track)
                .map(|i| p.tracks[i].grouping.clone())
                .ok_or_else(not_found_track)
        })
        .ok_or_else(not_found_proj)?
    }

    fn set_selected(
        &self,
        project: ProjectContext,
        track: TrackRef,
        selected: bool,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let event = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.tracks[i].selected = selected;
            Ok::<_, DawError>(TrackEvent::SelectionChanged {
                guid: track_guid,
                selected,
            })
        })??;
        publish_track_events(self, &guid, vec![event]);
        Ok(())
    }

    fn select_exclusive(&self, project: ProjectContext, track: TrackRef) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let target = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let target_guid = p.tracks[target].guid.clone();
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                let next = t.guid == target_guid;
                if t.selected != next {
                    t.selected = next;
                    events.push(TrackEvent::SelectionChanged {
                        guid: t.guid.clone(),
                        selected: next,
                    });
                }
            }
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn clear_selection(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                if t.selected {
                    t.selected = false;
                    events.push(TrackEvent::SelectionChanged {
                        guid: t.guid.clone(),
                        selected: false,
                    });
                }
            }
            events
        })?;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn mute_all(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                if !t.muted {
                    t.muted = true;
                    events.push(TrackEvent::MuteChanged {
                        guid: t.guid.clone(),
                        muted: true,
                    });
                }
            }
            events
        })?;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn unmute_all(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let mut events = Vec::new();
            for t in p.tracks.iter_mut() {
                if t.muted {
                    t.muted = false;
                    events.push(TrackEvent::MuteChanged {
                        guid: t.guid.clone(),
                        muted: false,
                    });
                }
            }
            events
        })?;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn add(&self, project: ProjectContext, name: &str, at_index: Option<u32>) -> DawResult<String> {
        insert_track(self, &project, Uuid::new_v4().to_string(), name, at_index)
    }

    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        name: &str,
        at_index: Option<u32>,
    ) -> DawResult<String> {
        crate::check_new_guid("Track", guid)?;
        insert_track(self, &project, guid.to_string(), name, at_index)
    }

    fn move_to(&self, project: ProjectContext, track: TrackRef, index: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let from = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let last = p.tracks.len() - 1;
            if index as usize > last {
                return Err(DawError::out_of_range(
                    index,
                    last as u32,
                    "Tracks::move_to",
                ));
            }
            let before = p.tracks.clone();
            let moving = p.tracks.remove(from);
            p.tracks.insert(index as usize, moving);
            reconcile_track_structure(&mut p.tracks);
            Ok::<_, DawError>(moved_events(&before, &p.tracks))
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn remove(&self, project: ProjectContext, track: TrackRef) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let before = p.tracks.clone();
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let removed = p.tracks[i].guid.clone();
            p.tracks.remove(i);
            p.track_ext.remove(&removed);
            p.track_ext_state
                .retain(|(track_guid, _, _), _| track_guid != &removed);
            p.sends.remove(&removed);
            p.receives.remove(&removed);
            p.hw_outputs.remove(&removed);
            for sends in p.sends.values_mut() {
                sends.retain(|route| route.dest_track_guid.as_deref() != Some(removed.as_str()));
                for (idx, route) in sends.iter_mut().enumerate() {
                    route.index = idx as u32;
                }
            }
            for receives in p.receives.values_mut() {
                // The far end of a receive is its `dest_track_guid` —
                // the track that was feeding this one. See
                // `routing_sync::make_receive_mirror`.
                receives.retain(|route| route.dest_track_guid.as_deref() != Some(removed.as_str()));
                for (idx, route) in receives.iter_mut().enumerate() {
                    route.index = idx as u32;
                }
            }
            reconcile_track_structure(&mut p.tracks);
            let mut events = vec![TrackEvent::Removed(removed)];
            events.extend(moved_events(&before, &p.tracks));
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn remove_all(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let events = p
                .tracks
                .iter()
                .map(|track| TrackEvent::Removed(track.guid.clone()))
                .collect::<Vec<_>>();
            p.tracks.clear();
            p.track_ext.clear();
            p.track_ext_state.clear();
            p.sends.clear();
            p.receives.clear();
            p.hw_outputs.clear();
            events
        })?;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn rename(&self, project: ProjectContext, track: TrackRef, name: &str) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let event = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.tracks[i].name = name.to_string();
            Ok::<_, DawError>(TrackEvent::Renamed {
                guid: track_guid,
                name: name.to_string(),
            })
        })??;
        publish_track_events(self, &guid, vec![event]);
        Ok(())
    }

    fn set_color(&self, project: ProjectContext, track: TrackRef, color: u32) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let event = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            let color = crate::color_from_service(color);
            p.tracks[i].color = color;
            Ok::<_, DawError>(TrackEvent::ColorChanged {
                guid: track_guid,
                color,
            })
        })??;
        publish_track_events(self, &guid, vec![event]);
        Ok(())
    }

    fn set_folder_depth(
        &self,
        project: ProjectContext,
        track: TrackRef,
        folder_depth: i32,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let before = p.tracks.clone();
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.tracks[i].folder_depth = folder_depth;
            reconcile_track_structure(&mut p.tracks);
            // The depth change leads, because a client that reordered
            // first and re-parented second would redraw the tree twice
            // and be wrong in between.
            let mut events = vec![TrackEvent::FolderDepthChanged {
                guid: track_guid,
                folder_depth,
            }];
            events.extend(moved_events(&before, &p.tracks));
            Ok::<_, DawError>(events)
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_num_channels(
        &self,
        project: ProjectContext,
        track: TrackRef,
        num_channels: u32,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        // REAPER caps tracks at 128 channels. Allow any count 1..=128 —
        // standalone doesn't enforce stereo pairing.
        let n = num_channels.clamp(1, 128);
        self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.track_ext
                .entry(track_guid)
                .or_insert_with(TrackExt::default)
                .num_channels = n;
            Ok::<(), DawError>(())
        })?
    }

    fn set_record_input(
        &self,
        project: ProjectContext,
        track: TrackRef,
        input: RecordInput,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            // Mirrored onto the stored `Track` as well as the side map:
            // a strip reads `Track::record_input` from the bulk read, and
            // the two must not disagree.
            p.tracks[i].record_input = input;
            p.track_ext
                .entry(track_guid.clone())
                .or_insert_with(TrackExt::default)
                .record_input = input;
            Ok::<_, DawError>(vec![TrackEvent::RecordInputChanged {
                guid: track_guid,
                input,
            }])
        })??;
        // Applying a patch list sets every source track's input at once,
        // and a second client that is not told keeps showing where a
        // take used to come from.
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn reorder_selected(
        &self,
        project: ProjectContext,
        index: u32,
        behavior: ReorderTracksBehavior,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let before = p.tracks.clone();
            if !p.tracks.iter().any(|track| track.selected) {
                return Ok::<_, DawError>(Vec::new());
            }

            let insert_at = selected_reorder_insert_index(&p.tracks, index);
            let mut selected = Vec::new();
            let mut remaining = Vec::with_capacity(p.tracks.len());
            for track in p.tracks.drain(..) {
                if track.selected {
                    selected.push(track);
                } else {
                    remaining.push(track);
                }
            }

            let insert_at = insert_at.min(remaining.len());
            if matches!(
                behavior,
                ReorderTracksBehavior::MakeChildOfPreviousTrack
                    | ReorderTracksBehavior::ExtendFolder
            ) && insert_at > 0
            {
                remaining[insert_at - 1].folder_depth =
                    remaining[insert_at - 1].folder_depth.max(1);
            }
            remaining.splice(insert_at..insert_at, selected);
            p.tracks = remaining;
            reconcile_track_structure(&mut p.tracks);
            Ok::<_, DawError>(moved_events(&before, &p.tracks))
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_visibility(
        &self,
        project: ProjectContext,
        track: TrackRef,
        visible_in_tcp: bool,
        visible_in_mixer: bool,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.tracks[i].visible_in_tcp = visible_in_tcp;
            p.tracks[i].visible_in_mixer = visible_in_mixer;
            Ok::<_, DawError>(vec![
                TrackEvent::TcpVisibilityChanged {
                    guid: track_guid.clone(),
                    visible: visible_in_tcp,
                },
                TrackEvent::MixerVisibilityChanged {
                    guid: track_guid,
                    visible: visible_in_mixer,
                },
            ])
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn set_tcp_height(
        &self,
        project: ProjectContext,
        track: TrackRef,
        height_pixels: u32,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            p.track_ext
                .entry(track_guid.clone())
                .or_insert_with(TrackExt::default)
                .tcp_height_pixels = height_pixels;
            // The track's own `height` is what a read and a save see; 0
            // is the default height, which it keeps as `None`.
            p.tracks[i].height = (height_pixels > 0).then_some(height_pixels);
            Ok::<_, DawError>(vec![TrackEvent::HeightChanged {
                guid: track_guid,
                height: Some(height_pixels),
            }])
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    // ── Fixed lanes ─────────────────────────────────────────────────

    fn set_lane_count(
        &self,
        project: ProjectContext,
        track: TrackRef,
        count: u32,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let track_guid = p.tracks[i].guid.clone();
            let t = &mut p.tracks[i];
            let had = t.lane_count;
            t.lane_count = count;
            t.lane_names.truncate(count as usize);
            t.lane_play_mask &= lane_bits(count);
            if had == 0 && count > 0 {
                // Lanes just switched on: lane 0 plays, as in REAPER.
                t.lane_play_mask = 1;
            }
            let ext = p.track_ext.entry(track_guid.clone()).or_default();
            if count == 0 {
                ext.comping = LaneComping::default();
            } else {
                let keep = |l: Option<u32>| l.filter(|&l| l < count);
                ext.comping.record_lane = keep(ext.comping.record_lane);
                ext.comping.comp_lane = keep(ext.comping.comp_lane);
                ext.comping.last_comp_lane = keep(ext.comping.last_comp_lane);
                ext.comping
                    .areas
                    .retain(|a| a.comp_lane < count && a.source_lane < count);
            }
            // Items past the new end lose their lane; every item loses it
            // when lanes go off.
            for item_guid in p
                .items_by_track
                .get(&track_guid)
                .cloned()
                .unwrap_or_default()
            {
                if let Some(entry) = p.items.get_mut(&item_guid)
                    && entry.item.fixed_lane.is_some_and(|l| l >= count)
                {
                    entry.item.fixed_lane = None;
                }
            }
            Ok::<(), DawError>(())
        })?
    }

    fn set_lane_play_mask(
        &self,
        project: ProjectContext,
        track: TrackRef,
        mask: u64,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            p.tracks[i].lane_play_mask = mask & lane_bits(p.tracks[i].lane_count);
            Ok::<_, DawError>(vec![lanes_event(&p.tracks[i])])
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    fn ext_state(&self, project: ProjectContext, track: TrackRef) -> Vec<daw_proto::TrackExtEntry> {
        let Some(guid) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, &track)?;
            let own = &p.tracks[i].guid;
            let mut entries: Vec<daw_proto::TrackExtEntry> = p
                .track_ext_state
                .iter()
                .filter(|((track_guid, section, _), _)| track_guid == own && section.is_empty())
                .map(|((_, _, key), value)| daw_proto::TrackExtEntry {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect();
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            Some(entries)
        })
        .ok()
        .flatten()
        .unwrap_or_default()
    }

    fn set_ext_state(
        &self,
        project: ProjectContext,
        track: TrackRef,
        key: &str,
        value: &str,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let at = (p.tracks[i].guid.clone(), String::new(), key.to_owned());
            if value.is_empty() {
                p.track_ext_state.remove(&at);
            } else {
                p.track_ext_state.insert(at, value.to_owned());
            }
            Ok::<(), DawError>(())
        })?
    }

    fn set_lane_name(
        &self,
        project: ProjectContext,
        track: TrackRef,
        lane: u32,
        name: &str,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let events = self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let t = &mut p.tracks[i];
            check_lane(lane, t.lane_count)?;
            // Fill the gap up to `lane` with REAPER's default names, so
            // the list stays positional.
            while t.lane_names.len() <= lane as usize {
                t.lane_names.push((t.lane_names.len() + 1).to_string());
            }
            t.lane_names[lane as usize] = name.to_string();
            Ok::<_, DawError>(vec![lanes_event(t)])
        })??;
        publish_track_events(self, &guid, events);
        Ok(())
    }

    // ── Comping ─────────────────────────────────────────────────────

    fn comping(&self, project: ProjectContext, track: TrackRef) -> DawResult<LaneComping> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            Ok(p.track_ext
                .get(&p.tracks[i].guid)
                .map(|e| e.comping.clone())
                .unwrap_or_default())
        })?
    }

    fn set_comp_areas(
        &self,
        project: ProjectContext,
        track: TrackRef,
        areas: Vec<CompArea>,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let lane_count = p.tracks[i].lane_count;
            for a in &areas {
                check_lane(a.comp_lane, lane_count)?;
                check_lane(a.source_lane, lane_count)?;
            }
            let track_guid = p.tracks[i].guid.clone();
            p.track_ext.entry(track_guid).or_default().comping.areas = areas;
            Ok::<(), DawError>(())
        })?
    }

    fn comps(&self, project: ProjectContext, track: TrackRef) -> DawResult<Vec<Comp>> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            let t = &p.tracks[i];
            Ok(p.track_ext
                .get(&t.guid)
                .map(|e| e.comping.comps(&t.lane_names))
                .unwrap_or_default())
        })?
    }

    fn create_comp(&self, project: ProjectContext, track: TrackRef, name: &str) -> DawResult<u32> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let lane = self.with_project(&guid, |p| {
            find_track_index(&p.tracks, &track)
                .map(|i| p.tracks[i].lane_count)
                .ok_or_else(not_found_track)
        })??;
        self.set_lane_count(project.clone(), track.clone(), lane + 1)?;
        self.set_lane_name(project.clone(), track.clone(), lane, name)?;
        self.set_active_comp(project, track, Some(lane))?;
        Ok(lane)
    }

    fn set_active_comp(
        &self,
        project: ProjectContext,
        track: TrackRef,
        lane: Option<u32>,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            let i = find_track_index(&p.tracks, &track).ok_or_else(not_found_track)?;
            if let Some(lane) = lane {
                check_lane(lane, p.tracks[i].lane_count)?;
            }
            let track_guid = p.tracks[i].guid.clone();
            let comping = &mut p.track_ext.entry(track_guid).or_default().comping;
            if comping.comp_lane != lane {
                comping.last_comp_lane = comping.comp_lane;
                comping.comp_lane = lane;
            }
            Ok::<(), DawError>(())
        })?
    }
}

/// The bits of a play mask that name a lane the track has.
fn lane_bits(lane_count: u32) -> u64 {
    if lane_count >= 64 {
        u64::MAX
    } else {
        (1u64 << lane_count) - 1
    }
}

/// Lane `lane` exists on a track with `lane_count` lanes.
pub(crate) fn check_lane(lane: u32, lane_count: u32) -> DawResult<()> {
    if lane < lane_count {
        Ok(())
    } else {
        Err(DawError::out_of_range(lane, lane_count, "fixed lane"))
    }
}

// ── Inherent helpers for reading extended track state ────────────────
//
// These don't go through the `Tracks` proto trait (it has no
// `get_num_channels` / `get_record_input` methods today) — routing,
// mixer code, and tests can call them directly on `Standalone`.

impl Standalone {
    /// Channel count for a track. Returns 2 (stereo default) if the
    /// track exists but no override has been set; `None` if the track
    /// can't be resolved.
    pub fn track_num_channels(&self, project: &ProjectContext, track: &TrackRef) -> Option<u32> {
        let guid = resolve_project(self, project)?;
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, track)?;
            let g = &p.tracks[i].guid;
            Some(p.track_ext.get(g).map(|e| e.num_channels).unwrap_or(2))
        })
        .ok()
        .flatten()
    }

    /// Record input for a track. Returns `RecordInput::None` if unset;
    /// `None` if the track can't be resolved.
    pub fn track_record_input(
        &self,
        project: &ProjectContext,
        track: &TrackRef,
    ) -> Option<RecordInput> {
        let guid = resolve_project(self, project)?;
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, track)?;
            let g = &p.tracks[i].guid;
            Some(
                p.track_ext
                    .get(g)
                    .map(|e| e.record_input)
                    .unwrap_or(RecordInput::None),
            )
        })
        .ok()
        .flatten()
    }

    /// TCP height override for a track. Returns `0` for host/default
    /// height, or `None` if the track can't be resolved.
    pub fn track_tcp_height(&self, project: &ProjectContext, track: &TrackRef) -> Option<u32> {
        let guid = resolve_project(self, project)?;
        self.with_project(&guid, |p| {
            let i = find_track_index(&p.tracks, track)?;
            let g = &p.tracks[i].guid;
            Some(p.track_ext.get(g).map(|e| e.tcp_height_pixels).unwrap_or(0))
        })
        .ok()
        .flatten()
    }
}
