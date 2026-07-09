//! Cabinet impulse-response convolution — a native FX backend for the rig.
//!
//! A guitar cab is a (mostly) linear, time-invariant filter, captured as a short
//! impulse response (`.wav`). [`Convolver`] applies it by **direct time-domain
//! FIR convolution**: mono in (guitar amps are mono), one multiply-accumulate
//! per IR tap per sample, output broadcast to both channels (stereo width comes
//! from later stereo effects, not the cab).
//!
//! IRs are truncated to [`MAX_TAPS`] — cab IRs are routinely trimmed to 1–2k
//! taps with no audible loss, and direct convolution stays cheap there. A
//! partitioned-FFT convolver (for long reverb IRs) is a future upgrade; this is
//! deliberately dependency-free and allocation-free on the hot path.

use std::path::Path;

/// Maximum IR length kept after loading. 4096 taps ≈ 85 ms at 48 kHz — well
/// beyond any cabinet IR. Longer files are truncated.
pub const MAX_TAPS: usize = 4096;

/// A loaded cabinet IR + its delay-line state. Mono in / mono out (broadcast to
/// stereo by [`process_interleaved`](Self::process_interleaved)).
pub struct Convolver {
    /// The impulse response, `ir[0]` = newest-sample coefficient.
    ir: Vec<f32>,
    /// Circular delay line of the last `ir.len()` mono input samples.
    hist: Vec<f32>,
    /// Next write position in `hist`.
    write: usize,
    /// IR file path — for the UI label.
    pub ir_path: String,
    /// Display name (filename stem).
    pub display_name: String,
    /// Activation flag for the [`crate::nam::PluginInstance`] adapter. An IR is
    /// sample-rate-agnostic, so a loaded convolver is ready immediately;
    /// `deactivate` clears it.
    prepared: bool,
}

impl Convolver {
    /// Load a cabinet IR from a `.wav` file. Uses channel 0, converts to `f32`,
    /// and truncates to [`MAX_TAPS`].
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let mut reader =
            hound::WavReader::open(path).map_err(|e| format!("open IR {}: {e}", path.display()))?;
        let spec = reader.spec();
        let ch = spec.channels.max(1) as usize;

        let interleaved: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
            hound::SampleFormat::Int => {
                let scale = 1.0 / ((1i64 << (spec.bits_per_sample.max(1) - 1)) as f32);
                reader
                    .samples::<i32>()
                    .filter_map(Result::ok)
                    .map(|s| s as f32 * scale)
                    .collect()
            }
        };

        // Channel 0 only (guitar cabs are captured mono per mic).
        let mut ir: Vec<f32> = interleaved.iter().step_by(ch).copied().collect();
        ir.truncate(MAX_TAPS);
        if ir.is_empty() {
            return Err(format!("IR {} has no samples", path.display()));
        }

        let display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Cab IR")
            .to_string();
        let taps = ir.len();
        Ok(Self {
            ir,
            hist: vec![0.0; taps],
            write: 0,
            ir_path: path.to_string_lossy().to_string(),
            display_name,
            prepared: true,
        })
    }

    /// Build a convolver from an in-memory IR (testing / synthesized cabs).
    pub fn from_ir(ir: Vec<f32>, name: impl Into<String>) -> Self {
        let ir = if ir.is_empty() { vec![1.0] } else { ir };
        let taps = ir.len().min(MAX_TAPS);
        let ir: Vec<f32> = ir.into_iter().take(taps).collect();
        Self {
            hist: vec![0.0; ir.len()],
            ir,
            write: 0,
            ir_path: String::new(),
            display_name: name.into(),
            prepared: true,
        }
    }

    /// Clear the delay line so the next block starts from silence.
    pub fn reset(&mut self) {
        for s in &mut self.hist {
            *s = 0.0;
        }
        self.write = 0;
    }

    /// Number of IR taps.
    pub fn taps(&self) -> usize {
        self.ir.len()
    }

    /// Convolve one mono input sample.
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let taps = self.ir.len();
        self.hist[self.write] = x;
        let mut pos = self.write; // newest sample
        self.write = if self.write + 1 == taps {
            0
        } else {
            self.write + 1
        };

        let mut acc = 0.0f32;
        for k in 0..taps {
            acc += self.ir[k] * self.hist[pos];
            pos = if pos == 0 { taps - 1 } else { pos - 1 };
        }
        acc
    }

    /// Process one interleaved-stereo block in place: collapse to mono, convolve
    /// with the cab IR, broadcast back to both channels.
    pub fn process_interleaved(&mut self, inout: &mut [f32]) {
        let frames = inout.len() / 2;
        for i in 0..frames {
            let mono = 0.5 * (inout[2 * i] + inout[2 * i + 1]);
            let y = self.tick(mono);
            inout[2 * i] = y;
            inout[2 * i + 1] = y;
        }
    }
}

