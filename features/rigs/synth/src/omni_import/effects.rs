//! **Omnisphere effects → native blocks**, with their parameters.
//!
//! Each effect's `P0`..`P14` were decoded against the real plugin with
//! `tools/omni_probe/fxprobe.py` (a click or a held note through the effect
//! in the part's first Common FX slot). Effects without a translation here
//! fall back to the nearest block type at its defaults.

use signal_proto::block::BlockType;
use signal_sampler::rig::RigBlock;

use super::model::classify_effect;

/// The tempo synced times are realized at (the harness's; a host tempo
/// following synced effects is a later pass).
pub const SYNC_BPM: f32 = 120.0;

/// Chorus Echo's synced division table, `P1` → beats: `floor(P1·14)`
/// indexes whole, half, quarter, 8th, 16th, 32nd, 64th, then dotted half,
/// quarter, 8th, 16th, then half, quarter, 8th, 16th triplets (measured:
/// every boundary lands on a 1/14 step).
const CE_DIVISIONS: [f32; 15] = [
    4.0,
    2.0,
    1.0,
    0.5,
    0.25,
    0.125,
    0.0625,
    3.0,
    1.5,
    0.75,
    0.375,
    4.0 / 3.0,
    2.0 / 3.0,
    1.0 / 3.0,
    1.0 / 6.0,
];

/// A configured native block for Omnisphere effect `name` with params `p`,
/// or `None` for an empty slot.
#[must_use]
pub fn effect_block(name: &str, p: &[f32; 15]) -> Option<RigBlock> {
    if name.is_empty() || name == "No Effect" {
        return None;
    }
    Some(
        match name {
            "Chorus Echo" => chorus_echo(p),
            "PRO-Verb" => pro_verb(p),
            "Studio EQ" => studio_eq(p),
            "Vintage 2-Band EQ" => vintage_2band(p),
            "Super Verb" => super_verb(p),
            "Analog Chorus" => analog_chorus(p),
            "Tape Slammer" => tape_slammer(p),
            "Graphic 12-Band EQ" => graphic_12band(p),
            "BPM Delay" => bpm_delay(p),
            "Vintage Tremolo" => vintage_tremolo(p),
            "Vintage Compressor" => vintage_compressor(p),
            "Multiband Compressor" => multiband_compressor(p),
            "Magnetic Echo" => magnetic_echo(p),
            _ => RigBlock::of_type(classify_effect(name).unwrap_or(BlockType::Custom)),
        }
        .named(name),
    )
}

/// Chorus Echo (measured, echo modes):
/// - `P5` mode in 0.02 steps: 0 chorus only, 0.02 short echo
///   (`8 + 467·P1` ms), 0.04 long echo (`50 + 1940·P1` ms), 0.06 synced echo
///   (`P1` → [`CE_DIVISIONS`]);
/// - `P0` mix, power-complementary (dry `√(1−m)`, echo `√m`);
/// - `P2` feedback (the repeat ratio);
/// - `P12` output level (0.5 = unity).
///
/// The chorus half (`P3` rate, `P1` depth in chorus mode) is a later pass.
fn chorus_echo(p: &[f32; 15]) -> RigBlock {
    let mode = (p[5] * 50.0).round() as u32;
    let time_ms = match mode {
        1 => 8.0 + 467.0 * p[1],
        2 => 50.0 + 1940.0 * p[1],
        _ => {
            let i = ((p[1] * 14.0).floor() as usize).min(CE_DIVISIONS.len() - 1);
            CE_DIVISIONS[i] * 60_000.0 / SYNC_BPM
        }
    };
    let m = p[0].clamp(0.0, 1.0);
    // Output level: unity at 0.5 (measured +24 dB at 1, silent at 0).
    let out = if p[12] <= 0.5 {
        (p[12] / 0.5).max(0.0)
    } else {
        10f32.powf((p[12] - 0.5) * 48.0 / 20.0)
    };
    let wet = if mode == 0 { 0.0 } else { m.sqrt() };
    RigBlock::of_type(BlockType::Delay)
        .with_param("style", "1")
        .with_param("time", format!("{:.2}", time_ms.clamp(2.0, 2500.0)))
        .with_param("feedback", format!("{:.3}", p[2].clamp(0.0, 0.95)))
        .with_param("mix", format!("{:.4}", (wet * out).min(1.0)))
        .with_param("dry", format!("{:.4}", ((1.0 - m).sqrt() * out).min(1.0)))
        .with_param("tap_div", "7")
}

