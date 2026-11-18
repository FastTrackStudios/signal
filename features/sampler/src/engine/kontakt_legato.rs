//! A CSS legato transition exactly as Kontakt runs the instrument's script —
//! `LegatoEngineSpec::kontakt_transition`. Every step and number was
//! null-tested against Kontakt 8 renders of CSS 1st Violins (each transition
//! explained to −41 dB by the four voices below); the KSP line references are
//! in `KontaktTransitionSpec`'s doc and the orchestra decode notes.

use super::*;
use crate::spec::KontaktTransitionSpec;

/// The transition voice's phrase-start dip: a measured recovery curve, or a
/// plain linear ramp back from `(gain, frames)`.
#[derive(Clone)]
pub(crate) enum KontaktDip {
    Curve(voice::FlexEnv),
    Ramp(f32, usize),
}

impl SampleEngine {
    /// Fire one transition `from → to` (the overlap delay has already been
    /// waited by the reactive countdown).
    pub(crate) fn fire_kontakt_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        ioi_ms: f32,
        kt: &KontaktTransitionSpec,
    ) {
        let expressive = self.legato_expressive;
        let (zone, lt_od_ms, lt_base) = {
            let cfg = self.patch.spec.legato_cfg();
            let base = if expressive {
                cfg.lt_offset_expressive.clone()
            } else {
                cfg.lt_offset_low_latency.clone()
            };
            let zone = cfg.velocity_range(velocity);
            let od = cfg.overlap_delay_ms(ioi_ms, velocity, expressive) as f32;
            // the share of the overlap delay that deepens the entry
            let lt_od = if cfg.overlap_offset_zone1_only && zone > 1 {
                0.0
            } else {
                od
            };
            (zone, lt_od, base)
        };
        let interval = u32::from(to.abs_diff(from));
        let big_leap = interval > 12;
        let line = self.cur_line as u8;
        let sr = self.sample_rate;
        let ms = move |v: f32| ms_to_frames_f(v, sr);

        // 1. Retire the old pair with linear fades: the body over the
        //    zone's old-body fade (the big-leap one beyond an octave), the
        //    previous transition voice by IOI (`lt_retire`).
        let lt_retire_ms = kt.lt_retire.as_ref().map_or(0.0, |r| {
            let curve = match &r.loud {
                Some(loud) if zone >= r.loud_from_zone => loud,
                _ if expressive => &r.expressive,
                _ => &r.low_latency,
            };
            curve.value_at(ioi_ms)
        });
        let old_body_ms = if big_leap {
            kt.big_leap_old_fade_ms
        } else {
            kt.old_body_fade_by_zone_ms
                .get(usize::from(zone.clamp(1, 3) - 1))
                .copied()
                .unwrap_or(kt.old_body_fade_ms)
        };
        self.voices
            .fade_out_note_line(line, from, ms(lt_retire_ms), ms(old_body_ms));

        // 2. The old note's "note in phrase" release, at a flat level.
        self.release_gain_override = Some(db_to_gain(kt.in_phrase_release_db));
        self.release_context = Some(crate::spec::ReleaseContext::InPhrase);
        self.release_extra_hold = ms(kt.in_phrase_release_delay_ms);
        self.spawn_release(from);
        self.release_extra_hold = 0;
        self.release_context = None;
        self.release_gain_override = None;

        // 3. The destination body: silent, then a linear fade-in — after
        //    `body_wait_ms` normally; immediately (and entered 75 ms in)
        //    beyond an octave.
        let (wait, fade) = if big_leap {
            (0, ms(kt.big_leap_new_fade_ms))
        } else {
            (ms(kt.body_wait_ms), ms(kt.body_fade_ms))
        };
        self.sustain_fade_in = Some((wait, fade, fade, 0));
        // Per-layer handoff (each layer on its own clock), ms per layer.
        let handoff = self.kontakt_handoff(kt, from, to, zone, big_leap);
        self.sustain_fade_in_by_dyn = handoff
            .iter()
            .map(|(a, d, w, f, o)| (a.clone(), d.clone(), (ms(*w), ms(*f), ms(*f), 0), ms(*o)))
            .collect();
        self.spawn_extra_start = if big_leap {
            ms(kt.big_leap_body_offset_ms)
        } else {
            0
        };
        // Zone 2 plays a random body variant (`$jpvdn := random(2,4)`,
        // 18108); zones 1 and 3 the plain zone.
        if zone == 2 {
            let v = match self.forced_variants.pop_front() {
                Some(v) => v,
                None => self.draw_uniform(2, 4),
            };
            self.spawn_variant = Some(v);
        }
        self.in_transition = true;
        self.play_direction = if to >= from { "up" } else { "down" }.to_string();
        self.trigger_zoned_sustain(to);
        self.in_transition = false;
        self.spawn_variant = None;
        self.spawn_extra_start = 0;
        self.sustain_fade_in = None;
        self.sustain_fade_in_by_dyn.clear();

        // 4. The transition sample (interval ≤ an octave): the LOWER note's
        //    interval-coded zone of BOTH CC2 sides, entered at the mode/zone
        //    offset + the overlap delay.
        if !big_leap {
            let base_ms = lt_base
                .get(usize::from(zone.max(1) - 1))
                .copied()
                .unwrap_or(0) as f32;
            let offset = ms(base_ms + lt_od_ms);
            let env = self.kontakt_transition_env(kt, expressive, zone, ioi_ms);
            let dip = self.kontakt_phrase_dip(kt);
            let ioi_dip_db = kt
                .lt_ioi_dip
                .as_ref()
                .map(|d| (d.db.value_at(ioi_ms), d.recover_ms));
            let (leg_nv, leg_vib) = self.legato_pair_ids(false);
            for leg in [leg_nv, leg_vib].into_iter().flatten() {
                if handoff.is_empty() && ioi_dip_db.is_none() {
                    self.spawn_kontakt_lt(
                        &leg,
                        from,
                        to,
                        interval,
                        offset,
                        env.clone(),
                        dip.clone(),
                    );
                    continue;
                }
                // Per layer: the IOI dip and the fade-out along the body's
                // handoff, as one gain envelope.
                let dyns = self
                    .patch
                    .spec
                    .articulation(&leg)
                    .map(|a| a.dynamics.clone())
                    .unwrap_or_default();
                // the body layer this transition fades under: its own vibrato
                // side's entry, else any entry of the dynamic
                let leg_vib = self
                    .patch
                    .spec
                    .articulation(&leg)
                    .is_some_and(crate::spec::ArticulationSpec::is_vibrato);
                let side_of = |a: &str| {
                    a.is_empty()
                        || self
                            .patch
                            .spec
                            .articulation(a)
                            .is_some_and(|x| x.is_vibrato() == leg_vib)
                };
                let layers: Vec<(String, f32, f32)> = dyns
                    .into_iter()
                    .map(|label| {
                        let (w, f) = handoff
                            .iter()
                            .find(|(a, d, _, _, _)| d.eq_ignore_ascii_case(&label) && side_of(a))
                            .or_else(|| {
                                handoff
                                    .iter()
                                    .find(|(_, d, _, _, _)| d.eq_ignore_ascii_case(&label))
                            })
                            .map_or((0.0, 0.0), |(_, _, w, f, _)| (*w, *f));
                        (label, w, f)
                    })
                    .collect();
                for (label, w, f) in layers {
                    let shape =
                        lt_gain_shape(ioi_dip_db, &kt.lt_fade_curve_db, w, f, self.sample_rate);
                    let d = shape.map(KontaktDip::Curve).or_else(|| dip.clone());
                    self.spawn_kontakt_lt_layer(
                        &leg,
                        &label,
                        from,
                        to,
                        interval,
                        offset,
                        env.clone(),
                        d,
                    );
                }
            }
        }
        self.line_mut().note = Some(to);
        self.line_mut().connected = true;
    }

    /// The transition voice's amplitude envelope for this transition: first
    /// node `(CC66·CC67)/127²` reached in the mode's `lt_seg0_ms`, then
    /// `lt_env`.
    fn kontakt_transition_env(
        &self,
        kt: &KontaktTransitionSpec,
        expressive: bool,
        zone: u8,
        ioi_ms: f32,
    ) -> Option<voice::FlexEnv> {
        let (cc66, cc67) = kt
            .lt_node
            .as_ref()
            .map_or((127.0, 127.0), |n| n.ccs(expressive, zone, ioi_ms));
        let node = (cc66 * cc67) / (127.0 * 127.0);
        let seg0 = if expressive {
            kt.lt_seg0_ms_expressive
        } else {
            kt.lt_seg0_ms_low_latency
        };
        let mut segs = vec![(seg0, node, 0.5)];
        segs.extend(kt.lt_env.iter().map(|s| (s.time_ms, s.level, s.curve)));
        if kt.lt_node1_cc66_depth > 0.0 {
            if let Some(first) = segs.get_mut(1) {
                first.1 = 1.0 - kt.lt_node1_cc66_depth * (1.0 - cc66 / 127.0);
            }
        }
        voice::FlexEnv::from_segments(&segs, 0.0, self.sample_rate, false)
    }

    /// The transition voice's phrase-start dip (KSP `$1z3x0`, 20038-20069):
    /// within the window of the phrase anchor it plays `phrase_dip_db` down
    /// (recovering over the window); otherwise 0 dB and the anchor moves to
    /// `now − window` so the dip never applies twice in a row.
    fn kontakt_phrase_dip(&mut self, kt: &KontaktTransitionSpec) -> Option<KontaktDip> {
        let now = self.frames_rendered;
        let window = ms_to_frames_f(kt.phrase_dip_window_ms, self.sample_rate);
        if now.saturating_sub(self.ab_anchor_frame) > window as u64 {
            self.ab_anchor_frame = now.saturating_sub(window as u64);
            return None;
        }
        let start = db_to_gain(kt.phrase_dip_db);
        if kt.phrase_dip_curve.is_empty() {
            return Some(KontaktDip::Ramp(start, window));
        }
        let segs: Vec<(f32, f32, f32)> = kt
            .phrase_dip_curve
            .iter()
            .map(|s| (s.time_ms, s.level, s.curve))
            .collect();
        voice::FlexEnv::from_segments(&segs, start, self.sample_rate, false).map(KontaktDip::Curve)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_kontakt_lt(
        &mut self,
        leg: &str,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
    ) {
        self.spawn_kontakt_lt_at(leg, from, to, interval, offset, env, dip, 1.0);
    }

    /// Each layer's (dynamic, wait ms, fade ms) for this transition: the
    /// high-key override, else the layer table (the re-strike one for the
    /// same key). Empty when the library hands every layer over alike.
    fn kontakt_handoff(
        &self,
        kt: &KontaktTransitionSpec,
        from: u8,
        to: u8,
        zone: u8,
        big_leap: bool,
    ) -> Vec<(String, String, f32, f32, f32)> {
        let table = if from == to && !kt.retrigger_body_handoff.is_empty() {
            &kt.retrigger_body_handoff
        } else {
            &kt.body_handoff
        };
        let high = kt
            .high_key_handoff
            .as_ref()
            .filter(|h| from.max(to) >= h.min_key && !big_leap);
        table
            .iter()
            .map(|b| {
                let (w, f) =
                    high.map_or_else(|| b.for_zone(zone, big_leap), |h| (h.wait_ms, h.fade_ms));
                let o = b.offset_for(zone, big_leap, self.legato_expressive);
                (b.articulation.clone(), b.dynamic.clone(), w, f, o)
            })
            .collect()
    }

    /// One layer (`label`) of [`spawn_kontakt_lt`](Self::spawn_kontakt_lt).
    #[allow(clippy::too_many_arguments)]
    fn spawn_kontakt_lt_layer(
        &mut self,
        leg: &str,
        label: &str,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
    ) {
        self.spawn_kontakt_lt_layers(leg, Some(label), from, to, interval, offset, env, dip, 1.0);
    }

    /// [`spawn_kontakt_lt`](Self::spawn_kontakt_lt) at an extra `level`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_kontakt_lt_at(
        &mut self,
        leg: &str,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
        level: f32,
    ) {
        self.spawn_kontakt_lt_layers(leg, None, from, to, interval, offset, env, dip, level);
    }

    /// The transition voices of `leg`'s layers (only `only`, when given).
    #[allow(clippy::too_many_arguments)]
    fn spawn_kontakt_lt_layers(
        &mut self,
        leg: &str,
        only: Option<&str>,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
        level: f32,
    ) {
        let lower = from.min(to);
        let direction = if to > from { "up" } else { "down" };
        let Some(a) = self.patch.spec.articulation(leg) else {
            return;
        };
        let dyns: Vec<String> = a
            .dynamics
            .iter()
            .filter(|d| only.is_none_or(|o| d.eq_ignore_ascii_case(o)))
            .cloned()
            .collect();
        let side = self.vibrato_side_gain(leg);
        for label in dyns {
            let g = side * self.group_curve_gain(leg, &label).unwrap_or(1.0) * level;
            if g <= 0.0 {
                continue;
            }
            let idx = self.patch.spec.zones.iter().position(|z| {
                z.articulation.eq_ignore_ascii_case(leg)
                    && z.direction.eq_ignore_ascii_case(direction)
                    && z.interval == interval
                    && z.dynamic.eq_ignore_ascii_case(&label)
                    && (z.key_min..=z.key_max).contains(&lower)
                    && self
                        .solo_mic
                        .as_ref()
                        .is_none_or(|m| z.mic.eq_ignore_ascii_case(m))
            });
            let Some(idx) = idx else { continue };
            // Tagged `to` (so the next transition retires it); pitched from
            // the LOWER note, which is where Kontakt maps the zone.
            let pitch = f64::from(lower) - f64::from(to);
            let offset = self.script_offset(idx, offset);
            if self.spawn_zone_voice_at(idx, to, VoiceKind::Legato, g, None, pitch, offset) {
                if let Some(v) = self.voices.last_spawned_mut() {
                    if let Some(e) = env.clone() {
                        v.set_flex_env(e);
                    }
                    match dip.clone() {
                        Some(KontaktDip::Curve(c)) => v.set_flex_env2(c),
                        Some(KontaktDip::Ramp(d, frames)) => {
                            let full = v.target_gain;
                            v.gain = full * d;
                            v.ramp_gain(full, frames);
                        }
                        None => {}
                    }
                }
            }
        }
    }
}

