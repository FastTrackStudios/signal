//! Sync, backend-generic **handles** over the daw service traits — the
//! ergonomic surface for in-process consumers.
//!
//! The async client side already has this shape (`daw_control::Project` /
//! `TrackHandle` over vox callers); this module is its synchronous twin,
//! generic over *any* backend implementing the `#[architect::rpc]` service
//! traits — `Standalone`, the REAPER in-process services, a test mock. That
//! is the architect pattern: program to the trait bundle, and the same
//! calling code runs over every backend.
//!
//! What it replaces (the dialect every in-process consumer used to write):
//!
//! ```text
//! <Standalone as Tracks>::set_volume(&daw, ProjectContext::Current,
//!     TrackRef::guid(&guid), 0.9)?;                       // UFCS: four traits
//!                                                          // have set_volume
//! let idx = <Standalone as FxChains>::add(&daw, ctx.clone(), "slot")?;
//! let fx  = <Standalone as FxChains>::get(&daw, ctx, idx)…  // the add+get dance
//! ```
//!
//! with:
//!
//! ```text
//! use daw_proto::handle::DawHandle as _;
//!
//! let project = daw.current();                 // ProjectHandle
//! let track = project.add_track("Keys")?;      // TrackHandle
//! track.set_volume(0.9)?;
//! track.mute(true)?;
//! let slot = track.add_fx_slot("keys")?;       // add + get, one call
//! track.send_to(bus.guid()).post_fx().replace_master_send().apply()?;
//! ```
//!
//! Handles are cheap (a backend ref + guid) and deliberately thin: every
//! method is one service call, no caching, no state — the ergonomics live in
//! naming, defaulted contexts, and the fused multi-call rituals (fx-slot
//! reservation, bus routing, folder-depth bookkeeping via [`TrackTree`]).

use crate::error::{DawError, DawResult};
use crate::routing::Routing;
use crate::track::Tracks;
use crate::track::{TracksDirectExt as _, TracksProjectScope, TracksTrackScope};
use crate::{
    Fx, FxChainContext, ProjectContext, RecordInput, RouteLocation, RouteRef, SendMode, Track,
    TrackRef,
};

impl<D: Tracks + ?Sized> Clone for ProjectHandle<'_, D> {
    fn clone(&self) -> Self {
        Self {
            daw: self.daw,
            scope: self.scope.clone(),
        }
    }
}

impl<D: Tracks + ?Sized> Clone for TrackHandle<'_, D> {
    fn clone(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            guid: self.guid.clone(),
        }
    }
}

/// Entry point: `daw.current()` / `daw.project(guid)` for any backend
/// implementing [`Tracks`]. Import as `use daw_proto::handle::DawHandle as _;`.
pub trait DawHandle: Tracks + Sized {
    /// A handle on the current project.
    fn current(&self) -> ProjectHandle<'_, Self> {
        ProjectHandle {
            daw: self,
            scope: self.tracks_direct().project(ProjectContext::Current),
        }
    }

    /// A handle on a specific project.
    fn project(&self, guid: impl Into<String>) -> ProjectHandle<'_, Self> {
        let ctx = ProjectContext::Project(guid.into());
        ProjectHandle {
            daw: self,
            scope: self.tracks_direct().project(ctx),
        }
    }
}

impl<D: Tracks + Sized> DawHandle for D {}

/// A project, bound to its backend — makes tracks and scopes every child
/// handle to this project's context.
pub struct ProjectHandle<'d, D: Tracks + ?Sized> {
    daw: &'d D,
    /// The generated `Tracks` project scope — [`std::ops::Deref`] target, so
    /// every `Tracks` method with a leading `ProjectContext` is available
    /// here directly (`add`, `all`, `remove_all`, …).
    scope: TracksProjectScope<'d, D>,
}

impl<'d, D: Tracks + ?Sized> std::ops::Deref for ProjectHandle<'d, D> {
    type Target = TracksProjectScope<'d, D>;
    fn deref(&self) -> &Self::Target {
        &self.scope
    }
}

impl<'d, D: Tracks + ?Sized> ProjectHandle<'d, D> {
    pub fn context(&self) -> ProjectContext {
        self.scope.project().clone()
    }

    /// Add a track (appended); returns its handle. (The bare `Tracks::add`
    /// with an explicit index is available through deref.)
    pub fn add_track(&self, name: &str) -> DawResult<TrackHandle<'d, D>> {
        let guid = self.scope.add(name, None)?;
        Ok(self.track(guid))
    }

    /// A handle on an existing track by guid.
    pub fn track(&self, guid: impl Into<String>) -> TrackHandle<'d, D> {
        let guid = guid.into();
        TrackHandle {
            scope: self.scope.track(TrackRef::Guid(guid.clone())),
            guid,
        }
    }

    /// Every track, in mixer order (alias for the scope's `all`).
    pub fn tracks(&self) -> Vec<Track> {
        self.scope.all()
    }

    /// The master track's handle, if the backend exposes one.
    pub fn master(&self) -> Option<TrackHandle<'d, D>> {
        self.scope.master().map(|t| self.track(t.guid))
    }

    /// A [`TrackTree`] builder over this project — folder hierarchies with
    /// the REAPER depth convention handled for you.
    pub fn tree(&self) -> TrackTree<'d, D> {
        TrackTree {
            project: self.clone(),
            open: 0,
            last: None,
        }
    }
}

