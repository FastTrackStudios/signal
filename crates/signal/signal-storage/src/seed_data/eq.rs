//! EQ block presets — modeled after iconic equalizer plugins.
//!
//! Each preset corresponds to a real EQ with its band controls.
//! Parameter IDs represent frequency bands. Values are normalized
//! 0.0–1.0 where 0.5 = flat / 0 dB gain.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![reaeq(), pro_q4()]
}

// ─── Cockos ReaEQ ───────────────────────────────────────────────
// Bands: Low, LowMid, Mid, HighMid, High (5-band parametric EQ)
// The stock REAPER EQ — lightweight, zero-latency, unlimited bands.
// Here modeled as a 5-band gain structure where 0.5 = 0 dB (flat).

fn reaeq_block(low: f32, low_mid: f32, mid: f32, high_mid: f32, high: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("low", "Low", low),
        BlockParameter::new("low_mid", "LowMid", low_mid),
        BlockParameter::new("mid", "Mid", mid),
        BlockParameter::new("high_mid", "HighMid", high_mid),
        BlockParameter::new("high", "High", high),
    ])
}

fn reaeq() -> Preset {
    Preset::new(
        seed_id("eq-reaeq"),
        "ReaEQ",
        BlockType::Eq,
        // Default: all bands flat at 0 dB
        Snapshot::new(
            seed_id("eq-reaeq-default"),
            "Default",
            reaeq_block(0.50, 0.50, 0.50, 0.50, 0.50),
        ),
        vec![
            // Vocal Presence: boost high-mid for articulation, cut low to reduce mud
            Snapshot::new(
                seed_id("eq-reaeq-vocal-presence"),
                "Vocal Presence",
                reaeq_block(0.35, 0.48, 0.52, 0.68, 0.58),
            ),
            // Guitar Scoop: classic V-curve — boost lows and highs, cut mids
            Snapshot::new(
                seed_id("eq-reaeq-guitar-scoop"),
                "Guitar Scoop",
                reaeq_block(0.62, 0.45, 0.32, 0.47, 0.65),
            ),
            // Bass Warmth: boost lows for body, cut high-mid to reduce harshness
            Snapshot::new(
                seed_id("eq-reaeq-bass-warmth"),
                "Bass Warmth",
                reaeq_block(0.68, 0.55, 0.50, 0.38, 0.45),
            ),
        ],
    )
}

// ─── FabFilter Pro-Q 4 ─────────────────────────────────────────
// Bands: Low, LowMid, Mid, HighMid, High, Output (6 controls)
// The industry-standard surgical EQ — dynamic bands, linear phase,
// spectrum analyzer. Output gain at the end of the chain.

fn proq4_block(low: f32, low_mid: f32, mid: f32, high_mid: f32, high: f32, output: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("low", "Low", low),
        BlockParameter::new("low_mid", "LowMid", low_mid),
        BlockParameter::new("mid", "Mid", mid),
        BlockParameter::new("high_mid", "HighMid", high_mid),
        BlockParameter::new("high", "High", high),
        BlockParameter::new("output", "Output", output),
    ])
}

fn pro_q4() -> Preset {
    Preset::new(
        seed_id("eq-proq4"),
        "Pro-Q 4",
        BlockType::Eq,
        // Default: all bands flat, unity output
        Snapshot::new(
            seed_id("eq-proq4-default"),
            "Default",
            proq4_block(0.50, 0.50, 0.50, 0.50, 0.50, 0.50),
        ),
        vec![
            // Surgical Cut: narrow mid-band cut for resonance removal, everything else flat
            Snapshot::new(
                seed_id("eq-proq4-surgical-cut"),
                "Surgical Cut",
                proq4_block(0.50, 0.50, 0.28, 0.50, 0.50, 0.50),
            ),
            // Hi-Fi: slight low-end warmth, airy high boost, mid untouched
            Snapshot::new(
                seed_id("eq-proq4-hifi"),
                "Hi-Fi",
                proq4_block(0.58, 0.50, 0.50, 0.52, 0.64, 0.50),
            ),
            // Warm Analog: low-end boost for body, gentle high roll-off
            Snapshot::new(
                seed_id("eq-proq4-warm-analog"),
                "Warm Analog",
                proq4_block(0.65, 0.55, 0.50, 0.48, 0.38, 0.50),
            ),
            // Bright Mix: high-mid and high boost for presence and air
            Snapshot::new(
                seed_id("eq-proq4-bright-mix"),
                "Bright Mix",
                proq4_block(0.50, 0.50, 0.50, 0.62, 0.68, 0.52),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_eq_presets_are_eq_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Eq);
        }
    }

    #[test]
    fn reaeq_has_4_snapshots() {
        let reaeq = &presets()[0];
        assert_eq!(reaeq.name(), "ReaEQ");
        assert_eq!(reaeq.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn proq4_has_5_snapshots() {
        let proq4 = &presets()[1];
        assert_eq!(proq4.name(), "Pro-Q 4");
        assert_eq!(proq4.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn reaeq_parameter_count() {
        let reaeq = &presets()[0];
        for snap in reaeq.snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "ReaEQ should have 5 params"
            );
        }
    }

    #[test]
    fn proq4_parameter_count() {
        let proq4 = &presets()[1];
        for snap in proq4.snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "Pro-Q 4 should have 6 params"
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
    fn reaeq_default_is_flat() {
        let reaeq = &presets()[0];
        let default = reaeq
            .snapshots()
            .iter()
            .find(|s| s.name() == "Default")
            .unwrap();
        let block = default.block();
        for param in block.parameters() {
            assert!(
                (param.value().get() - 0.50).abs() < 0.001,
                "ReaEQ default param '{}' should be 0.50 (flat) but was {}",
                param.id(),
                param.value().get(),
            );
        }
    }

    #[test]
    fn proq4_surgical_cut_has_low_mid() {
        let proq4 = &presets()[1];
        let surgical = proq4
            .snapshots()
            .iter()
            .find(|s| s.name() == "Surgical Cut")
            .unwrap();
        let block = surgical.block();
        let params = block.parameters();
        let mid = params.iter().find(|p| p.id() == "mid").unwrap();
        assert!(
            mid.value().get() < 0.35,
            "Surgical Cut mid should be a narrow cut (< 0.35) but was {}",
            mid.value().get(),
        );
    }
}
