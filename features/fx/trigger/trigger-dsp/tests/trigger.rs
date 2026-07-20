//! Trigger DSP tests — verify detection, velocity, sample playback, and signal flow.

use audiocore_dsp::{AudioConfig, Processor};
use trigger_dsp::chain::TriggerChain;
use trigger_dsp::detector::{DetectAlgorithm, DetectMode, TriggerDetector};
use trigger_dsp::sampler::{MixMode, Sample, Sampler, VelocityLayer};
use trigger_dsp::velocity::{VelocityCurve, VelocityMapper};

const SAMPLE_RATE: f64 = 48000.0;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: 512,
    }
}

/// Create a short click sample (1ms impulse).
fn click_sample() -> Sample {
    let len = (SAMPLE_RATE * 0.001) as usize; // 1ms
    let mut data = vec![[0.0; 2]; len];
    // Exponential decay click
    for (i, s) in data.iter_mut().enumerate() {
        let t = i as f64 / SAMPLE_RATE;
        let val = (-t * 5000.0).exp();
        *s = [val, val];
    }
    Sample::new(data, SAMPLE_RATE)
}

/// Create a longer tone sample (50ms sine).
fn tone_sample() -> Sample {
    let len = (SAMPLE_RATE * 0.050) as usize;
    let data: Vec<[f64; 2]> = (0..len)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            let val = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            [val, val]
        })
        .collect();
    Sample::new(data, SAMPLE_RATE)
}

fn make_chain() -> TriggerChain {
    let mut t = TriggerChain::new();
    t.threshold_db = -20.0;
    t.release_ratio = 0.5;
    t.detect_time_ms = 0.0; // No confirmation delay for testing
    t.release_time_ms = 5.0;
    t.retrigger_ms = 10.0;
    t.reactivity_ms = 5.0;
    t.detect_mode = DetectMode::Peak;
    t.dynamics = 0.5;
    t.velocity_curve = VelocityCurve::Linear;
    t.mix_mode = MixMode::Layer;
    t.mix_amount = 1.0;
    t.update(config());
    t
}

// ── Detector unit tests ─────────────────────────────────────────────────

#[test]
fn detector_triggers_on_loud_signal() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.release_ratio = 0.5;
    det.detect_time_ms = 0.0;
    det.reactivity_ms = 1.0;
    det.retrigger_ms = 1.0;
    det.update(SAMPLE_RATE);

    let amplitude = 0.5; // ~ -6 dBFS, well above -20 dB threshold
    let mut triggered = false;
    for _ in 0..500 {
        if det.tick(amplitude).is_some() {
            triggered = true;
            break;
        }
    }
    assert!(triggered, "Detector should trigger on loud signal");
}

#[test]
fn detector_stays_silent_on_quiet_signal() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.reactivity_ms = 5.0;
    det.retrigger_ms = 1.0;
    det.update(SAMPLE_RATE);

    let amplitude = 0.001; // ~ -60 dBFS, well below -20 dB threshold
    for _ in 0..2000 {
        assert!(
            det.tick(amplitude).is_none(),
            "Detector should not trigger on quiet signal"
        );
    }
}

#[test]
fn detector_retrigger_prevention() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.release_ratio = 0.1; // Very low release threshold for quick release
    det.detect_time_ms = 0.0;
    det.release_time_ms = 0.0;
    det.retrigger_ms = 50.0; // 50ms retrigger lockout
    det.reactivity_ms = 1.0;
    det.update(SAMPLE_RATE);

    let retrigger_samples = (50.0 * 0.001 * SAMPLE_RATE) as usize;

    // First trigger
    let mut first_trigger_at = None;
    for i in 0..500 {
        if det.tick(0.5).is_some() {
            first_trigger_at = Some(i);
            break;
        }
    }
    assert!(first_trigger_at.is_some(), "First trigger should fire");

    // Drop signal to release, then bring back up
    for _ in 0..100 {
        det.tick(0.0);
    }

    // Try to retrigger — should be blocked within retrigger window
    let mut second_trigger_at = None;
    for i in 0..retrigger_samples {
        if det.tick(0.5).is_some() {
            second_trigger_at = Some(i);
            break;
        }
    }

    // The second trigger should not fire within the retrigger window
    // (or if it does, it should be near the end)
    if let Some(at) = second_trigger_at {
        assert!(
            at >= retrigger_samples / 2,
            "Retrigger should be prevented for ~{retrigger_samples} samples, but fired at {at}"
        );
    }
}

