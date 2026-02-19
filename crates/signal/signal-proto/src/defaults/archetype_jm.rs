//! Example PluginBlockDef for Neural DSP "Archetype: John Mayer X".
//!
//! This demonstrates how a single plugin with ~200 parameters is decomposed
//! into virtual modules and blocks for the grid UI.

use crate::plugin_block::{ParamMapping, PluginBlockDef, VirtualBlock, VirtualModule};
use crate::{BlockType, ModuleType};

/// Build an example `PluginBlockDef` for Archetype: John Mayer X.
///
/// Parameter indices are illustrative — real values come from the DAW's
/// parameter enumeration at runtime.
pub fn archetype_john_mayer() -> PluginBlockDef {
    PluginBlockDef::new("Archetype: John Mayer X", 204)
        .with_vendor("Neural DSP")
        .with_module(
            VirtualModule::new("pedals", "Pedals", ModuleType::PreFx)
                .with_block(
                    VirtualBlock::new("justa-boost", "Justa Boost", BlockType::Boost)
                        .with_params(vec![
                            ParamMapping::new("Level", 0, 0.5),
                            ParamMapping::new("Tone", 1, 0.5),
                            ParamMapping::new("On/Off", 2, 1.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("antelope-filter", "Antelope Filter", BlockType::Filter)
                        .with_params(vec![
                            ParamMapping::new("Frequency", 10, 0.5),
                            ParamMapping::new("Resonance", 11, 0.3),
                            ParamMapping::new("On/Off", 12, 0.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("halfman-od", "Halfman OD", BlockType::Drive)
                        .with_params(vec![
                            ParamMapping::new("Gain", 20, 0.4),
                            ParamMapping::new("Tone", 21, 0.5),
                            ParamMapping::new("Volume", 22, 0.6),
                            ParamMapping::new("On/Off", 23, 0.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("tealbreaker", "Tealbreaker", BlockType::Drive)
                        .with_params(vec![
                            ParamMapping::new("Drive", 30, 0.5),
                            ParamMapping::new("Tone", 31, 0.5),
                            ParamMapping::new("Level", 32, 0.5),
                            ParamMapping::new("On/Off", 33, 1.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("millipede-delay", "Millipede Delay", BlockType::Delay)
                        .with_params(vec![
                            ParamMapping::new("Time", 40, 0.4),
                            ParamMapping::new("Feedback", 41, 0.3),
                            ParamMapping::new("Mix", 42, 0.25),
                            ParamMapping::new("On/Off", 43, 0.0),
                        ]),
                ),
        )
        .with_module(
            VirtualModule::new("pre-fx", "Pre-FX", ModuleType::PreFx)
                .with_block(
                    VirtualBlock::new("harmonic-tremolo", "Harmonic Tremolo", BlockType::Tremolo)
                        .with_params(vec![
                            ParamMapping::new("Rate", 50, 0.3),
                            ParamMapping::new("Depth", 51, 0.6),
                            ParamMapping::new("Mix", 52, 0.5),
                            ParamMapping::new("On/Off", 53, 0.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("spring-reverb", "Spring Reverb", BlockType::Reverb)
                        .with_params(vec![
                            ParamMapping::new("Decay", 60, 0.4),
                            ParamMapping::new("Tone", 61, 0.5),
                            ParamMapping::new("Mix", 62, 0.3),
                            ParamMapping::new("On/Off", 63, 0.0),
                        ]),
                ),
        )
        .with_module(
            VirtualModule::new("amp", "Amp", ModuleType::Amp)
                .with_block(
                    VirtualBlock::new("amp-block", "Amp", BlockType::Amp)
                        .with_params(vec![
                            ParamMapping::new("Gain", 80, 0.45),
                            ParamMapping::new("Bass", 81, 0.5),
                            ParamMapping::new("Mid", 82, 0.55),
                            ParamMapping::new("Treble", 83, 0.6),
                            ParamMapping::new("Presence", 84, 0.5),
                            ParamMapping::new("Master", 85, 0.5),
                        ]),
                ),
        )
        .with_module(
            VirtualModule::new("cab", "Cab", ModuleType::Amp)
                .with_block(
                    VirtualBlock::new("cab-block", "Cabinet", BlockType::Cabinet)
                        .with_params(vec![
                            ParamMapping::new("Mic Position", 100, 0.5),
                            ParamMapping::new("Room", 101, 0.3),
                            ParamMapping::new("Low Cut", 102, 0.2),
                            ParamMapping::new("High Cut", 103, 0.8),
                        ]),
                ),
        )
        .with_module(
            VirtualModule::new("eq", "EQ", ModuleType::Eq)
                .with_block(
                    VirtualBlock::new("eq-block", "EQ", BlockType::Eq)
                        .with_params(vec![
                            ParamMapping::new("Low", 120, 0.5),
                            ParamMapping::new("Low-Mid", 121, 0.5),
                            ParamMapping::new("High-Mid", 122, 0.5),
                            ParamMapping::new("High", 123, 0.5),
                            ParamMapping::new("On/Off", 124, 1.0),
                        ]),
                ),
        )
        .with_module(
            VirtualModule::new("post-fx", "Post-FX", ModuleType::Time)
                .with_block(
                    VirtualBlock::new("dream-delay", "Dream Delay", BlockType::Delay)
                        .with_params(vec![
                            ParamMapping::new("Time", 140, 0.5),
                            ParamMapping::new("Feedback", 141, 0.4),
                            ParamMapping::new("Mod Rate", 142, 0.3),
                            ParamMapping::new("Mod Depth", 143, 0.2),
                            ParamMapping::new("Mix", 144, 0.3),
                            ParamMapping::new("On/Off", 145, 1.0),
                        ]),
                )
                .with_block(
                    VirtualBlock::new("studio-verb", "Studio Verb", BlockType::Reverb)
                        .with_params(vec![
                            ParamMapping::new("Decay", 160, 0.5),
                            ParamMapping::new("Pre-Delay", 161, 0.2),
                            ParamMapping::new("Damping", 162, 0.5),
                            ParamMapping::new("Size", 163, 0.6),
                            ParamMapping::new("Mix", 164, 0.25),
                            ParamMapping::new("On/Off", 165, 1.0),
                        ]),
                ),
        )
}
