//! The guide engine: renders click / count-in / guide voices into
//! caller-provided buffers, driven by a caller-provided clock.
//!
//! This is the portable replacement for the legacy plugin's
//! `Plugin::process` — no REAPER, no audio I/O, no threads. The host
//! (daw-standalone's render callback, a test harness, an offline
//! bouncer) calls [`GuideEngine::render`] once per block with a
//! [`BlockClock`] describing the transport at block start.

use crate::audio::{
    ClickPlayer, ClickPlayerState, CountPlayer, CountPlayerState, GuidePlayer, GuidePlayerState,
};
use crate::samples::SampleBank;
use crate::schedule::{CueEvent, CueSchedule, GuideSection, GuideSongTiming, ScheduleOptions};

/// Transport snapshot at the start of a render block.
#[derive(Debug, Clone, Copy)]
pub struct BlockClock {
    /// Whether the transport is rolling. When false the engine only
    /// flushes voice tails (no new triggers).
    pub playing: bool,
    /// Timeline position in seconds at the first frame of the block.
    pub pos_seconds: f64,
    /// Timeline position in quarter notes at the first frame of the block.
    pub pos_beats: f64,
    /// Tempo in quarter notes per minute.
    pub tempo_bpm: f64,
    /// Time signature numerator.
    pub time_sig_num: u32,
    /// Time signature denominator.
    pub time_sig_den: u32,
    /// Render sample rate.
    pub sample_rate: f64,
}

/// Output buses for one render block. All six slices must be the same
/// length; the engine ADDS into them (clear them first if needed).
pub struct GuideBuses<'a> {
    pub click_l: &'a mut [f32],
    pub click_r: &'a mut [f32],
    pub count_l: &'a mut [f32],
    pub count_r: &'a mut [f32],
    pub guide_l: &'a mut [f32],
    pub guide_r: &'a mut [f32],
}

/// Finer click subdivision settings.
#[derive(Debug, Clone, Copy)]
pub struct SubdivisionSettings {
    /// Click on eighth notes.
    pub eighth: bool,
    /// Click on sixteenth notes.
    pub sixteenth: bool,
    /// Click on beat-unit triplets.
    pub triplet: bool,
}

impl Default for SubdivisionSettings {
    fn default() -> Self {
        Self {
            eighth: false,
            sixteenth: false,
            triplet: false,
        }
    }
}

/// Click subdivision grid settings.
#[derive(Debug, Clone)]
pub struct ClickSubdivisions {
    /// Click on every beat (quarter note).
    pub beat: bool,
    /// Accent click on beat 1 of each measure.
    pub measure_accent: bool,
    /// Finer subdivisions.
    pub subdivisions: SubdivisionSettings,
}

impl Default for ClickSubdivisions {
    fn default() -> Self {
        Self {
            beat: true,
            measure_accent: true,
            subdivisions: SubdivisionSettings::default(),
        }
    }
}

/// Engine configuration (the legacy plugin's parameters, minus the
/// REAPER/GUI plumbing).
#[derive(Debug, Clone)]
pub struct GuideConfig {
    /// Click subdivision settings.
    pub click: ClickSubdivisions,
    /// Count-in voices.
    pub enable_count: bool,
    /// Section guide announcements.
    pub enable_guide: bool,
    /// Linear gain for the click bus.
    pub click_gain: f32,
    /// Linear gain for the count bus.
    pub count_gain: f32,
    /// Linear gain for the guide bus.
    pub guide_gain: f32,
    /// Count-in / announcement scheduling options.
    pub schedule: ScheduleOptions,
    /// Where triggers come from. See [`TriggerSource`].
    pub source: TriggerSource,
}

/// What drives the engine.
///
/// The two are mutually exclusive on purpose. Running both would
/// double-trigger every beat: a stamped guide track carries the *same*
/// clicks and cues the internal grid would generate, so a host playing
/// that track into the plugin while the plugin also follows the transport
/// hears everything twice, slightly flammed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerSource {
    /// Follow the host transport: the engine derives the click grid from
    /// tempo/time-signature and fires scheduled cues from its own
    /// [`CueSchedule`]. Self-contained — no input needed.
    #[default]
    HostTransport,
    /// Play only what's handed to [`GuideEngine::trigger`] — MIDI notes
    /// from the host. The internal grid and cue schedule stay silent, so
    /// what you hear is exactly what's on the track: editable, visible,
    /// and movable in the piano roll.
    Midi,
}

