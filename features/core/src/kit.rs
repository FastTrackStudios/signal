//! The drum kit: the mix method's structure for a kit, as a preset.
//!
//! **The tree** is the Crescendum "Unbreakable" session's, track for track
//! (its folders, faders and sends), with the golden session's one-note
//! tracks added where it puts them (a `Sub` beside the kick's `SUM`, a
//! `Fund` in the snare's and beside each tom). Lanes are named by their
//! path in that tree (`Kick/SUM/In`), and each is a track of its own in
//! the per-track mode.
//!
//! **Every block has its mix phase** ([`Phase`]) and runs in phase order;
//! each phase is made gain neutral by measuring it
//! ([`apply_neutral_trims`](crate::graph::apply_neutral_trims)), so the
//! kit is as loud with its processing on as bypassed and the faders do
//! the balance. What a track gets follows the method:
//!
//! - **A mic or a sample** (`Kick/SUM/In`, `Toms/T1`): Rescue and Tone
//!   only.
//! - **A piece's folder** (`Kick`, `Snare`): the trigger-keyed gate
//!   (Rescue), then its Tone: compression, EQ, saturation.
//! - **The one-note tracks** (`Sub`, `Fund`): the parallel filters (Tone):
//!   gated by the trigger, band-passed on the drum's note, saturated.
//! - **The verbs** (Depth): each piece's own, in a folder so several are
//!   stacked (the snare's NonLin, Plate and Hall).
//! - **Process**: the parallel compressors (Tone) and the kit's FX
//!   (Depth), fed by the kit and muted: each a mute away.
//!
//! The settings are starting points, mild on purpose: the structure is
//! the point here, and a mix is dialled in on it.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::host::AssetRef;
use crate::state::{BlockSpec, LaneSpec, Phase, Preset, SendSpec, TriggerSpec};

/// The library pieces a kit's reinforcement plays: each a `.signalengine`
/// (a Modern and Massive 2 kick, say). A piece left out has an empty
/// sample track.
#[derive(Clone, Debug, Default)]
pub struct KitPieces {
    pub kick: Option<AssetRef>,
    pub snare: Option<AssetRef>,
    pub toms: [Option<AssetRef>; 4],
}

/// The parallel compressors and the kit's FX, as the session's `Process`
/// folder has them: fed by the whole kit, muted.
const COMPRESS: [&str; 5] = ["Dry", "Tight", "Punch", "Smash", "Crunch"];
/// The kit's verb bank, and the algorithm each opens on.
const KIT_VERBS: [(&str, &str); 6] = [
    ("Wood Room", "room"),
    ("Music Club", "room"),
    ("Stadium", "hall"),
    ("RMX 16", "random"),
    ("Nonlin", "nonlinear"),
    ("Brick Wall", "reflections"),
];

/// A track: a lane on its own port, into `to` (the outputs when empty).
fn track(path: &str, input: &[u16], to: &str) -> LaneSpec {
    LaneSpec {
        name: path.into(),
        port: Some(path.into()),
        input: input.to_vec(),
        to: (!to.is_empty()).then(|| to.into()),
        ..LaneSpec::default()
    }
}

fn at(lane: LaneSpec, gain_db: f32) -> LaneSpec {
    LaneSpec { gain_db, ..lane }
}

fn with(lane: LaneSpec, blocks: Vec<BlockSpec>) -> LaneSpec {
    LaneSpec { blocks, ..lane }
}

fn muted(lane: LaneSpec) -> LaneSpec {
    LaneSpec { mute: true, ..lane }
}

/// Rescue EQ: the corrective high-pass.
fn rescue_eq(low_cut: f32) -> BlockSpec {
    BlockSpec::signal("rescue_eq", "eq")
        .algorithm("channel")
        .param("low_cut", low_cut)
        .phase(Phase::Rescue)
}

/// The trigger-keyed gate: the drum, not its bleed (Rescue). `mode`: 0
/// kick, 1 snare top, 2 snare bottom, 3 toms.
fn gate(key: &str, mode: f32, length_ms: f32) -> BlockSpec {
    BlockSpec::signal("gate", "gate")
        .param("mode", mode)
        .param("threshold", -40.0)
        .param("length", length_ms)
        .key(key)
        .phase(Phase::Rescue)
}

