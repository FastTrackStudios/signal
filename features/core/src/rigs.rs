//! Factory presets: what a fresh instance can start from.

use alloc::vec;
use alloc::vec::Vec;

use crate::host::AssetRef;
use crate::state::{
    BlockSpec, LaneSpec, MacroMode, MacroSpec, MacroTarget, Preset, SendSpec, TriggerSpec,
};

fn target(
    lane: &str,
    block: &str,
    param: &str,
    mode: MacroMode,
    min: f32,
    max: f32,
) -> MacroTarget {
    MacroTarget {
        lane: lane.into(),
        block: block.into(),
        param: param.into(),
        mode,
        min,
        max,
    }
}

/// A stereo mix chain: EQ, compressor, delay, reverb. Macro 1 ("Tone")
/// sets the EQ's top end; macro 2 ("Space") modulates the delay and reverb
/// sends up from where they are set.
#[must_use]
pub fn dsp_chain() -> Preset {
    Preset {
        name: "Mix Chain".into(),
        inputs: 2,
        outputs: 2,
        lanes: vec![LaneSpec {
            name: "Main".into(),
            port: None,
            input: vec![0, 1],
            output: 0,
            gain_db: 0.0,
            mute: false,
            blocks: vec![
                BlockSpec::signal("eq", "eq")
                    .algorithm("channel")
                    .param("low_cut", 60.0)
                    .param("mid_freq", 400.0)
                    .param("mid_gain", -2.0),
                BlockSpec::signal("comp", "comp")
                    .param("threshold", -20.0)
                    .param("ratio", 3.0)
                    .param("makeup", 3.0),
                BlockSpec::signal("delay", "delay")
                    .algorithm("clean")
                    .param("time", 375.0)
                    .param("mix", 0.1),
                BlockSpec::signal("reverb", "reverb")
                    .algorithm("plate")
                    .param("mix", 0.15),
            ],
            ..LaneSpec::default()
        }],
        macros: vec![
            MacroSpec {
                name: "Tone".into(),
                targets: vec![target("Main", "eq", "high_gain", MacroMode::Set, -6.0, 6.0)],
            },
            MacroSpec {
                name: "Space".into(),
                targets: vec![
                    target("Main", "delay", "mix", MacroMode::Modulate, 0.0, 0.3),
                    target("Main", "reverb", "mix", MacroMode::Modulate, 0.0, 0.4),
                ],
            },
        ],
    }
}

/// The guitar rig: mono guitar in on channel 1, stereo out. Gate, drive,
/// the amp (a NAM capture), the cab (an IR), EQ, delay, reverb. The
/// captures are private (AGENTS.md): the preset only names them.
///
/// Macro 1 ("Gain") drives the amp; macro 2 ("Ambience") brings the delay
/// and reverb up.
#[must_use]
pub fn guitar_rig(amp: AssetRef, cab: AssetRef) -> Preset {
    let blocks: Vec<BlockSpec> = vec![
        BlockSpec::signal("gate", "gate")
            .param("threshold", -60.0)
            .param("reduction", -30.0)
            .param("length", 150.0),
        BlockSpec::signal("drive", "drive")
            .param("drive", 3.0)
            .param("mix", 0.5),
        BlockSpec::signal("amp", "nam").asset(amp),
        BlockSpec::signal("cab", "cab").asset(cab),
        BlockSpec::signal("eq", "eq")
            .algorithm("channel")
            .param("low_cut", 80.0)
            .param("high_cut", 9000.0),
        BlockSpec::signal("delay", "delay")
            .algorithm("tape")
            .param("time", 420.0)
            .param("mix", 0.12),
        BlockSpec::signal("reverb", "reverb")
            .algorithm("room")
            .param("mix", 0.12),
    ];
    Preset {
        name: "Guitar Rig".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![LaneSpec {
            name: "Guitar".into(),
            port: None,
            input: vec![0],
            output: 0,
            gain_db: 0.0,
            mute: false,
            blocks,
            ..LaneSpec::default()
        }],
        macros: vec![
            MacroSpec {
                name: "Gain".into(),
                targets: vec![target(
                    "Guitar",
                    "amp",
                    "input",
                    MacroMode::Set,
                    -12.0,
                    12.0,
                )],
            },
            MacroSpec {
                name: "Ambience".into(),
                targets: vec![
                    target("Guitar", "delay", "mix", MacroMode::Modulate, 0.0, 0.25),
                    target("Guitar", "reverb", "mix", MacroMode::Modulate, 0.0, 0.3),
                ],
            },
        ],
    }
}

