//! Modulation device typed parameters (Auto Filter, Chorus, Chorus Legacy, Phaser, PhaserNew, Flanger, Saturator).

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

// ─── Auto Filter ───────────────────────────────────────────────────────────

/// Auto Filter parameters.
#[derive(Debug, Clone)]
pub struct AutoFilterParams {
    pub filter_type: i32,
    pub circuit_lp_hp: i32,
    pub circuit_bp_no_mo: i32,
    /// true = 24dB, false = 12dB.
    pub slope: bool,
    /// Cutoff frequency in Hz.
    pub cutoff: f64,
    pub resonance: f64,
    pub morph: f64,
    pub drive: f64,
    pub env_amount: f64,
    pub env_attack: f64,
    pub env_release: f64,
    pub lfo_amount: f64,
    pub lfo_type: i32,
    pub lfo_frequency: f64,
    pub lfo_sync: bool,
    pub lfo_beat_rate: f64,
    pub lfo_phase: f64,
    pub lfo_offset: f64,
    pub lfo_spin: f64,
}

impl Default for AutoFilterParams {
    fn default() -> Self {
        Self {
            filter_type: 0,
            circuit_lp_hp: 0,
            circuit_bp_no_mo: 0,
            slope: false,
            cutoff: 1000.0,
            resonance: 0.5,
            morph: 0.0,
            drive: 0.0,
            env_amount: 0.0,
            env_attack: 10.0,
            env_release: 100.0,
            lfo_amount: 0.0,
            lfo_type: 0,
            lfo_frequency: 1.0,
            lfo_sync: false,
            lfo_beat_rate: 4.0,
            lfo_phase: 0.0,
            lfo_offset: 0.0,
            lfo_spin: 0.0,
        }
    }
}

pub fn parse_auto_filter(node: Node<'_, '_>) -> AutoFilterParams {
    AutoFilterParams {
        filter_type: param_i32(node, "FilterType", 0),
        circuit_lp_hp: param_i32(node, "CircuitLpHp", 0),
        circuit_bp_no_mo: param_i32(node, "CircuitBpNoMo", 0),
        slope: param_bool(node, "Slope", false),
        cutoff: param_f64(node, "Cutoff", 1000.0),
        resonance: param_f64(node, "Resonance", 0.5),
        morph: param_f64(node, "Morph", 0.0),
        drive: param_f64(node, "Drive", 0.0),
        env_amount: param_f64(node, "EnvAmount", 0.0),
        env_attack: param_f64(node, "EnvAttack", 10.0),
        env_release: param_f64(node, "EnvRelease", 100.0),
        lfo_amount: param_f64(node, "LfoAmount", 0.0),
        lfo_type: param_i32(node, "LfoType", 0),
        lfo_frequency: param_f64(node, "LfoFrequency", 1.0),
        lfo_sync: param_bool(node, "LfoSync", false),
        lfo_beat_rate: param_f64(node, "LfoBeatRate", 4.0),
        lfo_phase: param_f64(node, "LfoPhase", 0.0),
        lfo_offset: param_f64(node, "LfoOffset", 0.0),
        lfo_spin: param_f64(node, "LfoSpin", 0.0),
    }
}

pub fn write_auto_filter<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &AutoFilterParams,
) -> io::Result<()> {
    write_param_i32(w, "FilterType", p.filter_type)?;
    write_param_i32(w, "CircuitLpHp", p.circuit_lp_hp)?;
    write_param_i32(w, "CircuitBpNoMo", p.circuit_bp_no_mo)?;
    write_param_bool(w, "Slope", p.slope)?;
    write_param_f64(w, "Cutoff", p.cutoff)?;
    write_param_f64(w, "Resonance", p.resonance)?;
    write_param_f64(w, "Morph", p.morph)?;
    write_param_f64(w, "Drive", p.drive)?;
    write_param_f64(w, "EnvAmount", p.env_amount)?;
    write_param_f64(w, "EnvAttack", p.env_attack)?;
    write_param_f64(w, "EnvRelease", p.env_release)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    write_param_i32(w, "LfoType", p.lfo_type)?;
    write_param_f64(w, "LfoFrequency", p.lfo_frequency)?;
    write_param_bool(w, "LfoSync", p.lfo_sync)?;
    write_param_f64(w, "LfoBeatRate", p.lfo_beat_rate)?;
    write_param_f64(w, "LfoPhase", p.lfo_phase)?;
    write_param_f64(w, "LfoOffset", p.lfo_offset)?;
    write_param_f64(w, "LfoSpin", p.lfo_spin)?;
    Ok(())
}

