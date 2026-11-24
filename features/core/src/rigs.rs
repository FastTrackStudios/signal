//! Factory presets: what a fresh instance can start from.

use alloc::vec;
use alloc::vec::Vec;

use crate::host::AssetRef;
use crate::state::{BlockSpec, LaneSpec, MacroMode, MacroSpec, MacroTarget, Preset};

fn target(
    lane: &str,
    block: &str,
    param: &str,
    mode: MacroMode,
    min: f32,
    max: f32,
) -> MacroTarget {
    MacroTarget {
        lane: lane.into(),
        block: block.into(),
        param: param.into(),
        mode,
        min,
        max,
    }
}

/// A stereo mix chain: EQ, compressor, delay, reverb. Macro 1 ("Tone")
/// sets the EQ's top end; macro 2 ("Space") modulates the delay and reverb
/// sends up from where they are set.
#[must_use]
pub fn dsp_chain() -> Preset {
    Preset {
        name: "Mix Chain".into(),
        inputs: 2,
        outputs: 2,
        lanes: vec![LaneSpec {
            name: "Main".into(),
            port: None,
            input: vec![0, 1],
            output: 0,
            gain_db: 0.0,
            mute: false,
            blocks: vec![
                BlockSpec::signal("eq", "eq")
                    .param("low_cut", 60.0)
                    .param("mid_freq", 400.0)
                    .param("mid_gain", -2.0),
                BlockSpec::signal("comp", "comp")
                    .param("threshold", -20.0)
                    .param("ratio", 3.0)
                    .param("makeup", 3.0),
                BlockSpec::signal("delay", "delay")
                    .param("time", 375.0)
                    .param("mix", 0.1),
                BlockSpec::signal("reverb", "reverb")
                    .param("algorithm", 2.0)
                    .param("mix", 0.15),
            ],
        }],
        macros: vec![
            MacroSpec {
                name: "Tone".into(),
                targets: vec![target("Main", "eq", "high_gain", MacroMode::Set, -6.0, 6.0)],
            },
            MacroSpec {
                name: "Space".into(),
                targets: vec![
                    target("Main", "delay", "mix", MacroMode::Modulate, 0.0, 0.3),
                    target("Main", "reverb", "mix", MacroMode::Modulate, 0.0, 0.4),
                ],
            },
        ],
    }
}

/// The guitar rig: mono guitar in on channel 1, stereo out. Gate, drive,
/// the amp (a NAM capture), the cab (an IR), EQ, delay, reverb. The
/// captures are private (AGENTS.md): the preset only names them.
///
/// Macro 1 ("Gain") drives the amp; macro 2 ("Ambience") brings the delay
/// and reverb up.
#[must_use]
pub fn guitar_rig(amp: AssetRef, cab: AssetRef) -> Preset {
    let blocks: Vec<BlockSpec> = vec![
        BlockSpec::signal("gate", "gate")
            .param("threshold", -60.0)
            .param("reduction", -30.0)
            .param("length", 150.0),
        BlockSpec::signal("drive", "drive")
            .param("drive", 3.0)
            .param("mix", 0.5),
        BlockSpec::signal("amp", "nam").asset(amp),
        BlockSpec::signal("cab", "cab").asset(cab),
        BlockSpec::signal("eq", "eq")
            .param("low_cut", 80.0)
            .param("high_cut", 9000.0),
        BlockSpec::signal("delay", "delay")
            .param("time", 420.0)
            .param("mix", 0.12),
        BlockSpec::signal("reverb", "reverb")
            .param("algorithm", 0.0)
            .param("mix", 0.12),
    ];
    Preset {
        name: "Guitar Rig".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![LaneSpec {
            name: "Guitar".into(),
            port: None,
            input: vec![0],
            output: 0,
            gain_db: 0.0,
            mute: false,
            blocks,
        }],
        macros: vec![
            MacroSpec {
                name: "Gain".into(),
                targets: vec![target(
                    "Guitar",
                    "amp",
                    "input",
                    MacroMode::Set,
                    -12.0,
                    12.0,
                )],
            },
            MacroSpec {
                name: "Ambience".into(),
                targets: vec![
                    target("Guitar", "delay", "mix", MacroMode::Modulate, 0.0, 0.25),
                    target("Guitar", "reverb", "mix", MacroMode::Modulate, 0.0, 0.3),
                ],
            },
        ],
    }
}
