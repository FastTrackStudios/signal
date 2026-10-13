//! The maximal-template checklist, regenerated from checks — never
//! hand-ticked.
//!
//! `docs/spec/session/maximal-template.md` lists every shape the
//! reference session has to cover. Each item here carries the doc's text
//! verbatim and, when the golden session carries that shape, a check that
//! walks the built tree and says so. [`render`] writes the checklist with
//! a tick exactly where a check exists *and passes*; [`regenerate`] swaps
//! it into the doc. A test asserts the doc equals the regeneration, so a
//! tick that nothing proves cannot ship, and `just daw-template` is how
//! the doc is brought up to date.

use std::path::{Path, PathBuf};

use super::kind::Kind;
use super::rpp::{armed, Built, Flat};
use super::shape::{maximal, GroupRole, Layout, Routing, BUS_CHAIN, ONE_NOTE_CHAIN};

/// The golden session as the checks see it: the built maximal fixture
/// and which scene renders are committed beside it.
#[derive(Debug, Clone)]
pub struct Golden {
    /// The built maximal session.
    pub built: Built,
    /// Slugs of the scene renders committed under `scenes/`.
    pub scenes: Vec<String>,
}

impl Golden {
    /// Build the maximal session and list the committed scene renders
    /// under `fixtures` (the fixtures directory).
    #[must_use]
    pub fn load(fixtures: &Path) -> Self {
        let mut scenes: Vec<String> = std::fs::read_dir(fixtures.join("scenes"))
            .map(|dir| {
                dir.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "png"))
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    .collect()
            })
            .unwrap_or_default();
        scenes.sort();
        Self {
            built: super::build(&maximal()),
            scenes,
        }
    }

    /// The track at a `/`-joined path.
    #[must_use]
    pub fn track(&self, path: &str) -> Option<&Flat> {
        self.built.tracks.iter().find(|t| t.path == path)
    }

    /// The direct children of the folder at `path`, in order.
    #[must_use]
    pub fn children(&self, path: &str) -> Vec<&Flat> {
        let prefix = format!("{path}/");
        self.built
            .tracks
            .iter()
            .filter(|t| {
                t.path
                    .strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains('/'))
            })
            .collect()
    }

    /// The names of the direct children of `path`.
    #[must_use]
    pub fn names(&self, path: &str) -> Vec<&str> {
        self.children(path)
            .into_iter()
            .map(|t| t.name.as_str())
            .collect()
    }

    /// Whether a scene render is committed.
    #[must_use]
    pub fn has_scene(&self, slug: &str) -> bool {
        self.scenes.iter().any(|s| s == slug)
    }
}

/// One line of the checklist.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    /// The `###` heading the item sits under, verbatim.
    pub section: &'static str,
    /// The item's text, verbatim, with the doc's own line breaks.
    pub text: &'static str,
    /// The check that ticks it, when the golden session carries the shape.
    pub check: Option<fn(&Golden) -> bool>,
}

fn names_eq(g: &Golden, path: &str, expected: &[&str]) -> bool {
    g.names(path) == expected
}

fn is_folder(g: &Golden, path: &str) -> bool {
    g.track(path).is_some_and(|t| t.is_folder)
}

fn is_stereo_leaf(g: &Golden, path: &str) -> bool {
    g.track(path).is_some_and(|t| t.stereo && !t.is_folder)
}

fn sends_to(g: &Golden, path: &str, bus: &str) -> bool {
    g.track(path)
        .is_some_and(|t| t.routing.send() == Some(bus) && !t.routing.parent_send())
}

/// A summed piece: its close mics under `Sum`, everything else beside it.
fn summed(g: &Golden, piece: &str, mics: &[&str], beside: &[&str]) -> bool {
    let mut expected = vec!["Sum"];
    expected.extend_from_slice(beside);
    is_folder(g, &format!("{piece}/Sum"))
        && names_eq(g, piece, &expected)
        && names_eq(g, &format!("{piece}/Sum"), mics)
}

fn fx_names(t: &Flat) -> Vec<&str> {
    t.fx.iter().map(|f| f.name).collect()
}

fn drum_pieces(g: &Golden) -> bool {
    const FAMILY: [(&str, u32); 6] = [
        ("Drums", 0x00B2_3A3F),
        ("Drums/Kick", 0x008C_2F35),
        ("Drums/Snare", 0x00C9_4540),
        ("Drums/Toms", 0x00B8_613F),
        ("Drums/Cymbals", 0x00C7_6B7A),
        ("Drums/Rooms", 0x0093_425C),
    ];
    names_eq(
        g,
        "Drums",
        &["Kick", "Snare", "Toms", "Cymbals", "Rooms", "Process"],
    ) && FAMILY.iter().all(|(path, colour)| {
        g.track(path)
            .is_some_and(|t| t.is_folder && t.colour == *colour)
    })
}

