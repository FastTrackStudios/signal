//! The mix's processing, from the FTS processors built in: what a
//! prepared song is mixed with in this window, no plugin loaded.
//!
//! - Every recorded track gets a **Tone** chain — FTS EQ, Comp, Saturate,
//!   the three the strip's Tone rack draws — bypassed: a channel ready to
//!   mix, costing nothing until it is switched on.
//! - Every bus gets FTS EQ and Comp, bypassed.
//! - The drum Process folder's parallel compressors get FTS Comp set
//!   from gentle to crushed, its rooms FTS Reverb set from a small room
//!   to an arena: on, since a muted track's chain is not run
//!   (`daw_standalone` render), and they are muted until picked.
//! - A drum piece keyed from its trigger (`key_drum_gates`, a send onto
//!   its channels 3/4) gets FTS Gate, on and keyed from that sidechain,
//!   ahead of its Tone chain: the threshold read off the trigger's own
//!   hits ([`gate_threshold`]).
//! - The golden template's returns — the kit's Verb banks, the vocals'
//!   Vox FX delays, verbs, widener and octaves — their FX set, on
//!   ([`return_fx`]); its one-note `Fund` and `Sub` tracks a gate, a
//!   band-pass and saturation, bypassed.
//! - The instrument folders and the folders directly in them get the
//!   bus chain, bypassed, as the golden mix's do.
//! - A vocal recorded on two channels is a dual-capsule mic's front and
//!   rear capsules (a Sphere-style mic, left front, right rear): FTS Mic
//!   goes first on it, on, making the pair the modelled mic again — one
//!   signal, mono — before its Tone chain.
//!
//! A track that already has FX keeps them and gets nothing added.

#![cfg(feature = "native")]

use daw::service::{
    Effects, FxChainContext, FxRef, FxTarget, Items as _, ProjectContext, SetParameterRequest,
    Takes as _, Tracks,
};

/// The Tone chain, in order.
const TONE: &[&str] = &["FTS EQ", "FTS Comp", "FTS Saturate"];

/// What a dual-capsule recording is made one mic by.
const MIC: &str = "FTS Mic";

/// A bus's chain.
const BUS: &[&str] = &["FTS EQ", "FTS Comp"];

