//! What does each of this plugin's controls actually *do*?
//!
//! Every measurement plan so far has started with someone deciding which knob
//! drives the nonlinearity. That is a guess, and it was wrong on three of
//! sixteen units in the first fleet run: the LA-3A, dbx 160 and SSL all have
//! a `Gain`-shaped control that sits *after* the gain element, so sweeping it
//! produced a flawless set of identical measurements and told us nothing. The
//! failure is silent — the capture looks completely successful.
//!
//! So stop guessing. Sweep every control, measure what moves, and let the
//! plugin say which of its parameters are worth a real capture.
//!
//! For each control this reports:
//!
//! - **What kind it is.** A control whose display text repeats and then jumps
//!   is a selector, not a continuous parameter, and it must be *enumerated*
//!   rather than sampled — sampling a Distressor's `Ratio` at eight even
//!   steps will miss `NUKE` and land twice on the same ratio. The discrete
//!   states come back with the stored value that selects each.
//! - **How much it moves the distortion**, as a THD span. This is what
//!   identifies a drive control: the LA-3A's `Peak Reduction` spans 15,000x
//!   where its `Gain` spans 1.05x.
//! - **How much it moves the gain**, which separates a compression control
//!   from a trim: a trim moves gain and nothing else.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example param_scan -- \
//!     --plugin "/Library/Audio/Plug-Ins/VST3/uaudio_distressor.vst3" \
//!     --first 8 --out scan.json
//! ```
//!
//! Selecting the controls matters for UADx plugins, which expose ~2091
//! parameters of which eight are the front panel; the rest are internal and
//! sweeping them is both pointless and slow. `--first 8` takes them in
//! enumeration order, `--ids 48-55` by parameter id — and those are different
//! things, since UADx's first eight parameters carry ids 48..55.

use std::path::PathBuf;

use signal_analyzer::harmonics::{self, ToneSpec};
use signal_analyzer::transfer_curve;
use signal_plugin_host::HostedPlugin;

const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

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

/// One measurement at one parameter setting.
struct Point {
    value: f64,
    text: String,
    thd: f64,
    gain_db: f64,
    usable: bool,
}

