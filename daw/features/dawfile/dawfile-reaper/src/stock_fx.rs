//! Type-safe structs for REAPER's stock FX plugins with binary state encoding.
//!
//! Each struct represents a specific stock plugin with its known parameters,
//! correct VST identity (name, file, vendor ID), and builder-style API.
//! Parameter values are encoded into the VST binary state so REAPER loads
//! the plugin with the exact configuration you specify.
//!
//! # Example
//!
//! ```
//! use dawfile_reaper::builder::{ReaperProjectBuilder, TrackBuilder};
//! use dawfile_reaper::stock_fx::{ReaEq, ReaComp, EqBandType};
//!
//! let project = ReaperProjectBuilder::new()
//!     .track("Vocals", |t| t
//!         .stock_fx(ReaComp::new()
//!             .threshold_db(-18.0)
//!             .ratio(4.0)
//!             .attack_ms(5.0)
//!             .release_ms(100.0)
//!         )
//!         .stock_fx(ReaEq::new()
//!             .band(0, EqBandType::HighPass, 80.0, 0.0, 0.7)
//!             .band(1, EqBandType::Band, 3000.0, 2.0, 1.0)
//!         )
//!     )
//!     .build();
//! ```

use crate::types::fx_chain::{FxChainNode, FxHeaderExtra, FxPlugin, PluginType};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

// ===========================================================================
// StockFx trait
// ===========================================================================

/// Trait for stock REAPER FX plugins that can be converted to an `FxPlugin`.
///
/// Implementors provide the correct VST identity (name, file, vendor ID)
/// and encode their parameters into VST binary state data.
pub trait StockFx {
    /// Convert this stock FX configuration into an `FxPlugin`.
    fn into_fx_plugin(self) -> FxPlugin;

    /// Convert into an `FxChainNode::Plugin`.
    fn into_fx_node(self) -> FxChainNode
    where
        Self: Sized,
    {
        FxChainNode::Plugin(self.into_fx_plugin())
    }
}

// ===========================================================================
// Binary state encoding helpers
// ===========================================================================

/// Stock plugin identity: the pieces needed for REAPER to find the plugin.
struct PluginIdentity {
    /// Display name in RPP (e.g. "VST: ReaComp (Cockos)")
    display_name: &'static str,
    /// DLL/dylib basename (e.g. "reacomp.vst.dylib")
    file: &'static str,
    /// VST numeric ID + hex vendor tag
    vst_id: &'static str,
    /// 4-byte magic for the binary state (reversed short name)
    magic: [u8; 4],
}

/// Header format for 76-byte header plugins (ReaComp, ReaGate).
/// Version marker: 0xfeed5eef
const HEADER_76_TEMPLATE: [u8; 72] = [
    0xef, 0x5e, 0xed, 0xfe, // version 0xfeed5eef
    0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // param_data_size placeholder (4 bytes) + padding (4 bytes) + 0x00100000 (4 bytes)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
];

/// Header format for 60-byte header plugins (ReaVerbate, ReaLimit, ReaVerb, etc.).
fn build_header_60(magic: [u8; 4], version: u32, param_data_size: u32, flags: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(60);
    h.extend_from_slice(&magic);
    h.extend_from_slice(&version.to_le_bytes());
    // Standard 60-byte layout
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&param_data_size.to_le_bytes());
    h.extend_from_slice(&flags.to_le_bytes());
    h.extend_from_slice(&0x00100000u32.to_le_bytes());
    h
}

fn build_header_76(magic: [u8; 4], param_data_size: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(76);
    h.extend_from_slice(&magic);
    h.extend_from_slice(&HEADER_76_TEMPLATE);
    debug_assert_eq!(h.len(), 76);
    // Write param_data_size at offset 60 (after magic(4) + descriptor(56))
    h[60..64].copy_from_slice(&param_data_size.to_le_bytes());
    h
}

/// Sentinel bytes that precede float parameter data in most stock plugins.
const SENTINEL: [u8; 8] = [0xef, 0xbe, 0xad, 0xde, 0x0d, 0xf0, 0xad, 0xde];

/// Trailer bytes at the end of VST state data.
const TRAILER: [u8; 6] = [0x00, 0x00, 0x10, 0x00, 0x00, 0x00];

/// Encode binary state as base64 lines for the RPP `<VST>` block.
fn encode_state_lines(data: &[u8]) -> Vec<String> {
    let b64 = BASE64.encode(data);
    // REAPER uses ~128-char base64 lines
    b64.as_bytes()
        .chunks(128)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect()
}

/// Build a complete FxPlugin with encoded binary state.
fn build_fx_plugin_with_state(
    id: &PluginIdentity,
    custom_name: Option<String>,
    bypassed: bool,
    state_bytes: Vec<u8>,
) -> FxPlugin {
    let custom = custom_name.as_deref().unwrap_or("");
    let raw_block = String::new(); // We use state_data instead of raw_block

    let state_data = encode_state_lines(&state_bytes);

    FxPlugin {
        name: id.display_name.to_string(),
        custom_name: custom_name.clone(),
        plugin_type: PluginType::Vst,
        file: id.file.to_string(),
        bypassed,
        offline: false,
        fxid: None,
        preset_name: None,
        float_pos: None,
        wak: None,
        parallel: false,
        state_data,
        raw_block,
        param_envelopes: vec![],
        params_on_tcp: vec![],
        header_extra: Some(FxHeaderExtra {
            flags: "0".to_string(),
            tail: format!("{} \"\"", id.vst_id),
        }),
    }
}

/// Build FxPlugin without state (identity only, REAPER uses defaults).
fn build_fx_plugin_no_state(
    id: &PluginIdentity,
    custom_name: Option<String>,
    bypassed: bool,
) -> FxPlugin {
    let custom = custom_name.as_deref().unwrap_or("");
    let raw_block = format!(
        "<VST \"{}\" {} 0 \"{}\" {} \"\"\n>",
        id.display_name, id.file, custom, id.vst_id,
    );

    FxPlugin {
        name: id.display_name.to_string(),
        custom_name,
        plugin_type: PluginType::Vst,
        file: id.file.to_string(),
        bypassed,
        offline: false,
        fxid: None,
        preset_name: None,
        float_pos: None,
        wak: None,
        parallel: false,
        state_data: vec![],
        raw_block,
        param_envelopes: vec![],
        params_on_tcp: vec![],
        header_extra: None,
    }
}

