//! Pitch block presets — modeled after iconic pitch-shifting pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![whammy(), pog2()]
}

// ─── Digitech Whammy ────────────────────────────────────────────
// Knobs: Shift, Mix, Tone

fn whammy_block(shift: f32, mix: f32, tone: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("shift", "Shift", shift),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("tone", "Tone", tone),
    ])
}

fn whammy() -> Preset {
    Preset::new(
        seed_id("pitch-whammy"),
        "Whammy",
        BlockType::Pitch,
        // Default: octave up, moderate mix
        Snapshot::new(
            seed_id("pitch-whammy-default"),
            "Default",
            whammy_block(0.50, 0.50, 0.50),
        ),
        vec![
            // Harmony — subtle pitch offset blended with dry for interval harmony
            Snapshot::new(
                seed_id("pitch-whammy-harmony"),
                "Harmony",
                whammy_block(0.30, 0.40, 0.55),
            ),
            // Dive bomb — full wet, extreme shift for dramatic drops
            Snapshot::new(
                seed_id("pitch-whammy-dive"),
                "Dive Bomb",
                whammy_block(0.0, 0.90, 0.45),
            ),
        ],
    )
}

// ─── Electro-Harmonix POG 2 ────────────────────────────────────
// Knobs: SubOctave, DryLevel, Octave, Attack

fn pog2_block(sub_octave: f32, dry_level: f32, octave: f32, attack: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("sub_octave", "SubOctave", sub_octave),
        BlockParameter::new("dry_level", "DryLevel", dry_level),
        BlockParameter::new("octave", "Octave", octave),
        BlockParameter::new("attack", "Attack", attack),
    ])
}

fn pog2() -> Preset {
    Preset::new(
        seed_id("pitch-pog2"),
        "POG 2",
        BlockType::Pitch,
        // Default: balanced organ-like blend of all three voices
        Snapshot::new(
            seed_id("pitch-pog2-default"),
            "Default",
            pog2_block(0.40, 0.50, 0.40, 0.50),
        ),
        vec![
            // Sub bass — heavy sub-octave for thick low end
            Snapshot::new(
                seed_id("pitch-pog2-sub-bass"),
                "Sub Bass",
                pog2_block(0.80, 0.30, 0.10, 0.40),
            ),
            // Shimmer — octave up with slow attack for pad-like textures
            Snapshot::new(
                seed_id("pitch-pog2-shimmer"),
                "Shimmer",
                pog2_block(0.10, 0.35, 0.75, 0.80),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_pitch_presets_are_pitch_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Pitch);
        }
    }

    #[test]
    fn whammy_has_3_snapshots() {
        let w = &presets()[0];
        assert_eq!(w.name(), "Whammy");
        assert_eq!(w.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn pog2_has_3_snapshots() {
        let p = &presets()[1];
        assert_eq!(p.name(), "POG 2");
        assert_eq!(p.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn whammy_has_3_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Whammy should have 3 params"
            );
        }
    }

    #[test]
    fn pog2_has_4_params() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "POG 2 should have 4 params"
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
