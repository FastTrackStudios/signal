//! The classic guitar choruses — one configurable modulated-line engine,
//! voiced six ways.
//!
//! Every one of these boxes is, underneath, the same machine: the guitar
//! (mono, as every one of them took it) into a filtered, slightly saturating
//! delay line, read by one to three taps an LFO sweeps, the taps matrixed to
//! two outputs. What makes a CE-2 a CE-2 and a Small Clone a Small Clone is
//! the numbers — centre delay, how far the depth swings it, the LFO's shape,
//! where the filters sit, how the taps reach the two sides — so they are one
//! engine and a [`Spec`] each.
//!
//! Shared behaviour:
//! - **Rate** is the knob's Hz on every engine. **Depth** is a swing in ms on
//!   a per-engine taper (a pitch in cents on the SCF), always held under the
//!   mode's pitch ceiling by [`crate::dsp::tame_swing`], so full depth at full
//!   rate is a deep warble rather than a siren.
//! - **Colour** is the wet's tone on the BBD-style engines (noon is the unit
//!   as built), the pre-delay on the SCF, and the Lag (centre delay) on the
//!   Julia.
//! - **Feedback** is scaled per mode (a little in chorus, most of the way in
//!   flanger) and its average level gain is compensated, so turning it up
//!   changes the colour, not the volume.
//! - **Width** 0 is the unit's own mono output, 1 its full stereo spread;
//!   every spread is built from delay/phase differences between taps rather
//!   than polarity tricks, so the mono sum keeps the chorus.
//! - Flanger mode sweeps the delay exponentially (evenly spaced in pitch
//!   across the notches' travel); vibrato mode is the same line with the dry
//!   removed by the chain, on a sine (a triangle's square-wave pitch is a
//!   trill, not a vibrato).

use std::f64::consts::PI;

use crate::dsp::{ModLine, SvfLp, rounded_tri, rounded_tri_slope, soft_sat, tame_swing};
use crate::engine::{EffectType, EngineType, Frame, StereoEngine};

/// Samples between control updates (targets, filter coefficients). Delay
/// times are ramped linearly across each block, so nothing steps.
const CTL: usize = 16;

/// What the Colour knob does on an engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColorRole {
    /// The wet's lowpass, log-swept `tone.0 → tone.1 → tone.2`.
    Tone,
    /// Pre-delay: the centre delay the taps swing around.
    Predelay,
    /// Julia's Lag: the centre delay — and, as on a BBD whose clock the LFO
    /// swings by a fixed fraction, the swing scales with it.
    Lag,
}

/// Delay geometry for one effect mode.
#[derive(Clone, Copy, Debug)]
struct Geo {
    /// Centre delay, ms.
    centre_ms: f64,
    /// Swing at full depth: ±ms (chorus, vibrato) or ±octaves (flanger).
    swing: f64,
    /// Depth knob taper (power).
    taper: f64,
}

#[derive(Clone, Copy, Debug)]
struct Spec {
    taps: usize,
    /// LFO phase of each tap, in cycles.
    tap_phase: [f64; 3],
    /// Each tap's centre delay as a multiple of the mode's centre.
    tap_centre: [f64; 3],
    /// Rounded-triangle `r` (0 = sine, → 1 = triangle).
    lfo_r: f64,
    /// Depth of a slow secondary drift on each tap's centre, ±fraction.
    drift: f64,
    chorus: Geo,
    flanger: Geo,
    vibrato: Geo,
    color: ColorRole,
    /// Wet lowpass, Hz, at Colour 0 / 0.5 / 1 (or fixed when Colour is not
    /// tone).
    tone: [f64; 3],
    /// Input (anti-alias) lowpass as a multiple of the wet lowpass; 0 = none.
    pre_ratio: f64,
    /// A second 2-pole stage on the wet (4-pole total).
    four_pole: bool,
    /// Wet high-pass, Hz (the coupling caps on a pedal's BBD path — the CE-2's
    /// passband starts at ≈15 Hz). Kept that low on purpose: a thinner wet
    /// is a level drop on a guitar's low strings.
    low_cut: f64,
    /// Bucket saturation (`soft_sat` k); 0 = clean digital line.
    sat: f64,
    /// Depth is pitch (cents at full depth, chorus / vibrato) rather than
    /// delay swing — the SCF's pitch-modulation mode.
    pitch_mode: Option<(f64, f64)>,
    /// Stereo matrix at full width, per tap, for L and R.
    st_l: [f64; 3],
    st_r: [f64; 3],
    /// The unit's own mono output, per tap.
    mono: [f64; 3],
}

