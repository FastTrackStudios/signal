//! Pitch-DSP throughput benchmark — runs all algorithms in parallel.
//!
//! Each algorithm gets its own thread, processes 10 seconds of audio,
//! and reports realtime ratio + latency. Total wall-clock time is
//! determined by the slowest algorithm, not the sum.
//!
//! Run:  cargo bench -p pitch-dsp --bench throughput
//!
//! Uses ATD_Dataset vocal clips if available, falls back to synthetic sine.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use audiocore_dsp::{AudioConfig, Processor};
use pitch_dsp::chain::{Algorithm, PitchChain};

const SAMPLE_RATE: f64 = 48000.0;
const DURATION_SECS: f64 = 10.0;
const TOTAL_SAMPLES: usize = (SAMPLE_RATE * DURATION_SECS) as usize;
const BLOCK_SIZE: usize = 512;
const RUNS: usize = 5;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: BLOCK_SIZE,
    }
}

fn gen_sine(freq: f64, num_samples: usize) -> Vec<f64> {
    use std::f64::consts::PI;
    (0..num_samples)
        .map(|i| (2.0 * PI * freq * i as f64 / SAMPLE_RATE).sin() * 0.5)
        .collect()
}

fn load_wav_mono(path: &std::path::Path, max_samples: usize) -> Option<Vec<f64>> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let step = spec.channels as usize;
    let samples: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f64;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .enumerate()
                .filter(|(i, _)| i % step == 0)
                .map(|(_, s)| s as f64 / max)
                .take(max_samples)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .enumerate()
            .filter(|(i, _)| i % step == 0)
            .map(|(_, s)| s as f64)
            .take(max_samples)
            .collect(),
    };
    if samples.is_empty() {
        None
    } else {
        Some(samples)
    }
}

/// Load ~10s clips from ATD_Dataset (diverse vocal content).
fn load_atd_clips() -> Option<Vec<(String, Arc<Vec<f64>>)>> {
    let base = std::path::Path::new(env!("HOME")).join("Downloads/ATD_Dataset/Training");
    if !base.exists() {
        return None;
    }

    let clip_paths = [
        ("female_belt", "female1/f1_arpeggios_belt_c_a/Original.wav"),
        (
            "female_breathy",
            "female1/f1_arpeggios_breathy_a/Original.wav",
        ),
        ("male_belt", "male1/m1_arpeggios_belt_c_a/Original.wav"),
    ];

    let mut clips = Vec::new();
    for (name, rel) in &clip_paths {
        let path = base.join(rel);
        if let Some(samples) = load_wav_mono(&path, TOTAL_SAMPLES) {
            clips.push((name.to_string(), Arc::new(samples)));
        }
    }

    if clips.is_empty() {
        None
    } else {
        Some(clips)
    }
}

struct BenchResult {
    algo_name: &'static str,
    mode: &'static str,
    clip_name: String,
    latency_samples: usize,
    latency_ms: f64,
    median_realtime_x: f64,
    min_realtime_x: f64,
    max_realtime_x: f64,
}

fn bench_one(
    algo: Algorithm,
    algo_name: &'static str,
    live: bool,
    semitones: f64,
    clip_name: &str,
    audio: &[f64],
) -> BenchResult {
    let mode = if live { "live" } else { "standard" };
    let blocks: Vec<&[f64]> = audio.chunks(BLOCK_SIZE).collect();
    let audio_len = audio.len();

    let mut times = Vec::with_capacity(RUNS);

    for _ in 0..RUNS {
        let mut chain = PitchChain::new();
        chain.algorithm = algo;
        chain.semitones = semitones;
        chain.mix = 1.0;
        chain.live = live;
        chain.update(config());
        chain.reset();

        let start = Instant::now();
        for blk in &blocks {
            let mut left = blk.to_vec();
            let mut right = left.clone();
            chain.process(&mut left, &mut right);
            std::hint::black_box(&left);
        }
        times.push(start.elapsed());
    }

    times.sort();
    let audio_secs = audio_len as f64 / SAMPLE_RATE;

    let to_rt = |d: std::time::Duration| audio_secs / d.as_secs_f64();

    // Get latency from a fresh chain.
    let mut chain = PitchChain::new();
    chain.algorithm = algo;
    chain.semitones = semitones;
    chain.live = live;
    chain.update(config());
    let mut l = vec![0.0; BLOCK_SIZE];
    let mut r = l.clone();
    chain.process(&mut l, &mut r);
    let lat = chain.latency();

    BenchResult {
        algo_name,
        mode,
        clip_name: clip_name.to_string(),
        latency_samples: lat,
        latency_ms: lat as f64 / SAMPLE_RATE * 1000.0,
        median_realtime_x: to_rt(times[RUNS / 2]),
        min_realtime_x: to_rt(times[RUNS - 1]), // slowest run
        max_realtime_x: to_rt(times[0]),        // fastest run
    }
}

