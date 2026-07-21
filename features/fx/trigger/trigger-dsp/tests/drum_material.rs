//! Real drum multitrack tests using RomanStyx_SevenFeel WAV files.
//!
//! All tests are `#[ignore]` — they require WAV files in ~/Downloads/RomanStyx_SevenFeel/.
//! Run with: cargo test --package trigger-dsp -- --ignored

use audiocore_dsp::{AudioConfig, Processor};
use trigger_dsp::detector::{DetectMode, TriggerDetector};
use trigger_dsp::sampler::{MixMode, Sample};
use trigger_dsp::velocity::VelocityMapper;
use trigger_dsp::TriggerChain;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use std::path::PathBuf;

// ── Audio loading ───────────────────────────────────────────────────

struct Audio {
    samples: Vec<f64>,
    sample_rate: f64,
}

fn drum_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/cody".into());
    PathBuf::from(home).join("Downloads/RomanStyx_SevenFeel")
}

fn load_wav(filename: &str) -> Option<Audio> {
    let path = drum_data_dir().join(filename);
    if !path.exists() {
        eprintln!("Skipping: {} not found at {}", filename, path.display());
        return None;
    }

    let file = std::fs::File::open(&path).expect("open file");
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("wav");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("probe format");

    let mut reader = probed.format;
    let track = reader.default_track().expect("default track");
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.expect("sample rate") as f64;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("create decoder");

    let mut samples = Vec::new();

    while let Ok(packet) = reader.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match decoded {
            AudioBufferRef::F32(buf) => {
                let channels = buf.spec().channels.count();
                let frames = buf.frames();
                for frame in 0..frames {
                    let mut sum = 0.0;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f64;
                    }
                    samples.push(sum / channels as f64);
                }
            }
            AudioBufferRef::S16(buf) => {
                let channels = buf.spec().channels.count();
                let frames = buf.frames();
                for frame in 0..frames {
                    let mut sum = 0.0;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f64 / 32768.0;
                    }
                    samples.push(sum / channels as f64);
                }
            }
            AudioBufferRef::S32(buf) => {
                let channels = buf.spec().channels.count();
                let frames = buf.frames();
                for frame in 0..frames {
                    let mut sum = 0.0;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f64 / 2147483648.0;
                    }
                    samples.push(sum / channels as f64);
                }
            }
            _ => {}
        }
    }

    Some(Audio {
        samples,
        sample_rate,
    })
}

// ── Offline analysis helper ─────────────────────────────────────────

struct TriggerEvent {
    sample_pos: usize,
    velocity: f64,
}

