//! Amp & distortion device typed parameters (Amp, Overdrive, Pedal, DrumBuss, Tube, Roar).

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

// ─── Amp ──────────────────────────────────────────────────────────────────

/// Amp parameters.
#[derive(Debug, Clone)]
pub struct AmpParams {
    /// 0=Clean, 1=Boost, 2=Blues, 3=Rock, 4=Lead, 5=Heavy, 6=Bass.
    pub amp_type: i32,
    pub bass: f64,
    pub middle: f64,
    pub treble: f64,
    pub presence: f64,
    pub gain: f64,
    pub volume: f64,
    pub dual_mono: bool,
    pub dry_wet: f64,
}

impl Default for AmpParams {
    fn default() -> Self {
        Self {
            amp_type: 0,
            bass: 0.5,
            middle: 0.5,
            treble: 0.5,
            presence: 0.5,
            gain: 0.5,
            volume: 0.5,
            dual_mono: false,
            dry_wet: 1.0,
        }
    }
}

pub fn parse_amp(node: Node<'_, '_>) -> AmpParams {
    AmpParams {
        amp_type: param_i32(node, "AmpType", 0),
        bass: param_f64(node, "Bass", 0.5),
        middle: param_f64(node, "Middle", 0.5),
        treble: param_f64(node, "Treble", 0.5),
        presence: param_f64(node, "Presence", 0.5),
        gain: param_f64(node, "Gain", 0.5),
        volume: param_f64(node, "Volume", 0.5),
        dual_mono: param_bool(node, "DualMono", false),
        dry_wet: param_f64(node, "DryWet", 1.0),
    }
}

pub fn write_amp<W: Write>(w: &mut AbletonXmlWriter<W>, p: &AmpParams) -> io::Result<()> {
    write_param_i32(w, "AmpType", p.amp_type)?;
    write_param_f64(w, "Bass", p.bass)?;
    write_param_f64(w, "Middle", p.middle)?;
    write_param_f64(w, "Treble", p.treble)?;
    write_param_f64(w, "Presence", p.presence)?;
    write_param_f64(w, "Gain", p.gain)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_bool(w, "DualMono", p.dual_mono)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Overdrive ────────────────────────────────────────────────────────────

/// Overdrive parameters.
#[derive(Debug, Clone)]
pub struct OverdriveParams {
    pub mid_freq: f64,
    pub band_width: f64,
    pub drive: f64,
    pub dry_wet: f64,
    pub tone: f64,
    pub preserve_dynamics: f64,
}

impl Default for OverdriveParams {
    fn default() -> Self {
        Self {
            mid_freq: 3000.0,
            band_width: 0.5,
            drive: 0.0,
            dry_wet: 1.0,
            tone: 0.5,
            preserve_dynamics: 0.0,
        }
    }
}

pub fn parse_overdrive(node: Node<'_, '_>) -> OverdriveParams {
    OverdriveParams {
        mid_freq: param_f64(node, "MidFreq", 3000.0),
        band_width: param_f64(node, "BandWidth", 0.5),
        drive: param_f64(node, "Drive", 0.0),
        dry_wet: param_f64(node, "DryWet", 1.0),
        tone: param_f64(node, "Tone", 0.5),
        preserve_dynamics: param_f64(node, "PreserveDynamics", 0.0),
    }
}

pub fn write_overdrive<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &OverdriveParams,
) -> io::Result<()> {
    write_param_f64(w, "MidFreq", p.mid_freq)?;
    write_param_f64(w, "BandWidth", p.band_width)?;
    write_param_f64(w, "Drive", p.drive)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "Tone", p.tone)?;
    write_param_f64(w, "PreserveDynamics", p.preserve_dynamics)?;
    Ok(())
}

// ─── Pedal ────────────────────────────────────────────────────────────────

/// Pedal parameters.
#[derive(Debug, Clone)]
pub struct PedalParams {
    /// 0=Overdrive, 1=Distortion, 2=Fuzz.
    pub pedal_type: i32,
    pub gain: f64,
    pub output: f64,
    pub bass: f64,
    pub mid: f64,
    pub treble: f64,
    /// Mid frequency selector.
    pub mid_freq: i32,
    pub sub: bool,
    pub dry_wet: f64,
    pub oversampling: bool,
}

