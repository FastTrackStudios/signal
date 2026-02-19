//! Delay block presets — modeled after iconic delay pedals.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![
        strymon_timeline(),
        boss_dd8(),
        mxr_carbon_copy(),
        ehx_memory_man(),
        way_huge_supa_puss(),
        fabfilter_timeless(),
        readelay(),
    ]
}

// ─── Strymon Timeline ───────────────────────────────────────────
// Knobs: Time, Repeats, Mix, Filter, Grit
// The flagship multi-delay — 12 delay machines.

fn timeline_block(time: f32, repeats: f32, mix: f32, filter: f32, grit: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("time", "Time", time),
        BlockParameter::new("repeats", "Repeats", repeats),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("filter", "Filter", filter),
        BlockParameter::new("grit", "Grit", grit),
    ])
}

fn strymon_timeline() -> Preset {
    Preset::new(
        seed_id("delay-timeline"),
        "Strymon Timeline",
        BlockType::Delay,
        // Default: clean digital delay — quarter note feel, moderate repeats
        Snapshot::new(
            seed_id("delay-timeline-default"),
            "Default",
            timeline_block(0.50, 0.40, 0.35, 0.50, 0.10),
        ),
        vec![
            // Dotted eighth: the Edge / U2 rhythmic delay
            Snapshot::new(
                seed_id("delay-timeline-dotted"),
                "Dotted Eighth",
                timeline_block(0.38, 0.35, 0.40, 0.52, 0.05),
            ),
            // Tape: warm, degrading repeats with grit and filtering
            Snapshot::new(
                seed_id("delay-timeline-tape"),
                "Tape Echo",
                timeline_block(0.45, 0.45, 0.38, 0.38, 0.55),
            ),
            // Ambient: long time, high repeats, filtered wash
            Snapshot::new(
                seed_id("delay-timeline-ambient"),
                "Ambient",
                timeline_block(0.75, 0.65, 0.50, 0.40, 0.15),
            ),
            // Slapback: short time, single repeat — rockabilly
            Snapshot::new(
                seed_id("delay-timeline-slapback"),
                "Slapback",
                timeline_block(0.15, 0.15, 0.40, 0.55, 0.20),
            ),
        ],
    )
}

// ─── Boss DD-8 ──────────────────────────────────────────────────
// Knobs: E.Level, Feedback, Time
// The workhorse digital delay — 11 modes, simple controls.

fn dd8_block(level: f32, feedback: f32, time: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("level", "E.Level", level),
        BlockParameter::new("feedback", "Feedback", feedback),
        BlockParameter::new("time", "Time", time),
    ])
}

fn boss_dd8() -> Preset {
    Preset::new(
        seed_id("delay-dd8"),
        "Boss DD-8",
        BlockType::Delay,
        // Default: standard digital — clean and precise
        Snapshot::new(
            seed_id("delay-dd8-default"),
            "Default",
            dd8_block(0.45, 0.40, 0.50),
        ),
        vec![
            // Analog mode: warm, slightly filtered repeats
            Snapshot::new(
                seed_id("delay-dd8-analog"),
                "Analog",
                dd8_block(0.42, 0.45, 0.48),
            ),
            // Shimmer: long delay with pitch-shifted trails
            Snapshot::new(
                seed_id("delay-dd8-shimmer"),
                "Shimmer",
                dd8_block(0.50, 0.55, 0.65),
            ),
            // Short slapback: quick single repeat
            Snapshot::new(
                seed_id("delay-dd8-slapback"),
                "Slapback",
                dd8_block(0.48, 0.18, 0.15),
            ),
        ],
    )
}

// ─── MXR Carbon Copy ────────────────────────────────────────────
// Knobs: Delay (time), Mix, Regen (feedback)
// The classic analog delay — warm, dark, bucket-brigade tone.

fn carbon_copy_block(delay: f32, mix: f32, regen: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("delay", "Delay", delay),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("regen", "Regen", regen),
    ])
}

