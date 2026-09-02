//! Capture a compressor's gain behaviour across scenarios × frequencies.
//!
//! This is the compressor's answer to `eq_sweep`: it renders a pulsing tone
//! through the plugin at every frequency in the sweep, reads the applied gain
//! back out row by row, and writes the result somewhere small enough to carry
//! between machines. What comes out is the material a model is fitted to —
//! attack and release corners, the settled depth, and how all of that moves
//! with frequency.
//!
//! Ported from the legacy `fts-analyzer` repo's `capture-compressor`, which is
//! where the method was worked out. The one capability it lacked is the one
//! that matters most here: it could only drive a plugin by **parameter id**,
//! so it could sweep attack against release but could not measure a factory
//! preset. This version takes either.
//!
//! ```sh
//! # every factory preset, matched by parameter name
//! cargo run --release -p signal-analyzer --example comp_capture -- \
//!     --plugin "/Library/Audio/Plug-Ins/CLAP/FabFilter Pro-C 2.clap" \
//!     --presets ~/Documents/FabFilter/Presets/"Pro-C 2" \
//!     --out captures/proc2-presets
//!
//! # a parameter grid — attack against release, the timing model's material
//! cargo run --release -p signal-analyzer --example comp_capture -- \
//!     --plugin "/Library/Audio/Plug-Ins/CLAP/FabFilter Pro-C 2.clap" \
//!     --scenarios scenarios/attack-release.json \
//!     --out captures/proc2-timing
//! ```
//!
//! Output is one `.bin` per scenario (see
//! [`signal_analyzer::comp_probe::write_capture`]) plus a `metadata.json`
//! carrying everything needed to interpret them.
//!
//! Finished captures are archived at `/run/media/AudioHaven/Plugin Analysis`,
//! one directory per plugin; `scripts/capture-proc.sh` writes there directly
//! when that volume is mounted. See its `README.md` for the format.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use signal_analyzer::param_grid;
use signal_analyzer::comp_probe::{
    self, gain_reduction_db, pulse_tone, PulseSpec, Waveform, TEST_FREQUENCIES,
};
use signal_import::fabfilter::parser;
use signal_plugin_host::HostedPlugin;

const BLOCK: usize = 512;
/// Silence rendered and discarded before each measurement, so the previous
/// scenario's envelope state cannot leak into this one. The runbook's first
/// trap in a different guise: a plugin resets nothing you do not reset.
const SETTLE_MS: f32 = 500.0;

/// A named set of parameter values to measure under.
#[derive(Clone)]
struct Scenario {
    name: String,
    params: Vec<(u32, f64)>,
}

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// Every occurrence of a repeatable flag, in command-line order.
fn args_all(name: &str) -> Vec<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .enumerate()
        .filter(|(_, x)| x.as_str() == name)
        .filter_map(|(i, _)| a.get(i + 1).cloned())
        .collect()
}