#[test]
fn detector_hysteresis_prevents_chatter() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.release_ratio = 0.5; // Release at half detect level
    det.detect_time_ms = 0.0;
    det.release_time_ms = 0.0;
    det.retrigger_ms = 1.0;
    det.reactivity_ms = 0.5;
    det.update(SAMPLE_RATE);

    let detect_level = 10.0_f64.powf(-20.0 / 20.0); // 0.1
    let between_level = detect_level * 0.7; // Between detect and release thresholds

    // Trigger first
    for _ in 0..200 {
        det.tick(0.5);
    }
    assert!(det.is_active(), "Should be active after loud signal");

    // Drop to level between thresholds — should stay active
    for _ in 0..500 {
        det.tick(between_level);
    }
    assert!(
        det.is_active(),
        "Should stay active between thresholds (hysteresis)"
    );
}

// ── Velocity unit tests ─────────────────────────────────────────────────

#[test]
fn velocity_linear_scaling() {
    let mapper = VelocityMapper {
        dynamics: 1.0,
        fixed_velocity: 0.5,
        curve: VelocityCurve::Linear,
        min_velocity: 0.0,
        max_velocity: 1.0,
    };

    let threshold = 0.1;
    // At threshold: velocity = 0.5 * (1.0)^1.0 = 0.5
    let vel_at_threshold = mapper.map(threshold, threshold);
    assert!(
        (vel_at_threshold - 0.5).abs() < 0.01,
        "Velocity at threshold should be ~0.5: got {vel_at_threshold:.4}"
    );

    // At 2x threshold: velocity = 0.5 * (2.0)^1.0 = 1.0
    let vel_above = mapper.map(threshold * 2.0, threshold);
    assert!(
        vel_above > vel_at_threshold,
        "Louder signal should have higher velocity"
    );
}

#[test]
fn velocity_fixed_ignores_level() {
    let mapper = VelocityMapper {
        dynamics: 0.0,
        fixed_velocity: 0.8,
        curve: VelocityCurve::Linear,
        min_velocity: 0.0,
        max_velocity: 1.0,
    };

    let v1 = mapper.map(0.1, 0.01);
    let v2 = mapper.map(1.0, 0.01);
    assert!(
        (v1 - v2).abs() < 0.01,
        "Fixed velocity should not vary: {v1:.4} vs {v2:.4}"
    );
}

#[test]
fn velocity_to_midi_range() {
    assert_eq!(VelocityMapper::to_midi(0.0), 1);
    assert_eq!(VelocityMapper::to_midi(1.0), 127);
    assert_eq!(VelocityMapper::to_midi(0.5), 64);
}

// ── Sampler unit tests ──────────────────────────────────────────────────

#[test]
fn sampler_plays_on_trigger() {
    let mut sampler = Sampler::new();
    sampler.set_sample_rate(SAMPLE_RATE);
    sampler.mix_mode = MixMode::Layer;
    sampler.set_single_sample(tone_sample());

    assert!(!sampler.is_playing());

    sampler.trigger(0.8);
    assert!(sampler.is_playing());
    assert_eq!(sampler.active_voice_count(), 1);

    // Collect some output
    let mut peak = 0.0_f64;
    for _ in 0..1000 {
        let (l, _) = sampler.tick(0.0, 0.0);
        peak = peak.max(l.abs());
    }
    assert!(peak > 0.01, "Sample should produce output: peak={peak:.6}");
}

