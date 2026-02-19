//! Module seed data — default module collections for development/demo.

use signal_proto::{
    seed_id, BlockParameterOverride, BlockType, Module, ModuleBlock, ModuleBlockSource,
    ModulePreset, ModuleSnapshot, ModuleType, PresetId, SignalChain, SignalNode, SnapshotId,
};

/// All default module collections (presets).
pub fn presets() -> Vec<ModulePreset> {
    vec![
        drive_full_stack(),
        drive_duo(),
        time_parallel(),
        guitar_source(),
        guitar_dynamics(),
        guitar_special(),
        guitar_volume(),
        guitar_pre_fx(),
        guitar_amp(),
        guitar_modulation(),
        guitar_motion(),
        guitar_master(),
        vocal_rescue(),
        vocal_correction(),
        vocal_tonal(),
        vocal_modulation(),
        vocal_time(),
    ]
}

fn drive_full_stack() -> ModulePreset {
    ModulePreset::new(
        seed_id("drive-full-stack"),
        "Full Drive Stack",
        ModuleType::Drive,
        ModuleSnapshot::new(
            seed_id("drive-full-stack-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "boost",
                    "EP Booster",
                    BlockType::Boost,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("boost-ep")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-1",
                    "Tubescreamer",
                    BlockType::Drive,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("drive-ts808")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-2",
                    "Klon",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-klon")),
                        snapshot_id: SnapshotId::from(seed_id("drive-klon-sweetener")),
                        saved_at_version: None,
                    },
                )
                .with_overrides(vec![
                    BlockParameterOverride::new("treble", 0.55),
                    BlockParameterOverride::new("output", 0.65),
                ]),
                ModuleBlock::new(
                    "drive-3",
                    "OCD",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-ocd")),
                        snapshot_id: SnapshotId::from(seed_id("drive-ocd-classic")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![ModuleSnapshot::new(
            seed_id("drive-full-stack-push"),
            "Push",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "boost",
                    "EP Booster",
                    BlockType::Boost,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("boost-ep")),
                        snapshot_id: SnapshotId::from(seed_id("boost-ep-full")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-1",
                    "Tubescreamer",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-ts808")),
                        snapshot_id: SnapshotId::from(seed_id("drive-ts808-stacked")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-2",
                    "Klon",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-klon")),
                        snapshot_id: SnapshotId::from(seed_id("drive-klon-pushed")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-3",
                    "OCD",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-ocd")),
                        snapshot_id: SnapshotId::from(seed_id("drive-ocd-thick-lead")),
                        saved_at_version: None,
                    },
                )
                .with_overrides(vec![BlockParameterOverride::new("drive", 0.85)]),
            ]),
        )],
    )
}

fn drive_duo() -> ModulePreset {
    ModulePreset::new(
        seed_id("drive-duo"),
        "Drive Duo",
        ModuleType::Drive,
        ModuleSnapshot::new(
            seed_id("drive-duo-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "drive-1",
                    "Bluesbreaker",
                    BlockType::Drive,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("drive-bluesbreaker")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "drive-2",
                    "Morning Glory",
                    BlockType::Drive,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("drive-morning-glory")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![ModuleSnapshot::new(
            seed_id("drive-duo-lead"),
            "Lead",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "drive-1",
                    "Bluesbreaker",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-bluesbreaker")),
                        snapshot_id: SnapshotId::from(seed_id("drive-bluesbreaker-breakup")),
                        saved_at_version: None,
                    },
                )
                .with_overrides(vec![BlockParameterOverride::new("volume", 0.82)]),
                ModuleBlock::new(
                    "drive-2",
                    "Morning Glory",
                    BlockType::Drive,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("drive-morning-glory")),
                        snapshot_id: SnapshotId::from(seed_id("drive-morning-glory-lead")),
                        saved_at_version: None,
                    },
                ),
            ]),
        )],
    )
}

