//! Gate block presets — modeled after popular noise gate plugins.
//!
//! Each preset corresponds to a real gate plugin with its actual parameter
//! layout. Parameter IDs match the control names. Values are normalized
//! 0.0–1.0.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![reagate(), pro_g()]
}

// ─── Cockos ReaGate ─────────────────────────────────────────────
// Knobs: Threshold, Attack, Hold, Release, Hysteresis

fn reagate_block(threshold: f32, attack: f32, hold: f32, release: f32, hysteresis: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("threshold", "Threshold", threshold),
        BlockParameter::new("attack", "Attack", attack),
        BlockParameter::new("hold", "Hold", hold),
        BlockParameter::new("release", "Release", release),
        BlockParameter::new("hysteresis", "Hysteresis", hysteresis),
    ])
}

fn reagate() -> Preset {
    Preset::new(
        seed_id("gate-reagate"),
        "ReaGate",
        BlockType::Gate,
        // Default: moderate threshold, balanced timing
        Snapshot::new(
            seed_id("gate-reagate-default"),
            "Default",
            reagate_block(0.35, 0.10, 0.30, 0.40, 0.20),
        ),
        vec![
            // Tight — fast attack/release, short hold for drums
            Snapshot::new(
                seed_id("gate-reagate-tight"),
                "Tight",
                reagate_block(0.45, 0.05, 0.10, 0.15, 0.25),
            ),
            // Gentle — slow attack, long hold for vocals
            Snapshot::new(
                seed_id("gate-reagate-gentle"),
                "Gentle",
                reagate_block(0.30, 0.35, 0.60, 0.55, 0.15),
            ),
            // Noise Floor — very low threshold, fast response
            Snapshot::new(
                seed_id("gate-reagate-noise-floor"),
                "Noise Floor",
                reagate_block(0.15, 0.05, 0.20, 0.25, 0.10),
            ),
        ],
    )
}

// ─── FabFilter Pro-G ────────────────────────────────────────────
// Knobs: Threshold, Range, Attack, Hold, Release, Lookahead

fn prog_block(
    threshold: f32,
    range: f32,
    attack: f32,
    hold: f32,
    release: f32,
    lookahead: f32,
) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("threshold", "Threshold", threshold),
        BlockParameter::new("range", "Range", range),
        BlockParameter::new("attack", "Attack", attack),
        BlockParameter::new("hold", "Hold", hold),
        BlockParameter::new("release", "Release", release),
        BlockParameter::new("lookahead", "Lookahead", lookahead),
    ])
}

fn pro_g() -> Preset {
    Preset::new(
        seed_id("gate-prog"),
        "Pro-G",
        BlockType::Gate,
        // Default: balanced gating with moderate lookahead
        Snapshot::new(
            seed_id("gate-prog-default"),
            "Default",
            prog_block(0.40, 0.80, 0.15, 0.25, 0.35, 0.20),
        ),
        vec![
            // Drum Gate — fast attack, short hold, full range
            Snapshot::new(
                seed_id("gate-prog-drum"),
                "Drum Gate",
                prog_block(0.50, 1.00, 0.05, 0.10, 0.20, 0.30),
            ),
            // Sidechain — moderate threshold, long hold
            Snapshot::new(
                seed_id("gate-prog-sidechain"),
                "Sidechain",
                prog_block(0.35, 0.85, 0.10, 0.55, 0.40, 0.15),
            ),
            // Expander — high threshold, reduced range for gentle expansion
            Snapshot::new(
                seed_id("gate-prog-expander"),
                "Expander",
                prog_block(0.55, 0.45, 0.20, 0.30, 0.50, 0.10),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_gate_presets_are_gate_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Gate);
        }
    }

    #[test]
    fn reagate_has_4_snapshots() {
        let reagate = &presets()[0];
        assert_eq!(reagate.name(), "ReaGate");
        assert_eq!(reagate.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn pro_g_has_4_snapshots() {
        let prog = &presets()[1];
        assert_eq!(prog.name(), "Pro-G");
        assert_eq!(prog.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn reagate_has_5_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "ReaGate should have 5 params"
            );
        }
    }

    #[test]
    fn pro_g_has_6_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "Pro-G should have 6 params"
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
    fn reagate_tight_has_fast_attack() {
        let reagate = &presets()[0];
        let tight = reagate
            .snapshots()
            .iter()
            .find(|s| s.name() == "Tight")
            .unwrap();
        let block = tight.block();
        let params = block.parameters();
        let attack = params.iter().find(|p| p.id() == "attack").unwrap();
        assert!(
            attack.value().get() <= 0.10,
            "Tight snapshot should have fast (low) attack"
        );
    }

    #[test]
    fn pro_g_expander_has_reduced_range() {
        let prog = &presets()[1];
        let expander = prog
            .snapshots()
            .iter()
            .find(|s| s.name() == "Expander")
            .unwrap();
        let block = expander.block();
        let params = block.parameters();
        let range = params.iter().find(|p| p.id() == "range").unwrap();
        assert!(
            range.value().get() < 0.50,
            "Expander snapshot should have reduced range for gentle expansion"
        );
    }
}
