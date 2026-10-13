//! Delay and reverb device typed parameters.

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

// ─── Helpers (same pattern as dynamics.rs) ─────────────────────────────────

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

// ─── Reverb ────────────────────────────────────────────────────────────────

/// Reverb parameters. Maps to REAPER's ReaVerbate.
#[derive(Debug, Clone)]
pub struct ReverbParams {
    pub pre_delay: f64,
    pub decay_time: f64,
    pub room_size: f64,
    pub room_type: i32,
    pub stereo_separation: f64,
    pub mix_reflect: f64,
    pub mix_diffuse: f64,
    pub mix_direct: f64,
    pub shelf_hi_on: bool,
    pub shelf_hi_freq: f64,
    pub shelf_hi_gain: f64,
    pub shelf_lo_on: bool,
    pub shelf_lo_freq: f64,
    pub shelf_lo_gain: f64,
    pub chorus_on: bool,
    pub spin_on: bool,
    pub freeze_on: bool,
    pub flat_on: bool,
    pub cut_on: bool,
    pub diffuse_delay: f64,
    pub all_pass_gain: f64,
    pub all_pass_size: f64,
    pub band_high_on: bool,
    pub band_low_on: bool,
    pub band_freq: f64,
    pub band_width: f64,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            pre_delay: 1.0,
            decay_time: 1000.0,
            room_size: 100.0,
            room_type: 0,
            stereo_separation: 100.0,
            mix_reflect: 60.0,
            mix_diffuse: 60.0,
            mix_direct: 100.0,
            shelf_hi_on: false,
            shelf_hi_freq: 4500.0,
            shelf_hi_gain: -6.0,
            shelf_lo_on: false,
            shelf_lo_freq: 250.0,
            shelf_lo_gain: 0.0,
            chorus_on: true,
            spin_on: true,
            freeze_on: false,
            flat_on: false,
            cut_on: false,
            diffuse_delay: 1.0,
            all_pass_gain: 0.6,
            all_pass_size: 60.0,
            band_high_on: false,
            band_low_on: false,
            band_freq: 3500.0,
            band_width: 7.0,
        }
    }
}

pub fn parse_reverb(node: Node<'_, '_>) -> ReverbParams {
    ReverbParams {
        pre_delay: param_f64(node, "PreDelay", 1.0),
        decay_time: param_f64(node, "DecayTime", 1000.0),
        room_size: param_f64(node, "RoomSize", 100.0),
        room_type: param_i32(node, "RoomType", 0),
        stereo_separation: param_f64(node, "StereoSeparation", 100.0),
        mix_reflect: param_f64(node, "MixReflect", 60.0),
        mix_diffuse: param_f64(node, "MixDiffuse", 60.0),
        mix_direct: param_f64(node, "MixDirect", 100.0),
        shelf_hi_on: param_bool(node, "ShelfHiOn", false),
        shelf_hi_freq: param_f64(node, "ShelfHiFreq", 4500.0),
        shelf_hi_gain: param_f64(node, "ShelfHiGain", -6.0),
        shelf_lo_on: param_bool(node, "ShelfLoOn", false),
        shelf_lo_freq: param_f64(node, "ShelfLoFreq", 250.0),
        shelf_lo_gain: param_f64(node, "ShelfLoGain", 0.0),
        chorus_on: param_bool(node, "ChorusOn", true),
        spin_on: param_bool(node, "SpinOn", true),
        freeze_on: param_bool(node, "FreezeOn", false),
        flat_on: param_bool(node, "FlatOn", false),
        cut_on: param_bool(node, "CutOn", false),
        diffuse_delay: param_f64(node, "DiffuseDelay", 1.0),
        all_pass_gain: param_f64(node, "AllPassGain", 0.6),
        all_pass_size: param_f64(node, "AllPassSize", 60.0),
        band_high_on: param_bool(node, "BandHighOn", false),
        band_low_on: param_bool(node, "BandLowOn", false),
        band_freq: param_f64(node, "BandFreq", 3500.0),
        band_width: param_f64(node, "BandWidth", 7.0),
    }
}