impl Default for GuideConfig {
    fn default() -> Self {
        Self {
            click: ClickSubdivisions::default(),
            enable_count: true,
            enable_guide: true,
            click_gain: 1.0,
            count_gain: 1.0,
            guide_gain: 1.0,
            schedule: ScheduleOptions::default(),
            source: TriggerSource::default(),
        }
    }
}

/// A sound the engine can fire.
///
/// Public because the engine can be driven from outside its own grid —
/// the plugin shell maps incoming MIDI notes onto these and hands them to
/// [`GuideEngine::trigger`]. See [`TriggerSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuideTrigger {
    Beat,
    Accent,
    Eighth,
    Sixteenth,
    Triplet,
    Count(usize),
    Guide(String),
}

/// The guide engine. See module docs.
pub struct GuideEngine {
    pub config: GuideConfig,
    bank: SampleBank,
    schedule: CueSchedule,

    /// Triggers pushed in from outside (MIDI), consumed next block.
    /// Separate from `pending` because `prepare_block` clears that.
    external: Vec<(usize, GuideTrigger)>,

    click_state: ClickPlayerState,
    count_state: CountPlayerState,
    guide_state: GuidePlayerState,
    current_guide_key: Option<String>,

    /// Last-triggered grid indices per subdivision (i64 grid steps).
    /// `i64::MIN` means "re-initialize on next rolling block" (fresh
    /// start or after a stop/seek).
    last_beat_idx: i64,
    last_eighth_idx: i64,
    last_sixteenth_idx: i64,
    last_triplet_idx: i64,

    /// Timeline seconds at the end of the previous rolling block, used to
    /// detect seeks and relocate the cue cursor. `None` forces relocation.
    prev_block_end_seconds: Option<f64>,
    /// Cursor into `schedule.cues`.
    next_cue: usize,

    /// Scratch trigger list, reused across blocks.
    pending: Vec<(usize, GuideTrigger)>,
}

impl GuideEngine {
    #[must_use]
    pub fn new(config: GuideConfig) -> Self {
        Self {
            config,
            bank: SampleBank::default(),
            schedule: CueSchedule::default(),
            external: Vec::new(),
            click_state: ClickPlayerState::new(),
            count_state: CountPlayerState::new(),
            guide_state: GuidePlayerState::new(),
            current_guide_key: None,
            last_beat_idx: i64::MIN,
            last_eighth_idx: i64::MIN,
            last_sixteenth_idx: i64::MIN,
            last_triplet_idx: i64::MIN,
            prev_block_end_seconds: None,
            next_cue: 0,
            pending: Vec::with_capacity(32),
        }
    }

    /// Access the sample bank (to load click/count/guide PCM into it).
    pub const fn bank_mut(&mut self) -> &mut SampleBank {
        &mut self.bank
    }

    #[must_use]
    pub const fn bank(&self) -> &SampleBank {
        &self.bank
    }

    /// Replace the sample bank wholesale.
    pub fn set_bank(&mut self, bank: SampleBank) {
        self.bank = bank;
    }

    /// Build and install the cue schedule for a song's sections.
    pub fn set_sections(&mut self, sections: &[GuideSection], timing: &GuideSongTiming) {
        self.schedule = CueSchedule::build(sections, timing, &self.config.schedule);
        self.prev_block_end_seconds = None;
        self.next_cue = 0;
    }

    /// Install a prebuilt cue schedule (e.g. one with extra spoken cues).
    pub fn set_schedule(&mut self, schedule: CueSchedule) {
        self.schedule = schedule;
        self.prev_block_end_seconds = None;
        self.next_cue = 0;
    }

    #[must_use]
    pub const fn schedule(&self) -> &CueSchedule {
        &self.schedule
    }

    /// Reset all playback state (voice tails, grid trackers, cue cursor).
    pub fn reset(&mut self) {
        self.external.clear();
        self.click_state.reset();
        self.count_state.reset();
        self.guide_state.reset();
        self.current_guide_key = None;
        self.last_beat_idx = i64::MIN;
        self.last_eighth_idx = i64::MIN;
        self.last_sixteenth_idx = i64::MIN;
        self.last_triplet_idx = i64::MIN;
        self.prev_block_end_seconds = None;
        self.next_cue = 0;
    }

