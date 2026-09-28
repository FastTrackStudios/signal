//! Voice — a single playing sample with pitch shifting and amplitude envelope.
//!
//! Pitch shifting is implemented as a playback rate change with linear
//! interpolation between frames. A pitch shift of N semitones multiplies
//! the playback rate by `2^(N/12)`.
//!
//! The amplitude envelope:
//! - Attack: a ramp from silence to `gain` (`with_attack`)
//! - Decay / sustain: after the attack, a ramp from full level to the
//!   sustain level over the decay time (`with_decay`; off by default)
//! - Releasing: linear fade to zero over `release_frames`, from wherever the
//!   decay had got to
//!
//! CSS sustain samples have their own natural releases baked in; we do not
//! apply an envelope to them. Release samples are played to completion at
//! reduced gain.

use std::sync::Arc;

use super::cache::SampleData;
use crate::native::{Adsr, AdsrParams};

/// A voice's filter envelope: its own ADSR driving its own lowpass, so every
/// note opens and closes on its own (a module-wide envelope would close the
/// filter on every ringing note at one note's release).
///
/// The lowpass only ever sits at or below the layer's cutoff knob, so it
/// never fights the layer's tone filter: `amount` > 0 closes it by up to
/// [`FILTER_ENV_OCTAVES`] × `amount` below the cutoff and lets the envelope
/// open it back up; `amount` < 0 closes it as the envelope rises.
#[derive(Debug, Clone)]
pub struct VoiceFilter {
    env: Adsr,
    amount: f32,
    base_hz: f32,
    /// Key-tracking offset in octaves (see `with_keytrack`).
    key_octaves: f32,
    sample_rate: f32,
    /// Two-pole state-variable lowpass, one per channel: (ic1, ic2).
    state: [(f32, f32); 2],
    g: f32,
    k: f32,
    /// Coefficients are recomputed every [`FILTER_COEF_EVERY`] frames.
    countdown: u16,
}

/// The state-variable filter's damping for a 0..1 resonance knob.
fn resonance_k(resonance: f32) -> f32 {
    std::f32::consts::SQRT_2 - (std::f32::consts::SQRT_2 - 0.1) * resonance.clamp(0.0, 1.0)
}

/// A voice's vibrato settings (see [`Voice::with_vibrato`]).
#[derive(Debug, Clone, Copy)]
pub struct Vibrato {
    pub rate_hz: f32,
    pub cents: f32,
    pub delay_frames: u32,
    pub sample_rate: f32,
}

/// Octaves the filter envelope sweeps at full amount.
pub const FILTER_ENV_OCTAVES: f32 = 7.0;
const FILTER_COEF_EVERY: u16 = 16;

impl VoiceFilter {
    #[must_use]
    pub fn new(sample_rate: f32, params: AdsrParams, amount: f32, base_hz: f32) -> Self {
        let mut env = Adsr::new(sample_rate, params);
        env.note_on();
        Self {
            env,
            amount: amount.clamp(-1.0, 1.0),
            base_hz,
            key_octaves: 0.0,
            sample_rate,
            state: [(0.0, 0.0); 2],
            g: 0.0,
            k: std::f32::consts::SQRT_2,
            countdown: 0,
        }
    }

    /// Resonance 0..1 (0 = Butterworth, 1 ≈ Q 10). While a voice filter is
    /// on it is the layer's filter, so it carries the resonance knob.
    #[must_use]
    pub fn with_resonance(mut self, resonance: f32) -> Self {
        self.k = resonance_k(resonance);
        self
    }

    /// Key tracking: the cutoff moves `keytrack` octaves per octave played,
    /// around middle C (60).
    #[must_use]
    pub fn with_keytrack(mut self, keytrack: f32, note: u8) -> Self {
        self.key_octaves = keytrack.clamp(0.0, 1.0) * (f32::from(note) - 60.0) / 12.0;
        self
    }

    /// Follow the layer's cutoff, amount and resonance while the note is held.
    pub fn set_base(&mut self, base_hz: f32, amount: f32, resonance: f32) {
        self.base_hz = base_hz;
        self.amount = amount.clamp(-1.0, 1.0);
        self.k = resonance_k(resonance);
    }

