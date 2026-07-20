//! FTS EQ — CLAP/VST3 parametric equalizer plugin.
//!
//! A thin nice-plug shell over the [`eq`] engine's realtime chain
//! ([`eq::EqChain`]: a serial cascade of [`eq::Band`]s built by the Pro-Q-style
//! ZPK design pipeline). The band layout is fixed and focused:
//!
//! 1. Low cut  (Highpass, 12 dB/oct, switchable)
//! 2. Low shelf   (freq / gain / Q)
//! 3. Peak 1      (freq / gain / Q)
//! 4. Peak 2      (freq / gain / Q)
//! 5. High shelf  (freq / gain / Q)
//! 6. High cut (Lowpass, 12 dB/oct, switchable)
//!
//! Shelf and peak bands bypass automatically at 0 dB; the cut filters have
//! explicit on/off switches. Params are pushed into the chain at the top of
//! every block (dirty-checked so coefficients only recompute on change),
//! matching `level-plugin`'s `sync_params` idiom.
//!
//! GUI is deliberately absent for now (headless, host-generic params), matching
//! `signal-sampler-clap`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use eq::{EqChain, FilterType};

const PLUGIN_NAME: &str = "FTS EQ";

/// Fixed band slots in the chain, in series order.
const BAND_LOWCUT: usize = 0;
const BAND_LOWSHELF: usize = 1;
const BAND_PEAK1: usize = 2;
const BAND_PEAK2: usize = 3;
const BAND_HIGHSHELF: usize = 4;
const BAND_HIGHCUT: usize = 5;
const NUM_BANDS: usize = 6;

/// Shelf/peak gain below this magnitude bypasses the band entirely.
const GAIN_EPSILON_DB: f32 = 0.05;

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct EqParams {
    /// Low-cut (highpass) enable.
    #[id = "lc_on"]
    pub lowcut_on: FloatParam,
    /// Low-cut corner frequency.
    #[id = "lc_freq"]
    pub lowcut_freq: FloatParam,

    /// Low-shelf corner frequency.
    #[id = "ls_freq"]
    pub lowshelf_freq: FloatParam,
    /// Low-shelf gain (0 dB = bypassed).
    #[id = "ls_gain"]
    pub lowshelf_gain: FloatParam,
    /// Low-shelf Q.
    #[id = "ls_q"]
    pub lowshelf_q: FloatParam,

    /// Peak 1 center frequency.
    #[id = "p1_freq"]
    pub peak1_freq: FloatParam,
    /// Peak 1 gain (0 dB = bypassed).
    #[id = "p1_gain"]
    pub peak1_gain: FloatParam,
    /// Peak 1 Q.
    #[id = "p1_q"]
    pub peak1_q: FloatParam,

    /// Peak 2 center frequency.
    #[id = "p2_freq"]
    pub peak2_freq: FloatParam,
    /// Peak 2 gain (0 dB = bypassed).
    #[id = "p2_gain"]
    pub peak2_gain: FloatParam,
    /// Peak 2 Q.
    #[id = "p2_q"]
    pub peak2_q: FloatParam,

    /// High-shelf corner frequency.
    #[id = "hs_freq"]
    pub highshelf_freq: FloatParam,
    /// High-shelf gain (0 dB = bypassed).
    #[id = "hs_gain"]
    pub highshelf_gain: FloatParam,
    /// High-shelf Q.
    #[id = "hs_q"]
    pub highshelf_q: FloatParam,

    /// High-cut (lowpass) enable.
    #[id = "hc_on"]
    pub highcut_on: FloatParam,
    /// High-cut corner frequency.
    #[id = "hc_freq"]
    pub highcut_freq: FloatParam,

    /// Output trim applied after the chain.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

/// On/off switch as a 0/1 FloatParam (the idiom the eq-ui param tree already
/// uses with this nice-plug stack).
fn switch_param(name: &str, default_on: bool) -> FloatParam {
    FloatParam::new(
        name,
        if default_on { 1.0 } else { 0.0 },
        FloatRange::Linear { min: 0.0, max: 1.0 },
    )
    .with_value_to_string(Arc::new(|v| {
        if v > 0.5 { "On".to_string() } else { "Off".to_string() }
    }))
    .with_string_to_value(Arc::new(|s| match s.trim().to_lowercase().as_str() {
        "on" | "1" | "true" => Some(1.0),
        "off" | "0" | "false" => Some(0.0),
        _ => s.parse().ok(),
    }))
}

/// Log-skewed frequency param (same skew as the parked eq-plugin shell).
fn freq_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_unit(" Hz")
    .with_value_to_string(Arc::new(|v| {
        if v >= 1000.0 {
            format!("{:.1}k", v / 1000.0)
        } else {
            format!("{v:.0}")
        }
    }))
}

