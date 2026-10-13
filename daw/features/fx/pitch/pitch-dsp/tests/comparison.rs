//! Pitch algorithm comparison / benchmarking harness.
//!
//! Renders test signals through every pitch-shifting algorithm and writes WAV
//! files for A/B comparison against commercial plugins.
//!
//! Run with:
//!   cargo test -p pitch-dsp --test comparison -- --ignored --nocapture

use audiocore_dsp::{AudioConfig, Processor};
use pitch_dsp::chain::{semitones_to_ratio, Algorithm, PitchChain};

use std::f64::consts::PI;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SAMPLE_RATE: f64 = 48000.0;
const SAMPLE_RATE_U32: u32 = 48000;
const BLOCK_SIZE: usize = 512;
const WARMUP_BLOCKS: usize = 10;
const DURATION_S: f64 = 2.0;

const ALL_ALGORITHMS: [Algorithm; 8] = [
    Algorithm::FreqDivider,
    Algorithm::Pll,
    Algorithm::Granular,
    Algorithm::Psola,
    Algorithm::Wsola,
    Algorithm::Rubberband,
    Algorithm::Allpass,
    Algorithm::Spectral,
];

const OUTPUT_DIR: &str = "target/pitch-comparison";

// ---------------------------------------------------------------------------
// 1. Test signal generators
// ---------------------------------------------------------------------------

/// Generate a sine wave at `freq` Hz.
fn gen_sine(freq: f64, sample_rate: f64, duration_s: f64) -> Vec<f64> {
    let n = (sample_rate * duration_s) as usize;
    (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / sample_rate).sin() * 0.5)
        .collect()
}

/// Generate a logarithmic sine sweep from `start_hz` to `end_hz`.
fn gen_sweep(start_hz: f64, end_hz: f64, sample_rate: f64, duration_s: f64) -> Vec<f64> {
    let n = (sample_rate * duration_s) as usize;
    let ln_ratio = (end_hz / start_hz).ln();
    (0..n)
        .map(|i| {
            let t = i as f64 / sample_rate;
            // Instantaneous phase for log sweep:
            //   phi(t) = 2pi * start * T / ln(end/start) * (exp(t/T * ln(end/start)) - 1)
            let phase = 2.0 * PI * start_hz * duration_s / ln_ratio
                * ((t / duration_s * ln_ratio).exp() - 1.0);
            phase.sin() * 0.5
        })
        .collect()
}

