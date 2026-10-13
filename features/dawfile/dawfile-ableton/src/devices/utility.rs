//! Utility device typed parameters (Utility, Tuner, Cabinet, Erosion, Redux, Redux Legacy, Vinyl).

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

// ─── Helpers ───────────────────────────────────────────────────────────────

fn param_f64(node: Node<'_, '_>, name: &str, default: f64) -> f64 {
    child(node, name)
        .and_then(|n| child_f64(n, "Manual"))
        .unwrap_or(default)
}

fn param_i32(node: Node<'_, '_>, name: &str, default: i32) -> i32 {
    child(node, name)
        .and_then(|n| child_i32(n, "Manual"))
        .unwrap_or(default)
}

fn param_bool(node: Node<'_, '_>, name: &str, default: bool) -> bool {
    child(node, name)
        .and_then(|n| child_bool(n, "Manual"))
        .unwrap_or(default)
}

fn write_param_f64<W: Write>(w: &mut AbletonXmlWriter<W>, tag: &str, value: f64) -> io::Result<()> {
    w.start(tag)?;
    w.value_float("Manual", value)?;
    w.automation_target("AutomationTarget")?;
    w.end(tag)
}

fn write_param_i32<W: Write>(w: &mut AbletonXmlWriter<W>, tag: &str, value: i32) -> io::Result<()> {
    w.start(tag)?;
    w.value_int("Manual", value as i64)?;
    w.automation_target("AutomationTarget")?;
    w.end(tag)
}

fn write_param_bool<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    tag: &str,
    value: bool,
) -> io::Result<()> {
    w.start(tag)?;
    w.value_bool("Manual", value)?;
    w.automation_target("AutomationTarget")?;
    w.end(tag)
}

// ─── Utility (StereoGain) ──────────────────────────────────────────────────

/// Utility (StereoGain) parameters.
#[derive(Debug, Clone)]
pub struct UtilityParams {
    pub phase_invert_l: bool,
    pub phase_invert_r: bool,
    pub channel_mode: i32,
    pub stereo_width: f64,
    pub mid_side_balance: f64,
    pub mono: bool,
    pub bass_mono: bool,
    pub bass_mono_frequency: f64,
    pub balance: f64,
    pub gain: f64,
    pub mute: bool,
    pub dc_filter: bool,
}

impl Default for UtilityParams {
    fn default() -> Self {
        Self {
            phase_invert_l: false,
            phase_invert_r: false,
            channel_mode: 0,
            stereo_width: 100.0,
            mid_side_balance: 0.0,
            mono: false,
            bass_mono: false,
            bass_mono_frequency: 120.0,
            balance: 0.0,
            gain: 0.0,
            mute: false,
            dc_filter: false,
        }
    }
}

pub fn parse_utility(node: Node<'_, '_>) -> UtilityParams {
    UtilityParams {
        phase_invert_l: param_bool(node, "PhaseInvertL", false),
        phase_invert_r: param_bool(node, "PhaseInvertR", false),
        channel_mode: param_i32(node, "ChannelMode", 0),
        stereo_width: param_f64(node, "StereoWidth", 100.0),
        mid_side_balance: param_f64(node, "MidSideBalance", 0.0),
        mono: param_bool(node, "Mono", false),
        bass_mono: param_bool(node, "BassMono", false),
        bass_mono_frequency: param_f64(node, "BassMonoFrequency", 120.0),
        balance: param_f64(node, "Balance", 0.0),
        gain: param_f64(node, "Gain", 0.0),
        mute: param_bool(node, "Mute", false),
        dc_filter: param_bool(node, "DcFilter", false),
    }
}

pub fn write_utility<W: Write>(w: &mut AbletonXmlWriter<W>, p: &UtilityParams) -> io::Result<()> {
    write_param_bool(w, "PhaseInvertL", p.phase_invert_l)?;
    write_param_bool(w, "PhaseInvertR", p.phase_invert_r)?;
    write_param_i32(w, "ChannelMode", p.channel_mode)?;
    write_param_f64(w, "StereoWidth", p.stereo_width)?;
    write_param_f64(w, "MidSideBalance", p.mid_side_balance)?;
    write_param_bool(w, "Mono", p.mono)?;
    write_param_bool(w, "BassMono", p.bass_mono)?;
    write_param_f64(w, "BassMonoFrequency", p.bass_mono_frequency)?;
    write_param_f64(w, "Balance", p.balance)?;
    write_param_f64(w, "Gain", p.gain)?;
    write_param_bool(w, "Mute", p.mute)?;
    write_param_bool(w, "DcFilter", p.dc_filter)?;
    Ok(())
}