/// The FX and settings for one of the golden template's returns, by
/// the folder it sits in and its name: a Verb bank's (`Room`, `Plate`,
/// `Hall`, `Nonlin`, …) and the vocals' `Vox FX`. All wet — they are fed
/// by sends. Every time is one the plugin takes itself: a tail in seconds
/// or a note value, a delay's note value, read against the song's tempo
/// by the plugin (the drum pieces' own returns are [`drum_return`]).
fn return_fx(parent: &str, name: &str) -> Option<(&'static str, Vec<(&'static str, String)>)> {
    let set = |pairs: &[(&'static str, &str)]| {
        pairs
            .iter()
            .map(|(p, v)| (*p, (*v).to_owned()))
            .collect::<Vec<_>>()
    };
    let delay = |note: &str, feedback: &str| {
        set(&[
            ("Time Sync", "Sync"),
            ("Time L Div", note),
            ("Link", "On"),
            ("Feedback", feedback),
            ("Mix", "100%"),
        ])
    };
    Some(match (parent, name) {
        // A room is a size: its time whatever the tempo.
        ("verb", "room") => (
            "FTS Reverb",
            set(&[
                ("Space", "Room"),
                ("Size", "25%"),
                ("Decay Mode", "Time"),
                ("Decay Time", "0.40"),
                ("Mix", "100%"),
            ]),
        ),
        // A plate a bar long, gone by the next.
        ("verb", "plate") => (
            "FTS Reverb",
            set(&[
                ("Space", "Plate"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/1"),
                ("Mix", "100%"),
            ]),
        ),
        ("verb", "hall") => (
            "FTS Reverb",
            set(&[
                ("Space", "Concert Hall"),
                ("Decay Mode", "Time"),
                ("Decay Time", "2.50"),
                ("Mix", "100%"),
            ]),
        ),
        // Gated: a quarter note, shut on the beat.
        ("verb", "nonlin") => (
            "FTS Reverb",
            set(&[
                ("Space", "Non-Linear"),
                ("Character A", "35%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/4"),
                ("Mix", "100%"),
            ]),
        ),
        // The vocal's short verb: a half note, under the next line.
        ("verb", "short") => (
            "FTS Reverb",
            set(&[
                ("Space", "Plate"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/2"),
                ("Mix", "100%"),
            ]),
        ),
        // The long one: behind the voice, a sixteenth of pre-delay so the
        // words land first.
        ("verb", "long") => (
            "FTS Reverb",
            set(&[
                ("Space", "Concert Hall"),
                ("Decay Mode", "Time"),
                ("Decay Time", "3.50"),
                ("Pre-Delay Sync", "Sync"),
                ("Pre-Delay Div", "1/16"),
                ("Mix", "100%"),
            ]),
        ),
        ("verb", "moment") => (
            "FTS Reverb",
            set(&[
                ("Space", "Bloom"),
                ("Decay Mode", "Time"),
                ("Decay Time", "5.00"),
                ("Mix", "100%"),
            ]),
        ),
        ("verb", "throw") => (
            "FTS Reverb",
            set(&[
                ("Space", "Cathedral"),
                ("Decay Mode", "Time"),
                ("Decay Time", "8.00"),
                ("Mix", "100%"),
            ]),
        ),
        // A slap is a room's first reflection, not a note value.
        ("delay", "slap") => (
            "FTS Delay",
            set(&[
                ("Time L", "110.0 ms"),
                ("Link", "On"),
                ("Feedback", "10%"),
                ("Mix", "100%"),
            ]),
        ),
        ("delay", "short") => ("FTS Delay", delay("1/8", "20%")),
        ("delay", "long") => ("FTS Delay", delay("1/4D", "35%")),
        ("delay", "throw") => ("FTS Delay", delay("1/2", "50%")),
        ("vox fx", "wide") => (
            "FTS Unison",
            set(&[
                ("Voices", "2"),
                ("Spread", "100%"),
                ("Dry", "0%"),
                ("Voices Mix", "100%"),
            ]),
        ),
        ("pitch", "oct+") => ("FTS Pitch", set(&[("Semitones", "12"), ("Mix", "100%")])),
        ("pitch", "oct-") => ("FTS Pitch", set(&[("Semitones", "-12"), ("Mix", "100%")])),
        _ => return None,
    })
}

/// A one-note track's chain (the template's `Fund` and `Sub`), in Dan
/// Worrall's order: the band-pass rings, the gate after it tightens the
/// ring into a note on every hit, the compressor evens the notes.
const ONE_NOTE: &[&str] = &["FTS EQ", "FTS Gate", "FTS Comp", "FTS Saturate"];

/// A drum piece's reverb return — `Kick/Verb/Room`, `Snare/Verb/Plate`,
/// `Toms/Verb/Nonlin` — by its piece and name: its FTS Reverb settings, in
/// the plugin's own words, and where its fader starts (dB).
///
/// Every tail is a time the reverb itself takes — seconds (`Decay Mode`
/// Time) or a note value it reads against the song's tempo (Sync) — and
/// every pre-delay too; nothing is worked out here. The returns are fed
/// post-fader at unity from the piece's `Sum` and the reverb is all wet:
/// the fader is the blend, and it starts far down — felt more than heard,
/// as the golden mix has them (its kick room at −22.8 dB, its snare verb
/// at −19.9).
///
/// - **Kick**: a small room, a quarter of a second whatever the tempo (a
///   room is a size), no pre-delay (the room is the kick's own) and no low
///   cut — a kick verb lengthens the low end, which is the point (the
///   golden mix's Valhalla Room has none).
/// - **Snare**: the plate a half note, gone before the next backbeat, a
///   thirty-second of pre-delay so the crack lands before the bloom; a
///   250 Hz low cut (no mud from the drum's body) and a 7 kHz high cut (no
///   hi-hat bleed splashing). The hall a bar.
/// - **Toms**: a room a quarter note long, lower than the snare's, a
///   sixty-fourth of pre-delay, cut at 150 Hz and 6 kHz (a tom's ring is
///   low; its mic hears the cymbals).
/// - **Non-Linear**, gated (Character A is its Shape; 25–50 % is Gate):
///   each hit opens a window that shuts on time — the snare's a quarter
///   note, the classic gated snare; the toms' an eighth, so a fill does
///   not smear.
///
/// Post-EQ bands are numbered (`Post Shape 6`): the first the low cut,
/// the last the high cut.
fn drum_return(piece: &str, name: &str) -> Option<(&'static [(&'static str, &'static str)], f64)> {
    Some(match (piece, name) {
        ("kick", "room") => (
            &[
                ("Space", "Room"),
                ("Mix", "100%"),
                ("Size", "15%"),
                ("Decay Mode", "Time"),
                ("Decay Time", "0.25"),
                ("Pre-Delay Sync", "Free"),
                ("Pre-Delay", "0.0 ms"),
                ("Bass", "0.80"),
            ],
            -23.0,
        ),
        ("snare", "plate") => (
            &[
                ("Space", "Plate"),
                ("Mix", "100%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/2"),
                ("Pre-Delay Sync", "Sync"),
                ("Pre-Delay Div", "1/32"),
                ("Post Shape 1", "Low Cut"),
                ("Post Freq 1", "250.0 Hz"),
                ("Post Shape 6", "High Cut"),
                ("Post Freq 6", "7.0 kHz"),
            ],
            -20.0,
        ),
        ("snare", "hall") => (
            &[
                ("Space", "Concert Hall"),
                ("Mix", "100%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/1"),
                ("Pre-Delay Sync", "Sync"),
                ("Pre-Delay Div", "1/32"),
                ("Post Shape 1", "Low Cut"),
                ("Post Freq 1", "250.0 Hz"),
                ("Post Shape 6", "High Cut"),
                ("Post Freq 6", "7.0 kHz"),
            ],
            -20.0,
        ),
        ("snare", "nonlin") => (
            &[
                ("Space", "Non-Linear"),
                ("Mix", "100%"),
                ("Character A", "35%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/4"),
                ("Pre-Delay Sync", "Free"),
                ("Pre-Delay", "0.0 ms"),
                ("Post Shape 1", "Low Cut"),
                ("Post Freq 1", "250.0 Hz"),
                ("Post Shape 6", "High Cut"),
                ("Post Freq 6", "7.0 kHz"),
            ],
            -20.0,
        ),
        ("toms", "room") => (
            &[
                ("Space", "Room"),
                ("Mix", "100%"),
                ("Size", "35%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/4"),
                ("Pre-Delay Sync", "Sync"),
                ("Pre-Delay Div", "1/64"),
                ("Post Shape 1", "Low Cut"),
                ("Post Freq 1", "150.0 Hz"),
                ("Post Shape 6", "High Cut"),
                ("Post Freq 6", "6.0 kHz"),
            ],
            -26.0,
        ),
        ("toms", "nonlin") => (
            &[
                ("Space", "Non-Linear"),
                ("Mix", "100%"),
                ("Character A", "35%"),
                ("Decay Mode", "Sync"),
                ("Decay Div", "1/8"),
                ("Pre-Delay Sync", "Free"),
                ("Pre-Delay", "0.0 ms"),
                ("Post Shape 1", "Low Cut"),
                ("Post Freq 1", "150.0 Hz"),
                ("Post Shape 6", "High Cut"),
                ("Post Freq 6", "6.0 kHz"),
            ],
            -26.0,
        ),
        _ => return None,
    })
}

/// The drum piece a return sits under (`Drums/Snare/Verb/Plate` →
/// `snare`) and its name, lowercase, when it is one.
fn drum_return_of(lower: &[String]) -> Option<(&str, &str)> {
    let [.., piece, verb, name] = lower else {
        return None;
    };
    (verb == "verb" && lower.iter().all(|p| p != "process"))
        .then_some((piece.as_str(), name.as_str()))
}

/// Set the drum pieces' reverb returns ([`drum_return`]): each one's FTS
/// Reverb (added if it has none) and its fader. `dry`, only say. One line
/// a return; a setting the reverb does not know, a line under it.
///
/// # Errors
///
/// A backend call failed.
pub fn drum_returns(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    dry: bool,
) -> eyre::Result<Vec<String>> {
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut said = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        let path = crate::gain_stage::path_of(track, &by_guid);
        let lower: Vec<String> = path.iter().map(|p| p.trim().to_lowercase()).collect();
        let Some((settings, fader_db)) =
            drum_return_of(&lower).and_then(|(piece, name)| drum_return(piece, name))
        else {
            continue;
        };
        let word = |p: &str| {
            settings
                .iter()
                .find(|(k, _)| *k == p)
                .map_or("", |(_, v)| *v)
        };
        let tail = match word("Decay Mode") {
            "Sync" => format!("{} note", word("Decay Div")),
            "Time" => format!("{} s", word("Decay Time")),
            _ => format!("Decay {}", word("Decay")),
        };
        said.push(format!(
            "{}: {} {tail}, {fader_db:.0} dB",
            path.join("/"),
            word("Space")
        ));
        if dry {
            continue;
        }
        let chain = FxChainContext::Track(track.guid.clone());
        let mut unknown = Vec::new();
        match Effects::list(daw, project.clone(), chain.clone())
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case("FTS Reverb"))
        {
            Some(fx) => set(
                daw,
                project,
                &chain,
                &fx.guid,
                "FTS Reverb",
                settings,
                &mut unknown,
            )?,
            None => set_up(daw, project, &chain, "FTS Reverb", settings, &mut unknown)?,
        }
        if !unknown.is_empty() {
            said.push(format!("  not known: {}", unknown.join(", ")));
        }
        Tracks::set_volume(
            daw,
            project.clone(),
            daw::service::TrackRef::Guid(track.guid.clone()),
            10_f64.powf(fader_db / 20.0),
        )?;
    }
    Ok(said)
}

/// Where a one-note track's fader starts, dB: the band-pass's auto gain
/// brings it back to the mics' level, all of it at one frequency — a
/// blend under them, ridden from here in context.
const ONE_NOTE_FADER_DB: f64 = -12.0;

/// The gate on a keyed drum piece, before its threshold: Kick and Toms
/// shut all the way, Snare to 20 dB under — its room stays in the
/// gaps.
fn gate_settings(piece: &str) -> &'static [(&'static str, &'static str)] {
    if piece.eq_ignore_ascii_case("snare") {
        &[
            ("Key", "Sidechain"),
            ("Range", "20.0 dB"),
            ("Release", "150.0 ms"),
        ]
    } else {
        &[("Key", "Sidechain")]
    }
}