const fn geo(centre_ms: f64, swing: f64, taper: f64) -> Geo {
    Geo {
        centre_ms,
        swing,
        taper,
    }
}

/// The six voicings. Sources for the numbers are in the notes on each.
#[expect(clippy::too_many_lines, reason = "one data table: six units' voicings")]
fn spec(kind: EngineType) -> Spec {
    match kind {
        // Boss CE-2 (1979): MN3007 (1024 stages) on an MN3101 clock, a
        // Schmitt-trigger/integrator triangle LFO (≈0.3–3.5 Hz on the
        // pedal), the wet through anti-alias poles at 6.6/6.9 kHz and summed
        // with the dry at a fixed level (ElectroSmash, Anasounds). The delay
        // sits around 8.5–10.75 ms; the top of this depth runs a little past
        // that (±2 ms, ≈48 cents at 3.5 Hz). The warmth is that steep top
        // cut. Its own output is mono; full width puts a second tap on the
        // inverted LFO on the right (the CE-3's stereo idea, done with delay
        // rather than its dry−wet polarity so the mono sum keeps the chorus).
        EngineType::Ce2 => Spec {
            taps: 2,
            tap_phase: [0.0, 0.5, 0.0],
            tap_centre: [1.0, 1.0, 1.0],
            lfo_r: 0.97,
            drift: 0.0,
            chorus: geo(9.0, 2.0, 1.2),
            flanger: geo(1.6, 1.6, 1.0),
            // CE-1 vibrato: ≈3–7 ms, faster (3–11 Hz) — see the ceiling.
            vibrato: geo(5.0, 2.0, 1.1),
            color: ColorRole::Tone,
            tone: [3_500.0, 6_200.0, 11_000.0],
            pre_ratio: 1.3,
            four_pole: true,
            low_cut: 18.0,
            sat: 0.06,
            pitch_mode: None,
            st_l: [1.0, 0.0, 0.0],
            st_r: [0.0, 1.0, 0.0],
            mono: [1.0, 0.0, 0.0],
        },
        // Roland SDD-320 Dimension D: two MN3007 lines (each with an NE570
        // compander — i.e. quiet) on one near-triangle LFO, the second
        // inverted, so one is at its longest when the other is at its
        // shortest and the average delay never moves. Each output is its own
        // line plus the other's, high-passed and polarity-inverted — the
        // high-pass keeps the lows from cancelling in mono. Equal and
        // opposite pitch: space, no sweep. Modes 1→4 switched LFO depth (and
        // rate); here Depth runs through them continuously, from barely there
        // (±0.3 ms, the "5 vs 5.5 ms" of modes 2–4) to past mode 4.
        EngineType::Dimension => Spec {
            taps: 2,
            tap_phase: [0.0, 0.5, 0.0],
            tap_centre: [1.0, 1.0, 1.0],
            lfo_r: 0.99,
            drift: 0.0,
            chorus: geo(5.5, 1.6, 1.0),
            flanger: geo(1.4, 1.2, 1.0),
            vibrato: geo(5.0, 2.4, 1.0),
            color: ColorRole::Tone,
            tone: [4_000.0, 9_000.0, 16_000.0],
            pre_ratio: 1.3,
            four_pole: false,
            low_cut: 18.0,
            sat: 0.03,
            pitch_mode: None,
            // Plus the high-passed, inverted cross-feed (see `tick`).
            st_l: [1.0, 0.0, 0.0],
            st_r: [0.0, 1.0, 0.0],
            mono: [0.5, 0.5, 0.0],
        },
        // EHX Small Clone: one MN3007/MN3207 (1024 stages), Rate up to
        // ≈9 Hz, a two-position Depth switch (mono, no mix knob). A longer,
        // deeper sweep and a darker wet than the CE-2 — the switch's "deep"
        // is the top of this knob, and "Nevermind" is depth up, rate around
        // 1–1.5 Hz: ≈50–70 cents of swirl. A rounder LFO than the Boss.
        EngineType::Clone => Spec {
            taps: 2,
            tap_phase: [0.0, 0.5, 0.0],
            tap_centre: [1.0, 1.0, 1.0],
            lfo_r: 0.85,
            drift: 0.0,
            chorus: geo(8.0, 5.0, 1.2),
            flanger: geo(1.8, 2.0, 1.0),
            vibrato: geo(8.0, 5.0, 1.2),
            color: ColorRole::Tone,
            tone: [2_600.0, 5_000.0, 9_500.0],
            pre_ratio: 1.4,
            four_pole: true,
            low_cut: 18.0,
            sat: 0.08,
            pitch_mode: None,
            st_l: [1.0, 0.0, 0.0],
            st_r: [0.0, 1.0, 0.0],
            mono: [1.0, 0.0, 0.0],
        },
        // Three-voice rack chorus (Dyno My Piano / TC 1210 / Eventide
        // TriceraChorus / Strymon Ola "multi"): one three-phase LFO,
        // left / centre / right at −120° / 0° / +120°, at staggered centres,
        // each drifting slowly so the three never lock into one repeating
        // pattern. Three voices 120° apart cancel most of the net pitch
        // swing — a high wet mix without the warble.
        EngineType::TriChorus => Spec {
            taps: 3,
            tap_phase: [0.0, 1.0 / 3.0, 2.0 / 3.0],
            tap_centre: [0.8, 1.0, 1.25],
            lfo_r: 0.0,
            drift: 0.08,
            chorus: geo(9.0, 3.0, 1.4),
            flanger: geo(1.6, 1.5, 1.0),
            vibrato: geo(6.0, 3.5, 1.2),
            color: ColorRole::Tone,
            tone: [5_000.0, 12_000.0, 20_000.0],
            pre_ratio: 0.0,
            four_pole: false,
            low_cut: 18.0,
            sat: 0.0,
            pitch_mode: None,
            st_l: [1.0, 0.5, 0.0],
            st_r: [0.0, 0.5, 1.0],
            mono: [1.0, 1.0, 1.0],
        },
        // After the TC Electronic SCF / 2290 school: bright, clean, full-band,
        // wide. Depth is a pitch deviation held constant as the speed moves
        // (0.1–10 Hz on the unit) — the sweep shrinks as the rate rises, so a
        // fast SCF shimmers instead of warbling. The SCF puts the effect in
        // opposite polarity L/R; here the two sides swing in antiphase
        // instead, which sums to mono. Colour is the pre-delay (the 2290's
        // long ~20 ms chorus at the top).
        EngineType::Scf => Spec {
            taps: 2,
            tap_phase: [0.0, 0.5, 0.0],
            tap_centre: [1.0, 1.0, 1.0],
            lfo_r: 0.0,
            drift: 0.0,
            chorus: geo(7.0, 12.0, 1.0),
            flanger: geo(1.5, 1.5, 1.0),
            vibrato: geo(2.0, 12.0, 1.0),
            color: ColorRole::Predelay,
            tone: [16_000.0, 16_000.0, 16_000.0],
            pre_ratio: 0.0,
            four_pole: false,
            low_cut: 18.0,
            sat: 0.0,
            pitch_mode: Some((32.0, 90.0)),
            st_l: [1.0, 0.0, 0.0],
            st_r: [0.0, 1.0, 0.0],
            mono: [1.0, 0.0, 0.0],
        },
        // Walrus Julia: analogue BBD chorus/vibrato. The mix knob is its
        // D-C-V control (dry → 50/50 chorus at noon → all-wet vibrato),
        // Colour its Lag — the centre delay, "from smooth tight modulation to
        // noisy nauseating detune": the swing is a fraction of the centre,
        // as it is when the LFO swings a BBD clock, so more lag is more
        // detune.
        _ => Spec {
            taps: 2,
            tap_phase: [0.0, 0.5, 0.0],
            tap_centre: [1.0, 1.0, 1.0],
            lfo_r: 0.97,
            drift: 0.0,
            // Swing here is the fraction of the centre at full depth.
            chorus: geo(6.0, 0.45, 1.2),
            flanger: geo(1.6, 1.8, 1.0),
            vibrato: geo(4.0, 0.5, 1.1),
            color: ColorRole::Lag,
            tone: [5_800.0, 5_800.0, 5_800.0],
            pre_ratio: 1.5,
            four_pole: true,
            low_cut: 18.0,
            sat: 0.06,
            pitch_mode: None,
            st_l: [1.0, 0.0, 0.0],
            st_r: [0.0, 1.0, 0.0],
            mono: [1.0, 0.0, 0.0],
        },
    }
}