// ─── Tuner ─────────────────────────────────────────────────────────────────

/// Tuner parameters.
#[derive(Debug, Clone)]
pub struct TunerParams {
    /// Reference pitch (default 440 Hz).
    pub tuning_freq: f64,
}

impl Default for TunerParams {
    fn default() -> Self {
        Self { tuning_freq: 440.0 }
    }
}

pub fn parse_tuner(node: Node<'_, '_>) -> TunerParams {
    TunerParams {
        tuning_freq: param_f64(node, "TuningFreq", 440.0),
    }
}

pub fn write_tuner<W: Write>(w: &mut AbletonXmlWriter<W>, p: &TunerParams) -> io::Result<()> {
    write_param_f64(w, "TuningFreq", p.tuning_freq)?;
    Ok(())
}

// ─── Cabinet ───────────────────────────────────────────────────────────────

/// Cabinet parameters.
#[derive(Debug, Clone)]
pub struct CabinetParams {
    pub cabinet_type: i32,
    pub microphone_type_switch: bool,
    pub microphone_position: i32,
    pub dual_mono: bool,
    pub dry_wet: f64,
}

impl Default for CabinetParams {
    fn default() -> Self {
        Self {
            cabinet_type: 0,
            microphone_type_switch: false,
            microphone_position: 0,
            dual_mono: false,
            dry_wet: 1.0,
        }
    }
}

pub fn parse_cabinet(node: Node<'_, '_>) -> CabinetParams {
    CabinetParams {
        cabinet_type: param_i32(node, "CabinetType", 0),
        microphone_type_switch: param_bool(node, "MicrophoneTypeSwitch", false),
        microphone_position: param_i32(node, "MicrophonePosition", 0),
        dual_mono: param_bool(node, "DualMono", false),
        dry_wet: param_f64(node, "DryWet", 1.0),
    }
}

pub fn write_cabinet<W: Write>(w: &mut AbletonXmlWriter<W>, p: &CabinetParams) -> io::Result<()> {
    write_param_i32(w, "CabinetType", p.cabinet_type)?;
    write_param_bool(w, "MicrophoneTypeSwitch", p.microphone_type_switch)?;
    write_param_i32(w, "MicrophonePosition", p.microphone_position)?;
    write_param_bool(w, "DualMono", p.dual_mono)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Erosion ───────────────────────────────────────────────────────────────

/// Erosion parameters.
#[derive(Debug, Clone)]
pub struct ErosionParams {
    pub mode: i32,
    pub freq: f64,
    pub amplitude: f64,
    pub band_q: f64,
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            mode: 0,
            freq: 1000.0,
            amplitude: 0.0,
            band_q: 0.7,
        }
    }
}

pub fn parse_erosion(node: Node<'_, '_>) -> ErosionParams {
    ErosionParams {
        mode: param_i32(node, "Mode", 0),
        freq: param_f64(node, "Freq", 1000.0),
        amplitude: param_f64(node, "Amplitude", 0.0),
        band_q: param_f64(node, "BandQ", 0.7),
    }
}

pub fn write_erosion<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ErosionParams) -> io::Result<()> {
    write_param_i32(w, "Mode", p.mode)?;
    write_param_f64(w, "Freq", p.freq)?;
    write_param_f64(w, "Amplitude", p.amplitude)?;
    write_param_f64(w, "BandQ", p.band_q)?;
    Ok(())
}

// ─── Redux ─────────────────────────────────────────────────────────────────

/// Redux parameters.
#[derive(Debug, Clone)]
pub struct ReduxParams {
    pub sample_rate: f64,
    pub jitter: f64,
    pub bit_depth: f64,
    pub dry_wet: f64,
}

impl Default for ReduxParams {
    fn default() -> Self {
        Self {
            sample_rate: 44100.0,
            jitter: 0.0,
            bit_depth: 16.0,
            dry_wet: 1.0,
        }
    }
}

pub fn parse_redux(node: Node<'_, '_>) -> ReduxParams {
    ReduxParams {
        sample_rate: param_f64(node, "SampleRate", 44100.0),
        jitter: param_f64(node, "Jitter", 0.0),
        bit_depth: param_f64(node, "BitDepth", 16.0),
        dry_wet: param_f64(node, "DryWet", 1.0),
    }
}