fn sums_beside_sends(g: &Golden) -> bool {
    // Every Sum is a folder of leaves; whatever the piece sends to is a
    // sibling of the Sum, never inside it.
    let sums: Vec<&Flat> = g
        .built
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::Sum)
        .collect();
    !sums.is_empty()
        && sums.iter().all(|sum| {
            let inside = g.children(&sum.path);
            inside
                .iter()
                .all(|t| !t.is_folder && t.kind != Kind::Verb && t.kind != Kind::Sub)
        })
}

fn kick(g: &Golden) -> bool {
    summed(
        g,
        "Drums/Kick",
        &["In", "Out", "Trig", "Sample"],
        &["Sub", "Verb"],
    ) && names_eq(g, "Drums/Kick/Verb", &["Room"])
}

fn snare(g: &Golden) -> bool {
    summed(
        g,
        "Drums/Snare",
        &["Top", "Bottom", "Trig", "Sample", "Fund"],
        &["Verb"],
    ) && names_eq(g, "Drums/Snare/Verb", &["Plate", "Hall", "Nonlin"])
}

fn toms(g: &Golden) -> bool {
    names_eq(
        g,
        "Drums/Toms",
        &["Tom 1", "Tom 2", "Tom 3", "Tom 4", "Verb"],
    ) && (1..=4).all(|n| {
        names_eq(
            g,
            &format!("Drums/Toms/Tom {n}"),
            &[
                &format!("T{n}"),
                &format!("T{n} Trig"),
                &format!("T{n} Sample"),
                "Fund",
            ],
        )
    }) && names_eq(g, "Drums/Toms/Verb", &["Room", "Nonlin"])
}

fn cymbals_and_rooms(g: &Golden) -> bool {
    names_eq(g, "Drums/Cymbals", &["OH", "Hi-Hat", "Ride"])
        && names_eq(g, "Drums/Rooms", &["Rooms", "Rooms Far"])
        && is_stereo_leaf(g, "Drums/Rooms/Rooms")
        && is_stereo_leaf(g, "Drums/Rooms/Rooms Far")
}

fn one_note_chains(g: &Golden) -> bool {
    let one_note: Vec<&str> = ONE_NOTE_CHAIN.iter().map(|f| f.name).collect();
    let mut seen = false;
    for t in &g.built.tracks {
        match t.kind {
            Kind::Fundamental | Kind::Sub => {
                seen = true;
                if fx_names(t) != one_note {
                    return false;
                }
            }
            Kind::Trigger => {
                if !t.fx.is_empty() {
                    return false;
                }
            }
            _ => {}
        }
    }
    seen
}

fn compress(g: &Golden) -> bool {
    g.track("Process/Compress")
        .is_some_and(|t| t.kind == Kind::Compress && t.name.starts_with("Compress"))
        && names_eq(
            g,
            "Process/Compress",
            &["Dry", "Tight", "Punch", "Smash", "Crunch"],
        )
}

fn process_fx(g: &Golden) -> bool {
    names_eq(g, "Process/FX", &["Room Sim", "Verb"])
        && names_eq(
            g,
            "Process/FX/Verb",
            &[
                "Wood Room",
                "Music Club",
                "Stadium",
                "RMX 16",
                "Nonlin",
                "Brick Wall",
            ],
        )
}

fn drums_to_drum_bus(g: &Golden) -> bool {
    // Process is in Drums: it reaches DRUM BUS with the kit, through it.
    sends_to(g, "Drums", "DRUM BUS")
        && g.track("Drums/Process")
            .is_some_and(|t| t.routing.parent_send())
}

/// Process — the folder and everything in it — is in the mixer and out of
/// the track panel: mixing, not arranging.
fn process_in_the_mixer_only(g: &Golden) -> bool {
    let process: Vec<&super::Flat> = g
        .built
        .tracks
        .iter()
        .filter(|t| t.path == "Drums/Process" || t.path.starts_with("Drums/Process/"))
        .collect();
    !process.is_empty() && process.iter().all(|t| t.mixer_only)
}

fn drum_scenes(g: &Golden) -> bool {
    [
        "drum-tracking",
        "drum-mixing",
        "drum-overview",
        "drum-advanced",
        "drum-fx",
    ]
    .iter()
    .all(|s| g.has_scene(s))
}

/// The bass guitar's DI and amp directly under it: the folder is the
/// piece and sums them itself, with no `Sum` between.
fn bass_guitar(g: &Golden) -> bool {
    is_folder(g, "Bass/Guitar")
        && names_eq(g, "Bass/Guitar", &["DI", "Amp"])
        && g.track("Bass/Guitar").is_some_and(|t| t.piece)
}

