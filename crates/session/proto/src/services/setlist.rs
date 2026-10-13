//! Setlist control service — the main live surface: queries, navigation,
//! transport, loop, recording, build/refresh, and the event streams.

use super::error::SessionServiceError;
use crate::setlist::{ActiveIndices, Setlist};
use crate::song::{Section, Song, SongChartHydration};
use crate::{SectionId, SongId};
use daw_proto::MusicalPosition;
use facet::Facet;
use vox::Tx;

/// Measure information for RPC
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct MeasureInfo {
    /// Measure number
    pub measure: i32,
    /// Time in seconds
    pub time_seconds: f64,
    /// Time signature numerator
    pub time_sig_numerator: i32,
    /// Time signature denominator
    pub time_sig_denominator: i32,
}

/// Transport state for a specific song
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SongTransportState {
    /// Song index in the setlist
    pub song_index: usize,
    /// Current position with time, musical, and MIDI representations
    /// The musical position comes from REAPER's tempo map and properly
    /// accounts for tempo and time signature changes throughout the project
    pub position: daw_proto::Position,
    /// Position as progress within the song (0.0 - 1.0)
    pub progress: f64,
    /// Current section index (if in a section)
    pub section_index: Option<usize>,
    /// Progress within current section (0.0 - 1.0)
    pub section_progress: Option<f64>,
    /// Whether this song's project is currently playing
    pub is_playing: bool,
    /// Whether this song's project is looping
    pub is_looping: bool,
    /// Loop region start/end in seconds (relative to song start)
    /// Only present when looping is enabled and loop points are set
    pub loop_region: Option<daw_proto::LoopRegion>,
    /// Current tempo in BPM
    pub bpm: f64,
    /// Time signature numerator
    pub time_sig_num: u32,
    /// Time signature denominator
    pub time_sig_denom: u32,
}

impl Default for SongTransportState {
    fn default() -> Self {
        Self {
            song_index: 0,
            position: daw_proto::Position::default(),
            progress: 0.0,
            section_index: None,
            section_progress: None,
            is_playing: false,
            is_looping: false,
            loop_region: None,
            bpm: 120.0,
            time_sig_num: 4,
            time_sig_denom: 4,
        }
    }
}

/// Events emitted by the setlist service
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Facet)]
pub enum SetlistEvent {
    /// Setlist was changed/rebuilt
    SetlistChanged(Setlist),
    /// A single song entry was hydrated/updated in-place
    SongHydrated {
        song_id: SongId,
        index: usize,
        song: Song,
    },
    /// Chart/chord payload for a song was hydrated/updated.
    SongChartHydrated {
        song_id: SongId,
        index: usize,
        chart: SongChartHydration,
    },
    /// Transport state updated for one or more songs
    /// This is sent at 60Hz and includes state for ALL songs that have active projects
    TransportUpdate(Vec<SongTransportState>),
    /// Entered a new song
    SongEntered {
        song_id: SongId,
        index: usize,
        song: Song,
    },
    /// Exited a song
    SongExited { song_id: SongId, index: usize },
    /// Entered a new section
    SectionEntered {
        song_id: SongId,
        song_index: usize,
        section_id: SectionId,
        section_index: usize,
        section: Section,
    },
    /// Exited a section
    SectionExited {
        song_id: SongId,
        song_index: usize,
        section_id: SectionId,
        section_index: usize,
    },
    /// Position changed (legacy - prefer `TransportUpdate`)
    PositionChanged {
        seconds: f64,
        indices: ActiveIndices,
    },
    /// A take was recorded and is waiting to be judged.
    ///
    /// What puts the review panel on every tablet in the room at the
    /// same moment — which is the moment it is useful, and about
    /// thirty seconds long.
    TakeRecorded {
        song_id: SongId,
        index: usize,
        pass: crate::review::Pass,
    },
    /// Somebody rated a take, or marked a stretch of one.
    ///
    /// Carried to every other tablet so consensus forms where people
    /// can see it — the drummer's ✕ on the take the guitarist just
    /// called perfect is the useful part, and it is useful immediately.
    TakeMarked {
        song_id: SongId,
        index: usize,
        pass: u32,
        mark: crate::review::Mark,
    },
}

