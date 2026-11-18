//! A script's monophonic legato over a one-layer sustain
//! (`ArticulationSpec::script_legato`, MOJO 2's legato engine; the decode is
//! `_notes/decode-legato.md` beside the library, script_1.ksp line numbers
//! below).
//!
//! The line keeps the script's globals: the last legato-path note
//! (`$fvjr3`), the one before it (`$fy05x`, for a legato back), the frame of
//! the last connection (`$1xho1`, for the speed law) and the natural flag
//! (`$uzpex`). One callback runs at a time (the `%24hey` queue): a note
//! arriving while another waits out its hold aborts that wait and runs a
//! millisecond later, after the aborted callback has finished
//! (`ScriptLegState::deferred`). What a callback waits for — a target, a
//! unison tail, a natural note's end — is `ScriptLegPending`, fired from
//! `render_split` on its exact frame.

use super::*;
use crate::spec::{ScriptLegatoSpec, ScriptLegatoTiming};

/// The line's legato memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScriptLegState {
    /// The last legato-path note, cleared when it is released.
    prev: Option<u8>,
    /// The note before it (the target of a legato back).
    back: Option<u8>,
    /// Frame of the last fresh note or transition.
    last_on: u64,
    /// The last note-on's velocity (a legato back plays at it).
    last_vel: u8,
    /// What the running callback waits for.
    pending: Option<ScriptLegPending>,
    /// The voices of the last target (`MARK_7`).
    target_strike: Option<u32>,
    /// Natural legato, once its key has toggled it (else the shipped state).
    natural: Option<bool>,
    /// A note-on waiting for the queue: `(frame, note, velocity, stray)`,
    /// `stray` the aborted callback's target (see `script_legato_note_on`).
    deferred: Option<(u64, u8, u8, Option<ScriptLegStage>)>,
}

/// A callback's wait.
#[derive(Debug, Clone)]
pub(crate) struct ScriptLegPending {
    due: u64,
    /// The transition's voices.
    strike: u32,
    note: u8,
    velocity: u8,
    stage: ScriptLegStage,
}

/// What follows a wait.
#[derive(Debug, Clone)]
pub(crate) enum ScriptLegStage {
    /// The transition fades, the sustain enters at its offset.
    Target {
        artic: String,
        fade_out: usize,
        target_offset: usize,
    },
    /// Natural octave: the transition fades, the unison tail enters.
    Tail { artic: String, fade_out: usize },
    /// A natural note's (or tail's) end: the release sample, if the key is
    /// still down and the note still the line's last.
    Expire { artic: String },
}

/// The connection's speed in percent and hold in ms, as the script forms
/// them in integers (26064-26100). `m` is the speed index 0 (slow) ..= 127.
pub(crate) fn legato_speed_and_hold(t: &ScriptLegatoTiming, interval: u8, m: i32) -> (i32, i32) {
    let k = i32::from(interval.max(1)) - 1;
    let lo = k * (t.slow_large_pct - t.slow_small_pct) / 11 + t.slow_small_pct;
    let hi = k * (t.fast_large_pct - t.fast_small_pct) / 11 + t.fast_small_pct;
    let speed = (m * (hi - lo) / 127 + lo).max(1);
    let h = k * (t.hold_large_ms - t.hold_small_ms) as i32 / 11 + t.hold_small_ms as i32;
    let f = (100.0 / speed as f32 * 100.0) as i32;
    (speed, h * f / 100)
}

/// The speed index from the time since the last connection (Speed mode):
/// `(ioi − slowest)·127/(fastest − slowest)`, the time clamped to the range.
pub(crate) fn legato_speed_index(t: &ScriptLegatoTiming, ioi_ms: i32) -> i32 {
    let span = t.fastest_ms - t.slowest_ms;
    if span == 0 {
        return 0;
    }
    let ioi = ioi_ms.clamp(
        t.fastest_ms.min(t.slowest_ms),
        t.fastest_ms.max(t.slowest_ms),
    );
    (ioi - t.slowest_ms) * 127 / span
}