/// PRO-Verb (measured on a click):
/// - `P2` decay: RT60 ≈ `20 s · e^(5.5·(P2 − 1))` (0.54 → 1.6 s, 1 → 20 s);
/// - `P3` predelay ≈ `428 ms · P3^2.36`;
/// - `P0` mix (as Chorus Echo's, power-complementary);
/// - `P4` diffusion (0 on the echo-like presets);
/// - `P9` / `P10` the low / high band time multipliers (0 shortens a band
///   to ~0.6×, 1 lengthens it ~1.5×) — as low-end and damping here;
/// - `P12` stereo width (0 mono).
fn pro_verb(p: &[f32; 15]) -> RigBlock {
    let rt = 20.0 * (5.5 * (p[2] - 1.0)).exp();
    reverb_block(rt, 428.0 * p[3].max(0.0).powf(2.36), p[0], None)
        .with_param("diffusion", format!("{:.3}", p[4].clamp(0.0, 1.0)))
        // High multiplier 0.865 (the default) ≈ neutral damping.
        .with_param("damping", format!("{:.3}", (1.0 - p[10]).clamp(0.0, 1.0)))
        .with_param("low_end", format!("{:.3}", p[9].clamp(0.0, 1.0)))
}

/// PRO-Verb's measured wet level (dB re the dry click) for a tail of RT60
/// `rt` at full mix — what [`reverb_block`]'s level law lands on.
fn pro_verb_wet_db(rt: f32) -> f32 {
    -1.5 + 3.5 * (rt.max(0.05) / 1.64).log10()
}

/// A native reverb with RT60 `rt` (s), `predelay_ms`, mix `m` (power-
/// complementary) and, when given, a wet level (dB re the dry, at full
/// mix) to land on instead of PRO-Verb's.
fn reverb_block(rt: f32, predelay_ms: f32, m: f32, wet_db: Option<f32>) -> RigBlock {
    let m = m.clamp(0.0, 1.0);
    // Our hall's `decay_time` reads ~1.35× long by RT60 (Schroeder fit),
    // and its wet energy grows with the tail where PRO-Verb's is partly
    // normalized: level fitted over RT 0.24–6.6 s.
    let mut level_db = 0.3 - 5.8 * rt.max(0.05).log10();
    // Short tails on the room (the hall bottoms out near half a second),
    // trimmed: the room runs wetter than PRO-Verb's short settings.
    let room = rt < 0.8;
    if room {
        level_db -= 6.0 * (0.8 / rt.max(0.05)).log10();
    }
    if let Some(w) = wet_db {
        level_db += w - pro_verb_wet_db(rt);
    }
    RigBlock::of_type(BlockType::Reverb)
        .with_param("algorithm", if room { "0" } else { "1" })
        .with_param(
            "decay_time",
            format!("{:.3}", (rt / 1.35).clamp(0.05, 60.0)),
        )
        .with_param("level", format!("{:.2}", level_db.clamp(-60.0, 12.0)))
        .with_param("predelay", format!("{:.1}", predelay_ms.clamp(0.0, 200.0)))
        .with_param("mix", format!("{:.4}", m.sqrt()))
        .with_param("dry", format!("{:.4}", (1.0 - m).sqrt()))
        // The tone the RT/level laws were calibrated at (a caller may
        // override).
        .with_param("damping", "0.135")
        .with_param("low_end", "0.750")
}

