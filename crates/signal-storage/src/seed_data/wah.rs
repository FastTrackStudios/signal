//! Wah block presets — modeled after iconic wah pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![cry_baby(), vox_wah()]
}

// ─── Dunlop Cry Baby ───────────────────────────────────────────
// Knobs: Position, Q, Range

fn crybaby_block(position: f32, q: f32, range: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("position", "Position", position),
        BlockParameter::new("q", "Q", q),
        BlockParameter::new("range", "Range", range),
    ])
}

fn cry_baby() -> Preset {
    Preset::new(
        seed_id("wah-crybaby"),
        "Cry Baby",
        BlockType::Wah,
        // Default: classic mid-sweep with moderate Q
        Snapshot::new(
            seed_id("wah-crybaby-default"),
            "Default",
            crybaby_block(0.50, 0.50, 0.50),
        ),
        vec![
            // Cocked wah — parked in a fixed position for tonal coloring
            Snapshot::new(
                seed_id("wah-crybaby-cocked"),
                "Cocked Wah",
                crybaby_block(0.72, 0.60, 0.55),
            ),
            // Funk sweep — narrow Q, wide range for sharp quack
            Snapshot::new(
                seed_id("wah-crybaby-funk"),
                "Funk Sweep",
                crybaby_block(0.35, 0.80, 0.75),
            ),
        ],
    )
}

// ─── Vox Wah ────────────────────────────────────────────────────
// Knobs: Position, Tone, Volume

fn vox_block(position: f32, tone: f32, volume: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("position", "Position", position),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("volume", "Volume", volume),
    ])
}

fn vox_wah() -> Preset {
    Preset::new(
        seed_id("wah-vox"),
        "Vox Wah",
        BlockType::Wah,
        // Default: classic Vox sweep with balanced tone
        Snapshot::new(
            seed_id("wah-vox-default"),
            "Default",
            vox_block(0.50, 0.50, 0.50),
        ),
        vec![
            // Bright lead — treble-forward sweep for cutting solos
            Snapshot::new(
                seed_id("wah-vox-bright"),
                "Bright Lead",
                vox_block(0.65, 0.72, 0.55),
            ),
            // Dark rhythm — mellow sweep for background texture
            Snapshot::new(
                seed_id("wah-vox-dark"),
                "Dark Rhythm",
                vox_block(0.30, 0.28, 0.45),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wah_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_wah_presets_are_wah_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Wah);
        }
    }

    #[test]
    fn cry_baby_has_3_snapshots() {
        let cb = &presets()[0];
        assert_eq!(cb.name(), "Cry Baby");
        assert_eq!(cb.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn vox_wah_has_3_snapshots() {
        let vox = &presets()[1];
        assert_eq!(vox.name(), "Vox Wah");
        assert_eq!(vox.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn cry_baby_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Cry Baby should have 3 params"
            );
        }
    }

    #[test]
    fn vox_wah_has_3_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Vox Wah should have 3 params"
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
