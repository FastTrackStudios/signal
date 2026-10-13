//! `impl Routing for Standalone` — full implementation.
//!
//! Storage lives in `ProjectState`:
//! - `sends: HashMap<src_guid, Vec<TrackRoute>>` — track→track sends
//! - `receives: HashMap<dest_guid, Vec<TrackRoute>>` — auto-mirrored
//!   from sends so the destination side reflects the same edge
//! - `hw_outputs: HashMap<src_guid, Vec<TrackRoute>>` — hardware outs
//!
//! Master send (REAPER's `B_MAINSEND`) lives on `TrackExt`.
//!
//! Channel mapping for sends defaults to "all source channels →
//! corresponding dest channels"; callers can refine via
//! `set_source_channels` / `set_dest_channels`. Source spans clamp to
//! the source track's actual `num_channels` (see [`TrackExt`]).

use daw_proto::event_bus::DawEvent;
use daw_proto::routing::RoutingEvent;
use daw_proto::{
    ChannelMapping, DawError, DawResult, ProjectContext, RouteLocation, RouteRef, RouteType,
    Routing, SendMode, TrackRef, TrackRoute,
};

use crate::sync::{ProjectState, Standalone, TrackExt};

/// Like [`mutate_route`] but publishes the [`RoutingEvent`] the closure
/// returns (given the resolved `source_track_guid`, `route_type`, and
/// `route_index`) onto the bus.
fn mutate_route_evt(
    daw: &Standalone,
    project: &ProjectContext,
    location: &RouteLocation,
    f: impl FnOnce(&str, &str, RouteType, u32, &mut TrackRoute) -> RoutingEvent,
) -> DawResult<()> {
    let pg = resolve_project(daw, project).ok_or_else(not_found_proj)?;
    let event = daw.with_project_mut(&pg, |p| {
        let track_guid = resolve_track_guid(p, &location.track).ok_or_else(not_found_track)?;
        let v = route_kind_vec_mut(p, &track_guid, location.route_type);
        let i = resolve_route_idx(v, &location.route).ok_or_else(not_found_route)?;
        let event = f(&pg, &track_guid, location.route_type, i as u32, &mut v[i]);
        let mirror = (v[i].route_type == RouteType::Send).then(|| v[i].clone());
        if let Some(send) = mirror {
            sync_receive_mirror(p, &send);
        }
        Ok::<RoutingEvent, DawError>(event)
    })??;
    daw.bus_events.publish(DawEvent::Routing(event));
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

fn not_found_proj() -> DawError {
    DawError::not_found("Project", "context")
}

fn not_found_route() -> DawError {
    DawError::not_found("Route", "")
}

fn not_found_track() -> DawError {
    DawError::not_found("Track", "")
}

fn resolve_track_guid(p: &ProjectState, track: &TrackRef) -> Option<String> {
    match track {
        TrackRef::Guid(g) => p
            .tracks
            .iter()
            .find(|t| t.guid == *g)
            .map(|t| t.guid.clone()),
        TrackRef::Index(i) => p.tracks.get(*i as usize).map(|t| t.guid.clone()),
        TrackRef::Master => Some("master".to_string()),
    }
}

fn track_num_channels(p: &ProjectState, guid: &str) -> u32 {
    p.track_ext.get(guid).map(|e| e.num_channels).unwrap_or(2)
}

fn track_name_lookup(p: &ProjectState, guid: &str) -> Option<String> {
    p.tracks
        .iter()
        .find(|t| t.guid == guid)
        .map(|t| t.name.clone())
}

fn find_route_by_ref(routes: &[TrackRoute], p: &ProjectState, rref: &RouteRef) -> Option<usize> {
    match rref {
        RouteRef::Index(i) => {
            let u = *i as usize;
            (u < routes.len()).then_some(u)
        }
        RouteRef::ByDestination(track) => {
            let dest_guid = resolve_track_guid(p, track)?;
            routes
                .iter()
                .position(|r| r.dest_track_guid.as_deref() == Some(dest_guid.as_str()))
        }
    }
}

/// Synthesize the receive mirror on the destination side from a send.
///
/// The ends are SWAPPED, which is the whole subtlety: a route is stored
/// in a list belonging to one track, and `dest_track_*` is the OTHER
/// end of it. For a send that is where the signal goes; for a receive
/// it is where the signal comes from. REAPER fills it from
/// `TrackRoute::partner()` and means exactly that, so a mirror that
/// copied both ends unchanged would name the track doing the looking —
/// and every consumer listing receives would print its own name back at
/// it. (Reported from the session panel; see daw#30.)
fn make_receive_mirror(send: &TrackRoute, from_name: Option<String>) -> Option<TrackRoute> {
    let dest_guid = send.dest_track_guid.clone()?;
    Some(TrackRoute {
        index: 0,
        route_type: RouteType::Receive,
        // The receiving track owns this route…
        source_track_guid: dest_guid,
        // …and the far end is the track feeding it.
        dest_track_guid: Some(send.source_track_guid.clone()),
        dest_track_name: from_name,
        hw_output_index: None,
        hw_output_name: None,
        volume: send.volume,
        pan: send.pan,
        muted: send.muted,
        mono: send.mono,
        phase_inverted: send.phase_inverted,
        send_mode: send.send_mode,
        automation_mode: send.automation_mode,
        source_channels: send.source_channels.clone(),
        dest_channels: send.dest_channels.clone(),
        midi_channel_mapping: send.midi_channel_mapping.clone(),
    })
}

fn renumber(routes: &mut [TrackRoute]) {
    for (i, r) in routes.iter_mut().enumerate() {
        r.index = i as u32;
    }
}

/// Sync the receive-side mirror for a given (source → dest) edge.
/// Replaces any existing receive entry with the same source on the
/// dest's list; idempotent for unchanged sends.
fn sync_receive_mirror(p: &mut ProjectState, send: &TrackRoute) {
    let Some(dest_guid) = send.dest_track_guid.as_ref().cloned() else {
        return;
    };
    // The name of the track doing the sending, looked up while the
    // state is in hand: a receive names its partner, and the send it
    // was built from carries only the partner's guid.
    let from_name = p
        .tracks
        .iter()
        .find(|track| track.guid == send.source_track_guid)
        .map(|track| track.name.clone());
    let receives = p.receives.entry(dest_guid).or_default();
    // Drop any existing receive from the same source — which is the
    // far end of a receive, and therefore its `dest_track_guid`.
    receives.retain(|r| r.dest_track_guid.as_deref() != Some(send.source_track_guid.as_str()));
    if let Some(mirror) = make_receive_mirror(send, from_name) {
        receives.push(mirror);
        renumber(receives);
    }
}

fn drop_receive_mirror(p: &mut ProjectState, src_guid: &str, dest_guid: &str) {
    if let Some(receives) = p.receives.get_mut(dest_guid) {
        receives.retain(|r| r.dest_track_guid.as_deref() != Some(src_guid));
        renumber(receives);
    }
}

fn route_kind_vec_mut<'a>(
    p: &'a mut ProjectState,
    track_guid: &str,
    rt: RouteType,
) -> &'a mut Vec<TrackRoute> {
    match rt {
        RouteType::Send => p.sends.entry(track_guid.to_string()).or_default(),
        RouteType::Receive => p.receives.entry(track_guid.to_string()).or_default(),
        RouteType::HardwareOutput => p.hw_outputs.entry(track_guid.to_string()).or_default(),
    }
}

