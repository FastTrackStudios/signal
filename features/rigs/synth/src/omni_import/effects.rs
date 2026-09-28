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
}