/// The kits' macros work up from the preset (a macro at rest changes
/// nothing): up to +12 dB, as a normalized offset on a parameter whose
/// range is 72 dB (a sample's level) or 36 dB (the drive's output).
const UP_12_DB_OF_72: f32 = 12.0 / 72.0;
const UP_12_DB_OF_36: f32 = 12.0 / 36.0;

/// The library pieces a kit's reinforcement plays: each a `.signalengine`
/// (a Modern and Massive 2 kick, say). A piece left out has no sample
/// lane's block, and says so.
#[derive(Clone, Debug, Default)]
pub struct KitPieces {
    pub kick: Option<AssetRef>,
    pub snare: Option<AssetRef>,
    pub toms: [Option<AssetRef>; 4],
}

/// A track of the kit: a lane on its own port (in the per-track mode, the
/// Signal instance on the DAW track of that name).
fn track(name: &str, input: &[u16], to: &str) -> LaneSpec {
    LaneSpec {
        name: name.into(),
        port: Some(name.into()),
        input: input.to_vec(),
        to: (!to.is_empty()).then(|| to.into()),
        ..LaneSpec::default()
    }
}

/// A trigger mic: heard nowhere, its hits play the piece's sample and key
/// its gates.
fn trig(name: &str, channel: u16) -> LaneSpec {
    LaneSpec {
        trigger: Some(TriggerSpec {
            threshold_db: -30.0,
            note: None,
        }),
        mute: true,
        ..track(name, &[channel], "")
    }
}

/// The piece's reinforcement, played by its trigger's hits.
fn sample(name: &str, key: &str, engine: Option<&AssetRef>, to: &str) -> LaneSpec {
    sample_at(name, key, engine, to, -6.0)
}

/// [`sample`], its hardest hit `level` dB under 0 dBFS.
fn sample_at(name: &str, key: &str, engine: Option<&AssetRef>, to: &str, level: f32) -> LaneSpec {
    let mut lane = track(name, &[], to);
    if let Some(engine) = engine {
        lane.blocks.push(
            BlockSpec::signal("drum", "drum")
                .asset(engine.clone())
                .param("level", level)
                // Reinforcement plays hard: half the drummer's dynamics
                // over the library's upper layers.
                .param("dynamics", 0.5)
                .param("velocity", 120.0)
                .key(key),
        );
    }
    lane
}

/// The one-note chain (the kick's `Sub`, a snare's or a tom's `Fund`):
/// the raw mics, gated by the trigger, band-passed around the drum's
/// fundamental, and driven, so the note is there without the bleed.
fn one_note(name: &str, key: &str, mode: f32, fundamental: f32, to: &str) -> LaneSpec {
    let band = |n: u8, shape: f32, freq: f32| {
        move |b: BlockSpec| {
            b.param(alloc::format!("band.{n}.used"), 1.0)
                .param(alloc::format!("band.{n}.shape"), shape)
                .param(alloc::format!("band.{n}.freq"), freq)
        }
    };
    let eq = BlockSpec::signal("band", "eq").algorithm("parametric");
    let eq = band(1, 3.0, fundamental * 0.6)(eq); // Low Cut
    let eq = band(2, 4.0, fundamental * 2.5)(eq); // High Cut
    let eq = band(3, 0.0, fundamental)(eq) // a Bell on the note
        .param("band.3.gain", 6.0)
        .param("band.3.q", 2.0);
    LaneSpec {
        blocks: vec![
            BlockSpec::signal("gate", "gate")
                .param("mode", mode)
                .param("threshold", -40.0)
                .param("length", 250.0)
                .key(key),
            eq,
            BlockSpec::signal("drive", "drive")
                .param("drive", 9.0)
                .param("level", -6.0),
        ],
        ..track(name, &[], to)
    }
}

/// A section bus: what its tracks sum to, with its own processing, its
/// sends to the parallel compressors and the verbs.
fn bus(name: &str, to: &str, blocks: Vec<BlockSpec>, sends: Vec<SendSpec>) -> LaneSpec {
    LaneSpec {
        blocks,
        sends,
        ..track(name, &[], to)
    }
}

fn channel_eq(low_cut: f32) -> BlockSpec {
    BlockSpec::signal("eq", "eq")
        .algorithm("channel")
        .param("low_cut", low_cut)
}

fn comp(threshold: f32, ratio: f32, attack: f32, release: f32) -> BlockSpec {
    BlockSpec::signal("comp", "comp")
        .param("threshold", threshold)
        .param("ratio", ratio)
        .param("attack", attack)
        .param("release", release)
}

