//! `impl Routing for Reaper` — sends, receives, hardware outputs.
//!
//! Sync trait + REAPER C API. Mount via `daw_proto::routing::serve(Reaper)`.
//! Each call resolves a `(ProjectContext, TrackRef)` pair to a concrete
//! `reaper_high::Track`, then dispatches on `RouteType` to the right
//! REAPER send/receive/hardware-output family of functions.

use daw_proto::{
    AutomationMode, ChannelMapping, DawError, DawResult, MidiChannelMapping,
    MidiDestinationChannel, MidiSourceChannel, ProjectContext, RouteLocation, RouteRef, RouteType,
    Routing, SendMode, TrackRef, TrackRoute,
};
use reaper_high::{Pan, Project, Reaper, SendPartnerType, Track, TrackRoute as ReaperTrackRoute};
use reaper_medium::{
    EditMode, ReaperVolumeValue, SendTarget, TrackAttributeKey, TrackSendAttributeKey,
    TrackSendCategory,
};
use tracing::warn;

use crate::project_context::find_project_by_guid;
use crate::safe_wrappers::routing as routing_sw;
use crate::track::resolve_track_pub;

// ============================================================================
// Resolution helpers
// ============================================================================

fn resolve_project(ctx: &ProjectContext) -> Option<Project> {
    match ctx {
        ProjectContext::Current => Some(Reaper::get().current_project()),
        ProjectContext::Project(guid) => find_project_by_guid(guid),
    }
}

fn resolve(ctx: &ProjectContext, track: &TrackRef) -> Option<(Project, Track)> {
    let proj = resolve_project(ctx)?;
    let t = resolve_track_pub(&proj, track)?;
    Some((proj, t))
}

/// Resolve a [`RouteRef`] to a concrete enumeration index on a track.
///
/// `RouteRef::Index(i)` returns `Some(i)`. `RouteRef::ByDestination` walks
/// the appropriate list (sends or receives — hardware outputs have no
/// destination track) and returns the index of the first route whose
/// partner track matches the destination's GUID.
pub(crate) fn resolve_route_index(
    proj: &Project,
    reaper_track: &Track,
    route_type: RouteType,
    route: &RouteRef,
) -> Option<u32> {
    match route {
        RouteRef::Index(idx) => Some(*idx),
        RouteRef::ByDestination(dest) => {
            if matches!(route_type, RouteType::HardwareOutput) {
                warn!("ByDestination is not meaningful for HardwareOutput routes");
                return None;
            }
            let dest_track = resolve_track_pub(proj, dest)?;
            let dest_guid = dest_track.guid().to_string_without_braces();
            let count = match route_type {
                RouteType::Send => reaper_track.typed_send_count(SendPartnerType::Track),
                RouteType::Receive => reaper_track.receive_count(),
                RouteType::HardwareOutput => unreachable!(),
            };
            for i in 0..count {
                let route = match route_type {
                    RouteType::Send => reaper_track.typed_send_by_index(SendPartnerType::Track, i),
                    RouteType::Receive => reaper_track.receive_by_index(i),
                    RouteType::HardwareOutput => unreachable!(),
                };
                let Some(route) = route else { continue };
                if let Some(reaper_high::TrackRoutePartner::Track(t)) = route.partner()
                    && t.guid().to_string_without_braces() == dest_guid
                {
                    return Some(i);
                }
            }
            None
        }
    }
}

/// Fetch the concrete [`ReaperTrackRoute`] for a given route_type + index
/// on a track. For [`RouteType::Send`], REAPER addresses the underlying
/// send list as `[hardware_outputs..., track_sends...]`, so we offset by
/// the hardware-output count.
fn fetch_route(
    reaper_track: &Track,
    route_type: RouteType,
    route_index: u32,
) -> Option<ReaperTrackRoute> {
    match route_type {
        RouteType::Send => {
            let hw_count = reaper_track.typed_send_count(SendPartnerType::HardwareOutput);
            reaper_track.send_by_index(hw_count + route_index)
        }
        RouteType::Receive => reaper_track.receive_by_index(route_index),
        RouteType::HardwareOutput => {
            reaper_track.typed_send_by_index(SendPartnerType::HardwareOutput, route_index)
        }
    }
}

