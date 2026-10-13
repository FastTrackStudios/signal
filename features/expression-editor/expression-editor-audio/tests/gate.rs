//! The transient gate, on signals whose attack times are known.
//!
//! These are about the detector itself — where it fires and what it
//! measures — where `quantize.rs` is about what happens to the hits
//! afterwards.

use expression_editor_audio::gate::{GateConfig, detect};

const SR: f64 = 44100.0;

fn noise(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
}

fn take(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs) as usize]
}

/// A struck hit: instant attack, exponential decay.
fn strike(out: &mut [f64], at: f64, amp: f64, hz: f64, decay: f64) {
    let mut seed = 0xbeefu64;
    let start = (at * SR) as usize;
    let n = (decay * SR) as usize;
    for i in 0..n {
        let Some(slot) = out.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / SR;
        let env = (-t / (decay * 0.3)).exp();
        *slot +=
            (0.7 * (core::f64::consts::TAU * hz * t).sin() + 0.3 * noise(&mut seed)) * env * amp;
    }
}

/// A tone that fades up and down over `len` — loud, never struck.
fn swell(out: &mut [f64], at: f64, amp: f64, len: f64) {
    let start = (at * SR) as usize;
    let n = (len * SR) as usize;
    for i in 0..n {
        let Some(slot) = out.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / SR;
        let env = (t / len * core::f64::consts::PI).sin();
        *slot += (core::f64::consts::TAU * 600.0 * t).sin() * env * amp;
    }
}

/// A realistic threshold — near the noise floor of the material, which
/// is what the control is for. See the note in `gate.rs`: the crest test
/// degenerates below it.
fn cfg() -> GateConfig {
    GateConfig {
        threshold_db: -30.0,
        ..GateConfig::default()
    }
}

fn times(audio: &[f64], c: GateConfig) -> Vec<f64> {
    detect(audio, SR, c)
        .iter()
        .map(|h| h.sample as f64 / SR)
        .collect()
}

#[test]
fn a_hit_is_found_within_a_millisecond_of_where_it_was_played() {
    // The property the whole quantizer rests on. An STFT detector is
    // quantised to its hop — 5.8 ms at the usual settings — which is
    // both audible on a kit and the same size as the error being
    // corrected. A sample-by-sample gate has no such floor.
    let played = [0.1234, 0.4567, 0.7891, 1.1111];
    let mut audio = take(1.5);
    for &t in &played {
        strike(&mut audio, t, 0.8, 120.0, 0.10);
    }

    let got = times(&audio, cfg());
    assert_eq!(got.len(), played.len(), "found {got:?}");
    for (want, at) in played.iter().zip(&got) {
        let error = (at - want).abs();
        assert!(
            error < 0.001,
            "hit at {want}s found at {at}s — {}ms out",
            error * 1000.0
        );
    }
}

#[test]
fn a_swell_is_not_a_transient() {
    // The case the crest test exists for: as loud as a hit, and not
    // struck. No threshold alone can tell these apart.
    let mut audio = take(1.2);
    swell(&mut audio, 0.2, 0.8, 0.6);
    let got = times(
        &audio,
        GateConfig {
            crest_db: 9.0,
            ..cfg()
        },
    );
    assert!(got.is_empty(), "a swell fired the gate at {got:?}");
}

#[test]
fn lowering_the_crest_admits_softer_attacks() {
    // Same signal, two settings: the control has to actually do
    // something or it is decoration.
    let mut audio = take(1.2);
    swell(&mut audio, 0.2, 0.8, 0.6);
    let strict = times(
        &audio,
        GateConfig {
            crest_db: 9.0,
            ..cfg()
        },
    );
    let loose = times(
        &audio,
        GateConfig {
            crest_db: 0.5,
            ..cfg()
        },
    );
    assert!(strict.len() < loose.len(), "{strict:?} vs {loose:?}");
}

#[test]
fn the_threshold_rejects_bleed() {
    let mut audio = take(1.5);
    strike(&mut audio, 0.2, 0.8, 120.0, 0.10);
    // 40 dB down — the shape of another drum leaking into this mic.
    strike(&mut audio, 0.6, 0.008, 120.0, 0.10);
    strike(&mut audio, 1.0, 0.8, 120.0, 0.10);

    let got = times(&audio, cfg());
    assert_eq!(got.len(), 2, "the bleed survived: {got:?}");
    assert!(got.iter().all(|t| (t - 0.6).abs() > 0.1));
}

