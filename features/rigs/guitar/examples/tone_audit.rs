//! How driven each patch of a profile is, per side: every patch rendered
//! offline with its delays and reverbs off, a steady G (196 Hz) played in at
//! a moderate and a hard pick, and the harmonic distortion of what comes out
//! — on the whole chain, and on the amps alone (drives and compressors off
//! too), so it shows where the dirt comes from. With an output directory it
//! also writes each patch playing the shipped chords DI, to listen to.
//!
//!   cargo run --profile release-fast -p signal-guitar --example tone_audit -- \
//!     <Profile> [out-dir]
//!
//! THD under ~3 % reads clean, 3–10 % edge of breakup, past that driven.
//! Nothing in the library is written.

use signal_guitar::library::RigLibrary;
use signal_guitar::measure::render_through;
use signal_sampler::rig_profile::RigPatch;

const SR: u32 = 48_000;
/// The test note: a guitar's G (196 Hz), or `TONE_AUDIT_F0` (82 for a
/// bass's low E).
fn f0() -> f64 {
    std::env::var("TONE_AUDIT_F0").ok().and_then(|v| v.parse().ok()).unwrap_or(196.0)
}
/// The input levels a capture is swept over (sine peak, dBFS): -18 plays
/// like the shipped chords DI, -8 is digging in.
const LEVELS: [f64; 7] = [-30.0, -26.0, -22.0, -18.0, -14.0, -10.0, -6.0];

/// Time effects, and the limiter: off for the measurement.
fn is_time(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("dly") || n.starts_with("verb") || n.starts_with("pre delay") || n.starts_with("pre verb") || n == "limiter"
}

/// Only the amps and cabs (and what levels them) left in.
fn amp_only(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("amp l") || n.starts_with("amp r") || n.starts_with("cab ") || n == "patch trim" || n == "volume pedal"
}

fn goertzel(x: &[f32], f: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f / f64::from(SR);
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s0 = f64::from(v) + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt()
}

/// THD %, harmonics 2–12 over the fundamental, of the last second.
fn thd(x: &[f32]) -> f64 {
    let tail = &x[x.len().saturating_sub(SR as usize)..];
    let h1 = goertzel(tail, f0());
    if h1 < 1e-9 {
        return f64::NAN;
    }
    let rest: f64 = (2..=12).map(|k| goertzel(tail, f0() * f64::from(k)).powi(2)).sum();
    100.0 * rest.sqrt() / h1
}

fn sine(peak_db: f64) -> Vec<f32> {
    let a = 10f64.powf(peak_db / 20.0);
    (0..2 * SR as usize).map(|i| (a * (2.0 * std::f64::consts::PI * f0() * i as f64 / f64::from(SR)).sin()) as f32).collect()
}

fn with(patch: &RigPatch, off: impl Fn(&str) -> bool) -> RigPatch {
    let mut p = patch.clone();
    for b in &mut p.chain {
        if off(&b.name) {
            b.bypassed = true;
        }
    }
    p
}

fn sides(p: &RigPatch, input: &[f32]) -> (f64, f64) {
    render_through(p, SR, input).map_or((f64::NAN, f64::NAN), |(l, r)| (thd(&l), thd(&r)))
}

