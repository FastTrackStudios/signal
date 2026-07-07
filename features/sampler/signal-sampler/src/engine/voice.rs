//! Voice — a single playing sample with pitch shifting and amplitude envelope.
//!
//! Pitch shifting is implemented as a playback rate change with linear
//! interpolation between frames. A pitch shift of N semitones multiplies
//! the playback rate by `2^(N/12)`.
//!
//! The amplitude envelope is a simple two-stage model:
//! - Playing: flat at `gain`
//! - Releasing: linear fade to zero over `release_frames`
//!
//! CSS sustain samples have their own natural releases baked in; we do not
//! apply an envelope to them. Release samples are played to completion at
//! reduced gain.

use std::sync::Arc;

use super::cache::SampleData;

// ── Voice state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceState {
    /// Sample is playing normally.
    Playing,
    /// Note-off received — fading out.
    Releasing { frames_remaining: usize },
    /// Playback finished — ready for reuse.
    Done,
}

/// Stem class of the articulation that spawned a voice — the routing key for
/// stem-aware output buses (see `docs/plan/document-mode.md`, "Stem-aware
/// output buses"). Longs: sustain/legato/tremolo bodies + their releases.
/// Shorts: short articulations (staccato/spiccato/pizz/…) + their releases.
/// Default routing sends every class to the main bus (no behavior change).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArticClass {
    Longs,
    Shorts,
}

/// Classification of what triggered this voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceKind {
    /// Non-vibrato sustain, lower CC1 dynamic layer.
    SustainNVLo,
    /// Non-vibrato sustain, upper CC1 dynamic layer.
    SustainNVHi,
    /// Vibrato sustain, lower CC1 dynamic layer (CC2 crossfade pair).
    SustainVibLo,
    /// Vibrato sustain, upper CC1 dynamic layer (CC2 crossfade pair).
    SustainVibHi,
    /// Primary sustain layer A (lower dynamic).
    SustainLo,
    /// Primary sustain layer B (upper dynamic, for CC1 crossfade).
    SustainHi,
    /// Zoned sustain dynamic layer (CSS N-layer CC1 crossfade). Carries its
    /// vib side + dynamic-layer index in [`Voice::dyn_layer`]; the engine sets
    /// each one's gain from CC1/CC2 so a held note swells the full range.
    SustainLayer,
    /// Legato transition sample.
    Legato,
    /// Release trail (triggered on note-off).
    Release,
    /// Short note (one-shot — plays to completion regardless of note-off).
    Short,
    /// Zone-mode voice (Spectrasonics-style explicit-zone libraries).
    /// Held until note-off, then released over `release_frames`.
    Zoned,
}

impl VoiceKind {
    /// Stable name for the render trace / debug output.
    pub fn trace_name(&self) -> &'static str {
        match self {
            VoiceKind::SustainNVLo => "SustainNVLo",
            VoiceKind::SustainNVHi => "SustainNVHi",
            VoiceKind::SustainVibLo => "SustainVibLo",
            VoiceKind::SustainVibHi => "SustainVibHi",
            VoiceKind::SustainLo => "SustainLo",
            VoiceKind::SustainHi => "SustainHi",
            VoiceKind::SustainLayer => "SustainLayer",
            VoiceKind::Legato => "Legato",
            VoiceKind::Release => "Release",
            VoiceKind::Short => "Short",
            VoiceKind::Zoned => "Zoned",
        }
    }
}

// ── Voice ─────────────────────────────────────────────────────────────────────

/// One active sample playback.
pub struct Voice {
    /// The decoded sample data (shared via Arc).
    pub data: Arc<SampleData>,

    /// Current playback position in frames (fractional for pitch shifting).
    position: f64,

    /// First frame allowed for playback. Defaults to sample start.
    start_frame: usize,

    /// One-past-last frame allowed for playback. Defaults to sample length.
    end_frame: usize,

    loop_range: Option<(usize, usize)>,
    reverse: bool,
    alternating_loop: bool,

    /// Seamless forward-loop crossfade length in frames. As the read
    /// approaches `loop_end`, the pre-`loop_start` material is blended in over
    /// this many frames so the wrap carries no amplitude/phase discontinuity
    /// (the click heard on a held/looped note). `0` = hard wrap, no fade.
    loop_xfade: usize,

    /// Playback rate. 1.0 = original pitch, 2^(semitones/12) for transposition.
    rate: f64,

    /// Output gain [0.0, 1.0].
    pub gain: f32,
    pan_l: f32,
    pan_r: f32,

    /// Target gain for CC1 crossfade blend (updated per render block).
    pub target_gain: f32,

    /// Gain smoothing — frames to ramp from `gain` to `target_gain`.
    gain_ramp_frames: usize,

    pub state: VoiceState,
    pub kind: VoiceKind,

    /// Delayed-release countdown: while > 0 the voice stays at full gain
    /// (Playing), then begins releasing over `pending_fade`. Used by legato so
    /// the outgoing source-pitch note holds through the transition sample's
    /// quiet pre-bow-change "breath" and only fades at the arrival tick — filling
    /// the gap that otherwise ticks between notes. `0` = no delay.
    release_hold: usize,
    /// Fade length (frames) to apply when `release_hold` reaches 0.
    pending_fade: usize,

    /// Delayed attack: while > 0 the voice stays at silence (gain held at 0,
    /// gain-ramp paused). When it reaches 0 the pending `gain_ramp_frames`
    /// begins, fading in to `target_gain`. This is the CSS "fade the sustain in
    /// underneath the transition" handoff (`CSS_W` helper: wait, then
    /// `fade_in`) — the looping sustain is spawned muted and emerges under the
    /// one-shot bow-change transition. `0` = no delay (normal attack).
    attack_delay: usize,

    /// MIDI note this voice belongs to (for note-off matching).
    pub note: u8,

    /// Index into the patch's `LibrarySpec.mics` declaration order.
    /// `None` for libraries without explicit mics (folds to mic 0 in
    /// multi-mic render).
    pub mic_index: Option<u8>,

    /// Hashed choke/exclusive group id for zone-mode voices.
    pub choke_group: Option<u64>,

    /// Release fade duration in frames. Used when state transitions to Releasing.
    release_frames: usize,

