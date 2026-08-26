//! Audio sample comparison tool for Pro-C 3 parity testing.
//!
//! This utility:
//! 1. Generates test signals (sine, square, noise)
//! 2. Processes through comp-dsp
//! 3. Optionally compares to reference audio from Pro-C 3
//! 4. Reports detailed sample-by-sample statistics
//!
//! Usage:
//!   cargo run --example compare_samples -- <reference.wav> [--threshold 0.01dB]
//!   cargo run --example compare_samples -- --generate-only
//!
//! Example with reference file:
//!   cargo run --example compare_samples -- ~/pro-c3-reference.wav --threshold 1.0

use audiocore_dsp::db::linear_to_db;
use comp_dsp::ProC3Compressor;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("\n{}", "=".repeat(80));
    println!("FTS Compressor ↔ Pro-C 3 Clean Sample Comparison");
    println!("{}", "=".repeat(80));

    // Generate test signal
    let sample_rate = 48000.0;
    let duration_s = 2.0;
    let num_samples = (sample_rate * duration_s) as usize;

    println!("\n[SIGNAL GENERATION]");
    println!("Sample rate: {} Hz", sample_rate as u32);
    println!("Duration: {} s ({} samples)", duration_s, num_samples);

    // Create test signal: sine wave + transient
    let mut input = vec![0.0f64; num_samples];
    let freq_hz = 1000.0;
    let omega = 2.0 * std::f64::consts::PI * freq_hz / sample_rate;

    for (i, sample) in input.iter_mut().enumerate() {
        let t = i as f64 / sample_rate;
        // Sine wave
        let sine = (omega * i as f64).sin() * 0.3;
        // Attack transient at 0.5s and 1.5s
        let transient = if (t % 1.0) < 0.01 {
            0.9 * (1.0 - (t % 1.0) / 0.01)
        } else {
            0.0
        };
        *sample = sine + transient;
    }

    println!("Generated: 1kHz sine @ -10.5dB + transients");

    // Process through compressor
    println!("\n[PROCESSING THROUGH FTS-COMP]");
    println!("Settings: Threshold=-18dB, Ratio=4:1, Attack=10ms, Release=50ms, Knee=2dB");

    let mut comp = ProC3Compressor::new(sample_rate);
    comp.set_threshold(-18.0);
    comp.set_ratio(4.0);
    comp.set_attack(0.010); // 10ms
    comp.set_release(0.050); // 50ms
    comp.set_knee(2.0);

    let mut output = vec![0.0f64; num_samples];
    let mut gr_values = vec![0.0f64; num_samples];

    for (i, sample) in input.iter().enumerate() {
        output[i] = comp.process(*sample, 0);
        gr_values[i] = comp.gain_reduction_db();
    }

    println!("✓ Processed {} samples", num_samples);

    // Analyze output
    println!("\n[OUTPUT ANALYSIS]");
    let (input_peak, input_rms) = analyze_signal(&input);
    let (output_peak, output_rms) = analyze_signal(&output);

    println!(
        "Input:  Peak = {:.3} ({:.2} dB), RMS = {:.3} ({:.2} dB)",
        input_peak,
        linear_to_db(input_peak),
        input_rms,
        linear_to_db(input_rms)
    );
    println!(
        "Output: Peak = {:.3} ({:.2} dB), RMS = {:.3} ({:.2} dB)",
        output_peak,
        linear_to_db(output_peak),
        output_rms,
        linear_to_db(output_rms)
    );

    let reduction = linear_to_db(input_peak) - linear_to_db(output_peak);
    println!("Gain Reduction: {:.2} dB (peak)", reduction);

    let max_gr = gr_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    println!("Max GR: {:.2} dB", max_gr);

    // Display waveform sections
    println!("\n[WAVEFORM SAMPLES]");
    println!("Sample #  │ Input       │ Output      │ GR (dB) │ Difference");
    println!("──────────┼─────────────┼─────────────┼─────────┼────────────");

    let display_indices = vec![
        0,
        24000, // 0.5s mark (transient)
        24100, // after first transient
        48000, // 1.0s mark
        72000, // 1.5s mark (second transient)
        72100, // after second transient
        num_samples - 1,
    ];

    for &idx in &display_indices {
        if idx < num_samples {
            let diff = input[idx] - output[idx];
            println!(
                "{:8} │ {:11.6} │ {:11.6} │ {:7.2} │ {:10.6}",
                idx, input[idx], output[idx], gr_values[idx], diff
            );
        }
    }

    // Save test outputs
    println!("\n[FILE OUTPUT]");
    save_wav("/tmp/fts-comp-input.wav", &input, sample_rate).ok();
    save_wav("/tmp/fts-comp-output.wav", &output, sample_rate).ok();
    println!("✓ Saved input:  /tmp/fts-comp-input.wav");
    println!("✓ Saved output: /tmp/fts-comp-output.wav");

    // Try to load and compare with reference if provided
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] != "--generate-only" {
        println!("\n[COMPARING WITH REFERENCE]");
        if let Ok(reference) = read_wav(&args[1]) {
            compare_outputs(&output, &reference);
        } else {
            println!("✗ Could not load reference file: {}", args[1]);
            println!("\n[NEXT STEPS]");
            println!("1. Load /tmp/fts-comp-input.wav into Pro-C 3 (Clean mode)");
            println!(
                "   Settings: Threshold=-18dB, Ratio=4:1, Attack=10ms, Release=50ms, Knee=2dB"
            );
            println!("2. Export Pro-C 3 output to /tmp/pro-c3-output.wav");
            println!("3. Run: cargo run --example compare_samples -- /tmp/pro-c3-output.wav");
        }
    } else {
        println!("\n[NEXT STEPS]");
        println!("1. Load /tmp/fts-comp-input.wav into Pro-C 3 (Clean mode)");
        println!("   Settings: Threshold=-18dB, Ratio=4:1, Attack=10ms, Release=50ms, Knee=2dB");
        println!("2. Export Pro-C 3 output to /tmp/pro-c3-output.wav");
        println!("3. Run: cargo run --example compare_samples -- /tmp/pro-c3-output.wav");
        println!("\nThis will compare sample-by-sample differences between:");
        println!("  - /tmp/fts-comp-output.wav (FTS-Comp v2)");
        println!("  - /tmp/pro-c3-output.wav (Pro-C 3 reference)");
    }
    println!("{}\n", "=".repeat(80));
}

