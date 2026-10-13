//! Creative effect device typed parameters
//! (BeatRepeat, Corpus, Resonator, Vocoder, Looper, GrainDelay, FilterDelay, Hybrid).

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

// ─── BeatRepeat ───────────────────────────────────────────────────────────

/// BeatRepeat parameters.
#[derive(Debug, Clone)]
pub struct BeatRepeatParams {
    pub chance: f64,
    pub interval: f64,
    pub offset: f64,
    pub grid: f64,
    pub block_tripplets: bool,
    pub grid_chance: f64,
    pub grid_chance_type: i32,
    pub gate: f64,
    pub damp_volume: f64,
    pub damp_pitch: f64,
    pub base_pitch: f64,
    pub mix_type: i32,
    pub wet_level: f64,
    pub filter_on: bool,
    pub mid_freq: f64,
    pub band_width: f64,
    pub instant_repeat: bool,
}

impl Default for BeatRepeatParams {
    fn default() -> Self {
        Self {
            chance: 100.0,
            interval: 4.0,
            offset: 0.0,
            grid: 4.0,
            block_tripplets: false,
            grid_chance: 0.0,
            grid_chance_type: 0,
            gate: 8.0,
            damp_volume: 0.0,
            damp_pitch: 0.0,
            base_pitch: 0.0,
            mix_type: 0,
            wet_level: 1.0,
            filter_on: false,
            mid_freq: 1000.0,
            band_width: 6.0,
            instant_repeat: false,
        }
    }
}

pub fn parse_beat_repeat(node: Node<'_, '_>) -> BeatRepeatParams {
    BeatRepeatParams {
        chance: param_f64(node, "Chance", 100.0),
        interval: param_f64(node, "Interval", 4.0),
        offset: param_f64(node, "Offset", 0.0),
        grid: param_f64(node, "Grid", 4.0),
        block_tripplets: param_bool(node, "BlockTripplets", false),
        grid_chance: param_f64(node, "GridChance", 0.0),
        grid_chance_type: param_i32(node, "GridChanceType", 0),
        gate: param_f64(node, "Gate", 8.0),
        damp_volume: param_f64(node, "DampVolume", 0.0),
        damp_pitch: param_f64(node, "DampPitch", 0.0),
        base_pitch: param_f64(node, "BasePitch", 0.0),
        mix_type: param_i32(node, "MixType", 0),
        wet_level: param_f64(node, "WetLevel", 1.0),
        filter_on: param_bool(node, "FilterOn", false),
        mid_freq: param_f64(node, "MidFreq", 1000.0),
        band_width: param_f64(node, "BandWidth", 6.0),
        instant_repeat: param_bool(node, "InstantRepeat", false),
    }
}

pub fn write_beat_repeat<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &BeatRepeatParams,
) -> io::Result<()> {
    write_param_f64(w, "Chance", p.chance)?;
    write_param_f64(w, "Interval", p.interval)?;
    write_param_f64(w, "Offset", p.offset)?;
    write_param_f64(w, "Grid", p.grid)?;
    write_param_bool(w, "BlockTripplets", p.block_tripplets)?;
    write_param_f64(w, "GridChance", p.grid_chance)?;
    write_param_i32(w, "GridChanceType", p.grid_chance_type)?;
    write_param_f64(w, "Gate", p.gate)?;
    write_param_f64(w, "DampVolume", p.damp_volume)?;
    write_param_f64(w, "DampPitch", p.damp_pitch)?;
    write_param_f64(w, "BasePitch", p.base_pitch)?;
    write_param_i32(w, "MixType", p.mix_type)?;
    write_param_f64(w, "WetLevel", p.wet_level)?;
    write_param_bool(w, "FilterOn", p.filter_on)?;
    write_param_f64(w, "MidFreq", p.mid_freq)?;
    write_param_f64(w, "BandWidth", p.band_width)?;
    write_param_bool(w, "InstantRepeat", p.instant_repeat)?;
    Ok(())
}

// ─── Corpus ───────────────────────────────────────────────────────────────

/// Corpus parameters (physical modelling resonator).
#[derive(Debug, Clone)]
pub struct CorpusParams {
    pub resonance_type: i32,
    pub resonator_quality: i32,
    pub frequency: f64,
    pub transpose: f64,
    pub fine_transpose: f64,
    pub detune: f64,
    pub decay: f64,
    pub freq_damping: f64,
    pub radius: f64,
    pub amp_freq: f64,
    pub inharmonics: f64,
    pub tube_opening: f64,
    pub ratio: f64,
    pub excitation_x: f64,
    pub listening_xl: f64,
    pub listening_xr: f64,
    // LFO
    pub lfo_on_off: bool,
    pub lfo_type: i32,
    pub lfo_sync: i32,
    pub lfo_rate: f64,
    pub lfo_sync_rate: f64,
    pub lfo_stereo_mode: i32,
    pub lfo_spin: f64,
    pub lfo_phase: f64,
    pub lfo_offset: f64,
    pub lfo_amount: f64,
    // Filter
    pub filter_on: bool,
    pub filter_mid_freq: f64,
    pub filter_band_width: f64,
    // MIDI
    pub midi_pitch: bool,
    pub midi_mode: i32,
    pub pitch_bend_range: f64,
    pub midi_gate: bool,
    pub decay_on_note_off: f64,
    // Output
    pub drive: f64,
    pub stereo_width: f64,
    pub bleed: f64,
    pub dry_wet: f64,
}