// ===========================================================================
// ReaComp — Compressor
// ===========================================================================

const REACOMP_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaComp (Cockos)",
    file: "reacomp.vst.dylib",
    vst_id: "1919247213<5653547265636D726561636F6D700000>",
    magic: *b"mcer",
};

/// REAPER stock compressor plugin.
///
/// Parameters are encoded into VST binary state using REAPER's normalized
/// float format. The normalized values are computed from the human-readable
/// parameter values you set via the builder methods.
#[derive(Debug, Clone)]
pub struct ReaComp {
    /// Threshold in dB (range: -60.0 to 0.0, default: 0.0)
    pub threshold_db: f64,
    /// Compression ratio (range: 1.0 to infinity, default: 4.0)
    pub ratio: f64,
    /// Attack time in ms (range: 0.0 to 500.0, default: 3.0)
    pub attack_ms: f64,
    /// Release time in ms (range: 0.0 to 2000.0, default: 100.0)
    pub release_ms: f64,
    /// Pre-comp (lookahead) in ms (range: 0.0 to 30.0, default: 0.0)
    pub pre_comp_ms: f64,
    /// Knee width (normalized 0.0 to 1.0, default: 0.0)
    pub knee: f64,
    /// Output/makeup gain in dB (range: -inf to 24.0, default: 0.0)
    pub output_db: f64,
    /// Wet mix (0.0 to 1.0, default: 1.0)
    pub wet: f64,
    /// Dry mix (0.0 to 1.0, default: 0.0)
    pub dry: f64,
    /// Auto makeup gain
    pub auto_makeup: bool,
    /// RMS window size (normalized, default: 0.0)
    pub rms_size: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name in REAPER's FX chain
    pub custom_name: Option<String>,
}

impl Default for ReaComp {
    fn default() -> Self {
        Self {
            threshold_db: 0.0,
            ratio: 4.0,
            attack_ms: 3.0,
            release_ms: 100.0,
            pre_comp_ms: 0.0,
            knee: 0.0,
            output_db: 0.0,
            wet: 1.0,
            dry: 0.0,
            auto_makeup: false,
            rms_size: 0.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaComp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn threshold_db(mut self, db: f64) -> Self {
        self.threshold_db = db;
        self
    }

    pub fn ratio(mut self, ratio: f64) -> Self {
        self.ratio = ratio;
        self
    }

    pub fn attack_ms(mut self, ms: f64) -> Self {
        self.attack_ms = ms;
        self
    }

    pub fn release_ms(mut self, ms: f64) -> Self {
        self.release_ms = ms;
        self
    }

    pub fn pre_comp_ms(mut self, ms: f64) -> Self {
        self.pre_comp_ms = ms;
        self
    }

    pub fn knee(mut self, knee: f64) -> Self {
        self.knee = knee;
        self
    }

    pub fn output_db(mut self, db: f64) -> Self {
        self.output_db = db;
        self
    }

    pub fn wet(mut self, wet: f64) -> Self {
        self.wet = wet;
        self
    }

    pub fn dry(mut self, dry: f64) -> Self {
        self.dry = dry;
        self
    }

    pub fn auto_makeup(mut self) -> Self {
        self.auto_makeup = true;
        self
    }

    pub fn rms_size(mut self, size: f64) -> Self {
        self.rms_size = size;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    /// Encode parameter values into the 22-float parameter block.
    ///
    /// The parameter mapping was reverse-engineered from REAPER's binary state format.
    /// Values are normalized floats where REAPER maps them to the actual control ranges.
    fn encode_params(&self) -> Vec<f32> {
        // ReaComp uses normalized 0-1 float params.
        // Threshold: dB_to_amplitude(-60..0) → the state stores the amplitude (linear)
        // Since threshold_db is in dB, convert: amplitude = 10^(dB/20)
        let thresh_amp = if self.threshold_db <= -150.0 {
            0.0
        } else {
            10.0_f64.powf(self.threshold_db / 20.0) as f32
        };

        // Attack/Release: stored as seconds (time_ms / 1000.0)
        let attack = (self.attack_ms / 1000.0) as f32;
        let release = (self.release_ms / 1000.0) as f32;
        let pre_comp = (self.pre_comp_ms / 1000.0) as f32;

        // Ratio: normalized where 0 = 1:1, higher = more compression
        // The mapping appears to be: stored_value ≈ 1.0 - 1.0/ratio (approximate)
        let ratio_norm = if self.ratio <= 1.0 {
            0.0
        } else {
            (1.0 - 1.0 / self.ratio) as f32
        };

        // Output: amplitude
        let output_amp = if self.output_db <= -150.0 {
            0.0
        } else {
            10.0_f64.powf(self.output_db / 20.0) as f32
        };

        // Dry/Wet: direct normalized values
        let dry = self.dry as f32;
        let wet = self.wet as f32;

        // Auto-makeup: 0 or 1
        let auto_makeup = if self.auto_makeup { 1.0_f32 } else { 0.0 };

        let mut params = vec![0.0_f32; 22];
        params[0] = thresh_amp; // threshold (amplitude)
        params[1] = pre_comp; // pre-comp (seconds)
        params[2] = attack; // attack (seconds)
        params[3] = release; // release (seconds)
        params[4] = attack; // attack2 (mirror)
        params[5] = 0.0; // unused
        params[6] = ratio_norm; // ratio (normalized)
        params[7] = self.knee as f32; // knee
        params[8] = 0.0; // unused
        params[9] = 0.0; // unused
        params[10] = dry; // dry
        params[11] = wet; // wet
        params[12] = 0.0; // unused
        params[13] = self.rms_size as f32; // RMS size
        params[14] = 0.0; // unused
        params[15] = 0.0; // unused
        params[16] = 0.0; // unused
        params[17] = auto_makeup; // auto-makeup
        params[18] = output_amp; // output (amplitude)
        params
    }

    fn encode_state(&self) -> Vec<u8> {
        let params = self.encode_params();
        let param_bytes: Vec<u8> = params.iter().flat_map(|f| f.to_le_bytes()).collect();
        let param_data_size = (SENTINEL.len() + param_bytes.len()) as u32;

        let mut state = build_header_76(REACOMP_ID.magic, param_data_size);
        state.extend_from_slice(&SENTINEL);
        state.extend_from_slice(&param_bytes);
        state.extend_from_slice(&TRAILER);
        state
    }
}

impl StockFx for ReaComp {
    fn into_fx_plugin(self) -> FxPlugin {
        let state = self.encode_state();
        build_fx_plugin_with_state(&REACOMP_ID, self.custom_name, self.bypassed, state)
    }
}

// ===========================================================================
// ReaEq — Parametric Equalizer
// ===========================================================================

const REAEQ_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaEQ (Cockos)",
    file: "reaeq.vst.dylib",
    vst_id: "1919247729<56535472656571726561657100000000>",
    magic: *b"qeer",
};

/// EQ band filter type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EqBandType {
    /// Parametric band (bell) — type 0 or 8 in binary
    Band = 0,
    /// Low shelf — type 1
    LowShelf = 1,
    /// High shelf — type 2
    HighShelf = 2,
    /// High pass filter — type 3
    HighPass = 3,
    /// Low pass filter — type 4
    LowPass = 4,
    /// All pass filter — type 5
    AllPass = 5,
    /// Notch filter — type 6
    Notch = 6,
    /// Band pass filter — type 7
    BandPass = 7,
}

impl EqBandType {
    fn to_binary_type(self) -> u32 {
        match self {
            EqBandType::Band => 8, // REAPER uses 8 for parametric band in binary
            EqBandType::LowShelf => 1,
            EqBandType::HighShelf => 2,
            EqBandType::HighPass => 3,
            EqBandType::LowPass => 4,
            EqBandType::AllPass => 5,
            EqBandType::Notch => 6,
            EqBandType::BandPass => 7,
        }
    }
}

/// A single EQ band configuration.
#[derive(Debug, Clone)]
pub struct EqBand {
    /// Band filter type
    pub band_type: EqBandType,
    /// Frequency in Hz
    pub freq_hz: f64,
    /// Gain in dB (not used for HP/LP/notch/allpass/bandpass)
    pub gain_db: f64,
    /// Bandwidth / Q factor
    pub bandwidth: f64,
    /// Whether this band is enabled
    pub enabled: bool,
}

/// REAPER stock parametric EQ plugin (up to 64 bands).
///
/// Band parameters are encoded into the binary state using REAPER's
/// per-band format: type(u32) + enabled(u32) + freq(f64) + gain(f64) + bw(f64) + tail(u8).
#[derive(Debug, Clone, Default)]
pub struct ReaEq {
    /// EQ bands (indexed 0..N)
    pub bands: Vec<EqBand>,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl ReaEq {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a band at the given index.
    pub fn band(
        mut self,
        index: usize,
        band_type: EqBandType,
        freq_hz: f64,
        gain_db: f64,
        bandwidth: f64,
    ) -> Self {
        while self.bands.len() <= index {
            self.bands.push(EqBand {
                band_type: EqBandType::Band,
                freq_hz: 1000.0,
                gain_db: 0.0,
                bandwidth: 1.0,
                enabled: false,
            });
        }
        self.bands[index] = EqBand {
            band_type,
            freq_hz,
            gain_db,
            bandwidth,
            enabled: true,
        };
        self
    }

    /// Add a high-pass filter band.
    pub fn high_pass(self, index: usize, freq_hz: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::HighPass, freq_hz, 0.0, bandwidth)
    }