/// Tone compression: threshold dB, ratio, attack and release ms.
fn comp(threshold: f32, ratio: f32, attack: f32, release: f32) -> BlockSpec {
    BlockSpec::signal("comp", "comp")
        .param("threshold", threshold)
        .param("ratio", ratio)
        .param("attack", attack)
        .param("release", release)
        .phase(Phase::Tone)
}

/// Tonal EQ, flat until it's dialled in.
fn tone_eq() -> BlockSpec {
    BlockSpec::signal("eq", "eq")
        .algorithm("parametric")
        .phase(Phase::Tone)
}

fn sat(drive: f32) -> BlockSpec {
    BlockSpec::signal("sat", "drive")
        .param("drive", drive)
        .phase(Phase::Tone)
}

fn verb(algorithm: &str) -> BlockSpec {
    BlockSpec::signal("verb", "reverb")
        .algorithm(algorithm)
        .param("mix", 1.0)
        .phase(Phase::Depth)
}

/// A trigger mic: heard nowhere; its hits play the piece's sample and key
/// its gates.
fn trig(path: &str, channel: u16) -> LaneSpec {
    muted(LaneSpec {
        trigger: Some(TriggerSpec {
            threshold_db: -30.0,
            note: None,
        }),
        ..track(path, &[channel], "")
    })
}

/// The piece's sample, played by its trigger.
fn sample(path: &str, key: &str, engine: Option<&AssetRef>, to: &str) -> LaneSpec {
    let mut lane = track(path, &[], to);
    if let Some(engine) = engine {
        lane.blocks.push(
            BlockSpec::signal("drum", "drum")
                .asset(engine.clone())
                .param("level", -6.0)
                .param("dynamics", 0.5)
                .param("velocity", 120.0)
                .key(key),
        );
    }
    lane
}

/// A one-note track (Dan Worrall's parallel filters: his over-resonant
/// band-pass, then a gate): one band-pass on the drum's note at 12 dB/oct
/// (the EQ's slope 2), as tight as `q`, then gated by the trigger so every
/// hit gets the same solid note, then saturated so it reads on small
/// speakers. Blended under the drum on its fader. `fundamental` is a
/// starting guess until it's measured off the hits.
fn one_note(path: &str, key: &str, mode: f32, fundamental: f32, q: f32, to: &str) -> LaneSpec {
    let band_pass = BlockSpec::signal("band_pass", "eq")
        .algorithm("parametric")
        .param("band.1.used", 1.0)
        .param("band.1.shape", 6.0) // Band Pass
        .param("band.1.freq", fundamental)
        .param("band.1.q", q)
        .param("band.1.slope", 2.0)
        .phase(Phase::Tone);
    at(
        with(
            track(path, &[], to),
            vec![
                band_pass,
                BlockSpec::signal("gate", "gate")
                    .param("mode", mode)
                    .param("threshold", -40.0)
                    .param("length", 250.0)
                    .key(key)
                    .phase(Phase::Tone),
                sat(6.0),
            ],
        ),
        -12.0,
    )
}

