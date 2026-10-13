//! Push-based change detection via REAPER's IReaperControlSurface callbacks.
//!
//! Implements [`ControlSurfaceMiddleware`] from `reaper-high`. REAPER calls
//! each `set_*` method whenever the corresponding state changes, so we don't
//! have to wait for the next 30Hz poll tick to detect it. Each callback
//! translates into the matching `TrackEvent` / `TransportEvent` /
//! `FxEvent` / `RoutingEvent` and publishes through the same channel the
//! pollers use, so the sync forwarders in `daw-synchronization::subscriptions`
//! see push events that are structurally identical to what the pollers emit.
//!
//! The pollers are kept (not all REAPER state changes fire callbacks —
//! markers, regions, tempo-map points, items, etc.), so this is a parallel
//! fast path, not a replacement. To avoid duplicate publishes (callback +
//! the next poll diffing the same field), each callback also patches the
//! poller's cache before publishing, so the next poll sees no diff.
//!
//! All callbacks are called by REAPER on the main thread, synchronously,
//! and may fire at high rate (FX param sweeps), so the implementation
//! avoids any heap allocation in the hot path beyond what publishing an
//! event already requires.

use reaper_high::{
    ControlSurfaceEvent, ControlSurfaceMiddleware, Project, Reaper as ReaperHigh,
    get_media_track_guid,
};
use reaper_medium::{
    ExtSetBpmAndPlayRateArgs, ExtSetFxChangeArgs, ExtSetFxEnabledArgs, ExtSetFxParamArgs,
    ExtSetRecvPanArgs, ExtSetRecvVolumeArgs, ExtSetSendPanArgs, ExtSetSendVolumeArgs, MediaTrack,
    ProjectContext as ReaperProjectContext, ReaProject, SetPlayStateArgs, SetRepeatStateArgs,
    SetSurfaceMuteArgs, SetSurfacePanArgs, SetSurfaceRecArmArgs, SetSurfaceSelectedArgs,
    SetSurfaceSoloArgs, SetSurfaceVolumeArgs, SetTrackTitleArgs, TrackFxLocation,
    VersionDependentTrackFxLocation,
};

use daw_proto::track::TrackStreamEvent;
use daw_proto::transport::TransportEvent;
use daw_proto::{FxChainContext, FxEvent, RouteType, RoutingEvent, TrackEvent};

use crate::event_hub::hub;
use crate::project_context::{MAX_PROJECT_TABS, project_guid};
use crate::transport::read_transport_state_for_project;

/// Middleware adapter that maps each REAPER control-surface callback to
/// our internal event hub / per-domain broadcasters.
///
/// How aggressively the surface translates REAPER callbacks into event-hub
/// publishes.
///
/// - [`Mode::Full`] — publish every callback (volume, pan, mute, FX params,
///   transport, sends/receives, the lot). Genuine sub-tick push for every
///   subscriber. Use this when nothing on the receive side is going to
///   *apply* the published events back to REAPER (web UIs, MIDI/OSC surface
///   adapters, telemetry — all read-only). With a bidirectional sync
///   forwarder on the same hub you'll get echo loops; pick `PushOnly` then.
/// - [`Mode::PushOnly`] — publish only the callbacks that have no
///   equivalent in the 30 Hz poller path (FX parameter changes, marker /
///   region nudges). Everything else falls back to the poller, which gives
///   33 ms latency but no echo problem for bidirectional sync. This is
///   what the multi-instance sync test uses, and what the session window
///   attached to a live REAPER runs: it applies what it receives, so
///   `Full` would feed it its own writes back. Buying the 33 ms back
///   would mean giving every event an origin so a client could drop its
///   own echo — a field on the wire that every future publisher has to
///   set correctly, to shave one poll tick off a change somebody else
///   made. The window's own moves are already immediate; the wait is
///   only on changes made in REAPER, where a tick is not noticeable.
/// - [`Mode::Off`] — middleware registers but every callback returns
///   `false`. Equivalent to not registering at all; kept so the surface
///   can be hot-toggled without re-registering with REAPER.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    Full,
    #[default]
    PushOnly,
    Off,
}

impl Mode {
    /// Parse from `FTS_CSURF_MODE` env: `"full"`, `"push-only"`, `"off"`.
    /// Anything else (or unset) returns the default `PushOnly`.
    pub fn from_env() -> Self {
        match std::env::var("FTS_CSURF_MODE").as_deref() {
            Ok("full") => Self::Full,
            Ok("push-only") => Self::PushOnly,
            Ok("off") => Self::Off,
            _ => Self::default(),
        }
    }
}