/// Super Verb's rooms (`P3`, 0.02 steps), measured: RT60 (s) and wet level
/// (dB re the dry click) at size `P1` = 0, 0.5, 1.
#[rustfmt::skip]
const SUPER_VERB_ROOMS: [([f32; 3], [f32; 3]); 50] = [
    ([0.03, 0.19, 0.38], [-28.2, -10.1, -2.9]),
    ([0.02, 0.30, 1.18], [-19.8, -12.2, 1.6]),
    ([0.03, 0.33, 0.89], [-11.1, -8.0, 0.7]),
    ([0.06, 0.57, 1.35], [-3.0, -3.3, -0.8]),
    ([0.10, 0.97, 1.85], [-12.4, -5.4, -1.8]),
    ([0.08, 0.79, 1.11], [-0.7, -1.5, -1.2]),
    ([0.26, 0.61, 2.70], [-0.5, -1.1, 1.3]),
    ([0.30, 0.54, 3.60], [-6.7, -5.7, -1.6]),
    ([0.18, 0.66, 1.70], [-9.7, -3.4, 1.9]),
    ([0.63, 0.98, 2.20], [-9.9, -8.0, -4.2]),
    ([0.40, 0.95, 2.09], [-7.2, -5.9, -2.5]),
    ([0.58, 1.17, 2.71], [-3.4, -3.5, -1.7]),
    ([0.88, 1.37, 3.47], [-4.3, -2.8, 2.0]),
    ([0.06, 1.30, 2.66], [-1.6, -2.2, 0.2]),
    ([0.49, 1.16, 1.54], [-6.0, -4.2, -3.6]),
    ([1.23, 1.57, 6.14], [-5.5, -4.0, 1.5]),
    ([0.98, 1.99, 4.16], [-5.1, -4.9, -4.6]),
    ([1.24, 1.71, 4.15], [-4.1, -2.7, 2.2]),
    ([1.02, 2.60, 6.97], [-4.2, -4.2, -0.9]),
    ([0.95, 2.88, 6.21], [-6.1, -3.0, 1.1]),
    ([1.14, 4.62, 6.50], [-0.3, -0.9, 2.4]),
    ([1.47, 4.55, 8.69], [-5.6, -4.8, 0.5]),
    ([1.45, 2.13, 7.40], [-4.3, -2.2, 3.0]),
    ([2.14, 4.74, 12.74], [-0.1, 0.3, 6.2]),
    ([2.19, 4.87, 13.94], [-2.1, -1.1, 4.8]),
    ([2.24, 5.33, 12.55], [-3.6, -2.4, -0.2]),
    ([2.32, 7.13, 20.93], [-4.4, -0.6, 3.6]),
    ([1.24, 3.57, 5.36], [-3.9, -2.7, -1.4]),
    ([1.72, 3.44, 8.77], [-5.0, -2.6, 1.4]),
    ([1.97, 3.41, 6.58], [-3.2, -2.3, 2.2]),
    ([1.75, 2.63, 5.21], [-1.7, -0.7, 0.9]),
    ([2.56, 5.98, 9.56], [-5.6, -4.5, 2.0]),
    ([1.55, 7.25, 18.41], [-7.5, -2.5, 5.0]),
    ([2.47, 8.22, 19.46], [-6.3, -4.5, -4.5]),
    ([3.22, 7.36, 21.34], [-1.3, 2.7, 8.8]),
    ([2.04, 9.06, 18.60], [-5.5, 0.7, 6.1]),
    ([1.04, 7.56, 19.07], [-6.6, -2.8, -2.4]),
    ([1.18, 10.86, 14.62], [-4.0, -0.1, 4.5]),
    ([1.44, 7.26, 19.12], [-9.0, -5.6, -2.6]),
    ([3.10, 9.45, 18.96], [-10.6, -6.5, 1.2]),
    ([3.33, 10.34, 14.48], [-4.3, -0.2, 8.3]),
    ([2.64, 12.17, 22.67], [-6.1, -0.4, 4.0]),
    ([2.64, 8.32, 24.08], [-7.0, -2.0, 4.9]),
    ([1.83, 12.37, 23.42], [-4.1, 2.4, 4.8]),
    ([2.96, 12.80, 21.25], [-10.9, -5.0, -0.4]),
    ([2.25, 13.10, 14.55], [2.9, 5.9, 18.9]),
    ([2.99, 8.58, 12.95], [1.6, 1.5, 17.7]),
    ([2.29, 12.76, 19.99], [-2.8, 1.3, 11.8]),
    ([2.41, 14.34, 18.64], [-6.8, -1.6, 6.6]),
    ([2.47, 8.95, 22.55], [-2.9, 0.7, 8.2]),
];

