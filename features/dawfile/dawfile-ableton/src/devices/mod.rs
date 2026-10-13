//! Typed parameter models for Ableton built-in devices.
//!
//! Each sub-module contains parameter structs, a parser, and a writer for one
//! category of devices. The [`BuiltinParams`] enum wraps all variants so a
//! [`Device`](crate::types::Device) can carry its recognised parameters.

pub mod amp_distortion;
pub mod creative;
pub mod delay_reverb;
pub mod dynamics;
pub mod eq;
pub mod filter;
pub mod instruments;
pub mod modulation;
pub mod racks;
pub mod utility;

use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

/// Typed parameters for known Ableton built-in devices.
/// Enables cross-DAW parameter translation (e.g., Ableton EQ Eight -> REAPER ReaEQ).
#[derive(Debug, Clone)]
pub enum BuiltinParams {
    Eq8(eq::Eq8Params),
    Compressor(dynamics::CompressorParams),
    GlueCompressor(dynamics::GlueCompressorParams),
    Gate(dynamics::GateParams),
    Limiter(dynamics::LimiterParams),
    MultibandDynamics(dynamics::MultibandDynamicsParams),
    Reverb(delay_reverb::ReverbParams),
    Delay(delay_reverb::DelayParams),
    Echo(delay_reverb::EchoParams),
    AutoFilter(modulation::AutoFilterParams),
    Chorus(modulation::ChorusParams),
    ChorusLegacy(modulation::ChorusLegacyParams),
    Phaser(modulation::PhaserParams),
    PhaserNew(modulation::PhaserNewParams),
    Flanger(modulation::FlangerParams),
    Saturator(modulation::SaturatorParams),
    Utility(utility::UtilityParams),
    Tuner(utility::TunerParams),
    Cabinet(utility::CabinetParams),
    Erosion(utility::ErosionParams),
    Redux(utility::ReduxParams),
    ReduxLegacy(utility::ReduxLegacyParams),
    Vinyl(utility::VinylParams),
    // Amp & distortion
    Amp(amp_distortion::AmpParams),
    Overdrive(amp_distortion::OverdriveParams),
    Pedal(amp_distortion::PedalParams),
    DrumBuss(amp_distortion::DrumBussParams),
    Tube(amp_distortion::TubeParams),
    Roar(amp_distortion::RoarParams),
    // Filter & frequency domain
    ChannelEq(filter::ChannelEqParams),
    FilterEq3(filter::FilterEq3Params),
    AutoPan(filter::AutoPanParams),
    FrequencyShifter(filter::FrequencyShifterParams),
    Shifter(filter::ShifterParams),
    Spectral(filter::SpectralParams),
    Transmute(filter::TransmuteParams),
    // Creative
    BeatRepeat(creative::BeatRepeatParams),
    Corpus(creative::CorpusParams),
    Resonator(creative::ResonatorParams),
    Vocoder(creative::VocoderParams),
    Looper(creative::LooperParams),
    GrainDelay(creative::GrainDelayParams),
    FilterDelay(creative::FilterDelayParams),
    Hybrid(creative::HybridParams),
    // Instruments
    Simpler(instruments::SimplerParams),
    Sampler(instruments::SamplerParams),
    Operator(instruments::OperatorParams),
    Drift(instruments::DriftParams),
    Wavetable(instruments::WavetableParams),
    Meld(instruments::MeldParams),
    Collision(instruments::CollisionParams),
    Tension(instruments::TensionParams),
    Electric(instruments::ElectricParams),
    Analog(instruments::AnalogParams),
    Impulse(instruments::ImpulseParams),
    // Racks
    DrumRack(racks::RackParams),
    InstrumentRack(racks::RackParams),
    AudioEffectRack(racks::RackParams),
}

