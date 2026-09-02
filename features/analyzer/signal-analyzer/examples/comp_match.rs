//! Does our compressor do what the measured one did?
//!
//! The EQ work has `eq_match`, and every number in the Pro-Q effort came from
//! it: a translation is only as good as the measurement that accepts or
//! rejects it. The compressor has had no such thing. Its styles exist as
//! parameter mappings — `comp-profiles` names an 1176's controls and maps
//! them onto the core DSP — but nothing has ever checked that the DSP behaves
//! like the unit those controls came from.
//!
//! So this renders **our** compressor through the identical stimulus the
//! archived captures were taken with, reads the gain back out the same way,
//! and reports the difference.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example comp_match -- \
//!     --capture "/run/media/AudioHaven/Plugin Analysis/FabFilter Pro-C 3/captures/timing" \
//!     [--scenario atk-0.005ms_rel-10.00ms] [--freq 1000] [--json out.json]
//! ```
//!
//! Reading the result: `settled` is the static curve — threshold, ratio and
//! knee — while `rms` and `worst` are dominated by the corners, which is
//! where the time constants live. A small settled error with a large worst
//! error means the levels are right and the timing is not, and those are
//! fitted in different places.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use comp_dsp::ProC3Compressor;
use signal_analyzer::comp_probe::{self, PulseSpec, Waveform};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Filesystem-safe form of a scenario name, matching what the capture wrote.
fn safe(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Render our compressor over the stimulus and read the gain back out
/// exactly as the capture did.
fn render_ours(
    spec: &PulseSpec,
    sample_rate: f64,
    row: usize,
    settings: &Settings,
) -> Vec<f32> {
    let mut comp = ProC3Compressor::new(sample_rate);
    comp.set_threshold(settings.threshold_db);
    comp.set_ratio(settings.ratio);
    comp.set_knee(settings.knee_db);
    comp.set_attack_ms(settings.attack_ms);
    comp.set_release_ms(settings.release_ms);
    comp.set_style(settings.style);
    if let Some(range) = settings.range_db {
        comp.set_range_db(range);
    }
    comp.reset();

    let stimulus = comp_probe::pulse_tone(spec, sample_rate);
    // Settle first, the same half-second of silence the capture rendered, so
    // neither side is measured through its own start-up.
    for _ in 0..(sample_rate * 0.5) as usize {
        let _ = comp.process(0.0, 0);
    }
    let out: Vec<f32> =
        stimulus.iter().map(|s| comp.process(*s as f64, 0) as f32).collect();
    comp_probe::gain_reduction_db(&stimulus, &out, row)
}

/// What our DSP should be set to for one captured scenario.
struct Settings {
    threshold_db: f64,
    ratio: f64,
    knee_db: f64,
    attack_ms: f64,
    release_ms: f64,
    style: i32,
    range_db: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        // Pro-C 3's own defaults, which is what a capture that does not name
        // a control was measured at.
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 18.0,
            attack_ms: 31.25,
            release_ms: 309.5,
            style: 0,
            range_db: None,
        }
    }
}

/// Translate a captured scenario's stored parameter values into real units.
///
/// The stored floats are positions along curves the plugin owns, and reading
/// them as units is how a converter ends up confidently wrong — so the
/// conversions come from `signal-import`, where they were measured off the
/// plugin rather than guessed.
fn settings_for(params: &[(u32, f64)]) -> Settings {
    use signal_import::fabfilter::proc3;
    let mut s = Settings::default();
    for &(id, stored) in params {
        match id as usize {
            proc3::field::THRESHOLD => s.threshold_db = stored,
            proc3::field::RATIO => s.ratio = proc3::ratio(stored),
            proc3::field::KNEE => s.knee_db = stored,
            proc3::field::ATTACK => s.attack_ms = proc3::attack_ms(stored),
            proc3::field::RELEASE => s.release_ms = proc3::release_ms(stored),
            proc3::field::STYLE => s.style = stored.round() as i32,
            proc3::field::RANGE => s.range_db = Some(stored),
            _ => {}
        }
    }
    s
}