/// Band gain in dB, 0 = neutral/bypass.
fn gain_param(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear { min: -30.0, max: 30.0 },
    )
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

/// Q with the Pro-Q display range (1.0 shown = Butterworth internally).
fn q_param(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        1.0,
        FloatRange::Skewed {
            min: 0.1,
            max: 18.0,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            lowcut_on: switch_param("Low Cut", false),
            lowcut_freq: freq_param("Low Cut Freq", 80.0, 20.0, 1_000.0),

            lowshelf_freq: freq_param("Low Shelf Freq", 120.0, 20.0, 2_000.0),
            lowshelf_gain: gain_param("Low Shelf Gain"),
            lowshelf_q: q_param("Low Shelf Q"),

            peak1_freq: freq_param("Peak 1 Freq", 400.0, 20.0, 20_000.0),
            peak1_gain: gain_param("Peak 1 Gain"),
            peak1_q: q_param("Peak 1 Q"),

            peak2_freq: freq_param("Peak 2 Freq", 2_500.0, 20.0, 20_000.0),
            peak2_gain: gain_param("Peak 2 Gain"),
            peak2_q: q_param("Peak 2 Q"),

            highshelf_freq: freq_param("High Shelf Freq", 8_000.0, 1_000.0, 20_000.0),
            highshelf_gain: gain_param("High Shelf Gain"),
            highshelf_q: q_param("High Shelf Q"),

            highcut_on: switch_param("High Cut", false),
            highcut_freq: freq_param("High Cut Freq", 18_000.0, 1_000.0, 20_000.0),

            output_gain: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsEq {
    params: Arc<EqParams>,
    /// One stereo chain: 6 fixed bands, each band ticks both channels.
    chain: EqChain,
    /// f64 scratch (the chain processes f64); pre-sized in `initialize`.
    left_buf: Vec<f64>,
    right_buf: Vec<f64>,
    sample_rate: f64,
}

impl Default for FtsEq {
    fn default() -> Self {
        let mut chain = EqChain::new();
        // Fixed slots — created once; sync_params only mutates them.
        for slot in 0..NUM_BANDS {
            let idx = chain.add_band();
            debug_assert_eq!(idx, slot);
            if let Some(band) = chain.band_mut(idx) {
                band.enabled = false;
                band.filter_type = match slot {
                    BAND_LOWCUT => FilterType::Highpass,
                    BAND_LOWSHELF => FilterType::LowShelf,
                    BAND_HIGHSHELF => FilterType::HighShelf,
                    BAND_HIGHCUT => FilterType::Lowpass,
                    _ => FilterType::Peak,
                };
                band.order = 2; // 12 dB/oct cuts; 2nd-order shelves/bells
            }
        }
        Self {
            params: Arc::new(EqParams::default()),
            chain,
            left_buf: Vec::new(),
            right_buf: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsEq {
    /// Apply host params to one band slot; recompute coefficients on change only.
    fn sync_band(
        chain: &mut EqChain,
        slot: usize,
        enabled: bool,
        freq_hz: f64,
        gain_db: f64,
        q: f64,
    ) {
        let Some(band) = chain.band_mut(slot) else { return };
        let dirty = band.enabled != enabled
            || (band.freq_hz - freq_hz).abs() > 0.01
            || (band.gain_db - gain_db).abs() > 0.01
            || (band.q - q).abs() > 0.001;
        if dirty {
            band.enabled = enabled;
            band.freq_hz = freq_hz;
            band.gain_db = gain_db;
            band.q = q;
            chain.update_band(slot);
        }
    }

    /// Push the current params into the chain (no allocation; coefficient
    /// recompute is dirty-checked per band).
    fn sync_params(&mut self) {
        // Pro-Q display convention: shown Q = 1.0 means Butterworth (1/√2).
        let q_scale = std::f64::consts::FRAC_1_SQRT_2;
        let p = self.params.clone();

        // Cuts: Butterworth response, Q fixed at neutral.
        Self::sync_band(
            &mut self.chain,
            BAND_LOWCUT,
            p.lowcut_on.value() > 0.5,
            p.lowcut_freq.value() as f64,
            0.0,
            q_scale,
        );
        Self::sync_band(
            &mut self.chain,
            BAND_HIGHCUT,
            p.highcut_on.value() > 0.5,
            p.highcut_freq.value() as f64,
            0.0,
            q_scale,
        );

        // Tonal bands: enabled whenever their gain leaves neutral.
        let tonal: [(usize, &FloatParam, &FloatParam, &FloatParam); 4] = [
            (BAND_LOWSHELF, &p.lowshelf_freq, &p.lowshelf_gain, &p.lowshelf_q),
            (BAND_PEAK1, &p.peak1_freq, &p.peak1_gain, &p.peak1_q),
            (BAND_PEAK2, &p.peak2_freq, &p.peak2_gain, &p.peak2_q),
            (BAND_HIGHSHELF, &p.highshelf_freq, &p.highshelf_gain, &p.highshelf_q),
        ];
        for (slot, freq, gain, q) in tonal {
            let gain_db = gain.value();
            Self::sync_band(
                &mut self.chain,
                slot,
                gain_db.abs() > GAIN_EPSILON_DB,
                freq.value() as f64,
                gain_db as f64,
                q.value() as f64 * q_scale,
            );
        }
    }
}

impl Plugin for FtsEq {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.chain.set_sample_rate(self.sample_rate);
        // Pre-size the f64 scratch so `process` never allocates.
        let max = buffer_config.max_buffer_size as usize;
        self.left_buf.resize(max, 0.0);
        self.right_buf.resize(max, 0.0);
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        let n = buffer.samples();
        if n > self.left_buf.len() {
            // Host exceeded its declared max buffer size; grow (rare, non-RT-safe).
            self.left_buf.resize(n, 0.0);
            self.right_buf.resize(n, 0.0);
        }

        // f32 host buffer → f64 scratch.
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            self.left_buf[i] = frame.get_mut(0).map(|s| *s as f64).unwrap_or(0.0);
            self.right_buf[i] = frame.get_mut(1).map(|s| *s as f64).unwrap_or(0.0);
        }

        self.chain
            .process(&mut self.left_buf[..n], &mut self.right_buf[..n]);

        let out_gain = 10.0_f64.powf(self.params.output_gain.value() as f64 / 20.0);
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            if let Some(s) = frame.get_mut(0) {
                *s = (self.left_buf[i] * out_gain) as f32;
            }
            if let Some(s) = frame.get_mut(1) {
                *s = (self.right_buf[i] * out_gain) as f32;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsEq {
    const CLAP_ID: &'static str = "com.fasttrackstudio.eq";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Parametric EQ: low/high cut, low/high shelf, and two peak bands");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Equalizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsEq {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsEqPlugin00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nice_export_clap!(FtsEq);
nice_export_vst3!(FtsEq);
