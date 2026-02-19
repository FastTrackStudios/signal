//! Reverb block presets — modeled after iconic reverb pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![
        strymon_bigsky(),
        boss_rv6(),
        walrus_slo(),
        eventide_space(),
        ehx_oceans11(),
    ]
}

// ─── Strymon BigSky ─────────────────────────────────────────────
// Knobs: Decay, Pre-Delay, Mix, Tone, Mod
// The studio-grade multi-reverb — 12 reverb machines in one box.

fn bigsky_block(decay: f32, pre_delay: f32, mix: f32, tone: f32, modulation: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("decay", "Decay", decay),
        BlockParameter::new("pre_delay", "Pre-Delay", pre_delay),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("mod", "Mod", modulation),
    ])
}

fn strymon_bigsky() -> Preset {
    Preset::new(
        seed_id("reverb-bigsky"),
        "Strymon BigSky",
        BlockType::Reverb,
        // Default: lush hall — moderate decay, subtle mod
        Snapshot::new(
            seed_id("reverb-bigsky-default"),
            "Default",
            bigsky_block(0.55, 0.30, 0.40, 0.50, 0.25),
        ),
        vec![
            // Room: short decay, low mix — natural space
            Snapshot::new(
                seed_id("reverb-bigsky-room"),
                "Room",
                bigsky_block(0.30, 0.15, 0.30, 0.52, 0.10),
            ),
            // Shimmer: long decay, bright tone, heavy mod
            Snapshot::new(
                seed_id("reverb-bigsky-shimmer"),
                "Shimmer",
                bigsky_block(0.80, 0.25, 0.55, 0.65, 0.70),
            ),
            // Plate: medium decay, no pre-delay, warm
            Snapshot::new(
                seed_id("reverb-bigsky-plate"),
                "Plate",
                bigsky_block(0.50, 0.08, 0.45, 0.45, 0.15),
            ),
            // Ambient wash: long decay, high mix — post-rock territory
            Snapshot::new(
                seed_id("reverb-bigsky-ambient"),
                "Ambient Wash",
                bigsky_block(0.88, 0.35, 0.65, 0.48, 0.45),
            ),
        ],
    )
}

// ─── Boss RV-6 ──────────────────────────────────────────────────
// Knobs: E.Level (effect level), Tone, Time
// The workhorse digital reverb — 8 modes, simple controls.

fn rv6_block(level: f32, tone: f32, time: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("level", "E.Level", level),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("time", "Time", time),
    ])
}

fn boss_rv6() -> Preset {
    Preset::new(
        seed_id("reverb-rv6"),
        "Boss RV-6",
        BlockType::Reverb,
        // Default: hall mode — balanced and musical
        Snapshot::new(
            seed_id("reverb-rv6-default"),
            "Default",
            rv6_block(0.45, 0.50, 0.50),
        ),
        vec![
            // Spring: short time, bright tone — surf/country
            Snapshot::new(
                seed_id("reverb-rv6-spring"),
                "Spring",
                rv6_block(0.50, 0.60, 0.35),
            ),
            // Modulate: longer time, lush chorus-like tail
            Snapshot::new(
                seed_id("reverb-rv6-modulate"),
                "Modulate",
                rv6_block(0.48, 0.48, 0.65),
            ),
            // Subtle: low level, short time — just enough depth
            Snapshot::new(
                seed_id("reverb-rv6-subtle"),
                "Subtle",
                rv6_block(0.25, 0.50, 0.30),
            ),
        ],
    )
}

// ─── Walrus Audio Slo ───────────────────────────────────────────
// Knobs: Decay, Filter, Mix, Depth, X
// The ambient reverb — three modes (Dark, Rise, Dream), deeply
// textural. X knob function varies by mode.

fn slo_block(decay: f32, filter: f32, mix: f32, depth: f32, x: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("decay", "Decay", decay),
        BlockParameter::new("filter", "Filter", filter),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("x", "X", x),
    ])
}

fn walrus_slo() -> Preset {
    Preset::new(
        seed_id("reverb-slo"),
        "Walrus Audio Slo",
        BlockType::Reverb,
        // Default: Dream mode — lush pad with gentle vibrato
        Snapshot::new(
            seed_id("reverb-slo-default"),
            "Default",
            slo_block(0.60, 0.50, 0.50, 0.40, 0.35),
        ),
        vec![
            // Dark: sub-octave reverb, filter rolled dark
            Snapshot::new(
                seed_id("reverb-slo-dark"),
                "Dark Octave",
                slo_block(0.70, 0.30, 0.55, 0.45, 0.60),
            ),
            // Rise: long swell, high X for slow rise time
            Snapshot::new(
                seed_id("reverb-slo-rise"),
                "Rise",
                slo_block(0.75, 0.52, 0.60, 0.50, 0.75),
            ),
            // Pad: max decay, full mix — infinite sustain pad
            Snapshot::new(
                seed_id("reverb-slo-pad"),
                "Infinite Pad",
                slo_block(1.00, 0.45, 0.80, 0.55, 0.40),
            ),
            // Subtle dream: low mix, gentle modulation
            Snapshot::new(
                seed_id("reverb-slo-subtle"),
                "Subtle Dream",
                slo_block(0.45, 0.55, 0.25, 0.20, 0.25),
            ),
        ],
    )
}

// ─── Eventide Space ─────────────────────────────────────────────
// Knobs: Mix, Decay, Size, Delay (pre-delay), Tone
// The flagship multi-algorithm reverb — deep parameter control.

fn space_block(mix: f32, decay: f32, size: f32, delay: f32, tone: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("decay", "Decay", decay),
        BlockParameter::new("size", "Size", size),
        BlockParameter::new("delay", "Delay", delay),
        BlockParameter::new("tone", "Tone", tone),
    ])
}

