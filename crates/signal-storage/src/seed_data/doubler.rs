//! Doubler block presets — modeled after ADT and chorus-based doubling effects.
//!
//! Each preset corresponds to a real effect with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![adt_classic(), chorus_doubler()]
}

// ─── ADT Classic ────────────────────────────────────────────────
// Knobs: Delay, Depth, Mix
// Automatic Double Tracking — short modulated delay for vocal/guitar doubling.

fn adt_block(delay: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("delay", "Delay", delay),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn adt_classic() -> Preset {
    Preset::new(
        seed_id("doubler-adt"),
        "ADT Classic",
        BlockType::Doubler,
        // Default: subtle tape-style doubling
        Snapshot::new(
            seed_id("doubler-adt-default"),
            "Default",
            adt_block(0.35, 0.30, 0.50),
        ),
        vec![
            // Tight double — very short delay, minimal modulation
            Snapshot::new(
                seed_id("doubler-adt-tight"),
                "Tight Double",
                adt_block(0.15, 0.15, 0.55),
            ),
            // Wide spread — longer delay with more depth for spacious feel
            Snapshot::new(
                seed_id("doubler-adt-wide"),
                "Wide Spread",
                adt_block(0.60, 0.55, 0.65),
            ),
        ],
    )
}

// ─── Chorus Doubler ─────────────────────────────────────────────
// Knobs: Rate, Depth, Mix
// Chorus-based doubling — uses pitch modulation to simulate a second take.

fn chorus_dbl_block(rate: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn chorus_doubler() -> Preset {
    Preset::new(
        seed_id("doubler-chorus"),
        "Chorus Doubler",
        BlockType::Doubler,
        // Default: moderate chorus-style thickening
        Snapshot::new(
            seed_id("doubler-chorus-default"),
            "Default",
            chorus_dbl_block(0.40, 0.40, 0.50),
        ),
        vec![
            // Subtle shimmer — slow rate, light depth for gentle movement
            Snapshot::new(
                seed_id("doubler-chorus-shimmer"),
                "Subtle Shimmer",
                chorus_dbl_block(0.20, 0.25, 0.45),
            ),
            // Thick stack — fast modulation, deep mix for wall-of-sound
            Snapshot::new(
                seed_id("doubler-chorus-thick"),
                "Thick Stack",
                chorus_dbl_block(0.65, 0.70, 0.72),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubler_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_doubler_presets_are_doubler_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Doubler);
        }
    }

    #[test]
    fn adt_has_3_snapshots() {
        let adt = &presets()[0];
        assert_eq!(adt.name(), "ADT Classic");
        assert_eq!(adt.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn chorus_doubler_has_3_snapshots() {
        let cd = &presets()[1];
        assert_eq!(cd.name(), "Chorus Doubler");
        assert_eq!(cd.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn adt_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "ADT Classic should have 3 params"
            );
        }
    }

    #[test]
    fn chorus_doubler_has_3_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Chorus Doubler should have 3 params"
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