/// Each parallel compressor's settings, in the plugin's own words.
const COMPRESS: &[Preset] = &[
    (
        "Dry",
        &[
            ("Style", "Clean"),
            ("Ratio", "2.0:1"),
            ("Threshold", "-10.0 dB"),
            ("Attack", "30.0 ms"),
            ("Release", "150 ms"),
        ],
    ),
    (
        "Tight",
        &[
            ("Style", "VCA"),
            ("Ratio", "4.0:1"),
            ("Threshold", "-25.0 dB"),
            ("Attack", "1.0 ms"),
            ("Release", "50 ms"),
        ],
    ),
    (
        "Punch",
        &[
            ("Style", "FET"),
            ("Ratio", "4.0:1"),
            ("Threshold", "-25.0 dB"),
            ("Attack", "10.0 ms"),
            ("Release", "80 ms"),
        ],
    ),
    (
        "Smash",
        &[
            ("Style", "FET"),
            ("Ratio", "20.0:1"),
            ("Threshold", "-40.0 dB"),
            ("Attack", "0.5 ms"),
            ("Release", "100 ms"),
            ("Drive", "30%"),
        ],
    ),
    (
        "Crunch",
        &[
            ("Style", "FET"),
            ("Ratio", "20.0:1"),
            ("Threshold", "-50.0 dB"),
            ("Attack", "0.1 ms"),
            ("Release", "60 ms"),
            ("Drive", "60%"),
            ("Character", "Clip"),
        ],
    ),
];