/// Generate deterministic white noise using a simple xorshift64 PRNG.
fn gen_white_noise(sample_rate: f64, duration_s: f64, seed: u64) -> Vec<f64> {
    let n = (sample_rate * duration_s) as usize;
    let mut state = seed;
    (0..n)
        .map(|_| {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Map to -0.5..0.5
            state as f64 / u64::MAX as f64 - 0.5
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 2. WAV writer (16-bit PCM, mono, no external crate)
// ---------------------------------------------------------------------------

fn write_wav(path: &str, samples: &[f64], sample_rate: u32) {
    let num_samples = samples.len() as u32;
    let bits_per_sample: u16 = 16;
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * (bits_per_sample as u32 / 8) * num_channels as u32;
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = num_samples * (bits_per_sample as u32 / 8) * num_channels as u32;
    let file_size = 36 + data_size; // total minus 8 for RIFF header

    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let quantised = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&quantised.to_le_bytes());
    }

    fs::write(path, &buf).expect("failed to write WAV file");
}

/// Read a 16-bit mono WAV and return f64 samples. Returns None on any error.
fn read_wav(path: &str) -> Option<Vec<f64>> {
    let data = fs::read(path).ok()?;
    if data.len() < 44 {
        return None;
    }
    // Verify RIFF/WAVE
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    // Find data chunk — scan for "data" marker
    let mut pos = 12;
    while pos + 8 < data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                as usize;
        if chunk_id == b"data" {
            let start = pos + 8;
            let end = (start + chunk_size).min(data.len());
            let sample_bytes = &data[start..end];
            let samples: Vec<f64> = sample_bytes
                .chunks_exact(2)
                .map(|c| {
                    let val = i16::from_le_bytes([c[0], c[1]]);
                    val as f64 / 32767.0
                })
                .collect();
            return Some(samples);
        }
        pos += 8 + chunk_size;
        // Align to 2-byte boundary
        if !chunk_size.is_multiple_of(2) {
            pos += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 3. Analysis functions
// ---------------------------------------------------------------------------

/// Autocorrelation-based pitch detector. Returns frequency in Hz.
///
/// Searches for the strongest autocorrelation peak between `min_hz` and
/// `max_hz`, then refines with parabolic interpolation.
fn measure_pitch(samples: &[f64], sample_rate: f64) -> f64 {
    let min_hz = 30.0;
    let max_hz = 5000.0;

    let min_lag = (sample_rate / max_hz) as usize;
    let max_lag = ((sample_rate / min_hz) as usize).min(samples.len() / 2);

    if max_lag <= min_lag || samples.len() < max_lag * 2 {
        return 0.0;
    }

    // Use a fixed analysis window (not the entire signal) to avoid
    // normalized-autocorrelation degeneracy on long pure tones.
    let window_size = (sample_rate as usize).min(samples.len()); // ~1 second
    let analysis = &samples[samples.len() - window_size..];
    let n = analysis.len();

    let compute_corr = |lag: usize| -> f64 {
        let mut sum = 0.0;
        let mut ea = 0.0;
        let mut eb = 0.0;
        let count = n - lag;
        for i in 0..count {
            sum += analysis[i] * analysis[i + lag];
            ea += analysis[i] * analysis[i];
            eb += analysis[i + lag] * analysis[i + lag];
        }
        let d = (ea * eb).sqrt();
        if d > 1e-12 {
            sum / d
        } else {
            0.0
        }
    };

    // Find the first correlation peak above 0.8 threshold (fundamental).
    // Walk from min_lag upward; detect when correlation rises above threshold
    // after having been below it (a true peak, not the always-high lag=0 region).
    let threshold = 0.8;
    let mut prev_corr = compute_corr(min_lag);
    let mut best_lag = min_lag;
    let mut best_corr = prev_corr;
    let mut found_dip = prev_corr < threshold;

    for lag in (min_lag + 1)..=max_lag {
        let corr = compute_corr(lag);
        if corr < threshold {
            found_dip = true;
        }
        if found_dip && corr > threshold && corr < prev_corr {
            // We just passed a peak — the previous lag was the peak.
            best_lag = lag - 1;
            best_corr = prev_corr;
            break;
        }
        if found_dip && corr > best_corr {
            best_lag = lag;
            best_corr = corr;
        }
        prev_corr = corr;
    }

    // Parabolic interpolation around the peak for sub-sample accuracy.
    if best_lag > min_lag && best_lag < max_lag {
        let alpha = compute_corr(best_lag - 1);
        let beta = best_corr;
        let gamma = compute_corr(best_lag + 1);

        let denom = 2.0 * (2.0 * beta - alpha - gamma);
        if denom.abs() > 1e-12 {
            let correction = (alpha - gamma) / denom;
            let refined_lag = best_lag as f64 + correction;
            return sample_rate / refined_lag;
        }
    }

    sample_rate / best_lag as f64
}

/// Root mean square level.
fn measure_rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// Cross-correlation coefficient between two signals (0.0-1.0).
///
/// Uses normalized cross-correlation at zero lag, clamped to [0, 1].
fn spectral_similarity(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }

    let mut sum_ab = 0.0;
    let mut sum_aa = 0.0;
    let mut sum_bb = 0.0;
    for i in 0..n {
        sum_ab += a[i] * b[i];
        sum_aa += a[i] * a[i];
        sum_bb += b[i] * b[i];
    }

    let denom = (sum_aa * sum_bb).sqrt();
    if denom < 1e-12 {
        return 0.0;
    }
    (sum_ab / denom).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn algo_name(algo: Algorithm) -> &'static str {
    match algo {
        Algorithm::FreqDivider => "freqdiv",
        Algorithm::Pll => "pll",
        Algorithm::Granular => "granular",
        Algorithm::Psola => "psola",
        Algorithm::Wsola => "wsola",
        Algorithm::Rubberband => "rubberband",
        Algorithm::Allpass => "allpass",
        Algorithm::PolyOctave => "polyoctave",
        Algorithm::Spectral => "spectral",
    }
}

/// Whether an algorithm only supports octave shifts.
fn is_octave_only(algo: Algorithm) -> bool {
    matches!(
        algo,
        Algorithm::FreqDivider | Algorithm::Pll | Algorithm::PolyOctave
    )
}

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: BLOCK_SIZE,
    }
}

