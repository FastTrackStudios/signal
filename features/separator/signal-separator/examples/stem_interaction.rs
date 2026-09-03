//! Where two elements sit against each other, band by band.
//!
//! Built for the kick-and-bass question: which one owns the sub, where
//! do they hand over, and are they carved apart or fighting?
//!
//! ```text
//! cargo run -p signal-separator --example stem_interaction -- \
//!     first=kick.wav second=bass.wav [check=true_kick.wav,true_bass.wav]
//! ```
//!
//! Given `check=`, the same measurement is run on ground-truth stems and
//! printed alongside. That matters because every number here is taken
//! from *estimated* stems — the point of the second column is to show
//! how much of the answer is the record and how much is the separator.

use std::path::Path;

use signal_analyzer::elements::{self, BandDominance};

fn main() {
    let mut first = None;
    let mut second = None;
    let mut check: Option<(String, String)> = None;

    for arg in std::env::args().skip(1) {
        match arg.split_once('=') {
            Some(("first", v)) => first = Some(v.to_string()),
            Some(("second", v)) => second = Some(v.to_string()),
            Some(("check", v)) => {
                let (a, b) = v.split_once(',').expect("check=truth_a.wav,truth_b.wav");
                check = Some((a.to_string(), b.to_string()));
            }
            _ => {}
        }
    }

    let (Some(fa), Some(fb)) = (first, second) else {
        eprintln!("usage: stem_interaction first=a.wav second=b.wav [check=ta.wav,tb.wav]");
        std::process::exit(2);
    };

    let (a, sr) = read(&fa);
    let (b, _) = read(&fb);
    let est = elements::dominance(&a, &b, sr);

    let truth = check.map(|(ta, tb)| {
        let (x, s) = read(&ta);
        let (y, _) = read(&tb);
        elements::dominance(&x, &y, s)
    });

    println!("  first  = {}", short(&fa));
    println!("  second = {}", short(&fb));
    println!();
    match &truth {
        Some(_) => println!(
            "{:>9}{:>12}{:>12}{:>12}{:>12}",
            "band", "1st wins", "margin dB", "true wins", "true dB"
        ),
        None => println!("{:>9}{:>12}{:>12}", "band", "1st wins", "margin dB"),
    }
    println!("{}", "-".repeat(if truth.is_some() { 57 } else { 33 }));

    // Only the region where a kick and a bass actually meet.
    let mut agree = Vec::new();
    for (i, d) in est.iter().enumerate() {
        if !(25.0..=600.0).contains(&d.centre_hz) || !d.first_wins.is_finite() {
            continue;
        }
        match &truth {
            Some(t) => {
                let td = t[i];
                if td.first_wins.is_finite() {
                    agree.push((d.first_wins - td.first_wins).abs());
                }
                println!(
                    "{:>8.0}{:>11.0}%{:>+12.1}{:>11.0}%{:>+12.1}",
                    d.centre_hz,
                    d.first_wins * 100.0,
                    d.median_margin_db,
                    td.first_wins * 100.0,
                    td.median_margin_db
                );
            }
            None => println!(
                "{:>8.0}{:>11.0}%{:>+12.1}",
                d.centre_hz,
                d.first_wins * 100.0,
                d.median_margin_db
            ),
        }
    }

    if !agree.is_empty() {
        let mean = agree.iter().sum::<f64>() / agree.len() as f64;
        let worst = agree.iter().copied().fold(0.0_f64, f64::max);
        println!("{}", "-".repeat(57));
        println!(
            "  ownership error vs ground truth: mean {:.1} pts, worst {:.1} pts",
            mean * 100.0,
            worst * 100.0
        );
    }

    println!();
    println!("  'wins' is the share of sounding frames where the first element");
    println!("  is louder in that band. Near 50% they are sharing it.");
    if let Some(cross) = crossover(&est) {
        println!("  handover (first drops below 50%): {cross:.0} Hz");
    }
}

/// Lowest band where the first element stops owning the region — the
/// point where a kick hands the low end to a bass.
fn crossover(d: &[BandDominance]) -> Option<f64> {
    let mut owned = false;
    for band in d.iter().filter(|b| b.first_wins.is_finite()) {
        if band.centre_hz < 25.0 {
            continue;
        }
        if band.first_wins > 0.5 {
            owned = true;
        } else if owned {
            return Some(band.centre_hz);
        }
    }
    None
}

fn short(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

fn read(path: &str) -> (Vec<f32>, f64) {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    };
    let mono = samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    (mono, spec.sample_rate as f64)
}
