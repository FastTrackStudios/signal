//! Tremolo block presets — modeled after iconic tremolo effects.
//!
//! Each preset corresponds to a real effect with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![tremolo_classic(), harmonic_trem()]
}

// ─── Tremolo Classic ────────────────────────────────────────────
// Knobs: Speed, Depth, Wave
// Classic amp-style tremolo with waveform selection.

fn classic_block(speed: f32, depth: f32, wave: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("wave", "Wave", wave),
    ])
}

fn tremolo_classic() -> Preset {
    Preset::new(
        seed_id("tremolo-classic"),
        "Tremolo Classic",
        BlockType::Tremolo,
        // Default: moderate speed and depth, sine wave
        Snapshot::new(
            seed_id("tremolo-classic-default"),
            "Default",
            classic_block(0.45, 0.50, 0.25),
        ),
        vec![
            // Surf — fast, deep tremolo for that Dick Dale vibe
            Snapshot::new(
                seed_id("tremolo-classic-surf"),
                "Surf",
                classic_block(0.75, 0.80, 0.20),
            ),
            // Gentle pulse — slow, shallow for subtle rhythmic movement
            Snapshot::new(
                seed_id("tremolo-classic-gentle"),
                "Gentle Pulse",
                classic_block(0.22, 0.30, 0.30),
            ),
        ],
    )
}

// ─── Harmonic Tremolo ───────────────────────────────────────────
// Knobs: Speed, Depth, Mix
// Splits signal into high/low bands and alternates between them.

fn harmonic_block(speed: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn harmonic_trem() -> Preset {
    Preset::new(
        seed_id("tremolo-harmonic"),
        "Harmonic Trem",
        BlockType::Tremolo,
        // Default: moderate harmonic panning effect
        Snapshot::new(
            seed_id("tremolo-harmonic-default"),
            "Default",
            harmonic_block(0.40, 0.50, 0.50),
        ),
        vec![
            // Lush — slow speed, deep mix for rich movement
            Snapshot::new(
                seed_id("tremolo-harmonic-lush"),
                "Lush",
                harmonic_block(0.20, 0.65, 0.70),
            ),
            // Choppy — fast, high depth for stutter-like rhythmic effect
            Snapshot::new(
                seed_id("tremolo-harmonic-choppy"),
                "Choppy",
                harmonic_block(0.78, 0.82, 0.55),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tremolo_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_tremolo_presets_are_tremolo_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Tremolo);
        }
    }

    #[test]
    fn tremolo_classic_has_3_snapshots() {
        let tc = &presets()[0];
        assert_eq!(tc.name(), "Tremolo Classic");
        assert_eq!(tc.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn harmonic_trem_has_3_snapshots() {
        let ht = &presets()[1];
        assert_eq!(ht.name(), "Harmonic Trem");
        assert_eq!(ht.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn tremolo_classic_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Tremolo Classic should have 3 params"
            );
        }
    }

    #[test]
    fn harmonic_trem_has_3_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Harmonic Trem should have 3 params"
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