/// Render `input` through a PitchChain with the given algorithm and semitone shift.
///
/// Returns the output samples (after warmup blocks are discarded).
fn render(algo: Algorithm, semitones: f64, input: &[f64]) -> Vec<f64> {
    let mut chain = PitchChain::new();
    chain.algorithm = algo;
    chain.semitones = semitones;
    chain.mix = 1.0;
    chain.update(config());
    chain.reset();

    let total_blocks = input.len().div_ceil(BLOCK_SIZE);
    let mut output = Vec::with_capacity(input.len());

    for b in 0..total_blocks {
        let start = b * BLOCK_SIZE;
        let end = (start + BLOCK_SIZE).min(input.len());
        let block_len = end - start;

        let mut left = input[start..end].to_vec();
        // Pad last block if needed
        left.resize(BLOCK_SIZE, 0.0);
        let mut right = left.clone();

        chain.process(&mut left, &mut right);

        if b >= WARMUP_BLOCKS {
            output.extend_from_slice(&left[..block_len]);
        }
    }

    output
}

/// Convert frequency error to cents: 1200 * log2(measured / expected).
fn error_cents(measured: f64, expected: f64) -> f64 {
    if expected < 1.0 || measured < 1.0 {
        return f64::NAN;
    }
    1200.0 * (measured / expected).log2()
}

// ---------------------------------------------------------------------------
// Test signals
// ---------------------------------------------------------------------------

struct TestSignal {
    name: &'static str,
    samples: Vec<f64>,
    /// Fundamental frequency (if tonal) for pitch verification. 0 = noise.
    fundamental_hz: f64,
}

fn make_test_signals() -> Vec<TestSignal> {
    vec![
        TestSignal {
            name: "sine_440",
            samples: gen_sine(440.0, SAMPLE_RATE, DURATION_S),
            fundamental_hz: 440.0,
        },
        TestSignal {
            name: "sine_220",
            samples: gen_sine(220.0, SAMPLE_RATE, DURATION_S),
            fundamental_hz: 220.0,
        },
        TestSignal {
            name: "sweep_100_4000",
            samples: gen_sweep(100.0, 4000.0, SAMPLE_RATE, DURATION_S),
            fundamental_hz: 0.0, // sweep has no single fundamental
        },
    ]
}

