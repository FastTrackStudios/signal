//! What a vocal is doing, in the leveller's terms — and how much of it
//! survives separation.
//!
//! ```text
//! cargo run -p signal-separator --example vocal_profile -- true.wav [est.wav]
//! ```
use signal_separator::vocal::{VocalLevelAnalysis, analyse_vocal};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: vocal_profile <true.wav> [estimated.wav]");
        std::process::exit(2);
    }
    let t = read(&args[0]).and_then(|(x, sr)| analyse_vocal(&x, sr)).expect("truth");
    let e = args.get(1).and_then(|p| read(p)).and_then(|(x, sr)| analyse_vocal(&x, sr));

    let head = if e.is_some() { "  true      est       Δ" } else { "  value" };
    println!("{:<34}{head}", "measurement");
    println!("{}", "-".repeat(62));
    for (name, tv, ev) in rows(&t, e.as_ref()) {
        match ev {
            Some(v) => println!("{name:<34}{tv:>8.1}{v:>10.1}{:>+9.1}", v - tv),
            None => println!("{name:<34}{tv:>8.1}"),
        }
    }
}

fn rows(t: &VocalLevelAnalysis, e: Option<&VocalLevelAnalysis>) -> Vec<(String, f64, Option<f64>)> {
    let mut out = Vec::new();
    let mut add = |n: &str, a: Option<f64>, b: Option<f64>| {
        if let Some(a) = a { out.push((n.to_string(), a, b)); }
    };
    add("auto target dB (rider)", t.auto_target_db, e.and_then(|x| x.auto_target_db));
    add("voiced p10 dB", t.voiced_level.map(|s| s.p10), e.and_then(|x| x.voiced_level).map(|s| s.p10));
    add("voiced p50 dB", t.voiced_level.map(|s| s.p50), e.and_then(|x| x.voiced_level).map(|s| s.p50));
    add("voiced p90 dB", t.voiced_level.map(|s| s.p90), e.and_then(|x| x.voiced_level).map(|s| s.p90));
    add("retained range dB (p90-p10)",
        t.voiced_level.map(|s| s.p90 - s.p10),
        e.and_then(|x| x.voiced_level).map(|s| s.p90 - s.p10));
    add("silence floor dB (gate)", Some(t.silence_db), e.map(|x| x.silence_db));
    add("voiced share", Some(t.voiced_share), e.map(|x| x.voiced_share));
    add("consonant share", Some(t.consonant_share), e.map(|x| x.consonant_share));
    add("silent share", Some(t.silent_share), e.map(|x| x.silent_share));
    add("phrase p50 ms", t.phrase_ms.map(|s| s.p50), e.and_then(|x| x.phrase_ms).map(|s| s.p50));
    add("gap p50 ms", t.gap_ms.map(|s| s.p50), e.and_then(|x| x.gap_ms).map(|s| s.p50));
    add("consonant centroid p50 Hz (de-ess)",
        t.consonant_centroid_hz.map(|s| s.p50),
        e.and_then(|x| x.consonant_centroid_hz).map(|s| s.p50));
    add("consonant over voiced dB", t.consonant_over_voiced_db, e.and_then(|x| x.consonant_over_voiced_db));
    add("quiet p50 dB (breaths)", t.quiet_level_db.map(|s| s.p50), e.and_then(|x| x.quiet_level_db).map(|s| s.p50));
    add("quiet centroid p50 Hz (breaths)",
        t.quiet_centroid_hz.map(|s| s.p50),
        e.and_then(|x| x.quiet_centroid_hz).map(|s| s.p50));
    out
}

fn read(path: &str) -> Option<(Vec<f32>, f64)> {
    let mut r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let s: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let sc = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().filter_map(Result::ok).map(|v| v as f32 * sc).collect()
        }
    };
    Some((s.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect(), spec.sample_rate as f64))
}