fn mxr_carbon_copy() -> Preset {
    Preset::new(
        seed_id("delay-carbon-copy"),
        "MXR Carbon Copy",
        BlockType::Delay,
        // Default: classic analog delay — warm, medium time
        Snapshot::new(
            seed_id("delay-carbon-copy-default"),
            "Default",
            carbon_copy_block(0.45, 0.40, 0.35),
        ),
        vec![
            // Slapback: very short, single repeat — rockabilly/country
            Snapshot::new(
                seed_id("delay-carbon-copy-slapback"),
                "Slapback",
                carbon_copy_block(0.12, 0.45, 0.20),
            ),
            // Ambient: long delay, high regen — trails that build
            Snapshot::new(
                seed_id("delay-carbon-copy-ambient"),
                "Ambient Trails",
                carbon_copy_block(0.80, 0.50, 0.65),
            ),
            // Subtle: low mix — just enough depth for rhythm parts
            Snapshot::new(
                seed_id("delay-carbon-copy-subtle"),
                "Subtle",
                carbon_copy_block(0.40, 0.22, 0.30),
            ),
            // Self-oscillation: regen cranked — experimental feedback wash
            Snapshot::new(
                seed_id("delay-carbon-copy-oscillation"),
                "Oscillation",
                carbon_copy_block(0.55, 0.55, 0.88),
            ),
        ],
    )
}

// ─── EHX Deluxe Memory Man ──────────────────────────────────────
// Knobs: Delay (time), Feedback, Blend, Depth (chorus/vibrato), Rate
// The original analog delay — bucket-brigade with built-in
// chorus and vibrato modulation.

fn memory_man_block(delay: f32, feedback: f32, blend: f32, depth: f32, rate: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("delay", "Delay", delay),
        BlockParameter::new("feedback", "Feedback", feedback),
        BlockParameter::new("blend", "Blend", blend),
        BlockParameter::new("depth", "Depth", depth),
        BlockParameter::new("rate", "Rate", rate),
    ])
}

fn ehx_memory_man() -> Preset {
    Preset::new(
        seed_id("delay-memory-man"),
        "EHX Deluxe Memory Man",
        BlockType::Delay,
        // Default: classic analog delay with subtle chorus modulation
        Snapshot::new(
            seed_id("delay-memory-man-default"),
            "Default",
            memory_man_block(0.45, 0.40, 0.50, 0.25, 0.30),
        ),
        vec![
            // Chorus delay: strong modulation, short delay — thickens sound
            Snapshot::new(
                seed_id("delay-memory-man-chorus"),
                "Chorus Delay",
                memory_man_block(0.20, 0.30, 0.55, 0.60, 0.50),
            ),
            // Vibrato: full wet, no feedback — pure pitch modulation
            Snapshot::new(
                seed_id("delay-memory-man-vibrato"),
                "Vibrato",
                memory_man_block(0.15, 0.10, 0.85, 0.70, 0.55),
            ),
            // Long echo: extended delay time, moderate feedback
            Snapshot::new(
                seed_id("delay-memory-man-long-echo"),
                "Long Echo",
                memory_man_block(0.78, 0.50, 0.45, 0.15, 0.20),
            ),
            // Edge-style: rhythmic dotted pattern with light mod
            Snapshot::new(
                seed_id("delay-memory-man-rhythmic"),
                "Rhythmic",
                memory_man_block(0.38, 0.38, 0.42, 0.20, 0.25),
            ),
        ],
    )
}

// ─── Way Huge Supa-Puss ─────────────────────────────────────────
// Knobs: Delay (time), Feedback, Mix, Tone, Speed, Depth
// Analog delay with tap tempo and deep modulation controls —
// up to 900ms of warm, dark repeats.

fn supa_puss_block(
    delay: f32,
    feedback: f32,
    mix: f32,
    tone: f32,
    speed: f32,
    depth: f32,
) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("delay", "Delay", delay),
        BlockParameter::new("feedback", "Feedback", feedback),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("speed", "Speed", speed),
        BlockParameter::new("depth", "Depth", depth),
    ])
}

fn way_huge_supa_puss() -> Preset {
    Preset::new(
        seed_id("delay-supa-puss"),
        "Way Huge Supa-Puss",
        BlockType::Delay,
        // Default: warm analog with light modulation
        Snapshot::new(
            seed_id("delay-supa-puss-default"),
            "Default",
            supa_puss_block(0.45, 0.40, 0.40, 0.50, 0.30, 0.25),
        ),
        vec![
            // Dark trails: rolled-off tone, deep feedback
            Snapshot::new(
                seed_id("delay-supa-puss-dark"),
                "Dark Trails",
                supa_puss_block(0.55, 0.58, 0.42, 0.30, 0.20, 0.15),
            ),
            // Modulated: heavy chorus-like modulation on repeats
            Snapshot::new(
                seed_id("delay-supa-puss-modulated"),
                "Modulated",
                supa_puss_block(0.42, 0.42, 0.45, 0.48, 0.65, 0.60),
            ),
            // Long ambient: near-max delay time, high feedback
            Snapshot::new(
                seed_id("delay-supa-puss-ambient"),
                "Long Ambient",
                supa_puss_block(0.85, 0.62, 0.50, 0.42, 0.25, 0.30),
            ),
        ],
    )
}

