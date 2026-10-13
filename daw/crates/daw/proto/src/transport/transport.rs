//! Transport state and service
//!
//! This module defines the transport state representation and the TransportService trait
//! for controlling DAW playback, recording, and navigation.

use crate::primitives::{Position, Tempo, TimeSignature};
use crate::transport::error::TransportError;
use facet::Facet;

/// Current playback state
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Default, Facet, Debug)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
    Recording,
}

/// Recording mode
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Default, Facet, Debug)]
pub enum RecordMode {
    #[default]
    Normal,
    TimeSelection,
    Item,
}

/// Loop region (start and end positions in seconds)
#[derive(Clone, Debug, Default, Facet, PartialEq)]
pub struct LoopRegion {
    /// Loop start position in seconds
    pub start_seconds: f64,
    /// Loop end position in seconds
    pub end_seconds: f64,
}

impl LoopRegion {
    /// Create a new loop region
    pub fn new(start_seconds: f64, end_seconds: f64) -> Self {
        Self {
            start_seconds,
            end_seconds,
        }
    }

    /// Get loop duration in seconds
    pub fn duration(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    /// Check if the loop region is valid (end > start)
    pub fn is_valid(&self) -> bool {
        self.end_seconds > self.start_seconds
    }
}

/// Complete transport state
#[derive(Clone, Debug, Facet)]
pub struct Transport {
    pub play_state: PlayState,
    pub record_mode: RecordMode,
    pub looping: bool,
    /// Metronome / click enabled.
    pub metronome: bool,
    /// Loop region (start/end positions). Only meaningful when looping is true.
    pub loop_region: Option<LoopRegion>,
    /// REAPER-style time selection range, independent from loop points.
    pub time_selection: Option<LoopRegion>,
    pub tempo: Tempo,
    pub playrate: f64,
    pub time_signature: TimeSignature,
    pub playhead_position: Position,
    pub edit_position: Position,
}

impl Transport {
    /// Create a new transport state with default values
    pub fn new() -> Self {
        Self {
            play_state: PlayState::default(),
            record_mode: RecordMode::default(),
            looping: false,
            metronome: false,
            loop_region: None,
            time_selection: None,
            tempo: Tempo::default(),
            playrate: 1.0,
            time_signature: TimeSignature::default(),
            playhead_position: Position::start(),
            edit_position: Position::start(),
        }
    }

    /// Check if transport is currently playing or recording
    pub fn is_playing(&self) -> bool {
        matches!(self.play_state, PlayState::Playing | PlayState::Recording)
    }

    /// Check if transport is currently recording
    pub fn is_recording(&self) -> bool {
        matches!(self.play_state, PlayState::Recording)
    }

    /// Check if transport is paused
    pub fn is_paused(&self) -> bool {
        matches!(self.play_state, PlayState::Paused)
    }

    /// Check if transport is stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self.play_state, PlayState::Stopped)
    }

    /// Get effective BPM (tempo * playrate)
    pub fn effective_bpm(&self) -> f64 {
        self.tempo.bpm() * self.playrate
    }

    /// Set the tempo
    ///
    /// Note: Tempo validation happens at construction via `Tempo::from_bpm()` or `Tempo::try_from_bpm()`
    pub fn set_tempo(&mut self, tempo: Tempo) -> Result<(), TransportError> {
        self.tempo = tempo;
        Ok(())
    }

    /// Reset transport to initial stopped state
    pub fn reset(&mut self) {
        self.play_state = PlayState::Stopped;
        self.playhead_position = Position::start();
        self.edit_position = Position::start();
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

// Append AllProjectsTransport + ProjectTransportState at end for back-compat (used by daw-control facade subscribe_all_projects)

// =========================================================================
// All-projects transport snapshot — wire types kept for the daw-control
// facade `subscribe_all_projects` helper even though streaming methods
// retired with the architect::rpc port (sibling-trait territory if
// revived). They're plain data; safe to keep around.
// =========================================================================

/// Transport state update for all projects.
///
/// Contains a list of (project_guid, transport_state) pairs for all
/// projects whose state changed since the last update.
#[derive(Clone, Debug, Facet)]
pub struct AllProjectsTransport {
    pub projects: Vec<ProjectTransportState>,
}

/// Transport state for a specific project.
#[derive(Clone, Debug, Facet)]
pub struct ProjectTransportState {
    pub project_guid: String,
    pub transport: Transport,
}