fn main() {
    let Some(dir) = arg("--capture") else {
        eprintln!(
            "usage: comp_match --capture <capture dir> [--scenario name] \
             [--freq 1000] [--limit n] [--json out]"
        );
        std::process::exit(2);
    };
    let dir = PathBuf::from(dir);
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).expect("metadata.json"))
            .expect("parse metadata");

    let sample_rate = meta["sample_rate"].as_f64().unwrap_or(48_000.0);
    let row_ms = meta["row_ms"].as_f64().unwrap_or(1.0);
    let row = ((row_ms * sample_rate) / 1000.0).max(1.0) as usize;
    let freqs: Vec<f64> =
        meta["frequencies"].as_array().map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_default();
    let want_freq: f64 = num("--freq", 1000.0);
    let Some(fi) = freqs.iter().position(|f| (f - want_freq).abs() < 1e-6) else {
        eprintln!("{want_freq} Hz is not in this capture; it has {freqs:?}");
        std::process::exit(2);
    };

    let base = PulseSpec {
        freq_hz: want_freq as f32,
        gain_high_db: meta["gain_high_db"].as_f64().unwrap_or(-6.0) as f32,
        gain_low_db: meta["gain_low_db"].as_f64().unwrap_or(-20.0) as f32,
        time_high_ms: meta["time_high_ms"].as_f64().unwrap_or(240.0) as f32,
        time_low_ms: meta["time_low_ms"].as_f64().unwrap_or(240.0) as f32,
        waveform: Waveform::Sine,
        duration_s: meta["duration_s"].as_f64().unwrap_or(3.0) as f32,
    };

    // Whatever the capture held constant for every scenario.
    let pinned: Vec<(u32, f64)> = meta["pinned"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| Some((p["id"].as_u64()? as u32, p["value"].as_f64()?)))
                .collect()
        })
        .unwrap_or_default();

    let only = arg("--scenario");
    let limit: usize = num("--limit", usize::MAX);
    let scenarios = meta["scenarios"].as_array().cloned().unwrap_or_default();

    let mut rows: Vec<(String, comp_probe::GainComparison, f32, f32)> = Vec::new();
    for sc in scenarios.iter().take(limit) {
        let name = sc["name"].as_str().unwrap_or("").to_string();
        if let Some(want) = &only {
            if &name != want {
                continue;
            }
        }
        let bin = dir.join(format!("{}.bin", safe(&name)));
        let Ok(curves) = comp_probe::read_capture(&bin) else { continue };
        let Some(reference) = curves.get(fi) else { continue };

        // Controls the capture pinned for the whole run come first, then the
        // scenario's own. Missing this measured our DSP at its default 18 dB
        // knee against a reference captured with the knee pinned to 0, and
        // charged the difference to the static curve.
        let mut params: Vec<(u32, f64)> = pinned.clone();
        let own: Vec<(u32, f64)> = sc["params"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| Some((p["id"].as_u64()? as u32, p["value"].as_f64()?)))
                    .collect()
            })
            .unwrap_or_default();
        params.extend(own);
        let ours = render_ours(&base, sample_rate, row, &settings_for(&params));
        // Keep the two settled levels, not just their difference: the sign
        // and magnitude say whether we compress too much or too little, which
        // the absolute difference hides.
        let settled_of = |c: &[f32]| {
            let start = (c.len() as f32 * 0.6) as usize;
            let n = c.len().saturating_sub(start).max(1);
            c[start..].iter().sum::<f32>() / n as f32
        };
        let (ref_settled, our_settled) = (settled_of(reference), settled_of(&ours));
        rows.push((name, comp_probe::compare_gain_curves(reference, &ours), ref_settled, our_settled));
    }

    if rows.is_empty() {
        eprintln!("no scenarios matched");
        std::process::exit(1);
    }

    rows.sort_by(|a, b| b.1.rms_diff_db.partial_cmp(&a.1.rms_diff_db).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "{} — {} scenarios at {want_freq:.0} Hz\n{:<34}{:>9}{:>9}{:>10}",
        meta["plugin_name"].as_str().unwrap_or("?"),
        rows.len(),
        "scenario",
        "rms dB",
        "worst",
        "settled"
    );
    for (n, c, r, o) in rows.iter().take(20) {
        println!("{:<34}{:>9.2}{:>9.2}{:>10.2}   plugin {r:>7.2}  ours {o:>7.2}", &n[..n.len().min(32)], c.rms_diff_db, c.max_diff_db, c.settled_diff_db);
    }
    let mean = rows.iter().map(|(_, c, _, _)| c.rms_diff_db as f64).sum::<f64>() / rows.len() as f64;
    let settled = rows.iter().map(|(_, c, _, _)| c.settled_diff_db as f64).sum::<f64>() / rows.len() as f64;
    println!("\nmean rms {mean:.2} dB   mean settled {settled:.2} dB   over {} scenarios", rows.len());

    if let Some(out) = arg("--json") {
        let doc = serde_json::json!({
            "capture": dir.to_string_lossy(),
            "plugin_name": meta["plugin_name"],
            "freq_hz": want_freq,
            "mean_rms_db": mean,
            "mean_settled_db": settled,
            "scenarios": rows.iter().map(|(n, c, r, o)| serde_json::json!({
                "name": n, "rms_db": c.rms_diff_db,
                "worst_db": c.max_diff_db, "settled_db": c.settled_diff_db,
                "plugin_settled_db": r, "our_settled_db": o,
            })).collect::<Vec<_>>(),
        });
        let _ = std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap());
    }
    let _: BTreeMap<(), ()> = BTreeMap::new();
}