impl Default for CorpusParams {
    fn default() -> Self {
        Self {
            resonance_type: 0,
            resonator_quality: 0,
            frequency: 200.0,
            transpose: 0.0,
            fine_transpose: 0.0,
            detune: 0.0,
            decay: 1.0,
            freq_damping: 0.5,
            radius: 0.5,
            amp_freq: 100.0,
            inharmonics: 0.0,
            tube_opening: 0.5,
            ratio: 0.5,
            excitation_x: 0.5,
            listening_xl: 0.5,
            listening_xr: 0.5,
            lfo_on_off: false,
            lfo_type: 0,
            lfo_sync: 0,
            lfo_rate: 1.0,
            lfo_sync_rate: 4.0,
            lfo_stereo_mode: 0,
            lfo_spin: 0.0,
            lfo_phase: 0.0,
            lfo_offset: 0.0,
            lfo_amount: 0.0,
            filter_on: false,
            filter_mid_freq: 1000.0,
            filter_band_width: 6.0,
            midi_pitch: false,
            midi_mode: 0,
            pitch_bend_range: 2.0,
            midi_gate: false,
            decay_on_note_off: 0.5,
            drive: 0.0,
            stereo_width: 0.0,
            bleed: 0.0,
            dry_wet: 0.5,
        }
    }
}

