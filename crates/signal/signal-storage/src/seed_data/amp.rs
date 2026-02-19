//! Amp block presets — modeled after iconic guitar amplifiers.
//!
//! Each preset corresponds to a real amp with its actual front-panel controls.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![
        fender_twin_reverb(),
        vox_ac30(),
        marshall_jcm800(),
        mesa_dual_rectifier(),
        soldano_slo100(),
    ]
}

// ─── Fender Twin Reverb ─────────────────────────────────────────
// Knobs: Volume, Treble, Middle, Bass, Reverb
// The definitive clean amp — 85 watts of headroom.

fn twin_block(volume: f32, treble: f32, middle: f32, bass: f32, reverb: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("volume", "Volume", volume),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("middle", "Middle", middle),
        BlockParameter::new("bass", "Bass", bass),
        BlockParameter::new("reverb", "Reverb", reverb),
    ])
}

fn fender_twin_reverb() -> Preset {
    Preset::new(
        seed_id("amp-twin"),
        "Fender Twin Reverb",
        BlockType::Amp,
        // Default: classic Fender clean — bright, scooped, touch of reverb
        Snapshot::new(
            seed_id("amp-twin-default"),
            "Default",
            twin_block(0.45, 0.65, 0.50, 0.55, 0.30),
        ),
        vec![
            // Jazz: warm, rolled-off treble, bass forward, lush reverb
            Snapshot::new(
                seed_id("amp-twin-jazz"),
                "Jazz",
                twin_block(0.38, 0.40, 0.55, 0.68, 0.45),
            ),
            // Surf: bright, heavy reverb — Dick Dale territory
            Snapshot::new(
                seed_id("amp-twin-surf"),
                "Surf",
                twin_block(0.50, 0.72, 0.48, 0.50, 0.75),
            ),
            // Country sparkle: bright and snappy, dry
            Snapshot::new(
                seed_id("amp-twin-country"),
                "Country Sparkle",
                twin_block(0.55, 0.75, 0.52, 0.45, 0.15),
            ),
            // Edge of breakup: volume pushed, starts to compress
            Snapshot::new(
                seed_id("amp-twin-edge"),
                "Edge of Breakup",
                twin_block(0.70, 0.60, 0.50, 0.52, 0.20),
            ),
        ],
    )
}

// ─── Vox AC30 ───────────────────────────────────────────────────
// Top Boost channel knobs: Volume, Treble, Bass, Tone Cut, Master
// The chimey British amp — class A, no negative feedback.

fn ac30_block(volume: f32, treble: f32, bass: f32, tone_cut: f32, master: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("volume", "Volume", volume),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("bass", "Bass", bass),
        BlockParameter::new("tone_cut", "Tone Cut", tone_cut),
        BlockParameter::new("master", "Master", master),
    ])
}

fn vox_ac30() -> Preset {
    Preset::new(
        seed_id("amp-ac30"),
        "Vox AC30",
        BlockType::Amp,
        // Default: classic chime — treble up, moderate volume, low cut
        Snapshot::new(
            seed_id("amp-ac30-default"),
            "Default",
            ac30_block(0.55, 0.65, 0.45, 0.30, 0.50),
        ),
        vec![
            // Clean chime: lower volume, bright treble — Beatles/Brian May clean
            Snapshot::new(
                seed_id("amp-ac30-chime"),
                "Chime",
                ac30_block(0.40, 0.70, 0.42, 0.25, 0.45),
            ),
            // Jangle: mid-gain for 80s jangle-pop — The Smiths, R.E.M.
            Snapshot::new(
                seed_id("amp-ac30-jangle"),
                "Jangle",
                ac30_block(0.60, 0.62, 0.48, 0.35, 0.55),
            ),
            // Cranked: volume and master up for classic AC30 breakup
            Snapshot::new(
                seed_id("amp-ac30-cranked"),
                "Cranked",
                ac30_block(0.78, 0.58, 0.50, 0.40, 0.72),
            ),
            // Dark: tone cut up, treble rolled back — warm and thick
            Snapshot::new(
                seed_id("amp-ac30-dark"),
                "Dark",
                ac30_block(0.50, 0.38, 0.60, 0.70, 0.50),
            ),
        ],
    )
}

// ─── Marshall JCM800 ────────────────────────────────────────────
// Knobs: Preamp, Master, Bass, Middle, Treble, Presence
// The quintessential rock amp — 2203 single-channel.