fn route_kind_vec<'a>(
    p: &'a ProjectState,
    track_guid: &str,
    rt: RouteType,
) -> Option<&'a Vec<TrackRoute>> {
    match rt {
        RouteType::Send => p.sends.get(track_guid),
        RouteType::Receive => p.receives.get(track_guid),
        RouteType::HardwareOutput => p.hw_outputs.get(track_guid),
    }
}

impl Routing for Standalone {
    fn sends(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some(pg) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&pg, |p| {
            let Some(g) = resolve_track_guid(p, &track) else {
                return Vec::new();
            };
            p.sends.get(&g).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn receives(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some(pg) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&pg, |p| {
            let Some(g) = resolve_track_guid(p, &track) else {
                return Vec::new();
            };
            p.receives.get(&g).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn hardware_outputs(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some(pg) = resolve_project(self, &project) else {
            return Vec::new();
        };
        self.with_project(&pg, |p| {
            let Some(g) = resolve_track_guid(p, &track) else {
                return Vec::new();
            };
            p.hw_outputs.get(&g).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn get_route(&self, project: ProjectContext, location: RouteLocation) -> Option<TrackRoute> {
        let pg = resolve_project(self, &project)?;
        self.with_project(&pg, |p| {
            let g = resolve_track_guid(p, &location.track)?;
            let v = route_kind_vec(p, &g, location.route_type)?;
            let i = find_route_by_ref(v, p, &location.route)?;
            Some(v[i].clone())
        })
        .ok()
        .flatten()
    }

    fn send_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        Routing::sends(self, project, track).len() as u32
    }

    fn receive_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        Routing::receives(self, project, track).len() as u32
    }

    fn add_send(&self, project: ProjectContext, source: TrackRef, dest: TrackRef) -> Option<u32> {
        let pg = resolve_project(self, &project)?;
        let result = self
            .with_project_mut(&pg, |p| {
                let src_guid = resolve_track_guid(p, &source)?;
                let dest_guid = resolve_track_guid(p, &dest)?;
                if src_guid == dest_guid {
                    // No self-routing (would feedback-loop).
                    return None;
                }
                let dest_name = track_name_lookup(p, &dest_guid);
                let src_channels = track_num_channels(p, &src_guid);
                let dest_channels = track_num_channels(p, &dest_guid);

                let route = TrackRoute {
                    index: 0,
                    route_type: RouteType::Send,
                    source_track_guid: src_guid.clone(),
                    dest_track_guid: Some(dest_guid.clone()),
                    dest_track_name: dest_name,
                    hw_output_index: None,
                    hw_output_name: None,
                    volume: 1.0,
                    pan: 0.0,
                    muted: false,
                    mono: false,
                    phase_inverted: false,
                    send_mode: SendMode::PostFader,
                    automation_mode: Default::default(),
                    source_channels: ChannelMapping {
                        start_channel: 0,
                        num_channels: src_channels,
                    },
                    dest_channels: ChannelMapping {
                        start_channel: 0,
                        num_channels: dest_channels,
                    },
                    midi_channel_mapping: None,
                };
                let (new_index, created) = {
                    let sends = p.sends.entry(src_guid).or_default();
                    let i = sends.len() as u32;
                    sends.push(route.clone());
                    renumber(sends);
                    (i, sends[i as usize].clone())
                };
                sync_receive_mirror(p, &route);
                Some((new_index, created))
            })
            .ok()
            .flatten();
        let (new_index, created) = result?;
        self.bus_events
            .publish(DawEvent::Routing(RoutingEvent::RouteCreated {
                project_guid: pg,
                source_track_guid: created.source_track_guid.clone(),
                route: created,
            }));
        Some(new_index)
    }

    fn add_hardware_output(
        &self,
        project: ProjectContext,
        track: TrackRef,
        hw_output: u32,
    ) -> Option<u32> {
        let pg = resolve_project(self, &project)?;
        self.with_project_mut(&pg, |p| {
            let src_guid = resolve_track_guid(p, &track)?;
            let src_channels = track_num_channels(p, &src_guid);
            let route = TrackRoute {
                index: 0,
                route_type: RouteType::HardwareOutput,
                source_track_guid: src_guid.clone(),
                dest_track_guid: None,
                dest_track_name: None,
                hw_output_index: Some(hw_output),
                hw_output_name: Some(format!("HW {}", hw_output)),
                volume: 1.0,
                pan: 0.0,
                muted: false,
                mono: false,
                phase_inverted: false,
                send_mode: SendMode::PostFader,
                automation_mode: Default::default(),
                source_channels: ChannelMapping {
                    start_channel: 0,
                    num_channels: src_channels,
                },
                dest_channels: ChannelMapping {
                    start_channel: hw_output,
                    num_channels: src_channels,
                },
                midi_channel_mapping: None,
            };
            let outs = p.hw_outputs.entry(src_guid).or_default();
            let i = outs.len() as u32;
            outs.push(route);
            renumber(outs);
            Some(i)
        })
        .ok()
        .flatten()
    }

    fn remove_route(&self, project: ProjectContext, location: RouteLocation) -> DawResult<()> {
        let pg = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let (src_guid, route_type, route_index) = self.with_project_mut(&pg, |p| {
            let track_guid = resolve_track_guid(p, &location.track).ok_or_else(not_found_track)?;
            let v = route_kind_vec_mut(p, &track_guid, location.route_type);
            let i = {
                // Inline resolution that doesn't need an immutable
                // borrow of `p`.
                match &location.route {
                    RouteRef::Index(idx) => {
                        let u = *idx as usize;
                        if u < v.len() { Some(u) } else { None }
                    }
                    RouteRef::ByDestination(_) => {
                        // Resolving by destination needs `p.tracks`,
                        // which would re-borrow `p`. Snapshot the
                        // dest guid up-front from the v we already
                        // have to keep borrows clean.
                        v.iter().position(|r| {
                            matches!(&location.route, RouteRef::ByDestination(t)
                            if matches_track(r, t))
                        })
                    }
                }
            }
            .ok_or_else(not_found_route)?;

            let removed = v.remove(i);
            renumber(v);

            // Drop mirror if this was a send.
            if removed.route_type == RouteType::Send {
                if let Some(dg) = removed.dest_track_guid.as_deref() {
                    drop_receive_mirror(p, &removed.source_track_guid, dg);
                }
            } else if removed.route_type == RouteType::Receive {
                // Removing a receive on the dest implicitly removes
                // the matching send on the source. The source is the
                // receive's FAR end — `dest_track_guid`, the same field
                // a send puts its destination in — and the near end is
                // the track whose list this was.
                let src = removed.dest_track_guid.clone().unwrap_or_default();
                let dest = track_guid.clone();
                if let Some(sends) = p.sends.get_mut(&src) {
                    sends.retain(|s| s.dest_track_guid.as_deref() != Some(dest.as_str()));
                    renumber(sends);
                }
            }
            Ok::<(String, RouteType, u32), DawError>((
                removed.source_track_guid,
                removed.route_type,
                i as u32,
            ))
        })??;
        self.bus_events
            .publish(DawEvent::Routing(RoutingEvent::RouteDeleted {
                project_guid: pg,
                source_track_guid: src_guid,
                route_type,
                route_index,
            }));
        Ok(())
    }

    fn set_volume(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        volume: f64,
    ) -> DawResult<()> {
        mutate_route_evt(self, &project, &location, |pg, src, rt, idx, r| {
            r.volume = volume.max(0.0);
            RoutingEvent::VolumeChanged {
                project_guid: pg.to_string(),
                source_track_guid: src.to_string(),
                route_type: rt,
                route_index: idx,
                volume: r.volume,
            }
        })
    }

    fn set_pan(&self, project: ProjectContext, location: RouteLocation, pan: f64) -> DawResult<()> {
        mutate_route_evt(self, &project, &location, |pg, src, rt, idx, r| {
            r.pan = pan.clamp(-1.0, 1.0);
            RoutingEvent::PanChanged {
                project_guid: pg.to_string(),
                source_track_guid: src.to_string(),
                route_type: rt,
                route_index: idx,
                pan: r.pan,
            }
        })
    }

    fn set_muted(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        muted: bool,
    ) -> DawResult<()> {
        mutate_route_evt(self, &project, &location, |pg, src, rt, idx, r| {
            r.muted = muted;
            RoutingEvent::MuteChanged {
                project_guid: pg.to_string(),
                source_track_guid: src.to_string(),
                route_type: rt,
                route_index: idx,
                muted,
            }
        })
    }

    fn set_mono(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        mono: bool,
    ) -> DawResult<()> {
        mutate_route(self, &project, &location, |r| r.mono = mono)
    }

    fn set_phase(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        inverted: bool,
    ) -> DawResult<()> {
        mutate_route(self, &project, &location, |r| r.phase_inverted = inverted)
    }

    fn is_muted(&self, project: ProjectContext, location: RouteLocation) -> bool {
        Routing::get_route(self, project, location)
            .map(|r| r.muted)
            .unwrap_or(false)
    }

    fn set_send_mode(
        &self,
        project: ProjectContext,
        track: TrackRef,
        route: RouteRef,
        mode: SendMode,
    ) -> DawResult<()> {
        let location = RouteLocation {
            track,
            route_type: RouteType::Send,
            route,
        };
        mutate_route(self, &project, &location, |r| r.send_mode = mode)
    }

    fn set_source_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start: u32,
        num: u32,
    ) -> DawResult<()> {
        mutate_route(self, &project, &location, |r| {
            r.source_channels = ChannelMapping {
                start_channel: start,
                num_channels: num.clamp(1, 128),
            }
        })
    }

    fn set_dest_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start: u32,
        num: u32,
    ) -> DawResult<()> {
        mutate_route(self, &project, &location, |r| {
            r.dest_channels = ChannelMapping {
                start_channel: start,
                num_channels: num.clamp(1, 128),
            }
        })
    }

    fn parent_send_enabled(&self, project: ProjectContext, track: TrackRef) -> bool {
        let Some(pg) = resolve_project(self, &project) else {
            return false;
        };
        self.with_project(&pg, |p| {
            let Some(g) = resolve_track_guid(p, &track) else {
                return false;
            };
            p.track_ext
                .get(&g)
                .map(|e| e.parent_send_enabled)
                .unwrap_or(true)
        })
        .unwrap_or(true)
    }

    fn set_parent_send_enabled(
        &self,
        project: ProjectContext,
        track: TrackRef,
        enabled: bool,
    ) -> DawResult<()> {
        let pg = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let guid = self.with_project_mut(&pg, |p| {
            let g = resolve_track_guid(p, &track).ok_or_else(not_found_track)?;
            // Mirrored onto the stored `Track` as well as the side map: a
            // strip reads `Track::parent_send` from the bulk read, and the
            // two must not disagree.
            if let Some(t) = p.tracks.iter_mut().find(|t| t.guid == g) {
                t.parent_send = enabled;
            }
            p.track_ext
                .entry(g.clone())
                .or_insert_with(TrackExt::default)
                .parent_send_enabled = enabled;
            Ok::<String, DawError>(g)
        })??;
        // The IO indicator on every strip shows this, so it is an event
        // rather than something a mixer has to ask about again.
        let event = daw_proto::track::TrackStreamEvent {
            project_guid: pg,
            event: daw_proto::track::TrackEvent::ParentSendChanged { guid, enabled },
        };
        self.bus_events
            .publish(daw_proto::event_bus::DawEvent::Track(event.clone()));
        self.track_events.publish(event);
        Ok(())
    }
}

fn matches_track(route: &TrackRoute, tref: &TrackRef) -> bool {
    match tref {
        TrackRef::Guid(g) => route.dest_track_guid.as_deref() == Some(g.as_str()),
        TrackRef::Index(_) => false, // can't match without ProjectState lookup
        TrackRef::Master => route.dest_track_guid.is_none(),
    }
}

/// Mutate a route via `RouteLocation`. Re-syncs the receive mirror if
/// the route is a send so its dest sees the same change.
fn mutate_route(
    daw: &Standalone,
    project: &ProjectContext,
    location: &RouteLocation,
    f: impl FnOnce(&mut TrackRoute),
) -> DawResult<()> {
    let pg = resolve_project(daw, project).ok_or_else(not_found_proj)?;
    daw.with_project_mut(&pg, |p| {
        let track_guid = resolve_track_guid(p, &location.track).ok_or_else(not_found_track)?;
        let snapshot_for_mirror = {
            let v = route_kind_vec_mut(p, &track_guid, location.route_type);
            let i = resolve_route_idx(v, &location.route).ok_or_else(not_found_route)?;
            f(&mut v[i]);
            (v[i].route_type == RouteType::Send).then(|| v[i].clone())
        };
        if let Some(send) = snapshot_for_mirror {
            sync_receive_mirror(p, &send);
        }
        Ok::<(), DawError>(())
    })?
}

/// Index resolution that doesn't need an immutable borrow on `p`
/// (matches against an already-mut-borrowed route vec).
fn resolve_route_idx(routes: &[TrackRoute], rref: &RouteRef) -> Option<usize> {
    match rref {
        RouteRef::Index(i) => {
            let u = *i as usize;
            (u < routes.len()).then_some(u)
        }
        RouteRef::ByDestination(t) => routes.iter().position(|r| matches_track(r, t)),
    }
}