    /// Render one block into separate click / count / guide buses
    /// (adding into the slices).
    pub fn render(&mut self, buses: &mut GuideBuses<'_>, clock: &BlockClock) {
        let n = buses.click_l.len();
        debug_assert!(
            [
                buses.click_r.len(),
                buses.count_l.len(),
                buses.count_r.len(),
                buses.guide_l.len(),
                buses.guide_r.len(),
            ]
            .iter()
            .all(|&len| len == n),
            "all guide buses must be the same length"
        );
        self.prepare_block(n, clock);
        self.mix_block(n, |i, [cl, cr, nl, nr, gl, gr]| {
            if let Some(v) = buses.click_l.get_mut(i) {
                *v += cl;
            }
            if let Some(v) = buses.click_r.get_mut(i) {
                *v += cr;
            }
            if let Some(v) = buses.count_l.get_mut(i) {
                *v += nl;
            }
            if let Some(v) = buses.count_r.get_mut(i) {
                *v += nr;
            }
            if let Some(v) = buses.guide_l.get_mut(i) {
                *v += gl;
            }
            if let Some(v) = buses.guide_r.get_mut(i) {
                *v += gr;
            }
        });
    }

    /// Render one block with all three buses summed to a stereo pair
    /// (adding into the slices).
    pub fn render_stereo(&mut self, left: &mut [f32], right: &mut [f32], clock: &BlockClock) {
        let n = left.len().min(right.len());
        self.prepare_block(n, clock);
        self.mix_block(n, |i, [cl, cr, nl, nr, gl, gr]| {
            if let Some(v) = left.get_mut(i) {
                *v += cl + nl + gl;
            }
            if let Some(v) = right.get_mut(i) {
                *v += cr + nr + gr;
            }
        });
    }

    /// Queue a sound to fire `offset_frames` into the next rendered
    /// block.
    ///
    /// The host shell calls this per MIDI note-on, passing the event's
    /// sample offset so a note lands where it was played rather than at
    /// the block boundary — block-quantised cues audibly flam against a
    /// click.
    ///
    /// Honoured under either [`TriggerSource`]; the mode only controls
    /// whether the engine *also* generates its own.
    pub fn trigger(&mut self, offset_frames: usize, trigger: GuideTrigger) {
        self.external.push((offset_frames, trigger));
    }