fn flag(name: &str) -> bool {
    std::env::args().any(|x| x == name)
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Collect `.ffp` files under a directory, recursively.
fn collect_ffp(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut items: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    items.sort();
    for path in items {
        if path.is_dir() {
            collect_ffp(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ffp")) {
            out.push(path);
        }
    }
}

/// Turn a preset folder into scenarios, matching preset keys to plugin
/// parameters **by name**.
///
/// Not by position. A preset folder is not one layout — six of Pro-C 3's 122
/// factory presets carry 69 keys in a different order under the same
/// signature — so positional binding would quietly measure the wrong
/// parameters on exactly the presets most worth checking. Names are stable
/// across those variants, which is why the `.ffp` carries them.
fn scenarios_from_presets(
    dir: &Path,
    params: &[signal_plugin_host::PluginParamInfo],
) -> (Vec<Scenario>, Vec<String>) {
    let by_name: BTreeMap<String, u32> =
        params.iter().map(|p| (p.name.to_lowercase(), p.id)).collect();

    let mut files = Vec::new();
    collect_ffp(dir, &mut files);

    let mut scenarios = Vec::new();
    let mut warnings = Vec::new();
    for path in files {
        // Name a preset by its path relative to the library root, not by its
        // file stem. Factory libraries reuse a stem across categories — Pro-C
        // 3 ships two presets called "Clean" and Pro-C 2 two called "Low-Mid
        // Control bM" — and a bare stem makes the second silently overwrite
        // the first's capture. The category is part of the identity.
        let name = path
            .strip_prefix(dir)
            .unwrap_or(&path)
            .with_extension("")
            .to_string_lossy()
            .into_owned();
        let Ok(bytes) = std::fs::read(&path) else {
            warnings.push(format!("{name}: unreadable"));
            continue;
        };

        // Two formats, two binding rules. Pro-C 3 ships text presets, which
        // name their parameters and must be bound by name because the folder
        // holds more than one layout. Pro-C 2 ships binary ones, which name
        // nothing — there the plugin's own parameter order is the only key
        // there is, and it is the right one, because the installed build
        // wrote the file.
        let values = if parser::is_text_format(&bytes) {
            let text = String::from_utf8_lossy(&bytes);
            let preset = match parser::parse_ffp_text(&text) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(format!("{name}: {e}"));
                    continue;
                }
            };
            let mut values = Vec::new();
            let mut unmatched = Vec::new();
            for (key, value) in &preset.parameters {
                match by_name.get(&key.to_lowercase()) {
                    Some(&id) => values.push((id, *value)),
                    None => unmatched.push(key.clone()),
                }
            }
            if !unmatched.is_empty() {
                // Named, not dropped quietly — an unmatched key is either a
                // layout the plugin renamed or a parser fault, and both are
                // worth seeing before the numbers are believed.
                warnings.push(format!(
                    "{name}: {} keys unmatched: {}",
                    unmatched.len(),
                    unmatched.join(", ")
                ));
            }
            values
        } else {
            let preset = match parser::parse_ffp_binary(&bytes) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(format!("{name}: {e}"));
                    continue;
                }
            };
            // Positional binding is only sound if the counts agree. If they
            // do not, this is a preset from a different build and every
            // value after the first divergence would land on the wrong
            // parameter — refuse it rather than measure nonsense.
            if preset.values.len() != params.len() {
                warnings.push(format!(
                    "{name}: {} holds {} values but the plugin has {} parameters — skipped",
                    preset.signature,
                    preset.values.len(),
                    params.len()
                ));
                continue;
            }
            params.iter().zip(&preset.values).map(|(p, &v)| (p.id, v)).collect()
        };

        scenarios.push(Scenario { name, params: values });
    }
    (scenarios, warnings)
}

/// Turn a `--sweep` spec into scenarios, resolving axis names against the
/// plugin's own parameter list and labelling each point with the plugin's
/// display text.
///
/// The label matters more than it looks. A grid point named
/// `Attack-0.0271_Release-0.0880` is unreadable and, worse, is in *stored*
/// units — the thing §2 of the runbook exists to warn about. Asking the plugin
/// gives `atk-0.01ms_rel-19ms`, which is the same naming the reference
/// captures used and can be read without a decoder.
fn scenarios_from_sweep(
    spec: &str,
    plugin: &mut HostedPlugin,
    params: &[signal_plugin_host::PluginParamInfo],
) -> Result<Vec<Scenario>, String> {
    let axes = param_grid::parse(spec).map_err(|e| e.to_string())?;
    let ids: Vec<u32> = axes
        .iter()
        .map(|a| {
            params
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&a.name))
                .map(|p| p.id)
                .ok_or_else(|| {
                    format!(
                        "no parameter called '{}' — this plugin has: {}",
                        a.name,
                        params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                    )
                })
        })
        .collect::<Result<_, _>>()?;

    Ok(param_grid::grid(&axes)
        .into_iter()
        .map(|point| {
            let mut label = Vec::new();
            let mut values = Vec::new();
            for ((name, value), &id) in point.iter().zip(&ids) {
                let shown = plugin
                    .value_to_text(id, *value)
                    .unwrap_or_else(|| format!("{value:.4}"));
                label.push(format!("{}-{}", short(name), shown.replace(' ', "")));
                values.push((id, *value));
            }
            Scenario { name: label.join("_"), params: values }
        })
        .collect())
}

/// Abbreviate a parameter name for a scenario label: "Attack" -> "atk".
fn short(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "attack" => "atk".into(),
        "release" => "rel".into(),
        "threshold" => "thr".into(),
        "ratio" => "ratio".into(),
        _ => lower.replace(' ', "-"),
    }
}

