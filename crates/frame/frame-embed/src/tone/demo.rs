//! Demo settings and feeds for a rack: what the harness renders, and what
//! a host without an engine can show. Varied per track `index`, the way
//! session's `tone::placeholder` varies its seeded tracks.

use surface_proto::{Catalog, EntryId, ParamAddr, ParamKind, ParamValue, StreamFrame};

use super::Which;
use super::catalog::HISTORY;

fn norm(catalog: &Catalog, addr: &str, real: f64) -> Option<ParamValue> {
    let p = catalog.param(&ParamAddr::new(addr))?;
    let value = match &p.kind {
        ParamKind::Choice { options } => {
            (real.round() / (options.len().max(2) - 1) as f64).clamp(0.0, 1.0)
        }
        ParamKind::Toggle => f64::from(u8::from(real >= 0.5)),
        ParamKind::Continuous => p.range.to_normalized(real),
        // Text params (names) carry no number to set.
        ParamKind::Text => return None,
    };
    Some(ParamValue {
        addr: p.addr.clone(),
        value,
    })
}

/// A cheap deterministic hash → 0..1.
fn rnd(seed: usize, k: usize) -> f64 {
    let mut x = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (k as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x % 10_000) as f64 / 10_000.0
}

/// Engineering-unit settings for track `index`'s `units`.
#[must_use]
pub fn settings(units: &[Which], index: usize) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let r = |k: usize| rnd(index, k);
    for (u, which) in units.iter().enumerate() {
        let ns = which.ns();
        let mut set = |k: &str, v: f64| out.push((format!("{ns}/{k}"), v));
        let s = u * 32;
        match which {
            w if w.is_eq() => {
                // A low cut or shelf, a cut in the mids, a presence lift, air.
                let bands = [
                    (
                        40.0 + 80.0 * r(s),
                        0.0,
                        0.7,
                        if r(s + 1) > 0.5 { 3.0 } else { 1.0 },
                    ),
                    (
                        180.0 + 500.0 * r(s + 2),
                        -2.0 - 6.0 * r(s + 3),
                        1.0 + 3.0 * r(s + 4),
                        0.0,
                    ),
                    (
                        1500.0 + 3000.0 * r(s + 5),
                        1.0 + 5.0 * r(s + 6),
                        0.8 + 1.5 * r(s + 7),
                        0.0,
                    ),
                    (8000.0 + 6000.0 * r(s + 8), 1.0 + 4.0 * r(s + 9), 0.71, 2.0),
                ];
                for (n, (f, g, q, shape)) in bands.iter().enumerate() {
                    let n = n + 1;
                    set(&format!("b{n}_freq"), *f);
                    set(
                        &format!("b{n}_gain"),
                        if *which == Which::PolishEq && n != 2 {
                            0.0
                        } else {
                            *g
                        },
                    );
                    set(&format!("b{n}_q"), *q);
                    set(&format!("b{n}_shape"), *shape);
                    set(&format!("b{n}_on"), 1.0);
                }
                // The first band is a low shelf with some gain when it is one.
                if r(s + 1) <= 0.5 {
                    set("b1_gain", 2.0 + 3.0 * r(s + 10));
                }
                set("range", if r(s + 11) > 0.6 { 1.0 } else { 2.0 });
            }
            w if w.is_comp() => {
                set("threshold", -12.0 - 14.0 * r(s));
                set("ratio", 2.0 + 4.0 * r(s + 1));
                set("attack", 1.0 + 30.0 * r(s + 2));
                set("release", 60.0 + 400.0 * r(s + 3));
            }
            Which::Gate => {
                set("threshold", -44.0 + 14.0 * r(s));
                set("range", -12.0 - 18.0 * r(s + 1));
            }
            Which::Sat => {
                set("drive", 1.2 + 4.0 * r(s));
                set("bias", 0.05 + 0.3 * r(s + 1));
                set("circuit", (r(s + 2) * 4.99).floor());
            }
            Which::DeEss => {
                set("low", 4000.0 + 2000.0 * r(s));
                set("high", 9000.0 + 3000.0 * r(s + 1));
                set("threshold", -30.0 + 10.0 * r(s + 2));
            }
            Which::Delay => {
                set("time", 120.0 + 400.0 * r(s));
                set("feedback", 0.25 + 0.5 * r(s + 1));
                set("mix", 0.2 + 0.4 * r(s + 2));
                set("family", (r(s + 3) * 5.99).floor());
            }
            _ => {
                set("decay", 0.8 + 3.0 * r(s));
                set("predelay", 10.0 + 60.0 * r(s + 1));
                set("mix", 0.1 + 0.4 * r(s + 2));
                set("family", (r(s + 3) * 4.99).floor());
            }
        }
    }
    out
}

/// [`settings`] as normalized param values for `catalog`.
#[must_use]
pub fn values(catalog: &Catalog, units: &[Which], index: usize) -> Vec<ParamValue> {
    settings(units, index)
        .into_iter()
        .filter_map(|(a, v)| norm(catalog, &a, v))
        .collect()
}