fn measure(
    plugin: &mut HostedPlugin,
    id: u32,
    value: f64,
    freq: f64,
    level_db: f64,
    sample_rate: f64,
) -> Point {
    plugin.set_param(id, value);
    // Flush state so the previous setting cannot leak into this one.
    let quiet = vec![0.0f32; (sample_rate * 0.35) as usize];
    let _ = render(plugin, &quiet);

    let spec = ToneSpec { freq_hz: freq, level_db, duration_s: 1.5 };
    let stimulus = harmonics::tone(&spec, sample_rate);
    let rendered = render(plugin, &stimulus);
    let skip = sample_rate as usize / 2;
    let h = harmonics::analyze(&rendered, freq, sample_rate, 8, skip);
    let tc = transfer_curve::extract(&stimulus, &rendered, skip, 65, 2048);
    let text = plugin.value_to_text(id, value).unwrap_or_default();
    Point {
        value,
        text,
        thd: h.thd_percent,
        gain_db: 20.0 * tc.small_signal_gain.abs().max(1e-12).log10(),
        usable: tc.is_usable(),
    }
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!(
            "usage: param_scan --plugin <path> [--param-range 48-55] [--steps 12] \
             [--freq 1000] [--level -12] [--out scan.json] [--no-resume]"
        );
        std::process::exit(2);
    };
    let sample_rate: f64 = num("--sample-rate", 48_000.0);
    let steps: usize = num("--steps", 12);
    let freq: f64 = num("--freq", 1000.0);
    let level: f64 = num("--level", -12.0);

    // Every parameter is measured on a **fresh instance**.
    //
    // Restoring the swept parameter to its default is not enough, because a
    // plugin can be left somewhere a parameter write does not bring it back
    // from. Soundtoys' Decapitator does exactly this: after its Bypass and
    // Style sweeps it sits in passthrough — 0.0 dB gain, 0.0000% THD — for
    // every remaining control, so Drive, Punish, Tone and Mix all reported
    // "does nothing" on a saturator whose Drive demonstrably goes from 3.18%
    // to 35.19% THD. Nothing in the output says the measurement is void; it
    // is a clean table of zeroes.
    //
    // A fresh instance costs about 2 ms (the bundle is cached process-wide
    // after the first load), against ~2 s of rendering per parameter. There
    // is no reason to carry state across a sweep for that.
    let load = |path: &str| -> HostedPlugin {
        match HostedPlugin::load(path) {
            Ok(Some(mut p)) => {
                p.prepare(sample_rate, BLOCK as u32).expect("prepare");
                p
            }
            other => {
                eprintln!("{path}: could not load ({other:?})");
                std::process::exit(1);
            }
        }
    };
    let mut plugin = load(&path);
    let descriptor = plugin.descriptor().clone();
    let all = plugin.params();

    // Which parameters to scan. UADx exposes thousands; only a handful are
    // the front panel.
    //
    // `--first N` takes the first N in enumeration order; `--ids lo-hi`
    // selects by parameter **id**. The two are not the same and the
    // difference is easy to get wrong: `load_plugin` prints the *id* of each
    // of the first eight parameters, and on UADx those ids are 48..55 while
    // their positions are 0..7. Selecting positions 48..55 instead lands on
    // `MIDI CC 0|36` and friends, which sweep cleanly and mean nothing.
    let selected: Vec<_> = if let Some(n) = arg("--first").and_then(|v| v.parse::<usize>().ok()) {
        all.iter().take(n).cloned().collect()
    } else if let Some(spec) = arg("--ids") {
        let (lo, hi) = spec.split_once('-').unwrap_or((spec.as_str(), spec.as_str()));
        let (lo, hi): (u32, u32) =
            (lo.trim().parse().unwrap_or(0), hi.trim().parse().unwrap_or(u32::MAX));
        all.iter().filter(|p| p.id >= lo && p.id <= hi).cloned().collect()
    } else if all.len() <= 64 {
        all.clone()
    } else {
        eprintln!(
            "{} exposes {} parameters — pass --first N (enumeration order) or \
             --ids lo-hi (parameter ids) to choose the real controls",
            descriptor.name,
            all.len()
        );
        std::process::exit(2);
    };
    if selected.is_empty() {
        eprintln!("no parameters selected");
        std::process::exit(2);
    }

    eprintln!("{} — scanning {} controls", descriptor.name, selected.len());

    // ── Plan the work before doing any of it ────────────────────────────
    //
    // Enumerating a parameter's display text is a pure query and costs
    // nothing, so it is done for every control up front. That yields the
    // exact number of renders the scan will perform, which is what makes an
    // honest ETA possible — an estimate based on "parameters done" would be
    // wildly wrong, since a two-state switch and a twelve-point continuous
    // sweep differ six-fold in cost.
    struct Plan {
        param: signal_plugin_host::PluginParamInfo,
        discrete: bool,
        states: Vec<(f64, String)>,
        settings: Vec<f64>,
    }

    const TEXT_PROBES: usize = 96;
    let plans: Vec<Plan> = selected
        .iter()
        .map(|p| {
            let mut states: Vec<(f64, String)> = Vec::new();
            for i in 0..=TEXT_PROBES {
                let v = p.min + (p.max - p.min) * (i as f64 / TEXT_PROBES as f64);
                let t = plugin.value_to_text(p.id, v).unwrap_or_default();
                if states.last().map(|(_, s)| s != &t).unwrap_or(true) {
                    states.push((v, t));
                }
            }
            // A selector repeats its text across stretches of its range and
            // has few distinct values; a continuous control changes text
            // almost every step.
            let discrete = states.len() <= 24 && states.len() < TEXT_PROBES / 2;
            let settings: Vec<f64> = if discrete {
                states.iter().map(|(v, _)| *v).collect()
            } else {
                (0..steps)
                    .map(|i| p.min + (p.max - p.min) * (i as f64 / (steps.max(2) - 1) as f64))
                    .collect()
            };
            Plan { param: p.clone(), discrete, states, settings }
        })
        .collect();

    // ── Resume ──────────────────────────────────────────────────────────
    //
    // A full fleet scan runs for the better part of an hour, so losing it to
    // a crash, a disconnected ssh session or a plugin that wedges is not
    // acceptable. Results are checkpointed after every parameter, and a
    // resumed run reuses whatever is already on disk.
    let dest = PathBuf::from(arg("--out").unwrap_or_else(|| "scan.json".into()));
    let resume = !std::env::args().any(|a| a == "--no-resume");
    let mut report: Vec<serde_json::Value> = Vec::new();
    let mut done_names: std::collections::HashSet<String> = Default::default();
    if resume {
        if let Ok(text) = std::fs::read_to_string(&dest) {
            if let Ok(prev) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(arr) = prev["parameters"].as_array() {
                    for entry in arr {
                        if let Some(n) = entry["name"].as_str() {
                            done_names.insert(n.to_string());
                            report.push(entry.clone());
                        }
                    }
                }
            }
        }
    }

    let total_points: usize = plans
        .iter()
        .filter(|pl| !done_names.contains(&pl.param.name))
        .map(|pl| pl.settings.len())
        .sum();
    let skipped: usize = plans.iter().filter(|pl| done_names.contains(&pl.param.name)).count();
    let total = plans.len();
    if skipped > 0 {
        eprintln!("  resuming: {skipped}/{total} controls already done");
    }
    eprintln!("  {total_points} renders to do (~{:.0}s of audio)", total_points as f64 * 1.85);

    let started = std::time::Instant::now();
    let mut points_done = 0usize;

    let checkpoint = |report: &Vec<serde_json::Value>| {
        // Written after every control, so an interrupted scan resumes from
        // the last completed one rather than from the beginning.
        let partial = serde_json::json!({
            "plugin_path": path,
            "plugin_name": descriptor.name,
            "plugin_id": descriptor.id,
            "sample_rate": sample_rate,
            "probe_freq_hz": freq,
            "probe_level_db": level,
            "complete": false,
            "parameters": report,
        });
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&dest, serde_json::to_string_pretty(&partial).unwrap());
    };

    for (index, pl) in plans.iter().enumerate() {
        let p = &pl.param;
        if done_names.contains(&p.name) {
            eprintln!("  [{:>2}/{total}] {:<18} cached", index + 1, p.name);
            continue;
        }

        // Per-point progress is a carriage-returned line, which is right on a
        // terminal and unreadable in a log — the returns smear every update
        // onto one line. Off when stderr is redirected; the per-control
        // summary below still lands either way.
        let interactive = {
            use std::io::IsTerminal;
            std::io::stderr().is_terminal()
        };
        // Fresh instance for this parameter — see `load` above.
        plugin = load(&path);

        let mut points: Vec<Point> = Vec::new();
        for (k, v) in pl.settings.iter().enumerate() {
            if !interactive {
                points.push(measure(&mut plugin, p.id, *v, freq, level, sample_rate));
                points_done += 1;
                continue;
            }
            let elapsed = started.elapsed().as_secs_f64();
            let rate = if points_done > 0 { elapsed / points_done as f64 } else { 1.9 };
            let eta = rate * (total_points.saturating_sub(points_done)) as f64;
            let pct = if total_points > 0 {
                100.0 * points_done as f64 / total_points as f64
            } else {
                100.0
            };
            eprint!(
                "  [{:>2}/{total}] {:<18} {}/{}  {:>3.0}%  ETA {:>2.0}m{:02.0}s      \r",
                index + 1,
                p.name,
                k + 1,
                pl.settings.len(),
                pct,
                (eta / 60.0).floor(),
                eta % 60.0
            );
            use std::io::Write;
            let _ = std::io::stderr().flush();

            points.push(measure(&mut plugin, p.id, *v, freq, level, sample_rate));
            points_done += 1;
        }

        let usable: Vec<&Point> = points.iter().filter(|q| q.usable && q.thd > 0.0).collect();
        let (thd_lo, thd_hi) = usable.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), q| {
            (lo.min(q.thd), hi.max(q.thd))
        });
        let (g_lo, g_hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
            (lo.min(q.gain_db), hi.max(q.gain_db))
        });
        let thd_span = if thd_lo.is_finite() && thd_lo > 0.0 { thd_hi / thd_lo } else { f64::NAN };

        eprintln!(
            "  [{:>2}/{total}] {:<18} {:<12} THD {:>9.4}%..{:<9.4}% ({:>8.1}x)  gain {:>6.1}..{:<6.1} dB",
            index + 1,
            p.name,
            if pl.discrete { format!("{} states", pl.states.len()) } else { "continuous".into() },
            if thd_lo.is_finite() { thd_lo } else { 0.0 },
            thd_hi,
            thd_span,
            if g_lo.is_finite() { g_lo } else { 0.0 },
            if g_hi.is_finite() { g_hi } else { 0.0 },
        );

        report.push(serde_json::json!({
            "id": p.id,
            "name": p.name,
            "min": p.min,
            "max": p.max,
            "default": p.default,
            "kind": if pl.discrete { "discrete" } else { "continuous" },
            "states": pl.states.iter().map(|(v, t)| serde_json::json!({"value": v, "text": t})).collect::<Vec<_>>(),
            "thd_min_percent": if thd_lo.is_finite() { Some(thd_lo) } else { None },
            "thd_max_percent": if thd_hi > 0.0 { Some(thd_hi) } else { None },
            "thd_span_ratio": if thd_span.is_finite() { Some(thd_span) } else { None },
            "gain_min_db": if g_lo.is_finite() { Some(g_lo) } else { None },
            "gain_max_db": if g_hi.is_finite() { Some(g_hi) } else { None },
            "points": points.iter().map(|q| serde_json::json!({
                "value": q.value, "text": q.text, "thd_percent": q.thd,
                "gain_db": q.gain_db, "usable": q.usable,
            })).collect::<Vec<_>>(),
        }));
        checkpoint(&report);
    }

    // Controls that win this ranking on a technicality rather than by
    // driving anything.
    //
    // Bypass and Power take the unit from whatever it does to nothing at all.
    // Mix is worse: a dry/wet blend at 0% passes the input through untouched,
    // so its THD span comes out in the hundreds of thousands — it topped
    // every unit in the first fleet scan, ahead of the actual drive control
    // by four orders of magnitude. None of these change how hard the gain
    // element is driven, which is the thing being looked for.
    let is_switch = |n: &str| {
        let n = n.to_lowercase();
        ["bypass", "power", "meter", "mix", "blend", "dry", "wet"]
            .iter()
            .any(|w| n.contains(w))
    };
    // A two-state control cannot be a drive axis. It is a mode — an enable,
    // a routing switch, a Comp/Limit selector — and it earns a huge THD span
    // by taking the unit from processing to not processing, exactly as Bypass
    // does. SSL's `External S/C` ranked first at 8,771x and the Capitol's
    // `L Ch In` at 94,013x on precisely that basis. Modes still need
    // measuring, but by enumerating them, not by sweeping them for
    // saturation.
    let two_state = |r: &serde_json::Value| {
        r["kind"] == "discrete" && r["states"].as_array().map(|a| a.len() <= 2).unwrap_or(false)
    };
    let mut ranked: Vec<_> = report
        .iter()
        .filter(|r| !is_switch(r["name"].as_str().unwrap_or("")) && !two_state(r))
        .filter_map(|r| {
            r["thd_span_ratio"].as_f64().map(|s| (s, r["name"].as_str().unwrap_or("").to_string()))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    eprintln!(
        "\nscanned {} controls in {:.0}s\ndrive candidates, most to least (THD span):",
        plans.len(),
        started.elapsed().as_secs_f64()
    );
    for (span, name) in ranked.iter().take(5) {
        eprintln!("  {span:>10.1}x  {name}");
    }

    let out = serde_json::json!({
        "plugin_path": path,
        "plugin_name": descriptor.name,
        "plugin_id": descriptor.id,
        "sample_rate": sample_rate,
        "probe_freq_hz": freq,
        "probe_level_db": level,
        "complete": true,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "parameters": report,
        "drive_ranking": ranked.iter().map(|(s, n)| serde_json::json!({"name": n, "thd_span_ratio": s})).collect::<Vec<_>>(),
    });
    std::fs::write(&dest, serde_json::to_string_pretty(&out).unwrap()).expect("write");
    eprintln!("wrote {}", dest.display());
}