/// Service for building and retrieving song information
///
/// This service extracts song structure from DAW projects by analyzing
pub mod setlist_service {
    use super::{
        ActiveIndices, AudioLatencyInfo, MeasureInfo, MusicalPosition, Section,
        SessionServiceError, Setlist, SetlistEvent, Song, SongChartHydration, Tx,
    };

    #[architect::rpc]
    pub trait SetlistService {
        // =========================================================================
        // Query Methods
        // =========================================================================

        /// Get the full setlist
        ///
        /// # Errors
        ///
        /// Returns an error if the setlist has not been built yet.
        fn setlist(&self) -> Result<Setlist, SessionServiceError>;

        /// Get all songs in the setlist
        ///
        /// # Errors
        ///
        /// Returns an error if the setlist has not been built yet.
        fn songs(&self) -> Result<Vec<Song>, SessionServiceError>;

        /// Get a specific song by index
        ///
        /// # Errors
        ///
        /// Returns an error if `index` is out of range.
        fn song(&self, index: usize) -> Result<Song, SessionServiceError>;

        /// Get all sections for a specific song
        ///
        /// # Errors
        ///
        /// Returns an error if `song_index` is out of range.
        fn sections(&self, song_index: usize) -> Result<Vec<Section>, SessionServiceError>;

        /// Get a specific section by song and section index
        ///
        /// # Errors
        ///
        /// Returns an error if `song_index` or `section_index` is out of range.
        fn section(
            &self,
            song_index: usize,
            section_index: usize,
        ) -> Result<Section, SessionServiceError>;

        /// Get measure information for a song
        ///
        /// # Errors
        ///
        /// Returns an error if `song_index` is out of range.
        fn measures(&self, song_index: usize) -> Result<Vec<MeasureInfo>, SessionServiceError>;

        /// Get the hydrated chart payload (keyflow chart text + detected
        /// chords) for a song, if one exists.
        ///
        /// Charts are stripped from `Setlist`/`Song` structural payloads and
        /// stream as `SetlistEvent::SongChartHydrated` deltas; a remote that
        /// connects after hydration already ran uses this to backfill its
        /// chart cache (the browser chart pane's initial snapshot).
        async fn song_chart(
            &self,
            song_index: usize,
        ) -> Result<Option<SongChartHydration>, SessionServiceError>;

        // =========================================================================
        // Active State Queries
        // =========================================================================

        /// Get the currently active song
        ///
        /// # Errors
        ///
        /// Returns an error if no song is currently active.
        fn active_song(&self) -> Result<Song, SessionServiceError>;

        /// Get the currently active section
        ///
        /// # Errors
        ///
        /// Returns an error if no section is currently active.
        fn active_section(&self) -> Result<Section, SessionServiceError>;

        /// Get the song at a specific position in seconds
        ///
        /// # Errors
        ///
        /// Returns an error if `seconds` falls outside the setlist.
        fn song_at(&self, seconds: f64) -> Result<Song, SessionServiceError>;

        /// Get the section at a specific position in seconds
        ///
        /// # Errors
        ///
        /// Returns an error if `seconds` falls outside any section.
        fn section_at(&self, seconds: f64) -> Result<Section, SessionServiceError>;

        // =========================================================================
        // Navigation Commands
        // =========================================================================

        /// Navigate to a specific song by index
        async fn go_to_song(&self, index: usize) -> Result<(), SessionServiceError>;

        /// Navigate to the next song in the setlist
        async fn next_song(&self) -> Result<(), SessionServiceError>;

        /// Navigate to the previous song in the setlist
        async fn previous_song(&self) -> Result<(), SessionServiceError>;

