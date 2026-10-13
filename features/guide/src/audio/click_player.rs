//! Click sample playback
//!
//! Handles playback of click samples: beat, eighth, sixteenth, triplet, and
//! measure accent. Ported from fts-guide; the shared-state `Arc<Mutex<..>>`
//! sample slots became plain `Option<&AudioSample>` references.

use crate::samples::AudioSample;

use super::routing::AudioRouter;

/// State of a single click slot: playback position and whether it's currently playing
#[derive(Debug, Clone, Copy)]
pub struct ClickSlot {
    pub playback_position: usize,
    pub is_playing: bool,
}

impl ClickSlot {
    const fn new() -> Self {
        Self {
            playback_position: 0,
            is_playing: false,
        }
    }
}

/// Click sample player state
#[derive(Debug, Clone)]
pub struct ClickPlayerState {
    pub beat: ClickSlot,
    pub eighth: ClickSlot,
    pub sixteenth: ClickSlot,
    pub triplet: ClickSlot,
    pub measure_accent: ClickSlot,
}

impl ClickPlayerState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            beat: ClickSlot::new(),
            eighth: ClickSlot::new(),
            sixteenth: ClickSlot::new(),
            triplet: ClickSlot::new(),
            measure_accent: ClickSlot::new(),
        }
    }

    /// Reset all playback states
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for ClickPlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Advance one voice slot by one frame, mixing into `left`/`right`.
/// Shared body of the per-subdivision `play_*` fns (logic unchanged from
/// the legacy per-slot implementations).
fn play_slot(
    slot: &mut ClickSlot,
    sample_data: Option<&AudioSample>,
    gain: f32,
    left: &mut f32,
    right: &mut f32,
) {
    if !slot.is_playing {
        return;
    }
    if let Some(decoded_audio) = sample_data {
        if slot.playback_position < decoded_audio.frames() {
            AudioRouter::mix_decoded_audio(
                decoded_audio,
                slot.playback_position,
                gain,
                left,
                right,
            );
            slot.playback_position = slot.playback_position.saturating_add(1);

            if slot.playback_position >= decoded_audio.frames() {
                slot.is_playing = false;
                slot.playback_position = 0;
            }
        } else {
            slot.is_playing = false;
            slot.playback_position = 0;
        }
    }
}

/// Click sample player
pub struct ClickPlayer;

impl ClickPlayer {
    /// Play beat sample and mix to output
    pub fn play_beat(
        state: &mut ClickPlayerState,
        sample_data: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        play_slot(&mut state.beat, sample_data, gain, click_left, click_right);
    }

    /// Play eighth sample and mix to output
    pub fn play_eighth(
        state: &mut ClickPlayerState,
        sample_data: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        play_slot(
            &mut state.eighth,
            sample_data,
            gain,
            click_left,
            click_right,
        );
    }

    /// Play sixteenth sample and mix to output
    pub fn play_sixteenth(
        state: &mut ClickPlayerState,
        sample_data: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        play_slot(
            &mut state.sixteenth,
            sample_data,
            gain,
            click_left,
            click_right,
        );
    }

    /// Play triplet sample and mix to output
    pub fn play_triplet(
        state: &mut ClickPlayerState,
        sample_data: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        play_slot(
            &mut state.triplet,
            sample_data,
            gain,
            click_left,
            click_right,
        );
    }

    /// Play measure accent sample and mix to output
    pub fn play_measure_accent(
        state: &mut ClickPlayerState,
        sample_data: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        play_slot(
            &mut state.measure_accent,
            sample_data,
            gain,
            click_left,
            click_right,
        );
    }

    /// Play all active click samples and mix to output
    pub fn play_all(
        state: &mut ClickPlayerState,
        sample_data_beat: Option<&AudioSample>,
        sample_data_eighth: Option<&AudioSample>,
        sample_data_sixteenth: Option<&AudioSample>,
        sample_data_triplet: Option<&AudioSample>,
        sample_data_measure_accent: Option<&AudioSample>,
        gain: f32,
        click_left: &mut f32,
        click_right: &mut f32,
    ) {
        Self::play_beat(state, sample_data_beat, gain, click_left, click_right);
        Self::play_eighth(state, sample_data_eighth, gain, click_left, click_right);
        Self::play_sixteenth(state, sample_data_sixteenth, gain, click_left, click_right);
        Self::play_triplet(state, sample_data_triplet, gain, click_left, click_right);
        Self::play_measure_accent(
            state,
            sample_data_measure_accent,
            gain,
            click_left,
            click_right,
        );
    }
}
