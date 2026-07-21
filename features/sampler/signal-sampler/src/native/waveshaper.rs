//! Native **Waveshaper** — the `Native` implementation of
//! `BlockType::Waveshaper` (Omnisphere's in-oscillator Crusher / Shaper /
//! Reducer, Nord drive-ish shaping).
//!
//! Three stages, each bypassed at zero:
//! - **drive** — normalized tanh saturation (`tanh(g·x)/tanh(g)`, exact
//!   identity as drive → 0).
//! - **crush** — bit-depth reduction (quantization).
//! - **reduce** — sample-rate reduction (sample-and-hold).
//!
//! Plus a wet/dry **mix**. All four are runtime params (mod-matrix drivable)
//! and build-time block params.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

pub struct NativeWaveshaper {
    /// 0..1 → saturation amount.
    drive: f32,
    /// 0..1 → bit depth 16 → 2 bits.
    crush: f32,
    /// 0..1 → hold factor 1 → 64 samples.
    reduce: f32,
    /// Wet/dry, 1 = fully shaped.
    mix: f32,
    // Sample-and-hold state.
    hold_l: f32,
    hold_r: f32,
    hold_count: f32,
    prepared: bool,
}

impl NativeWaveshaper {
    pub fn new(_sample_rate: u32) -> Self {
        Self {
            drive: 0.0,
            crush: 0.0,
            reduce: 0.0,
            mix: 1.0,
            hold_l: 0.0,
            hold_r: 0.0,
            hold_count: 0.0,
            prepared: false,
        }
    }

    #[must_use]
    pub fn with_drive(mut self, v: f32) -> Self {
        self.drive = v.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_crush(mut self, v: f32) -> Self {
        self.crush = v.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_reduce(mut self, v: f32) -> Self {
        self.reduce = v.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_mix(mut self, v: f32) -> Self {
        self.mix = v.clamp(0.0, 1.0);
        self
    }

    #[inline]
    fn shape(&self, x: f32) -> f32 {
        let mut y = x;
        if self.drive > 0.0 {
            let g = self.drive * 8.0 + 1e-3;
            y = (g * y).tanh() / g.tanh().max(1e-3);
            // Normalized so a full-scale sine keeps roughly its level.
            y = y.clamp(-1.5, 1.5);
        }
        if self.crush > 0.0 {
            // 16 → 2 bits.
            let bits = 16.0 - self.crush * 14.0;
            let steps = 2f32.powf(bits - 1.0);
            y = (y * steps).round() / steps;
        }
        y
    }
}

impl PluginInstance for NativeWaveshaper {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.waveshaper".into(),
            name: "Waveshaper".into(),
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
            mk(0, "drive", self.drive as f64),
            mk(1, "crush", self.crush as f64),
            mk(2, "reduce", self.reduce as f64),
            mk(3, "mix", self.mix as f64),
        ]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            0 => Some(self.drive as f64),
            1 => Some(self.crush as f64),
            2 => Some(self.reduce as f64),
            3 => Some(self.mix as f64),
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

    fn prepare(&mut self, _sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.hold_l = 0.0;
        self.hold_r = 0.0;
        self.hold_count = 0.0;
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
                0 => self.drive = v,
                1 => self.crush = v,
                2 => self.reduce = v,
                3 => self.mix = v,
                _ => {}
            }
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        let hold_len = 1.0 + self.reduce * 63.0;
        for f in 0..frames {
            let (dry_l, dry_r) = (in_l[f], in_r[f]);
            let (mut wl, mut wr) = (dry_l, dry_r);
            if self.reduce > 0.0 {
                self.hold_count += 1.0;
                if self.hold_count >= hold_len {
                    self.hold_count = 0.0;
                    self.hold_l = wl;
                    self.hold_r = wr;
                }
                wl = self.hold_l;
                wr = self.hold_r;
            }
            wl = self.shape(wl);
            wr = self.shape(wr);
            if self.mix >= 1.0 {
                out_l[f] = wl;
                out_r[f] = wr;
            } else {
                out_l[f] = dry_l + (wl - dry_l) * self.mix;
                out_r[f] = dry_r + (wr - dry_r) * self.mix;
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

    fn sine(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (core::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin() * 0.9)
            .collect()
    }

    fn run(ws: &mut NativeWaveshaper, input: &[f32]) -> Vec<f32> {
        let n = input.len();
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        ws.prepare(48_000.0, n as u32).unwrap();
        ws.process_block(input, input, &mut l, &mut r, &ev).unwrap();
        l
    }

    #[test]
    fn defaults_are_transparent() {
        let input = sine(2_048);
        let mut ws = NativeWaveshaper::new(48_000);
        let out = run(&mut ws, &input);
        let diff: f32 = input.iter().zip(&out).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff / 2_048.0 < 1e-3, "zeroed stages pass audio unchanged");
    }

    #[test]
    fn drive_saturates() {
        let input = sine(2_048);
        let mut ws = NativeWaveshaper::new(48_000).with_drive(1.0);
        let out = run(&mut ws, &input);
        // Hard tanh drive flattens peaks → waveform differs and stays bounded.
        let diff: f32 = input
            .iter()
            .zip(&out)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / 2_048.0;
        assert!(diff > 0.05, "drive audibly shapes, diff={diff}");
        assert!(out.iter().all(|s| s.abs() <= 1.5));
    }

    #[test]
    fn crush_quantizes() {
        let input = sine(2_048);
        let mut ws = NativeWaveshaper::new(48_000).with_crush(1.0);
        let out = run(&mut ws, &input);
        // 2-bit output has very few distinct values.
        let mut vals: Vec<i32> = out.iter().map(|s| (s * 1000.0).round() as i32).collect();
        vals.sort_unstable();
        vals.dedup();
        assert!(
            vals.len() <= 8,
            "2-bit crush leaves few levels, got {}",
            vals.len()
        );
    }

    #[test]
    fn reduce_holds_samples() {
        let input = sine(2_048);
        let mut ws = NativeWaveshaper::new(48_000).with_reduce(1.0);
        let out = run(&mut ws, &input);
        // 64-sample hold → long runs of identical values.
        let repeats = out.windows(2).filter(|w| w[0] == w[1]).count();
        assert!(repeats > 1_500, "sample-and-hold repeats, got {repeats}");
    }
}
