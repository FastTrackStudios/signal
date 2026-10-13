//! `impl Transport for Standalone` — engine-backed.
//!
//! Every mutation writes through the per-project [`TransportBundle`]
//! (atomic, lock-free) and is mirrored into the proto `Transport`
//! struct in `ProjectState` so existing snapshot consumers still see
//! a consistent view. Reads that are time-sensitive (`get_position`,
//! `is_playing`) go straight to the engine atomics; reads that need
//! the full proto struct read `ProjectState`.
//!
//! The engine's soft clock drives playhead advance on a wall-clock
//! tick (see [`crate::transport_engine::bundle`]) — works in headless
//! tests, native, and WASM. A future mixer integration can flip the
//! soft clock off and drive `advance()` sample-accurately from the
//! audio callback.

use daw_proto::transport::service::Transport;
use daw_proto::{
    DawError, DawResult, LoopRegion, PlayState, ProjectContext, TimeSignature,
    Transport as TransportState,
    primitives::{Position, PositionInSeconds, Tempo},
};

use crate::sync::Standalone;
use crate::transport_engine::{LoopRegionSamples, PlayStateRepr, SampleClock};

impl daw_proto::transport::service::TransportStreamSource for Standalone {
    fn events_hub(&self) -> &architect::PubSub<daw_proto::transport::TransportStreamEvent> {
        &self.transport_events
    }
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

fn play_state_proto(s: PlayStateRepr) -> PlayState {
    match s {
        PlayStateRepr::Stopped => PlayState::Stopped,
        PlayStateRepr::Playing => PlayState::Playing,
        PlayStateRepr::Paused => PlayState::Paused,
        PlayStateRepr::Recording => PlayState::Recording,
    }
}

fn play_state_repr(s: PlayState) -> PlayStateRepr {
    match s {
        PlayState::Stopped => PlayStateRepr::Stopped,
        PlayState::Playing => PlayStateRepr::Playing,
        PlayState::Paused => PlayStateRepr::Paused,
        PlayState::Recording => PlayStateRepr::Recording,
    }
}

/// Set engine play-state AND mirror into proto `Transport`.
///
/// Going into `Recording` starts a recording pass (every armed track's
/// input captured — `crate::recording`); leaving it by any road (stop,
/// play/stop, pause, toggle) finishes the pass through
/// [`finish_recording`], which places each capture as a new take. A pass
/// that cannot start refuses the record, and the transport stays put.
fn set_play_state(daw: &Standalone, guid: &str, ps: PlayState) -> DawResult<()> {
    let bundle = daw.transport_engine_for(guid);
    let was_recording = matches!(bundle.shared.play_state(), PlayStateRepr::Recording);
    let recording = matches!(ps, PlayState::Recording);
    if recording && !was_recording {
        begin_recording(daw, guid, &bundle)?;
    }
    bundle.shared.set_play_state(play_state_repr(ps));
    let mirrored = daw.with_project_mut(guid, |p| p.transport.play_state = ps);
    if was_recording && !recording {
        // After the state change, so the audio thread has stopped
        // capturing by the time the pass is taken.
        finish_recording(daw, guid)?;
    }
    mirrored
}

/// Start the recording pass for `guid` (see `crate::recording`).
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "audio", feature = "decode")
))]
fn begin_recording(
    daw: &Standalone,
    guid: &str,
    bundle: &crate::transport_engine::TransportBundle,
) -> DawResult<()> {
    crate::recording::begin(daw, guid, bundle.shared.clone())
}

/// Without an audio engine there is no input to capture.
#[cfg(not(all(
    not(target_arch = "wasm32"),
    any(feature = "audio", feature = "decode")
)))]
fn begin_recording(
    _daw: &Standalone,
    _guid: &str,
    _bundle: &crate::transport_engine::TransportBundle,
) -> DawResult<()> {
    Ok(())
}

/// Finish the recording pass on `guid`: each armed track's capture put on
/// it as a new take on a fixed lane ([`Standalone::finish_recording`]).
/// What it placed is kept for [`Standalone::last_recording`].
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "audio", feature = "decode")
))]
fn finish_recording(daw: &Standalone, guid: &str) -> DawResult<()> {
    daw.finish_recording(guid).map(|_| ())
}