#[test]
fn sampler_round_robin() {
    let mut sampler = Sampler::new();
    sampler.set_sample_rate(SAMPLE_RATE);
    sampler.mix_mode = MixMode::Replace;

    // Create a layer with 3 different samples
    let mut layer = VelocityLayer::new(0.0, 1.0);
    for freq in [500.0, 1000.0, 2000.0] {
        let len = (SAMPLE_RATE * 0.010) as usize;
        let data: Vec<[f64; 2]> = (0..len)
            .map(|i| {
                let t = i as f64 / SAMPLE_RATE;
                let val = (2.0 * std::f64::consts::PI * freq * t).sin();
                [val, val]
            })
            .collect();
        layer.add_sample(Sample::new(data, SAMPLE_RATE));
    }
    sampler.add_layer(layer);

    // Trigger 6 times — should cycle through all 3 samples twice
    let mut outputs = Vec::new();
    for _ in 0..6 {
        sampler.trigger(0.8);
        // Collect first sample of each trigger
        let (l, _) = sampler.tick(0.0, 0.0);
        outputs.push(l);
        // Let the sample finish
        for _ in 0..1000 {
            sampler.tick(0.0, 0.0);
        }
        sampler.reset();
    }

    // Outputs should cycle: [a, b, c, a, b, c]
    // Check that position 0 and 3 match, 1 and 4 match
    for i in 0..3 {
        assert!(
            (outputs[i] - outputs[i + 3]).abs() < 1e-10,
            "Round-robin should cycle: output[{i}]={:.6} != output[{}]={:.6}",
            outputs[i],
            i + 3,
            outputs[i + 3]
        );
    }
}

#[test]
fn sampler_velocity_layers() {
    let mut sampler = Sampler::new();
    sampler.set_sample_rate(SAMPLE_RATE);
    sampler.mix_mode = MixMode::Replace;

    // Soft layer: 0-0.5 velocity, quiet sample
    let mut soft_layer = VelocityLayer::new(0.0, 0.5);
    let mut soft_sample = tone_sample();
    soft_sample.gain = 0.3;
    soft_layer.add_sample(soft_sample);
    sampler.add_layer(soft_layer);

    // Hard layer: 0.5-1.0 velocity, loud sample
    let mut hard_layer = VelocityLayer::new(0.5, 1.0);
    let mut hard_sample = tone_sample();
    hard_sample.gain = 1.0;
    hard_layer.add_sample(hard_sample);
    sampler.add_layer(hard_layer);

    // Soft trigger
    sampler.trigger(0.3);
    let mut soft_peak = 0.0_f64;
    for _ in 0..2400 {
        let (l, _) = sampler.tick(0.0, 0.0);
        soft_peak = soft_peak.max(l.abs());
    }
    sampler.reset();

    // Hard trigger
    sampler.trigger(0.8);
    let mut hard_peak = 0.0_f64;
    for _ in 0..2400 {
        let (l, _) = sampler.tick(0.0, 0.0);
        hard_peak = hard_peak.max(l.abs());
    }

    assert!(
        hard_peak > soft_peak,
        "Hard hit should be louder: soft={soft_peak:.4}, hard={hard_peak:.4}"
    );
}

#[test]
fn sampler_mix_modes() {
    let dry = 0.5;

    // Replace: output should be sample only
    let mut sampler = Sampler::new();
    sampler.set_sample_rate(SAMPLE_RATE);
    sampler.mix_mode = MixMode::Replace;
    sampler.set_single_sample(click_sample());
    sampler.trigger(1.0);
    let (l, _) = sampler.tick(dry, dry);
    // With Replace, dry signal should NOT appear
    // The sample's first value is ~1.0 * gain * velocity
    assert!(
        (l - dry).abs() > 0.01,
        "Replace mode should not pass dry signal"
    );
    sampler.reset();

    // Layer: output should be dry + sample
    sampler.mix_mode = MixMode::Layer;
    sampler.trigger(1.0);
    let (l, _) = sampler.tick(dry, dry);
    assert!(
        l > dry,
        "Layer mode should add sample to dry: got {l:.4}, dry={dry}"
    );
}

// ── Chain integration tests ─────────────────────────────────────────────