/// Parse `--set "Auto Gain=0;Knee=0"` into resolved `(id, value)` pairs.
fn parse_base(
    spec: &str,
    params: &[signal_plugin_host::PluginParamInfo],
) -> Result<Vec<(u32, f64)>, String> {
    let mut out = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, value) = part
            .split_once('=')
            .ok_or_else(|| format!("'{part}' is not Name=value"))?;
        let name = name.trim();
        let value: f64 = value
            .trim()
            .parse()
            .map_err(|_| format!("'{}' is not a number", value.trim()))?;
        let id = params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
            .ok_or_else(|| format!("no parameter called '{name}'"))?;
        out.push((id, value));
    }
    Ok(out)
}

/// Parse the legacy scenario JSON: `{"scenarios":[{"name":…,"params":{"7":0.01}}]}`.
fn scenarios_from_json(path: &Path) -> Result<Vec<Scenario>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let list = json["scenarios"].as_array().ok_or("expected a 'scenarios' array")?;
    list.iter()
        .map(|s| {
            let name = s["name"].as_str().ok_or("scenario missing 'name'")?.to_string();
            let obj = s["params"].as_object().ok_or("scenario missing 'params'")?;
            let params = obj
                .iter()
                .map(|(k, v)| {
                    let id = k.parse::<u32>().map_err(|_| format!("bad param id '{k}'"))?;
                    let val = v.as_f64().ok_or(format!("bad value for param {k}"))?;
                    Ok((id, val))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Scenario { name, params })
        })
        .collect()
}

