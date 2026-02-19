//! Phaser block presets — modeled after iconic phaser pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![phase_90(), small_stone()]
}

// ─── MXR Phase 90 ───────────────────────────────────────────────
// Knobs: Speed
// The classic single-knob phaser.

fn phase90_block(speed: f32) -> Block {
    Block::from_parameters(vec![BlockParameter::new("speed", "Speed", speed)])
}

fn phase_90() -> Preset {
    Preset::new(
        seed_id("phaser-phase90"),
        "Phase 90",
        BlockType::Phaser,
        // Default: moderate speed — the classic Van Halen-era swirl
        Snapshot::new(
            seed_id("phaser-phase90-default"),
            "Default",
            phase90_block(0.45),
        ),
        vec![
            // Slow swirl — gentle, watery movement for clean passages
            Snapshot::new(
                seed_id("phaser-phase90-slow"),
                "Slow Swirl",
                phase90_block(0.15),
            ),
            // Fast — rapid Leslie-like effect for dramatic moments
            Snapshot::new(seed_id("phaser-phase90-fast"), "Fast", phase90_block(0.80)),
        ],
    )
}

// ─── Electro-Harmonix Small Stone ───────────────────────────────
// Knobs: Rate, Color

fn small_stone_block(rate: f32, color: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("color", "Color", color),
    ])
}

fn small_stone() -> Preset {
    Preset::new(
        seed_id("phaser-small-stone"),
        "Small Stone",
        BlockType::Phaser,
        // Default: warm phase with moderate rate
        Snapshot::new(
            seed_id("phaser-small-stone-default"),
            "Default",
            small_stone_block(0.45, 0.50),
        ),
        vec![
            // Deep color — color switch engaged for more dramatic sweep
            Snapshot::new(
                seed_id("phaser-small-stone-deep"),
                "Deep Color",
                small_stone_block(0.40, 0.85),
            ),
            // Mellow wash — slow rate, low color for subtle background phase
            Snapshot::new(
                seed_id("phaser-small-stone-mellow"),
                "Mellow Wash",
                small_stone_block(0.18, 0.25),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phaser_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_phaser_presets_are_phaser_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Phaser);
        }
    }

    #[test]
    fn phase_90_has_3_snapshots() {
        let p90 = &presets()[0];
        assert_eq!(p90.name(), "Phase 90");
        assert_eq!(p90.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn small_stone_has_3_snapshots() {
        let ss = &presets()[1];
        assert_eq!(ss.name(), "Small Stone");
        assert_eq!(ss.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn phase_90_has_1_param() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                1,
                "Phase 90 should have 1 param"
            );
        }
    }

    #[test]
    fn small_stone_has_2_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                2,
                "Small Stone should have 2 params"
            );
        }
    }

    #[test]
    fn parameter_values_in_range() {
        for preset in presets() {
            for snapshot in preset.snapshots() {
                let block = snapshot.block();
                for param in block.parameters() {
                    let v = param.value().get();
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "preset '{}' snapshot '{}' param '{}' value {} out of range",
                        preset.name(),
                        snapshot.name(),
                        param.id(),
                        v,
                    );
                }
            }
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            assert!(
                ids.insert(preset.id().to_string()),
                "duplicate preset id: {}",
                preset.id()
            );
        }
    }

    #[test]
    fn snapshot_ids_globally_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            for snapshot in preset.snapshots() {
                assert!(
                    ids.insert(snapshot.id().to_string()),
                    "duplicate snapshot id: {}",
                    snapshot.id()
                );
            }
        }
    }
}