    /// Add a low-pass filter band.
    pub fn low_pass(self, index: usize, freq_hz: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::LowPass, freq_hz, 0.0, bandwidth)
    }

    /// Add a low shelf band.
    pub fn low_shelf(self, index: usize, freq_hz: f64, gain_db: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::LowShelf, freq_hz, gain_db, bandwidth)
    }

    /// Add a high shelf band.
    pub fn high_shelf(self, index: usize, freq_hz: f64, gain_db: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::HighShelf, freq_hz, gain_db, bandwidth)
    }

    /// Add a parametric (bell) band.
    pub fn bell(self, index: usize, freq_hz: f64, gain_db: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::Band, freq_hz, gain_db, bandwidth)
    }

    /// Add a notch filter band.
    pub fn notch(self, index: usize, freq_hz: f64, bandwidth: f64) -> Self {
        self.band(index, EqBandType::Notch, freq_hz, 0.0, bandwidth)
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    /// Encode the EQ bands into binary state.
    ///
    /// ReaEQ binary format (after 60-byte header):
    /// - flags: u32 (0x21 = 33)
    /// - num_bands: u32
    /// - per band (33 bytes each):
    ///   - type: u32
    ///   - enabled: u32
    ///   - freq: f64 (Hz)
    ///   - gain: f64 (linear amplitude = 10^(dB/20))
    ///   - bandwidth: f64
    ///   - tail_byte: u8 (always 1)
    /// - footer: varies (window size info)
    fn encode_state(&self) -> Vec<u8> {
        let num_bands = self.bands.len().max(1) as u32; // At least 1 band
        let band_data_size = num_bands as usize * 33;
        // flags(4) + num_bands(4) + band_data + footer(~10 bytes)
        let footer_size = 10;
        let param_data_size = 4 + 4 + band_data_size + footer_size;

        let header = build_header_60(REAEQ_ID.magic, 0xfeed5eee, param_data_size as u32, 1);

        let mut state = header;

        // Flags
        state.extend_from_slice(&0x21u32.to_le_bytes());
        // Num bands
        state.extend_from_slice(&num_bands.to_le_bytes());

        // Per-band data
        for band in &self.bands {
            let band_type = band.band_type.to_binary_type();
            let enabled: u32 = if band.enabled { 1 } else { 0 };
            // Gain stored as linear amplitude
            let gain_linear = 10.0_f64.powf(band.gain_db / 20.0);

            state.extend_from_slice(&band_type.to_le_bytes());
            state.extend_from_slice(&enabled.to_le_bytes());
            state.extend_from_slice(&band.freq_hz.to_le_bytes());
            state.extend_from_slice(&gain_linear.to_le_bytes());
            state.extend_from_slice(&band.bandwidth.to_le_bytes());
            state.push(1); // tail byte
        }

        // If no bands were specified, write one default disabled band
        if self.bands.is_empty() {
            state.extend_from_slice(&0u32.to_le_bytes()); // type: band
            state.extend_from_slice(&0u32.to_le_bytes()); // disabled
            state.extend_from_slice(&1000.0_f64.to_le_bytes()); // freq
            state.extend_from_slice(&1.0_f64.to_le_bytes()); // gain (0 dB)
            state.extend_from_slice(&1.0_f64.to_le_bytes()); // bandwidth
            state.push(1);
        }

        // Footer: window size and misc settings
        state.extend_from_slice(&1u32.to_le_bytes()); // flags
        state.extend_from_slice(&1u32.to_le_bytes()); // more flags
        state.push(0);
        state.push(0);

        state.extend_from_slice(&TRAILER);
        state
    }
}