/// Each room's settings: fed by a send, so all wet. Every tail a time the
/// reverb takes itself — a room's in seconds (a room is a size, whatever
/// the song), a plate's and a gate's in notes against the song's tempo.
const ROOMS: &[Preset] = &[
    (
        "Room Sim",
        &[
            ("Space", "Room"),
            ("Size", "20%"),
            ("Decay Mode", "Time"),
            ("Decay Time", "0.30"),
            ("Mix", "100%"),
        ],
    ),
    (
        "Wood Room",
        &[
            ("Space", "Chamber"),
            ("Size", "35%"),
            ("Decay Mode", "Time"),
            ("Decay Time", "0.60"),
            ("Damping", "50%"),
            ("Mix", "100%"),
        ],
    ),
    (
        "Music Club",
        &[
            ("Space", "Studio"),
            ("Size", "50%"),
            ("Decay Mode", "Time"),
            ("Decay Time", "0.90"),
            ("Mix", "100%"),
        ],
    ),
    (
        "Stadium",
        &[
            ("Space", "Arena"),
            ("Size", "95%"),
            ("Decay Mode", "Time"),
            ("Decay Time", "4.00"),
            ("Mix", "100%"),
        ],
    ),
    (
        "RMX 16",
        &[
            ("Space", "Digital Plate"),
            ("Decay Mode", "Sync"),
            ("Decay Div", "1/2"),
            ("Mix", "100%"),
        ],
    ),
    (
        "Nonlin",
        &[
            ("Space", "Non-Linear"),
            ("Character A", "35%"),
            ("Decay Mode", "Sync"),
            ("Decay Div", "1/4"),
            ("Mix", "100%"),
        ],
    ),
    (
        "Brick Wall",
        &[
            ("Space", "Plate"),
            ("Decay Mode", "Time"),
            ("Decay Time", "1.80"),
            ("Mix", "100%"),
        ],
    ),
];

/// A preset table's settings for the track `name`.
fn settings_of(
    table: &'static [Preset],
    name: &str,
) -> Option<&'static [(&'static str, &'static str)]> {
    table.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// A track's name and its FX's settings, in the plugin's own words.
type Preset = (&'static str, &'static [(&'static str, &'static str)]);

/// What [`add`] put on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chains {
    pub tone: usize,
    pub buses: usize,
    pub process: usize,
    /// Drum pieces gated from their trigger: name and threshold, dBFS —
    /// `None` for a trigger with nothing on it to key from, whose gate is
    /// left bypassed.
    pub gates: Vec<(String, Option<f64>)>,
    /// Returns given their FX.
    pub returns: usize,
    /// One-note tracks: name, piece, and the fundamental tuned to (Hz) —
    /// `None` left bypassed, nothing to tune to.
    pub one_notes: Vec<(String, String, Option<f64>)>,
    /// Settings a plugin did not know (`fx param=text`), said, not fatal.
    pub unknown: Vec<String>,
    /// Vocals recorded as a capsule pair, FTS Mic first on each.
    pub mics: Vec<String>,
}