/// A drum-like input: a hit every `period` readings, decaying.
fn input_db(i: usize, period: usize, seed: usize) -> f64 {
    let phase = i % period;
    let accent = if (i / period).is_multiple_of(2) {
        0.0
    } else {
        -3.0 * rnd(seed, i / period)
    };
    -2.0 + accent - phase as f64 * (40.0 / period as f64)
}

/// The reduction a history of input levels gets: the transfer's target,
/// approached at the attack and release rates (session `reduction_db`).
#[must_use]
pub fn reduction(
    input: &[f64],
    threshold: f64,
    ratio: f64,
    knee: f64,
    attack_ms: f64,
    release_ms: f64,
    hz: f64,
) -> Vec<f64> {
    let step = 1000.0 / hz;
    let coef = |ms: f64| (-step / ms.max(0.01)).exp();
    let (a, r) = (coef(attack_ms), coef(release_ms));
    let mut held = 0.0;
    input
        .iter()
        .map(|&db| {
            let slope = 1.0 - 1.0 / ratio.max(1.0);
            let over = db - threshold;
            let target = if knee > 0.001 && over.abs() < knee / 2.0 {
                let x = over + knee / 2.0;
                slope * x * x / (2.0 * knee)
            } else if over > 0.0 {
                slope * over
            } else {
                0.0
            };
            let rate = if target > held { a } else { r };
            held = target + (held - target) * rate;
            -held
        })
        .collect()
}

/// Feed frames for track `index` at reading `tick` (30 per second).
#[must_use]
pub fn frames(catalog: &Catalog, units: &[Which], index: usize, tick: usize) -> Vec<StreamFrame> {
    let settings: std::collections::HashMap<String, f64> =
        settings(units, index).into_iter().collect();
    let g = |ns: &str, k: &str, d: f64| settings.get(&format!("{ns}/{k}")).copied().unwrap_or(d);
    let period = 6 + index % 9;
    let len = HISTORY as usize;
    let input: Vec<f64> = (0..len)
        .map(|i| input_db(i + tick, period, index))
        .collect();
    let mut out = Vec::new();
    let mut push = |id: String, values: Vec<f64>| {
        if catalog.stream(&EntryId::new(id.clone())).is_some() {
            out.push(StreamFrame {
                id: EntryId::new(id),
                values,
            });
        }
    };
    for which in units {
        let ns = which.ns();
        match which {
            w if w.is_eq() => {
                // Pink-ish, with a body bump and some ring.
                let spec: Vec<f64> = (0..96)
                    .map(|b| {
                        let t = b as f64 / 95.0;
                        let hz = 20.0 * 1000f64.powf(t);
                        -18.0 - 10.0 * t
                            + 8.0 * (-((hz / 180.0).ln()).powi(2)).exp()
                            + 6.0 * (b as f64 * 0.7 + index as f64).sin() * 0.5
                            - 12.0 * (t - 0.85).max(0.0) * 6.0
                    })
                    .collect();
                push(format!("{ns}/spectrum"), spec);
            }
            w if w.is_comp() => {
                let gr = reduction(
                    &input,
                    g(ns, "threshold", -18.0),
                    g(ns, "ratio", 4.0),
                    6.0,
                    g(ns, "attack", 10.0),
                    g(ns, "release", 150.0),
                    30.0,
                );
                push(format!("{ns}/in_history"), input.clone());
                push(format!("{ns}/gr"), vec![*gr.last().unwrap_or(&0.0)]);
                push(format!("{ns}/gr_history"), gr);
            }
            Which::Gate => {
                let thr = g(ns, "threshold", -40.0);
                push(
                    format!("{ns}/in_history"),
                    input.iter().map(|v| v - 6.0).collect(),
                );
                push(
                    format!("{ns}/open_history"),
                    input
                        .iter()
                        .map(|v| f64::from(u8::from(v - 6.0 > thr)))
                        .collect(),
                );
                push(format!("{ns}/in"), vec![input[len - 1] - 6.0]);
            }
            Which::Sat => push(
                format!("{ns}/peak"),
                vec![10f64.powf(input[len - 1] / 20.0).clamp(0.0, 1.0)],
            ),
            Which::DeEss => {
                let lvl: Vec<f64> = (0..len)
                    .map(|i| -30.0 + 18.0 * ((i + tick) as f64 * 0.45).sin().max(0.0))
                    .collect();
                let thr = g(ns, "threshold", -24.0);
                push(
                    format!("{ns}/gr_history"),
                    lvl.iter().map(|l| -(l - thr).clamp(0.0, 12.0)).collect(),
                );
                push(format!("{ns}/gr"), vec![-(lvl[len - 1] - thr).max(0.0)]);
                push(format!("{ns}/level_history"), lvl);
                push(
                    format!("{ns}/spectrum"),
                    (0..96)
                        .map(|b| -20.0 - 12.0 * (b as f64 / 95.0) + 5.0 * (b as f64 * 0.9).sin())
                        .collect(),
                );
            }
            _ => push(format!("{ns}/wet"), vec![0.3 + 0.4 * rnd(index, tick / 10)]),
        }
    }
    out
}