#[cfg(not(all(
    not(target_arch = "wasm32"),
    any(feature = "audio", feature = "decode")
)))]
fn finish_recording(_daw: &Standalone, _guid: &str) -> DawResult<()> {
    Ok(())
}

/// Reconcile proto `Transport.playhead_position` from the engine —
/// callers that hand out the full proto struct should run this first.
fn sync_playhead_to_proto(daw: &Standalone, guid: &str) {
    let bundle = daw.transport_engine_for(guid);
    let snap = bundle.snapshot();
    let pos = Position::from_time(PositionInSeconds::from_seconds(snap.seconds.0));
    let _ = daw.with_project_mut(guid, |p| {
        p.transport.playhead_position = pos.clone();
        p.transport.play_state = play_state_proto(snap.play_state);
    });
}

impl Transport for Standalone {
    fn play(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        set_play_state(self, &guid, PlayState::Playing)
    }

    fn pause(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        set_play_state(self, &guid, PlayState::Paused)
    }

    fn stop(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        set_play_state(self, &guid, PlayState::Stopped)
    }

    fn play_pause(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let next = match bundle.shared.play_state() {
            PlayStateRepr::Playing => PlayState::Paused,
            _ => PlayState::Playing,
        };
        set_play_state(self, &guid, next)
    }

    fn play_stop(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let next = match bundle.shared.play_state() {
            PlayStateRepr::Playing | PlayStateRepr::Recording => PlayState::Stopped,
            _ => PlayState::Playing,
        };
        set_play_state(self, &guid, next)
    }

    fn record(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        set_play_state(self, &guid, PlayState::Recording)
    }

    fn stop_recording(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        if matches!(bundle.shared.play_state(), PlayStateRepr::Recording) {
            set_play_state(self, &guid, PlayState::Stopped)?;
        }
        Ok(())
    }

    fn toggle_recording(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let next = match bundle.shared.play_state() {
            PlayStateRepr::Recording => PlayState::Stopped,
            _ => PlayState::Recording,
        };
        set_play_state(self, &guid, next)
    }

    fn set_position(&self, project: ProjectContext, seconds: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let clock = SampleClock::new(bundle.shared.sample_rate());
        let samples = clock.seconds_to_samples(crate::transport_engine::InstantSeconds(seconds));
        bundle.shared.set_playhead(samples);
        let pos = Position::from_time(PositionInSeconds::from_seconds(seconds));
        self.with_project_mut(&guid, |p| {
            p.transport.playhead_position = pos.clone();
            p.transport.edit_position = pos;
        })
    }

    fn get_position(&self, project: ProjectContext) -> f64 {
        let Some(guid) = resolve_project(self, &project) else {
            return 0.0;
        };
        self.transport_engine_for(&guid).snapshot().seconds.0
    }

    fn goto_start(&self, project: ProjectContext) -> DawResult<()> {
        <Self as Transport>::set_position(self, project, 0.0)
    }

    fn goto_end(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        // Standalone has no real "end" — approximate with loop end.
        let end = self
            .with_project(&guid, |p| {
                p.transport
                    .loop_region
                    .as_ref()
                    .map(|r| r.end_seconds)
                    .unwrap_or(0.0)
            })
            .unwrap_or(0.0);
        <Self as Transport>::set_position(self, ProjectContext::Project(guid), end)
    }

    fn get_state(&self, project: ProjectContext) -> TransportState {
        let Some(guid) = resolve_project(self, &project) else {
            return TransportState::default();
        };
        // Engine is the source of truth for playhead + play_state —
        // mirror before snapshotting so callers see a fresh view.
        sync_playhead_to_proto(self, &guid);
        self.with_project(&guid, |p| p.transport.clone())
            .unwrap_or_default()
    }

    fn get_play_state(&self, project: ProjectContext) -> PlayState {
        let Some(guid) = resolve_project(self, &project) else {
            return PlayState::Stopped;
        };
        play_state_proto(self.transport_engine_for(&guid).shared.play_state())
    }

    fn is_playing(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_project(self, &project) else {
            return false;
        };
        self.transport_engine_for(&guid)
            .shared
            .play_state()
            .is_advancing()
    }