fn bass_synth(g: &Golden) -> bool {
    names_eq(g, "Bass/Synth", &["Sub", "Synth"])
}

fn bass_to_bus(g: &Golden) -> bool {
    sends_to(g, "Bass", "BASS BUS")
}

/// Guitars use no `Sum` folders, and they grow by depth rather than by
/// the kit's pattern.
///
/// The kit gathers a piece's close mics under a `Sum` because they are
/// one drum heard several ways. A guitar's depth is different in kind:
/// a double is two *performances*, an octave layer is a third, and each
/// channel of each is captured on its own sources. Folders, yes — a
/// `Sum`, never.
///
/// This used to assert that no guitar track was a folder at all and
/// that Rhythm was a stereo leaf, which held only while the fixture's
/// guitars were flat. `flow.guitars.golden` requires the opposite.
fn guitars_no_sums(g: &Golden) -> bool {
    let guitar_tracks: Vec<&Flat> = g
        .built
        .tracks
        .iter()
        .filter(|t| t.path.starts_with("Electric/") || t.path.starts_with("Acoustic/"))
        .collect();
    !guitar_tracks.is_empty()
        && guitar_tracks.iter().all(|t| t.kind != Kind::Sum)
        && names_eq(g, "Electric", &["Rhythm", "Lead", "Solo"])
}

/// The three shapes every guitar scene and gesture has to work on.
///
/// Rhythm at full depth, Lead as a DI-only double with no folder under
/// its channels, and Solo as two single tracks. Having all three in one
/// fixture is what proves `flow.guitars.same-everywhere`: the same
/// scene renders against a twenty-eight-source part and a one-track one.
fn guitars_three_shapes(g: &Golden) -> bool {
    let has = |path: &str| g.built.tracks.iter().any(|t| t.path == path);
    has("Electric/Rhythm/Main/L/Amp 1/SM57")
        && has("Electric/Rhythm/Octave/R/Amp 2/Royer")
        && has("Electric/Lead/L")
        && !g
            .built
            .tracks
            .iter()
            .any(|t| t.path.starts_with("Electric/Lead/L/"))
        && has("Electric/Solo/Main")
        && has("Electric/Solo/Harmony")
}

fn guitars_separate(g: &Golden) -> bool {
    let top = |name: &str| g.track(name).is_some_and(|t| t.depth == 0 && t.is_folder);
    top("Electric")
        && top("Acoustic")
        && g.track("Guitars").is_none()
        && g.built.tracks.iter().all(|t| t.name != "GUITAR BUS")
        && g.track("MIX BUS/INST BUS/ELECTRIC BUS").is_some()
        && g.track("MIX BUS/INST BUS/ACOUSTIC BUS").is_some()
}

fn electric_sends(g: &Golden) -> bool {
    const GTR: [&str; 3] = ["GTR RHYTHM", "GTR LEAD", "GTR SOLO"];
    let parts = g.children("Electric");
    !parts.is_empty()
        && parts.iter().all(|t| {
            t.routing
                .send()
                .is_some_and(|to| GTR.contains(&to) && !t.routing.parent_send())
        })
        && names_eq(g, "MIX BUS/INST BUS/ELECTRIC BUS", &GTR)
        && GTR
            .iter()
            .all(|b| g.track(&format!("Electric/{b}")).is_none())
}

fn folder_leads_bus(g: &Golden, folder: &str, bus: &str, group: u32) -> bool {
    g.track(folder).is_some_and(|t| {
        t.routing == Routing::DeadEnd && t.group == Some(GroupRole::VcaLead(group))
    }) && g.track(bus).is_some_and(|t| {
        t.routing.send() == g.track(folder).map(|f| f.name.as_str())
            && t.routing.parent_send()
            && t.group == Some(GroupRole::VcaFollow(group))
    }) && g.built.tracks.iter().all(|t| !t.name.contains("VCA"))
}

fn electric_bus_returns(g: &Golden) -> bool {
    folder_leads_bus(g, "Electric", "MIX BUS/INST BUS/ELECTRIC BUS", 1)
}

fn acoustic(g: &Golden) -> bool {
    names_eq(g, "Acoustic", &["Steel", "Nylon", "Nashville"])
        && g.children("Acoustic")
            .iter()
            .all(|t| t.routing.send() == Some("ACOUSTIC BUS") && !t.routing.parent_send())
        && folder_leads_bus(g, "Acoustic", "MIX BUS/INST BUS/ACOUSTIC BUS", 2)
}

