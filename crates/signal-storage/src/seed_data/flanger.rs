//! Flanger block presets — modeled after iconic flanger pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![electric_mistress(), bf3()]
}

// ─── Electro-Harmonix Electric Mistress ─────────────────────────
// Knobs: Rate, Range, Color

fn mistress_block(rate: f32, range: f32, color: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("range", "Range", range),
        BlockParameter::new("color", "Color", color),
    ])
}

fn electric_mistress() -> Preset {
    Preset::new(
        seed_id("flanger-mistress"),
        "Electric Mistress",
        BlockType::Flanger,
        // Default: classic through-zero flanger sweep
        Snapshot::new(
            seed_id("flanger-mistress-default"),
            "Default",
            mistress_block(0.40, 0.50, 0.50),
        ),
        vec![
            // Jet — fast rate, wide range for dramatic jet-plane sweep
            Snapshot::new(
                seed_id("flanger-mistress-jet"),
                "Jet Sweep",
                mistress_block(0.75, 0.80, 0.65),
            ),
            // Subtle — slow, narrow sweep for gentle chorus-like movement
            Snapshot::new(
                seed_id("flanger-mistress-subtle"),
                "Subtle",
                mistress_block(0.18, 0.30, 0.35),
            ),
        ],
    )
}

// ─── Boss BF-3 ──────────────────────────────────────────────────
// Knobs: Rate, Depth, Resonance, Mix

fn bf3_block(rate: f32, depth: f32, resonance: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("resonance", "Resonance", resonance),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn bf3() -> Preset {
    Preset::new(
        seed_id("flanger-bf3"),
        "BF-3",
        BlockType::Flanger,
        // Default: classic Boss flanger — moderate everything
        Snapshot::new(
            seed_id("flanger-bf3-default"),
            "Default",
            bf3_block(0.45, 0.50, 0.45, 0.50),
        ),
        vec![
            // Metallic — high resonance for sharp, ringing flange
            Snapshot::new(
                seed_id("flanger-bf3-metallic"),
                "Metallic",
                bf3_block(0.55, 0.60, 0.82, 0.60),
            ),
            // Warm swirl — slow rate, low resonance, deep mix
            Snapshot::new(
                seed_id("flanger-bf3-warm"),
                "Warm Swirl",
                bf3_block(0.20, 0.65, 0.25, 0.55),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flanger_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_flanger_presets_are_flanger_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Flanger);
        }
    }

    #[test]
    fn electric_mistress_has_3_snapshots() {
        let em = &presets()[0];
        assert_eq!(em.name(), "Electric Mistress");
        assert_eq!(em.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn bf3_has_3_snapshots() {
        let bf = &presets()[1];
        assert_eq!(bf.name(), "BF-3");
        assert_eq!(bf.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn electric_mistress_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Electric Mistress should have 3 params"
            );
        }
    }

    #[test]
    fn bf3_has_4_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "BF-3 should have 4 params"
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