pub fn parse_corpus(node: Node<'_, '_>) -> CorpusParams {
    CorpusParams {
        resonance_type: param_i32(node, "ResonanceType", 0),
        resonator_quality: param_i32(node, "ResonatorQuality", 0),
        frequency: param_f64(node, "Frequency", 200.0),
        transpose: param_f64(node, "Transpose", 0.0),
        fine_transpose: param_f64(node, "FineTranspose", 0.0),
        detune: param_f64(node, "Detune", 0.0),
        decay: param_f64(node, "Decay", 1.0),
        freq_damping: param_f64(node, "FreqDamping", 0.5),
        radius: param_f64(node, "Radius", 0.5),
        amp_freq: param_f64(node, "AmpFreq", 100.0),
        inharmonics: param_f64(node, "Inharmonics", 0.0),
        tube_opening: param_f64(node, "TubeOpening", 0.5),
        ratio: param_f64(node, "Ratio", 0.5),
        excitation_x: param_f64(node, "ExcitationX", 0.5),
        listening_xl: param_f64(node, "ListeningXL", 0.5),
        listening_xr: param_f64(node, "ListeningXR", 0.5),
        lfo_on_off: param_bool(node, "LfoOnOff", false),
        lfo_type: param_i32(node, "LfoType", 0),
        lfo_sync: param_i32(node, "LfoSync", 0),
        lfo_rate: param_f64(node, "LfoRate", 1.0),
        lfo_sync_rate: param_f64(node, "LfoSyncRate", 4.0),
        lfo_stereo_mode: param_i32(node, "LfoStereoMode", 0),
        lfo_spin: param_f64(node, "LfoSpin", 0.0),
        lfo_phase: param_f64(node, "LfoPhase", 0.0),
        lfo_offset: param_f64(node, "LfoOffset", 0.0),
        lfo_amount: param_f64(node, "LfoAmount", 0.0),
        filter_on: param_bool(node, "FilterOn", false),
        filter_mid_freq: param_f64(node, "FilterMidFreq", 1000.0),
        filter_band_width: param_f64(node, "FilterBandWidth", 6.0),
        midi_pitch: param_bool(node, "MidiPitch", false),
        midi_mode: param_i32(node, "MidiMode", 0),
        pitch_bend_range: param_f64(node, "PitchBendRange", 2.0),
        midi_gate: param_bool(node, "MidiGate", false),
        decay_on_note_off: param_f64(node, "DecayOnNoteOff", 0.5),
        drive: param_f64(node, "Drive", 0.0),
        stereo_width: param_f64(node, "StereoWidth", 0.0),
        bleed: param_f64(node, "Bleed", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
    }
}

pub fn write_corpus<W: Write>(w: &mut AbletonXmlWriter<W>, p: &CorpusParams) -> io::Result<()> {
    write_param_i32(w, "ResonanceType", p.resonance_type)?;
    write_param_i32(w, "ResonatorQuality", p.resonator_quality)?;
    write_param_f64(w, "Frequency", p.frequency)?;
    write_param_f64(w, "Transpose", p.transpose)?;
    write_param_f64(w, "FineTranspose", p.fine_transpose)?;
    write_param_f64(w, "Detune", p.detune)?;
    write_param_f64(w, "Decay", p.decay)?;
    write_param_f64(w, "FreqDamping", p.freq_damping)?;
    write_param_f64(w, "Radius", p.radius)?;
    write_param_f64(w, "AmpFreq", p.amp_freq)?;
    write_param_f64(w, "Inharmonics", p.inharmonics)?;
    write_param_f64(w, "TubeOpening", p.tube_opening)?;
    write_param_f64(w, "Ratio", p.ratio)?;
    write_param_f64(w, "ExcitationX", p.excitation_x)?;
    write_param_f64(w, "ListeningXL", p.listening_xl)?;
    write_param_f64(w, "ListeningXR", p.listening_xr)?;
    write_param_bool(w, "LfoOnOff", p.lfo_on_off)?;
    write_param_i32(w, "LfoType", p.lfo_type)?;
    write_param_i32(w, "LfoSync", p.lfo_sync)?;
    write_param_f64(w, "LfoRate", p.lfo_rate)?;
    write_param_f64(w, "LfoSyncRate", p.lfo_sync_rate)?;
    write_param_i32(w, "LfoStereoMode", p.lfo_stereo_mode)?;
    write_param_f64(w, "LfoSpin", p.lfo_spin)?;
    write_param_f64(w, "LfoPhase", p.lfo_phase)?;
    write_param_f64(w, "LfoOffset", p.lfo_offset)?;
    write_param_f64(w, "LfoAmount", p.lfo_amount)?;
    write_param_bool(w, "FilterOn", p.filter_on)?;
    write_param_f64(w, "FilterMidFreq", p.filter_mid_freq)?;
    write_param_f64(w, "FilterBandWidth", p.filter_band_width)?;
    write_param_bool(w, "MidiPitch", p.midi_pitch)?;
    write_param_i32(w, "MidiMode", p.midi_mode)?;
    write_param_f64(w, "PitchBendRange", p.pitch_bend_range)?;
    write_param_bool(w, "MidiGate", p.midi_gate)?;
    write_param_f64(w, "DecayOnNoteOff", p.decay_on_note_off)?;
    write_param_f64(w, "Drive", p.drive)?;
    write_param_f64(w, "StereoWidth", p.stereo_width)?;
    write_param_f64(w, "Bleed", p.bleed)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}

// ─── Resonator ────────────────────────────────────────────────────────────

/// A single resonator voice (II-V have pitch relative to I).
#[derive(Debug, Clone)]
pub struct ResonatorVoice {
    pub on: bool,
    pub pitch: f64,
    pub tune: f64,
    pub gain: f64,
}

impl Default for ResonatorVoice {
    fn default() -> Self {
        Self {
            on: false,
            pitch: 0.0,
            tune: 0.0,
            gain: 0.0,
        }
    }
}

/// Resonator parameters.
#[derive(Debug, Clone)]
pub struct ResonatorParams {
    pub in_filter_on: bool,
    pub in_filter_freq: f64,
    pub in_filter_mode: i32,
    pub res_mode: bool,
    pub res_decay: f64,
    pub res_const: bool,
    pub res_color: f64,
    pub width: f64,
    pub dry_wet: f64,
    pub global_gain: f64,
    // Resonator I (base note)
    pub res_on1: bool,
    pub res_note: f64,
    pub res_note_scroll_position: i32,
    pub res_tune1: f64,
    pub res_gain1: f64,
    // Resonators II-V
    pub voices: [ResonatorVoice; 4],
    pub relative_pitch_scroll_position: i32,
}

impl Default for ResonatorParams {
    fn default() -> Self {
        Self {
            in_filter_on: false,
            in_filter_freq: 1000.0,
            in_filter_mode: 0,
            res_mode: false,
            res_decay: 0.5,
            res_const: false,
            res_color: 0.5,
            width: 0.0,
            dry_wet: 0.5,
            global_gain: 0.0,
            res_on1: true,
            res_note: 60.0,
            res_note_scroll_position: 0,
            res_tune1: 0.0,
            res_gain1: 0.0,
            voices: Default::default(),
            relative_pitch_scroll_position: 0,
        }
    }
}

pub fn parse_resonator(node: Node<'_, '_>) -> ResonatorParams {
    let voices = [
        ResonatorVoice {
            on: param_bool(node, "ResOn2", false),
            pitch: param_f64(node, "ResPitch2", 0.0),
            tune: param_f64(node, "ResTune2", 0.0),
            gain: param_f64(node, "ResGain2", 0.0),
        },
        ResonatorVoice {
            on: param_bool(node, "ResOn3", false),
            pitch: param_f64(node, "ResPitch3", 0.0),
            tune: param_f64(node, "ResTune3", 0.0),
            gain: param_f64(node, "ResGain3", 0.0),
        },
        ResonatorVoice {
            on: param_bool(node, "ResOn4", false),
            pitch: param_f64(node, "ResPitch4", 0.0),
            tune: param_f64(node, "ResTune4", 0.0),
            gain: param_f64(node, "ResGain4", 0.0),
        },
        ResonatorVoice {
            on: param_bool(node, "ResOn5", false),
            pitch: param_f64(node, "ResPitch5", 0.0),
            tune: param_f64(node, "ResTune5", 0.0),
            gain: param_f64(node, "ResGain5", 0.0),
        },
    ];

    ResonatorParams {
        in_filter_on: param_bool(node, "InFilterOn", false),
        in_filter_freq: param_f64(node, "InFilterFreq", 1000.0),
        in_filter_mode: param_i32(node, "InFilterMode", 0),
        res_mode: param_bool(node, "ResMode", false),
        res_decay: param_f64(node, "ResDecay", 0.5),
        res_const: param_bool(node, "ResConst", false),
        res_color: param_f64(node, "ResColor", 0.5),
        width: param_f64(node, "Width", 0.0),
        dry_wet: param_f64(node, "DryWet", 0.5),
        global_gain: param_f64(node, "GlobalGain", 0.0),
        res_on1: param_bool(node, "ResOn1", true),
        res_note: param_f64(node, "ResNote", 60.0),
        res_note_scroll_position: param_i32(node, "ResNoteScrollPosition", 0),
        res_tune1: param_f64(node, "ResTune1", 0.0),
        res_gain1: param_f64(node, "ResGain1", 0.0),
        voices,
        relative_pitch_scroll_position: param_i32(node, "RelativePitchScrollPosition", 0),
    }
}

pub fn write_resonator<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &ResonatorParams,
) -> io::Result<()> {
    write_param_bool(w, "InFilterOn", p.in_filter_on)?;
    write_param_f64(w, "InFilterFreq", p.in_filter_freq)?;
    write_param_i32(w, "InFilterMode", p.in_filter_mode)?;
    write_param_bool(w, "ResMode", p.res_mode)?;
    write_param_f64(w, "ResDecay", p.res_decay)?;
    write_param_bool(w, "ResConst", p.res_const)?;
    write_param_f64(w, "ResColor", p.res_color)?;
    write_param_f64(w, "Width", p.width)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "GlobalGain", p.global_gain)?;
    write_param_bool(w, "ResOn1", p.res_on1)?;
    write_param_f64(w, "ResNote", p.res_note)?;
    write_param_i32(w, "ResNoteScrollPosition", p.res_note_scroll_position)?;
    write_param_f64(w, "ResTune1", p.res_tune1)?;
    write_param_f64(w, "ResGain1", p.res_gain1)?;

    for (i, v) in p.voices.iter().enumerate() {
        let n = i + 2;
        write_param_bool(w, &format!("ResOn{n}"), v.on)?;
        write_param_f64(w, &format!("ResPitch{n}"), v.pitch)?;
        write_param_f64(w, &format!("ResTune{n}"), v.tune)?;
        write_param_f64(w, &format!("ResGain{n}"), v.gain)?;
    }

    write_param_i32(
        w,
        "RelativePitchScrollPosition",
        p.relative_pitch_scroll_position,
    )?;
    Ok(())
}

