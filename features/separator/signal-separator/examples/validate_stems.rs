//! Measure how much separation distorts the numbers we actually use.
//!
//! Run against a multitrack where the true stems exist, so the mix is
//! the exact sum of them:
//!
//! ```text
//! cargo run -p signal-separator --example validate_stems -- \
//!     truth=/…/truth/kick.wav est=/…/est/drums_(Kick)_….wav
//! ```
//!
//! # Why not SDR
//!
//! Signal-to-distortion ratio says how close two waveforms are. That is
//! not the question. What matters is whether the *measurement* survives:
//! if the kick's fundamental comes back within a hertz or two while SDR
//! is mediocre, that measurement is usable and SDR was misleading. So
//! this reports the error in each metric the analysis relies on, and
//! leaves waveform similarity alone.

use std::collections::BTreeMap;
use std::path::Path;

use signal_analyzer::elements::{self, ElementProfile, Region};

fn main() {
    let pairs: Vec<(String, String, String)> = std::env::args()
        .skip(1)
        .filter_map(|a| {
            let (label, rest) = a.split_once('=')?;
            let (truth, est) = rest.split_once(':')?;
            Some((label.to_string(), truth.to_string(), est.to_string()))
        })
        .collect();

    if pairs.is_empty() {
        eprintln!("usage: validate_stems <label>=<truth.wav>:<estimated.wav> ...");
        std::process::exit(2);
    }

    println!(
        "{:<10}{:>10}{:>10}{:>9}{:>10}{:>9}{:>10}",
        "stem", "LUFS Δ", "crest Δ", "fund Δ", "centroid", "flat Δ", "curve Δ"
    );
    println!("{}", "-".repeat(68));

    let mut totals: BTreeMap<&str, Vec<f64>> = BTreeMap::new();

    for (label, truth_path, est_path) in &pairs {
        let (Some(t), Some(e)) = (measure(truth_path), measure(est_path)) else {
            println!("{label:<10}  (could not measure — silent or unreadable)");
            continue;
        };

        let lufs = e.loudness_lufs - t.loudness_lufs;
        let crest = e.crest_db - t.crest_db;
        let fund = match (t.fundamental_hz, e.fundamental_hz) {
            (Some(a), Some(b)) => Some(b - a),
            _ => None,
        };
        let centroid = e.fullness.centroid_hz - t.fullness.centroid_hz;
        let flat = e.fullness.flatness - t.fullness.flatness;
        let curve = curve_error(&t, &e);

        totals.entry("lufs").or_default().push(lufs.abs());
        totals.entry("crest").or_default().push(crest.abs());
        totals.entry("curve").or_default().push(curve);
        if let Some(f) = fund {
            totals.entry("fund").or_default().push(f.abs());
        }

        println!(
            "{label:<10}{lufs:>+10.2}{crest:>+10.2}{:>9}{centroid:>+10.0}{flat:>+9.3}{curve:>10.2}",
            fund.map(|f| format!("{f:+.1}")).unwrap_or_else(|| "—".into()),
        );
    }

    println!("{}", "-".repeat(68));
    for (k, v) in &totals {
        if v.is_empty() {
            continue;
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let worst = v.iter().copied().fold(0.0_f64, f64::max);
        println!("  mean |{k}| {mean:.2}   worst {worst:.2}");
    }
    println!("\n  Δ is estimated minus true. Curve Δ is the mean absolute");
    println!("  difference across the sixth-octave EQ profile, 50 Hz–16 kHz.");

    // Region balance is the question "how clicky is this kick, how
    // clear is this bass" — and the regions most worth having are the
    // ones separation is least likely to preserve, so report the error
    // per region rather than assuming.
    println!("\n  region balance, dB relative to each stem's own total");
    println!("  {:<10}{:<12}{:>9}{:>9}{:>9}", "stem", "region", "true", "est", "Δ");
    println!("  {}", "-".repeat(49));
    for (label, truth_path, est_path) in &pairs {
        let Some(regions) = regions_for(label) else { continue };
        let (Some(t), Some(e)) = (measure(truth_path), measure(est_path)) else { continue };
        let tb = elements::region_balance(&t.profile, regions);
        let eb = elements::region_balance(&e.profile, regions);
        for ((name, tv), (_, ev)) in tb.iter().zip(&eb) {
            println!("  {:<10}{:<12}{:>9.1}{:>9.1}{:>+9.1}", label, name, tv, ev, ev - tv);
        }
    }
}

/// Which named regions suit this stem.
fn regions_for(label: &str) -> Option<&'static [Region]> {
    match label {
        "kick" => Some(elements::KICK_REGIONS),
        "bass" => Some(elements::BASS_REGIONS),
        "vocals" | "lead" | "backing" => Some(elements::VOCAL_REGIONS),
        _ => None,
    }
}

fn measure(path: &str) -> Option<ElementProfile> {
    let mut reader = hound::WavReader::open(Path::new(path)).ok()?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;

    // Mono sum: the measurements are level and spectrum, not imaging.
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    };
    let mono: Vec<f32> = samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();

    elements::profile(&mono, spec.sample_rate as f64)
}

/// Mean absolute difference between two EQ profiles over the range that
/// carries audible content.
///
/// Both profiles are already normalised to their own mean, so this is a
/// difference in *shape* — it does not punish a stem for coming back
/// quieter, only for coming back a different colour.
fn curve_error(t: &ElementProfile, e: &ElementProfile) -> f64 {
    let mut n = 0usize;
    let mut sum = 0.0;
    for ((f, a), (_, b)) in t.profile.iter().zip(&e.profile) {
        if (50.0..=16_000.0).contains(f) && a.is_finite() && b.is_finite() {
            sum += (a - b).abs();
            n += 1;
        }
    }
    if n == 0 {
        f64::NAN
    } else {
        sum / n as f64
    }
}