/// Log interpolation through three points at 0, 0.5, 1.
fn log3(points: [f64; 3], pos: f64) -> f64 {
    let pos = pos.clamp(0.0, 1.0);
    let (from, to, frac) = if pos < 0.5 {
        (points[0], points[1], pos * 2.0)
    } else {
        (points[1], points[2], pos.mul_add(2.0, -1.0))
    };
    from * (to / from).powf(frac)
}

/// Normalise a row of tap gains. The taps are the same guitar a few ms
/// apart: coherent in the lows (they add in amplitude), independent in the
/// highs (in power) — so the row's level is taken halfway between the two.
/// (Pure power normalisation put a half-width CE-2 2.5 dB up on the side
/// that blends its two taps.)
fn norm_row(row: [f64; 3]) -> [f64; 3] {
    let pow: f64 = row.iter().map(|g| g * g).sum();
    let amp: f64 = row.iter().map(|g| g.abs()).sum();
    let scale = 1.0 / 0.5f64.mul_add(amp * amp, 0.5 * pow).max(1.0e-9).sqrt();
    [row[0] * scale, row[1] * scale, row[2] * scale]
}

/// One of the classic chorus engines.
pub struct Classic {
    kind: EngineType,
    spec: Spec,
    sr: f64,
    /// One line per side, read at the same (shared) tap delays: a centred
    /// input fills both alike — the unit exactly as it was — and a panned one
    /// stays where it was panned, where one line fed the sum would put its
    /// chorus in both sides.
    line: ModLine,
    line_r: ModLine,
    pre: [SvfLp; 2],
    pre_r: [SvfLp; 2],
    post_l: [SvfLp; 2],
    post_r: [SvfLp; 2],
    hp_l: f64,
    hp_r: f64,
    hp_k: f64,
    phase: f64,
    drift_phase: f64,
    fb_state: f64,
    fb_state_r: f64,
    // Dimension's inverted cross-feed: amount and the two high-passes' state
    // (per side's line).
    cross: f64,
    cross_hp: [f64; 2],
    cross_hp_r: [f64; 2],
    cross_k: f64,
    ctl: usize,
    // Each tap's delay (samples) and its per-sample ramp to the end of the
    // control block.
    delay: [f64; 3],
    delay_step: [f64; 3],
    flanger: bool,
    // Control-rate values.
    gl: [f64; 3],
    gr: [f64; 3],
    fb_gain: f64,
    wet_gain: f64,
    shape_r: f64,
    primed: bool,
    last_delay_ms: f64,
}