// ============================================================================
// Conversion: REAPER -> daw-proto
// ============================================================================

pub(crate) fn convert_track_route(
    reaper_route: &ReaperTrackRoute,
    route_type: RouteType,
    index: u32,
) -> TrackRoute {
    let source_track = reaper_route.track();
    let source_track_guid = source_track.guid().to_string_without_braces();

    let (dest_track_guid, dest_track_name, hw_output_index, hw_output_name) =
        match reaper_route.partner() {
            Some(reaper_high::TrackRoutePartner::Track(track)) => {
                let guid = track.guid().to_string_without_braces();
                let name = track.name().map(|n| n.to_str().to_string());
                (Some(guid), name, None, None)
            }
            Some(reaper_high::TrackRoutePartner::HardwareOutput(idx)) => {
                let name = reaper_route.name().to_str().to_string();
                (None, None, Some(idx), Some(name))
            }
            None => (None, None, None, None),
        };

    let volume = reaper_route.volume().map(|v| v.get()).unwrap_or(1.0);
    let pan = reaper_route
        .pan()
        .map(|p| p.reaper_value().get())
        .unwrap_or(0.0);

    let muted = reaper_route.is_muted().unwrap_or(false);
    let mono = reaper_route.is_mono();
    let phase_inverted = reaper_route.phase_is_inverted();

    let automation_mode = match reaper_route.automation_mode() {
        reaper_medium::AutomationMode::TrimRead => AutomationMode::TrimRead,
        reaper_medium::AutomationMode::Read => AutomationMode::Read,
        reaper_medium::AutomationMode::Touch => AutomationMode::Touch,
        reaper_medium::AutomationMode::Write => AutomationMode::Write,
        reaper_medium::AutomationMode::Latch => AutomationMode::Latch,
        _ => AutomationMode::TrimRead,
    };
    let midi_channel_mapping = read_midi_channel_mapping(reaper_route, route_type);

    TrackRoute {
        index,
        route_type,
        source_track_guid,
        dest_track_guid,
        dest_track_name,
        hw_output_index,
        hw_output_name,
        volume,
        pan,
        muted,
        mono,
        phase_inverted,
        send_mode: read_send_mode(reaper_route, route_type),
        automation_mode,
        source_channels: ChannelMapping::default(),
        dest_channels: ChannelMapping::default(),
        midi_channel_mapping,
    }
}

/// Read the send mode from a REAPER track route via medium-level API.
/// REAPER send mode values: 0 = post-fader, 1 = pre-FX, 3 = post-FX.
fn read_send_mode(reaper_route: &ReaperTrackRoute, route_type: RouteType) -> SendMode {
    let track = reaper_route.track();
    let (category, cat_index) = match route_type {
        RouteType::Send => {
            let hw_count = track.typed_send_count(SendPartnerType::HardwareOutput);
            let route_idx = reaper_route.index();
            if route_idx < hw_count {
                (TrackSendCategory::HardwareOutput, route_idx)
            } else {
                (TrackSendCategory::Send, route_idx - hw_count)
            }
        }
        RouteType::Receive => (TrackSendCategory::Receive, reaper_route.index()),
        RouteType::HardwareOutput => (TrackSendCategory::HardwareOutput, reaper_route.index()),
    };

    let Ok(media_track) = track.raw() else {
        return SendMode::PostFader;
    };

    let raw_mode = routing_sw::get_track_send_info_value(
        Reaper::get().medium_reaper(),
        media_track,
        category,
        cat_index,
        TrackSendAttributeKey::SendMode,
    ) as i32;

    match raw_mode {
        1 => SendMode::PreFx,
        3 => SendMode::PostFx,
        _ => SendMode::PostFader,
    }
}