// ─── Vocoder ──────────────────────────────────────────────────────────────

/// Vocoder parameters.
#[derive(Debug, Clone)]
pub struct VocoderParams {
    pub band_count: i32,
    pub low_frequency: f64,
    pub high_frequency: f64,
    pub formant_shift: f64,
    pub filter_band_width: f64,
    pub retro: bool,
    pub level_gate: f64,
    pub output_gain: f64,
    pub envelope_rate: f64,
    pub envelope_release: f64,
    pub uvd_threshold: f64,
    pub uvd_slow: bool,
    pub uvd_level: f64,
    // Carrier source
    pub carrier_type: i32,
    pub carrier_noise_rate: f64,
    pub carrier_noise_crackle: f64,
    pub carrier_oscillator_pitch: f64,
    pub carrier_oscillator_waveform: i32,
    pub carrier_flatten: bool,
    pub mono_stereo: i32,
    pub dry_wet: f64,
    pub modulator_amount: f64,
}

impl Default for VocoderParams {
    fn default() -> Self {
        Self {
            band_count: 0,
            low_frequency: 50.0,
            high_frequency: 16000.0,
            formant_shift: 0.0,
            filter_band_width: 0.5,
            retro: false,
            level_gate: -40.0,
            output_gain: 0.0,
            envelope_rate: 50.0,
            envelope_release: 50.0,
            uvd_threshold: 0.0,
            uvd_slow: false,
            uvd_level: 0.0,
            carrier_type: 0,
            carrier_noise_rate: 0.5,
            carrier_noise_crackle: 0.0,
            carrier_oscillator_pitch: 48.0,
            carrier_oscillator_waveform: 0,
            carrier_flatten: false,
            mono_stereo: 0,
            dry_wet: 1.0,
            modulator_amount: 1.0,
        }
    }
}

pub fn parse_vocoder(node: Node<'_, '_>) -> VocoderParams {
    // FilterBank is a nested element with BandCount inside it.
    let band_count = child(node, "FilterBank")
        .map(|n| param_i32(n, "BandCount", 0))
        .unwrap_or(0);
    // CarrierSource is nested.
    let carrier = child(node, "CarrierSource");

    VocoderParams {
        band_count,
        low_frequency: param_f64(node, "LowFrequency", 50.0),
        high_frequency: param_f64(node, "HighFrequency", 16000.0),
        formant_shift: param_f64(node, "FormantShift", 0.0),
        filter_band_width: param_f64(node, "FilterBandWidth", 0.5),
        retro: param_bool(node, "Retro", false),
        level_gate: param_f64(node, "LevelGate", -40.0),
        output_gain: param_f64(node, "OutputGain", 0.0),
        envelope_rate: param_f64(node, "EnvelopeRate", 50.0),
        envelope_release: param_f64(node, "EnvelopeRelease", 50.0),
        uvd_threshold: param_f64(node, "UvdThreshold", 0.0),
        uvd_slow: param_bool(node, "UvdSlow", false),
        uvd_level: param_f64(node, "UvdLevel", 0.0),
        carrier_type: carrier.map(|n| param_i32(n, "Type", 0)).unwrap_or(0),
        carrier_noise_rate: carrier
            .map(|n| param_f64(n, "NoiseRate", 0.5))
            .unwrap_or(0.5),
        carrier_noise_crackle: carrier
            .map(|n| param_f64(n, "NoiseCrackle", 0.0))
            .unwrap_or(0.0),
        carrier_oscillator_pitch: carrier
            .map(|n| param_f64(n, "OscillatorPitch", 48.0))
            .unwrap_or(48.0),
        carrier_oscillator_waveform: carrier
            .map(|n| param_i32(n, "OscillatorWaveform", 0))
            .unwrap_or(0),
        carrier_flatten: param_bool(node, "CarrierFlatten", false),
        mono_stereo: param_i32(node, "MonoStereo", 0),
        dry_wet: param_f64(node, "DryWet", 1.0),
        modulator_amount: param_f64(node, "ModulatorAmount", 1.0),
    }
}

pub fn write_vocoder<W: Write>(w: &mut AbletonXmlWriter<W>, p: &VocoderParams) -> io::Result<()> {
    w.start("FilterBank")?;
    write_param_i32(w, "BandCount", p.band_count)?;
    w.end("FilterBank")?;

    write_param_f64(w, "LowFrequency", p.low_frequency)?;
    write_param_f64(w, "HighFrequency", p.high_frequency)?;
    write_param_f64(w, "FormantShift", p.formant_shift)?;
    write_param_f64(w, "FilterBandWidth", p.filter_band_width)?;
    write_param_bool(w, "Retro", p.retro)?;
    write_param_f64(w, "LevelGate", p.level_gate)?;
    write_param_f64(w, "OutputGain", p.output_gain)?;
    write_param_f64(w, "EnvelopeRate", p.envelope_rate)?;
    write_param_f64(w, "EnvelopeRelease", p.envelope_release)?;
    write_param_f64(w, "UvdThreshold", p.uvd_threshold)?;
    write_param_bool(w, "UvdSlow", p.uvd_slow)?;
    write_param_f64(w, "UvdLevel", p.uvd_level)?;

    w.start("CarrierSource")?;
    write_param_i32(w, "Type", p.carrier_type)?;
    write_param_f64(w, "NoiseRate", p.carrier_noise_rate)?;
    write_param_f64(w, "NoiseCrackle", p.carrier_noise_crackle)?;
    write_param_f64(w, "OscillatorPitch", p.carrier_oscillator_pitch)?;
    write_param_i32(w, "OscillatorWaveform", p.carrier_oscillator_waveform)?;
    w.end("CarrierSource")?;

    write_param_bool(w, "CarrierFlatten", p.carrier_flatten)?;
    write_param_i32(w, "MonoStereo", p.mono_stereo)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    write_param_f64(w, "ModulatorAmount", p.modulator_amount)?;
    Ok(())
}

