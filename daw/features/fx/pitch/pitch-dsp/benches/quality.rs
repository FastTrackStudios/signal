//! Pitch-DSP quality benchmark — Rubberband as reference.
//!
//! Processes audio through Rubberband (quality ceiling) then compares every
//! other algorithm against it. This answers: "how much quality do I trade
//! for lower latency and higher speed?"
//!
//! Metrics (all measured vs Rubberband reference output):
//!   - **SI-SDR (dB)**: signal similarity (higher = closer to Rubberband)
//!   - **MCD (dB)**: mel cepstral distortion / timbre match (lower = better)
//!   - **Spectral Convergence**: spectral fidelity (lower = better, 0 = identical)
//!   - **F0 RMSE (cents)**: absolute pitch accuracy (lower = better)
//!
//! Datasets: synthetic sine, EGFxSet guitar DI, MDB-stem-synth vocals
//!
//! Run:  cargo bench -p pitch-dsp --bench quality

use std::f64::consts::PI;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use audiocore_dsp::{AudioConfig, Processor};
use pitch_dsp::chain::{Algorithm, PitchChain};
use rustfft::{num_complex::Complex, FftPlanner};

const SAMPLE_RATE: f64 = 48000.0;
const BLOCK_SIZE: usize = 512;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: BLOCK_SIZE,
    }
}

// Algorithms to compare (everything except Rubberband, which is the reference).
const TEST_ALGOS: &[(Algorithm, &str)] = &[
    (Algorithm::FreqDivider, "freq_divider"),
    (Algorithm::Pll, "pll"),
    (Algorithm::Granular, "granular"),
    (Algorithm::Psola, "psola"),
    (Algorithm::Wsola, "wsola"),
    (Algorithm::Allpass, "allpass"),
];

// ===========================================================================
// Quality metrics (all compare against a reference signal)
// ===========================================================================

/// Scale-Invariant Signal-to-Distortion Ratio (dB). Higher = closer to reference.
fn si_sdr(reference: &[f64], estimate: &[f64]) -> f64 {
    let len = reference.len().min(estimate.len());
    let (r, e) = (&reference[..len], &estimate[..len]);

    let dot_re: f64 = r.iter().zip(e).map(|(a, b)| a * b).sum();
    let dot_rr: f64 = r.iter().map(|a| a * a).sum();
    if dot_rr < 1e-20 {
        return f64::NEG_INFINITY;
    }

    let alpha = dot_re / dot_rr;
    let s_target: f64 = r.iter().map(|a| (alpha * a).powi(2)).sum();
    let noise: f64 = r.iter().zip(e).map(|(a, b)| (alpha * a - b).powi(2)).sum();
    if noise < 1e-20 {
        return 100.0;
    }
    10.0 * (s_target / noise).log10()
}

/// Spectral Convergence. Lower = better (0 = identical).
fn spectral_convergence(reference: &[f64], estimate: &[f64]) -> f64 {
    let fft_size = 2048;
    let hop = fft_size / 4;
    let ref_spec = stft_magnitude(reference, fft_size, hop);
    let est_spec = stft_magnitude(estimate, fft_size, hop);

    let frames = ref_spec.len().min(est_spec.len());
    if frames == 0 {
        return 1.0;
    }
    let bins = ref_spec[0].len();

    let (mut diff_e, mut ref_e) = (0.0, 0.0);
    for f in 0..frames {
        for b in 0..bins {
            let d = ref_spec[f][b] - est_spec[f][b];
            diff_e += d * d;
            ref_e += ref_spec[f][b] * ref_spec[f][b];
        }
    }
    if ref_e < 1e-20 {
        1.0
    } else {
        (diff_e / ref_e).sqrt()
    }
}