fn read_midi_channel_mapping(
    reaper_route: &ReaperTrackRoute,
    route_type: RouteType,
) -> Option<MidiChannelMapping> {
    let track = reaper_route.track();
    let (category, cat_index) = match route_type {
        RouteType::Send => {
            let hw_count = track.typed_send_count(SendPartnerType::HardwareOutput);
            let route_idx = reaper_route.index();
            if route_idx < hw_count {
                return None;
            }
            (TrackSendCategory::Send, route_idx - hw_count)
        }
        RouteType::Receive => (TrackSendCategory::Receive, reaper_route.index()),
        RouteType::HardwareOutput => return None,
    };

    let Ok(media_track) = track.raw() else {
        return None;
    };

    let raw_flags = routing_sw::get_track_send_info_value(
        Reaper::get().medium_reaper(),
        media_track,
        category,
        cat_index,
        TrackSendAttributeKey::MidiFlags,
    ) as i32;

    Some(parse_midi_channel_mapping(raw_flags))
}

fn parse_midi_channel_mapping(raw_flags: i32) -> MidiChannelMapping {
    let src_bits = raw_flags & 0x1f;
    let dst_bits = (raw_flags >> 5) & 0x1f;

    let source = match src_bits {
        1..=16 => MidiSourceChannel::Channel(src_bits as u8),
        _ => MidiSourceChannel::All,
    };
    let destination = match dst_bits {
        1..=16 => MidiDestinationChannel::Channel(dst_bits as u8),
        _ => MidiDestinationChannel::Original,
    };

    MidiChannelMapping {
        source,
        destination,
    }
}

fn send_mode_to_raw(mode: SendMode) -> i32 {
    match mode {
        SendMode::PostFader => 0,
        SendMode::PreFx => 1,
        SendMode::PostFx => 3,
    }
}

/// Resolve `(category, category_local_index)` for a route. Send routes
/// in REAPER's index space include hardware outputs at the front; this
/// returns the index relative to the route's own category.
fn category_for(
    reaper_track: &Track,
    route_type: RouteType,
    route_index: u32,
) -> (TrackSendCategory, u32) {
    match route_type {
        RouteType::Send => {
            let hw_count = reaper_track.typed_send_count(SendPartnerType::HardwareOutput);
            if route_index < hw_count {
                (TrackSendCategory::HardwareOutput, route_index)
            } else {
                (TrackSendCategory::Send, route_index - hw_count)
            }
        }
        RouteType::Receive => (TrackSendCategory::Receive, route_index),
        RouteType::HardwareOutput => (TrackSendCategory::HardwareOutput, route_index),
    }
}

// ============================================================================
// impl Routing for Reaper
// ============================================================================

impl Routing for crate::Reaper {
    // ── Queries ────────────────────────────────────────────────────

    fn sends(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some((_, t)) = resolve(&project, &track) else {
            return Vec::new();
        };
        t.typed_sends(SendPartnerType::Track)
            .enumerate()
            .map(|(i, r)| convert_track_route(&r, RouteType::Send, i as u32))
            .collect()
    }

    fn receives(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some((_, t)) = resolve(&project, &track) else {
            return Vec::new();
        };
        t.receives()
            .enumerate()
            .map(|(i, r)| convert_track_route(&r, RouteType::Receive, i as u32))
            .collect()
    }

    fn hardware_outputs(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute> {
        let Some((_, t)) = resolve(&project, &track) else {
            return Vec::new();
        };
        t.typed_sends(SendPartnerType::HardwareOutput)
            .enumerate()
            .map(|(i, r)| convert_track_route(&r, RouteType::HardwareOutput, i as u32))
            .collect()
    }

    fn get_route(&self, project: ProjectContext, location: RouteLocation) -> Option<TrackRoute> {
        let (proj, reaper_track) = resolve(&project, &location.track)?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)?;
        Some(convert_track_route(
            &reaper_route,
            location.route_type,
            route_index,
        ))
    }

