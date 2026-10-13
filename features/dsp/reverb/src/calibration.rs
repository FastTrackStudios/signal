//! Measured decay → RT60, per engine and per size: what turns a time into
//! the setting that rings it.
//!
//! An engine's `decay` is its own law — a loop gain, or a time computed from
//! line lengths that size then stretches — so the same knob position meant
//! 0.5 s on one engine and 20 s on another, and a time worked out by formula
//! landed 40–80 % long in a small room and a spring asked for 2 s rang 1.2 s
//! in a small tank and 2.2 s in a large one. These tables are what each one
//! actually does, measured through the chain (impulse — a burst for Swell —
//! after the setting has settled, Schroeder decay, T20 × 3, modulation 0.2)
//! at five sizes. The chain maps the user's `decay` across the engine's
//! nominal span (`AlgorithmType::t60_range`, log-spaced) to a time, and
//! hands the engine the setting that measured to that time at the current
//! size, interpolated between the measured ones — so a time is the time
//! heard, whatever the size.
//!
//! Only the rising part of each curve is kept (a flat floor — the early
//! cluster dominating a short tail — and anything past an engine's stable
//! top are dropped), so each row is strictly increasing and invertible. A
//! time an engine cannot reach at a size is held to the nearest it can.
//!
//! The tables are generated (`calibration_tables.rs`): after changing an
//! engine's decay law, re-measure —
//! `cargo run --release -p reverb-dsp --example calibrate > src/calibration_tables.rs`
//! — and check with `cargo run --release -p reverb-dsp --example time_accuracy`.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::algorithm::AlgorithmType;

/// Hand every engine its raw `decay`, tables or not — only for the tool
/// that measures the tables (it has to see each engine's own law).
static RAW: AtomicBool = AtomicBool::new(false);

/// See [`RAW`]. Measurement only.
pub fn measure_raw(on: bool) {
    RAW.store(on, Ordering::Relaxed);
}

/// Whether the tables are bypassed (see [`measure_raw`]).
#[must_use]
pub fn raw() -> bool {
    RAW.load(Ordering::Relaxed)
}

/// One engine's measured curve: `decay` settings and the RT60 (s) each gave.
pub struct DecayTable {
    pub decay: &'static [f64],
    pub t60: &'static [f64],
}

impl DecayTable {
    /// The span of times this engine reaches.
    #[must_use]
    pub const fn range(&self) -> (f64, f64) {
        (self.t60[0], self.t60[self.t60.len() - 1])
    }

    /// The engine `decay` that measured to `t60_s` (interpolated in log
    /// time; clamped to the table's ends).
    #[must_use]
    pub fn engine_decay(&self, t60_s: f64) -> f64 {
        let n = self.t60.len();
        if t60_s <= self.t60[0] {
            return self.decay[0];
        }
        if t60_s >= self.t60[n - 1] {
            return self.decay[n - 1];
        }
        let lt = t60_s.ln();
        for i in 1..n {
            if t60_s <= self.t60[i] {
                let (a, b) = (self.t60[i - 1].ln(), self.t60[i].ln());
                let f = if b > a { (lt - a) / (b - a) } else { 0.0 };
                return self.decay[i - 1] + f * (self.decay[i] - self.decay[i - 1]);
            }
        }
        self.decay[n - 1]
    }
}

/// One engine's measured curves across sizes.
pub struct SizedTable {
    /// The sizes measured, rising.
    pub sizes: &'static [f64],
    /// The row a `decay` position's time is read from (size 0.7).
    pub nominal: usize,
    /// One curve per size.
    pub rows: &'static [DecayTable],
}

impl SizedTable {
    /// The span of times the engine reaches at the reference size.
    #[must_use]
    pub const fn range(&self) -> (f64, f64) {
        self.rows[self.nominal].range()
    }

    /// The times the engine reaches at `size`, s: its neighbouring
    /// sizes' spans, interpolated (in log time).
    #[must_use]
    pub fn reach(&self, size: f64) -> (f64, f64) {
        let n = self.sizes.len();
        let i = self
            .sizes
            .iter()
            .position(|&s| s >= size)
            .unwrap_or(n - 1)
            .max(1);
        let (s0, s1) = (self.sizes[i - 1], self.sizes[i]);
        let f = if s1 > s0 {
            ((size - s0) / (s1 - s0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (a, b) = (self.rows[i - 1].range(), self.rows[i].range());
        let lerp = |x: f64, y: f64| (x.ln() + f * (y.ln() - x.ln())).exp();
        (lerp(a.0, b.0), lerp(a.1, b.1))
    }

    /// The engine `decay` that rings `t60_s` at `size`: each neighbouring
    /// size's setting for that time, interpolated between them.
    #[must_use]
    pub fn engine_decay(&self, t60_s: f64, size: f64) -> f64 {
        let n = self.sizes.len();
        if size <= self.sizes[0] {
            return self.rows[0].engine_decay(t60_s);
        }
        if size >= self.sizes[n - 1] {
            return self.rows[n - 1].engine_decay(t60_s);
        }
        let i = self
            .sizes
            .iter()
            .position(|&s| s >= size)
            .unwrap_or(n - 1)
            .max(1);
        let (s0, s1) = (self.sizes[i - 1], self.sizes[i]);
        let f = if s1 > s0 {
            (size - s0) / (s1 - s0)
        } else {
            0.0
        };
        let (d0, d1) = (
            self.rows[i - 1].engine_decay(t60_s),
            self.rows[i].engine_decay(t60_s),
        );
        d0 + f * (d1 - d0)
    }
}

include!("calibration_tables.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_rises_and_inverts() {
        for &(a, v) in MEASURED {
            let t = table(a, v).expect("measured");
            assert_eq!(t.rows.len(), t.sizes.len(), "{a:?}/{v}");
            for (row, size) in t.rows.iter().zip(t.sizes) {
                assert!(row.t60.len() >= 2, "{a:?}/{v} at {size} has a curve");
                assert!(
                    row.t60.windows(2).all(|w| w[1] > w[0]),
                    "{a:?}/{v} at {size} rises"
                );
                assert!(
                    row.decay.windows(2).all(|w| w[1] > w[0]),
                    "{a:?}/{v} at {size} decay rises"
                );
                for (d, s) in row.decay.iter().zip(row.t60) {
                    assert!(
                        (row.engine_decay(*s) - d).abs() < 1e-9,
                        "{a:?}/{v} inverts at {s}"
                    );
                }
            }
        }
    }
}
