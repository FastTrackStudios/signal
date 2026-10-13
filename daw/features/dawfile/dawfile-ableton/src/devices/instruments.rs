//! Typed parameter models for Ableton built-in instrument devices.

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

// ─── Helpers ──────────────────────────────────────────────────────────────

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

/// Navigate into a nested section and read a param.
fn nested_f64(node: Node<'_, '_>, section: &str, name: &str, default: f64) -> f64 {
    child(node, section)
        .map(|s| param_f64(s, name, default))
        .unwrap_or(default)
}

fn nested_i32(node: Node<'_, '_>, section: &str, name: &str, default: i32) -> i32 {
    child(node, section)
        .map(|s| param_i32(s, name, default))
        .unwrap_or(default)
}

fn nested_bool(node: Node<'_, '_>, section: &str, name: &str, default: bool) -> bool {
    child(node, section)
        .map(|s| param_bool(s, name, default))
        .unwrap_or(default)
}

// ─── Simpler (OriginalSimpler) ────────────────────────────────────────────

/// Key parameters for Simpler.
#[derive(Debug, Clone)]
pub struct SimplerParams {
    /// Filter on/off.
    pub filter_on: bool,
    /// Filter type index.
    pub filter_type: i32,
    /// Filter frequency in Hz.
    pub filter_freq: f64,
    /// Filter resonance.
    pub filter_res: f64,
    /// Amp envelope attack.
    pub amp_attack: f64,
    /// Amp envelope decay.
    pub amp_decay: f64,
    /// Amp envelope sustain.
    pub amp_sustain: f64,
    /// Amp envelope release.
    pub amp_release: f64,
    /// Volume.
    pub volume: f64,
    /// Pan.
    pub pan: f64,
    /// Pitch coarse (semitones).
    pub pitch_coarse: f64,
    /// Pitch fine (cents).
    pub pitch_fine: f64,
}

impl Default for SimplerParams {
    fn default() -> Self {
        Self {
            filter_on: false,
            filter_type: 0,
            filter_freq: 18000.0,
            filter_res: 0.5,
            amp_attack: 0.0,
            amp_decay: 1.0,
            amp_sustain: 1.0,
            amp_release: 0.3,
            volume: 0.0,
            pan: 0.0,
            pitch_coarse: 0.0,
            pitch_fine: 0.0,
        }
    }
}

pub fn parse_simpler(node: Node<'_, '_>) -> SimplerParams {
    let mut p = SimplerParams::default();

    // Filter is nested: Filter > SimplerFilter
    if let Some(filter_holder) = child(node, "Filter") {
        // The actual filter params may be one level deeper
        let f = child(filter_holder, "IsOn")
            .map(|_| filter_holder)
            .or_else(|| child(filter_holder, "SimplerFilter"))
            .unwrap_or(filter_holder);
        p.filter_on = param_bool(f, "IsOn", false);
        p.filter_type = param_i32(f, "Type", 0);
        p.filter_freq = param_f64(f, "Freq", 18000.0);
        p.filter_res = param_f64(f, "Res", 0.5);
    }

    // AuxEnv holds the amp envelope
    if let Some(env_holder) = child(node, "AuxEnv") {
        let e = child(env_holder, "SimplerAuxEnvelope")
            .or_else(|| child(env_holder, "Envelope"))
            .unwrap_or(env_holder);
        p.amp_attack = param_f64(e, "AttackTime", 0.0);
        p.amp_decay = param_f64(e, "DecayTime", 1.0);
        p.amp_sustain = param_f64(e, "SustainLevel", 1.0);
        p.amp_release = param_f64(e, "ReleaseTime", 0.3);
    }

    // VolumeAndPan
    if let Some(vp) = child(node, "VolumeAndPan") {
        p.volume = param_f64(vp, "Volume", 0.0);
        p.pan = param_f64(vp, "Pan", 0.0);
    }

    // Pitch
    if let Some(pitch) = child(node, "Pitch") {
        p.pitch_coarse = param_f64(pitch, "PitchCoarse", 0.0);
        p.pitch_fine = param_f64(pitch, "PitchFine", 0.0);
    }

    p
}

pub fn write_simpler<W: Write>(w: &mut AbletonXmlWriter<W>, p: &SimplerParams) -> io::Result<()> {
    w.start("Filter")?;
    write_param_bool(w, "IsOn", p.filter_on)?;
    write_param_i32(w, "Type", p.filter_type)?;
    write_param_f64(w, "Freq", p.filter_freq)?;
    write_param_f64(w, "Res", p.filter_res)?;
    w.end("Filter")?;

    w.start("AuxEnv")?;
    write_param_f64(w, "AttackTime", p.amp_attack)?;
    write_param_f64(w, "DecayTime", p.amp_decay)?;
    write_param_f64(w, "SustainLevel", p.amp_sustain)?;
    write_param_f64(w, "ReleaseTime", p.amp_release)?;
    w.end("AuxEnv")?;

    w.start("VolumeAndPan")?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_f64(w, "Pan", p.pan)?;
    w.end("VolumeAndPan")?;

    w.start("Pitch")?;
    write_param_f64(w, "PitchCoarse", p.pitch_coarse)?;
    write_param_f64(w, "PitchFine", p.pitch_fine)?;
    w.end("Pitch")?;

    Ok(())
}

// ─── Sampler (MultiSampler) ──────────────────────────────────────────────

/// Key parameters for Sampler. Same structure as Simpler at the top level.
pub type SamplerParams = SimplerParams;

pub fn parse_sampler(node: Node<'_, '_>) -> SamplerParams {
    // Same nested structure as Simpler.
    parse_simpler(node)
}

pub fn write_sampler<W: Write>(w: &mut AbletonXmlWriter<W>, p: &SamplerParams) -> io::Result<()> {
    write_simpler(w, p)
}

// ─── Operator ─────────────────────────────────────────────────────────────

/// A single Operator oscillator.
#[derive(Debug, Clone)]
pub struct OperatorOsc {
    pub is_on: bool,
    pub volume: f64,
    pub coarse_tune: f64,
    pub fine_tune: f64,
    pub waveform: i32,
    pub feedback: f64,
    /// Envelope ADSR.
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
}

impl Default for OperatorOsc {
    fn default() -> Self {
        Self {
            is_on: true,
            volume: 0.5,
            coarse_tune: 0.0,
            fine_tune: 0.0,
            waveform: 0,
            feedback: 0.0,
            attack: 0.0,
            decay: 1.0,
            sustain: 0.5,
            release: 0.3,
        }
    }
}

