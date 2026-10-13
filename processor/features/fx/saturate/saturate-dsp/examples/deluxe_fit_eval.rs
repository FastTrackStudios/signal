//! Parallel evaluator for fitting Deluxe's detector: runs the real engine
//! over a pack of datasets for a batch of detector parameter sets and writes
//! the output-envelope residuals. The measurement tools' optimiser calls it
//! for function values and for whole finite-difference Jacobians at once.
//!
//! ```sh
//! deluxe_fit_eval pack.txt params.txt out.f64 [threads]
//! ```
//! `pack.txt`: one dataset per line —
//! `name crush crunch release(0|1) darkness_hz weight window input.f64 env_ref.f64 mask.f64`
//! (raw little-endian f64; `env_ref` is the reference's RMS envelope in dB
//! per `window` samples, `mask` 1/0 per window).
//! `params.txt`: one parameter set per line, the [`DetectorParams`] fields in
//! declaration order. `out.f64`: for each set, the residuals of every
//! dataset's masked windows, `weight·(ours − ref)/√n`, concatenated.

use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use dsp_core::num::count_to_f64;
use saturate_dsp::deluxe::{Deluxe, DeluxeParams, DetectorParams};

struct Dataset {
    settings: DeluxeParams,
    weight: f64,
    window: usize,
    input: Vec<f64>,
    env_ref: Vec<f64>,
    mask: Vec<bool>,
}

fn read_f64(path: &str) -> Result<Vec<f64>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(raw.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8]))).collect())
}

fn parse_pack(path: &str) -> Result<Vec<Dataset>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let [_, crush, crunch, rel, dark, weight, window, input, env, mask] = f.as_slice() else {
                return Err(format!("bad pack line: {line}"));
            };
            let num = |s: &str| s.parse::<f64>().map_err(|e| format!("{s}: {e}"));
            Ok(Dataset {
                settings: DeluxeParams { crush: num(crush)?, crunch: num(crunch)?, darkness_hz: num(dark)?, mix: 100.0, release_on: num(rel)? != 0.0 },
                weight: num(weight)?,
                window: window.parse().map_err(|e| format!("{window}: {e}"))?,
                input: read_f64(input)?,
                env_ref: read_f64(env)?,
                mask: read_f64(mask)?.iter().map(|&m| m > 0.5).collect(),
            })
        })
        .collect()
}

fn parse_params(path: &str) -> Result<Vec<DetectorParams>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let v: Vec<f64> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|e| format!("{line}: {e}"))?;
            if v.len() != 22 {
                return Err(format!("expected 22 values: {line}"));
            }
            let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
            Ok(DetectorParams {
                offset_db: at(0),
                theta_db: at(1),
                smooth_ms: at(2),
                power: at(3),
                soft: at(4),
                charge_per_s: at(5),
                charge_max_db_per_ms: at(6),
                tau_on_ms: at(7),
                tau_off_ms: at(8),
                floor_db: at(9),
                floor_knee_db: at(10),
                fast_k: at(11),
                fast_t_db: at(12),
                fast_w_db: at(13),
                fast_attack_ms: at(14),
                fast_release_on_ms: at(15),
                fast_release_off_ms: at(16),
                slow_weight: at(17),
                slow_charge_ms: at(18),
                slow_release_ms: at(19),
                smooth_attack_ms: at(20),
                det_floor_db: at(21),
            })
        })
        .collect()
}

/// The residuals of one dataset under one parameter set.
fn residuals(d: &Dataset, p: DetectorParams) -> Vec<f64> {
    let mut fx = Deluxe::new(48_000.0);
    fx.set(d.settings);
    fx.set_detector(p);
    let mut out = Vec::new();
    let ys: Vec<f64> = d.input.iter().map(|&x| fx.process(x, x).0).collect();
    let window = count_to_f64(d.window.max(1));
    let env: Vec<f64> = ys.chunks_exact(d.window.max(1)).map(|w| 10.0 * (w.iter().map(|y| y * y).sum::<f64>() / window + 1e-18).log10()).collect();
    let used = d.mask.iter().zip(&env).filter(|&(&m, _)| m).count().max(1);
    let scale = d.weight / count_to_f64(used).sqrt();
    for ((&m, &ours), &reference) in d.mask.iter().zip(&env).zip(&d.env_ref) {
        if m {
            out.push(scale * (ours - reference));
        }
    }
    out
}

fn run(args: &[String]) -> Result<(), String> {
    let [_, pack, params, output, rest @ ..] = args else {
        return Err("usage: deluxe_fit_eval pack.txt params.txt out.f64 [threads]".into());
    };
    let data = parse_pack(pack)?;
    let sets = parse_params(params)?;
    let threads = rest.first().and_then(|t| t.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get));
    let jobs: Vec<(usize, usize)> = (0..sets.len()).flat_map(|s| (0..data.len()).map(move |d| (s, d))).collect();
    let results: Vec<Mutex<Vec<f64>>> = jobs.iter().map(|_| Mutex::new(Vec::new())).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(s, d)) = jobs.get(i) else { break };
                if let (Some(set), Some(ds), Some(slot)) = (sets.get(s), data.get(d), results.get(i)) {
                    let r = residuals(ds, *set);
                    if let Ok(mut guard) = slot.lock() {
                        *guard = r;
                    }
                }
            });
        }
    });
    let mut bytes = Vec::new();
    for slot in &results {
        let r = slot.lock().map_err(|_| "poisoned".to_string())?;
        for v in r.iter() {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(output, bytes).map_err(|e| format!("{output}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("deluxe_fit_eval: {e}");
            ExitCode::FAILURE
        }
    }
}