// ---------------------------------------------------------------------------
// 4. Main comparison test
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn render_all_algorithms() {
    let out_dir = OUTPUT_DIR;
    fs::create_dir_all(out_dir).expect("failed to create output directory");

    let signals = make_test_signals();
    let semitone_values: &[f64] = &[-12.0, -7.0];

    // Write dry reference files
    for sig in &signals {
        let path = format!("{}/dry_{}.wav", out_dir, sig.name);
        write_wav(&path, &sig.samples, SAMPLE_RATE_U32);
        eprintln!("  wrote {}", path);
    }

    // Results table header
    eprintln!();
    eprintln!(
        "| {:<14} | {:<16} | {:>5} | {:>18} | {:>14} | {:>13} | {:>8} | {:>12} |",
        "Algorithm",
        "Signal",
        "Shift",
        "Output Pitch (Hz)",
        "Expected (Hz)",
        "Error (cents)",
        "RMS",
        "Latency"
    );
    eprintln!(
        "|{:-<16}|{:-<18}|{:-<7}|{:-<20}|{:-<16}|{:-<15}|{:-<10}|{:-<14}|",
        "", "", "", "", "", "", "", ""
    );

    for algo in ALL_ALGORITHMS {
        for st in semitone_values {
            // Skip non-octave shifts for octave-only algorithms
            if is_octave_only(algo) && *st != -12.0 && *st != -24.0 {
                continue;
            }

            for sig in &signals {
                let output = render(algo, *st, &sig.samples);

                // Write WAV
                let filename = format!(
                    "{}/{}_{}_{}st.wav",
                    out_dir,
                    algo_name(algo),
                    sig.name,
                    *st as i32
                );
                write_wav(&filename, &output, SAMPLE_RATE_U32);

                // Measure pitch on the last 50% of output (after transients)
                let analysis_start = output.len() / 2;
                let analysis_slice = &output[analysis_start..];

                let output_pitch = if sig.fundamental_hz > 0.0 {
                    measure_pitch(analysis_slice, SAMPLE_RATE)
                } else {
                    0.0
                };

                let expected_pitch = if sig.fundamental_hz > 0.0 {
                    sig.fundamental_hz * semitones_to_ratio(*st)
                } else {
                    0.0
                };

                let cents = if expected_pitch > 0.0 {
                    error_cents(output_pitch, expected_pitch)
                } else {
                    f64::NAN
                };

                let rms = measure_rms(analysis_slice);

                // Get latency
                let mut chain = PitchChain::new();
                chain.algorithm = algo;
                chain.semitones = *st;
                chain.update(config());
                // Run one block so sync_params executes
                let mut dummy_l = vec![0.0; BLOCK_SIZE];
                let mut dummy_r = vec![0.0; BLOCK_SIZE];
                chain.process(&mut dummy_l, &mut dummy_r);
                let latency = chain.latency();

                let pitch_str = if expected_pitch > 0.0 {
                    format!("{:.1}", output_pitch)
                } else {
                    "N/A".to_string()
                };

                let expected_str = if expected_pitch > 0.0 {
                    format!("{:.1}", expected_pitch)
                } else {
                    "N/A".to_string()
                };

                let cents_str = if cents.is_finite() {
                    format!("{:.1}", cents)
                } else {
                    "N/A".to_string()
                };

                eprintln!(
                    "| {:<14} | {:<16} | {:>5} | {:>18} | {:>14} | {:>13} | {:>8.4} | {:>7} smp |",
                    algo_name(algo),
                    sig.name,
                    format!("{}st", *st as i32),
                    pitch_str,
                    expected_str,
                    cents_str,
                    rms,
                    latency,
                );
            }
        }
    }

    eprintln!();
    eprintln!("WAV files written to {}/", out_dir);
}

// ---------------------------------------------------------------------------
// 5. Reference comparison test
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn compare_with_reference() {
    let ref_dir = format!("{}/reference", OUTPUT_DIR);

    if !Path::new(&ref_dir).exists() {
        eprintln!();
        eprintln!("=== Reference comparison skipped ===");
        eprintln!();
        eprintln!("Place reference renders in {}/ named like:", ref_dir);
        eprintln!("  {{plugin}}_{{signal}}_{{semitones}}st.wav");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  soundtoys_sine_440_-12st.wav");
        eprintln!("  eventide_sweep_100_4000_-7st.wav");
        eprintln!();
        eprintln!("Then re-run this test to see spectral similarity scores.");
        return;
    }

    let signals = make_test_signals();
    let semitone_values: &[f64] = &[-12.0, -7.0];

    // Collect reference WAV filenames
    let ref_files: Vec<String> = fs::read_dir(&ref_dir)
        .expect("failed to read reference directory")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".wav") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    if ref_files.is_empty() {
        eprintln!("No .wav files found in {}/", ref_dir);
        return;
    }

    eprintln!();
    eprintln!("=== Reference Comparison ===");
    eprintln!();
    eprintln!(
        "| {:<14} | {:<16} | {:>5} | {:<30} | {:>10} |",
        "Algorithm", "Signal", "Shift", "Reference", "Similarity"
    );
    eprintln!(
        "|{:-<16}|{:-<18}|{:-<7}|{:-<32}|{:-<12}|",
        "", "", "", "", ""
    );

    for algo in ALL_ALGORITHMS {
        for st in semitone_values {
            if is_octave_only(algo) && *st != -12.0 && *st != -24.0 {
                continue;
            }

            for sig in &signals {
                // Render our output
                let output = render(algo, *st, &sig.samples);

                // Compare against each matching reference file
                for ref_name in &ref_files {
                    // Check if reference matches this signal + semitones
                    let sig_tag = sig.name;
                    let st_tag = format!("{}st", *st as i32);

                    if !ref_name.contains(sig_tag) || !ref_name.contains(&st_tag) {
                        continue;
                    }

                    let ref_path = format!("{}/{}", ref_dir, ref_name);
                    let ref_samples = match read_wav(&ref_path) {
                        Some(s) => s,
                        None => {
                            eprintln!("  warning: could not read {}", ref_path);
                            continue;
                        }
                    };

                    let similarity = spectral_similarity(&output, &ref_samples);

                    eprintln!(
                        "| {:<14} | {:<16} | {:>5} | {:<30} | {:>9.4} |",
                        algo_name(algo),
                        sig.name,
                        format!("{}st", *st as i32),
                        ref_name,
                        similarity,
                    );
                }
            }
        }
    }

    eprintln!();
}