/// The Crescendum kit, on the mix method's structure.
///
/// | inputs (multi-channel) | tracks |
/// |---|---|
/// | 0, 1, 2 | `Kick/SUM/In`, `Kick/SUM/Out`, `Kick/SUM/Trig` |
/// | 3, 4, 5 | `Snare/SUM/Top`, `Snare/SUM/Bottom`, `Snare/SUM/Trig` |
/// | 6–9 | `Toms/T1`–`Toms/T4` |
/// | 10–13 | `Toms/Trig/T1 Trig`–`T4 Trig` |
/// | 14/15, 16, 17 | `Cymbals/OHs`, `Cymbals/Hi Hat`, `Cymbals/Ride` |
/// | 18/19 | `Rooms` |
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn drum_kit(pieces: &KitPieces) -> Preset {
    let mut lanes = vec![
        // The session's fader: -9 dB into the DRUM BUS.
        at(
            with(track("Drums", &[], ""), vec![comp(-12.0, 2.0, 30.0, 150.0)]),
            -9.0,
        ),
        // The kit, and what the Process folder is fed.
        LaneSpec {
            sends: COMPRESS
                .iter()
                .map(|c| SendSpec::to(alloc::format!("Process/Compress/{c}"), 0.0))
                .chain(core::iter::once(SendSpec::to("Process/FX/Room Sim", 0.0)))
                .chain(
                    KIT_VERBS
                        .iter()
                        .map(|(v, _)| SendSpec::to(alloc::format!("Process/FX/Verb/{v}"), 0.0)),
                )
                .collect(),
            ..track("Drum Kit", &[], "Drums")
        },
        // Kick: gated by its trigger, then its tone.
        with(
            track("Kick", &[], "Drum Kit"),
            vec![
                gate("Kick/SUM/Trig", 0.0, 300.0),
                comp(-18.0, 4.0, 20.0, 80.0),
                tone_eq(),
                sat(3.0),
            ],
        ),
        at(
            with(track("Kick/Verb", &[], "Kick"), vec![verb("room")]),
            -22.8,
        ),
        LaneSpec {
            sends: vec![
                SendSpec::to("Kick/Verb", 0.0),
                SendSpec::to("Kick/Sub", 0.0),
            ],
            ..track("Kick/SUM", &[], "Kick")
        },
        with(
            track("Kick/SUM/In", &[0], "Kick/SUM"),
            vec![rescue_eq(30.0), comp(-18.0, 3.0, 15.0, 80.0)],
        ),
        with(
            track("Kick/SUM/Out", &[1], "Kick/SUM"),
            vec![rescue_eq(30.0), comp(-18.0, 3.0, 15.0, 80.0)],
        ),
        trig("Kick/SUM/Trig", 2),
        at(
            sample(
                "Kick/SUM/Sample",
                "Kick/SUM/Trig",
                pieces.kick.as_ref(),
                "Kick/SUM",
            ),
            2.2,
        ),
        one_note("Kick/Sub", "Kick/SUM/Trig", 0.0, 55.0, 2.0, "Kick"),
        // Snare: gated, then its tone; three verbs stacked in its VERB.
        with(
            track("Snare", &[], "Drum Kit"),
            vec![gate("Snare/SUM/Trig", 1.0, 400.0), tone_eq(), sat(3.0)],
        ),
        at(track("Snare/VERB", &[], "Snare"), -19.9),
        with(
            track("Snare/VERB/NonLin", &[], "Snare/VERB"),
            vec![verb("nonlinear")],
        ),
        muted(with(
            track("Snare/VERB/Plate", &[], "Snare/VERB"),
            vec![verb("plate")],
        )),
        muted(with(
            track("Snare/VERB/Hall", &[], "Snare/VERB"),
            vec![verb("hall")],
        )),
        LaneSpec {
            sends: ["Snare/VERB/NonLin", "Snare/VERB/Plate", "Snare/VERB/Hall"]
                .into_iter()
                .map(|v| SendSpec::to(v, 0.0))
                .collect(),
            ..track("Snare/SUM", &[], "Snare")
        },
        LaneSpec {
            sends: vec![SendSpec::to("Snare/SUM/Fund", 0.0).pre()],
            ..with(
                track("Snare/SUM/Top", &[3], "Snare/SUM"),
                vec![rescue_eq(60.0), comp(-20.0, 3.0, 10.0, 80.0)],
            )
        },
        at(
            LaneSpec {
                sends: vec![SendSpec::to("Snare/SUM/Fund", 0.0).pre()],
                ..with(
                    track("Snare/SUM/Bottom", &[4], "Snare/SUM"),
                    vec![rescue_eq(100.0), comp(-20.0, 3.0, 10.0, 80.0)],
                )
            },
            2.7,
        ),
        sample(
            "Snare/SUM/Sample",
            "Snare/SUM/Trig",
            pieces.snare.as_ref(),
            "Snare/SUM",
        ),
        trig("Snare/SUM/Trig", 5),
        one_note(
            "Snare/SUM/Fund",
            "Snare/SUM/Trig",
            1.0,
            200.0,
            8.0,
            "Snare/SUM",
        ),
        // Toms: each mic gated by its own trigger; the samples in Trig.
        at(track("Toms", &[], "Drum Kit"), 6.7),
        at(track("Toms/Trig", &[], "Toms"), -17.4),
    ];
    let fundamentals = [140.0, 110.0, 85.0, 70.0];
    for (t, fundamental) in fundamentals.iter().enumerate() {
        let n = t + 1;
        let tom = alloc::format!("Toms/T{n}");
        let trigger = alloc::format!("Toms/Trig/T{n} Trig");
        let channel = u16::try_from(6 + t).unwrap_or(0);
        lanes.push(LaneSpec {
            sends: vec![SendSpec::to(alloc::format!("Toms/T{n} Fund"), 0.0).pre()],
            ..with(
                track(&tom, &[channel], "Toms"),
                vec![
                    gate(&trigger, 3.0, 500.0),
                    rescue_eq(fundamental * 0.5),
                    comp(-20.0, 3.0, 15.0, 120.0),
                ],
            )
        });
        lanes.push(trig(&trigger, u16::try_from(10 + t).unwrap_or(0)));
        lanes.push(sample(
            &alloc::format!("Toms/Trig/T{n} Sample"),
            &trigger,
            pieces.toms.get(t).and_then(Option::as_ref),
            "Toms/Trig",
        ));
        lanes.push(one_note(
            &alloc::format!("Toms/T{n} Fund"),
            &trigger,
            3.0,
            *fundamental,
            8.0,
            "Toms",
        ));
    }
    lanes.extend([
        track("Cymbals", &[], "Drum Kit"),
        at(
            with(
                track("Cymbals/OHs", &[14, 15], "Cymbals"),
                vec![rescue_eq(150.0)],
            ),
            6.8,
        ),
        at(
            with(
                track("Cymbals/Hi Hat", &[16], "Cymbals"),
                vec![rescue_eq(250.0)],
            ),
            12.0,
        ),
        at(
            with(
                track("Cymbals/Ride", &[17], "Cymbals"),
                vec![rescue_eq(200.0)],
            ),
            9.5,
        ),
        // The session's rooms: a crusher, its compressor switched off.
        at(
            with(
                track("Rooms", &[18, 19], "Drum Kit"),
                vec![
                    sat(9.0),
                    BlockSpec {
                        bypass: true,
                        ..comp(-24.0, 6.0, 10.0, 100.0)
                    },
                ],
            ),
            -7.5,
        ),
        // Process: a mute away.
        track("Process", &[], "Drums"),
        track("Process/Compress", &[], "Process"),
        track("Process/FX", &[], "Process"),
        muted(with(
            track("Process/FX/Room Sim", &[], "Process/FX"),
            vec![verb("room").param("decay", 0.2)],
        )),
        track("Process/FX/Verb", &[], "Process/FX"),
    ]);
    let parallel = |name: &str| -> Vec<BlockSpec> {
        let c = |t, r, a, rel| comp(t, r, a, rel);
        match name {
            "Tight" => vec![c(-24.0, 4.0, 30.0, 100.0)],
            "Punch" => vec![c(-30.0, 20.0, 15.0, 40.0).param("sc_hpf", 120.0)],
            "Smash" => vec![c(-36.0, 20.0, 0.1, 60.0).param("makeup", 6.0)],
            "Crunch" => vec![c(-30.0, 10.0, 1.0, 50.0), sat(18.0)],
            // Dry: the kit as it is, to blend against.
            _ => vec![],
        }
    };
    for c in COMPRESS {
        lanes.push(muted(with(
            track(
                &alloc::format!("Process/Compress/{c}"),
                &[],
                "Process/Compress",
            ),
            parallel(c),
        )));
    }
    for (v, algorithm) in KIT_VERBS {
        lanes.push(muted(with(
            track(
                &alloc::format!("Process/FX/Verb/{v}"),
                &[],
                "Process/FX/Verb",
            ),
            vec![verb(algorithm)],
        )));
    }
    Preset {
        name: String::from("Drum Kit"),
        inputs: 20,
        outputs: 2,
        lanes,
        macros: vec![],
    }
}
