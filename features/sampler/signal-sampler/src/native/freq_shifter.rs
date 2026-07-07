//! **Dual Frequency Shifter** — the `Native` implementation of
//! `BlockType::Dfs` (Omnisphere 3's per-layer DFS: two frequency shifters,
//! serial or parallel).
//!
//! A true frequency shifter (not a pitch shifter): single-sideband
//! modulation via a Hilbert-transform pair — two parallel 4-section allpass
//! cascades ~90° apart (Niemitalo coefficients), then
//! `y = I·cos(ωt) ∓ Q·sin(ωt)` picks the up/down sideband. Each shifter has
//! its own shift amount and wet mix; B follows A in series or sums in
//! parallel.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

/// One 2nd-order allpass section of the Hilbert pair:
/// `H(z) = (a² + z⁻²) / (1 + a²·z⁻²)`.
#[derive(Clone, Copy, Debug, Default)]
struct Ap {
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Ap {
    fn new(a: f32) -> Self {
        Self {
            a2: a * a,
            ..Default::default()
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.a2 * (x + self.y2) - self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        (self.x1, self.x2, self.y1, self.y2) = (0.0, 0.0, 0.0, 0.0);
    }
}

/// The Hilbert pair: I (in-phase) and Q (quadrature, ~90° behind).
#[derive(Clone, Copy, Debug)]
struct Hilbert {
    i_path: [Ap; 4],
    q_path: [Ap; 4],
    /// One-sample delay on the I path (pairs with Q's phase response).
    i_delay: f32,
}

impl Hilbert {
    fn new() -> Self {
        // Olli Niemitalo's classic coefficient set.
        Self {
            i_path: [
                Ap::new(0.479_083_2),
                Ap::new(0.876_218_6),
                Ap::new(0.976_598_8),
                Ap::new(0.997_502_6),
            ],
            q_path: [
                Ap::new(0.161_758_4),
                Ap::new(0.733_029_9),
                Ap::new(0.945_349_7),
                Ap::new(0.990_599_4),
            ],
            i_delay: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let mut i = x;
        for ap in &mut self.i_path {
            i = ap.tick(i);
        }
        let out_i = self.i_delay;
        self.i_delay = i;
        let mut q = x;
        for ap in &mut self.q_path {
            q = ap.tick(q);
        }
        (out_i, q)
    }

    fn reset(&mut self) {
        for ap in self.i_path.iter_mut().chain(self.q_path.iter_mut()) {
            ap.reset();
        }
        self.i_delay = 0.0;
    }
}

/// One frequency shifter (per channel state).
#[derive(Clone, Copy, Debug)]
struct Shifter {
    hilbert: Hilbert,
    phase: f32,
}

impl Shifter {
    fn new() -> Self {
        Self {
            hilbert: Hilbert::new(),
            phase: 0.0,
        }
    }

    /// Shift by `hz` (negative = down), mixed at `mix`.
    #[inline]
    fn tick(&mut self, x: f32, hz: f32, mix: f32, sample_rate: f32) -> f32 {
        if mix <= 0.0 || hz.abs() < 0.01 {
            return x;
        }
        let (i, q) = self.hilbert.tick(x);
        let w = core::f32::consts::TAU * self.phase;
        self.phase = (self.phase + hz.abs() / sample_rate.max(1.0)).fract();
        let shifted = if hz >= 0.0 {
            i * w.cos() + q * w.sin()
        } else {
            i * w.cos() - q * w.sin()
        };
        x + (shifted - x) * mix
    }

    fn reset(&mut self) {
        self.hilbert.reset();
        self.phase = 0.0;
    }
}

/// The `Dfs` block: two shifters, serial (A→B) or parallel (A+B)/2.
pub struct NativeDfs {
    sample_rate: f32,
    /// Shift amounts in Hz (±).
    pub shift_a_hz: f32,
    pub shift_b_hz: f32,
    pub mix_a: f32,
    pub mix_b: f32,
    pub parallel: bool,
    a_l: Shifter,
    a_r: Shifter,
    b_l: Shifter,
    b_r: Shifter,
    prepared: bool,
}

impl NativeDfs {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            shift_a_hz: 0.0,
            shift_b_hz: 0.0,
            mix_a: 0.0,
            mix_b: 0.0,
            parallel: false,
            a_l: Shifter::new(),
            a_r: Shifter::new(),
            b_l: Shifter::new(),
            b_r: Shifter::new(),
            prepared: false,
        }
    }

    #[must_use]
    pub fn with_shifter_a(mut self, hz: f32, mix: f32) -> Self {
        self.shift_a_hz = hz;
        self.mix_a = mix.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_shifter_b(mut self, hz: f32, mix: f32) -> Self {
        self.shift_b_hz = hz;
        self.mix_b = mix.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
}

impl PluginInstance for NativeDfs {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.dfs".into(),
            name: "Dual Freq Shifter".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        let mk = |id, name: &str, default: f64| PluginParamInfo {
            id,
            name: name.into(),
            min: 0.0,
            max: 1.0,
            default,
        };
        vec![
            // 0.5 center → ±2 kHz.
            mk(0, "shift_a", (self.shift_a_hz / 4000.0 + 0.5) as f64),
            mk(1, "mix_a", self.mix_a as f64),
            mk(2, "shift_b", (self.shift_b_hz / 4000.0 + 0.5) as f64),
            mk(3, "mix_b", self.mix_b as f64),
        ]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            0 => Some((self.shift_a_hz / 4000.0 + 0.5) as f64),
            1 => Some(self.mix_a as f64),
            2 => Some((self.shift_b_hz / 4000.0 + 0.5) as f64),
            3 => Some(self.mix_b as f64),
            _ => None,
        }
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0) as f32;
        for s in [&mut self.a_l, &mut self.a_r, &mut self.b_l, &mut self.b_r] {
            s.reset();
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            let v = (value as f32).clamp(0.0, 1.0);
            match id {
                0 => self.shift_a_hz = (v - 0.5) * 4000.0,
                1 => self.mix_a = v,
                2 => self.shift_b_hz = (v - 0.5) * 4000.0,
                3 => self.mix_b = v,
                _ => {}
            }
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        let sr = self.sample_rate;
        for f in 0..frames {
            let (xl, xr) = (in_l[f], in_r[f]);
            let al = self.a_l.tick(xl, self.shift_a_hz, self.mix_a, sr);
            let ar = self.a_r.tick(xr, self.shift_a_hz, self.mix_a, sr);
            if self.parallel {
                let bl = self.b_l.tick(xl, self.shift_b_hz, self.mix_b, sr);
                let br = self.b_r.tick(xr, self.shift_b_hz, self.mix_b, sr);
                out_l[f] = (al + bl) * 0.5;
                out_r[f] = (ar + br) * 0.5;
            } else {
                out_l[f] = self.b_l.tick(al, self.shift_b_hz, self.mix_b, sr);
                out_r[f] = self.b_r.tick(ar, self.shift_b_hz, self.mix_b, sr);
            }
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossings(b: &[f32]) -> usize {
        b.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    fn run(dfs: &mut NativeDfs, freq: f32, n: usize) -> Vec<f32> {
        let input: Vec<f32> = (0..n)
            .map(|i| (core::f32::consts::TAU * freq * i as f32 / 48_000.0).sin() * 0.8)
            .collect();
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        dfs.prepare(48_000.0, n as u32).unwrap();
        dfs.process_block(&input, &input, &mut l, &mut r, &ev)
            .unwrap();
        l
    }

    #[test]
    fn zero_mix_is_transparent() {
        let mut dfs = NativeDfs::new(48_000);
        let out = run(&mut dfs, 440.0, 4_096);
        let expect: Vec<f32> = (0..4_096)
            .map(|i| (core::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin() * 0.8)
            .collect();
        let diff: f32 = out
            .iter()
            .zip(&expect)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / 4_096.0;
        assert!(diff < 1e-6, "bypassed shifter is exact, diff={diff}");
    }

    #[test]
    fn upshift_moves_the_frequency() {
        // 440 Hz + 220 Hz shift → 660 Hz: zero-crossing rate rises ×1.5.
        let n = 48_000;
        let mut dfs = NativeDfs::new(48_000).with_shifter_a(220.0, 1.0);
        let out = run(&mut dfs, 440.0, n);
        let base = 440.0;
        let measured = zero_crossings(&out[n / 4..]) as f32 / (0.75 * n as f32 / 48_000.0);
        assert!(
            (measured - (base + 220.0)).abs() < 25.0,
            "shifted tone ≈ 660 Hz, measured {measured}"
        );
    }

    #[test]
    fn downshift_moves_down_and_serial_stacks() {
        let n = 48_000;
        let mut dfs = NativeDfs::new(48_000)
            .with_shifter_a(-100.0, 1.0)
            .with_shifter_b(-100.0, 1.0);
        let out = run(&mut dfs, 440.0, n);
        // Serial A→B: 440 − 100 − 100 = 240 Hz.
        let measured = zero_crossings(&out[n / 4..]) as f32 / (0.75 * n as f32 / 48_000.0);
        assert!(
            (measured - 240.0).abs() < 25.0,
            "serial downshift ≈ 240 Hz, measured {measured}"
        );
    }
}