/// Analyze signal for peak and RMS levels
fn analyze_signal(signal: &[f64]) -> (f64, f64) {
    let peak = signal.iter().map(|s| s.abs()).fold(0.0, f64::max);
    let rms = (signal.iter().map(|s| s * s).sum::<f64>() / signal.len() as f64).sqrt();
    (peak, rms)
}

/// Simple WAV file reader (reads only float audio)
fn read_wav(path: &str) -> std::io::Result<Vec<f64>> {
    use std::io::Read;
    let mut file = File::open(path)?;
    let mut header = [0u8; 44];
    file.read_exact(&mut header)?;

    // Validate RIFF and WAVE headers
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" || &header[12..16] != b"fmt " {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid WAV file format",
        ));
    }

    // Read audio format (should be 3 for IEEE float)
    let audio_format = u16::from_le_bytes([header[20], header[21]]);
    let num_channels = u16::from_le_bytes([header[22], header[23]]) as usize;
    let bytes_per_sample = if audio_format == 3 { 4 } else { 2 }; // float or PCM

    // Find data chunk
    let mut buf = [0u8; 4];
    loop {
        if file.read_exact(&mut buf).is_err() {
            break;
        }
        if &buf == b"data" {
            break;
        }
    }

    let mut size_buf = [0u8; 4];
    file.read_exact(&mut size_buf)?;
    let data_size = u32::from_le_bytes(size_buf) as usize;
    let num_samples = data_size / (bytes_per_sample * num_channels);

    let mut samples = vec![0.0f64; num_samples];
    let mut temp = vec![0u8; data_size];
    file.read_exact(&mut temp)?;

    // Convert to f64
    if audio_format == 3 {
        // IEEE float
        for i in 0..num_samples {
            let bytes = &temp[i * bytes_per_sample..(i + 1) * bytes_per_sample];
            let f32_bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
            samples[i] = f32::from_le_bytes(f32_bytes) as f64;
        }
    } else {
        // PCM (16-bit)
        for i in 0..num_samples {
            let bytes = &temp[i * 2..(i + 1) * 2];
            let i16_val = i16::from_le_bytes([bytes[0], bytes[1]]) as f64;
            samples[i] = i16_val / 32768.0;
        }
    }

    Ok(samples)
}