impl Default for PedalParams {
    fn default() -> Self {
        Self {
            pedal_type: 0,
            gain: 0.5,
            output: 0.5,
            bass: 0.5,
            mid: 0.5,
            treble: 0.5,
            mid_freq: 0,
            sub: false,
            dry_wet: 1.0,
            oversampling: false,
        }
    }
}

pub fn parse_pedal(node: Node<'_, '_>) -> PedalParams {
    PedalParams {
        pedal_type: param_i32(node, "Type", 0),
        gain: param_f64(node, "Gain", 0.5),
        output: param_f64(node, "Output", 0.5),
        bass: param_f64(node, "Bass", 0.5),
        mid: param_f64(node, "Mid", 0.5),
        treble: param_f64(node, "Treble", 0.5),
        mid_freq: param_i32(node, "MidFreq", 0),
        sub: param_bool(node, "Sub", false),
        dry_wet: param_f64(node, "DryWet", 1.0),
        oversampling: param_bool(node, "Oversampling", false),
    }
}

pub fn write_pedal<W: Write>(w: &mut AbletonXmlWriter<W>, p: &PedalParams) -> io::Result<()> {
    write_param_i32(w, "Type", p.pedal_type)?;
    write_param_f64(w, "Gain", p.gain)?;
    write_param_f64(w, "Output", p.output)?;
    write_param_f64(w, "Bass", p.bass)?;
    write_param_f64(w, "Mid", p.mid)?;
    write_param_f64(w, "Treble", p.treble)?;
    write_param_i32(w, "MidFreq", p.mid_freq)?;
    write_param_bool(w, "Sub", p.sub)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_bool(w, "Oversampling", p.oversampling)?;
    Ok(())
}

// ─── DrumBuss ─────────────────────────────────────────────────────────────

/// DrumBuss parameters.
#[derive(Debug, Clone)]
pub struct DrumBussParams {
    pub enable_compression: bool,
    pub drive_amount: f64,
    pub drive_type: i32,
    pub crunch_amount: f64,
    pub dampening_frequency: f64,
    pub transient_shaping: f64,
    pub boom_frequency: f64,
    pub boom_amount: f64,
    pub boom_decay: f64,
    pub boom_audition: bool,
    pub input_trim: f64,
    pub output_gain: f64,
    pub dry_wet: f64,
}

impl Default for DrumBussParams {
    fn default() -> Self {
        Self {
            enable_compression: true,
            drive_amount: 0.0,
            drive_type: 0,
            crunch_amount: 0.0,
            dampening_frequency: 10000.0,
            transient_shaping: 0.0,
            boom_frequency: 80.0,
            boom_amount: 0.0,
            boom_decay: 0.5,
            boom_audition: false,
            input_trim: 0.0,
            output_gain: 0.0,
            dry_wet: 1.0,
        }
    }
}

pub fn parse_drum_buss(node: Node<'_, '_>) -> DrumBussParams {
    DrumBussParams {
        enable_compression: param_bool(node, "EnableCompression", true),
        drive_amount: param_f64(node, "DriveAmount", 0.0),
        drive_type: param_i32(node, "DriveType", 0),
        crunch_amount: param_f64(node, "CrunchAmount", 0.0),
        dampening_frequency: param_f64(node, "DampeningFrequency", 10000.0),
        transient_shaping: param_f64(node, "TransientShaping", 0.0),
        boom_frequency: param_f64(node, "BoomFrequency", 80.0),
        boom_amount: param_f64(node, "BoomAmount", 0.0),
        boom_decay: param_f64(node, "BoomDecay", 0.5),
        boom_audition: param_bool(node, "BoomAudition", false),
        input_trim: param_f64(node, "InputTrim", 0.0),
        output_gain: param_f64(node, "OutputGain", 0.0),
        dry_wet: param_f64(node, "DryWet", 1.0),
    }
}

