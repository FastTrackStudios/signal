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
            rate,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            state: VoiceState::Playing,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
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
            rate,
            gain,
            pan_l: 1.0,
            pan_r: 1.0,
            target_gain: gain,
            gain_ramp_frames: 0,
            state: VoiceState::Playing,
            kind,
            note,
            mic_index: None,
            choke_group: None,
            release_frames,
            dyn_layer: None,
            line: 0,
            artic_class: ArticClass::Longs,
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

    /// Start at silence and ramp up to the spawn gain over `frames` — the
    /// attack envelope. `frames == 0` keeps the sample's natural attack.
    pub fn with_attack(mut self, frames: usize) -> Self {
        if frames > 0 {
            self.gain = 0.0; // target_gain stays at the intended spawn gain
            self.gain_ramp_frames = frames;
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

    /// Render one stereo frame. Returns (L, R) and advances internal state.
    #[inline]
    pub fn next_frame(&mut self) -> (f32, f32) {
        if self.state == VoiceState::Done {
            return (0.0, 0.0);
        }

        // Gain smoothing
        if self.gain_ramp_frames > 0 {
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

        let frac = (self.position - frame_idx as f64) as f32;
        let (l0, r0) = self.data.frame(frame_idx);
        let (l1, r1) = self
            .data
            .frame((frame_idx + 1).min(self.end_frame.saturating_sub(1)));

        let l = l0 + (l1 - l0) * frac;
        let r = r0 + (r1 - r0) * frac;

        let amp = self.gain * env;

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
        self.silence_note_filtered(note, fade_frames, None);
    }

    /// Line-scoped variant of [`silence_note`](Self::silence_note): a legato
    /// transition firing on one divisi line must not fade a unison note held
    /// by another line.
    pub fn silence_note_line(&mut self, line: u8, note: u8, fade_frames: usize) {
        self.silence_note_filtered(note, fade_frames, Some(line));
    }

    fn silence_note_filtered(&mut self, note: u8, fade_frames: usize, line: Option<u8>) {
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
                v.ramp_gain(0.0, fade_frames);
                v.state = VoiceState::Releasing {
                    frames_remaining: fade_frames,
                };
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
}
