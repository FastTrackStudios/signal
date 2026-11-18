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
    /// The strike (note-on, or the note-off that triggered release samples)
    /// that spawned this voice — shared by every mic and layer sample of it;
    /// what a polyphony limit counts. Set by [`VoicePool::spawn`].
    strike: u32,
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
    amp_bp: Option<(
        Arc<crate::native::breakpoints::Breakpoints>,
        crate::native::breakpoints::EnvPlayer,
        f32,
    )>,
    /// The breakpoint envelope at control rate: evaluated every
    /// [`ENV_CONTROL_FRAMES`] frames and interpolated between (`a` → `b`,
    /// `i` frames into the period). Per-sample evaluation made the envelope
    /// the render thread's top cost when a forearm lands on the keys.
    bp_ctrl: (f32, f32, u32, bool),
    /// Per-voice filter envelope (`with_filter_env`); `None` = no filter.
    filter: Option<VoiceFilter>,
    /// The layer's Kontakt group insert filters/EQs (`with_inserts`).
    inserts: Option<super::inserts::InsertState>,
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
    /// The key whose note-off releases this voice (its envelope, a
    /// note-off fade): its own note, or the key of the event that played it
    /// (a script's `play_note(…, −1)` child ends with its parent — MOJO 2's
    /// divisi plays a chord's voices from one note's callback).
    pub env_key: u8,

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
    /// The CC-independent part of this voice's gain (zone volume, instrument
    /// gain, makeup, unison compensation). CC1/CC2 re-levelling multiplies the
    /// controller gain by THIS — re-levelling to the bare controller gain
    /// dropped it (a held note fell to its unscaled level on the first CC).
    pub level: f32,
    /// Curvature of the note-off release (`k` of `(e^(−k·x) − e^(−k)) /
    /// (1 − e^(−k))`, x = elapsed fraction). 0 = the linear fade. Kontakt's
    /// AHDSR release on CSS sustains measured k = 2.6 over 580 ms.
    release_k: f32,
    /// Frames of a linear fade multiplied onto the key-up release
    /// (`cut_release`); 0 = none.
    release_cut: usize,
    /// `release_k` while a NOTE-OFF release runs; retire/steal fades leave it
    /// 0 (linear), as Kontakt's script fades are.
    release_k_active: f32,
    /// Second amplitude envelope multiplied with `flex` — Kontakt runs a
    /// group's ENV_FLEX and ENV_AHDSR together (release samples: AHD 250 ms
    /// attack / 25 s decay to 0.25 under the flex tail).
    flex2: Option<FlexEnv>,
    /// A pitch envelope (`with_pitch_env`): its level (0..1) times
    /// `pitch_env_cents` nudges the read rate — Kontakt's group ENV_AHDSR
    /// routed to pitch (CSW marcato: ~+10 c held 333 ms, decaying 333 ms).
    /// Dropped once it settles at 0.
    pitch_env: Option<FlexEnv>,
    pitch_env_cents: f32,

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
    /// The loudest `env_peak` this voice has reached — its onset level, for
    /// how far the body has decayed since ([`VoicePool::note_body_ratio`]).
    max_peak: f32,
    /// Render-trace correlation id (set at spawn when tracing) — pairs this
    /// voice's lifetime with its `TraceKind::VoiceSpawn`/`VoiceEnd` events.
    pub trace_id: Option<u64>,
    /// A script's handle on this voice (0 = none): the fretboard tags the
    /// voices it must later release or fade by themselves (a stitch, a
    /// squeak), as Kontakt scripts keep `play_note` event ids.
    pub script_tag: u32,
    /// Time-preserving pitch shift (L/R), `None` when the note is on the
    /// recorded grid (`pitch_cents == 0`) — those voices pay nothing.
    pitch: Option<[crate::engine::pitch_shift::PitchShifter; 2]>,
    /// Time stretch (a script's `ENGINE_PAR_SPEED` on a time-machine
    /// group): the playhead moves at `speed` × the pitch's rate while two
    /// read heads play the sample at its pitch. `None` plays it plainly.
    stretch: Option<Stretch>,
    /// A script's `fade_out` owns this voice's end (see `script_fade`).
    script_faded: bool,
    /// Its key went up (a note-off reached it). A script's fade is not a
    /// key-up: Kontakt's `fade_out` ramps the voice's level and leaves its
    /// envelopes held, so a script-faded voice's envelopes release only
    /// with the key.
    key_up: bool,
    /// Reads through Kontakt's Normal-quality resampler
    /// ([`super::resample`]) instead of the linear one: set by the pool for a
    /// library whose `performance.interpolation` asks for it.
    kt: Option<KontaktVoice>,
    /// Plays grain by grain through Kontakt's Time Machine II
    /// ([`super::tm2`]): set for a zone of a Time Machine II group whose
    /// marks are found. Boxed (its block buffers are ~12 KB): one allocation
    /// at the spawn, none while it plays.
    tm2: Option<Box<Tm2Voice>>,
}

/// A Time Machine II voice: its machine (made at its first frame, from the
/// voice's start), the engine frame it started at (its blocks lie on the
/// engine's [`KONTAKT_BLOCK`](super::resample::KONTAKT_BLOCK) grid, as
/// `BDsp::GetBlockMachine` runs the machine once a host block), and the
/// block of frames planned.
#[derive(Clone)]
struct Tm2Voice {
    marks: super::tm2::Marks,
    /// Speed (1 = as recorded) and the sample's rate over the engine's.
    speed: f64,
    ratio: f64,
    origin: u64,
    machine: Option<super::tm2::Machine>,
    layout: super::resample::Layout,
    frames: [super::tm2::Frame; super::resample::KONTAKT_BLOCK],
    old: [super::resample::Tap; 2 * super::resample::KONTAKT_BLOCK],
    new: [super::resample::Tap; super::resample::KONTAKT_BLOCK],
    len: usize,
    used: usize,
    /// The step the last frame moved at, which forms the next block's.
    step: f64,
    /// The virtual position at the planned block's start, and its move a
    /// frame: the voice's position.
    vstart: f64,
    per: f32,
}

impl std::fmt::Debug for Tm2Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tm2Voice")
            .field("speed", &self.speed)
            .field("origin", &self.origin)
            .field("used", &self.used)
            .finish_non_exhaustive()
    }
}

/// A voice reading through Kontakt's Normal-quality resampler: the engine
/// frame it started at (its blocks lie on the engine's absolute
/// [`KONTAKT_BLOCK`](super::resample::KONTAKT_BLOCK) grid), what it plays,
/// its float cursor and the block of reads planned. Inline, so spawning one
/// allocates nothing.
#[derive(Clone, Copy, Debug)]
struct KontaktVoice {
    origin: u64,
    started: bool,
    layout: super::resample::Layout,
    /// The cursor after the planned block.
    cursor: super::resample::Cursor,
    /// The planned block: `taps[used..len]` are the next frames' reads.
    taps: super::resample::Plan,
    len: usize,
    used: usize,
    /// The block was copied whole (`|step − 1| ≤ 1e-4`): each read is its
    /// frame, its fraction ignored.
    copy: bool,
    /// The step the last frame moved at, which forms the next block's.
    step: f64,
}

impl KontaktVoice {
    fn new(origin: u64) -> Self {
        Self {
            origin,
            started: false,
            layout: super::resample::Layout {
                start: 0,
                end: 0,
                looped: None,
            },
            cursor: super::resample::Cursor::default(),
            taps: [super::resample::Tap::default(); super::resample::KONTAKT_BLOCK],
            len: 0,
            used: 0,
            copy: false,
            step: 1.0,
        }
    }
}

/// A stretching voice's grain, output frames (43 ms at 48 kHz).
pub const STRETCH_GRAIN: u64 = 2048;

/// The state of a time-stretched voice: two read heads, each a pitch-rate
/// cursor that restarts at the playhead once a grain, half a grain apart,
/// crossfaded by triangles that sum to one — KONTRA's model of Kontakt's
/// time machine (head 0 plays the note's start in full).
#[derive(Clone, Copy, Debug)]
pub struct Stretch {
    speed: f64,
    heads: [f64; 2],
    /// Output frames since the stretch began, which place its grains.
    grain: u64,
}

/// Catmull-Rom between `y1` and `y2` at `t` (KONTRA's stretched read).
#[inline]
fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

