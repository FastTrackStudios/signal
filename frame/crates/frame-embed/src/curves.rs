//! Curve providers for the Tone rack's panels.
//!
//! frame's builtins (`eq.response`, `comp.transfer`, …) plus the ones the
//! rack needs that frame did not have:
//!
//! | provider | inputs | series |
//! |---|---|---|
//! | `sat.transfer` | `drive`, `bias`, `circuit`, `bits`, `reach` (a live peak: only \|x\| ≤ reach) | `transfer` (x, y in −1..1), `unity` |
//! | `sat.harmonics` | `drive`, `bias`, `circuit`, `level` | `rungs`, `evens`, `odds`: (k, share) for H2…H8, k = 0…6 |
//! | `echo.taps` | `time` (ms), `feedback`, `family` | `taps`: stems (x in delay times, y level 0..1), `dry` |
//! | `room.tail` | `decay` (s), `predelay` (ms), `mix`, `family` | `tail`: (x 0..1 of the decay window, y 0..1) |
//!
//! These are frame's own stand-ins so a surface works standalone. **The
//! host overrides any of them with the plugin's own maths** — the whole
//! point of the rack is that its curve and the plugin window's cannot
//! disagree — by registering the same name
//! ([`crate::Embed::register_curve`], or on a [`CurveRegistry`]):
//!
//! ```ignore
//! embed.register_curve("eq.response", |i| session_eq_response(i));      // eq_ui::calculate_combined_response
//! embed.register_curve("comp.transfer", |i| session_comp_transfer(i));  // comp_ui::compress_transfer
//! embed.register_curve("sat.transfer", |i| session_sat_transfer(i));    // saturate_dsp::…::transfer_curve
//! embed.register_curve("room.tail", |i| session_room_tail(i));          // session live::tail(room.key())
//! ```

use std::f64::consts::PI;

use surface_runtime::{CurveInputs, CurveRegistry, Series};

/// frame's builtins plus the Tone rack's.
#[must_use]
pub fn registry() -> CurveRegistry {
    let mut r = CurveRegistry::builtin();
    register(&mut r);
    r
}

/// Add the Tone rack's providers to `r` (overriding nothing a host already
/// registered under the same name would want kept — call this first).
pub fn register(r: &mut CurveRegistry) {
    r.register("sat.transfer", sat_transfer);
    r.register("sat.harmonics", sat_harmonics);
    r.register("echo.taps", echo_taps);
    r.register("room.tail", room_tail);
}

/// The saturator circuits, in the plugin's rail order.
pub const CIRCUITS: [&str; 5] = ["Valve", "Tape", "Core", "Solid", "Steps"];

/// One sample of a circuit's static curve: `x` in −1..1 → out, normalised
/// so full scale in lands at ±1 out.
#[must_use]
pub fn sat_sample(x: f64, drive: f64, bias: f64, circuit: usize) -> f64 {
    let d = drive.max(0.05);
    let shape = |v: f64| match circuit {
        // Valve: an asymmetric soft knee — the bias pushes one side into
        // the bend first, which is where the even harmonics come from.
        0 => (d * (v + bias)).tanh() - (d * bias).tanh(),
        // Tape: softer, rounder, arctan.
        1 => (d * 1.2 * (v + bias * 0.5)).atan() - (d * 1.2 * bias * 0.5).atan(),
        // Core (transformer): cubic soft clip.
        2 => {
            let u = (d * (v + bias * 0.3)).clamp(-1.5, 1.5);
            u - u * u * u / 6.75 - {
                let b = (d * bias * 0.3).clamp(-1.5, 1.5);
                b - b * b * b / 6.75
            }
        }
        // Solid state: nearly linear, then a hard shoulder.
        3 => {
            let u = d * (v + bias * 0.2);
            u / (1.0 + u.abs().powi(6)).powf(1.0 / 6.0) - {
                let b = d * bias * 0.2;
                b / (1.0 + b.abs().powi(6)).powf(1.0 / 6.0)
            }
        }
        // Steps: a rail clip; the quantiser is `bits`.
        _ => (d * v).clamp(-1.0, 1.0),
    };
    let top = shape(1.0).abs().max(shape(-1.0).abs()).max(1e-6);
    shape(x) / top
}