// ─── Looper ───────────────────────────────────────────────────────────────

/// Looper parameters.
#[derive(Debug, Clone)]
pub struct LooperParams {
    pub state: i32,
    pub feedback: f64,
    pub reverse: bool,
    pub monitor: i32,
    pub pitch: f64,
    pub local_quantization: i32,
    pub song_control: i32,
    pub tempo_control: i32,
    pub overdub_after_record: bool,
    pub fixed_length_record: i32,
}

impl Default for LooperParams {
    fn default() -> Self {
        Self {
            state: 0,
            feedback: 1.0,
            reverse: false,
            monitor: 0,
            pitch: 0.0,
            local_quantization: 0,
            song_control: 0,
            tempo_control: 0,
            overdub_after_record: false,
            fixed_length_record: 0,
        }
    }
}

pub fn parse_looper(node: Node<'_, '_>) -> LooperParams {
    LooperParams {
        state: param_i32(node, "State", 0),
        feedback: param_f64(node, "Feedback", 1.0),
        reverse: param_bool(node, "Reverse", false),
        monitor: param_i32(node, "Monitor", 0),
        pitch: param_f64(node, "Pitch", 0.0),
        local_quantization: param_i32(node, "LocalQuantization", 0),
        song_control: param_i32(node, "SongControl", 0),
        tempo_control: param_i32(node, "TempoControl", 0),
        overdub_after_record: param_bool(node, "OverdubAfterRecord", false),
        fixed_length_record: param_i32(node, "FixedLengthRecord", 0),
    }
}

pub fn write_looper<W: Write>(w: &mut AbletonXmlWriter<W>, p: &LooperParams) -> io::Result<()> {
    write_param_i32(w, "State", p.state)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_bool(w, "Reverse", p.reverse)?;
    write_param_i32(w, "Monitor", p.monitor)?;
    write_param_f64(w, "Pitch", p.pitch)?;
    write_param_i32(w, "LocalQuantization", p.local_quantization)?;
    write_param_i32(w, "SongControl", p.song_control)?;
    write_param_i32(w, "TempoControl", p.tempo_control)?;
    write_param_bool(w, "OverdubAfterRecord", p.overdub_after_record)?;
    write_param_i32(w, "FixedLengthRecord", p.fixed_length_record)?;
    Ok(())
}

// ─── GrainDelay ───────────────────────────────────────────────────────────

/// GrainDelay parameters.
#[derive(Debug, Clone)]
pub struct GrainDelayParams {
    pub spray: f64,
    pub freq: f64,
    pub pitch: f64,
    pub pitch_scroll_position: i32,
    pub random_pitch: f64,
    pub feedback: f64,
    pub dry_wet: f64,
    pub sync_mode: bool,
    pub beat_delay_enum: i32,
    pub bar_delay_offset: f64,
    pub ms_delay: f64,
}

impl Default for GrainDelayParams {
    fn default() -> Self {
        Self {
            spray: 0.0,
            freq: 1.0,
            pitch: 0.0,
            pitch_scroll_position: 0,
            random_pitch: 0.0,
            feedback: 0.5,
            dry_wet: 0.5,
            sync_mode: true,
            beat_delay_enum: 0,
            bar_delay_offset: 0.0,
            ms_delay: 500.0,
        }
    }
}

pub fn parse_grain_delay(node: Node<'_, '_>) -> GrainDelayParams {
    GrainDelayParams {
        spray: param_f64(node, "Spray", 0.0),
        freq: param_f64(node, "Freq", 1.0),
        pitch: param_f64(node, "Pitch", 0.0),
        pitch_scroll_position: param_i32(node, "PitchScrollPosition", 0),
        random_pitch: param_f64(node, "RandomPitch", 0.0),
        feedback: param_f64(node, "Feedback", 0.5),
        dry_wet: param_f64(node, "NewDryWet", 0.5),
        sync_mode: param_bool(node, "SyncMode", true),
        beat_delay_enum: param_i32(node, "BeatDelayEnum", 0),
        bar_delay_offset: param_f64(node, "BarDelayOffset", 0.0),
        ms_delay: param_f64(node, "MsDelay", 500.0),
    }
}

pub fn write_grain_delay<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &GrainDelayParams,
) -> io::Result<()> {
    write_param_f64(w, "Spray", p.spray)?;
    write_param_f64(w, "Freq", p.freq)?;
    write_param_f64(w, "Pitch", p.pitch)?;
    write_param_i32(w, "PitchScrollPosition", p.pitch_scroll_position)?;
    write_param_f64(w, "RandomPitch", p.random_pitch)?;
    write_param_f64(w, "Feedback", p.feedback)?;
    write_param_f64(w, "NewDryWet", p.dry_wet)?;
    write_param_bool(w, "SyncMode", p.sync_mode)?;
    write_param_i32(w, "BeatDelayEnum", p.beat_delay_enum)?;
    write_param_f64(w, "BarDelayOffset", p.bar_delay_offset)?;
    write_param_f64(w, "MsDelay", p.ms_delay)?;
    Ok(())
}

// ─── FilterDelay ──────────────────────────────────────────────────────────

/// A single FilterDelay tap (1, 2, or 3).
#[derive(Debug, Clone)]
pub struct FilterDelayTap {
    pub on: bool,
    pub filter_on: bool,
    pub mid_freq: f64,
    pub band_width: f64,
    pub delay_time_switch: bool,
    pub beat_delay_enum: i32,
    pub beat_delay_offset: f64,
    pub delay_time: f64,
    pub feedback: f64,
    pub pan: f64,
    pub volume: f64,
}

