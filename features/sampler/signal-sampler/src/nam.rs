//! Built-in Neural Amp Modeler backend for the FX chain.
//!
//! Wraps a `neural_amp_modeler::NamModel` for use as one variant of
//! [`crate::mixer::FxBackend`]. NAM models are **mono in / mono out** and
//! operate on `f64` buffers; the FX chain processes interleaved-stereo
//! `f32`. [`NamProcessor::process_interleaved`] handles the conversion:
//! input is collapsed to mono by summing L+R (with `input_gain_lin`
//! applied), the model is run, and the mono output is broadcast back to
//! both channels with `output_gain_lin`. Guitar amps are mono — stereo
//! width comes from cabinet IRs downstream.
//!
//! Scratch buffers (`in_mono`, `out_mono`) are pre-sized at
//! `reset(sample_rate, max_block)` time so the hot path doesn't allocate.

use neural_amp_modeler::NamModel;

/// Decibels → linear gain factor.
fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A loaded NAM model with per-block scratch + user-facing input/output
/// trim. Lives inside an [`FxSlot`](crate::mixer::FxSlot) under the
/// [`FxBackend::Nam`](crate::mixer::FxBackend) variant.
pub struct NamProcessor {
    model: NamModel,
    /// Mono input scratch (L+R summed, gained). Reused per block.
    in_mono: Vec<f64>,
    /// Mono output scratch the model writes into. Reused per block.
    out_mono: Vec<f64>,
    /// Path the model was loaded from — for the UI label.
    pub model_path: String,
    /// Display name (filename stem) — cached so the UI doesn't touch the
    /// audio thread to label the slot.
    pub display_name: String,
    /// User trim before NAM. Default 0 dB.
    pub input_gain_db: f32,
    /// User trim after NAM. Default 0 dB.
    pub output_gain_db: f32,
    /// Level calibration before the model (dB): the interface's full-scale
    /// dBu minus the level the capture declares it was fed at, so the model
    /// sees the analog level it was trained on. Separate from the trims —
    /// the drive knob rewrites those live and must not undo this.
    pub calibration_in_db: f32,
    /// Level calibration after the model (dB): the capture's declared output
    /// level minus the interface's, so every capture lands in one level
    /// domain. See [`calibration_for`].
    pub calibration_out_db: f32,
    /// Sample rate the model was prepared at.
    pub sample_rate: f64,
    /// Activation flag for the [`PluginInstance`] adapter. `load` leaves the
    /// model reset-and-ready, so it starts `true`; `deactivate` clears it.
    prepared: bool,
}