// ─── Chorus ────────────────────────────────────────────────────────────────

/// Chorus-Ensemble parameters.
#[derive(Debug, Clone)]
pub struct ChorusParams {
    pub mode: i32,
    pub shaping: f64,
    pub rate: f64,
    pub amount: f64,
    pub feedback: f64,
    pub invert_feedback: bool,
    pub vibrato_offset: f64,
    pub highpass_enabled: bool,
    pub highpass_frequency: f64,
    pub width: f64,
    pub warmth: f64,
    pub output_gain: f64,
    pub dry_wet: f64,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self {
            mode: 0,
            shaping: 0.0,
            rate: 1.0,
            amount: 50.0,
            feedback: 0.0,
            invert_feedback: false,
            vibrato_offset: 0.0,
            highpass_enabled: false,
            highpass_frequency: 200.0,
            width: 1.0,
            warmth: 0.0,
            output_gain: 0.0,
            dry_wet: 0.5,
        }
    }
}

pub fn parse_chorus(node: Node<'_, '_>) -> ChorusParams {
    ChorusParams {
        mode: param_i32(node, "Mode", 0),
        shaping: param_f64(node, "Shaping", 0.0),
        rate: param_f64(node, "Rate", 1.0),
        amount: param_f64(node, "Amount", 50.0),
        feedback: param_f64(node, "Feedback", 0.0),
        invert_feedback: param_bool(node, "InvertFeedback", false),
        vibrato_offset: param_f64(node, "VibratoOffset", 0.0),
        highpass_enabled: param_bool(node, "HighpassEnabled", false),
        highpass_frequency: param_f64(node, "HighpassFrequency", 200.0),
        width: param_f64(node, "Width", 1.0),
        warmth: param_f64(node, "Warmth", 0.0),
        output_gain: param_f64(node, "OutputGain", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
    }
}

pub fn write_chorus<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ChorusParams) -> io::Result<()> {
    write_param_i32(w, "Mode", p.mode)?;
    write_param_f64(w, "Shaping", p.shaping)?;
    write_param_f64(w, "Rate", p.rate)?;
    write_param_f64(w, "Amount", p.amount)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_bool(w, "InvertFeedback", p.invert_feedback)?;
    write_param_f64(w, "VibratoOffset", p.vibrato_offset)?;
    write_param_bool(w, "HighpassEnabled", p.highpass_enabled)?;
    write_param_f64(w, "HighpassFrequency", p.highpass_frequency)?;
    write_param_f64(w, "Width", p.width)?;
    write_param_f64(w, "Warmth", p.warmth)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Phaser ────────────────────────────────────────────────────────────────

/// Phaser parameters.
#[derive(Debug, Clone)]
pub struct PhaserParams {
    pub dry_wet: f64,
    pub pole_count: f64,
    pub q: f64,
    pub center_frequency: f64,
    pub feedback: f64,
    pub lfo_amount: f64,
}

impl Default for PhaserParams {
    fn default() -> Self {
        Self {
            dry_wet: 0.5,
            pole_count: 4.0,
            q: 0.7,
            center_frequency: 1000.0,
            feedback: 0.0,
            lfo_amount: 0.5,
        }
    }
}

pub fn parse_phaser(node: Node<'_, '_>) -> PhaserParams {
    PhaserParams {
        dry_wet: param_f64(node, "DryWet", 0.5),
        pole_count: param_f64(node, "PoleCount", 4.0),
        q: param_f64(node, "Q", 0.7),
        center_frequency: param_f64(node, "CenterFrequency", 1000.0),
        feedback: param_f64(node, "Feedback", 0.0),
        lfo_amount: param_f64(node, "LfoAmount", 0.5),
    }
}

pub fn write_phaser<W: Write>(w: &mut AbletonXmlWriter<W>, p: &PhaserParams) -> io::Result<()> {
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "PoleCount", p.pole_count)?;
    write_param_f64(w, "Q", p.q)?;
    write_param_f64(w, "CenterFrequency", p.center_frequency)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    Ok(())
}

// ─── Flanger ───────────────────────────────────────────────────────────────

/// Flanger parameters.
#[derive(Debug, Clone)]
pub struct FlangerParams {
    pub dry_wet: f64,
    pub delay_time: f64,
    pub feedback: f64,
    pub feedback_sign: bool,
    pub lfo_amount: f64,
    pub hipass: f64,
    pub hi_quality: bool,
}

impl Default for FlangerParams {
    fn default() -> Self {
        Self {
            dry_wet: 0.5,
            delay_time: 1.0,
            feedback: 0.5,
            feedback_sign: true,
            lfo_amount: 0.5,
            hipass: 0.0,
            hi_quality: false,
        }
    }
}

