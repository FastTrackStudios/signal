//! Guide sample playback
//!
//! Handles playback of guide samples for section announcements.
//! Ported from fts-guide `audio/guide_player.rs`; the shared
//! `Arc<Mutex<..>>` key/sample-map became plain references.

use std::collections::HashMap;

use crate::samples::AudioSample;

use super::routing::AudioRouter;

/// Guide sample player state
#[derive(Debug, Clone)]
pub struct GuidePlayerState {
    pub playback_position_guide: usize,
    pub is_playing_guide: bool,
    pub guide_has_triggered: bool,
}

impl GuidePlayerState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            playback_position_guide: 0,
            is_playing_guide: false,
            guide_has_triggered: false,
        }
    }

    /// Reset playback state
    pub const fn reset(&mut self) {
        self.playback_position_guide = 0;
        self.is_playing_guide = false;
        self.guide_has_triggered = false;
    }
}

impl Default for GuidePlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Guide sample player
pub struct GuidePlayer;

impl GuidePlayer {
    /// Play guide sample and mix to output
    ///
    /// Returns true if guide is still playing, false if it finished
    pub fn play(
        state: &mut GuidePlayerState,
        current_guide_key: &Option<String>,
        guide_samples: &HashMap<String, AudioSample>,
        gain: f32,
        guide_left: &mut f32,
        guide_right: &mut f32,
    ) -> bool {
        if !state.is_playing_guide {
            return false;
        }

        if let Some(guide_key) = current_guide_key {
            if let Some(guide_audio) = guide_samples.get(guide_key) {
                if state.playback_position_guide < guide_audio.frames() {
                    AudioRouter::mix_decoded_audio(
                        guide_audio,
                        state.playback_position_guide,
                        gain,
                        guide_left,
                        guide_right,
                    );
                    state.playback_position_guide = state.playback_position_guide.saturating_add(1);

                    if state.playback_position_guide >= guide_audio.frames() {
                        // Finished playing
                        state.is_playing_guide = false;
                        state.playback_position_guide = 0;
                        return false;
                    }
                    return true;
                }
                // Past end of sample
                state.is_playing_guide = false;
                state.playback_position_guide = 0;
                return false;
            }
        }

        // No guide key or sample not found
        state.is_playing_guide = false;
        state.playback_position_guide = 0;
        false
    }

    /// Stop guide playback
    pub fn stop(state: &mut GuidePlayerState, current_guide_key: &mut Option<String>) {
        state.is_playing_guide = false;
        state.playback_position_guide = 0;
        *current_guide_key = None;
    }
}