/// Try to parse typed builtin parameters from a device XML node.
///
/// Returns `None` if `tag` is not a recognised built-in device.
pub fn parse_builtin_params(tag: &str, node: Node<'_, '_>) -> Option<BuiltinParams> {
    match tag {
        "Eq8" => Some(BuiltinParams::Eq8(eq::parse(node))),
        "Compressor2" => Some(BuiltinParams::Compressor(dynamics::parse_compressor(node))),
        "GlueCompressor" => Some(BuiltinParams::GlueCompressor(
            dynamics::parse_glue_compressor(node),
        )),
        "Gate" => Some(BuiltinParams::Gate(dynamics::parse_gate(node))),
        "Limiter" => Some(BuiltinParams::Limiter(dynamics::parse_limiter(node))),
        "MultibandDynamics" => Some(BuiltinParams::MultibandDynamics(
            dynamics::parse_multiband_dynamics(node),
        )),
        "Reverb" => Some(BuiltinParams::Reverb(delay_reverb::parse_reverb(node))),
        "Delay" => Some(BuiltinParams::Delay(delay_reverb::parse_delay(node))),
        "Echo" => Some(BuiltinParams::Echo(delay_reverb::parse_echo(node))),
        "AutoFilter" => Some(BuiltinParams::AutoFilter(modulation::parse_auto_filter(
            node,
        ))),
        "Chorus2" | "ChorusEnsemble" => Some(BuiltinParams::Chorus(modulation::parse_chorus(node))),
        "Chorus" => Some(BuiltinParams::ChorusLegacy(
            modulation::parse_chorus_legacy(node),
        )),
        "Phaser" => Some(BuiltinParams::Phaser(modulation::parse_phaser(node))),
        "PhaserNew" => Some(BuiltinParams::PhaserNew(modulation::parse_phaser_new(node))),
        "Flanger" => Some(BuiltinParams::Flanger(modulation::parse_flanger(node))),
        "Saturator" => Some(BuiltinParams::Saturator(modulation::parse_saturator(node))),
        "StereoGain" => Some(BuiltinParams::Utility(utility::parse_utility(node))),
        "Tuner" => Some(BuiltinParams::Tuner(utility::parse_tuner(node))),
        "Cabinet" => Some(BuiltinParams::Cabinet(utility::parse_cabinet(node))),
        "Erosion" => Some(BuiltinParams::Erosion(utility::parse_erosion(node))),
        "Redux2" => Some(BuiltinParams::Redux(utility::parse_redux(node))),
        "Redux" => Some(BuiltinParams::ReduxLegacy(utility::parse_redux_legacy(
            node,
        ))),
        "Vinyl" => Some(BuiltinParams::Vinyl(utility::parse_vinyl(node))),
        // Amp & distortion
        "Amp" => Some(BuiltinParams::Amp(amp_distortion::parse_amp(node))),
        "Overdrive" => Some(BuiltinParams::Overdrive(amp_distortion::parse_overdrive(
            node,
        ))),
        "Pedal" => Some(BuiltinParams::Pedal(amp_distortion::parse_pedal(node))),
        "DrumBuss" => Some(BuiltinParams::DrumBuss(amp_distortion::parse_drum_buss(
            node,
        ))),
        "Tube" => Some(BuiltinParams::Tube(amp_distortion::parse_tube(node))),
        "Roar" => Some(BuiltinParams::Roar(amp_distortion::parse_roar(node))),
        // Filter & frequency domain
        "ChannelEq" => Some(BuiltinParams::ChannelEq(filter::parse_channel_eq(node))),
        "FilterEQ3" => Some(BuiltinParams::FilterEq3(filter::parse_filter_eq3(node))),
        "AutoPan" => Some(BuiltinParams::AutoPan(filter::parse_auto_pan(node))),
        "FrequencyShifter" => Some(BuiltinParams::FrequencyShifter(
            filter::parse_frequency_shifter(node),
        )),
        "Shifter" => Some(BuiltinParams::Shifter(filter::parse_shifter(node))),
        "Spectral" => Some(BuiltinParams::Spectral(filter::parse_spectral(node))),
        "Transmute" => Some(BuiltinParams::Transmute(filter::parse_transmute(node))),
        // Creative
        "BeatRepeat" => Some(BuiltinParams::BeatRepeat(creative::parse_beat_repeat(node))),
        "Corpus" => Some(BuiltinParams::Corpus(creative::parse_corpus(node))),
        "Resonator" => Some(BuiltinParams::Resonator(creative::parse_resonator(node))),
        "Vocoder" => Some(BuiltinParams::Vocoder(creative::parse_vocoder(node))),
        "Looper" => Some(BuiltinParams::Looper(creative::parse_looper(node))),
        "GrainDelay" => Some(BuiltinParams::GrainDelay(creative::parse_grain_delay(node))),
        "FilterDelay" => Some(BuiltinParams::FilterDelay(creative::parse_filter_delay(
            node,
        ))),
        "Hybrid" => Some(BuiltinParams::Hybrid(creative::parse_hybrid(node))),
        // Instruments
        "OriginalSimpler" => Some(BuiltinParams::Simpler(instruments::parse_simpler(node))),
        "MultiSampler" => Some(BuiltinParams::Sampler(instruments::parse_sampler(node))),
        "Operator" => Some(BuiltinParams::Operator(instruments::parse_operator(node))),
        "Drift" => Some(BuiltinParams::Drift(instruments::parse_drift(node))),
        "InstrumentVector" => Some(BuiltinParams::Wavetable(instruments::parse_wavetable(node))),
        "InstrumentMeld" => Some(BuiltinParams::Meld(instruments::parse_meld(node))),
        "Collision" => Some(BuiltinParams::Collision(instruments::parse_collision(node))),
        "StringStudio" => Some(BuiltinParams::Tension(instruments::parse_tension(node))),
        "LoungeLizard" => Some(BuiltinParams::Electric(instruments::parse_electric(node))),
        "UltraAnalog" => Some(BuiltinParams::Analog(instruments::parse_analog(node))),
        "InstrumentImpulse" => Some(BuiltinParams::Impulse(instruments::parse_impulse(node))),
        // Racks
        "DrumGroupDevice" => Some(BuiltinParams::DrumRack(racks::parse_rack(node))),
        "InstrumentGroupDevice" => Some(BuiltinParams::InstrumentRack(racks::parse_rack(node))),
        "AudioEffectGroupDevice" => Some(BuiltinParams::AudioEffectRack(racks::parse_rack(node))),
        _ => None,
    }
}