impl Classic {
    #[must_use]
    pub fn new(kind: EngineType) -> Self {
        let mut s = Self {
            kind,
            spec: spec(kind),
            sr: 48_000.0,
            line: ModLine::new(48_000 / 20),
            line_r: ModLine::new(48_000 / 20),
            pre: [SvfLp::default(); 2],
            pre_r: [SvfLp::default(); 2],
            post_l: [SvfLp::default(); 2],
            post_r: [SvfLp::default(); 2],
            hp_l: 0.0,
            hp_r: 0.0,
            hp_k: 0.0,
            phase: 0.0,
            drift_phase: 0.0,
            fb_state: 0.0,
            fb_state_r: 0.0,
            cross: 0.0,
            cross_hp: [0.0; 2],
            cross_hp_r: [0.0; 2],
            cross_k: 0.0,
            ctl: 0,
            delay: [0.0; 3],
            delay_step: [0.0; 3],
            flanger: false,
            gl: [0.0; 3],
            gr: [0.0; 3],
            fb_gain: 0.0,
            wet_gain: 1.0,
            shape_r: 0.0,
            primed: false,
            last_delay_ms: 0.0,
        };
        s.update(48_000.0);
        s
    }

    /// Control-rate update: targets for the next `CTL` samples.
    fn control(&mut self, frame: &Frame) {
        let (centre_ms, swing, flanger) = self.sweep(frame);
        self.retarget_taps(frame, centre_ms, swing, flanger);
        self.voice_filters(frame);
        self.voice_outputs(frame);
    }