        /// Navigate to a specific section within the current song
        async fn go_to_section(&self, index: usize) -> Result<(), SessionServiceError>;

        /// Navigate to the next section in the current song
        async fn next_section(&self) -> Result<(), SessionServiceError>;

        /// Navigate to the previous section in the current song
        async fn previous_section(&self) -> Result<(), SessionServiceError>;

        /// Seek to a specific time in the current song (song-relative seconds)
        async fn seek_to(&self, seconds: f64) -> Result<(), SessionServiceError>;

        /// Seek to a specific time within a song (absolute seconds from project start)
        ///
        /// This switches to the correct project and seeks to the specified time.
        async fn seek_to_time(
            &self,
            song_index: usize,
            seconds: f64,
        ) -> Result<(), SessionServiceError>;

        /// Seek to a specific song by index (goes to song start)
        async fn seek_to_song(&self, song_index: usize) -> Result<(), SessionServiceError>;

        /// Seek to a specific section within a song
        async fn seek_to_section(
            &self,
            song_index: usize,
            section_index: usize,
        ) -> Result<(), SessionServiceError>;

        /// Seek to a musical position within a song
        ///
        /// The position is relative to the song's start (measure 0 = first measure of the song).
        async fn seek_to_musical_position(
            &self,
            song_index: usize,
            position: MusicalPosition,
        ) -> Result<(), SessionServiceError>;

        /// Seek to a specific measure within a song (0-indexed from song start)
        ///
        /// This switches to the correct project and seeks to the start of the specified measure.
        async fn goto_measure(
            &self,
            song_index: usize,
            measure: i32,
        ) -> Result<(), SessionServiceError>;

        // =========================================================================
        // Playback Commands
        // =========================================================================

        /// Toggle playback (play/pause)
        async fn toggle_playback(&self) -> Result<(), SessionServiceError>;

        /// Start playback
        async fn play(&self) -> Result<(), SessionServiceError>;

        /// Pause playback
        async fn pause(&self) -> Result<(), SessionServiceError>;

        /// Stop playback
        async fn stop(&self) -> Result<(), SessionServiceError>;

        // =========================================================================
        // Loop Control
        // =========================================================================

        /// Toggle looping for the current song
        async fn toggle_song_loop(&self) -> Result<(), SessionServiceError>;

        /// Toggle looping for the current section
        async fn toggle_section_loop(&self) -> Result<(), SessionServiceError>;

        /// Set a custom loop region
        async fn set_loop_region(
            &self,
            start_seconds: f64,
            end_seconds: f64,
        ) -> Result<(), SessionServiceError>;

        /// Clear any active loop
        async fn clear_loop(&self) -> Result<(), SessionServiceError>;

        // =========================================================================
        // Recording — targets the active song's project (song-specific)
        // =========================================================================

        /// Start recording into the active song's project.
        async fn record(&self) -> Result<(), SessionServiceError>;

        /// Stop recording in the active song's project.
        async fn stop_recording(&self) -> Result<(), SessionServiceError>;

        /// Toggle recording in the active song's project.
        async fn toggle_recording(&self) -> Result<(), SessionServiceError>;

        /// Arm or disarm the selected tracks in the active song's project for
        /// recording. (Selected, not all — arming every track would capture
        /// silence onto click/guide/reference tracks.)
        async fn set_song_record_arm(&self, armed: bool) -> Result<(), SessionServiceError>;

        // =========================================================================
        // Take review
        // =========================================================================

        /// Every pass at a song, and what everyone thought of them.
        ///
        /// Read on arrival: a tablet that joins halfway through the
        /// evening should show the takes already rated, not an empty
        /// list that fills in as new ones happen.
        /// A list rather than a `Review`, which is the index the
        /// caller builds over it — `Review::from_passes` puts one back
        /// together on the other side.
        async fn song_review(
            &self,
            song_index: usize,
        ) -> Result<Vec<crate::review::Pass>, SessionServiceError>;