impl Default for FilterDelayTap {
    fn default() -> Self {
        Self {
            on: true,
            filter_on: false,
            mid_freq: 1000.0,
            band_width: 6.0,
            delay_time_switch: true,
            beat_delay_enum: 0,
            beat_delay_offset: 0.0,
            delay_time: 500.0,
            feedback: 0.0,
            pan: 0.0,
            volume: -6.0,
        }
    }
}

/// FilterDelay parameters.
#[derive(Debug, Clone)]
pub struct FilterDelayParams {
    pub taps: [FilterDelayTap; 3],
    pub dry_volume: f64,
    pub delay_transition_mode: i32,
}

impl Default for FilterDelayParams {
    fn default() -> Self {
        Self {
            taps: Default::default(),
            dry_volume: 0.0,
            delay_transition_mode: 0,
        }
    }
}

pub fn parse_filter_delay(node: Node<'_, '_>) -> FilterDelayParams {
    let mut taps: [FilterDelayTap; 3] = Default::default();
    for (i, tap) in taps.iter_mut().enumerate() {
        let n = i + 1;
        tap.on = param_bool(node, &format!("On{n}"), true);
        tap.filter_on = param_bool(node, &format!("FilterOn{n}"), false);
        tap.mid_freq = param_f64(node, &format!("MidFreq{n}"), 1000.0);
        tap.band_width = param_f64(node, &format!("BandWidth{n}"), 6.0);
        tap.delay_time_switch = param_bool(node, &format!("DelayTimeSwitch{n}"), true);
        tap.beat_delay_enum = param_i32(node, &format!("BeatDelayEnum{n}"), 0);
        tap.beat_delay_offset = param_f64(node, &format!("BeatDelayOffset{n}"), 0.0);
        tap.delay_time = param_f64(node, &format!("DelayTime{n}"), 500.0);
        tap.feedback = param_f64(node, &format!("Feedback{n}"), 0.0);
        tap.pan = param_f64(node, &format!("Pan{n}"), 0.0);
        tap.volume = param_f64(node, &format!("Volume{n}"), -6.0);
    }

    FilterDelayParams {
        taps,
        dry_volume: param_f64(node, "DryVolume", 0.0),
        delay_transition_mode: param_i32(node, "DelayTransitionMode", 0),
    }
}

pub fn write_filter_delay<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    p: &FilterDelayParams,
) -> io::Result<()> {
    for (i, tap) in p.taps.iter().enumerate() {
        let n = i + 1;
        write_param_bool(w, &format!("On{n}"), tap.on)?;
        write_param_bool(w, &format!("FilterOn{n}"), tap.filter_on)?;
        write_param_f64(w, &format!("MidFreq{n}"), tap.mid_freq)?;
        write_param_f64(w, &format!("BandWidth{n}"), tap.band_width)?;
        write_param_bool(w, &format!("DelayTimeSwitch{n}"), tap.delay_time_switch)?;
        write_param_i32(w, &format!("BeatDelayEnum{n}"), tap.beat_delay_enum)?;
        write_param_f64(w, &format!("BeatDelayOffset{n}"), tap.beat_delay_offset)?;
        write_param_f64(w, &format!("DelayTime{n}"), tap.delay_time)?;
        write_param_f64(w, &format!("Feedback{n}"), tap.feedback)?;
        write_param_f64(w, &format!("Pan{n}"), tap.pan)?;
        write_param_f64(w, &format!("Volume{n}"), tap.volume)?;
    }

    write_param_f64(w, "DryVolume", p.dry_volume)?;
    write_param_i32(w, "DelayTransitionMode", p.delay_transition_mode)?;
    Ok(())
}

// ─── Hybrid (Hybrid Reverb) ──────────────────────────────────────────────

/// Hybrid Reverb parameters.
#[derive(Debug, Clone)]
pub struct HybridParams {
    pub current_tab: i32,
    // Pre-delay
    pub pre_delay_sync: bool,
    pub pre_delay_time: f64,
    pub pre_delay_sixteenth: f64,
    pub pre_delay_feedback_time: f64,
    pub pre_delay_feedback_sixteenth: f64,
    // Convolution IR
    pub convolution_ir_post_processing_on: bool,
    pub convolution_ir_attack_time: f64,
    pub convolution_ir_decay_time: f64,
    pub convolution_ir_size: f64,
    // Algorithm
    pub algorithm_type: i32,
    pub algorithm_delay: f64,
    pub algorithm_freeze: bool,
    pub algorithm_freeze_in: bool,
    pub algorithm_decay: f64,
    pub algorithm_size: f64,
    pub algorithm_damping: f64,
    pub algorithm_diffusion: f64,
    pub algorithm_modulation: f64,
    pub algorithm_shape: f64,
    pub algorithm_bass_multiplier: f64,
    pub algorithm_bass_crossover: f64,
    pub algorithm_shimmer: f64,
    pub algorithm_pitch_shift: f64,
    pub algorithm_tides_amount: f64,
    pub algorithm_tides_rate: f64,
    pub algorithm_tides_waveform: f64,
    pub algorithm_tides_phase_offset: f64,
    // EQ
    pub eq_on: bool,
    pub eq_pre_algo: bool,
    pub eq_low_band_type: i32,
    pub eq_low_band_frequency: f64,
    pub eq_low_band_gain: f64,
    pub eq_low_band_slope: f64,
    pub eq_peak1_frequency: f64,
    pub eq_peak1_gain: f64,
    pub eq_peak1_q: f64,
    pub eq_peak2_frequency: f64,
    pub eq_peak2_gain: f64,
    pub eq_peak2_q: f64,
    pub eq_high_band_type: i32,
    pub eq_high_band_frequency: f64,
    pub eq_high_band_gain: f64,
    pub eq_high_band_slope: f64,
    // Routing / mix
    pub send: f64,
    pub routing: i32,
    pub convo_algo_blend: f64,
    pub vintage: f64,
    pub stereo_width: f64,
    pub bass_mono: bool,
    pub dry_wet: f64,
}