    fn send_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        resolve(&project, &track)
            .map(|(_, t)| t.typed_send_count(SendPartnerType::Track))
            .unwrap_or(0)
    }

    fn receive_count(&self, project: ProjectContext, track: TrackRef) -> u32 {
        resolve(&project, &track)
            .map(|(_, t)| t.receive_count())
            .unwrap_or(0)
    }

    // ── CRUD ───────────────────────────────────────────────────────

    fn add_send(&self, project: ProjectContext, source: TrackRef, dest: TrackRef) -> Option<u32> {
        let (proj, source_track) = resolve(&project, &source)?;
        let dest_track = resolve_track_pub(&proj, &dest)?;
        let route = source_track.add_send_to(&dest_track);
        route.track_route_index()
    }

    fn add_hardware_output(
        &self,
        project: ProjectContext,
        track: TrackRef,
        hw_output: u32,
    ) -> Option<u32> {
        let (_, reaper_track) = resolve(&project, &track)?;
        let raw_track = reaper_track.raw().ok()?;
        let medium = Reaper::get().medium_reaper();
        match routing_sw::create_track_send(medium, raw_track, SendTarget::HardwareOutput) {
            Ok(index) => {
                // hw_output is typically the stereo pair index (0 = 1-2, 1 = 3-4, ...).
                let dst_chan = (hw_output * 2) as f64;
                routing_sw::set_track_send_info_value(
                    medium,
                    raw_track,
                    TrackSendCategory::HardwareOutput,
                    index,
                    TrackSendAttributeKey::DstChan,
                    dst_chan,
                );
                Some(index)
            }
            Err(e) => {
                warn!("Failed to create hardware output: {:?}", e);
                None
            }
        }
    }

    fn remove_route(&self, project: ProjectContext, location: RouteLocation) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let raw_track = reaper_track
            .raw()
            .map_err(|e| DawError::invalid_object("Track", &format!("{e:?}")))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;

        let (category, actual_index) = match location.route_type {
            RouteType::Send => {
                let hw_count = reaper_track.typed_send_count(SendPartnerType::HardwareOutput);
                (
                    TrackSendCategory::Send,
                    route_index - hw_count.min(route_index),
                )
            }
            RouteType::Receive => (TrackSendCategory::Receive, route_index),
            RouteType::HardwareOutput => (TrackSendCategory::HardwareOutput, route_index),
        };

        routing_sw::remove_track_send(
            Reaper::get().medium_reaper(),
            raw_track,
            category,
            actual_index,
        );
        Ok(())
    }

    // ── Levels ─────────────────────────────────────────────────────

    fn set_volume(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        volume: f64,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)
            .ok_or_else(|| DawError::not_found("Route", &format!("idx={route_index}")))?;
        let volume_value = ReaperVolumeValue::new(volume)
            .map_err(|e| DawError::operation_failed(format!("invalid volume: {e:?}")))?;
        reaper_route
            .set_volume(volume_value, EditMode::NormalTweak)
            .map_err(|e| DawError::operation_failed(format!("set_volume: {e:?}")))?;
        Ok(())
    }

    fn set_pan(&self, project: ProjectContext, location: RouteLocation, pan: f64) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)
            .ok_or_else(|| DawError::not_found("Route", &format!("idx={route_index}")))?;
        let pan_obj = Pan::from_normalized_value(pan.clamp(-1.0, 1.0));
        reaper_route
            .set_pan(pan_obj, EditMode::NormalTweak)
            .map_err(|e| DawError::operation_failed(format!("set_pan: {e:?}")))?;
        Ok(())
    }

    // ── State ──────────────────────────────────────────────────────

    fn set_muted(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        muted: bool,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)
            .ok_or_else(|| DawError::not_found("Route", &format!("idx={route_index}")))?;
        if muted {
            reaper_route
                .mute()
                .map_err(|e| DawError::operation_failed(format!("mute: {e:?}")))?;
        } else {
            reaper_route
                .unmute()
                .map_err(|e| DawError::operation_failed(format!("unmute: {e:?}")))?;
        }
        Ok(())
    }

    fn set_mono(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        mono: bool,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)
            .ok_or_else(|| DawError::not_found("Route", &format!("idx={route_index}")))?;
        reaper_route
            .set_mono(mono)
            .map_err(|e| DawError::operation_failed(format!("set_mono: {e:?}")))?;
        Ok(())
    }

    fn set_phase(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        inverted: bool,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let reaper_route = fetch_route(&reaper_track, location.route_type, route_index)
            .ok_or_else(|| DawError::not_found("Route", &format!("idx={route_index}")))?;
        reaper_route
            .set_phase_inverted(inverted)
            .map_err(|e| DawError::operation_failed(format!("set_phase: {e:?}")))?;
        Ok(())
    }

    fn is_muted(&self, project: ProjectContext, location: RouteLocation) -> bool {
        let Some((proj, reaper_track)) = resolve(&project, &location.track) else {
            return false;
        };
        let Some(route_index) =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
        else {
            return false;
        };
        let Some(reaper_route) = fetch_route(&reaper_track, location.route_type, route_index)
        else {
            return false;
        };
        reaper_route.is_muted().unwrap_or(false)
    }

    // ── Send mode ──────────────────────────────────────────────────

    fn set_send_mode(
        &self,
        project: ProjectContext,
        track: TrackRef,
        route: RouteRef,
        mode: SendMode,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", track)))?;
        let raw_track = reaper_track
            .raw()
            .map_err(|e| DawError::invalid_object("Track", &format!("{e:?}")))?;
        let route_index = resolve_route_index(&proj, &reaper_track, RouteType::Send, &route)
            .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", route)))?;

        let hw_count = reaper_track.typed_send_count(SendPartnerType::HardwareOutput);
        let (category, actual_index) = if route_index < hw_count {
            (TrackSendCategory::HardwareOutput, route_index)
        } else {
            (TrackSendCategory::Send, route_index - hw_count)
        };

        routing_sw::set_track_send_info_value(
            Reaper::get().medium_reaper(),
            raw_track,
            category,
            actual_index,
            TrackSendAttributeKey::SendMode,
            send_mode_to_raw(mode) as f64,
        );
        Ok(())
    }

    // ── Channel mapping ────────────────────────────────────────────

    fn set_source_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start_channel: u32,
        _num_channels: u32,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let raw_track = reaper_track
            .raw()
            .map_err(|e| DawError::invalid_object("Track", &format!("{e:?}")))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let (category, actual_index) =
            category_for(&reaper_track, location.route_type, route_index);
        routing_sw::set_track_send_info_value(
            Reaper::get().medium_reaper(),
            raw_track,
            category,
            actual_index,
            TrackSendAttributeKey::SrcChan,
            start_channel as f64,
        );
        Ok(())
    }

    fn set_dest_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start_channel: u32,
        _num_channels: u32,
    ) -> DawResult<()> {
        let (proj, reaper_track) = resolve(&project, &location.track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", location.track)))?;
        let raw_track = reaper_track
            .raw()
            .map_err(|e| DawError::invalid_object("Track", &format!("{e:?}")))?;
        let route_index =
            resolve_route_index(&proj, &reaper_track, location.route_type, &location.route)
                .ok_or_else(|| DawError::not_found("Route", &format!("{:?}", location.route)))?;
        let (category, actual_index) =
            category_for(&reaper_track, location.route_type, route_index);
        routing_sw::set_track_send_info_value(
            Reaper::get().medium_reaper(),
            raw_track,
            category,
            actual_index,
            TrackSendAttributeKey::DstChan,
            start_channel as f64,
        );
        Ok(())
    }

    // ── Parent send (folder routing) ───────────────────────────────

    fn parent_send_enabled(&self, project: ProjectContext, track: TrackRef) -> bool {
        let Some((_, reaper_track)) = resolve(&project, &track) else {
            return true;
        };
        let Ok(raw_track) = reaper_track.raw() else {
            return true;
        };
        let value = routing_sw::get_media_track_info_value(
            Reaper::get().medium_reaper(),
            raw_track,
            TrackAttributeKey::MainSend,
        );
        value > 0.0
    }

    fn set_parent_send_enabled(
        &self,
        project: ProjectContext,
        track: TrackRef,
        enabled: bool,
    ) -> DawResult<()> {
        let (_, reaper_track) = resolve(&project, &track)
            .ok_or_else(|| DawError::not_found("Track", &format!("{:?}", track)))?;
        let raw_track = reaper_track
            .raw()
            .map_err(|e| DawError::invalid_object("Track", &format!("{e:?}")))?;
        routing_sw::set_media_track_info_value(
            Reaper::get().medium_reaper(),
            raw_track,
            TrackAttributeKey::MainSend,
            if enabled { 1.0 } else { 0.0 },
        );
        Ok(())
    }
}