    /// The sweep for this block: centre delay (ms), swing (samples, or
    /// octaves on a flanger) and whether it is the flanger's exponential
    /// sweep. Also sets the LFO shape.
    fn sweep(&mut self, frame: &Frame) -> (f64, f64, bool) {
        let sp = self.spec;
        let depth = frame.depth.clamp(0.0, 1.0);
        let color = frame.color.clamp(0.0, 1.0);
        let rate = frame.rate_hz.max(1.0e-3);
        let vibrato = frame.effect == EffectType::Vibrato;
        let mode = match frame.effect {
            EffectType::Chorus => sp.chorus,
            EffectType::Flanger => sp.flanger,
            EffectType::Vibrato => sp.vibrato,
        };
        let ceiling = frame.effect.ceiling_cents();

        // LFO shape: the engine's own, but a sine in vibrato mode.
        self.shape_r = if vibrato { 0.0 } else { sp.lfo_r };
        let slope = rounded_tri_slope(self.shape_r);

        if frame.effect == EffectType::Flanger {
            // ± octaves around the centre; the pitch is held under the
            // ceiling by taming the equivalent linear swing.
            let oct = mode.swing * depth.powf(mode.taper);
            let lin = mode.centre_ms * 0.001 * (oct.exp2() - (-oct).exp2()) * 0.5;
            let tamed = if lin > 0.0 {
                tame_swing(lin, rate, slope, ceiling) / lin
            } else {
                1.0
            };
            return (mode.centre_ms, oct * tamed, true);
        }

        let mut centre_ms = mode.centre_ms;
        let swing_s = if let Some((c_chorus, c_vib)) = sp.pitch_mode {
            // Pitch mode: depth is cents; the swing follows the rate.
            let cents = if vibrato { c_vib } else { c_chorus } * depth.powf(mode.taper);
            ((cents / 1200.0).exp2() - 1.0) / (slope * rate)
        } else {
            let full_ms = if sp.color == ColorRole::Lag {
                // Lag 2 → 6 → 16 ms; the swing a fraction of it.
                centre_ms = log3([2.0, 6.0, 16.0], color);
                mode.swing * centre_ms
            } else {
                mode.swing
            };
            // The Dimension never stops: its depth walks mode 1 (±0.3 ms)
            // to past mode 4.
            let shaped = if self.kind == EngineType::Dimension {
                let floor = 0.3 / full_ms.max(0.3);
                (1.0 - floor).mul_add(depth, floor)
            } else {
                depth.powf(mode.taper)
            };
            tame_swing(full_ms * 0.001 * shaped, rate, slope, ceiling)
        };
        let swing_s = if sp.pitch_mode.is_some() {
            swing_s.min(mode.swing * 0.001)
        } else {
            swing_s
        };
        if sp.color == ColorRole::Predelay {
            // Pre-delay 1.5 → 7 → 25 ms (a vibrato's 0.5 → 1.5 → 6: it has
            // no dry to be late against), the sweep riding on top.
            let pre = if vibrato {
                [0.5, 1.5, 6.0]
            } else {
                [1.5, 7.0, 25.0]
            };
            centre_ms = swing_s.mul_add(1000.0, log3(pre, color));
        } else if vibrato {
            // Vibrato: as short as the swing allows (less smear).
            centre_ms = swing_s.mul_add(1000.0, 1.0).max(centre_ms * 0.5);
        }
        // Never let a tap swing through its floor.
        let min_mult = sp.tap_centre[..sp.taps]
            .iter()
            .copied()
            .fold(f64::MAX, f64::min);
        let centre_ms = centre_ms.max(swing_s.mul_add(1000.0, 0.6) / min_mult);
        (centre_ms, swing_s * self.sr, false)
    }