    /// For `SustainLayer` voices: which vib side + dynamic-layer index this is,
    /// so the engine can set its CC1/CC2 crossfade gain. `None` otherwise.
    pub dyn_layer: Option<DynLayer>,

    /// Mono legato line this voice belongs to (engine `LineId`). Line-scoped
    /// operations (legato fade of the outgoing note, per-line note-off,
    /// per-line CC1 dynamics) match on it so divisi lines never silence each
    /// other's voices. Single-line/live play leaves everything on line 0.
    pub line: u8,

    /// Stem class of the articulation that spawned this voice (bus routing).
    pub artic_class: ArticClass,

    /// Decoded ENV_FLEX amplitude envelope (the instrument's real per-voice amp
    /// AHDSR), multiplied into the output. `None` = flat unity (legacy voices /
    /// families with no decoded envelope).
    flex: Option<FlexEnv>,
}

/// One decoded Kontakt ENV_FLEX amplitude envelope, evaluated per frame and
/// multiplied into the voice's output. Segments are `(len_frames, from, to,
/// tension)` — the actual shipped `(time_ms, level, curve)` triplets from the
/// instrument's GroupList (see `nkx-extract/CSS_GROUP_MOD.md` §2), converted to
/// frames at construction. Segment 0 ramps from 0 (or the caller-supplied
/// `seg0_from`) to its level; each later segment ramps from the previous
/// segment's level.
///
/// `hold_end` (frames) freezes the envelope timeline at that point while the
/// voice is still `Playing` — used for **sustain** families whose final decoded
/// segment is a slow 20 s decay-to-0 that represents the bow eventually running
/// out. Real CSS loops the body indefinitely under a sustain hold, so we freeze
/// at the end of the hold segment and let the engine's own note-off release
/// fade retire the voice; the 20 s tail is never entered while held. `None`
/// (shorts / releases) plays the whole envelope straight through (one-shot).
#[derive(Debug, Clone)]
pub struct FlexEnv {
    /// `(len_frames, from_level, to_level, tension)` per segment.
    segs: Vec<(f64, f32, f32, f32)>,
    /// Frames elapsed along the envelope timeline.
    pos: f64,
    /// While `Some`, `pos` is clamped to this value until the voice releases —
    /// the sustain-hold freeze (see struct docs).
    hold_end: Option<f64>,
}

impl FlexEnv {
    /// Build from decoded `(time_ms, level, tension)` segments. `seg0_from` is
    /// the starting level of segment 0 (0.0 for attack-from-silence). When
    /// `hold` is true the timeline freezes at the end of the *last segment that
    /// does not decay to ~0* (the sustain hold point).
    pub fn from_segments(
        segments: &[(f32, f32, f32)],
        seg0_from: f32,
        sample_rate: u32,
        hold: bool,
    ) -> Option<Self> {
        if segments.is_empty() {
            return None;
        }
        let mut segs = Vec::with_capacity(segments.len());
        let mut from = seg0_from;
        let mut cum = 0.0f64;
        let mut hold_end: Option<f64> = None;
        for &(ms, level, tension) in segments.iter() {
            let len = (ms as f64 * sample_rate as f64 / 1000.0).max(1.0);
            segs.push((len, from, level, tension.clamp(0.0, 1.0)));
            // The sustain hold point is the end of the last non-decaying
            // segment (the plateau at level≈1.0 before the long fade to 0).
            if hold && level > 0.01 {
                hold_end = Some(cum + len);
            }
            cum += len;
            from = level;
        }
        Some(Self {
            segs,
            pos: 0.0,
            hold_end,
        })
    }

    /// Kontakt segment-tension shape law. `t` is normalized progress [0,1]
    /// through a segment; `tension` ∈ [0,1] is the shipped `curve` field
    /// (0.5 = linear, <0.5 concave, >0.5 convex). PINNED law (not fitted): a
    /// fixed exponential-tension curve, the standard shape for Kontakt bipolar
    /// curve controls — `f(t) = (e^{a t} − 1)/(e^a − 1)` with
    /// `a = (tension − 0.5)·2·K`, `K = 6` (full-scale curvature), linear at
    /// tension = 0.5. The A/B metric (RMS over ~1 s windows) is insensitive to
    /// the exact curvature — only segment levels/times move it — so this is
    /// identified, not tuned.
    #[inline]
    fn shape(t: f32, tension: f32) -> f32 {
        let a = (tension - 0.5) * 12.0;
        if a.abs() < 1.0e-3 {
            t
        } else {
            ((a * t).exp() - 1.0) / (a.exp() - 1.0)
        }
    }

    /// Current envelope level for the present `pos`, without advancing.
    #[inline]
    fn level_at(&self, pos: f64) -> f32 {
        let mut acc = 0.0f64;
        for &(len, from, to, tension) in &self.segs {
            if pos < acc + len {
                let t = ((pos - acc) / len) as f32;
                return from + (to - from) * Self::shape(t.clamp(0.0, 1.0), tension);
            }
            acc += len;
        }
        // Past the last segment: hold its final level.
        self.segs.last().map(|s| s.2).unwrap_or(0.0)
    }

    /// Advance one frame (respecting the sustain-hold freeze) and return the
    /// level. `released` = the voice has received note-off (freeze lifts).
    #[inline]
    fn next(&mut self, released: bool) -> f32 {
        let lvl = self.level_at(self.pos);
        let frozen = !released && self.hold_end.is_some_and(|h| self.pos >= h);
        if !frozen {
            self.pos += 1.0;
        }
        lvl
    }
}

/// Identifies a zoned sustain dynamic layer for CC1/CC2 crossfade gain control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynLayer {
    /// True = vibrato side, false = non-vibrato side (CC2 crossfade).
    pub vib: bool,
    /// Dynamic-layer index (0 = softest), for CC1 crossfade.
    pub index: u8,
}

