//! Filter and frequency-domain device typed parameters
//! (ChannelEq, FilterEQ3, AutoPan, FrequencyShifter, Shifter, Spectral, Transmute).

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

// ─── ChannelEq ────────────────────────────────────────────────────────────

/// ChannelEq parameters (simple 3-band channel strip EQ).
#[derive(Debug, Clone)]
pub struct ChannelEqParams {
    pub highpass_on: bool,
    pub low_shelf_gain: f64,
    pub mid_gain: f64,
    pub mid_frequency: f64,
    pub high_shelf_gain: f64,
    pub gain: f64,
}

impl Default for ChannelEqParams {
    fn default() -> Self {
        Self {
            highpass_on: false,
            low_shelf_gain: 0.0,
            mid_gain: 0.0,
            mid_frequency: 1000.0,
            high_shelf_gain: 0.0,
            gain: 0.0,
        }
    }
}

pub fn parse_channel_eq(node: Node<'_, '_>) -> ChannelEqParams {
    ChannelEqParams {
        highpass_on: param_bool(node, "HighpassOn", false),
        low_shelf_gain: param_f64(node, "LowShelfGain", 0.0),
        mid_gain: param_f64(node, "MidGain", 0.0),
        mid_frequency: param_f64(node, "MidFrequency", 1000.0),
        high_shelf_gain: param_f64(node, "HighShelfGain", 0.0),
        gain: param_f64(node, "Gain", 0.0),
    }
}

pub fn write_channel_eq<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &ChannelEqParams,
) -> io::Result<()> {
    write_param_bool(w, "HighpassOn", p.highpass_on)?;
    write_param_f64(w, "LowShelfGain", p.low_shelf_gain)?;
    write_param_f64(w, "MidGain", p.mid_gain)?;
    write_param_f64(w, "MidFrequency", p.mid_frequency)?;
    write_param_f64(w, "HighShelfGain", p.high_shelf_gain)?;
    write_param_f64(w, "Gain", p.gain)?;
    Ok(())
}

// ─── FilterEQ3 ────────────────────────────────────────────────────────────

/// FilterEQ3 parameters (legacy 3-band EQ with kill switches).
#[derive(Debug, Clone)]
pub struct FilterEq3Params {
    pub gain_lo: f64,
    pub gain_mid: f64,
    pub gain_hi: f64,
    pub freq_lo: f64,
    pub freq_hi: f64,
    pub low_on: bool,
    pub mid_on: bool,
    pub high_on: bool,
    pub slope: i32,
    pub flat_response: bool,
}

impl Default for FilterEq3Params {
    fn default() -> Self {
        Self {
            gain_lo: 0.0,
            gain_mid: 0.0,
            gain_hi: 0.0,
            freq_lo: 250.0,
            freq_hi: 2500.0,
            low_on: true,
            mid_on: true,
            high_on: true,
            slope: 0,
            flat_response: false,
        }
    }
}

pub fn parse_filter_eq3(node: Node<'_, '_>) -> FilterEq3Params {
    FilterEq3Params {
        gain_lo: param_f64(node, "GainLo", 0.0),
        gain_mid: param_f64(node, "GainMid", 0.0),
        gain_hi: param_f64(node, "GainHi", 0.0),
        freq_lo: param_f64(node, "FreqLo", 250.0),
        freq_hi: param_f64(node, "FreqHi", 2500.0),
        low_on: param_bool(node, "LowOn", true),
        mid_on: param_bool(node, "MidOn", true),
        high_on: param_bool(node, "HighOn", true),
        slope: param_i32(node, "Slope", 0),
        flat_response: param_bool(node, "FlatResponse", false),
    }
}

pub fn write_filter_eq3<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &FilterEq3Params,
) -> io::Result<()> {
    write_param_f64(w, "GainLo", p.gain_lo)?;
    write_param_f64(w, "GainMid", p.gain_mid)?;
    write_param_f64(w, "GainHi", p.gain_hi)?;
    write_param_f64(w, "FreqLo", p.freq_lo)?;
    write_param_f64(w, "FreqHi", p.freq_hi)?;
    write_param_bool(w, "LowOn", p.low_on)?;
    write_param_bool(w, "MidOn", p.mid_on)?;
    write_param_bool(w, "HighOn", p.high_on)?;
    write_param_i32(w, "Slope", p.slope)?;
    write_param_bool(w, "FlatResponse", p.flat_response)?;
    Ok(())
}