/// Mel Cepstral Distortion (dB). Lower = better. <3 dB near-transparent, <5 dB good.
fn mel_cepstral_distortion(reference: &[f64], estimate: &[f64]) -> f64 {
    let fft_size = 2048;
    let hop = fft_size / 4;
    let n_mels = 40;
    let n_mfcc = 13;

    let ref_mfccs = extract_mfccs(reference, SAMPLE_RATE, fft_size, hop, n_mels, n_mfcc);
    let est_mfccs = extract_mfccs(estimate, SAMPLE_RATE, fft_size, hop, n_mels, n_mfcc);

    let frames = ref_mfccs.len().min(est_mfccs.len());
    if frames == 0 {
        return f64::INFINITY;
    }

    let mut total = 0.0;
    for f in 0..frames {
        let mut dist = 0.0;
        for c in 1..n_mfcc {
            let d = ref_mfccs[f][c] - est_mfccs[f][c];
            dist += d * d;
        }
        total += dist.sqrt();
    }

    (10.0 * 2.0_f64.sqrt() / 10.0_f64.ln()) * total / frames as f64
}

/// F0 RMSE in cents. Absolute pitch accuracy measurement.
fn f0_rmse_cents(signal: &[f64], expected_hz: f64, sample_rate: f64) -> f64 {
    // Use a window large enough for low frequencies (down to ~30 Hz).
    let window = 4096;
    let hop = 512;
    let mut errors_sq = Vec::new();

    let mut pos = 0;
    while pos + window <= signal.len() {
        let frame = &signal[pos..pos + window];
        let rms: f64 = (frame.iter().map(|s| s * s).sum::<f64>() / frame.len() as f64).sqrt();
        if rms < 0.01 {
            pos += hop;
            continue;
        }

        if let Some(detected_hz) = detect_pitch_yin(frame, sample_rate) {
            if detected_hz > 20.0 && detected_hz < sample_rate / 2.0 {
                let cents = 1200.0 * (detected_hz / expected_hz).log2();
                errors_sq.push(cents * cents);
            }
        }
        pos += hop;
    }

    if errors_sq.is_empty() {
        return f64::INFINITY;
    }
    (errors_sq.iter().sum::<f64>() / errors_sq.len() as f64).sqrt()
}

// ===========================================================================
// DSP helpers
// ===========================================================================

fn stft_magnitude(signal: &[f64], fft_size: usize, hop: usize) -> Vec<Vec<f64>> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let bins = fft_size / 2 + 1;

    let hann: Vec<f64> = (0..fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / fft_size as f64).cos()))
        .collect();

    let mut frames = Vec::new();
    let mut pos = 0;
    while pos + fft_size <= signal.len() {
        let mut buf: Vec<Complex<f64>> = signal[pos..pos + fft_size]
            .iter()
            .zip(hann.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();
        fft.process(&mut buf);
        frames.push(buf[..bins].iter().map(|c| c.norm()).collect());
        pos += hop;
    }
    frames
}

fn extract_mfccs(
    signal: &[f64],
    sample_rate: f64,
    fft_size: usize,
    hop: usize,
    n_mels: usize,
    n_mfcc: usize,
) -> Vec<Vec<f64>> {
    let mel_fb = mel_filterbank(fft_size, sample_rate, n_mels);
    let spec = stft_magnitude(signal, fft_size, hop);

    spec.iter()
        .map(|frame| {
            let mel_energies: Vec<f64> = mel_fb
                .iter()
                .map(|filter| {
                    let e: f64 = filter.iter().zip(frame).map(|(&w, &m)| w * m * m).sum();
                    (e + 1e-10).ln()
                })
                .collect();
            dct_ii(&mel_energies, n_mfcc)
        })
        .collect()
}

fn mel_filterbank(fft_size: usize, sample_rate: f64, n_mels: usize) -> Vec<Vec<f64>> {
    let bins = fft_size / 2 + 1;
    let hz_to_mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
    let mel_to_hz = |m: f64| 700.0 * (10.0_f64.powf(m / 2595.0) - 1.0);

    let mel_min = hz_to_mel(20.0);
    let mel_max = hz_to_mel(sample_rate / 2.0);

    let mel_pts: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64)
        .collect();
    let bin_pts: Vec<f64> = mel_pts
        .iter()
        .map(|&m| mel_to_hz(m) * fft_size as f64 / sample_rate)
        .collect();

    (0..n_mels)
        .map(|m| {
            let (left, center, right) = (bin_pts[m], bin_pts[m + 1], bin_pts[m + 2]);
            (0..bins)
                .map(|b| {
                    let bf = b as f64;
                    if bf < left || bf > right {
                        0.0
                    } else if bf <= center {
                        if (center - left).abs() < 1e-10 {
                            1.0
                        } else {
                            (bf - left) / (center - left)
                        }
                    } else if (right - center).abs() < 1e-10 {
                        1.0
                    } else {
                        (right - bf) / (right - center)
                    }
                })
                .collect()
        })
        .collect()
}

