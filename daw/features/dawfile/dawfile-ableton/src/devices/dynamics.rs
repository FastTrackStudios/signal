//! Dynamics device typed parameters (Compressor, Glue Compressor, Gate, Limiter, Multiband Dynamics).

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

// ─── Helper macros ─────────────────────────────────────────────────────────

/// Read `<Tag><Manual Value="..."/></Tag>` as f64.
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

// ─── Compressor ────────────────────────────────────────────────────────────

/// Compressor parameters. Maps to REAPER's ReaComp.
#[derive(Debug, Clone)]
pub struct CompressorParams {
    /// Threshold in dB.
    pub threshold: f64,
    /// Ratio (1.0 = 1:1).
    pub ratio: f64,
    /// Attack in ms.
    pub attack: f64,
    /// Release in ms.
    pub release: f64,
    /// Auto release.
    pub auto_release: bool,
    /// Makeup gain in dB.
    pub gain: f64,
    /// Automatic gain compensation.
    pub gain_compensation: bool,
    /// Dry/wet mix (0.0-1.0).
    pub dry_wet: f64,
    /// 0=Peak, 1=RMS, 2=Expand.
    pub model: i32,
    /// Knee.
    pub knee: f64,
    /// 0=off, 1=1ms, 2=10ms.
    pub look_ahead: i32,
    /// Expansion ratio.
    pub expansion_ratio: f64,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            ratio: 1.0,
            attack: 10.0,
            release: 100.0,
            auto_release: false,
            gain: 0.0,
            gain_compensation: false,
            dry_wet: 1.0,
            model: 0,
            knee: 6.0,
            look_ahead: 0,
            expansion_ratio: 2.0,
        }
    }
}

pub fn parse_compressor(node: Node<'_, '_>) -> CompressorParams {
    CompressorParams {
        threshold: param_f64(node, "Threshold", 0.0),
        ratio: param_f64(node, "Ratio", 1.0),
        attack: param_f64(node, "Attack", 10.0),
        release: param_f64(node, "Release", 100.0),
        auto_release: param_bool(node, "AutoReleaseControlOnOff", false),
        gain: param_f64(node, "Gain", 0.0),
        gain_compensation: param_bool(node, "GainCompensation", false),
        dry_wet: param_f64(node, "DryWet", 1.0),
        model: param_i32(node, "Model", 0),
        knee: param_f64(node, "Knee", 6.0),
        look_ahead: param_i32(node, "LookAhead", 0),
        expansion_ratio: param_f64(node, "ExpansionRatio", 2.0),
    }
}

pub fn write_compressor<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &CompressorParams,
) -> io::Result<()> {
    write_param_f64(w, "Threshold", p.threshold)?;
    write_param_f64(w, "Ratio", p.ratio)?;
    write_param_f64(w, "Attack", p.attack)?;
    write_param_f64(w, "Release", p.release)?;
    write_param_bool(w, "AutoReleaseControlOnOff", p.auto_release)?;
    write_param_f64(w, "Gain", p.gain)?;
    write_param_bool(w, "GainCompensation", p.gain_compensation)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_i32(w, "Model", p.model)?;
    write_param_f64(w, "Knee", p.knee)?;
    write_param_i32(w, "LookAhead", p.look_ahead)?;
    write_param_f64(w, "ExpansionRatio", p.expansion_ratio)?;
    Ok(())
}

// ─── Glue Compressor ───────────────────────────────────────────────────────

/// Glue Compressor parameters.
#[derive(Debug, Clone)]
pub struct GlueCompressorParams {
    pub threshold: f64,
    pub range: f64,
    pub makeup: f64,
    pub attack: f64,
    pub ratio: f64,
    pub release: f64,
    pub dry_wet: f64,
    pub peak_clip_in: bool,
    pub oversample: bool,
}

impl Default for GlueCompressorParams {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            range: -40.0,
            makeup: 0.0,
            attack: 0.01,
            ratio: 2.0,
            release: 0.1,
            dry_wet: 1.0,
            peak_clip_in: false,
            oversample: false,
        }
    }
}

pub fn parse_glue_compressor(node: Node<'_, '_>) -> GlueCompressorParams {
    GlueCompressorParams {
        threshold: param_f64(node, "Threshold", 0.0),
        range: param_f64(node, "Range", -40.0),
        makeup: param_f64(node, "Makeup", 0.0),
        attack: param_f64(node, "Attack", 0.01),
        ratio: param_f64(node, "Ratio", 2.0),
        release: param_f64(node, "Release", 0.1),
        dry_wet: param_f64(node, "DryWet", 1.0),
        peak_clip_in: param_bool(node, "PeakClipIn", false),
        oversample: param_bool(node, "Oversample", false),
    }
}

