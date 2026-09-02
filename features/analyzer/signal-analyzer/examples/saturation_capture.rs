//! Measure what a processor *adds*: its saturation, across level and frequency.
//!
//! Two complementary measurements, because neither answers the whole question.
//!
//! **Steady tones** ([`signal_analyzer::harmonics`]) — a sine at a known
//! frequency and level, and the amplitude of every harmonic that comes back.
//! Sweeping the level traces the saturation curve: how distortion grows with
//! drive, and how the even/odd balance shifts as it does. Even and odd are
//! kept apart because they are what the ear separates — odd order reads as
//! hardness, even as warmth, and two devices with identical THD and opposite
//! balance sound nothing alike.
//!
//! **A swept sine** ([`signal_analyzer::swept_sine`]) — Farina's method, which
//! returns the linear impulse response *and* one impulse response per harmonic
//! order from a single pass. Those per-order responses are the coefficients of
//! a parallel Hammerstein model (a static polynomial per branch, each followed
//! by a filter), so this is a measurement we can play back rather than only
//! plot.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example saturation_capture -- \
//!     --plugin "/Library/Audio/Plug-Ins/VST3/uaudio_ua_1176ln_rev_e.vst3" \
//!     --out "sat/1176-rev-e" --set "Ratio=0"
//! ```
//!
//! # Reading the result on a compressor
//!
//! A compressor's detector will happily manufacture harmonics that are not
//! saturation at all: at low frequencies a fast detector tracks *within* the
//! cycle, and the resulting ripple is indistinguishable from second-order
//! distortion in a single measurement. They separate by their behaviour, not
//! their spectrum — true saturation is instantaneous and does not move when
//! the time constants do. `--release-param` measures the whole level sweep at
//! two release settings and reports both, so the part that moves can be told
//! from the part that does not. Treat anything that moves as detector ripple.

use std::path::PathBuf;

use signal_analyzer::harmonics::{self, ToneSpec};
use signal_analyzer::swept_sine::{self, SweepSpec};
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