/// Write the typed builtin parameters into an already-opened device element.
pub fn write_builtin_params<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    params: &BuiltinParams,
) -> io::Result<()> {
    match params {
        BuiltinParams::Eq8(p) => eq::write(w, p),
        BuiltinParams::Compressor(p) => dynamics::write_compressor(w, p),
        BuiltinParams::GlueCompressor(p) => dynamics::write_glue_compressor(w, p),
        BuiltinParams::Gate(p) => dynamics::write_gate(w, p),
        BuiltinParams::Limiter(p) => dynamics::write_limiter(w, p),
        BuiltinParams::MultibandDynamics(p) => dynamics::write_multiband_dynamics(w, p),
        BuiltinParams::Reverb(p) => delay_reverb::write_reverb(w, p),
        BuiltinParams::Delay(p) => delay_reverb::write_delay(w, p),
        BuiltinParams::Echo(p) => delay_reverb::write_echo(w, p),
        BuiltinParams::AutoFilter(p) => modulation::write_auto_filter(w, p),
        BuiltinParams::Chorus(p) => modulation::write_chorus(w, p),
        BuiltinParams::ChorusLegacy(p) => modulation::write_chorus_legacy(w, p),
        BuiltinParams::Phaser(p) => modulation::write_phaser(w, p),
        BuiltinParams::PhaserNew(p) => modulation::write_phaser_new(w, p),
        BuiltinParams::Flanger(p) => modulation::write_flanger(w, p),
        BuiltinParams::Saturator(p) => modulation::write_saturator(w, p),
        BuiltinParams::Utility(p) => utility::write_utility(w, p),
        BuiltinParams::Tuner(p) => utility::write_tuner(w, p),
        BuiltinParams::Cabinet(p) => utility::write_cabinet(w, p),
        BuiltinParams::Erosion(p) => utility::write_erosion(w, p),
        BuiltinParams::Redux(p) => utility::write_redux(w, p),
        BuiltinParams::ReduxLegacy(p) => utility::write_redux_legacy(w, p),
        BuiltinParams::Vinyl(p) => utility::write_vinyl(w, p),
        // Amp & distortion
        BuiltinParams::Amp(p) => amp_distortion::write_amp(w, p),
        BuiltinParams::Overdrive(p) => amp_distortion::write_overdrive(w, p),
        BuiltinParams::Pedal(p) => amp_distortion::write_pedal(w, p),
        BuiltinParams::DrumBuss(p) => amp_distortion::write_drum_buss(w, p),
        BuiltinParams::Tube(p) => amp_distortion::write_tube(w, p),
        BuiltinParams::Roar(p) => amp_distortion::write_roar(w, p),
        // Filter & frequency domain
        BuiltinParams::ChannelEq(p) => filter::write_channel_eq(w, p),
        BuiltinParams::FilterEq3(p) => filter::write_filter_eq3(w, p),
        BuiltinParams::AutoPan(p) => filter::write_auto_pan(w, p),
        BuiltinParams::FrequencyShifter(p) => filter::write_frequency_shifter(w, p),
        BuiltinParams::Shifter(p) => filter::write_shifter(w, p),
        BuiltinParams::Spectral(p) => filter::write_spectral(w, p),
        BuiltinParams::Transmute(p) => filter::write_transmute(w, p),
        // Creative
        BuiltinParams::BeatRepeat(p) => creative::write_beat_repeat(w, p),
        BuiltinParams::Corpus(p) => creative::write_corpus(w, p),
        BuiltinParams::Resonator(p) => creative::write_resonator(w, p),
        BuiltinParams::Vocoder(p) => creative::write_vocoder(w, p),
        BuiltinParams::Looper(p) => creative::write_looper(w, p),
        BuiltinParams::GrainDelay(p) => creative::write_grain_delay(w, p),
        BuiltinParams::FilterDelay(p) => creative::write_filter_delay(w, p),
        BuiltinParams::Hybrid(p) => creative::write_hybrid(w, p),
        // Instruments
        BuiltinParams::Simpler(p) => instruments::write_simpler(w, p),
        BuiltinParams::Sampler(p) => instruments::write_sampler(w, p),
        BuiltinParams::Operator(p) => instruments::write_operator(w, p),
        BuiltinParams::Drift(p) => instruments::write_drift(w, p),
        BuiltinParams::Wavetable(p) => instruments::write_wavetable(w, p),
        BuiltinParams::Meld(p) => instruments::write_meld(w, p),
        BuiltinParams::Collision(p) => instruments::write_collision(w, p),
        BuiltinParams::Tension(p) => instruments::write_tension(w, p),
        BuiltinParams::Electric(p) => instruments::write_electric(w, p),
        BuiltinParams::Analog(p) => instruments::write_analog(w, p),
        BuiltinParams::Impulse(p) => instruments::write_impulse(w, p),
        // Racks
        BuiltinParams::DrumRack(p) => racks::write_rack(w, p),
        BuiltinParams::InstrumentRack(p) => racks::write_rack(w, p),
        BuiltinParams::AudioEffectRack(p) => racks::write_rack(w, p),
    }
}
