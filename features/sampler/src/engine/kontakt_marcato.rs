//! A CSS marcato transition exactly as Kontakt runs the instrument's script
//! (`$ocjln=6`) — `ArticulationSpec::marcato_transition`. Every step was
//! null-tested against Kontakt 8 renders of CSS 1st Violins with
//! `features/rigs/orchestra/tests/marcato_fit.py --model` (−34 to −43 dB on
//! every probed transition); the numbers and KSP lines are in
//! [`MarcatoTransitionSpec`]'s doc.

use super::*;
use crate::spec::{EnvSegmentSpec, MarcatoTransitionSpec, ksp_lerp, lerp_table};

/// The LT's velocity term (dB) in the script's integer deci-dB:
/// `%qgipe[51] := -$0kd5m`, `%lwgt4[51] := %qgipe[51]*-100000/127`,
/// `$arhiq := (127-$EVENT_VELOCITY)*%lwgt4[51]/10000` (CSSS 6139-6141, 11771).
fn marcato_lt_vel_db(vel_db: f32, velocity: u8) -> f32 {
    if vel_db == 0.0 {
        return 0.0;
    }
    let lw = -(vel_db.round() as i64) * -100_000 / 127;
    let ddb = i64::from(127 - velocity.min(127)) * lw / 10_000;
    ddb as f32 / 10.0
}