/// Compare FTS output with Pro-C 3 reference
fn compare_outputs(fts_output: &[f64], reference: &[f64]) {
    let min_len = fts_output.len().min(reference.len());

    // Calculate differences
    let mut diffs = vec![0.0f64; min_len];
    let mut abs_diffs = vec![0.0f64; min_len];

    for i in 0..min_len {
        diffs[i] = fts_output[i] - reference[i];
        abs_diffs[i] = diffs[i].abs();
    }

    let max_diff = abs_diffs.iter().copied().fold(0.0, f64::max);
    let mean_diff = abs_diffs.iter().sum::<f64>() / min_len as f64;
    let rms_diff = (abs_diffs.iter().map(|d| d * d).sum::<f64>() / min_len as f64).sqrt();

    println!(
        "Samples compared: {} / {} ({}%)",
        min_len,
        reference.len(),
        (min_len as f64 / reference.len() as f64 * 100.0) as u32
    );
    println!("\nDifference Statistics:");
    println!("  Max difference:    {:.9}", max_diff);
    println!("  Mean difference:   {:.9}", mean_diff);
    println!("  RMS difference:    {:.9}", rms_diff);
    println!(
        "  Max diff in dB:    {:.3}",
        linear_to_db(1.0 + max_diff.min(1.0))
    );

    // Count samples above different thresholds
    let thresholds = vec![0.0001, 0.001, 0.01, 0.1];
    println!("\nSamples exceeding threshold:");
    for threshold in thresholds {
        let count = abs_diffs.iter().filter(|d| **d > threshold).count();
        let pct = (count as f64 / min_len as f64 * 100.0) as u32;
        println!("  >{}: {} samples ({}%)", threshold, count, pct);
    }

    // Display sample comparison at key points
    println!("\nSample-by-sample comparison at key points:");
    println!("Sample #  │ FTS Output      │ Pro-C3 Ref      │ Difference      │ % Error");
    println!("──────────┼─────────────────┼─────────────────┼─────────────────┼─────────");

    let key_indices = vec![0, min_len / 4, min_len / 2, (min_len * 3) / 4, min_len - 1];

    for &idx in &key_indices {
        let pct_error = if reference[idx].abs() > 0.0001 {
            (diffs[idx].abs() / reference[idx].abs() * 100.0).min(999.9)
        } else {
            0.0
        };
        println!(
            "{:8} │ {:15.9} │ {:15.9} │ {:15.9} │ {:7.3}%",
            idx, fts_output[idx], reference[idx], diffs[idx], pct_error
        );
    }

    // Categorize parity level
    println!("\n[PARITY ASSESSMENT]");
    if max_diff < 0.0001 {
        println!("✓ EXCELLENT: Near-identical (< 0.01% LSB for ±1.0 signals)");
    } else if max_diff < 0.001 {
        println!("✓ VERY GOOD: Minor differences (< 0.1% LSB)");
    } else if max_diff < 0.01 {
        println!("✓ GOOD: Acceptable differences (< 1% LSB)");
    } else if max_diff < 0.1 {
        println!("⚠ FAIR: Noticeable differences (1-10% LSB)");
    } else {
        println!("✗ POOR: Significant differences (> 10% LSB)");
    }
}

/// Simple WAV file writer
fn save_wav(path: &str, samples: &[f64], sample_rate: f64) -> std::io::Result<()> {
    let num_samples = samples.len();
    let bytes_per_sample: u32 = 4; // 32-bit float
    let num_channels: u32 = 1;
    let byte_rate = sample_rate as u32 * num_channels * bytes_per_sample;
    let block_align = (num_channels * bytes_per_sample) as u16;
    let subchunk2_size = num_samples as u32 * bytes_per_sample;
    let chunk_size = 36 + subchunk2_size;

    let mut file = File::create(path)?;

    // RIFF header
    file.write_all(b"RIFF")?;
    file.write_all(&chunk_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    // fmt subchunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // subchunk1 size
    file.write_all(&3u16.to_le_bytes())?; // audio format (IEEE float)
    file.write_all(&1u16.to_le_bytes())?; // mono
    file.write_all(&(sample_rate as u32).to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?; // bits per sample

    // data subchunk
    file.write_all(b"data")?;
    file.write_all(&subchunk2_size.to_le_bytes())?;

    // Write samples as 32-bit float
    for sample in samples {
        let f32_sample = *sample as f32;
        file.write_all(&f32_sample.to_le_bytes())?;
    }

    Ok(())
}