#[test]
fn chain_triggers_on_loud_input() {
    let mut chain = make_chain();
    chain.sampler.set_single_sample(tone_sample());
    chain.mix_mode = MixMode::Layer;
    chain.update(config());

    // Feed a loud transient
    let len = 4096;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    // Insert a loud hit at the start
    for i in 0..100 {
        left[i] = 0.5;
        right[i] = 0.5;
    }

    chain.process(&mut left, &mut right);

    assert!(
        chain.triggered_this_block,
        "Chain should detect trigger on loud transient"
    );
    assert!(
        chain.last_velocity > 0.0,
        "Velocity should be non-zero: {}",
        chain.last_velocity
    );
}

#[test]
fn chain_no_trigger_on_quiet_input() {
    let mut chain = make_chain();
    chain.sampler.set_single_sample(tone_sample());
    chain.update(config());

    let amplitude = 0.001; // Well below threshold
    let mut left = vec![amplitude; 4096];
    let mut right = vec![amplitude; 4096];

    chain.process(&mut left, &mut right);

    assert!(
        !chain.triggered_this_block,
        "Chain should not trigger on quiet signal"
    );
}

#[test]
fn chain_sidechain_hpf_blocks_bass_trigger() {
    // Without HPF: 50Hz bass at -10dBFS should trigger (above -20dB threshold)
    let mut chain_no_hpf = make_chain();
    chain_no_hpf.sampler.set_single_sample(tone_sample());
    chain_no_hpf.update(config());

    // With HPF at 500Hz: bass should be filtered from sidechain
    let mut chain_hpf = make_chain();
    chain_hpf.sampler.set_single_sample(tone_sample());
    chain_hpf.set_sc_hpf(500.0);
    chain_hpf.update(config());

    let len = 8192;
    let amp = 10.0_f64.powf(-10.0 / 20.0);
    let bass: Vec<f64> = (0..len)
        .map(|i| (2.0 * std::f64::consts::PI * 50.0 * i as f64 / SAMPLE_RATE).sin() * amp)
        .collect();

    let mut l1 = bass.clone();
    let mut r1 = bass.clone();
    let mut l2 = bass.clone();
    let mut r2 = bass.clone();

    chain_no_hpf.process(&mut l1, &mut r1);
    chain_hpf.process(&mut l2, &mut r2);

    // Without HPF should trigger, with HPF should not
    assert!(
        chain_no_hpf.triggered_this_block,
        "Without HPF, loud bass should trigger"
    );
    assert!(
        !chain_hpf.triggered_this_block,
        "With HPF, bass should be filtered and not trigger"
    );
}

#[test]
fn chain_sidechain_listen_outputs_filtered() {
    let mut chain = make_chain();
    chain.set_sc_hpf(1000.0);
    chain.sc_listen = true;
    chain.update(config());

    let len = 4096;
    let mut left: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            (2.0 * std::f64::consts::PI * 50.0 * t).sin() * 0.5
                + (2.0 * std::f64::consts::PI * 5000.0 * t).sin() * 0.5
        })
        .collect();
    let mut right = left.clone();

    chain.process(&mut left, &mut right);

    let peak: f64 = left[1024..].iter().map(|x| x.abs()).fold(0.0, f64::max);
    assert!(
        peak > 0.01,
        "SC listen should output filtered signal: peak={peak:.4}"
    );
}

// ── NaN safety ──────────────────────────────────────────────────────────

#[test]
fn no_nan_from_edge_cases() {
    let mut chain = make_chain();
    chain.sampler.set_single_sample(tone_sample());
    chain.update(config());

    let signals: Vec<f64> = vec![0.0, 1.0, -1.0, 1e6, -1e6, 1e-30, f64::MIN_POSITIVE];

    for &s in &signals {
        let mut left = vec![s; 64];
        let mut right = vec![s; 64];
        chain.process(&mut left, &mut right);
        for (i, (&l, &r)) in left.iter().zip(right.iter()).enumerate() {
            assert!(l.is_finite(), "NaN/Inf from input {s} at [{i}]: left={l}");
            assert!(r.is_finite(), "NaN/Inf from input {s} at [{i}]: right={r}");
        }
    }
}

