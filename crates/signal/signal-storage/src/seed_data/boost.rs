//! Boost block presets — modeled after iconic clean boost pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![ep_booster(), mxr_micro_amp()]
}

// ─── Xotic EP Booster ───────────────────────────────────────────
// Knobs: Boost, Tone

fn ep_block(boost: f32, tone: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("boost", "Boost", boost),
        BlockParameter::new("tone", "Tone", tone),
    ])
}

fn ep_booster() -> Preset {
    Preset::new(
        seed_id("boost-ep"),
        "EP Booster",
        BlockType::Boost,
        // Default: moderate boost with neutral tone
        Snapshot::new(seed_id("boost-ep-default"), "Default", ep_block(0.50, 0.50)),
        vec![
            // Full boost — cranked for maximum clean volume lift
            Snapshot::new(seed_id("boost-ep-full"), "Full Boost", ep_block(0.85, 0.55)),
            // Bright sparkle — lighter boost with treble emphasis
            Snapshot::new(
                seed_id("boost-ep-bright"),
                "Bright Sparkle",
                ep_block(0.40, 0.75),
            ),
        ],
    )
}

// ─── MXR Micro Amp ──────────────────────────────────────────────
// Knobs: Gain

fn micro_block(gain: f32) -> Block {
    Block::from_parameters(vec![BlockParameter::new("gain", "Gain", gain)])
}

fn mxr_micro_amp() -> Preset {
    Preset::new(
        seed_id("boost-micro"),
        "MXR Micro Amp",
        BlockType::Boost,
        // Default: moderate clean gain boost
        Snapshot::new(seed_id("boost-micro-default"), "Default", micro_block(0.50)),
        vec![
            // Subtle lift — just enough to push the front of an amp
            Snapshot::new(
                seed_id("boost-micro-subtle"),
                "Subtle Lift",
                micro_block(0.30),
            ),
            // Hot signal — cranked for maximum headroom push
            Snapshot::new(seed_id("boost-micro-hot"), "Hot Signal", micro_block(0.80)),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_boost_presets_are_boost_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Boost);
        }
    }

    #[test]
    fn ep_booster_has_3_snapshots() {
        let ep = &presets()[0];
        assert_eq!(ep.name(), "EP Booster");
        assert_eq!(ep.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn micro_amp_has_3_snapshots() {
        let micro = &presets()[1];
        assert_eq!(micro.name(), "MXR Micro Amp");
        assert_eq!(micro.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn ep_booster_has_2_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                2,
                "EP Booster should have 2 params"
            );
        }
    }

    #[test]
    fn micro_amp_has_1_param() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                1,
                "MXR Micro Amp should have 1 param"
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