/// Super Verb (measured on a click): `P3` picks one of 50 rooms, `P1` its
/// size (RT and wet level from [`SUPER_VERB_ROOMS`], log-interpolated),
/// `P0` mix, `P8` predelay (~`1000 ms · P8^2.9`), `P10` wet level (0 →
/// −6 dB, 0.5 unity, 1 → +4 dB), `P13` output level (0.75 unity).
fn super_verb(p: &[f32; 15]) -> RigBlock {
    let room = &SUPER_VERB_ROOMS[((p[3] * 50.0).round() as usize).min(49)];
    let s = p[1].clamp(0.0, 1.0);
    let (i, t) = if s < 0.5 {
        (0, s / 0.5)
    } else {
        (1, (s - 0.5) / 0.5)
    };
    let rt = (room.0[i].ln() + t * (room.0[i + 1].ln() - room.0[i].ln())).exp();
    let wet = room.1[i] + t * (room.1[i + 1] - room.1[i]);
    let wet_level = if p[10] < 0.5 {
        -6.3 * (1.0 - p[10] / 0.5)
    } else {
        3.8 * (p[10] - 0.5) / 0.5
    };
    const OUT: [(f32, f32); 5] = [
        (0.0, -60.0),
        (0.25, -18.8),
        (0.5, -6.8),
        (0.75, 0.0),
        (1.0, 10.0),
    ];
    let o = p[13].clamp(0.0, 1.0);
    let out = OUT.windows(2).find(|w| o <= w[1].0).map_or(10.0, |w| {
        w[0].1 + (w[1].1 - w[0].1) * (o - w[0].0) / (w[1].0 - w[0].0)
    });
    reverb_block(
        rt,
        1000.0 * p[8].max(0.0).powf(2.9),
        p[0],
        Some(wet + wet_level + out),
    )
}

/// Studio EQ (measured frequency responses): two bands, each `gain`
/// (±18 dB about 0.5), `freq` (`20 Hz · 2^(10·P)`), `Q` and a type in 0.02
/// steps — band 1 (`P0`..`P3`): low shelf / bell / low cut; band 2
/// (`P4`..`P7`): high shelf / bell / high cut.
fn studio_eq(p: &[f32; 15]) -> RigBlock {
    // Q from the half-gain bandwidths: 0.5 → 0.43, 0.75 → 1.3, 1 → 3.9;
    // wider below.
    let q = |v: f32| {
        let k = if v >= 0.5 { 6.4 } else { 3.4 };
        0.43 * (k * (v - 0.5)).exp2()
    };
    let mut b = RigBlock::of_type(BlockType::Eq);
    for (i, (g, f, w, t, shapes)) in [
        (p[0], p[1], p[2], p[3], [1u32, 0, 3]),
        (p[4], p[5], p[6], p[7], [2u32, 0, 4]),
    ]
    .into_iter()
    .enumerate()
    {
        let kind = ((t * 50.0).round() as usize).min(2);
        let n = i + 1;
        let f0 = 20.0 * (10.0 * f).exp2();
        // Measured: a shelf's knob sits at the edge of its transition (the
        // low shelf's midpoint ~1.7×, the high's ~0.65×) and is steep and
        // resonant; the low-pass corner sits ~1.6× up, sharper.
        let (hz, qq) = match (i, kind) {
            (0, 0) => (f0 * 1.7, q(w) * 2.5),
            (1, 0) => (f0 * 0.65, q(w) * 2.5),
            (1, 2) => (f0 * 1.6, q(w) * 2.0),
            _ => (f0, q(w)),
        };
        b = b
            .with_param(format!("b{n}_used"), "1")
            .with_param(format!("b{n}_on"), "1")
            .with_param(format!("b{n}_shape"), shapes[kind].to_string())
            .with_param(
                format!("b{n}_freq"),
                format!("{:.1}", hz.clamp(10.0, 22_000.0)),
            )
            .with_param(format!("b{n}_gain"), format!("{:.2}", 36.0 * (g - 0.5)))
            .with_param(format!("b{n}_q"), format!("{qq:.3}"));
    }
    b
}