pub fn parse_flanger(node: Node<'_, '_>) -> FlangerParams {
    FlangerParams {
        dry_wet: param_f64(node, "DryWet", 0.5),
        delay_time: param_f64(node, "DelayTime", 1.0),
        feedback: param_f64(node, "Feedback", 0.5),
        feedback_sign: param_bool(node, "FeedbackSign", true),
        lfo_amount: param_f64(node, "LfoAmount", 0.5),
        hipass: param_f64(node, "Hipass", 0.0),
        hi_quality: param_bool(node, "HiQuality", false),
    }
}

pub fn write_flanger<W: Write>(w: &mut AbletonXmlWriter<W>, p: &FlangerParams) -> io::Result<()> {
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "DelayTime", p.delay_time)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_bool(w, "FeedbackSign", p.feedback_sign)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    write_param_f64(w, "Hipass", p.hipass)?;
    write_param_bool(w, "HiQuality", p.hi_quality)?;
    Ok(())
}

// ─── Saturator ─────────────────────────────────────────────────────────────

/// Saturator parameters.
#[derive(Debug, Clone)]
pub struct SaturatorParams {
    pub pre_drive: f64,
    pub drive_type: i32,
    pub base_drive: f64,
    pub color_on: bool,
    pub color_frequency: f64,
    pub color_width: f64,
    pub color_depth: f64,
    pub post_drive: f64,
    pub dry_wet: f64,
    pub oversampling: bool,
}

impl Default for SaturatorParams {
    fn default() -> Self {
        Self {
            pre_drive: 0.0,
            drive_type: 0,
            base_drive: 0.0,
            color_on: false,
            color_frequency: 1000.0,
            color_width: 50.0,
            color_depth: 0.0,
            post_drive: 0.0,
            dry_wet: 1.0,
            oversampling: false,
        }
    }
}

pub fn parse_saturator(node: Node<'_, '_>) -> SaturatorParams {
    SaturatorParams {
        pre_drive: param_f64(node, "PreDrive", 0.0),
        drive_type: param_i32(node, "DriveType", 0),
        base_drive: param_f64(node, "BaseDrive", 0.0),
        color_on: param_bool(node, "ColorOn", false),
        color_frequency: param_f64(node, "ColorFrequency", 1000.0),
        color_width: param_f64(node, "ColorWidth", 50.0),
        color_depth: param_f64(node, "ColorDepth", 0.0),
        post_drive: param_f64(node, "PostDrive", 0.0),
        dry_wet: param_f64(node, "DryWet", 1.0),
        oversampling: param_bool(node, "Oversampling", false),
    }
}

pub fn write_saturator<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &SaturatorParams,
) -> io::Result<()> {
    write_param_f64(w, "PreDrive", p.pre_drive)?;
    write_param_i32(w, "DriveType", p.drive_type)?;
    write_param_f64(w, "BaseDrive", p.base_drive)?;
    write_param_bool(w, "ColorOn", p.color_on)?;
    write_param_f64(w, "ColorFrequency", p.color_frequency)?;
    write_param_f64(w, "ColorWidth", p.color_width)?;
    write_param_f64(w, "ColorDepth", p.color_depth)?;
    write_param_f64(w, "PostDrive", p.post_drive)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "Oversampling", p.oversampling)?;
    Ok(())
}

// ─── Chorus Legacy ────────────────────────────────────────────────────────

/// Legacy Chorus parameters (XML tag `Chorus`).
#[derive(Debug, Clone)]
pub struct ChorusLegacyParams {
    pub delay_time1: f64,
    pub high_pass_freq: f64,
    pub delay_time2: f64,
    pub delay2_mode: i32,
    pub link: bool,
    pub mod_amount: f64,
    pub mod_freq: f64,
    pub lfo_multiplier: bool,
    pub feedback: f64,
    pub feedback_sign: i32,
    pub dry_wet: f64,
    pub hi_quality: bool,
}

impl Default for ChorusLegacyParams {
    fn default() -> Self {
        Self {
            delay_time1: 1.0,
            high_pass_freq: 20.0,
            delay_time2: 1.0,
            delay2_mode: 0,
            link: true,
            mod_amount: 0.5,
            mod_freq: 1.0,
            lfo_multiplier: false,
            feedback: 0.0,
            feedback_sign: 0,
            dry_wet: 0.5,
            hi_quality: false,
        }
    }
}