    /// Advance the LFO to the end of this block and aim each tap's delay
    /// there. The delays are ramped to linearly, sample by sample: at LFO
    /// rates the piecewise-linear sweep is indistinguishable from the curve,
    /// and it keeps the transcendental maths off the per-sample path.
    fn retarget_taps(&mut self, frame: &Frame, centre_ms: f64, swing: f64, flanger: bool) {
        let sp = self.spec;
        let sr = self.sr;
        let rate = frame.rate_hz.max(1.0e-3);
        let block = CTL as f64;
        self.phase = rate.mul_add(block / sr, self.phase).fract();
        // A slow secondary drift on each tap's centre (Tri-Chorus), at a
        // rate of its own so it adds little pitch however fast the LFO is.
        let drift_inc = 0.02f64.mul_add(rate, 0.11) / sr * block;
        self.drift_phase = (self.drift_phase + drift_inc).fract();

        let base = self.lfo(self.phase);
        for tap in 0..sp.taps {
            let drift = if sp.drift > 0.0 {
                let ph = (self.drift_phase + tap as f64 / 3.0) * 2.0 * PI;
                sp.drift.mul_add(ph.sin(), 1.0)
            } else {
                1.0
            };
            // Antiphase taps share the (odd) shape: no second evaluation.
            let offset = sp.tap_phase[tap];
            let lfo = if offset.abs() < 1.0e-9 {
                base
            } else if (offset - 0.5).abs() < 1.0e-9 {
                -base
            } else {
                self.lfo((self.phase + offset).fract())
            };
            let centre = centre_ms * sp.tap_centre[tap] * drift * 0.001 * sr;
            let target = if flanger {
                // Exponential sweep: ± octaves.
                centre * (swing * lfo).exp2()
            } else {
                swing.mul_add(lfo, centre)
            };
            if self.primed && flanger == self.flanger {
                self.delay_step[tap] = (target - self.delay[tap]) / block;
            } else {
                self.delay[tap] = target;
                self.delay_step[tap] = 0.0;
            }
        }
        self.flanger = flanger;
        self.primed = true;
    }

    /// The anti-alias and wet filters.
    fn voice_filters(&mut self, frame: &Frame) {
        let sp = self.spec;
        let sr = self.sr;
        let tone = if sp.color == ColorRole::Tone {
            log3(sp.tone, frame.color.clamp(0.0, 1.0))
        } else {
            sp.tone[1]
        };
        if sp.pre_ratio > 0.0 {
            for p in self.pre.iter_mut().chain(self.pre_r.iter_mut()) {
                p.set(tone * sp.pre_ratio, 0.6, sr);
            }
        }
        // Two stages: a 4-pole Butterworth (Q 0.54 / 1.31), or one 2-pole.
        let (q1, q2) = if sp.four_pole {
            (0.541, 1.307)
        } else {
            (0.707, 0.707)
        };
        self.post_l[0].set(tone, q1, sr);
        self.post_r[0].set(tone, q1, sr);
        self.post_l[1].set(tone, q2, sr);
        self.post_r[1].set(tone, q2, sr);
    }