fn keys(g: &Golden) -> bool {
    names_eq(g, "Keys", &["Piano", "Rhodes", "Wurli", "Organ"])
        && is_stereo_leaf(g, "Keys/Piano")
        && sends_to(g, "Keys", "KEYS BUS")
}

/// A family is the mixing level and a synth the tracking one, so the
/// families are folders and the synths belonging to none sit beside
/// them at the top of `Synths/` — reachable without inventing a family
/// to hold them.
fn synths(g: &Golden) -> bool {
    names_eq(
        g,
        "Synths",
        &[
            "Sub Bass Synth",
            "Texture",
            "SY Arps",
            "SY Pads",
            "SY Leads",
            "SY Chords",
        ],
    ) && sends_to(g, "Synths", "KEYS BUS")
}

/// Percussion is its own folder beside the drums with its own bus: it
/// is tracked one instrument at a time and comps on the track like a
/// bass, so the kit's folder comping would be the wrong gesture.
fn percussion(g: &Golden) -> bool {
    names_eq(g, "Percussion", &["Shaker", "Tambourine", "Claps"])
        && sends_to(g, "Percussion", "PERC BUS")
}

/// The orchestra's golden shape: four sections, each to its own bus.
/// Only the shape — divisi, seating and spot mics are a later effort.
fn orchestra(g: &Golden) -> bool {
    names_eq(
        g,
        "Orchestra",
        &["Winds", "Brass", "Strings", "Orch Percussion"],
    )
}

/// Hue in degrees of an `0xRRGGBB` colour.
fn hue(rgb: u32) -> f64 {
    let red = f64::from(rgb >> 16 & 0xFF) / 255.0;
    let green = f64::from(rgb >> 8 & 0xFF) / 255.0;
    let blue = f64::from(rgb & 0xFF) / 255.0;
    let brightest = red.max(green).max(blue);
    let darkest = red.min(green).min(blue);
    let span = brightest - darkest;
    if span <= f64::EPSILON {
        return 0.0;
    }
    let sixths = if (brightest - red).abs() <= f64::EPSILON {
        ((green - blue) / span).rem_euclid(6.0)
    } else if (brightest - green).abs() <= f64::EPSILON {
        (blue - red) / span + 2.0
    } else {
        (red - green) / span + 4.0
    };
    sixths * 60.0
}

fn colours_in_hue_order(g: &Golden) -> bool {
    // Electrics blue, acoustics seafoam, keys green: the session order
    // walks the hue wheel one way. Synths sit after keys and are purple —
    // the catalogue's violet family — and so is every synth under them
    // and the synth bass under Bass.
    let order = ["Electric", "Acoustic", "Keys", "Synths"];
    let tops: Vec<&Flat> = order.iter().filter_map(|n| g.track(n)).collect();
    let indices: Vec<usize> = tops
        .iter()
        .filter_map(|t| g.built.tracks.iter().position(|x| x.path == t.path))
        .collect();
    let hues: Vec<f64> = tops.iter().take(3).map(|t| hue(t.colour)).collect();
    let purple = |t: &Flat| (255.0..=300.0).contains(&hue(t.colour));
    let synths = g
        .built
        .tracks
        .iter()
        .filter(|t| t.path.starts_with("Synths") || t.path.starts_with("Bass/Synth"));
    tops.len() == order.len()
        && indices.windows(2).all(|w| w.first() < w.get(1))
        && hues
            .windows(2)
            .all(|w| w.first().zip(w.get(1)).is_some_and(|(a, b)| a > b))
        && hues.first().is_some_and(|h| (200.0..=240.0).contains(h))
        && hues.last().is_some_and(|h| (100.0..=150.0).contains(h))
        && synths.clone().count() > 2
        && synths.into_iter().all(purple)
}

/// Three leads, the BGV parts, a choir and the three language VCAs.
///
/// Each lead is a mix track with a Main and a DBL under it, and a
/// source per language THEY SING — Belen has no Portuguese and Aline
/// has only Portuguese. Absence is the representation; there is no
/// empty placeholder to mistake for a part nobody recorded.
fn vocals(g: &Golden) -> bool {
    names_eq(
        g,
        "Vocals",
        &[
            "Ron", "Belen", "Aline", "Vox FX", "VOX EN", "VOX ES", "VOX PT",
        ],
    ) // The BGVs and the choir: folders of their own beside Vocals.
        && g.track("BGVs").is_some()
        && g.track("Choir").is_some()
        && names_eq(g, "Vocals/Ron", &["Main", "DBL"])
        && names_eq(g, "Vocals/Ron/Main", &["EN", "ES", "PT"])
        && names_eq(g, "Vocals/Belen/Main", &["EN", "ES"])
        && names_eq(g, "Vocals/Aline/Main", &["PT"])
        && sends_to(g, "Vocals", "LEAD VOX BUS")
}