/// A parallel compressor (the session's `Compress` folder): fed by the
/// section buses' sends, straight to the outputs.
fn parallel(name: &str, comp: BlockSpec, drive: Option<f32>) -> LaneSpec {
    let mut blocks = vec![comp];
    if let Some(d) = drive {
        blocks.push(BlockSpec::signal("drive", "drive").param("drive", d));
    }
    LaneSpec {
        blocks,
        ..track(name, &[], "")
    }
}

fn verb(name: &str, algorithm: &str, decay: f32) -> LaneSpec {
    LaneSpec {
        blocks: vec![
            BlockSpec::signal("reverb", "reverb")
                .algorithm(algorithm)
                .param("decay", decay)
                .param("mix", 1.0),
        ],
        ..track(name, &[], "")
    }
}

/// A rock kit as the Crescendum mix session has it ("Unbreakable"): every
/// mic and trigger a track, the triggers playing library reinforcement
/// and keying the one-note chains, section buses into `Drums`, parallel
/// compression and the verbs beside.
///
/// | inputs (multi-channel) | tracks |
/// |---|---|
/// | 0, 1, 2   | Kick In, Kick Out, Kick Trig |
/// | 3, 4, 5   | Snare Top, Snare Bottom, Snare Trig |
/// | 6–9       | T1–T4 |
/// | 10–13     | T1 Trig–T4 Trig |
/// | 14/15, 16, 17 | OHs, Hi Hat, Ride |
/// | 18/19     | Rooms |
///
/// In the per-track mode each lane is the Signal instance on its DAW
/// track (one session, keyed by the kit's id): the triggers' hits cross
/// to the sample and one-note tracks through the session, and the host
/// sums the tracks into its folders as the buses here would.
#[must_use]
pub fn drum_kit(pieces: &KitPieces) -> Preset {
    let to_parallel = |kick: f32| {
        vec![
            SendSpec::to("Compress Tight", kick),
            SendSpec::to("Compress Punch", kick),
            SendSpec::to("Compress Smash", kick - 6.0),
            SendSpec::to("Compress Crunch", kick - 12.0),
        ]
    };
    let mut lanes = vec![
        // Kick.
        LaneSpec {
            sends: vec![SendSpec::to("Kick Sub", 0.0).pre()],
            blocks: vec![channel_eq(30.0), comp(-18.0, 3.0, 15.0, 80.0)],
            ..track("Kick In", &[0], "Kick")
        },
        LaneSpec {
            sends: vec![SendSpec::to("Kick Sub", 0.0).pre()],
            blocks: vec![channel_eq(30.0), comp(-18.0, 3.0, 15.0, 80.0)],
            ..track("Kick Out", &[1], "Kick")
        },
        trig("Kick Trig", 2),
        sample("Kick Sample", "Kick Trig", pieces.kick.as_ref(), "Kick"),
        one_note("Kick Sub", "Kick Trig", 0.0, 55.0, "Kick"),
        bus(
            "Kick",
            "Drums",
            vec![
                channel_eq(25.0)
                    .param("low_freq", 60.0)
                    .param("low_gain", 2.0),
                comp(-16.0, 4.0, 10.0, 60.0),
                BlockSpec::signal("drive", "drive")
                    .param("drive", 4.0)
                    .param("mix", 0.5),
            ],
            {
                let mut s = to_parallel(0.0);
                s.push(SendSpec::to("Kick Verb", -18.0));
                s
            },
        ),
        verb("Kick Verb", "room", 0.3),
        // Snare.
        LaneSpec {
            sends: vec![SendSpec::to("Snare Fund", 0.0).pre()],
            blocks: vec![channel_eq(80.0), comp(-20.0, 3.0, 8.0, 100.0)],
            ..track("Snare Top", &[3], "Snare")
        },
        LaneSpec {
            blocks: vec![channel_eq(120.0)],
            ..track("Snare Bottom", &[4], "Snare")
        },
        trig("Snare Trig", 5),
        sample("Snare Sample", "Snare Trig", pieces.snare.as_ref(), "Snare"),
        one_note("Snare Fund", "Snare Trig", 1.0, 200.0, "Snare"),
        bus(
            "Snare",
            "Drums",
            vec![channel_eq(60.0), comp(-18.0, 4.0, 5.0, 80.0)],
            {
                let mut s = to_parallel(0.0);
                s.push(SendSpec::to("Snare Plate", -14.0));
                s.push(SendSpec::to("Snare Nonlin", -20.0));
                s
            },
        ),
        verb("Snare Plate", "plate", 0.5),
        verb("Snare Nonlin", "nonlinear", 0.4),
    ];
    // Toms: each close mic gated by its trigger, its sample beside.
    let fundamentals = [140.0, 110.0, 85.0, 70.0];
    for (t, fundamental) in fundamentals.iter().enumerate() {
        let n = t + 1;
        let close = alloc::format!("T{n}");
        let trigger = alloc::format!("T{n} Trig");
        let channel = u16::try_from(6 + t).unwrap_or(0);
        let trig_channel = u16::try_from(10 + t).unwrap_or(0);
        lanes.push(LaneSpec {
            blocks: vec![
                BlockSpec::signal("gate", "gate")
                    .param("mode", 3.0)
                    .param("threshold", -40.0)
                    .param("length", 400.0)
                    .key(trigger.clone()),
                channel_eq(fundamental * 0.5),
                comp(-20.0, 3.0, 10.0, 120.0),
            ],
            ..track(&close, &[channel], "Toms")
        });
        lanes.push(trig(&trigger, trig_channel));
        lanes.push(sample(
            &alloc::format!("T{n} Sample"),
            &trigger,
            pieces.toms.get(t).and_then(Option::as_ref),
            "Toms",
        ));
    }
    lanes.extend([
        bus("Toms", "Drums", vec![comp(-18.0, 3.0, 10.0, 120.0)], {
            let mut s = to_parallel(-3.0);
            s.push(SendSpec::to("Toms Room", -16.0));
            s
        }),
        verb("Toms Room", "room", 0.4),
        // Cymbals and rooms.
        LaneSpec {
            blocks: vec![channel_eq(200.0)],
            ..track("OHs", &[14, 15], "Cymbals")
        },
        LaneSpec {
            blocks: vec![channel_eq(300.0)],
            ..track("Hi Hat", &[16], "Cymbals")
        },
        LaneSpec {
            blocks: vec![channel_eq(250.0)],
            ..track("Ride", &[17], "Cymbals")
        },
        bus("Cymbals", "Drums", vec![], vec![]),
        bus(
            "Rooms",
            "Drums",
            vec![channel_eq(60.0), comp(-24.0, 6.0, 20.0, 150.0)],
            vec![],
        ),
        LaneSpec {
            ..track("Room Mics", &[18, 19], "Rooms")
        },
        // The kit, and the parallel compressors beside it.
        bus("Drums", "", vec![comp(-12.0, 2.0, 30.0, 150.0)], vec![]),
        parallel("Compress Tight", comp(-24.0, 4.0, 1.0, 50.0), None),
        parallel("Compress Punch", comp(-24.0, 6.0, 20.0, 80.0), None),
        parallel(
            "Compress Smash",
            comp(-36.0, 20.0, 0.5, 60.0).param("makeup", 6.0),
            None,
        ),
        parallel("Compress Crunch", comp(-30.0, 10.0, 1.0, 50.0), Some(12.0)),
    ]);
    Preset {
        name: "Drum Kit".into(),
        inputs: 20,
        outputs: 2,
        lanes,
        macros: vec![
            MacroSpec {
                name: "Samples".into(),
                // The pieces the kit has.
                targets: [
                    ("Kick Sample", &pieces.kick),
                    ("Snare Sample", &pieces.snare),
                ]
                .into_iter()
                .filter(|(_, piece)| piece.is_some())
                .map(|(lane, _)| {
                    target(
                        lane,
                        "drum",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_72,
                    )
                })
                .collect(),
            },
            MacroSpec {
                name: "Sub".into(),
                targets: vec![
                    target(
                        "Kick Sub",
                        "drive",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_36,
                    ),
                    target(
                        "Snare Fund",
                        "drive",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_36,
                    ),
                ],
            },
            MacroSpec {
                name: "Smash".into(),
                targets: vec![target(
                    "Compress Smash",
                    "comp",
                    "makeup",
                    MacroMode::Modulate,
                    0.0,
                    0.5,
                )],
            },
        ],
    }
}