/// Our chorus `depth` for an Omnisphere depth `d` (linear in pitch swing:
/// ±11 cents at 1.56 Hz for d = 1): ours swings ~`depth^1.7`, so this
/// linearizes it. Calibrated by pitch-tracking both.
fn chorus_depth(d: f32) -> f32 {
    (0.25 * d.max(0.0).powf(0.59)).min(1.0)
}

/// Analog Chorus (measured, pitch-tracked on a held note): `P1` rate
/// (`0.3 + 3.4·P1` Hz), `P2` depth (linear; ±0.65 ms of delay swing at 1;
/// the 0.04 mode runs deeper and 0.61× slower), `P0` mix.
fn analog_chorus(p: &[f32; 15]) -> RigBlock {
    let deep = ((p[4] * 50.0).round() as u32) == 2;
    // The 0.04 mode runs slower (×0.61) and deeper.
    let depth = chorus_depth(p[2].clamp(0.0, 1.0) * if deep { 4.8 } else { 1.0 });
    let rate = (0.3 + 3.4 * p[1].clamp(0.0, 1.0)) * if deep { 0.61 } else { 1.0 };
    let m = p[0].clamp(0.0, 1.0);
    RigBlock::of_type(BlockType::Chorus)
        .with_param("rate", format!("{rate:.3}"))
        .with_param("depth", format!("{depth:.4}"))
        .with_param("mix", format!("{m:.4}"))
}

/// Tape Slammer (measured static curves on a held note): `P2` sets the
/// threshold (~`−36·P2` dBFS RMS, −32 at 1; above it the output stays put — a
/// limiter), `P5` the makeup (`20·P5` dB). At its defaults (0.363, 0.114)
/// it is a +2.3 dB gain on ordinary levels.
fn tape_slammer(p: &[f32; 15]) -> RigBlock {
    RigBlock::of_type(BlockType::Compressor)
        .with_param(
            "threshold",
            format!("{:.1}", {
                // ~−36 dB per unit, flattening at the top (1 → −32 dB).
                let t = p[2].clamp(0.0, 1.0);
                if t <= 0.7 {
                    -36.0 * t
                } else {
                    -25.2 - 6.8 * (t - 0.7) / 0.3
                }
            }),
        )
        .with_param("ratio", "20")
        .with_param("attack", "1")
        .with_param("release", "100")
        .with_param("knee", "6")
        .with_param("makeup", format!("{:.1}", (20.0 * p[5]).clamp(-24.0, 24.0)))
}