impl Voice {
    /// Create a new voice.
    ///
    /// - `semitone_offset`: how many semitones to shift the sample pitch.
    ///   Positive = up, negative = down.
    /// - `gain`: initial output gain (0.0–1.0).
    /// - `release_frames`: fade-out length when note-off arrives.
    pub fn new(
        data: Arc<SampleData>,
        note: u8,
        kind: VoiceKind,
        semitone_offset: i8,
        gain: f32,
        release_frames: usize,
    ) -> Self {
        let rate = 2.0f64.powf(semitone_offset as f64 / 12.0);
        let end_frame = data.num_frames;
        Self {
            data,
            position: 0.0,
            start_frame: 0,
            end_frame,
            loop_range: None,
            reverse: false,
            alternating_loop: false,
            loop_xfade: 0,
            rate,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            state: VoiceState::Playing,
            release_hold: 0,
            pending_fade: 0,
            attack_delay: 0,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
            flex: None,
        }
    }

    /// Create a voice with an explicit playback rate.
    ///
    /// Used by the zone-mode path (Spectrasonics-style libraries) where pitch
    /// shifting combines an integer semitone offset (note - root_key) with a
    /// per-zone fine-tune in cents. Caller computes `rate = 2^(total_cents/1200)`.
    pub fn with_rate(
        data: Arc<SampleData>,
        note: u8,
        kind: VoiceKind,
        rate: f64,
        gain: f32,
        release_frames: usize,
    ) -> Self {
        let end_frame = data.num_frames;
        Self {
            data,
            position: 0.0,
            start_frame: 0,
            end_frame,
            loop_range: None,
            reverse: false,
            alternating_loop: false,
            loop_xfade: 0,
            rate,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            state: VoiceState::Playing,
            release_hold: 0,
            pending_fade: 0,
            attack_delay: 0,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
            flex: None,
        }
    }

    /// Set the mic index this voice routes to. `mic_index` indexes into
    /// `LibrarySpec.mics` in declaration order.
    pub fn data_num_frames(&self) -> usize {
        self.data.num_frames
    }

    pub fn with_mic_index(mut self, mic_index: Option<u8>) -> Self {
        self.mic_index = mic_index;
        self
    }

    /// Tag this voice with its mono legato line (engine `LineId`).
    pub fn with_line(mut self, line: u8) -> Self {
        self.line = line;
        self
    }

    /// Tag this voice with the stem class of its source articulation.
    pub fn with_artic_class(mut self, class: ArticClass) -> Self {
        self.artic_class = class;
        self
    }

    pub fn with_choke_group(mut self, choke_group: Option<u64>) -> Self {
        self.choke_group = choke_group;
        self
    }

    pub fn with_pan(mut self, pan: f32) -> Self {
        let pan = pan.clamp(-1.0, 1.0);
        if self.data.channels >= 2 {
            // Stereo source: use a *balance* law so a centered (pan=0) stereo
            // sample passes L→L / R→R at unity. An equal-power law would apply
            // a spurious −3 dB at center to material that is already stereo —
            // which is how CSS (all-stereo) ships, and was making everything
            // 3 dB too quiet. Only attenuate the far channel as we pan off-center.
            self.pan_l = if pan <= 0.0 { 1.0 } else { 1.0 - pan };
            self.pan_r = if pan >= 0.0 { 1.0 } else { 1.0 + pan };
        } else {
            // Mono source: equal-power pan (constant perceived loudness).
            let theta = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            self.pan_l = theta.cos();
            self.pan_r = theta.sin();
        }
        self
    }

    /// Tag this voice as a zoned sustain dynamic layer (CC1/CC2 crossfade).
    pub fn with_dyn_layer(mut self, layer: DynLayer) -> Self {
        self.dyn_layer = Some(layer);
        self
    }

    /// Attach the decoded ENV_FLEX amplitude envelope (the instrument's real
    /// per-voice amp AHDSR). Multiplied into the output every frame.
    pub fn with_flex_env(mut self, flex: FlexEnv) -> Self {
        self.flex = Some(flex);
        self
    }

    /// Start at silence and ramp up to the spawn gain over `frames` — the
    /// attack envelope. `frames == 0` keeps the sample's natural attack.
    pub fn with_attack(mut self, frames: usize) -> Self {
        if frames > 0 {
            self.gain = 0.0; // target_gain stays at the intended spawn gain
            self.gain_ramp_frames = frames;
        }
        self
    }

    /// CSS legato handoff: spawn this (looping sustain) voice muted, wait
    /// `delay_frames`, then fade it in to its spawn gain over `fade_frames`.
    /// Mirrors the `CSS_W` helper (`wait($haa1x); fade_in($id, $0fznn)`) that
    /// brings the destination sustain up underneath the one-shot bow-change
    /// transition (spec §2.1 step 7). `delay_frames == 0 && fade_frames == 0`
    /// leaves the natural attack unchanged.
    pub fn with_fade_in_under(mut self, delay_frames: usize, fade_frames: usize) -> Self {
        if delay_frames > 0 || fade_frames > 0 {
            self.gain = 0.0; // target_gain stays at the intended spawn gain
            self.attack_delay = delay_frames;
            self.gain_ramp_frames = fade_frames;
        }
        self
    }

    pub fn with_sample_window(mut self, start_frame: usize, end_frame: Option<usize>) -> Self {
        let start = start_frame.min(self.data.num_frames);
        let end = end_frame
            .unwrap_or(self.data.num_frames)
            .min(self.data.num_frames)
            .max(start);
        self.start_frame = start;
        self.position = start as f64;
        self.end_frame = end;
        self
    }

    pub fn with_forward_loop(mut self, loop_start: usize, loop_end: usize) -> Self {
        let start = loop_start.min(self.end_frame);
        let end = loop_end.min(self.end_frame);
        if end > start + 1 {
            self.loop_range = Some((start, end));
        }
        self
    }

    pub fn with_alternating_loop(mut self, loop_start: usize, loop_end: usize) -> Self {
        self = self.with_forward_loop(loop_start, loop_end);
        if self.loop_range.is_some() {
            self.alternating_loop = true;
        }
        self
    }

    /// Enable a seamless forward-loop crossfade of `frames` frames. Clamped to
    /// the material available before `loop_start` and to half the loop length.
    /// No-op unless a forward (non-ping-pong) loop is set — ping-pong loops
    /// reflect the read position and are already continuous at the turnaround.
    pub fn with_loop_xfade(mut self, frames: usize) -> Self {
        if let Some((loop_start, loop_end)) = self.loop_range {
            if !self.alternating_loop && loop_end > loop_start {
                let head_room = loop_start.saturating_sub(self.start_frame);
                let loop_len = loop_end - loop_start;
                self.loop_xfade = frames.min(head_room).min(loop_len / 2);
            }
        }
        self
    }