// ─── FabFilter Timeless 3 ────────────────────────────────────────
// Knobs: Time, Feedback, Mix, Tone, Stretch
// Granular tape-style delay with pitch/time stretching and a
// distinctive filter section. Tone sweeps from dark (0.0) to
// bright (1.0). Stretch controls granular time-stretching of
// the delay repeats — 0.5 = normal speed, below = slower/
// pitch-down, above = faster/pitch-up.

fn timeless_block(time: f32, feedback: f32, mix: f32, tone: f32, stretch: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("time", "Time", time),
        BlockParameter::new("feedback", "Feedback", feedback),
        BlockParameter::new("mix", "Mix", mix),
        BlockParameter::new("tone", "Tone", tone),
        BlockParameter::new("stretch", "Stretch", stretch),
    ])
}

fn fabfilter_timeless() -> Preset {
    Preset::new(
        seed_id("delay-timeless"),
        "FabFilter Timeless 3",
        BlockType::Delay,
        // Default: clean digital delay with neutral tone and no stretch
        Snapshot::new(
            seed_id("delay-timeless-default"),
            "Default",
            timeless_block(0.45, 0.38, 0.35, 0.50, 0.50),
        ),
        vec![
            // Tape warmth: filtered repeats with slight pitch drift
            Snapshot::new(
                seed_id("delay-timeless-tape"),
                "Tape Warmth",
                timeless_block(0.50, 0.45, 0.40, 0.35, 0.48),
            ),
            // Granular pad: long time, high feedback, stretched grains
            Snapshot::new(
                seed_id("delay-timeless-granular"),
                "Granular Pad",
                timeless_block(0.72, 0.62, 0.50, 0.42, 0.35),
            ),
            // Bright rhythmic: short time, bright tone, clean stretch
            Snapshot::new(
                seed_id("delay-timeless-bright"),
                "Bright Rhythmic",
                timeless_block(0.32, 0.30, 0.38, 0.72, 0.50),
            ),
            // Ambient shimmer: max stretch up, long feedback, filtered
            Snapshot::new(
                seed_id("delay-timeless-shimmer"),
                "Ambient Shimmer",
                timeless_block(0.68, 0.58, 0.48, 0.45, 0.68),
            ),
        ],
    )
}

// ─── ReaDelay ───────────────────────────────────────────────────
// Knobs: Length, Feedback, Wet
// REAPER's built-in multi-tap delay. Simple, transparent, and
// CPU-efficient. Length controls delay time, Feedback controls
// how many repeats, Wet controls the effect level.

fn readelay_block(length: f32, feedback: f32, wet: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("length", "Length", length),
        BlockParameter::new("feedback", "Feedback", feedback),
        BlockParameter::new("wet", "Wet", wet),
    ])
}