// ─── AutoPan ──────────────────────────────────────────────────────────────

/// AutoPan parameters (nested Lfo flattened).
#[derive(Debug, Clone)]
pub struct AutoPanParams {
    pub lfo_type: i32,
    pub lfo_frequency: f64,
    pub lfo_rate_type: i32,
    pub lfo_beat_rate: f64,
    pub lfo_stereo_mode: i32,
    pub lfo_spin: f64,
    pub lfo_phase: f64,
    pub lfo_offset: f64,
    pub lfo_is_on: bool,
    pub lfo_quantize: bool,
    pub lfo_beat_quantize: i32,
    pub lfo_noise_width: f64,
    pub lfo_amount: f64,
    pub lfo_invert: bool,
    pub lfo_shape: f64,
}

impl Default for AutoPanParams {
    fn default() -> Self {
        Self {
            lfo_type: 0,
            lfo_frequency: 1.0,
            lfo_rate_type: 0,
            lfo_beat_rate: 4.0,
            lfo_stereo_mode: 0,
            lfo_spin: 0.0,
            lfo_phase: 0.0,
            lfo_offset: 0.0,
            lfo_is_on: true,
            lfo_quantize: false,
            lfo_beat_quantize: 0,
            lfo_noise_width: 0.0,
            lfo_amount: 50.0,
            lfo_invert: false,
            lfo_shape: 0.0,
        }
    }
}

pub fn parse_auto_pan(node: Node<'_, '_>) -> AutoPanParams {
    // Parameters are nested inside an Lfo element.
    let lfo = child(node, "Lfo");
    let lfo_node = |name: &str| lfo.and_then(|n| child(n, name));

    AutoPanParams {
        lfo_type: lfo.map(|n| param_i32(n, "Type", 0)).unwrap_or(0),
        lfo_frequency: lfo.map(|n| param_f64(n, "Frequency", 1.0)).unwrap_or(1.0),
        lfo_rate_type: lfo.map(|n| param_i32(n, "RateType", 0)).unwrap_or(0),
        lfo_beat_rate: lfo.map(|n| param_f64(n, "BeatRate", 4.0)).unwrap_or(4.0),
        lfo_stereo_mode: lfo.map(|n| param_i32(n, "StereoMode", 0)).unwrap_or(0),
        lfo_spin: lfo.map(|n| param_f64(n, "Spin", 0.0)).unwrap_or(0.0),
        lfo_phase: lfo.map(|n| param_f64(n, "Phase", 0.0)).unwrap_or(0.0),
        lfo_offset: lfo.map(|n| param_f64(n, "Offset", 0.0)).unwrap_or(0.0),
        lfo_is_on: lfo_node("IsOn")
            .and_then(|n| child_bool(n, "Manual"))
            .unwrap_or(true),
        lfo_quantize: lfo_node("Quantize")
            .and_then(|n| child_bool(n, "Manual"))
            .unwrap_or(false),
        lfo_beat_quantize: lfo.map(|n| param_i32(n, "BeatQuantize", 0)).unwrap_or(0),
        lfo_noise_width: lfo.map(|n| param_f64(n, "NoiseWidth", 0.0)).unwrap_or(0.0),
        lfo_amount: lfo.map(|n| param_f64(n, "LfoAmount", 50.0)).unwrap_or(50.0),
        lfo_invert: lfo_node("LfoInvert")
            .and_then(|n| child_bool(n, "Manual"))
            .unwrap_or(false),
        lfo_shape: lfo.map(|n| param_f64(n, "LfoShape", 0.0)).unwrap_or(0.0),
    }
}

