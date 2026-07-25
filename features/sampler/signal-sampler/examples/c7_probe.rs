//! `c7_probe` — play the LA Custom C7 Grand and prove it actually sounds.
//!
//! Loads the pack, plays notes up the keyboard, renders each one for several
//! seconds, and reports what came out: peak, RMS, and **the longest silent
//! gap inside the note**. That gap is the number that matters for streaming —
//! a sample whose chunks arrive late doesn't sound wrong, it drops out.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example c7_probe            # streamed
//! FTS_STREAM=off cargo run -p signal-sampler --release --example c7_probe
//! ```

use std::path::Path;

use signal_sampler::SamplerRig;
use signal_sampler::engine::budget;

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/LA Custom C7 Grand.signalpack";
const ID: &str = "piano";

fn rss_anon_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("RssAnon:"))?
                .split_whitespace()
                .nth(1)?
                .parse::<f64>()
                .ok()
        })
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let sr = 48_000usize;
    let pack = std::env::args().nth(1).unwrap_or_else(|| PACK.to_string());
    let secs: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);

    let rig = SamplerRig::new_offline(sr as u32);
    let t = std::time::Instant::now();
    rig.load_pack(ID, Path::new(&pack))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    println!("loaded {pack}\n  in {:?} · anon {:.1} MB", t.elapsed(), rss_anon_mb());

    // Let the preloader settle so the first note isn't racing it.
    let mut buf = vec![0.0f32; 512 * 2];
    for _ in 0..60 {
        buf.fill(0.0);
        let _ = rig.render_offline(&mut buf);
    }
    // Wait for the background preload to settle: notes played while 5718
    // heads are still decoding measure the preloader, not the streamer.
    let mut last = 0u64;
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let now = budget::used_bytes();
        if now == last && now > 0 {
            break;
        }
        last = now;
    }
    println!("preload settled at {:.1} MB", last as f64 / 1048576.0);

    let blocks = (sr as f64 * secs / 512.0) as usize;
    let mut worst_note = 0u8;
    let mut worst_gap_ms = 0.0f64;
    let mut failures = Vec::new();

    println!("\n note   peak     rms      longest gap   holes");
    for note in [24u8, 36, 48, 55, 60, 64, 67, 72, 84, 96, 108] {
        rig.midi_message(0, 0x90, note, 96);
        let (mut pk, mut rms, mut n) = (0.0f32, 0.0f64, 0u64);
        // Crackle: single zero samples inside a sounding block. A dropout
        // long enough to measure in blocks is rare; a hole one sample wide,
        // every chunk boundary, is what static actually is.
        let mut holes = 0u64;
        // Gaps are counted only after the note has started sounding, so the
        // attack itself is never mistaken for a dropout.
        let (mut started, mut gap, mut worst) = (false, 0usize, 0usize);
        // Gaps are only counted while the note is HELD: after note-off the
        // tail decays to silence on purpose.
        let release_at = blocks / 2;
        for b in 0..blocks {
            buf.fill(0.0);
            rig.render_offline(&mut buf)?;
            let block_peak = buf.iter().fold(0.0f32, |a, s| a.max(s.abs()));
            if block_peak > 1e-4 {
                started = true;
                gap = 0;
            } else if started && b < release_at {
                gap += 1;
                worst = worst.max(gap);
            }
            // Release halfway through, so the tail is exercised too.
            if b == release_at {
                rig.midi_message(0, 0x80, note, 0);
            }
            pk = pk.max(block_peak);
            if block_peak > 1e-4 {
                // Count zeros surrounded by audio, per channel.
                for f in 1..(buf.len() / 2 - 1) {
                    let (prev, cur, next) = (buf[(f - 1) * 2], buf[f * 2], buf[(f + 1) * 2]);
                    if cur == 0.0 && prev.abs() > 1e-4 && next.abs() > 1e-4 {
                        holes += 1;
                    }
                }
            }
            for &s in buf.iter() {
                rms += (s as f64) * (s as f64);
                n += 1;
            }
            // Real time — the streamer gets the wall clock a live rig gives it.
            std::thread::sleep(std::time::Duration::from_micros(10_667));
        }
        let rms = (rms / n as f64).sqrt();
        let gap_ms = worst as f64 * 512.0 / sr as f64 * 1000.0;
        if gap_ms > worst_gap_ms {
            worst_gap_ms = gap_ms;
            worst_note = note;
        }
        let verdict = if pk <= 1e-3 {
            failures.push(format!("note {note} silent"));
            "SILENT"
        } else if gap_ms > 30.0 {
            failures.push(format!("note {note} dropped out for {gap_ms:.0} ms"));
            "DROPOUT"
        } else {
            "ok"
        };
        let verdict = if holes > 50 { "CRACKLE" } else { verdict };
        if holes > 50 {
            failures.push(format!("note {note}: {holes} holes"));
        }
        println!("  {note:>3}  {pk:>6.4}  {rms:>7.5}   {gap_ms:>6.1} ms  {holes:>6}   {verdict}");
        // Let voices finish before the next note.
        for _ in 0..40 {
            buf.fill(0.0);
            rig.render_offline(&mut buf)?;
        }
    }

    println!(
        "\nresident {:.1} MB charged · anon {:.1} MB",
        budget::used_bytes() as f64 / 1048576.0,
        rss_anon_mb(),
    );
    if failures.is_empty() {
        println!("PASS — every note sounded, worst gap {worst_gap_ms:.1} ms (note {worst_note})");
        Ok(())
    } else {
        println!("FAIL — {}", failures.join("; "));
        std::process::exit(1);
    }
}
