//! Drive block presets — modeled after iconic overdrive pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{metadata::Metadata, seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![
        tubescreamer(),
        klon_centaur(),
        fulltone_ocd(),
        bluesbreaker(),
        morning_glory(),
        parametric_od(),
    ]
}

// ─── Ibanez Tubescreamer TS808 ──────────────────────────────────
// Knobs: Drive, Tone, Level

fn ts_block(drive: f32, tone: f32, level: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("level", "Level", level),
    ])
}

fn tubescreamer() -> Preset {
    Preset::new(
        seed_id("drive-ts808"),
        "Tubescreamer",
        BlockType::Drive,
        // Default: classic mid-push rhythm tone
        Snapshot::new(
            seed_id("drive-ts808-default"),
            "Default",
            ts_block(0.50, 0.55, 0.50),
        ),
        vec![
            // Low drive, high level — transparent volume boost into amp
            Snapshot::new(
                seed_id("drive-ts808-clean-boost"),
                "Clean Boost",
                ts_block(0.15, 0.50, 0.80),
            ),
            // Just enough drive to start breaking up on hard picking
            Snapshot::new(
                seed_id("drive-ts808-edge-breakup"),
                "Edge Breakup",
                ts_block(0.35, 0.58, 0.60),
            ),
            // Classic TS tone — mid hump, moderate drive
            Snapshot::new(
                seed_id("drive-ts808-mid-push"),
                "Mid-Push",
                ts_block(0.60, 0.65, 0.50),
            ),
            // High drive for stacking into another pedal or dirty amp
            Snapshot::new(
                seed_id("drive-ts808-stacked"),
                "Stacked Driver",
                ts_block(0.78, 0.48, 0.55),
            ),
        ],
    )
}

// ─── Klon Centaur ───────────────────────────────────────────────
// Knobs: Gain, Treble, Output

fn klon_block(gain: f32, treble: f32, output: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("gain", "Gain", gain),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("output", "Output", output),
    ])
}

fn klon_centaur() -> Preset {
    Preset::new(
        seed_id("drive-klon"),
        "Klon Centaur",
        BlockType::Drive,
        // Default: the classic "transparent" Klon tone
        Snapshot::new(
            seed_id("drive-klon-default"),
            "Default",
            klon_block(0.35, 0.50, 0.60),
        ),
        vec![
            // Gain nearly off — clean boost with Klon's buffer character
            Snapshot::new(
                seed_id("drive-klon-transparent"),
                "Transparent Boost",
                klon_block(0.10, 0.48, 0.75),
            ),
            // Light gain, sweetens the signal without obvious clipping
            Snapshot::new(
                seed_id("drive-klon-sweetener"),
                "Sweetener",
                klon_block(0.30, 0.52, 0.58),
            ),
            // Mid-high gain — the Klon pushed into real overdrive
            Snapshot::new(
                seed_id("drive-klon-pushed"),
                "Pushed",
                klon_block(0.62, 0.55, 0.50),
            ),
            // High output, moderate gain — cuts through for solos
            Snapshot::new(
                seed_id("drive-klon-solo"),
                "Solo Lift",
                klon_block(0.45, 0.60, 0.80),
            ),
        ],
    )
}

// ─── Fulltone OCD ───────────────────────────────────────────────
// Knobs: Drive, Tone, Volume

fn ocd_block(drive: f32, tone: f32, volume: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("volume", "Volume", volume),
    ])
}

fn fulltone_ocd() -> Preset {
    Preset::new(
        seed_id("drive-ocd"),
        "Fulltone OCD",
        BlockType::Drive,
        // Default: medium crunch, balanced tone
        Snapshot::new(
            seed_id("drive-ocd-default"),
            "Default",
            ocd_block(0.45, 0.50, 0.55),
        ),
        vec![
            // Low drive — just a touch of hair on clean amps
            Snapshot::new(
                seed_id("drive-ocd-low-crunch"),
                "Low Crunch",
                ocd_block(0.25, 0.45, 0.60),
            ),
            // Classic OCD — the sound that made the pedal famous
            Snapshot::new(
                seed_id("drive-ocd-classic"),
                "Classic",
                ocd_block(0.55, 0.52, 0.50),
            ),
            // High drive, rolled-back tone — thick Marshall-style lead
            Snapshot::new(
                seed_id("drive-ocd-thick-lead"),
                "Thick Lead",
                ocd_block(0.78, 0.38, 0.48),
            ),
        ],
    )
}

