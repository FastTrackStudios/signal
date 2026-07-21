//! Greenfield-compat tests — the coverage intents of the original in-tree
//! (pre-legacy-import) trigger-dsp test suite, ported onto the legacy
//! engine's API: silence stays silent, one physical hit (including its full
//! ring-out) fires exactly once, the retrigger window gates flams, louder
//! hits map to higher velocities, and per-sample detection places onsets
//! accurately inside a block.

use audiocore_dsp::{AudioConfig, Processor};
use trigger_dsp::chain::TriggerChain;
use trigger_dsp::detector::TriggerDetector;
use trigger_dsp::velocity::{VelocityCurve, VelocityMapper};

const SR: f64 = 48_000.0;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    }
}

/// Detector tuned like the greenfield defaults: -30 dB threshold, immediate
/// confirmation, envelope smoothing slow enough to bridge 60 Hz zero
/// crossings, 40 ms retrigger guard.
fn detector() -> TriggerDetector {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -30.0;
    det.release_ratio = 0.5;
    det.detect_time_ms = 0.0;
    det.release_time_ms = 0.0;
    det.retrigger_ms = 40.0;
    det.reactivity_ms = 10.0;
    det.update(SR);
    det
}

/// Write a synthetic kick: a decaying 60 Hz sine, `amp` peak, ~`decay_ms`
/// amplitude time constant, starting `at` samples into `buf`.
fn add_kick(buf: &mut [f64], at: usize, amp: f64, decay_ms: f64) {
    let w = 2.0 * std::f64::consts::PI * 60.0 / SR;
    let decay = -1.0 / (decay_ms * 0.001 * SR);
    for (n, sample) in buf.iter_mut().enumerate().skip(at) {
        let n = (n - at) as f64;
        *sample += amp * (n * decay).exp() * (n * w).sin();
    }
}

/// Run the detector over `buf`, returning (onset sample index, peak level)
/// per trigger.
fn collect_hits(det: &mut TriggerDetector, buf: &[f64]) -> Vec<(usize, f64)> {
    buf.iter()
        .enumerate()
        .filter_map(|(i, &x)| det.tick(x).map(|peak| (i, peak)))
        .collect()
}

#[test]
fn silence_never_fires() {
    let mut det = detector();
    let buf = vec![0.0; 48_000];
    assert!(collect_hits(&mut det, &buf).is_empty());
}

#[test]
fn sub_threshold_noise_never_fires() {
    let mut det = detector();
    // A steady tone well under the -30 dB threshold (~-46 dBFS).
    let buf: Vec<f64> = (0..24_000).map(|i| 0.005 * (i as f64 * 0.05).sin()).collect();
    assert!(collect_hits(&mut det, &buf).is_empty());
}

#[test]
fn kick_fires_exactly_once_including_ring_out() {
    let mut det = detector();
    let mut buf = vec![0.0; 48_000]; // 1 s: burst + full ring-out
    add_kick(&mut buf, 1000, 0.8, 30.0);
    let hits = collect_hits(&mut det, &buf);
    assert_eq!(hits.len(), 1, "expected exactly one hit: {hits:?}");
    // Onset near the burst start (envelope smoothing adds a couple ms).
    let at = hits[0].0 as i64;
    assert!((at - 1000).unsigned_abs() < 500, "onset at {at}");
}

#[test]
fn double_hit_inside_retrigger_window_fires_once() {
    let mut det = detector();
    let mut buf = vec![0.0; 48_000];
    add_kick(&mut buf, 1000, 0.8, 30.0);
    // Second burst 20 ms later — inside the 40 ms guard.
    add_kick(&mut buf, 1000 + (0.020 * SR) as usize, 0.8, 30.0);
    assert_eq!(collect_hits(&mut det, &buf).len(), 1);
}

#[test]
fn double_hit_outside_retrigger_window_fires_twice() {
    let mut det = detector();
    let mut buf = vec![0.0; 48_000];
    let second = 1000 + (0.200 * SR) as usize; // 200 ms later, fully decayed
    add_kick(&mut buf, 1000, 0.8, 30.0);
    add_kick(&mut buf, second, 0.8, 30.0);
    let hits = collect_hits(&mut det, &buf);
    assert_eq!(hits.len(), 2, "expected both hits: {hits:?}");
    let at = hits[1].0 as i64;
    assert!(
        (at - second as i64).unsigned_abs() < 500,
        "second onset at {at}, expected ~{second}"
    );
}

#[test]
fn louder_burst_maps_to_higher_velocity() {
    let mapper = VelocityMapper {
        dynamics: 1.0,
        fixed_velocity: 0.5,
        curve: VelocityCurve::Linear,
        min_velocity: 0.0,
        max_velocity: 1.0,
    };
    let threshold = 10.0_f64.powf(-30.0 / 20.0);

    let mut det_q = detector();
    let mut det_l = detector();
    let mut buf_q = vec![0.0; 24_000];
    let mut buf_l = vec![0.0; 24_000];
    add_kick(&mut buf_q, 1000, 0.2, 30.0);
    add_kick(&mut buf_l, 1000, 0.8, 30.0);
    let hq = collect_hits(&mut det_q, &buf_q);
    let hl = collect_hits(&mut det_l, &buf_l);
    assert_eq!(hq.len(), 1);
    assert_eq!(hl.len(), 1);

    let vq = VelocityMapper::to_midi(mapper.map(hq[0].1, threshold));
    let vl = VelocityMapper::to_midi(mapper.map(hl[0].1, threshold));
    assert!(vl > vq, "loud {vl} !> quiet {vq}");
    assert!((1..=127).contains(&vq) && (1..=127).contains(&vl));
}

#[test]
fn detect_tick_places_onset_in_block() {
    // The shell-facing per-sample API: run a block through detect_tick and
    // check the onset lands near the burst (the greenfield process_block
    // offset intent).
    let mut chain = TriggerChain::new();
    chain.threshold_db = -30.0;
    chain.detect_time_ms = 0.0;
    chain.retrigger_ms = 40.0;
    chain.reactivity_ms = 10.0;
    chain.dynamics = 1.0;
    chain.update(config());

    let mut buf = vec![0.0; 24_000];
    add_kick(&mut buf, 5000, 0.8, 30.0);

    let hits: Vec<(usize, f64)> = buf
        .iter()
        .enumerate()
        .filter_map(|(i, &x)| chain.detect_tick(x, x).map(|vel| (i, vel)))
        .collect();
    assert_eq!(hits.len(), 1, "expected one hit: {hits:?}");
    let at = hits[0].0 as i64;
    assert!((at - 5000).unsigned_abs() < 500, "onset at {at}");
    assert!(hits[0].1 > 0.0 && hits[0].1 <= 1.0);
    assert!(chain.triggered_this_block);
}