pub fn write_glue_compressor<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &GlueCompressorParams,
) -> io::Result<()> {
    write_param_f64(w, "Threshold", p.threshold)?;
    write_param_f64(w, "Range", p.range)?;
    write_param_f64(w, "Makeup", p.makeup)?;
    write_param_f64(w, "Attack", p.attack)?;
    write_param_f64(w, "Ratio", p.ratio)?;
    write_param_f64(w, "Release", p.release)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "PeakClipIn", p.peak_clip_in)?;
    write_param_bool(w, "Oversample", p.oversample)?;
    Ok(())
}

// ─── Gate ──────────────────────────────────────────────────────────────────

/// Gate parameters. Maps to REAPER's ReaGate.
#[derive(Debug, Clone)]
pub struct GateParams {
    pub threshold: f64,
    pub attack: f64,
    pub hold: f64,
    pub release: f64,
    /// Hysteresis (return dB).
    pub return_db: f64,
    pub gain: f64,
    pub flip_mode: i32,
    pub look_ahead: i32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            threshold: -30.0,
            attack: 0.01,
            hold: 100.0,
            release: 50.0,
            return_db: 0.0,
            gain: 0.0,
            flip_mode: 0,
            look_ahead: 0,
        }
    }
}

pub fn parse_gate(node: Node<'_, '_>) -> GateParams {
    GateParams {
        threshold: param_f64(node, "Threshold", -30.0),
        attack: param_f64(node, "Attack", 0.01),
        hold: param_f64(node, "Hold", 100.0),
        release: param_f64(node, "Release", 50.0),
        return_db: param_f64(node, "Return", 0.0),
        gain: param_f64(node, "Gain", 0.0),
        flip_mode: param_i32(node, "FlipMode", 0),
        look_ahead: param_i32(node, "LookAhead", 0),
    }
}

pub fn write_gate<W: Write>(w: &mut AbletonXmlWriter<W>, p: &GateParams) -> io::Result<()> {
    write_param_f64(w, "Threshold", p.threshold)?;
    write_param_f64(w, "Attack", p.attack)?;
    write_param_f64(w, "Hold", p.hold)?;
    write_param_f64(w, "Release", p.release)?;
    write_param_f64(w, "Return", p.return_db)?;
    write_param_f64(w, "Gain", p.gain)?;
    write_param_i32(w, "FlipMode", p.flip_mode)?;
    write_param_i32(w, "LookAhead", p.look_ahead)?;
    Ok(())
}

// ─── Limiter ───────────────────────────────────────────────────────────────

/// Limiter parameters. Maps to REAPER's ReaLimit.
#[derive(Debug, Clone)]
pub struct LimiterParams {
    pub gain: f64,
    pub ceiling: f64,
    pub release: f64,
    pub auto_release: bool,
    pub link_amount: f64,
    pub lookahead: i32,
    pub mode: i32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            gain: 0.0,
            ceiling: 0.0,
            release: 300.0,
            auto_release: true,
            link_amount: 100.0,
            lookahead: 1,
            mode: 0,
        }
    }
}

pub fn parse_limiter(node: Node<'_, '_>) -> LimiterParams {
    LimiterParams {
        gain: param_f64(node, "Gain", 0.0),
        ceiling: param_f64(node, "Ceiling", 0.0),
        release: param_f64(node, "Release", 300.0),
        auto_release: param_bool(node, "AutoRelease", true),
        link_amount: param_f64(node, "LinkAmount", 100.0),
        lookahead: param_i32(node, "Lookahead", 1),
        mode: param_i32(node, "Mode", 0),
    }
}

pub fn write_limiter<W: Write>(w: &mut AbletonXmlWriter<W>, p: &LimiterParams) -> io::Result<()> {
    write_param_f64(w, "Gain", p.gain)?;
    write_param_f64(w, "Ceiling", p.ceiling)?;
    write_param_f64(w, "Release", p.release)?;
    write_param_bool(w, "AutoRelease", p.auto_release)?;
    write_param_f64(w, "LinkAmount", p.link_amount)?;
    write_param_i32(w, "Lookahead", p.lookahead)?;
    write_param_i32(w, "Mode", p.mode)?;
    Ok(())
}

// ─── Multiband Dynamics ────────────────────────────────────────────────────

/// Multiband Dynamics parameters. Maps to REAPER's ReaXcomp.
#[derive(Debug, Clone)]
pub struct MultibandDynamicsParams {
    /// Low-mid crossover frequency in Hz.
    pub split_low_mid: f64,
    /// Mid-high crossover frequency in Hz.
    pub split_mid_high: f64,
    pub split_low_mid_on: bool,
    pub split_mid_high_on: bool,
    pub soft_knee: bool,
    pub output_gain: f64,
    pub global_amount: f64,
    pub global_time: f64,
    /// Low, Mid, High bands.
    pub bands: [MbdBand; 3],
}

