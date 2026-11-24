//! `level`: the vocal leveler as an offline block.
//!
//! **Analysis** ([`plan`], feature `offline`): one pass over the vocal
//! (the dsp crate's `level::envelope`) makes four lanes — gate, breath,
//! ride, de-ess — and the ride is to one level for the whole track, read a
//! phrase at a time, then the makeup that puts the leveled vocal's sung
//! level where its louder lines were sung. The gate and the breath window
//! are set from each recording's own noise floor. This is Studio's
//! `VOCAL_LEVEL`, curve for curve (a test there holds them equal).
//!
//! **Apply** ([`LevelApply`]): plays the lanes. Gate, breath and ride sum
//! in dB into one full-band gain; the de-ess lane is the gain of a band
//! around the "ess" (`ess_hz`, `ess_q`), because sibilance is a band, and a
//! full-band duck takes the vowel down with it. Each lane's `*_amount`
//! scales it as it plays (the leveler's dials: 0 off, 100 % as made).
//!
//! A parameter change ramps, in dB, over the next piece, so a graph that
//! ends each piece at a lane's breakpoint (its own playback does) plays
//! the lane exactly; a piece of length 0 jumps there (a seek).

use crate::block::BlockProcessor;
use crate::param::{ParamSpec, Unit};

/// The block's name.
pub const NAME: &str = "level";

pub const GATE: usize = 0;
pub const BREATH: usize = 1;
pub const RIDE: usize = 2;
pub const ESS: usize = 3;
pub const GATE_AMOUNT: usize = 4;
pub const BREATH_AMOUNT: usize = 5;
pub const RIDE_AMOUNT: usize = 6;
pub const ESS_AMOUNT: usize = 7;
pub const ESS_HZ: usize = 8;
pub const ESS_Q: usize = 9;

/// The lanes an analysis writes, in the order of their parameters.
pub const LANES: [usize; 4] = [GATE, BREATH, RIDE, ESS];

pub static PARAMS: [ParamSpec; 10] = [
    ParamSpec::new("gate", "Gate", Unit::Db, -60.0, 0.0, 0.0),
    ParamSpec::new("breath", "De-Breath", Unit::Db, -40.0, 0.0, 0.0),
    ParamSpec::new("ride", "Ride", Unit::Db, -48.0, 48.0, 0.0),
    ParamSpec::new("ess", "De-Ess", Unit::Db, -24.0, 0.0, 0.0),
    ParamSpec::new("gate_amount", "Gate Amount", Unit::Percent, 0.0, 2.0, 1.0),
    ParamSpec::new(
        "breath_amount",
        "De-Breath Amount",
        Unit::Percent,
        0.0,
        2.0,
        1.0,
    ),
    ParamSpec::new("ride_amount", "Ride Amount", Unit::Percent, 0.0, 2.0, 1.0),
    ParamSpec::new("ess_amount", "De-Ess Amount", Unit::Percent, 0.0, 2.0, 1.0),
    ParamSpec::new(
        "ess_hz",
        "De-Ess Frequency",
        Unit::Hz,
        2000.0,
        12000.0,
        7000.0,
    ),
    ParamSpec::new("ess_q", "De-Ess Q", Unit::Plain(2), 0.3, 4.0, 1.0),
];

/// A ramp in dB, from where it is to a target, over one piece.
#[derive(Clone, Copy, Debug, Default)]
struct Ramp {
    now: f64,
    target: f64,
}

impl Ramp {
    /// The linear gain at the piece's start and its per-sample ratio.
    fn piece(&self, len: usize) -> (f64, f64) {
        let g = libm::pow(10.0, self.now / 20.0);
        if len == 0 || self.now == self.target {
            return (g, 1.0);
        }
        #[allow(clippy::cast_precision_loss)]
        let step = (self.target - self.now) / (20.0 * len as f64);
        (g, libm::pow(10.0, step))
    }
}

/// A band-pass biquad (RBJ, 0 dB peak), transposed direct form II.
#[derive(Clone, Copy, Debug, Default)]
struct Band {
    b0: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s: [[f64; 2]; 2],
}

