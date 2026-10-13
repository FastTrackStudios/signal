//! The modulation stack — programmatic vibrato and swells.
//!
//! Rows sum. An oscillator row contributes a wave; a curve row placed
//! *after* an oscillator acts as that oscillator's envelope, so
//! `Sine → CurveUp` is vibrato that grows across the selection. A curve
//! row can envelope either amplitude or rate.
//!
//! Rate curves integrate phase continuously rather than switching
//! frequency between cycles, so an accelerating vibrato speeds up
//! smoothly instead of stepping.

use crate::shape::Shape;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Triangle,
}

impl Wave {
    pub const ALL: [Wave; 4] = [Wave::Sine, Wave::Square, Wave::Saw, Wave::Triangle];

    pub fn label(&self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Square => "Square",
            Wave::Saw => "Saw",
            Wave::Triangle => "Triangle",
        }
    }

    /// One cycle over `phase` in turns, range -1..1.
    pub fn sample(&self, phase: f64) -> f64 {
        let p = phase.rem_euclid(1.0);
        match self {
            Wave::Sine => (core::f64::consts::TAU * p).sin(),
            Wave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Saw => 2.0 * p - 1.0,
            Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        }
    }
}

/// What a curve row envelopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveTarget {
    Amplitude,
    Rate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    /// `rate` is cycles across the whole selection.
    Oscillator {
        wave: Wave,
        amplitude: f64,
        rate: f64,
    },
    /// Spans the selection once, 0→1 (`up`) or 1→0.
    Curve {
        shape: Shape,
        depth: f64,
        up: bool,
        target: CurveTarget,
    },
}

/// An ordered stack of rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    pub rows: Vec<Row>,
}

impl Stack {
    /// Vibrato that grows from nothing to full across the selection.
    pub fn growing_vibrato() -> Self {
        Self {
            rows: vec![
                Row::Oscillator {
                    wave: Wave::Sine,
                    amplitude: 0.35,
                    rate: 12.0,
                },
                Row::Curve {
                    shape: Shape::EaseIn,
                    depth: 1.0,
                    up: true,
                    target: CurveTarget::Amplitude,
                },
            ],
        }
    }

    /// Vibrato that starts full and recedes.
    pub fn receding_vibrato() -> Self {
        Self {
            rows: vec![
                Row::Oscillator {
                    wave: Wave::Sine,
                    amplitude: 0.35,
                    rate: 12.0,
                },
                Row::Curve {
                    shape: Shape::EaseOut,
                    depth: 1.0,
                    up: false,
                    target: CurveTarget::Amplitude,
                },
            ],
        }
    }

    /// Stack output at `x` (0..1 across the selection).
    pub fn value(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let mut total = 0.0;
        for (i, row) in self.rows.iter().enumerate() {
            let Row::Oscillator {
                wave,
                amplitude,
                rate,
            } = *row
            else {
                continue;
            };
            let amp = amplitude * envelope(&self.rows[i + 1..], x, CurveTarget::Amplitude);
            // Integrated phase: ∫rate(x)dx, so a rate envelope
            // accelerates the oscillator instead of jumping its
            // frequency.
            let phase = rate * integrated_rate(&self.rows[i + 1..], x);
            total += wave.sample(phase) * amp;
        }
        total
    }

    /// Sample the stack across `count` points.
    pub fn render(&self, count: usize) -> Vec<f64> {
        let n = count.max(2);
        (0..n)
            .map(|i| self.value(i as f64 / (n - 1) as f64))
            .collect()
    }
}

/// Product of every curve row following an oscillator that targets
/// `target`. Rows stop applying at the next oscillator.
fn envelope(following: &[Row], x: f64, target: CurveTarget) -> f64 {
    let mut factor = 1.0;
    for row in following {
        match *row {
            Row::Oscillator { .. } => break,
            Row::Curve {
                shape,
                depth,
                up,
                target: t,
            } if t == target => {
                let s = shape.amount(x);
                let s = if up { s } else { 1.0 - s };
                factor *= 1.0 - depth + depth * s;
            }
            _ => {}
        }
    }
    factor
}

/// ∫₀ˣ rate_factor dt, normalized so a flat factor of 1 gives back `x`.
fn integrated_rate(following: &[Row], x: f64) -> f64 {
    const STEPS: usize = 64;
    let has_rate_curve = following.iter().any(|r| {
        matches!(
            r,
            Row::Curve {
                target: CurveTarget::Rate,
                ..
            }
        )
    });
    if !has_rate_curve {
        return x;
    }
    let mut acc = 0.0;
    let dt = x / STEPS as f64;
    for i in 0..STEPS {
        let t = dt * (i as f64 + 0.5);
        acc += envelope(following, t, CurveTarget::Rate) * dt;
    }
    acc
}

/// Apply a stack to values over a span, tapering at the edges.
///
/// The taper is what keeps a modulation edit from clicking: dropping a
/// vibrato in cold at a nonzero phase would step the pitch. `taper` is
/// the fraction of the span eased in and out.
pub fn apply(values: &mut [f64], stack: &Stack, taper: f64) {
    let n = values.len();
    if n < 2 {
        return;
    }
    let taper = taper.clamp(0.0, 0.5);
    for (i, v) in values.iter_mut().enumerate() {
        let x = i as f64 / (n - 1) as f64;
        let edge = if taper <= 0.0 {
            1.0
        } else {
            let in_ = (x / taper).clamp(0.0, 1.0);
            let out = ((1.0 - x) / taper).clamp(0.0, 1.0);
            crate::shape::smoothstep(in_.min(out))
        };
        *v += stack.value(x) * edge;
    }
}