pub fn write_auto_pan<W: Write>(w: &mut AbletonXmlWriter<W>, p: &AutoPanParams) -> io::Result<()> {
    w.start("Lfo")?;
    write_param_i32(w, "Type", p.lfo_type)?;
    write_param_f64(w, "Frequency", p.lfo_frequency)?;
    write_param_i32(w, "RateType", p.lfo_rate_type)?;
    write_param_f64(w, "BeatRate", p.lfo_beat_rate)?;
    write_param_i32(w, "StereoMode", p.lfo_stereo_mode)?;
    write_param_f64(w, "Spin", p.lfo_spin)?;
    write_param_f64(w, "Phase", p.lfo_phase)?;
    write_param_f64(w, "Offset", p.lfo_offset)?;
    write_param_bool(w, "IsOn", p.lfo_is_on)?;
    write_param_bool(w, "Quantize", p.lfo_quantize)?;
    write_param_i32(w, "BeatQuantize", p.lfo_beat_quantize)?;
    write_param_f64(w, "NoiseWidth", p.lfo_noise_width)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    write_param_bool(w, "LfoInvert", p.lfo_invert)?;
    write_param_f64(w, "LfoShape", p.lfo_shape)?;
    w.end("Lfo")?;
    Ok(())
}

// ─── FrequencyShifter ─────────────────────────────────────────────────────

/// FrequencyShifter parameters.
#[derive(Debug, Clone)]
pub struct FrequencyShifterParams {
    pub modulation_mode: i32,
    pub coarse: f64,
    pub fine: f64,
    pub ring_mod_coarse: f64,
    pub amount: f64,
    pub invert_r: bool,
    pub drive_on: bool,
    pub drive: f64,
    pub lfo_amount: f64,
}

impl Default for FrequencyShifterParams {
    fn default() -> Self {
        Self {
            modulation_mode: 0,
            coarse: 0.0,
            fine: 0.0,
            ring_mod_coarse: 0.0,
            amount: 0.0,
            invert_r: false,
            drive_on: false,
            drive: 0.0,
            lfo_amount: 0.0,
        }
    }
}

pub fn parse_frequency_shifter(node: Node<'_, '_>) -> FrequencyShifterParams {
    FrequencyShifterParams {
        modulation_mode: param_i32(node, "ModulationMode", 0),
        coarse: param_f64(node, "Coarse", 0.0),
        fine: param_f64(node, "Fine", 0.0),
        ring_mod_coarse: param_f64(node, "RingModCoarse", 0.0),
        amount: param_f64(node, "Amount", 0.0),
        invert_r: param_bool(node, "InvertR", false),
        drive_on: param_bool(node, "DriveOn", false),
        drive: param_f64(node, "Drive", 0.0),
        lfo_amount: param_f64(node, "LfoAmount", 0.0),
    }
}

pub fn write_frequency_shifter<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &FrequencyShifterParams,
) -> io::Result<()> {
    write_param_i32(w, "ModulationMode", p.modulation_mode)?;
    write_param_f64(w, "Coarse", p.coarse)?;
    write_param_f64(w, "Fine", p.fine)?;
    write_param_f64(w, "RingModCoarse", p.ring_mod_coarse)?;
    write_param_f64(w, "Amount", p.amount)?;
    write_param_bool(w, "InvertR", p.invert_r)?;
    write_param_bool(w, "DriveOn", p.drive_on)?;
    write_param_f64(w, "Drive", p.drive)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    Ok(())
}

// ─── Shifter ──────────────────────────────────────────────────────────────

/// Shifter parameters (simplified — core pitch/frequency shift controls).
#[derive(Debug, Clone)]
pub struct ShifterParams {
    // Pitch section
    pub pitch_coarse: f64,
    pub pitch_fine: f64,
    pub pitch_window_size: f64,
    // Frequency/Ring shift section
    pub fshift_coarse: f64,
    pub ring_mod_coarse: f64,
    pub ring_mod_drive: bool,
    pub ring_mod_drive_amount: f64,
    pub mod_fine: f64,
    // LFO
    pub lfo_amount: f64,
    pub lfo_amount_pitch: f64,
    pub lfo_waveform: i32,
    pub lfo_sync_on: bool,
    pub lfo_rate_hz: f64,
    pub lfo_synced_rate: f64,
    pub lfo_spin_on: bool,
    pub lfo_phase_offset: f64,
    pub lfo_spin_amount: f64,
    // Delay
    pub delay_on: bool,
    pub delay_sync_on: bool,
    pub delay_time_seconds: f64,
    pub delay_synced_time: f64,
    pub delay_feedback: f64,
    // Global
    pub shifter_mode: i32,
    pub pitch_mode: i32,
    pub tone: f64,
    pub wide: bool,
    pub dry_wet: f64,
}