        /// Rate a take, or mark a stretch of one.
        ///
        /// The mark carries who made it, so the service does not have
        /// to know which tablet asked — and two people rating at once
        /// are two marks rather than a race.
        async fn mark_take(
            &self,
            song_index: usize,
            pass: u32,
            mark: crate::review::Mark,
        ) -> Result<(), SessionServiceError>;

        // =========================================================================
        // Build/Refresh
        // =========================================================================

        /// Build setlist from all currently open DAW projects
        ///
        /// Scans all open projects, extracts song structures from each,
        /// and assembles them into a complete setlist.
        async fn build_from_open_projects(&self) -> Result<(), SessionServiceError>;

        /// Refresh the setlist
        ///
        /// Rebuilds the setlist from open projects, updating any changes
        /// to song structures or project state.
        async fn refresh(&self) -> Result<(), SessionServiceError>;

        /// Stamp demo markers and regions into the current REAPER project
        ///
        /// Creates SONGSTART, SONGEND, =END, COUNT-IN markers and section regions
        /// (Intro, Verse, Chorus, Bridge, etc.) in the currently active project.
        /// After stamping, call `build_from_open_projects()` to build the setlist.
        async fn load_demo_setlist(&self) -> Result<(), SessionServiceError>;

        /// Generate a combined setlist project from all open song projects.
        ///
        /// Scans open projects, skips any that are already combined setlists
        /// (identified by `ExtState` `FTS/is_combined_setlist`), builds songs,
        /// generates a combined RPP with configurable gap between songs,
        /// and opens it as a new project tab.
        ///
        /// `gap_measures` — number of empty measures between songs (default: 2)
        async fn generate_combined_setlist(
            &self,
            gap_measures: u32,
        ) -> Result<String, SessionServiceError>;

        // =========================================================================
        // Subscriptions
        // =========================================================================

        /// Every setlist change, as it happens — remotes render from this
        /// stream instead of polling (architect `#[subscribe]`; works on
        /// wasm over `WsLink`, replacing the old reverse-dispatch
        /// `WebClientService` push).
        #[subscribe]
        fn events(&self) -> SetlistEvent;

        /// Active song/section cursor changes, as they happen.
        #[subscribe]
        fn active_indices(&self) -> ActiveIndices;

        // =========================================================================
        // Audio Engine (proxy)
        // =========================================================================

        /// Get audio output latency in seconds
        ///
        /// This proxies to the DAW's `AudioEngineService`. The latency represents
        /// the delay between when audio is rendered and when it reaches the speakers.
        /// Clients can use this to compensate visual display during playback.
        ///
        /// Returns 0.0 if the audio engine is not running or unavailable.
        async fn get_audio_latency(&self) -> Result<f64, SessionServiceError>;

        /// Get complete audio latency information
        ///
        /// Returns input/output latency, sample rate, and audio engine state.
        /// Useful for latency display badges.
        async fn get_audio_latency_info(&self) -> Result<AudioLatencyInfo, SessionServiceError>;
    }
}

pub use setlist_service::{
    Service as SetlistServiceLayer, SetlistService, SetlistServiceClient,
    SetlistServiceRpcDispatcher as SetlistServiceDispatcher, layer as setlist_service_layer,
    serve as serve_setlist_service, setlist_service_rpc_service_descriptor,
    setlist_service_rpc_service_descriptor as setlist_service_service_descriptor,
};

/// Complete audio latency information for display
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct AudioLatencyInfo {
    /// Input latency in samples
    pub input_samples: u32,
    /// Output latency in samples
    pub output_samples: u32,
    /// Output latency in seconds
    pub output_seconds: f64,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Whether audio engine is running
    pub is_running: bool,
}

// SelfRef compatibility: SetlistEvent has no lifetime parameters, so Ref<'a> = Self.
#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for SetlistEvent {
    type Ref<'a> = Self;
}
