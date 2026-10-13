//! The watch remote's session-domain wire DTOs.
//!
//! A tiny JSON projection of the setlist/transport/mixer surface served by
//! the engine's `/watch/v1` HTTP+SSE bridge (watchOS can't speak vox over
//! WebSocket; see `signal-guitar-proto::watch` for the rig-side twin).
//!
//! These shapes are the source of truth for the Swift side: the
//! `gen_watch_swift` example reflects them through facet and emits the
//! matching `Codable` structs into the watch app
//! (`apps/fasttrackstudio/watchos/`). Change a field here → re-run the
//! generator → Swift follows.

use facet::Facet;

/// One mixer track as the watch renders it.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct WatchTrack {
    /// Stable id used to address the track (mute/solo/volume commands).
    pub guid: String,
    pub name: String,
    pub index: u32,
    pub muted: bool,
    pub soloed: bool,
    /// Fader position (0 = −inf, 1 = 0 dB).
    pub volume: f32,
    /// Pan −1..1.
    pub pan: f32,
    pub is_folder: bool,
    /// 0RGB track color; 0 = unset.
    pub color: u32,
}

/// One chord of the watch's chord window (the current chord + the next 3).
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct WatchChord {
    /// Display symbol (e.g. "Gmaj7", "/D").
    pub symbol: String,
    /// Absolute measure (from song start) the chord lands on.
    pub measure: i32,
    /// Beat within the measure (0-based).
    pub beat: i32,
    /// True for the chord under the playhead (first entry of the window).
    pub is_current: bool,
}

/// The watch's session page state: setlist cursor + transport + the chord
/// window + the mixer — refreshed over `/watch/v1/session/events`.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct WatchSessionState {
    /// Setlist song names, in order.
    pub songs: Vec<String>,
    /// Index of the current song (−1 = none).
    pub song_index: i32,
    /// Current song's section names, in order.
    pub sections: Vec<String>,
    /// Index of the current section (−1 = none).
    pub section_index: i32,
    pub is_playing: bool,
    /// 0..1 through the current song.
    pub song_progress: f32,
    /// 0..1 through the current section.
    pub section_progress: f32,
    /// The chord window: current chord first, then the next three.
    pub chords: Vec<WatchChord>,
    /// The current chart section's lyric line ("" when the chart has none).
    pub lyric_line: String,
    /// The mixer, in track order.
    pub tracks: Vec<WatchTrack>,
    /// Bumped on every state build (client-side dedup aid).
    pub revision: u64,
}
