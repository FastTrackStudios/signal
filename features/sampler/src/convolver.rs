//! Cabinet impulse-response convolution — a native FX backend for the rig.
//!
//! A guitar cab is a (mostly) linear, time-invariant filter, captured as a short
//! impulse response (`.wav`). [`Convolver`] applies it by **direct time-domain
//! FIR convolution** — exact and zero-latency: mono in (guitar amps are mono),
//! output broadcast to both channels (stereo width comes from later stereo
//! effects, not the cab).
//!
//! Each output sample is one contiguous dot product: the input history is kept
//! twice, back to back, so the last `taps` samples are always one slice, and
//! the IR is stored reversed to line up with it. Summed in 8 independent lanes
//! the loop vectorises (NEON / AVX). The old per-tap wrap-around index kept it
//! scalar and made a 4096-tap cab the most expensive block in the rig (~20 %
//! of a 64-frame budget); an IR's silent tail is trimmed at load too
//! ([`trim_tail`]).
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
    /// The impulse response, reversed: `ir[taps - 1]` is the newest-sample
    /// coefficient, so it lines up with the history window oldest → newest.
    ir: Vec<f32>,
    /// The last `taps` mono inputs, stored twice (`2 * taps`): the window
    /// ending at the newest sample is always `hist[write + 1 ..= write + taps]`.
    hist: Vec<f32>,
    /// Where the next sample goes (in `0..taps`).
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
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, loaded, or contains no samples.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let loaded =
            fts_sample::load_planar_f32(path, None, fts_sample::ResampleQuality::default())
                .map_err(|e| format!("open IR {e}"))?;

        // Channel 0 only (guitar cabs are captured mono per mic).
        let ir: Vec<f32> = loaded.channels.into_iter().next().unwrap_or_default();
        if ir.is_empty() {
            return Err(format!("IR {} has no samples", path.display()));
        }

        let display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Cab IR")
            .to_string();
        let mut conv = Self::from_ir(ir, display_name);
        conv.ir_path = path.to_string_lossy().to_string();
        Ok(conv)
    }

    /// Load a cabinet IR from a file's bytes (any format the decoder takes;
    /// `name` is the key the rig knows it by) — the browser path.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes do not decode or hold no samples.
    pub fn from_bytes(bytes: &[u8], name: &str) -> Result<Self, String> {
        let ext = name.rsplit_once('.').map(|(_, e)| e);
        let loaded =
            fts_sample::decode_bytes(bytes, ext).map_err(|e| format!("decode IR {name}: {e}"))?;
        let ir: Vec<f32> = loaded.channels.into_iter().next().unwrap_or_default();
        if ir.is_empty() {
            return Err(format!("IR {name} has no samples"));
        }
        let mut conv = Self::from_ir(ir, crate::assets::stem(name));
        conv.ir_path = name.to_string();
        Ok(conv)
    }

    /// Build a convolver from an in-memory IR (testing / synthesized cabs).
    pub fn from_ir(ir: Vec<f32>, name: impl Into<String>) -> Self {
        let mut ir = if ir.is_empty() { vec![1.0] } else { ir };
        ir.truncate(MAX_TAPS);
        let taps = trim_tail(&ir);
        ir.truncate(taps);
        ir.reverse();
        Self {
            hist: vec![0.0; 2 * taps],
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
    #[must_use]
    pub fn taps(&self) -> usize {
        self.ir.len()
    }

    /// Convolve one mono input sample.
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let taps = self.ir.len();
        let w = self.write;
        self.hist[w] = x;
        self.hist[w + taps] = x;
        self.write = if w + 1 == taps { 0 } else { w + 1 };
        // The last `taps` inputs, oldest → newest.
        let window = &self.hist[w + 1..w + 1 + taps];
        dot(&self.ir, window)
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

/// `a · b`, summed in 8 independent lanes so it vectorises.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut lanes = [0.0f32; 8];
    let (ac, ar) = a.split_at(n - n % 8);
    let (bc, br) = b.split_at(n - n % 8);
    for (x, y) in ac.chunks_exact(8).zip(bc.chunks_exact(8)) {
        for i in 0..8 {
            lanes[i] = x[i].mul_add(y[i], lanes[i]);
        }
    }
    let mut acc = lanes.iter().sum::<f32>();
    for (x, y) in ar.iter().zip(br) {
        acc = x.mul_add(*y, acc);
    }
    acc
}

/// How many taps of `ir` matter: where the energy left in the tail falls
/// below −70 dB of the whole — a cab IR's padding and noise floor — and at
/// least 256 taps (and never more than it has). Inaudible, and the cost is
/// per tap.
#[must_use]
pub fn trim_tail(ir: &[f32]) -> usize {
    let total: f64 = ir.iter().map(|x| f64::from(*x) * f64::from(*x)).sum();
    if total <= 0.0 {
        return ir.len();
    }
    let floor = total * 1e-7;
    let mut tail = 0.0f64;
    let mut keep = ir.len();
    for (i, x) in ir.iter().enumerate().rev() {
        tail += f64::from(*x) * f64::from(*x);
        if tail > floor {
            keep = i + 1;
            break;
        }
    }
    keep.max(256.min(ir.len()))
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
