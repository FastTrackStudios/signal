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
    let m = p[0].clamp(0.0, 1.0);
    // Our hall's `decay_time` reads ~1.35× long by RT60 (Schroeder fit),
    // and its wet energy grows with the tail where PRO-Verb's is partly
    // normalized: level fitted over RT 0.24–6.6 s.
    let level_db = 0.3 - 5.8 * rt.max(0.05).log10();
    // Short tails on the room (the hall bottoms out near half a second),
    // trimmed: the room runs wetter than PRO-Verb's short settings.
    let room = rt < 0.8;
    let level_db = if room {
        level_db - 6.0 * (0.8 / rt.max(0.05)).log10()
    } else {
        level_db
    };
    RigBlock::of_type(BlockType::Reverb)
        .with_param("algorithm", if room { "0" } else { "1" })
        .with_param(
            "decay_time",
            format!("{:.3}", (rt / 1.35).clamp(0.05, 60.0)),
        )
        .with_param("level", format!("{level_db:.2}"))
        .with_param(
            "predelay",
            format!("{:.1}", (428.0 * p[3].max(0.0).powf(2.36)).min(200.0)),
        )
        .with_param("diffusion", format!("{:.3}", p[4].clamp(0.0, 1.0)))
        // High multiplier 0.865 (the default) ≈ neutral damping.
        .with_param("damping", format!("{:.3}", (1.0 - p[10]).clamp(0.0, 1.0)))
        .with_param("low_end", format!("{:.3}", p[9].clamp(0.0, 1.0)))
        .with_param("mix", format!("{:.4}", m.sqrt()))
        .with_param("dry", format!("{:.4}", (1.0 - m).sqrt()))
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