/// Graphic 12-Band EQ (measured): `P0`..`P11` are ±15 dB bells (0.5
/// flat) at the centres below, `P13` the output gain (0.5 unity, +22 dB at
/// 1).
fn graphic_12band(p: &[f32; 15]) -> RigBlock {
    const CENTRES: [f32; 12] = [
        32.0, 125.0, 250.0, 450.0, 700.0, 1000.0, 1800.0, 2800.0, 4000.0, 7000.0, 10_000.0,
        16_000.0,
    ];
    let mut b = RigBlock::of_type(BlockType::Eq);
    for (i, hz) in CENTRES.iter().enumerate() {
        b = eq_band(b, i + 1, 0, *hz, 30.0 * (p[i] - 0.5), 1.4);
    }
    b.with_param("output_gain", format!("{:.2}", 44.0 * (p[13] - 0.5)))
}

/// BPM Delay (measured on a click): `P4` picks a division (`round(P4·50)`:
/// 4, 2, 1, ½, ¼, ⅛, 1/16, 1/32 beats, then dotted 6 … 3/8, then
/// triplets 8/3 … 1/6), `P7` the feedback (1 = endless), `P0` the mix,
/// `P1` the wet level (0.5 → −4 dB), `P5` = 0 silences the echo.
fn bpm_delay(p: &[f32; 15]) -> RigBlock {
    const BEATS: [f32; 18] = [
        4.0,
        2.0,
        1.0,
        0.5,
        0.25,
        0.125,
        0.0625,
        0.03125,
        6.0,
        3.0,
        1.5,
        0.75,
        0.375,
        8.0 / 3.0,
        4.0 / 3.0,
        2.0 / 3.0,
        1.0 / 3.0,
        1.0 / 6.0,
    ];
    let k = ((p[4] * 50.0).round() as usize).min(BEATS.len() - 1);
    let m = p[0].clamp(0.0, 1.0);
    let wet_gain = if p[5] <= 0.0 {
        0.0
    } else {
        p[1].clamp(0.0, 1.0).powf(0.68)
    };
    RigBlock::of_type(BlockType::Delay)
        .with_param("style", "1")
        .with_param(
            "time",
            format!("{:.2}", (BEATS[k] * 60_000.0 / SYNC_BPM).clamp(2.0, 2500.0)),
        )
        .with_param("feedback", format!("{:.3}", p[7].clamp(0.0, 0.95)))
        .with_param("mix", format!("{:.4}", (m.sqrt() * wet_gain).min(1.0)))
        .with_param("dry", format!("{:.4}", (1.0 - m).sqrt()))
        // Measured: its echoes sit ~3 dB above our clean delay's.
        .with_param("level", "3.0")
        .with_param("tap_div", "7")
}

/// Vintage Tremolo (measured on a held note): `P0` rate (2.33 Hz at 0.24,
/// 6.25 at 0.6, 13.3 at 1; log-interpolated), `P1` depth (0 none … 1
/// chopping to silence; ours cannot quite, and is ~1.5× gentler below),
/// `P4` its intensity (0 off).
fn vintage_tremolo(p: &[f32; 15]) -> RigBlock {
    let depth = (1.5 * p[1].clamp(0.0, 1.0)).min(1.0) * (p[4] / 0.635).clamp(0.0, 1.0);
    let r = p[0].clamp(0.0, 1.0);
    let rate = if r <= 0.6 {
        2.33 * (6.25f32 / 2.33).powf((r - 0.24) / 0.36)
    } else {
        6.25 * (13.33f32 / 6.25).powf((r - 0.6) / 0.4)
    };
    RigBlock::of_type(BlockType::Trem)
        .with_param("rate", format!("{:.3}", rate.clamp(0.05, 12.0)))
        .with_param("depth", format!("{depth:.3}"))
        .with_param("mix", "1")
}