fn dct_ii(input: &[f64], n_out: usize) -> Vec<f64> {
    let n = input.len();
    (0..n_out)
        .map(|k| {
            input
                .iter()
                .enumerate()
                .map(|(i, &x)| {
                    x * (PI * k as f64 * (2.0 * i as f64 + 1.0) / (2.0 * n as f64)).cos()
                })
                .sum()
        })
        .collect()
}

/// YIN pitch detection — more robust than raw autocorrelation, handles low frequencies.
fn detect_pitch_yin(frame: &[f64], sample_rate: f64) -> Option<f64> {
    let min_period = (sample_rate / 2000.0) as usize;
    let max_period = (sample_rate / 30.0).min(frame.len() as f64 / 2.0) as usize;
    if min_period >= max_period || max_period > frame.len() / 2 {
        return None;
    }

    // Cumulative mean normalized difference function (CMND).
    let mut d = vec![0.0f64; max_period + 1];
    d[0] = 1.0;
    let mut running_sum = 0.0;

    for tau in 1..=max_period {
        let mut sum = 0.0;
        for i in 0..frame.len() - tau {
            let diff = frame[i] - frame[i + tau];
            sum += diff * diff;
        }
        running_sum += sum;
        d[tau] = if running_sum > 0.0 {
            sum * tau as f64 / running_sum
        } else {
            1.0
        };
    }

    // Find first dip below threshold (0.2) after min_period.
    let threshold = 0.2;
    let mut best_tau = None;

    for tau in min_period..max_period {
        if d[tau] < threshold {
            // Find the local minimum.
            let mut min_tau = tau;
            while min_tau + 1 < max_period && d[min_tau + 1] < d[min_tau] {
                min_tau += 1;
            }
            best_tau = Some(min_tau);
            break;
        }
    }

    // Fallback: absolute minimum if no dip below threshold.
    let best_tau = best_tau.or_else(|| {
        let mut min_val = f64::MAX;
        let mut min_t = min_period;
        for (tau, &val) in d.iter().enumerate().take(max_period).skip(min_period) {
            if val < min_val {
                min_val = val;
                min_t = tau;
            }
        }
        if min_val < 0.5 {
            Some(min_t)
        } else {
            None
        }
    })?;

    // Parabolic interpolation.
    if best_tau > min_period && best_tau + 1 < max_period {
        let a = d[best_tau - 1];
        let b = d[best_tau];
        let c = d[best_tau + 1];
        let denom = a - 2.0 * b + c;
        if denom.abs() > 1e-10 {
            let delta = 0.5 * (a - c) / denom;
            return Some(sample_rate / (best_tau as f64 + delta));
        }
    }

    Some(sample_rate / best_tau as f64)
}

// ===========================================================================
// Audio loading
// ===========================================================================

fn load_wav_mono(path: &Path, target_sr: f64) -> Option<Vec<f64>> {
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
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .enumerate()
            .filter(|(i, _)| i % step == 0)
            .map(|(_, s)| s as f64)
            .collect(),
    };

    if (spec.sample_rate as f64 - target_sr).abs() > 1.0 {
        let ratio = target_sr / spec.sample_rate as f64;
        let new_len = (samples.len() as f64 * ratio) as usize;
        Some(
            (0..new_len)
                .map(|i| {
                    let src = (i as f64 / ratio).min((samples.len() - 1) as f64);
                    let idx = src as usize;
                    let frac = src - idx as f64;
                    if idx + 1 < samples.len() {
                        samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
                    } else {
                        samples[idx]
                    }
                })
                .collect(),
        )
    } else {
        Some(samples)
    }
}