    /// The stereo matrix, the cross-feed and the feedback.
    fn voice_outputs(&mut self, frame: &Frame) {
        let sp = self.spec;
        let depth = frame.depth.clamp(0.0, 1.0);
        // A vibrato is one voice, the same pitch on both sides: two taps in
        // antiphase (or three at 120°) would sum — on a mono rig, or in the
        // room — to a chorus.
        let vibrato = frame.effect == EffectType::Vibrato;
        let (st_l, st_r, mono) = if vibrato {
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
        } else {
            (sp.st_l, sp.st_r, sp.mono)
        };
        let width = if vibrato {
            0.0
        } else {
            frame.width.clamp(0.0, 1.0)
        };
        if self.kind == EngineType::Dimension {
            // Depth walks the four buttons: the inverted cross-feed from
            // 0.12 to 0.3 of the other line. It is part of the stereo image,
            // so it narrows with the width.
            self.cross = 0.18f64.mul_add(depth, 0.12) * width;
        }
        let mono = norm_row(mono);
        let (st_l, st_r) = (norm_row(st_l), norm_row(st_r));
        for tap in 0..3 {
            self.gl[tap] = (st_l[tap] - mono[tap]).mul_add(width, mono[tap]);
            self.gr[tap] = (st_r[tap] - mono[tap]).mul_add(width, mono[tap]);
        }
        self.gl = norm_row(self.gl);
        self.gr = norm_row(self.gr);

        // Feedback: a little in chorus, most of the way in flanger. The
        // loop lifts the lows it reinforces by up to 1/(1−g) and the average
        // by 1/√(1−g²); (1−g)^0.35 sits between and holds the level within
        // ~1.5 dB from 0 to full feedback on both pink noise and a guitar.
        let fb_max = match frame.effect {
            EffectType::Chorus => 0.55,
            EffectType::Flanger => 0.92,
            EffectType::Vibrato => 0.3,
        };
        let fb = frame.feedback.clamp(0.0, 1.0) * fb_max;
        self.fb_gain = fb;
        self.wet_gain = (1.0 - fb).powf(0.35);
        if vibrato {
            // One tap, no pair to average: 0.8 dB less than the chorus's
            // makeup (the Dimension's, set on its cross-fed pair, 0.4 dB).
            self.wet_gain *= if self.kind == EngineType::Dimension {
                0.95
            } else {
                0.91
            };
        }
    }

    #[inline]
    fn lfo(&self, phase: f64) -> f64 {
        rounded_tri(phase, self.shape_r)
    }
}

impl StereoEngine for Classic {
    fn update(&mut self, sample_rate: f64) {
        self.sr = sample_rate;
        // Longest read: SCF's 25 ms pre-delay + 12 ms swing, Tri-Chorus's
        // 9 ms × 1.25 × 1.12 + swing — 45 ms covers all.
        self.line.ensure((sample_rate * 0.045) as usize);
        self.line_r.ensure((sample_rate * 0.045) as usize);
        self.hp_k = 1.0 - (-2.0 * PI * self.spec.low_cut / sample_rate).exp();
        // The cross-feed's high-pass, ~250 Hz.
        self.cross_k = 1.0 - (-2.0 * PI * 250.0 / sample_rate).exp();
        self.primed = false;
        self.ctl = 0;
    }

    fn reset(&mut self) {
        self.line.clear();
        self.line_r.clear();
        for f in self
            .pre
            .iter_mut()
            .chain(self.pre_r.iter_mut())
            .chain(self.post_l.iter_mut())
            .chain(self.post_r.iter_mut())
        {
            f.reset();
        }
        self.hp_l = 0.0;
        self.hp_r = 0.0;
        self.phase = 0.0;
        self.drift_phase = 0.0;
        self.fb_state = 0.0;
        self.fb_state_r = 0.0;
        self.cross_hp = [0.0; 2];
        self.cross_hp_r = [0.0; 2];
        self.primed = false;
        self.ctl = 0;
    }