/// The BGVs to their own bus, and the vocal template's returns folded
/// in: `Vox FX` beside the leads with the same Delay, Verb, Wide and
/// Pitch as `vocal-fx.rpp`, and every lead's mix track sending to every
/// one of them.
fn bgvs_and_vocal_fx(g: &Golden) -> bool {
    const FX: &str = "Vocals/Vox FX";
    let returns: Vec<String> = g
        .built
        .tracks
        .iter()
        .filter(|t| t.path.starts_with(&format!("{FX}/")) && !t.is_folder)
        .map(|t| t.path.clone())
        .collect();
    let kinds = |path: &str, kind: Kind| g.children(path).iter().all(|t| t.kind == kind);
    sends_to(g, "BGVs", "BGV BUS")
        && sends_to(g, "Choir", "BGV BUS")
        && g.track(FX).is_some_and(|t| t.kind == Kind::Fx)
        && names_eq(g, FX, &["Delay", "Verb", "Wide", "Pitch"])
        && names_eq(
            g,
            &format!("{FX}/Delay"),
            &["Slap", "Short", "Long", "Throw"],
        )
        && names_eq(
            g,
            &format!("{FX}/Verb"),
            &["Room", "Short", "Long", "Moment", "Throw"],
        )
        && names_eq(g, &format!("{FX}/Pitch"), &["Oct+", "Oct-"])
        && kinds(&format!("{FX}/Delay"), Kind::Return)
        && kinds(&format!("{FX}/Verb"), Kind::Verb)
        && kinds(&format!("{FX}/Pitch"), Kind::Return)
        && g.track(&format!("{FX}/Wide"))
            .is_some_and(|t| t.kind == Kind::Return)
        && returns.len() == 12
        && ["Ron", "Belen", "Aline"].iter().all(|lead| {
            g.track(&format!("Vocals/{lead}"))
                .is_some_and(|t| t.aux == returns)
        })
}

fn bus_tree(g: &Golden) -> bool {
    names_eq(g, "MIX BUS", &["INST BUS", "VOX BUS"])
        && names_eq(
            g,
            "MIX BUS/INST BUS",
            &[
                "DRUM BUS",
                "BASS BUS",
                "ELECTRIC BUS",
                "ACOUSTIC BUS",
                "KEYS BUS",
            ],
        )
        && names_eq(
            g,
            "MIX BUS/INST BUS/ELECTRIC BUS",
            &["GTR RHYTHM", "GTR LEAD", "GTR SOLO"],
        )
        && names_eq(g, "MIX BUS/VOX BUS", &["LEAD VOX BUS", "BGV BUS"])
        && g.built.tracks.iter().all(|t| t.name != "GUITAR BUS")
        && g.track("MIX BUS").is_some_and(|t| t.depth == 0)
}

fn buses_are_buses(g: &Golden) -> bool {
    let chain: Vec<&str> = BUS_CHAIN.iter().map(|f| f.name).collect();
    let buses: Vec<&Flat> = g
        .built
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::Bus)
        .collect();
    !buses.is_empty()
        && buses
            .iter()
            .all(|t| !t.has_items() && !armed(Layout::Maximal, t) && fx_names(t) == chain)
}

fn buses_scene(g: &Golden) -> bool {
    g.has_scene("buses")
}

const DRUMS: &str = "Drums — the kit's pieces and `Process/`, in one folder";
const GUIDE: &str = "Guide and Keyflow — `Guide/`, `Keyflow/`";
const PERCUSSION: &str = "Percussion — `Percussion/`";
const BASS: &str = "Bass — `Bass/`";
const GUITARS: &str = "Guitars — `Electric/`, `Acoustic/` (a different shape every song)";
const KEYS: &str = "Keys — `Keys/`, Synths — `Synths/`";
const ORCHESTRA: &str = "Orchestra — `Orchestra/` (expanded in a later spec)";
const VOCALS: &str = "Vocals — `Vocals/`";
const BUSES: &str = "Buses — `MIX BUS/`";