/// Kontakt `ENV_FLEX` segment shape: `a = (0.5 − tension)·FLEX_CURVE_SCALE`,
/// level `(e^{a·t} − 1)/(e^a − 1)` along the segment.
pub const FLEX_CURVE_SCALE: f32 = 15.0;

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
    /// Per segment, the shape's constants: curvature `a` (0 = linear),
    /// `1 / (e^a − 1)`, and `e^(a / len)` — one frame's step along it.
    curve: Vec<(f64, f64, f64)>,
    /// Frames elapsed along the envelope timeline.
    pos: f64,
    /// While `Some`, `pos` is clamped to this value until the voice releases —
    /// the sustain-hold freeze (see struct docs).
    hold_end: Option<f64>,
    /// Where `pos` is, walked frame by frame: the segment, the frames into
    /// it, and `e^(a·t/len)` there — so a frame costs a multiply, not two
    /// `exp`s and a walk of the segment list (it runs per voice per frame).
    seg: usize,
    t: f64,
    eat: f64,
    /// The segment a note-off jumps to (from the current level) — Kontakt's
    /// release stage after the sustain loop. `None` = keep walking.
    release_seg: Option<usize>,
    /// Modulation intensity: the gain is `1 − depth·(1 − level)` (Kontakt's
    /// volume envelope routed at less than full intensity sits above a
    /// floor of `1 − depth`). 1 = the plain envelope.
    depth: f32,
    /// Once released and past its last segment the envelope is off: silent,
    /// and its voice is done (Kontakt stops a voice when its volume envelope
    /// ends, whatever the intensity).
    stops_voice: bool,
    released: bool,
    /// Runs as Kontakt's control-rate AHDSR instead of the segments
    /// ([`Self::kontakt_ahdsr`]).
    kontakt: Option<super::kontakt_ahdsr::Ahdsr>,
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
        let curve = segs
            .iter()
            .map(|&(len, _, _, tension)| {
                let a = Self::curvature(tension);
                if a == 0.0 {
                    (0.0, 0.0, 1.0)
                } else {
                    (a, 1.0 / (a.exp() - 1.0), (a / len).exp())
                }
            })
            .collect();
        Some(Self {
            segs,
            curve,
            pos: 0.0,
            hold_end,
            seg: 0,
            t: 0.0,
            eat: 1.0,
            release_seg: None,
            depth: 1.0,
            stops_voice: false,
            released: false,
            kontakt: None,
        })
    }

    /// Run at `depth` intensity (see [`FlexEnv::depth`]); the voice ends
    /// with the envelope.
    #[must_use]
    pub fn with_depth_and_stop(mut self, depth: f32) -> Self {
        self.depth = depth.clamp(0.0, 1.0);
        self.stops_voice = true;
        self
    }

    /// Released and run past its last segment, with `stops_voice` set; a
    /// Kontakt AHDSR once it is off (Kontakt ends a voice with its volume
    /// envelope, whatever the depth).
    #[must_use]
    pub fn ended(&self) -> bool {
        if let Some(k) = &self.kontakt {
            return k.is_off();
        }
        self.stops_voice && self.released && self.settled()
    }

    /// Kontakt's volume AHDSR at its control rate
    /// ([`super::kontakt_ahdsr`]): `settings` replace the segments; the
    /// depth and the key-up behave as before.
    #[must_use]
    pub fn kontakt_ahdsr(settings: super::kontakt_ahdsr::Settings) -> Self {
        Self {
            segs: Vec::new(),
            curve: Vec::new(),
            pos: 0.0,
            hold_end: None,
            seg: 0,
            t: 0.0,
            eat: 1.0,
            release_seg: None,
            depth: 1.0,
            stops_voice: true,
            released: false,
            kontakt: Some(super::kontakt_ahdsr::Ahdsr::new(settings)),
        }
    }

    /// Declare the release segment (see [`Self::release`]).
    #[must_use]
    pub fn with_release_segment(mut self, seg: Option<usize>) -> Self {
        self.release_seg = seg.filter(|&s| s < self.segs.len());
        self
    }

    /// Note-off: jump to the release segment from the current level (once;
    /// a no-op when there is none or the envelope is already past it).
    pub fn release(&mut self) {
        if let Some(k) = self.kontakt.as_mut() {
            if !self.released {
                k.release();
            }
            self.released = true;
            return;
        }
        self.released = true;
        let Some(rs) = self.release_seg.take() else {
            return;
        };
        if self.seg >= rs {
            // Already at (or past) the release segment — a held envelope
            // frozen at its hold point sits on the release segment's first
            // frame. The key-up still lifts the freeze, so it walks on.
            self.hold_end = None;
            return;
        }
        let lvl = self.level_here();
        self.segs[rs].1 = lvl;
        self.pos = self.segs[..rs].iter().map(|s| s.0).sum();
        self.seg = rs;
        self.t = 0.0;
        self.eat = 1.0;
        self.hold_end = None;
    }

    /// Kontakt segment-tension shape law. `t` is normalized progress [0,1]
    /// through a segment; `tension` ∈ [0,1] is the shipped `curve` field
    /// (0.5 = linear, <0.5 concave, >0.5 convex). PINNED law (not fitted): a
    /// fixed exponential-tension curve, the standard shape for Kontakt bipolar
    /// curve controls — `f(t) = (e^{a t} − 1)/(e^a − 1)` with
    /// `a = (0.5 − tension)·FLEX_CURVE_SCALE`, linear at tension = 0.5
    /// (sign and scale null-tested against Kontakt).
    #[inline]
    fn shape(t: f32, tension: f32) -> f32 {
        // Sign measured against Kontakt (CSS legato envelope, null −41 dB):
        // tension > 0.5 is CONCAVE (fast start), < 0.5 convex. The earlier
        // `(tension − 0.5)` had every curved segment mirrored. Scale 15: the
        // decoded CSS marcato body/transition envelopes null Kontakt at
        // −42 dB with it, −37 at 12 or 16.
        let a = (0.5 - tension) * FLEX_CURVE_SCALE;
        if a.abs() < 1.0e-3 {
            t
        } else {
            ((a * t).exp() - 1.0) / (a.exp() - 1.0)
        }
    }

    /// The shape law's `a` for `tension` — 0 for a (near-)linear segment.
    fn curvature(tension: f32) -> f64 {
        let a = (0.5 - f64::from(tension)) * f64::from(FLEX_CURVE_SCALE);
        if a.abs() < 1.0e-3 { 0.0 } else { a }
    }

    /// The level where the walk is (see [`level_at`](Self::level_at), the
    /// same law evaluated from scratch).
    #[inline]
    fn level_here(&self) -> f32 {
        let Some(&(len, from, to, _)) = self.segs.get(self.seg) else {
            return self.segs.last().map_or(0.0, |s| s.2);
        };
        let (a, inv, _) = self.curve[self.seg];
        let frac = if a == 0.0 {
            (self.t / len).clamp(0.0, 1.0)
        } else {
            ((self.eat - 1.0) * inv).clamp(0.0, 1.0)
        };
        from + (to - from) * frac as f32
    }

    /// One frame along the timeline.
    #[inline]
    fn step(&mut self) {
        self.pos += 1.0;
        let Some(&(len, ..)) = self.segs.get(self.seg) else {
            return;
        };
        self.t += 1.0;
        if self.t < len {
            self.eat *= self.curve[self.seg].2;
            return;
        }
        // Into the next segment(s): re-anchor the exponential there.
        let mut len = len;
        while self.t >= len {
            self.t -= len;
            self.seg += 1;
            match self.segs.get(self.seg) {
                Some(s) => len = s.0,
                None => return,
            }
        }
        let a = self.curve[self.seg].0;
        self.eat = if a == 0.0 {
            1.0
        } else {
            (a * self.t / len).exp()
        };
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
    pub(crate) fn next(&mut self, released: bool) -> f32 {
        if self.ended() {
            return 0.0;
        }
        if self.kontakt.is_some() {
            if released {
                self.release();
            }
            let lvl = self
                .kontakt
                .as_mut()
                .map_or(0.0, super::kontakt_ahdsr::Ahdsr::tick);
            return 1.0 - self.depth * (1.0 - lvl);
        }
        let lvl = self.level_here();
        let frozen = !released && self.hold_end.is_some_and(|h| self.pos >= h);
        if !frozen {
            self.step();
        }
        if self.depth < 1.0 {
            1.0 - self.depth * (1.0 - lvl)
        } else {
            lvl
        }
    }

    /// The walk is past the last segment: the level holds from here on.
    #[inline]
    fn settled(&self) -> bool {
        self.seg >= self.segs.len()
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
            level: 1.0,
            release_k: 0.0,
            release_cut: 0,
            release_k_active: 0.0,
            flex2: None,
            pitch_env: None,
            pitch_env_cents: 0.0,
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
            strike: 0,
            frames_out: 0,
            arrival_emitted: None,
            arrival_drained: false,
            bloom_frames: 0,
            ds_level: 1.0,
            sustain: 1.0,
            decay_left: 0,
            amp_bp: None,
            bp_ctrl: (0.0, 0.0, 0, false),
            filter: None,
            inserts: None,
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
            env_key: note,
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
            max_peak: 0.0,
            trace_id: None,
            script_tag: 0,
            pitch: None,
            stretch: None,
            script_faded: false,
            key_up: false,
            kt: None,
            tm2: None,
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
            level: 1.0,
            release_k: 0.0,
            release_cut: 0,
            release_k_active: 0.0,
            flex2: None,
            pitch_env: None,
            pitch_env_cents: 0.0,
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
            strike: 0,
            frames_out: 0,
            arrival_emitted: None,
            arrival_drained: false,
            bloom_frames: 0,
            ds_level: 1.0,
            sustain: 1.0,
            decay_left: 0,
            amp_bp: None,
            bp_ctrl: (0.0, 0.0, 0, false),
            filter: None,
            inserts: None,
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
            env_key: note,
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
            max_peak: 0.0,
            trace_id: None,
            script_tag: 0,
            pitch: None,
            stretch: None,
            script_faded: false,
            key_up: false,
            kt: None,
            tm2: None,
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

    /// Stretch time by `speed` (1.0 = as recorded) at the voice's pitch: the
    /// playhead advances `speed` × faster while the pitch holds. A unity
    /// speed is a no-op.
    #[must_use]
    pub fn with_stretch(mut self, speed: f64) -> Self {
        self.stretch = ((speed - 1.0).abs() > 1e-9 && speed.is_finite()).then_some(Stretch {
            speed: speed.clamp(0.0, 8.0),
            heads: [0.0; 2],
            grain: 0,
        });
        self
    }

    /// Play through Kontakt's Time Machine II at `speed` (1 = as recorded),
    /// grain by grain from `marks` (the sample's, [`super::tm2::MarkTable`]);
    /// `ratio` is the sample's rate over the engine's. Unlike
    /// [`Self::with_stretch`], unity speed plays grains too, as Kontakt's
    /// does for every voice of a Time Machine II group.
    #[must_use]
    pub fn with_tm2(mut self, marks: super::tm2::Marks, speed: f64, ratio: f64) -> Self {
        use super::resample::{KONTAKT_BLOCK, Layout, Tap};
        self.stretch = None;
        self.tm2 = Some(Box::new(Tm2Voice {
            marks,
            speed: if speed.is_finite() { speed } else { 1.0 },
            ratio,
            origin: 0,
            machine: None,
            layout: Layout {
                start: 0,
                end: 0,
                looped: None,
            },
            frames: [super::tm2::Frame::default(); KONTAKT_BLOCK],
            old: [Tap::default(); 2 * KONTAKT_BLOCK],
            new: [Tap::default(); KONTAKT_BLOCK],
            len: 0,
            used: 0,
            step: 1.0,
            vstart: 0.0,
            per: 0.0,
        }));
        self
    }

    /// One stretched frame: the two heads at the playhead's grain, each read
    /// through a four-point Hermite, crossfaded by their triangles.
    fn read_stretched(&mut self) -> (f32, f32) {
        let pos = self.position;
        let Some(st) = self.stretch.as_mut() else {
            return (0.0, 0.0);
        };
        let half = STRETCH_GRAIN / 2;
        let g = st.grain % STRETCH_GRAIN;
        if st.grain == 0 {
            st.heads = [pos; 2];
        } else if g == half {
            st.heads[0] = pos;
        } else if g == 0 {
            st.heads[1] = pos;
        }
        let phase = ((st.grain + half) % STRETCH_GRAIN) as f32 / STRETCH_GRAIN as f32;
        let w = 1.0 - (2.0 * phase - 1.0).abs();
        let heads = st.heads;
        let (a, b) = (self.read_hermite(heads[0]), self.read_hermite(heads[1]));
        (w * a.0 + (1.0 - w) * b.0, w * a.1 + (1.0 - w) * b.1)
    }

    /// A four-point Hermite read at `pos`, the frames clamped to the sample.
    fn read_hermite(&self, pos: f64) -> (f32, f32) {
        let i = pos.floor() as i64;
        let t = (pos - i as f64) as f32;
        let last = self.end_frame.saturating_sub(1) as i64;
        let f = |k: i64| self.data.frame((i + k).clamp(0, last.max(0)) as usize);
        let (y0, y1, y2, y3) = (f(-1), f(0), f(1), f(2));
        (
            hermite(y0.0, y1.0, y2.0, y3.0, t),
            hermite(y0.1, y1.1, y2.1, y3.1, t),
        )
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

    /// The key whose note-off releases this voice (see [`Voice::env_key`]).
    pub fn with_env_key(mut self, key: u8) -> Self {
        self.env_key = key;
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
    pub fn with_pan(self, pan: f32) -> Self {
        self.with_pan_law(pan, false)
    }

    /// [`with_pan`](Self::with_pan), with `mono_balance` putting a mono
    /// source on the stereo balance law (see
    /// `PerformanceSpec::mono_balance_pan`).
    #[must_use]
    pub fn with_pan_law(mut self, pan: f32, mono_balance: bool) -> Self {
        let pan = pan.clamp(-1.0, 1.0);
        if self.data.channels >= 2 || mono_balance {
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

    /// Curve the note-off release (see [`Self::release_k`]).
    #[must_use]
    pub fn with_release_curve(mut self, k: f32) -> Self {
        self.release_k = k;
        self
    }

    /// Replace this voice's flex envelope after spawn (a per-event envelope,
    /// e.g. the CSS transition shaped by that transition's CC66–68).
    pub fn set_flex_env(&mut self, env: FlexEnv) {
        self.flex = Some(env);
    }

    /// Replace the second envelope after spawn (see [`Self::flex2`]).
    pub fn set_flex_env2(&mut self, env: FlexEnv) {
        self.flex2 = Some(env);
    }

    /// Attach the second envelope (see [`Self::flex2`]).
    #[must_use]
    pub fn with_flex_env2(mut self, env: FlexEnv) -> Self {
        self.flex2 = Some(env);
        self
    }

    /// Attach a pitch envelope: its level (0..1, one-shot) times `cents`
    /// offsets the voice's pitch (see [`Self::pitch_env`]).
    #[must_use]
    pub fn with_pitch_env(mut self, env: FlexEnv, cents: f32) -> Self {
        if cents != 0.0 {
            self.pitch_env = Some(env);
            self.pitch_env_cents = cents;
        }
        self
    }

    /// Whether `idx` is before the playable window. A position inside the
    /// loop is always playable: a voice that STARTS past its loop start (a
    /// CSS body variant entering the file deep, Kontakt's velocity-3 zone at
    /// frame 168000 looping 48010..end) wraps below its start frame — which
    /// used to end the voice at its first wrap.
    #[inline]
    fn below_window(&self, idx: usize) -> bool {
        idx < self.start_frame && !self.loop_range.is_some_and(|(ls, _)| idx >= ls)
    }

    /// Release-fade gain for `t` = remaining fraction (1 → 0).
    #[inline]
    fn release_shape(&self, t: f32) -> f32 {
        let k = self.release_k_active;
        if k < 1e-4 {
            return t;
        }
        let x = 1.0 - t;
        ((-k * x).exp() - (-k).exp()) / (1.0 - (-k).exp())
    }

    /// Record the CC-independent part of the gain (see [`Self::level`]).
    #[must_use]
    pub fn with_level(mut self, level: f32) -> Self {
        self.level = level;
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

    /// Fade a key-up release linearly to silence over `frames` on top of
    /// its own shape, ending the voice there (see
    /// `PerformanceSpec::sustain_noteoff_cut_ms`). A voice not releasing
    /// from a key-up is left alone.
    pub fn cut_release(&mut self, frames: usize) {
        if frames > 0
            && matches!(self.state, VoiceState::Releasing { .. })
            && !matches!(self.kind, VoiceKind::Release | VoiceKind::Short)
        {
            self.release_cut = frames;
        }
    }

    /// Run the layer's group insert chain (see [`super::inserts`]).
    #[must_use]
    pub fn with_inserts(mut self, inserts: Option<super::inserts::InsertState>) -> Self {
        self.inserts = inserts;
        self
    }

    /// Whether the voice runs a group insert chain.
    /// A balance (−1..1) the insert chain applies after its drive stages
    /// (see [`super::inserts::InsertState::set_pan`]); none without inserts.
    #[must_use]
    pub fn with_insert_pan(mut self, pan: f32) -> Self {
        if let Some(f) = &mut self.inserts {
            f.set_pan(pan);
        }
        self
    }

    #[must_use]
    pub fn has_inserts(&self) -> bool {
        self.inserts.is_some()
    }

    /// The controllers the insert chain's routes follow, read again.
    pub fn set_insert_ccs(&mut self, cc: impl Fn(u8) -> u8) {
        if let Some(f) = &mut self.inserts {
            f.set_ccs(cc);
        }
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
    /// Fade a just-spawned voice in from silence over `frames` (Kontakt's
    /// `fade_in` on a new event) — the in-place form of
    /// [`with_fade_in_under`](Self::with_fade_in_under) with no delay.
    pub fn start_fade_in(&mut self, frames: usize) {
        if frames > 0 {
            self.gain = 0.0;
            self.gain_ramp_frames = frames;
        }
    }

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

    /// A script's `wait(hold)` + `fade_out(fade)` on a voice it just
    /// started: full for `hold_frames` (its fade-in included), then a
    /// linear fade to silence over `fade_frames`.
    #[must_use]
    pub fn with_delayed_fade(mut self, hold_frames: usize, fade_frames: usize) -> Self {
        self.release_hold = hold_frames.max(1);
        self.pending_fade = fade_frames.max(1);
        self.release_k_active = 0.0;
        self.script_faded = true;
        self
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
        self.key_up = true;
        match self.kind {
            // A one-shot plays to its end — unless a breakpoint amp envelope
            // shapes it, which releases with the key like any other voice.
            // A one-shot plays to its end; its envelope still takes the
            // key-up (Kontakt's short groups release from the note-off).
            VoiceKind::Short if self.amp_bp.is_none() => {
                if let Some(f) = &mut self.flex {
                    f.release();
                }
            }
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
                            }) * bp.unit_s();
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
                    self.release_k_active = self.release_k;
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
                    self.release_cut = 0;
                }
            }
        }
    }

    /// Whether the voice's envelopes walk past their hold: it is fading
    /// (`state`, the state the frame plays in) by its key-up or by anything
    /// but a script.
    #[inline]
    fn envelopes_released(state: &VoiceState, key_up: bool, script_faded: bool) -> bool {
        !matches!(state, VoiceState::Playing) && (key_up || !script_faded)
    }

    /// Returns true when this voice should be removed from the pool.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.state == VoiceState::Done || self.flex.as_ref().is_some_and(FlexEnv::ended)
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

    /// One frame read through Kontakt's Normal-quality resampler: plans a
    /// block of reads when the last is used up (from the voice's first frame
    /// to the engine's next [`KONTAKT_BLOCK`] boundary, then a whole block
    /// at a time, each at the step its first frame moves at), and plays the
    /// next read — the frame itself in a copied block, else the 4-point
    /// Lagrange about it. A voice Kontakt's path does not model (reversed,
    /// ping-pong, stretched, pitch-shifted, crossfaded loop) falls back to
    /// the linear read at its first frame.
    ///
    /// [`KONTAKT_BLOCK`]: super::resample::KONTAKT_BLOCK
    fn read_kontakt(&mut self) -> (f32, f32) {
        use super::resample::{self, Cursor, KONTAKT_BLOCK, Layout};
        let modelled = !self.reverse
            && !self.alternating_loop
            && self.stretch.is_none()
            && self.pitch.is_none()
            && self.loop_xfade == 0;
        let now = self.frames_out.saturating_sub(1);
        let (bend, rate, glide) = (self.bend, self.rate, self.glide_cents);
        let glide_on = self.glide_frames > 0;
        let (start, end, looped, position) = (
            self.start_frame as i64,
            self.end_frame as i64,
            self.loop_range,
            self.position,
        );
        let Some(kt) = self.kt.as_mut() else {
            return (0.0, 0.0);
        };
        if !kt.started {
            if !modelled {
                self.kt = None;
                return self.read_interp(self.position);
            }
            let pos = position.floor();
            kt.cursor = Cursor {
                pos: pos as i64,
                frac: (position - pos) as f32,
            };
            kt.layout = Layout {
                start,
                end,
                // Kontakt takes a loop while the playhead is before its end.
                looped: looped
                    .map(|(a, b)| (a as i64, b as i64))
                    .filter(|&(_, le)| kt.cursor.pos < le),
            };
            kt.step = bend
                * if glide_on {
                    rate * 2f64.powf(f64::from(glide) / 1200.0)
                } else {
                    rate
                };
            kt.started = true;
            kt.len = 0;
            kt.used = 0;
        }
        if kt.used >= kt.len {
            let at = kt.origin + now;
            let frames = KONTAKT_BLOCK - (at % KONTAKT_BLOCK as u64) as usize;
            let layout = kt.layout;
            kt.copy = resample::block(
                &layout,
                &mut kt.cursor,
                kt.step as f32,
                &mut kt.taps[..frames],
            );
            kt.len = frames;
            kt.used = 0;
        }
        let (tap, copy, layout) = (kt.taps[kt.used], kt.copy, kt.layout);
        let last = self.end_frame.saturating_sub(1);
        if copy {
            return match layout.frame(tap.at) {
                Some(f) => {
                    self.data
                        .frame_pair_cursored(f, last, &mut self.stream_cursor)
                        .0
                }
                None => (0.0, 0.0),
            };
        }
        let f = [-1i64, 0, 1, 2].map(|k| layout.frame(tap.at + k));
        let y = match f {
            // The common case: four frames in a row, two cursored reads.
            [Some(a), _, _, Some(d)] if d == a + 3 => {
                let (y0, y1) = self
                    .data
                    .frame_pair_cursored(a, last, &mut self.stream_cursor);
                let (y2, y3) = self
                    .data
                    .frame_pair_cursored(a + 2, last, &mut self.stream_cursor);
                [y0, y1, y2, y3]
            }
            _ => f.map(|f| {
                f.map_or((0.0, 0.0), |f| {
                    self.data
                        .frame_pair_cursored(f, last, &mut self.stream_cursor)
                        .0
                })
            }),
        };
        (
            resample::lagrange(tap.frac, y[0].0, y[1].0, y[2].0, y[3].0),
            resample::lagrange(tap.frac, y[0].1, y[1].1, y[2].1, y[3].1),
        )
    }

    /// One frame of a Time Machine II voice: plans a block of grain frames
    /// when the last is used up (from the voice's first frame to the
    /// engine's next [`KONTAKT_BLOCK`] boundary, then a whole block at a
    /// time, each at the step its first frame moves at), and plays the next:
    /// the grain's read at its weight plus the fading grain's at its own,
    /// each by Normal quality's Lagrange (a copied block reads frames).
    /// `None` once the grains ran out.
    ///
    /// [`KONTAKT_BLOCK`]: super::resample::KONTAKT_BLOCK
    fn read_tm2(&mut self) -> Option<(f32, f32)> {
        use super::resample::{KONTAKT_BLOCK, Layout};
        let now = self.frames_out.saturating_sub(1);
        let glide = (self.glide_frames > 0).then_some(self.glide_cents);
        let (bend, rate, end, position) = (self.bend, self.rate, self.end_frame, self.position);
        let t = self.tm2.as_deref_mut()?;
        if t.machine.is_none() {
            t.layout = Layout {
                start: 0,
                end: end as i64,
                looped: None,
            };
            t.step = bend * glide.map_or(rate, |g| rate * 2f64.powf(f64::from(g) / 1200.0));
            t.machine = Some(super::tm2::Machine::new(
                t.marks.clone(),
                end as i64,
                (t.speed, t.ratio),
                position,
            ));
            t.len = 0;
            t.used = 0;
        }
        if t.used >= t.len {
            let at = t.origin + now;
            let frames = KONTAKT_BLOCK - (at % KONTAKT_BLOCK as u64) as usize;
            let layout = t.layout;
            let step = t.step as f32;
            let m = t.machine.as_mut()?;
            t.vstart = m.vpos;
            t.per = m.per_frame();
            m.block(
                &layout,
                step,
                &mut t.frames[..frames],
                &mut t.old,
                &mut t.new,
            );
            t.len = frames;
            t.used = 0;
        }
        let (f, layout) = (t.frames[t.used], t.layout);
        if f.silent {
            return None;
        }
        let mut out = (0.0, 0.0);
        for (tap, gain) in [(f.new, f.w), (f.old, f.g)] {
            if gain != 0.0 {
                let v = self.read_tap(&layout, tap, f.copy);
                out = (out.0 + gain * v.0, out.1 + gain * v.1);
            }
        }
        Some(out)
    }

    /// One planned read of Kontakt's resampler: the frame itself in a copied
    /// block, else the 4-point Lagrange about it.
    fn read_tap(
        &mut self,
        layout: &super::resample::Layout,
        tap: super::resample::Tap,
        copy: bool,
    ) -> (f32, f32) {
        use super::resample;
        let last = self.end_frame.saturating_sub(1);
        if copy {
            return match layout.frame(tap.at) {
                Some(f) => {
                    self.data
                        .frame_pair_cursored(f, last, &mut self.stream_cursor)
                        .0
                }
                None => (0.0, 0.0),
            };
        }
        let f = [-1i64, 0, 1, 2].map(|k| layout.frame(tap.at + k));
        let y = match f {
            [Some(a), _, _, Some(d)] if d == a + 3 => {
                let (y0, y1) = self
                    .data
                    .frame_pair_cursored(a, last, &mut self.stream_cursor);
                let (y2, y3) = self
                    .data
                    .frame_pair_cursored(a + 2, last, &mut self.stream_cursor);
                [y0, y1, y2, y3]
            }
            _ => f.map(|f| {
                f.map_or((0.0, 0.0), |f| {
                    self.data
                        .frame_pair_cursored(f, last, &mut self.stream_cursor)
                        .0
                })
            }),
        };
        (
            resample::lagrange(tap.frac, y[0].0, y[1].0, y[2].0, y[3].0),
            resample::lagrange(tap.frac, y[0].1, y[1].1, y[2].1, y[3].1),
        )
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
                let elapsed = self.release_frames.saturating_sub(*frames_remaining);
                *frames_remaining -= 1;
                if self.release_cut > 0 {
                    // the script's fade on top of the release: silent at its end
                    let lin = 1.0 - elapsed as f32 / self.release_cut as f32;
                    if lin <= 0.0 {
                        self.state = VoiceState::Done;
                        return (0.0, 0.0);
                    }
                    self.release_shape(t) * lin
                } else {
                    self.release_shape(t)
                }
            }
            VoiceState::Done => return (0.0, 0.0),
        };

        // Read sample with linear interpolation
        let frame_idx = self.position as usize;
        if frame_idx >= self.end_frame || self.below_window(frame_idx) {
            self.state = VoiceState::Done;
            return (0.0, 0.0);
        }

        let (mut l, mut r) = if self.tm2.is_some() {
            match self.read_tm2() {
                Some(v) => v,
                None => {
                    self.state = VoiceState::Done;
                    return (0.0, 0.0);
                }
            }
        } else if self.stretch.is_some() {
            self.read_stretched()
        } else if self.kt.is_some() {
            self.read_kontakt()
        } else {
            self.read_interp(self.position)
        };

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
        if let Some(f) = &mut self.inserts {
            (l, r) = f.tick(l, r);
        }

        // Decoded ENV_FLEX amplitude envelope. Freezes at the sustain-hold
        // point while Playing (held note stays steady); advances through the
        // decay once Releasing. Multiplied on top of the note-off release fade.
        let released = Self::envelopes_released(&self.state, self.key_up, self.script_faded);
        let flex = match &mut self.flex {
            Some(f) => f.next(released),
            None => 1.0,
        } * match &mut self.flex2 {
            Some(f) => f.next(released),
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
            Some((bp, player, dt)) => {
                // Control rate: the envelope one period ahead, interpolated.
                let (a, b, i, primed) = &mut self.bp_ctrl;
                let k = ENV_CONTROL_FRAMES as f32;
                if !*primed {
                    *a = player.tick(bp, *dt * k);
                    *b = player.tick(bp, *dt * k);
                    *i = 0;
                    *primed = true;
                } else if *i >= ENV_CONTROL_FRAMES {
                    *a = *b;
                    *b = player.tick(bp, *dt * k);
                    *i = 0;
                }
                let v = *a + (*b - *a) * (*i as f32 / k);
                *i += 1;
                (1.0, v)
            }
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
        // Pitch envelope: e^y to second order (y ≤ ~0.01 here; the error is
        // y³/6), so no exp per frame.
        let penv = match &mut self.pitch_env {
            Some(e) => {
                let lvl = e.next(true);
                if lvl == 0.0 && e.settled() {
                    self.pitch_env = None;
                }
                let y = f64::from(lvl * self.pitch_env_cents) * (std::f64::consts::LN_2 / 1200.0);
                1.0 + y + 0.5 * y * y
            }
            None => 1.0,
        };
        let step = self.bend
            * vib
            * penv
            * if self.glide_frames > 0 {
                let s = self.rate * 2f64.powf(self.glide_cents as f64 / 1200.0);
                self.glide_cents += self.glide_step;
                self.glide_frames -= 1;
                if self.glide_frames == 0 {
                    // The glide's end pitch holds (an outgoing voice stays
                    // bent toward the new note until it is cut).
                    self.rate *= 2f64.powf(f64::from(self.glide_end) / 1200.0);
                    self.glide_cents = 0.0;
                    self.glide_end = 0.0;
                }
                s
            } else {
                self.rate
            };
        let step = match self.stretch.as_mut() {
            Some(st) => {
                for h in &mut st.heads {
                    *h += step;
                }
                st.grain += 1;
                step * st.speed
            }
            None => step,
        };
        if let Some(t) = self.tm2.as_deref_mut() {
            // The machine's virtual position, frame by frame.
            t.step = step;
            t.used += 1;
            self.position = t.vstart + t.used as f64 * f64::from(t.per);
        } else if let Some(kt) = self.kt.as_mut() {
            // Kontakt's cursor: the next planned read, folded into the loop.
            kt.step = step;
            kt.used += 1;
            let next = kt.taps.get(kt.used).filter(|_| kt.used < kt.len);
            let (at, frac) = next.map_or((kt.cursor.pos, kt.cursor.frac), |t| (t.at, t.frac));
            self.position = kt.layout.map(at) as f64 + f64::from(frac);
        } else if self.reverse {
            self.position -= step;
        } else {
            self.position += step;
        }
        // Keep looping while Releasing too — a looped body (sustain/legato) must
        // fade out over its loop, not stop looping and run forward into the
        // sample's abrupt end (which clicks → "note-off noise"). Only a Done
        // voice ignores the loop.
        if self.kt.is_none()
            && self.tm2.is_none()
            && matches!(
                self.state,
                VoiceState::Playing | VoiceState::Releasing { .. }
            )
        {
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
        let mut i = 0;
        while i < num_frames {
            // The common case — a plain sample playing or fading, nothing
            // else moving — runs in `render_fast`, many frames at a time;
            // anything else (and the frame that ends a run) goes through
            // `next_frame`.
            if FAST_STATS.load(std::sync::atomic::Ordering::Relaxed) {
                fast_stats(self);
            }
            if FAST_PATH.load(std::sync::atomic::Ordering::Relaxed) && self.fast_ok() {
                let n = self.render_fast(&mut output[i * 2..num_frames * 2], &mut block_peak);
                i += n;
                rendered += n;
                if i >= num_frames {
                    break;
                }
            }
            let (l, r) = self.next_frame();
            output[i * 2] += l;
            output[i * 2 + 1] += r;
            block_peak = block_peak.max(l.abs()).max(r.abs());
            rendered += 1;
            i += 1;
            if self.is_done() {
                break;
            }
        }
        self.update_decay_retire(block_peak, rendered);
        rendered
    }

    /// Nothing but the sample, its gain (and its ramp) and its envelopes (the
    /// fade, the flex envelope, the breakpoint envelope) is moving: no hold,
    /// attack delay, bloom, decay, pitch shift, filter, vibrato, glide,
    /// reverse or ping-pong loop, no marker to emit. Then
    /// [`render_fast`](Self::render_fast) plays it.
    #[inline]
    fn fast_ok(&self) -> bool {
        (matches!(self.state, VoiceState::Playing)
            || matches!(self.state, VoiceState::Releasing { frames_remaining } if frames_remaining > 0))
            && self.start_hold == 0
            && self.release_hold == 0
            && self.attack_delay == 0
            && self.bloom_frames == 0
            && self.decay_left == 0
            && self.pitch.is_none()
            && self.stretch.is_none()
            && self.kt.is_none()
            && self.tm2.is_none()
            && self.filter.is_none()
            && self.inserts.is_none()
            && self.release_cut == 0
            && self.vib_cents <= 0.0
            && self.glide_frames == 0
            && self.pitch_env.is_none()
            && !self.reverse
            && !(self.alternating_loop && self.loop_range.is_some())
            && (self.marker_arrival_file.is_none() || self.arrival_emitted.is_some())
    }

    /// Play frames while [`fast_ok`](Self::fast_ok) holds, reading straight
    /// from the run of samples that holds them — `next_frame`'s arithmetic,
    /// in its order, without its per-frame branches and per-frame sample
    /// lookups. Adds into `out` (interleaved), folds the output peak into
    /// `peak`, returns the frames played; it stops before any frame
    /// `next_frame` has to decide (the end of the sample, the end of the run,
    /// the loop seam, the end of the fade).
    fn render_fast(&mut self, out: &mut [f32], peak: &mut f32) -> usize {
        let frames = out.len() / 2;
        let idx0 = self.position as usize;
        if idx0 >= self.end_frame || idx0 < self.start_frame {
            return 0;
        }
        let ch = self.data.channels.max(1) as usize;
        let last = self.end_frame.saturating_sub(1);
        // `next_frame`'s constants for this run.
        let bloom = if self.bloom_total > 0 {
            let t = 1.0 - self.bloom_frames as f32 / self.bloom_total as f32;
            1.0 + (self.bloom_target - 1.0) * t
        } else {
            1.0
        };
        let step = self.bend * 1.0f64 * self.rate;
        let seam = match (self.loop_range, self.loop_xfade > 0) {
            (Some((_, loop_end)), true) => loop_end as f64 - self.loop_xfade as f64,
            _ => f64::INFINITY,
        };
        // Copied out: `span` below borrows the voice mutably for the loop.
        let (win_start, loop_lo) = (self.start_frame, self.loop_range.map(|(ls, _)| ls));
        let below = move |idx: usize| idx < win_start && !loop_lo.is_some_and(|ls| idx >= ls);
        let Some((span, base)) = self.data.span_cursored(idx0, &mut self.stream_cursor) else {
            return 0;
        };
        let span_end = base + span.len() / ch;
        let (mut position, mut state) = (self.position, self.state.clone());
        let mut n = 0;
        let mut furthest = 0usize;
        while n < frames {
            let idx = position as usize;
            // Frames `next_frame` decides: past the sample, at its last
            // frame, out of this run, at the loop seam, the fade's end.
            if idx >= self.end_frame || below(idx) || idx + 1 > last {
                break;
            }
            if idx < base || idx + 1 >= span_end || position >= seam {
                break;
            }
            // The gain ramp (attack), as `next_frame` steps it.
            if self.gain_ramp_frames > 0 {
                self.gain += (self.target_gain - self.gain) / self.gain_ramp_frames as f32;
                self.gain_ramp_frames -= 1;
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
            let env = match &mut state {
                VoiceState::Playing => 1.0f32,
                VoiceState::Releasing { frames_remaining } => {
                    if *frames_remaining == 0 {
                        break;
                    }
                    let t = *frames_remaining as f32 / self.release_frames.max(1) as f32;
                    *frames_remaining -= 1;
                    let k = self.release_k_active;
                    if k < 1e-4 {
                        t
                    } else {
                        let x = 1.0 - t;
                        ((-k * x).exp() - (-k).exp()) / (1.0 - (-k).exp())
                    }
                }
                VoiceState::Done => break,
            };
            let frac = (position - idx as f64) as f32;
            let (a, b) = ((idx - base) * ch, (idx + 1 - base) * ch);
            let ((l0, r0), (l1, r1)) = if ch >= 2 {
                (
                    (span.get(a), span.get(a + 1)),
                    (span.get(b), span.get(b + 1)),
                )
            } else {
                let (x, y) = (span.get(a), span.get(b));
                ((x, x), (y, y))
            };
            furthest = b;
            let (l, r) = (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac);
            // The envelopes, as `next_frame` steps them.
            let released = Self::envelopes_released(&state, self.key_up, self.script_faded);
            let flex = match &mut self.flex {
                Some(f) => f.next(released),
                None => 1.0,
            } * match &mut self.flex2 {
                Some(f) => f.next(released),
                None => 1.0,
            };
            let (env, bp) = match &mut self.amp_bp {
                Some((bpts, player, dt)) => {
                    let (a, b, i, primed) = &mut self.bp_ctrl;
                    let k = ENV_CONTROL_FRAMES as f32;
                    if !*primed {
                        *a = player.tick(bpts, *dt * k);
                        *b = player.tick(bpts, *dt * k);
                        *i = 0;
                        *primed = true;
                    } else if *i >= ENV_CONTROL_FRAMES {
                        *a = *b;
                        *b = player.tick(bpts, *dt * k);
                        *i = 0;
                    }
                    let v = *a + (*b - *a) * (*i as f32 / k);
                    *i += 1;
                    (1.0, v)
                }
                None => (env, 1.0),
            };
            let amp = self.gain * env * flex * bloom * self.ds_level * bp;
            position += step;
            if let Some((loop_start, loop_end)) = self.loop_range {
                if position >= loop_end as f64 {
                    let len = (loop_end - loop_start) as f64;
                    position = loop_start as f64 + (position - loop_end as f64) % len;
                }
            }
            let (ol, or) = (
                flush_denormal(l * amp * self.pan_l),
                flush_denormal(r * amp * self.pan_r),
            );
            out[n * 2] += ol;
            out[n * 2 + 1] += or;
            *peak = peak.max(ol.abs()).max(or.abs());
            n += 1;
        }
        self.position = position;
        self.state = state;
        self.frames_out += n as u64;
        // The read-ahead the per-frame reads would have issued.
        if n > 0 {
            self.stream_cursor.touch(base * ch + furthest);
        }
        n
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
        self.max_peak = self.max_peak.max(self.env_peak);
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
/// Count why voices miss the fast path ([`FAST_MISS`]) — diagnostics.
pub static FAST_STATS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Voice-blocks missing the fast path, per reason: not sounding, start hold,
/// release hold, attack delay, gain ramp, bloom, decay, pitch shift,
/// filter, flex envelope, breakpoint envelope, vibrato, glide, reverse,
/// ping-pong loop, marker.
pub static FAST_MISS: [std::sync::atomic::AtomicU64; 16] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 16];

fn fast_stats(v: &Voice) {
    use std::sync::atomic::Ordering::Relaxed;
    let reasons = [
        !(matches!(v.state, VoiceState::Playing)
            || matches!(v.state, VoiceState::Releasing { frames_remaining } if frames_remaining > 0)),
        v.start_hold != 0,
        v.release_hold != 0,
        v.attack_delay != 0,
        false,
        v.bloom_frames != 0,
        v.decay_left != 0,
        v.pitch.is_some(),
        v.filter.is_some(),
        false,
        false,
        v.vib_cents > 0.0,
        v.glide_frames != 0,
        v.reverse,
        v.alternating_loop && v.loop_range.is_some(),
        !(v.marker_arrival_file.is_none() || v.arrival_emitted.is_some()),
    ];
    for (i, r) in reasons.iter().enumerate() {
        if *r {
            FAST_MISS[i].fetch_add(1, Relaxed);
        }
    }
}

/// The voices' fast path on (the default) or off — off only to check it
/// against the per-frame path (`set_fast_path`).
static FAST_PATH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Play voices through their per-frame path only (`false`), for comparing it
/// with the fast path; `true` restores the fast path.
pub fn set_fast_path(on: bool) {
    FAST_PATH.store(on, std::sync::atomic::Ordering::Relaxed);
}

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

/// Held notes the CPU guard never cuts below, per voice pool — a two-handed
/// chord with the pedal down stays whole.
const SHED_NOTE_FLOOR: usize = 16;

/// Frames per control-rate step of a voice's breakpoint envelope (a third of
/// a millisecond at 48 kHz — well under anything an envelope does audibly).
const ENV_CONTROL_FRAMES: u32 = 16;
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
    /// The current strike, stamped on every voice spawned (see
    /// [`begin_strike`](Self::begin_strike)).
    strike: u32,
    /// Scratch for [`make_room_for_note`](Self::make_room_for_note): the
    /// re-struck key's distinct strikes. Pre-allocated so a note-on never
    /// allocates on the audio thread (`tests/rt_safety.rs`); a key's strikes
    /// cannot outnumber the voices, so `MAX_VOICES` holds them.
    room_strikes: Vec<u32>,
    /// Scratch for [`make_room_for_note`](Self::make_room_for_note): the
    /// sounding keys and whether each is all released. At most one entry per
    /// MIDI key, so 128 is the bound.
    room_notes: Vec<(u8, bool)>,
    /// How spawned voices read between frames (the library's
    /// `performance.interpolation`).
    interp: super::resample::Interp,
    /// The engine frame the next rendered frame is (and a voice spawned now
    /// starts at): the grid Kontakt's resampler plans its blocks on.
    now: u64,
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
            strike: 0,
            room_strikes: Vec::with_capacity(MAX_VOICES),
            room_notes: Vec::with_capacity(128),
            interp: super::resample::Interp::Linear,
            now: 0,
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
            strike: 0,
            room_strikes: Vec::with_capacity(MAX_VOICES),
            room_notes: Vec::with_capacity(128),
            interp: super::resample::Interp::Linear,
            now: 0,
        }
    }

    /// How voices spawned from now on read between frames.
    pub fn set_interp(&mut self, interp: super::resample::Interp) {
        self.interp = interp;
    }

    /// The engine frame the next rendered frame is.
    pub fn set_now(&mut self, now: u64) {
        self.now = now;
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

    /// A new strike begins: the voices spawned from here on (every mic and
    /// layer sample of one note-on, or the release samples of one note-off)
    /// count as one note to a polyphony limit.
    pub fn begin_strike(&mut self) {
        self.strike = self.strike.wrapping_add(1);
    }

    /// The strike the voices spawned from here on carry (see
    /// [`begin_strike`](Self::begin_strike)).
    #[must_use]
    pub fn current_strike(&self) -> u32 {
        self.strike
    }

    /// Fade every still-playing voice of `strike` to silence over
    /// `fade_frames` (a phrase hit fading the key's previous hit only).
    pub fn fade_strike(&mut self, strike: u32, fade_frames: usize) {
        for v in &mut self.voices {
            if v.strike == strike && matches!(v.state, VoiceState::Playing) {
                v.ramp_gain(0.0, fade_frames);
                v.state = VoiceState::Releasing {
                    frames_remaining: fade_frames,
                };
            }
        }
    }

    pub fn spawn(&mut self, voice: Voice) {
        let mut voice = voice;
        // New voices sound at the current wheel position.
        voice.bend = self.bend;
        voice.strike = self.strike;
        if let Some(t) = voice.tm2.as_deref_mut() {
            // A Time Machine II voice reads through its own machine.
            t.origin = self.now;
        } else if self.interp == super::resample::Interp::KontaktNormal {
            voice.kt = Some(KontaktVoice::new(self.now));
        }
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
    /// How far `note`'s body has decayed from its onset: its current peak
    /// over the loudest it reached (0‥1; 1 when nothing has sounded yet).
    pub fn note_body_ratio(&self, note: u8) -> f32 {
        let (now, max) = self
            .voices
            .iter()
            .filter(|v| {
                v.note == note && v.kind != VoiceKind::Release && v.state != VoiceState::Done
            })
            .fold((0.0f32, 0.0f32), |(n, m), v| {
                (n + v.env_peak, m + v.max_peak)
            });
        if max <= 1e-9 {
            1.0
        } else {
            (now / max).clamp(0.0, 1.0)
        }
    }

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
    /// [`Voice::cut_release`] on `line`'s voices of `note`.
    pub fn cut_release_line(&mut self, line: u8, note: u8, frames: usize) {
        for v in &mut self.voices {
            if v.note == note && v.line == line {
                v.cut_release(frames);
            }
        }
    }

    pub fn note_off_line(&mut self, line: u8, note: u8, release_frames: Option<usize>) {
        for v in &mut self.voices {
            if v.env_key == note && v.line == line {
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

    /// A script's `fade_out` of every voice `note` sounds on `line` except
    /// release voices: linear from the current level over `frames`.
    pub fn fade_out_note_bodies(&mut self, line: u8, note: u8, frames: usize) {
        for v in &mut self.voices {
            if v.note != note || v.line != line || matches!(v.kind, VoiceKind::Release) {
                continue;
            }
            Self::script_fade(v, frames.max(1));
        }
    }

    /// The key-up of `note` on `line` as the engine's envelopes take it:
    /// every non-release voice's flex envelope enters its release (a voice
    /// a script no longer tracks still ends with its envelope).
    pub fn release_note_envelopes(&mut self, line: u8, note: u8) {
        for v in &mut self.voices {
            if v.env_key == note && v.line == line && !matches!(v.kind, VoiceKind::Release) {
                if let Some(f) = &mut v.flex {
                    f.release();
                }
            }
        }
    }

    /// A script's `fade_out` of every voice on `line` but the release
    /// samples (a monophonic legato's `MARK_1 + MARK_2 + MARK_7`), linear
    /// from the current level over `frames`.
    pub fn fade_out_line_bodies(&mut self, line: u8, frames: usize) {
        for v in &mut self.voices {
            // a released note still ringing is faded too (the script
            // fades by marks, key up or down); one already fading by a
            // script keeps its fade
            if v.line == line && !matches!(v.kind, VoiceKind::Release) && !v.script_faded {
                Self::script_fade(v, frames.max(1));
            }
        }
    }

    /// A script's `fade_out($ALL_EVENTS)`: every voice not already fading
    /// by a script, linear from its current level over `frames`.
    pub fn fade_out_all(&mut self, frames: usize) {
        for v in &mut self.voices {
            if !v.script_faded {
                Self::script_fade(v, frames.max(1));
            }
        }
    }

    /// A script's `fade_out` of the voices of one strike (its note-on), from
    /// their current level, linear over `frames`.
    pub fn fade_out_strike_bodies(&mut self, strike: u32, frames: usize) {
        for v in &mut self.voices {
            if v.strike == strike && !matches!(v.kind, VoiceKind::Release) {
                Self::script_fade(v, frames.max(1));
            }
        }
    }

    /// A script's `fade_out` of every sounding release voice (MOJO 2's
    /// `by_marks($MARK_3)`): linear from the current level over `frames`.
    /// A voice already in a script's fade (a release sample's tail) is taken
    /// over from where that fade is, as Kontakt's later fade takes over
    /// (`ScriptFadeOut` 0x3d6aac); one released by its key keeps its release.
    pub fn fade_out_releases(&mut self, frames: usize) {
        for v in &mut self.voices {
            if !matches!(v.kind, VoiceKind::Release) {
                continue;
            }
            match v.state {
                VoiceState::Playing => Self::script_fade(v, frames.max(1)),
                VoiceState::Releasing { frames_remaining } if v.script_faded => {
                    // the running fade's level becomes the voice's gain
                    let level = frames_remaining as f32 / v.release_frames.max(1) as f32;
                    v.gain *= level;
                    Self::script_fade(v, frames.max(1));
                }
                _ => {}
            }
        }
    }

    fn script_fade(v: &mut Voice, fade: usize) {
        v.script_faded = true;
        v.target_gain = v.gain;
        v.gain_ramp_frames = 0;
        v.stage1_run = 0;
        v.stage2_frames = 0;
        v.release_frames = fade;
        v.release_k_active = 0.0;
        v.state = VoiceState::Releasing {
            frames_remaining: fade,
        };
    }

    /// Kontakt `fade_out`: a LINEAR fade to silence over `trans_fade` frames
    /// for `note`'s transition voices on `line` and `sus_fade` for its body
    /// layers — no gain ramp on top (which made `retire_note_line` a
    /// (1 − t)² curve).
    /// Kontakt `fade_out` on every voice of `note` on `line`, whatever its
    /// kind: a linear fade over `frames` from the voice's current level.
    /// Release the voices tagged `tag` (`Voice::script_tag`): KSP
    /// `note_off` on a `play_note` id.
    pub fn release_tagged(&mut self, tag: u32, release_frames: Option<usize>) {
        for v in &mut self.voices {
            if tag != 0 && v.script_tag == tag {
                match release_frames {
                    Some(frames) => v.note_off_with_release_frames(frames),
                    None => v.note_off(),
                }
            }
        }
    }

    /// Kontakt `fade_out` on the voices tagged `tag`: a linear fade over
    /// `frames` (≤ 1: at once) from the current level.
    pub fn fade_out_tagged(&mut self, tag: u32, frames: usize) {
        for v in &mut self.voices {
            if tag == 0 || v.script_tag != tag {
                continue;
            }
            v.target_gain = v.gain;
            v.gain_ramp_frames = 0;
            v.stage1_run = 0;
            v.stage2_frames = 0;
            v.release_frames = frames.max(1);
            v.release_k_active = 0.0;
            v.state = VoiceState::Releasing {
                frames_remaining: frames.max(1),
            };
        }
    }

    pub fn fade_out_note_line_any(&mut self, line: u8, note: u8, frames: usize) {
        for v in &mut self.voices {
            if v.note != note || v.line != line {
                continue;
            }
            v.target_gain = v.gain;
            v.gain_ramp_frames = 0;
            v.stage1_run = 0;
            v.stage2_frames = 0;
            v.release_frames = frames.max(1);
            v.release_k_active = 0.0;
            v.state = VoiceState::Releasing {
                frames_remaining: frames.max(1),
            };
        }
    }

    pub fn fade_out_note_line(&mut self, line: u8, note: u8, trans_fade: usize, sus_fade: usize) {
        for v in &mut self.voices {
            if v.note != note || v.line != line {
                continue;
            }
            let fade = match v.kind {
                VoiceKind::Legato => trans_fade,
                // `sus_fade == 0`: leave the bodies as they are.
                VoiceKind::SustainLayer
                | VoiceKind::SustainNVLo
                | VoiceKind::SustainNVHi
                | VoiceKind::SustainVibLo
                | VoiceKind::SustainVibHi
                | VoiceKind::SustainLo
                | VoiceKind::SustainHi
                    if sus_fade > 0 =>
                {
                    sus_fade
                }
                _ => continue,
            }
            .max(1);
            // Kontakt's fade_out starts from the voice's CURRENT level and
            // replaces any fade-in still running (null test: a zone-3 first
            // note mid fade-in, ×0.23, fades linearly to 0 from there). Freeze
            // the gain ramp and any staged fade-in where they are.
            v.target_gain = v.gain;
            v.gain_ramp_frames = 0;
            v.stage1_run = 0;
            v.stage2_frames = 0;
            v.release_frames = fade;
            v.release_k_active = 0.0;
            v.state = VoiceState::Releasing {
                frames_remaining: fade,
            };
        }
    }

    /// Retire a CSS marcato transition's outgoing pair on `line`: the body
    /// fades linearly over `body_fade` (and is cut there), bends 0 → `bend`
    /// cents over `glide` frames and takes `sink` as its second envelope; the
    /// previous transition voice fades over `lt_fade`. Each fade starts from
    /// the voice's current level (see [`Self::fade_out_note_line`]).
    pub fn marcato_retire(
        &mut self,
        line: u8,
        note: u8,
        body_fade: usize,
        lt_fade: usize,
        bend: f32,
        glide: usize,
        sink: Option<&FlexEnv>,
    ) {
        for v in &mut self.voices {
            if v.note != note || v.line != line || v.is_done() {
                continue;
            }
            let body = matches!(
                v.kind,
                VoiceKind::SustainLayer
                    | VoiceKind::SustainNVLo
                    | VoiceKind::SustainNVHi
                    | VoiceKind::SustainVibLo
                    | VoiceKind::SustainVibHi
                    | VoiceKind::SustainLo
                    | VoiceKind::SustainHi
            );
            let fade = match v.kind {
                VoiceKind::Legato => lt_fade,
                _ if body => body_fade,
                _ => continue,
            }
            .max(1);
            v.target_gain = v.gain;
            v.gain_ramp_frames = 0;
            v.stage1_run = 0;
            v.stage2_frames = 0;
            v.release_frames = fade;
            v.release_k_active = 0.0;
            v.state = VoiceState::Releasing {
                frames_remaining: fade,
            };
            if body {
                v.set_pitch_glide(0.0, bend, glide);
                if let Some(e) = sink {
                    v.set_flex_env2(e.clone());
                }
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
        let frames = outputs.first().map_or(0, |b| b.len() / 2);
        self.render_multi_range(outputs, 0, frames);
    }

    /// [`render_multi`](Self::render_multi) into frames `start..start + frames`
    /// of every buffer — so the engine can split a block at a sample-exact
    /// event (a legato transition firing mid-block).
    pub fn render_multi_range(&mut self, outputs: &mut [Vec<f32>], start: usize, frames: usize) {
        if outputs.is_empty() {
            return;
        }
        let nmics = outputs.len();
        let (a, b) = (start * 2, (start + frames) * 2);
        let len = b - a;
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
            v.render_block(&mut outputs[idx][a..b]);
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
        let frames = outputs.first().map_or(0, |b| b.len() / 2);
        self.render_matrix_range(outputs, nmics, route, 0, frames);
    }

    /// [`render_matrix`](Self::render_matrix) into frames `start..start + frames`.
    pub fn render_matrix_range(
        &mut self,
        outputs: &mut [Vec<f32>],
        nmics: usize,
        route: [usize; 2],
        start: usize,
        frames: usize,
    ) {
        if outputs.is_empty() || nmics == 0 {
            return;
        }
        let nbuses = outputs.len() / nmics;
        let (a, b) = (start * 2, (start + frames) * 2);
        let len = b - a;
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
            v.render_block(&mut outputs[bus * nmics + mic][a..b]);
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

    /// Make room for a new strike of `incoming`.
    ///
    /// **Other keys**, under a limit of `max_notes` sounding keys (a patch's
    /// voice count counts notes — every mic, layer sample and release of a
    /// key is one of them): while that many other keys are sounding, fade out
    /// every voice of the stalest — a key already released first, else the
    /// oldest held. Returns keys stolen.
    ///
    /// **The same key**, re-struck: its strike before this one rings on (the
    /// overlap a player hears), anything older fades over
    /// [`REPEAT_FADE`] — the new strike takes the string over, as on the
    /// instrument. Without it, repeats of one note under the sustain pedal
    /// piled up with no limit (a Dolceola lane reached 232 voices in a plain
    /// worship passage). Counting each strike against the note limit instead
    /// held the voices down but stole the chord under a pedalled melody.
    pub fn make_room_for_note(&mut self, incoming: u8, max_notes: usize) -> usize {
        self.make_room_for_note_with(incoming, max_notes, MAX_REPEAT_STRIKES)
    }

    /// [`make_room_for_note`](Self::make_room_for_note) keeping
    /// `repeat_strikes` strikes of the same key (0 = every strike rings on,
    /// as Kontakt plays a library like CSS).
    pub fn make_room_for_note_with(
        &mut self,
        incoming: u8,
        max_notes: usize,
        repeat_strikes: usize,
    ) -> usize {
        const STEAL_NOTE_FADE: usize = 240; // 5 ms: fast, not a click
        // A voice with no more than a repeat fade left — a steal, a repeat,
        // a release nearly over — no longer counts.
        let fading = |v: &Voice| matches!(v.state, VoiceState::Releasing { frames_remaining } if frames_remaining <= REPEAT_FADE);
        // The same key: keep the latest strike, fade the older ones.
        let strikes = &mut self.room_strikes;
        strikes.clear();
        for v in &self.voices {
            if v.note == incoming
                && v.state != VoiceState::Done
                && !fading(v)
                && !strikes.contains(&v.strike)
            {
                strikes.push(v.strike);
            }
        }
        if repeat_strikes > 0 && strikes.len() >= repeat_strikes {
            let keep = &strikes[strikes.len() + 1 - repeat_strikes..];
            for v in self.voices.iter_mut().filter(|v| {
                v.note == incoming && v.state != VoiceState::Done && !keep.contains(&v.strike)
            }) {
                if !fading(v) {
                    v.ramp_gain(0.0, REPEAT_FADE);
                    v.state = VoiceState::Releasing {
                        frames_remaining: REPEAT_FADE,
                    };
                }
            }
        }
        // Other keys, under the note limit.
        let mut stolen = 0;
        loop {
            // Sounding keys, in first-voice order, with whether all their
            // voices are already releasing.
            let notes = &mut self.room_notes;
            notes.clear();
            for v in &self.voices {
                if v.state == VoiceState::Done || v.note == incoming || fading(v) {
                    continue;
                }
                let released =
                    matches!(v.state, VoiceState::Releasing { .. }) || v.kind == VoiceKind::Release;
                match notes.iter_mut().find(|(n, _)| *n == v.note) {
                    Some(e) => e.1 &= released,
                    None => notes.push((v.note, released)),
                }
            }
            if notes.len() < max_notes.max(1) {
                return stolen;
            }
            let victim = notes
                .iter()
                .find(|(_, released)| *released)
                .or_else(|| notes.first())
                .map(|(n, _)| *n);
            let Some(victim) = victim else {
                return stolen;
            };
            for v in self
                .voices
                .iter_mut()
                .filter(|v| v.note == victim && v.state != VoiceState::Done)
            {
                v.ramp_gain(0.0, STEAL_NOTE_FADE);
                v.state = VoiceState::Releasing {
                    frames_remaining: STEAL_NOTE_FADE,
                };
            }
            self.stolen = self.stolen.saturating_add(1);
            NOTE_STEALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            crate::lane_health::stolen();
            stolen += 1;
        }
    }

    /// Shed load under CPU pressure (the keys rig's overload guard).
    ///
    /// `level` 1: fade the OLD release tails — voices past the middle of
    /// their release, most of their level already gone. A note just let go
    /// of keeps ringing: that is the one a player hears end early (a
    /// Dolceola note rings for a second after key-up). `level` 2, a real
    /// overload: every releasing voice and release sample, and the oldest
    /// held note of a pile-up. Fades are quick but not clicks. Every voice
    /// faded is counted to the lane (`lane_health`).
    pub fn shed(&mut self, level: u8) {
        const SHED_FADE: usize = 1440; // 30 ms at 48 kHz
        let mut held: Vec<usize> = Vec::new();
        let mut faded = 0usize;
        for (i, v) in self.voices.iter_mut().enumerate() {
            match v.state {
                VoiceState::Done => {}
                VoiceState::Releasing { frames_remaining } => {
                    let old = frames_remaining < v.release_frames / 2;
                    if frames_remaining > SHED_FADE && (level >= 2 || old) {
                        v.ramp_gain(0.0, SHED_FADE);
                        v.state = VoiceState::Releasing {
                            frames_remaining: SHED_FADE,
                        };
                        faded += 1;
                    }
                }
                _ if v.kind == VoiceKind::Release => {
                    if level >= 2 {
                        v.ramp_gain(0.0, SHED_FADE);
                        v.state = VoiceState::Releasing {
                            frames_remaining: SHED_FADE,
                        };
                        faded += 1;
                    }
                }
                _ => held.push(i),
            }
        }
        crate::lane_health::faded(faded);
        if level >= 2 {
            // One note — the oldest held — and only while the lane holds more
            // than a big chord's worth: the guard trims a runaway pile-up, it
            // never thins a chord. Spawn order is pool order.
            let mut notes: Vec<u8> = Vec::new();
            for &i in &held {
                let n = self.voices[i].note;
                if !notes.contains(&n) {
                    notes.push(n);
                }
            }
            if notes.len() > SHED_NOTE_FLOOR {
                let victim = notes[0];
                let mut cut = 0usize;
                for &i in &held {
                    let v = &mut self.voices[i];
                    if v.note == victim {
                        v.ramp_gain(0.0, SHED_FADE);
                        v.state = VoiceState::Releasing {
                            frames_remaining: SHED_FADE,
                        };
                        cut += 1;
                    }
                }
                crate::lane_health::faded(cut);
            }
        }
        self.stolen = self.stolen.saturating_add(1);
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
            target: "sampler::trigger",
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

/// Strikes of one key that ring at once: the new one and the one before.
pub const MAX_REPEAT_STRIKES: usize = 2;
/// How long an older strike of a re-struck key takes to fade (100 ms at
/// 48 kHz) — the new strike taking the string over, not a cut.
pub const REPEAT_FADE: usize = 4800;

/// Notes stolen at a polyphony limit, process-wide (every sampler pool and
/// wavetable oscillator) — for a rig's health log: a note cut short by a
/// voice limit sounds like a note cut short by anything else.
pub static NOTE_STEALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many notes polyphony limits have stolen so far (process-wide).
#[must_use]
pub fn note_steals() -> u64 {
    NOTE_STEALS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod stretch_tests {
    use super::*;

    fn ramp(n: usize) -> Arc<SampleData> {
        let pcm: Vec<f32> = (0..n).map(|i| i as f32 / 65_536.0).collect();
        Arc::new(SampleData::from_f32(pcm, 1, 48_000, n))
    }

    /// Kontakt's Normal quality plays a ramp along its line (a copy at
    /// unity, the Lagrange at another step), across cache seams and blocks,
    /// and ends when the sample does (KONTRA's
    /// `kontakts_resampler_reads_a_ramp_along_its_line_at_every_quality`).
    #[test]
    fn kontakts_resampler_reads_a_ramp_along_its_line() {
        for step in [1.0, 1.25, 0.8, 0.943_874_3] {
            for origin in [0u64, 77] {
                let mut v = Voice::with_rate(ramp(4096), 60, VoiceKind::Short, step, 1.0, 64);
                v.kt = Some(KontaktVoice::new(origin));
                let out: Vec<f32> = (0..2000).map(|_| v.next_frame().0).collect();
                for (n, x) in out.iter().enumerate().skip(4).take(1500) {
                    let want = n as f64 * step / 65_536.0;
                    let tol = if step == 1.0 { 0.0 } else { 2e-4 * want + 1e-7 };
                    assert!(
                        (f64::from(*x) - want).abs() <= tol,
                        "step {step} origin {origin} frame {n}: {x} vs {want}"
                    );
                }
                for _ in 0..(4096.0 / step) as usize {
                    v.next_frame();
                }
                assert!(v.is_done(), "step {step}: ends with its sample");
            }
        }
    }

    /// A looped voice keeps reading the loop through Kontakt's path.
    #[test]
    fn a_kontakt_voice_loops() {
        let mut v = Voice::with_rate(ramp(4096), 60, VoiceKind::Short, 1.5, 1.0, 64)
            .with_forward_loop(1000, 3000);
        v.kt = Some(KontaktVoice::new(0));
        for i in 0..20_000 {
            // The Lagrange about the fold reads both sides of the jump, so
            // it may overshoot the loop's end a little (as Kontakt does).
            let (l, _) = v.next_frame();
            assert!(
                i < 2000 || (800.0 / 65_536.0..3300.0 / 65_536.0).contains(&l),
                "{l}"
            );
        }
        assert!(!v.is_done());
        let x = v.position;
        assert!((1000.0..3000.0).contains(&x), "{x}");
    }

    fn marks(frames: i64, every: i64) -> super::super::tm2::Marks {
        super::super::tm2::Marks {
            frames,
            at: (0..frames / every)
                .map(|i| i * every)
                .collect::<Vec<_>>()
                .into(),
        }
    }

    /// A Time Machine II voice at unity speed and pitch walks its sample
    /// frame by frame (the grains tile it) from the mark nearest its start
    /// (grains start on marks: 1000 plays from 800), and ends with its
    /// grains; the pool gives it no other resampler.
    #[test]
    fn a_time_machine_2_voice_at_unity_plays_the_sample() {
        let mut pool = VoicePool::new();
        pool.set_interp(super::super::resample::Interp::KontaktNormal);
        pool.set_now(77);
        pool.spawn(
            Voice::with_rate(ramp(8192), 60, VoiceKind::Short, 1.0, 1.0, 64)
                .with_sample_window(1000, None)
                .with_tm2(marks(8192, 400), 1.0, 1.0),
        );
        let v = &mut pool.voices_mut()[0];
        assert!(v.kt.is_none());
        for n in 0..6000 {
            let (l, _) = v.next_frame();
            // The fading grain reads the same frames as the new one here, so
            // the crossfade sums to the frame (to rounding).
            let want = (800 + n) as f32 / 65_536.0;
            assert!((l - want).abs() <= want * 1e-6, "frame {n}: {l} vs {want}");
        }
        for _ in 0..2000 {
            v.next_frame();
        }
        assert!(v.is_done());
    }

    /// At half speed a Time Machine II voice keeps its pitch (each frame one
    /// step on from the last, but at the grain seams) and lasts twice as long.
    #[test]
    fn a_time_machine_2_voice_at_half_speed_keeps_its_pitch() {
        let mut v = Voice::with_rate(ramp(8192), 60, VoiceKind::Short, 1.0, 1.0, 64).with_tm2(
            marks(8192, 400),
            0.5,
            1.0,
        );
        let out: Vec<f32> = (0..15_000).map(|_| v.next_frame().0).collect();
        assert!(
            !v.is_done(),
            "still playing at twice its length less a grain"
        );
        let steady = out
            .windows(2)
            .filter(|w| ((w[1] - w[0]) * 65_536.0 - 1.0).abs() < 1e-3)
            .count();
        assert!(steady > 7000, "{steady}");
        for _ in 0..2000 {
            v.next_frame();
        }
        assert!(v.is_done());
    }

    /// A script's fade is not a key-up: the voice's held envelope stays at
    /// its hold while the fade runs (Kontakt's `fade_out` ramps the level
    /// alone), so the fade is linear; the key-up then releases it.
    #[test]
    fn a_script_fade_leaves_the_envelope_held() {
        let ones = Arc::new(SampleData::from_f32(vec![0.5; 48_000], 1, 48_000, 48_000));
        let env = FlexEnv::from_segments(&[(1.0, 1.0, 0.5), (100.0, 0.0, 0.5)], 0.0, 48_000, true)
            .map(|e| e.with_release_segment(Some(1)));
        let mut pool = VoicePool::new();
        let mut v = Voice::with_rate(ones, 60, VoiceKind::Short, 1.0, 1.0, 64);
        if let Some(e) = env {
            v = v.with_flex_env(e);
        }
        pool.spawn(v);
        let v = &mut pool.voices_mut()[0];
        for _ in 0..1000 {
            v.next_frame();
        }
        VoicePool::script_fade(v, 1000);
        for k in 0..900 {
            let (l, _) = v.next_frame();
            let want = 0.5 * (1.0 - k as f32 / 1000.0);
            assert!((l - want).abs() < 1e-3, "frame {k}: {l} vs {want}");
        }
    }

    /// A voice without the setting never takes Kontakt's path.
    #[test]
    fn a_default_pool_spawns_linear_voices() {
        let mut pool = VoicePool::new();
        pool.spawn(Voice::with_rate(
            ramp(64),
            60,
            VoiceKind::Short,
            1.25,
            1.0,
            64,
        ));
        assert!(pool.voices_mut()[0].kt.is_none());
        pool.set_interp(super::super::resample::Interp::KontaktNormal);
        pool.spawn(Voice::with_rate(
            ramp(64),
            60,
            VoiceKind::Short,
            1.25,
            1.0,
            64,
        ));
        assert!(pool.voices_mut()[1].kt.is_some());
    }

    /// A stretched voice plays its start as recorded (head 0 alone, at the
    /// pitch's step) and lasts the sample's length over its speed.
    #[test]
    fn a_stretching_voice_keeps_its_pitch_and_takes_its_speeds_time() {
        for (speed, ends) in [(2.0, 4096usize), (0.5, 16_384)] {
            let mut v =
                Voice::new(ramp(8192), 60, VoiceKind::Short, 0, 1.0, 64).with_stretch(speed);
            let first: Vec<f32> = (0..4).map(|_| v.next_frame().0).collect();
            assert_eq!(
                first[1] - first[0],
                1.0 / 65_536.0,
                "speed {speed}: pitch holds"
            );
            for _ in 4..ends - 512 {
                v.next_frame();
            }
            assert!(!v.is_done(), "speed {speed}: still playing before its end");
            for _ in 0..1024 {
                v.next_frame();
            }
            assert!(v.is_done(), "speed {speed}: ended on time");
        }
    }
}