pub fn parse_chorus_legacy(node: Node<'_, '_>) -> ChorusLegacyParams {
    ChorusLegacyParams {
        delay_time1: param_f64(node, "DelayTime1", 1.0),
        high_pass_freq: param_f64(node, "HighPassFreq", 20.0),
        delay_time2: param_f64(node, "DelayTime2", 1.0),
        delay2_mode: param_i32(node, "Delay2Mode", 0),
        link: param_bool(node, "Link", true),
        mod_amount: param_f64(node, "ModAmount", 0.5),
        mod_freq: param_f64(node, "ModFreq", 1.0),
        lfo_multiplier: param_bool(node, "LfoMultiplier", false),
        feedback: param_f64(node, "Feedback", 0.0),
        feedback_sign: param_i32(node, "FeedbackSign", 0),
        dry_wet: param_f64(node, "DryWet", 0.5),
        hi_quality: param_bool(node, "HiQuality", false),
    }
}

pub fn write_chorus_legacy<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &ChorusLegacyParams,
) -> io::Result<()> {
    write_param_f64(w, "DelayTime1", p.delay_time1)?;
    write_param_f64(w, "HighPassFreq", p.high_pass_freq)?;
    write_param_f64(w, "DelayTime2", p.delay_time2)?;
    write_param_i32(w, "Delay2Mode", p.delay2_mode)?;
    write_param_bool(w, "Link", p.link)?;
    write_param_f64(w, "ModAmount", p.mod_amount)?;
    write_param_f64(w, "ModFreq", p.mod_freq)?;
    write_param_bool(w, "LfoMultiplier", p.lfo_multiplier)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_i32(w, "FeedbackSign", p.feedback_sign)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "HiQuality", p.hi_quality)?;
    Ok(())
}

// ─── PhaserNew ────────────────────────────────────────────────────────────

/// PhaserNew parameters (Live 12 Phaser-Flanger).
#[derive(Debug, Clone)]
pub struct PhaserNewParams {
    // Modulation
    pub modulation_amount: f64,
    pub modulation_waveform: i32,
    pub modulation_frequency: f64,
    pub modulation_frequency2: f64,
    pub modulation_sync: bool,
    pub modulation_sync2: bool,
    pub modulation_synced_rate: f64,
    pub modulation_synced_rate2: f64,
    pub modulation_phase_offset: f64,
    pub modulation_spin_enabled: bool,
    pub modulation_spin: f64,
    pub modulation_duty_cycle: f64,
    pub modulation_lfo_blend: f64,
    pub modulation_envelope_enabled: bool,
    pub modulation_envelope_amount: f64,
    pub modulation_envelope_attack: f64,
    pub modulation_envelope_release: f64,
    // Core
    pub mode: i32,
    pub notches: f64,
    pub flanger_delay_time: f64,
    pub doubler_delay_time: f64,
    pub modulation_blend: f64,
    pub center_frequency: f64,
    pub spread: f64,
    pub feedback: f64,
    pub warmth: f64,
    pub safe_bass_frequency: f64,
    pub invert_wet: bool,
    pub output_gain: f64,
    pub dry_wet: f64,
}

impl Default for PhaserNewParams {
    fn default() -> Self {
        Self {
            modulation_amount: 50.0,
            modulation_waveform: 0,
            modulation_frequency: 1.0,
            modulation_frequency2: 1.0,
            modulation_sync: false,
            modulation_sync2: false,
            modulation_synced_rate: 4.0,
            modulation_synced_rate2: 4.0,
            modulation_phase_offset: 0.0,
            modulation_spin_enabled: false,
            modulation_spin: 0.0,
            modulation_duty_cycle: 50.0,
            modulation_lfo_blend: 0.0,
            modulation_envelope_enabled: false,
            modulation_envelope_amount: 0.0,
            modulation_envelope_attack: 10.0,
            modulation_envelope_release: 100.0,
            mode: 0,
            notches: 4.0,
            flanger_delay_time: 1.0,
            doubler_delay_time: 10.0,
            modulation_blend: 0.0,
            center_frequency: 1000.0,
            spread: 0.5,
            feedback: 0.0,
            warmth: 0.0,
            safe_bass_frequency: 50.0,
            invert_wet: false,
            output_gain: 0.0,
            dry_wet: 0.5,
        }
    }
}