    fn is_recording(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_project(self, &project) else {
            return false;
        };
        matches!(
            self.transport_engine_for(&guid).shared.play_state(),
            PlayStateRepr::Recording
        )
    }

    fn get_tempo(&self, project: ProjectContext) -> f64 {
        let Some(guid) = resolve_project(self, &project) else {
            return 120.0;
        };
        self.transport_engine_for(&guid).shared.tempo_bpm()
    }

    fn set_tempo(&self, project: ProjectContext, bpm: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.transport_engine_for(&guid).shared.set_tempo_bpm(bpm);
        self.with_project_mut(&guid, |p| {
            p.transport.tempo = Tempo::from_bpm(bpm);
        })
    }

    fn toggle_loop(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let next = !bundle.shared.is_looping();
        bundle.shared.set_looping(next);
        self.with_project_mut(&guid, |p| {
            p.transport.looping = next;
        })
    }

    fn set_metronome(&self, project: ProjectContext, enabled: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.with_project_mut(&guid, |p| {
            p.transport.metronome = enabled;
        })?;
        // The transport pump doesn't watch this field — publish the
        // discrete event straight onto the subscriber path via the
        // engine bundle's state broadcaster equivalent: reuse track
        // hub? Transport events flow through pumps; emit by nudging
        // shared state is overkill — the bus bridge subscribes via
        // pumps, so push through the engine's event hook instead.
        self.transport_engine_for(&guid)
            .shared
            .set_metronome(enabled);
        Ok(())
    }

    fn metronome_enabled(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_project(self, &project) else {
            return false;
        };
        self.with_project(&guid, |p| p.transport.metronome)
            .unwrap_or(false)
    }

    fn is_looping(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_project(self, &project) else {
            return false;
        };
        self.transport_engine_for(&guid).shared.is_looping()
    }

    fn set_loop(&self, project: ProjectContext, enabled: bool) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.transport_engine_for(&guid).shared.set_looping(enabled);
        self.with_project_mut(&guid, |p| {
            p.transport.looping = enabled;
        })
    }

    fn get_time_selection(&self, project: ProjectContext) -> Option<LoopRegion> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| p.transport.time_selection.clone())
            .ok()
            .flatten()
    }

    fn set_time_selection(
        &self,
        project: ProjectContext,
        start_seconds: f64,
        end_seconds: f64,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        let clock = SampleClock::new(bundle.shared.sample_rate());
        let (lo, hi) = (
            start_seconds.min(end_seconds),
            start_seconds.max(end_seconds),
        );
        // Time selection acts as the engine loop region — REAPER pairs
        // them by default and we don't have a separate "loop points"
        // concept yet.
        bundle.shared.set_loop_region(Some(LoopRegionSamples {
            start: clock.seconds_to_samples(crate::transport_engine::InstantSeconds(lo)),
            end: clock.seconds_to_samples(crate::transport_engine::InstantSeconds(hi)),
        }));
        self.with_project_mut(&guid, |p| {
            p.transport.time_selection = Some(LoopRegion::new(lo, hi));
            p.transport.loop_region = Some(LoopRegion::new(lo, hi));
        })
    }

    fn clear_time_selection(&self, project: ProjectContext) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        self.transport_engine_for(&guid)
            .shared
            .set_loop_region(None);
        self.with_project_mut(&guid, |p| {
            p.transport.time_selection = None;
            p.transport.loop_region = None;
        })
    }

    fn get_playrate(&self, project: ProjectContext) -> f64 {
        let Some(guid) = resolve_project(self, &project) else {
            return 1.0;
        };
        self.transport_engine_for(&guid).shared.playrate()
    }

    fn set_playrate(&self, project: ProjectContext, rate: f64) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(not_found_proj)?;
        let bundle = self.transport_engine_for(&guid);
        bundle.shared.set_playrate(rate);
        let clamped = bundle.shared.playrate();
        self.with_project_mut(&guid, |p| {
            p.transport.playrate = clamped;
        })
    }

    fn get_time_signature(&self, project: ProjectContext) -> TimeSignature {
        let Some(guid) = resolve_project(self, &project) else {
            return TimeSignature::default();
        };
        self.with_project(&guid, |p| p.transport.time_signature)
            .unwrap_or_default()
    }
}