/// Put the mix's chains on `project` (see the module docs).
///
/// # Errors
///
/// A backend call failed.
pub fn add(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
) -> eyre::Result<Chains> {
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut done = Chains::default();
    for track in &tracks {
        let chain = FxChainContext::Track(track.guid.clone());
        if Effects::count(daw, project.clone(), chain.clone()) > 0 {
            continue;
        }
        let path = crate::gain_stage::path_of(track, &by_guid);
        let lower: Vec<String> = path.iter().map(|p| p.trim().to_lowercase()).collect();
        let under = |what: &str| lower.iter().rev().skip(1).any(|p| p == what);
        let name = track.name.trim();
        // The template's one-note tracks: a band-pass at the drum's
        // fundamental, found off its hits.
        if !track.is_folder && dynamic_template::apply::is_one_note(name) {
            one_note(daw, project, &chain, track, &tracks, &mut done)?;
            continue;
        }
        // A drum piece keyed from its trigger: the gate, then its tone.
        let keys: Vec<String> = daw_proto::Routing::receives(
            daw,
            project.clone(),
            daw::service::TrackRef::Guid(track.guid.clone()),
        )
        .into_iter()
        .filter(|r| r.dest_channels.start_channel >= 2)
        // A receive's far end — the track feeding it — is its
        // `dest_track_guid`; its `source_track_guid` is this track.
        .filter_map(|r| r.dest_track_guid)
        .collect();
        if !keys.is_empty() {
            let threshold = gate_threshold(daw, project, &tracks, &keys);
            let gate = format!("{:.1} dB", threshold.unwrap_or(-40.0));
            let mut settings: Vec<(&str, &str)> = gate_settings(name).to_vec();
            settings.push(("Threshold", gate.as_str()));
            set_up(
                daw,
                project,
                &chain,
                "FTS Gate",
                &settings,
                &mut done.unknown,
            )?;
            if threshold.is_none() {
                // A dead trigger opens the gate on its own noise, or never.
                Effects::set_enabled(
                    daw,
                    project.clone(),
                    FxTarget {
                        context: chain.clone(),
                        fx: FxRef::Index(0),
                    },
                    false,
                )?;
            }
            bypassed(daw, project, &chain, TONE)?;
            done.gates.push((name.to_owned(), threshold));
            continue;
        }
        if !track.is_folder
            && drum_return_of(&lower).is_some_and(|(p, n)| drum_return(p, n).is_some())
        {
            // Set after the loop, fader and all.
            continue;
        }
        let parent = lower.iter().rev().nth(1).map_or("", String::as_str);
        let in_process = under(&dynamic_template::apply::PROCESS_FOLDER.to_lowercase());
        if !in_process
            && !track.is_folder
            && let Some((fx, settings)) = return_fx(parent, &name.to_lowercase())
        {
            let settings: Vec<(&str, &str)> =
                settings.iter().map(|(p, v)| (*p, v.as_str())).collect();
            set_up(daw, project, &chain, fx, &settings, &mut done.unknown)?;
            done.returns += 1;
            continue;
        }
        if under(&dynamic_template::apply::PROCESS_FOLDER.to_lowercase()) {
            let (fx, settings) = match (settings_of(COMPRESS, name), settings_of(ROOMS, name)) {
                (Some(s), _) => ("FTS Comp", s),
                (_, Some(s)) => ("FTS Reverb", s),
                _ => continue,
            };
            let Some(guid) = Effects::add(daw, project.clone(), chain.clone(), fx) else {
                continue;
            };
            let target = FxTarget {
                context: chain.clone(),
                fx: FxRef::Guid(guid),
            };
            for (param, text) in settings {
                match builtin_fx::setting(fx, param, text) {
                    Some((index, value)) => Effects::set_parameter(
                        daw,
                        project.clone(),
                        SetParameterRequest {
                            target: target.clone(),
                            index,
                            value,
                        },
                    )?,
                    None => done.unknown.push(format!("{fx} {param}={text}")),
                }
            }
            done.process += 1;
            continue;
        }
        let referenced = lower.iter().any(|p| {
            p == &dynamic_template::apply::REFERENCES_FOLDER.to_lowercase()
                || p == "guide"
                || p == "mix bus"
        });
        let bus = dynamic_template::apply::is_bus_track(name);
        let recorded = !track.is_folder
            && !daw
                .get_items(
                    project.clone(),
                    daw::service::TrackRef::Guid(track.guid.clone()),
                )
                .is_empty();
        // An instrument folder, or a folder directly in one: Drums, Bass,
        // Guitars' Clean and Rhythm, the vocals' Main — not the kit's own
        // wiring, the returns' folder or a trigger.
        let group = track.is_folder
            && !referenced
            && path.len() <= 2
            && !dynamic_template::apply::is_trigger(name)
            && ![
                "drum kit",
                "process",
                "fx",
                "sum",
                "rehearsal",
                "vox fx",
                "delay",
                "verb",
                "pitch",
            ]
            .iter()
            .any(|w| name.eq_ignore_ascii_case(w));
        let names = if bus || group {
            BUS
        } else if recorded && !referenced && !dynamic_template::apply::is_trigger(name) {
            // A trigger is a key, not a channel: it has no tone.
            TONE
        } else {
            continue;
        };
        if names == TONE && lower.iter().any(|p| p == "vocals") && capsule_pair(daw, project, track)
        {
            set_up(daw, project, &chain, MIC, &[], &mut done.unknown)?;
            done.mics.push(name.to_owned());
        }
        bypassed(daw, project, &chain, names)?;
        if bus || group {
            done.buses += 1;
        } else {
            done.tone += 1;
        }
    }
    let returns = drum_returns(daw, project, false)?;
    done.returns += returns.iter().filter(|l| !l.starts_with(' ')).count();
    done.unknown.extend(
        returns
            .iter()
            .filter(|l| l.starts_with(' '))
            .map(|l| l.trim().to_owned()),
    );
    Ok(done)
}