/// Operator FM synth parameters.
#[derive(Debug, Clone)]
pub struct OperatorParams {
    /// Algorithm index (0-10).
    pub algorithm: i32,
    /// Global volume.
    pub volume: f64,
    /// Global tone.
    pub tone: f64,
    /// Four operator oscillators (A/B/C/D).
    pub operators: [OperatorOsc; 4],
    /// Filter on/off.
    pub filter_on: bool,
    /// Filter type.
    pub filter_type: i32,
    /// Filter frequency.
    pub filter_freq: f64,
    /// Filter resonance.
    pub filter_res: f64,
}

impl Default for OperatorParams {
    fn default() -> Self {
        Self {
            algorithm: 0,
            volume: 0.8,
            tone: 0.5,
            operators: Default::default(),
            filter_on: false,
            filter_type: 0,
            filter_freq: 18000.0,
            filter_res: 0.5,
        }
    }
}

/// Ableton uses lettered operator tags.
const OPERATOR_TAGS: [&str; 4] = ["OperatorA", "OperatorB", "OperatorC", "OperatorD"];

pub fn parse_operator(node: Node<'_, '_>) -> OperatorParams {
    let mut p = OperatorParams::default();

    // Globals section
    if let Some(g) = child(node, "Globals") {
        p.algorithm = param_i32(g, "Algorithm", 0);
        p.volume = param_f64(g, "Volume", 0.8);
        p.tone = param_f64(g, "Tone", 0.5);
    }

    // Per-operator
    for (i, tag) in OPERATOR_TAGS.iter().enumerate() {
        if let Some(op) = child(node, tag) {
            p.operators[i].is_on = param_bool(op, "IsOn", i == 0);
            p.operators[i].volume = param_f64(op, "Volume", 0.5);
            p.operators[i].waveform = param_i32(op, "WaveForm", 0);
            p.operators[i].feedback = param_f64(op, "Feedback", 0.0);
            if let Some(tune) = child(op, "Tune") {
                p.operators[i].coarse_tune = param_f64(tune, "Coarse", 0.0);
                p.operators[i].fine_tune = param_f64(tune, "Fine", 0.0);
            }
            if let Some(env) = child(op, "Envelope") {
                p.operators[i].attack = param_f64(env, "AttackTime", 0.0);
                p.operators[i].decay = param_f64(env, "DecayTime", 1.0);
                p.operators[i].sustain = param_f64(env, "SustainLevel", 0.5);
                p.operators[i].release = param_f64(env, "ReleaseTime", 0.3);
            }
        }
    }

    // Filter
    if let Some(f) = child(node, "Filter") {
        p.filter_on = param_bool(f, "OnOff", false);
        p.filter_type = param_i32(f, "Type", 0);
        p.filter_freq = param_f64(f, "Frequency", 18000.0);
        p.filter_res = param_f64(f, "Resonance", 0.5);
    }

    p
}

pub fn write_operator<W: Write>(w: &mut AbletonXmlWriter<W>, p: &OperatorParams) -> io::Result<()> {
    w.start("Globals")?;
    write_param_i32(w, "Algorithm", p.algorithm)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_f64(w, "Tone", p.tone)?;
    w.end("Globals")?;

    for (i, tag) in OPERATOR_TAGS.iter().enumerate() {
        let op = &p.operators[i];
        w.start(tag)?;
        write_param_bool(w, "IsOn", op.is_on)?;
        write_param_f64(w, "Volume", op.volume)?;
        write_param_i32(w, "WaveForm", op.waveform)?;
        write_param_f64(w, "Feedback", op.feedback)?;
        w.start("Tune")?;
        write_param_f64(w, "Coarse", op.coarse_tune)?;
        write_param_f64(w, "Fine", op.fine_tune)?;
        w.end("Tune")?;
        w.start("Envelope")?;
        write_param_f64(w, "AttackTime", op.attack)?;
        write_param_f64(w, "DecayTime", op.decay)?;
        write_param_f64(w, "SustainLevel", op.sustain)?;
        write_param_f64(w, "ReleaseTime", op.release)?;
        w.end("Envelope")?;
        w.end(tag)?;
    }

    w.start("Filter")?;
    write_param_bool(w, "OnOff", p.filter_on)?;
    write_param_i32(w, "Type", p.filter_type)?;
    write_param_f64(w, "Frequency", p.filter_freq)?;
    write_param_f64(w, "Resonance", p.filter_res)?;
    w.end("Filter")?;

    Ok(())
}

// ─── Drift ────────────────────────────────────────────────────────────────

/// Drift analog-style synth parameters.
#[derive(Debug, Clone)]
pub struct DriftParams {
    // Oscillators
    pub osc1_type: i32,
    pub osc1_shape: f64,
    pub osc1_on: bool,
    pub osc2_type: i32,
    pub osc2_detune: f64,
    pub osc2_transpose: f64,
    pub osc2_on: bool,
    // Mixer
    pub mixer_osc1_gain: f64,
    pub mixer_osc2_gain: f64,
    pub mixer_noise_level: f64,
    pub mixer_noise_on: bool,
    // Filter
    pub filter_type: i32,
    pub filter_freq: f64,
    pub filter_res: f64,
    pub filter_env_amount: f64,
    // Amp envelope (Envelope1)
    pub amp_attack: f64,
    pub amp_decay: f64,
    pub amp_sustain: f64,
    pub amp_release: f64,
    // Filter envelope (Envelope2)
    pub filt_attack: f64,
    pub filt_decay: f64,
    pub filt_sustain: f64,
    pub filt_release: f64,
    // LFO
    pub lfo_shape: i32,
    pub lfo_rate: f64,
    pub lfo_amount: f64,
    // Global
    pub voice_mode: i32,
    pub volume: f64,
}

impl Default for DriftParams {
    fn default() -> Self {
        Self {
            osc1_type: 0,
            osc1_shape: 0.5,
            osc1_on: true,
            osc2_type: 0,
            osc2_detune: 0.0,
            osc2_transpose: 0.0,
            osc2_on: false,
            mixer_osc1_gain: 0.8,
            mixer_osc2_gain: 0.8,
            mixer_noise_level: 0.0,
            mixer_noise_on: false,
            filter_type: 0,
            filter_freq: 18000.0,
            filter_res: 0.0,
            filter_env_amount: 0.0,
            amp_attack: 0.001,
            amp_decay: 0.5,
            amp_sustain: 0.7,
            amp_release: 0.3,
            filt_attack: 0.001,
            filt_decay: 0.5,
            filt_sustain: 0.7,
            filt_release: 0.3,
            lfo_shape: 0,
            lfo_rate: 1.0,
            lfo_amount: 0.0,
            voice_mode: 0,
            volume: 0.8,
        }
    }
}