fn main() {
    eprintln!("pitch-dsp throughput benchmark");
    eprintln!("  {TOTAL_SAMPLES} samples ({DURATION_SECS}s) per clip, {RUNS} runs each, block size {BLOCK_SIZE}");
    eprintln!();

    // Load audio clips.
    let clips: Vec<(String, Arc<Vec<f64>>)> = match load_atd_clips() {
        Some(c) => {
            eprintln!("  Loaded {} ATD_Dataset clips", c.len());
            c
        }
        None => {
            eprintln!("  ATD_Dataset not found, using synthetic sine");
            vec![(
                "sine_440hz".to_string(),
                Arc::new(gen_sine(440.0, TOTAL_SAMPLES)),
            )]
        }
    };

    let all_algos: &[(Algorithm, &str, f64)] = &[
        (Algorithm::FreqDivider, "freq_divider", -12.0),
        (Algorithm::Pll, "pll", -12.0),
        (Algorithm::Granular, "granular", -7.0),
        (Algorithm::Psola, "psola", -7.0),
        (Algorithm::Wsola, "wsola", -7.0),
        (Algorithm::Rubberband, "rubberband", -7.0),
        (Algorithm::Allpass, "allpass", -7.0),
    ];

    // Spawn all benchmarks in parallel: algo x mode x clip.
    let mut handles = Vec::new();

    for &(algo, name, st) in all_algos {
        for &live in &[false, true] {
            for (clip_name, audio) in &clips {
                let audio = Arc::clone(audio);
                let clip_name = clip_name.clone();
                handles.push(thread::spawn(move || {
                    bench_one(algo, name, live, st, &clip_name, &audio)
                }));
            }
        }
    }

    let mut results: Vec<BenchResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Sort by mode, then algo name for clean output.
    results.sort_by(|a, b| {
        a.mode
            .cmp(b.mode)
            .then(a.algo_name.cmp(b.algo_name))
            .then(a.clip_name.cmp(&b.clip_name))
    });

    // Print results table.
    eprintln!();
    println!(
        "┌────────────────┬──────────┬─────────────────┬────────┬────────────────────────────────┐"
    );
    println!(
        "│ Algorithm      │ Mode     │ Clip            │Lat(ms) │ Realtime X (min/median/max)    │"
    );
    println!(
        "├────────────────┼──────────┼─────────────────┼────────┼────────────────────────────────┤"
    );

    for r in &results {
        let bar_len = (r.median_realtime_x.min(200.0) / 10.0) as usize;
        let bar: String = "█".repeat(bar_len);
        let status = if r.median_realtime_x >= 1.0 {
            " OK"
        } else {
            " !!"
        };

        println!(
            "│ {:<14} │ {:<8} │ {:<15} │{:>5.1}ms │ {:>6.0}x/{:>6.0}x/{:>6.0}x {}{} │",
            r.algo_name,
            r.mode,
            &r.clip_name[..r.clip_name.len().min(15)],
            r.latency_ms,
            r.min_realtime_x,
            r.median_realtime_x,
            r.max_realtime_x,
            bar,
            status,
        );
    }

    println!(
        "└────────────────┴──────────┴─────────────────┴────────┴────────────────────────────────┘"
    );

    // Summary: best candidates for low-latency.
    println!();
    println!("=== Low-Latency Candidates (live mode, latency < 5ms) ===");
    let mut candidates: Vec<&BenchResult> = results
        .iter()
        .filter(|r| r.mode == "live" && r.latency_ms < 5.0)
        .collect();
    candidates.sort_by(|a, b| a.latency_ms.partial_cmp(&b.latency_ms).unwrap());

    for r in &candidates {
        println!(
            "  {:<14}  latency: {:>5.1}ms ({:>4} smp)  speed: {:>6.0}x realtime",
            r.algo_name, r.latency_ms, r.latency_samples, r.median_realtime_x,
        );
    }

    if candidates.is_empty() {
        println!("  (none under 5ms)");
    }
}