fn sat_transfer(i: &CurveInputs) -> Vec<Series> {
    let drive = i.or("drive", 2.0);
    let bias = i.or("bias", 0.15);
    let circuit = i.or("circuit", 0.0).round().max(0.0) as usize;
    let bits = i.or("bits", 0.0);
    // `reach` (a live peak, 0..1): only the part of the curve the signal
    // is on — the saturator's "is it doing anything" stroke.
    let reach = i.get("reach").map(|r| r.clamp(0.0, 1.0));
    let n = i.samples.max(16);
    let pts = (0..n)
        .map(|k| {
            let x = -1.0 + 2.0 * k as f64 / (n - 1) as f64;
            let mut y = sat_sample(x, drive, bias, circuit);
            if bits >= 1.0 {
                let levels = (bits.clamp(1.0, 16.0) - 1.0).exp2();
                y = (y * levels).round() / levels;
            }
            (x, y)
        })
        .filter(|(x, _)| reach.is_none_or(|r| x.abs() <= r))
        .collect();
    vec![
        Series::new("transfer", pts),
        Series::new("unity", vec![(-1.0, -1.0), (1.0, 1.0)]),
    ]
}

/// H2…H8 of the circuit at `level` (0..1 peak), by DFT of one cycle
/// through [`sat_sample`] — measured, like the plugin's probe, not guessed.
#[must_use]
pub fn harmonics(drive: f64, bias: f64, circuit: usize, level: f64) -> [f64; 7] {
    const N: usize = 256;
    let wave: Vec<f64> = (0..N)
        .map(|k| {
            sat_sample(
                level * (2.0 * PI * k as f64 / N as f64).sin(),
                drive,
                bias,
                circuit,
            )
        })
        .collect();
    let amp = |h: usize| {
        let (mut re, mut im) = (0.0, 0.0);
        for (k, v) in wave.iter().enumerate() {
            let a = 2.0 * PI * (h * k) as f64 / N as f64;
            re += v * a.cos();
            im += v * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / N as f64
    };
    let fundamental = amp(1).max(1e-9);
    let mut out = [0.0; 7];
    for (k, o) in out.iter_mut().enumerate() {
        *o = (amp(k + 2) / fundamental).clamp(0.0, 1.0);
    }
    out
}

fn sat_harmonics(i: &CurveInputs) -> Vec<Series> {
    let rungs = harmonics(
        i.or("drive", 2.0),
        i.or("bias", 0.15),
        i.or("circuit", 0.0).round().max(0.0) as usize,
        i.or("level", 1.0).clamp(0.0, 1.0),
    );
    // A share on a square root so the fourth and sixth show beside a second
    // that dwarfs them (session draws the ladder the same way).
    let pts: Vec<(f64, f64)> = rungs
        .iter()
        .enumerate()
        .map(|(k, r)| (k as f64, (r * 4.0).clamp(0.0, 1.0).sqrt()))
        .collect();
    // Rung k is harmonic k + 2: the evens are the even k.
    let only = |even: bool| {
        pts.iter()
            .map(|&(k, v)| {
                (
                    k,
                    if (k as usize).is_multiple_of(2) == even {
                        v
                    } else {
                        f64::NAN
                    },
                )
            })
            .collect()
    };
    vec![
        Series::new("rungs", pts.clone()),
        Series::new("evens", only(true)),
        Series::new("odds", only(false)),
    ]
}

/// The even share of what the circuit adds (the number an engineer reads
/// a saturator by).
#[must_use]
pub fn even_share(rungs: &[f64; 7]) -> f64 {
    let total: f64 = rungs.iter().sum();
    if total <= 1e-12 {
        0.0
    } else {
        rungs.iter().step_by(2).sum::<f64>() / total
    }
}

/// Where a rhythmic delay's taps land, in delay times (session's `PATTERN`).
const PATTERN: [f64; 5] = [1.0, 0.5, 0.75, 0.5, 1.25];

/// The delay families, in the plugin's order.
pub const DELAY_FAMILIES: [&str; 6] = ["Digital", "Tape", "Analog", "Rhythmic", "Pitch", "Special"];

/// Taps of a delay: `(when, level)`, when in delay times, the dry hit first.
#[must_use]
pub fn taps(feedback: f64, family: usize) -> Vec<(f64, f64)> {
    let feedback = feedback.clamp(0.0, 0.99);
    let mut out = Vec::new();
    let (mut when, mut level) = (0.0, 1.0);
    for tap in 0..16 {
        if when > 4.5 || level < 0.03 {
            break;
        }
        out.push((when, level));
        when += if family == 3 {
            PATTERN[tap % PATTERN.len()]
        } else {
            1.0
        };
        level *= feedback;
        if tap == 0 {
            level = level.max(0.0);
        }
    }
    out
}

fn echo_taps(i: &CurveInputs) -> Vec<Series> {
    let family = i.or("family", 0.0).round().max(0.0) as usize;
    let all = taps(i.or("feedback", 0.4), family);
    let mut stems = Vec::new();
    for (k, (x, level)) in all.iter().enumerate().skip(1) {
        // Tape loses top each pass; a chip smears; pitch rises — the
        // family's shape is drawn by the panel, the level is the taps'.
        let dull = if family == 1 {
            (1.0 - 0.08 * k as f64).max(0.4)
        } else {
            1.0
        };
        stems.extend([(*x, 0.0), (*x, level * dull), (*x, f64::NAN)]);
    }
    vec![
        Series::new("taps", stems),
        Series::new("dry", vec![(0.0, 0.0), (0.0, 1.0)]),
    ]
}

/// The reverb families, in the plugin's order.
pub const ROOM_FAMILIES: [&str; 5] = ["Room", "Hall", "Plate", "Spring", "Bloom"];

/// A reverb's envelope at `t` (0..1 of its decay window), 0..1.
#[must_use]
pub fn tail_at(t: f64, family: usize) -> f64 {
    // −60 dB across the window, as a level on a 60 dB scale: a straight
    // line falling to the floor, shaped per family.
    let base = (1.0 - t).clamp(0.0, 1.0);
    match family {
        // A room: early reflections, then a quick fall.
        0 => base.powf(1.6) * (1.0 + 0.12 * (t * 60.0).sin().abs() * (1.0 - t)),
        // A plate: dense from the first millisecond, a long even fall.
        2 => base.powf(0.9),
        // A spring: drips — a ringing ripple on the fall.
        3 => base * (0.8 + 0.2 * (t * 34.0).cos()),
        // Bloom: rises before it falls.
        4 => (t * 5.0).min(1.0) * base.powf(0.8),
        // A hall.
        _ => base,
    }
}

fn room_tail(i: &CurveInputs) -> Vec<Series> {
    let family = i.or("family", 1.0).round().max(0.0) as usize;
    let mix = i.or("mix", 0.3).clamp(0.0, 1.0);
    // The mix scales the tail's height, never below a quarter (session).
    let scale = mix.mul_add(0.75, 0.25);
    let window = (i.or("decay", 2.0) * 1000.0).max(1.0);
    let start = (i.or("predelay", 20.0) / window).clamp(0.0, 0.5);
    let n = i.samples.clamp(16, 128);
    let pts = (0..n)
        .map(|k| {
            let t = k as f64 / (n - 1) as f64;
            (
                start + t * (1.0 - start),
                tail_at(t, family).clamp(0.0, 1.0) * scale * 0.92,
            )
        })
        .collect();
    vec![Series::new("tail", pts)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use surface_proto::Axis;

    #[test]
    fn a_valve_is_asymmetric_and_its_evens_show() {
        let valve = harmonics(3.0, 0.3, 0, 1.0);
        let rail = harmonics(3.0, 0.0, 4, 1.0);
        assert!(
            even_share(&valve) > 0.25,
            "biased valve adds evens: {valve:?}"
        );
        assert!(
            even_share(&rail) < 0.05,
            "a symmetric clip is odd: {rail:?}"
        );
        assert!(
            (sat_sample(1.0, 2.0, 0.2, 0) - 1.0).abs() < 1e-9
                || (sat_sample(-1.0, 2.0, 0.2, 0) + 1.0).abs() < 1e-9
        );
    }

    #[test]
    fn delay_taps_decay_by_the_feedback() {
        let t = taps(0.5, 0);
        assert_eq!(t[0], (0.0, 1.0));
        assert!((t[1].1 - 0.5).abs() < 1e-9 && (t[2].1 - 0.25).abs() < 1e-9);
        assert!(
            taps(0.9, 3)
                .windows(2)
                .any(|w| (w[1].0 - w[0].0 - 0.5).abs() < 1e-9),
            "rhythmic taps land on subdivisions"
        );
    }

    #[test]
    fn the_registry_has_the_racks_providers() {
        let r = registry();
        for p in [
            "eq.response",
            "comp.transfer",
            "sat.transfer",
            "sat.harmonics",
            "echo.taps",
            "room.tail",
        ] {
            assert!(r.contains(p), "{p}");
        }
        let s = r
            .eval(
                "room.tail",
                &CurveInputs::new(Axis::Linear { min: 0.0, max: 1.0 }).with("family", 4.0),
            )
            .unwrap();
        let tail = &s[0].points;
        assert!(tail[1].1 < tail[10].1, "a bloom rises before it falls");
    }
}