fn jcm800_block(
    preamp: f32,
    master: f32,
    bass: f32,
    middle: f32,
    treble: f32,
    presence: f32,
) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("preamp", "Preamp", preamp),
        BlockParameter::new("master", "Master", master),
        BlockParameter::new("bass", "Bass", bass),
        BlockParameter::new("middle", "Middle", middle),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("presence", "Presence", presence),
    ])
}

fn marshall_jcm800() -> Preset {
    Preset::new(
        seed_id("amp-jcm800"),
        "Marshall JCM800",
        BlockType::Amp,
        // Default: classic rock crunch — preamp around 6, moderate master
        Snapshot::new(
            seed_id("amp-jcm800-default"),
            "Default",
            jcm800_block(0.60, 0.45, 0.50, 0.65, 0.60, 0.55),
        ),
        vec![
            // Clean-ish: low preamp, higher master — pedal platform
            Snapshot::new(
                seed_id("amp-jcm800-pedal-platform"),
                "Pedal Platform",
                jcm800_block(0.30, 0.55, 0.52, 0.60, 0.55, 0.50),
            ),
            // Classic rock: the AC/DC tone — mids pushed, preamp cooking
            Snapshot::new(
                seed_id("amp-jcm800-classic-rock"),
                "Classic Rock",
                jcm800_block(0.70, 0.50, 0.48, 0.72, 0.65, 0.58),
            ),
            // High gain: preamp maxed, tight bass — 80s metal
            Snapshot::new(
                seed_id("amp-jcm800-high-gain"),
                "High Gain",
                jcm800_block(0.85, 0.42, 0.42, 0.68, 0.70, 0.62),
            ),
        ],
    )
}

// ─── Mesa/Boogie Dual Rectifier ─────────────────────────────────
// Knobs: Gain, Master, Bass, Mid, Treble, Presence
// The modern high-gain standard — three channels, here modeled
// as channel 2/3 "Modern High Gain" mode.

fn recto_block(gain: f32, master: f32, bass: f32, mid: f32, treble: f32, presence: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("gain", "Gain", gain),
        BlockParameter::new("master", "Master", master),
        BlockParameter::new("bass", "Bass", bass),
        BlockParameter::new("mid", "Mid", mid),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("presence", "Presence", presence),
    ])
}

fn mesa_dual_rectifier() -> Preset {
    Preset::new(
        seed_id("amp-recto"),
        "Mesa Dual Rectifier",
        BlockType::Amp,
        // Default: modern high gain — scooped mids, heavy bass
        Snapshot::new(
            seed_id("amp-recto-default"),
            "Default",
            recto_block(0.65, 0.50, 0.65, 0.40, 0.62, 0.55),
        ),
        vec![
            // Nu-metal scoop: deep V in the mids, bass and treble high
            Snapshot::new(
                seed_id("amp-recto-scooped"),
                "Scooped",
                recto_block(0.75, 0.45, 0.72, 0.25, 0.70, 0.50),
            ),
            // Tight rhythm: tighter bass, mids present for mix cut
            Snapshot::new(
                seed_id("amp-recto-tight"),
                "Tight Rhythm",
                recto_block(0.68, 0.48, 0.50, 0.55, 0.65, 0.60),
            ),
            // Vintage mode: lower gain, warmer — classic rock tones from a Recto
            Snapshot::new(
                seed_id("amp-recto-vintage"),
                "Vintage",
                recto_block(0.45, 0.52, 0.58, 0.55, 0.55, 0.48),
            ),
            // Lead: high gain, presence cranked for solo cut
            Snapshot::new(
                seed_id("amp-recto-lead"),
                "Lead",
                recto_block(0.80, 0.55, 0.55, 0.50, 0.65, 0.72),
            ),
        ],
    )
}

// ─── Soldano SLO-100 ───────────────────────────────────────────
// Knobs: Preamp, Master, Bass, Middle, Treble, Presence
// The original boutique high-gain amp — smooth overdrive,
// creamy lead tones.

fn slo_block(
    preamp: f32,
    master: f32,
    bass: f32,
    middle: f32,
    treble: f32,
    presence: f32,
) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("preamp", "Preamp", preamp),
        BlockParameter::new("master", "Master", master),
        BlockParameter::new("bass", "Bass", bass),
        BlockParameter::new("middle", "Middle", middle),
        BlockParameter::new("treble", "Treble", treble),
        BlockParameter::new("presence", "Presence", presence),
    ])
}

