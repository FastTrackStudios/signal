//! De-esser block presets — modeled after studio de-esser plugins.
//!
//! Each preset corresponds to a real plugin with its actual parameter layout.
//! Parameter IDs match the control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![t_de_esser_2(), pro_ds()]
}

// ─── Techivation T-De-Esser 2 ───────────────────────────────────
// Knobs: Threshold, Frequency, Range, Mix
// Surgical de-essing with frequency-targeted processing and
// parallel mix control for transparent sibilance taming.

fn tde2_block(threshold: f32, frequency: f32, range: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("threshold", "Threshold", threshold),
        BlockParameter::new("frequency", "Frequency", frequency),
        BlockParameter::new("range", "Range", range),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn t_de_esser_2() -> Preset {
    Preset::new(
        seed_id("deesser-tde2"),
        "T-De-Esser 2",
        BlockType::DeEsser,
        // Default: moderate de-essing centered around 6–7 kHz
        Snapshot::new(
            seed_id("deesser-tde2-default"),
            "Default",
            tde2_block(0.50, 0.60, 0.45, 1.00),
        ),
        vec![
            // Gentle: high threshold (less reduction), narrow range — barely touches
            Snapshot::new(
                seed_id("deesser-tde2-gentle"),
                "Gentle",
                tde2_block(0.72, 0.60, 0.25, 1.00),
            ),
            // Aggressive: low threshold (catches more), wide range — heavy taming
            Snapshot::new(
                seed_id("deesser-tde2-aggressive"),
                "Aggressive",
                tde2_block(0.25, 0.58, 0.80, 1.00),
            ),
            // Parallel: moderate threshold with lower mix for parallel processing blend
            Snapshot::new(
                seed_id("deesser-tde2-parallel"),
                "Parallel",
                tde2_block(0.40, 0.62, 0.50, 0.55),
            ),
        ],
    )
}

// ─── FabFilter Pro-DS ───────────────────────────────────────────
// Knobs: Threshold, Range, Frequency, Lookahead
// Intelligent de-esser with linear-phase lookahead for
// transparent, artifact-free sibilance reduction.

fn prods_block(threshold: f32, range: f32, frequency: f32, lookahead: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("threshold", "Threshold", threshold),
        BlockParameter::new("range", "Range", range),
        BlockParameter::new("frequency", "Frequency", frequency),
        BlockParameter::new("lookahead", "Lookahead", lookahead),
    ])
}

fn pro_ds() -> Preset {
    Preset::new(
        seed_id("deesser-prods"),
        "Pro-DS",
        BlockType::DeEsser,
        // Default: balanced de-essing with moderate lookahead
        Snapshot::new(
            seed_id("deesser-prods-default"),
            "Default",
            prods_block(0.50, 0.45, 0.60, 0.50),
        ),
        vec![
            // Vocal: tuned for vocal sibilance ~6 kHz, moderate threshold
            Snapshot::new(
                seed_id("deesser-prods-vocal"),
                "Vocal",
                prods_block(0.48, 0.40, 0.58, 0.55),
            ),
            // Bright Source: lower frequency target for bright recordings
            Snapshot::new(
                seed_id("deesser-prods-bright"),
                "Bright Source",
                prods_block(0.45, 0.50, 0.42, 0.60),
            ),
            // Heavy: low threshold, wide range — aggressive sibilance control
            Snapshot::new(
                seed_id("deesser-prods-heavy"),
                "Heavy",
                prods_block(0.22, 0.78, 0.55, 0.65),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_esser_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_de_esser_presets_are_de_esser_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::DeEsser);
        }
    }

    #[test]
    fn tde2_has_4_snapshots() {
        let tde2 = &presets()[0];
        assert_eq!(tde2.name(), "T-De-Esser 2");
        assert_eq!(tde2.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn prods_has_4_snapshots() {
        let prods = &presets()[1];
        assert_eq!(prods.name(), "Pro-DS");
        assert_eq!(prods.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_plugin_knobs() {
        let presets = presets();
        // T-De-Esser 2: 4 knobs (threshold, frequency, range, mix)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "T-De-Esser 2 should have 4 params"
            );
        }
        // Pro-DS: 4 knobs (threshold, range, frequency, lookahead)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "Pro-DS should have 4 params"
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

    #[test]
    fn tde2_aggressive_has_low_threshold() {
        let tde2 = &presets()[0];
        let aggressive = tde2
            .snapshots()
            .iter()
            .find(|s| s.name() == "Aggressive")
            .unwrap();
        let block = aggressive.block();
        let params = block.parameters();
        let threshold = params.iter().find(|p| p.id() == "threshold").unwrap();
        assert!(threshold.value().get() < 0.30);
    }

    #[test]
    fn prods_heavy_has_wide_range() {
        let prods = &presets()[1];
        let heavy = prods
            .snapshots()
            .iter()
            .find(|s| s.name() == "Heavy")
            .unwrap();
        let block = heavy.block();
        let params = block.parameters();
        let range = params.iter().find(|p| p.id() == "range").unwrap();
        assert!(range.value().get() > 0.70);
    }
}