pub fn write_reverb<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ReverbParams) -> io::Result<()> {
    write_param_f64(w, "PreDelay", p.pre_delay)?;
    write_param_f64(w, "DecayTime", p.decay_time)?;
    write_param_f64(w, "RoomSize", p.room_size)?;
    write_param_i32(w, "RoomType", p.room_type)?;
    write_param_f64(w, "StereoSeparation", p.stereo_separation)?;
    write_param_f64(w, "MixReflect", p.mix_reflect)?;
    write_param_f64(w, "MixDiffuse", p.mix_diffuse)?;
    write_param_f64(w, "MixDirect", p.mix_direct)?;
    write_param_bool(w, "ShelfHiOn", p.shelf_hi_on)?;
    write_param_f64(w, "ShelfHiFreq", p.shelf_hi_freq)?;
    write_param_f64(w, "ShelfHiGain", p.shelf_hi_gain)?;
    write_param_bool(w, "ShelfLoOn", p.shelf_lo_on)?;
    write_param_f64(w, "ShelfLoFreq", p.shelf_lo_freq)?;
    write_param_f64(w, "ShelfLoGain", p.shelf_lo_gain)?;
    write_param_bool(w, "ChorusOn", p.chorus_on)?;
    write_param_bool(w, "SpinOn", p.spin_on)?;
    write_param_bool(w, "FreezeOn", p.freeze_on)?;
    write_param_bool(w, "FlatOn", p.flat_on)?;
    write_param_bool(w, "CutOn", p.cut_on)?;
    write_param_f64(w, "DiffuseDelay", p.diffuse_delay)?;
    write_param_f64(w, "AllPassGain", p.all_pass_gain)?;
    write_param_f64(w, "AllPassSize", p.all_pass_size)?;
    write_param_bool(w, "BandHighOn", p.band_high_on)?;
    write_param_bool(w, "BandLowOn", p.band_low_on)?;
    write_param_f64(w, "BandFreq", p.band_freq)?;
    write_param_f64(w, "BandWidth", p.band_width)?;
    Ok(())
}

// ─── Delay ─────────────────────────────────────────────────────────────────

/// Delay parameters (unified Simple/PingPong).
#[derive(Debug, Clone)]
pub struct DelayParams {
    pub link: bool,
    pub ping_pong: bool,
    pub sync_l: bool,
    pub sync_r: bool,
    /// Time in ms when unsynced.
    pub time_l: f64,
    /// Time in ms when unsynced.
    pub time_r: f64,
    /// Synced divisions (sixteenths).
    pub synced_sixteenth_l: i32,
    /// Synced divisions (sixteenths).
    pub synced_sixteenth_r: i32,
    pub offset_l: f64,
    pub offset_r: f64,
    pub feedback: f64,
    pub freeze: bool,
    pub filter_on: bool,
    pub filter_frequency: f64,
    pub filter_bandwidth: f64,
    pub mod_frequency: f64,
    pub mod_amount_time: f64,
    pub mod_amount_filter: f64,
    pub dry_wet: f64,
    pub eco_processing: bool,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            link: true,
            ping_pong: false,
            sync_l: true,
            sync_r: true,
            time_l: 500.0,
            time_r: 500.0,
            synced_sixteenth_l: 4,
            synced_sixteenth_r: 4,
            offset_l: 0.0,
            offset_r: 0.0,
            feedback: 0.5,
            freeze: false,
            filter_on: false,
            filter_frequency: 1000.0,
            filter_bandwidth: 6.0,
            mod_frequency: 1.0,
            mod_amount_time: 0.0,
            mod_amount_filter: 0.0,
            dry_wet: 0.5,
            eco_processing: false,
        }
    }
}

pub fn parse_delay(node: Node<'_, '_>) -> DelayParams {
    DelayParams {
        link: param_bool(node, "Link", true),
        ping_pong: param_bool(node, "PingPong", false),
        sync_l: param_bool(node, "SyncL", true),
        sync_r: param_bool(node, "SyncR", true),
        time_l: param_f64(node, "TimeL", 500.0),
        time_r: param_f64(node, "TimeR", 500.0),
        synced_sixteenth_l: param_i32(node, "SyncedSixteenthL", 4),
        synced_sixteenth_r: param_i32(node, "SyncedSixteenthR", 4),
        offset_l: param_f64(node, "OffsetL", 0.0),
        offset_r: param_f64(node, "OffsetR", 0.0),
        feedback: param_f64(node, "Feedback", 0.5),
        freeze: param_bool(node, "Freeze", false),
        filter_on: param_bool(node, "FilterOn", false),
        filter_frequency: param_f64(node, "FilterFrequency", 1000.0),
        filter_bandwidth: param_f64(node, "FilterBandwidth", 6.0),
        mod_frequency: param_f64(node, "ModFrequency", 1.0),
        mod_amount_time: param_f64(node, "ModAmountTime", 0.0),
        mod_amount_filter: param_f64(node, "ModAmountFilter", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
        eco_processing: param_bool(node, "EcoProcessing", false),
    }
}

pub fn write_delay<W: Write>(w: &mut AbletonXmlWriter<W>, p: &DelayParams) -> io::Result<()> {
    write_param_bool(w, "Link", p.link)?;
    write_param_bool(w, "PingPong", p.ping_pong)?;
    write_param_bool(w, "SyncL", p.sync_l)?;
    write_param_bool(w, "SyncR", p.sync_r)?;
    write_param_f64(w, "TimeL", p.time_l)?;
    write_param_f64(w, "TimeR", p.time_r)?;
    write_param_i32(w, "SyncedSixteenthL", p.synced_sixteenth_l)?;
    write_param_i32(w, "SyncedSixteenthR", p.synced_sixteenth_r)?;
    write_param_f64(w, "OffsetL", p.offset_l)?;
    write_param_f64(w, "OffsetR", p.offset_r)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_bool(w, "Freeze", p.freeze)?;
    write_param_bool(w, "FilterOn", p.filter_on)?;
    write_param_f64(w, "FilterFrequency", p.filter_frequency)?;
    write_param_f64(w, "FilterBandwidth", p.filter_bandwidth)?;
    write_param_f64(w, "ModFrequency", p.mod_frequency)?;
    write_param_f64(w, "ModAmountTime", p.mod_amount_time)?;
    write_param_f64(w, "ModAmountFilter", p.mod_amount_filter)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "EcoProcessing", p.eco_processing)?;
    Ok(())
}

