//! Curve providers behind the lit visualizers (`viz.delay_taps`,
//! `viz.reverb_space`, `viz.mod_lfo`, `viz.eq_glow`): the data their shaders
//! read, and their vector fallbacks' geometry, from the same numbers — so the
//! two paths can't disagree. Ported from processor's `delay-ui`, `reverb-ui`
//! and `modulation-ui` painters (`paint_delay`, `paint_reverb`, `paint_mod`)
//! and signal's rig lanes (how taps follow from time, feedback and tempo).
//!
//! Series named in seconds/Hz/dB are data (shader inputs); the rest are the
//! fallback's geometry in **unit space** (`x` 0..1 across, `y` 0..1 up), for
//! `Curve` controls over `Axis::Linear { 0, 1 }` and `PlotRange::linear(0, 1)`.
//! A NaN `y` lifts the pen, so one series can hold many segments.
//!
//! | provider | inputs | series |
//! |---|---|---|
//! | `delay.taps` | `delay_ms` (the delay time), `feedback`, `mix`, `tap_div_l`, `tap_div_r` (division index, [`DIVISIONS`]), `bpm`, `on`; clock | `left`, `right` (`at_s, level`), `lane` (`window_s, beat_s`); `beats`, `bars`, `env_up`, `env_down`, `stems_{0,1,2}`, `caps_{0,1,2}`, `head`, `trail_{0..5}`, `blooms` |
//! | `reverb.tail` | `decay` (s), `predelay` (ms), `density`, `bpm`, `on`; clock | `beats`, `bars`, `env`, `refl_{0,1,2}`, `pre`, `shimmer_{0,1,2}` (by brightness), `band_{0..3}` |
//! | `mod.lfo` | `rate` (Hz), `depth`, `engine` ([`MOD_ENGINES`] index), `on`; clock | `bright`, `thin`, `mid`, `faint`, `grid`, `fill_up`, `fill_down`, `teeth_hi`, `teeth_mid`, `teeth_lo`, `dots_bright`, `dots_base`, `trail_{0,1,2}` |
//! | `eq.bands` | `b{n}_freq`, `b{n}_gain`, `b{n}_on`, `b{n}_used` (n = 1..=24) | `bands` (`freq_hz, gain_db` per used, enabled band) |
//! | `eq.grid` | `range` (a zoom param's option index), `span_{i}` (the spans it picks from) or `db_range` | `minor`, `zero`: processor's dB grid (a line every 2/3/6 dB by span) in unit space |

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::curves::{CurveInputs, CurveRegistry, Series};

/// Tap divisions (signal's rig order): multiples of a quarter note; `Free`
/// (0) uses the delay's own time.
pub const DIVISIONS: [(&str, f64); 11] = [
    ("1/4", 1.0),
    ("1/8.", 0.75),
    ("1/8", 0.5),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("Golden", 0.618),
    ("Silver", 0.414),
    ("Free", 0.0),
    ("1/4.", 1.5),
    ("1/2", 2.0),
    ("1/4T", 2.0 / 3.0),
];

/// The delay families, in the shader's order.
pub const DELAY_FAMILIES: [&str; 6] = ["Digital", "Tape", "Analog", "Pitch", "Rhythmic", "Special"];
/// The reverb families, in the shader's order.
pub const REVERB_FAMILIES: [&str; 8] = [
    "Room",
    "Hall",
    "Plate",
    "Spring",
    "Ambient",
    "Random",
    "Special",
    "Convolution",
];
/// The modulation engines, in the shader's order.
pub const MOD_ENGINES: [&str; 6] = [
    "Chorus", "Flanger", "Phaser", "Tremolo", "Vibrato", "Rotary",
];

/// The most taps a delay lane carries (processor's `MAX_TAPS`).
pub const MAX_TAPS: usize = 32;

pub(crate) fn register(r: &mut CurveRegistry) {
    r.register_animated("delay.taps", delay_taps);
    r.register_animated("reverb.tail", reverb_tail);
    r.register_animated("mod.lfo", mod_lfo);
    r.register("eq.bands", eq_bands);
    r.register("eq.grid", eq_grid);
}

/// A series's points.
type Points = Vec<(f64, f64)>;

const GAP: (f64, f64) = (f64::NAN, f64::NAN);

fn gap_at(x: f64) -> (f64, f64) {
    (x, f64::NAN)
}

