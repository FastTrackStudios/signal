//! End-to-end checks against a synthesized "vocal": a loud voiced tone, a quiet
//! voiced tone, a burst of white noise (consonant), and silence. These exercise
//! the classifier, segmenter, offline envelope, and realtime rider without
//! needing an audio asset on disk.

use level_dsp::{
    analyze, apply_envelope, render_gain_envelope, BlockClass, ClassifyConfig, RenderConfig,
    RiderConfig, SegmentConfig, VocalRider,
};

const SR: f64 = 48_000.0;

/// Deterministic LCG noise in [-1, 1] — no rand dependency, no `Math.random`.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }
}

fn rms_db(x: &[f64]) -> f64 {
    let p = x.iter().map(|s| s * s).sum::<f64>() / x.len().max(1) as f64;
    10.0 * p.sqrt().max(1e-12).log10() * 2.0 / 2.0 // == 20*log10(rms)
}

/// Build: 0.5 s loud 220 Hz, 0.5 s quiet 220 Hz, 0.2 s noise, 0.3 s silence.
fn build_signal() -> Vec<f64> {
    let mut out = Vec::new();
    let tone = |n: usize, amp: f64, start: usize| {
        (0..n).map(move |i| amp * (core::f64::consts::TAU * 220.0 * (start + i) as f64 / SR).sin())
    };
    out.extend(tone((SR * 0.5) as usize, 0.5, 0));
    out.extend(tone((SR * 0.5) as usize, 0.05, 0));
    let mut lcg = Lcg(0x1234_5678);
    out.extend((0..(SR * 0.2) as usize).map(|_| 0.3 * lcg.next()));
    out.extend(std::iter::repeat_n(0.0, (SR * 0.3) as usize));
    out
}

#[test]
fn classifies_voiced_noise_and_silence() {
    let sig = build_signal();
    let a = analyze(
        &sig,
        SR,
        ClassifyConfig::default(),
        SegmentConfig::default(),
    );

    // Sample a block in each region.
    let block_dur = a.block_samples as f64 / SR;
    let class_at = |t: f64| {
        let idx = (t / block_dur) as usize;
        a.blocks[idx.min(a.blocks.len() - 1)].class
    };

    assert_eq!(class_at(0.25), BlockClass::Tonal, "loud tone must be tonal");
    assert_eq!(class_at(0.75), BlockClass::Tonal, "quiet tone still tonal");
    assert_eq!(
        class_at(1.10),
        BlockClass::Consonant,
        "white noise must be consonant"
    );
    assert_eq!(class_at(1.35), BlockClass::Silence, "tail must be silence");

    // At least one retained tonal segment spanning the sung region.
    assert!(!a.segments.is_empty(), "expected tonal segments");
    assert!(a.auto_target_db.is_some(), "expected an auto-target");
}

#[test]
fn envelope_boosts_the_quiet_tonal_region() {
    let sig = build_signal();
    let a = analyze(
        &sig,
        SR,
        ClassifyConfig::default(),
        SegmentConfig::default(),
    );

    // Target the loud region's level so the quiet tone gets boosted.
    let cfg = RenderConfig {
        target_db: Some(-9.0),
        ..RenderConfig::default()
    };
    let env = render_gain_envelope(&a, cfg);
    assert_eq!(env.len(), a.blocks.len());

    let block_dur = a.block_samples as f64 / SR;
    let g_at = |t: f64| {
        let idx = (t / block_dur) as usize;
        env[idx.min(env.len() - 1)].gain_db
    };
    // Quiet tone (~ -35 dB) should be pushed up toward -9 dB.
    assert!(
        g_at(0.75) > 6.0,
        "quiet tonal region should be boosted, got {} dB",
        g_at(0.75)
    );
    // Silence/noise get no ride.
    assert!(g_at(1.35).abs() < 1e-6, "silence should stay at 0 dB");

    // Applying the envelope must lift the quiet region's RMS.
    let mut processed = sig.clone();
    apply_envelope(&mut processed, &env, SR);
    let quiet_before = rms_db(&sig[(SR * 0.55) as usize..(SR * 0.95) as usize]);
    let quiet_after = rms_db(&processed[(SR * 0.55) as usize..(SR * 0.95) as usize]);
    assert!(
        quiet_after > quiet_before + 3.0,
        "quiet region should be louder after riding: {quiet_before} -> {quiet_after}"
    );
}

#[test]
fn realtime_rider_reduces_level_spread() {
    let sig = build_signal();
    // Seed the rider's silence floor from an offline pass (as the plugin would).
    let a = analyze(
        &sig,
        SR,
        ClassifyConfig::default(),
        SegmentConfig::default(),
    );
    let mut rider = VocalRider::new(
        SR,
        ClassifyConfig::default(),
        RiderConfig {
            target_db: -12.0,
            slew_ms: 30.0,
            ..RiderConfig::default()
        },
        a.silence_db,
    );

    let out: Vec<f64> = sig.iter().map(|&s| rider.process_sample(s)).collect();

    // Compare loud-vs-quiet spread before and after: riding should shrink it.
    let loud_in = rms_db(&sig[(SR * 0.1) as usize..(SR * 0.4) as usize]);
    let quiet_in = rms_db(&sig[(SR * 0.6) as usize..(SR * 0.9) as usize]);
    let loud_out = rms_db(&out[(SR * 0.1) as usize..(SR * 0.4) as usize]);
    let quiet_out = rms_db(&out[(SR * 0.6) as usize..(SR * 0.9) as usize]);

    let spread_in = (loud_in - quiet_in).abs();
    let spread_out = (loud_out - quiet_out).abs();
    assert!(
        spread_out < spread_in,
        "rider should reduce loud/quiet spread: {spread_in} -> {spread_out}"
    );
}