impl NamProcessor {
    /// Load a `.nam` model from disk and prepare it for `sample_rate` /
    /// `max_block` frames. The display name is derived from the
    /// filename stem.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(
        path: impl AsRef<std::path::Path>,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        let path = path.as_ref();
        let mut model = NamModel::load(path)?;
        model.reset(sample_rate, max_block);
        let display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("NAM")
            .to_string();
        Ok(Self {
            model,
            in_mono: vec![0.0; max_block],
            out_mono: vec![0.0; max_block],
            model_path: path.to_string_lossy().to_string(),
            display_name,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            calibration_in_db: 0.0,
            calibration_out_db: 0.0,
            sample_rate,
            prepared: true,
        })
    }

    /// Build from a `.nam` file's bytes — the browser path, where models
    /// arrive over HTTP rather than from a disk. `name` is the display name
    /// (and the key the rig knows the model by).
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not a model this engine can run.
    pub fn from_bytes(
        bytes: &[u8],
        name: impl Into<String>,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        let mut model = NamModel::from_bytes(bytes)?;
        model.reset(sample_rate, max_block);
        let name = name.into();
        Ok(Self {
            model,
            in_mono: vec![0.0; max_block],
            out_mono: vec![0.0; max_block],
            display_name: crate::assets::stem(&name),
            model_path: name,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            calibration_in_db: 0.0,
            calibration_out_db: 0.0,
            sample_rate,
            prepared: true,
        })
    }

    /// Run a smaller version of a slimmable (A2) model — `val` in 0..=1,
    /// cheaper toward 0. Returns false for a model that is not slimmable.
    /// The browser uses it when more models are playing than one thread can
    /// run at full size.
    pub fn set_slimmable_size(&mut self, val: f64) -> bool {
        self.model.set_slimmable_size(val)
    }

    /// Sample rate the model was trained at, if the `.nam` file declares it
    /// (modern format, v0.5+). Guitar models are usually 48 kHz; running the
    /// model at a different host rate shifts its voicing/pitch, so the rig
    /// uses this to warn (and, later, to resample).
    // r[impl sampler.nam.expected-rate]
    #[must_use]
    pub fn expected_sample_rate(&self) -> Option<f64> {
        self.model.expected_sample_rate()
    }

    /// Model loudness in dB (modern format), if present. Used to level-match
    /// models across a swap so "Clean" and "Lead" don't jump in volume.
    ///
    /// This is the model's *declared* loudness metadata — often missing or
    /// inconsistent between models. Prefer [`measured_loudness`](Self::measured_loudness)
    /// for the level-match guarantee; this is the fallback.
    #[must_use]
    pub fn loudness(&self) -> Option<f64> {
        self.model.loudness()
    }

    /// The analog input level (dBu) the model was captured at — the level that a
    /// 0 dBFS signal represents at the modeled gear's input (`.nam` v0.10+).
    /// Feeds input-staging calibration so the model gets the drive it expects.
    #[must_use]
    pub fn input_level(&self) -> Option<f64> {
        self.model.input_level()
    }

    /// The analog output level (dBu) at which the model's 0 dBFS was recorded
    /// (`.nam` v0.10+). Informational here — uniform output volume is handled by
    /// LUFS makeup, not by restoring the captured analog level.
    #[must_use]
    pub fn output_level(&self) -> Option<f64> {
        self.model.output_level()
    }

    /// Measure this model's integrated loudness (LUFS) by running the shared DI
    /// reference through it, cache-first. Off the hot path. Re-resets the model
    /// to `(self.sample_rate, max_block)` afterward so it stays live-ready.
    /// Returns `None` if the model produced silence (no reliable measurement).
    pub fn measured_loudness(&mut self, max_block: usize) -> Option<f64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = std::path::PathBuf::from(&self.model_path);
            crate::nam_calibrate::measured_loudness(
                &mut self.model,
                &path,
                self.sample_rate,
                max_block,
            )
        }
        // No DI render cache in the browser: the declared loudness stands in
        // (levels there come from the preset snapshots' own calibration).
        #[cfg(target_arch = "wasm32")]
        {
            let _ = max_block;
            self.loudness()
        }
    }

    /// Re-prepare the model at a new sample rate / block size. Resets
    /// internal state so the next block starts clean (recommended after
    /// loading or when the audio config changes).
    pub fn reset(&mut self, sample_rate: f64, max_block: usize) {
        self.sample_rate = sample_rate;
        self.in_mono.resize(max_block, 0.0);
        self.out_mono.resize(max_block, 0.0);
        self.model.reset(sample_rate, max_block);
    }

    /// Process one interleaved-stereo `[L, R, L, R, …]` `f32` buffer
    /// in place: collapse to mono → run NAM → broadcast mono back to
    /// both channels. `input_gain` and `output_gain` apply pre/post the
    /// model so the user can match the NAM model's expected input level
    /// without recalibrating the entire chain.
    // r[impl sampler.nam.mono-fold]
    // r[impl sampler.nam.no-hot-alloc]
    pub fn process_interleaved(&mut self, inout: &mut [f32]) {
        let frames = inout.len() / 2;
        if frames == 0 {
            return;
        }
        if frames > self.in_mono.len() {
            self.in_mono.resize(frames, 0.0);
            self.out_mono.resize(frames, 0.0);
        }
        let gin = db_to_lin(self.input_gain_db + self.calibration_in_db) as f64;
        let gout = db_to_lin(self.output_gain_db + self.calibration_out_db) as f64;
        // De-interleave + sum to mono (× input gain). Halve the sum so
        // a centered signal lands at unity instead of doubling.
        for i in 0..frames {
            let l = inout[2 * i] as f64;
            let r = inout[2 * i + 1] as f64;
            self.in_mono[i] = ((l + r) * 0.5) * gin;
        }
        self.model
            .process(&self.in_mono[..frames], &mut self.out_mono[..frames]);
        // Broadcast mono back to both channels with output gain.
        for i in 0..frames {
            let y = (self.out_mono[i] * gout) as f32;
            inout[2 * i] = y;
            inout[2 * i + 1] = y;
        }
    }
}