pub fn write_drum_buss<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &DrumBussParams,
) -> io::Result<()> {
    write_param_bool(w, "EnableCompression", p.enable_compression)?;
    write_param_f64(w, "DriveAmount", p.drive_amount)?;
    write_param_i32(w, "DriveType", p.drive_type)?;
    write_param_f64(w, "CrunchAmount", p.crunch_amount)?;
    write_param_f64(w, "DampeningFrequency", p.dampening_frequency)?;
    write_param_f64(w, "TransientShaping", p.transient_shaping)?;
    write_param_f64(w, "BoomFrequency", p.boom_frequency)?;
    write_param_f64(w, "BoomAmount", p.boom_amount)?;
    write_param_f64(w, "BoomDecay", p.boom_decay)?;
    write_param_bool(w, "BoomAudition", p.boom_audition)?;
    write_param_f64(w, "InputTrim", p.input_trim)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Tube ─────────────────────────────────────────────────────────────────

/// Tube parameters.
#[derive(Debug, Clone)]
pub struct TubeParams {
    pub dry_wet: f64,
    pub pre_drive: f64,
    pub post_drive: f64,
    pub bias: f64,
    pub auto_bias: f64,
    pub auto_bias_attack: f64,
    pub auto_bias_release: f64,
    pub tone: f64,
    /// Tube type (enum).
    pub tube_type: i32,
    /// Oversampling mode (enum).
    pub oversampling: i32,
}

impl Default for TubeParams {
    fn default() -> Self {
        Self {
            dry_wet: 1.0,
            pre_drive: 0.0,
            post_drive: 0.0,
            bias: 0.0,
            auto_bias: 0.0,
            auto_bias_attack: 10.0,
            auto_bias_release: 100.0,
            tone: 0.5,
            tube_type: 0,
            oversampling: 0,
        }
    }
}

pub fn parse_tube(node: Node<'_, '_>) -> TubeParams {
    TubeParams {
        dry_wet: param_f64(node, "DryWet", 1.0),
        pre_drive: param_f64(node, "PreDrive", 0.0),
        post_drive: param_f64(node, "PostDrive", 0.0),
        bias: param_f64(node, "Bias", 0.0),
        auto_bias: param_f64(node, "AutoBias", 0.0),
        auto_bias_attack: param_f64(node, "AutoBiasAttack", 10.0),
        auto_bias_release: param_f64(node, "AutoBiasRelease", 100.0),
        tone: param_f64(node, "Tone", 0.5),
        tube_type: param_i32(node, "Type", 0),
        oversampling: param_i32(node, "Oversampling", 0),
    }
}

pub fn write_tube<W: Write>(w: &mut AbletonXmlWriter<W>, p: &TubeParams) -> io::Result<()> {
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "PreDrive", p.pre_drive)?;
    write_param_f64(w, "PostDrive", p.post_drive)?;
    write_param_f64(w, "Bias", p.bias)?;
    write_param_f64(w, "AutoBias", p.auto_bias)?;
    write_param_f64(w, "AutoBiasAttack", p.auto_bias_attack)?;
    write_param_f64(w, "AutoBiasRelease", p.auto_bias_release)?;
    write_param_f64(w, "Tone", p.tone)?;
    write_param_i32(w, "Type", p.tube_type)?;
    write_param_i32(w, "Oversampling", p.oversampling)?;
    Ok(())
}

// ─── Roar ─────────────────────────────────────────────────────────────────

/// A single Roar shaping stage.
#[derive(Debug, Clone)]
pub struct RoarStage {
    pub on: bool,
    pub shaper_on: bool,
    pub shaper_type: i32,
    pub shaper_amount: f64,
    pub shaper_bias: f64,
    pub shaper_trim: f64,
    pub filter_on: bool,
    pub filter_type: i32,
    pub filter_frequency: f64,
    pub filter_resonance: f64,
    pub filter_morph: f64,
    pub filter_peak_gain: f64,
    pub filter_pre_on: bool,
}

impl Default for RoarStage {
    fn default() -> Self {
        Self {
            on: true,
            shaper_on: true,
            shaper_type: 0,
            shaper_amount: 0.0,
            shaper_bias: 0.0,
            shaper_trim: 0.0,
            filter_on: false,
            filter_type: 0,
            filter_frequency: 1000.0,
            filter_resonance: 0.5,
            filter_morph: 0.0,
            filter_peak_gain: 0.0,
            filter_pre_on: false,
        }
    }
}

/// Roar parameters (simplified — captures core shaping, feedback, and output).
#[derive(Debug, Clone)]
pub struct RoarParams {
    // Input section
    pub input_gain: f64,
    pub input_tone_amount: f64,
    pub input_tone_frequency: f64,
    pub input_color_on: bool,
    pub input_routing_mode: i32,
    pub input_blend: f64,
    pub input_low_mid_crossover: f64,
    pub input_mid_high_crossover: f64,
    // Stages
    pub stages: [RoarStage; 3],
    // Feedback
    pub feedback_amount: f64,
    pub feedback_time_mode: i32,
    pub feedback_time: f64,
    pub feedback_synced_rate: f64,
    pub feedback_note: f64,
    pub feedback_frequency: f64,
    pub feedback_bandwidth: f64,
    pub feedback_invert: bool,
    pub feedback_gate_on: bool,
    // Global / output
    pub global_modulation_amount: f64,
    pub compression_amount: f64,
    pub compressor_highpass_filter_on: bool,
    pub output_gain: f64,
    pub dry_wet: f64,
    pub hi_quality: bool,
}

