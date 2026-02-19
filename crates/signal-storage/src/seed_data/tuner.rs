//! Tuner block presets.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![graillon3()]
}

fn graillon3_block(speed: f32, sensitivity: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("sensitivity", "Sensitivity", sensitivity),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn graillon3() -> Preset {
    Preset::new(
        seed_id("tuner-graillon3"),
        "Graillon 3",
        BlockType::Tuner,
        Snapshot::new(
            seed_id("tuner-graillon3-default"),
            "Default",
            graillon3_block(0.50, 0.60, 1.0),
        ),
        vec![
            Snapshot::new(
                seed_id("tuner-graillon3-fast"),
                "Fast",
                graillon3_block(0.82, 0.55, 1.0),
            ),
            Snapshot::new(
                seed_id("tuner-graillon3-gentle"),
                "Gentle",
                graillon3_block(0.35, 0.70, 0.85),
            ),
        ],
    )
}