/// Stateless other than the dispatch mode: the per-domain caches live in
/// their respective modules (`track::track_cache`, `routing_stream`,
/// `fx_stream`). Instantiated once at extension load time and registered
/// with `ReaperSession`.
#[derive(Debug, Default)]
pub struct DawControlSurface {
    mode: Mode,
}

impl DawControlSurface {
    /// Build a surface in [`Mode::PushOnly`] — the default for bidirectional
    /// sync hosts. Use [`Self::with_mode`] for `Full` (sub-tick push for
    /// read-only subscribers like a web UI feed) or `Off`.
    pub fn new() -> Self {
        Self::with_mode(Mode::default())
    }

    pub fn with_mode(mode: Mode) -> Self {
        Self { mode }
    }
}

impl ControlSurfaceMiddleware for DawControlSurface {
    fn handle_event(&self, event: ControlSurfaceEvent) -> bool {
        use ControlSurfaceEvent::*;
        if self.mode == Mode::Off {
            return false;
        }
        let full = self.mode == Mode::Full;
        match event {
            // ── Push-only events (no poller equivalent) — handled in both modes
            ExtSetFxParam(args) => on_fx_param(args, false),
            ExtSetFxParamRecFx(args) => on_fx_param(args, true),
            ExtSetProjectMarkerChange(_) => {
                crate::poll_and_broadcast_markers();
                crate::poll_and_broadcast_regions();
                true
            }
            // ── Full-mode handlers — duplicate the 30 Hz poller with sub-tick
            //    latency. The cache-membership guard on each handler still
            //    skips publishes for entities the poller hasn't seen yet,
            //    so on-track-init churn doesn't flood the event hub.
            SetSurfaceVolume(args) if full => on_surface_volume(args),
            SetSurfacePan(args) if full => on_surface_pan(args),
            SetSurfaceMute(args) if full => on_surface_mute(args),
            SetSurfaceSelected(args) if full => on_surface_selected(args),
            SetSurfaceSolo(args) if full => on_surface_solo(args),
            SetSurfaceRecArm(args) if full => on_surface_rec_arm(args),
            SetTrackTitle(args) if full => on_track_title(args),
            SetPlayState(args) if full => on_play_state(args),
            SetRepeatState(args) if full => on_repeat_state(args),
            ExtSetBpmAndPlayRate(args) if full => on_bpm_or_playrate(args),
            ExtSetFxEnabled(args) if full => on_fx_enabled(args),
            ExtSetFxChange(args) if full => on_fx_change(args),
            ExtSetSendVolume(args) if full => on_send_volume(args),
            ExtSetSendPan(args) if full => on_send_pan(args),
            ExtSetRecvVolume(args) if full => on_recv_volume(args),
            ExtSetRecvPan(args) if full => on_recv_pan(args),
            _ => false,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Resolve the project containing `media_track` to its stable guid.
/// Falls back to current project on failure.
fn project_guid_for_track(media_track: MediaTrack) -> Option<String> {
    // `Track::new(mt, None)` fills the ReaProject via `get_track_project_raw`
    // (validates against current project then enumerates others).
    let track = reaper_high::Track::new(media_track, None);
    let project = track.project();
    Some(project_guid(&project))
}

/// Track guid (no braces) for the cache + event payloads.
fn track_guid_str(media_track: MediaTrack) -> String {
    get_media_track_guid(media_track).to_string_without_braces()
}

fn publish_track_event(project_guid: String, event: TrackEvent) {
    hub().publish_track(TrackStreamEvent {
        project_guid,
        event,
    });
}

/// Patch the cached `Track` field for the (project_guid, track_guid)
/// entry, applying `f` to the cached value. If the cache hasn't seen
/// this track yet, do nothing — the next poll will seed it.
/// Patch the cached `Track` field for the (project_guid, track_guid)
/// entry. Returns `true` if the cache had the entry and the patch was
/// applied. Returns `false` if the track isn't cached yet — callers use
/// this as a signal to skip publishing the event so they don't push
/// events for tracks the follower hasn't seen Added for yet.
fn patch_track_cache<F: FnOnce(&mut daw_proto::Track)>(
    project_guid: &str,
    track_guid: &str,
    f: F,
) -> bool {
    let Ok(mut cache) = crate::track::track_cache().lock() else {
        return false;
    };
    let Some(project) = cache.get_mut(project_guid) else {
        return false;
    };
    let Some(t) = project.get_mut(track_guid) else {
        return false;
    };
    f(t);
    true
}

// ── Per-track scalar callbacks ─────────────────────────────────────────

fn on_surface_volume(args: SetSurfaceVolumeArgs) -> bool {
    // Only publish a VolumeChanged event if the track is already in our
    // cache. During track add, REAPER fires SetSurfaceVolume *before*
    // SetTrackListChange, so the track isn't in TRACK_CACHE yet. Publishing
    // a VolumeChanged for a track the follower hasn't seen Added for yet
    // creates an out-of-order event that floods the apply pipeline with
    // "track not found" lookups and starves the Added event.
    //
    // patch_track_cache returns true if the track exists in the cache.
    // Once the timer poller has seen the track, csurf events publish
    // normally and beat the next poll tick.
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let volume = args.volume.get();
    if !patch_track_cache(&project_guid, &guid, |t| t.volume = volume) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::VolumeChanged { guid, volume });
    true
}

fn on_surface_pan(args: SetSurfacePanArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let pan = args.pan.get();
    if !patch_track_cache(&project_guid, &guid, |t| t.pan = pan) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::PanChanged { guid, pan });
    true
}

fn on_surface_mute(args: SetSurfaceMuteArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let muted = args.is_mute;
    if !patch_track_cache(&project_guid, &guid, |t| t.muted = muted) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::MuteChanged { guid, muted });
    true
}

