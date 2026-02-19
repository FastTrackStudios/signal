//! Rotary block presets — modeled after iconic rotary speaker emulations.
//!
//! Each preset corresponds to a real unit with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![leslie_122(), vent()]
}

// ─── Leslie 122 ─────────────────────────────────────────────────
// Knobs: Speed, Drive, Mix, Balance
// The definitive rotary speaker — dual rotor (horn + drum) with tube preamp.

fn leslie_block(speed: f32, drive: f32, mix: f32, balance: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("balance", "Balance", balance),
    ])
}

fn leslie_122() -> Preset {
    Preset::new(
        seed_id("rotary-leslie"),
        "Leslie 122",
        BlockType::Rotary,
        // Default: slow chorale speed — warm, gentle rotation
        Snapshot::new(
            seed_id("rotary-leslie-default"),
            "Default",
            leslie_block(0.30, 0.35, 0.55, 0.50),
        ),
        vec![
            // Fast — tremolo speed for dramatic organ-style whirl
            Snapshot::new(
                seed_id("rotary-leslie-fast"),
                "Fast",
                leslie_block(0.80, 0.40, 0.60, 0.50),
            ),
            // Brake — rotor stopped, just the cabinet character remains
            Snapshot::new(
                seed_id("rotary-leslie-brake"),
                "Brake",
                leslie_block(0.0, 0.30, 0.50, 0.50),
            ),
        ],
    )
}

// ─── Neo Instruments Vent ───────────────────────────────────────
// Knobs: Speed, Depth, Mix

fn vent_block(speed: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn vent() -> Preset {
    Preset::new(
        seed_id("rotary-vent"),
        "Vent",
        BlockType::Rotary,
        // Default: balanced rotary effect
        Snapshot::new(
            seed_id("rotary-vent-default"),
            "Default",
            vent_block(0.40, 0.50, 0.50),
        ),
        vec![
            // Slow swirl — gentle movement for ambient pads
            Snapshot::new(
                seed_id("rotary-vent-slow"),
                "Slow Swirl",
                vent_block(0.18, 0.40, 0.55),
            ),
            // Full spin — cranked speed and depth for maximum rotation
            Snapshot::new(
                seed_id("rotary-vent-full"),
                "Full Spin",
                vent_block(0.85, 0.75, 0.65),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotary_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_rotary_presets_are_rotary_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Rotary);
        }
    }

    #[test]
    fn leslie_has_3_snapshots() {
        let l = &presets()[0];
        assert_eq!(l.name(), "Leslie 122");
        assert_eq!(l.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn vent_has_3_snapshots() {
        let v = &presets()[1];
        assert_eq!(v.name(), "Vent");
        assert_eq!(v.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn leslie_has_4_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "Leslie 122 should have 4 params"
            );
        }
    }

    #[test]
    fn vent_has_3_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Vent should have 3 params"
            );
        }
    }

    #[test]
    fn leslie_brake_has_zero_speed() {
        let l = &presets()[0];
        let brake = l.snapshots().iter().find(|s| s.name() == "Brake").unwrap();
        let block = brake.block();
        let params = block.parameters();
        let speed = params.iter().find(|p| p.id() == "speed").unwrap();
        assert!((speed.value().get() - 0.0).abs() < f32::EPSILON);
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