pub fn parse_drift(node: Node<'_, '_>) -> DriftParams {
    DriftParams {
        osc1_type: param_i32(node, "Oscillator1_Type", 0),
        osc1_shape: param_f64(node, "Oscillator1_Shape", 0.5),
        osc1_on: param_bool(node, "Mixer_OscillatorOn1", true),
        osc2_type: param_i32(node, "Oscillator2_Type", 0),
        osc2_detune: param_f64(node, "Oscillator2_Detune", 0.0),
        osc2_transpose: param_f64(node, "Oscillator2_Transpose", 0.0),
        osc2_on: param_bool(node, "Mixer_OscillatorOn2", false),
        mixer_osc1_gain: param_f64(node, "Mixer_OscillatorGain1", 0.8),
        mixer_osc2_gain: param_f64(node, "Mixer_OscillatorGain2", 0.8),
        mixer_noise_level: param_f64(node, "Mixer_NoiseLevel", 0.0),
        mixer_noise_on: param_bool(node, "Mixer_NoiseOn", false),
        filter_type: param_i32(node, "Filter_Type", 0),
        filter_freq: param_f64(node, "Filter_Frequency", 18000.0),
        filter_res: param_f64(node, "Filter_Resonance", 0.0),
        filter_env_amount: param_f64(node, "Filter_ModAmount1", 0.0),
        amp_attack: param_f64(node, "Envelope1_Attack", 0.001),
        amp_decay: param_f64(node, "Envelope1_Decay", 0.5),
        amp_sustain: param_f64(node, "Envelope1_Sustain", 0.7),
        amp_release: param_f64(node, "Envelope1_Release", 0.3),
        filt_attack: param_f64(node, "Envelope2_Attack", 0.001),
        filt_decay: param_f64(node, "Envelope2_Decay", 0.5),
        filt_sustain: param_f64(node, "Envelope2_Sustain", 0.7),
        filt_release: param_f64(node, "Envelope2_Release", 0.3),
        lfo_shape: param_i32(node, "Lfo_Shape", 0),
        lfo_rate: param_f64(node, "Lfo_Rate", 1.0),
        lfo_amount: param_f64(node, "Lfo_Amount", 0.0),
        voice_mode: param_i32(node, "Global_VoiceMode", 0),
        volume: param_f64(node, "Global_Volume", 0.8),
    }
}

pub fn write_drift<W: Write>(w: &mut AbletonXmlWriter<W>, p: &DriftParams) -> io::Result<()> {
    write_param_i32(w, "Oscillator1_Type", p.osc1_type)?;
    write_param_f64(w, "Oscillator1_Shape", p.osc1_shape)?;
    write_param_i32(w, "Oscillator2_Type", p.osc2_type)?;
    write_param_f64(w, "Oscillator2_Detune", p.osc2_detune)?;
    write_param_f64(w, "Oscillator2_Transpose", p.osc2_transpose)?;
    write_param_f64(w, "Mixer_OscillatorGain1", p.mixer_osc1_gain)?;
    write_param_f64(w, "Mixer_OscillatorGain2", p.mixer_osc2_gain)?;
    write_param_bool(w, "Mixer_OscillatorOn1", p.osc1_on)?;
    write_param_bool(w, "Mixer_OscillatorOn2", p.osc2_on)?;
    write_param_f64(w, "Mixer_NoiseLevel", p.mixer_noise_level)?;
    write_param_bool(w, "Mixer_NoiseOn", p.mixer_noise_on)?;
    write_param_i32(w, "Filter_Type", p.filter_type)?;
    write_param_f64(w, "Filter_Frequency", p.filter_freq)?;
    write_param_f64(w, "Filter_Resonance", p.filter_res)?;
    write_param_f64(w, "Filter_ModAmount1", p.filter_env_amount)?;
    write_param_f64(w, "Envelope1_Attack", p.amp_attack)?;
    write_param_f64(w, "Envelope1_Decay", p.amp_decay)?;
    write_param_f64(w, "Envelope1_Sustain", p.amp_sustain)?;
    write_param_f64(w, "Envelope1_Release", p.amp_release)?;
    write_param_f64(w, "Envelope2_Attack", p.filt_attack)?;
    write_param_f64(w, "Envelope2_Decay", p.filt_decay)?;
    write_param_f64(w, "Envelope2_Sustain", p.filt_sustain)?;
    write_param_f64(w, "Envelope2_Release", p.filt_release)?;
    write_param_i32(w, "Lfo_Shape", p.lfo_shape)?;
    write_param_f64(w, "Lfo_Rate", p.lfo_rate)?;
    write_param_f64(w, "Lfo_Amount", p.lfo_amount)?;
    write_param_i32(w, "Global_VoiceMode", p.voice_mode)?;
    write_param_f64(w, "Global_Volume", p.volume)?;
    Ok(())
}

// ─── Wavetable (InstrumentVector) ─────────────────────────────────────────

/// Wavetable synth key parameters.
#[derive(Debug, Clone)]
pub struct WavetableParams {
    // Oscillators
    pub osc1_on: bool,
    pub osc1_position: f64,
    pub osc1_transpose: f64,
    pub osc1_gain: f64,
    pub osc2_on: bool,
    pub osc2_position: f64,
    pub osc2_transpose: f64,
    pub osc2_gain: f64,
    // Filters
    pub filter1_on: bool,
    pub filter1_type: i32,
    pub filter1_freq: f64,
    pub filter1_res: f64,
    pub filter2_on: bool,
    pub filter2_type: i32,
    pub filter2_freq: f64,
    pub filter2_res: f64,
    // Amp envelope
    pub amp_attack: f64,
    pub amp_decay: f64,
    pub amp_sustain: f64,
    pub amp_release: f64,
    // Unison
    pub unison_mode: i32,
    pub unison_voices: f64,
    pub unison_amount: f64,
    // Global
    pub volume: f64,
    pub mono_poly: i32,
    pub poly_voices: i32,
}

impl Default for WavetableParams {
    fn default() -> Self {
        Self {
            osc1_on: true,
            osc1_position: 0.0,
            osc1_transpose: 0.0,
            osc1_gain: 0.8,
            osc2_on: false,
            osc2_position: 0.0,
            osc2_transpose: 0.0,
            osc2_gain: 0.8,
            filter1_on: false,
            filter1_type: 0,
            filter1_freq: 18000.0,
            filter1_res: 0.0,
            filter2_on: false,
            filter2_type: 0,
            filter2_freq: 18000.0,
            filter2_res: 0.0,
            amp_attack: 0.0,
            amp_decay: 0.5,
            amp_sustain: 1.0,
            amp_release: 0.3,
            unison_mode: 0,
            unison_voices: 1.0,
            unison_amount: 0.0,
            volume: 0.8,
            mono_poly: 0,
            poly_voices: 8,
        }
    }
}

