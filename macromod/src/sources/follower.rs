//! Envelope follower modulation source configuration.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Input source for the envelope follower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum FollowerInput {
    /// Follows the track's audio level.
    #[default]
    TrackAudio,
    /// Follows sidechain input.
    Sidechain,
}

/// Configuration for an envelope follower modulation source.
///
/// Tracks audio dynamics and converts them into a modulation signal
/// using attack/release smoothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FollowerConfig {
    /// Attack time in milliseconds (how fast the follower responds to rising levels).
    pub attack_ms: f32,
    /// Release time in milliseconds (how fast the follower responds to falling levels).
    pub release_ms: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
    /// Which audio signal to follow.
    #[serde(default)]
    pub input: FollowerInput,
}

impl Default for FollowerConfig {
    fn default() -> Self {
        Self {
            attack_ms: 10.0,
            release_ms: 100.0,
            depth: 1.0,
            input: FollowerInput::TrackAudio,
        }
    }
}
