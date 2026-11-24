//! The drum kit: the mix method's structure for a kit, as a preset.
//!
//! **The tree** is Studio's golden session, track for track, as the
//! Crescendum "Unbreakable" session has it (its Sum folders, trigger-keyed
//! gates, stacked verbs, muted Process folder) and Studio names it: each
//! lane is named by its Studio track path (`Drums/Kick/Sum/In`), and in
//! the per-track mode the Signal on that track runs it. One lane is
//! Signal's alone: `Drums/Kit`, the sum the Process folder is fed from (in
//! Studio, the host's sends do that).
//!
//! **Every block has its mix phase** ([`Phase`]) and runs in phase order;
//! each phase is made gain neutral by measuring it
//! ([`apply_neutral_trims`](crate::graph::apply_neutral_trims)), so the
//! kit is as loud with its processing on as bypassed and the faders do
//! the balance. What a track gets follows the method:
//!
//! - **A mic or a sample** (`Drums/Kick/Sum/In`, `Drums/Toms/Tom 1/T1`): Rescue and Tone
//!   only.
//! - **A piece's folder** (`Drums/Kick`, `Drums/Toms/Tom 1`): the trigger-keyed gate
//!   (Rescue), then its Tone: compression, EQ, saturation.
//! - **The one-note tracks** (`Sub`, `Fund`): the parallel filters (Tone):
//!   gated by the trigger, band-passed on the drum's note, saturated.
//! - **The verbs** (Depth): each piece's own, in a folder so several are
//!   stacked (the snare's Plate, Hall and Nonlin).
//! - **Process**: the parallel compressors (Tone) and the kit's FX
//!   (Depth), fed by the kit and muted: each a mute away.
//!
//! The settings are starting points, mild on purpose: the structure is
//! the point here, and a mix is dialled in on it.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::host::AssetRef;
use crate::state::{BlockSpec, LaneSpec, MidiHitSpec, Phase, Preset, SendSpec, TriggerSpec};

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

/// A mic's chain: Rescue EQ, then Tone (compression, EQ, saturation),
/// mild until it's dialled in.
fn mic_chain(low_cut: f32) -> Vec<BlockSpec> {
    vec![
        rescue_eq(low_cut),
        comp(-18.0, 3.0, 15.0, 80.0),
        tone_eq(),
        sat(0.0),
    ]
}