    pub fn reversed(mut self) -> Self {
        self.reverse = true;
        self.loop_range = None;
        self.position = self.end_frame.saturating_sub(1) as f64;
        self
    }

    /// Schedule a gain ramp to `target` over `frames` frames.
    pub fn ramp_gain(&mut self, target: f32, frames: usize) {
        self.target_gain = target;
        self.gain_ramp_frames = frames;
    }

    /// Trigger note-off. Short notes and release samples play to completion.
    pub fn note_off(&mut self) {
        self.note_off_with_release_frames(self.release_frames);
    }

    pub fn note_off_with_release_frames(&mut self, release_frames: usize) {
        match self.kind {
            VoiceKind::Short | VoiceKind::Release => {
                // Play to end — do not release early.
            }
            _ => {
                if self.state == VoiceState::Playing {
                    let frames = release_frames.max(1);
                    // Update the divisor too — `next_frame` computes
                    // `env = frames_remaining / self.release_frames`, so if
                    // we set `frames_remaining` to a number larger than the
                    // voice's stored `release_frames` the envelope amplifies
                    // beyond 1.0 instead of fading. Sync them now.
                    self.release_frames = frames;
                    self.state = VoiceState::Releasing {
                        frames_remaining: frames,
                    };
                }
            }
        }
    }

    pub fn repedal(&mut self) {
        match self.kind {
            VoiceKind::Short | VoiceKind::Release | VoiceKind::Legato => {}
            _ => {
                if matches!(self.state, VoiceState::Releasing { .. }) {
                    self.state = VoiceState::Playing;
                }
            }
        }
    }

    /// Returns true when this voice should be removed from the pool.
    pub fn is_done(&self) -> bool {
        self.state == VoiceState::Done
    }

    /// Linearly-interpolated stereo read at fractional frame `pos`. Does not
    /// advance state; `pos` must be within the playable window.
    #[inline]
    fn read_interp(&self, pos: f64) -> (f32, f32) {
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let (l0, r0) = self.data.frame(idx);
        let (l1, r1) = self
            .data
            .frame((idx + 1).min(self.end_frame.saturating_sub(1)));
        (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
    }

    /// Render one stereo frame. Returns (L, R) and advances internal state.
    #[inline]
    pub fn next_frame(&mut self) -> (f32, f32) {
        if self.state == VoiceState::Done {
            return (0.0, 0.0);
        }

        // Delayed release: hold at full gain (Playing) until the countdown
        // elapses, then begin the fade. Lets a legato source note cover the
        // transition sample's pre-arrival dip, then hand off at the tick.
        if self.release_hold > 0 {
            self.release_hold -= 1;
            if self.release_hold == 0 && self.state == VoiceState::Playing {
                let f = self.pending_fade.max(1);
                self.release_frames = f;
                self.state = VoiceState::Releasing {
                    frames_remaining: f,
                };
            }
        }

        // Delayed attack (CSS_W fade-in-under): hold at silence until the wait
        // elapses (the read position still advances at the bottom of this
        // frame, so the loop stays phase-aligned), then the gain ramp brings
        // the sustain in underneath the transition.
        if self.attack_delay > 0 {
            self.attack_delay -= 1;
            self.gain = 0.0;
        } else if self.gain_ramp_frames > 0 {
            self.gain += (self.target_gain - self.gain) / self.gain_ramp_frames as f32;
            self.gain_ramp_frames -= 1;
        } else {
            self.gain = self.target_gain;
        }

        // Envelope
        let env = match &mut self.state {
            VoiceState::Playing => 1.0f32,
            VoiceState::Releasing { frames_remaining } => {
                if *frames_remaining == 0 {
                    self.state = VoiceState::Done;
                    return (0.0, 0.0);
                }
                let t = *frames_remaining as f32 / self.release_frames.max(1) as f32;
                *frames_remaining -= 1;
                t
            }
            VoiceState::Done => return (0.0, 0.0),
        };

        // Read sample with linear interpolation
        let frame_idx = self.position as usize;
        if frame_idx >= self.end_frame || frame_idx < self.start_frame {
            self.state = VoiceState::Done;
            return (0.0, 0.0);
        }

        let (mut l, mut r) = self.read_interp(self.position);

        // Seamless forward-loop crossfade. As the read approaches `loop_end`,
        // blend in the pre-`loop_start` material so the wrap has no
        // amplitude/phase discontinuity — otherwise a held/looped note clicks
        // once per loop. Ping-pong loops reflect the position and stay
        // continuous, so they are excluded.
        if self.loop_xfade > 0 && !self.reverse {
            if let Some((loop_start, loop_end)) = self.loop_range {
                if !self.alternating_loop {
                    let xf = self.loop_xfade as f64;
                    let seam = loop_end as f64 - xf;
                    if self.position >= seam {
                        let t = (((self.position - seam) / xf) as f32).clamp(0.0, 1.0);
                        // An equal distance before `loop_start` as we are before
                        // `loop_end`; at the wrap point this lands on `loop_start`.
                        let wrap_pos = loop_start as f64 - (loop_end as f64 - self.position);
                        if wrap_pos >= self.start_frame as f64 {
                            let (wl, wr) = self.read_interp(wrap_pos);
                            // Equal-power crossfade: the loop tail and the
                            // pre-`loop_start` material are decorrelated, so a
                            // linear (amplitude) blend would dip ~3 dB at the
                            // midpoint — a pulse every loop. Constant-power sin/cos
                            // gains hold the level steady across the seam.
                            let (fi, fo) = (t * std::f32::consts::FRAC_PI_2).sin_cos();
                            l = l * fo + wl * fi;
                            r = r * fo + wr * fi;
                        }
                    }
                }
            }
        }

        // Decoded ENV_FLEX amplitude envelope. Freezes at the sustain-hold
        // point while Playing (held note stays steady); advances through the
        // decay once Releasing. Multiplied on top of the note-off release fade.
        let flex = match &mut self.flex {
            Some(f) => f.next(!matches!(self.state, VoiceState::Playing)),
            None => 1.0,
        };

        let amp = self.gain * env * flex;

        // Advance position
        if self.reverse {
            self.position -= self.rate;
        } else {
            self.position += self.rate;
        }
        // Keep looping while Releasing too — a looped body (sustain/legato) must
        // fade out over its loop, not stop looping and run forward into the
        // sample's abrupt end (which clicks → "note-off noise"). Only a Done
        // voice ignores the loop.
        if matches!(
            self.state,
            VoiceState::Playing | VoiceState::Releasing { .. }
        ) {
            if let Some((loop_start, loop_end)) = self.loop_range {
                if self.alternating_loop {
                    if !self.reverse && self.position >= loop_end as f64 {
                        self.position =
                            (loop_end.saturating_sub(2)) as f64 - (self.position - loop_end as f64);
                        self.reverse = true;
                    } else if self.reverse && self.position < loop_start as f64 {
                        self.position = loop_start as f64 + (loop_start as f64 - self.position);
                        self.reverse = false;
                    }
                } else if !self.reverse && self.position >= loop_end as f64 {
                    let len = (loop_end - loop_start) as f64;
                    self.position = loop_start as f64 + (self.position - loop_end as f64) % len;
                }
            }
        }

        (
            flush_denormal(l * amp * self.pan_l),
            flush_denormal(r * amp * self.pan_r),
        )
    }

    /// Render a block of stereo frames into `output` (interleaved L/R).
    /// Returns the number of frames rendered (may be less if sample ends).
    pub fn render_block(&mut self, output: &mut [f32]) -> usize {
        let num_frames = output.len() / 2;
        let mut rendered = 0;
        for i in 0..num_frames {
            let (l, r) = self.next_frame();
            output[i * 2] += l;
            output[i * 2 + 1] += r;
            rendered += 1;
            if self.is_done() {
                break;
            }
        }
        rendered
    }
}

/// Flush denormals to zero. On `no_std`/embedded and WASM targets the host may
/// not set hardware FTZ/DAZ, so a denormal on a quiet release tail can spike
/// CPU. The branch is predictable (almost always false) and cheap.
#[inline(always)]
fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-30 { 0.0 } else { x }
}