fn on_surface_selected(args: SetSurfaceSelectedArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let selected = args.is_selected;
    if !patch_track_cache(&project_guid, &guid, |t| t.selected = selected) {
        return false;
    }
    publish_track_event(
        project_guid,
        TrackEvent::SelectionChanged { guid, selected },
    );
    true
}

fn on_surface_solo(args: SetSurfaceSoloArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let soloed = args.is_solo;
    if !patch_track_cache(&project_guid, &guid, |t| t.soloed = soloed) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::SoloChanged { guid, soloed });
    true
}

fn on_surface_rec_arm(args: SetSurfaceRecArmArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let armed = args.is_armed;
    if !patch_track_cache(&project_guid, &guid, |t| t.armed = armed) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::ArmChanged { guid, armed });
    true
}

fn on_track_title(args: SetTrackTitleArgs<'_>) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let guid = track_guid_str(args.track);
    let name = args.name.to_str().to_string();
    if !patch_track_cache(&project_guid, &guid, |t| t.name = name.clone()) {
        return false;
    }
    publish_track_event(project_guid, TrackEvent::Renamed { guid, name });
    true
}

// ── Transport ──────────────────────────────────────────────────────────
//
// Play / repeat / bpm callbacks broadcast a `TransportEvent::Snapshot`
// for every open project. REAPER doesn't tell us which project the event
// is "for" (it's typically the active one for transport, global for tempo
// maps), so we re-broadcast all so subscribers stay consistent. The
// pollers' diff path will not re-emit because the snapshot publish only
// runs when the cached state differs.

fn broadcast_transport_snapshots() -> bool {
    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(reaper_medium::ProjectRef::Tab(tab_index), 0)
        else {
            break;
        };
        let project = Project::new(result.project);
        let project_guid_str = project_guid(&project);
        let reaper_ctx = ReaperProjectContext::Proj(result.project);
        let state = read_transport_state_for_project(&project, reaper_ctx, medium);
        hub().publish_transport_state(TransportEvent::Snapshot {
            project_guid: project_guid_str,
            state,
        });
    }
    true
}

fn on_play_state(_args: SetPlayStateArgs) -> bool {
    broadcast_transport_snapshots()
}

fn on_repeat_state(_args: SetRepeatStateArgs) -> bool {
    broadcast_transport_snapshots()
}

fn on_bpm_or_playrate(_args: ExtSetBpmAndPlayRateArgs) -> bool {
    broadcast_transport_snapshots()
}

// ── FX ─────────────────────────────────────────────────────────────────

fn fx_context_for(track: MediaTrack, is_input_fx: bool) -> FxChainContext {
    let guid = track_guid_str(track);
    if is_input_fx {
        FxChainContext::Input(guid)
    } else {
        FxChainContext::Track(guid)
    }
}

fn fx_guid_str(track: MediaTrack, location: TrackFxLocation) -> Option<String> {
    // SAFETY: track came from REAPER's own callback args, so it's live
    // for the duration of this main-thread call.
    let guid = unsafe {
        ReaperHigh::get()
            .medium_reaper()
            .track_fx_get_fx_guid(track, location)
            .ok()?
    };
    let s = ReaperHigh::get().medium_reaper().guid_to_string(&guid);
    let raw = s.to_str();
    // Strip braces for our string-guid convention.
    Some(
        raw.trim_start_matches('{')
            .trim_end_matches('}')
            .to_string(),
    )
}