impl Default for RoarParams {
    fn default() -> Self {
        Self {
            input_gain: 0.0,
            input_tone_amount: 0.0,
            input_tone_frequency: 1000.0,
            input_color_on: false,
            input_routing_mode: 0,
            input_blend: 0.5,
            input_low_mid_crossover: 200.0,
            input_mid_high_crossover: 2000.0,
            stages: Default::default(),
            feedback_amount: 0.0,
            feedback_time_mode: 0,
            feedback_time: 100.0,
            feedback_synced_rate: 4.0,
            feedback_note: 60.0,
            feedback_frequency: 1000.0,
            feedback_bandwidth: 0.5,
            feedback_invert: false,
            feedback_gate_on: false,
            global_modulation_amount: 0.0,
            compression_amount: 0.0,
            compressor_highpass_filter_on: false,
            output_gain: 0.0,
            dry_wet: 1.0,
            hi_quality: false,
        }
    }
}

fn parse_roar_stage(node: Node<'_, '_>, prefix: &str) -> RoarStage {
    RoarStage {
        on: param_bool(node, &format!("{prefix}_On"), true),
        shaper_on: param_bool(node, &format!("{prefix}_Shaper_On"), true),
        shaper_type: param_i32(node, &format!("{prefix}_Shaper_Type"), 0),
        shaper_amount: param_f64(node, &format!("{prefix}_Shaper_Amount"), 0.0),
        shaper_bias: param_f64(node, &format!("{prefix}_Shaper_Bias"), 0.0),
        shaper_trim: param_f64(node, &format!("{prefix}_Shaper_Trim"), 0.0),
        filter_on: param_bool(node, &format!("{prefix}_Filter_On"), false),
        filter_type: param_i32(node, &format!("{prefix}_Filter_Type"), 0),
        filter_frequency: param_f64(node, &format!("{prefix}_Filter_Frequency"), 1000.0),
        filter_resonance: param_f64(node, &format!("{prefix}_Filter_Resonance"), 0.5),
        filter_morph: param_f64(node, &format!("{prefix}_Filter_Morph"), 0.0),
        filter_peak_gain: param_f64(node, &format!("{prefix}_Filter_PeakGain"), 0.0),
        filter_pre_on: param_bool(node, &format!("{prefix}_Filter_PreOn"), false),
    }
}

pub fn parse_roar(node: Node<'_, '_>) -> RoarParams {
    RoarParams {
        input_gain: param_f64(node, "Input_InputGain", 0.0),
        input_tone_amount: param_f64(node, "Input_ToneAmount", 0.0),
        input_tone_frequency: param_f64(node, "Input_ToneFrequency", 1000.0),
        input_color_on: param_bool(node, "Input_ColorOn", false),
        input_routing_mode: param_i32(node, "Input_RoutingMode", 0),
        input_blend: param_f64(node, "Input_Blend", 0.5),
        input_low_mid_crossover: param_f64(node, "Input_LowMidCrossover", 200.0),
        input_mid_high_crossover: param_f64(node, "Input_MidHighCrossover", 2000.0),
        stages: [
            parse_roar_stage(node, "Stage1"),
            parse_roar_stage(node, "Stage2"),
            parse_roar_stage(node, "Stage3"),
        ],
        feedback_amount: param_f64(node, "Feedback_FeedbackAmount", 0.0),
        feedback_time_mode: param_i32(node, "Feedback_FeedbackTimeMode", 0),
        feedback_time: param_f64(node, "Feedback_FeedbackTime", 100.0),
        feedback_synced_rate: param_f64(node, "Feedback_FeedbackSyncedRate", 4.0),
        feedback_note: param_f64(node, "Feedback_FeedbackNote", 60.0),
        feedback_frequency: param_f64(node, "Feedback_FeedbackFrequency", 1000.0),
        feedback_bandwidth: param_f64(node, "Feedback_FeedbackBandwidth", 0.5),
        feedback_invert: param_bool(node, "Feedback_FeedbackInvert", false),
        feedback_gate_on: param_bool(node, "Feedback_FeedbackGateOn", false),
        global_modulation_amount: param_f64(node, "GlobalModulationAmount", 0.0),
        compression_amount: param_f64(node, "Output_CompressionAmount", 0.0),
        compressor_highpass_filter_on: param_bool(node, "Output_CompressorHighpassFilterOn", false),
        output_gain: param_f64(node, "Output_OutputGain", 0.0),
        dry_wet: param_f64(node, "Output_DryWet", 1.0),
        hi_quality: param_bool(node, "HiQuality", false),
    }
}