fn readelay() -> Preset {
    Preset::new(
        seed_id("delay-readelay"),
        "ReaDelay",
        BlockType::Delay,
        // Default: clean utility delay — moderate time, moderate feedback
        Snapshot::new(
            seed_id("delay-readelay-default"),
            "Default",
            readelay_block(0.45, 0.35, 0.40),
        ),
        vec![
            // Slapback: quick single repeat for doubling
            Snapshot::new(
                seed_id("delay-readelay-slapback"),
                "Slapback",
                readelay_block(0.12, 0.15, 0.45),
            ),
            // Ping-pong: medium time, moderate feedback for stereo bounce
            Snapshot::new(
                seed_id("delay-readelay-pingpong"),
                "Ping Pong",
                readelay_block(0.50, 0.45, 0.42),
            ),
            // Long trail: extended time and feedback for ambient wash
            Snapshot::new(
                seed_id("delay-readelay-trail"),
                "Long Trail",
                readelay_block(0.78, 0.60, 0.48),
            ),
        ],
    )
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 7);
    }

    #[test]
    fn all_delay_presets_are_delay_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Delay);
        }
    }

    #[test]
    fn timeline_has_5_snapshots() {
        let tl = &presets()[0];
        assert_eq!(tl.name(), "Strymon Timeline");
        assert_eq!(tl.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn dd8_has_4_snapshots() {
        let dd8 = &presets()[1];
        assert_eq!(dd8.name(), "Boss DD-8");
        assert_eq!(dd8.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn carbon_copy_has_5_snapshots() {
        let cc = &presets()[2];
        assert_eq!(cc.name(), "MXR Carbon Copy");
        assert_eq!(cc.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn memory_man_has_5_snapshots() {
        let mm = &presets()[3];
        assert_eq!(mm.name(), "EHX Deluxe Memory Man");
        assert_eq!(mm.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn supa_puss_has_4_snapshots() {
        let sp = &presets()[4];
        assert_eq!(sp.name(), "Way Huge Supa-Puss");
        assert_eq!(sp.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn timeless_has_5_snapshots() {
        let tl = &presets()[5];
        assert_eq!(tl.name(), "FabFilter Timeless 3");
        assert_eq!(tl.snapshots().len(), 5); // default + 4
    }

    #[test]
    fn readelay_has_4_snapshots() {
        let rd = &presets()[6];
        assert_eq!(rd.name(), "ReaDelay");
        assert_eq!(rd.snapshots().len(), 4); // default + 3
    }

    #[test]
    fn parameter_counts_match_pedal_knobs() {
        let presets = presets();
        // Timeline: 5 knobs (time, repeats, mix, filter, grit)
        for snap in presets[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Timeline should have 5 params"
            );
        }
        // DD-8: 3 knobs (level, feedback, time)
        for snap in presets[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "DD-8 should have 3 params"
            );
        }
        // Carbon Copy: 3 knobs (delay, mix, regen)
        for snap in presets[2].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "Carbon Copy should have 3 params"
            );
        }
        // Memory Man: 5 knobs (delay, feedback, blend, depth, rate)
        for snap in presets[3].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Memory Man should have 5 params"
            );
        }
        // Supa-Puss: 6 knobs (delay, feedback, mix, tone, speed, depth)
        for snap in presets[4].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                6,
                "Supa-Puss should have 6 params"
            );
        }
        // Timeless 3: 5 knobs (time, feedback, mix, tone, stretch)
        for snap in presets[5].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                5,
                "Timeless should have 5 params"
            );
        }
        // ReaDelay: 3 knobs (length, feedback, wet)
        for snap in presets[6].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                3,
                "ReaDelay should have 3 params"
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
    fn carbon_copy_oscillation_has_high_regen() {
        let cc = &presets()[2];
        let osc = cc
            .snapshots()
            .iter()
            .find(|s| s.name() == "Oscillation")
            .unwrap();
        let block = osc.block();
        let params = block.parameters();
        let regen = params.iter().find(|p| p.id() == "regen").unwrap();
        assert!(regen.value().get() > 0.85);
    }

    #[test]
    fn timeline_slapback_has_short_time() {
        let tl = &presets()[0];
        let slap = tl
            .snapshots()
            .iter()
            .find(|s| s.name() == "Slapback")
            .unwrap();
        let block = slap.block();
        let params = block.parameters();
        let time = params.iter().find(|p| p.id() == "time").unwrap();
        assert!(time.value().get() < 0.20);
    }

    #[test]
    fn memory_man_vibrato_has_high_depth_and_wet_blend() {
        let mm = &presets()[3];
        let vib = mm
            .snapshots()
            .iter()
            .find(|s| s.name() == "Vibrato")
            .unwrap();
        let block = vib.block();
        let params = block.parameters();
        let depth = params.iter().find(|p| p.id() == "depth").unwrap();
        let blend = params.iter().find(|p| p.id() == "blend").unwrap();
        assert!(depth.value().get() > 0.65);
        assert!(blend.value().get() > 0.80);
    }

    #[test]
    fn timeless_granular_has_slow_stretch() {
        let tl = &presets()[5];
        let gran = tl
            .snapshots()
            .iter()
            .find(|s| s.name() == "Granular Pad")
            .unwrap();
        let block = gran.block();
        let params = block.parameters();
        let stretch = params.iter().find(|p| p.id() == "stretch").unwrap();
        // Granular pad: stretch below 0.5 = slowed/pitch-down grains
        assert!(stretch.value().get() < 0.50);
    }

    #[test]
    fn readelay_slapback_has_short_length() {
        let rd = &presets()[6];
        let slap = rd
            .snapshots()
            .iter()
            .find(|s| s.name() == "Slapback")
            .unwrap();
        let block = slap.block();
        let params = block.parameters();
        let length = params.iter().find(|p| p.id() == "length").unwrap();
        assert!(length.value().get() < 0.20);
    }
}
