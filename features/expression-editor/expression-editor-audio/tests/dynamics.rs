//! Gating, compression, breath and sibilance.
//!
//! Built from signals whose content is known: a vowel is a harmonic
//! tone, a sibilant is bright noise, a breath is quiet broadband. The
//! interesting assertions are the ones that separate breath from
//! sibilance, because a pitch tracker sees both as "unvoiced" and
//! nothing else in the analysis tells them apart.

use expression_editor_audio::dynamics::{
    BreathConfig, CompressorConfig, Detection, DynamicsConfig, GateConfig, SibilanceConfig, analyse,
};
use expression_editor_audio::frames::{FrameFeature, frame_features};

const SR: f64 = 44100.0;
const WINDOW: usize = 1024;
const HOP: usize = 256;

fn frame_rate() -> f64 {
    SR / HOP as f64
}

/// A pitched vowel: harmonics, low zero-crossing rate, little high end.
fn vowel(secs: f64, amp: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|_| {
            phase += core::f64::consts::TAU * 220.0 / SR;
            (phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin()) * amp
        })
        .collect()
}

/// Bright noise — an "s". High-passed, so most of its energy is up top.
fn sibilant(secs: f64, amp: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut state = 0x2545F491_4F6CDD1Du64;
    let mut prev = 0.0;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let w = (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            // Difference of successive samples is a 6 dB/oct high-pass,
            // which is what makes this read as an "s" and not a rumble.
            let out = (w - prev) * amp * 2.0;
            prev = w;
            out
        })
        .collect()
}

/// Quiet broadband air — a breath. Same noise, low-passed and much
/// quieter, which is exactly how the two differ in practice.
fn breath(secs: f64, amp: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut state = 0x9E3779B9_7F4A7C15u64;
    let mut lp = 0.0;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let w = (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            lp += (w - lp) * 0.05;
            lp * amp
        })
        .collect()
}

fn silence(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs) as usize]
}

fn cat(parts: &[Vec<f64>]) -> Vec<f64> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

struct Take {
    features: Vec<FrameFeature>,
    voiced: Vec<bool>,
}

/// Analyse a buffer, marking frames voiced where the vowel is.
fn take(audio: &[f64], voiced_spans: &[(f64, f64)]) -> Take {
    let frames = audio.len().saturating_sub(WINDOW) / HOP + 1;
    let features = frame_features(audio, SR, WINDOW, HOP, frames);
    let voiced = (0..frames)
        .map(|f| {
            let t = f as f64 / frame_rate();
            voiced_spans.iter().any(|(a, b)| t >= *a && t <= *b)
        })
        .collect();
    Take { features, voiced }
}

// ── the features themselves ──────────────────────────────────────────

#[test]
fn a_sibilant_is_brighter_and_busier_than_a_vowel() {
    // The claim the whole sibilance detector rests on.
    let v = frame_features(&vowel(0.3, 0.3), SR, WINDOW, HOP, 20);
    let s = frame_features(&sibilant(0.3, 0.3), SR, WINDOW, HOP, 20);

    let mean = |f: &[FrameFeature], g: fn(&FrameFeature) -> f64| {
        f.iter().map(g).sum::<f64>() / f.len() as f64
    };
    assert!(
        mean(&s, |f| f.high_ratio) > mean(&v, |f| f.high_ratio) * 3.0,
        "sibilant {:.3} vs vowel {:.3}",
        mean(&s, |f| f.high_ratio),
        mean(&v, |f| f.high_ratio)
    );
    assert!(
        mean(&s, |f| f.zcr) > mean(&v, |f| f.zcr) * 3.0,
        "sibilant {:.3} vs vowel {:.3}",
        mean(&s, |f| f.zcr),
        mean(&v, |f| f.zcr)
    );
}

#[test]
fn silence_reports_no_brightness_rather_than_maximum() {
    // A zero-energy frame has no ratio. Reporting one would make
    // silence the brightest thing in the take and every gap a sibilant.
    let f = frame_features(&silence(0.2), SR, WINDOW, HOP, 10);
    assert!(f.iter().all(|f| f.high_ratio == 0.0));
    assert!(f.iter().all(|f| f.rms < 1e-9));
}