/// Piecewise-linear lookup of `x` in `(x, y)` points (held past the ends).
fn lerp_table(points: &[(f32, f32)], x: f32) -> f32 {
    if x <= points[0].0 {
        return points[0].1;
    }
    for w in points.windows(2) {
        if x <= w[1].0 {
            let t = (x - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + t * (w[1].1 - w[0].1);
        }
    }
    points[points.len() - 1].1
}

/// Vintage Compressor (measured static curves): `P2` the makeup (0 → 0,
/// 0.104 → +2, 0.5 → +16.9, 1 → +28 dB), `P0` the threshold (limiting
/// only at the top of the range, ~−30·P0 dBFS RMS).
fn vintage_compressor(p: &[f32; 15]) -> RigBlock {
    let makeup = lerp_table(&[(0.0, 0.0), (0.104, 2.0), (0.5, 16.9), (1.0, 28.0)], p[2]);
    RigBlock::of_type(BlockType::Compressor)
        .with_param("threshold", format!("{:.1}", -30.0 * p[0].clamp(0.0, 1.0)))
        .with_param("ratio", "8")
        .with_param("attack", "5")
        .with_param("release", "150")
        .with_param("knee", "6")
        .with_param("makeup", format!("{:.1}", makeup.clamp(-24.0, 24.0)))
}

/// Multiband Compressor (measured static curves, at its default bands):
/// ~2.5:1 above ~−30 dBFS RMS, `P14` the output (0.5 → +4.3, 0.66 → +9.2,
/// 1 → +21 dB; silent at 0).
fn multiband_compressor(p: &[f32; 15]) -> RigBlock {
    let o = p[14].clamp(0.0, 1.0);
    let makeup = 0.4
        + if o >= 0.5 {
            4.3 + 33.2 * (o - 0.5)
        } else {
            4.3 + 20.0 * (o.max(1e-3) / 0.5).log10()
        };
    RigBlock::of_type(BlockType::Compressor)
        .with_param("threshold", "-30")
        .with_param("ratio", "2.5")
        .with_param("attack", "10")
        .with_param("release", "150")
        .with_param("knee", "6")
        .with_param("makeup", format!("{:.1}", makeup.clamp(-24.0, 24.0)))
}

/// Magnetic Echo (a tape echo, measured on a click): `P3` the time
/// (`72·e^(2.89·P3)` ms), `P5` the feedback (repeat ratio ≈ `P5^0.8`),
/// `P1` the echo level (`30·(P1 − 0.5)` dB), `P13` the output
/// (`30·(P13 − 0.6)` dB), `P0` a linear dry/echo crossfade.
fn magnetic_echo(p: &[f32; 15]) -> RigBlock {
    let m = p[0].clamp(0.0, 1.0);
    let out = 10f32.powf(30.0 * (p[13] - 0.6) / 20.0);
    let echo = 10f32.powf(30.0 * (p[1] - 0.5) / 20.0);
    RigBlock::of_type(BlockType::Delay)
        // Clean: the tape style adds latency and loss the plugin's lacks.
        .with_param("style", "1")
        .with_param(
            "time",
            format!("{:.2}", (72.0 * (2.89 * p[3]).exp()).clamp(2.0, 2500.0)),
        )
        .with_param(
            "feedback",
            format!("{:.3}", p[5].clamp(0.0, 1.0).powf(0.8).min(0.95)),
        )
        .with_param("mix", format!("{:.4}", (m * echo * out).min(1.0)))
        // Its echoes sit ~1.5 dB above our clean delay's; gain past unity
        // rides the wet level.
        .with_param(
            "level",
            format!(
                "{:.2}",
                1.5 + (20.0 * (m * echo * out).max(1e-6).log10()).max(0.0)
            ),
        )
        .with_param("dry", format!("{:.4}", ((1.0 - m) * out).min(1.0)))
        .with_param("tap_div", "7")
}

/// One EQ band's params (`n` from 1).
fn eq_band(b: RigBlock, n: usize, shape: u32, hz: f32, gain_db: f32, q: f32) -> RigBlock {
    b.with_param(format!("b{n}_used"), "1")
        .with_param(format!("b{n}_on"), "1")
        .with_param(format!("b{n}_shape"), shape.to_string())
        .with_param(
            format!("b{n}_freq"),
            format!("{:.1}", hz.clamp(10.0, 22_000.0)),
        )
        .with_param(format!("b{n}_gain"), format!("{gain_db:.2}"))
        .with_param(format!("b{n}_q"), format!("{q:.3}"))
}

/// Vintage 2-Band EQ (a Pultec, measured frequency responses): `P0` low
/// boost (a bell, ~+20 dB at 1) and `P2` low cut (a shelf, ~−20 dB) at the
/// `P1` selector (`round(P1·50)`, wrapping from 7: 20 / 30 / 50 / 65 / 90 /
/// 125 / 250 Hz); `P3` high boost (a bell, ~+20 dB) at the `P5` selector
/// (octaves from 2 kHz) with `P7` its bandwidth; `P6` high cut (a steep
/// shelf from ~8 kHz, ~−26 dB).
fn vintage_2band(p: &[f32; 15]) -> RigBlock {
    const LOW: [f32; 7] = [20.0, 30.0, 50.0, 65.0, 90.0, 125.0, 250.0];
    let lo = LOW[((p[1] * 50.0).round() as usize) % LOW.len()];
    let k = ((p[5] * 50.0).round() as i32).rem_euclid(10) as f32;
    let hi = (2000.0 * (k / 2.0).exp2()).min(20_000.0);
    // Broad: at the default 0.445 the boost spans octaves (Q ≈ 0.3).
    let hq = 0.3 * ((0.445 - p[7].clamp(0.0, 1.0)) * 3.0).exp2();
    let mut b = RigBlock::of_type(BlockType::Eq);
    b = eq_band(b, 1, 0, lo, 20.0 * p[0], 1.0);
    b = eq_band(b, 2, 1, lo, -20.0 * p[2], 0.7);
    b = eq_band(b, 3, 0, hi, 20.0 * p[3], hq);
    eq_band(b, 4, 2, 15_000.0, -26.0 * p[6], 1.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ce(p0: f32, p1: f32, p5: f32) -> RigBlock {
        let mut p = [0.0; 15];
        p[0] = p0;
        p[1] = p1;
        p[5] = p5;
        p[12] = 0.5;
        effect_block("Chorus Echo", &p).unwrap()
    }

    #[test]
    fn chorus_echo_times_follow_the_measured_modes() {
        let t = |b: &RigBlock| b.param_f32("time").unwrap();
        // Synced: 0.275 → an 8th (250 ms at 120), 0.655 → dotted 8th.
        assert!((t(&ce(1.0, 0.275, 0.06)) - 250.0).abs() < 0.1);
        assert!((t(&ce(1.0, 0.655, 0.06)) - 375.0).abs() < 0.1);
        // Free long: 0.5 → ~1020 ms; short: 0.5 → ~242 ms.
        assert!((t(&ce(1.0, 0.5, 0.04)) - 1020.0).abs() < 5.0);
        assert!((t(&ce(1.0, 0.5, 0.02)) - 241.5).abs() < 5.0);
        // Mix is power-complementary.
        let b = ce(0.5, 0.275, 0.06);
        assert!((b.param_f32("mix").unwrap() - 0.7071).abs() < 1e-3);
        assert!((b.param_f32("dry").unwrap() - 0.7071).abs() < 1e-3);
    }

    #[test]
    fn pro_verb_decay_and_predelay_follow_the_measured_laws() {
        let mut p = [0.0; 15];
        p[0] = 1.0;
        p[2] = 0.54;
        p[3] = 1.0;
        let b = effect_block("PRO-Verb", &p).unwrap();
        // RT60 1.59 s, realized through the hall's 1.35× long decay knob.
        assert!((b.param_f32("decay_time").unwrap() - 1.59 / 1.35).abs() < 0.02);
        // Predelay at full: 428 ms, clamped to our 200 ms.
        assert!((b.param_f32("predelay").unwrap() - 200.0).abs() < 0.1);
        assert_eq!(b.param_str("algorithm").as_deref(), Some("1"));
    }
}