/// A vertical segment `x, y0 → y1`, then a gap.
fn seg(out: &mut Vec<(f64, f64)>, x: f64, y0: f64, y1: f64) {
    out.extend([(x, y0), (x, y1), gap_at(x)]);
}

/// Beat and bar lines across `window` s (processor's `lane::beats`): none
/// past 64 lines.
fn beat_lines(window: f64, beat: f64) -> (Points, Points) {
    let (mut beats, mut bars) = (Vec::new(), Vec::new());
    if beat <= 0.0 || window <= 0.0 {
        return (beats, bars);
    }
    let count = (window / beat).ceil() as usize;
    if count > 64 {
        return (beats, bars);
    }
    for i in 1..=count {
        let t = beat * i as f64;
        if t > window {
            break;
        }
        let x = t / window;
        seg(if i % 4 == 0 { &mut bars } else { &mut beats }, x, 0.0, 1.0);
    }
    (beats, bars)
}

/// Which of three buckets `v` (0..1) falls in.
fn bucket(v: f64) -> usize {
    ((v.clamp(0.0, 1.0) * 3.0) as usize).min(2)
}

// ── delay ────────────────────────────────────────────────────────────────

/// One tap: seconds after the dry hit, level 0..1 (normalised to the
/// loudest), pan −1..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tap {
    pub at: f64,
    pub level: f64,
    pub pan: f64,
}

/// A delay lane: its taps (left run, then right, at most [`MAX_TAPS`]), the
/// seconds it shows, the beat.
#[derive(Debug, Clone, PartialEq)]
pub struct DelayLane {
    pub taps: Vec<Tap>,
    pub window: f64,
    pub beat: f64,
}

/// Division index → multiple of a quarter (0 = free).
#[must_use]
pub fn div_factor(index: f64) -> f64 {
    DIVISIONS
        .get(index.max(0.0).round() as usize)
        .map_or(0.0, |d| d.1)
}

/// The taps a stereo delay makes (signal's rig lane): each side repeats at
/// its division of the tempo (or the free `time`), `mix · feedbackⁿ`, until
/// the tail drops under 1.5 % or leaves an 8-beat ceiling; then scaled
/// to the loudest and windowed to whole beats (processor's `view_of` /
/// `window_for`).
#[must_use]
pub fn delay_lane(inputs: &CurveInputs) -> DelayLane {
    let bpm = inputs.or("bpm", 120.0).clamp(20.0, 400.0);
    let quarter = 60_000.0 / bpm;
    let win_ms = quarter * 8.0;
    let fb = inputs.or("feedback", 0.3).clamp(0.0, 0.98);
    let mix = inputs.or("mix", 0.5).clamp(0.02, 1.0);
    let time = inputs.or("delay_ms", 350.0).max(1.0);
    let side = |div: f64| {
        let f = div_factor(div);
        if f > 0.0 { quarter * f } else { time }
    };
    let (t_l, t_r) = (
        side(inputs.or("tap_div_l", 7.0)),
        side(inputs.or("tap_div_r", 7.0)),
    );
    let stems = |side_t: f64, up: bool| {
        let mut out = Vec::new();
        let (mut amp, mut t) = (mix, side_t);
        while t <= win_ms && amp > 0.015 && out.len() < 32 {
            out.push((t, amp, up));
            amp *= fb;
            t += side_t;
        }
        out
    };
    let mut raw = stems(t_l, true);
    raw.extend(stems(t_r, false));
    raw.truncate(MAX_TAPS);
    let peak = raw.iter().map(|t| t.1).fold(0.0_f64, f64::max);
    let scale = if peak > f64::EPSILON { 1.0 / peak } else { 1.0 };
    let taps: Vec<Tap> = raw
        .iter()
        .map(|(t, amp, up)| Tap {
            at: t / 1000.0,
            level: (amp * scale).clamp(0.0, 1.0),
            pan: if *up { -0.8 } else { 0.8 },
        })
        .collect();
    let beat = quarter / 1000.0;
    DelayLane {
        window: window_for(&taps, win_ms / 1000.0, beat),
        taps,
        beat,
    }
}

/// Seconds of lane the taps need: a beat of air after the last, whole
/// beats, at least two (processor's `window_for`).
fn window_for(taps: &[Tap], ceiling: f64, beat: f64) -> f64 {
    let last = taps.iter().map(|t| t.at).fold(0.0_f64, f64::max);
    if beat <= 0.0 {
        return (last * 1.15).clamp(0.05, ceiling.max(0.05));
    }
    let beats = ((last + beat) / beat).ceil().max(2.0);
    (beats * beat).min(ceiling.max(beat * 2.0))
}