impl Default for HybridParams {
    fn default() -> Self {
        Self {
            current_tab: 0,
            pre_delay_sync: false,
            pre_delay_time: 0.0,
            pre_delay_sixteenth: 4.0,
            pre_delay_feedback_time: 0.0,
            pre_delay_feedback_sixteenth: 4.0,
            convolution_ir_post_processing_on: false,
            convolution_ir_attack_time: 0.0,
            convolution_ir_decay_time: 1.0,
            convolution_ir_size: 1.0,
            algorithm_type: 0,
            algorithm_delay: 0.0,
            algorithm_freeze: false,
            algorithm_freeze_in: false,
            algorithm_decay: 1.0,
            algorithm_size: 50.0,
            algorithm_damping: 0.5,
            algorithm_diffusion: 0.5,
            algorithm_modulation: 0.0,
            algorithm_shape: 0.5,
            algorithm_bass_multiplier: 1.0,
            algorithm_bass_crossover: 200.0,
            algorithm_shimmer: 0.0,
            algorithm_pitch_shift: 0.0,
            algorithm_tides_amount: 0.0,
            algorithm_tides_rate: 1.0,
            algorithm_tides_waveform: 0.0,
            algorithm_tides_phase_offset: 0.0,
            eq_on: false,
            eq_pre_algo: false,
            eq_low_band_type: 0,
            eq_low_band_frequency: 200.0,
            eq_low_band_gain: 0.0,
            eq_low_band_slope: 1.0,
            eq_peak1_frequency: 500.0,
            eq_peak1_gain: 0.0,
            eq_peak1_q: 0.71,
            eq_peak2_frequency: 2000.0,
            eq_peak2_gain: 0.0,
            eq_peak2_q: 0.71,
            eq_high_band_type: 0,
            eq_high_band_frequency: 5000.0,
            eq_high_band_gain: 0.0,
            eq_high_band_slope: 1.0,
            send: 0.0,
            routing: 0,
            convo_algo_blend: 0.5,
            vintage: 0.0,
            stereo_width: 0.0,
            bass_mono: false,
            dry_wet: 0.5,
        }
    }
}

pub fn parse_hybrid(node: Node<'_, '_>) -> HybridParams {
    HybridParams {
        current_tab: param_i32(node, "CurrentTab", 0),
        pre_delay_sync: param_bool(node, "PreDelay_Sync", false),
        pre_delay_time: param_f64(node, "PreDelay_Time", 0.0),
        pre_delay_sixteenth: param_f64(node, "PreDelay_Sixteenth", 4.0),
        pre_delay_feedback_time: param_f64(node, "PreDelay_FeedbackTime", 0.0),
        pre_delay_feedback_sixteenth: param_f64(node, "PreDelay_FeedbackSixteenth", 4.0),
        convolution_ir_post_processing_on: param_bool(
            node,
            "Convolution_IrPostProcessingOn",
            false,
        ),
        convolution_ir_attack_time: param_f64(node, "Convolution_IrAttackTime", 0.0),
        convolution_ir_decay_time: param_f64(node, "Convolution_IrDecayTime", 1.0),
        convolution_ir_size: param_f64(node, "Convolution_IrSize", 1.0),
        algorithm_type: param_i32(node, "Algorithm_Type", 0),
        algorithm_delay: param_f64(node, "Algorithm_Delay", 0.0),
        algorithm_freeze: param_bool(node, "Algorithm_Freeze", false),
        algorithm_freeze_in: param_bool(node, "Algorithm_FreezeIn", false),
        algorithm_decay: param_f64(node, "Algorithm_Decay", 1.0),
        algorithm_size: param_f64(node, "Algorithm_Size", 50.0),
        algorithm_damping: param_f64(node, "Algorithm_Damping", 0.5),
        algorithm_diffusion: param_f64(node, "Algorithm_Diffusion", 0.5),
        algorithm_modulation: param_f64(node, "Algorithm_Modulation", 0.0),
        algorithm_shape: param_f64(node, "Algorithm_Shape", 0.5),
        algorithm_bass_multiplier: param_f64(node, "Algorithm_BassMultiplier", 1.0),
        algorithm_bass_crossover: param_f64(node, "Algorithm_BassCrossover", 200.0),
        algorithm_shimmer: param_f64(node, "Algorithm_Shimmer", 0.0),
        algorithm_pitch_shift: param_f64(node, "Algorithm_PitchShift", 0.0),
        algorithm_tides_amount: param_f64(node, "Algorithm_TidesAmount", 0.0),
        algorithm_tides_rate: param_f64(node, "Algorithm_TidesRate", 1.0),
        algorithm_tides_waveform: param_f64(node, "Algorithm_TidesWaveform", 0.0),
        algorithm_tides_phase_offset: param_f64(node, "Algorithm_TidesPhaseOffset", 0.0),
        eq_on: param_bool(node, "Eq_On", false),
        eq_pre_algo: param_bool(node, "Eq_PreAlgo", false),
        eq_low_band_type: param_i32(node, "Eq_LowBandType", 0),
        eq_low_band_frequency: param_f64(node, "Eq_LowBandFrequency", 200.0),
        eq_low_band_gain: param_f64(node, "Eq_LowBandGain", 0.0),
        eq_low_band_slope: param_f64(node, "Eq_LowBandSlope", 1.0),
        eq_peak1_frequency: param_f64(node, "Eq_Peak1Frequency", 500.0),
        eq_peak1_gain: param_f64(node, "Eq_Peak1Gain", 0.0),
        eq_peak1_q: param_f64(node, "Eq_Peak1Q", 0.71),
        eq_peak2_frequency: param_f64(node, "Eq_Peak2Frequency", 2000.0),
        eq_peak2_gain: param_f64(node, "Eq_Peak2Gain", 0.0),
        eq_peak2_q: param_f64(node, "Eq_Peak2Q", 0.71),
        eq_high_band_type: param_i32(node, "Eq_HighBandType", 0),
        eq_high_band_frequency: param_f64(node, "Eq_HighBandFrequency", 5000.0),
        eq_high_band_gain: param_f64(node, "Eq_HighBandGain", 0.0),
        eq_high_band_slope: param_f64(node, "Eq_HighBandSlope", 1.0),
        send: param_f64(node, "Send", 0.0),
        routing: param_i32(node, "Routing", 0),
        convo_algo_blend: param_f64(node, "ConvoAlgoBlend", 0.5),
        vintage: param_f64(node, "Vintage", 0.0),
        stereo_width: param_f64(node, "StereoWidth", 0.0),
        bass_mono: param_bool(node, "BassMono", false),
        dry_wet: param_f64(node, "DryWet", 0.5),
    }
}