pub fn parse_wavetable(node: Node<'_, '_>) -> WavetableParams {
    WavetableParams {
        osc1_on: param_bool(node, "Voice_Oscillator1_On", true),
        osc1_position: param_f64(node, "Voice_Oscillator1_Wavetables_WavePosition", 0.0),
        osc1_transpose: param_f64(node, "Voice_Oscillator1_Pitch_Transpose", 0.0),
        osc1_gain: param_f64(node, "Voice_Oscillator1_Gain", 0.8),
        osc2_on: param_bool(node, "Voice_Oscillator2_On", false),
        osc2_position: param_f64(node, "Voice_Oscillator2_Wavetables_WavePosition", 0.0),
        osc2_transpose: param_f64(node, "Voice_Oscillator2_Pitch_Transpose", 0.0),
        osc2_gain: param_f64(node, "Voice_Oscillator2_Gain", 0.8),
        filter1_on: param_bool(node, "Voice_Filter1_On", false),
        filter1_type: param_i32(node, "Voice_Filter1_Type", 0),
        filter1_freq: param_f64(node, "Voice_Filter1_Frequency", 18000.0),
        filter1_res: param_f64(node, "Voice_Filter1_Resonance", 0.0),
        filter2_on: param_bool(node, "Voice_Filter2_On", false),
        filter2_type: param_i32(node, "Voice_Filter2_Type", 0),
        filter2_freq: param_f64(node, "Voice_Filter2_Frequency", 18000.0),
        filter2_res: param_f64(node, "Voice_Filter2_Resonance", 0.0),
        amp_attack: param_f64(node, "Voice_Modulators_AmpEnvelope_Times_Attack", 0.0),
        amp_decay: param_f64(node, "Voice_Modulators_AmpEnvelope_Times_Decay", 0.5),
        amp_sustain: param_f64(node, "Voice_Modulators_AmpEnvelope_Sustain", 1.0),
        amp_release: param_f64(node, "Voice_Modulators_AmpEnvelope_Times_Release", 0.3),
        unison_mode: param_i32(node, "Voice_Unison_Mode", 0),
        unison_voices: param_f64(node, "Voice_Unison_VoiceCount", 1.0),
        unison_amount: param_f64(node, "Voice_Unison_Amount", 0.0),
        volume: param_f64(node, "Volume", 0.8),
        mono_poly: param_i32(node, "MonoPoly", 0),
        poly_voices: param_i32(node, "PolyVoices", 8),
    }
}

pub fn write_wavetable<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &WavetableParams,
) -> io::Result<()> {
    write_param_bool(w, "Voice_Oscillator1_On", p.osc1_on)?;
    write_param_f64(
        w,
        "Voice_Oscillator1_Wavetables_WavePosition",
        p.osc1_position,
    )?;
    write_param_f64(w, "Voice_Oscillator1_Pitch_Transpose", p.osc1_transpose)?;
    write_param_f64(w, "Voice_Oscillator1_Gain", p.osc1_gain)?;
    write_param_bool(w, "Voice_Oscillator2_On", p.osc2_on)?;
    write_param_f64(
        w,
        "Voice_Oscillator2_Wavetables_WavePosition",
        p.osc2_position,
    )?;
    write_param_f64(w, "Voice_Oscillator2_Pitch_Transpose", p.osc2_transpose)?;
    write_param_f64(w, "Voice_Oscillator2_Gain", p.osc2_gain)?;
    write_param_bool(w, "Voice_Filter1_On", p.filter1_on)?;
    write_param_i32(w, "Voice_Filter1_Type", p.filter1_type)?;
    write_param_f64(w, "Voice_Filter1_Frequency", p.filter1_freq)?;
    write_param_f64(w, "Voice_Filter1_Resonance", p.filter1_res)?;
    write_param_bool(w, "Voice_Filter2_On", p.filter2_on)?;
    write_param_i32(w, "Voice_Filter2_Type", p.filter2_type)?;
    write_param_f64(w, "Voice_Filter2_Frequency", p.filter2_freq)?;
    write_param_f64(w, "Voice_Filter2_Resonance", p.filter2_res)?;
    write_param_f64(w, "Voice_Modulators_AmpEnvelope_Times_Attack", p.amp_attack)?;
    write_param_f64(w, "Voice_Modulators_AmpEnvelope_Times_Decay", p.amp_decay)?;
    write_param_f64(w, "Voice_Modulators_AmpEnvelope_Sustain", p.amp_sustain)?;
    write_param_f64(
        w,
        "Voice_Modulators_AmpEnvelope_Times_Release",
        p.amp_release,
    )?;
    write_param_i32(w, "Voice_Unison_Mode", p.unison_mode)?;
    write_param_f64(w, "Voice_Unison_VoiceCount", p.unison_voices)?;
    write_param_f64(w, "Voice_Unison_Amount", p.unison_amount)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_i32(w, "MonoPoly", p.mono_poly)?;
    write_param_i32(w, "PolyVoices", p.poly_voices)?;
    Ok(())
}

// ─── Meld (InstrumentMeld) ───────────────────────────────────────────────

/// Meld engine parameters (A or B).
#[derive(Debug, Clone)]
pub struct MeldEngine {
    pub on: bool,
    pub osc_type: i32,
    pub osc_transpose: f64,
    pub osc_detune: f64,
    pub osc_macro1: f64,
    pub osc_macro2: f64,
    pub filter_on: bool,
    pub filter_type: i32,
    pub filter_freq: f64,
    pub amp_attack: f64,
    pub amp_decay: f64,
    pub amp_sustain: f64,
    pub amp_release: f64,
    pub volume: f64,
    pub pan: f64,
}

impl Default for MeldEngine {
    fn default() -> Self {
        Self {
            on: true,
            osc_type: 0,
            osc_transpose: 0.0,
            osc_detune: 0.0,
            osc_macro1: 0.0,
            osc_macro2: 0.0,
            filter_on: false,
            filter_type: 0,
            filter_freq: 18000.0,
            amp_attack: 0.001,
            amp_decay: 0.5,
            amp_sustain: 1.0,
            amp_release: 0.3,
            volume: 0.8,
            pan: 0.0,
        }
    }
}

/// Meld synth parameters.
#[derive(Debug, Clone)]
pub struct MeldParams {
    pub engine_a: MeldEngine,
    pub engine_b: MeldEngine,
    pub drive: f64,
    pub volume: f64,
    pub mono_poly: i32,
    pub poly_voices: i32,
}

impl Default for MeldParams {
    fn default() -> Self {
        Self {
            engine_a: MeldEngine::default(),
            engine_b: MeldEngine {
                on: false,
                ..Default::default()
            },
            drive: 0.0,
            volume: 0.8,
            mono_poly: 0,
            poly_voices: 8,
        }
    }
}