fn write_wav(path: &std::path::Path, l: &[f32], r: &[f32]) {
    let spec = hound::WavSpec { channels: 2, sample_rate: SR, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let Ok(mut w) = hound::WavWriter::create(path, spec) else { return };
    for (a, b) in l.iter().zip(r) {
        let _ = w.write_sample((a.clamp(-1.0, 1.0) * 32767.0) as i16);
        let _ = w.write_sample((b.clamp(-1.0, 1.0) * 32767.0) as i16);
    }
    let _ = w.finalize();
}

fn main() {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        eprintln!("usage: tone_audit <Profile> [out-dir]");
        std::process::exit(2);
    };
    let out = args.get(1).filter(|a| *a != "--models").map(std::path::PathBuf::from);
    let models_mode = args.iter().any(|a| a == "--models");
    // The instrument first (`SIGNAL_INSTRUMENT=bass`): it names the library.
    signal_guitar::instrument::init();
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let Some(def) = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name)) else {
        eprintln!("no profile {name}");
        std::process::exit(2);
    };
    let built = signal_guitar::nodes::profile_from_library(def, &lib.drive_presets);
    if args.iter().any(|a| a == "--trims") {
        for p in &built.patches {
            for b in p.chain.iter().filter(|b| b.name.starts_with("Amp ") && !b.nam.is_empty()) {
                println!("{:<20} {} drive={:?} in={:.1} out={:.1}", p.name, b.name, b.param_f32("drive"), b.input_trim_db, b.output_trim_db);
            }
        }
        return;
    }
    let (soft, hard) = (sine(-18.0), sine(-8.0));
    if args.iter().any(|a| a == "--fx") {
        // How loud each patch's delays and reverbs sit under the playing:
        // the chords DI with them off, then each alone, the difference
        // being the wet signal — its loudness against the dry, dB.
        let chords = signal_guitar::di_player::recording(0, SR);
        let is_dly = |n: &str| n.to_lowercase().starts_with("dly");
        let is_verb = |n: &str| n.to_lowercase().starts_with("verb");
        let lufs = |x: &[f64]| signal_sampler::loudness::integrated_lufs(x, f64::from(SR));
        let mono = |(l, r): (Vec<f32>, Vec<f32>)| -> Vec<f64> { l.iter().zip(&r).map(|(a, b)| f64::from(a + b) * 0.5).collect() };
        let rows = signal_guitar::levelling::par_map(&built.patches, 0, |p| {
            let lim = |n: &str| n.eq_ignore_ascii_case("limiter");
            let dry = with(p, |n| is_dly(n) || is_verb(n) || lim(n));
            let dly = with(p, |n| is_verb(n) || lim(n));
            let verb = with(p, |n| is_dly(n) || lim(n));
            let r = |q: &RigPatch| render_through(q, SR, &chords).map(mono);
            let (Some(d), Some(a), Some(b)) = (r(&dry), r(&dly), r(&verb)) else { return (f64::NAN, f64::NAN, f64::NAN) };
            let base = lufs(&d);
            let wet = |x: &[f64]| {
                let diff: Vec<f64> = x.iter().zip(&d).map(|(x, y)| x - y).collect();
                lufs(&diff) - base
            };
            (base, wet(&a), wet(&b))
        });
        println!("{:<20} {:>7} {:>7} {:>7}   blocks on", "patch", "dry", "delay", "reverb");
        for (p, (base, dl, vb)) in built.patches.iter().zip(&rows) {
            let on: Vec<String> = p.chain.iter().filter(|b| !b.bypassed && (is_dly(&b.name) || is_verb(&b.name))).map(|b| {
                let lvl = b.param_f32("level").map_or(String::new(), |v| format!(" {v:.1}"));
                format!("{}{lvl}", b.name)
            }).collect();
            let show = |v: f64| if v < -60.0 || v.is_nan() { "—".to_string() } else { format!("{v:+.1}") };
            println!("{:<20} {:>7.1} {:>7} {:>7}   {}", p.name, base, show(*dl), show(*vb), on.join(", "));
        }
        return;
    }
    if models_mode {
        // Every capture in the library's models/, alone in Amp L.
        let dir = signal_guitar::library::rig_dir().join("models");
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "nam")).collect();
        files.sort();
        let template = built.patches.first().expect("a patch").clone();
        let rows = signal_guitar::levelling::par_map(&files, 0, |f| {
            let mut p = with(&template, |n| n != "Amp L");
            for b in &mut p.chain {
                if b.name == "Amp L" {
                    b.nam = f.to_string_lossy().into_owned();
                    b.bypassed = false;
                }
            }
            LEVELS.iter().map(|&db| sides(&p, &sine(db)).0).collect::<Vec<f64>>()
        });
        print!("THD % at input dB:");
        for l in LEVELS {
            print!(" {l:>5}");
        }
        println!();
        let mut all: Vec<_> = files.iter().zip(rows).collect();
        all.sort_by(|a, b| a.1[3].total_cmp(&b.1[3]));
        for (f, r) in all {
            print!("{:<18}", "");
            for v in r {
                print!(" {v:>5.1}");
            }
            println!("   {}", f.file_name().unwrap().to_string_lossy());
        }
        return;
    }
    let chords = signal_guitar::di_player::recording(0, SR);
    // Every capture's level curve first, so a trimmed amp keeps its level.
    let mut caps: Vec<String> = built.patches.iter().flat_map(|p| p.chain.iter().filter(|b| !b.nam.is_empty()).map(|b| b.nam.clone())).collect();
    caps.sort();
    caps.dedup();
    signal_guitar::levelling::par_map(&caps, 0, |c| {
        let _ = signal_sampler::nam_calibrate::drive_curve(std::path::Path::new(c), f64::from(SR));
    });
    let built = signal_guitar::nodes::profile_from_library(def, &lib.drive_presets);
    let rows = signal_guitar::levelling::par_map(&built.patches, 0, |p| {
        let full = with(p, is_time);
        let amps = with(p, |n| !amp_only(n));
        let lufs = render_through(p, SR, &chords).map_or(f64::NAN, |(l, r)| {
            if let Some(dir) = &out {
                write_wav(&dir.join(format!("{}.wav", p.name)), &l, &r);
            }
            let mono: Vec<f64> = l.iter().zip(&r).map(|(a, b)| f64::from(a + b) * 0.5).collect();
            signal_sampler::loudness::integrated_lufs(&mono, f64::from(SR))
        });
        (sides(&full, &soft), sides(&full, &hard), sides(&amps, &hard), lufs)
    });
    println!("THD %   L / R          chain -18 dB     chain -8 dB      amps -8 dB      LUFS");
    for (p, (s, h, a, lufs)) in built.patches.iter().zip(&rows) {
        let amp_l = p.chain.iter().find(|b| b.name == "Amp L" && !b.bypassed).map_or("", |b| b.nam.rsplit('/').next().unwrap_or(""));
        let amp_r = p.chain.iter().find(|b| b.name == "Amp R" && !b.bypassed).map_or("", |b| b.nam.rsplit('/').next().unwrap_or(""));
        let drives: Vec<String> = p.chain.iter().filter(|b| !b.bypassed && (b.name.starts_with("Drive") || b.name == "Boost") && !b.nam.is_empty()).map(|b| format!("{}={}", b.name, b.nam.rsplit('/').next().unwrap_or(""))).collect();
        println!(
            "{:<16} {:>6.1} / {:<6.1}  {:>6.1} / {:<6.1}  {:>6.1} / {:<6.1} {:>6.1}  {} | {} {:?}",
            p.name, s.0, s.1, h.0, h.1, a.0, a.1, lufs, amp_l, amp_r, drives
        );
    }
}