impl Band {
    fn design(&mut self, sample_rate: f64, hz: f64, q: f64) {
        let w = 2.0 * core::f64::consts::PI * (hz / sample_rate).clamp(1e-6, 0.49);
        let alpha = libm::sin(w) / (2.0 * q.max(0.05));
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * libm::cos(w) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let Some(s) = self.s.get_mut(ch) else {
            return 0.0;
        };
        let y = self.b0.mul_add(x, s[0]);
        s[0] = self.a1.mul_add(-y, s[1]);
        s[1] = self.b2.mul_add(x, -self.a2 * y);
        y
    }
}

/// The apply stage.
pub struct LevelApply {
    sample_rate: f64,
    values: [f32; 10],
    full: Ramp,
    ess: Ramp,
    band: Band,
}

impl LevelApply {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sample_rate: f64::from(sample_rate.max(1.0)),
            values: PARAMS.map(|p| p.default),
            full: Ramp::default(),
            ess: Ramp::default(),
            band: Band::default(),
        };
        s.redesign();
        s.retarget();
        s.full.now = s.full.target;
        s.ess.now = s.ess.target;
        s
    }

    fn v(&self, i: usize) -> f64 {
        f64::from(self.values.get(i).copied().unwrap_or(0.0))
    }

    fn retarget(&mut self) {
        self.full.target = self.v(GATE).mul_add(
            self.v(GATE_AMOUNT),
            self.v(BREATH)
                .mul_add(self.v(BREATH_AMOUNT), self.v(RIDE) * self.v(RIDE_AMOUNT)),
        );
        self.ess.target = self.v(ESS) * self.v(ESS_AMOUNT);
    }

    fn redesign(&mut self) {
        let (hz, q) = (self.v(ESS_HZ), self.v(ESS_Q));
        self.band.design(self.sample_rate, hz, q);
    }

    /// The full-band gain (dB) and the band's (dB) it plays now.
    #[must_use]
    pub const fn gains_db(&self) -> (f64, f64) {
        (self.full.now, self.ess.now)
    }
}

impl BlockProcessor for LevelApply {
    fn params(&self) -> &'static [ParamSpec] {
        &PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(slot) = self.values.get_mut(index) else {
            return;
        };
        if *slot == value {
            return;
        }
        *slot = value;
        if index == ESS_HZ || index == ESS_Q {
            self.redesign();
        } else {
            self.retarget();
        }
    }

    fn reset(&mut self) {
        self.band.s = [[0.0; 2]; 2];
        self.full.now = self.full.target;
        self.ess.now = self.ess.target;
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let (mut g, gr) = self.full.piece(len);
        let (mut e, er) = self.ess.piece(len);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (xl, xr) = (f64::from(*l), f64::from(*r));
            let (bl, br) = (self.band.tick(0, xl), self.band.tick(1, xr));
            let k = e - 1.0;
            #[allow(clippy::cast_possible_truncation)]
            {
                *l = (g * k.mul_add(bl, xl)) as f32;
                *r = (g * k.mul_add(br, xr)) as f32;
            }
            g *= gr;
            e *= er;
        }
        // Where the piece ends is where the ramp was going: exact, with no
        // drift from the running product.
        self.full.now = self.full.target;
        self.ess.now = self.ess.target;
    }
}

#[cfg(feature = "offline")]
pub use analysis::*;

#[cfg(feature = "offline")]
mod analysis {
    //! The analysis: Studio's `VOCAL_LEVEL`, over the audio arriving at
    //! the block.

    use alloc::vec::Vec;

    use signal_dsp::level::envelope::{self as env, Contributions, EnvPoint};

    use super::super::{Curve, EDGE_S, Range};

    /// How much of a phrase a ride hears, seconds.
    pub const PHRASE_S: f64 = 0.5;
    /// How far above a recording's noise floor its gate opens, dB.
    pub const GATE_ABOVE_DB: f64 = 10.0;
    /// How far a shut gate takes the noise down, dB.
    pub const GATE_FLOOR_DB: f64 = -30.0;
    /// How far under the song's level a breath is, dB.
    pub const BREATH_UNDER_DB: f64 = 18.0;
    /// How far a breath is taken down, dB.
    pub const BREATH_DB: f64 = 10.0;
    /// How far an "ess" is eased, dB.
    pub const ESS_DB: f64 = 4.0;
    /// The quiet lines' share of the sung phrases.
    pub const QUIET_SHARE: f64 = 0.1;
    /// The loud lines' share: where the makeup puts the track.
    pub const LOUD_SHARE: f64 = 0.9;
    /// The most the makeup moves the whole track, dB.
    pub const MAKEUP_MAX_DB: f64 = 24.0;