    fn note_off(&mut self) {
        self.env.note_off();
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let e = self.env.tick();
        if self.countdown == 0 {
            self.countdown = FILTER_COEF_EVERY;
            let octaves = if self.amount >= 0.0 {
                self.amount * (e - 1.0)
            } else {
                self.amount * e
            } * FILTER_ENV_OCTAVES
                + self.key_octaves;
            let hz = (self.base_hz * octaves.exp2()).clamp(20.0, self.sample_rate * 0.45);
            self.g = (std::f32::consts::PI * hz / self.sample_rate).tan();
        }
        self.countdown -= 1;
        let (g, k) = (self.g, self.k);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let mut out = [l, r];
        for (x, (ic1, ic2)) in out.iter_mut().zip(self.state.iter_mut()) {
            let v3 = *x - *ic2;
            let v1 = a1 * *ic1 + a2 * v3;
            let v2 = *ic2 + a2 * *ic1 + a3 * v3;
            *ic1 = 2.0 * v1 - *ic1;
            *ic2 = 2.0 * v2 - *ic2;
            *x = v2;
        }
        (out[0], out[1])
    }
}

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
/// stem-aware output buses (see `docs/plan/document-mode.md`, "Stem-aware output buses").
///
/// Longs: sustain/legato/tremolo bodies + their releases.
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
    #[must_use]
    pub fn trace_name(&self) -> &'static str {
        match self {
            Self::SustainNVLo => "SustainNVLo",
            Self::SustainNVHi => "SustainNVHi",
            Self::SustainVibLo => "SustainVibLo",
            Self::SustainVibHi => "SustainVibHi",
            Self::SustainLo => "SustainLo",
            Self::SustainHi => "SustainHi",
            Self::SustainLayer => "SustainLayer",
            Self::Legato => "Legato",
            Self::Release => "Release",
            Self::Short => "Short",
            Self::Zoned => "Zoned",
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
    /// Holds the loop region of a STREAMED sample resident while this voice
    /// sounds. A loop reads the same frames forever and wraps backwards, so
    /// nothing else in the streamer would keep them — see
    /// [`crate::engine::stream::LoopPin`]. `None` for resident samples.
    stream_pin: Option<crate::engine::stream::LoopPin>,

    /// Seamless forward-loop crossfade length in frames. As the read
    /// approaches `loop_end`, the pre-`loop_start` material is blended in over
    /// this many frames so the wrap carries no amplitude/phase discontinuity
    /// (the click heard on a held/looped note). `0` = hard wrap, no fade.
    loop_xfade: usize,

    /// Playback rate. 1.0 = original pitch, 2^(semitones/12) for transposition.
    rate: f64,

    /// Live pitch-bend factor multiplied onto the playback rate each frame
    /// (1.0 = no bend). Set by the pool on spawn and on wheel moves, so a
    /// bend reaches held voices and new ones alike.
    bend: f64,

    /// Output gain [0.0, 1.0].
    pub gain: f32,
    pan_l: f32,
    pan_r: f32,

    /// Target gain for CC1 crossfade blend (updated per render block).
    pub target_gain: f32,

    /// Gain smoothing — frames to ramp from `gain` to `target_gain`.
    gain_ramp_frames: usize,

    /// CSS two-stage destination swell (`$ocjln=6`, KSP §3.5). When
    /// `stage1_run > 0`, the gain ramp runs for `stage1_run` frames at the
    /// `gain_ramp_frames` (stage-1 `$mlnoy`) rate — reaching `stage1_run/$mlnoy`
    /// of target (the `$x444h`=90 divisor makes stage-1 *overshoot* a linear
    /// split, ~90 %) — then switches to a slower ramp of `stage2_frames`
    /// (`$rixqv`) that completes to target. `0` = single-stage (`with_fade_in_under`).
    stage1_run: usize,
    stage2_frames: usize,

    /// Portamento micro-glide (CSS `$upjkh`/`$ma0b1`, KSP §3.2/§3.5): the voice
    /// ramps its pitch from `glide_cents` to `glide_end` over `glide_frames`
    /// (`$1mwwo`≈60 ms). Incoming voices ramp `-jyttf → 0` (scoop into pitch);
    /// outgoing (retiring) voices ramp `0 → +jyttf` (bend toward the new note as
    /// they fade). Applied as a small rate modulation (the pitch drift over
    /// 60 ms is far below the timing floor). `glide_frames == 0` = no glide.
    glide_cents: f32,
    glide_end: f32,
    glide_step: f32,
    glide_frames: usize,

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

    /// True start delay: while > 0 the voice outputs silence and its read
    /// position does NOT advance — the sample genuinely starts later. This is
    /// the per-zone arrival alignment (a zone whose measured heard-arrival is
    /// SHORTER than the schedule's pre-roll is held back so the arrival still
    /// lands exactly on the tick). Unlike `attack_delay` (which mutes while
    /// the position advances, keeping a loop phase-aligned), nothing of the
    /// sample is consumed during the hold. `0` = start immediately.
    start_hold: usize,

    /// Playback-emitted ARRIVAL marker (r[signal.sampling.markers.arrival],
    /// emitted-by-playback): the zone's heard-arrival position in FILE
    /// frames. When the voice's real playhead crosses it — after every
    /// start-offset skip, start hold, and rate scaling — the voice records
    /// the OUTPUT frame it happened on (`spawn_frame + frames_out`). A
    /// marker exists because the voice actually played through that sample
    /// position at that output moment; nothing is estimated. `None` = the
    /// zone carries no marker.
    marker_arrival_file: Option<f64>,
    /// Engine `frames_rendered` at spawn — the base for stamping emissions
    /// with absolute output frames.
    spawn_frame: u64,
    /// Output frames this voice has produced (holds included — they occupy
    /// real output time).
    frames_out: u64,
    /// The emitted arrival (absolute output frame), once crossed. Drained by
    /// the engine after each block via `take_emitted_arrival`.
    arrival_emitted: Option<u64>,
    arrival_drained: bool,

    /// Slow secondary bloom (CSS `%1wcdh`): a multiplicative gain ramp from
    /// 1.0 to `bloom_target` over `bloom_frames`, running AFTER the start
    /// hold / attack delay elapse. Multiplicative and separate from the
    /// `gain`/`target_gain` machinery, so CC1/CC2 re-levelling
    /// (`update_sustain_gains`) never cancels it. `bloom_frames == 0` = off.
    bloom_frames: usize,
    /// Decay / sustain stage (`with_decay`): a multiplier that holds at 1.0
    /// through the attack, then moves to `sustain` over `decay_left` frames.
    /// 1.0 / 0 frames = off, so a voice without it is unchanged.
    ds_level: f32,
    sustain: f32,
    decay_left: usize,
    /// A breakpoint amplitude envelope (`with_amp_points` — an Omnisphere
    /// layer's AENV), in place of the linear release fade: it holds at its
    /// sustain point while the key is down and runs its own release after.
    amp_bp: Option<(Arc<crate::native::breakpoints::Breakpoints>, crate::native::breakpoints::EnvPlayer, f32)>,
    /// Per-voice filter envelope (`with_filter_env`); `None` = no filter.
    filter: Option<VoiceFilter>,
    /// Vibrato (`with_vibrato`): phase and step (cycles/frame), depth in
    /// cents, frames left before it starts, and its fade-in.
    vib_phase: f32,
    vib_inc: f32,
    vib_cents: f32,
    vib_wait: u32,
    vib_fade: f32,
    vib_fade_step: f32,
    bloom_total: usize,
    bloom_target: f32,

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

    /// Decoded `ENV_FLEX` amplitude envelope (the instrument's real per-voice amp
    /// AHDSR), multiplied into the output. `None` = flat unity (legacy voices /
    /// families with no decoded envelope).
    flex: Option<FlexEnv>,

    /// This voice's private window into a streamed sample.
    ///
    /// The whole point of streaming architecture (HISE's two swapped buffers
    /// per voice, JUCE's per-voice fill) is that the audio thread reads from
    /// something the VOICE owns. Asking a shared chunk index per sample —
    /// what this did before — put an arc-swap guard and contended atomics on
    /// every sample of every voice, and cost a chord hundreds of
    /// milliseconds in a 5.33 ms callback.
    stream_cursor: fts_sample::stream::StreamCursor,
    /// True once the voice has produced audible output. Gates auto-retirement
    /// so a voice still in its silent attack pre-roll / fade-in-under handoff
    /// is never mistaken for a decayed one.
    has_sounded: bool,
    /// Consecutive frames the voice's own output has stayed below the
    /// audibility floor after having sounded. When it exceeds
    /// [`RETIRE_SILENCE_FRAMES`] the voice retires (frees its polyphony slot) —
    /// so notes decayed to silence under a held sustain pedal stop hogging the
    /// pool and forcing voice-steals that cut still-ringing notes.
    quiet_frames: usize,
    /// Peak-follower of the voice's own output magnitude (fast attack, slow
    /// release). Read at note-off so the release/key-up noise can be scaled to
    /// sit a fixed dB UNDER the note body that actually sounded — a soft note
    /// is a genuinely quiet recording, so a fixed-level release would drown it.
    env_peak: f32,
    /// Render-trace correlation id (set at spawn when tracing) — pairs this
    /// voice's lifetime with its `TraceKind::VoiceSpawn`/`VoiceEnd` events.
    pub trace_id: Option<u64>,
    /// Time-preserving pitch shift (L/R), `None` when the note is on the
    /// recorded grid (`pitch_cents == 0`) — those voices pay nothing.
    pitch: Option<[crate::engine::pitch_shift::PitchShifter; 2]>,
}