pub fn write_redux<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ReduxParams) -> io::Result<()> {
    write_param_f64(w, "SampleRate", p.sample_rate)?;
    write_param_f64(w, "Jitter", p.jitter)?;
    write_param_f64(w, "BitDepth", p.bit_depth)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Vinyl Distortion ──────────────────────────────────────────────────────

/// Vinyl Distortion parameters.
#[derive(Debug, Clone)]
pub struct VinylParams {
    pub drive: f64,
    pub crackle_density: f64,
    pub crackle_volume: f64,
    pub band1_on: bool,
    pub gain1: f64,
    pub freq1: f64,
    pub q1: f64,
    pub band2_on: bool,
    pub gain2: f64,
    pub freq2: f64,
    pub q2: f64,
}

impl Default for VinylParams {
    fn default() -> Self {
        Self {
            drive: 0.0,
            crackle_density: 0.0,
            crackle_volume: 0.0,
            band1_on: false,
            gain1: 0.0,
            freq1: 1000.0,
            q1: 0.7,
            band2_on: false,
            gain2: 0.0,
            freq2: 3000.0,
            q2: 0.7,
        }
    }
}

pub fn parse_vinyl(node: Node<'_, '_>) -> VinylParams {
    VinylParams {
        drive: param_f64(node, "Drive", 0.0),
        crackle_density: param_f64(node, "CrackleDensity", 0.0),
        crackle_volume: param_f64(node, "CrackleVolume", 0.0),
        band1_on: param_bool(node, "Band1On", false),
        gain1: param_f64(node, "Gain1", 0.0),
        freq1: param_f64(node, "Freq1", 1000.0),
        q1: param_f64(node, "Q1", 0.7),
        band2_on: param_bool(node, "Band2On", false),
        gain2: param_f64(node, "Gain2", 0.0),
        freq2: param_f64(node, "Freq2", 3000.0),
        q2: param_f64(node, "Q2", 0.7),
    }
}

pub fn write_vinyl<W: Write>(w: &mut AbletonXmlWriter<W>, p: &VinylParams) -> io::Result<()> {
    write_param_f64(w, "Drive", p.drive)?;
    write_param_f64(w, "CrackleDensity", p.crackle_density)?;
    write_param_f64(w, "CrackleVolume", p.crackle_volume)?;
    write_param_bool(w, "Band1On", p.band1_on)?;
    write_param_f64(w, "Gain1", p.gain1)?;
    write_param_f64(w, "Freq1", p.freq1)?;
    write_param_f64(w, "Q1", p.q1)?;
    write_param_bool(w, "Band2On", p.band2_on)?;
    write_param_f64(w, "Gain2", p.gain2)?;
    write_param_f64(w, "Freq2", p.freq2)?;
    write_param_f64(w, "Q2", p.q2)?;
    Ok(())
}

// ─── Redux Legacy ─────────────────────────────────────────────────────────

/// Redux Legacy parameters (XML tag `Redux` — older version, distinct from Redux2).
#[derive(Debug, Clone)]
pub struct ReduxLegacyParams {
    pub bit_depth_on: bool,
    pub bit_depth: f64,
    pub sample_res_mode: bool,
    pub sample_res_rough: f64,
    pub sample_res_soft: f64,
}

impl Default for ReduxLegacyParams {
    fn default() -> Self {
        Self {
            bit_depth_on: true,
            bit_depth: 16.0,
            sample_res_mode: false,
            sample_res_rough: 1.0,
            sample_res_soft: 1.0,
        }
    }
}

pub fn parse_redux_legacy(node: Node<'_, '_>) -> ReduxLegacyParams {
    ReduxLegacyParams {
        bit_depth_on: param_bool(node, "BitDepthOn", true),
        bit_depth: param_f64(node, "BitDepth", 16.0),
        sample_res_mode: param_bool(node, "SampleResMode", false),
        sample_res_rough: param_f64(node, "SampleResRough", 1.0),
        sample_res_soft: param_f64(node, "SampleResSoft", 1.0),
    }
}

pub fn write_redux_legacy<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &ReduxLegacyParams,
) -> io::Result<()> {
    write_param_bool(w, "BitDepthOn", p.bit_depth_on)?;
    write_param_f64(w, "BitDepth", p.bit_depth)?;
    write_param_bool(w, "SampleResMode", p.sample_res_mode)?;
    write_param_f64(w, "SampleResRough", p.sample_res_rough)?;
    write_param_f64(w, "SampleResSoft", p.sample_res_soft)?;
    Ok(())
}