// ---------------------------------------------------------------------------
// Sanity tests (not ignored — run in CI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod sanity {
    use super::*;

    #[test]
    fn gen_sine_correct_frequency() {
        let samples = gen_sine(440.0, SAMPLE_RATE, 2.0);
        let pitch = measure_pitch(&samples, SAMPLE_RATE);
        let err = error_cents(pitch, 440.0).abs();
        assert!(
            err < 5.0,
            "Sine generator pitch error too large: {:.1} cents (measured {:.1} Hz)",
            err,
            pitch
        );
    }

    #[test]
    fn gen_sweep_length() {
        let samples = gen_sweep(100.0, 4000.0, SAMPLE_RATE, 1.0);
        assert_eq!(samples.len(), SAMPLE_RATE as usize);
    }

    #[test]
    fn gen_noise_deterministic() {
        let a = gen_white_noise(SAMPLE_RATE, 0.1, 42);
        let b = gen_white_noise(SAMPLE_RATE, 0.1, 42);
        assert_eq!(a, b, "Noise with same seed must be identical");

        let c = gen_white_noise(SAMPLE_RATE, 0.1, 99);
        assert_ne!(a, c, "Noise with different seed should differ");
    }

    #[test]
    fn rms_known_value() {
        // RMS of a sine wave with amplitude A is A / sqrt(2)
        let samples = gen_sine(440.0, SAMPLE_RATE, 1.0);
        let rms = measure_rms(&samples);
        let expected = 0.5 / (2.0f64).sqrt(); // amplitude 0.5
        assert!(
            (rms - expected).abs() < 0.001,
            "RMS: expected {:.4}, got {:.4}",
            expected,
            rms
        );
    }

    #[test]
    fn spectral_similarity_identical() {
        let a = gen_sine(440.0, SAMPLE_RATE, 0.5);
        let sim = spectral_similarity(&a, &a);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical signals should have similarity 1.0, got {}",
            sim
        );
    }

    #[test]
    fn spectral_similarity_different() {
        let a = gen_sine(440.0, SAMPLE_RATE, 0.5);
        let b = gen_sine(880.0, SAMPLE_RATE, 0.5);
        let sim = spectral_similarity(&a, &b);
        assert!(
            sim < 0.5,
            "Different frequencies should have low similarity, got {}",
            sim
        );
    }

    #[test]
    fn wav_roundtrip() {
        let dir = std::env::temp_dir();
        let path = format!("{}/pitch_dsp_test_roundtrip.wav", dir.display());

        let original = gen_sine(440.0, SAMPLE_RATE, 0.1);
        write_wav(&path, &original, SAMPLE_RATE_U32);

        let loaded = read_wav(&path).expect("failed to read WAV back");
        assert_eq!(original.len(), loaded.len(), "WAV sample count mismatch");

        // 16-bit quantisation error should be < 1/32768
        let max_err: f64 = original
            .iter()
            .zip(loaded.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(
            max_err < 0.0001,
            "WAV roundtrip error too large: {}",
            max_err
        );

        let _ = fs::remove_file(&path);
    }
}