fn on_fx_param(args: ExtSetFxParamArgs, is_input_fx: bool) -> bool {
    // REAPER hands us a 0..1 normalized value. Our `FxEvent::ParameterChanged`
    // carries `value: f64` and the apply path uses the same normalized range
    // (it calls TrackFX_SetParamNormalized), so we forward the value as-is.
    let location = TrackFxLocation::from_raw(args.fx_index as i32);
    let Some(fx_guid) = fx_guid_str(args.track, location) else {
        return false;
    };
    let context = fx_context_for(args.track, is_input_fx);
    crate::fx_stream::publish_from_callback(FxEvent::ParameterChanged {
        context,
        fx_guid,
        param_index: args.param_index,
        value: args.param_value.get(),
    });
    true
}

fn on_fx_enabled(args: ExtSetFxEnabledArgs) -> bool {
    let (location, is_input_fx) = match args.fx_location {
        VersionDependentTrackFxLocation::New(loc) => {
            (loc, matches!(loc, TrackFxLocation::InputFxChain(_)))
        }
        VersionDependentTrackFxLocation::Old(idx) => (TrackFxLocation::NormalFxChain(idx), false),
    };
    let Some(fx_guid) = fx_guid_str(args.track, location) else {
        return false;
    };
    let context = fx_context_for(args.track, is_input_fx);
    crate::fx_stream::publish_from_callback(FxEvent::EnabledChanged {
        context,
        fx_guid,
        enabled: args.is_enabled,
    });
    true
}

fn on_fx_change(_args: ExtSetFxChangeArgs) -> bool {
    // FX added / removed / reordered. Defer to the existing chain-diff
    // poller (it already builds the full Added/Removed/Moved diff).
    crate::poll_and_broadcast_fx();
    // And to the track poller, which is where a *count* change becomes
    // `TrackEvent::FxCountChanged`. Without this the mixer's FX button
    // would be correct only as far as the next periodic tick; REAPER has
    // just told us the chain changed, so there is no reason to wait.
    crate::poll_and_broadcast_tracks();
    true
}

// ── Routing ────────────────────────────────────────────────────────────
//
// REAPER's send/receive index for control-surface callbacks: send_index
// starts with hardware outputs and continues with track sends, per the
// medium-level docs (`ExtSetSendVolumeArgs::send_index`). Our routing
// pollers cache hardware-outputs and track-sends in separate buckets
// keyed by `RouteType`, so we split here.

fn split_send_index(track: MediaTrack, raw_index: u32) -> Option<(RouteType, u32)> {
    // `GetTrackNumSends(track, 1 /* hardware */)` gives the hardware
    // output count; raw_index < hw_count → HardwareOutput, otherwise
    // it's a Send with index `raw_index - hw_count`.
    let hw_count = unsafe {
        ReaperHigh::get()
            .medium_reaper()
            .get_track_num_sends(track, reaper_medium::TrackSendCategory::HardwareOutput)
    };
    if raw_index < hw_count {
        Some((RouteType::HardwareOutput, raw_index))
    } else {
        Some((RouteType::Send, raw_index - hw_count))
    }
}

fn on_send_volume(args: ExtSetSendVolumeArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let Some((route_type, route_index)) = split_send_index(args.track, args.send_index) else {
        return false;
    };
    crate::routing_stream::publish_from_callback(RoutingEvent::VolumeChanged {
        project_guid,
        source_track_guid: track_guid_str(args.track),
        route_type,
        route_index,
        volume: args.volume.get(),
    });
    true
}

fn on_send_pan(args: ExtSetSendPanArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    let Some((route_type, route_index)) = split_send_index(args.track, args.send_index) else {
        return false;
    };
    crate::routing_stream::publish_from_callback(RoutingEvent::PanChanged {
        project_guid,
        source_track_guid: track_guid_str(args.track),
        route_type,
        route_index,
        pan: args.pan.get(),
    });
    true
}

fn on_recv_volume(args: ExtSetRecvVolumeArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    crate::routing_stream::publish_from_callback(RoutingEvent::VolumeChanged {
        project_guid,
        source_track_guid: track_guid_str(args.track),
        route_type: RouteType::Receive,
        route_index: args.receive_index,
        volume: args.volume.get(),
    });
    true
}

fn on_recv_pan(args: ExtSetRecvPanArgs) -> bool {
    let Some(project_guid) = project_guid_for_track(args.track) else {
        return false;
    };
    crate::routing_stream::publish_from_callback(RoutingEvent::PanChanged {
        project_guid,
        source_track_guid: track_guid_str(args.track),
        route_type: RouteType::Receive,
        route_index: args.receive_index,
        pan: args.pan.get(),
    });
    true
}

// Quiet unused-import warnings for `ReaProject` if no path here ends up
// using it once the file compiles in isolation.
#[allow(dead_code)]
fn _force_reaproject_in_scope(_: ReaProject) {}