impl Default for ShifterParams {
    fn default() -> Self {
        Self {
            pitch_coarse: 0.0,
            pitch_fine: 0.0,
            pitch_window_size: 0.0,
            fshift_coarse: 0.0,
            ring_mod_coarse: 0.0,
            ring_mod_drive: false,
            ring_mod_drive_amount: 0.0,
            mod_fine: 0.0,
            lfo_amount: 0.0,
            lfo_amount_pitch: 0.0,
            lfo_waveform: 0,
            lfo_sync_on: false,
            lfo_rate_hz: 1.0,
            lfo_synced_rate: 4.0,
            lfo_spin_on: false,
            lfo_phase_offset: 0.0,
            lfo_spin_amount: 0.0,
            delay_on: false,
            delay_sync_on: false,
            delay_time_seconds: 0.5,
            delay_synced_time: 4.0,
            delay_feedback: 0.0,
            shifter_mode: 0,
            pitch_mode: 0,
            tone: 0.5,
            wide: false,
            dry_wet: 1.0,
        }
    }
}

pub fn parse_shifter(node: Node<'_, '_>) -> ShifterParams {
    ShifterParams {
        pitch_coarse: param_f64(node, "Pitch_Coarse", 0.0),
        pitch_fine: param_f64(node, "Pitch_Fine", 0.0),
        pitch_window_size: param_f64(node, "Pitch_WindowSize", 0.0),
        fshift_coarse: param_f64(node, "ModBasedShifting_FShift_Coarse", 0.0),
        ring_mod_coarse: param_f64(node, "ModBasedShifting_RingMod_Coarse", 0.0),
        ring_mod_drive: param_bool(node, "ModBasedShifting_RingMod_Drive", false),
        ring_mod_drive_amount: param_f64(node, "ModBasedShifting_RingMod_DriveAmount", 0.0),
        mod_fine: param_f64(node, "ModBasedShifting_Fine", 0.0),
        lfo_amount: param_f64(node, "Lfo_Amount", 0.0),
        lfo_amount_pitch: param_f64(node, "Lfo_AmountPitch", 0.0),
        lfo_waveform: param_i32(node, "Lfo_Waveform", 0),
        lfo_sync_on: param_bool(node, "Lfo_SyncOn", false),
        lfo_rate_hz: param_f64(node, "Lfo_RateHz", 1.0),
        lfo_synced_rate: param_f64(node, "Lfo_SyncedRate", 4.0),
        lfo_spin_on: param_bool(node, "Lfo_SpinOn", false),
        lfo_phase_offset: param_f64(node, "Lfo_PhaseOffset", 0.0),
        lfo_spin_amount: param_f64(node, "Lfo_SpinAmount", 0.0),
        delay_on: param_bool(node, "Delay_On", false),
        delay_sync_on: param_bool(node, "Delay_SyncOn", false),
        delay_time_seconds: param_f64(node, "Delay_TimeSeconds", 0.5),
        delay_synced_time: param_f64(node, "Delay_SyncedTime", 4.0),
        delay_feedback: param_f64(node, "Delay_Feedback", 0.0),
        shifter_mode: param_i32(node, "Global_ShifterMode", 0),
        pitch_mode: param_i32(node, "Global_PitchMode", 0),
        tone: param_f64(node, "Global_Tone", 0.5),
        wide: param_bool(node, "Global_Wide", false),
        dry_wet: param_f64(node, "Global_DryWet", 1.0),
    }
}