impl std::fmt::Debug for NamProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamProcessor")
            .field("display_name", &self.display_name)
            .field("model_path", &self.model_path)
            .field("sample_rate", &self.sample_rate)
            .field("input_gain_db", &self.input_gain_db)
            .field("output_gain_db", &self.output_gain_db)
            .finish()
    }
}

// ── daw `PluginInstance` adapter ────────────────────────────────────────────
//
// Lets a `NamProcessor` be inserted into daw's per-track FX chain via
// `Standalone::insert_plugin_instance`, so the live guitar rig runs ON daw's
// audio engine instead of signal's own cpal streams (see `crate::rig`). NAM is
// mono in / mono out; `process_block` sums L+R to the mono path the model wants
// and broadcasts the result back to both output channels — the same conversion
// `process_interleaved` does, just on planar buffers.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

impl PluginInstance for NamProcessor {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: format!("signal.nam:{}", self.model_path),
            name: self.display_name.clone(),
            vendor: "Signal (NAM)".to_string(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        // Input/output trims are driven directly via `input_gain_db` /
        // `output_gain_db` (the rig sets patch-level trims), not as host params.
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

    // r[impl sampler.nam.prepared-flag]
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.reset(sample_rate, block_size as usize);
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    // r[impl sampler.nam.planar-interleaved-parity]
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if frames == 0 {
            return Ok(());
        }
        if frames > self.in_mono.len() {
            self.in_mono.resize(frames, 0.0);
            self.out_mono.resize(frames, 0.0);
        }
        let gin = db_to_lin(self.input_gain_db + self.calibration_in_db) as f64;
        let gout = db_to_lin(self.output_gain_db + self.calibration_out_db) as f64;
        // Sum L+R to mono (× input gain). Halve so a centered signal lands at
        // unity instead of doubling — matches `process_interleaved`.
        for i in 0..frames {
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            self.in_mono[i] = ((l + r) * 0.5) * gin;
        }
        self.model
            .process(&self.in_mono[..frames], &mut self.out_mono[..frames]);
        for i in 0..frames {
            let y = (self.out_mono[i] * gout) as f32;
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
    use signal_plugin_host::{PluginEvents, PluginInstance};

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets")
            .join(name)
    }

    /// A known input through the `PluginInstance::process_block` planar path is
    /// non-silent and equals the interleaved path frame-for-frame — the two
    /// process routes agree (so the rig hears the same tone on daw's engine).
    // r[verify sampler.nam.mono-fold]
    // r[verify sampler.nam.planar-interleaved-parity]
    #[test]
    fn process_block_matches_process_interleaved() {
        let sr = 48_000.0;
        let Ok(mut a) = NamProcessor::load(fixture("amp_a.nam"), sr, 256) else {
            eprintln!("skip: amp_a.nam fixture failed to load");
            return;
        };
        let mut b = NamProcessor::load(fixture("amp_a.nam"), sr, 256).expect("load b");
        assert!(a.is_prepared() && b.is_prepared());

        const N: usize = 256;
        let sig: Vec<f32> = (0..N).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();

        // Planar path.
        let (mut ol, mut or_) = (vec![0.0f32; N], vec![0.0f32; N]);
        a.process_block(&sig, &sig, &mut ol, &mut or_, &PluginEvents::default())
            .unwrap();

        // Interleaved path (broadcast the same mono into both channels).
        let mut inter: Vec<f32> = sig.iter().flat_map(|&s| [s, s]).collect();
        b.process_interleaved(&mut inter);

        let energy: f64 = ol.iter().map(|x| (*x as f64).powi(2)).sum();
        assert!(energy > 1e-9, "NAM output should be non-silent");
        for i in 0..N {
            assert!(
                (ol[i] - or_[i]).abs() < 1e-9,
                "L/R must match (mono broadcast)"
            );
            assert!(
                (ol[i] - inter[2 * i]).abs() < 1e-5,
                "planar vs interleaved diverge at {i}: {} != {}",
                ol[i],
                inter[2 * i]
            );
        }
    }

    /// `deactivate` clears the prepared flag; `prepare` restores it.
    // r[verify sampler.nam.prepared-flag]
    #[test]
    fn prepare_deactivate_toggles_prepared() {
        let Ok(mut a) = NamProcessor::load(fixture("amp_a.nam"), 48_000.0, 64) else {
            eprintln!("skip: amp_a.nam fixture failed to load");
            return;
        };
        assert!(a.is_prepared());
        a.deactivate();
        assert!(!a.is_prepared());
        a.prepare(48_000.0, 128).unwrap();
        assert!(a.is_prepared());
    }
}

/// The interface's input calibration, dBu at 0 dBFS, as f32 bits; NaN = off.
static INTERFACE_CAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x7fc0_0000);