    fn tick(&mut self, in_l: f64, in_r: f64, frame: &Frame) -> (f64, f64) {
        if self.ctl == 0 {
            self.control(frame);
        }
        self.ctl = (self.ctl + 1) % CTL;
        let sp = self.spec;

        // Into the lines — one a side, each through the anti-alias filter
        // and the buckets' soft ceiling, with its own feedback. The unit took
        // one input; each side here is that unit fed that side, so a centred
        // guitar (both sides alike) is the unit exactly and a panned one
        // keeps its pan.
        let into = |x: f64, fb_state: f64, pre: &mut [SvfLp; 2]| {
            let fb = soft_sat(fb_state * self.fb_gain, 0.4);
            let mut v = x + fb;
            if sp.pre_ratio > 0.0 {
                let once = pre[0].tick(v);
                v = pre[1].tick(once);
            }
            if sp.sat > 0.0 {
                v = soft_sat(v, sp.sat);
            }
            v
        };
        let (fl, fr) = (self.fb_state, self.fb_state_r);
        let il = into(in_l, fl, &mut self.pre);
        let ir = into(in_r, fr, &mut self.pre_r);
        self.line.write(il);
        self.line_r.write(ir);

        // Taps, at the same delays on both lines.
        let (mut tl, mut tr) = ([0.0; 3], [0.0; 3]);
        for k in 0..sp.taps.min(3) {
            self.delay[k] += self.delay_step[k];
            tl[k] = self.line.read(self.delay[k]);
            tr[k] = self.line_r.read(self.delay[k]);
        }
        self.last_delay_ms = self.delay[0] * 1000.0 / self.sr;
        self.fb_state = tl[0];
        self.fb_state_r = tr[0];

        let mut wl = self.gl[0].mul_add(tl[0], self.gl[1].mul_add(tl[1], self.gl[2] * tl[2]));
        let mut wr = self.gr[0].mul_add(tr[0], self.gr[1].mul_add(tr[1], self.gr[2] * tr[2]));
        if self.cross > 0.0 {
            // Each side less the other's highs — the other tap of its own
            // side's line.
            let hp = |t: &[f64; 3], st: &mut [f64; 2], k: f64| {
                st[0] = (t[0] - st[0]).mul_add(k, st[0]);
                st[1] = (t[1] - st[1]).mul_add(k, st[1]);
                (t[0] - st[0], t[1] - st[1])
            };
            let (_, h1) = hp(&tl, &mut self.cross_hp, self.cross_k);
            let (h0, _) = hp(&tr, &mut self.cross_hp_r, self.cross_k);
            // The two lines are close copies (the swing is small), so the
            // inverted feed partly cancels; 1/(1−k/2) puts the channels back
            // about as far above the dry as the mono sum sits below it
            // (measured on pink noise, plucked chords and a real guitar).
            let g = 1.0 / 0.5f64.mul_add(-self.cross, 1.0);
            wl = self.cross.mul_add(-h1, wl) * g;
            wr = self.cross.mul_add(-h0, wr) * g;
        }
        wl = self.post_l[0].tick(wl);
        wr = self.post_r[0].tick(wr);
        if sp.four_pole {
            wl = self.post_l[1].tick(wl);
            wr = self.post_r[1].tick(wr);
        }
        // Low cut: subtract a one-pole lowpass.
        self.hp_l = (wl - self.hp_l).mul_add(self.hp_k, self.hp_l);
        self.hp_r = (wr - self.hp_r).mul_add(self.hp_k, self.hp_r);
        (
            (wl - self.hp_l) * self.wet_gain,
            (wr - self.hp_r) * self.wet_gain,
        )
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }
}