/// One track, bound to its backend + project. Every mixer/routing/FX op is a
/// single unambiguous method — no UFCS, no context/ref wrapping.
pub struct TrackHandle<'d, D: Tracks + ?Sized> {
    /// The generated `Tracks` track scope — [`std::ops::Deref`] target, so
    /// every `Tracks` method with a leading `(ProjectContext, TrackRef)` is
    /// available here directly: `set_volume`, `set_pan`, `set_muted`,
    /// `set_soloed`, `rename`, `set_folder_depth`, `remove`, `get`, ….
    scope: TracksTrackScope<'d, D>,
    guid: String,
}

impl<'d, D: Tracks + ?Sized> std::ops::Deref for TrackHandle<'d, D> {
    type Target = TracksTrackScope<'d, D>;
    fn deref(&self) -> &Self::Target {
        &self.scope
    }
}

impl<'d, D: Tracks + ?Sized> TrackHandle<'d, D> {
    pub fn guid(&self) -> &str {
        &self.guid
    }

    fn ctx(&self) -> ProjectContext {
        self.scope.project().clone()
    }

    /// Full track state, if the track still exists (alias for `get`).
    pub fn info(&self) -> Option<Track> {
        self.scope.get()
    }

    /// Arm the track to record/monitor hardware audio input `channel` — what
    /// makes a live engine open an input stream and feed this track's bus.
    ///
    /// All three: the record input, armed, and monitoring it. A live engine
    /// feeds a track its input only when it is armed and monitoring (a
    /// record input alone is where a take would come from); setting the
    /// input alone left a guitar rig's track silent while the interface
    /// delivered the guitar.
    pub fn arm_audio_input(&self, channel: u32) -> DawResult<()> {
        self.scope.set_record_input(RecordInput::Audio { channel })?;
        self.scope.set_armed(true)?;
        self.scope.set_input_monitor(crate::track::InputMonitoringMode::Normal)
    }
}

impl<'d, D: crate::FxChains + Tracks + ?Sized> TrackHandle<'d, D> {
    /// The generated `FxChains` scope over this track's playback chain —
    /// `add`, `get`, `set_enabled`, `move_to`, … with the chain context
    /// elided.
    pub fn fx(&self) -> crate::fx_chains::FxChainsChainScope<'d, D> {
        use crate::fx_chains::FxChainsDirectExt as _;
        self.scope
            .backend()
            .fx_chains_direct()
            .chain(FxChainContext::track(self.guid.clone()))
    }

    /// Reserve one FX slot (the add + guid-fetch dance, fused). The returned
    /// handle's guid is constant for the project's life — swapping the
    /// instance behind it never rebuilds a renderer snapshot.
    pub fn add_fx_slot(&self, label: &str) -> DawResult<FxSlotHandle<'d, D>> {
        let fx_scope = self.fx();
        let index = fx_scope.add(label)?;
        let fx = fx_scope
            .get(index)
            .ok_or_else(|| DawError::Internal(format!("fx slot {label:?} vanished after add")))?;
        Ok(FxSlotHandle {
            daw: self.scope.backend(),
            track_guid: self.guid.clone(),
            index,
            fx,
        })
    }

    /// Enable/bypass the FX at `index` (bypass = `enabled(false)`).
    pub fn set_fx_enabled(&self, index: u32, enabled: bool) -> DawResult<()> {
        self.fx().set_enabled(index, enabled)
    }
}

impl<'d, D: Routing + Tracks + ?Sized> TrackHandle<'d, D> {
    /// Start a send from this track — finish with
    /// [`SendBuilder::apply`]. `dest` is the destination track's guid.
    pub fn send_to(&self, dest: impl Into<String>) -> SendBuilder<'d, '_, D> {
        SendBuilder {
            track: self,
            dest: dest.into(),
            mode: None,
            volume: None,
            replace_master: false,
        }
    }
}

/// A reserved FX slot on a track.
pub struct FxSlotHandle<'d, D: ?Sized> {
    daw: &'d D,
    track_guid: String,
    index: u32,
    fx: Fx,
}

impl<'d, D: ?Sized> FxSlotHandle<'d, D> {
    /// The slot's guid (constant for the project's life).
    pub fn guid(&self) -> &str {
        &self.fx.guid
    }