/// The parametric EQ's shapes, as its `band.N.shape` numbers them.
mod shape {
    pub const BELL: f32 = 0.0;
    pub const LOW_SHELF: f32 = 1.0;
    pub const HIGH_SHELF: f32 = 2.0;
    pub const LOW_CUT: f32 = 3.0;
    pub const HIGH_CUT: f32 = 4.0;
}

/// A parametric EQ `id` of `(shape, Hz, dB, Q)` bands.
fn peq(id: &str, bands: &[(f32, f32, f32, f32)]) -> BlockSpec {
    let mut b = BlockSpec::signal(id, "eq").algorithm("parametric");
    for (n, (shape, freq, gain, q)) in bands.iter().enumerate() {
        let n = n + 1;
        b = b
            .param(alloc::format!("band.{n}.used"), 1.0)
            .param(alloc::format!("band.{n}.shape"), *shape)
            .param(alloc::format!("band.{n}.freq"), *freq)
            .param(alloc::format!("band.{n}.gain"), *gain)
            .param(alloc::format!("band.{n}.q"), *q);
    }
    b
}

/// A compressor `id`: threshold dB, ratio, attack and release ms.
fn comp_as(id: &str, threshold: f32, ratio: f32, attack: f32, release: f32) -> BlockSpec {
    BlockSpec::signal(id, "comp")
        .param("threshold", threshold)
        .param("ratio", ratio)
        .param("attack", attack)
        .param("release", release)
}