/// The checklist, in the doc's order.
pub const ITEMS: &[Item] = &[
    Item {
        section: DRUMS,
        text: "Every piece a folder: **Kick, Snare, Toms, Cymbals, Rooms**, in one\nred family (folder crimson, pieces oxblood → scarlet → terracotta →\nrose → wine).",
        check: Some(drum_pieces),
    },
    Item {
        section: DRUMS,
        text: "Each piece's close mics under a **Sum**; the piece's sends beside\nthe Sum, not inside it.",
        check: Some(sums_beside_sends),
    },
    Item {
        section: DRUMS,
        text: "**Kick**: In, Out, Trig, Sample; Sub; **Verb/** Room.",
        check: Some(kick),
    },
    Item {
        section: DRUMS,
        text: "**Snare**: Top, Bottom, Trig, Sample, Fund; **Verb/** Plate, Hall, Nonlin.",
        check: Some(snare),
    },
    Item {
        section: DRUMS,
        text: "**Toms**: per tom T*n*, Trig, Sample, Fund; one shared **Verb/** Room, Nonlin.",
        check: Some(toms),
    },
    Item {
        section: DRUMS,
        text: "**Cymbals**: OH, Hi-Hat, Ride. **Rooms**: two stereo pairs, Rooms\nand Rooms Far.",
        check: Some(cymbals_and_rooms),
    },
    Item {
        section: DRUMS,
        text: "One-note tracks (Fund, Sub) get Gate → band-pass at the fundamental\n→ Sat; triggers get nothing.",
        check: Some(one_note_chains),
    },
    Item {
        section: DRUMS,
        text: "**Process/Compress/**: Dry, Tight, Punch, Smash, Crunch — a balance\ngroup (`session-daw::balance`): one fader up, the others down by the\nsame amount between them; a fader at silence is out.",
        check: Some(compress),
    },
    Item {
        section: DRUMS,
        text: "**Process/FX/**: Room Sim (captured rooms, compressed fast on the\nway back) and a **Verb** bank: Wood Room, Music Club, Stadium,\nRMX 16, Nonlin, Brick Wall.",
        check: Some(process_fx),
    },
    Item {
        section: DRUMS,
        text: "Drums — the kit and Process in it — reaches the mix by a send to\n**DRUM BUS**.",
        check: Some(drums_to_drum_bus),
    },
    Item {
        section: DRUMS,
        text: "**Process** is in the mixer and out of the track panel, all of it.",
        check: Some(process_in_the_mixer_only),
    },
    Item {
        section: DRUMS,
        text: "Scenes: Tracking, Mixing, Overview (pieces collapsed), Advanced,\nFX.",
        check: Some(drum_scenes),
    },
    Item {
        section: GUIDE,
        text: "**Guide/** Click, Guide, Shaker at the top of the session, to the\nheadphone mixes only (`flow.scenes.guide-folder`).",
        check: None,
    },
    Item {
        section: GUIDE,
        text: "**Keyflow/** KEY, CHORD, LINES, Lyrics, HITS as MIDI items\n(`flow.scenes.keyflow-folder`).",
        check: None,
    },
    Item {
        section: PERCUSSION,
        text: "Shaker, Tambourine, Claps → **PERC BUS** under INST BUS\n(`flow.percussion.folder`).",
        check: Some(percussion),
    },
    Item {
        section: BASS,
        text: "**Guitar/** DI, Amp — directly under the piece, no Sum.",
        check: Some(bass_guitar),
    },
    Item {
        section: BASS,
        text: "**Synth/** Sub, Synth.",
        check: Some(bass_synth),
    },
    Item {
        section: BASS,
        text: "To **BASS BUS** by send.",
        check: Some(bass_to_bus),
    },
    Item {
        section: BASS,
        text: "Upright / DI-only variants when a song has them.",
        check: None,
    },
    Item {
        section: GUITARS,
        text: "**No Sum folders** anywhere under the guitars: the kit's Sum\ngathers one drum heard several ways, and a guitar's depth is a\ndifferent thing — a double is two performances, an octave layer a\nthird, each channel on its own sources.",
        check: Some(guitars_no_sums),
    },
    Item {
        section: GUITARS,
        text: "The three shapes in one fixture (`flow.guitars.golden`): **Rhythm**\ndouble-tracked with two octave layers and seven sources a channel,\n**Lead** a DI-only double with no folder under its channels, and\n**Solo** a single track with a Harmony beside it.",
        check: Some(guitars_three_shapes),
    },
    Item {
        section: GUITARS,
        text: "Electrics and acoustics are **separate top-level folders with\ntheir own buses** — no Guitars folder, no GUITAR BUS.",
        check: Some(guitars_separate),
    },
    Item {
        section: GUITARS,
        text: "Every electric part goes to exactly one of **GTR RHYTHM, GTR LEAD,\nGTR SOLO** by a send — the three live under **ELECTRIC BUS** in the\nbus list, not in the folder. A part moves between them mid-song by\nautomating the sends. Stems are the three buses.",
        check: Some(electric_sends),
    },
    Item {
        section: GUITARS,
        text: "**ELECTRIC BUS sends back into the Electric folder**, which is a\ndead end that meters the electrics after their bus, and is the\n**VCA lead of ELECTRIC BUS** (group 1, with mute and solo): the\nfolder's fader is the electrics' fader. No VCA track.",
        check: Some(electric_bus_returns),
    },
    Item {
        section: GUITARS,
        text: "**Acoustic/** Steel, Nylon, Nashville (the high-strung layer over\nthe steel) → **ACOUSTIC BUS**, which returns to the Acoustic folder\nthe same way (group 2).",
        check: Some(acoustic),
    },
    Item {
        section: GUITARS,
        text: "**Rhythm**: double-tracked, each channel with seven sources (DI,\npedalboard, two amps with a 57 and a 121 each), and Main and\nOctave layers doing the same part (`flow.guitars.golden`).",
        check: None,
    },
    Item {
        section: GUITARS,
        text: "**Lead**: a double-tracked DI-only part — L and R tracks, no\nfolder under them.",
        check: None,
    },
    Item {
        section: GUITARS,
        text: "**Solo**: one DI track, with a Harmony Solo beside it.",
        check: None,
    },
    Item {
        section: GUITARS,
        text: "Sources start at the default balance\n(`flow.guitars.mixing.source-defaults`): DI muted and centred\nbeside other sources, pedalboard muted beside an amp, one amp's\n57 and 121 hard left and right, two amps' mics still hard left and\nright with Amp 1's track 60 % left and Amp 2's 60 % right.",
        check: None,
    },
    Item {
        section: KEYS,
        text: "Keys as parts: Piano (a stereo track), Rhodes, Wurli, Organ → **KEYS BUS**.",
        check: Some(keys),
    },
    Item {
        section: KEYS,
        text: "Keys: **Wurli** beside the Rhodes (`flow.keys.parts`).",
        check: None,
    },
    Item {
        section: KEYS,
        text: "Synths by family — SY Arps, SY Pads, SY Leads, SY Chords — with the general synths beside them → **KEYS BUS**.",
        check: Some(synths),
    },
    Item {
        section: KEYS,
        text: "Synths sorted by family — **SY Arps, SY Pads, SY Leads, SY\nChords** — with two general synth tracks beside them\n(`flow.synths.families`, `flow.synths.golden`).",
        check: None,
    },
    Item {
        section: KEYS,
        text: "Colours, in session order and hue order: electrics blue,\nacoustics seafoam, keys green; synths purple, the synth bass\nincluded.",
        check: Some(colours_in_hue_order),
    },
    Item {
        section: KEYS,
        text: "Clav, strings and pads as the song has them.",
        check: None,
    },
    Item {
        section: ORCHESTRA,
        text: "**Winds/** Flute, Oboe, Clarinet, Bassoon.",
        check: Some(orchestra),
    },
    Item {
        section: ORCHESTRA,
        text: "**Brass/** Trumpets, Horns, Trombones, Bass Trombone, Tuba.",
        check: None,
    },
    Item {
        section: ORCHESTRA,
        text: "**Strings/** Violin 1, Violin 2, Viola, Cello, Bass.",
        check: None,
    },
    Item {
        section: ORCHESTRA,
        text: "**Orch Percussion/** — present, empty for now.",
        check: None,
    },
    Item {
        section: ORCHESTRA,
        text: "One bus per section under INST BUS (`flow.orchestra.golden`).",
        check: None,
    },
    Item {
        section: VOCALS,
        text: "Leads Ron (EN/ES/PT), Belen (EN/ES) and Aline (PT), each a mix track with Main and DBL and a source per language; BGV parts; a four-section choir; VOX EN/ES/PT VCAs → **LEAD VOX BUS**.",
        check: Some(vocals),
    },
    Item {
        section: VOCALS,
        text: "BGVs folder → **BGV BUS**; the vocal FX template (`vocal-fx.rpp`)\nfolded in.",
        check: Some(bgvs_and_vocal_fx),
    },
    Item {
        section: VOCALS,
        text: "**Three languages** as source tracks under each layer —\n`Vocals / Ron / Main / {EN, ES, PT}` — with a VCA per language over\nevery language's sources (`flow.vocals.language`,\n`flow.vocals.golden`).",
        check: None,
    },
    Item {
        section: VOCALS,
        text: "Leads **Ron** (EN, ES, PT), **Belen** (EN, ES), **Aline** (PT), each\na mix track with Main and DBL under it and a source per language\nunder those.",
        check: None,
    },
    Item {
        section: VOCALS,
        text: "BGV parts: Octave Down, Octave Up, Higher Harmony, Lower Harmony,\nWhisper, Bass, Tenor, Alto, Soprano — doubled — and a many-layer\n\"Hey!\" in `All`.",
        check: None,
    },
    Item {
        section: VOCALS,
        text: "A four-section **Choir** per language.",
        check: None,
    },
    Item {
        section: BUSES,
        text: "`MIX BUS / INST BUS / {DRUM, BASS, ELECTRIC/{GTR RHYTHM, GTR LEAD,\nGTR SOLO}, ACOUSTIC, KEYS}` and `VOX BUS / {LEAD VOX, BGV}` —\n`dynamic_template::buses`' tree without GUITAR BUS, with the three\nelectric buses added.",
        check: Some(bus_tree),
    },
    Item {
        section: BUSES,
        text: "A bus is a track with no items, unarmed; its chain is EQ → Comp.",
        check: Some(buses_are_buses),
    },
    Item {
        section: BUSES,
        text: "Scene: **Buses** — the tree alone.",
        check: Some(buses_scene),
    },
    Item {
        section: BUSES,
        text: "Monitor buses beside MIX BUS (Click + Guide, Headphones, Talkback,\nUtility).",
        check: None,
    },
];