// ─── Echo ──────────────────────────────────────────────────────────────────

/// Echo parameters.
#[derive(Debug, Clone)]
pub struct EchoParams {
    pub time_link: bool,
    pub sync_l: bool,
    pub sync_r: bool,
    pub time_l: f64,
    pub time_r: f64,
    pub synced_division_l: f64,
    pub synced_division_r: f64,
    pub offset_l: f64,
    pub offset_r: f64,
    pub repitch: bool,
    pub feedback: f64,
    pub feedback_invert: bool,
    pub channel_mode: i32,
    pub input_gain: f64,
    pub output_gain: f64,
    pub gate_on: bool,
    pub gate_threshold: f64,
    pub gate_release: f64,
    pub ducking_on: bool,
    pub ducking_threshold: f64,
    pub ducking_release: f64,
    pub filter_on: bool,
    pub filter_hp_freq: f64,
    pub filter_hp_res: f64,
    pub filter_lp_freq: f64,
    pub filter_lp_res: f64,
    pub mod_frequency: f64,
    pub mod_amount_delay: f64,
    pub mod_amount_filter: f64,
    pub reverb_level: f64,
    pub reverb_decay: f64,
    pub stereo_width: f64,
    pub dry_wet: f64,
    pub noise_on: bool,
    pub noise_amount: f64,
    pub wobble_on: bool,
    pub wobble_amount: f64,
}

impl Default for EchoParams {
    fn default() -> Self {
        Self {
            time_link: true,
            sync_l: true,
            sync_r: true,
            time_l: 500.0,
            time_r: 500.0,
            synced_division_l: 4.0,
            synced_division_r: 4.0,
            offset_l: 0.0,
            offset_r: 0.0,
            repitch: false,
            feedback: 0.5,
            feedback_invert: false,
            channel_mode: 0,
            input_gain: 0.0,
            output_gain: 0.0,
            gate_on: false,
            gate_threshold: -40.0,
            gate_release: 100.0,
            ducking_on: false,
            ducking_threshold: -20.0,
            ducking_release: 100.0,
            filter_on: false,
            filter_hp_freq: 100.0,
            filter_hp_res: 0.7,
            filter_lp_freq: 5000.0,
            filter_lp_res: 0.7,
            mod_frequency: 1.0,
            mod_amount_delay: 0.0,
            mod_amount_filter: 0.0,
            reverb_level: 0.0,
            reverb_decay: 0.5,
            stereo_width: 0.0,
            dry_wet: 0.5,
            noise_on: false,
            noise_amount: 0.0,
            wobble_on: false,
            wobble_amount: 0.0,
        }
    }
}