    /// Schedule grid-based triggers (click subdivisions) for this block.
    fn schedule_grid_triggers(
        &mut self,
        n: usize,
        samples_per_quarter: f64,
        beats_start: f64,
        beats_end: f64,
        clock: &BlockClock,
    ) {
        let num = i64::from(clock.time_sig_num.max(1));
        let beat_unit_quarters = 4.0 / f64::from(clock.time_sig_den.max(1));
        let triplet_interval = beat_unit_quarters / 3.0;

        let mut schedule_grid =
            |interval: f64,
             last_idx: &mut i64,
             pending: &mut Vec<(usize, GuideTrigger)>,
             make: &dyn Fn(i64) -> GuideTrigger| {
                if interval <= 0.0 {
                    return;
                }
                if *last_idx == i64::MIN {
                    let base_idx = ((beats_start / interval) - 1e-6).ceil();
                    *last_idx = crate::cast::i64_from_f64_round(base_idx).saturating_sub(1);
                }
                while let Some(k) = (*last_idx).checked_add(1) {
                    // k is a beat grid index; convert to f64 for beat time calculation.
                    // Precision loss from i64->f64 is acceptable for beat-level granularity.
                    let k_beats = crate::cast::f64_from_i64(k);
                    let t = k_beats * interval;
                    if t >= beats_end {
                        break;
                    }
                    let sample_offset = (t - beats_start) * samples_per_quarter;
                    let offset = if sample_offset.is_finite() && sample_offset >= 0.0 {
                        let offset_nonneg = crate::cast::i64_from_f64_round(sample_offset).max(0);
                        // offset_nonneg is guaranteed non-negative, safe to cast to usize
                        crate::cast::usize_from_i64_nonneg(offset_nonneg).min(n.saturating_sub(1))
                    } else {
                        0
                    };
                    pending.push((offset, make(k)));
                    *last_idx = k;
                }
            };

        if self.config.click.beat || self.config.click.measure_accent {
            let enable_beat = self.config.click.beat;
            let enable_accent = self.config.click.measure_accent;
            let mut tmp: Vec<(usize, GuideTrigger)> = Vec::new();
            schedule_grid(1.0, &mut self.last_beat_idx, &mut tmp, &|k| {
                if enable_accent && k.rem_euclid(num) == 0 {
                    GuideTrigger::Accent
                } else {
                    GuideTrigger::Beat
                }
            });
            for (offset, trig) in tmp {
                match trig {
                    GuideTrigger::Accent => {
                        if self.bank.measure_accent.is_some() {
                            self.pending.push((offset, GuideTrigger::Accent));
                        } else if enable_beat {
                            self.pending.push((offset, GuideTrigger::Beat));
                        }
                    }
                    trig => {
                        if enable_beat {
                            self.pending.push((offset, trig));
                        }
                    }
                }
            }
        }
        if self.config.click.subdivisions.eighth {
            let mut tmp = std::mem::take(&mut self.pending);
            schedule_grid(0.5, &mut self.last_eighth_idx, &mut tmp, &|_| {
                GuideTrigger::Eighth
            });
            self.pending = tmp;
        }
        if self.config.click.subdivisions.sixteenth {
            let mut tmp = std::mem::take(&mut self.pending);
            schedule_grid(0.25, &mut self.last_sixteenth_idx, &mut tmp, &|_| {
                GuideTrigger::Sixteenth
            });
            self.pending = tmp;
        }
        if self.config.click.subdivisions.triplet {
            let mut tmp = std::mem::take(&mut self.pending);
            schedule_grid(
                triplet_interval,
                &mut self.last_triplet_idx,
                &mut tmp,
                &|_| GuideTrigger::Triplet,
            );
            self.pending = tmp;
        }
    }

    /// Schedule cue-based triggers (count-in voices, guide announcements) for this block.
    fn schedule_cue_triggers(
        &mut self,
        n: usize,
        sec_start: f64,
        sec_end: f64,
        continuous: bool,
        clock: &BlockClock,
    ) {
        if !continuous {
            self.next_cue = self
                .schedule
                .cues
                .partition_point(|c| c.time_seconds < sec_start - 1e-9);
        }
        while let Some(cue) = self.schedule.cues.get(self.next_cue) {
            if cue.time_seconds >= sec_end {
                break;
            }
            let time_delta = cue.time_seconds - sec_start;
            let sample_offset = time_delta * clock.sample_rate;
            let offset = if sample_offset.is_finite() && sample_offset >= 0.0 {
                let offset_nonneg = crate::cast::i64_from_f64_round(sample_offset).max(0);
                // offset_nonneg is guaranteed non-negative, safe to cast to usize
                crate::cast::usize_from_i64_nonneg(offset_nonneg).min(n.saturating_sub(1))
            } else {
                0
            };
            match &cue.event {
                CueEvent::Count { index } => {
                    if self.config.enable_count && *index < 8 {
                        self.pending.push((offset, GuideTrigger::Count(*index)));
                    }
                }
                CueEvent::Guide { keys, .. } => {
                    if self.config.enable_guide {
                        if let Some(key) = keys.iter().find(|k| self.bank.guides.contains_key(*k)) {
                            self.pending
                                .push((offset, GuideTrigger::Guide(key.clone())));
                        }
                    }
                }
            }
            let Some(next_cue) = self.next_cue.checked_add(1) else {
                break;
            };
            self.next_cue = next_cue;
        }
    }