fn load_mdb_annotation(path: &Path) -> Option<Vec<(f64, f64)>> {
    let content = std::fs::read_to_string(path).ok()?;
    let entries: Vec<(f64, f64)> = content
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(',');
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn egfxset_filename_to_midi(filename: &str) -> Option<u8> {
    let stem = filename.strip_suffix(".wav")?;
    let (s, f) = stem.split_once('-')?;
    let string: u8 = s.parse().ok()?;
    let fret: u8 = f.parse().ok()?;
    let open_midi = match string {
        1 => 64,
        2 => 59,
        3 => 55,
        4 => 50,
        5 => 45,
        6 => 40,
        _ => return None,
    };
    Some(open_midi + fret)
}

fn midi_to_hz(midi: u8) -> f64 {
    440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
}

// ===========================================================================
// Processing
// ===========================================================================

fn process_audio(algo: Algorithm, semitones: f64, live: bool, input: &[f64]) -> Vec<f64> {
    let mut chain = PitchChain::new();
    chain.algorithm = algo;
    chain.semitones = semitones;
    chain.mix = 1.0;
    chain.live = live;
    chain.update(config());
    chain.reset();

    let mut output = Vec::with_capacity(input.len());
    for chunk in input.chunks(BLOCK_SIZE) {
        let mut left = chunk.to_vec();
        left.resize(BLOCK_SIZE, 0.0);
        let mut right = left.clone();
        chain.process(&mut left, &mut right);
        output.extend_from_slice(&left[..chunk.len()]);
    }
    output
}

// ===========================================================================
// Results
// ===========================================================================

struct QualityResult {
    algo_name: &'static str,
    source: String,
    f0_rmse_cents: f64,
    si_sdr_db: f64,
    mcd_db: f64,
    spec_conv: f64,
    latency_ms: f64,
}

/// Compare an algorithm's output against Rubberband reference.
fn compare_vs_reference(
    algo: Algorithm,
    algo_name: &'static str,
    semitones: f64,
    input: &[f64],
    rb_output: &[f64],
    expected_hz: f64,
    source: String,
) -> QualityResult {
    let output = process_audio(algo, semitones, true, input);

    // Trim warmup from both signals.
    let skip = (SAMPLE_RATE * 0.5) as usize;
    let out = if output.len() > skip {
        &output[skip..]
    } else {
        &output
    };
    let rb = if rb_output.len() > skip {
        &rb_output[skip..]
    } else {
        rb_output
    };

    let len = out.len().min(rb.len());
    let out = &out[..len];
    let rb = &rb[..len];

    // Get latency.
    let mut chain = PitchChain::new();
    chain.algorithm = algo;
    chain.semitones = semitones;
    chain.live = true;
    chain.update(config());
    let mut l = vec![0.0; BLOCK_SIZE];
    let mut r = l.clone();
    chain.process(&mut l, &mut r);
    let lat = chain.latency();

    QualityResult {
        algo_name,
        source,

        f0_rmse_cents: f0_rmse_cents(out, expected_hz, SAMPLE_RATE),
        si_sdr_db: si_sdr(rb, out),
        mcd_db: mel_cepstral_distortion(rb, out),
        spec_conv: spectral_convergence(rb, out),
        latency_ms: lat as f64 / SAMPLE_RATE * 1000.0,
    }
}

// ===========================================================================
// Test suites
// ===========================================================================

fn bench_sine() -> Vec<QualityResult> {
    let duration = 10.0;
    let n = (SAMPLE_RATE * duration) as usize;

    let freqs = [220.0, 440.0, 880.0];
    let shifts = [-12.0, -7.0, -3.0, 3.0, 7.0, 12.0];

    let mut handles = Vec::new();

    for &freq in &freqs {
        let input: Arc<Vec<f64>> = Arc::new(
            (0..n)
                .map(|i| (2.0 * PI * freq * i as f64 / SAMPLE_RATE).sin() * 0.5)
                .collect(),
        );

        for &shift in &shifts {
            // Generate Rubberband reference.
            let rb_output = Arc::new(process_audio(Algorithm::Rubberband, shift, false, &input));
            let expected_hz = freq * 2.0_f64.powf(shift / 12.0);

            for &(algo, name) in TEST_ALGOS {
                // FreqDivider/PLL only do octave down.
                if matches!(algo, Algorithm::FreqDivider | Algorithm::Pll) && shift != -12.0 {
                    continue;
                }

                let input = Arc::clone(&input);
                let rb_output = Arc::clone(&rb_output);
                let source = format!("sine_{freq:.0}hz_{shift:+.0}st");
                handles.push(thread::spawn(move || {
                    compare_vs_reference(algo, name, shift, &input, &rb_output, expected_hz, source)
                }));
            }

            // Also report Rubberband's own F0 accuracy (self-reference = perfect SI-SDR).
            let rb_out2 = Arc::clone(&rb_output);
            let source = format!("sine_{freq:.0}hz_{shift:+.0}st");
            handles.push(thread::spawn(move || {
                let skip = (SAMPLE_RATE * 0.5) as usize;
                let out = &rb_out2[skip..];
                QualityResult {
                    algo_name: "RUBBERBAND*",
                    source,

                    f0_rmse_cents: f0_rmse_cents(out, expected_hz, SAMPLE_RATE),
                    si_sdr_db: f64::INFINITY, // self-reference
                    mcd_db: 0.0,
                    spec_conv: 0.0,
                    latency_ms: 21.3,
                }
            }));
        }
    }

    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

fn bench_egfxset() -> Vec<QualityResult> {
    let base = Path::new(env!("HOME")).join("Downloads/EGFxSet/Clean/Bridge");
    if !base.exists() {
        eprintln!("  EGFxSet not found, skipping");
        return Vec::new();
    }

    // Spread across the guitar range, avoid very low notes where detection struggles.
    let test_files = [
        "1-0.wav", "1-5.wav", "1-12.wav", // E4, A4, E5
        "2-3.wav", "3-0.wav", "3-7.wav", // D4, G3, D4
        "4-0.wav", "4-5.wav", // D3, G3
        "5-0.wav", "5-7.wav", // A2, E3
    ];

    let shift = -5.0;
    let mut handles = Vec::new();

    for filename in &test_files {
        let path = base.join(filename);
        let midi = match egfxset_filename_to_midi(filename) {
            Some(m) => m,
            None => continue,
        };
        let original_hz = midi_to_hz(midi);
        let expected_hz = original_hz * 2.0_f64.powf(shift / 12.0);

        let audio = match load_wav_mono(&path, SAMPLE_RATE) {
            Some(a) => Arc::new(a),
            None => continue,
        };

        // Rubberband reference.
        let rb_output = Arc::new(process_audio(Algorithm::Rubberband, shift, false, &audio));

        let fname = filename.to_string();

        for &(algo, name) in TEST_ALGOS {
            if matches!(algo, Algorithm::FreqDivider | Algorithm::Pll) {
                continue;
            }

            let audio = Arc::clone(&audio);
            let rb_output = Arc::clone(&rb_output);
            let source = format!("guitar_{fname}");
            handles.push(thread::spawn(move || {
                compare_vs_reference(algo, name, shift, &audio, &rb_output, expected_hz, source)
            }));
        }

        // Rubberband self-entry.
        let rb_out2 = Arc::clone(&rb_output);
        let source = format!("guitar_{fname}");
        handles.push(thread::spawn(move || {
            let skip = (SAMPLE_RATE * 0.5) as usize;
            let out = &rb_out2[skip..];
            QualityResult {
                algo_name: "RUBBERBAND*",
                source,

                f0_rmse_cents: f0_rmse_cents(out, expected_hz, SAMPLE_RATE),
                si_sdr_db: f64::INFINITY,
                mcd_db: 0.0,
                spec_conv: 0.0,
                latency_ms: 21.3,
            }
        }));
    }

    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

fn bench_mdb() -> Vec<QualityResult> {
    let base = Path::new(env!("HOME")).join("Downloads/MDB-stem-synth/MDB-stem-synth");
    let audio_dir = base.join("audio_stems");
    let annot_dir = base.join("annotation_stems");

    if !audio_dir.exists() {
        eprintln!("  MDB-stem-synth not found, skipping");
        return Vec::new();
    }

    // Find stems with substantial pitched content (>30% voiced frames).
    let mut good_stems: Vec<(String, f64)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&annot_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "csv") {
                continue;
            }

            if let Some(annot) = load_mdb_annotation(&path) {
                let total = annot.len();
                let voiced: Vec<f64> = annot
                    .iter()
                    .filter(|(_, f)| *f > 50.0)
                    .map(|(_, f)| *f)
                    .collect();
                let voiced_ratio = voiced.len() as f64 / total as f64;

                // Only keep stems that are >30% voiced and have a reasonable average pitch.
                if voiced_ratio > 0.3 && !voiced.is_empty() {
                    let avg_f0 = voiced.iter().sum::<f64>() / voiced.len() as f64;
                    if avg_f0 > 80.0 && avg_f0 < 1000.0 {
                        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                        good_stems.push((stem, avg_f0));
                    }
                }
            }
        }
    }

    // Take up to 10 diverse stems.
    good_stems.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let step = (good_stems.len() / 10).max(1);
    let selected: Vec<_> = good_stems.iter().step_by(step).take(10).cloned().collect();

    eprintln!(
        "  MDB: found {} good stems, selected {}",
        good_stems.len(),
        selected.len()
    );

    let shift = -5.0;
    let max_samples = (SAMPLE_RATE * 10.0) as usize;
    let mut handles = Vec::new();

    for (stem_name, avg_f0) in &selected {
        let wav_path = audio_dir.join(format!("{stem_name}.wav"));
        let audio = match load_wav_mono(&wav_path, SAMPLE_RATE) {
            Some(mut a) => {
                a.truncate(max_samples);
                Arc::new(a)
            }
            None => continue,
        };

        let expected_hz = avg_f0 * 2.0_f64.powf(shift / 12.0);
        let rb_output = Arc::new(process_audio(Algorithm::Rubberband, shift, false, &audio));

        let stem_short: String = stem_name.chars().take(25).collect();

        for &(algo, name) in TEST_ALGOS {
            if matches!(algo, Algorithm::FreqDivider | Algorithm::Pll) {
                continue;
            }

            let audio = Arc::clone(&audio);
            let rb_output = Arc::clone(&rb_output);
            let source = format!("mdb_{stem_short}");
            handles.push(thread::spawn(move || {
                compare_vs_reference(algo, name, shift, &audio, &rb_output, expected_hz, source)
            }));
        }

        // Rubberband self-entry.
        let rb_out2 = Arc::clone(&rb_output);
        let source = format!("mdb_{stem_short}");
        let ef = expected_hz;
        handles.push(thread::spawn(move || {
            let skip = (SAMPLE_RATE * 0.5) as usize;
            let out = &rb_out2[skip..];
            QualityResult {
                algo_name: "RUBBERBAND*",
                source,

                f0_rmse_cents: f0_rmse_cents(out, ef, SAMPLE_RATE),
                si_sdr_db: f64::INFINITY,
                mcd_db: 0.0,
                spec_conv: 0.0,
                latency_ms: 21.3,
            }
        }));
    }

    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

// ===========================================================================
// Main
// ===========================================================================

fn main() {
    eprintln!("pitch-dsp quality benchmark (vs Rubberband reference)");
    eprintln!("  All SI-SDR/MCD/SC metrics measured against Rubberband output");
    eprintln!();

    let h_sine = thread::spawn(bench_sine);
    let h_guitar = thread::spawn(bench_egfxset);
    let h_mdb = thread::spawn(bench_mdb);

    let mut all = Vec::new();
    all.extend(h_sine.join().unwrap());
    all.extend(h_guitar.join().unwrap());
    all.extend(h_mdb.join().unwrap());

    // === Summary per algorithm (averaged across all tests) ===
    println!();
    println!("=== Algorithm Quality vs Rubberband (averaged across all tests) ===");
    println!("┌────────────────┬────────┬────────────┬────────────┬────────────┬────────────┐");
    println!("│ Algorithm      │Lat(ms) │ F0 RMSE(c) │ SI-SDR(dB) │  MCD(dB)   │ Spec Conv  │");
    println!("├────────────────┼────────┼────────────┼────────────┼────────────┼────────────┤");

    let algo_order = [
        "RUBBERBAND*",
        "allpass",
        "granular",
        "psola",
        "wsola",
        "freq_divider",
        "pll",
    ];

    for name in &algo_order {
        let results: Vec<&QualityResult> = all.iter().filter(|r| r.algo_name == *name).collect();
        if results.is_empty() {
            continue;
        }

        let finite = |vals: Vec<f64>| -> f64 {
            let f: Vec<f64> = vals.into_iter().filter(|v| v.is_finite()).collect();
            if f.is_empty() {
                f64::NAN
            } else {
                f.iter().sum::<f64>() / f.len() as f64
            }
        };

        let avg_f0 = finite(results.iter().map(|r| r.f0_rmse_cents).collect());
        let avg_sdr = finite(results.iter().map(|r| r.si_sdr_db).collect());
        let avg_mcd = finite(results.iter().map(|r| r.mcd_db).collect());
        let avg_sc = finite(results.iter().map(|r| r.spec_conv).collect());
        let lat = results[0].latency_ms;

        let label = if *name == "RUBBERBAND*" {
            "rubberband*"
        } else {
            name
        };

        println!(
            "│ {:<14} │{:>5.1}ms │ {:>8.1}   │ {:>8.1}   │ {:>8.2}   │ {:>8.4}   │",
            label, lat, avg_f0, avg_sdr, avg_mcd, avg_sc,
        );
    }
    println!("└────────────────┴────────┴────────────┴────────────┴────────────┴────────────┘");
    println!("  * = reference (self-comparison, SI-SDR=inf, MCD=0, SC=0)");

    // === Per-source breakdown for top candidates ===
    let categories = [
        ("Sine", "sine_"),
        ("Guitar DI", "guitar_"),
        ("MDB Vocals", "mdb_"),
    ];

    for (cat_name, prefix) in &categories {
        let cat_results: Vec<&QualityResult> = all
            .iter()
            .filter(|r| r.source.starts_with(prefix) && r.algo_name != "RUBBERBAND*")
            .collect();
        if cat_results.is_empty() {
            continue;
        }

        println!();
        println!("=== {cat_name} Detail (vs Rubberband) ===");
        println!(
            "┌────────────────┬──────────────────────────┬────────────┬────────────┬────────────┐"
        );
        println!(
            "│ Algorithm      │ Source                   │ SI-SDR(dB) │  MCD(dB)   │ F0 RMSE(c) │"
        );
        println!(
            "├────────────────┼──────────────────────────┼────────────┼────────────┼────────────┤"
        );

        for r in &cat_results {
            let src_display = if r.source.len() > 24 {
                &r.source[..24]
            } else {
                &r.source
            };
            println!(
                "│ {:<14} │ {:<24} │ {:>8.1}   │ {:>8.2}   │ {:>8.1}   │",
                r.algo_name, src_display, r.si_sdr_db, r.mcd_db, r.f0_rmse_cents,
            );
        }
        println!(
            "└────────────────┴──────────────────────────┴────────────┴────────────┴────────────┘"
        );
    }

    // === Speed vs Quality tradeoff summary ===
    println!();
    println!("=== Speed vs Quality Tradeoff ===");
    println!("  (from throughput bench: allpass=550x, granular=400x,");
    println!("   psola=48x, wsola=110x, rubberband=39x)");
    println!();

    for name in &["allpass", "granular", "psola", "wsola"] {
        let results: Vec<&QualityResult> = all
            .iter()
            .filter(|r| r.algo_name == *name && r.si_sdr_db.is_finite())
            .collect();
        if results.is_empty() {
            continue;
        }

        let avg_sdr: f64 = results.iter().map(|r| r.si_sdr_db).sum::<f64>() / results.len() as f64;
        let lat = results[0].latency_ms;

        let verdict = if lat < 1.0 && avg_sdr > 5.0 {
            "BEST low-latency"
        } else if lat < 7.0 && avg_sdr > 10.0 {
            "good balance"
        } else if avg_sdr > 15.0 {
            "high quality"
        } else if lat < 1.0 {
            "fastest, lower quality"
        } else {
            ""
        };

        println!(
            "  {:<14}  lat: {:>5.1}ms  quality: {:>5.1} dB vs RB  -> {verdict}",
            name, lat, avg_sdr,
        );
    }

    println!();
    println!("Scale: SI-SDR > 20dB = near-identical to RB, 10-20dB = good, <10dB = audible diff");
    println!("       MCD < 3dB = same timbre, 3-5dB = slight diff, >5dB = different character");
    println!("       F0 RMSE < 5c = transparent, < 15c = good, < 50c = fair, > 50c = wrong pitch");
}