pub fn write_shifter<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ShifterParams) -> io::Result<()> {
    write_param_f64(w, "Pitch_Coarse", p.pitch_coarse)?;
    write_param_f64(w, "Pitch_Fine", p.pitch_fine)?;
    write_param_f64(w, "Pitch_WindowSize", p.pitch_window_size)?;
    write_param_f64(w, "ModBasedShifting_FShift_Coarse", p.fshift_coarse)?;
    write_param_f64(w, "ModBasedShifting_RingMod_Coarse", p.ring_mod_coarse)?;
    write_param_bool(w, "ModBasedShifting_RingMod_Drive", p.ring_mod_drive)?;
    write_param_f64(
        w,
        "ModBasedShifting_RingMod_DriveAmount",
        p.ring_mod_drive_amount,
    )?;
    write_param_f64(w, "ModBasedShifting_Fine", p.mod_fine)?;
    write_param_f64(w, "Lfo_Amount", p.lfo_amount)?;
    write_param_f64(w, "Lfo_AmountPitch", p.lfo_amount_pitch)?;
    write_param_i32(w, "Lfo_Waveform", p.lfo_waveform)?;
    write_param_bool(w, "Lfo_SyncOn", p.lfo_sync_on)?;
    write_param_f64(w, "Lfo_RateHz", p.lfo_rate_hz)?;
    write_param_f64(w, "Lfo_SyncedRate", p.lfo_synced_rate)?;
    write_param_bool(w, "Lfo_SpinOn", p.lfo_spin_on)?;
    write_param_f64(w, "Lfo_PhaseOffset", p.lfo_phase_offset)?;
    write_param_f64(w, "Lfo_SpinAmount", p.lfo_spin_amount)?;
    write_param_bool(w, "Delay_On", p.delay_on)?;
    write_param_bool(w, "Delay_SyncOn", p.delay_sync_on)?;
    write_param_f64(w, "Delay_TimeSeconds", p.delay_time_seconds)?;
    write_param_f64(w, "Delay_SyncedTime", p.delay_synced_time)?;
    write_param_f64(w, "Delay_Feedback", p.delay_feedback)?;
    write_param_i32(w, "Global_ShifterMode", p.shifter_mode)?;
    write_param_i32(w, "Global_PitchMode", p.pitch_mode)?;
    write_param_f64(w, "Global_Tone", p.tone)?;
    write_param_bool(w, "Global_Wide", p.wide)?;
    write_param_f64(w, "Global_DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Spectral ─────────────────────────────────────────────────────────────

/// Spectral (Spectral Time / Spectral Resonator) parameters.
#[derive(Debug, Clone)]
pub struct SpectralParams {
    // Freezer
    pub freezer_on: bool,
    pub freezer_freeze_on: bool,
    pub freezer_avoid_freezing_onsets: bool,
    pub freezer_sync_interval_units: i32,
    pub freezer_sync_interval_beats: f64,
    pub freezer_sync_interval_seconds: f64,
    pub freezer_main_mode: i32,
    pub freezer_retrigger_mode: i32,
    pub freezer_fade_type: i32,
    pub freezer_fade_in: f64,
    pub freezer_crossfade_percent: f64,
    pub freezer_fade_out: f64,
    pub freezer_sensitivity: f64,
    // Delay
    pub delay_on: bool,
    pub delay_time_seconds: f64,
    pub delay_time_unit: i32,
    pub delay_time_sixteenths: f64,
    pub delay_time_divisions: f64,
    pub delay_feedback: f64,
    pub delay_tilt: f64,
    pub delay_spray: f64,
    pub delay_mask: f64,
    pub delay_stereo_spread: f64,
    pub delay_frequency_shift: f64,
    pub delay_dry_wet: f64,
    // Global
    pub input_send_gain: f64,
    pub resolution: i32,
    pub dry_wet: f64,
    pub zero_latency_dry: bool,
    pub processing_order: i32,
}

impl Default for SpectralParams {
    fn default() -> Self {
        Self {
            freezer_on: false,
            freezer_freeze_on: false,
            freezer_avoid_freezing_onsets: false,
            freezer_sync_interval_units: 0,
            freezer_sync_interval_beats: 4.0,
            freezer_sync_interval_seconds: 1.0,
            freezer_main_mode: 0,
            freezer_retrigger_mode: 0,
            freezer_fade_type: 0,
            freezer_fade_in: 0.0,
            freezer_crossfade_percent: 50.0,
            freezer_fade_out: 0.0,
            freezer_sensitivity: 0.5,
            delay_on: false,
            delay_time_seconds: 0.5,
            delay_time_unit: 0,
            delay_time_sixteenths: 4.0,
            delay_time_divisions: 4.0,
            delay_feedback: 0.5,
            delay_tilt: 0.0,
            delay_spray: 0.0,
            delay_mask: 0.0,
            delay_stereo_spread: 0.0,
            delay_frequency_shift: 0.0,
            delay_dry_wet: 0.5,
            input_send_gain: 0.0,
            resolution: 0,
            dry_wet: 0.5,
            zero_latency_dry: false,
            processing_order: 0,
        }
    }
}

pub fn parse_spectral(node: Node<'_, '_>) -> SpectralParams {
    SpectralParams {
        freezer_on: param_bool(node, "Freezer_On", false),
        freezer_freeze_on: param_bool(node, "Freezer_FreezeOn", false),
        freezer_avoid_freezing_onsets: param_bool(node, "Freezer_AvoidFreezingOnsets", false),
        freezer_sync_interval_units: param_i32(node, "Freezer_SyncIntervalUnits", 0),
        freezer_sync_interval_beats: param_f64(node, "Freezer_SyncIntervalBeats", 4.0),
        freezer_sync_interval_seconds: param_f64(node, "Freezer_SyncIntervalSeconds", 1.0),
        freezer_main_mode: param_i32(node, "Freezer_MainMode", 0),
        freezer_retrigger_mode: param_i32(node, "Freezer_RetriggerMode", 0),
        freezer_fade_type: param_i32(node, "Freezer_FadeType", 0),
        freezer_fade_in: param_f64(node, "Freezer_FadeIn", 0.0),
        freezer_crossfade_percent: param_f64(node, "Freezer_CrossfadePercent", 50.0),
        freezer_fade_out: param_f64(node, "Freezer_FadeOut", 0.0),
        freezer_sensitivity: param_f64(node, "Freezer_Sensitivity", 0.5),
        delay_on: param_bool(node, "Delay_On", false),
        delay_time_seconds: param_f64(node, "Delay_TimeSeconds", 0.5),
        delay_time_unit: param_i32(node, "Delay_TimeUnit", 0),
        delay_time_sixteenths: param_f64(node, "Delay_TimeSixteenths", 4.0),
        delay_time_divisions: param_f64(node, "Delay_TimeDivisions", 4.0),
        delay_feedback: param_f64(node, "Delay_Feedback", 0.5),
        delay_tilt: param_f64(node, "Delay_Tilt", 0.0),
        delay_spray: param_f64(node, "Delay_Spray", 0.0),
        delay_mask: param_f64(node, "Delay_Mask", 0.0),
        delay_stereo_spread: param_f64(node, "Delay_StereoSpread", 0.0),
        delay_frequency_shift: param_f64(node, "Delay_FrequencyShift", 0.0),
        delay_dry_wet: param_f64(node, "Delay_DryWet", 0.5),
        input_send_gain: param_f64(node, "InputSendGain", 0.0),
        resolution: param_i32(node, "Resolution", 0),
        dry_wet: param_f64(node, "DryWet", 0.5),
        zero_latency_dry: param_bool(node, "ZeroLatencyDry", false),
        processing_order: param_i32(node, "ProcessingOrder", 0),
    }
}

pub fn write_spectral<W: Write>(w: &mut AbletonXmlWriter<W>, p: &SpectralParams) -> io::Result<()> {
    write_param_bool(w, "Freezer_On", p.freezer_on)?;
    write_param_bool(w, "Freezer_FreezeOn", p.freezer_freeze_on)?;
    write_param_bool(
        w,
        "Freezer_AvoidFreezingOnsets",
        p.freezer_avoid_freezing_onsets,
    )?;
    write_param_i32(
        w,
        "Freezer_SyncIntervalUnits",
        p.freezer_sync_interval_units,
    )?;
    write_param_f64(
        w,
        "Freezer_SyncIntervalBeats",
        p.freezer_sync_interval_beats,
    )?;
    write_param_f64(
        w,
        "Freezer_SyncIntervalSeconds",
        p.freezer_sync_interval_seconds,
    )?;
    write_param_i32(w, "Freezer_MainMode", p.freezer_main_mode)?;
    write_param_i32(w, "Freezer_RetriggerMode", p.freezer_retrigger_mode)?;
    write_param_i32(w, "Freezer_FadeType", p.freezer_fade_type)?;
    write_param_f64(w, "Freezer_FadeIn", p.freezer_fade_in)?;
    write_param_f64(w, "Freezer_CrossfadePercent", p.freezer_crossfade_percent)?;
    write_param_f64(w, "Freezer_FadeOut", p.freezer_fade_out)?;
    write_param_f64(w, "Freezer_Sensitivity", p.freezer_sensitivity)?;
    write_param_bool(w, "Delay_On", p.delay_on)?;
    write_param_f64(w, "Delay_TimeSeconds", p.delay_time_seconds)?;
    write_param_i32(w, "Delay_TimeUnit", p.delay_time_unit)?;
    write_param_f64(w, "Delay_TimeSixteenths", p.delay_time_sixteenths)?;
    write_param_f64(w, "Delay_TimeDivisions", p.delay_time_divisions)?;
    write_param_f64(w, "Delay_Feedback", p.delay_feedback)?;
    write_param_f64(w, "Delay_Tilt", p.delay_tilt)?;
    write_param_f64(w, "Delay_Spray", p.delay_spray)?;
    write_param_f64(w, "Delay_Mask", p.delay_mask)?;
    write_param_f64(w, "Delay_StereoSpread", p.delay_stereo_spread)?;
    write_param_f64(w, "Delay_FrequencyShift", p.delay_frequency_shift)?;
    write_param_f64(w, "Delay_DryWet", p.delay_dry_wet)?;
    write_param_f64(w, "InputSendGain", p.input_send_gain)?;
    write_param_i32(w, "Resolution", p.resolution)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "ZeroLatencyDry", p.zero_latency_dry)?;
    write_param_i32(w, "ProcessingOrder", p.processing_order)?;
    Ok(())
}

// ─── Transmute ────────────────────────────────────────────────────────────

/// Transmute parameters.
#[derive(Debug, Clone)]
pub struct TransmuteParams {
    // MIDI/Pitch
    pub midi_gate: i32,
    pub mono_poly_mode: i32,
    pub polyphony: f64,
    pub transpose: f64,
    pub pitch_bend_range: f64,
    pub glide: f64,
    // Partial frequencies
    pub frequency_dial_mode: i32,
    pub frequency_hz: f64,
    pub frequency_note: f64,
    pub shift: f64,
    pub partial_stretch: f64,
    // Decay/damping
    pub decay_time: f64,
    pub high_freq_damp: f64,
    pub low_freq_damp: f64,
    // Modulation
    pub mod_mode: i32,
    pub mod_rate: f64,
    pub pitch_mod_amount: f64,
    // Global
    pub pitch_mode: i32,
    pub num_harmonics: f64,
    pub unison: f64,
    pub unison_amount: f64,
    pub input_send: f64,
    pub dry_wet: f64,
}

impl Default for TransmuteParams {
    fn default() -> Self {
        Self {
            midi_gate: 0,
            mono_poly_mode: 0,
            polyphony: 1.0,
            transpose: 0.0,
            pitch_bend_range: 12.0,
            glide: 0.0,
            frequency_dial_mode: 0,
            frequency_hz: 440.0,
            frequency_note: 69.0,
            shift: 0.0,
            partial_stretch: 0.0,
            decay_time: 1.0,
            high_freq_damp: 0.0,
            low_freq_damp: 0.0,
            mod_mode: 0,
            mod_rate: 1.0,
            pitch_mod_amount: 0.0,
            pitch_mode: 0,
            num_harmonics: 24.0,
            unison: 1.0,
            unison_amount: 0.0,
            input_send: 0.0,
            dry_wet: 0.5,
        }
    }
}

pub fn parse_transmute(node: Node<'_, '_>) -> TransmuteParams {
    TransmuteParams {
        midi_gate: param_i32(node, "MidiPitch_MidiGate", 0),
        mono_poly_mode: param_i32(node, "MidiPitch_MonoPolyMode", 0),
        polyphony: param_f64(node, "MidiPitch_Polyphony", 1.0),
        transpose: param_f64(node, "MidiPitch_Transpose", 0.0),
        pitch_bend_range: param_f64(node, "MidiPitch_PitchBendRange", 12.0),
        glide: param_f64(node, "MidiPitch_Glide", 0.0),
        frequency_dial_mode: param_i32(node, "PartialFrequencies_FrequencyDialMode", 0),
        frequency_hz: param_f64(node, "PartialFrequencies_FrequencyHz", 440.0),
        frequency_note: param_f64(node, "PartialFrequencies_FrequencyNote", 69.0),
        shift: param_f64(node, "PartialFrequencies_Shift", 0.0),
        partial_stretch: param_f64(node, "PartialFrequencies_PartialStretch", 0.0),
        decay_time: param_f64(node, "DecayDamping_DecayTime", 1.0),
        high_freq_damp: param_f64(node, "DecayDamping_HighFreqDamp", 0.0),
        low_freq_damp: param_f64(node, "DecayDamping_LowFreqDamp", 0.0),
        mod_mode: param_i32(node, "Modulation_ModMode", 0),
        mod_rate: param_f64(node, "Modulation_ModRate", 1.0),
        pitch_mod_amount: param_f64(node, "Modulation_PitchModAmount", 0.0),
        pitch_mode: param_i32(node, "Global_PitchMode", 0),
        num_harmonics: param_f64(node, "Global_NumHarmonics", 24.0),
        unison: param_f64(node, "Global_Unison", 1.0),
        unison_amount: param_f64(node, "Global_UnisonAmount", 0.0),
        input_send: param_f64(node, "Global_InputSend", 0.0),
        dry_wet: param_f64(node, "Global_DryWet", 0.5),
    }
}

pub fn write_transmute<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &TransmuteParams,
) -> io::Result<()> {
    write_param_i32(w, "MidiPitch_MidiGate", p.midi_gate)?;
    write_param_i32(w, "MidiPitch_MonoPolyMode", p.mono_poly_mode)?;
    write_param_f64(w, "MidiPitch_Polyphony", p.polyphony)?;
    write_param_f64(w, "MidiPitch_Transpose", p.transpose)?;
    write_param_f64(w, "MidiPitch_PitchBendRange", p.pitch_bend_range)?;
    write_param_f64(w, "MidiPitch_Glide", p.glide)?;
    write_param_i32(
        w,
        "PartialFrequencies_FrequencyDialMode",
        p.frequency_dial_mode,
    )?;
    write_param_f64(w, "PartialFrequencies_FrequencyHz", p.frequency_hz)?;
    write_param_f64(w, "PartialFrequencies_FrequencyNote", p.frequency_note)?;
    write_param_f64(w, "PartialFrequencies_Shift", p.shift)?;
    write_param_f64(w, "PartialFrequencies_PartialStretch", p.partial_stretch)?;
    write_param_f64(w, "DecayDamping_DecayTime", p.decay_time)?;
    write_param_f64(w, "DecayDamping_HighFreqDamp", p.high_freq_damp)?;
    write_param_f64(w, "DecayDamping_LowFreqDamp", p.low_freq_damp)?;
    write_param_i32(w, "Modulation_ModMode", p.mod_mode)?;
    write_param_f64(w, "Modulation_ModRate", p.mod_rate)?;
    write_param_f64(w, "Modulation_PitchModAmount", p.pitch_mod_amount)?;
    write_param_i32(w, "Global_PitchMode", p.pitch_mode)?;
    write_param_f64(w, "Global_NumHarmonics", p.num_harmonics)?;
    write_param_f64(w, "Global_Unison", p.unison)?;
    write_param_f64(w, "Global_UnisonAmount", p.unison_amount)?;
    write_param_f64(w, "Global_InputSend", p.input_send)?;
    write_param_f64(w, "Global_DryWet", p.dry_wet)?;
    Ok(())
}
