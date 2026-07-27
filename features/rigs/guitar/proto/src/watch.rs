//! The watch remote's wire DTOs — a tiny JSON projection of
//! [`PerformanceModel`] served by the engine's
//! `/watch/v1` HTTP+SSE bridge (watchOS can't speak vox over WebSocket;
//! Apple TN3135 forbids it outside audio-streaming sessions).
//!
//! These shapes are the source of truth for the Swift side: the
//! `gen_watch_swift` example reflects them through facet and emits the
//! matching `Codable` structs into the watch app
//! (`apps/fasttrackstudio/watchos/`). Change a field here → re-run the
//! generator → Swift follows.

use facet::Facet;

use crate::{PerfStack, PerformanceModel};

/// One footswitch stack (scene folder) as the watch renders it.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct WatchStack {
    /// Folder / footswitch name (e.g. "Clean", "Lead") — also drives the
    /// tile color, mirroring the web perform grid's `folder_color`.
    pub name: String,
    /// Display name of the patch at the rotation cursor.
    pub current_patch: String,
    /// Rotation cursor (0-based) — the secondary ring segment.
    pub position: u32,
    /// Patches in the rotation — the secondary ring segment count.
    pub patch_count: u32,
    /// Chain preloaded and ready (loading dot when false).
    pub available: bool,
    /// Holds the currently-active patch (highlight ring).
    pub is_active: bool,
}

/// The watch remote's whole world, refreshed over `/watch/v1/events`.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct WatchState {
    pub profile_name: String,
    pub stacks: Vec<WatchStack>,
    /// Global time/FX bypass engaged (hold-layer switch).
    pub fx_bypass: bool,
    /// Boost level in dB (`0.0` = off; cycles +1 → +2 → +3 → −1).
    pub boost_db: f32,
    /// Current tempo — drives the tap-tempo blink.
    pub tempo_bpm: u32,
    /// Fullscreen tuner overlay engaged (synced across remotes).
    pub tuner_visible: bool,
    /// Current setlist song name, if any (context line).
    pub song: String,
    /// Monotonic state version (bumps on every mutation).
    pub revision: u64,
}

impl From<&PerformanceModel> for WatchState {
    fn from(p: &PerformanceModel) -> Self {
        let stack = |s: &PerfStack| WatchStack {
            name: s.name.clone(),
            current_patch: s.current_patch.clone(),
            position: s.position,
            patch_count: s.patch_count,
            available: s.available,
            is_active: s.is_active,
        };
        Self {
            profile_name: p.profile_name.clone(),
            stacks: p.stacks.iter().map(stack).collect(),
            fx_bypass: p.fx_bypass,
            boost_db: p.boost_db,
            tempo_bpm: p.tempo_bpm,
            tuner_visible: p.tuner_visible,
            song: p
                .songs
                .get(p.song_index as usize)
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            revision: p.revision,
        }
    }
}