#[test]
fn retrigger_sets_the_shortest_gap_between_hits() {
    // Three hits 30 ms apart. At a 50 ms retrigger they cannot all be
    // separate; at 10 ms they can.
    let mut audio = take(1.0);
    for i in 0..3 {
        strike(&mut audio, 0.2 + i as f64 * 0.03, 0.8, 120.0, 0.02);
    }
    let wide = times(
        &audio,
        GateConfig {
            retrigger_secs: 0.050,
            ..cfg()
        },
    );
    let tight = times(
        &audio,
        GateConfig {
            retrigger_secs: 0.010,
            measure_secs: 0.008,
            ..cfg()
        },
    );
    assert!(wide.len() < tight.len(), "{wide:?} vs {tight:?}");
    // And the gap is honoured, not merely influenced.
    for pair in wide.windows(2) {
        assert!(pair[1] - pair[0] >= 0.05 - 1e-6, "gap too short: {pair:?}");
    }
}

#[test]
fn a_retrigger_shorter_than_the_measurement_window_is_clamped() {
    // Two hits cannot be measured separately if their measurement
    // windows overlap, so a retrigger below the window would be a
    // setting that silently does nothing.
    let mut audio = take(1.0);
    for i in 0..4 {
        strike(&mut audio, 0.2 + i as f64 * 0.012, 0.8, 120.0, 0.01);
    }
    let got = times(
        &audio,
        GateConfig {
            retrigger_secs: 0.0,
            measure_secs: 0.025,
            ..cfg()
        },
    );
    for pair in got.windows(2) {
        assert!(
            pair[1] - pair[0] >= 0.025 - 1e-6,
            "hits closer than the measurement window: {pair:?}"
        );
    }
}

#[test]
fn a_louder_hit_measures_louder_on_both_scales() {
    let mut audio = take(1.2);
    strike(&mut audio, 0.2, 0.9, 120.0, 0.10);
    strike(&mut audio, 0.7, 0.3, 120.0, 0.10);
    let hits = detect(&audio, SR, cfg());
    assert_eq!(hits.len(), 2);
    assert!(hits[0].peak > hits[1].peak);
    assert!(hits[0].rms > hits[1].rms);
}

#[test]
fn a_dc_offset_does_not_inflate_the_measured_level() {
    // A close-miked kick has real offset. An RMS that counts it reports
    // a quiet hit as a loud one, which then outranks real hits when
    // sensitivity is applied.
    let mut clean = take(1.0);
    strike(&mut clean, 0.2, 0.4, 120.0, 0.10);
    let mut offset = clean.clone();
    for s in &mut offset {
        *s += 0.25;
    }

    let a = detect(&clean, SR, cfg());
    let b = detect(&offset, SR, cfg());
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert!(
        (a[0].rms - b[0].rms).abs() < 0.02,
        "offset changed RMS from {} to {}",
        a[0].rms,
        b[0].rms
    );
}

#[test]
fn silence_produces_nothing() {
    assert!(detect(&take(0.5), SR, cfg()).is_empty());
    assert!(detect(&[], SR, cfg()).is_empty());
}

#[test]
fn a_hit_at_the_very_end_is_still_reported() {
    // Its measurement window runs off the end of the take. Dropping it
    // would silently leave the last note of every take unquantized.
    let mut audio = take(0.5);
    strike(&mut audio, 0.49, 0.8, 120.0, 0.10);
    let got = times(&audio, cfg());
    assert_eq!(got.len(), 1, "the last hit went missing: {got:?}");
}

#[test]
fn the_peak_holdoff_measures_the_drum_and_not_the_stick() {
    // A sharp click on top of a fuller body. Measuring from sample zero
    // reports the click's level; the drum is what the user means.
    let mut audio = take(0.6);
    strike(&mut audio, 0.2, 0.35, 120.0, 0.12);
    // A very short, very loud spike right on the attack.
    let spike = (0.2 * SR) as usize;
    for i in 0..40 {
        audio[spike + i] += 0.9 * (1.0 - i as f64 / 40.0);
    }

    let with_holdoff = detect(&audio, SR, cfg());
    let without = detect(
        &audio,
        SR,
        GateConfig {
            peak_holdoff_secs: 0.0,
            ..cfg()
        },
    );
    assert_eq!(with_holdoff.len(), 1);
    assert!(
        with_holdoff[0].peak < without[0].peak,
        "the holdoff should skip the spike: {} vs {}",
        with_holdoff[0].peak,
        without[0].peak
    );
}