fn parse_meld_engine(node: Node<'_, '_>, prefix: &str) -> MeldEngine {
    let p = |suffix: &str, def: f64| param_f64(node, &format!("{prefix}{suffix}"), def);
    let pi = |suffix: &str, def: i32| param_i32(node, &format!("{prefix}{suffix}"), def);
    let pb = |suffix: &str, def: bool| param_bool(node, &format!("{prefix}{suffix}"), def);

    MeldEngine {
        on: pb("_On", true),
        osc_type: pi("_Oscillator_OscillatorType", 0),
        osc_transpose: p("_Oscillator_Pitch_Transpose", 0.0),
        osc_detune: p("_Oscillator_Pitch_Detune", 0.0),
        osc_macro1: p("_Oscillator_Macro1", 0.0),
        osc_macro2: p("_Oscillator_Macro2", 0.0),
        filter_on: pb("_Filter_On", false),
        filter_type: pi("_Filter_FilterType", 0),
        filter_freq: p("_Filter_Frequency", 18000.0),
        amp_attack: p("_AmpEnvelope_Times_Attack", 0.001),
        amp_decay: p("_AmpEnvelope_Times_Decay", 0.5),
        amp_sustain: p("_AmpEnvelope_Sustain", 1.0),
        amp_release: p("_AmpEnvelope_Times_Release", 0.3),
        volume: p("_Volume", 0.8),
        pan: p("_Pan", 0.0),
    }
}

fn write_meld_engine<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    prefix: &str,
    e: &MeldEngine,
) -> io::Result<()> {
    write_param_bool(w, &format!("{prefix}_On"), e.on)?;
    write_param_i32(
        w,
        &format!("{prefix}_Oscillator_OscillatorType"),
        e.osc_type,
    )?;
    write_param_f64(
        w,
        &format!("{prefix}_Oscillator_Pitch_Transpose"),
        e.osc_transpose,
    )?;
    write_param_f64(
        w,
        &format!("{prefix}_Oscillator_Pitch_Detune"),
        e.osc_detune,
    )?;
    write_param_f64(w, &format!("{prefix}_Oscillator_Macro1"), e.osc_macro1)?;
    write_param_f64(w, &format!("{prefix}_Oscillator_Macro2"), e.osc_macro2)?;
    write_param_bool(w, &format!("{prefix}_Filter_On"), e.filter_on)?;
    write_param_i32(w, &format!("{prefix}_Filter_FilterType"), e.filter_type)?;
    write_param_f64(w, &format!("{prefix}_Filter_Frequency"), e.filter_freq)?;
    write_param_f64(
        w,
        &format!("{prefix}_AmpEnvelope_Times_Attack"),
        e.amp_attack,
    )?;
    write_param_f64(w, &format!("{prefix}_AmpEnvelope_Times_Decay"), e.amp_decay)?;
    write_param_f64(w, &format!("{prefix}_AmpEnvelope_Sustain"), e.amp_sustain)?;
    write_param_f64(
        w,
        &format!("{prefix}_AmpEnvelope_Times_Release"),
        e.amp_release,
    )?;
    write_param_f64(w, &format!("{prefix}_Volume"), e.volume)?;
    write_param_f64(w, &format!("{prefix}_Pan"), e.pan)?;
    Ok(())
}

pub fn parse_meld(node: Node<'_, '_>) -> MeldParams {
    MeldParams {
        engine_a: parse_meld_engine(node, "MeldVoice_EngineA"),
        engine_b: parse_meld_engine(node, "MeldVoice_EngineB"),
        drive: param_f64(node, "MeldVoice_Drive", 0.0),
        volume: param_f64(node, "Volume", 0.8),
        mono_poly: param_i32(node, "MonoPoly", 0),
        poly_voices: param_i32(node, "PolyVoices", 8),
    }
}

pub fn write_meld<W: Write>(w: &mut AbletonXmlWriter<W>, p: &MeldParams) -> io::Result<()> {
    write_meld_engine(w, "MeldVoice_EngineA", &p.engine_a)?;
    write_meld_engine(w, "MeldVoice_EngineB", &p.engine_b)?;
    write_param_f64(w, "MeldVoice_Drive", p.drive)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_i32(w, "MonoPoly", p.mono_poly)?;
    write_param_i32(w, "PolyVoices", p.poly_voices)?;
    Ok(())
}

// ─── Collision ────────────────────────────────────────────────────────────

/// Collision physical modeling parameters.
#[derive(Debug, Clone)]
pub struct CollisionParams {
    // Mallet exciter
    pub mallet_on: bool,
    pub mallet_volume: f64,
    pub mallet_stiffness: f64,
    pub mallet_noise: f64,
    // Noise exciter
    pub noise_on: bool,
    pub noise_volume: f64,
    pub noise_filter_type: i32,
    pub noise_filter_freq: f64,
    pub noise_filter_q: f64,
    // Resonator 1
    pub res1_on: bool,
    pub res1_type: i32,
    pub res1_transpose: f64,
    pub res1_decay: f64,
    pub res1_volume: f64,
    // Resonator 2
    pub res2_on: bool,
    pub res2_type: i32,
    pub res2_transpose: f64,
    pub res2_decay: f64,
    pub res2_volume: f64,
    // Global
    pub resonator_order: i32,
    pub polyphony: i32,
    pub volume: f64,
}

impl Default for CollisionParams {
    fn default() -> Self {
        Self {
            mallet_on: true,
            mallet_volume: 0.8,
            mallet_stiffness: 50.0,
            mallet_noise: 0.0,
            noise_on: false,
            noise_volume: 0.5,
            noise_filter_type: 0,
            noise_filter_freq: 2000.0,
            noise_filter_q: 0.5,
            res1_on: true,
            res1_type: 0,
            res1_transpose: 0.0,
            res1_decay: 4.0,
            res1_volume: 0.8,
            res2_on: false,
            res2_type: 0,
            res2_transpose: 0.0,
            res2_decay: 4.0,
            res2_volume: 0.8,
            resonator_order: 0,
            polyphony: 6,
            volume: 0.8,
        }
    }
}

