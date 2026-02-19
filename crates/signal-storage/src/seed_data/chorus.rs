//! Chorus block presets — modeled after chorus effects.
//!
//! Each preset corresponds to a real chorus plugin/effect with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![js_chorus(), tal_chorus()]
}

// ─── JS Chorus ──────────────────────────────────────────────────
// Knobs: Rate, Depth, Mix
// Jesusonic/REAPER built-in chorus — lightweight, zero-latency,
// always available in any REAPER install.

fn js_block(rate: f32, depth: f32, mix: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
    ])
}

fn js_chorus() -> Preset {
    Preset::new(
        seed_id("chorus-js"),
        "JS Chorus",
        BlockType::Chorus,
        // Default: moderate chorus — usable on guitars, keys, vocals
        Snapshot::new(
            seed_id("chorus-js-default"),
            "Default",
            js_block(0.40, 0.45, 0.50),
        ),
        vec![
            // Subtle: low rate, low depth, low mix — just a hint of motion
            Snapshot::new(
                seed_id("chorus-js-subtle"),
                "Subtle",
                js_block(0.20, 0.20, 0.25),
            ),
            // Thick: moderate rate, high depth, high mix — lush doubling
            Snapshot::new(
                seed_id("chorus-js-thick"),
                "Thick",
                js_block(0.45, 0.75, 0.70),
            ),
            // Fast shimmer: high rate, moderate depth — shimmering texture
            Snapshot::new(
                seed_id("chorus-js-fast-shimmer"),
                "Fast Shimmer",
                js_block(0.80, 0.40, 0.50),
            ),
        ],
    )
}

// ─── TAL-Chorus-LX ──────────────────────────────────────────────
// Knobs: Rate, Depth, Mix, Volume
// TAL Software's faithful emulation of the Roland Juno-60 chorus —
// the definitive analog chorus sound. Two classic modes (Chorus I
// and Chorus II) plus modern deep-modulation settings.

fn tal_block(rate: f32, depth: f32, mix: f32, volume: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("rate", "Rate", rate),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("volume", "Volume", volume),
    ])
}

fn tal_chorus() -> Preset {
    Preset::new(
        seed_id("chorus-tal"),
        "TAL-Chorus-LX",
        BlockType::Chorus,
        // Default: balanced Juno-style chorus — warm and musical
        Snapshot::new(
            seed_id("chorus-tal-default"),
            "Default",
            tal_block(0.45, 0.50, 0.50, 0.55),
        ),
        vec![
            // Juno I: slower rate, moderate depth — the classic Chorus I button
            Snapshot::new(
                seed_id("chorus-tal-juno-i"),
                "Juno I",
                tal_block(0.30, 0.45, 0.50, 0.55),
            ),
            // Juno II: faster rate, deeper — the classic Chorus II button
            Snapshot::new(
                seed_id("chorus-tal-juno-ii"),
                "Juno II",
                tal_block(0.55, 0.65, 0.55, 0.55),
            ),
            // Lush Pad: slow rate, max depth, high mix — thick ensemble wash
            Snapshot::new(
                seed_id("chorus-tal-lush-pad"),
                "Lush Pad",
                tal_block(0.20, 1.00, 0.75, 0.60),
            ),
            // Subtle warmth: low depth, low mix — gentle analog character
            Snapshot::new(
                seed_id("chorus-tal-subtle-warmth"),
                "Subtle Warmth",
                tal_block(0.35, 0.25, 0.30, 0.50),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chorus_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_chorus_presets_are_chorus_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Chorus);
        }
    }

    #[test]
    fn js_chorus_has_4_snapshots() {
        let js = &presets()[0];
        assert_eq!(js.name(), "JS Chorus");
        assert_eq!(js.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn tal_chorus_has_5_snapshots() {
        let tal = &presets()[1];
        assert_eq!(tal.name(), "TAL-Chorus-LX");
        assert_eq!(tal.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn parameter_counts_match_knobs() {
        let presets = presets();
        // JS Chorus: 3 knobs (rate, depth, mix)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "JS Chorus should have 3 params"
            );
        }
        // TAL-Chorus-LX: 4 knobs (rate, depth, mix, volume)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                4,
                "TAL-Chorus-LX should have 4 params"
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
    fn tal_juno_ii_has_higher_rate_than_juno_i() {
        let tal = &presets()[1];
        let juno_i = tal
            .snapshots()
            .iter()
            .find(|s| s.name() == "Juno I")
            .unwrap();
        let juno_ii = tal
            .snapshots()
            .iter()
            .find(|s| s.name() == "Juno II")
            .unwrap();
        let rate_i = juno_i
            .block()
            .parameters()
            .iter()
            .find(|p| p.id() == "rate")
            .unwrap()
            .value()
            .get();
        let rate_ii = juno_ii
            .block()
            .parameters()
            .iter()
            .find(|p| p.id() == "rate")
            .unwrap()
            .value()
            .get();
        assert!(
            rate_ii > rate_i,
            "Juno II rate ({}) should be higher than Juno I rate ({})",
            rate_ii,
            rate_i
        );
    }
}