/// A transition layer's gain over time: the IOI dip `(dB, recover ms)`
/// recovering linearly to 0 dB, times the fade-out `curve_db` (evenly spaced
/// points) over `fade_ms` after `wait_ms`, then silence. `None` when neither
/// applies. Both shapes are piecewise linear in gain; their product is taken
/// at every breakpoint of either.
fn lt_gain_shape(
    ioi_dip: Option<(f32, f32)>,
    curve_db: &[f32],
    wait_ms: f32,
    fade_ms: f32,
    sample_rate: u32,
) -> Option<voice::FlexEnv> {
    let dip = ioi_dip.filter(|(db, _)| *db < 0.0);
    let fade = (!curve_db.is_empty() && fade_ms > 0.0).then_some(());
    if dip.is_none() && fade.is_none() {
        return None;
    }
    // (time ms, gain) breakpoints of each shape
    let dip_pts: Vec<(f32, f32)> = match dip {
        Some((db, rec)) => vec![(0.0, db_to_gain(db)), (rec.max(1.0), 1.0)],
        None => vec![(0.0, 1.0)],
    };
    let mut fade_pts: Vec<(f32, f32)> = vec![(0.0, 1.0)];
    if fade.is_some() {
        let n = curve_db.len();
        for (i, db) in curve_db.iter().enumerate() {
            let t = wait_ms + fade_ms * i as f32 / (n.max(2) - 1) as f32;
            fade_pts.push((t, db_to_gain(*db)));
        }
        fade_pts.push((wait_ms + fade_ms + 1.0, 0.0));
    }
    let at = |pts: &[(f32, f32)], t: f32| -> f32 {
        if t <= pts[0].0 {
            return pts[0].1;
        }
        for w in pts.windows(2) {
            if t <= w[1].0 {
                let span = (w[1].0 - w[0].0).max(1e-6);
                return w[0].1 + (w[1].1 - w[0].1) * (t - w[0].0) / span;
            }
        }
        pts[pts.len() - 1].1
    };
    let mut times: Vec<f32> = dip_pts.iter().chain(fade_pts.iter()).map(|p| p.0).collect();
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let start = at(&dip_pts, 0.0) * at(&fade_pts, 0.0);
    let mut segs = Vec::new();
    let mut prev = 0.0f32;
    for &t in times.iter().filter(|t| **t > 0.0) {
        segs.push((t - prev, at(&dip_pts, t) * at(&fade_pts, t), 0.5));
        prev = t;
    }
    if segs.is_empty() {
        segs.push((1.0, start, 0.5));
    }
    voice::FlexEnv::from_segments(&segs, start, sample_rate, false)
}