pub fn parse_collision(node: Node<'_, '_>) -> CollisionParams {
    let mut p = CollisionParams::default();

    if let Some(m) = child(node, "Mallet") {
        p.mallet_on = param_bool(m, "OnOff", true);
        p.mallet_volume = param_f64(m, "Volume", 0.8);
        p.mallet_stiffness = param_f64(m, "Stiffness", 50.0);
        p.mallet_noise = param_f64(m, "NoiseAmount", 0.0);
    }
    if let Some(n) = child(node, "Noise") {
        p.noise_on = param_bool(n, "OnOff", false);
        p.noise_volume = param_f64(n, "Volume", 0.5);
        p.noise_filter_type = param_i32(n, "FilterType", 0);
        p.noise_filter_freq = param_f64(n, "Freq", 2000.0);
        p.noise_filter_q = param_f64(n, "Q", 0.5);
    }
    if let Some(r) = child(node, "Resonator1") {
        p.res1_on = param_bool(r, "OnOff", true);
        p.res1_type = param_i32(r, "Type", 0);
        p.res1_transpose = param_f64(r, "Transpose", 0.0);
        p.res1_decay = param_f64(r, "Decay", 4.0);
        p.res1_volume = param_f64(r, "Volume", 0.8);
    }
    if let Some(r) = child(node, "Resonator2") {
        p.res2_on = param_bool(r, "OnOff", false);
        p.res2_type = param_i32(r, "Type", 0);
        p.res2_transpose = param_f64(r, "Transpose", 0.0);
        p.res2_decay = param_f64(r, "Decay", 4.0);
        p.res2_volume = param_f64(r, "Volume", 0.8);
    }
    p.resonator_order = param_i32(node, "ResonatorOrder", 0);
    p.polyphony = param_i32(node, "Polyphony", 6);
    p.volume = param_f64(node, "Volume", 0.8);

    p
}

pub fn write_collision<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &CollisionParams,
) -> io::Result<()> {
    w.start("Mallet")?;
    write_param_bool(w, "OnOff", p.mallet_on)?;
    write_param_f64(w, "Volume", p.mallet_volume)?;
    write_param_f64(w, "Stiffness", p.mallet_stiffness)?;
    write_param_f64(w, "NoiseAmount", p.mallet_noise)?;
    w.end("Mallet")?;

    w.start("Noise")?;
    write_param_bool(w, "OnOff", p.noise_on)?;
    write_param_f64(w, "Volume", p.noise_volume)?;
    write_param_i32(w, "FilterType", p.noise_filter_type)?;
    write_param_f64(w, "Freq", p.noise_filter_freq)?;
    write_param_f64(w, "Q", p.noise_filter_q)?;
    w.end("Noise")?;

    w.start("Resonator1")?;
    write_param_bool(w, "OnOff", p.res1_on)?;
    write_param_i32(w, "Type", p.res1_type)?;
    write_param_f64(w, "Transpose", p.res1_transpose)?;
    write_param_f64(w, "Decay", p.res1_decay)?;
    write_param_f64(w, "Volume", p.res1_volume)?;
    w.end("Resonator1")?;

    w.start("Resonator2")?;
    write_param_bool(w, "OnOff", p.res2_on)?;
    write_param_i32(w, "Type", p.res2_type)?;
    write_param_f64(w, "Transpose", p.res2_transpose)?;
    write_param_f64(w, "Decay", p.res2_decay)?;
    write_param_f64(w, "Volume", p.res2_volume)?;
    w.end("Resonator2")?;

    write_param_i32(w, "ResonatorOrder", p.resonator_order)?;
    write_param_i32(w, "Polyphony", p.polyphony)?;
    write_param_f64(w, "Volume", p.volume)?;

    Ok(())
}

// ─── Tension (StringStudio) ──────────────────────────────────────────────

/// Tension string physical model key parameters.
#[derive(Debug, Clone)]
pub struct TensionParams {
    // Excitator
    pub excitator_on: bool,
    pub excitator_type: i32,
    pub excitator_stiffness: f64,
    pub excitator_velocity: f64,
    // String
    pub string_damping: f64,
    pub string_decay: f64,
    pub string_decay_ratio: f64,
    pub string_inharmonicity: f64,
    // Body
    pub body_on: bool,
    pub body_type: i32,
    pub body_size: i32,
    pub body_decay: f64,
    pub body_level: f64,
    // Filter
    pub filter_on: bool,
    pub filter_type: i32,
    pub filter_freq: f64,
    pub filter_q: f64,
    // Global
    pub polyphony: i32,
    pub volume: f64,
    pub transpose: f64,
}

impl Default for TensionParams {
    fn default() -> Self {
        Self {
            excitator_on: true,
            excitator_type: 0,
            excitator_stiffness: 0.5,
            excitator_velocity: 0.5,
            string_damping: 0.5,
            string_decay: 0.5,
            string_decay_ratio: 0.5,
            string_inharmonicity: 0.0,
            body_on: false,
            body_type: 0,
            body_size: 2,
            body_decay: 0.5,
            body_level: 0.5,
            filter_on: false,
            filter_type: 0,
            filter_freq: 18000.0,
            filter_q: 0.5,
            polyphony: 6,
            volume: 0.0,
            transpose: 0.0,
        }
    }
}

pub fn parse_tension(node: Node<'_, '_>) -> TensionParams {
    TensionParams {
        excitator_on: param_bool(node, "ExcitatorToggle", true),
        excitator_type: param_i32(node, "ExcitatorType", 0),
        excitator_stiffness: param_f64(node, "ExcitatorStiffness", 0.5),
        excitator_velocity: param_f64(node, "ExcitatorVelocity", 0.5),
        string_damping: param_f64(node, "StringDamping", 0.5),
        string_decay: param_f64(node, "StringDecay", 0.5),
        string_decay_ratio: param_f64(node, "StringDecayRatio", 0.5),
        string_inharmonicity: param_f64(node, "StringInharmonicity", 0.0),
        body_on: param_bool(node, "BodyToggle", false),
        body_type: param_i32(node, "BodyType", 0),
        body_size: param_i32(node, "BodySize", 2),
        body_decay: param_f64(node, "BodyDecay", 0.5),
        body_level: param_f64(node, "BodyLevel", 0.5),
        filter_on: param_bool(node, "FilterToggle", false),
        filter_type: param_i32(node, "FilterType", 0),
        filter_freq: param_f64(node, "FilterCutoffFrequency", 18000.0),
        filter_q: param_f64(node, "FilterQFactor", 0.5),
        polyphony: param_i32(node, "Polyphony", 6),
        volume: param_f64(node, "Volume", 0.0),
        transpose: param_f64(node, "Transpose", 0.0),
    }
}

pub fn write_tension<W: Write>(w: &mut AbletonXmlWriter<W>, p: &TensionParams) -> io::Result<()> {
    write_param_bool(w, "ExcitatorToggle", p.excitator_on)?;
    write_param_i32(w, "ExcitatorType", p.excitator_type)?;
    write_param_f64(w, "ExcitatorStiffness", p.excitator_stiffness)?;
    write_param_f64(w, "ExcitatorVelocity", p.excitator_velocity)?;
    write_param_f64(w, "StringDamping", p.string_damping)?;
    write_param_f64(w, "StringDecay", p.string_decay)?;
    write_param_f64(w, "StringDecayRatio", p.string_decay_ratio)?;
    write_param_f64(w, "StringInharmonicity", p.string_inharmonicity)?;
    write_param_bool(w, "BodyToggle", p.body_on)?;
    write_param_i32(w, "BodyType", p.body_type)?;
    write_param_i32(w, "BodySize", p.body_size)?;
    write_param_f64(w, "BodyDecay", p.body_decay)?;
    write_param_f64(w, "BodyLevel", p.body_level)?;
    write_param_bool(w, "FilterToggle", p.filter_on)?;
    write_param_i32(w, "FilterType", p.filter_type)?;
    write_param_f64(w, "FilterCutoffFrequency", p.filter_freq)?;
    write_param_f64(w, "FilterQFactor", p.filter_q)?;
    write_param_i32(w, "Polyphony", p.polyphony)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_f64(w, "Transpose", p.transpose)?;
    Ok(())
}

