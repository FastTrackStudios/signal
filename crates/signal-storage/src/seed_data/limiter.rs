//! Limiter block presets — modeled after studio-grade limiting plugins.
//!
//! Each preset corresponds to a real limiter with its actual parameter layout.
//! Parameter IDs match the plugin control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![realimit(), pro_l2()]
}

// ─── Cockos ReaLimit ────────────────────────────────────────────
// Knobs: Threshold, Ceiling, Release, Brickwall

fn realimit_block(threshold: f32, ceiling: f32, release: f32, brickwall: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("threshold", "Threshold", threshold),
        BlockParameter::new("ceiling", "Ceiling", ceiling),
        BlockParameter::new("release", "Release", release),
        BlockParameter::new("brickwall", "Brickwall", brickwall),
    ])
}

fn realimit() -> Preset {
    Preset::new(
        seed_id("limiter-realimit"),
        "ReaLimit",
        BlockType::Limiter,
        // Default: moderate limiting, ceiling near unity
        Snapshot::new(
            seed_id("limiter-realimit-default"),
            "Default",
            realimit_block(0.50, 0.95, 0.30, 0.85),
        ),
        vec![
            // Gentle — light limiting, high threshold lets most signal through
            Snapshot::new(
                seed_id("limiter-realimit-gentle"),
                "Gentle",
                realimit_block(0.75, 0.92, 0.40, 0.70),
            ),
            // Broadcast — heavy limiting, low threshold, fast release for consistency
            Snapshot::new(
                seed_id("limiter-realimit-broadcast"),
                "Broadcast",
                realimit_block(0.25, 0.90, 0.15, 0.95),
            ),
            // Mastering — moderate threshold, high ceiling, smooth release
            Snapshot::new(
                seed_id("limiter-realimit-mastering"),
                "Mastering",
                realimit_block(0.40, 0.98, 0.45, 0.80),
            ),
        ],
    )
}

// ─── FabFilter Pro-L 2 ──────────────────────────────────────────
// Knobs: Gain, Output, Attack, Release, Lookahead

fn prol2_block(gain: f32, output: f32, attack: f32, release: f32, lookahead: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("gain", "Gain", gain),
        BlockParameter::new("output", "Output", output),
        BlockParameter::new("attack", "Attack", attack),
        BlockParameter::new("release", "Release", release),
        BlockParameter::new("lookahead", "Lookahead", lookahead),
    ])
}

fn pro_l2() -> Preset {
    Preset::new(
        seed_id("limiter-prol2"),
        "Pro-L 2",
        BlockType::Limiter,
        // Default: balanced limiting, moderate gain, unity output
        Snapshot::new(
            seed_id("limiter-prol2-default"),
            "Default",
            prol2_block(0.35, 0.50, 0.40, 0.45, 0.50),
        ),
        vec![
            // Transparent — low gain, moderate lookahead for clean limiting
            Snapshot::new(
                seed_id("limiter-prol2-transparent"),
                "Transparent",
                prol2_block(0.20, 0.50, 0.35, 0.50, 0.65),
            ),
            // Loud — high gain, fast attack for maximum loudness
            Snapshot::new(
                seed_id("limiter-prol2-loud"),
                "Loud",
                prol2_block(0.80, 0.48, 0.75, 0.30, 0.40),
            ),
            // Bus Glue — moderate gain, slow attack preserves transients
            Snapshot::new(
                seed_id("limiter-prol2-bus-glue"),
                "Bus Glue",
                prol2_block(0.40, 0.50, 0.20, 0.55, 0.50),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_limiter_presets_are_limiter_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Limiter);
        }
    }

    #[test]
    fn realimit_has_4_snapshots() {
        let realimit = &presets()[0];
        assert_eq!(realimit.name(), "ReaLimit");
        assert_eq!(realimit.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn prol2_has_4_snapshots() {
        let prol2 = &presets()[1];
        assert_eq!(prol2.name(), "Pro-L 2");
        assert_eq!(prol2.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_knob_count() {
        let presets = presets();
        // ReaLimit: 4 knobs (threshold, ceiling, release, brickwall)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "ReaLimit should have 4 params"
            );
        }
        // Pro-L 2: 5 knobs (gain, output, attack, release, lookahead)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Pro-L 2 should have 5 params"
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
    fn broadcast_has_low_threshold() {
        let realimit = &presets()[0];
        let broadcast = realimit
            .snapshots()
            .iter()
            .find(|s| s.name() == "Broadcast")
            .unwrap();
        let block = broadcast.block();
        let params = block.parameters();
        let threshold = params.iter().find(|p| p.id() == "threshold").unwrap();
        assert!(threshold.value().get() <= 0.30);
    }

    #[test]
    fn loud_has_high_gain() {
        let prol2 = &presets()[1];
        let loud = prol2
            .snapshots()
            .iter()
            .find(|s| s.name() == "Loud")
            .unwrap();
        let block = loud.block();
        let params = block.parameters();
        let gain = params.iter().find(|p| p.id() == "gain").unwrap();
        assert!(gain.value().get() >= 0.75);
    }
}