fn run_detector(audio: &Audio, threshold_db: f64, sc_hpf_freq: f64) -> Vec<TriggerEvent> {
    let mut detector = TriggerDetector::new();
    detector.detect_threshold_db = threshold_db;
    detector.retrigger_ms = 10.0;
    detector.reactivity_ms = 10.0;
    detector.detect_time_ms = 1.0;
    detector.release_time_ms = 5.0;
    detector.release_ratio = 0.5;
    detector.mode = DetectMode::Peak;
    detector.update(audio.sample_rate);

    let velocity = VelocityMapper::new();
    let threshold_lin = 10.0_f64.powf(threshold_db / 20.0);

    // Optional sidechain HPF
    let mut hpf = if sc_hpf_freq > 0.0 {
        let mut band = eq_dsp::band::Band::new();
        band.filter_type = eq_dsp::FilterType::Highpass;
        band.freq_hz = sc_hpf_freq;
        band.q = 0.707;
        band.order = 2;
        band.enabled = true;
        band.update(audio.sample_rate);
        Some(band)
    } else {
        None
    };

    let mut events = Vec::new();

    for (i, &sample) in audio.samples.iter().enumerate() {
        let sc = if let Some(ref mut h) = hpf {
            h.tick(sample, 0)
        } else {
            sample
        };

        if let Some(peak_level) = detector.tick(sc) {
            let vel = velocity.map(peak_level, threshold_lin);
            events.push(TriggerEvent {
                sample_pos: i,
                velocity: vel,
            });
        }
    }

    events
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
#[ignore]
fn kick_detection_finds_all_hits() {
    let audio = match load_wav("01_KickIn.wav") {
        Some(a) => a,
        None => return,
    };

    let events = run_detector(&audio, -30.0, 0.0);

    println!("Detected {} kick hits", events.len());
    println!("sample_pos,time_sec,velocity");
    for e in &events {
        let time = e.sample_pos as f64 / audio.sample_rate;
        println!("{},{:.3},{:.3}", e.sample_pos, time, e.velocity);
    }

    assert!(
        events.len() > 10,
        "Expected >10 kick hits, got {}",
        events.len()
    );
}

#[test]
#[ignore]
fn kick_detection_no_false_triggers_on_snare() {
    let audio = match load_wav("03_SnareUp.wav") {
        Some(a) => a,
        None => return,
    };

    // SC HPF at 200Hz to reject snare, only pass low-freq kick bleed
    let events = run_detector(&audio, -20.0, 200.0);

    println!(
        "Detected {} triggers on snare track (with 200Hz HPF)",
        events.len()
    );

    // With proper sidechain filtering and threshold, should detect very few
    assert!(
        events.len() < 5,
        "Expected <5 false triggers on snare, got {}",
        events.len()
    );
}

#[test]
#[ignore]
fn kick_velocity_distribution() {
    let audio = match load_wav("01_KickIn.wav") {
        Some(a) => a,
        None => return,
    };

    let events = run_detector(&audio, -30.0, 0.0);
    assert!(!events.is_empty(), "No triggers detected");

    let velocities: Vec<f64> = events.iter().map(|e| e.velocity).collect();
    let min_vel = velocities.iter().cloned().fold(f64::MAX, f64::min);
    let max_vel = velocities.iter().cloned().fold(f64::MIN, f64::max);

    println!("Velocity range: {:.3} - {:.3}", min_vel, max_vel);

    // Check MIDI velocity spread
    let midi_vels: Vec<u8> = velocities
        .iter()
        .map(|&v| trigger_dsp::velocity::VelocityMapper::to_midi(v))
        .collect();
    let mut unique: Vec<u8> = midi_vels.clone();
    unique.sort();
    unique.dedup();

    println!("Unique MIDI velocities: {:?}", unique);

    assert!(
        unique.len() >= 3,
        "Expected at least 3 distinct MIDI velocities, got {}",
        unique.len()
    );
}

#[test]
#[ignore]
fn kick_retrigger_spacing() {
    let audio = match load_wav("01_KickIn.wav") {
        Some(a) => a,
        None => return,
    };

    let retrigger_ms = 10.0;
    let min_samples = (retrigger_ms * 0.001 * audio.sample_rate) as usize;

    let events = run_detector(&audio, -30.0, 0.0);
    assert!(events.len() > 1, "Need at least 2 triggers");

    for i in 1..events.len() {
        let spacing = events[i].sample_pos - events[i - 1].sample_pos;
        assert!(
            spacing >= min_samples,
            "Trigger {} and {} too close: {} samples (min {})",
            i - 1,
            i,
            spacing,
            min_samples
        );
    }

    println!(
        "All {} trigger intervals >= {} samples ({}ms)",
        events.len() - 1,
        min_samples,
        retrigger_ms
    );
}

#[test]
#[ignore]
fn kick_replacement_output_finite() {
    let audio = match load_wav("01_KickIn.wav") {
        Some(a) => a,
        None => return,
    };

    let mut chain = TriggerChain::new();
    chain.threshold_db = -30.0;
    chain.mix_mode = MixMode::Replace;

    // Create a simple sine click as replacement sample
    let click_len = 4800; // 0.1s at 48kHz
    let click_data: Vec<[f64; 2]> = (0..click_len)
        .map(|i| {
            let t = i as f64 / 48000.0;
            let env = (-t * 50.0).exp();
            let s = (t * 200.0 * std::f64::consts::TAU).sin() * env * 0.8;
            [s, s]
        })
        .collect();
    let click_sample = Sample::new(click_data, 48000.0);
    chain.sampler.set_single_sample(click_sample);

    chain.update(AudioConfig {
        sample_rate: audio.sample_rate,
        max_buffer_size: 512,
    });

    let mut left = audio.samples.clone();
    let mut right = audio.samples.clone();
    chain.process(&mut left, &mut right);

    // Verify output is finite
    let all_finite = left.iter().all(|s| s.is_finite()) && right.iter().all(|s| s.is_finite());
    assert!(all_finite, "Output contains non-finite samples");

    // Verify there's audible content at trigger points (not all zeros)
    let rms: f64 = (left.iter().map(|s| s * s).sum::<f64>() / left.len() as f64).sqrt();
    println!("Output RMS: {:.6}", rms);
    assert!(rms > 1e-6, "Output is silent (RMS = {})", rms);
}

#[test]
#[ignore]
fn kick_in_vs_kick_out_hit_count() {
    let kick_in = match load_wav("01_KickIn.wav") {
        Some(a) => a,
        None => return,
    };
    let kick_out = match load_wav("02_KickOut.wav") {
        Some(a) => a,
        None => return,
    };

    let events_in = run_detector(&kick_in, -30.0, 0.0);
    let events_out = run_detector(&kick_out, -30.0, 0.0);

    println!(
        "KickIn: {} hits, KickOut: {} hits",
        events_in.len(),
        events_out.len()
    );

    let diff = (events_in.len() as i64 - events_out.len() as i64).unsigned_abs() as usize;
    assert!(
        diff <= 2,
        "Hit count difference too large: {} (in={}, out={})",
        diff,
        events_in.len(),
        events_out.len()
    );
}