fn list(name: &str, default: &[f64]) -> Vec<f64> {
    match arg(name) {
        Some(s) => s.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
        None => default.to_vec(),
    }
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

/// Flush the processor's state so one measurement cannot inherit the
/// previous one's envelope.
fn settle(plugin: &mut HostedPlugin, sample_rate: f64) {
    let quiet = vec![0.0f32; (sample_rate * 0.5) as usize];
    let _ = render(plugin, &quiet);
}

/// Measure THD at one drive setting, cheaply, for the purpose of choosing
/// where to measure properly.
fn probe_thd(
    plugin: &mut HostedPlugin,
    id: u32,
    value: f64,
    freq: f64,
    level_db: f64,
    sample_rate: f64,
) -> (f64, bool) {
    plugin.set_param(id, value);
    settle(plugin, sample_rate);
    let spec = ToneSpec { freq_hz: freq, level_db, duration_s: 1.5 };
    let stimulus = harmonics::tone(&spec, sample_rate);
    let rendered = render(plugin, &stimulus);
    let h = harmonics::analyze(&rendered, freq, sample_rate, 8, sample_rate as usize / 2);
    let tc = transfer_curve::extract(&stimulus, &rendered, sample_rate as usize / 2, 65, 2048);
    (h.thd_percent, tc.is_usable())
}

/// Choose drive settings that spread the measurements evenly across the
/// unit's **distortion** range rather than evenly across its knob.
///
/// A waveshaper fitted from these points is only as good as its coverage, and
/// knob position is a poor proxy for coverage: the Fairchild 660 sits under
/// 0.03% THD for the first two thirds of its Input range and then climbs to
/// 0.5% in the last third, so a linear sweep spends most of its renders
/// measuring the same nearly-clean curve and barely samples the part that
/// actually bends.
///
/// So probe the range cheaply, then pick settings whose THD is spaced
/// geometrically between the quietest and the loudest usable distortion —
/// including both endpoints, because the extremes are exactly the points a
/// shaper needs pinned. Settings where the transfer curve stops being a
/// function at all (the unit driven into silence or hard limiting) are
/// excluded: there is no shape there to fit.
fn thd_spaced_drive(
    plugin: &mut HostedPlugin,
    id: u32,
    min: f64,
    max: f64,
    steps: usize,
    freq: f64,
    level_db: f64,
    sample_rate: f64,
) -> Vec<(f64, f64)> {
    const PROBES: usize = 17;
    let mut probed: Vec<(f64, f64)> = Vec::new();
    for i in 0..PROBES {
        let v = min + (max - min) * (i as f64 / (PROBES - 1) as f64);
        let (thd, usable) = probe_thd(plugin, id, v, freq, level_db, sample_rate);
        if usable && thd.is_finite() && thd > 0.0 {
            probed.push((v, thd));
        }
        eprintln!("   probe {v:.3} -> {thd:.4}% {}", if usable { "" } else { "(unusable)" });
    }
    if probed.len() < 2 {
        // Nothing measurable — fall back to a linear spread rather than
        // returning nothing to measure.
        return (0..steps)
            .map(|i| {
                let v = min + (max - min) * (i as f64 / (steps.max(2) - 1) as f64);
                (v, f64::NAN)
            })
            .collect();
    }

    let lo = probed.iter().map(|(_, t)| *t).fold(f64::INFINITY, f64::min);
    let hi = probed.iter().map(|(_, t)| *t).fold(f64::NEG_INFINITY, f64::max);
    let mut chosen: Vec<(f64, f64)> = Vec::new();
    for i in 0..steps.max(2) {
        // Geometric spacing: equal ratios of THD, which is how distortion is
        // heard and how a shaper's error distributes.
        let f = i as f64 / (steps.max(2) - 1) as f64;
        let target = lo * (hi / lo).powf(f);
        let best = probed
            .iter()
            .min_by(|a, b| {
                (a.1 / target).ln().abs().partial_cmp(&(b.1 / target).ln().abs()).unwrap()
            })
            .copied()
            .unwrap();
        if !chosen.iter().any(|(v, _)| (*v - best.0).abs() < 1e-12) {
            chosen.push(best);
        }
    }
    chosen.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    chosen
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!(
            "usage: saturation_capture --plugin <path> --out <dir> \
             [--levels -40,-30,…] [--freqs 100,1000,5000] [--set \"Name=v;…\"] \
             [--release-param <name>] [--no-sweep]"
        );
        std::process::exit(2);
    };
    let out_dir = PathBuf::from(arg("--out").unwrap_or_else(|| "sat".into()));
    let sample_rate: f64 = num("--sample-rate", 48_000.0);
    let levels = list("--levels", &[-48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, -3.0, 0.0]);
    let freqs = list("--freqs", &[100.0, 220.0, 440.0, 1000.0, 2200.0, 5000.0]);
    let n_harmonics: usize = num("--harmonics", 8);

    let mut plugin = match HostedPlugin::load(&path) {
        Ok(Some(mut p)) => {
            p.prepare(sample_rate, BLOCK as u32).expect("prepare");
            p
        }
        other => {
            eprintln!("{path}: could not load ({other:?})");
            std::process::exit(1);
        }
    };
    let descriptor = plugin.descriptor().clone();
    let params = plugin.params();

    // Fixed controls, by name — the same syntax comp_capture uses.
    let mut pinned = Vec::new();
    if let Some(spec) = arg("--set") {
        for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let Some((name, value)) = part.split_once('=') else {
                eprintln!("'{part}' is not Name=value");
                std::process::exit(2);
            };
            let value: f64 = value.trim().parse().unwrap_or_else(|_| {
                eprintln!("'{value}' is not a number");
                std::process::exit(2);
            });
            match params.iter().find(|p| p.name.eq_ignore_ascii_case(name.trim())) {
                Some(p) => pinned.push((p.id, p.name.clone(), value)),
                None => {
                    eprintln!("no parameter called '{}'", name.trim());
                    std::process::exit(2);
                }
            }
        }
    }
    for (id, _, v) in &pinned {
        plugin.set_param(*id, *v);
    }

    // Conditions: settings held for a whole level sweep. Two kinds, and they
    // answer different questions.
    //
    // `--drive-param` sweeps the unit's *own* input stage across its full
    // range. This is the measurement that matters for hardware models: an
    // 1176's Input knob drives the FET and the transformers, not just the
    // detector, so its saturation only appears in earnest well past the
    // point where a fixed-level stimulus stops revealing anything. Pushing it
    // to the stop is both how the unit is used and the only way to see the
    // top of its curve.
    //
    // `--release-param` instead measures the same sweep at two release
    // settings, which is how detector ripple is told from real saturation:
    // ripple moves with the time constants, saturation does not.
    let find = |name: &str| {
        params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
            .map(|p| (p.id, p.name.clone(), p.min, p.max))
    };
    let drive = arg("--drive-param").and_then(|n| find(&n));
    let release = arg("--release-param").and_then(|n| find(&n));
    if arg("--drive-param").is_some() && drive.is_none() {
        eprintln!("no parameter called '{}'", arg("--drive-param").unwrap());
        std::process::exit(2);
    }

    let drive_steps: usize = num("--drive-steps", 6);
    let thd_spaced = arg("--drive-spacing").as_deref() != Some("linear");
    let conditions: Vec<(String, Vec<(u32, f64)>)> = match (&drive, &release) {
        (Some((id, name, min, max)), _) => {
            let probe_freq = freqs.first().copied().unwrap_or(1000.0);
            // Probe at a mid level, not the loudest. At full level the unit is
            // already saturating whatever the drive knob is doing, which
            // compresses the range the probe can see and makes the drive
            // settings hard to tell apart: the 1176 spans 0.04%..0.67% THD
            // across its Input range at -12 dBFS, but only 0.50%..1.61% at
            // 0 dBFS. The wider view places the captures better.
            let probe_level = {
                let mut sorted = levels.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                sorted.get(sorted.len() / 2).copied().unwrap_or(-12.0)
            };
            let values: Vec<f64> = if thd_spaced {
                eprintln!("── probing {name} for THD coverage");
                let v = thd_spaced_drive(
                    &mut plugin, *id, *min, *max, drive_steps, probe_freq, probe_level, sample_rate,
                );
                // Report the THD span, not the first and last entries —
                // the list is ordered by knob position, and the quietest
                // setting is rarely the lowest one.
                let lo = v.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
                let hi = v.iter().map(|x| x.1).fold(f64::NEG_INFINITY, f64::max);
                eprintln!("   chose {} settings spanning {lo:.4}%..{hi:.4}% THD", v.len());
                // A control that does not move the distortion is not a drive
                // control. Several units have a "Gain" that sits *after* the
                // nonlinearity — a clean output trim — and sweeping it
                // produces a flawless set of identical measurements. The
                // symptom is a THD span of essentially 1:1, and it is worth
                // saying out loud, because the capture otherwise looks
                // entirely successful.
                if lo > 0.0 && hi / lo < 1.5 {
                    eprintln!(
                        "   WARNING: '{name}' barely changes distortion ({:.2}x span). \
                         It is probably a clean trim after the nonlinearity rather than a \
                         drive control — try the one that feeds the gain element.",
                        hi / lo
                    );
                }
                v.into_iter().map(|(x, _)| x).collect()
            } else {
                (0..drive_steps.max(2))
                    .map(|i| min + (max - min) * (i as f64 / (drive_steps.max(2) - 1) as f64))
                    .collect()
            };
            values
                .into_iter()
                .map(|v| {
                    (format!("{}-{v:.3}", name.to_lowercase().replace(' ', "-")), vec![(*id, v)])
                })
                .collect()
        }
        (None, Some((id, _, min, max))) => vec![
            ("release-min".to_string(), vec![(*id, *min)]),
            ("release-max".to_string(), vec![(*id, *max)]),
        ],
        (None, None) => vec![("default".to_string(), Vec::new())],
    };

    eprintln!("{} — {} parameters", descriptor.name, params.len());
    for (_, name, v) in &pinned {
        eprintln!("  pinned {name} = {v}");
    }

    std::fs::create_dir_all(&out_dir).expect("create output dir");
    let mut tone_rows = Vec::new();
    let mut sweep_rows = Vec::new();
    let mut curves: Vec<(f64, f64, String, transfer_curve::TransferCurve)> = Vec::new();

    for (pass_name, settings) in &conditions {
        for (id, v) in settings {
            plugin.set_param(*id, *v);
        }
        if !settings.is_empty() {
            eprintln!("── {pass_name}");
        }

        // ── Steady tones: the saturation curve ──────────────────────────
        for &freq in &freqs {
            for &level in &levels {
                let spec = ToneSpec { freq_hz: freq, level_db: level, duration_s: 3.0 };
                settle(&mut plugin, sample_rate);
                let rendered = render(&mut plugin, &harmonics::tone(&spec, sample_rate));
                // Skip a second so the envelope has settled — measuring across
                // the attack reads the transient, not the steady state.
                let h = harmonics::analyze(
                    &rendered,
                    freq,
                    sample_rate,
                    n_harmonics,
                    sample_rate as usize,
                );
                // The static curve at this operating point, with the gain
                // divided out — the saturation on its own, in a form a
                // waveshaper can apply.
                let tc = transfer_curve::extract(
                    &harmonics::tone(&spec, sample_rate),
                    &rendered,
                    sample_rate as usize,
                    129,
                    2048,
                );
                curves.push((freq, level, pass_name.clone(), tc.clone()));

                tone_rows.push(serde_json::json!({
                    "pass": pass_name,
                    "freq_hz": freq,
                    "input_db": level,
                    "output_db": h.fundamental_db,
                    "gain_db": h.fundamental_db - level,
                    "thd_percent": h.thd_percent,
                    "thd_db": h.thd_db,
                    "even_db": h.even_db,
                    "odd_db": h.odd_db,
                    "noise_floor_db": h.noise_floor_db,
                    "harmonics_db": h.harmonics_db,
                    "small_signal_gain_db": 20.0 * tc.small_signal_gain.abs().max(1e-12).log10(),
                    "asymmetry": tc.asymmetry,
                    "nonlinearity": tc.nonlinearity,
                }));
            }
            eprintln!("   tones at {freq} Hz");
        }

        // ── Swept sine: per-order responses ─────────────────────────────
        if arg("--no-sweep").is_none() && !std::env::args().any(|a| a == "--no-sweep") {
            for &level in &[-24.0, -12.0, -6.0, 0.0] {
                let spec = SweepSpec { start_hz: 20.0, end_hz: 20_000.0, duration_s: 8.0, level_db: level };
                settle(&mut plugin, sample_rate);
                let rendered = render(&mut plugin, &swept_sine::sweep(&spec, sample_rate));
                let d = swept_sine::deconvolve(&rendered, &spec, sample_rate);
                let energy = |b: &[f32]| {
                    b.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>()
                };
                let linear = energy(d.order(1, &spec)).max(1e-30);
                let orders: Vec<f64> = (2..=n_harmonics)
                    .map(|n| 10.0 * (energy(d.order(n, &spec)) / linear).log10())
                    .collect();
                sweep_rows.push(serde_json::json!({
                    "pass": pass_name,
                    "level_db": level,
                    "orders_db_rel_linear": orders,
                }));
            }
            eprintln!("   swept sine, 4 levels");
        }
    }

    // Does the saturation actually separate from the gain? Compare the
    // normalised shapes measured at every operating point against the one
    // measured at the reference level. Tight agreement means a fixed
    // waveshaper plus a separate gain is a faithful decomposition; poor
    // agreement means the two are entangled and applying them independently
    // cannot reproduce the device however well each half is fitted.
    let mut separability = Vec::new();
    for &freq in &freqs {
        let at = |f: f64, want_level: f64, pass: &str| {
            curves.iter().find(|(cf, cl, cp, _)| {
                (*cf - f).abs() < 1e-9 && (*cl - want_level).abs() < 1e-9 && cp == pass
            })
        };
        let reference_level = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for (pass_name, _) in &conditions {
            let Some((_, _, _, reference)) = at(freq, reference_level, pass_name) else { continue };
            for &level in &levels {
                if (level - reference_level).abs() < 1e-9 {
                    continue; // the reference does not agree with itself informatively
                }
                let Some((_, _, _, c)) = at(freq, level, pass_name) else { continue };
                // How much bend there was to compare. Agreement is only
                // meaningful where the residual rises above the measurement
                // floor: at levels where the device is barely distorting,
                // the "shape" being compared is mostly noise, and a poor
                // agreement there says nothing about separability.
                let residual_rms = |c: &transfer_curve::TransferCurve| {
                    let r = c.residual();
                    if r.is_empty() {
                        return 0.0;
                    }
                    (r.iter().map(|(_, y)| y * y).sum::<f64>() / r.len() as f64).sqrt()
                };
                separability.push(serde_json::json!({
                    "pass": pass_name,
                    "freq_hz": freq,
                    "level_db": level,
                    "vs_level_db": reference_level,
                    "agreement_db": transfer_curve::agreement(reference, c),
                    "residual_rms": residual_rms(c),
                    "reference_residual_rms": residual_rms(reference),
                }));
            }
        }
    }

    // Does the shape survive being driven harder? Compare each condition's
    // curve, at the loudest stimulus level, against the first condition's.
    // This is the question a drive sweep exists to answer: if the normalised
    // shape is the same at every drive setting, one waveshaper plus a gain in
    // front of it reproduces the unit; if it changes, the nonlinearity is
    // inside the gain element and the two cannot be separated.
    let mut drive_separability = Vec::new();
    if conditions.len() > 1 {
        let reference_level = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for &freq in &freqs {
            let at = |pass: &str| {
                curves.iter().find(|(cf, cl, cp, _)| {
                    (*cf - freq).abs() < 1e-9
                        && (*cl - reference_level).abs() < 1e-9
                        && cp == pass
                })
            };
            let residual_rms = |c: &transfer_curve::TransferCurve| {
                let r = c.residual();
                if r.is_empty() {
                    return 0.0;
                }
                (r.iter().map(|(_, y)| y * y).sum::<f64>() / r.len() as f64).sqrt()
            };
            // Compare against the condition with the *most* bend, not the
            // first one. `agreement` normalises by the reference's residual
            // energy, so a reference that is barely distorting makes every
            // comparison look bad for a reason that is about measurement
            // noise rather than about the device. The best-measured curve is
            // the honest yardstick.
            let Some((_, _, ref_name, first)) = curves
                .iter()
                .filter(|(cf, cl, _, c)| {
                    (*cf - freq).abs() < 1e-9
                        && (*cl - reference_level).abs() < 1e-9
                        && c.is_usable()
                })
                .max_by(|a, b| {
                    residual_rms(&a.3)
                        .partial_cmp(&residual_rms(&b.3))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(f, l, p, c)| (f, l, p.clone(), c))
            else {
                continue;
            };
            for (pass_name, settings) in conditions.iter() {
                if *pass_name == ref_name {
                    continue;
                }
                let Some((_, _, _, c)) = at(pass_name) else { continue };
                drive_separability.push(serde_json::json!({
                    "freq_hz": freq,
                    "level_db": reference_level,
                    "condition": pass_name,
                    "vs_condition": ref_name,
                    "settings": settings.iter().map(|(id, v)| {
                        let name = params.iter().find(|p| p.id == *id).map(|p| p.name.clone()).unwrap_or_default();
                        serde_json::json!({"name": name, "value": v})
                    }).collect::<Vec<_>>(),
                    "agreement_db": transfer_curve::agreement(first, c),
                    "residual_rms": residual_rms(c),
                    "gain_db": 20.0 * c.small_signal_gain.abs().max(1e-12).log10(),
                }));
            }
        }
    }

    // The curves themselves, normalised — this is the thing to implement.
    let shapes: Vec<_> = curves
        .iter()
        .map(|(f, l, p, c)| {
            let n = c.normalised();
            serde_json::json!({
                "pass": p, "freq_hz": f, "level_db": l,
                "small_signal_gain": c.small_signal_gain,
                "peak": c.peak,
                "x": n.iter().map(|(x, _)| *x).collect::<Vec<_>>(),
                "y": n.iter().map(|(_, y)| *y).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Did anything we pinned actually take effect?
    //
    // A capture where the output is bit-identical to the input is not a
    // measurement of a transparent setting — far more often it is a setting
    // that never reached the processor. Soundtoys' Decapitator does this:
    // its edit controller acknowledges `Style = E` (value_to_text returns the
    // new style) while the processor passes audio through untouched, and it
    // never recovers. Ten of its eighteen jobs archived a tidy 0.0000% before
    // this check existed.
    let passthrough = !tone_rows.is_empty()
        && tone_rows.iter().all(|t| {
            t["thd_percent"].as_f64().map(|v| v < 1e-9).unwrap_or(false)
                && t["gain_db"].as_f64().map(|v| v.abs() < 1e-6).unwrap_or(false)
        });
    if passthrough && !pinned.is_empty() {
        eprintln!(
            "   WARNING: output is bit-identical to input at every level and frequency, \
             with {} control(s) pinned. Either this setting is genuinely transparent, or \
             it never reached the processor — check that the plugin responds to it at all \
             before treating this as data.",
            pinned.len()
        );
    }

    let report = serde_json::json!({
        "plugin_path": path,
        "plugin_name": descriptor.name,
        "plugin_id": descriptor.id,
        "sample_rate": sample_rate,
        "harmonics": n_harmonics,
        "pinned": pinned.iter().map(|(id, n, v)| serde_json::json!({"id": id, "name": n, "value": v})).collect::<Vec<_>>(),
        "passthrough": passthrough,
        "release_param": release.as_ref().map(|(_, n, _, _)| n.clone()),
        "tones": tone_rows,
        "sweeps": sweep_rows,
        "separability": separability,
        "drive_separability": drive_separability,
        "drive_param": drive.as_ref().map(|(_, n, _, _)| n.clone()),
        "shapes": shapes,
    });
    let path_out = out_dir.join("saturation.json");
    std::fs::write(&path_out, serde_json::to_string_pretty(&report).unwrap()).expect("write");
    eprintln!("wrote {}", path_out.display());
}