/// Whether `track` was recorded on a dual-capsule mic: every recording
/// it plays from has two channels (front and rear).
fn capsule_pair(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> bool {
    let paths: std::collections::BTreeSet<String> = daw
        .get_items(
            project.clone(),
            daw::service::TrackRef::Guid(track.guid.clone()),
        )
        .iter()
        .filter_map(|i| {
            daw.get_active_take(project.clone(), daw::service::ItemRef::Guid(i.guid.clone()))
        })
        .filter_map(|t| t.source_file_path)
        .collect();
    !paths.is_empty() && paths.iter().all(|p| wav_channels(p) == Some(2))
}

/// A WAV file's channel count, read off its `fmt ` chunk.
fn wav_channels(path: &str) -> Option<u16> {
    use std::io::Read as _;
    let mut head = vec![0_u8; 4_096];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    channels_in(head.get(..n)?)
}

/// The channel count in the head of a RIFF / RF64 WAVE file.
fn channels_in(head: &[u8]) -> Option<u16> {
    let riff = head.get(..4)?;
    if (riff != b"RIFF" && riff != b"RF64") || head.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut at = 12_usize;
    let field = |at: usize, from: usize, len: usize| {
        let from = at.checked_add(from)?;
        head.get(from..from.checked_add(len)?)
    };
    while let Some(id) = head.get(at..at.checked_add(4)?) {
        let size = u32::from_le_bytes(field(at, 4, 4)?.try_into().ok()?);
        if id == b"fmt " {
            return Some(u16::from_le_bytes(field(at, 10, 2)?.try_into().ok()?));
        }
        // Chunks are padded to an even size.
        let size = usize::try_from(size).ok()?;
        at = at
            .checked_add(8)?
            .checked_add(size)?
            .checked_add(size % 2)?;
    }
    None
}

/// `names` on `chain`, each added bypassed: ready, costing nothing.
fn bypassed(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    chain: &FxChainContext,
    names: &[&str],
) -> eyre::Result<()> {
    for fx in names {
        let Some(guid) = Effects::add(daw, project.clone(), chain.clone(), fx) else {
            continue;
        };
        Effects::set_enabled(
            daw,
            project.clone(),
            FxTarget {
                context: chain.clone(),
                fx: FxRef::Guid(guid),
            },
            false,
        )?;
    }
    Ok(())
}

/// `fx` added to `chain`, on, with `settings` in the plugin's own words;
/// a setting it does not know goes to `unknown`.
fn set_up(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    chain: &FxChainContext,
    fx: &str,
    settings: &[(&str, &str)],
    unknown: &mut Vec<String>,
) -> eyre::Result<()> {
    let Some(guid) = Effects::add(daw, project.clone(), chain.clone(), fx) else {
        return Ok(());
    };
    set(daw, project, chain, &guid, fx, settings, unknown)
}

/// `settings` on the FX `guid`, in the plugin's own words. A name ending
/// in a band's number (`Post Shape 6`) is that band's copy of the
/// parameter: the bands repeat their parameters in order, so the n-th
/// is `(n − 1) × band width` past the first.
fn set(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    chain: &FxChainContext,
    guid: &str,
    fx: &str,
    settings: &[(&str, &str)],
    unknown: &mut Vec<String>,
) -> eyre::Result<()> {
    /// The reverb's post-EQ: four parameters a band.
    const POST_BAND_WIDTH: u32 = 4;
    let target = FxTarget {
        context: chain.clone(),
        fx: FxRef::Guid(guid.to_owned()),
    };
    for (param, text) in settings {
        let numbered = param
            .rsplit_once(' ')
            .and_then(|(base, n)| Some((base, n.parse::<u32>().ok()?)));
        let (base, band) = match numbered {
            Some((base, n)) if base.starts_with("Post ") => (base, n.saturating_sub(1)),
            _ => (*param, 0),
        };
        match builtin_fx::setting(fx, base, text) {
            Some((index, value)) => Effects::set_parameter(
                daw,
                project.clone(),
                SetParameterRequest {
                    target: target.clone(),
                    index: index + band * POST_BAND_WIDTH,
                    value,
                },
            )?,
            None => unknown.push(format!("{fx} {param}={text}")),
        }
    }
    Ok(())
}

/// How far under a trigger's strong hits its gate opens: far enough for
/// the soft ones — a ghost note, a flam's grace — to open it too.
const GATE_UNDER_HITS_DB: f64 = 24.0;

/// How far over the trigger's floor the gate stays shut.
const GATE_OVER_FLOOR_DB: f64 = 6.0;

/// A trigger whose strong hits are under this, dBFS, is no trigger: a
/// struck piezo is hot, and this is crosstalk on a dead one or a drum
/// never hit (unbreakable's T4, at -61).
const DEAD_TRIGGER_DB: f64 = -45.0;

/// The threshold for a gate keyed from `keys` (trigger tracks, or a
/// folder of them), read off the trigger's own recording — offline, the
/// whole song at once: [`GATE_UNDER_HITS_DB`] under the strong hits of
/// the quietest trigger played (a folder of tom triggers opens for every
/// tom; one that far under the loudest is a tom not played), and at
/// least [`GATE_OVER_FLOOR_DB`] over the noise between them.
/// `None` when no trigger has audio, or only under [`DEAD_TRIGGER_DB`].
fn gate_threshold(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    tracks: &[daw_proto::Track],
    keys: &[String],
) -> Option<f64> {
    use daw::service::{ItemRef, Peaks as _, TakeRef};
    let sources = tracks.iter().filter(|t| {
        !t.is_folder
            && (keys.contains(&t.guid) || t.parent_guid.as_ref().is_some_and(|p| keys.contains(p)))
    });
    let mut levels: Vec<(f64, f64)> = Vec::new();
    for track in sources {
        let mut points: Vec<f64> = Vec::new();
        let mut step = 0.0;
        for item in daw.get_items(
            project.clone(),
            daw::service::TrackRef::Guid(track.guid.clone()),
        ) {
            if !daw_ui::studio::project::on_playing_lane(track, &item) {
                continue;
            }
            let peaks = daw.take_peaks(
                project.clone(),
                ItemRef::Guid(item.guid.clone()),
                TakeRef::Active,
                crate::midi::ANALYSIS_BLOCK,
            );
            if let Some(wave) = crate::midi::Wave::from_peaks(&peaks) {
                step = wave.step;
                points.extend(
                    wave.points
                        .iter()
                        .map(|(high, low)| f64::from(high.abs().max(low.abs())) * item.volume),
                );
            }
        }
        let Some(hits) = crate::gain_stage::level(&points, step) else {
            continue;
        };
        levels.push((hits, floor_db(&points)));
    }
    // A trigger far under the loudest of its set is a drum not played —
    // or a dead trigger — reading crosstalk: it would open the gate on
    // everything else's.
    let loudest = levels
        .iter()
        .map(|(h, _)| *h)
        .fold(f64::NEG_INFINITY, f64::max);
    let (hits, floor) = levels
        .iter()
        .copied()
        .filter(|(h, _)| *h >= loudest - GATE_UNDER_HITS_DB && *h >= DEAD_TRIGGER_DB)
        .reduce(|(h, f), (h2, f2)| (h.min(h2), f.max(f2)))?;
    Some(
        (hits - GATE_UNDER_HITS_DB)
            .max(floor + GATE_OVER_FLOOR_DB)
            .min(hits - GATE_OVER_FLOOR_DB),
    )
}

/// A recording's floor, dBFS: the 20th percentile of its peaks — the
/// gaps between hits, which a drum trigger is mostly made of.
fn floor_db(points: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = points.iter().map(|p| p.abs()).collect();
    sorted.sort_by(f64::total_cmp);
    let at = sorted.len().saturating_sub(1) / 5;
    sorted
        .get(at)
        .filter(|p| **p > 0.0)
        .map_or(-120.0, |p| 20.0 * p.log10())
}

/// A one-note track (`Fund`, the kick's `Sub`) dialled in to its drum —
/// Dan Worrall's parallel band-pass:
///
/// - **FTS EQ**: one band-pass at the drum's fundamental, found off its
///   hits ([`crate::fundamental`]), 12 dB/oct (steeper rings in odd
///   ways), every other band off, auto gain on so the Q can go wide or
///   narrow without the level jumping. The kick's is wider — a sub kick's
///   weight; the snare's and toms' narrow, so they ring.
/// - **FTS Gate**, keyed from the drum's trigger: the ring tightened into
///   one solid note per hit. A tom's lets go slower than the snare's.
/// - **FTS Comp**, on for a tom — they vary in tone more than the rest.
/// - **FTS Saturate**, there, bypassed.
///
/// With no trigger or no fundamental to tune to, the chain goes on
/// bypassed, as the template's: a slot ready, costing nothing.
fn one_note(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    chain: &FxChainContext,
    track: &daw_proto::Track,
    tracks: &[daw_proto::Track],
    done: &mut Chains,
) -> eyre::Result<()> {
    let receives = daw_proto::Routing::receives(
        daw,
        project.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
    );
    let (keys, feeds): (Vec<_>, Vec<_>) = receives
        .into_iter()
        .partition(|r| r.dest_channels.start_channel >= 2);
    let keys: Vec<String> = keys.into_iter().filter_map(|r| r.dest_track_guid).collect();
    // What feeds it — mics, or the kick's Sum — as the mics.
    let mut sources: Vec<String> = Vec::new();
    for feed in feeds.into_iter().filter_map(|r| r.dest_track_guid) {
        match tracks.iter().find(|t| t.guid == feed) {
            Some(t) if t.is_folder => sources.extend(
                tracks
                    .iter()
                    .filter(|c| {
                        !c.is_folder
                            && c.parent_guid.as_deref() == Some(feed.as_str())
                            && !dynamic_template::apply::is_trigger(&c.name)
                            && !dynamic_template::apply::is_one_note(&c.name)
                    })
                    .map(|c| c.guid.clone()),
            ),
            Some(_) => sources.push(feed),
            None => {}
        }
    }
    // Which drum: the piece it is in (a Fund in a Sum is the piece's).
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let piece = crate::gain_stage::path_of(track, &by_guid)
        .iter()
        .rev()
        .skip(1)
        .find(|p| !p.trim().eq_ignore_ascii_case("sum"))
        .map(|p| p.trim().to_lowercase())
        .unwrap_or_default();
    let (range, q, release) =
        if track.name.trim().eq_ignore_ascii_case("sub") || piece.contains("kick") {
            (crate::fundamental::KICK, "2.00", "200.0 ms")
        } else if piece.contains("snare") {
            (crate::fundamental::SNARE, "8.00", "150.0 ms")
        } else {
            (crate::fundamental::TOM, "8.00", "300.0 ms")
        };
    let tom = range == crate::fundamental::TOM;
    let tuned = keys
        .first()
        .and_then(|trigger| crate::fundamental::find(daw, project, trigger, &sources, range));
    let threshold = gate_threshold(daw, project, tracks, &keys);
    let (Some(hz), Some(threshold)) = (tuned, threshold) else {
        bypassed(daw, project, chain, ONE_NOTE)?;
        done.one_notes.push((track.name.clone(), piece, None));
        return Ok(());
    };
    // The band: B1 a band-pass at the note, every band after it off.
    let mut eq: Vec<(String, String)> = vec![
        ("Auto Gain".into(), "1".into()),
        ("B1 On".into(), "On".into()),
        ("B1 Type".into(), "Bandpass".into()),
        ("B1 Freq".into(), format!("{hz:.1} Hz")),
        ("B1 Q".into(), q.into()),
        ("B1 Slope".into(), "12 dB/oct".into()),
    ];
    for n in 2_u32.. {
        let on = format!("B{n} On");
        if builtin_fx::setting("FTS EQ", &on, "Off").is_none() {
            break;
        }
        eq.push((on, "Off".into()));
    }
    let eq: Vec<(&str, &str)> = eq.iter().map(|(p, v)| (p.as_str(), v.as_str())).collect();
    set_up(daw, project, chain, "FTS EQ", &eq, &mut done.unknown)?;
    let threshold = format!("{threshold:.1} dB");
    set_up(
        daw,
        project,
        chain,
        "FTS Gate",
        &[
            ("Key", "Sidechain"),
            ("Threshold", threshold.as_str()),
            ("Attack", "0.50 ms"),
            ("Hold", "10.0 ms"),
            ("Release", release),
        ],
        &mut done.unknown,
    )?;
    set_up(
        daw,
        project,
        chain,
        "FTS Comp",
        &[
            ("Style", "Clean"),
            ("Ratio", "3.0:1"),
            ("Threshold", "-24.0 dB"),
            ("Attack", "10.0 ms"),
            ("Release", "150 ms"),
        ],
        &mut done.unknown,
    )?;
    if !tom {
        Effects::set_enabled(
            daw,
            project.clone(),
            FxTarget {
                context: chain.clone(),
                fx: FxRef::Index(2),
            },
            false,
        )?;
    }
    bypassed(daw, project, chain, &["FTS Saturate"])?;
    Tracks::set_volume(
        daw,
        project.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
        10_f64.powf(ONE_NOTE_FADER_DB / 20.0),
    )?;
    done.one_notes.push((track.name.clone(), piece, Some(hz)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV's channels are read off its `fmt ` chunk, past any chunk
    /// before it; anything else is no WAV.
    #[test]
    fn a_wav_says_its_channels() {
        let mut head = b"RIFF\0\0\0\0WAVE".to_vec();
        // A chunk before the format, odd-sized, padded.
        head.extend_from_slice(b"bext\x03\0\0\0abc\0");
        head.extend_from_slice(b"fmt \x10\0\0\0\x01\0\x02\0");
        head.extend_from_slice(&[0; 12]);
        assert_eq!(super::channels_in(&head), Some(2));
        head[22 + 12] = 1;
        assert_eq!(super::channels_in(&head), Some(1));
        assert_eq!(super::channels_in(b"OggS not a wave"), None);
    }

    /// Every return's settings are ones its plugin knows — the reverbs'
    /// times, the delays' note values, the numbered post-EQ bands.
    #[test]
    fn returns_speak_their_plugins_words() {
        let known = |fx: &str, settings: &[(&str, &str)], what: &str| {
            for (param, text) in settings {
                let base = param
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .trim_end();
                assert!(
                    builtin_fx::setting(fx, base, text).is_some(),
                    "{what}: {param}={text}"
                );
            }
        };
        for (piece, name) in [
            ("kick", "room"),
            ("snare", "plate"),
            ("snare", "hall"),
            ("snare", "nonlin"),
            ("toms", "room"),
            ("toms", "nonlin"),
        ] {
            let (settings, fader) = drum_return(piece, name).expect("a drum return");
            assert!(
                (-30.0..=-15.0).contains(&fader),
                "{piece} {name} starts down"
            );
            known("FTS Reverb", settings, &format!("{piece} {name}"));
        }
        assert!(drum_return("bass", "room").is_none());
        // The check refuses what a plugin does not know.
        assert!(builtin_fx::setting("FTS Reverb", "Decay Div", "1/3").is_none());
        for (name, settings) in ROOMS.iter().chain(COMPRESS) {
            let fx = if ROOMS.iter().any(|(n, _)| n == name) {
                "FTS Reverb"
            } else {
                "FTS Comp"
            };
            known(fx, settings, name);
        }
        for (parent, name) in [
            ("verb", "room"),
            ("verb", "plate"),
            ("verb", "hall"),
            ("verb", "short"),
            ("verb", "long"),
            ("verb", "moment"),
            ("verb", "throw"),
            ("delay", "short"),
            ("delay", "long"),
            ("delay", "throw"),
        ] {
            let (fx, settings) = return_fx(parent, name).expect("a return");
            let settings: Vec<(&str, &str)> =
                settings.iter().map(|(p, v)| (*p, v.as_str())).collect();
            known(fx, &settings, &format!("{parent} {name}"));
        }
    }

    #[test]
    fn a_drum_return_is_a_pieces_verb_outside_process() {
        let path = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            drum_return_of(&path(&["drums", "snare", "verb", "plate"])),
            Some(("snare", "plate"))
        );
        assert_eq!(
            drum_return_of(&path(&["drums", "process", "fx", "verb", "nonlin"])),
            None
        );
        assert_eq!(drum_return_of(&path(&["drums", "kick", "sum", "in"])), None);
    }
}
