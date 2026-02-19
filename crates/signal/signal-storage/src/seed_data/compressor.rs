//! Compressor block presets — modeled after iconic compression pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![
        keeley_compressor_plus(),
        wampler_ego(),
        mxr_dyna_comp(),
        boss_cp1x(),
        xotic_sp(),
    ]
}

// ─── Keeley Compressor Plus ─────────────────────────────────────
// Knobs: Sustain, Level, Blend, Tone

fn keeley_block(sustain: f32, level: f32, blend: f32, tone: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("sustain", "Sustain", sustain),
        BlockParameter::new("level", "Level", level),
        BlockParameter::new("blend", "Blend", blend),
        BlockParameter::new("tone", "Tone", tone),
    ])
}

fn keeley_compressor_plus() -> Preset {
    Preset::new(
        seed_id("comp-keeley"),
        "Keeley Compressor Plus",
        BlockType::Compressor,
        // Default: smooth studio compression, blend around noon
        Snapshot::new(
            seed_id("comp-keeley-default"),
            "Default",
            keeley_block(0.50, 0.55, 0.50, 0.50),
        ),
        vec![
            // Subtle compression — nearly transparent, just evens out dynamics
            Snapshot::new(
                seed_id("comp-keeley-transparent"),
                "Transparent",
                keeley_block(0.25, 0.60, 0.35, 0.52),
            ),
            // Heavy squeeze for country chicken-picking
            Snapshot::new(
                seed_id("comp-keeley-squeeze"),
                "Squeeze",
                keeley_block(0.80, 0.45, 0.70, 0.55),
            ),
            // Always-on sustainer — moderate compression, full blend
            Snapshot::new(
                seed_id("comp-keeley-sustainer"),
                "Sustainer",
                keeley_block(0.65, 0.50, 0.85, 0.48),
            ),
        ],
    )
}

// ─── Wampler Ego Compressor ─────────────────────────────────────
// Knobs: Sustain, Attack, Tone, Volume, Blend

fn ego_block(sustain: f32, attack: f32, tone: f32, volume: f32, blend: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("sustain", "Sustain", sustain),
        BlockParameter::new("attack", "Attack", attack),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("volume", "Volume", volume),
        BlockParameter::new("blend", "Blend", blend),
    ])
}

fn wampler_ego() -> Preset {
    Preset::new(
        seed_id("comp-ego"),
        "Wampler Ego Compressor",
        BlockType::Compressor,
        // Default: balanced studio compression
        Snapshot::new(
            seed_id("comp-ego-default"),
            "Default",
            ego_block(0.50, 0.50, 0.50, 0.50, 0.50),
        ),
        vec![
            // Fast attack, high blend — percussive funk rhythm
            Snapshot::new(
                seed_id("comp-ego-funk"),
                "Funk Snap",
                ego_block(0.60, 0.75, 0.55, 0.52, 0.70),
            ),
            // Slow attack preserves pick transient, subtle blend
            Snapshot::new(
                seed_id("comp-ego-pick"),
                "Pick Attack",
                ego_block(0.45, 0.25, 0.50, 0.55, 0.40),
            ),
            // High sustain for singing lead lines
            Snapshot::new(
                seed_id("comp-ego-sustain"),
                "Singing Sustain",
                ego_block(0.78, 0.40, 0.58, 0.48, 0.65),
            ),
            // Gentle glue — low sustain, parallel blend for natural feel
            Snapshot::new(
                seed_id("comp-ego-glue"),
                "Gentle Glue",
                ego_block(0.30, 0.50, 0.48, 0.55, 0.30),
            ),
        ],
    )
}

// ─── MXR Dyna Comp ──────────────────────────────────────────────
// Knobs: Output, Sensitivity
// The classic two-knob compressor — simple but iconic.

fn dyna_block(output: f32, sensitivity: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("output", "Output", output),
        BlockParameter::new("sensitivity", "Sensitivity", sensitivity),
    ])
}

fn mxr_dyna_comp() -> Preset {
    Preset::new(
        seed_id("comp-dyna"),
        "MXR Dyna Comp",
        BlockType::Compressor,
        // Default: classic country/pop compression
        Snapshot::new(
            seed_id("comp-dyna-default"),
            "Default",
            dyna_block(0.55, 0.50),
        ),
        vec![
            // Low sensitivity — subtle sustain enhancement
            Snapshot::new(
                seed_id("comp-dyna-subtle"),
                "Subtle",
                dyna_block(0.60, 0.30),
            ),
            // Full squeeze — Mark Knopfler-style heavy compression
            Snapshot::new(
                seed_id("comp-dyna-full-squeeze"),
                "Full Squeeze",
                dyna_block(0.45, 0.85),
            ),
            // Country twang — mid sensitivity, boosted output
            Snapshot::new(
                seed_id("comp-dyna-country"),
                "Country Twang",
                dyna_block(0.70, 0.65),
            ),
        ],
    )
}

// ─── Boss CP-1X ─────────────────────────────────────────────────
// Knobs: Level, Attack, Ratio, Compression

fn cp1x_block(level: f32, attack: f32, ratio: f32, compression: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("level", "Level", level),
        BlockParameter::new("attack", "Attack", attack),
        BlockParameter::new("ratio", "Ratio", ratio),
        BlockParameter::new("compression", "Compression", compression),
    ])
}