impl SampleEngine {
    /// The current articulation's legato, when it is switched on.
    fn script_legato_on(&self) -> Option<ScriptLegatoSpec> {
        let spec = self
            .patch
            .spec
            .articulation(&self.articulation)?
            .script_legato
            .clone()?;
        let cc = usize::from(spec.enable_cc & 127);
        let on = if self.cc_sent[cc] {
            self.cc_values[cc] > 63
        } else {
            spec.on
        };
        on.then_some(spec)
    }

    /// Whether `artic`'s legato is switched on (it plays monophonic).
    pub(crate) fn script_legato_on_for(&self, artic: &str) -> bool {
        let Some(spec) = self
            .patch
            .spec
            .articulation(artic)
            .and_then(|a| a.script_legato.as_ref())
        else {
            return false;
        };
        let cc = usize::from(spec.enable_cc & 127);
        if self.cc_sent[cc] {
            self.cc_values[cc] > 63
        } else {
            spec.on
        }
    }

    fn script_natural(&self, spec: &ScriptLegatoSpec) -> bool {
        spec.natural
            .as_ref()
            .is_some_and(|n| self.script_leg.natural.unwrap_or(n.on))
    }

    /// The natural-legato key (latched): toggles, sounds nothing. `true`
    /// when `note` was it.
    pub(crate) fn script_natural_key(&mut self, note: u8) -> bool {
        let Some(spec) = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.script_legato.clone())
        else {
            return false;
        };
        if spec.natural.as_ref().is_none_or(|n| n.key != note) {
            return false;
        }
        let on = self.script_natural(&spec);
        self.script_leg.natural = Some(!on);
        true
    }

    /// Whether a note `note` after the line's last note is an octave
    /// connection (25595): the pedal down and a leap past `min_leap` (a
    /// line with no last note counts as −1).
    fn script_octave(&self, spec: &ScriptLegatoSpec, prev: Option<u8>, note: u8) -> bool {
        spec.octave.as_ref().is_some_and(|o| {
            self.cc_values[usize::from(o.pedal_cc & 127)] >= 64
                && (i32::from(note) - prev.map_or(-1, i32::from)).abs() > i32::from(o.min_leap)
        })
    }

    /// A note-on of a legato articulation. `false` when its legato is off
    /// (the caller plays the plain short).
    pub(crate) fn script_legato_note_on(&mut self, note: u8, velocity: u8) -> bool {
        let Some(spec) = self.script_legato_on() else {
            return false;
        };
        // Every note fades the sounding release samples first, before it
        // queues (25571).
        self.fade_sustain_releases();
        if let Some((_, n, v, stray)) = self.script_leg.deferred.take() {
            self.script_connect_deferred(&spec, n, v, stray);
        }
        let Some(p) = self.script_leg.pending.take() else {
            self.script_connect(&spec, note, velocity);
            return true;
        };
        // The running callback's wait is aborted; it resumes at the next
        // millisecond with this note's globals: its transition fades and,
        // waiting for a target, it starts one on THIS note, which this
        // note's own connection then fades (decode-legato.md §8.1).
        let stray = match &p.stage {
            ScriptLegStage::Target { fade_out, .. } | ScriptLegStage::Tail { fade_out, .. } => {
                self.voices.fade_out_strike_bodies(p.strike, *fade_out);
                match p.stage {
                    s @ ScriptLegStage::Target { .. } => Some(s),
                    _ => None,
                }
            }
            ScriptLegStage::Expire { .. } => None,
        };
        let due = self.frames_rendered + ms_to_frames_f(1.0, self.sample_rate) as u64;
        self.script_leg.deferred = Some((due, note, velocity, stray));
        true
    }

    fn fade_sustain_releases(&mut self) {
        let fade_rel = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map_or(0.0, |a| a.fade_releases_ms);
        if fade_rel > 0.0 {
            self.voices
                .fade_out_releases(ms_to_frames_f(fade_rel, self.sample_rate));
        }
    }

    /// A queued note's turn: the aborted callback's stray target first.
    fn script_connect_deferred(
        &mut self,
        spec: &ScriptLegatoSpec,
        note: u8,
        velocity: u8,
        stray: Option<ScriptLegStage>,
    ) {
        if let Some(ScriptLegStage::Target {
            artic,
            fade_out,
            target_offset,
        }) = stray
            && self.held_notes.contains_key(&note)
        {
            self.script_target(&artic, note, velocity, fade_out, target_offset);
        }
        self.script_connect(spec, note, velocity);
    }

    /// The note's own work (25570-26469): fresh or a transition.
    fn script_connect(&mut self, spec: &ScriptLegatoSpec, note: u8, velocity: u8) {
        let prev = self.script_leg.prev;
        self.script_leg.back = prev;
        self.script_leg.last_vel = velocity;
        let octave = self.script_octave(spec, prev, note);
        let t = match (&spec.octave, octave) {
            (Some(o), true) => &o.timing,
            _ => &spec.timing,
        };
        let since_ms = frames_to_ms(
            self.frames_rendered.saturating_sub(self.script_leg.last_on),
            self.sample_rate,
        );
        let connects = prev.is_some_and(|p| p.abs_diff(note) <= spec.max_interval)
            && (spec.bypass_ms <= 0.0 || since_ms <= spec.bypass_ms);
        match prev {
            Some(from) if connects => self.script_transition(spec, from, note, velocity, octave),
            _ => {
                // A fresh note: the line is monophonic, its voices fade
                // over the fade-in (25653-25690), then the sustain plays.
                let fade = ms_to_frames_f(t.fade_in_ms, self.sample_rate);
                let line = self.cur_line as u8;
                self.voices.fade_out_line_bodies(line, fade);
                self.trigger_zoned_short(note, velocity);
            }
        }
        self.script_leg.prev = Some(note);
        self.script_leg.last_on = self.frames_rendered;
    }

    /// The zone of `artic` holding `key` at `interval`, its dynamic by
    /// `thresholds` over `velocity` (none: its only layer).
    fn script_leg_zone(
        &self,
        artic: &str,
        direction: &str,
        thresholds: &[u8],
        velocity: u8,
        key: u8,
        interval: u8,
    ) -> Option<usize> {
        // no edges: the articulation's one layer, whatever its name
        let layer = thresholds.iter().filter(|&&e| velocity >= e).count();
        let dynamic = if thresholds.is_empty() {
            String::new()
        } else {
            self.patch
                .spec
                .articulation(artic)
                .and_then(|a| a.dynamics.get(layer).or(a.dynamics.last()).cloned())
                .unwrap_or_default()
        };
        self.zone_keys.get(usize::from(key)).and_then(|c| {
            c.iter().map(|&i| i as usize).find(|&i| {
                let z = &self.patch.spec.zones[i];
                z.interval == u32::from(interval) && self.zone_passes(z, artic, direction, &dynamic)
            })
        })
    }

    /// The transition `from → to` (26028-26469).
    fn script_transition(
        &mut self,
        spec: &ScriptLegatoSpec,
        from: u8,
        to: u8,
        velocity: u8,
        octave: bool,
    ) {
        let sr = self.sample_rate;
        let natural = self.script_natural(spec);
        let oct = spec.octave.as_ref().filter(|_| octave);
        let t = oct.map_or(&spec.timing, |o| &o.timing);
        let interval = from.abs_diff(to);
        let down = from >= to;
        // a fixed layer: as many edges as its index, all passed (layer 0:
        // one no velocity reaches, so the first dynamic, not any)
        let fixed: Vec<u8> = match spec.fixed_layer {
            Some(0) => vec![u8::MAX],
            Some(l) => vec![0; usize::from(l)],
            None => Vec::new(),
        };
        let (artic, thresholds) = match oct {
            Some(o) => (if down { &o.down } else { &o.up }, &o.layer_thresholds),
            None if spec.fixed_layer.is_some() => {
                (if down { &spec.down } else { &spec.up }, &fixed)
            }
            None => (
                if down { &spec.down } else { &spec.up },
                &spec.layer_thresholds,
            ),
        };
        let direction = if down { "down" } else { "up" };
        let m = match t.speed_cc {
            _ if t.speed_by_velocity => i32::from(velocity),
            Some(cc) => {
                let c = usize::from(cc & 127);
                127 - i32::from(if self.cc_sent[c] {
                    self.cc_values[c]
                } else {
                    t.speed_cc_default
                })
            }
            None => {
                let ioi = frames_to_ms(
                    self.frames_rendered.saturating_sub(self.script_leg.last_on),
                    sr,
                );
                legato_speed_index(t, ioi as i32)
            }
        };
        let (speed, hold_ms) = legato_speed_and_hold(t, interval, m);
        // natural plays the recording's own speed (26088-26092); the hold
        // still follows the computed one, but a natural note holds its
        // natural length (26094)
        let nat = spec.natural.as_ref().filter(|_| natural);
        let play_speed = if nat.is_some() { 100 } else { speed };
        let hold_ms = match nat {
            Some(n) if !octave => n.length_ms,
            _ => hold_ms as f32,
        };
        let gains = nat.map_or(&spec.gain_db, |n| &n.gain_db);

        // The line's voices fade over the fade-in (26438-26443, the
        // previous transition too, 26124).
        let sustain = self.articulation.clone();
        let line = self.cur_line as u8;
        self.voices
            .fade_out_line_bodies(line, ms_to_frames_f(t.fade_in_ms, sr));

        let idx = self.script_leg_zone(artic, direction, thresholds, velocity, to, interval);
        self.voices.begin_strike();
        let strike = self.voices.current_strike();
        if let Some(idx) = idx {
            let gain = gains
                .get(usize::from(velocity.min(127)))
                .map_or(1.0, |&db| db_to_gain(db));
            let saved = (
                self.spawn_extra_start,
                self.spawn_stretch,
                self.spawn_fade_in,
            );
            self.spawn_extra_start = kontakt_offset_frames(t.trans_offset_ms, sr);
            self.spawn_stretch = self.stretch_ratio(play_speed);
            self.spawn_fade_in = ms_to_frames_f(t.fade_in_ms / 2.0, sr);
            self.spawn_zone_voice(idx, to, VoiceKind::Short, gain, None, 0.0);
            (
                self.spawn_extra_start,
                self.spawn_stretch,
                self.spawn_fade_in,
            ) = saved;
        }
        // Its key-up before the target plays the release sample over no
        // body: a holding transition is not faded (37014-37579); a
        // natural transition IS the note (`MARK_7`, 26460) and is.
        let k = usize::from(to & 127);
        let natural_note = nat.is_some() && !octave;
        self.release_body_strike[k] = Some(if natural_note { strike } else { u32::MAX });
        self.release_sample_notes[k].clear();
        let has_release = self
            .patch
            .spec
            .articulation(&sustain)
            .is_some_and(|a| a.release_sample.is_some() || a.release_types.is_some());
        if has_release {
            let (dynamic_of, _) = self.short_layer_and_velvol(&sustain, velocity);
            self.release_sample_notes[k].push((sustain.clone(), dynamic_of, velocity));
        }
        let fade_out = ms_to_frames_f(t.fade_out_ms, sr);
        let stage = match nat {
            Some(_) if !octave => ScriptLegStage::Expire { artic: sustain },
            Some(n) => ScriptLegStage::Tail {
                artic: n.tail.clone(),
                fade_out,
            },
            None => ScriptLegStage::Target {
                artic: sustain,
                fade_out,
                target_offset: kontakt_offset_frames(t.target_offset_ms, sr),
            },
        };
        self.script_leg.pending = Some(ScriptLegPending {
            due: self.frames_rendered + ms_to_frames_f(hold_ms, sr) as u64,
            strike,
            note: to,
            velocity,
            stage,
        });
    }

    /// The key-up of a note while its articulation's legato is on. `true`
    /// when the legato took it (no release sample here).
    pub(crate) fn script_legato_note_off(&mut self, note: u8) -> bool {
        let Some(spec) = self.script_legato_on() else {
            return false;
        };
        if self.script_leg.prev != Some(note) {
            // Not the line's last note: its voices have been faded already.
            self.release_sample_notes[usize::from(note & 127)].clear();
            return true;
        }
        self.script_leg.prev = None;
        if spec.release_fade_ms > 0.0 {
            // `fade_out(MARK_7, Exp2(release knob))`; the key-up runs the
            // rest of the note's release as usual.
            if let Some(strike) = self.script_leg.target_strike.take() {
                let fade = ms_to_frames_f(spec.release_fade_ms, self.sample_rate);
                self.voices.fade_out_strike_bodies(strike, fade);
            }
            return false;
        }
        let back = self
            .script_leg
            .back
            .filter(|&b| b != note && self.held_notes.contains_key(&b));
        let Some(back) = back else {
            // The normal release sample (the caller plays it).
            return false;
        };
        // Legato back to the held note before it (33710-36437), at the last
        // note-on's velocity; no release sample.
        self.release_sample_notes[usize::from(note & 127)].clear();
        let line = self.cur_line as u8;
        self.voices.release_note_envelopes(line, note);
        if let Some(p) = self.script_leg.pending.take()
            && let ScriptLegStage::Target { fade_out, .. } | ScriptLegStage::Tail { fade_out, .. } =
                p.stage
        {
            self.voices.fade_out_strike_bodies(p.strike, fade_out);
        }
        let velocity = self.script_leg.last_vel;
        self.script_leg.back = Some(note);
        if note.abs_diff(back) <= spec.max_interval {
            let octave = self.script_octave(&spec, Some(note), back);
            self.script_transition(&spec, note, back, velocity, octave);
        }
        self.script_leg.prev = Some(back);
        self.script_leg.last_on = self.frames_rendered;
        true
    }

    /// Frames until the running callback's next step.
    pub(crate) fn script_leg_due_in(&self) -> Option<usize> {
        let a = self.script_leg.pending.as_ref().map(|p| p.due);
        let b = self.script_leg.deferred.as_ref().map(|d| d.0);
        let due = a.into_iter().chain(b).min()?;
        Some(due.saturating_sub(self.frames_rendered) as usize)
    }

    /// Run what is due: a queued note, the end of a hold (26470-27111), a
    /// natural note's end (27685-28249).
    pub(crate) fn fire_script_legato(&mut self) {
        if self
            .script_leg
            .deferred
            .as_ref()
            .is_some_and(|d| d.0 <= self.frames_rendered)
            && let Some((_, note, velocity, stray)) = self.script_leg.deferred.take()
            && let Some(spec) = self.script_legato_on()
        {
            self.script_connect_deferred(&spec, note, velocity, stray);
        }
        if self
            .script_leg
            .pending
            .as_ref()
            .is_none_or(|p| p.due > self.frames_rendered)
        {
            return;
        }
        let Some(p) = self.script_leg.pending.take() else {
            return;
        };
        let held = self.held_notes.contains_key(&p.note);
        match p.stage {
            ScriptLegStage::Target {
                artic,
                fade_out,
                target_offset,
            } => {
                self.voices.fade_out_strike_bodies(p.strike, fade_out);
                if held {
                    self.script_target(&artic, p.note, p.velocity, fade_out, target_offset);
                }
            }
            ScriptLegStage::Tail { artic, fade_out } => {
                self.voices.fade_out_strike_bodies(p.strike, fade_out);
                if held {
                    self.script_tail(&artic, p.note, p.velocity, fade_out);
                }
            }
            ScriptLegStage::Expire { artic } => {
                // still the line's last note and down: it ends here, with
                // the release sample; the next note is fresh
                if held && self.script_leg.prev == Some(p.note) {
                    self.script_leg.prev = None;
                    let line = self.cur_line as u8;
                    let k = usize::from(p.note & 127);
                    for (a, dynamic, velocity) in std::mem::take(&mut self.release_sample_notes[k])
                    {
                        self.script_release(line, p.note, &a, &dynamic, velocity);
                    }
                    let _ = artic;
                }
            }
        }
    }

    /// The target sustain: `artic` at `offset`, fading in over `fade`
    /// (26790-27111).
    fn script_target(&mut self, artic: &str, note: u8, velocity: u8, fade: usize, offset: usize) {
        let saved_artic = std::mem::replace(&mut self.articulation, artic.to_string());
        let saved = (
            self.spawn_extra_start,
            self.spawn_fade_in,
            self.in_transition,
        );
        self.spawn_extra_start = offset;
        self.spawn_fade_in = fade;
        self.in_transition = true;
        self.release_sample_notes[usize::from(note & 127)].clear();
        self.voices.begin_strike();
        self.script_leg.target_strike = Some(self.voices.current_strike());
        self.trigger_zoned_short(note, velocity);
        (
            self.spawn_extra_start,
            self.spawn_fade_in,
            self.in_transition,
        ) = saved;
        self.articulation = saved_artic;
    }

    /// Natural octave's unison tail (26475-26789): the tail articulation's
    /// zone at the key (the unison band), offset 0, the sustain's level,
    /// fading in over `fade`; then its natural end after `tail_ms`.
    fn script_tail(&mut self, tail: &str, note: u8, velocity: u8, fade: usize) {
        let Some(spec) = self.script_legato_on() else {
            return;
        };
        let Some(nat) = spec.natural.clone() else {
            return;
        };
        let sustain = self.articulation.clone();
        let level_db = self
            .patch
            .spec
            .articulation(&sustain)
            .and_then(|a| a.vel_gain_db.get(usize::from(velocity.min(127))).copied())
            .unwrap_or(0.0);
        self.voices.begin_strike();
        let strike = self.voices.current_strike();
        if let Some(idx) =
            self.script_leg_zone(tail, "down", &nat.tail_layer_thresholds, velocity, note, 0)
        {
            let saved = self.spawn_fade_in;
            self.spawn_fade_in = fade;
            self.spawn_zone_voice(idx, note, VoiceKind::Short, db_to_gain(level_db), None, 0.0);
            self.spawn_fade_in = saved;
        }
        self.release_body_strike[usize::from(note & 127)] = Some(strike);
        self.script_leg.pending = Some(ScriptLegPending {
            due: self.frames_rendered + ms_to_frames_f(nat.tail_ms, self.sample_rate) as u64,
            strike,
            note,
            velocity,
            stage: ScriptLegStage::Expire { artic: sustain },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trumpet_sus() -> ScriptLegatoTiming {
        ScriptLegatoTiming {
            fade_in_ms: 50.0,
            trans_offset_ms: 28.0,
            hold_small_ms: 116.0,
            hold_large_ms: 120.0,
            fade_out_ms: 95.0,
            target_offset_ms: 373.0,
            slow_small_pct: 95,
            fast_small_pct: 120,
            slow_large_pct: 95,
            fast_large_pct: 120,
            slowest_ms: 1420,
            fastest_ms: 46,
            speed_cc: None,
            speed_cc_default: 0,
            speed_by_velocity: false,
        }
    }

    /// KONTRA's trace of Trumpet 66 → 67 at 300 ms: speed 117 % (Time
    /// Machine 698062), the target 99 ms after the transition.
    #[test]
    fn trumpet_semitone_at_300_ms() {
        let t = trumpet_sus();
        let m = legato_speed_index(&t, 300);
        assert_eq!(m, 103);
        assert_eq!(legato_speed_and_hold(&t, 1, m), (115, 99));
    }

    /// The decode's worked values at i = 1: 120 % → 96 ms, 95 % → 121 ms.
    #[test]
    fn hold_follows_speed() {
        let t = trumpet_sus();
        assert_eq!(legato_speed_and_hold(&t, 1, 127), (120, 96));
        assert_eq!(legato_speed_and_hold(&t, 1, 0), (95, 121));
    }
}