/// A drum gate keyed by `key`, closing all the way: `mode` the piece's
/// (0 kick, 1 snare top, 2 snare bottom, 3 toms), open `length` ms.
fn hard_gate(key: &str, mode: f32, length: f32) -> BlockSpec {
    BlockSpec::signal("gate", "gate")
        .param("mode", mode)
        .param("threshold", -40.0)
        .param("reduction", -80.0)
        .param("length", length)
        .key(key)
}

fn level(lane: LaneSpec, gain_db: f32) -> LaneSpec {
    LaneSpec { gain_db, ..lane }
}

/// The kit of [`drum_kit`], mixed the modern way (Spiritbox, mixed by
/// Zakk Cervini; Will Putney, Nolly Getgood, Mark Mynett's published
/// mixes): the samples carry the kick and half the snare, the close mics
/// hard-gated by their triggers, the rooms crushed and tucked, a Nolly
/// "punch" bus (20:1 on a high-passed send), Mynett's two-stage "smash"
/// (about 18 dB of gain reduction, no cymbals), a saturated snare, 4:1
/// glue on the kit, and Mynett's plate (850 ms) and room (1.4 s) sends.
/// The research these come from, with its sources, is the drum-mixing
/// report (`metal-drum-mixing.md`); its numbers are starting points.
///
/// Same tracks and inputs as [`drum_kit`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn drum_kit_modern(pieces: &KitPieces) -> Preset {
    use shape::{BELL, HIGH_CUT, HIGH_SHELF, LOW_CUT, LOW_SHELF};
    let shells = |punch: f32, smash: f32| {
        vec![
            SendSpec::to("Compress Tight", -6.0),
            SendSpec::to("Compress Punch", punch),
            SendSpec::to("Compress Smash", smash),
        ]
    };
    let close_comp = || comp_as("comp", -10.0, 6.0, 15.0, 80.0);
    let mut lanes = vec![
        // Kick: the sample carries it, the mics under it.
        level(
            LaneSpec {
                sends: vec![SendSpec::to("Kick Sub", 0.0).pre()],
                blocks: vec![
                    hard_gate("Kick Trig", 0.0, 180.0),
                    peq(
                        "eq",
                        &[
                            (LOW_CUT, 30.0, 0.0, 0.7),
                            (BELL, 541.0, -6.0, 1.0),
                            (BELL, 7500.0, 5.0, 0.7),
                        ],
                    ),
                    close_comp(),
                ],
                ..track("Kick In", &[0], "Kick")
            },
            -4.0,
        ),
        level(
            LaneSpec {
                sends: vec![SendSpec::to("Kick Sub", 0.0).pre()],
                blocks: vec![
                    hard_gate("Kick Trig", 0.0, 180.0),
                    // The Sub carries the lows.
                    peq(
                        "eq",
                        &[(LOW_CUT, 120.0, 0.0, 0.7), (BELL, 265.0, -6.0, 1.0)],
                    ),
                    close_comp(),
                ],
                ..track("Kick Out", &[1], "Kick")
            },
            -6.0,
        ),
        trig("Kick Trig", 2),
        LaneSpec {
            blocks: {
                let mut b = sample_at(
                    "Kick Sample",
                    "Kick Trig",
                    pieces.kick.as_ref(),
                    "Kick",
                    0.0,
                )
                .blocks;
                b.push(peq(
                    "eq",
                    &[(BELL, 400.0, -3.0, 1.0), (BELL, 3800.0, 3.0, 1.0)],
                ));
                b
            },
            ..track("Kick Sample", &[], "Kick")
        },
        level(
            one_note_q("Kick Sub", "Kick Trig", 0.0, 55.0, 3.0, "Kick"),
            0.0,
        ),
        bus(
            "Kick",
            "Drums",
            vec![
                peq(
                    "eq",
                    &[
                        (BELL, 400.0, -4.0, 0.7),
                        (LOW_SHELF, 60.0, 8.0, 0.7),
                        (BELL, 3200.0, 3.0, 1.0),
                        (HIGH_SHELF, 10_000.0, 2.0, 0.7),
                    ],
                ),
                // 4–5 dB on the kick's peaks (near 0 dBFS here).
                comp_as("comp", -6.0, 4.0, 20.0, 60.0),
            ],
            {
                let mut s = shells(0.0, 0.0);
                s.push(SendSpec::to("Drum Room", -24.0));
                s
            },
        ),
        // Snare: half sample, the mics hard-gated.
        LaneSpec {
            sends: vec![SendSpec::to("Snare Fund", 0.0).pre()],
            blocks: vec![
                hard_gate("Snare Trig", 1.0, 500.0),
                peq(
                    "eq",
                    &[
                        (LOW_CUT, 60.0, 0.0, 0.7),
                        (BELL, 220.0, 4.0, 1.0),
                        (BELL, 4500.0, -3.0, 4.0),
                    ],
                ),
                comp_as("comp", -16.0, 4.0, 10.0, 80.0),
            ],
            ..track("Snare Top", &[3], "Snare")
        },
        // The bottom's polarity isn't flipped here (Signal has no flip yet):
        // it sits low, hard-gated, as fizz.
        level(
            LaneSpec {
                blocks: vec![
                    hard_gate("Snare Trig", 2.0, 400.0),
                    peq(
                        "eq",
                        &[(LOW_CUT, 150.0, 0.0, 0.7), (HIGH_SHELF, 8000.0, 2.0, 0.7)],
                    ),
                    comp_as("comp", -18.0, 4.0, 15.0, 80.0),
                ],
                ..track("Snare Bottom", &[4], "Snare")
            },
            -10.0,
        ),
        trig("Snare Trig", 5),
        LaneSpec {
            blocks: {
                let mut b = sample_at(
                    "Snare Sample",
                    "Snare Trig",
                    pieces.snare.as_ref(),
                    "Snare",
                    -3.0,
                )
                .blocks;
                b.push(peq(
                    "eq",
                    &[(BELL, 200.0, 4.0, 1.0), (BELL, 5000.0, 2.5, 1.0)],
                ));
                b
            },
            ..track("Snare Sample", &[], "Snare")
        },
        level(
            one_note_q("Snare Fund", "Snare Trig", 1.0, 230.0, 3.0, "Snare"),
            4.0,
        ),
        bus(
            "Snare",
            "Drums",
            vec![
                // Body and crack: the references' snares carry 125–250 Hz
                // and 4–8 kHz well above ours.
                peq(
                    "eq",
                    &[
                        (BELL, 140.0, 7.0, 1.0),
                        (BELL, 220.0, 7.0, 0.8),
                        (HIGH_SHELF, 5000.0, 4.0, 0.7),
                    ],
                ),
                comp_as("comp", -10.0, 3.0, 10.0, 80.0),
            ],
            {
                let mut s = shells(0.0, 0.0);
                s.push(SendSpec::to("Compress Crunch", 0.0));
                s.push(SendSpec::to("Snare Plate", -16.0));
                s.push(SendSpec::to("Snare Nonlin", -24.0));
                s
            },
        ),
        verb_at("Snare Plate", "plate", 0.45, 10.0, &[]),
        verb_at("Snare Nonlin", "nonlinear", 0.3, 5.0, &[]),
    ];
    // Toms, floor last: Mynett's EQ, per tom.
    let toms: [(f32, f32, f32, f32, f32, f32); 4] = [
        // HPF, cut, low boost Hz and dB, high boost Hz and dB
        (60.0, 790.0, 183.0, 4.0, 4520.0, 4.0),
        (60.0, 670.0, 133.0, 4.0, 3960.0, 4.0),
        (60.0, 402.0, 115.0, 5.2, 3850.0, 6.1),
        (50.0, 534.0, 85.0, 4.5, 3330.0, 3.9),
    ];
    for (t, (hpf, cut, low, low_db, high, high_db)) in toms.iter().enumerate() {
        let n = t + 1;
        let close = alloc::format!("T{n}");
        let trigger = alloc::format!("T{n} Trig");
        let channel = u16::try_from(6 + t).unwrap_or(0);
        let trig_channel = u16::try_from(10 + t).unwrap_or(0);
        lanes.push(LaneSpec {
            blocks: vec![
                hard_gate(&trigger, 3.0, if t == 3 { 700.0 } else { 550.0 }),
                peq(
                    "eq",
                    &[
                        (LOW_CUT, *hpf, 0.0, 0.7),
                        (BELL, *cut, -6.0, 1.0),
                        (BELL, *low, *low_db, 1.0),
                        (BELL, *high, *high_db, 1.0),
                    ],
                ),
                comp_as("comp", -16.0, 6.0, 20.0, 150.0),
            ],
            ..track(&close, &[channel], "Toms")
        });
        lanes.push(trig(&trigger, trig_channel));
        lanes.push(sample_at(
            &alloc::format!("T{n} Sample"),
            &trigger,
            pieces.toms.get(t).and_then(Option::as_ref),
            "Toms",
            -8.0,
        ));
    }
    lanes.extend([
        bus(
            "Toms",
            "Drums",
            vec![peq("eq", &[(BELL, 4000.0, 3.0, 1.0)])],
            {
                let mut s = shells(-3.0, -3.0);
                s.push(SendSpec::to("Snare Plate", -18.0));
                s.push(SendSpec::to("Drum Room", -20.0));
                s
            },
        ),
        // Cymbals: high-passed out of the kit's low end; the hat tucked.
        LaneSpec {
            blocks: vec![peq(
                "eq",
                &[(LOW_CUT, 125.0, 0.0, 0.7), (HIGH_SHELF, 12_000.0, 9.0, 0.7)],
            )],
            ..track("OHs", &[14, 15], "Cymbals")
        },
        level(
            LaneSpec {
                blocks: vec![peq("eq", &[(LOW_CUT, 300.0, 0.0, 0.7)])],
                ..track("Hi Hat", &[16], "Cymbals")
            },
            -6.0,
        ),
        LaneSpec {
            blocks: vec![peq(
                "eq",
                &[(LOW_CUT, 300.0, 0.0, 0.7), (BELL, 2500.0, -2.0, 1.0)],
            )],
            ..track("Ride", &[17], "Cymbals")
        },
        bus(
            "Cymbals",
            "Drums",
            // The references' cymbals are 8–15 dB brighter.
            vec![peq("eq", &[(HIGH_SHELF, 8000.0, 8.0, 0.7)])],
            vec![SendSpec::to("Drum Room", -22.0)],
        ),
        // Rooms: crushed and tucked.
        level(
            bus(
                "Rooms",
                "Drums",
                vec![
                    peq(
                        "eq",
                        &[(LOW_CUT, 80.0, 0.0, 0.7), (HIGH_CUT, 7000.0, 0.0, 0.7)],
                    ),
                    comp_as("comp", -30.0, 20.0, 0.1, 70.0).param("makeup", 6.0),
                ],
                vec![SendSpec::to("Compress Punch", -6.0)],
            ),
            -8.0,
        ),
        LaneSpec {
            ..track("Room Mics", &[18, 19], "Rooms")
        },
        // The kit: 4:1 glue (Cervini), a touch of drive.
        bus(
            "Drums",
            "",
            vec![
                // 3–4 dB on the kit's peaks.
                comp_as("comp", -6.0, 4.0, 30.0, 100.0),
                BlockSpec::signal("drive", "drive")
                    .param("drive", 2.0)
                    .param("mix", 0.5),
            ],
            vec![],
        ),
        // The parallels.
        level(
            parallel(
                "Compress Tight",
                comp_as("comp", -20.0, 4.0, 30.0, 100.0),
                None,
            ),
            -6.0,
        ),
        level(
            LaneSpec {
                blocks: vec![
                    // The detector high-passed: the kick's lows don't pump it.
                    comp_as("comp", -32.0, 20.0, 15.0, 40.0)
                        .param("makeup", 8.0)
                        .param("sc_hpf", 120.0),
                    peq(
                        "post",
                        &[(BELL, 5500.0, 2.5, 1.0), (HIGH_SHELF, 12_000.0, 2.5, 0.7)],
                    ),
                ],
                ..track("Compress Punch", &[], "")
            },
            -6.0,
        ),
        level(
            LaneSpec {
                blocks: vec![
                    comp_as("comp", -30.0, 6.0, 0.1, 100.0),
                    comp_as("limit", -20.0, 20.0, 2.0, 50.0).param("makeup", 12.0),
                    peq(
                        "post",
                        &[(LOW_SHELF, 190.0, 3.5, 0.7), (HIGH_SHELF, 6900.0, 3.5, 0.7)],
                    ),
                ],
                ..track("Compress Smash", &[], "")
            },
            -5.0,
        ),
        level(
            LaneSpec {
                blocks: vec![
                    BlockSpec::signal("drive", "drive")
                        .param("drive", 18.0)
                        .param("level", -6.0),
                    peq(
                        "post",
                        &[(LOW_CUT, 200.0, 0.0, 0.7), (HIGH_CUT, 10_000.0, 0.0, 0.7)],
                    ),
                ],
                ..track("Compress Crunch", &[], "")
            },
            -14.0,
        ),
        // Mynett's room: the air pushed before it.
        verb_at(
            "Drum Room",
            "room",
            0.6,
            15.0,
            &[(HIGH_SHELF, 11_000.0, 7.5, 0.7)],
        ),
    ]);
    // Everything heard into the master, as the references are heard:
    // mastered, their peaks held by a true limiter (20:1, fastest attack,
    // 1 ms lookahead), about -1 dBFS out.
    for lane in &mut lanes {
        if lane.to.is_none() && !lane.mute {
            lane.to = Some("Master".into());
        }
    }
    lanes.push(LaneSpec {
        blocks: vec![
            // The references' tilt: more weight and more top than ours.
            peq(
                "eq",
                &[
                    (LOW_SHELF, 70.0, 3.0, 0.7),
                    (BELL, 3500.0, 2.0, 0.8),
                    (HIGH_SHELF, 9000.0, 4.0, 0.7),
                ],
            ),
            comp_as("limit", -8.0, 20.0, 0.1, 50.0)
                .param("lookahead", 1.0)
                .param("makeup", 6.0),
        ],
        ..track("Master", &[], "")
    });
    // Modern reinforcement is near-constant: the samples follow the
    // drummer's dynamics a little (the kick least), from their upper layers.
    for lane in &mut lanes {
        let (dynamics, velocity) = match lane.name.as_str() {
            "Kick Sample" => (0.15, 124.0),
            "Snare Sample" => (0.3, 122.0),
            n if n.ends_with(" Sample") => (0.4, 118.0),
            _ => continue,
        };
        for b in lane.blocks.iter_mut().filter(|b| b.id == "drum") {
            b.params.insert("dynamics".into(), dynamics);
            b.params.insert("velocity".into(), velocity);
        }
    }
    Preset {
        name: "Drum Kit (Modern)".into(),
        inputs: 20,
        outputs: 2,
        lanes,
        macros: vec![
            MacroSpec {
                name: "Samples".into(),
                targets: [
                    ("Kick Sample", &pieces.kick),
                    ("Snare Sample", &pieces.snare),
                ]
                .into_iter()
                .filter(|(_, piece)| piece.is_some())
                .map(|(lane, _)| {
                    target(
                        lane,
                        "drum",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_72,
                    )
                })
                .collect(),
            },
            MacroSpec {
                name: "Sub".into(),
                targets: vec![
                    target(
                        "Kick Sub",
                        "drive",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_36,
                    ),
                    target(
                        "Snare Fund",
                        "drive",
                        "level",
                        MacroMode::Modulate,
                        0.0,
                        UP_12_DB_OF_36,
                    ),
                ],
            },
            MacroSpec {
                name: "Smash".into(),
                targets: vec![target(
                    "Compress Smash",
                    "limit",
                    "makeup",
                    MacroMode::Modulate,
                    0.0,
                    0.5,
                )],
            },
        ],
    }
}

/// [`one_note`] with the band-pass's `q`.
fn one_note_q(name: &str, key: &str, mode: f32, fundamental: f32, q: f32, to: &str) -> LaneSpec {
    let mut lane = one_note(name, key, mode, fundamental, to);
    for b in &mut lane.blocks {
        if b.id == "band" {
            b.params.insert("band.3.q".into(), q);
        }
    }
    lane
}

/// A reverb lane: `decay` 0–1, `predelay` ms, an EQ before it.
fn verb_at(
    name: &str,
    algorithm: &str,
    decay: f32,
    predelay: f32,
    pre: &[(f32, f32, f32, f32)],
) -> LaneSpec {
    let mut blocks = Vec::new();
    if !pre.is_empty() {
        blocks.push(peq("pre", pre));
    }
    blocks.push(
        BlockSpec::signal("reverb", "reverb")
            .algorithm(algorithm)
            .param("decay", decay)
            .param("predelay", predelay)
            .param("mix", 1.0),
    );
    LaneSpec {
        blocks,
        ..track(name, &[], "")
    }
}