// ─── Electric (LoungeLizard) ─────────────────────────────────────────────

/// Electric piano model key parameters.
#[derive(Debug, Clone)]
pub struct ElectricParams {
    // Mallet
    pub mallet_stiffness: f64,
    pub mallet_force: f64,
    pub mallet_noise_amount: f64,
    pub mallet_noise_pitch: f64,
    pub mallet_noise_decay: f64,
    // Fork
    pub fork_tine_decay: f64,
    pub fork_tine_volume: f64,
    pub fork_tine_color: f64,
    pub fork_tone_decay: f64,
    pub fork_tone_volume: f64,
    pub fork_release: f64,
    // Damper
    pub damper_tone: f64,
    pub damper_amount: f64,
    pub damper_balance: f64,
    // Pickup
    pub pickup_symmetry: f64,
    pub pickup_distance: f64,
    pub pickup_model: i32,
    // Global
    pub polyphony: i32,
    pub volume: f64,
    pub keyboard_transpose: f64,
}

impl Default for ElectricParams {
    fn default() -> Self {
        Self {
            mallet_stiffness: 50.0,
            mallet_force: 0.5,
            mallet_noise_amount: 0.3,
            mallet_noise_pitch: 0.5,
            mallet_noise_decay: 0.5,
            fork_tine_decay: 1.0,
            fork_tine_volume: 0.5,
            fork_tine_color: 0.5,
            fork_tone_decay: 1.0,
            fork_tone_volume: 0.5,
            fork_release: 0.5,
            damper_tone: 0.5,
            damper_amount: 0.5,
            damper_balance: 0.5,
            pickup_symmetry: 0.0,
            pickup_distance: 0.2,
            pickup_model: 0,
            polyphony: 6,
            volume: 0.8,
            keyboard_transpose: 0.0,
        }
    }
}

pub fn parse_electric(node: Node<'_, '_>) -> ElectricParams {
    ElectricParams {
        mallet_stiffness: param_f64(node, "MalletStiffness", 50.0),
        mallet_force: param_f64(node, "MalletForceStrength", 0.5),
        mallet_noise_amount: param_f64(node, "MalletNoiseAmount", 0.3),
        mallet_noise_pitch: param_f64(node, "MalletNoisePitch", 0.5),
        mallet_noise_decay: param_f64(node, "MalletNoiseDecay", 0.5),
        fork_tine_decay: param_f64(node, "ForkTineDecay", 1.0),
        fork_tine_volume: param_f64(node, "ForkTineVolume", 0.5),
        fork_tine_color: param_f64(node, "ForkTineColor", 0.5),
        fork_tone_decay: param_f64(node, "ForkToneBarDecay", 1.0),
        fork_tone_volume: param_f64(node, "ForkToneBarVolume", 0.5),
        fork_release: param_f64(node, "ForkReleaseTime", 0.5),
        damper_tone: param_f64(node, "DamperTone", 0.5),
        damper_amount: param_f64(node, "DamperAmount", 0.5),
        damper_balance: param_f64(node, "DamperBalance", 0.5),
        pickup_symmetry: param_f64(node, "PickupSymmetry", 0.0),
        pickup_distance: param_f64(node, "PickupDistance", 0.2),
        pickup_model: param_i32(node, "PickupModel", 0),
        polyphony: param_i32(node, "Polyphony", 6),
        volume: param_f64(node, "Volume", 0.8),
        keyboard_transpose: param_f64(node, "KeyboardTranspose", 0.0),
    }
}

pub fn write_electric<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ElectricParams) -> io::Result<()> {
    write_param_f64(w, "MalletStiffness", p.mallet_stiffness)?;
    write_param_f64(w, "MalletForceStrength", p.mallet_force)?;
    write_param_f64(w, "MalletNoiseAmount", p.mallet_noise_amount)?;
    write_param_f64(w, "MalletNoisePitch", p.mallet_noise_pitch)?;
    write_param_f64(w, "MalletNoiseDecay", p.mallet_noise_decay)?;
    write_param_f64(w, "ForkTineDecay", p.fork_tine_decay)?;
    write_param_f64(w, "ForkTineVolume", p.fork_tine_volume)?;
    write_param_f64(w, "ForkTineColor", p.fork_tine_color)?;
    write_param_f64(w, "ForkToneBarDecay", p.fork_tone_decay)?;
    write_param_f64(w, "ForkToneBarVolume", p.fork_tone_volume)?;
    write_param_f64(w, "ForkReleaseTime", p.fork_release)?;
    write_param_f64(w, "DamperTone", p.damper_tone)?;
    write_param_f64(w, "DamperAmount", p.damper_amount)?;
    write_param_f64(w, "DamperBalance", p.damper_balance)?;
    write_param_f64(w, "PickupSymmetry", p.pickup_symmetry)?;
    write_param_f64(w, "PickupDistance", p.pickup_distance)?;
    write_param_i32(w, "PickupModel", p.pickup_model)?;
    write_param_i32(w, "Polyphony", p.polyphony)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_f64(w, "KeyboardTranspose", p.keyboard_transpose)?;
    Ok(())
}

// ─── Analog (UltraAnalog) ────────────────────────────────────────────────

/// Analog signal chain (one of two).
#[derive(Debug, Clone)]
pub struct AnalogChain {
    pub osc_on: bool,
    pub osc_shape: i32,
    pub osc_octave: f64,
    pub osc_semi: f64,
    pub osc_detune: f64,
    pub osc_pulse_width: f64,
    pub osc_level: f64,
    pub filter_on: bool,
    pub filter_type: i32,
    pub filter_freq: f64,
    pub filter_q: f64,
    pub filter_env_amount: f64,
    pub amp_on: bool,
    pub amp_level: f64,
    pub amp_pan: f64,
}

impl Default for AnalogChain {
    fn default() -> Self {
        Self {
            osc_on: true,
            osc_shape: 0,
            osc_octave: 0.0,
            osc_semi: 0.0,
            osc_detune: 0.0,
            osc_pulse_width: 0.5,
            osc_level: 0.8,
            filter_on: true,
            filter_type: 0,
            filter_freq: 18000.0,
            filter_q: 0.5,
            filter_env_amount: 0.0,
            amp_on: true,
            amp_level: 0.8,
            amp_pan: 0.0,
        }
    }
}