fn write_roar_stage<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    prefix: &str,
    s: &RoarStage,
) -> io::Result<()> {
    write_param_bool(w, &format!("{prefix}_On"), s.on)?;
    write_param_bool(w, &format!("{prefix}_Shaper_On"), s.shaper_on)?;
    write_param_i32(w, &format!("{prefix}_Shaper_Type"), s.shaper_type)?;
    write_param_f64(w, &format!("{prefix}_Shaper_Amount"), s.shaper_amount)?;
    write_param_f64(w, &format!("{prefix}_Shaper_Bias"), s.shaper_bias)?;
    write_param_f64(w, &format!("{prefix}_Shaper_Trim"), s.shaper_trim)?;
    write_param_bool(w, &format!("{prefix}_Filter_On"), s.filter_on)?;
    write_param_i32(w, &format!("{prefix}_Filter_Type"), s.filter_type)?;
    write_param_f64(w, &format!("{prefix}_Filter_Frequency"), s.filter_frequency)?;
    write_param_f64(w, &format!("{prefix}_Filter_Resonance"), s.filter_resonance)?;
    write_param_f64(w, &format!("{prefix}_Filter_Morph"), s.filter_morph)?;
    write_param_f64(w, &format!("{prefix}_Filter_PeakGain"), s.filter_peak_gain)?;
    write_param_bool(w, &format!("{prefix}_Filter_PreOn"), s.filter_pre_on)?;
    Ok(())
}

pub fn write_roar<W: Write>(w: &mut AbletonXmlWriter<W>, p: &RoarParams) -> io::Result<()> {
    write_param_f64(w, "Input_InputGain", p.input_gain)?;
    write_param_f64(w, "Input_ToneAmount", p.input_tone_amount)?;
    write_param_f64(w, "Input_ToneFrequency", p.input_tone_frequency)?;
    write_param_bool(w, "Input_ColorOn", p.input_color_on)?;
    write_param_i32(w, "Input_RoutingMode", p.input_routing_mode)?;
    write_param_f64(w, "Input_Blend", p.input_blend)?;
    write_param_f64(w, "Input_LowMidCrossover", p.input_low_mid_crossover)?;
    write_param_f64(w, "Input_MidHighCrossover", p.input_mid_high_crossover)?;

    let prefixes = ["Stage1", "Stage2", "Stage3"];
    for (i, prefix) in prefixes.iter().enumerate() {
        write_roar_stage(w, prefix, &p.stages[i])?;
    }

    write_param_f64(w, "Feedback_FeedbackAmount", p.feedback_amount)?;
    write_param_i32(w, "Feedback_FeedbackTimeMode", p.feedback_time_mode)?;
    write_param_f64(w, "Feedback_FeedbackTime", p.feedback_time)?;
    write_param_f64(w, "Feedback_FeedbackSyncedRate", p.feedback_synced_rate)?;
    write_param_f64(w, "Feedback_FeedbackNote", p.feedback_note)?;
    write_param_f64(w, "Feedback_FeedbackFrequency", p.feedback_frequency)?;
    write_param_f64(w, "Feedback_FeedbackBandwidth", p.feedback_bandwidth)?;
    write_param_bool(w, "Feedback_FeedbackInvert", p.feedback_invert)?;
    write_param_bool(w, "Feedback_FeedbackGateOn", p.feedback_gate_on)?;
    write_param_f64(w, "GlobalModulationAmount", p.global_modulation_amount)?;
    write_param_f64(w, "Output_CompressionAmount", p.compression_amount)?;
    write_param_bool(
        w,
        "Output_CompressorHighpassFilterOn",
        p.compressor_highpass_filter_on,
    )?;
    write_param_f64(w, "Output_OutputGain", p.output_gain)?;
    write_param_f64(w, "Output_DryWet", p.dry_wet)?;
    write_param_bool(w, "HiQuality", p.hi_quality)?;
    Ok(())
}