/// The heading the checklist sits under.
pub const HEADING: &str = "## Checklist";

/// The checklist section as it must appear in the doc, from the
/// `## Checklist` heading up to (not including) the next `## ` heading.
#[must_use]
pub fn render(golden: &Golden) -> String {
    let mut out = String::from(HEADING);
    out.push('\n');
    let mut section: Option<&str> = None;
    for item in ITEMS {
        if section != Some(item.section) {
            out.push_str("\n### ");
            out.push_str(item.section);
            out.push_str("\n\n");
            section = Some(item.section);
        }
        let ticked = item.check.is_some_and(|check| check(golden));
        let mut lines = item.text.lines();
        if let Some(first) = lines.next() {
            out.push_str(if ticked { "- [x] " } else { "- [ ] " });
            out.push_str(first);
            out.push('\n');
        }
        for line in lines {
            out.push_str("      ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

/// Which items a golden ticks, as `(text, ticked)`, for a test that wants
/// to say which failed.
#[must_use]
pub fn ticks(golden: &Golden) -> Vec<(&'static str, bool)> {
    ITEMS
        .iter()
        .filter_map(|item| item.check.map(|check| (item.text, check(golden))))
        .collect()
}

/// The doc the checklist lives in.
#[must_use]
pub fn doc_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec/session/maximal-template.md")
}

/// The doc with its checklist section replaced by [`render`].
///
/// Takes the golden rather than loading one, so a caller regenerating
/// two documents builds the session once — and so the unit tests below
/// can hand it a session they have changed on purpose.
///
/// # Errors
///
/// When the doc has no `## Checklist` heading, or no `## ` heading after
/// it to end the section at.
pub fn regenerate(doc: &str, golden: &Golden) -> Result<String, String> {
    let start = doc
        .find(&format!("{HEADING}\n"))
        .ok_or_else(|| format!("no `{HEADING}` heading in the doc"))?;
    let after = start.saturating_add(HEADING.len());
    let rest = doc.get(after..).unwrap_or_default();
    let end = rest
        .find("\n## ")
        .map(|i| after.saturating_add(i).saturating_add(1))
        .ok_or_else(|| "no `## ` heading after the checklist".to_string())?;
    let mut out = String::with_capacity(doc.len());
    out.push_str(doc.get(..start).unwrap_or_default());
    out.push_str(&render(golden));
    out.push_str(doc.get(end..).unwrap_or_default());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden() -> Golden {
        Golden::load(&super::super::fixtures_dir())
    }

    #[test]
    fn a_check_that_fails_leaves_its_item_unticked() {
        let mut g = golden();
        // Take the kick's Sub away: the kick item is no longer true and
        // the render says so, while the snare's tick is unaffected.
        g.built.tracks.retain(|t| t.path != "Drums/Kick/Sub");
        let text = render(&g);
        assert!(text.contains("- [ ] **Kick**: In, Out, Trig, Sample; Sub; **Verb/** Room."));
        assert!(text.contains("- [x] **Snare**: Top, Bottom, Trig, Sample, Fund;"));
    }

    #[test]
    fn regenerate_only_touches_the_checklist_section() {
        let doc = "# Title\n\nintro\n\n## Checklist\n\nstale\n\n## Routing rules\n\nkept\n";
        let out = regenerate(doc, &golden()).unwrap_or_default();
        assert!(out.starts_with("# Title\n\nintro\n\n## Checklist\n\n### "));
        assert!(out.ends_with("\n## Routing rules\n\nkept\n"));
        assert!(!out.contains("stale"));
    }

    #[test]
    fn regenerate_refuses_a_doc_without_the_section() {
        assert!(regenerate("# nothing here\n", &golden()).is_err());
    }
}