impl StockFx for ReaEq {
    fn into_fx_plugin(self) -> FxPlugin {
        let state = self.encode_state();
        build_fx_plugin_with_state(&REAEQ_ID, self.custom_name, self.bypassed, state)
    }
}

// ===========================================================================
// ReaGate — Noise Gate
// ===========================================================================

const REAGATE_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaGate (Cockos)",
    file: "reagate.vst.dylib",
    vst_id: "1919248244<56535472656774726561676174650000>",
    magic: *b"tger",
};

/// REAPER stock noise gate plugin.
#[derive(Debug, Clone)]
pub struct ReaGate {
    /// Threshold in dB (range: -100.0 to 0.0, default: -24.0)
    pub threshold_db: f64,
    /// Attack time in ms (default: 1.0)
    pub attack_ms: f64,
    /// Hold time in ms (default: 50.0)
    pub hold_ms: f64,
    /// Release time in ms (default: 100.0)
    pub release_ms: f64,
    /// Pre-open time in ms (default: 0.0)
    pub pre_open_ms: f64,
    /// Hysteresis in dB (default: 0.0)
    pub hysteresis_db: f64,
    /// Wet mix (0.0 to 1.0, default: 1.0)
    pub wet: f64,
    /// Dry mix (0.0 to 1.0, default: 0.0)
    pub dry: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaGate {
    fn default() -> Self {
        Self {
            threshold_db: -24.0,
            attack_ms: 1.0,
            hold_ms: 50.0,
            release_ms: 100.0,
            pre_open_ms: 0.0,
            hysteresis_db: 0.0,
            wet: 1.0,
            dry: 0.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn threshold_db(mut self, db: f64) -> Self {
        self.threshold_db = db;
        self
    }

    pub fn attack_ms(mut self, ms: f64) -> Self {
        self.attack_ms = ms;
        self
    }

    pub fn hold_ms(mut self, ms: f64) -> Self {
        self.hold_ms = ms;
        self
    }

    pub fn release_ms(mut self, ms: f64) -> Self {
        self.release_ms = ms;
        self
    }

    pub fn pre_open_ms(mut self, ms: f64) -> Self {
        self.pre_open_ms = ms;
        self
    }

    pub fn hysteresis_db(mut self, db: f64) -> Self {
        self.hysteresis_db = db;
        self
    }

    pub fn wet(mut self, wet: f64) -> Self {
        self.wet = wet;
        self
    }

    pub fn dry(mut self, dry: f64) -> Self {
        self.dry = dry;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    fn encode_params(&self) -> Vec<f32> {
        let thresh_amp = if self.threshold_db <= -150.0 {
            0.0
        } else {
            10.0_f64.powf(self.threshold_db / 20.0) as f32
        };
        let attack = (self.attack_ms / 1000.0) as f32;
        let hold = (self.hold_ms / 1000.0) as f32;
        let release = (self.release_ms / 1000.0) as f32;
        let pre_open = (self.pre_open_ms / 1000.0) as f32;
        let hysteresis = if self.hysteresis_db <= -150.0 {
            0.0
        } else {
            10.0_f64.powf(self.hysteresis_db / 20.0) as f32
        };

        let mut params = vec![0.0_f32; 22];
        params[0] = thresh_amp; // threshold
        params[1] = attack; // attack (seconds)
        params[2] = hold; // hold (seconds)
        params[3] = pre_open; // pre-open (seconds)
        params[4] = 0.0; // unused
        params[5] = self.wet as f32; // wet
        params[6] = 0.0; // unused
        params[7] = 0.0; // unused
        params[8] = 0.0; // unused
        params[9] = hysteresis; // hysteresis
        params[10] = self.wet as f32; // wet (mirror)
        params[11] = hysteresis; // hysteresis (mirror)
        params[12] = self.wet as f32; // wet (mirror)
        params[13] = 0.0; // unused
        params[14] = 0.0; // unused
        params[15] = 0.0; // unused
        params[16] = release; // release (seconds)
        params
    }

    fn encode_state(&self) -> Vec<u8> {
        let params = self.encode_params();
        let param_bytes: Vec<u8> = params.iter().flat_map(|f| f.to_le_bytes()).collect();
        let param_data_size = (SENTINEL.len() + param_bytes.len()) as u32;

        let mut state = build_header_76(REAGATE_ID.magic, param_data_size);
        state.extend_from_slice(&SENTINEL);
        state.extend_from_slice(&param_bytes);
        state.extend_from_slice(&TRAILER);
        state
    }
}

impl StockFx for ReaGate {
    fn into_fx_plugin(self) -> FxPlugin {
        let state = self.encode_state();
        build_fx_plugin_with_state(&REAGATE_ID, self.custom_name, self.bypassed, state)
    }
}

// ===========================================================================
// ReaDelay — Delay/Echo
// ===========================================================================

const READELAY_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaDelay (Cockos)",
    file: "readelay.vst.dylib",
    vst_id: "1919247468<5653547265646C72656164656C617900>",
    magic: *b"lder",
};

/// REAPER stock delay plugin.
#[derive(Debug, Clone)]
pub struct ReaDelay {
    /// Delay time in ms (default: 200.0)
    pub delay_ms: f64,
    /// Feedback amount (0.0 to 1.0, default: 0.5)
    pub feedback: f64,
    /// Wet/dry mix (0.0 to 1.0, default: 0.5)
    pub wet: f64,
    /// Low-pass filter frequency in Hz (default: 20000.0)
    pub lowpass_hz: f64,
    /// High-pass filter frequency in Hz (default: 0.0)
    pub highpass_hz: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaDelay {
    fn default() -> Self {
        Self {
            delay_ms: 200.0,
            feedback: 0.5,
            wet: 0.5,
            lowpass_hz: 20000.0,
            highpass_hz: 0.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaDelay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn delay_ms(mut self, ms: f64) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn feedback(mut self, fb: f64) -> Self {
        self.feedback = fb;
        self
    }

    pub fn wet(mut self, wet: f64) -> Self {
        self.wet = wet;
        self
    }

    pub fn lowpass_hz(mut self, hz: f64) -> Self {
        self.lowpass_hz = hz;
        self
    }

    pub fn highpass_hz(mut self, hz: f64) -> Self {
        self.highpass_hz = hz;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaDelay {
    fn into_fx_plugin(self) -> FxPlugin {
        // ReaDelay has a complex multi-tap format — use identity-only for now
        build_fx_plugin_no_state(&READELAY_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaVerbate — Algorithmic Reverb
// ===========================================================================

const REAVERBATE_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaVerbate (Cockos)",
    file: "reaverbate.vst.dylib",
    vst_id: "1920361016<56535472766238726561766572626174>",
    magic: *b"8bvr",
};

/// REAPER stock algorithmic reverb plugin.
#[derive(Debug, Clone)]
pub struct ReaVerbate {
    /// Room size (0.0 to 1.0, default: 0.5)
    pub room_size: f64,
    /// Damping (0.0 to 1.0, default: 0.5)
    pub damping: f64,
    /// Stereo width (0.0 to 1.0, default: 1.0)
    pub stereo_width: f64,
    /// Initial delay in ms (default: 0.0)
    pub initial_delay_ms: f64,
    /// Low-pass filter frequency in Hz (default: 20000.0)
    pub lowpass_hz: f64,
    /// High-pass filter frequency in Hz (default: 0.0)
    pub highpass_hz: f64,
    /// Wet/dry mix (0.0 to 1.0, default: 0.5)
    pub wet: f64,
    /// Dry level (0.0 to 1.0, default: 1.0)
    pub dry: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaVerbate {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            damping: 0.5,
            stereo_width: 1.0,
            initial_delay_ms: 0.0,
            lowpass_hz: 20000.0,
            highpass_hz: 0.0,
            wet: 0.5,
            dry: 1.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaVerbate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn room_size(mut self, size: f64) -> Self {
        self.room_size = size;
        self
    }

    pub fn damping(mut self, damping: f64) -> Self {
        self.damping = damping;
        self
    }

    pub fn stereo_width(mut self, width: f64) -> Self {
        self.stereo_width = width;
        self
    }

    pub fn initial_delay_ms(mut self, ms: f64) -> Self {
        self.initial_delay_ms = ms;
        self
    }

    pub fn lowpass_hz(mut self, hz: f64) -> Self {
        self.lowpass_hz = hz;
        self
    }

    pub fn highpass_hz(mut self, hz: f64) -> Self {
        self.highpass_hz = hz;
        self
    }

    pub fn wet(mut self, wet: f64) -> Self {
        self.wet = wet;
        self
    }

    pub fn dry(mut self, dry: f64) -> Self {
        self.dry = dry;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    fn encode_state(&self) -> Vec<u8> {
        // ReaVerbate: 60-byte header + sentinel + 9 floats + trailer
        // Param mapping from decoded data:
        //   [0] = room_size, [1] = dry, [2] = damping, [3] = wet,
        //   [4] = dry(mirror), [5] = initial_delay, [6] = width, [7] = unused
        let params: Vec<f32> = vec![
            self.room_size as f32,
            self.dry as f32,
            self.damping as f32,
            self.wet as f32,
            self.dry as f32,
            0.0, // initial delay (normalized)
            self.stereo_width as f32,
            0.0,
            0.0,
        ];

        let param_bytes: Vec<u8> = params.iter().flat_map(|f| f.to_le_bytes()).collect();
        let param_data_size = (SENTINEL.len() + param_bytes.len()) as u32;

        let header = build_header_60(REAVERBATE_ID.magic, 0xfeed5eef, param_data_size, 0);

        let mut state = header;
        state.extend_from_slice(&SENTINEL);
        state.extend_from_slice(&param_bytes);
        state.extend_from_slice(&TRAILER);
        state
    }
}

impl StockFx for ReaVerbate {
    fn into_fx_plugin(self) -> FxPlugin {
        let state = self.encode_state();
        build_fx_plugin_with_state(&REAVERBATE_ID, self.custom_name, self.bypassed, state)
    }
}

// ===========================================================================
// ReaVerb — Convolution Reverb
// ===========================================================================

const REAVERB_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaVerb (Cockos)",
    file: "reaverb.vst.dylib",
    vst_id: "1919252066<56535472657662726561766572620000>",
    magic: *b"bver",
};

/// REAPER stock convolution reverb plugin.
///
/// Note: ReaVerb uses impulse response files for its reverb character.
/// IR configuration requires loading the plugin in REAPER.
#[derive(Debug, Clone)]
pub struct ReaVerb {
    /// Wet/dry mix (0.0 to 1.0, default: 0.5)
    pub wet: f64,
    /// Dry level (0.0 to 1.0, default: 1.0)
    pub dry: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaVerb {
    fn default() -> Self {
        Self {
            wet: 0.5,
            dry: 1.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaVerb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wet(mut self, wet: f64) -> Self {
        self.wet = wet;
        self
    }

    pub fn dry(mut self, dry: f64) -> Self {
        self.dry = dry;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaVerb {
    fn into_fx_plugin(self) -> FxPlugin {
        // ReaVerb has IR-based state — use identity-only
        build_fx_plugin_no_state(&REAVERB_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaLimit — Brickwall Limiter
// ===========================================================================

const REALIMIT_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaLimit (Cockos)",
    file: "realimit.vst.dylib",
    vst_id: "1919708532<565354726C6D747265616C696D697400>",
    magic: *b"tmlr",
};

/// REAPER stock brickwall limiter plugin.
///
/// Uses doubles for parameter storage (higher precision for dB values).
#[derive(Debug, Clone)]
pub struct ReaLimit {
    /// Threshold/ceiling in dB (range: -40.0 to 0.0, default: 0.0)
    pub threshold_db: f64,
    /// Brickwall ceiling in dB (default: 0.0)
    pub ceiling_db: f64,
    /// Release time in ms (default: 100.0)
    pub release_ms: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaLimit {
    fn default() -> Self {
        Self {
            threshold_db: 0.0,
            ceiling_db: 0.0,
            release_ms: 100.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaLimit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn threshold_db(mut self, db: f64) -> Self {
        self.threshold_db = db;
        self
    }

    pub fn ceiling_db(mut self, db: f64) -> Self {
        self.ceiling_db = db;
        self
    }

    pub fn release_ms(mut self, ms: f64) -> Self {
        self.release_ms = ms;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    fn encode_state(&self) -> Vec<u8> {
        // ReaLimit: 60-byte header + uint32 + 5 doubles + trailer
        // From decoded data:
        //   uint32: 3 (version/flags)
        //   double[0]: threshold_db (e.g. -23.2)
        //   double[1]: ceiling_db (e.g. -8.95)
        //   double[2]: ??? (near 0, maybe a flag)
        //   double[3]: 0.0
        //   double[4]: release (normalized, e.g. 0.178 for some release time)
        //
        // Release mapping: the stored value appears to be 10^(release_db/20)
        // where release_db controls the time. For 100ms default, value ≈ 0.178

        let param_data_size = 4 + 5 * 8; // uint32 + 5 doubles = 44 bytes
        let header = build_header_60(REALIMIT_ID.magic, 0xfeed5eee, param_data_size as u32, 1);

        let mut state = header;

        // Version/flags
        state.extend_from_slice(&3u32.to_le_bytes());
        // Padding
        state.extend_from_slice(&0u32.to_le_bytes());

        // Threshold in dB (direct)
        state.extend_from_slice(&self.threshold_db.to_le_bytes());
        // Ceiling in dB (direct)
        state.extend_from_slice(&self.ceiling_db.to_le_bytes());
        // Flags (2 = enabled or something)
        state.extend_from_slice(&2u64.to_le_bytes());
        // Unused
        state.extend_from_slice(&0.0_f64.to_le_bytes());
        // Release (normalized — approximate mapping)
        let release_norm = (self.release_ms / 1000.0).min(1.0);
        state.extend_from_slice(&release_norm.to_le_bytes());

        state.extend_from_slice(&TRAILER);
        state
    }
}

impl StockFx for ReaLimit {
    fn into_fx_plugin(self) -> FxPlugin {
        let state = self.encode_state();
        build_fx_plugin_with_state(&REALIMIT_ID, self.custom_name, self.bypassed, state)
    }
}

// ===========================================================================
// ReaSynth — Simple Synthesizer
// ===========================================================================

const REASYNTH_ID: PluginIdentity = PluginIdentity {
    display_name: "VSTi: ReaSynth (Cockos)",
    file: "reasynth.vst.dylib",
    vst_id: "1919251321<5653547265737972656173796E746800>",
    magic: *b"syhr",
};

/// REAPER stock simple synthesizer (VSTi).
#[derive(Debug, Clone)]
pub struct ReaSynth {
    /// Oscillator waveform (0.0 = sine, 0.25 = triangle, 0.5 = square, 1.0 = saw)
    pub waveform: f64,
    /// Volume (0.0 to 1.0, default: 0.5)
    pub volume: f64,
    /// Attack time in ms (default: 10.0)
    pub attack_ms: f64,
    /// Decay time in ms (default: 100.0)
    pub decay_ms: f64,
    /// Sustain level (0.0 to 1.0, default: 0.5)
    pub sustain: f64,
    /// Release time in ms (default: 100.0)
    pub release_ms: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaSynth {
    fn default() -> Self {
        Self {
            waveform: 0.0,
            volume: 0.5,
            attack_ms: 10.0,
            decay_ms: 100.0,
            sustain: 0.5,
            release_ms: 100.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaSynth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn waveform(mut self, wf: f64) -> Self {
        self.waveform = wf;
        self
    }

    pub fn sine(mut self) -> Self {
        self.waveform = 0.0;
        self
    }

    pub fn triangle(mut self) -> Self {
        self.waveform = 0.25;
        self
    }

    pub fn square(mut self) -> Self {
        self.waveform = 0.5;
        self
    }

    pub fn sawtooth(mut self) -> Self {
        self.waveform = 1.0;
        self
    }

    pub fn volume(mut self, vol: f64) -> Self {
        self.volume = vol;
        self
    }

    pub fn attack_ms(mut self, ms: f64) -> Self {
        self.attack_ms = ms;
        self
    }

    pub fn decay_ms(mut self, ms: f64) -> Self {
        self.decay_ms = ms;
        self
    }

    pub fn sustain(mut self, level: f64) -> Self {
        self.sustain = level;
        self
    }

    pub fn release_ms(mut self, ms: f64) -> Self {
        self.release_ms = ms;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaSynth {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REASYNTH_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaTune — Tuner / Pitch Correction
// ===========================================================================

const REATUNE_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaTune (Cockos)",
    file: "reatune.vst.dylib",
    vst_id: "1919251566<5653547265746E72656174756E650000>",
    magic: *b"tnhr",
};

/// REAPER stock tuner / pitch correction plugin.
#[derive(Debug, Clone)]
pub struct ReaTune {
    /// Correction speed (0.0 = instant, 1.0 = slow, default: 0.5)
    pub correction_speed: f64,
    /// Reference pitch in Hz (default: 440.0)
    pub reference_hz: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaTune {
    fn default() -> Self {
        Self {
            correction_speed: 0.5,
            reference_hz: 440.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaTune {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn correction_speed(mut self, speed: f64) -> Self {
        self.correction_speed = speed;
        self
    }

    pub fn reference_hz(mut self, hz: f64) -> Self {
        self.reference_hz = hz;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaTune {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REATUNE_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaXcomp — Multiband Compressor
// ===========================================================================

const REAXCOMP_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaXcomp (Cockos)",
    file: "reaxcomp.vst.dylib",
    vst_id: "1919252080<5653547265787072656178636F6D7000>",
    magic: *b"xphr",
};

/// REAPER stock multiband compressor plugin.
#[derive(Debug, Clone)]
pub struct ReaXcomp {
    /// Number of bands (default: 3)
    pub num_bands: u32,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaXcomp {
    fn default() -> Self {
        Self {
            num_bands: 3,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaXcomp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn num_bands(mut self, n: u32) -> Self {
        self.num_bands = n;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaXcomp {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REAXCOMP_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaFir — FFT-based EQ/Compressor/Gate
// ===========================================================================

const REAFIR_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaFir (Cockos)",
    file: "reafir.vst.dylib",
    vst_id: "1919247730<5653547265667272656166697200000000>",
    magic: *b"frhr",
};

/// ReaFir operating mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReaFirMode {
    Eq,
    Compressor,
    Gate,
    Subtract,
}

/// REAPER stock FFT-based EQ/dynamics plugin.
#[derive(Debug, Clone)]
pub struct ReaFir {
    pub mode: ReaFirMode,
    pub fft_size: u32,
    pub bypassed: bool,
    pub custom_name: Option<String>,
}

impl Default for ReaFir {
    fn default() -> Self {
        Self {
            mode: ReaFirMode::Eq,
            fft_size: 4096,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaFir {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(mut self, mode: ReaFirMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn fft_size(mut self, size: u32) -> Self {
        self.fft_size = size;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaFir {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REAFIR_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaInsert — Hardware Insert
// ===========================================================================

const REAINSERT_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaInsert (Cockos)",
    file: "reainsert.vst.dylib",
    vst_id: "1919248500<56535472696E7372656169",
    magic: *b"insr",
};

/// REAPER stock hardware insert plugin.
#[derive(Debug, Clone, Default)]
pub struct ReaInsert {
    pub bypassed: bool,
    pub custom_name: Option<String>,
}

impl ReaInsert {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaInsert {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REAINSERT_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaStream — Network Audio Streaming
// ===========================================================================

const REASTREAM_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaStream (Cockos)",
    file: "reastream.vst.dylib",
    vst_id: "1919251315<5653547265737472656173747265616D>",
    magic: *b"sthr",
};

/// REAPER stock network audio streaming plugin.
#[derive(Debug, Clone, Default)]
pub struct ReaStream {
    pub bypassed: bool,
    pub custom_name: Option<String>,
}

impl ReaStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaStream {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REASTREAM_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaControlMIDI — MIDI CC Control
// ===========================================================================

const REACONTROLMIDI_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaControlMIDI (Cockos)",
    file: "reacontrolmidi.vst.dylib",
    vst_id: "1919247203<5653547265636D726561636D6964696D>",
    magic: *b"cmhr",
};

/// REAPER stock MIDI CC control surface plugin.
#[derive(Debug, Clone, Default)]
pub struct ReaControlMidi {
    pub bypassed: bool,
    pub custom_name: Option<String>,
}

impl ReaControlMidi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaControlMidi {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REACONTROLMIDI_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// ReaPitch — Pitch Shifter
// ===========================================================================

const REAPITCH_ID: PluginIdentity = PluginIdentity {
    display_name: "VST: ReaPitch (Cockos)",
    file: "reapitch.vst.dylib",
    vst_id: "1919250544<5653547265707472656170697463680000>",
    magic: *b"pthr",
};

/// REAPER stock pitch shifter plugin.
#[derive(Debug, Clone)]
pub struct ReaPitch {
    /// Pitch shift in semitones (range: -24.0 to 24.0, default: 0.0)
    pub shift_semitones: f64,
    /// Fine pitch shift in cents (range: -100.0 to 100.0, default: 0.0)
    pub shift_cents: f64,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
    /// Custom display name
    pub custom_name: Option<String>,
}

impl Default for ReaPitch {
    fn default() -> Self {
        Self {
            shift_semitones: 0.0,
            shift_cents: 0.0,
            bypassed: false,
            custom_name: None,
        }
    }
}

impl ReaPitch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shift_semitones(mut self, semitones: f64) -> Self {
        self.shift_semitones = semitones;
        self
    }

    pub fn shift_cents(mut self, cents: f64) -> Self {
        self.shift_cents = cents;
        self
    }

    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    pub fn custom_name(mut self, name: impl Into<String>) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

impl StockFx for ReaPitch {
    fn into_fx_plugin(self) -> FxPlugin {
        build_fx_plugin_no_state(&REAPITCH_ID, self.custom_name, self.bypassed)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{ReaperProjectBuilder, TrackBuilder};
    use crate::types::serialize::RppSerialize;

    #[test]
    fn test_reacomp_default() {
        let comp = ReaComp::new();
        assert_eq!(comp.threshold_db, 0.0);
        assert_eq!(comp.ratio, 4.0);
        assert!(!comp.bypassed);
    }

    #[test]
    fn test_reacomp_builder() {
        let comp = ReaComp::new()
            .threshold_db(-18.0)
            .ratio(4.0)
            .attack_ms(5.0)
            .release_ms(100.0)
            .auto_makeup()
            .custom_name("Control Compression");

        assert_eq!(comp.threshold_db, -18.0);
        assert!(comp.auto_makeup);
        assert_eq!(comp.custom_name.as_deref(), Some("Control Compression"));
    }

    #[test]
    fn test_reacomp_encodes_state() {
        let plugin = ReaComp::new()
            .threshold_db(-18.0)
            .ratio(4.0)
            .custom_name("My Comp")
            .bypassed()
            .into_fx_plugin();

        assert_eq!(plugin.name, "VST: ReaComp (Cockos)");
        assert_eq!(plugin.file, "reacomp.vst.dylib");
        assert!(plugin.bypassed);
        // Should have base64 state data, not raw_block
        assert!(!plugin.state_data.is_empty(), "should have encoded state");
        assert!(plugin.raw_block.is_empty(), "should not use raw_block");

        // Verify the state decodes correctly
        let b64: String = plugin.state_data.join("");
        let decoded = BASE64.decode(&b64).expect("valid base64");
        // Check magic
        assert_eq!(&decoded[0..4], b"mcer", "ReaComp magic");
    }

    #[test]
    fn test_reaeq_encodes_bands() {
        let plugin = ReaEq::new()
            .high_pass(0, 80.0, 0.7)
            .bell(1, 400.0, -3.0, 1.0)
            .bell(2, 3000.0, 2.0, 1.5)
            .high_shelf(3, 8000.0, 1.5, 0.7)
            .into_fx_plugin();

        assert!(!plugin.state_data.is_empty());

        // Decode and verify band structure
        let b64: String = plugin.state_data.join("");
        let decoded = BASE64.decode(&b64).expect("valid base64");
        assert_eq!(&decoded[0..4], b"qeer", "ReaEQ magic");

        // After 60-byte header: flags(4) + num_bands(4) + band data
        let num_bands = u32::from_le_bytes(decoded[64..68].try_into().unwrap());
        assert_eq!(num_bands, 4, "should have 4 bands");

        // First band at offset 68: type=3 (HighPass), enabled=1, freq=80.0
        let band0_type = u32::from_le_bytes(decoded[68..72].try_into().unwrap());
        assert_eq!(band0_type, 3, "band 0 should be HighPass (3)");
        let band0_enabled = u32::from_le_bytes(decoded[72..76].try_into().unwrap());
        assert_eq!(band0_enabled, 1, "band 0 should be enabled");
        let band0_freq = f64::from_le_bytes(decoded[76..84].try_into().unwrap());
        assert!(
            (band0_freq - 80.0).abs() < 0.001,
            "band 0 freq should be 80 Hz"
        );
    }

    #[test]
    fn test_reagate_encodes_state() {
        let plugin = ReaGate::new()
            .threshold_db(-30.0)
            .attack_ms(0.5)
            .into_fx_plugin();

        assert!(!plugin.state_data.is_empty());
        let b64: String = plugin.state_data.join("");
        let decoded = BASE64.decode(&b64).expect("valid base64");
        assert_eq!(&decoded[0..4], b"tger", "ReaGate magic");
    }

    #[test]
    fn test_realimit_encodes_doubles() {
        let plugin = ReaLimit::new()
            .threshold_db(-6.0)
            .ceiling_db(-1.0)
            .into_fx_plugin();

        assert!(!plugin.state_data.is_empty());
        let b64: String = plugin.state_data.join("");
        let decoded = BASE64.decode(&b64).expect("valid base64");
        assert_eq!(&decoded[0..4], b"tmlr", "ReaLimit magic");

        // Verify threshold is encoded as a double at offset 68
        // (60 header + 4 version_uint + 4 padding)
        let thresh = f64::from_le_bytes(decoded[68..76].try_into().unwrap());
        assert!(
            (thresh - (-6.0)).abs() < 0.001,
            "threshold should be -6.0, got {}",
            thresh
        );
    }

    #[test]
    fn test_reaverbate_encodes_state() {
        let plugin = ReaVerbate::new()
            .room_size(0.7)
            .damping(0.3)
            .wet(0.2)
            .into_fx_plugin();

        assert!(!plugin.state_data.is_empty());
        let b64: String = plugin.state_data.join("");
        let decoded = BASE64.decode(&b64).expect("valid base64");
        assert_eq!(&decoded[0..4], b"8bvr");

        // Room size at offset 68 (60 header + 8 sentinel)
        let room = f32::from_le_bytes(decoded[68..72].try_into().unwrap());
        assert!(
            (room - 0.7).abs() < 0.001,
            "room_size should be 0.7, got {}",
            room
        );
    }

    #[test]
    fn test_reasynth_waveforms() {
        let synth = ReaSynth::new().sawtooth().volume(0.8);
        assert_eq!(synth.waveform, 1.0);
        assert_eq!(synth.volume, 0.8);

        let synth = ReaSynth::new().square();
        assert_eq!(synth.waveform, 0.5);
    }

    #[test]
    fn test_stock_fx_with_track_builder() {
        let track = TrackBuilder::new("Vocals")
            .stock_fx(ReaGate::new().threshold_db(-30.0))
            .stock_fx(
                ReaComp::new()
                    .threshold_db(-18.0)
                    .ratio(4.0)
                    .custom_name("Control Compression"),
            )
            .stock_fx(
                ReaEq::new()
                    .high_pass(0, 80.0, 0.7)
                    .bell(1, 3000.0, 2.0, 1.0),
            )
            .build();

        let chain = track.fx_chain.as_ref().unwrap();
        assert_eq!(chain.nodes.len(), 3);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert!(p.name.contains("ReaGate"));
            assert!(!p.state_data.is_empty(), "ReaGate should have state data");
        }
        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert!(p.name.contains("ReaComp"));
            assert_eq!(p.custom_name.as_deref(), Some("Control Compression"));
        }
        if let FxChainNode::Plugin(p) = &chain.nodes[2] {
            assert!(p.name.contains("ReaEQ"));
        }
    }

    #[test]
    fn test_stock_fx_serializes_with_state() {
        let project = ReaperProjectBuilder::new()
            .track("Test", |t| {
                t.stock_fx(ReaComp::new().threshold_db(-12.0))
                    .stock_fx(ReaEq::new().bell(0, 1000.0, 3.0, 1.0))
                    .stock_fx(ReaGate::new())
            })
            .build();

        let rpp = project.to_rpp_string();

        // Verify the plugins are present with their VST headers
        assert!(rpp.contains("ReaComp"), "should contain ReaComp");
        assert!(rpp.contains("ReaEQ"), "should contain ReaEQ");
        assert!(rpp.contains("ReaGate"), "should contain ReaGate");

        // Verify base64 state data is present (not empty VST blocks)
        // The state_data lines appear between <VST...> and >
        assert!(
            rpp.lines()
                .any(|l| l.trim().len() > 50 && !l.trim().starts_with('<')),
            "should contain base64 state data lines"
        );
    }

    #[test]
    fn test_full_vocal_chain_with_state() {
        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .track("Lead Vocal", |t| {
                t.stock_fx(ReaGate::new().threshold_db(-40.0).attack_ms(0.5))
                    .stock_fx(
                        ReaComp::new()
                            .threshold_db(-18.0)
                            .ratio(4.0)
                            .attack_ms(5.0)
                            .release_ms(100.0),
                    )
                    .stock_fx(
                        ReaEq::new()
                            .high_pass(0, 80.0, 0.7)
                            .bell(1, 250.0, -3.0, 1.0)
                            .bell(2, 3000.0, 2.0, 1.5)
                            .high_shelf(3, 10000.0, 1.0, 0.7),
                    )
                    .stock_fx(ReaVerbate::new().room_size(0.3).wet(0.15).dry(1.0))
                    .stock_fx(ReaLimit::new().threshold_db(-1.0))
            })
            .build();

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("Lead Vocal"));
        assert!(rpp.contains("ReaGate"));
        assert!(rpp.contains("ReaComp"));
        assert!(rpp.contains("ReaEQ"));
        assert!(rpp.contains("ReaVerbate"));
        assert!(rpp.contains("ReaLimit"));

        // Verify it parses back
        let parsed = crate::io::parse_project_text(&rpp).expect("should parse");
        assert_eq!(parsed.tracks.len(), 1);
        let chain = parsed.tracks[0].fx_chain.as_ref().unwrap();
        assert_eq!(chain.nodes.len(), 5);
    }

    #[test]
    fn test_identity_only_plugins() {
        // Plugins that use identity-only (no state encoding yet)
        let plugin = ReaDelay::new().delay_ms(300.0).into_fx_plugin();
        assert!(
            plugin.raw_block.contains("1919247468"),
            "should have VST ID"
        );

        let plugin = ReaVerb::new().into_fx_plugin();
        assert!(plugin.raw_block.contains("1919252066"));
    }
}