// ─── Marshall Bluesbreaker ──────────────────────────────────────
// Knobs: Gain, Tone, Volume
// Note: volume at ~0.75 is roughly unity gain on the real pedal.

fn bb_block(gain: f32, tone: f32, volume: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("gain", "Gain", gain),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("volume", "Volume", volume),
    ])
}

fn bluesbreaker() -> Preset {
    Preset::new(
        seed_id("drive-bluesbreaker"),
        "Bluesbreaker",
        BlockType::Drive,
        // Default: classic transparent breakup
        Snapshot::new(
            seed_id("drive-bluesbreaker-default"),
            "Default",
            bb_block(0.50, 0.52, 0.72),
        ),
        vec![
            // Low gain, just amp-like breakup on hard strums
            Snapshot::new(
                seed_id("drive-bluesbreaker-breakup"),
                "Breakup",
                bb_block(0.35, 0.48, 0.75),
            ),
            // Very clean, near-unity — transparent rhythm enhancement
            Snapshot::new(
                seed_id("drive-bluesbreaker-rhythm"),
                "Transparent Rhythm",
                bb_block(0.20, 0.50, 0.78),
            ),
        ],
    )
}

// ─── JHS Morning Glory ─────────────────────────────────────────
// Knobs: Volume, Drive, Tone
// (Bluesbreaker-inspired, but with more headroom and a hi-cut option)

fn mg_block(volume: f32, drive: f32, tone: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("volume", "Volume", volume),
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("tone", "Tone", tone),
    ])
}

fn morning_glory() -> Preset {
    Preset::new(
        seed_id("drive-morning-glory"),
        "Morning Glory",
        BlockType::Drive,
        // Default: light drive, balanced — the "always on" pedal
        Snapshot::new(
            seed_id("drive-morning-glory-default"),
            "Default",
            mg_block(0.55, 0.35, 0.52),
        ),
        vec![
            // Nearly clean — just adds body and presence
            Snapshot::new(
                seed_id("drive-morning-glory-enhancer"),
                "Enhancer",
                mg_block(0.60, 0.15, 0.48),
            ),
            // Light push for rhythm parts
            Snapshot::new(
                seed_id("drive-morning-glory-rhythm"),
                "Rhythm Push",
                mg_block(0.55, 0.40, 0.55),
            ),
            // More drive + volume for lead lines
            Snapshot::new(
                seed_id("drive-morning-glory-lead"),
                "Lead Lift",
                mg_block(0.70, 0.58, 0.58),
            ),
            // Full drive — the MG pushed hard
            Snapshot::new(
                seed_id("drive-morning-glory-driver"),
                "Driver",
                mg_block(0.55, 0.75, 0.50),
            ),
        ],
    )
}

// ─── Parametric Overdrive ────────────────────────────────────────
// Knobs: Drive, Tone, Level
// Maps to the Cockos JS amp/drive plugin bundled with REAPER.
// Parameter names match the JS plugin's slider names directly,
// so no daw_name override is needed — REAPER looks them up by display name.

fn pod_block(drive: f32, tone: f32, level: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("drive", "Drive", drive),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("level", "Level", level),
    ])
}