// ── the detectors ────────────────────────────────────────────────────

#[test]
fn nothing_is_detected_unless_it_is_switched_on() {
    // Opening a take must not silently give it four processors.
    let t = take(&cat(&[vowel(0.3, 0.4), sibilant(0.2, 0.3)]), &[(0.0, 0.3)]);
    let d = analyse(
        &t.features,
        &t.voiced,
        frame_rate(),
        &DynamicsConfig::default(),
    );
    assert!(d.gate.is_empty() && d.compressor.is_empty());
    assert!(d.breath.is_empty() && d.sibilance.is_empty());
    assert!(d.regions.is_empty());
}

#[test]
fn a_sibilant_is_found_and_a_breath_is_not_confused_for_one() {
    // Both are unvoiced, so a pitch tracker sees them identically. This
    // is the test that matters.
    let audio = cat(&[
        vowel(0.3, 0.4),
        sibilant(0.15, 0.3),
        vowel(0.3, 0.4),
        breath(0.2, 0.02),
    ]);
    let t = take(&audio, &[(0.0, 0.3), (0.45, 0.75)]);
    let cfg = DynamicsConfig {
        breath: Some(BreathConfig::default()),
        sibilance: Some(SibilanceConfig::default()),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);

    let sib: Vec<_> = d.regions_of(Detection::Sibilance).collect();
    let br: Vec<_> = d.regions_of(Detection::Breath).collect();
    assert_eq!(sib.len(), 1, "one sibilant, got {:?}", d.regions);
    assert_eq!(br.len(), 1, "one breath, got {:?}", d.regions);

    // And each landed in the right place.
    let sib_t = sib[0].start as f64 / frame_rate();
    assert!((0.28..0.48).contains(&sib_t), "sibilant at {sib_t}s");
    let br_t = br[0].start as f64 / frame_rate();
    assert!(br_t > 0.7, "breath at {br_t}s");
}

#[test]
fn a_region_can_be_marked_without_being_ducked() {
    // Breaths carry the performance; the default is to find them and
    // leave them alone.
    let audio = cat(&[vowel(0.3, 0.4), breath(0.2, 0.02)]);
    let t = take(&audio, &[(0.0, 0.3)]);
    let cfg = DynamicsConfig {
        breath: Some(BreathConfig::default()),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);

    assert_eq!(d.regions_of(Detection::Breath).count(), 1);
    assert!(
        d.breath.is_empty(),
        "a zero reduction writes no curve at all"
    );
}

