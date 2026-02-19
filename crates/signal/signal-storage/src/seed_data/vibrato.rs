//! Vibrato block presets — modeled after iconic vibrato pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![uni_vibe(), boss_vb2()]
}

// ─── Shin-ei Uni-Vibe ───────────────────────────────────────────
// Knobs: Speed, Depth, Mix
// The classic photocell-based vibrato/chorus made famous by Hendrix.

fn univibe_block(speed: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn uni_vibe() -> Preset {
    Preset::new(
        seed_id("vibrato-univibe"),
        "Uni-Vibe",
        BlockType::Vibrato,
        // Default: classic Hendrix-era throb
        Snapshot::new(
            seed_id("vibrato-univibe-default"),
            "Default",
            univibe_block(0.45, 0.50, 0.55),
        ),
        vec![
            // Slow throb — dreamy, psychedelic slow pulse
            Snapshot::new(
                seed_id("vibrato-univibe-slow"),
                "Slow Throb",
                univibe_block(0.18, 0.60, 0.65),
            ),
            // Fast warble — rapid modulation for intense effect
            Snapshot::new(
                seed_id("vibrato-univibe-fast"),
                "Fast Warble",
                univibe_block(0.78, 0.55, 0.50),
            ),
        ],
    )
}

// ─── Boss VB-2 ──────────────────────────────────────────────────
// Knobs: Rate, Depth, Rise

fn vb2_block(rate: f32, depth: f32, rise: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("rise", "Rise", rise),
    ])
}

fn boss_vb2() -> Preset {
    Preset::new(
        seed_id("vibrato-vb2"),
        "Boss VB-2",
        BlockType::Vibrato,
        // Default: moderate pitch vibrato with instant onset
        Snapshot::new(
            seed_id("vibrato-vb2-default"),
            "Default",
            vb2_block(0.45, 0.40, 0.30),
        ),
        vec![
            // Subtle shimmer — gentle pitch wobble for ambient textures
            Snapshot::new(
                seed_id("vibrato-vb2-shimmer"),
                "Subtle Shimmer",
                vb2_block(0.30, 0.20, 0.50),
            ),
            // Seasick — extreme depth, slow rate for dramatic pitch warping
            Snapshot::new(
                seed_id("vibrato-vb2-seasick"),
                "Seasick",
                vb2_block(0.20, 0.85, 0.15),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vibrato_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_vibrato_presets_are_vibrato_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Vibrato);
        }
    }

    #[test]
    fn uni_vibe_has_3_snapshots() {
        let uv = &presets()[0];
        assert_eq!(uv.name(), "Uni-Vibe");
        assert_eq!(uv.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn vb2_has_3_snapshots() {
        let vb = &presets()[1];
        assert_eq!(vb.name(), "Boss VB-2");
        assert_eq!(vb.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn uni_vibe_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Uni-Vibe should have 3 params"
            );
        }
    }

    #[test]
    fn vb2_has_3_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Boss VB-2 should have 3 params"
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