fn parametric_od() -> Preset {
    Preset::new(
        seed_id("drive-parametric-od"),
        "Parametric OD",
        BlockType::Drive,
        // Default: moderate drive, neutral tone, unity level
        Snapshot::new(
            seed_id("drive-parametric-od-default"),
            "Default",
            pod_block(0.50, 0.50, 0.50),
        ),
        vec![
            // Light touch — just enough to add warmth and presence
            Snapshot::new(
                seed_id("drive-parametric-od-light"),
                "Light",
                pod_block(0.25, 0.52, 0.58),
            ),
            // Heavy drive — pushed hard for lead tones
            Snapshot::new(
                seed_id("drive-parametric-od-heavy"),
                "Heavy",
                pod_block(0.78, 0.48, 0.46),
            ),
            // Bright cut — roll off lows, boost highs for single-coil clarity
            Snapshot::new(
                seed_id("drive-parametric-od-bright"),
                "Bright Cut",
                pod_block(0.45, 0.72, 0.52),
            ),
        ],
    )
    .with_metadata(Metadata::new().with_tag("source:JS: Amp (Cockos)"))
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 6);
    }

    #[test]
    fn all_drive_presets_are_drive_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Drive);
        }
    }

    #[test]
    fn tubescreamer_has_5_snapshots() {
        // default + 4 additional
        let ts = &presets()[0];
        assert_eq!(ts.name(), "Tubescreamer");
        assert_eq!(ts.snapshots().len(), 5);
    }

    #[test]
    fn klon_has_5_snapshots() {
        let klon = &presets()[1];
        assert_eq!(klon.name(), "Klon Centaur");
        assert_eq!(klon.snapshots().len(), 5);
    }

    #[test]
    fn ocd_has_4_snapshots() {
        let ocd = &presets()[2];
        assert_eq!(ocd.name(), "Fulltone OCD");
        assert_eq!(ocd.snapshots().len(), 4);
    }

    #[test]
    fn bluesbreaker_has_3_snapshots() {
        let bb = &presets()[3];
        assert_eq!(bb.name(), "Bluesbreaker");
        assert_eq!(bb.snapshots().len(), 3);
    }

    #[test]
    fn morning_glory_has_5_snapshots() {
        let mg = &presets()[4];
        assert_eq!(mg.name(), "Morning Glory");
        assert_eq!(mg.snapshots().len(), 5);
    }

    #[test]
    fn parametric_od_has_4_snapshots() {
        let pod = &presets()[5];
        assert_eq!(pod.name(), "Parametric OD");
        assert_eq!(pod.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parametric_od_has_source_tag() {
        let pod = &presets()[5];
        let tags = &pod.metadata().tags;
        let has_source = tags.as_slice().iter().any(|t| t.starts_with("source:"));
        assert!(has_source, "Parametric OD preset must have a source: tag");
    }

    #[test]
    fn all_snapshots_have_correct_parameter_count() {
        for preset in presets() {
            for snapshot in preset.snapshots() {
                let block = snapshot.block();
                let params = block.parameters();
                assert_eq!(
                    params.len(),
                    3,
                    "preset '{}' snapshot '{}' should have 3 params, got {}",
                    preset.name(),
                    snapshot.name(),
                    params.len()
                );
            }
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
    fn tubescreamer_clean_boost_has_low_drive_high_level() {
        let ts = &presets()[0];
        let clean_boost = ts
            .snapshots()
            .iter()
            .find(|s| s.name() == "Clean Boost")
            .unwrap();
        let block = clean_boost.block();
        let params = block.parameters();
        let drive = params.iter().find(|p| p.id() == "drive").unwrap();
        let level = params.iter().find(|p| p.id() == "level").unwrap();
        // Clean boost: low drive, high level
        assert!(drive.value().get() < 0.25);
        assert!(level.value().get() > 0.70);
    }

    #[test]
    fn klon_transparent_boost_has_minimal_gain() {
        let klon = &presets()[1];
        let transparent = klon
            .snapshots()
            .iter()
            .find(|s| s.name() == "Transparent Boost")
            .unwrap();
        let block = transparent.block();
        let params = block.parameters();
        let gain = params.iter().find(|p| p.id() == "gain").unwrap();
        assert!(gain.value().get() <= 0.15);
    }
}