/// One decoded Kontakt `ENV_FLEX` amplitude envelope, evaluated per frame and
/// multiplied into the voice's output.
///
/// Segments are `(len_frames, from, to, tension)` — the actual shipped `(time_ms, level, curve)` triplets from the
/// instrument's `GroupList` (see `nkx-extract/CSS_GROUP_MOD.md` §2), converted to
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
    #[must_use]
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
        for &(ms, level, tension) in segments {
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
        self.segs.last().map_or(0.0, |s| s.2)
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
    #[must_use]
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
            stream_pin: None,
            stream_cursor: Default::default(),
            loop_xfade: 0,
            rate,
            bend: 1.0,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            stage1_run: 0,
            stage2_frames: 0,
            glide_cents: 0.0,
            glide_end: 0.0,
            glide_step: 0.0,
            glide_frames: 0,
            state: VoiceState::Playing,
            release_hold: 0,
            pending_fade: 0,
            attack_delay: 0,
            start_hold: 0,
            marker_arrival_file: None,
            spawn_frame: 0,
            frames_out: 0,
            arrival_emitted: None,
            arrival_drained: false,
            bloom_frames: 0,
            ds_level: 1.0,
            sustain: 1.0,
            decay_left: 0,
            amp_bp: None,
            filter: None,
            vib_phase: 0.0,
            vib_inc: 0.0,
            vib_cents: 0.0,
            vib_wait: 0,
            vib_fade: 0.0,
            vib_fade_step: 1.0,
            bloom_total: 0,
            bloom_target: 1.0,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
            flex: None,
            has_sounded: false,
            quiet_frames: 0,
            env_peak: 0.0,
            trace_id: None,
            pitch: None,
        }
    }

    /// Create a voice with an explicit playback rate.
    ///
    /// Used by the zone-mode path (Spectrasonics-style libraries) where pitch
    /// shifting combines an integer semitone offset (note - `root_key`) with a
    /// per-zone fine-tune in cents. Caller computes `rate = 2^(total_cents/1200)`.
    #[must_use]
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
            stream_pin: None,
            stream_cursor: Default::default(),
            loop_xfade: 0,
            rate,
            bend: 1.0,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            stage1_run: 0,
            stage2_frames: 0,
            glide_cents: 0.0,
            glide_end: 0.0,
            glide_step: 0.0,
            glide_frames: 0,
            state: VoiceState::Playing,
            release_hold: 0,
            pending_fade: 0,
            attack_delay: 0,
            start_hold: 0,
            marker_arrival_file: None,
            spawn_frame: 0,
            frames_out: 0,
            arrival_emitted: None,
            arrival_drained: false,
            bloom_frames: 0,
            ds_level: 1.0,
            sustain: 1.0,
            decay_left: 0,
            amp_bp: None,
            filter: None,
            vib_phase: 0.0,
            vib_inc: 0.0,
            vib_cents: 0.0,
            vib_wait: 0,
            vib_fade: 0.0,
            vib_fade_step: 1.0,
            bloom_total: 0,
            bloom_target: 1.0,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
            flex: None,
            has_sounded: false,
            quiet_frames: 0,
            env_peak: 0.0,
            trace_id: None,
            pitch: None,
        }
    }

    /// Multiply the playback rate by `scale`. Used to compensate for a sample
    /// whose native sample rate differs from the engine's output rate: a
    /// 44.1 kHz sample played on a 48 kHz engine must advance its read head at
    /// `44100/48000` of the pitched rate, otherwise it sounds `48000/44100`
    /// (~147 cents) sharp. `Voice::new`/`Voice::with_rate` compute pitch only
    /// (they don't know the output rate); the spawning engine, which knows
    /// both, applies this. `scale == 1.0` (native == output) is a no-op.
    #[must_use]
    pub fn with_rate_scale(mut self, scale: f64) -> Self {
        self.rate *= scale;
        self
    }

    /// Apply a time-preserving pitch shift of `cents` (100 = a semitone),
    /// decoupled from playback `rate`. Used for whole-tone-grid fill so an
    /// off-grid note keeps its recorded arrival timing (`rate` carries only
    /// sample-rate conversion) while pitch moves here. A near-zero shift is a
    /// no-op — on-grid voices pay nothing.
    #[must_use]
    pub fn with_pitch_cents(mut self, cents: f64) -> Self {
        use crate::engine::pitch_shift::PitchShifter;
        if !PitchShifter::is_unity(cents) {
            self.pitch = Some([PitchShifter::new(cents), PitchShifter::new(cents)]);
        }
        self
    }

    /// Compensate the pitch shifter's startup latency, so the voice's intended
    /// first frame is what comes out on tick 0.
    ///
    /// The barberpole's full-gain head reads half a buffer behind the write
    /// pointer, so for its first `PitchShifter::startup_frames()` input frames
    /// its output belongs to sample content EARLIER than `start_frame` — and
    /// for a voice starting at frame 0 there is none, so it emitted silence
    /// for ~80 ms and then jumped when real audio caught up. That is why every
    /// off-grid fresh note spoke 80 ms late; on a whole-tone sampling grid
    /// that is half of all notes. Transitions hid the same latency rather than
    /// escaping it: starting partway in, their head had real audio to read —
    /// the wrong 80 ms of it.
    ///
    /// The fix is to run those frames through the shifter up front and then
    /// continue from where that left off: feed `start_frame .. start_frame +
    /// startup`, and start the read cursor past them. The ring stays
    /// contiguous — no fabricated history, no junction to cross — and the head
    /// is reading the intended frame when the voice is first heard.
    ///
    /// Call once after the builder chain (needs `start_frame` set). No-op
    /// without a shifter.
    ///
    /// Reads through [`SampleData::read_frames`] into thread-local scratch:
    /// this runs on the audio thread at every note-on, so it must neither
    /// allocate nor read a streamed sample frame-by-frame.
    pub fn prime_pitch_shifters(&mut self) {
        let Some(startup) = self.pitch.as_ref().map(|p| p[0].startup_frames()) else {
            return;
        };
        let data = self.data.clone();
        let start = self.start_frame;
        let n = startup.min(data.num_frames.saturating_sub(start));
        if n == 0 {
            return;
        }
        // Bulk read into REUSED scratch, through this voice's cursor.
        //
        // The obvious spelling — two `collect()`s over `data.frame(f)` — was
        // the single most expensive thing the audio thread did. It allocated
        // two vectors per voice at note-on and, for a streamed sample, took
        // an arc-swap guard per channel per frame: ~16,000 guarded reads per
        // voice across ~4-8k priming frames. A chord's worth of voices made
        // that ~590,000 guarded reads inside one callback, which is where the
        // 200-600 ms stalls came from. `read_frames` resolves a chunk once
        // per chunk crossed, and the scratch is per-thread, so a note-on
        // after the first allocates nothing.
        PRIME_SCRATCH.with(|scratch| {
            let (hl, hr) = &mut *scratch.borrow_mut();
            hl.clear();
            hr.clear();
            hl.resize(n, 0.0);
            hr.resize(n, 0.0);
            data.read_frames(start, hl, hr, &mut self.stream_cursor);
            if let Some(shift) = self.pitch.as_mut() {
                shift[0].prime(hl);
                shift[1].prime(hr);
            }
        });
        // Only the read CURSOR moves past what priming consumed; `start_frame`
        // stays put, since it is the window floor a loop crossfade reads back
        // into, not the cursor.
        self.position = (start + n) as f64;
    }

    /// Set the mic index this voice routes to. `mic_index` indexes into
    /// `LibrarySpec.mics` in declaration order.
    #[must_use]
    pub fn data_num_frames(&self) -> usize {
        self.data.num_frames
    }

    #[must_use]
    pub fn with_mic_index(mut self, mic_index: Option<u8>) -> Self {
        self.mic_index = mic_index;
        self
    }

    /// Tag this voice with its mono legato line (engine `LineId`).
    #[must_use]
    pub fn with_line(mut self, line: u8) -> Self {
        self.line = line;
        self
    }

    /// Tag this voice with the stem class of its source articulation.
    #[must_use]
    pub fn with_artic_class(mut self, class: ArticClass) -> Self {
        self.artic_class = class;
        self
    }

    #[must_use]
    pub fn with_choke_group(mut self, choke_group: Option<u64>) -> Self {
        self.choke_group = choke_group;
        self
    }

    #[must_use]
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
    #[must_use]
    pub fn with_dyn_layer(mut self, layer: DynLayer) -> Self {
        self.dyn_layer = Some(layer);
        self
    }

    /// Attach the decoded `ENV_FLEX` amplitude envelope (the instrument's real
    /// per-voice amp AHDSR). Multiplied into the output every frame.
    #[must_use]
    pub fn with_flex_env(mut self, flex: FlexEnv) -> Self {
        self.flex = Some(flex);
        self
    }

    /// Start at silence and ramp up to the spawn gain over `frames` — the
    /// attack envelope. `frames == 0` keeps the sample's natural attack.
    #[must_use]
    pub fn with_attack(mut self, frames: usize) -> Self {
        if frames > 0 {
            self.gain = 0.0; // target_gain stays at the intended spawn gain
            self.gain_ramp_frames = frames;
        }
        self
    }

    /// The decay and sustain of an ADSR: once the attack is done, fall from
    /// full level to `sustain` (0..=1) over `decay_frames`, and hold there
    /// until note-off. `decay_frames == 0` jumps straight to `sustain`;
    /// `sustain == 1.0` leaves the voice as it was.
    #[must_use]
    pub fn with_decay(mut self, decay_frames: usize, sustain: f32) -> Self {
        let sustain = sustain.clamp(0.0, 1.0);
        self.sustain = sustain;
        if decay_frames == 0 {
            self.ds_level = sustain;
        } else {
            self.decay_left = decay_frames;
        }
        self
    }

    /// Scale the voice's level (its gain and target gain) by `k`.
    #[must_use]
    pub fn scaled_gain(mut self, k: f32) -> Self {
        self.gain *= k;
        self.target_gain *= k;
        self
    }

    /// Shape the voice with a breakpoint amplitude envelope (`None`: off).
    /// `sample_rate` is the output rate the envelope advances at.
    #[must_use]
    pub fn with_amp_points(
        mut self,
        bp: Option<Arc<crate::native::breakpoints::Breakpoints>>,
        sample_rate: u32,
    ) -> Self {
        self.amp_bp = bp.map(|bp| {
            let mut player = crate::native::breakpoints::EnvPlayer::default();
            player.note_on();
            (bp, player, 1.0 / sample_rate.max(1) as f32)
        });
        self
    }

    /// Give the voice its own filter envelope (see [`VoiceFilter`]).
    #[must_use]
    pub fn with_filter_env(mut self, filter: Option<VoiceFilter>) -> Self {
        self.filter = filter;
        self
    }

    /// Follow the layer's cutoff knobs on a held note.
    pub fn set_filter_base(&mut self, base_hz: f32, amount: f32, resonance: f32) {
        if let Some(f) = &mut self.filter {
            f.set_base(base_hz, amount, resonance);
        }
    }

    /// Vibrato: `rate_hz`, `cents` deep, starting `delay_frames` after the
    /// note and fading in over as long again (at least 50 ms).
    #[must_use]
    pub fn with_vibrato(mut self, vib: Vibrato) -> Self {
        self.set_vibrato(vib);
        self.vib_wait = vib.delay_frames;
        let fade = vib.delay_frames.max((vib.sample_rate * 0.05) as u32).max(1);
        self.vib_fade = 0.0;
        self.vib_fade_step = 1.0 / fade as f32;
        self
    }

    /// Change the vibrato's rate and depth on a held note.
    pub fn set_vibrato(&mut self, vib: Vibrato) {
        self.vib_inc = vib.rate_hz / vib.sample_rate.max(1.0);
        self.vib_cents = vib.cents;
    }

    /// CSS legato handoff: spawn this (looping sustain) voice muted, wait
    /// `delay_frames`, then fade it in to its spawn gain over `fade_frames`.
    /// Mirrors the `CSS_W` helper (`wait($haa1x); fade_in($id, $0fznn)`) that
    /// brings the destination sustain up underneath the one-shot bow-change
    /// transition (spec §2.1 step 7). `delay_frames == 0 && fade_frames == 0`
    /// leaves the natural attack unchanged.
    #[must_use]
    pub fn with_fade_in_under(mut self, delay_frames: usize, fade_frames: usize) -> Self {
        if delay_frames > 0 || fade_frames > 0 {
            self.gain = 0.0; // target_gain stays at the intended spawn gain
            self.attack_delay = delay_frames;
            self.gain_ramp_frames = fade_frames;
        }
        self
    }

    /// CSS two-stage destination swell (KSP §3.1/§3.5): silent for
    /// `delay_frames`, then a fast stage-1 ramp toward target at the
    /// `stage1_denom` (`$mlnoy`) rate for `stage1_run` (`$qsazz`) frames —
    /// reaching ~`stage1_run/stage1_denom` of target — then a slower stage-2
    /// ramp of `stage2_frames` (`$rixqv`) that completes to target. Falls back
    /// to a single-stage fade when `stage2_frames == 0`.
    #[must_use]
    pub fn with_two_stage_fade_in(
        mut self,
        delay_frames: usize,
        stage1_run: usize,
        stage1_denom: usize,
        stage2_frames: usize,
    ) -> Self {
        self.gain = 0.0; // target_gain stays at the intended spawn gain
        self.attack_delay = delay_frames;
        self.gain_ramp_frames = stage1_denom.max(1);
        self.stage1_run = stage1_run;
        self.stage2_frames = stage2_frames;
        self
    }

    /// Portamento micro-glide: ramp the pitch from `start_cents` to `end_cents`
    /// over `frames` (CSS `$1mwwo`≈60 ms). Incoming: `(-jyttf, 0)`; outgoing:
    /// `(0, +jyttf)`. (`$upjkh`/`$ma0b1`.)
    #[must_use]
    pub fn with_pitch_glide(mut self, start_cents: f32, end_cents: f32, frames: usize) -> Self {
        self.set_pitch_glide(start_cents, end_cents, frames);
        self
    }

    /// Runtime glide setter — also used to bend an already-playing (retiring)
    /// voice toward the new note.
    pub fn set_pitch_glide(&mut self, start_cents: f32, end_cents: f32, frames: usize) {
        if frames > 0 && (start_cents - end_cents).abs() > f32::EPSILON {
            self.glide_cents = start_cents;
            self.glide_end = end_cents;
            self.glide_frames = frames;
            self.glide_step = (end_cents - start_cents) / frames as f32;
        }
    }

    /// Hold the voice back `frames` frames before it starts playing: silence
    /// out, read position frozen at the start (nothing of the sample is
    /// consumed). The per-zone arrival alignment — the scheduler pre-rolls a
    /// trigger by an upper-bound lead; each spawned voice is held back by
    /// `lead − its own measured arrival` so the heard arrival lands exactly
    /// on the grid tick, per round-robin / mic / dynamic layer. `0` = no-op.
    #[must_use]
    pub fn with_start_hold(mut self, frames: usize) -> Self {
        self.start_hold = frames;
        self
    }

    /// Attach the zone's ARRIVAL marker (FILE frames) for playback emission
    /// and the engine output frame this voice spawns at. See
    /// `marker_arrival_file` — the marker is emitted when the real playhead
    /// crosses the position, stamped with the actual output frame.
    #[must_use]
    pub fn with_arrival_marker(mut self, file_frame: f64, spawn_frame: u64) -> Self {
        self.marker_arrival_file = Some(file_frame);
        self.spawn_frame = spawn_frame;
        self
    }

    /// The playback-emitted arrival (absolute output frame), once, if the
    /// playhead has crossed the marker since the last drain. Lock-free /
    /// alloc-free: one Option read per voice per block.
    pub fn take_emitted_arrival(&mut self) -> Option<u64> {
        if self.arrival_drained {
            return None;
        }
        let e = self.arrival_emitted?;
        self.arrival_drained = true;
        Some(e)
    }

    /// Slow secondary bloom (CSS `%1wcdh`): multiply the voice's output by a
    /// ramp from 1.0 to `target` over `frames` frames (after any start hold
    /// / attack delay). Used by the legato handoff to swell the −6 dB
    /// connected sustain back to full body over ~1 s. No-op when `frames ==
    /// 0` or `target == 1.0`.
    #[must_use]
    pub fn with_slow_bloom(mut self, frames: usize, target: f32) -> Self {
        if frames > 0 && (target - 1.0).abs() > 1e-6 {
            self.bloom_frames = frames;
            self.bloom_total = frames;
            self.bloom_target = target;
        }
        self
    }

    #[must_use]
    pub fn with_sample_window(mut self, start_frame: usize, end_frame: Option<usize>) -> Self {
        let start = start_frame.min(self.data.num_frames);
        let end = end_frame
            .unwrap_or(self.data.num_frames)
            .min(self.data.num_frames)
            .max(start);
        self.start_frame = start;
        self.position = start as f64;
        self.end_frame = end;
        // A streamed sample only has its head resident; a window that starts
        // past it needs the chunk fetched before the voice arrives.
        self.data.prefetch_at(start);
        self
    }

    #[must_use]
    pub fn with_forward_loop(mut self, loop_start: usize, loop_end: usize) -> Self {
        let start = loop_start.min(self.end_frame);
        let end = loop_end.min(self.end_frame);
        if end > start + 1 {
            self.loop_range = Some((start, end));
            // A streamed sample must be told: the loop is read for as long as
            // the note is held, and the wrap jumps backwards past anything a
            // forward prefetch would fetch.
            self.stream_pin = self.data.pin_region(start, end);
        }
        self
    }

    #[must_use]
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
    #[must_use]
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

    #[must_use]
    pub fn reversed(mut self) -> Self {
        self.reverse = true;
        self.loop_range = None;
        self.position = self.end_frame.saturating_sub(1) as f64;
        // Reverse walks *away* from what prefetching fetches, so the window
        // is pinned for the life of the voice instead.
        self.stream_pin = self.data.pin_region(self.start_frame, self.end_frame);
        self
    }

    /// Schedule a gain ramp to `target` over `frames` frames.
    /// Retire this voice by HOLDING it at full for `hold_frames`, then fading to
    /// zero over `fade_frames` (via `release_hold`/`pending_fade`). For the
    /// document legato retire: the outgoing note holds through the transition's
    /// pre-bow, then crossfades out ENDING at the arrival tick — complementary
    /// to the incoming swell, so the old note doesn't bleed through the whole
    /// fade-in. No-op unless the voice is still Playing.
    pub fn hold_then_release(&mut self, hold_frames: usize, fade_frames: usize) {
        if matches!(self.state, VoiceState::Playing) {
            self.release_hold = hold_frames.max(1);
            self.pending_fade = fade_frames.max(1);
        }
    }

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
            // A one-shot plays to its end — unless a breakpoint amp envelope
            // shapes it, which releases with the key like any other voice.
            VoiceKind::Short if self.amp_bp.is_none() => {}
            VoiceKind::Release => {
                // Play to end — do not release early.
            }
            _ => {
                if self.state == VoiceState::Playing {
                    if let Some(f) = &mut self.filter {
                        f.note_off();
                    }
                    // A breakpoint envelope releases over its own last
                    // segment (plus a frame so it reaches its end).
                    let release_frames = match &mut self.amp_bp {
                        Some((bp, player, dt)) => {
                            player.note_off(bp);
                            let tail = bp.sustain.map_or(0.0, |s| {
                                bp.points.last().map_or(0.0, |l| l.time) - bp.points[s].time
                            });
                            (tail / *dt) as usize + 2
                        }
                        None => release_frames,
                    };
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
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.state == VoiceState::Done
    }

    /// Linearly-interpolated stereo read at fractional frame `pos`. Does not
    /// advance state; `pos` must be within the playable window.
    #[inline]
    fn read_interp(&mut self, pos: f64) -> (f32, f32) {
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        // Through this voice's own cursor: while the playhead stays inside
        // the chunk it already holds — thousands of consecutive samples —
        // this is four slice indexes and no atomics.
        let ((l0, r0), (l1, r1)) = self.data.frame_pair_cursored(
            idx,
            self.end_frame.saturating_sub(1),
            &mut self.stream_cursor,
        );
        (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
    }

    /// Render one stereo frame. Returns (L, R) and advances internal state.
    #[inline]
    pub fn next_frame(&mut self) -> (f32, f32) {
        if self.state == VoiceState::Done {
            return (0.0, 0.0);
        }

        // Output-frame clock for playback-emitted markers: every call
        // produces one output frame (holds included — they occupy real
        // output time), so the emission stamp `spawn_frame + frames_out` is
        // the exact output moment of the crossing.
        self.frames_out += 1;

        // True start hold (per-zone arrival alignment): nothing advances —
        // the sample genuinely begins `start_hold` frames after the spawn.
        // Sits before every other per-frame update so envelopes, fades, and
        // the read position all start when the hold elapses.
        if self.start_hold > 0 {
            self.start_hold -= 1;
            return (0.0, 0.0);
        }

        // Playback-emitted ARRIVAL: the playhead is at/past the marker on
        // this output frame (covers start-offset overshoot too — a voice
        // spawned past its marker emits on its first sounding frame, which
        // makes over-skips visible instead of silent).
        if self.arrival_emitted.is_none() {
            if let Some(m) = self.marker_arrival_file {
                if self.position >= m {
                    self.arrival_emitted = Some(self.spawn_frame + self.frames_out - 1);
                }
            }
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
            // Two-stage swell: after `stage1_run` frames of the fast stage-1
            // ramp (rate = stage-1 `$mlnoy`), switch to the slower stage-2 ramp
            // (`$rixqv`) that completes to target.
            if self.stage1_run > 0 {
                self.stage1_run -= 1;
                if self.stage1_run == 0 && self.stage2_frames > 0 {
                    self.gain_ramp_frames = self.stage2_frames;
                    self.stage2_frames = 0;
                }
            }
        } else {
            self.gain = self.target_gain;
        }

        // Slow secondary bloom (CSS `%1wcdh`): a multiplicative swell from
        // 1.0 to `bloom_target`, advanced only once the voice is audible
        // (start hold / attack delay done). Applied via `bloom_gain()` below.
        if self.attack_delay == 0 && self.bloom_frames > 0 {
            self.bloom_frames -= 1;
        }

        // Decay toward the sustain level, once the attack has finished.
        if self.decay_left > 0
            && self.attack_delay == 0
            && self.gain_ramp_frames == 0
            && self.state == VoiceState::Playing
        {
            self.ds_level += (self.sustain - self.ds_level) / self.decay_left as f32;
            self.decay_left -= 1;
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

        // Time-preserving pitch shift (whole-tone-grid fill): the sample is
        // read at true speed above (position advances by SR-conversion only,
        // so its recorded arrival timing is preserved) and the ±semitone
        // transposition is applied here, decoupled from playback rate. Applied
        // after the loop-xfade so the shifter sees a continuous stream across
        // the seam; before amp so gains/fades are unaffected.
        if let Some(shift) = &mut self.pitch {
            l = shift[0].tick(l);
            r = shift[1].tick(r);
        }

        if let Some(f) = &mut self.filter {
            (l, r) = f.tick(l, r);
        }

        // Decoded ENV_FLEX amplitude envelope. Freezes at the sustain-hold
        // point while Playing (held note stays steady); advances through the
        // decay once Releasing. Multiplied on top of the note-off release fade.
        let flex = match &mut self.flex {
            Some(f) => f.next(!matches!(self.state, VoiceState::Playing)),
            None => 1.0,
        };

        let bloom = if self.bloom_total > 0 {
            let t = 1.0 - self.bloom_frames as f32 / self.bloom_total as f32;
            1.0 + (self.bloom_target - 1.0) * t
        } else {
            1.0
        };
        // A breakpoint envelope replaces the linear release fade.
        let (env, bp) = match &mut self.amp_bp {
            Some((bp, player, dt)) => (1.0, player.tick(bp, *dt)),
            None => (env, 1.0),
        };
        let amp = self.gain * env * flex * bloom * self.ds_level * bp;

        // Advance position. During a portamento glide the read rate is nudged
        // by `glide_cents` (ramping to 0) so the pitch scoops into true tuning;
        // the tiny position drift over the ~60 ms glide is inaudible.
        // Vibrato: a triangle-smoothed sine (parabolic), a few cents either
        // way, so a linear cents→ratio is exact to well under a cent.
        let vib = if self.vib_cents > 0.0 {
            if self.vib_wait > 0 {
                self.vib_wait -= 1;
                1.0
            } else {
                self.vib_fade = (self.vib_fade + self.vib_fade_step).min(1.0);
                self.vib_phase = (self.vib_phase + self.vib_inc).fract();
                let x = self.vib_phase * 2.0 - 1.0; // -1..1
                let s = 4.0 * x * (1.0 - x.abs()); // parabolic sine
                1.0 + f64::from(s * self.vib_cents * self.vib_fade)
                    * (std::f64::consts::LN_2 / 1200.0)
            }
        } else {
            1.0
        };
        let step = self.bend
            * vib
            * if self.glide_frames > 0 {
                let s = self.rate * 2f64.powf(self.glide_cents as f64 / 1200.0);
                self.glide_cents += self.glide_step;
                self.glide_frames -= 1;
                if self.glide_frames == 0 {
                    self.glide_cents = self.glide_end;
                }
                s
            } else {
                self.rate
            };
        if self.reverse {
            self.position -= step;
        } else {
            self.position += step;
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
        let mut block_peak = 0.0f32;
        for i in 0..num_frames {
            let (l, r) = self.next_frame();
            output[i * 2] += l;
            output[i * 2 + 1] += r;
            block_peak = block_peak.max(l.abs()).max(r.abs());
            rendered += 1;
            if self.is_done() {
                break;
            }
        }
        self.update_decay_retire(block_peak, rendered);
        rendered
    }

    /// Free a voice that has decayed to silence. Once a voice has actually
    /// sounded (so silent attack pre-rolls don't trip it) and then stays below
    /// the audibility floor for [`RETIRE_SILENCE_FRAMES`], retire it. This
    /// matters for pianos under a held sustain pedal: without it, every struck
    /// note keeps its full multi-second sample alive in the pool long after it
    /// has faded to nothing, filling the polyphony budget and forcing
    /// voice-steals that cut still-ringing notes. A looping/held sustain never
    /// stays quiet, so it never retires this way.
    fn update_decay_retire(&mut self, block_peak: f32, frames: usize) {
        // Peak-follower: jump up to a louder block instantly, ease down slowly
        // so a momentary dip doesn't read as "quiet". Tracks the note body's
        // current loudness for release-gain scaling.
        if block_peak > self.env_peak {
            self.env_peak = block_peak;
        } else {
            self.env_peak *= ENV_PEAK_DECAY;
        }
        if self.state == VoiceState::Done {
            return;
        }
        if block_peak >= RETIRE_FLOOR {
            self.has_sounded = true;
            self.quiet_frames = 0;
        } else if self.has_sounded && !matches!(self.state, VoiceState::Playing) {
            // Only RELEASING tails silence-retire. A HELD voice must survive at
            // gain ≈ 0: during a CC1 sweep the inactive dynamic layers are
            // crossfaded to silence and must come BACK when the controller
            // returns to their range (param-test S14: the held note died
            // mid-crescendo because the loud layers were retired while quiet).
            self.quiet_frames = self.quiet_frames.saturating_add(frames);
            if self.quiet_frames >= RETIRE_SILENCE_FRAMES {
                self.state = VoiceState::Done;
            }
        }
    }

    /// Current peak-follower level of this voice (for release-gain scaling).
    #[must_use]
    pub fn env_peak(&self) -> f32 {
        self.env_peak
    }
}

thread_local! {
    /// Scratch for [`Voice::prime_pitch_shifters`], reused across note-ons.
    ///
    /// Priming needs a few thousand frames of history per channel. Allocating
    /// that per voice put two heap allocations in the note-on path on the
    /// audio thread; one buffer per thread costs ~64 KB and removes them.
    /// Thread-local rather than per-voice because only the audio thread
    /// primes, and it primes one voice at a time.
    static PRIME_SCRATCH: std::cell::RefCell<(Vec<f32>, Vec<f32>)> =
        std::cell::RefCell::new((Vec::new(), Vec::new()));
}

/// Flush denormals to zero. On `no_std`/embedded and WASM targets the host may
/// not set hardware FTZ/DAZ, so a denormal on a quiet release tail can spike
/// CPU. The branch is predictable (almost always false) and cheap.
#[inline(always)]
fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-30 { 0.0 } else { x }
}

// ── Voice pool ────────────────────────────────────────────────────────────────

/// Maximum simultaneous voices before stealing. A piano held under the sustain
/// pedal accumulates many multi-second voices, so this is generous; the decay
/// auto-retire (see [`Voice::update_decay_retire`]) keeps the *active* count
/// far below it in practice by freeing faded notes.
const MAX_VOICES: usize = 160;
const STEAL_FADE_FRAMES: usize = 128;
/// Output magnitude below which a voice is considered inaudible (~ -84 dBFS).
/// Deliberately deep so a still-audible (even very faint) held note is never
/// retired — only genuinely-silent tails free their slot.
const RETIRE_FLOOR: f32 = 6.0e-5;
/// Per-block decay of the voice peak-follower (~ -0.09 dB/block at 512-frame
/// blocks) — smooth enough to reflect the body's loudness at note-off without
/// chasing every sample.
const ENV_PEAK_DECAY: f32 = 0.99;
/// How long (frames) a sounded voice must stay below [`RETIRE_FLOOR`] before it
/// retires and frees its slot. ~0.4 s at 44.1/48 kHz — long enough not to clip
/// a real tail, short enough to reclaim slots during a busy pedal-held passage.
const RETIRE_SILENCE_FRAMES: usize = 19_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceStealPolicy {
    ReleaseFirstQuietest,
    Oldest,
    Quietest,
    SameNoteFirst,
    DropNew,
}

impl VoiceStealPolicy {
    #[must_use]
    pub fn parse(value: &str) -> Self {
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
    /// Live pitch-bend factor — stamped onto every spawned voice and pushed
    /// to the ones already sounding by [`set_bend`](Self::set_bend).
    bend: f64,
    /// Solo filter: when `Some`, only voices whose `note` is in the set
    /// contribute to the output. Muted voices still render (into a discard
    /// buffer) so their lifecycle — and therefore every other note's legato
    /// timing — is bit-identical to the full mix. Offline analysis only.
    solo_notes: Option<std::collections::BTreeSet<u8>>,
    solo_scratch: Vec<f32>,
}

impl Default for VoicePool {
    fn default() -> Self {
        Self::new()
    }
}

impl VoicePool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            stolen: 0,
            max_voices: MAX_VOICES,
            steal_policy: VoiceStealPolicy::ReleaseFirstQuietest,
            bend: 1.0,
            solo_notes: None,
            solo_scratch: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_max_voices(max_voices: usize) -> Self {
        let capacity = max_voices.clamp(1, MAX_VOICES);
        Self {
            voices: Vec::with_capacity(capacity),
            stolen: 0,
            max_voices: max_voices.max(1),
            steal_policy: VoiceStealPolicy::ReleaseFirstQuietest,
            bend: 1.0,
            solo_notes: None,
            solo_scratch: Vec::new(),
        }
    }

    pub fn set_max_voices(&mut self, max_voices: usize) {
        self.max_voices = max_voices.max(1);
        self.enforce_limit();
    }

    #[must_use]
    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    pub fn set_steal_policy(&mut self, policy: VoiceStealPolicy) {
        self.steal_policy = policy;
    }

    #[must_use]
    pub fn steal_policy(&self) -> VoiceStealPolicy {
        self.steal_policy
    }

    /// Add a voice, stealing one or more voices if at capacity.
    /// Set the live pitch-bend factor: held voices take it immediately,
    /// and every voice spawned from here on is stamped with it.
    pub fn set_bend(&mut self, factor: f64) {
        self.bend = factor.clamp(0.25, 4.0);
        for v in &mut self.voices {
            v.bend = self.bend;
        }
    }

    pub fn spawn(&mut self, voice: Voice) {
        let mut voice = voice;
        // New voices sound at the current wheel position.
        voice.bend = self.bend;
        // Remove done voices first
        self.voices.retain(|v| !v.is_done());

        if self.voices.len() >= self.max_voices
            && self.steal_one_for(&voice) == StealOutcome::DropIncoming
        {
            return;
        }
        self.voices.push(voice);
    }

    /// The most recently spawned voice, if any — used to post-configure a
    /// voice right after a spawn helper ran (e.g. the Pacific release-overlap
    /// fade). Best-effort: `None` if the incoming voice was dropped by the
    /// steal policy.
    pub fn last_spawned_mut(&mut self) -> Option<&mut Voice> {
        self.voices.last_mut()
    }

    /// Collect the trace ids of all currently-alive traced voices into `out`
    /// (cleared first). Drives the render-trace `VoiceEnd` sweep.
    pub fn alive_trace_ids_into(&self, out: &mut Vec<u64>) {
        out.clear();
        out.extend(self.voices.iter().filter_map(|v| v.trace_id));
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

    /// Loudest current peak-follower level among the note's still-sounding
    /// body voices (non-release, not Done). Used to scale a note's release tail
    /// so it sits under the actual body that played. `0.0` if nothing sounds.
    pub fn note_body_peak(&self, note: u8) -> f32 {
        self.voices
            .iter()
            .filter(|v| {
                v.note == note && v.kind != VoiceKind::Release && v.state != VoiceState::Done
            })
            .map(Voice::env_peak)
            .fold(0.0f32, f32::max)
    }

    /// Fade any still-*Playing* sustain voice of `note` over `fade_frames` — a
    /// re-strike of the same key re-excites the same string, so the previous
    /// ring is subsumed rather than stacked. Without this, repeated notes under
    /// a held sustain pedal pile up a fresh multi-second voice per hit, filling
    /// the pool and forcing steals that cut still-ringing notes. Release tails
    /// and already-releasing voices are left alone.
    pub fn retrigger_fade_note(&mut self, note: u8, fade_frames: usize) {
        for v in &mut self.voices {
            if v.note == note
                && v.kind != VoiceKind::Release
                && matches!(v.state, VoiceState::Playing)
            {
                v.ramp_gain(0.0, fade_frames);
                v.state = VoiceState::Releasing {
                    frames_remaining: fade_frames,
                };
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

    /// Pedal down: catch the notes whose keys are **still held**.
    ///
    /// A damper pedal lifts the dampers off strings that are ringing because
    /// the key is down. A note the player already let go of has been damped;
    /// the pedal arriving afterwards does not bring it back. Restoring every
    /// releasing voice made a late pedal freeze the release tails of notes
    /// that were over — the note hangs on instead of dying away.
    ///
    /// `held` is the set of keys currently down.
    pub fn repedal_held(&mut self, held: &dyn Fn(u8) -> bool) -> usize {
        let mut restored = 0;
        for voice in &mut self.voices {
            if matches!(voice.state, VoiceState::Releasing { .. }) && held(voice.note) {
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
    pub fn retire_note_line(
        &mut self,
        line: u8,
        note: u8,
        trans_fade: usize,
        sus_fade: usize,
        hold_frames: usize,
    ) {
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
            if hold_frames > 0 {
                // Document prefire: hold through the pre-bow, then crossfade out
                // ending at the tick (complementary to the incoming swell).
                v.hold_then_release(hold_frames, fade);
            } else {
                v.ramp_gain(0.0, fade);
                v.release_frames = fade;
                v.state = VoiceState::Releasing {
                    frames_remaining: fade,
                };
            }
        }
    }

    /// Bend the outgoing (retiring) voices of `note` on `line` from `start_cents`
    /// to `end_cents` over `frames` — the CSS `$upjkh` outgoing portamento glide
    /// (the departed note bends toward the new one as it fades).
    pub fn glide_note_line(
        &mut self,
        line: u8,
        note: u8,
        start_cents: f32,
        end_cents: f32,
        frames: usize,
    ) {
        for v in &mut self.voices {
            if v.note == note && v.line == line && v.kind != VoiceKind::Release {
                v.set_pitch_glide(start_cents, end_cents, frames);
            }
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

    #[must_use]
    pub fn active_choke_group_count(&self, group: u64) -> usize {
        self.voices
            .iter()
            .filter(|v| v.choke_group == Some(group) && v.state != VoiceState::Done)
            .count()
    }

    /// Render all active voices into an interleaved stereo buffer.
    /// Set the per-note solo filter (offline analysis). `None` = full mix.
    pub fn set_solo_notes(&mut self, notes: Option<std::collections::BTreeSet<u8>>) {
        self.solo_notes = notes;
    }

    pub fn render(&mut self, output: &mut [f32]) {
        match &self.solo_notes {
            None => {
                for v in &mut self.voices {
                    v.render_block(output);
                }
            }
            Some(set) => {
                // Muted voices render into a discard buffer so their lifecycle
                // (and every soloed note's legato timing) is unchanged.
                if self.solo_scratch.len() < output.len() {
                    self.solo_scratch.resize(output.len(), 0.0);
                }
                for v in &mut self.voices {
                    if set.contains(&v.note) {
                        v.render_block(output);
                    } else {
                        let scratch = &mut self.solo_scratch[..output.len()];
                        scratch.fill(0.0);
                        v.render_block(scratch);
                    }
                }
            }
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
        let len = outputs[0].len();
        for v in &mut self.voices {
            // Solo filter: muted voices render into a discard buffer so their
            // lifecycle (and every soloed note's timing) is unchanged.
            if self
                .solo_notes
                .as_ref()
                .is_some_and(|s| !s.contains(&v.note))
            {
                if self.solo_scratch.len() < len {
                    self.solo_scratch.resize(len, 0.0);
                }
                let scratch = &mut self.solo_scratch[..len];
                scratch.fill(0.0);
                v.render_block(scratch);
                continue;
            }
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
        let len = outputs[0].len();
        for v in &mut self.voices {
            if self
                .solo_notes
                .as_ref()
                .is_some_and(|s| !s.contains(&v.note))
            {
                if self.solo_scratch.len() < len {
                    self.solo_scratch.resize(len, 0.0);
                }
                let scratch = &mut self.solo_scratch[..len];
                scratch.fill(0.0);
                v.render_block(scratch);
                continue;
            }
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

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.voices.len()
    }

    #[must_use]
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
        tracing::debug!(
            target: "signal_sampler::trigger",
            active = self.voices.len(),
            max = self.max_voices,
            incoming_note,
            "voice steal — polyphony budget exceeded (a ringing note may be cut)"
        );
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
            VoiceStealPolicy::Quietest | VoiceStealPolicy::DropNew => None,
            VoiceStealPolicy::SameNoteFirst => incoming_note
                .and_then(|note| self.voices.iter().position(|v| v.note == note))
                .or_else(|| {
                    self.voices
                        .iter()
                        .position(|v| matches!(v.state, VoiceState::Releasing { .. }))
                }),
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
        Arc::new(SampleData::from_f32(vec![0.0; 32], 1, 48_000, 32))
    }

    fn voice(gain: f32) -> Voice {
        Voice::new(sample(), 60, VoiceKind::SustainLo, 0, gain, 128)
    }

    /// A mono sine `freq_hz` recorded at `sr`, `secs` long.
    fn sine_sample(freq_hz: f64, sr: u32, secs: f64) -> Arc<SampleData> {
        let n = (sr as f64 * secs) as usize;
        let frames: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq_hz * i as f64 / sr as f64).sin() as f32)
            .collect();
        Arc::new(SampleData::from_f32(frames, 1, sr, n))
    }

    /// Estimate frequency by counting zero crossings in a mono buffer.
    fn freq_of(buf: &[f32], out_sr: u32) -> f64 {
        let mut crossings = 0usize;
        for w in buf.windows(2) {
            if (w[0] <= 0.0 && w[1] > 0.0) || (w[0] >= 0.0 && w[1] < 0.0) {
                crossings += 1;
            }
        }
        (crossings as f64 / 2.0) / (buf.len() as f64 / out_sr as f64)
    }

    #[test]
    fn sample_rate_mismatch_is_pitch_compensated() {
        // A 441 Hz tone recorded at 44.1 kHz, played on a 48 kHz engine, must
        // stay 441 Hz — not 441 * 48000/44100 = 480 Hz. `with_rate_scale`
        // (applied by the engine at spawn) supplies the native/output ratio.
        const OUT_SR: u32 = 48_000;
        let data = sine_sample(441.0, 44_100, 0.5);

        // render_block writes interleaved stereo; measure channel 0.
        let mono = |buf: &[f32]| -> Vec<f32> { buf.iter().step_by(2).copied().collect() };

        // Uncompensated (bug): plays sharp.
        let mut bug = Voice::with_rate(data.clone(), 60, VoiceKind::SustainLo, 1.0, 1.0, 128);
        let mut buf = vec![0.0f32; 9_600 * 2];
        bug.render_block(&mut buf);
        let sharp = freq_of(&mono(&buf), OUT_SR);
        assert!(
            (sharp - 480.0).abs() < 6.0,
            "uncompensated 44.1k sample should sound ~480 Hz, got {sharp:.1}"
        );

        // Compensated: back in tune.
        let mut fixed = Voice::with_rate(data, 60, VoiceKind::SustainLo, 1.0, 1.0, 128)
            .with_rate_scale(44_100.0 / OUT_SR as f64);
        let mut buf = vec![0.0f32; 9_600 * 2];
        fixed.render_block(&mut buf);
        let tuned = freq_of(&mono(&buf), OUT_SR);
        assert!(
            (tuned - 441.0).abs() < 5.0,
            "compensated sample should sound ~441 Hz, got {tuned:.1}"
        );
    }

    #[test]
    fn decay_falls_to_sustain_after_the_attack_and_release_starts_from_there() {
        // A constant 1.0 sample, so the output is the envelope itself.
        let n = 48_000;
        let dc = Arc::new(SampleData::from_f32(vec![1.0; n], 1, 48_000, n));
        let mut v = Voice::new(dc.clone(), 60, VoiceKind::SustainLo, 0, 1.0, 1_000)
            .with_attack(100)
            .with_decay(1_000, 0.5);
        let mut buf = vec![0.0f32; 4_000 * 2];
        v.render_block(&mut buf);
        let at = |i: usize| buf[i * 2];
        let peak = at(100);
        assert!(peak > 0.0);
        assert!(
            (at(1_200) / peak - 0.5).abs() < 0.01,
            "sustain {}",
            at(1_200) / peak
        );
        assert!((at(3_999) / peak - 0.5).abs() < 0.01, "holds");
        v.note_off();
        let mut rel = vec![0.0f32; 1_200 * 2];
        v.render_block(&mut rel);
        assert!(
            rel[0] / peak <= 0.5 + 1e-3,
            "release starts at the sustain level"
        );
        assert!(rel[1_100 * 2].abs() < 1e-6, "and ends silent");

        // Without `with_decay` the voice holds full level, as before.
        let mut flat = Voice::new(dc, 60, VoiceKind::SustainLo, 0, 1.0, 1_000).with_attack(100);
        let mut buf2 = vec![0.0f32; 4_000 * 2];
        flat.render_block(&mut buf2);
        assert!((buf2[3_999 * 2] - buf2[100 * 2]).abs() < 1e-6);
    }

    #[test]
    fn the_filter_envelope_opens_each_note_from_closed_to_the_cutoff() {
        // A bright 4 kHz tone through a voice whose filter envelope sweeps
        // from 7 octaves under a 10 kHz cutoff up to it over 200 ms.
        let data = sine_sample(4_000.0, 48_000, 1.0);
        let params = AdsrParams {
            attack_s: 0.2,
            decay_s: 0.0,
            sustain: 1.0,
            release_s: 0.1,
        };
        let filter = VoiceFilter::new(48_000.0, params, 1.0, 10_000.0);
        let mut v = Voice::with_rate(data.clone(), 60, VoiceKind::SustainLo, 1.0, 1.0, 128)
            .with_filter_env(Some(filter));
        let mut buf = vec![0.0f32; 24_000 * 2];
        v.render_block(&mut buf);
        let rms = |a: usize, b: usize| {
            (buf[a * 2..b * 2]
                .iter()
                .step_by(2)
                .map(|x| x * x)
                .sum::<f32>()
                / (b - a) as f32)
                .sqrt()
        };
        let (early, late) = (rms(500, 1_500), rms(14_000, 24_000));
        assert!(early < late * 0.2, "closed at the start: {early} vs {late}");
        assert!(late > 0.5, "open once the attack is done: {late}");

        // Amount 0 is off: the engine gives no filter, the tone is untouched.
        let mut plain = Voice::with_rate(data, 60, VoiceKind::SustainLo, 1.0, 1.0, 128);
        let mut b2 = vec![0.0f32; 2_000 * 2];
        plain.render_block(&mut b2);
        assert!(b2[500 * 2..1_500 * 2].iter().any(|x| x.abs() > 0.5));
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
            VoiceStealPolicy::parse("same-note-first"),
            VoiceStealPolicy::SameNoteFirst
        );
        assert_eq!(VoiceStealPolicy::parse("oldest"), VoiceStealPolicy::Oldest);
        assert_eq!(VoiceStealPolicy::parse("none"), VoiceStealPolicy::DropNew);
        assert_eq!(
            VoiceStealPolicy::parse(""),
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
        let data = Arc::new(SampleData::from_f32(vec![0.0, 1.0, 2.0, 3.0], 1, 48_000, 4));
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8)
            .with_sample_window(1, Some(3));

        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
        assert_eq!(voice.next_frame().0, 0.0);
        assert!(voice.is_done());
    }

    /// The sustain pedal catches strings whose keys are still down, and only
    /// those. A note the player already let go of is damped — a pedal press
    /// afterwards must not freeze its release tail.
    #[test]
    fn pedal_down_catches_held_notes_and_leaves_released_ones_dying() {
        let data = Arc::new(SampleData::from_f32(vec![0.5; 4096], 1, 48_000, 4096));
        let mut pool = VoicePool::new();
        for note in [60u8, 64] {
            pool.spawn(Voice::with_rate(
                Arc::clone(&data),
                note,
                VoiceKind::Zoned,
                1.0,
                1.0,
                8,
            ));
        }
        // Both notes are let go: every voice enters its release.
        for v in pool.voices_mut() {
            v.note_off();
        }
        assert!(
            pool.voices_mut()
                .iter()
                .all(|v| matches!(v.state, VoiceState::Releasing { .. }))
        );

        // The player is still holding 60 (its key is down); 64 was released.
        let restored = pool.repedal_held(&|n| n == 60);
        assert_eq!(restored, 1, "only the held key is caught");
        let held_state = pool
            .voices_mut()
            .iter()
            .find(|v| v.note == 60)
            .map(|v| matches!(v.state, VoiceState::Playing))
            .expect("60");
        let released_state = pool
            .voices_mut()
            .iter()
            .find(|v| v.note == 64)
            .map(|v| matches!(v.state, VoiceState::Releasing { .. }))
            .expect("64");
        assert!(held_state, "held note sustains");
        assert!(released_state, "released note keeps dying away");
    }

    #[test]
    fn forward_loop_wraps_while_playing() {
        let data = Arc::new(SampleData::from_f32(vec![0.0, 1.0, 2.0, 3.0], 1, 48_000, 4));
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
        let data = Arc::new(SampleData::from_f32(vec![0.0, 1.0, 2.0, 3.0], 1, 48_000, 4));
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
        let data = Arc::new(SampleData::from_f32(vec![0.0, 1.0, 2.0, 3.0], 1, 48_000, 4));
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
        let data = Arc::new(SampleData::from_f32(vec![0.0, 1.0, 2.0], 1, 48_000, 3));
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Short, 1.0, 1.0, 8);

        voice.note_off();

        assert_eq!(voice.state, VoiceState::Playing);
        assert_eq!(voice.next_frame().0, 0.0);
        assert_eq!(voice.next_frame().0, 1.0);
        assert_eq!(voice.next_frame().0, 2.0);
    }

    #[test]
    fn repedal_restores_releasing_sustain_voices_whose_key_is_down() {
        let mut pool = VoicePool::new();
        pool.spawn(voice(1.0));

        pool.note_off(60);
        assert!(matches!(pool.voices[0].state, VoiceState::Releasing { .. }));

        // Key still down when the pedal arrives → caught.
        assert_eq!(pool.repedal_held(&|_| true), 1);
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
        let data = Arc::new(SampleData::from_f32(vec![1.0, 1.0], 2, 48_000, 1));

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
        Arc::new(SampleData::from_f32(frames, 2, sr, n))
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

    /// A held, looping note over a STREAMED sample must keep sounding. The
    /// loop wraps backwards, which linear prefetch never sees coming, and the
    /// loop region has to stay resident for as long as the voice holds it —
    /// this is the case that goes silent if chunks are evicted underneath a
    /// sustaining voice.
    ///
    /// (Relocated from the stream module's tests when it moved into
    /// fts-sample — the streamed-file layer has no `Voice` types.)
    #[test]
    fn a_looping_voice_keeps_sounding_over_a_streamed_sample() {
        use crate::engine::stream::StreamedSample;

        let (sr, ch) = (48_000u32, 2u16);
        let n = (sr as f64 * 6.0) as usize; // 6 s, well past head + chunks
        let pcm: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = (t * 220.0 * std::f64::consts::TAU).sin() as f32 * 0.8;
                [v, -v]
            })
            .collect();
        let ints: Vec<i32> = pcm
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
            .collect();
        let Ok(flac) = crate::engine::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };
        let tmp = std::env::temp_dir().join(format!("fts-loop-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));
        let streamed = StreamedSample::open(map, 0, flac.len(), ch, sr, n).expect("stream");
        let data = Arc::new(SampleData::streamed(streamed));

        // Loop the 3rd..5th second — entirely past the head, and longer than
        // one chunk, so it spans several.
        let (loop_start, loop_end) = (sr as usize * 3, sr as usize * 5);
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8)
            .with_forward_loop(loop_start, loop_end);

        // Play 30 seconds — many loop passes, and long enough that the idle
        // sweep runs several times underneath.
        let mut block = vec![0.0f32; 512 * 2];
        let blocks = (sr as usize * 30) / 512;
        let mut silent_runs = 0usize;
        let mut worst_run = 0usize;
        for _ in 0..blocks {
            block.fill(0.0);
            voice.render_block(&mut block);
            let peak = block.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            if peak < 1e-4 {
                silent_runs += 1;
                worst_run = worst_run.max(silent_runs);
            } else {
                silent_runs = 0;
            }
            // Real time: the streamer gets the same wall clock a live rig
            // would give it (512 frames ≈ 10.7 ms).
            std::thread::sleep(std::time::Duration::from_micros(10_667));
        }
        let _ = std::fs::remove_file(&tmp);
        assert!(
            worst_run <= 2,
            "held loop went silent for {worst_run} blocks in a row ({} ms)",
            worst_run * 512 * 1000 / sr as usize,
        );
    }
}
