//! Native **Amp** block — the voice-chain gain stage (`BlockType::Amp`,
//! `Native` impl). Unity by default; the `ModMatrix` will drive its gain from the
//! amp envelope once control-rate modulation lands (roadmap §2).

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

pub struct NativeAmp {
    gain: f32,
    /// Pan 0..1 (0.5 centre, balance law) and stereo width 0..1 (0.5 as
    /// recorded, 0 mono, 1 twice the side).
    pan: f32,
    width: f32,
    /// Tone, 0..1 each: `warmth` tilts the top (0.5 flat, up = darker, down
    /// = brighter, ±6 dB above ~2.5 kHz), `body` the bottom (0.5 flat, ±6 dB
    /// below ~220 Hz), `drive` saturates (0 = clean).
    warmth: f32,
    body: f32,
    drive: f32,
    sample_rate: f32,
    /// One-pole low-pass states for the two shelves, per channel.
    lo: [f32; 2],
    hi: [f32; 2],
    prepared: bool,
}

/// Shelf corners (Hz).
const BODY_HZ: f32 = 220.0;
const WARMTH_HZ: f32 = 2_500.0;

impl NativeAmp {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            gain: 1.0,
            pan: 0.5,
            width: 0.5,
            warmth: 0.5,
            body: 0.5,
            drive: 0.0,
            sample_rate: sample_rate.max(1) as f32,
            lo: [0.0; 2],
            hi: [0.0; 2],
            prepared: false,
        }
    }

    #[must_use]
    pub fn with_gain_db(mut self, db: f32) -> Self {
        self.gain = 10f32.powf(db / 20.0);
        self
    }

    /// Normalized 0..1 → amplitude 0..2 (unity at 0.5) — matches param 0.
    #[must_use]
    pub fn with_gain_norm(mut self, v: f32) -> Self {
        self.gain = v.clamp(0.0, 1.0) * 2.0;
        self
    }

    /// Width, pan and gain over `frames` of `l`/`r`, in place.
    fn finish_in_place(
        &self,
        l: &mut [f32],
        r: &mut [f32],
        frames: usize,
    ) -> Result<(), PluginError> {
        let side = self.width * 2.0;
        let p = self.pan * 2.0 - 1.0;
        let (gl, gr) = (
            self.gain * (1.0 - p.max(0.0)),
            self.gain * (1.0 + p.min(0.0)),
        );
        for f in 0..frames {
            let m = (l[f] + r[f]) * 0.5;
            let s = (l[f] - r[f]) * 0.5 * side;
            l[f] = (m + s) * gl;
            r[f] = (m - s) * gr;
        }
        Ok(())
    }
}