    /// Gather this block's triggers into `self.pending` (sorted by offset).
    fn prepare_block(&mut self, n: usize, clock: &BlockClock) {
        self.pending.clear();

        // Externally-pushed triggers always play, whatever the source —
        // they were explicitly asked for.
        self.pending.append(&mut self.external);

        // In MIDI mode the internal grid and cue schedule stay silent;
        // the incoming notes ARE the guide.
        if self.config.source == TriggerSource::Midi {
            self.pending.sort_by_key(|(offset, _)| *offset);
            // n is typically much smaller than 2^53, so usize->f64 precision is acceptable
            let block_samples_f64 = crate::cast::f64_from_usize(n);
            let block_duration = block_samples_f64 / clock.sample_rate;
            self.prev_block_end_seconds = Some(clock.pos_seconds + block_duration);
            return;
        }
        if n == 0 || clock.sample_rate <= 0.0 {
            return;
        }

        if !clock.playing {
            // Legacy behavior: stopping clears the beat tracker so the next
            // start re-initializes on (or before) the current position.
            self.last_beat_idx = i64::MIN;
            self.last_eighth_idx = i64::MIN;
            self.last_sixteenth_idx = i64::MIN;
            self.last_triplet_idx = i64::MIN;
            self.prev_block_end_seconds = None;
            return;
        }

        let samples_per_quarter = clock.sample_rate * 60.0 / clock.tempo_bpm.max(1.0);
        // n is typically much smaller than 2^53, so usize->f64 precision is acceptable
        let block_samples_f64 = crate::cast::f64_from_usize(n);
        let block_quarters = block_samples_f64 / samples_per_quarter;
        let beats_start = clock.pos_beats;
        let beats_end = beats_start + block_quarters;

        // Detect a seek up front: this block's start doesn't continue from
        // the previous block's end. On a seek we must re-anchor BOTH the
        // click grid trackers and the cue cursor to the new position.
        let sec_start = clock.pos_seconds;
        let continuous = self
            .prev_block_end_seconds
            .is_some_and(|prev| (prev - sec_start).abs() < 1e-4);
        if !continuous {
            self.last_beat_idx = i64::MIN;
            self.last_eighth_idx = i64::MIN;
            self.last_sixteenth_idx = i64::MIN;
            self.last_triplet_idx = i64::MIN;
        }

        // ── Click subdivisions (grid-index walk; sample accurate) ────────
        self.schedule_grid_triggers(n, samples_per_quarter, beats_start, beats_end, clock);

        // ── Scheduled cues (count-in voices, guide announcements) ────────
        // n is typically much smaller than 2^53, so usize->f64 precision is acceptable
        let block_samples_f64 = crate::cast::f64_from_usize(n);
        let block_duration = block_samples_f64 / clock.sample_rate;
        let sec_end = sec_start + block_duration;
        self.schedule_cue_triggers(n, sec_start, sec_end, continuous, clock);

        self.prev_block_end_seconds = Some(sec_end);
        self.pending.sort_by_key(|(offset, _)| *offset);
    }

    /// Apply a pending trigger to the player states.
    fn apply_trigger(&mut self, trigger: &GuideTrigger) {
        match trigger {
            GuideTrigger::Beat => {
                self.click_state.beat.is_playing = true;
                self.click_state.beat.playback_position = 0;
            }
            GuideTrigger::Accent => {
                self.click_state.measure_accent.is_playing = true;
                self.click_state.measure_accent.playback_position = 0;
            }
            GuideTrigger::Eighth => {
                self.click_state.eighth.is_playing = true;
                self.click_state.eighth.playback_position = 0;
            }
            GuideTrigger::Sixteenth => {
                self.click_state.sixteenth.is_playing = true;
                self.click_state.sixteenth.playback_position = 0;
            }
            GuideTrigger::Triplet => {
                self.click_state.triplet.is_playing = true;
                self.click_state.triplet.playback_position = 0;
            }
            GuideTrigger::Count(index) => {
                if let Some(is_playing) = self.count_state.is_playing_count.get_mut(*index) {
                    *is_playing = true;
                }
                if let Some(pos) = self.count_state.playback_position_count.get_mut(*index) {
                    *pos = 0;
                }
                let count_num = i32::try_from(*index).unwrap_or(0).saturating_add(1);
                self.count_state.current_count_number = count_num;
            }
            GuideTrigger::Guide(key) => {
                self.current_guide_key = Some(key.clone());
                self.guide_state.is_playing_guide = true;
                self.guide_state.playback_position_guide = 0;
            }
        }
    }

