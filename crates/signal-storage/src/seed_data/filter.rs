//! Filter block presets — modeled after iconic envelope filter pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![qtron(), volcano()]
}

// ─── Electro-Harmonix Micro Q-Tron ──────────────────────────────
// Knobs: Drive, Peak (Q/resonance), Mix
// Classic envelope filter — funky auto-wah tones from subtle
// to extreme. The Drive controls input sensitivity (how hard the
// envelope opens), Peak sets the resonance/Q of the filter, and
// Mix blends wet/dry.

fn qtron_block(drive: f32, peak: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("peak", "Peak", peak),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn qtron() -> Preset {
    Preset::new(
        seed_id("filter-qtron"),
        "Q-Tron",
        BlockType::Filter,
        // Default: moderate envelope sensitivity, balanced resonance
        Snapshot::new(
            seed_id("filter-qtron-default"),
            "Default",
            qtron_block(0.50, 0.55, 0.65),
        ),
        vec![
            // High drive + high peak — the classic Jerry Garcia / Bootsy Collins
            // deep funk quack. Envelope opens wide on every note.
            Snapshot::new(
                seed_id("filter-qtron-funk-quack"),
                "Funk Quack",
                qtron_block(0.78, 0.82, 0.75),
            ),
            // Low drive, low peak — gentle filter sweep that adds movement
            // without dominating the tone. Good for rhythm parts.
            Snapshot::new(
                seed_id("filter-qtron-subtle-wah"),
                "Subtle Wah",
                qtron_block(0.25, 0.30, 0.55),
            ),
            // High drive, very high peak — tight, resonant filter that
            // tracks dynamics aggressively. Synth-like bass tones.
            Snapshot::new(
                seed_id("filter-qtron-synth-bass"),
                "Synth Bass",
                qtron_block(0.72, 0.90, 0.80),
            ),
        ],
    )
}

// ─── Source Audio Soundblox 2 Manta / "Volcano" ─────────────────
// Knobs: Frequency, Resonance, Envelope, Mix
// Multimode filter with envelope follower — from subtle fixed-frequency
// tone shaping to screaming acid squelch. Frequency sets the base cutoff,
// Resonance controls the filter peak, Envelope determines how much
// playing dynamics sweep the cutoff, and Mix blends wet/dry.

fn volcano_block(frequency: f32, resonance: f32, envelope: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("frequency", "Frequency", frequency),
        BlockParameter::new("resonance", "Resonance", resonance),
        BlockParameter::new("envelope", "Envelope", envelope),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn volcano() -> Preset {
    Preset::new(
        seed_id("filter-volcano"),
        "Volcano",
        BlockType::Filter,
        // Default: moderate cutoff, balanced resonance and envelope
        Snapshot::new(
            seed_id("filter-volcano-default"),
            "Default",
            volcano_block(0.50, 0.45, 0.55, 0.70),
        ),
        vec![
            // High envelope sensitivity, moderate resonance — dynamic sweep
            // that opens up on hard picking and closes on soft playing.
            Snapshot::new(
                seed_id("filter-volcano-sweep"),
                "Sweep",
                volcano_block(0.45, 0.50, 0.78, 0.72),
            ),
            // Low envelope, set frequency — static tone shaping that acts
            // more like a fixed EQ filter than a dynamic effect.
            Snapshot::new(
                seed_id("filter-volcano-fixed-filter"),
                "Fixed Filter",
                volcano_block(0.60, 0.35, 0.12, 0.65),
            ),
            // High resonance + high envelope — TB-303-style acid squelch.
            // Self-resonant filter that screams on every note.
            Snapshot::new(
                seed_id("filter-volcano-acid-squelch"),
                "Acid Squelch",
                volcano_block(0.40, 0.85, 0.82, 0.80),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_filter_presets_are_filter_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Filter);
        }
    }

    #[test]
    fn qtron_has_4_snapshots() {
        // default + 3 additional
        let qt = &presets()[0];
        assert_eq!(qt.name(), "Q-Tron");
        assert_eq!(qt.snapshots().len(), 4);
    }

    #[test]
    fn volcano_has_4_snapshots() {
        // default + 3 additional
        let vol = &presets()[1];
        assert_eq!(vol.name(), "Volcano");
        assert_eq!(vol.snapshots().len(), 4);
    }

    #[test]
    fn qtron_snapshots_have_3_parameters() {
        let qt = &presets()[0];
        for snap in qt.snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Q-Tron snapshot '{}' should have 3 params",
                snap.name()
            );
        }
    }

    #[test]
    fn volcano_snapshots_have_4_parameters() {
        let vol = &presets()[1];
        for snap in vol.snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "Volcano snapshot '{}' should have 4 params",
                snap.name()
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
    fn qtron_funk_quack_has_high_peak() {
        let qt = &presets()[0];
        let funk = qt
            .snapshots()
            .iter()
            .find(|s| s.name() == "Funk Quack")
            .unwrap();
        let block = funk.block();
        let params = block.parameters();
        let peak = params.iter().find(|p| p.id() == "peak").unwrap();
        // Funk Quack: high resonance for that classic quack
        assert!(peak.value().get() > 0.75);
    }

    #[test]
    fn volcano_acid_squelch_has_high_resonance_and_envelope() {
        let vol = &presets()[1];
        let acid = vol
            .snapshots()
            .iter()
            .find(|s| s.name() == "Acid Squelch")
            .unwrap();
        let block = acid.block();
        let params = block.parameters();
        let res = params.iter().find(|p| p.id() == "resonance").unwrap();
        let env = params.iter().find(|p| p.id() == "envelope").unwrap();
        // Acid squelch: high resonance + high envelope sensitivity
        assert!(res.value().get() > 0.80);
        assert!(env.value().get() > 0.75);
    }
}