fn boss_cp1x() -> Preset {
    Preset::new(
        seed_id("comp-cp1x"),
        "Boss CP-1X",
        BlockType::Compressor,
        // Default: natural multi-band compression
        Snapshot::new(
            seed_id("comp-cp1x-default"),
            "Default",
            cp1x_block(0.50, 0.50, 0.45, 0.50),
        ),
        vec![
            // Light touch — low ratio, gentle compression for acoustic
            Snapshot::new(
                seed_id("comp-cp1x-acoustic"),
                "Acoustic",
                cp1x_block(0.55, 0.40, 0.30, 0.35),
            ),
            // Aggressive — high ratio, fast attack for tight rhythm
            Snapshot::new(
                seed_id("comp-cp1x-tight"),
                "Tight Rhythm",
                cp1x_block(0.48, 0.70, 0.75, 0.65),
            ),
            // Studio polish — moderate everything, unity level
            Snapshot::new(
                seed_id("comp-cp1x-studio"),
                "Studio Polish",
                cp1x_block(0.50, 0.45, 0.50, 0.55),
            ),
        ],
    )
}

// ─── Xotic SP Compressor ────────────────────────────────────────
// Knobs: Volume, Blend
// Internal DIP switches set attack/release, but the external controls
// are just Volume and Blend. The 3-way toggle (Lo/Mid/Hi) is modeled
// as a normalized "compression" parameter.

fn sp_block(volume: f32, blend: f32, compression: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("volume", "Volume", volume),
        BlockParameter::new("blend", "Blend", blend),
        BlockParameter::new("compression", "Compression", compression),
    ])
}

fn xotic_sp() -> Preset {
    Preset::new(
        seed_id("comp-sp"),
        "Xotic SP Compressor",
        BlockType::Compressor,
        // Default: mid compression, parallel blend, unity volume
        Snapshot::new(
            seed_id("comp-sp-default"),
            "Default",
            sp_block(0.50, 0.50, 0.50),
        ),
        vec![
            // Always-on transparent — low compression, subtle blend
            Snapshot::new(
                seed_id("comp-sp-always-on"),
                "Always On",
                sp_block(0.55, 0.30, 0.33),
            ),
            // High compression, full blend — heavy squash
            Snapshot::new(
                seed_id("comp-sp-squash"),
                "Squash",
                sp_block(0.42, 0.80, 1.00),
            ),
            // Boost — volume up, low compression for a clean volume lift
            Snapshot::new(
                seed_id("comp-sp-boost"),
                "Clean Boost",
                sp_block(0.75, 0.25, 0.33),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressor_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn all_compressor_presets_are_compressor_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Compressor);
        }
    }

    #[test]
    fn keeley_has_4_snapshots() {
        let keeley = &presets()[0];
        assert_eq!(keeley.name(), "Keeley Compressor Plus");
        assert_eq!(keeley.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn ego_has_5_snapshots() {
        let ego = &presets()[1];
        assert_eq!(ego.name(), "Wampler Ego Compressor");
        assert_eq!(ego.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn dyna_comp_has_4_snapshots() {
        let dyna = &presets()[2];
        assert_eq!(dyna.name(), "MXR Dyna Comp");
        assert_eq!(dyna.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn cp1x_has_4_snapshots() {
        let cp1x = &presets()[3];
        assert_eq!(cp1x.name(), "Boss CP-1X");
        assert_eq!(cp1x.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn sp_has_4_snapshots() {
        let sp = &presets()[4];
        assert_eq!(sp.name(), "Xotic SP Compressor");
        assert_eq!(sp.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_pedal_knobs() {
        let presets = presets();
        // Keeley: 4 knobs (sustain, level, blend, tone)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "Keeley should have 4 params"
            );
        }
        // Ego: 5 knobs (sustain, attack, tone, volume, blend)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Ego should have 5 params"
            );
        }
        // Dyna Comp: 2 knobs (output, sensitivity)
        for snap in presets[2].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                2,
                "Dyna Comp should have 2 params"
            );
        }
        // CP-1X: 4 knobs (level, attack, ratio, compression)
        for snap in presets[3].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "CP-1X should have 4 params"
            );
        }
        // SP: 3 controls (volume, blend, compression toggle)
        for snap in presets[4].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "SP should have 3 params"
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
    fn dyna_comp_full_squeeze_has_high_sensitivity() {
        let dyna = &presets()[2];
        let squeeze = dyna
            .snapshots()
            .iter()
            .find(|s| s.name() == "Full Squeeze")
            .unwrap();
        let block = squeeze.block();
        let params = block.parameters();
        let sensitivity = params.iter().find(|p| p.id() == "sensitivity").unwrap();
        assert!(sensitivity.value().get() > 0.80);
    }

    #[test]
    fn keeley_transparent_has_low_sustain() {
        let keeley = &presets()[0];
        let transparent = keeley
            .snapshots()
            .iter()
            .find(|s| s.name() == "Transparent")
            .unwrap();
        let block = transparent.block();
        let params = block.parameters();
        let sustain = params.iter().find(|p| p.id() == "sustain").unwrap();
        assert!(sustain.value().get() <= 0.30);
    }
}