/// Analog dual-oscillator synth parameters.
#[derive(Debug, Clone)]
pub struct AnalogParams {
    pub chain1: AnalogChain,
    pub chain2: AnalogChain,
    pub noise_on: bool,
    pub noise_level: f64,
    pub noise_color: f64,
    pub polyphony: i32,
    pub volume: f64,
    pub unison_on: bool,
    pub unison_detune: f64,
}

impl Default for AnalogParams {
    fn default() -> Self {
        Self {
            chain1: AnalogChain::default(),
            chain2: AnalogChain {
                osc_on: false,
                filter_on: false,
                amp_on: false,
                ..Default::default()
            },
            noise_on: false,
            noise_level: 0.0,
            noise_color: 0.5,
            polyphony: 6,
            volume: 0.8,
            unison_on: false,
            unison_detune: 0.0,
        }
    }
}

fn parse_analog_chain(node: Node<'_, '_>) -> AnalogChain {
    AnalogChain {
        osc_on: param_bool(node, "OscillatorToggle", true),
        osc_shape: param_i32(node, "OscillatorWaveShape", 0),
        osc_octave: param_f64(node, "OscillatorOct", 0.0),
        osc_semi: param_f64(node, "OscillatorSemi", 0.0),
        osc_detune: param_f64(node, "OscillatorDetune", 0.0),
        osc_pulse_width: param_f64(node, "OscillatorPulseWidth", 0.5),
        osc_level: param_f64(node, "OscillatorLevel", 0.8),
        filter_on: param_bool(node, "FilterToggle", true),
        filter_type: param_i32(node, "FilterType", 0),
        filter_freq: param_f64(node, "FilterCutoffFrequency", 18000.0),
        filter_q: param_f64(node, "FilterQFactor", 0.5),
        filter_env_amount: param_f64(node, "FilterEnvCutoffMod", 0.0),
        amp_on: param_bool(node, "AmplifierToggle", true),
        amp_level: param_f64(node, "AmplifierLevel", 0.8),
        amp_pan: param_f64(node, "AmplifierPan", 0.0),
    }
}

fn write_analog_chain<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    tag: &str,
    c: &AnalogChain,
) -> io::Result<()> {
    w.start(tag)?;
    write_param_bool(w, "OscillatorToggle", c.osc_on)?;
    write_param_i32(w, "OscillatorWaveShape", c.osc_shape)?;
    write_param_f64(w, "OscillatorOct", c.osc_octave)?;
    write_param_f64(w, "OscillatorSemi", c.osc_semi)?;
    write_param_f64(w, "OscillatorDetune", c.osc_detune)?;
    write_param_f64(w, "OscillatorPulseWidth", c.osc_pulse_width)?;
    write_param_f64(w, "OscillatorLevel", c.osc_level)?;
    write_param_bool(w, "FilterToggle", c.filter_on)?;
    write_param_i32(w, "FilterType", c.filter_type)?;
    write_param_f64(w, "FilterCutoffFrequency", c.filter_freq)?;
    write_param_f64(w, "FilterQFactor", c.filter_q)?;
    write_param_f64(w, "FilterEnvCutoffMod", c.filter_env_amount)?;
    write_param_bool(w, "AmplifierToggle", c.amp_on)?;
    write_param_f64(w, "AmplifierLevel", c.amp_level)?;
    write_param_f64(w, "AmplifierPan", c.amp_pan)?;
    w.end(tag)
}

pub fn parse_analog(node: Node<'_, '_>) -> AnalogParams {
    let mut p = AnalogParams::default();

    if let Some(c1) = child(node, "SignalChain1") {
        p.chain1 = parse_analog_chain(c1);
    }
    if let Some(c2) = child(node, "SignalChain2") {
        p.chain2 = parse_analog_chain(c2);
    }
    p.noise_on = param_bool(node, "NoiseToggle", false);
    p.noise_level = param_f64(node, "NoiseLevel", 0.0);
    p.noise_color = param_f64(node, "NoiseColor", 0.5);
    p.polyphony = param_i32(node, "Polyphony", 6);
    p.volume = param_f64(node, "Volume", 0.8);
    p.unison_on = param_bool(node, "KeyboardUnisonToggle", false);
    p.unison_detune = param_f64(node, "KeyboardDetune", 0.0);

    p
}

pub fn write_analog<W: Write>(w: &mut AbletonXmlWriter<W>, p: &AnalogParams) -> io::Result<()> {
    write_analog_chain(w, "SignalChain1", &p.chain1)?;
    write_analog_chain(w, "SignalChain2", &p.chain2)?;
    write_param_bool(w, "NoiseToggle", p.noise_on)?;
    write_param_f64(w, "NoiseLevel", p.noise_level)?;
    write_param_f64(w, "NoiseColor", p.noise_color)?;
    write_param_i32(w, "Polyphony", p.polyphony)?;
    write_param_f64(w, "Volume", p.volume)?;
    write_param_bool(w, "KeyboardUnisonToggle", p.unison_on)?;
    write_param_f64(w, "KeyboardDetune", p.unison_detune)?;
    Ok(())
}

// ─── Impulse (InstrumentImpulse) ─────────────────────────────────────────

/// Impulse 8-slot drum sampler global parameters.
#[derive(Debug, Clone)]
pub struct ImpulseParams {
    pub global_volume: f64,
    pub global_time: f64,
    pub global_pitch: f64,
    pub link_voices_7_8: bool,
}

impl Default for ImpulseParams {
    fn default() -> Self {
        Self {
            global_volume: 0.8,
            global_time: 1.0,
            global_pitch: 0.0,
            link_voices_7_8: false,
        }
    }
}

pub fn parse_impulse(node: Node<'_, '_>) -> ImpulseParams {
    ImpulseParams {
        global_volume: param_f64(node, "GlobalVolume", 0.8),
        global_time: param_f64(node, "GlobalTime", 1.0),
        global_pitch: param_f64(node, "GlobalPitch", 0.0),
        link_voices_7_8: param_bool(node, "LinkVoices7and8", false),
    }
}

pub fn write_impulse<W: Write>(w: &mut AbletonXmlWriter<W>, p: &ImpulseParams) -> io::Result<()> {
    write_param_f64(w, "GlobalVolume", p.global_volume)?;
    write_param_f64(w, "GlobalTime", p.global_time)?;
    write_param_f64(w, "GlobalPitch", p.global_pitch)?;
    write_param_bool(w, "LinkVoices7and8", p.link_voices_7_8)?;
    Ok(())
}
