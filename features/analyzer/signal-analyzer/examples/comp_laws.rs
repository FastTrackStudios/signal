//! What does a compressor's knob actually do?
//!
//! A profile in `comp-profiles` maps a unit's front panel onto our DSP —
//! this Attack knob means this many milliseconds — and until now every one
//! of those mappings was written from a manual's endpoints, or from nothing.
//! The manual gives an 1176's attack as "20 to 800 microseconds" and says
//! nothing about the shape between, or which end of the knob is which.
//!
//! This reads the law back off a capture. For each scenario it fits the
//! attack and release out of the measured gain curve and prints them against
//! the control value that produced them, so the mapping can be written from
//! what the unit does.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example comp_laws -- \
//!     --capture ".../UADx 1176LN Rev E/captures/timing" --param Attack
//! ```

use std::path::PathBuf;

use signal_analyzer::comp_probe::{self, PulseSpec, Waveform};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn main() {
    let Some(dir) = arg("--capture") else {
        eprintln!("usage: comp_laws --capture <dir> [--param Attack] [--freq 1000] [--json out]");
        std::process::exit(2);
    };
    let dir = PathBuf::from(dir);
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).expect("metadata.json"))
            .expect("metadata");

    let sample_rate = meta["sample_rate"].as_f64().unwrap_or(48_000.0);
    let row_ms = meta["row_ms"].as_f64().unwrap_or(1.0);
    let freqs: Vec<f64> = meta["frequencies"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();
    let want: f64 = num("--freq", 1000.0);
    let Some(fi) = freqs.iter().position(|f| (f - want).abs() < 1e-6) else {
        eprintln!("{want} Hz not in this capture");
        std::process::exit(2);
    };
    let _ = sample_rate;

    let spec = PulseSpec {
        freq_hz: want as f32,
        gain_high_db: meta["gain_high_db"].as_f64().unwrap_or(-6.0) as f32,
        gain_low_db: meta["gain_low_db"].as_f64().unwrap_or(-20.0) as f32,
        time_high_ms: meta["time_high_ms"].as_f64().unwrap_or(240.0) as f32,
        time_low_ms: meta["time_low_ms"].as_f64().unwrap_or(240.0) as f32,
        waveform: Waveform::Sine,
        duration_s: meta["duration_s"].as_f64().unwrap_or(3.0) as f32,
    };

    // Which parameter to report the law of. Its id comes from the capture's
    // own parameter list, so the caller names the control rather than an id.
    let param = arg("--param");
    let id_of = |name: &str| -> Option<u32> {
        meta["parameters"].as_array()?.iter().find_map(|p| {
            p["name"].as_str().filter(|n| n.eq_ignore_ascii_case(name))?;
            p["id"].as_u64().map(|v| v as u32)
        })
    };
    let want_id = param.as_deref().and_then(id_of);

    let mut rows = Vec::new();
    for sc in meta["scenarios"].as_array().cloned().unwrap_or_default() {
        let name = sc["name"].as_str().unwrap_or("").to_string();
        let Ok(curves) = comp_probe::read_capture(&dir.join(format!("{}.bin", safe(&name)))) else {
            continue;
        };
        let Some(curve) = curves.get(fi) else { continue };
        let Some(t) = comp_probe::fit_timing(curve, &spec, row_ms) else { continue };

        let value = want_id.and_then(|id| {
            sc["params"].as_array()?.iter().find_map(|p| {
                (p["id"].as_u64()? as u32 == id).then(|| p["value"].as_f64())?
            })
        });
        rows.push((name, value, t));
    }

    if rows.is_empty() {
        eprintln!("nothing to fit");
        std::process::exit(1);
    }
    rows.sort_by(|a, b| {
        a.1.unwrap_or(0.0).partial_cmp(&b.1.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "{} — {} scenarios at {want:.0} Hz\n{:<34}{:>8}{:>10}{:>10}{:>11}{:>11}",
        meta["plugin_name"].as_str().unwrap_or("?"),
        rows.len(),
        "scenario",
        param.as_deref().unwrap_or("value"),
        "settled",
        "released",
        "attack ms",
        "release ms"
    );
    for (n, v, t) in &rows {
        println!(
            "{:<34}{:>8}{:>10.2}{:>10.2}{:>11.2}{:>11.1}",
            &n[..n.len().min(32)],
            v.map(|x| format!("{x:.3}")).unwrap_or_else(|| "-".into()),
            t.settled_db,
            t.released_db,
            t.attack_ms,
            t.release_ms
        );
    }

    if let Some(out) = arg("--json") {
        let doc = serde_json::json!({
            "capture": dir.to_string_lossy(),
            "plugin_name": meta["plugin_name"],
            "param": param,
            "freq_hz": want,
            "rows": rows.iter().map(|(n, v, t)| serde_json::json!({
                "scenario": n, "value": v,
                "settled_db": t.settled_db, "released_db": t.released_db,
                "attack_ms": t.attack_ms, "attack_90_ms": t.attack_90_ms,
                "release_ms": t.release_ms, "release_90_ms": t.release_90_ms,
            })).collect::<Vec<_>>(),
        });
        let _ = std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap());
    }
}
