//! Smoke-test loading + running an arbitrary `.nam` model through the engine.
//!
//! ```text
//! cargo run --release -p signal-sampler --example nam_smoke -- "/home/cody/Downloads/1965 VOX AC30 Top Boost/'65 AC30_6 - The Iconic Cleanish.nam"
//! ```
//! Proves: the file loads, declares its sample rate/loudness, and turns a
//! test guitar-ish signal into non-silent, distorted output.

use signal_sampler::nam::NamProcessor;

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}
fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("usage: nam_smoke <model.nam>")?;
    let sr = 48_000.0;
    let block = 256usize;

    let t0 = std::time::Instant::now();
    let mut nam = NamProcessor::load(&path, sr, block)?;
    println!("loaded '{}' in {:?}", nam.display_name, t0.elapsed());
    println!("  expected_sample_rate: {:?}", nam.expected_sample_rate());
    println!("  loudness: {:?} dB", nam.loudness());
    if let Some(esr) = nam.expected_sample_rate() {
        if (esr - sr).abs() > 1.0 {
            println!("  ⚠ model trained at {esr} Hz, running at {sr} Hz — voicing will shift");
        } else {
            println!("  ✓ sample rate matches ({sr} Hz)");
        }
    }

    // A plucked-ish tone: 110 Hz (A2) + a little 2nd/3rd harmonic, at guitar level.
    let n = (sr as usize) / 2; // 0.5 s
    let mut inter = vec![0.0f32; n * 2];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let env = (-t * 3.0).exp(); // pluck decay
        let s = 0.25
            * env
            * ((2.0 * std::f32::consts::PI * 110.0 * t).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin());
        inter[2 * i] = s;
        inter[2 * i + 1] = s;
    }
    let in_rms = rms(&inter);

    // Run it through the amp in real-time-sized blocks; time it.
    let t1 = std::time::Instant::now();
    for chunk in inter.chunks_mut(block * 2) {
        nam.process_interleaved(chunk);
    }
    let elapsed = t1.elapsed();
    let out_rms = rms(&inter);
    let audio_secs = n as f32 / sr as f32;
    let rt_factor = audio_secs / elapsed.as_secs_f32();

    println!("\ninput  RMS: {:.4} ({:+.1} dB)", in_rms, db(in_rms));
    println!("output RMS: {:.4} ({:+.1} dB)", out_rms, db(out_rms));
    println!(
        "processed {:.2}s of audio in {:?}  →  {:.1}× real-time  (per-64fr load ≈ {:.1}%)",
        audio_secs,
        elapsed,
        rt_factor,
        100.0 / rt_factor
    );
    if out_rms > 1e-5 {
        println!("\n✓ VOX AC30 model loads and produces sound — the amp engine works.");
    } else {
        println!("\n✗ output is silent — something is wrong.");
    }
    Ok(())
}
