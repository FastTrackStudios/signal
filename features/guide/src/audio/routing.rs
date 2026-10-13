//! Stereo mixing helpers (ported from fts-guide `audio/routing.rs`).
//!
//! The legacy 8-channel REAPER routing (`route_single_channel`) is dropped:
//! the engine exposes separate click / count / guide buses instead and the
//! host decides where they go.

use crate::samples::AudioSample;

/// Audio mixing helpers.
pub struct AudioRouter;

impl AudioRouter {
    /// Mix a mono sample to stereo (duplicates to both channels)
    pub fn mix_mono_to_stereo(sample: f32, gain: f32, left: &mut f32, right: &mut f32) {
        *left += sample * gain;
        *right += sample * gain;
    }

    /// Mix a stereo sample (routes channel 0 to left, channel 1 to right)
    pub fn mix_stereo_to_stereo(
        sample_left: f32,
        sample_right: f32,
        gain: f32,
        left: &mut f32,
        right: &mut f32,
    ) {
        *left += sample_left * gain;
        *right += sample_right * gain;
    }

    /// Mix decoded audio (auto-detects mono vs stereo)
    pub fn mix_decoded_audio(
        decoded_audio: &AudioSample,
        position: usize,
        gain: f32,
        left: &mut f32,
        right: &mut f32,
    ) {
        if decoded_audio.data.len() == 1 {
            // Mono source: duplicate to both channels
            let sample_val = decoded_audio
                .data
                .get(0)
                .and_then(|ch| ch.get(position))
                .copied()
                .unwrap_or(0.0);
            Self::mix_mono_to_stereo(sample_val, gain, left, right);
        } else {
            // Stereo source: route channel 0 to left, channel 1 to right
            let left_val = decoded_audio
                .data
                .get(0)
                .and_then(|ch| ch.get(position))
                .copied()
                .unwrap_or(0.0);
            let right_val = decoded_audio
                .data
                .get(1)
                .and_then(|ch| ch.get(position))
                .copied()
                .unwrap_or(0.0);
            Self::mix_stereo_to_stereo(left_val, right_val, gain, left, right);
        }
    }
}