/// A piece's folder chain: its trigger-keyed gate (Rescue), then Tone.
fn piece_chain(key: &str, mode: f32, length_ms: f32) -> Vec<BlockSpec> {
    vec![
        gate(key, mode, length_ms),
        comp(-18.0, 4.0, 20.0, 80.0),
        tone_eq(),
        sat(3.0),
    ]
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

/// The kit's hit notes, as Studio writes them (`DRUMS_HITS_MIDI`): General
/// MIDI's kick and snare, the toms high to low.
pub const KICK_NOTE: u8 = 36;
pub const SNARE_NOTE: u8 = 38;
pub const TOM_NOTES: [u8; 4] = [45, 47, 41, 43];

/// The track that holds every piece's hit notes, just above the kit.
pub const DRUM_HITS: &str = "Drum Hits";

/// The piece's sample, played by its hits: its trigger's, or its own note
/// on this track's MIDI (`note`), which is also published as the piece's
/// hits so its gates key on the notes.
fn sample(path: &str, key: &str, note: u8, engine: Option<&AssetRef>, to: &str) -> LaneSpec {
    let mut lane = track(path, &[], to);
    lane.midi_hits.push(MidiHitSpec {
        note,
        piece: key.into(),
    });
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

/// The Unbreakable kit on the mix method's structure, by Studio's track
/// paths.
///
/// | inputs (multi-channel) | tracks |
/// |---|---|
/// | 0, 1, 2 | `Drums/Kick/Sum/In`, `Out`, `Trig` |
/// | 3, 4, 5 | `Drums/Snare/Sum/Top`, `Bottom`, `Trig` |
/// | 6–9 | `Drums/Toms/Tom 1/T1` … `Tom 4/T4` |
/// | 10–13 | `Drums/Toms/Tom 1/T1 Trig` … `Tom 4/T4 Trig` |
/// | 14/15, 16, 17 | `Drums/Cymbals/OHs`, `Hi Hat`, `Ride` |
/// | 18/19 | `Drums/Rooms` |
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn drum_kit(pieces: &KitPieces) -> Preset {
    let process_sends = || -> Vec<SendSpec> {
        COMPRESS
            .iter()
            .map(|c| SendSpec::to(alloc::format!("Drums/Process/Compress/{c}"), 0.0))
            .chain(core::iter::once(SendSpec::to(
                "Drums/Process/FX/Room Sim",
                0.0,
            )))
            .chain(
                KIT_VERBS
                    .iter()
                    .map(|(v, _)| SendSpec::to(alloc::format!("Drums/Process/FX/Verb/{v}"), 0.0)),
            )
            .collect()
    };
    let mut lanes = vec![
        at(
            with(track("Drums", &[], ""), vec![comp(-12.0, 2.0, 30.0, 150.0)]),
            -1.3,
        ),
        // Signal's own: what Process is fed from (Studio's sends).
        LaneSpec {
            sends: process_sends(),
            ..track("Drums/Kit", &[], "Drums")
        },
        // Kick.
        with(
            track("Drums/Kick", &[], "Drums/Kit"),
            piece_chain("Drums/Kick/Sum/Trig", 0.0, 300.0),
        ),
        LaneSpec {
            sends: vec![
                SendSpec::to("Drums/Kick/Verb/Room", 0.0),
                SendSpec::to("Drums/Kick/Sub", 0.0),
            ],
            ..track("Drums/Kick/Sum", &[], "Drums/Kick")
        },
        with(
            track("Drums/Kick/Sum/In", &[0], "Drums/Kick/Sum"),
            mic_chain(30.0),
        ),
        with(
            track("Drums/Kick/Sum/Out", &[1], "Drums/Kick/Sum"),
            mic_chain(30.0),
        ),
        trig("Drums/Kick/Sum/Trig", 2),
        sample(
            "Drums/Kick/Sum/Sample",
            "Drums/Kick/Sum/Trig",
            KICK_NOTE,
            pieces.kick.as_ref(),
            "Drums/Kick/Sum",
        ),
        one_note(
            "Drums/Kick/Sub",
            "Drums/Kick/Sum/Trig",
            0.0,
            55.0,
            2.0,
            "Drums/Kick",
        ),
        track("Drums/Kick/Verb", &[], "Drums/Kick"),
        at(
            with(
                track("Drums/Kick/Verb/Room", &[], "Drums/Kick/Verb"),
                vec![verb("room")],
            ),
            -23.0,
        ),
        // Snare: three verbs stacked.
        with(
            track("Drums/Snare", &[], "Drums/Kit"),
            piece_chain("Drums/Snare/Sum/Trig", 1.0, 400.0),
        ),
        LaneSpec {
            sends: ["Plate", "Hall", "Nonlin"]
                .into_iter()
                .map(|v| SendSpec::to(alloc::format!("Drums/Snare/Verb/{v}"), 0.0))
                .collect(),
            ..track("Drums/Snare/Sum", &[], "Drums/Snare")
        },
        LaneSpec {
            sends: vec![SendSpec::to("Drums/Snare/Sum/Fund", 0.0).pre()],
            ..with(
                track("Drums/Snare/Sum/Top", &[3], "Drums/Snare/Sum"),
                mic_chain(60.0),
            )
        },
        LaneSpec {
            sends: vec![SendSpec::to("Drums/Snare/Sum/Fund", 0.0).pre()],
            ..with(
                track("Drums/Snare/Sum/Bottom", &[4], "Drums/Snare/Sum"),
                mic_chain(100.0),
            )
        },
        trig("Drums/Snare/Sum/Trig", 5),
        sample(
            "Drums/Snare/Sum/Sample",
            "Drums/Snare/Sum/Trig",
            SNARE_NOTE,
            pieces.snare.as_ref(),
            "Drums/Snare/Sum",
        ),
        one_note(
            "Drums/Snare/Sum/Fund",
            "Drums/Snare/Sum/Trig",
            1.0,
            200.0,
            8.0,
            "Drums/Snare/Sum",
        ),
        track("Drums/Snare/Verb", &[], "Drums/Snare"),
        at(
            with(
                track("Drums/Snare/Verb/Plate", &[], "Drums/Snare/Verb"),
                vec![verb("plate")],
            ),
            -20.0,
        ),
        muted(at(
            with(
                track("Drums/Snare/Verb/Hall", &[], "Drums/Snare/Verb"),
                vec![verb("hall")],
            ),
            -20.0,
        )),
        muted(at(
            with(
                track("Drums/Snare/Verb/Nonlin", &[], "Drums/Snare/Verb"),
                vec![verb("nonlinear")],
            ),
            -20.0,
        )),
        // Toms: each a folder of its mic, trigger, sample and fundamental,
        // each sending to the toms' verbs.
        track("Drums/Toms", &[], "Drums/Kit"),
        track("Drums/Toms/Verb", &[], "Drums/Toms"),
        at(
            with(
                track("Drums/Toms/Verb/Room", &[], "Drums/Toms/Verb"),
                vec![verb("room")],
            ),
            -26.0,
        ),
        muted(at(
            with(
                track("Drums/Toms/Verb/Nonlin", &[], "Drums/Toms/Verb"),
                vec![verb("nonlinear")],
            ),
            -26.0,
        )),
    ];
    let fundamentals = [140.0, 110.0, 85.0, 70.0];
    for (t, fundamental) in fundamentals.iter().enumerate() {
        let n = t + 1;
        let tom = alloc::format!("Drums/Toms/Tom {n}");
        let mic = alloc::format!("{tom}/T{n}");
        let trigger = alloc::format!("{tom}/T{n} Trig");
        let fund = alloc::format!("{tom}/Fund");
        lanes.push(LaneSpec {
            sends: vec![
                SendSpec::to("Drums/Toms/Verb/Room", 0.0),
                SendSpec::to("Drums/Toms/Verb/Nonlin", 0.0),
            ],
            ..with(
                track(&tom, &[], "Drums/Toms"),
                piece_chain(&trigger, 3.0, 500.0),
            )
        });
        lanes.push(LaneSpec {
            sends: vec![SendSpec::to(fund.clone(), 0.0).pre()],
            ..with(
                track(&mic, &[u16::try_from(6 + t).unwrap_or(0)], &tom),
                mic_chain(fundamental * 0.5),
            )
        });
        lanes.push(trig(&trigger, u16::try_from(10 + t).unwrap_or(0)));
        lanes.push(sample(
            &alloc::format!("{tom}/T{n} Sample"),
            &trigger,
            TOM_NOTES[t],
            pieces.toms.get(t).and_then(Option::as_ref),
            &tom,
        ));
        lanes.push(one_note(&fund, &trigger, 3.0, *fundamental, 8.0, &tom));
    }
    lanes.extend([
        track("Drums/Cymbals", &[], "Drums/Kit"),
        with(
            track("Drums/Cymbals/OHs", &[14, 15], "Drums/Cymbals"),
            mic_chain(150.0),
        ),
        with(
            track("Drums/Cymbals/Hi Hat", &[16], "Drums/Cymbals"),
            mic_chain(250.0),
        ),
        with(
            track("Drums/Cymbals/Ride", &[17], "Drums/Cymbals"),
            mic_chain(200.0),
        ),
        with(
            track("Drums/Rooms", &[18, 19], "Drums/Kit"),
            mic_chain(60.0),
        ),
        // Process: a mute away.
        track("Drums/Process", &[], "Drums"),
        track("Drums/Process/Compress", &[], "Drums/Process"),
        track("Drums/Process/FX", &[], "Drums/Process"),
        muted(with(
            track("Drums/Process/FX/Room Sim", &[], "Drums/Process/FX"),
            vec![verb("room").param("decay", 0.2)],
        )),
        track("Drums/Process/FX/Verb", &[], "Drums/Process/FX"),
    ]);
    let parallel = |name: &str| -> Vec<BlockSpec> {
        match name {
            "Tight" => vec![comp(-24.0, 4.0, 30.0, 100.0)],
            "Punch" => vec![comp(-30.0, 20.0, 15.0, 40.0).param("sc_hpf", 120.0)],
            "Smash" => vec![comp(-36.0, 20.0, 0.1, 60.0).param("makeup", 6.0)],
            "Crunch" => vec![comp(-30.0, 10.0, 1.0, 50.0), sat(18.0)],
            // Dry: the kit as it is, to blend against (a compressor at 1:1,
            // to dial in).
            _ => vec![comp(0.0, 1.0, 10.0, 100.0)],
        }
    };
    for c in COMPRESS {
        lanes.push(muted(with(
            track(
                &alloc::format!("Drums/Process/Compress/{c}"),
                &[],
                "Drums/Process/Compress",
            ),
            parallel(c),
        )));
    }
    for (v, algorithm) in KIT_VERBS {
        lanes.push(muted(with(
            track(
                &alloc::format!("Drums/Process/FX/Verb/{v}"),
                &[],
                "Drums/Process/FX/Verb",
            ),
            vec![verb(algorithm)],
        )));
    }
    // Every piece's hit notes, on the track above the kit: heard nowhere,
    // they key the whole kit.
    let mut hits = muted(track(DRUM_HITS, &[], ""));
    hits.midi_hits = [
        ("Drums/Kick/Sum/Trig", KICK_NOTE),
        ("Drums/Snare/Sum/Trig", SNARE_NOTE),
    ]
    .into_iter()
    .map(|(p, n)| MidiHitSpec {
        note: n,
        piece: p.into(),
    })
    .chain((1..=4).map(|n| MidiHitSpec {
        note: TOM_NOTES[n - 1],
        piece: alloc::format!("Drums/Toms/Tom {n}/T{n} Trig"),
    }))
    .collect();
    lanes.push(hits);
    Preset {
        name: String::from("Drum Kit"),
        inputs: 20,
        outputs: 2,
        lanes,
        macros: vec![],
    }
}