fn eventide_space() -> Preset {
    Preset::new(
        seed_id("reverb-space"),
        "Eventide Space",
        BlockType::Reverb,
        // Default: Hall algorithm — large space, moderate decay
        Snapshot::new(
            seed_id("reverb-space-default"),
            "Default",
            space_block(0.40, 0.55, 0.60, 0.25, 0.50),
        ),
        vec![
            // Blackhole: huge size, near-infinite decay — soundscape
            Snapshot::new(
                seed_id("reverb-space-blackhole"),
                "Blackhole",
                space_block(0.55, 0.90, 0.85, 0.30, 0.42),
            ),
            // Spring: small size, short decay, bright
            Snapshot::new(
                seed_id("reverb-space-spring"),
                "Spring",
                space_block(0.45, 0.35, 0.30, 0.10, 0.62),
            ),
            // Plate: medium size, smooth decay — studio vocal plate
            Snapshot::new(
                seed_id("reverb-space-plate"),
                "Plate",
                space_block(0.38, 0.50, 0.45, 0.05, 0.48),
            ),
            // Modulated: large size, heavy modulation via decay interaction
            Snapshot::new(
                seed_id("reverb-space-modulated"),
                "Modulated Hall",
                space_block(0.50, 0.65, 0.70, 0.20, 0.52),
            ),
        ],
    )
}

// ─── EHX Oceans 11 ──────────────────────────────────────────────
// Knobs: Level, Tone, Time
// 11 reverb types in a compact package — the Swiss army knife reverb.

fn oceans_block(level: f32, tone: f32, time: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("level", "Level", level),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("time", "Time", time),
    ])
}

fn ehx_oceans11() -> Preset {
    Preset::new(
        seed_id("reverb-oceans11"),
        "EHX Oceans 11",
        BlockType::Reverb,
        // Default: hall — rich and even
        Snapshot::new(
            seed_id("reverb-oceans11-default"),
            "Default",
            oceans_block(0.45, 0.50, 0.50),
        ),
        vec![
            // Spring: classic drip, bright tone
            Snapshot::new(
                seed_id("reverb-oceans11-spring"),
                "Spring",
                oceans_block(0.50, 0.62, 0.40),
            ),
            // Shimmer: long time, high level for ethereal pads
            Snapshot::new(
                seed_id("reverb-oceans11-shimmer"),
                "Shimmer",
                oceans_block(0.55, 0.55, 0.78),
            ),
            // Plate: tight, even decay — studio workhorse
            Snapshot::new(
                seed_id("reverb-oceans11-plate"),
                "Plate",
                oceans_block(0.42, 0.48, 0.45),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverb_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn all_reverb_presets_are_reverb_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Reverb);
        }
    }

    #[test]
    fn bigsky_has_5_snapshots() {
        let bigsky = &presets()[0];
        assert_eq!(bigsky.name(), "Strymon BigSky");
        assert_eq!(bigsky.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn rv6_has_4_snapshots() {
        let rv6 = &presets()[1];
        assert_eq!(rv6.name(), "Boss RV-6");
        assert_eq!(rv6.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn slo_has_5_snapshots() {
        let slo = &presets()[2];
        assert_eq!(slo.name(), "Walrus Audio Slo");
        assert_eq!(slo.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn space_has_5_snapshots() {
        let space = &presets()[3];
        assert_eq!(space.name(), "Eventide Space");
        assert_eq!(space.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn oceans11_has_4_snapshots() {
        let oceans = &presets()[4];
        assert_eq!(oceans.name(), "EHX Oceans 11");
        assert_eq!(oceans.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_pedal_knobs() {
        let presets = presets();
        // BigSky: 5 knobs (decay, pre_delay, mix, tone, mod)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "BigSky should have 5 params"
            );
        }
        // RV-6: 3 knobs (level, tone, time)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "RV-6 should have 3 params"
            );
        }
        // Slo: 5 knobs (decay, filter, mix, depth, x)
        for snap in presets[2].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Slo should have 5 params"
            );
        }
        // Space: 5 knobs (mix, decay, size, delay, tone)
        for snap in presets[3].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Space should have 5 params"
            );
        }
        // Oceans 11: 3 knobs (level, tone, time)
        for snap in presets[4].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Oceans 11 should have 3 params"
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
    fn bigsky_shimmer_has_high_mod() {
        let bigsky = &presets()[0];
        let shimmer = bigsky
            .snapshots()
            .iter()
            .find(|s| s.name() == "Shimmer")
            .unwrap();
        let block = shimmer.block();
        let params = block.parameters();
        let modulation = params.iter().find(|p| p.id() == "mod").unwrap();
        assert!(modulation.value().get() > 0.65);
    }

    #[test]
    fn slo_infinite_pad_has_max_decay() {
        let slo = &presets()[2];
        let pad = slo
            .snapshots()
            .iter()
            .find(|s| s.name() == "Infinite Pad")
            .unwrap();
        let block = pad.block();
        let params = block.parameters();
        let decay = params.iter().find(|p| p.id() == "decay").unwrap();
        assert!((decay.value().get() - 1.0).abs() < 0.001);
    }

    #[test]
    fn space_blackhole_has_high_size_and_decay() {
        let space = &presets()[3];
        let blackhole = space
            .snapshots()
            .iter()
            .find(|s| s.name() == "Blackhole")
            .unwrap();
        let block = blackhole.block();
        let params = block.parameters();
        let size = params.iter().find(|p| p.id() == "size").unwrap();
        let decay = params.iter().find(|p| p.id() == "decay").unwrap();
        assert!(size.value().get() > 0.80);
        assert!(decay.value().get() > 0.85);
    }
}