impl std::fmt::Debug for Convolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Convolver")
            .field("display_name", &self.display_name)
            .field("taps", &self.ir.len())
            .finish()
    }
}

// ── daw `PluginInstance` adapter ────────────────────────────────────────────
//
// Lets a cab `Convolver` be inserted into daw's per-track FX chain via
// `Standalone::insert_plugin_instance`, so the live guitar rig runs ON daw's
// audio engine (see `crate::rig`). Mono in / mono out: `process_block` sums
// L+R, convolves, and broadcasts to both output channels — the same conversion
// `process_interleaved` does, on planar buffers.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

impl PluginInstance for Convolver {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: format!("signal.cabir:{}", self.ir_path),
            name: self.display_name.clone(),
            vendor: "Signal (Cab IR)".to_string(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }

    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
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
        // An IR is sample-rate-agnostic (no resampling yet); just clear the
        // delay line so the first block starts from silence.
        self.reset();
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
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        for i in 0..frames {
            let mono = 0.5 * (in_l[i] + in_r[i]);
            let y = self.tick(mono);
            out_l[i] = y;
            out_r[i] = y;
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeding a unit impulse through the convolver reproduces the IR exactly.
    #[test]
    fn impulse_reproduces_ir() {
        let ir = vec![0.5, -0.25, 0.125, 0.0625];
        let mut c = Convolver::from_ir(ir.clone(), "test");
        // Impulse then silence, as interleaved stereo (mono on both channels).
        let mut out = Vec::new();
        for n in 0..ir.len() {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let mut frame = [x, x];
            c.process_interleaved(&mut frame);
            out.push(frame[0]);
        }
        for (k, &expected) in ir.iter().enumerate() {
            assert!(
                (out[k] - expected).abs() < 1e-6,
                "tap {k}: {} != {expected}",
                out[k]
            );
        }
    }

    #[test]
    fn unit_ir_is_passthrough() {
        let mut c = Convolver::from_ir(vec![1.0], "unity");
        let mut frame = [0.7, 0.7];
        c.process_interleaved(&mut frame);
        assert!((frame[0] - 0.7).abs() < 1e-6);
        assert!((frame[1] - 0.7).abs() < 1e-6);
    }

    /// The `PluginInstance::process_block` planar path reproduces the IR from a
    /// unit impulse, exactly like `process_interleaved` — the two paths agree.
    #[test]
    fn plugin_instance_process_block_matches_ir() {
        use signal_plugin_host::{PluginEvents, PluginInstance};
        let ir = vec![0.5, -0.25, 0.125, 0.0625];
        let mut c = Convolver::from_ir(ir.clone(), "test");
        let ev = PluginEvents::default();
        let mut out = Vec::new();
        for n in 0..ir.len() {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let (il, ir_in) = ([x], [x]);
            let (mut ol, mut or_) = ([0.0f32], [0.0f32]);
            c.process_block(&il, &ir_in, &mut ol, &mut or_, &ev)
                .unwrap();
            assert!(
                (ol[0] - or_[0]).abs() < 1e-9,
                "L and R must match (mono broadcast)"
            );
            out.push(ol[0]);
        }
        for (k, &expected) in ir.iter().enumerate() {
            assert!(
                (out[k] - expected).abs() < 1e-6,
                "tap {k}: {} != {expected}",
                out[k]
            );
        }
        assert!(c.is_prepared());
    }

    #[test]
    fn reset_clears_tail() {
        let mut c = Convolver::from_ir(vec![1.0, 1.0, 1.0], "ones");
        let mut f = [1.0, 1.0];
        c.process_interleaved(&mut f); // history now has the impulse
        c.reset();
        let mut f2 = [0.0, 0.0];
        c.process_interleaved(&mut f2);
        assert_eq!(f2[0], 0.0, "after reset, silence in → silence out");
    }
}