    /// The knobs a hand turns.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Knobs {
        /// How far each line is brought to the song's level, 0..1.
        pub consistency: f64,
        /// The most a line is raised or lowered, dB.
        pub range_db: f64,
        /// The level to ride to (dBFS); the track's own when `None`.
        pub target_db: Option<f64>,
    }

    impl Default for Knobs {
        fn default() -> Self {
            Self {
                consistency: 0.9,
                range_db: 12.0,
                target_db: None,
            }
        }
    }

    /// What each lane is scaled by as it plays (the `*_amount`
    /// parameters): the makeup is measured through them.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Amounts {
        pub gate: f64,
        pub breath: f64,
        pub ride: f64,
        pub ess: f64,
    }

    impl Default for Amounts {
        fn default() -> Self {
            Self {
                gate: 1.0,
                breath: 1.0,
                ride: 1.0,
                ess: 1.0,
            }
        }
    }

    /// A stretch of the audio arriving at the block: mono (a dual-capsule
    /// vocal's front capsule), starting at `start_s` on the song.
    pub struct Segment<'a> {
        pub start_s: f64,
        pub samples: &'a [f64],
        pub sample_rate: f64,
    }

    /// The track-wide levels an analysis settled on, kept so an analysis
    /// of part of the song rides to the same place.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Levels {
        /// The level every line is ridden toward, dBFS.
        pub target_db: f64,
        /// Where the makeup put the leveled track's sung level, dBFS.
        pub level_db: f64,
        /// The gain over the whole track that brought it there, dB.
        pub makeup_db: f64,
    }

    /// The four lanes, on the song's time, in dB.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Lanes {
        pub gate: Curve,
        pub breath: Curve,
        pub ride: Curve,
        pub ess: Curve,
    }

    impl Lanes {
        /// By the block's parameter ids.
        #[must_use]
        pub fn by_param(&self) -> [(&'static str, &Curve); 4] {
            [
                (super::PARAMS[super::GATE].id, &self.gate),
                (super::PARAMS[super::BREATH].id, &self.breath),
                (super::PARAMS[super::RIDE].id, &self.ride),
                (super::PARAMS[super::ESS].id, &self.ess),
            ]
        }
    }

    /// An analysis.
    #[derive(Clone, Debug)]
    pub struct Plan {
        pub levels: Levels,
        pub lanes: Lanes,
        /// Each segment's lanes as the dsp crate makes them, in its own
        /// time (what Studio's take-envelope leveler keeps per item).
        pub contributions: Vec<Contributions>,
    }

    /// Why nothing was made.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Nothing {
        NoAudio,
        NothingSung,
    }

    struct Read {
        start_s: f64,
        analysis: env::Analysis,
    }

    fn power_mean(levels: impl Iterator<Item = f64>) -> Option<f64> {
        let (sum, n) = levels.fold((0.0, 0_u32), |(s, n), db| {
            (libm::pow(10.0, db / 10.0) + s, n.saturating_add(1))
        });
        (n > 0).then(|| 10.0 * libm::log10(sum / f64::from(n)))
    }

    fn value_at(points: &[EnvPoint], t: f64) -> f64 {
        let at = points.partition_point(|p| p.t_s < t);
        match (
            at.checked_sub(1).and_then(|i| points.get(i)),
            points.get(at),
        ) {
            (Some(a), Some(b)) if b.t_s > a.t_s => {
                ((t - a.t_s) / (b.t_s - a.t_s)).mul_add(b.db - a.db, a.db)
            }
            (Some(a), _) => a.db,
            (None, Some(b)) => b.db,
            (None, None) => 0.0,
        }
    }

    /// The four lanes for one segment, ridden to `target_db`.
    fn lanes_of(a: &env::Analysis, target_db: f64, knobs: &Knobs) -> Contributions {
        let tolerance = env::GenerationConfig::default().tolerance_db;
        Contributions {
            gate: env::gate_envelope(a, a.silence_db + GATE_ABOVE_DB, GATE_FLOOR_DB, tolerance),
            breath: env::breath_envelope(
                a,
                BREATH_DB,
                target_db - BREATH_UNDER_DB,
                a.silence_db,
                tolerance,
            ),
            sibilance: env::sibilance_spans(a, ESS_DB),
            ride: env::phrase_ride(
                a,
                &env::phrase_levels(a, PHRASE_S),
                target_db,
                knobs.consistency,
                (knobs.range_db, knobs.range_db),
                tolerance,
            ),
            bypass: env::Bypass::default(),
        }
    }

    /// The lanes through the amounts: what plays.
    fn scaled(c: &Contributions, k: &Amounts) -> Contributions {
        let scale = |pts: &[EnvPoint], g: f64| -> Vec<EnvPoint> {
            pts.iter()
                .map(|p| EnvPoint { db: p.db * g, ..*p })
                .collect()
        };
        Contributions {
            gate: scale(&c.gate, k.gate),
            breath: scale(&c.breath, k.breath),
            ride: scale(&c.ride, k.ride),
            sibilance: c
                .sibilance
                .iter()
                .map(|s| env::GainSpan {
                    db: s.db * k.ess,
                    ..*s
                })
                .collect(),
            bypass: env::Bypass::default(),
        }
    }

    /// Each sung block — song time and level — with `gain_at(segment, t)`.
    fn sung(reads: &[Read], gain_at: impl Fn(usize, f64) -> f64) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for (k, r) in reads.iter().enumerate() {
            for i in (0..r.analysis.blocks()).filter(|i| r.analysis.is_sung(*i)) {
                let t = r.analysis.time_of(i);
                out.push((
                    r.start_s + t,
                    r.analysis.rms_db.get(i).copied().unwrap_or(0.0) + gain_at(k, t),
                ));
            }
        }
        out
    }

    /// The share `of` of the sorted `phrases`.
    fn share(phrases: &[f64], of: f64) -> Option<f64> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let at = (of * phrases.len() as f64) as usize;
        phrases
            .get(at.min(phrases.len().saturating_sub(1)))
            .copied()
    }

    /// Analyse the audio arriving at a `level` block.
    ///
    /// `keep`: the levels of an earlier analysis, for one over part of
    /// the song (its segments only that part): the lines are ridden to the
    /// same target and the same makeup is added, so the part joins the
    /// rest.
    ///
    /// # Errors
    ///
    /// No audio, or nothing sung in it.
    pub fn plan(
        segments: &[Segment<'_>],
        knobs: &Knobs,
        amounts: &Amounts,
        keep: Option<Levels>,
    ) -> Result<Plan, Nothing> {
        let reads: Vec<Read> = segments
            .iter()
            .filter(|s| !s.samples.is_empty())
            .map(|s| Read {
                start_s: s.start_s,
                analysis: env::analyze(
                    s.samples,
                    s.sample_rate,
                    signal_dsp::level::ClassifyConfig::default(),
                ),
            })
            .collect();
        if reads.is_empty() {
            return Err(Nothing::NoAudio);
        }
        let (target_db, level_db) = match keep {
            Some(l) => (l.target_db, l.level_db),
            None => {
                let mut phrases: Vec<f64> = reads
                    .iter()
                    .flat_map(|r| {
                        let levels = env::phrase_levels(&r.analysis, PHRASE_S);
                        (0..r.analysis.blocks())
                            .filter(|i| r.analysis.is_sung(*i))
                            .filter_map(|i| levels.get(i).copied().flatten())
                            .collect::<Vec<_>>()
                    })
                    .collect();
                phrases.sort_by(f64::total_cmp);
                let (Some(quiet), Some(loud)) =
                    (share(&phrases, QUIET_SHARE), share(&phrases, LOUD_SHARE))
                else {
                    return Err(Nothing::NothingSung);
                };
                knobs
                    .target_db
                    .map_or_else(|| (f64::midpoint(quiet, loud), loud), |t| (t, t))
            }
        };
        let mut made: Vec<Contributions> = reads
            .iter()
            .map(|r| lanes_of(&r.analysis, target_db, knobs))
            .collect();
        let makeup_db = match keep {
            Some(l) => l.makeup_db,
            None => {
                let composites: Vec<Vec<EnvPoint>> = made
                    .iter()
                    .map(|c| env::composite(&scaled(c, amounts)))
                    .collect();
                let after = power_mean(
                    sung(&reads, |k, t| {
                        composites.get(k).map_or(0.0, |c| value_at(c, t))
                    })
                    .into_iter()
                    .map(|(_, db)| db),
                )
                .unwrap_or(level_db);
                (level_db - after).clamp(-MAKEUP_MAX_DB, MAKEUP_MAX_DB)
            }
        };
        // The makeup rides in the ride, through its amount; none with the
        // ride off.
        let makeup_made = if amounts.ride > 1e-3 {
            (makeup_db / amounts.ride).clamp(-MAKEUP_MAX_DB, MAKEUP_MAX_DB)
        } else {
            0.0
        };
        for lanes in &mut made {
            if lanes.ride.is_empty() {
                lanes.ride.push(EnvPoint::new(0.0, 0.0));
            }
            for p in &mut lanes.ride {
                p.db += makeup_made;
            }
        }
        let mut lanes = Lanes::default();
        for (r, c) in reads.iter().zip(&made) {
            let at = |t: f64| r.start_s + t;
            lanes
                .gate
                .0
                .extend(stepped(&c.gate).map(|(t, v)| (at(t), v)));
            lanes
                .breath
                .0
                .extend(ramped(&c.breath).map(|(t, v)| (at(t), v)));
            lanes
                .ride
                .0
                .extend(ramped(&c.ride).map(|(t, v)| (at(t), v)));
            lanes
                .ess
                .0
                .extend(spans(&c.sibilance).map(|(t, v)| (at(t), v)));
        }
        for c in [
            &mut lanes.gate,
            &mut lanes.breath,
            &mut lanes.ride,
            &mut lanes.ess,
        ] {
            c.0.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        Ok(Plan {
            levels: Levels {
                target_db,
                level_db,
                makeup_db,
            },
            lanes,
            contributions: made,
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    fn f(db: f64) -> f32 {
        db as f32
    }

    /// A ramped curve, as it is.
    fn ramped(points: &[EnvPoint]) -> impl Iterator<Item = (f64, f32)> + '_ {
        points.iter().map(|p| (p.t_s, f(p.db)))
    }

    /// A held curve (the gate's) as linear points: each step a point
    /// [`EDGE_S`] before it at the value held there, as the composite has.
    fn stepped(points: &[EnvPoint]) -> impl Iterator<Item = (f64, f32)> + '_ {
        points.iter().enumerate().flat_map(|(i, p)| {
            let before = i
                .checked_sub(1)
                .and_then(|j| points.get(j))
                .filter(|q| q.hold)
                .map(|q| ((p.t_s - EDGE_S).max(0.0), f(q.db)));
            before.into_iter().chain(core::iter::once((p.t_s, f(p.db))))
        })
    }

    /// The sibilance spans as a lane: 0 dB outside them, their gain
    /// inside, each edge a step.
    fn spans(spans: &[env::GainSpan]) -> impl Iterator<Item = (f64, f32)> + '_ {
        spans.iter().flat_map(|s| {
            [
                ((s.from_s - EDGE_S).max(0.0), 0.0),
                (s.from_s, f(s.db)),
                ((s.to_s - EDGE_S).max(0.0), f(s.db)),
                (s.to_s, 0.0),
            ]
        })
    }

    /// A synthetic vocal, deterministic for `seed`: test and bench
    /// material, never a recording. Phrases of a voiced tone (harmonics on
    /// a gliding pitch, a syllable shape four times a second), verses
    /// ~12 dB under choruses, a breath before each phrase, an "ess" at some
    /// syllables, and room noise under it all. Mono, `seconds` long.
    #[must_use]
    pub fn synth_vocal(sample_rate: f64, seconds: f64, seed: u64) -> Vec<f64> {
        let mut rng = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let mut noise = move || {
            rng = rng
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            #[allow(clippy::cast_precision_loss)]
            let u = (rng >> 11) as f64 / (1_u64 << 53) as f64;
            2.0f64.mul_add(u, -1.0)
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (seconds * sample_rate) as usize;
        let db = |d: f64| libm::pow(10.0, d / 20.0);
        let (mut phase, mut lp, mut hp_prev, mut hp) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f64 / sample_rate;
            // 16 s sections: verse, chorus, verse, chorus…
            let chorus = (t / 16.0) as u64 % 2 == 1;
            let level = if chorus { db(-14.0) } else { db(-26.0) };
            // 3 s phrases, the last 0.7 s of each a gap.
            let in_phrase = t % 3.0;
            let sung = in_phrase < 2.3;
            let syl = (in_phrase * 4.0) % 1.0;
            let shape = libm::sin(core::f64::consts::PI * syl).max(0.0);
            let f0 = 6.0f64.mul_add(
                libm::sin(2.0 * core::f64::consts::PI * 5.0 * t),
                200.0 + 40.0 * libm::sin(2.0 * core::f64::consts::PI * t / 3.0),
            );
            phase += 2.0 * core::f64::consts::PI * f0 / sample_rate;
            let voiced: f64 = (1..=8)
                .map(|h| libm::sin(phase * f64::from(h)) / f64::from(h))
                .sum();
            let w = noise();
            lp += 0.05 * (w - lp);
            hp = 0.9 * (hp + w - hp_prev);
            hp_prev = w;
            let syllable = (in_phrase * 4.0) as u64;
            let ess = sung && syllable.is_multiple_of(3) && syl < 0.18;
            let breath = !sung && in_phrase > 2.6;
            let mut x = db(-62.0) * w;
            if sung {
                x += level * shape * voiced * 0.5;
            }
            if ess {
                x += level * 0.6 * hp;
            }
            if breath {
                x += level * db(-6.0) * lp * 4.0;
            }
            out.push(x);
        }
        out
    }

    /// The range the segments cover.
    #[must_use]
    pub fn covered(segments: &[Segment<'_>]) -> Option<Range> {
        segments
            .iter()
            .filter(|s| s.sample_rate > 0.0)
            .map(|s| {
                #[allow(clippy::cast_precision_loss)]
                let len = s.samples.len() as f64 / s.sample_rate;
                Range::new(s.start_s, s.start_s + len)
            })
            .reduce(|a, b| a.union(&b))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn tone(n: usize) -> alloc::vec::Vec<f32> {
        (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let x = libm::sin(i as f64 * 0.05) as f32;
                0.5 * x
            })
            .collect()
    }

    #[test]
    fn at_its_defaults_it_passes_audio_bit_for_bit() {
        let mut b = LevelApply::new(48_000.0);
        let (mut l, mut r) = (tone(512), tone(512));
        let want = l.clone();
        b.process(&mut l, &mut r);
        assert_eq!(l, want);
        assert_eq!(r, want);
    }

    #[test]
    fn a_change_ramps_in_db_across_one_piece_and_a_zero_piece_jumps() {
        let mut b = LevelApply::new(48_000.0);
        b.set_param(RIDE, -12.0);
        let (mut l, mut r) = (vec![1.0_f32; 4], vec![1.0_f32; 4]);
        b.process(&mut l, &mut r);
        let db: alloc::vec::Vec<f64> = l
            .iter()
            .map(|s| 20.0 * libm::log10(f64::from(*s)))
            .collect();
        for (k, d) in db.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let want = -12.0 * k as f64 / 4.0;
            assert!((d - want).abs() < 1e-4, "{k}: {d} vs {want}");
        }
        assert_eq!(b.gains_db().0, -12.0);
        b.set_param(GATE, -30.0);
        b.process(&mut [], &mut []);
        assert_eq!(b.gains_db().0, -42.0, "a zero-length piece jumps");
    }

    #[test]
    fn the_amounts_scale_their_lane() {
        let mut b = LevelApply::new(48_000.0);
        b.set_param(GATE, -30.0);
        b.set_param(GATE_AMOUNT, 0.5);
        b.set_param(RIDE, 6.0);
        b.set_param(RIDE_AMOUNT, 0.0);
        b.process(&mut [], &mut []);
        assert_eq!(b.gains_db().0, -15.0);
    }

    #[test]
    fn de_ess_takes_down_the_band_and_leaves_the_low_end() {
        let sr = 48_000.0_f64;
        let mut b = LevelApply::new(48_000.0);
        b.set_param(ESS, -12.0);
        b.process(&mut [], &mut []);
        let gain_at = |b: &mut LevelApply, hz: f64| {
            b.reset();
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let mut l: alloc::vec::Vec<f32> = (0..9600)
                .map(|i| libm::sin(2.0 * core::f64::consts::PI * hz * i as f64 / sr) as f32)
                .collect();
            let mut r = l.clone();
            b.process(&mut l, &mut r);
            let peak = l[4800..].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            20.0 * libm::log10(f64::from(peak))
        };
        assert!(gain_at(&mut b, 7000.0) < -11.0);
        assert!(gain_at(&mut b, 200.0) > -0.5);
    }
}