// ── Reset ───────────────────────────────────────────────────────────────

#[test]
fn reset_clears_state() {
    let mut chain = make_chain();
    chain.sampler.set_single_sample(tone_sample());
    chain.update(config());

    // Trigger
    let mut left = vec![0.5; 2048];
    let mut right = vec![0.5; 2048];
    chain.process(&mut left, &mut right);

    chain.reset();

    // After reset, silence should stay silent
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    chain.process(&mut left, &mut right);

    for (i, (&l, &r)) in left.iter().zip(right.iter()).enumerate() {
        assert!(
            l == 0.0 && r == 0.0,
            "After reset, silence[{i}] = ({l}, {r})"
        );
    }
}

// ── Spectral algorithm tests ────────────────────────────────────────────

#[test]
fn detector_spectral_flux_triggers_on_transient() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.detect_time_ms = 0.0;
    det.retrigger_ms = 50.0;
    det.reactivity_ms = 5.0;
    det.algorithm = DetectAlgorithm::SpectralFlux;
    det.update(SAMPLE_RATE);

    assert!(
        det.latency_samples() > 0,
        "Spectral mode should report latency"
    );

    // Feed enough silence to fill the ODF ring buffer (31 hops × 441 samples)
    let mut triggered = false;
    for _ in 0..16000 {
        if det.tick(0.0).is_some() {
            triggered = true;
        }
    }
    assert!(!triggered, "Should not trigger on silence");

    // Loud transient burst — needs multiple hops to produce ODF values
    triggered = false;
    for i in 0..4096 {
        let t = i as f64 / SAMPLE_RATE;
        let sample = (2.0 * std::f64::consts::PI * 1000.0 * t).sin() * 0.8;
        if det.tick(sample).is_some() {
            triggered = true;
            break;
        }
    }
    assert!(triggered, "Spectral flux should trigger on loud transient");
}

#[test]
fn detector_superflux_triggers_on_transient() {
    let mut det = TriggerDetector::new();
    det.detect_threshold_db = -20.0;
    det.detect_time_ms = 0.0;
    det.retrigger_ms = 50.0;
    det.reactivity_ms = 5.0;
    det.algorithm = DetectAlgorithm::SuperFlux;
    det.update(SAMPLE_RATE);

    assert!(det.latency_samples() > 0, "SuperFlux should report latency");

    // Feed enough silence to fill the ODF ring buffer
    for _ in 0..16000 {
        det.tick(0.0);
    }

    // Loud transient
    let mut triggered = false;
    for i in 0..4096 {
        let t = i as f64 / SAMPLE_RATE;
        let sample = (2.0 * std::f64::consts::PI * 200.0 * t).sin() * 0.8;
        if det.tick(sample).is_some() {
            triggered = true;
            break;
        }
    }
    assert!(triggered, "SuperFlux should trigger on loud transient");
}

#[test]
fn detector_peak_envelope_has_zero_latency() {
    let mut det = TriggerDetector::new();
    det.algorithm = DetectAlgorithm::PeakEnvelope;
    det.update(SAMPLE_RATE);
    assert_eq!(
        det.latency_samples(),
        0,
        "PeakEnvelope should have zero latency"
    );
}

// ── Deterministic ───────────────────────────────────────────────────────

#[test]
fn deterministic_output() {
    let signal: Vec<f64> = (0..4096)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            // A hit followed by silence
            if i < 200 {
                (2.0 * std::f64::consts::PI * 1000.0 * t).sin() * 0.8
            } else {
                0.0
            }
        })
        .collect();

    let run = || {
        let mut chain = make_chain();
        chain.sampler.set_single_sample(tone_sample());
        chain.update(config());
        let mut left = signal.clone();
        let mut right = signal.clone();
        chain.process(&mut left, &mut right);
        left
    };

    let out1 = run();
    let out2 = run();

    for (i, (a, b)) in out1.iter().zip(out2.iter()).enumerate() {
        assert!(
            (*a).to_bits() == (*b).to_bits(),
            "Non-deterministic at sample {i}: {a} vs {b}"
        );
    }
}