pub fn write_hybrid<W: Write>(w: &mut AbletonXmlWriter<W>, p: &HybridParams) -> io::Result<()> {
    write_param_i32(w, "CurrentTab", p.current_tab)?;
    write_param_bool(w, "PreDelay_Sync", p.pre_delay_sync)?;
    write_param_f64(w, "PreDelay_Time", p.pre_delay_time)?;
    write_param_f64(w, "PreDelay_Sixteenth", p.pre_delay_sixteenth)?;
    write_param_f64(w, "PreDelay_FeedbackTime", p.pre_delay_feedback_time)?;
    write_param_f64(
        w,
        "PreDelay_FeedbackSixteenth",
        p.pre_delay_feedback_sixteenth,
    )?;
    write_param_bool(
        w,
        "Convolution_IrPostProcessingOn",
        p.convolution_ir_post_processing_on,
    )?;
    write_param_f64(w, "Convolution_IrAttackTime", p.convolution_ir_attack_time)?;
    write_param_f64(w, "Convolution_IrDecayTime", p.convolution_ir_decay_time)?;
    write_param_f64(w, "Convolution_IrSize", p.convolution_ir_size)?;
    write_param_i32(w, "Algorithm_Type", p.algorithm_type)?;
    write_param_f64(w, "Algorithm_Delay", p.algorithm_delay)?;
    write_param_bool(w, "Algorithm_Freeze", p.algorithm_freeze)?;
    write_param_bool(w, "Algorithm_FreezeIn", p.algorithm_freeze_in)?;
    write_param_f64(w, "Algorithm_Decay", p.algorithm_decay)?;
    write_param_f64(w, "Algorithm_Size", p.algorithm_size)?;
    write_param_f64(w, "Algorithm_Damping", p.algorithm_damping)?;
    write_param_f64(w, "Algorithm_Diffusion", p.algorithm_diffusion)?;
    write_param_f64(w, "Algorithm_Modulation", p.algorithm_modulation)?;
    write_param_f64(w, "Algorithm_Shape", p.algorithm_shape)?;
    write_param_f64(w, "Algorithm_BassMultiplier", p.algorithm_bass_multiplier)?;
    write_param_f64(w, "Algorithm_BassCrossover", p.algorithm_bass_crossover)?;
    write_param_f64(w, "Algorithm_Shimmer", p.algorithm_shimmer)?;
    write_param_f64(w, "Algorithm_PitchShift", p.algorithm_pitch_shift)?;
    write_param_f64(w, "Algorithm_TidesAmount", p.algorithm_tides_amount)?;
    write_param_f64(w, "Algorithm_TidesRate", p.algorithm_tides_rate)?;
    write_param_f64(w, "Algorithm_TidesWaveform", p.algorithm_tides_waveform)?;
    write_param_f64(
        w,
        "Algorithm_TidesPhaseOffset",
        p.algorithm_tides_phase_offset,
    )?;
    write_param_bool(w, "Eq_On", p.eq_on)?;
    write_param_bool(w, "Eq_PreAlgo", p.eq_pre_algo)?;
    write_param_i32(w, "Eq_LowBandType", p.eq_low_band_type)?;
    write_param_f64(w, "Eq_LowBandFrequency", p.eq_low_band_frequency)?;
    write_param_f64(w, "Eq_LowBandGain", p.eq_low_band_gain)?;
    write_param_f64(w, "Eq_LowBandSlope", p.eq_low_band_slope)?;
    write_param_f64(w, "Eq_Peak1Frequency", p.eq_peak1_frequency)?;
    write_param_f64(w, "Eq_Peak1Gain", p.eq_peak1_gain)?;
    write_param_f64(w, "Eq_Peak1Q", p.eq_peak1_q)?;
    write_param_f64(w, "Eq_Peak2Frequency", p.eq_peak2_frequency)?;
    write_param_f64(w, "Eq_Peak2Gain", p.eq_peak2_gain)?;
    write_param_f64(w, "Eq_Peak2Q", p.eq_peak2_q)?;
    write_param_i32(w, "Eq_HighBandType", p.eq_high_band_type)?;
    write_param_f64(w, "Eq_HighBandFrequency", p.eq_high_band_frequency)?;
    write_param_f64(w, "Eq_HighBandGain", p.eq_high_band_gain)?;
    write_param_f64(w, "Eq_HighBandSlope", p.eq_high_band_slope)?;
    write_param_f64(w, "Send", p.send)?;
    write_param_i32(w, "Routing", p.routing)?;
    write_param_f64(w, "ConvoAlgoBlend", p.convo_algo_blend)?;
    write_param_f64(w, "Vintage", p.vintage)?;
    write_param_f64(w, "StereoWidth", p.stereo_width)?;
    write_param_bool(w, "BassMono", p.bass_mono)?;
    write_param_f64(w, "DryWet", p.dry_wet)?;
    Ok(())
}