impl SampleEngine {
    /// The current articulation's tremolo-style transition, if it has one.
    pub(crate) fn simple_transition(&self) -> Option<crate::spec::SimpleTransitionSpec> {
        self.patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.simple_transition.clone())
    }

    /// Fire one tremolo / trills / harmonics transition `from → to` at the
    /// second note-on (see [`crate::spec::SimpleTransitionSpec`]).
    pub(crate) fn fire_simple_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        st: &crate::spec::SimpleTransitionSpec,
    ) {
        let interval = u32::from(to.abs_diff(from));
        let big = interval > 12;
        let zone = usize::from(
            self.patch
                .spec
                .legato_cfg()
                .velocity_range(velocity)
                .clamp(1, 3)
                - 1,
        );
        let (old_ms, new_ms) = if big {
            (st.big_fade_ms[0], st.big_fade_ms[1])
        } else {
            (st.old_fade_ms[zone], st.new_fade_ms[zone])
        };
        let line = self.cur_line as u8;
        let sr = self.sample_rate;
        let ms = move |v: f32| ms_to_frames_f(v, sr);

        // A key no zone covers (harmonics below their range): Kontakt runs
        // no transition — the old note keeps the line untouched, its body
        // fading and its release sample sounding at its own key-up. Null
        // test: harmonics A3 → F#3, −15.2 → −33.4 dB (fading the body at
        // the new key-on instead ran it out 50 ms early).
        let artic = self.articulation.clone();
        let covered = self.patch.spec.zones.iter().any(|z| {
            z.articulation.eq_ignore_ascii_case(&artic) && (z.key_min..=z.key_max).contains(&to)
        });
        if !covered && self.patch.spec.performance.strict_key_ranges {
            // A fall-back from a released key into an unmapped one ends the
            // line (nothing is left sounding on it).
            if !self.held_notes.contains_key(&from) {
                let fade = ms(self.patch.spec.performance.release_ms.unwrap_or(580) as f32);
                self.voices.note_off_line(line, from, Some(fade));
                // ... and its key-up plays its release sample, as ever.
                self.spawn_release(from);
                self.line_mut().note = None;
            }
            return;
        }

        // 1. The old body (and any transition voice still sounding) fades out.
        self.voices
            .fade_out_note_line(line, from, ms(old_ms), ms(old_ms));
        // 2. The old note's in-phrase release.
        self.release_gain_override = Some(db_to_gain(st.in_phrase_release_db));
        self.release_context = Some(crate::spec::ReleaseContext::InPhrase);
        self.spawn_release(from);
        self.release_context = None;
        self.release_gain_override = None;
        // 3. The new body from its sample start, fading in.
        let fade = ms(new_ms);
        self.sustain_fade_in = Some((0, fade, fade, 0));
        self.in_transition = true;
        self.play_direction = if to >= from { "up" } else { "down" }.to_string();
        self.trigger_zoned_sustain(to);
        self.in_transition = false;
        self.sustain_fade_in = None;
        // 4. The transition voice (tremolo / trills; within an octave).
        if !st.lt_articulation.is_empty() && !big {
            let segs: Vec<(f32, f32, f32)> = st
                .lt_env
                .iter()
                .map(|e| (e.time_ms, e.level, e.curve))
                .collect();
            let env = voice::FlexEnv::from_segments(&segs, 0.0, sr, false);
            let fade_out =
                voice::FlexEnv::from_segments(&[(st.lt_fade_out_ms, 0.0, 0.5)], 1.0, sr, false)
                    .map(super::kontakt_legato::KontaktDip::Curve);
            let lt = st.lt_articulation.clone();
            self.curves_cc1_only = true;
            self.spawn_kontakt_lt_at(
                &lt,
                from,
                to,
                interval,
                ms(st.lt_offset_ms),
                env,
                fade_out,
                db_to_gain(st.lt_db),
            );
            self.curves_cc1_only = false;
        }
        let l = self.line_mut();
        l.note = Some(to);
        l.marcato_ioi = None;
        l.connected = true;
    }

    /// The current articulation's marcato transition, if it has one.
    pub(crate) fn marcato_transition(&self) -> Option<MarcatoTransitionSpec> {
        self.patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.marcato_transition.clone())
    }

    /// Fire one marcato transition `from → to` at the second note-on (no
    /// overlap delay). `ioi_ms` is the time since the previous note-on.
    pub(crate) fn fire_marcato_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        ioi_ms: f32,
        mt: &MarcatoTransitionSpec,
    ) {
        self.trace_push(TraceKind::Transition {
            from,
            to,
            portamento: false,
        });
        // The script measures the IOI in whole ms of `$ENGINE_UPTIME`.
        let ioi = ioi_ms.max(0.0) as i64;
        let interval = to.abs_diff(from);
        let big = interval > 12;
        let hard = velocity > mt.hard_velocity && ioi > mt.hard_ioi_ms as i64;
        self.marcato_kbqnb = hard;
        let line = self.cur_line as u8;
        let sr = self.sample_rate;
        let ms = move |v: f32| ms_to_frames_f(v, sr);

        // `$igmiu` split and `$x444h` divisor → the two-stage fade-in.
        let split = if big {
            mt.split_big
        } else {
            mt.split[usize::from(hard)]
        };
        let divisor = ksp_lerp(&mt.node_divisor, ioi).max(1) as f32;
        let stage1_ms = mt.xfade_ms * split / divisor;
        let stage2_ms = mt.xfade_ms * (100.0 - split) / 100.0;
        // Stage 2 starts on the `$qsazz`-th 1 ms tick of the loop, which
        // decrements before it tests: at tick − 1 ms.
        let stage2_at_ms = ((mt.xfade_ms * split / 100.0) as i64 - 1).max(0) as f32;

        // CC19 / CC18 the script sets, and the glide depth.
        let pick = |soft: i64, other: [f32; 3]| -> f32 {
            match (big, hard) {
                (false, false) => soft as f32,
                (false, true) => other[0],
                (true, false) => other[1],
                (true, true) => other[2],
            }
        };
        let cc19 = pick(ksp_lerp(&mt.cc19_soft, ioi), mt.cc19_other);
        let cc18_soft = if interval <= 2 {
            &mt.cc18_soft_step
        } else {
            &mt.cc18_soft_leap
        };
        let cc18 = pick(ksp_lerp(cc18_soft, ioi), mt.cc18_other);
        let glide_tab = match interval {
            0 => None,
            1 => Some(&mt.glide_semitone),
            2 => Some(&mt.glide_tone),
            _ => Some(&mt.glide_wide),
        };
        let bend =
            glide_tab.map_or(0.0, |t| ksp_lerp(t, ioi) as f32) * if to > from { 1.0 } else { -1.0 };
        let glide = ms(mt.glide_ms);

        // 1. Retire the old pair: body fade + bend + sink, previous LT.
        let sink = (cc19 < 127.0)
            .then(|| {
                voice::FlexEnv::from_segments(
                    &[
                        (mt.old_sink_start_ms, 1.0, 0.5),
                        (mt.old_sink_ms, mt.old_sink_level, 0.5),
                    ],
                    1.0,
                    sr,
                    false,
                )
            })
            .flatten();
        let class = usize::from(hard);
        let lt_retire = mt
            .lt_retire_ms_by_class
            .map_or_else(|| ksp_lerp(&mt.lt_retire_ms, ioi) as f32, |c| c[class]);
        self.voices.marcato_retire(
            line,
            from,
            ms(mt.xfade_ms),
            ms(lt_retire),
            bend,
            glide,
            sink.as_ref(),
        );

        // the script's set_controller: routes reading the controller follow
        if mt.cc19_controller > 0 {
            if let Some(slot) = self.cc_values.get_mut(usize::from(mt.cc19_controller)) {
                *slot = cc19.clamp(0.0, 127.0) as u8;
            }
        }
        // A note inside the previous transition's crossfade loop waits for
        // that loop's next tick: everything it starts below starts late. A
        // note on the loop's last tick still finds it running (each pass
        // ends in its wait, the flag clears after it).
        let now = self.frames_rendered;
        let busy_hold = if mt.busy_tick_ms > 0.0 && now <= self.line().marcato_loop_end {
            ms(mt.busy_tick_ms)
        } else {
            0
        };
        let outer_hold = self.spawn_extra_hold;
        self.spawn_extra_hold += busy_hold;
        let loop_ms = if bend != 0.0 {
            mt.xfade_ms.max(mt.glide_ms)
        } else {
            mt.xfade_ms
        };
        self.line_mut().marcato_loop_end = now + busy_hold as u64 + ms(loop_ms) as u64;

        // 2. The old note's in-phrase release.
        self.release_gain_override = Some(db_to_gain(mt.in_phrase_release_db));
        self.release_context = Some(crate::spec::ReleaseContext::InPhrase);
        self.spawn_release(from);
        self.release_context = None;
        self.release_gain_override = None;

        // 3. The new body: next round-robin, entered deep, muted, two-stage
        //    fade-in under the CC-driven envelopes, scooping into pitch.
        let offs = if big {
            mt.body_offset_big_ms
        } else {
            mt.body_offset_ms
        };
        let env = |segs: &[EnvSegmentSpec], cc: f32| {
            let lv = |l: f32| if l < 0.0 { cc / 127.0 } else { l };
            let s: Vec<(f32, f32, f32)> = segs
                .iter()
                .map(|e| (e.time_ms, lv(e.level), e.curve))
                .collect();
            voice::FlexEnv::from_segments(&s, 0.0, sr, false)
        };
        self.sustain_flex = Some((env(&mt.body_env_cc19, cc19), env(&mt.body_env_cc18, cc18)));
        self.sustain_fade_in = Some((0, ms(stage2_at_ms), ms(stage1_ms), ms(stage2_ms)));
        // the script's wait before it plays the body: the body starts later
        // (held, not muted while it runs)
        let saved_hold = self.spawn_extra_hold;
        self.spawn_extra_hold += ms(mt.body_delay_ms);
        // CSSS draws an extra random(0, n) ms of entry per connection
        let extra = if mt.body_offset_random_ms > 0.0 {
            let n = mt.body_offset_random_ms as u16;
            match self.forced_offsets.pop_front() {
                Some(v) => f32::from(v.min(n)),
                None => (super::next_zone_random(&mut self.tune_rng) % (usize::from(n) + 1)) as f32,
            }
        } else {
            0.0
        };
        self.spawn_extra_start = ms(offs[usize::from(hard)] + extra);
        self.legato_glide = (bend != 0.0).then_some((-bend, glide));
        self.in_transition = true;
        self.play_direction = if to >= from { "up" } else { "down" }.to_string();
        self.trigger_zoned_sustain(to);
        self.spawn_extra_hold = saved_hold;
        self.in_transition = false;
        self.legato_glide = None;
        self.spawn_extra_start = 0;
        self.sustain_fade_in = None;
        self.sustain_flex = None;

        // 4. The marc-leg transition sample, per marcato dynamic layer.
        if !big {
            let lt_ms = mt
                .lt_offset_ms_by_class
                .map_or_else(|| ksp_lerp(&mt.lt_offset_ms, ioi) as f32, |c| c[class]);
            let lt_db_tab = if interval <= 2 {
                &mt.lt_db_step
            } else {
                &mt.lt_db_leap
            };
            let lt_db = mt
                .lt_db_by_class
                .map_or_else(|| lerp_table(lt_db_tab, ioi as f32), |c| c[class])
                + mt.lt_group_db
                + self.marcato_lt_ramp_db(mt)
                + marcato_lt_vel_db(mt.lt_vel_db, velocity);
            let lt_env: Vec<(f32, f32, f32)> = mt
                .lt_env
                .iter()
                .map(|e| (e.time_ms, e.level, e.curve))
                .collect();
            let lt_env = voice::FlexEnv::from_segments(&lt_env, 0.0, sr, false);
            self.spawn_marcato_lt(mt, from, to, u32::from(interval), lt_ms, lt_db, lt_env);
        }
        // A loud connected note plays the overlay too (CSB).
        let connected_from = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.overlay.as_ref())
            .and_then(|o| o.connected_from_velocity);
        if connected_from.is_some_and(|v| velocity >= v) {
            self.spawn_marcato_overlay_unconditional(to, velocity);
        }
        self.spawn_extra_hold = outer_hold;
        let l = self.line_mut();
        l.note = Some(to);
        l.marcato_ioi = Some(ioi as f32);
        l.connected = true;
    }

    /// The LT's phrase ramp (dB) for a connection now, moving the phrase
    /// anchor as the script does (`spec::ksp_phrase_ramp_ddb`).
    fn marcato_lt_ramp_db(&mut self, mt: &MarcatoTransitionSpec) -> f32 {
        if mt.lt_ramp.is_none() {
            return 0.0;
        }
        let now = self.frames_rendered;
        // `$ENGINE_UPTIME` counts whole ms
        let elapsed =
            (frames_to_ms(now.saturating_sub(self.ab_anchor_frame), self.sample_rate)) as i64;
        let (ddb, moved) = crate::spec::ksp_phrase_ramp_ddb(mt.lt_ramp, elapsed);
        if moved {
            let window = mt.lt_ramp.map_or(0, |r| r[2].max(0) as u32);
            self.ab_anchor_frame =
                now.saturating_sub(ms_to_frames(window, self.sample_rate) as u64);
        }
        ddb as f32 / 10.0
    }

    /// The marc-leg zones of the LOWER note (interval-coded), one voice per
    /// marcato dynamic layer the CC1 curves leave audible.
    #[allow(clippy::too_many_arguments)]
    fn spawn_marcato_lt(
        &mut self,
        mt: &MarcatoTransitionSpec,
        from: u8,
        to: u8,
        interval: u32,
        lt_ms: f32,
        lt_db: f32,
        env: Option<voice::FlexEnv>,
    ) {
        let lower = from.min(to);
        let direction = if to > from { "up" } else { "down" };
        let marc = self.articulation.clone();
        // Layers whose groups hold the same recording (CSW: every marc-leg
        // group holds the one MLeg set) play it as one voice per group in
        // Kontakt: their levels add, into one voice here.
        let mut plays: Vec<(usize, f32, usize)> = Vec::new();
        for layer in &mt.lt_layers {
            let g = self.group_curve_gain(&marc, &layer.dynamic).unwrap_or(1.0);
            if g <= 0.0 {
                continue;
            }
            let fits = |z: &crate::spec::ZoneSpec| {
                z.articulation.eq_ignore_ascii_case(&mt.lt_articulation)
                    && z.direction.eq_ignore_ascii_case(direction)
                    && z.interval == interval
                    && z.dynamic.eq_ignore_ascii_case(&layer.lt_dynamic)
                    && (z.key_min..=z.key_max).contains(&lower)
                    && self
                        .solo_mic
                        .as_ref()
                        .is_none_or(|m| z.mic.eq_ignore_ascii_case(m))
            };
            // A marcato row of a legato file (CSS, CSB), else a recording of
            // its own (CSW's MLeg: no marcato rows, played from its start at
            // its own level).
            let zones = &self.patch.spec.zones;
            let marked = zones.iter().position(|z| fits(z) && z.marcato_start > 0);
            let (idx, own) = match marked {
                Some(i) => (i, false),
                None => match zones.iter().position(|z| fits(z) && z.marcato_start == 0) {
                    Some(i) => (i, true),
                    None => continue,
                },
            };
            let z = &self.patch.spec.zones[idx];
            // A section whose transition file is entered by one marcato row
            // only (Cellos: one 3984 row per file, where the violins list
            // one per marc-leg layer) plays that row for every layer.
            let zone_db = if own {
                z.gain_db
            } else {
                let Some(&db) = z
                    .marcato_gains_db
                    .get(layer.gain_index as usize)
                    .or_else(|| z.marcato_gains_db.last())
                else {
                    continue;
                };
                db
            };
            // Past the zone's own start: marcato zones enter the same file
            // at `marcato_start`, then the IOI offset.
            // the marcato row's own start, then the script's offset (which
            // the zone's start-modulation range may limit)
            let offset = (z.marcato_start as usize).saturating_sub(z.sample_start as usize)
                * usize::from(!own)
                + self.script_offset(idx, ms_to_frames_f(lt_ms, self.sample_rate));
            let gain = g * db_to_gain(zone_db - z.gain_db + lt_db);
            match plays.iter_mut().find(|(i, _, o)| *i == idx && *o == offset) {
                Some(p) => p.1 += gain,
                None => plays.push((idx, gain, offset)),
            }
        }
        // Pitched from the lower note, where Kontakt maps the zone.
        let pitch = f64::from(lower) - f64::from(to);
        for (idx, gain, offset) in plays {
            if self.spawn_zone_voice_at(idx, to, VoiceKind::Legato, gain, None, pitch, offset) {
                if let Some(v) = self.voices.last_spawned_mut() {
                    if let Some(e) = env.clone() {
                        v.set_flex_env(e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::marcato_lt_vel_db;
    use crate::spec::ksp_phrase_ramp_ddb;

    #[test]
    fn lt_velocity_term_is_the_scripts_integer_deci_db() {
        // CSSS $0kd5m = -9: %lwgt4[51] = -7086
        assert_eq!(marcato_lt_vel_db(-9.0, 127), 0.0);
        assert_eq!(marcato_lt_vel_db(-9.0, 40), -6.1); // 87 * -7086 / 10000 = -61
        assert_eq!(marcato_lt_vel_db(-9.0, 120), -0.4);
        assert_eq!(marcato_lt_vel_db(0.0, 1), 0.0);
    }

    #[test]
    fn lt_ramp_follows_the_scripts_three_branches() {
        let r = Some([-60, -30, 250, 2000]); // CSSS $4lqhx $ee3a4 $xu41m $c2hkn
        let at = |t| ksp_phrase_ramp_ddb(r, t);
        assert_eq!(at(0), (-60, false));
        assert_eq!(at(125), (-45, false));
        assert_eq!(at(250), (-30, false));
        // B: (-3000 + t * (3000 / 2000 = 1)) / 100
        assert_eq!(at(1000), (-20, true));
        assert_eq!(at(2000), (-10, true));
        assert_eq!(at(2001), (0, true));
        assert_eq!(ksp_phrase_ramp_ddb(None, 0), (0, false));
    }
}