fn time_parallel() -> ModulePreset {
    ModulePreset::new(
        seed_id("time-parallel"),
        "Parallel Time",
        ModuleType::Time,
        ModuleSnapshot::new(
            seed_id("time-parallel-default"),
            "Default",
            Module::from_chain(SignalChain::new(vec![
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-1",
                            "Timeline",
                            BlockType::Delay,
                            ModuleBlockSource::PresetDefault {
                                preset_id: PresetId::from(seed_id("delay-timeline")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-2",
                            "DD-8",
                            BlockType::Delay,
                            ModuleBlockSource::PresetDefault {
                                preset_id: PresetId::from(seed_id("delay-dd8")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-1",
                            "BigSky",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetDefault {
                                preset_id: PresetId::from(seed_id("reverb-bigsky")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-2",
                            "RV-6",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetDefault {
                                preset_id: PresetId::from(seed_id("reverb-rv6")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
            ])),
        ),
        vec![ModuleSnapshot::new(
            seed_id("time-parallel-ambient"),
            "Ambient",
            Module::from_chain(SignalChain::new(vec![
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-1",
                            "Timeline",
                            BlockType::Delay,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("delay-timeline")),
                                snapshot_id: SnapshotId::from(seed_id("delay-timeline-ambient")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "dly-2",
                            "DD-8",
                            BlockType::Delay,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("delay-dd8")),
                                snapshot_id: SnapshotId::from(seed_id("delay-dd8-shimmer")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
                SignalNode::Split {
                    lanes: vec![
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-1",
                            "BigSky",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("reverb-bigsky")),
                                snapshot_id: SnapshotId::from(seed_id("reverb-bigsky-ambient")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::serial(vec![ModuleBlock::new(
                            "verb-2",
                            "RV-6",
                            BlockType::Reverb,
                            ModuleBlockSource::PresetSnapshot {
                                preset_id: PresetId::from(seed_id("reverb-rv6")),
                                snapshot_id: SnapshotId::from(seed_id("reverb-rv6-modulate")),
                                saved_at_version: None,
                            },
                        )]),
                        SignalChain::new(vec![]),
                    ],
                },
            ])),
        )],
    )
}

fn guitar_source() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-source"),
        "Source",
        ModuleType::Source,
        ModuleSnapshot::new(
            seed_id("gtr-source-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "input-gate",
                    "Input Gate",
                    BlockType::Gate,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("gate-reagate")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "input-volume",
                    "Input Volume",
                    BlockType::Volume,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("volume-utility")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn guitar_dynamics() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-dynamics"),
        "Dynamics",
        ModuleType::Dynamics,
        ModuleSnapshot::new(
            seed_id("gtr-dynamics-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "compressor",
                "Compressor",
                BlockType::Compressor,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("comp-cp1x")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn guitar_special() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-special"),
        "Special",
        ModuleType::Special,
        ModuleSnapshot::new(
            seed_id("gtr-special-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "envelope-filter",
                    "Envelope Filter",
                    BlockType::Filter,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("filter-qtron")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "wah-pedal",
                    "Wah Pedal",
                    BlockType::Wah,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("wah-crybaby")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "pitch-octave-fx",
                    "Pitch Octave FX",
                    BlockType::Pitch,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("pitch-pog2")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "doubler",
                    "Doubler",
                    BlockType::Doubler,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("doubler-adt")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn guitar_volume() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-volume"),
        "Volume",
        ModuleType::Volume,
        ModuleSnapshot::new(
            seed_id("gtr-volume-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "volume-pedal",
                "Volume Pedal",
                BlockType::Volume,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("volume-pedal")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn guitar_pre_fx() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-pre-fx"),
        "Pre-FX",
        ModuleType::PreFx,
        ModuleSnapshot::new(
            seed_id("gtr-pre-fx-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "pre-eq",
                "Pre EQ",
                BlockType::Eq,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("eq-reaeq")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn guitar_amp() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-amp"),
        "Amp",
        ModuleType::Amp,
        ModuleSnapshot::new(
            seed_id("gtr-amp-default"),
            "Default",
            Module::from_chain(SignalChain::new(vec![SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![ModuleBlock::new(
                        "amp-l",
                        "Amp L",
                        BlockType::Amp,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from(seed_id("amp-twin")),
                            saved_at_version: None,
                        },
                    )]),
                    SignalChain::serial(vec![ModuleBlock::new(
                        "amp-r",
                        "Amp R",
                        BlockType::Amp,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from(seed_id("amp-ac30")),
                            saved_at_version: None,
                        },
                    )]),
                ],
            }])),
        ),
        vec![],
    )
}

fn guitar_modulation() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-modulation"),
        "Modulation",
        ModuleType::Modulation,
        ModuleSnapshot::new(
            seed_id("gtr-modulation-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "chorus",
                    "Chorus",
                    BlockType::Chorus,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("chorus-tal")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "flanger",
                    "Flanger",
                    BlockType::Flanger,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("flanger-bf3")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "phaser",
                    "Phaser",
                    BlockType::Phaser,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("phaser-phase90")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn guitar_motion() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-motion"),
        "Motion",
        ModuleType::Motion,
        ModuleSnapshot::new(
            seed_id("gtr-motion-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "tremolo",
                    "Tremolo",
                    BlockType::Tremolo,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("tremolo-classic")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "vibrato",
                    "Vibrato",
                    BlockType::Vibrato,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("vibrato-univibe")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "rotary",
                    "Rotary",
                    BlockType::Rotary,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("rotary-leslie")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn guitar_master() -> ModulePreset {
    ModulePreset::new(
        seed_id("gtr-master"),
        "Mastering",
        ModuleType::Master,
        ModuleSnapshot::new(
            seed_id("gtr-master-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "mastering-eq",
                    "Mastering EQ",
                    BlockType::Eq,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("eq-proq4")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "multiband-compressor",
                    "Multiband Compressor",
                    BlockType::Compressor,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("comp-keeley")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "limiter",
                    "Limiter",
                    BlockType::Limiter,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("limiter-prol2")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "output-volume",
                    "Output Volume",
                    BlockType::Volume,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("volume-utility")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn vocal_rescue() -> ModulePreset {
    ModulePreset::new(
        seed_id("vox-rescue"),
        "Rescue",
        ModuleType::Rescue,
        ModuleSnapshot::new(
            seed_id("vox-rescue-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "de-esser",
                    "T De-Esser 2",
                    BlockType::DeEsser,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("deesser-tde2")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "gate",
                    "renegate",
                    BlockType::Gate,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("gate-reagate")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "rescue-eq",
                    "ReaEQ",
                    BlockType::Eq,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("eq-reaeq")),
                        snapshot_id: SnapshotId::from(seed_id("eq-reaeq-vocal-presence")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "compressor-control",
                    "ReaComp",
                    BlockType::Compressor,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("comp-cp1x")),
                        snapshot_id: SnapshotId::from(seed_id("comp-cp1x-studio")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn vocal_correction() -> ModulePreset {
    ModulePreset::new(
        seed_id("vox-correction"),
        "Correction",
        ModuleType::Correction,
        ModuleSnapshot::new(
            seed_id("vox-correction-default"),
            "Default",
            Module::from_blocks(vec![ModuleBlock::new(
                "tuner",
                "Graillon 3",
                BlockType::Tuner,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from(seed_id("tuner-graillon3")),
                    saved_at_version: None,
                },
            )]),
        ),
        vec![],
    )
}

fn vocal_tonal() -> ModulePreset {
    ModulePreset::new(
        seed_id("vox-tonal"),
        "Tonal",
        ModuleType::Tonal,
        ModuleSnapshot::new(
            seed_id("vox-tonal-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "compressor-style",
                    "ReaComp Style",
                    BlockType::Compressor,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("comp-cp1x")),
                        snapshot_id: SnapshotId::from(seed_id("comp-cp1x-acoustic")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "tonal-eq",
                    "ReaEQ",
                    BlockType::Eq,
                    ModuleBlockSource::PresetSnapshot {
                        preset_id: PresetId::from(seed_id("eq-reaeq")),
                        snapshot_id: SnapshotId::from(seed_id("eq-reaeq-vocal-presence")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "saturator",
                    "T Saturator",
                    BlockType::Saturator,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("saturator-tsat")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn vocal_modulation() -> ModulePreset {
    ModulePreset::new(
        seed_id("vox-modulation"),
        "Modulation",
        ModuleType::VocalModulation,
        ModuleSnapshot::new(
            seed_id("vox-modulation-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "chorus",
                    "TAL-Chorus",
                    BlockType::Chorus,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("chorus-tal")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "flanger",
                    "Flanger",
                    BlockType::Flanger,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("flanger-bf3")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}

fn vocal_time() -> ModulePreset {
    ModulePreset::new(
        seed_id("vox-time"),
        "Time",
        ModuleType::Time,
        ModuleSnapshot::new(
            seed_id("vox-time-default"),
            "Default",
            Module::from_blocks(vec![
                ModuleBlock::new(
                    "delay",
                    "Delay",
                    BlockType::Delay,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("delay-readelay")),
                        saved_at_version: None,
                    },
                ),
                ModuleBlock::new(
                    "reverb",
                    "Reverb",
                    BlockType::Reverb,
                    ModuleBlockSource::PresetDefault {
                        preset_id: PresetId::from(seed_id("reverb-rv6")),
                        saved_at_version: None,
                    },
                ),
            ]),
        ),
        vec![],
    )
}