pub fn parse_echo(node: Node<'_, '_>) -> EchoParams {
    EchoParams {
        time_link: param_bool(node, "TimeLink", true),
        sync_l: param_bool(node, "SyncL", true),
        sync_r: param_bool(node, "SyncR", true),
        time_l: param_f64(node, "TimeL", 500.0),
        time_r: param_f64(node, "TimeR", 500.0),
        synced_division_l: param_f64(node, "SyncedDivisionL", 4.0),
        synced_division_r: param_f64(node, "SyncedDivisionR", 4.0),
        offset_l: param_f64(node, "OffsetL", 0.0),
        offset_r: param_f64(node, "OffsetR", 0.0),
        repitch: param_bool(node, "Repitch", false),
        feedback: param_f64(node, "Feedback", 0.5),
        feedback_invert: param_bool(node, "FeedbackInvert", false),
        channel_mode: param_i32(node, "ChannelMode", 0),
        input_gain: param_f64(node, "InputGain", 0.0),
        output_gain: param_f64(node, "OutputGain", 0.0),
        gate_on: param_bool(node, "GateOn", false),
        gate_threshold: param_f64(node, "GateThreshold", -40.0),
        gate_release: param_f64(node, "GateRelease", 100.0),
        ducking_on: param_bool(node, "DuckingOn", false),
        ducking_threshold: param_f64(node, "DuckingThreshold", -20.0),
        ducking_release: param_f64(node, "DuckingRelease", 100.0),
        filter_on: param_bool(node, "FilterOn", false),
        filter_hp_freq: param_f64(node, "FilterHPFreq", 100.0),
        filter_hp_res: param_f64(node, "FilterHPRes", 0.7),
        filter_lp_freq: param_f64(node, "FilterLPFreq", 5000.0),
        filter_lp_res: param_f64(node, "FilterLPRes", 0.7),
        mod_frequency: param_f64(node, "ModFrequency", 1.0),
        mod_amount_delay: param_f64(node, "ModAmountDelay", 0.0),
        mod_amount_filter: param_f64(node, "ModAmountFilter", 0.0),
        reverb_level: param_f64(node, "ReverbLevel", 0.0),
        reverb_decay: param_f64(node, "ReverbDecay", 0.5),
        stereo_width: param_f64(node, "StereoWidth", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
        noise_on: param_bool(node, "NoiseOn", false),
        noise_amount: param_f64(node, "NoiseAmount", 0.0),
        wobble_on: param_bool(node, "WobbleOn", false),
        wobble_amount: param_f64(node, "WobbleAmount", 0.0),
    }
}

pub fn write_echo<W: Write>(w: &mut AbletonXmlWriter<W>, p: &EchoParams) -> io::Result<()> {
    write_param_bool(w, "TimeLink", p.time_link)?;
    write_param_bool(w, "SyncL", p.sync_l)?;
    write_param_bool(w, "SyncR", p.sync_r)?;
    write_param_f64(w, "TimeL", p.time_l)?;
    write_param_f64(w, "TimeR", p.time_r)?;
    write_param_f64(w, "SyncedDivisionL", p.synced_division_l)?;
    write_param_f64(w, "SyncedDivisionR", p.synced_division_r)?;
    write_param_f64(w, "OffsetL", p.offset_l)?;
    write_param_f64(w, "OffsetR", p.offset_r)?;
    write_param_bool(w, "Repitch", p.repitch)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_bool(w, "FeedbackInvert", p.feedback_invert)?;
    write_param_i32(w, "ChannelMode", p.channel_mode)?;
    write_param_f64(w, "InputGain", p.input_gain)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_bool(w, "GateOn", p.gate_on)?;
    write_param_f64(w, "GateThreshold", p.gate_threshold)?;
    write_param_f64(w, "GateRelease", p.gate_release)?;
    write_param_bool(w, "DuckingOn", p.ducking_on)?;
    write_param_f64(w, "DuckingThreshold", p.ducking_threshold)?;
    write_param_f64(w, "DuckingRelease", p.ducking_release)?;
    write_param_bool(w, "FilterOn", p.filter_on)?;
    write_param_f64(w, "FilterHPFreq", p.filter_hp_freq)?;
    write_param_f64(w, "FilterHPRes", p.filter_hp_res)?;
    write_param_f64(w, "FilterLPFreq", p.filter_lp_freq)?;
    write_param_f64(w, "FilterLPRes", p.filter_lp_res)?;
    write_param_f64(w, "ModFrequency", p.mod_frequency)?;
    write_param_f64(w, "ModAmountDelay", p.mod_amount_delay)?;
    write_param_f64(w, "ModAmountFilter", p.mod_amount_filter)?;
    write_param_f64(w, "ReverbLevel", p.reverb_level)?;
    write_param_f64(w, "ReverbDecay", p.reverb_decay)?;
    write_param_f64(w, "StereoWidth", p.stereo_width)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "NoiseOn", p.noise_on)?;
    write_param_f64(w, "NoiseAmount", p.noise_amount)?;
    write_param_bool(w, "WobbleOn", p.wobble_on)?;
    write_param_f64(w, "WobbleAmount", p.wobble_amount)?;
    Ok(())
}