/// Render one buffer through the plugin, mono in / mono out (left channel).
fn render(plugin: &mut HostedPlugin, mono: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(mono.len());
    let mut pos = 0;
    while pos < mono.len() {
        let n = BLOCK.min(mono.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = mono[pos + i];
            buf[2 * i + 1] = mono[pos + i];
        }
        if plugin.process_interleaved(&mut buf, &[], &[]).is_err() {
            break;
        }
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

/// Measure one scenario at every frequency.
fn run_scenario(
    plugin: &mut HostedPlugin,
    scenario: &Scenario,
    spec: &PulseSpec,
    freqs: &[f32],
    sample_rate: f64,
    row: usize,
    latency: usize,
) -> Vec<Vec<f32>> {
    for &(id, value) in &scenario.params {
        plugin.set_param(id, value);
    }

    // Flush the parameter writes and let any envelope settle before measuring.
    let settle = vec![0.0f32; (SETTLE_MS as f64 * sample_rate / 1000.0) as usize];
    let _ = render(plugin, &settle);

    freqs
        .iter()
        .map(|&f| {
            let stimulus = pulse_tone(&spec.at(f), sample_rate);
            let rendered = render(plugin, &stimulus);
            // Line the two up before reading any gain: a lookahead
            // compressor otherwise appears to react before the level step.
            let aligned = comp_probe::align_latency(&rendered, latency);
            gain_reduction_db(&stimulus, aligned, row)
        })
        .collect()
}

fn main() {
    let Some(plugin_path) = arg("--plugin") else {
        eprintln!(
            "usage: comp_capture --plugin <path> --out <dir> \
             (--presets <dir> | --sweep <spec> | --scenarios <json>) [--threads n] \
             [--duration s] \
             [--gain-high db] [--gain-low db] [--time-high ms] [--time-low ms] \
             [--row-ms ms] [--waveform sine|square|saw] [--set \"Name=v;...\"] [--dry-run]"
        );
        std::process::exit(2);
    };
    let out_dir = PathBuf::from(arg("--out").unwrap_or_else(|| "captures/out".into()));

    let sample_rate: f64 = num("--sample-rate", 48_000.0);
    let spec = PulseSpec {
        freq_hz: TEST_FREQUENCIES[0],
        gain_high_db: num("--gain-high", -6.0),
        gain_low_db: num("--gain-low", -20.0),
        time_high_ms: num("--time-high", 240.0),
        time_low_ms: num("--time-low", 240.0),
        waveform: match arg("--waveform").as_deref() {
            Some("square") => Waveform::Square,
            Some("saw") => Waveform::Saw,
            _ => Waveform::Sine,
        },
        duration_s: num("--duration", 3.0),
    };
    let row_ms: f32 = num("--row-ms", 1.0);
    let row = ((row_ms as f64) * sample_rate / 1000.0).max(1.0) as usize;

    let freqs: Vec<f32> = match arg("--freqs") {
        Some(list) => list.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
        None => TEST_FREQUENCIES.to_vec(),
    };

    // Load once up front: this is what reports the parameter list the presets
    // are matched against, and the latency every measurement is aligned by.
    let mut probe = match HostedPlugin::load(&plugin_path) {
        Ok(Some(mut p)) => {
            p.prepare(sample_rate, BLOCK as u32).expect("prepare");
            p
        }
        Ok(None) => {
            eprintln!("{plugin_path}: not a plugin this host can open");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{plugin_path}: {e:?}");
            std::process::exit(1);
        }
    };
    let descriptor = probe.descriptor().clone();
    let params = probe.params();
    let latency = probe.latency() as usize;
    let defaults: std::collections::BTreeMap<u32, f64> =
        params.iter().filter_map(|p| probe.param_value(p.id).map(|v| (p.id, v))).collect();

    // Controls pinned for the whole run, applied under every scenario.
    //
    // The motivating case: Pro-C's Auto Gain defaults to *on*, and it adds
    // makeup that varies with threshold and ratio. Sweeping threshold with it
    // enabled measures compression plus automatic makeup, and a static curve
    // fitted to that sum is wrong in a way that looks entirely plausible.
    // Anything held constant here is recorded in the metadata.
    let base: Vec<(u32, f64)> = {
        // Every `--set` on the command line, merged left to right, so a later
        // one refines an earlier one instead of being silently discarded.
        // Taking only the first is how a caller ends up believing a control
        // was pinned when it was not.
        let mut merged: Vec<(u32, f64)> = Vec::new();
        for spec in args_all("--set") {
            match parse_base(&spec, &params) {
                Ok(v) => {
                    for (id, value) in v {
                        match merged.iter_mut().find(|(i, _)| *i == id) {
                            Some(slot) => slot.1 = value,
                            None => merged.push((id, value)),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        merged
    };

    let (scenarios, warnings) = match (arg("--presets"), arg("--scenarios"), arg("--sweep")) {
        (Some(dir), _, _) => scenarios_from_presets(Path::new(&dir), &params),
        (None, Some(json), _) => match scenarios_from_json(Path::new(&json)) {
            Ok(s) => (s, Vec::new()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        },
        (None, None, Some(spec)) => match scenarios_from_sweep(&spec, &mut probe, &params) {
            Ok(s) => (s, Vec::new()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        },
        (None, None, None) => {
            (vec![Scenario { name: "default".into(), params: Vec::new() }], Vec::new())
        }
    };
    drop(probe);

    if scenarios.is_empty() {
        eprintln!("no scenarios to measure");
        std::process::exit(1);
    }

    // Pinned controls go first, so a scenario that also names one wins — an
    // axis being swept must not be overridden by a base value.
    let scenarios: Vec<Scenario> = scenarios
        .into_iter()
        .map(|mut sc| {
            let mut params = base.clone();
            params.append(&mut sc.params);
            Scenario { name: sc.name, params }
        })
        .collect();

    eprintln!("{} — {} parameters, {} samples latency", descriptor.name, params.len(), latency);
    eprintln!(
        "{} scenarios × {} frequencies, {:.1}s each at {} Hz",
        scenarios.len(),
        freqs.len(),
        spec.duration_s,
        sample_rate
    );
    for w in &warnings {
        eprintln!("  warning: {w}");
    }
    if flag("--dry-run") {
        eprintln!("--dry-run: nothing rendered");
        return;
    }

    std::fs::create_dir_all(&out_dir).expect("create output directory");

    let meta = serde_json::json!({
        "plugin_path": plugin_path,
        "plugin_name": descriptor.name,
        "plugin_id": descriptor.id,
        "sample_rate": sample_rate,
        "block_size": BLOCK,
        "latency_samples": latency,
        "gain_high_db": spec.gain_high_db,
        "gain_low_db": spec.gain_low_db,
        "time_high_ms": spec.time_high_ms,
        "time_low_ms": spec.time_low_ms,
        "waveform": format!("{:?}", spec.waveform),
        "duration_s": spec.duration_s,
        "row_ms": row_ms,
        "settle_ms": SETTLE_MS,
        "frequencies": freqs,
        // Every parameter's *resting* value as well as its range. A grid sets
        // the axes it names and leaves everything else at whatever the plugin
        // defaults to, so without these a capture cannot be reproduced — and
        // two captures taken months apart could differ for reasons nothing
        // records. `default` is what the parameter read at load, before
        // anything was set.
        "parameters": params.iter().map(|p| serde_json::json!({
            "id": p.id, "name": p.name, "min": p.min, "max": p.max,
            "default": defaults.get(&p.id),
        })).collect::<Vec<_>>(),
        "pinned": base.iter().map(|(id, v)| {
            let name = params.iter().find(|p| p.id == *id).map(|p| p.name.clone()).unwrap_or_default();
            serde_json::json!({ "id": id, "name": name, "value": v })
        }).collect::<Vec<_>>(),
        "warnings": warnings,
        "scenarios": scenarios.iter().map(|s| serde_json::json!({
            "name": s.name,
            "params": s.params.iter().map(|(id, v)| serde_json::json!({"id": id, "value": v}))
                .collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    std::fs::write(out_dir.join("metadata.json"), serde_json::to_string_pretty(&meta).unwrap())
        .expect("write metadata");

    let threads = num("--threads", std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4));
    let chunk = scenarios.len().div_ceil(threads.max(1));
    let done = Arc::new(AtomicUsize::new(0));
    let total = scenarios.len();
    // Creation is serialised, rendering is not. `PluginPool`'s stress runs
    // show concurrent creation is fine for the native CLAP backend — a
    // thousand Pro-C instances build cleanly at ~2.2 ms each — so this is a
    // precaution rather than a measured need, and it costs a couple of
    // seconds across a whole capture. It stays because the yabridge path has
    // not been checked the same way, and a sporadic load failure here would
    // read as measurement noise rather than as a crash.
    let load_lock = Arc::new(Mutex::new(()));

    let failures: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = scenarios
            .chunks(chunk.max(1))
            .map(|group| {
                let (load_lock, done, plugin_path, out_dir, spec, freqs) = (
                    Arc::clone(&load_lock),
                    Arc::clone(&done),
                    plugin_path.clone(),
                    out_dir.clone(),
                    spec,
                    freqs.clone(),
                );
                s.spawn(move || {
                    let mut plugin = {
                        let _guard = load_lock.lock().unwrap();
                        match HostedPlugin::load(&plugin_path) {
                            Ok(Some(mut p)) => match p.prepare(sample_rate, BLOCK as u32) {
                                Ok(()) => p,
                                Err(e) => {
                                    return group.iter().map(|g| format!("{}: prepare: {e:?}", g.name)).collect::<Vec<_>>();
                                }
                            },
                            other => {
                                return group
                                    .iter()
                                    .map(|g| format!("{}: load: {other:?}", g.name))
                                    .collect::<Vec<_>>();
                            }
                        }
                    };

                    let mut errors = Vec::new();
                    for scenario in group {
                        let curves = run_scenario(
                            &mut plugin, scenario, &spec, &freqs, sample_rate, row, latency,
                        );
                        // A scenario that rendered silence everywhere is the
                        // authorisation / side-chain failure, not a result.
                        // Say so rather than writing a file of zeroes.
                        if curves.iter().flatten().all(|v| *v == 0.0) {
                            errors.push(format!("{}: silent at every frequency", scenario.name));
                        }
                        let safe: String = scenario
                            .name
                            .chars()
                            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                            .collect();
                        if let Err(e) = comp_probe::write_capture(&out_dir.join(format!("{safe}.bin")), &curves) {
                            errors.push(format!("{}: write: {e}", scenario.name));
                        }
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        eprintln!("  [{n}/{total}] {}", scenario.name);
                    }
                    errors
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });

    eprintln!("\nwrote {} scenarios to {}", total, out_dir.display());
    if !failures.is_empty() {
        eprintln!("{} problem(s):", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}