/// Turn NAM level calibration on at `dbu` (the interface's full-scale input
/// level — the MiniFuse instrument input is +11.5 dBu at minimum gain), or
/// off with `None`. Chains built afterwards are calibrated.
pub fn set_interface_calibration_dbu(dbu: Option<f32>) {
    let bits = dbu
        .filter(|d| d.is_finite())
        .map_or(0x7fc0_0000, f32::to_bits);
    INTERFACE_CAL.store(bits, std::sync::atomic::Ordering::Relaxed);
}

/// The interface calibration, if NAM calibration is on.
#[must_use]
pub fn interface_calibration_dbu() -> Option<f32> {
    let v = f32::from_bits(INTERFACE_CAL.load(std::sync::atomic::Ordering::Relaxed));
    v.is_finite().then_some(v)
}

/// The size every slimmable (A2) model is built at, 0..=1 (1 = full).
/// Stored as `f32` bits; full size by default.
static MODEL_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Build A2 models at `size` from now on (clamped to 0.1..=1). A host that
/// cannot afford full-size models — the browser, when a board is too long
/// for its threads — trades a little accuracy for a lot of speed: a quarter
/// size runs about twice as fast.
pub fn set_model_size(size: f64) {
    let size = if size.is_finite() {
        size.clamp(0.1, 1.0)
    } else {
        1.0
    };
    MODEL_SIZE.store(
        (size as f32).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[must_use]
pub fn model_size() -> f64 {
    f64::from(f32::from_bits(
        MODEL_SIZE.load(std::sync::atomic::Ordering::Relaxed),
    ))
}

/// `(input dB, output dB)` calibration for a capture declaring
/// `input_level`/`output_level` (dBu at 0 dBFS), against an interface whose
/// full scale is `interface` dBu. Keeps the signal in the interface's level
/// domain across every model: a model is fed what it was trained on, and
/// hands back what it would have put out. An undeclared level is 0 dB.
#[must_use]
pub fn calibration_for(
    input_level: Option<f64>,
    output_level: Option<f64>,
    interface: f32,
) -> (f32, f32) {
    let i = f64::from(interface);
    (
        input_level.map_or(0.0, |l| (i - l) as f32),
        output_level.map_or(0.0, |l| (l - i) as f32),
    )
}

#[cfg(test)]
mod calibration_tests {
    use super::calibration_for;

    /// A capture trained at +12 dBu in and +18 dBu out, on a +11.5 dBu
    /// interface: fed 0.5 dB less, handed back 6.5 dB more.
    #[test]
    fn calibration_is_the_distance_between_level_domains() {
        let (i, o) = calibration_for(Some(12.0), Some(18.0), 11.5);
        assert!((i + 0.5).abs() < 1e-6 && (o - 6.5).abs() < 1e-6);
        assert_eq!(calibration_for(None, None, 11.5), (0.0, 0.0));
    }
}