    pub fn into_guid(self) -> String {
        self.fx.guid
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn track_guid(&self) -> &str {
        &self.track_guid
    }
}

impl<'d, D: crate::FxChains + ?Sized> FxSlotHandle<'d, D> {
    /// Enable/bypass this slot.
    pub fn set_enabled(&self, enabled: bool) -> DawResult<()> {
        self.daw.set_enabled(
            FxChainContext::track(self.track_guid.clone()),
            self.index,
            enabled,
        )
    }
}

/// Fluent send construction — fuses the add-send / set-mode /
/// detach-master-send / set-volume ritual into one readable chain:
///
/// ```text
/// mic.send_to(bus.guid()).post_fx().replace_master_send().apply()?;
/// ```
#[must_use = "call .apply() to create the send"]
pub struct SendBuilder<'d, 't, D: Tracks + ?Sized> {
    track: &'t TrackHandle<'d, D>,
    dest: String,
    mode: Option<SendMode>,
    volume: Option<f64>,
    replace_master: bool,
}

impl<'d, 't, D: Routing + Tracks + ?Sized> SendBuilder<'d, 't, D> {
    /// Tap post-FX, post-fader (the bus-mic convention).
    pub fn post_fx(mut self) -> Self {
        self.mode = Some(SendMode::PostFx);
        self
    }

    /// Explicit send mode.
    pub fn mode(mut self, mode: SendMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Initial send level (linear).
    pub fn volume(mut self, volume: f64) -> Self {
        self.volume = Some(volume);
        self
    }

    /// Disable the track's parent/master send — the track's ONLY output
    /// becomes this send (bus-mic routing).
    pub fn replace_master_send(mut self) -> Self {
        self.replace_master = true;
        self
    }

    /// Create the send; returns its route index on the source track.
    pub fn apply(self) -> DawResult<u32> {
        let ctx = self.track.ctx();
        let src = TrackRef::Guid(self.track.guid.clone());
        let daw = self.track.scope.backend();
        let idx = daw
            .add_send(ctx.clone(), src.clone(), TrackRef::Guid(self.dest.clone()))
            .ok_or_else(|| {
                DawError::Internal(format!(
                    "add_send {} → {} failed",
                    self.track.guid, self.dest
                ))
            })?;
        if let Some(mode) = self.mode {
            daw.set_send_mode(ctx.clone(), src.clone(), RouteRef::Index(idx), mode)?;
        }
        if let Some(volume) = self.volume {
            <D as Routing>::set_volume(
                daw,
                ctx.clone(),
                RouteLocation::send(src.clone(), RouteRef::Index(idx)),
                volume,
            )?;
        }
        if self.replace_master {
            daw.set_parent_send_enabled(ctx, src, false)?;
        }
        Ok(idx)
    }
}

/// Folder-hierarchy builder over the REAPER depth convention (positive depth
/// opens a folder; a negative depth on the **last child** closes levels).
/// Callers speak in `folder` / `track` / `end`; the builder owns the depth
/// bookkeeping that is otherwise fiddly to hand-roll (`-1` vs `-2` closes).
///
/// ```text
/// let mut tree = daw.current().tree();
/// let rig = tree.folder("Worship")?;      // opens
/// tree.folder("Keys")?;                    // nested folder
/// tree.track("Piano")?;
/// tree.track("Pad")?;
/// tree.end()?;                             // closes "Keys" on "Pad"
/// tree.folder("Organ")?;
/// tree.track("Organ")?;
/// tree.finish()?;                          // closes everything still open
/// ```
pub struct TrackTree<'d, D: Tracks + ?Sized> {
    project: ProjectHandle<'d, D>,
    /// Folders currently open (unclosed positive depths).
    open: u32,
    /// The most recently added track + the closes accumulated on it.
    last: Option<(TrackHandle<'d, D>, i32)>,
}

impl<'d, D: Tracks + ?Sized> TrackTree<'d, D> {
    /// Flush the pending depth adjustment on the previous track.
    fn flush(&mut self) -> DawResult<()> {
        if let Some((track, depth)) = self.last.take() {
            if depth != 0 {
                track.set_folder_depth(depth)?;
            }
        }
        Ok(())
    }

    /// Open a folder track (children follow until [`end`](Self::end)).
    pub fn folder(&mut self, name: &str) -> DawResult<TrackHandle<'d, D>> {
        self.flush()?;
        let track = self.project.add_track(name)?;
        self.open += 1;
        self.last = Some((track.clone(), 1));
        Ok(track)
    }

    /// Add a plain track at the current level.
    pub fn track(&mut self, name: &str) -> DawResult<TrackHandle<'d, D>> {
        self.flush()?;
        let track = self.project.add_track(name)?;
        self.last = Some((track.clone(), 0));
        Ok(track)
    }

    /// Close the innermost open folder (the close lands on the most recent
    /// track, per the depth convention).
    pub fn end(&mut self) -> DawResult<()> {
        if self.open == 0 {
            return Err(DawError::Internal(
                "TrackTree::end with no open folder".into(),
            ));
        }
        let Some((_, depth)) = self.last.as_mut() else {
            return Err(DawError::Internal("TrackTree::end before any track".into()));
        };
        *depth -= 1;
        self.open -= 1;
        Ok(())
    }

    /// Close every folder still open and flush. Call exactly once, last.
    pub fn finish(mut self) -> DawResult<()> {
        while self.open > 0 {
            self.end()?;
        }
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    // The handles are pure delegation; behavior tests live with the
    // backends (daw-standalone's Tracks tests, signal-sampler's lane-layout
    // test drives TrackTree through a real Standalone).
}