// ── Voice pool ────────────────────────────────────────────────────────────────

/// Maximum simultaneous voices before stealing.
const MAX_VOICES: usize = 64;
const STEAL_FADE_FRAMES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceStealPolicy {
    ReleaseFirstQuietest,
    Oldest,
    Quietest,
    SameNoteFirst,
    DropNew,
}

impl VoiceStealPolicy {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "oldest" => Self::Oldest,
            "quietest" => Self::Quietest,
            "same-note" | "same_note" | "same-note-first" | "same_note_first" => {
                Self::SameNoteFirst
            }
            "none" | "drop" | "drop-new" | "drop_new" => Self::DropNew,
            _ => Self::ReleaseFirstQuietest,
        }
    }
}

/// Pool of active voices with bounded polyphony and voice stealing.
pub struct VoicePool {
    voices: Vec<Voice>,
    stolen: usize,
    max_voices: usize,
    steal_policy: VoiceStealPolicy,
}

impl VoicePool {
    pub fn new() -> Self {
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            stolen: 0,
            max_voices: MAX_VOICES,
            steal_policy: VoiceStealPolicy::ReleaseFirstQuietest,
        }
    }

    pub fn with_max_voices(max_voices: usize) -> Self {
        let capacity = max_voices.max(1).min(MAX_VOICES);
        Self {
            voices: Vec::with_capacity(capacity),
            stolen: 0,
            max_voices: max_voices.max(1),
            steal_policy: VoiceStealPolicy::ReleaseFirstQuietest,
        }
    }

    pub fn set_max_voices(&mut self, max_voices: usize) {
        self.max_voices = max_voices.max(1);
        self.enforce_limit();
    }

    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    pub fn set_steal_policy(&mut self, policy: VoiceStealPolicy) {
        self.steal_policy = policy;
    }

    pub fn steal_policy(&self) -> VoiceStealPolicy {
        self.steal_policy
    }

    /// Add a voice, stealing one or more voices if at capacity.
    pub fn spawn(&mut self, voice: Voice) {
        // Remove done voices first
        self.voices.retain(|v| !v.is_done());

        if self.voices.len() >= self.max_voices {
            if self.steal_one_for(&voice) == StealOutcome::DropIncoming {
                return;
            }
        }
        self.voices.push(voice);
    }

    /// Send note-off to all voices playing `note` (except one-shot kinds).
    pub fn note_off(&mut self, note: u8) {
        self.note_off_with_release_frames(note, None);
    }

    pub fn note_off_with_release_frames(&mut self, note: u8, release_frames: Option<usize>) {
        for v in &mut self.voices {
            if v.note == note {
                match release_frames {
                    Some(frames) => v.note_off_with_release_frames(frames),
                    None => v.note_off(),
                }
            }
        }
    }

    /// Line-scoped note-off: only voices spawned by `line` release, so divisi
    /// lines playing the same pitch don't cut each other off. Live/single-line
    /// play (everything on line 0) behaves exactly like
    /// [`note_off_with_release_frames`](Self::note_off_with_release_frames).
    pub fn note_off_line(&mut self, line: u8, note: u8, release_frames: Option<usize>) {
        for v in &mut self.voices {
            if v.note == note && v.line == line {
                match release_frames {
                    Some(frames) => v.note_off_with_release_frames(frames),
                    None => v.note_off(),
                }
            }
        }
    }

    /// Send note-off to every active voice.
    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.note_off();
        }
    }

    pub fn repedal_releasing(&mut self) -> usize {
        let mut restored = 0;
        for voice in &mut self.voices {
            if matches!(voice.state, VoiceState::Releasing { .. }) {
                voice.repedal();
                if matches!(voice.state, VoiceState::Playing) {
                    restored += 1;
                }
            }
        }
        restored
    }

    pub fn panic(&mut self) {
        self.voices.clear();
    }

    /// Silence all voices for `note` immediately (used when legato transition fires).
    pub fn silence_note(&mut self, note: u8, fade_frames: usize) {
        self.silence_note_filtered(note, 0, fade_frames, None);
    }

    /// Line-scoped variant of [`silence_note`](Self::silence_note): a legato
    /// transition firing on one divisi line must not fade a unison note held
    /// by another line.
    pub fn silence_note_line(&mut self, line: u8, note: u8, fade_frames: usize) {
        self.silence_note_filtered(note, 0, fade_frames, Some(line));
    }

    /// Delayed line-scoped silence: hold the old note at full for `hold_frames`
    /// (covering the transition sample's pre-arrival dip), then fade over
    /// `fade_frames`. Fills the inter-note tick without a long source-pitch tail.
    pub fn silence_note_line_after(
        &mut self,
        line: u8,
        note: u8,
        hold_frames: usize,
        fade_frames: usize,
    ) {
        self.silence_note_filtered(note, hold_frames, fade_frames, Some(line));
    }

    /// CSS legato retire (spec §2.1 step 4): fade out the PREVIOUS pair on
    /// `line` for `note` with the transition and sustain voices on SEPARATE
    /// crossfade times (real persistent values `$fjtlu…`/`$tdjzq…`). Long
    /// overlapping fades — the previous 30 ms single fade caused the inter-note
    /// tick. `Legato` (transition) voices fade over `trans_fade`; sustain-layer
    /// voices over `sus_fade`.
    pub fn retire_note_line(&mut self, line: u8, note: u8, trans_fade: usize, sus_fade: usize) {
        for v in &mut self.voices {
            if v.note != note || v.line != line {
                continue;
            }
            let fade = match v.kind {
                VoiceKind::Legato => trans_fade,
                VoiceKind::SustainNVLo
                | VoiceKind::SustainNVHi
                | VoiceKind::SustainVibLo
                | VoiceKind::SustainVibHi
                | VoiceKind::SustainLo
                | VoiceKind::SustainHi
                | VoiceKind::SustainLayer => sus_fade,
                _ => continue,
            };
            let fade = fade.max(1);
            v.ramp_gain(0.0, fade);
            v.release_frames = fade;
            v.state = VoiceState::Releasing {
                frames_remaining: fade,
            };
        }
    }

    fn silence_note_filtered(
        &mut self,
        note: u8,
        hold_frames: usize,
        fade_frames: usize,
        line: Option<u8>,
    ) {
        for v in &mut self.voices {
            if v.note == note
                && line.is_none_or(|l| v.line == l)
                && matches!(
                    v.kind,
                    VoiceKind::SustainNVLo
                        | VoiceKind::SustainNVHi
                        | VoiceKind::SustainVibLo
                        | VoiceKind::SustainVibHi
                        | VoiceKind::SustainLo
                        | VoiceKind::SustainHi
                        | VoiceKind::SustainLayer
                        // A held legato TRANSITION voice loops its tail to
                        // carry the arrived note — it is the sounding note on
                        // its line and must crossfade out on the next legato
                        // move exactly like a sustain body would.
                        | VoiceKind::Legato
                )
            {
                if hold_frames > 0 {
                    // Hold at full, then fade — the env (release_frames = fade)
                    // carries the fade once the hold elapses.
                    v.release_hold = hold_frames;
                    v.pending_fade = fade_frames.max(1);
                } else {
                    v.ramp_gain(0.0, fade_frames);
                    v.release_frames = fade_frames.max(1);
                    v.state = VoiceState::Releasing {
                        frames_remaining: fade_frames,
                    };
                }
            }
        }
    }

    pub fn silence_choke_group(&mut self, group: u64, fade_frames: usize) {
        for v in &mut self.voices {
            if v.choke_group == Some(group) && v.state != VoiceState::Done {
                v.ramp_gain(0.0, fade_frames);
                v.state = VoiceState::Releasing {
                    frames_remaining: fade_frames,
                };
            }
        }
    }

    pub fn active_choke_group_count(&self, group: u64) -> usize {
        self.voices
            .iter()
            .filter(|v| v.choke_group == Some(group) && v.state != VoiceState::Done)
            .count()
    }

    /// Render all active voices into an interleaved stereo buffer.
    pub fn render(&mut self, output: &mut [f32]) {
        for v in &mut self.voices {
            v.render_block(output);
        }
        self.voices.retain(|v| !v.is_done());
    }

    /// Render voices into per-mic stereo buffers. Each voice writes only to
    /// the buffer at its `mic_index`. Voices with `mic_index == None` (or
    /// out-of-bounds) fold into buffer 0. **No allocation** — caller owns
    /// the `Vec<Vec<f32>>` storage.
    pub fn render_multi(&mut self, outputs: &mut [Vec<f32>]) {
        if outputs.is_empty() {
            return;
        }
        let nmics = outputs.len();
        for v in &mut self.voices {
            let idx = match v.mic_index {
                Some(i) if (i as usize) < nmics => i as usize,
                _ => 0,
            };
            v.render_block(&mut outputs[idx]);
        }
        self.voices.retain(|v| !v.is_done());
    }

    /// Render voices into a flat (bus × mic) matrix of stereo buffers:
    /// `outputs[bus * nmics + mic]`. Each voice routes to the bus of its
    /// [`ArticClass`] (`route[0]` = Longs, `route[1]` = Shorts; out-of-range
    /// folds to bus 0) and to its `mic_index` within that bus (like
    /// [`render_multi`](Self::render_multi)). Voices are visited in exactly
    /// the same order as `render`/`render_multi`, so when both classes route
    /// to the same bus the per-buffer accumulation — and therefore the audio,
    /// bit for bit — is identical to the unsplit render.
    pub fn render_matrix(&mut self, outputs: &mut [Vec<f32>], nmics: usize, route: [usize; 2]) {
        if outputs.is_empty() || nmics == 0 {
            return;
        }
        let nbuses = outputs.len() / nmics;
        for v in &mut self.voices {
            let want = match v.artic_class {
                ArticClass::Longs => route[0],
                ArticClass::Shorts => route[1],
            };
            let bus = if want < nbuses { want } else { 0 };
            let mic = match v.mic_index {
                Some(i) if (i as usize) < nmics => i as usize,
                _ => 0,
            };
            v.render_block(&mut outputs[bus * nmics + mic]);
        }
        self.voices.retain(|v| !v.is_done());
    }

    pub fn active_count(&self) -> usize {
        self.voices.len()
    }

    pub fn stolen_count(&self) -> usize {
        self.stolen
    }

    /// Mutable iterator over all active voices (used by engine for CC1 updates).
    pub fn voices_mut(&mut self) -> &mut Vec<Voice> {
        &mut self.voices
    }

    fn enforce_limit(&mut self) {
        self.voices.retain(|v| !v.is_done());
        while self.voices.len() > self.max_voices {
            if self.steal_one_for_note(None) == StealOutcome::DropIncoming {
                break;
            }
        }
    }

    fn steal_one_for(&mut self, incoming: &Voice) -> StealOutcome {
        self.steal_one_for_note(Some(incoming.note))
    }

    fn steal_one_for_note(&mut self, incoming_note: Option<u8>) -> StealOutcome {
        if self.steal_policy == VoiceStealPolicy::DropNew {
            return StealOutcome::DropIncoming;
        }

        self.stolen = self.stolen.saturating_add(1);
        if let Some(idx) = self.steal_index(incoming_note) {
            self.voices.remove(idx);
        } else if let Some(idx) = quietest_stealable_voice(&self.voices) {
            self.voices[idx].ramp_gain(0.0, STEAL_FADE_FRAMES);
            self.voices[idx].state = VoiceState::Releasing {
                frames_remaining: STEAL_FADE_FRAMES,
            };
        }
        StealOutcome::StoleExisting
    }

    fn steal_index(&self, incoming_note: Option<u8>) -> Option<usize> {
        match self.steal_policy {
            VoiceStealPolicy::ReleaseFirstQuietest => self
                .voices
                .iter()
                .position(|v| matches!(v.state, VoiceState::Releasing { .. })),
            VoiceStealPolicy::Oldest => Some(0),
            VoiceStealPolicy::Quietest => None,
            VoiceStealPolicy::SameNoteFirst => incoming_note
                .and_then(|note| self.voices.iter().position(|v| v.note == note))
                .or_else(|| {
                    self.voices
                        .iter()
                        .position(|v| matches!(v.state, VoiceState::Releasing { .. }))
                }),
            VoiceStealPolicy::DropNew => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StealOutcome {
    StoleExisting,
    DropIncoming,
}

fn quietest_stealable_voice(voices: &[Voice]) -> Option<usize> {
    voices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.kind != VoiceKind::Release)
        .min_by(|(_, a), (_, b)| {
            a.gain
                .partial_cmp(&b.gain)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Arc<SampleData> {
        Arc::new(SampleData {
            frames: Arc::new(vec![0.0; 32]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 32,
        })
    }

    fn voice(gain: f32) -> Voice {
        Voice::new(sample(), 60, VoiceKind::SustainLo, 0, gain, 128)
    }

    #[test]
    fn panic_clears_active_voices() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0));
        assert_eq!(pool.active_count(), 1);

        pool.panic();

        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn voice_steal_counts_and_fades_old_voice() {
        let mut pool = VoicePool::new();
        for _ in 0..MAX_VOICES {
            pool.spawn(voice(1.0));
        }

        pool.spawn(voice(1.0));

        assert_eq!(pool.stolen_count(), 1);
        assert_eq!(pool.active_count(), MAX_VOICES + 1);
        assert!(
            pool.voices
                .iter()
                .any(|v| matches!(v.state, VoiceState::Releasing { .. }))
        );
    }

    #[test]
    fn configurable_polyphony_limits_spawned_voices() {
        let mut pool = VoicePool::with_max_voices(2);

        pool.spawn(voice(1.0));
        pool.spawn(voice(0.8));
        pool.spawn(voice(0.6));

        assert_eq!(pool.max_voices(), 2);
        assert_eq!(pool.stolen_count(), 1);
        assert_eq!(pool.active_count(), 3);
        assert!(
            pool.voices
                .iter()
                .any(|v| matches!(v.state, VoiceState::Releasing { .. }))
        );
    }

    #[test]
    fn lowering_polyphony_enforces_limit_immediately() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0));
        pool.spawn(voice(0.8));
        pool.spawn(voice(0.6));

        pool.set_max_voices(1);

        assert_eq!(pool.max_voices(), 1);
        assert!(pool.stolen_count() >= 2);
    }

    #[test]
    fn drop_new_policy_rejects_voice_at_limit() {
        let mut pool = VoicePool::with_max_voices(1);
        pool.set_steal_policy(VoiceStealPolicy::DropNew);

        pool.spawn(voice(1.0));
        pool.spawn(voice(0.5));

        assert_eq!(pool.active_count(), 1);
        assert_eq!(pool.stolen_count(), 0);
    }

    #[test]
    fn parses_voice_steal_policy_names() {
        assert_eq!(
            VoiceStealPolicy::from_str("same-note-first"),
            VoiceStealPolicy::SameNoteFirst
        );
        assert_eq!(
            VoiceStealPolicy::from_str("oldest"),
            VoiceStealPolicy::Oldest
        );
        assert_eq!(
            VoiceStealPolicy::from_str("none"),
            VoiceStealPolicy::DropNew
        );
        assert_eq!(
            VoiceStealPolicy::from_str(""),
            VoiceStealPolicy::ReleaseFirstQuietest
        );
    }

    #[test]
    fn choke_group_releases_existing_group_voices() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0).with_choke_group(Some(7)));
        pool.spawn(voice(1.0).with_choke_group(Some(9)));

        pool.silence_choke_group(7, 16);

        assert!(matches!(
            pool.voices[0].state,
            VoiceState::Releasing {
                frames_remaining: 16
            }
        ));
        assert_eq!(pool.voices[1].state, VoiceState::Playing);
    }

    #[test]
    fn counts_active_choke_group_voices() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0).with_choke_group(Some(7)));
        pool.spawn(voice(1.0).with_choke_group(Some(7)));
        pool.spawn(voice(1.0).with_choke_group(Some(9)));

        assert_eq!(pool.active_choke_group_count(7), 2);
        assert_eq!(pool.active_choke_group_count(9), 1);
        assert_eq!(pool.active_choke_group_count(1), 0);
    }

    #[test]
    fn sample_window_starts_and_stops_voice() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![0.0, 1.0, 2.0, 3.0]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 4,
        });
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8)
            .with_sample_window(1, Some(3));

        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 0.0);
        assert!(voice.is_done());
    }

    #[test]
    fn forward_loop_wraps_while_playing() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![0.0, 1.0, 2.0, 3.0]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 4,
        });
        let mut voice =
            Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8).with_forward_loop(1, 3);

        assert_eq!(voice.next_frame().0, 0.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert!(!voice.is_done());
    }

    #[test]
    fn alternating_loop_ping_pongs_between_points() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![0.0, 1.0, 2.0, 3.0]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 4,
        });
        let mut voice =
            Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8).with_alternating_loop(1, 3);

        assert_eq!(voice.next_frame().0, 0.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert!(!voice.is_done());
    }

    #[test]
    fn reverse_playback_walks_sample_window_backwards() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![0.0, 1.0, 2.0, 3.0]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 4,
        });
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8)
            .with_sample_window(1, Some(4))
            .reversed();

        assert_eq!(voice.next_frame().0, 3.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 0.0);
        assert!(voice.is_done());
    }

    #[test]
    fn one_shot_voice_ignores_note_off() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![0.0, 1.0, 2.0]),
            channels: 1,
            sample_rate: 48_000,
            num_frames: 3,
        });
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Short, 1.0, 1.0, 8);

        voice.note_off();

        assert_eq!(voice.state, VoiceState::Playing);
        assert_eq!(voice.next_frame().0, 0.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
    }

    #[test]
    fn repedal_restores_releasing_sustain_voices() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0));

        pool.note_off(60);
        assert!(matches!(pool.voices[0].state, VoiceState::Releasing { .. }));

        assert_eq!(pool.repedal_releasing(), 1);
        assert_eq!(pool.voices[0].state, VoiceState::Playing);
    }

    #[test]
    fn note_off_can_override_release_frames() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0));

        pool.note_off_with_release_frames(60, Some(512));

        assert!(matches!(
            pool.voices[0].state,
            VoiceState::Releasing {
                frames_remaining: 512
            }
        ));
    }

    #[test]
    fn voice_pan_applies_equal_power_gains() {
        let data = Arc::new(SampleData {
            frames: Arc::new(vec![1.0, 1.0]),
            channels: 2,
            sample_rate: 48_000,
            num_frames: 1,
        });

        let mut left =
            Voice::with_rate(Arc::clone(&data), 60, VoiceKind::Zoned, 1.0, 1.0, 8).with_pan(-1.0);
        let mut right = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8).with_pan(1.0);

        assert_eq!(left.next_frame(), (1.0, 0.0));
        assert!({
            let (l, r) = right.next_frame();
            l.abs() < 1e-6 && (r - 1.0).abs() < 1e-6
        });
    }

    // ── Anti-click / anti-pop regression guards ──────────────────────────────
    //
    // A click/pop is an abrupt inter-sample step far above what the waveform's
    // own slope can produce. Two mechanisms prevent them, and these tests hold
    // them in place:
    //   * a short onset declick (`with_attack`) on legato / mid-sample voices —
    //     without it every note change steps from silence to a non-zero sample;
    //   * the forward-loop crossfade (`with_loop_xfade`) — without it a held
    //     note clicks at every loop wrap.

    /// Steady DC-offset stereo tone. Every sample is non-zero (offset by `dc`),
    /// so starting playback mid-sample produces a hard onset step unless the
    /// voice is declicked.
    fn click_tone(sr: u32, n: usize, freq: f32, dc: f32, amp: f32) -> Arc<SampleData> {
        let mut frames = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = amp * (core::f32::consts::TAU * freq * t).sin() + dc;
            frames.push(s);
            frames.push(s);
        }
        Arc::new(SampleData {
            frames: Arc::new(frames),
            channels: 2,
            sample_rate: sr,
            num_frames: n,
        })
    }

    /// Largest absolute inter-sample jump of the mono-summed buffer, counting
    /// the implicit pre-roll as silence so an onset step registers.
    fn max_step(buf: &[f32]) -> f32 {
        let mut prev = 0.0f32;
        let mut mx = 0.0f32;
        for c in buf.chunks(2) {
            let s = 0.5 * (c[0] + c[1]);
            mx = mx.max((s - prev).abs());
            prev = s;
        }
        mx
    }

    #[test]
    fn legato_onset_declick_removes_step() {
        let sr = 48_000;
        let data = click_tone(sr, sr as usize / 10, 220.0, 0.5, 0.4); // samples in [0.1, 0.9]
        let declick = sr as usize * 6 / 1000; // 6 ms
        // Start mid-sample, as a prefired legato transition does (start_offset).
        let mut declicked = Voice::with_rate(Arc::clone(&data), 60, VoiceKind::Legato, 1.0, 1.0, 8)
            .with_sample_window(1000, None)
            .with_attack(declick);
        let mut raw = Voice::with_rate(data, 60, VoiceKind::Legato, 1.0, 1.0, 8)
            .with_sample_window(1000, None);
        let mut a = vec![0.0f32; 512 * 2];
        let mut b = vec![0.0f32; 512 * 2];
        declicked.render_block(&mut a);
        raw.render_block(&mut b);
        // Control: without a declick the onset steps hard (≥ the DC floor).
        assert!(
            max_step(&b) > 0.08,
            "control onset step too small: {}",
            max_step(&b)
        );
        // Declicked: no click — deltas stay near the waveform's own slope.
        assert!(
            max_step(&a) < 0.02,
            "declicked legato onset clicked: {}",
            max_step(&a)
        );
    }

    #[test]
    fn forward_loop_crossfade_has_no_wrap_click() {
        let sr = 48_000;
        let n = sr as usize / 4;
        // loop_start / loop_end land on different phases, so a hard modulo wrap
        // would step; the crossfade must smooth it.
        let data = click_tone(sr, n, 110.0, 0.0, 0.5);
        let mut v = Voice::with_rate(data, 60, VoiceKind::SustainLayer, 1.0, 1.0, 1_000_000)
            .with_forward_loop(2000, 6000)
            .with_loop_xfade(sr as usize * 50 / 1000);
        let mut out = vec![0.0f32; 30_000 * 2]; // several loop periods
        v.render_block(&mut out);
        let maxd = out
            .chunks(2)
            .map(|c| 0.5 * (c[0] + c[1]))
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        // The tone's own max slope is ~TAU*110/sr*0.5 ≈ 0.007; a wrap
        // discontinuity would be ~0.5+. Generous bound, well clear of both.
        assert!(maxd < 0.05, "loop wrap clicked: max delta {maxd}");
    }
}
