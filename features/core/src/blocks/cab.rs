//! A cabinet IR, by direct FIR convolution: exact and zero-latency.
//!
//! Moved from `sampler::convolver` (the rig's cab): mono in, both sides
//! out; the input history is kept twice, back to back, so the newest
//! `taps` samples are always one slice, and the IR is stored reversed to
//! line up with it, summed in 8 lanes so the loop vectorises. A silent
//! tail is trimmed at load ([`trim_tail`]); IRs longer than [`MAX_TAPS`]
//! are cut.

use alloc::vec;
use alloc::vec::Vec;

use crate::block::{BlockProcessor, Unavailable};
use crate::param::{ParamSpec, Unit};

/// The longest IR kept: 4096 taps, about 85 ms at 48 kHz.
pub const MAX_TAPS: usize = 4096;

const PARAMS: &[ParamSpec] = &[ParamSpec::new("level", "Level", Unit::Db, -24.0, 24.0, 0.0)];

pub struct Cab {
    /// Reversed: the last tap lines up with the newest sample.
    ir: Vec<f32>,
    /// The last `taps` inputs, twice.
    hist: Vec<f32>,
    write: usize,
    level: f32,
}

impl Cab {
    /// From a WAV file's bytes (channel 0).
    ///
    /// # Errors
    ///
    /// When the bytes aren't a WAV this reads, or hold no samples.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, Unavailable> {
        let ir = crate::wav::first_channel(bytes).map_err(|why| Unavailable::Failed {
            why: alloc::format!("cab IR: {why}"),
        })?;
        if ir.is_empty() {
            return Err(Unavailable::Failed {
                why: "cab IR has no samples".into(),
            });
        }
        Ok(Self::from_ir(ir))
    }

    /// From an IR in memory.
    #[must_use]
    pub fn from_ir(mut ir: Vec<f32>) -> Self {
        if ir.is_empty() {
            ir.push(1.0);
        }
        ir.truncate(MAX_TAPS);
        let taps = trim_tail(&ir);
        ir.truncate(taps);
        ir.reverse();
        let taps = ir.len();
        Self {
            hist: vec![0.0; taps.saturating_mul(2)],
            ir,
            write: 0,
            level: 1.0,
        }
    }

    #[must_use]
    pub fn taps(&self) -> usize {
        self.ir.len()
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let taps = self.ir.len();
        let w = self.write;
        if let Some(s) = self.hist.get_mut(w) {
            *s = x;
        }
        if let Some(s) = self.hist.get_mut(w.saturating_add(taps)) {
            *s = x;
        }
        let next = w.saturating_add(1);
        self.write = if next >= taps { 0 } else { next };
        self.hist
            .get(next..next.saturating_add(taps))
            .map_or(0.0, |window| dot(&self.ir, window))
    }
}

/// `a · b`, in 8 independent lanes so it vectorises.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut lanes = [0.0_f32; 8];
    let mut ac = a.chunks_exact(8);
    let mut bc = b.chunks_exact(8);
    for (x, y) in (&mut ac).zip(&mut bc) {
        for ((lane, xi), yi) in lanes.iter_mut().zip(x).zip(y) {
            *lane += xi * yi;
        }
    }
    let mut acc: f32 = lanes.iter().sum();
    for (x, y) in ac.remainder().iter().zip(bc.remainder()) {
        acc += x * y;
    }
    acc
}

/// How many taps of `ir` matter: where the energy left in the tail falls
/// below −70 dB of the whole, and at least 256 (or all it has).
#[must_use]
pub fn trim_tail(ir: &[f32]) -> usize {
    let total: f64 = ir.iter().map(|x| f64::from(*x) * f64::from(*x)).sum();
    if total <= 0.0 {
        return ir.len();
    }
    let floor = total * 1e-7;
    let mut tail = 0.0_f64;
    let mut keep = ir.len();
    for (i, x) in ir.iter().enumerate().rev() {
        tail += f64::from(*x) * f64::from(*x);
        if tail > floor {
            keep = i.saturating_add(1);
            break;
        }
    }
    keep.max(256.min(ir.len()))
}

impl BlockProcessor for Cab {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index == 0 {
            self.level = crate::util::db_to_gain(value);
        }
    }

    fn reset(&mut self) {
        self.hist.iter_mut().for_each(|s| *s = 0.0);
        self.write = 0;
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let y = self.tick(0.5 * (*l + *r)) * self.level;
            *l = y;
            *r = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_impulse_comes_out_as_the_ir() {
        let ir: Vec<f32> = (0..300u16).map(|i| 1.0 / f32::from(i + 1)).collect();
        let mut cab = Cab::from_ir(ir.clone());
        let mut l = vec![0.0_f32; 300];
        let mut r = vec![0.0_f32; 300];
        l[0] = 1.0;
        r[0] = 1.0;
        cab.process(&mut l, &mut r);
        for (got, want) in l.iter().zip(&ir) {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
    }
}