#[test]
fn ducking_a_sibilant_only_touches_the_sibilant() {
    let audio = cat(&[vowel(0.3, 0.4), sibilant(0.15, 0.3), vowel(0.3, 0.4)]);
    let t = take(&audio, &[(0.0, 0.3), (0.45, 0.75)]);
    let cfg = DynamicsConfig {
        sibilance: Some(SibilanceConfig {
            reduction_db: -6.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);

    let r = d.regions_of(Detection::Sibilance).next().expect("found");
    assert!((d.sibilance[r.start].db + 6.0).abs() < 1e-9);
    assert_eq!(d.sibilance[0].db, 0.0, "the vowel before is untouched");
    let after = (r.end + 3).min(d.sibilance.len() - 1);
    assert_eq!(d.sibilance[after].db, 0.0, "and the vowel after");
}

#[test]
fn the_gate_closes_below_its_threshold_and_opens_above() {
    let audio = cat(&[vowel(0.3, 0.5), silence(0.3), vowel(0.3, 0.5)]);
    let t = take(&audio, &[(0.0, 0.3), (0.6, 0.9)]);
    let cfg = DynamicsConfig {
        gate: Some(GateConfig {
            floor_db: -20.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);

    let at = |secs: f64| d.gate[(secs * frame_rate()) as usize].db;
    assert!(at(0.2) > -1.0, "open on the vowel, got {}", at(0.2));
    assert!(at(0.5) < -10.0, "closed in the gap, got {}", at(0.5));
    assert!(at(0.85) > -3.0, "open again, got {}", at(0.85));
}

#[test]
fn the_gate_does_not_snap_which_is_what_makes_it_audible() {
    let audio = cat(&[vowel(0.2, 0.5), silence(0.4)]);
    let t = take(&audio, &[(0.0, 0.2)]);
    let cfg = DynamicsConfig {
        gate: Some(GateConfig::default()),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);

    // No single frame drops the whole way: a step function is what a
    // bad gate sounds like.
    let worst = d
        .gate
        .windows(2)
        .map(|w| (w[1].db - w[0].db).abs())
        .fold(0.0_f64, f64::max);
    assert!(worst < 8.0, "biggest single-frame jump was {worst} dB");
}

#[test]
fn the_compressor_pulls_down_loud_material_in_proportion() {
    let quiet = take(&vowel(0.4, 0.05), &[(0.0, 0.4)]);
    let loud = take(&vowel(0.4, 0.5), &[(0.0, 0.4)]);
    let cfg = DynamicsConfig {
        compressor: Some(CompressorConfig::default()),
        ..Default::default()
    };
    let dq = analyse(&quiet.features, &quiet.voiced, frame_rate(), &cfg);
    let dl = analyse(&loud.features, &loud.voiced, frame_rate(), &cfg);

    let settled = |c: &[expression_editor_audio::GainPoint]| c[c.len() - 2].db;
    assert!(settled(&dq.compressor) > -1.0, "quiet is left alone");
    assert!(
        settled(&dl.compressor) < settled(&dq.compressor) - 2.0,
        "loud is pulled down: {} vs {}",
        settled(&dl.compressor),
        settled(&dq.compressor)
    );
}

#[test]
fn a_higher_ratio_reduces_more() {
    let t = take(&vowel(0.4, 0.5), &[(0.0, 0.4)]);
    let run = |ratio: f64| {
        let cfg = DynamicsConfig {
            compressor: Some(CompressorConfig {
                ratio,
                ..Default::default()
            }),
            ..Default::default()
        };
        let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);
        d.compressor[d.compressor.len() - 2].db
    };
    assert!(run(8.0) < run(2.0), "8:1 {} vs 2:1 {}", run(8.0), run(2.0));
    // 1:1 is a bypass, not a subtle amount of compression.
    assert!(run(1.0).abs() < 1e-9);
}

// ── composition ──────────────────────────────────────────────────────

#[test]
fn the_curves_sum_in_db_so_two_cuts_are_a_bigger_cut() {
    // Gain stages multiply, and dB is where multiplication is addition.
    // Summing linear gains would make two 6 dB cuts into one.
    let audio = cat(&[vowel(0.3, 0.5), sibilant(0.2, 0.3)]);
    let t = take(&audio, &[(0.0, 0.3)]);
    let cfg = DynamicsConfig {
        compressor: Some(CompressorConfig::default()),
        sibilance: Some(SibilanceConfig {
            reduction_db: -6.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let d = analyse(&t.features, &t.voiced, frame_rate(), &cfg);
    let combined = d.combined(t.features.len());
    assert_eq!(combined.len(), t.features.len());

    let r = d
        .regions_of(Detection::Sibilance)
        .next()
        .expect("a sibilant");
    let i = r.start + r.len() / 2;
    let expected = d.sibilance[i].db + d.compressor[i].db;
    assert!(
        (combined[i].db - expected).abs() < 1e-9,
        "combined {} should be {}",
        combined[i].db,
        expected
    );
    assert!(combined[i].db < -6.0, "both cuts landed");
}

#[test]
fn an_empty_take_produces_nothing_rather_than_panicking() {
    let d = analyse(&[], &[], frame_rate(), &DynamicsConfig::default());
    assert!(d.regions.is_empty());
    assert!(d.combined(0).is_empty());
}