/// A single band within the Multiband Dynamics device.
#[derive(Debug, Clone)]
pub struct MbdBand {
    pub gain: f64,
    pub input_gain: f64,
    pub above_threshold: f64,
    pub below_threshold: f64,
    pub above_ratio: f64,
    pub below_ratio: f64,
    pub attack: f64,
    pub release: f64,
    pub active: bool,
    pub solo: bool,
}

impl Default for MbdBand {
    fn default() -> Self {
        Self {
            gain: 0.0,
            input_gain: 0.0,
            above_threshold: 0.0,
            below_threshold: -30.0,
            above_ratio: 1.0,
            below_ratio: 1.0,
            attack: 10.0,
            release: 100.0,
            active: true,
            solo: false,
        }
    }
}

impl Default for MultibandDynamicsParams {
    fn default() -> Self {
        Self {
            split_low_mid: 120.0,
            split_mid_high: 2500.0,
            split_low_mid_on: true,
            split_mid_high_on: true,
            soft_knee: false,
            output_gain: 0.0,
            global_amount: 100.0,
            global_time: 100.0,
            bands: Default::default(),
        }
    }
}

/// Band suffix names used in Ableton's flat parameter naming.
const BAND_SUFFIXES: [&str; 3] = ["Low", "Mid", "High"];

pub fn parse_multiband_dynamics(node: Node<'_, '_>) -> MultibandDynamicsParams {
    let mut params = MultibandDynamicsParams {
        split_low_mid: param_f64(node, "SplitLowMid", 120.0),
        split_mid_high: param_f64(node, "SplitMidHigh", 2500.0),
        split_low_mid_on: param_bool(node, "SplitLowMidOn", true),
        split_mid_high_on: param_bool(node, "SplitMidHighOn", true),
        soft_knee: param_bool(node, "SoftKnee", false),
        output_gain: param_f64(node, "OutputGain", 0.0),
        global_amount: param_f64(node, "GlobalAmount", 100.0),
        global_time: param_f64(node, "GlobalTime", 100.0),
        ..Default::default()
    };

    for (i, suffix) in BAND_SUFFIXES.iter().enumerate() {
        params.bands[i] = MbdBand {
            gain: param_f64(node, &format!("Gain{suffix}"), 0.0),
            input_gain: param_f64(node, &format!("InputGain{suffix}"), 0.0),
            above_threshold: param_f64(node, &format!("AboveThreshold{suffix}"), 0.0),
            below_threshold: param_f64(node, &format!("BelowThreshold{suffix}"), -30.0),
            above_ratio: param_f64(node, &format!("AboveRatio{suffix}"), 1.0),
            below_ratio: param_f64(node, &format!("BelowRatio{suffix}"), 1.0),
            attack: param_f64(node, &format!("Attack{suffix}"), 10.0),
            release: param_f64(node, &format!("Release{suffix}"), 100.0),
            active: param_bool(node, &format!("Band{suffix}On"), true),
            solo: param_bool(node, &format!("Band{suffix}Solo"), false),
        };
    }

    params
}

pub fn write_multiband_dynamics<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &MultibandDynamicsParams,
) -> io::Result<()> {
    write_param_f64(w, "SplitLowMid", p.split_low_mid)?;
    write_param_f64(w, "SplitMidHigh", p.split_mid_high)?;
    write_param_bool(w, "SplitLowMidOn", p.split_low_mid_on)?;
    write_param_bool(w, "SplitMidHighOn", p.split_mid_high_on)?;
    write_param_bool(w, "SoftKnee", p.soft_knee)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_f64(w, "GlobalAmount", p.global_amount)?;
    write_param_f64(w, "GlobalTime", p.global_time)?;

    for (i, suffix) in BAND_SUFFIXES.iter().enumerate() {
        let b = &p.bands[i];
        write_param_f64(w, &format!("Gain{suffix}"), b.gain)?;
        write_param_f64(w, &format!("InputGain{suffix}"), b.input_gain)?;
        write_param_f64(w, &format!("AboveThreshold{suffix}"), b.above_threshold)?;
        write_param_f64(w, &format!("BelowThreshold{suffix}"), b.below_threshold)?;
        write_param_f64(w, &format!("AboveRatio{suffix}"), b.above_ratio)?;
        write_param_f64(w, &format!("BelowRatio{suffix}"), b.below_ratio)?;
        write_param_f64(w, &format!("Attack{suffix}"), b.attack)?;
        write_param_f64(w, &format!("Release{suffix}"), b.release)?;
        write_param_bool(w, &format!("Band{suffix}On"), b.active)?;
        write_param_bool(w, &format!("Band{suffix}Solo"), b.solo)?;
    }

    Ok(())
}