pub fn parse_phaser_new(node: Node<'_, '_>) -> PhaserNewParams {
    PhaserNewParams {
        modulation_amount: param_f64(node, "Modulation_Amount", 50.0),
        modulation_waveform: param_i32(node, "Modulation_Waveform", 0),
        modulation_frequency: param_f64(node, "Modulation_Frequency", 1.0),
        modulation_frequency2: param_f64(node, "Modulation_Frequency2", 1.0),
        modulation_sync: param_bool(node, "Modulation_Sync", false),
        modulation_sync2: param_bool(node, "Modulation_Sync2", false),
        modulation_synced_rate: param_f64(node, "Modulation_SyncedRate", 4.0),
        modulation_synced_rate2: param_f64(node, "Modulation_SyncedRate2", 4.0),
        modulation_phase_offset: param_f64(node, "Modulation_PhaseOffset", 0.0),
        modulation_spin_enabled: param_bool(node, "Modulation_SpinEnabled", false),
        modulation_spin: param_f64(node, "Modulation_Spin", 0.0),
        modulation_duty_cycle: param_f64(node, "Modulation_DutyCycle", 50.0),
        modulation_lfo_blend: param_f64(node, "Modulation_LfoBlend", 0.0),
        modulation_envelope_enabled: param_bool(node, "Modulation_EnvelopeEnabled", false),
        modulation_envelope_amount: param_f64(node, "Modulation_EnvelopeAmount", 0.0),
        modulation_envelope_attack: param_f64(node, "Modulation_EnvelopeAttack", 10.0),
        modulation_envelope_release: param_f64(node, "Modulation_EnvelopeRelease", 100.0),
        mode: param_i32(node, "Mode", 0),
        notches: param_f64(node, "Notches", 4.0),
        flanger_delay_time: param_f64(node, "FlangerDelayTime", 1.0),
        doubler_delay_time: param_f64(node, "DoublerDelayTime", 10.0),
        modulation_blend: param_f64(node, "ModulationBlend", 0.0),
        center_frequency: param_f64(node, "CenterFrequency", 1000.0),
        spread: param_f64(node, "Spread", 0.5),
        feedback: param_f64(node, "Feedback", 0.0),
        warmth: param_f64(node, "Warmth", 0.0),
        safe_bass_frequency: param_f64(node, "SafeBassFrequency", 50.0),
        invert_wet: param_bool(node, "InvertWet", false),
        output_gain: param_f64(node, "OutputGain", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
    }
}

pub fn write_phaser_new<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &PhaserNewParams,
) -> io::Result<()> {
    write_param_f64(w, "Modulation_Amount", p.modulation_amount)?;
    write_param_i32(w, "Modulation_Waveform", p.modulation_waveform)?;
    write_param_f64(w, "Modulation_Frequency", p.modulation_frequency)?;
    write_param_f64(w, "Modulation_Frequency2", p.modulation_frequency2)?;
    write_param_bool(w, "Modulation_Sync", p.modulation_sync)?;
    write_param_bool(w, "Modulation_Sync2", p.modulation_sync2)?;
    write_param_f64(w, "Modulation_SyncedRate", p.modulation_synced_rate)?;
    write_param_f64(w, "Modulation_SyncedRate2", p.modulation_synced_rate2)?;
    write_param_f64(w, "Modulation_PhaseOffset", p.modulation_phase_offset)?;
    write_param_bool(w, "Modulation_SpinEnabled", p.modulation_spin_enabled)?;
    write_param_f64(w, "Modulation_Spin", p.modulation_spin)?;
    write_param_f64(w, "Modulation_DutyCycle", p.modulation_duty_cycle)?;
    write_param_f64(w, "Modulation_LfoBlend", p.modulation_lfo_blend)?;
    write_param_bool(
        w,
        "Modulation_EnvelopeEnabled",
        p.modulation_envelope_enabled,
    )?;
    write_param_f64(w, "Modulation_EnvelopeAmount", p.modulation_envelope_amount)?;
    write_param_f64(w, "Modulation_EnvelopeAttack", p.modulation_envelope_attack)?;
    write_param_f64(
        w,
        "Modulation_EnvelopeRelease",
        p.modulation_envelope_release,
    )?;
    write_param_i32(w, "Mode", p.mode)?;
    write_param_f64(w, "Notches", p.notches)?;
    write_param_f64(w, "FlangerDelayTime", p.flanger_delay_time)?;
    write_param_f64(w, "DoublerDelayTime", p.doubler_delay_time)?;
    write_param_f64(w, "ModulationBlend", p.modulation_blend)?;
    write_param_f64(w, "CenterFrequency", p.center_frequency)?;
    write_param_f64(w, "Spread", p.spread)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_f64(w, "Warmth", p.warmth)?;
    write_param_f64(w, "SafeBassFrequency", p.safe_bass_frequency)?;
    write_param_bool(w, "InvertWet", p.invert_wet)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}