fn soldano_slo100() -> Preset {
    Preset::new(
        seed_id("amp-slo100"),
        "Soldano SLO-100",
        BlockType::Amp,
        // Default: overdrive channel — the classic SLO crunch
        Snapshot::new(
            seed_id("amp-slo100-default"),
            "Default",
            slo_block(0.60, 0.48, 0.52, 0.60, 0.58, 0.55),
        ),
        vec![
            // Clean: normal channel, low preamp — surprisingly good cleans
            Snapshot::new(
                seed_id("amp-slo100-clean"),
                "Clean",
                slo_block(0.25, 0.55, 0.50, 0.55, 0.52, 0.45),
            ),
            // Crunch: mid preamp — classic rock rhythm
            Snapshot::new(
                seed_id("amp-slo100-crunch"),
                "Crunch",
                slo_block(0.55, 0.50, 0.50, 0.62, 0.60, 0.52),
            ),
            // Screaming lead: preamp cranked — smooth, singing sustain
            Snapshot::new(
                seed_id("amp-slo100-lead"),
                "Screaming Lead",
                slo_block(0.82, 0.45, 0.48, 0.58, 0.62, 0.65),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amp_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn all_amp_presets_are_amp_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Amp);
        }
    }

    #[test]
    fn twin_has_5_snapshots() {
        let twin = &presets()[0];
        assert_eq!(twin.name(), "Fender Twin Reverb");
        assert_eq!(twin.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn ac30_has_5_snapshots() {
        let ac30 = &presets()[1];
        assert_eq!(ac30.name(), "Vox AC30");
        assert_eq!(ac30.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn jcm800_has_4_snapshots() {
        let jcm = &presets()[2];
        assert_eq!(jcm.name(), "Marshall JCM800");
        assert_eq!(jcm.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn recto_has_5_snapshots() {
        let recto = &presets()[3];
        assert_eq!(recto.name(), "Mesa Dual Rectifier");
        assert_eq!(recto.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn slo100_has_4_snapshots() {
        let slo = &presets()[4];
        assert_eq!(slo.name(), "Soldano SLO-100");
        assert_eq!(slo.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_amp_controls() {
        let presets = presets();
        // Twin Reverb: 5 knobs (volume, treble, middle, bass, reverb)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Twin should have 5 params"
            );
        }
        // AC30: 5 knobs (volume, treble, bass, tone_cut, master)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "AC30 should have 5 params"
            );
        }
        // JCM800: 6 knobs (preamp, master, bass, middle, treble, presence)
        for snap in presets[2].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "JCM800 should have 6 params"
            );
        }
        // Dual Rectifier: 6 knobs (gain, master, bass, mid, treble, presence)
        for snap in presets[3].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "Recto should have 6 params"
            );
        }
        // SLO-100: 6 knobs (preamp, master, bass, middle, treble, presence)
        for snap in presets[4].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "SLO should have 6 params"
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
    fn twin_surf_has_high_reverb() {
        let twin = &presets()[0];
        let surf = twin
            .snapshots()
            .iter()
            .find(|s| s.name() == "Surf")
            .unwrap();
        let block = surf.block();
        let params = block.parameters();
        let reverb = params.iter().find(|p| p.id() == "reverb").unwrap();
        assert!(reverb.value().get() > 0.70);
    }

    #[test]
    fn jcm800_high_gain_has_high_preamp() {
        let jcm = &presets()[2];
        let hg = jcm
            .snapshots()
            .iter()
            .find(|s| s.name() == "High Gain")
            .unwrap();
        let block = hg.block();
        let params = block.parameters();
        let preamp = params.iter().find(|p| p.id() == "preamp").unwrap();
        assert!(preamp.value().get() > 0.80);
    }

    #[test]
    fn recto_scooped_has_low_mids() {
        let recto = &presets()[3];
        let scooped = recto
            .snapshots()
            .iter()
            .find(|s| s.name() == "Scooped")
            .unwrap();
        let block = scooped.block();
        let params = block.parameters();
        let mid = params.iter().find(|p| p.id() == "mid").unwrap();
        assert!(mid.value().get() < 0.30);
    }
}