impl PluginInstance for NativeAmp {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.amp".into(),
            name: "Amp".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        let mk = |id, name: &str| PluginParamInfo {
            id,
            name: name.into(),
            min: 0.0,
            max: 1.0,
            default: 0.5,
        };
        // gain normalized: amplitude = 2v, unity at 0.5.
        let mut drive = mk(5, "drive");
        drive.default = 0.0;
        vec![
            mk(0, "gain"),
            mk(1, "pan"),
            mk(2, "width"),
            mk(3, "warmth"),
            mk(4, "body"),
            drive,
        ]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            0 => Some((self.gain / 2.0) as f64),
            1 => Some(self.pan as f64),
            2 => Some(self.width as f64),
            3 => Some(self.warmth as f64),
            4 => Some(self.body as f64),
            5 => Some(self.drive as f64),
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
        self.lo = [0.0; 2];
        self.hi = [0.0; 2];
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
                // Normalized 0..1 → amplitude 0..2 (unity at 0.5).
                0 => self.gain = v * 2.0,
                1 => self.pan = v,
                2 => self.width = v,
                3 => self.warmth = v,
                4 => self.body = v,
                5 => self.drive = v,
                _ => {}
            }
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        let toned =
            (self.warmth - 0.5).abs() > 1e-4 || (self.body - 0.5).abs() > 1e-4 || self.drive > 0.0;
        if toned {
            // Shelves as a one-pole split: body scales what is under its
            // corner, warmth what is over its corner; ±6 dB at the ends.
            let db = |v: f32| 10f32.powf(((v - 0.5) * 12.0) / 20.0);
            let (g_body, g_top) = (db(self.body), db(1.0 - self.warmth));
            let a = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz / self.sample_rate).exp();
            let (a_lo, a_hi) = (a(BODY_HZ), a(WARMTH_HZ));
            let g_drive = 1.0 + 7.0 * self.drive;
            let comp = 1.0 / g_drive.sqrt();
            for f in 0..frames {
                for (c, x) in [(0usize, in_l[f]), (1, in_r[f])] {
                    self.lo[c] += a_lo * (x - self.lo[c]);
                    self.hi[c] += a_hi * (x - self.hi[c]);
                    let low = self.lo[c];
                    let top = x - self.hi[c];
                    let mid = x - low - top;
                    let mut y = low * g_body + mid + top * g_top;
                    if self.drive > 0.0 {
                        y = (y * g_drive).tanh() * comp;
                    }
                    if c == 0 {
                        out_l[f] = y;
                    } else {
                        out_r[f] = y;
                    }
                }
            }
            // Pan / width / gain on the toned signal.
            return self.finish_in_place(out_l, out_r, frames);
        }
        let neutral = (self.pan - 0.5).abs() < 1e-4 && (self.width - 0.5).abs() < 1e-4;
        if neutral {
            for f in 0..frames {
                out_l[f] = in_l[f] * self.gain;
                out_r[f] = in_r[f] * self.gain;
            }
            return Ok(());
        }
        // Width on mid/side, then a balance pan (centre = unity).
        let side = self.width * 2.0;
        let p = self.pan * 2.0 - 1.0;
        let (gl, gr) = (
            self.gain * (1.0 - p.max(0.0)),
            self.gain * (1.0 + p.min(0.0)),
        );
        for f in 0..frames {
            let m = (in_l[f] + in_r[f]) * 0.5;
            let s = (in_l[f] - in_r[f]) * 0.5 * side;
            out_l[f] = (m + s) * gl;
            out_r[f] = (m - s) * gr;
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

    fn tone_rms(amp: &mut NativeAmp, hz: f32) -> f32 {
        let n = 9_600;
        let x: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin() * 0.3)
            .collect();
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        amp.process_block(&x, &x, &mut l, &mut r, &PluginEvents::default())
            .unwrap();
        (l[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt()
    }

    fn set(amp: &mut NativeAmp, id: u32, v: f64) {
        let p = [(id, v)];
        let ev = PluginEvents {
            params: &p,
            ..PluginEvents::default()
        };
        amp.process_block(&[], &[], &mut [], &mut [], &ev).unwrap();
    }

    #[test]
    fn body_lifts_the_bottom_warmth_darkens_the_top_width_zero_is_mono() {
        let mut flat = NativeAmp::new(48_000);
        flat.prepare(48_000.0, 64).unwrap();
        let (low0, high0) = (tone_rms(&mut flat, 80.0), tone_rms(&mut flat, 8_000.0));
        let mut toned = NativeAmp::new(48_000);
        toned.prepare(48_000.0, 64).unwrap();
        set(&mut toned, 4, 1.0); // body +6 dB
        set(&mut toned, 3, 1.0); // warmth: top −6 dB
        let (low1, high1) = (tone_rms(&mut toned, 80.0), tone_rms(&mut toned, 8_000.0));
        assert!(low1 > low0 * 1.6, "body {low0} -> {low1}");
        assert!(high1 < high0 * 0.65, "warmth {high0} -> {high1}");

        let mut mono = NativeAmp::new(48_000);
        mono.prepare(48_000.0, 4).unwrap();
        set(&mut mono, 2, 0.0);
        let (mut l, mut r) = (vec![0.0; 4], vec![0.0; 4]);
        mono.process_block(
            &[1.0; 4],
            &[0.0; 4],
            &mut l,
            &mut r,
            &PluginEvents::default(),
        )
        .unwrap();
        assert!((l[0] - r[0]).abs() < 1e-6, "width 0 is mono");
    }

    #[test]
    fn gain_scales_input() {
        let mut amp = NativeAmp::new(48_000).with_gain_db(-6.0);
        amp.prepare(48_000.0, 4).unwrap();
        let input = vec![1.0f32; 4];
        let (mut l, mut r) = (vec![0.0; 4], vec![0.0; 4]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        amp.process_block(&input, &input, &mut l, &mut r, &ev)
            .unwrap();
        assert!((l[0] - 0.501).abs() < 0.01, "-6 dB ≈ ×0.5, got {}", l[0]);
    }
}