/// The playhead across the lane (0..1), or `None` when bypassed.
fn delay_head(lane: &DelayLane, time: f64, on: bool) -> Option<f64> {
    let beat = lane.beat.max(1e-3);
    let cycle = beat * (lane.window / beat).ceil().max(1.0);
    (on && cycle > 0.0).then(|| (time % cycle) / cycle)
}

/// `delay.taps`.
#[must_use]
pub fn delay_taps(inputs: &CurveInputs) -> Vec<Series> {
    let lane = delay_lane(inputs);
    let on = inputs.or("on", 1.0) >= 0.5;
    let window = lane.window.max(0.05);
    let side = |left: bool| {
        lane.taps
            .iter()
            .filter(|t| (t.pan < 0.0) == left)
            .map(|t| (t.at, t.level))
            .collect::<Vec<_>>()
    };
    let (beats, bars) = beat_lines(window, lane.beat);

    // The tail's envelope, as processor's fallback draws it: through the
    // taps in lane order, above and (mirrored) below the rule.
    let env = |dir: f64| {
        let mut pts = vec![(0.0, 0.5)];
        if lane.taps.len() > 1 {
            pts.extend(
                lane.taps
                    .iter()
                    .map(|t| (t.at / window, 0.5 + dir * t.level * 0.42)),
            );
        }
        pts.push((1.0, 0.5));
        if lane.taps.len() > 1 { pts } else { Vec::new() }
    };

    // Stems and caps, bucketed by how loud each bar is (its alpha and cap
    // size follow `level · gain` in processor's painter).
    let mut stems: [Vec<(f64, f64)>; 3] = Default::default();
    let mut caps: [Vec<(f64, f64)>; 3] = Default::default();
    for t in &lane.taps {
        let x = t.at / window;
        if x > 1.0 {
            continue;
        }
        let ang = (t.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * FRAC_PI_2;
        for (gain, dir) in [(ang.cos(), 1.0), (ang.sin(), -1.0)] {
            let s = t.level.clamp(0.0, 1.0) * gain;
            let bar = s * 0.5 * 0.92;
            if bar < 0.004 {
                continue;
            }
            let k = bucket(s);
            seg(&mut stems[k], x, 0.5, 0.5 + dir * bar);
            caps[k].push((x, 0.5 + dir * bar));
        }
    }
    let head_at = delay_head(&lane, inputs.or("time", 0.0).max(0.0), on);
    let head = head_at.map_or_else(Vec::new, |h| vec![(h, 0.0), (h, 1.0)]);
    // The sweep's trail (a gradient behind the head, in slices) and the
    // blooms on the taps it has just crossed.
    let mut trail: [Vec<(f64, f64)>; TRAIL] = Default::default();
    let mut blooms = Vec::new();
    if let Some(h) = head_at {
        for (j, slice) in trail.iter_mut().enumerate() {
            let x0 = (h - 0.08 + 0.08 * j as f64 / TRAIL as f64).max(0.0);
            let x1 = (h - 0.08 + 0.08 * (j + 1) as f64 / TRAIL as f64).max(0.0);
            if x1 > x0 {
                slice.extend([(x0, 1.0), (x1, 1.0)]);
            }
        }
        let beat = lane.beat.max(1e-3);
        let cycle = beat * (window / beat).ceil().max(1.0);
        for t in &lane.taps {
            let since = (h - t.at / window + 1.0).rem_euclid(1.0);
            let over = beat / cycle;
            let hit = if since < over {
                (1.0 - since / over).powi(3)
            } else {
                0.0
            };
            if hit > 0.2 {
                let ang = (t.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * FRAC_PI_2;
                for (gain, dir) in [(ang.cos(), 1.0), (ang.sin(), -1.0)] {
                    let bar = t.level * gain * 0.5 * 0.92 * (1.0 + 0.10 * hit);
                    if bar >= 0.004 {
                        blooms.push((t.at / window, 0.5 + dir * bar));
                    }
                }
            }
        }
    }
    let [t0, t1, t2, t3, t4, t5] = trail;
    let [s0, s1, s2] = stems;
    let [c0, c1, c2] = caps;
    vec![
        Series::new("left", side(true)),
        Series::new("right", side(false)),
        Series::new("lane", vec![(window, lane.beat)]),
        Series::new("beats", beats),
        Series::new("bars", bars),
        Series::new("env_up", env(1.0)),
        Series::new("env_down", env(-1.0)),
        Series::new("stems_0", s0),
        Series::new("stems_1", s1),
        Series::new("stems_2", s2),
        Series::new("caps_0", sorted(c0)),
        Series::new("caps_1", sorted(c1)),
        Series::new("caps_2", sorted(c2)),
        Series::new("head", head),
        Series::new("trail_0", t0),
        Series::new("trail_1", t1),
        Series::new("trail_2", t2),
        Series::new("trail_3", t3),
        Series::new("trail_4", t4),
        Series::new("trail_5", t5),
        Series::new("blooms", sorted(blooms)),
    ]
}

/// Slices of the delay sweep's trail.
pub const TRAIL: usize = 6;

fn sorted(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts
}

// ── reverb ───────────────────────────────────────────────────────────────

/// `reverb.tail`: processor's `paint_reverb` as geometry.
#[must_use]
pub fn reverb_tail(inputs: &CurveInputs) -> Vec<Series> {
    let decay = inputs.or("decay", 2.0).max(0.05);
    let predelay = inputs.or("predelay", 20.0).max(0.0) / 1000.0;
    let density = inputs.or("density", 0.5).clamp(0.0, 1.0);
    let beat = 60.0 / inputs.or("bpm", 120.0).clamp(20.0, 400.0);
    let on = inputs.or("on", 1.0) >= 0.5;
    let window = (decay * 1.15).max(0.2);
    let pre = predelay / window;
    let (beats, bars) = beat_lines(window, beat);
    let amp_at = |secs: f64| (-6.908 * secs / decay).exp();

    let steps = 96;
    let mut env = vec![(pre, 0.0)];
    for i in 0..=steps {
        let t = f64::from(i) / f64::from(steps);
        env.push((pre + t * (1.0 - pre), amp_at(t * window) * 0.88));
    }
    env.push((1.0, 0.0));

    let mut refl: [Vec<(f64, f64)>; 3] = Default::default();
    let n = 6 + (density * 26.0) as usize;
    for i in 0..n {
        let t = i as f64 / n as f64;
        let at = t.powf(0.55) * 0.45;
        let x = pre + at * (1.0 - pre);
        let alpha = 0.5 * (1.0 - t).max(0.06);
        seg(
            &mut refl[bucket(alpha / 0.5)],
            x,
            amp_at(at * window) * 0.88,
            0.0,
        );
    }

    // The shimmer riding the tail, once a bar: its head (by brightness)
    // and the band behind it (in slices).
    let mut shimmer: [Vec<(f64, f64)>; 3] = Default::default();
    let mut band: [Vec<(f64, f64)>; 4] = Default::default();
    if on {
        let bar = beat.max(1e-3) * 4.0;
        let phase = (inputs.or("time", 0.0).max(0.0) % bar) / bar;
        let head_t = phase * window;
        let amp = amp_at(head_t);
        let lit = (amp * (1.0 - phase * 0.35)).clamp(0.0, 1.0);
        let hx = pre + phase * (1.0 - pre);
        if lit > 0.01 && hx <= 1.0 {
            shimmer[bucket(lit)].push((hx, amp * 0.88));
            for (j, slice) in band.iter_mut().enumerate() {
                let x0 = (hx - 0.10 + 0.10 * j as f64 / 4.0).max(0.0);
                let x1 = (hx - 0.10 + 0.10 * (j + 1) as f64 / 4.0).max(0.0);
                if x1 > x0 {
                    slice.extend([(x0, amp * 0.88), (x1, amp * 0.88), GAP]);
                }
            }
        }
    }
    let pre_rect = if pre > 0.002 {
        vec![(0.0, 1.0), (pre, 1.0)]
    } else {
        Vec::new()
    };
    let [r0, r1, r2] = refl;
    vec![
        Series::new("beats", beats),
        Series::new("bars", bars),
        Series::new("env", env),
        Series::new("refl_0", r0),
        Series::new("refl_1", r1),
        Series::new("refl_2", r2),
        Series::new("pre", pre_rect),
        Series::new("shimmer_0", std::mem::take(&mut shimmer[0])),
        Series::new("shimmer_1", std::mem::take(&mut shimmer[1])),
        Series::new("shimmer_2", std::mem::take(&mut shimmer[2])),
        Series::new("band_0", std::mem::take(&mut band[0])),
        Series::new("band_1", std::mem::take(&mut band[1])),
        Series::new("band_2", std::mem::take(&mut band[2])),
        Series::new("band_3", std::mem::take(&mut band[3])),
    ]
}

// ── modulation ───────────────────────────────────────────────────────────

/// `mod.lfo`: processor's `paint_mod`, per engine, as geometry.
#[must_use]
pub fn mod_lfo(inputs: &CurveInputs) -> Vec<Series> {
    let on = inputs.or("on", 1.0) >= 0.5;
    let t = if on { inputs.or("time", 0.0) } else { 0.0 };
    let rate = inputs.or("rate", 1.0).clamp(0.01, 20.0);
    let depth = inputs.or("depth", 0.5).clamp(0.0, 1.0);
    let phase = t * rate;
    let engine = inputs.or("engine", 0.0).round() as i64;
    // Unit y up from a y-down fraction of the height.
    let up = |y_down: f64| 1.0 - y_down;
    let mut s = Parts::default();
    match engine {
        1 => {
            // Flanger: the comb, its teeth sliding.
            let sweep = (phase * TAU).sin() * 0.5 + 0.5;
            let teeth = 48;
            for i in 0..teeth {
                let x = (f64::from(i) + 0.5) / f64::from(teeth);
                let f = f64::from(i) / f64::from(teeth);
                let comb =
                    ((f * (4.0 + 12.0 * sweep) * TAU).cos() * 0.5 + 0.5).powf(1.0 + 2.0 * depth);
                let height = comb * 0.82;
                let half = 0.35 / f64::from(teeth);
                let dst = if comb > 0.82 {
                    &mut s.teeth_hi
                } else if comb > 0.4 {
                    &mut s.teeth_mid
                } else {
                    &mut s.teeth_lo
                };
                dst.extend([(x - half, height), (x + half, height), GAP]);
            }
        }
        2 => {
            // Phaser: notches travelling through a band.
            let stages = 4;
            let sweep = (phase * TAU).sin() * 0.5 + 0.5;
            for px in 0..=160 {
                let x = f64::from(px) / 160.0;
                let mut y = 0.0;
                for st in 0..stages {
                    let centre = (0.12 + 0.20 * f64::from(st) + sweep * 0.30).fract();
                    let d = (x - centre) / 0.045;
                    y -= depth * (-d * d).exp();
                }
                s.bright.push((x, up(0.5 - y * 0.40)));
            }
            for st in 0..stages {
                let centre = (0.12 + 0.20 * f64::from(st) + sweep * 0.30).fract();
                s.dots_base.push((centre, up(0.5 + depth * 0.40)));
            }
        }
        3 => {
            // Tremolo: a waveform pumping in level.
            let lfo = |x: f64| (((x - phase) * TAU).sin() * 0.5 + 0.5).mul_add(depth, 1.0 - depth);
            let (mut top, mut bottom) = (Vec::new(), Vec::new());
            for px in 0..=160 {
                let x = f64::from(px) / 160.0;
                let amp = (lfo(x) * (x * 34.0 * TAU).sin() * 0.40).abs();
                top.push((x, up(0.5 - amp)));
                bottom.push((x, up(0.5 + amp)));
                s.fill_up.push((x, up(0.5 - lfo(x) * 0.40)));
                s.fill_down.push((x, up(0.5 + lfo(x) * 0.40)));
            }
            s.thin = top;
            s.thin.push(GAP);
            s.thin.extend(bottom);
        }
        4 => {
            // Vibrato: a waveform stretching and compressing.
            for px in 0..=200 {
                let x = f64::from(px) / 200.0;
                let bend = depth * 0.5 * ((x * 2.0 - phase) * TAU).sin();
                s.mid
                    .push((x, up(0.5 + ((x * 26.0 + bend * 6.0) * TAU).sin() * 0.36)));
            }
            for i in 1..8 {
                let x0 = f64::from(i) / 8.0;
                let bend = depth * 0.5 * ((x0 * 2.0 - phase) * TAU).sin();
                let x = (x0 + bend * 0.03).clamp(0.0, 1.0);
                seg(&mut s.grid, x, up(0.12), up(0.88));
            }
        }
        5 => {
            // Rotary: the orbit, and the doppler it makes.
            let (rx, ry) = (0.34 * (0.5 + 0.5 * depth), 0.33);
            for i in 0..=72 {
                let a = f64::from(i) / 72.0 * TAU;
                s.faint.push((0.5 + a.cos() * rx, up(0.5 + a.sin() * ry)));
            }
            let head = phase * TAU;
            for i in 0..14 {
                let back = f64::from(i) * 0.075;
                let a = head - back;
                // Nearer the front of the orbit = louder: the doppler.
                let front = (a.sin() * 0.5 + 0.5).mul_add(0.6, 0.4);
                let alpha = (1.0 - back / 1.05) * 0.5 * front;
                s.trail[bucket(alpha / 0.5)].push((0.5 + a.cos() * rx, up(0.5 + a.sin() * ry)));
            }
            s.dots_bright
                .push((0.5 + head.cos() * rx, up(0.5 + head.sin() * ry)));
        }
        _ => {
            // Chorus: detuned copies drifting around the dry and back.
            for v in 0..3 {
                let offset = f64::from(v) / 3.0;
                let drift = ((phase + offset) * TAU).sin();
                let spread = depth * 0.30 * drift;
                let line: Vec<(f64, f64)> = (0..=120)
                    .map(|px| {
                        let x = f64::from(px) / 120.0;
                        let wobble = ((x * 6.0 + drift * 2.0) * TAU).sin();
                        (x, up(0.5 + wobble * 0.10 + spread * (offset - 0.5) * 2.0))
                    })
                    .collect();
                if v == 1 {
                    s.bright = line;
                } else {
                    if !s.faint.is_empty() {
                        s.faint.push(GAP);
                    }
                    s.faint.extend(line);
                }
            }
        }
    }
    s.series()
}

#[derive(Default)]
struct Parts {
    bright: Vec<(f64, f64)>,
    /// Tremolo's trace (1.4 px), vibrato's (1.6 px); `bright` is 1.8.
    thin: Vec<(f64, f64)>,
    mid: Vec<(f64, f64)>,
    faint: Vec<(f64, f64)>,
    grid: Vec<(f64, f64)>,
    fill_up: Vec<(f64, f64)>,
    fill_down: Vec<(f64, f64)>,
    teeth_hi: Vec<(f64, f64)>,
    teeth_mid: Vec<(f64, f64)>,
    teeth_lo: Vec<(f64, f64)>,
    dots_bright: Vec<(f64, f64)>,
    dots_base: Vec<(f64, f64)>,
    trail: [Vec<(f64, f64)>; 3],
}

impl Parts {
    fn series(self) -> Vec<Series> {
        vec![
            Series::new("bright", self.bright),
            Series::new("thin", self.thin),
            Series::new("mid", self.mid),
            Series::new("faint", self.faint),
            Series::new("grid", self.grid),
            Series::new("fill_up", self.fill_up),
            Series::new("fill_down", self.fill_down),
            Series::new("teeth_hi", self.teeth_hi),
            Series::new("teeth_mid", self.teeth_mid),
            Series::new("teeth_lo", self.teeth_lo),
            Series::new("dots_bright", self.dots_bright),
            Series::new("dots_base", self.dots_base),
            Series::new("trail_0", self.trail[0].clone()),
            Series::new("trail_1", self.trail[1].clone()),
            Series::new("trail_2", self.trail[2].clone()),
        ]
    }
}

// ── EQ ───────────────────────────────────────────────────────────────────

/// `eq.bands`: every enabled band's `(freq_hz, gain_db)`, in band order.
#[must_use]
pub fn eq_bands(inputs: &CurveInputs) -> Vec<Series> {
    let bands = (1..=24)
        .filter_map(|n| {
            let freq = inputs.get(&format!("b{n}_freq"))?;
            let on = inputs.or(&format!("b{n}_on"), 1.0) >= 0.5
                && inputs.or(&format!("b{n}_used"), 1.0) >= 0.5;
            on.then(|| (freq, inputs.or(&format!("b{n}_gain"), 0.0)))
        })
        .collect();
    vec![Series::new("bands", bands)]
}

/// `eq.grid`: processor's dB grid lines (`paint_grid`) for the graph's
/// current span — every 2 dB up to ±6, 3 up to ±12, else 6 — as unit-space
/// segments: `minor`, and the `zero` line.
#[must_use]
pub fn eq_grid(inputs: &CurveInputs) -> Vec<Series> {
    let span = match inputs.get("range") {
        Some(i) => inputs.or(&format!("span_{}", i.round().max(0.0) as i64), 12.0),
        None => inputs.or("db_range", 12.0),
    }
    .max(0.5);
    let step = if span <= 6.0 {
        2.0
    } else if span <= 12.0 {
        3.0
    } else {
        6.0
    };
    let (mut minor, mut zero) = (Vec::new(), Vec::new());
    let mut db = -span;
    while db <= span + 1e-9 {
        let y = 0.5 + db / (2.0 * span);
        let line = if db.abs() < 0.01 {
            &mut zero
        } else {
            &mut minor
        };
        line.extend([(0.0, y), (1.0, y), gap_at(1.0)]);
        db += step;
    }
    vec![Series::new("minor", minor), Series::new("zero", zero)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use surface_proto::Axis;

    fn args() -> CurveInputs {
        CurveInputs::new(Axis::Linear { min: 0.0, max: 1.0 })
    }

    #[test]
    fn a_quarter_note_delay_puts_its_taps_on_the_beats() {
        let lane = delay_lane(
            &args()
                .with("bpm", 120.0)
                .with("tap_div_l", 0.0)
                .with("tap_div_r", 0.0)
                .with("feedback", 0.5)
                .with("mix", 1.0),
        );
        assert!((lane.beat - 0.5).abs() < 1e-9);
        let left: Vec<f64> = lane
            .taps
            .iter()
            .filter(|t| t.pan < 0.0)
            .map(|t| t.at)
            .collect();
        assert_eq!(left[..3], [0.5, 1.0, 1.5]);
        assert!(
            (lane.taps[0].level - 1.0).abs() < 1e-9,
            "scaled to the loudest"
        );
        assert!(
            (lane.window / lane.beat).fract().abs() < 1e-9,
            "whole beats"
        );
    }

    #[test]
    fn a_long_tail_is_capped_at_the_shader_array() {
        let lane = delay_lane(
            &args()
                .with("delay_ms", 40.0)
                .with("feedback", 0.95)
                .with("mix", 1.0),
        );
        assert!(lane.taps.len() <= MAX_TAPS);
    }

    #[test]
    fn the_delay_playhead_needs_the_lane_on() {
        let s = delay_taps(&args().with("time", 0.3).with("on", 0.0));
        assert!(
            s.iter()
                .find(|s| s.name == "head")
                .unwrap()
                .points
                .is_empty()
        );
        let s = delay_taps(&args().with("time", 0.3));
        assert_eq!(s.iter().find(|s| s.name == "head").unwrap().points.len(), 2);
    }

    #[test]
    fn a_reverb_envelope_starts_after_its_predelay() {
        let s = reverb_tail(&args().with("decay", 2.0).with("predelay", 100.0));
        let env = &s.iter().find(|s| s.name == "env").unwrap().points;
        assert!((env[0].0 - 0.1 / 2.3).abs() < 1e-9);
        assert!(env[1].1 > 0.87 && env.last().unwrap().1 == 0.0);
    }

    #[test]
    fn every_engine_draws_something() {
        for engine in 0..6 {
            let s = mod_lfo(&args().with("engine", f64::from(engine)).with("time", 0.4));
            assert!(s.iter().any(|s| !s.points.is_empty()), "engine {engine}");
        }
    }

    #[test]
    fn the_eq_grid_follows_the_span() {
        let lines = |s: &[Series]| s[0].points.iter().filter(|p| p.1.is_finite()).count() / 2;
        // ±12: every 3 dB, less 0 → 8 minor lines.
        assert_eq!(lines(&eq_grid(&args().with("db_range", 12.0))), 8);
        // ±6 picked by index: every 2 dB → 6.
        assert_eq!(
            lines(&eq_grid(
                &args()
                    .with("range", 1.0)
                    .with("span_0", 3.0)
                    .with("span_1", 6.0)
            )),
            6
        );
        assert_eq!(eq_grid(&args())[1].points[0].1, 0.5);
    }

    #[test]
    fn eq_bands_skip_disabled_ones() {
        let s = eq_bands(
            &args()
                .with("b1_freq", 100.0)
                .with("b1_gain", 3.0)
                .with("b2_freq", 1000.0)
                .with("b2_on", 0.0),
        );
        assert_eq!(s[0].points, vec![(100.0, 3.0)]);
    }
}
