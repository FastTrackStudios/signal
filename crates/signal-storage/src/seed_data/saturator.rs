//! Saturator block presets.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![t_saturator()]
}

fn tsat_block(drive: f32, tone: f32, mix: f32, output: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("output", "Output", output),
    ])
}

fn t_saturator() -> Preset {
    Preset::new(
        seed_id("saturator-tsat"),
        "T Saturator",
        BlockType::Saturator,
        Snapshot::new(
            seed_id("saturator-tsat-default"),
            "Default",
            tsat_block(0.40, 0.55, 0.65, 0.50),
        ),
        vec![
            Snapshot::new(
                seed_id("saturator-tsat-warm"),
                "Warm",
                tsat_block(0.32, 0.45, 0.70, 0.52),
            ),
            Snapshot::new(
                seed_id("saturator-tsat-hot"),
                "Hot",
                tsat_block(0.70, 0.60, 0.78, 0.46),
            ),
        ],
    )
}