    /// Mix `n` frames, applying pending triggers at their offsets.
    fn mix_block(&mut self, n: usize, mut write: impl FnMut(usize, [f32; 6])) {
        let click_gain = self.config.click_gain;
        let count_gain = self.config.count_gain;
        let guide_gain = self.config.guide_gain;

        let pending = std::mem::take(&mut self.pending);
        let mut ti = 0;
        for i in 0..n {
            while ti < pending.len() {
                if let Some(p) = pending.get(ti) {
                    if p.0 != i {
                        break;
                    }
                    self.apply_trigger(&p.1);
                    let Some(next_ti) = ti.checked_add(1) else {
                        break;
                    };
                    ti = next_ti;
                } else {
                    break;
                }
            }

            let mut click_left = 0.0f32;
            let mut click_right = 0.0f32;
            let mut count_left = 0.0f32;
            let mut count_right = 0.0f32;
            let mut guide_left = 0.0f32;
            let mut guide_right = 0.0f32;

            ClickPlayer::play_all(
                &mut self.click_state,
                self.bank.beat.as_ref(),
                self.bank.eighth.as_ref(),
                self.bank.sixteenth.as_ref(),
                self.bank.triplet.as_ref(),
                self.bank.measure_accent.as_ref(),
                click_gain,
                &mut click_left,
                &mut click_right,
            );
            CountPlayer::play_all(
                &mut self.count_state,
                &self.bank.counts,
                count_gain,
                &mut count_left,
                &mut count_right,
            );
            GuidePlayer::play(
                &mut self.guide_state,
                &self.current_guide_key,
                &self.bank.guides,
                guide_gain,
                &mut guide_left,
                &mut guide_right,
            );

            write(
                i,
                [
                    click_left,
                    click_right,
                    count_left,
                    count_right,
                    guide_left,
                    guide_right,
                ],
            );
        }
        // Give the scratch buffer back (empty) for reuse.
        self.pending = pending;
        self.pending.clear();
    }
}

impl Default for GuideEngine {
    fn default() -> Self {
        Self::new(GuideConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of squared output energy for one rolling block at `pos_beats`.
    fn block_energy(engine: &mut GuideEngine, pos_seconds: f64, pos_beats: f64) -> f64 {
        // 120 bpm, 48 kHz → 24 000 samples/quarter, so a 24 000-frame block
        // is exactly one beat and always contains a beat onset at offset 0.
        let n = 24_000;
        let mut l = vec![0.0f32; n];
        let mut r = vec![0.0f32; n];
        engine.render_stereo(
            &mut l,
            &mut r,
            &BlockClock {
                playing: true,
                pos_seconds,
                pos_beats,
                tempo_bpm: 120.0,
                time_sig_num: 4,
                time_sig_den: 4,
                sample_rate: 48_000.0,
            },
        );
        l.iter()
            .chain(r.iter())
            .map(|s| {
                let s_f64 = f64::from(*s);
                s_f64 * s_f64
            })
            .sum()
    }

    /// A backward seek (to an already-played section) must NOT silence the
    /// click. Regression for the monotonic grid-index bug: the click grid
    /// trackers were only re-anchored on the cue cursor, so seeking back
    /// left them ahead of the playhead and no beats fired until playback
    /// climbed back to the old position.
    #[test]
    fn backward_seek_keeps_click_playing() {
        let mut bank = SampleBank::default();
        bank.synthesize_defaults(48_000);
        let config = GuideConfig {
            enable_count: false,
            enable_guide: false,
            ..Default::default()
        };
        let mut engine = GuideEngine::new(config);
        engine.set_bank(bank);

        // Roll forward a few beats so the grid trackers advance.
        assert!(
            block_energy(&mut engine, 2.0, 4.0) > 0.0,
            "first block silent"
        );
        assert!(
            block_energy(&mut engine, 2.5, 5.0) > 0.0,
            "second block silent"
        );
        assert!(
            block_energy(&mut engine, 3.0, 6.0) > 0.0,
            "third block silent"
        );

        // Seek BACKWARD to an earlier beat: the click must keep firing.
        let after_seek = block_energy(&mut engine, 1.0, 2.0);
        assert!(
            after_seek > 0.0,
            "click went silent after a backward seek (energy = {after_seek})"
        );

        // And a forward seek stays healthy too.
        let after_fwd = block_energy(&mut engine, 10.0, 20.0);
        assert!(after_fwd > 0.0, "click went silent after a forward seek");
    }
}
