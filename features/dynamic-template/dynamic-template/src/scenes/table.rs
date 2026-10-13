//! The scenes themselves, as data.
//!
//! Nine scenes and a common prelude, written as [`Scene`] values rather
//! than as functions. Rust first (spec #48): every type derives `Facet`,
//! so the styx step that follows is a loader and nothing else — this
//! table becomes the built-in layer under a user's and a session's.
//!
//! Every selector says what a track IS rather than what it is called.
//! `Process`, `Compress`, `FX`, `Verb`, `MIX BUS`, `Guide`, `Keyflow`
//! and `Headphones` are taxonomy kinds the template writes when it
//! creates a folder, so renaming `Process` to `Parallel` changes nothing
//! about which scene finds it. A `name` appears only where the track is
//! a return a user picked — `Fat Plate` is a choice of unit, not a
//! structure.

use std::sync::OnceLock;

use super::language::Language;
use super::selector::{Role, Selector};
use super::types::{Audience, Effect, Fold, GroupBy, Rule, Scene, Size};

/// A selector on a taxonomy path prefix.
fn under(path: &[&str]) -> Selector {
    Selector {
        group: path.iter().map(|s| (*s).to_owned()).collect(),
        ..Selector::default()
    }
}

/// Every scene, in the order the number keys recall them.
///
/// Built once and shared: a table is a constant in spirit, but `Scene`
/// owns its strings so that the styx loader can produce the same type.
#[must_use]
pub fn scenes() -> &'static [Scene] {
    static SCENES: OnceLock<Vec<Scene>> = OnceLock::new();
    SCENES.get_or_init(build)
}

/// The scene a slug names.
#[must_use]
pub fn scene(slug: &str) -> Option<&'static Scene> {
    scenes().iter().find(|s| s.slug == slug)
}

/// The common prelude: what is true of every scene until it says
/// otherwise.
///
/// `flow.scenes.guide-folder` and `flow.scenes.keyflow-folder`: the
/// Guide folder sits at the top of every session and every scene shows
/// it collapsed unless a flow opens it, and the Keyflow folder under it
/// is collapsed in every scene — except in **Write** and **Produce**,
/// the two modes whose whole subject is the song's knowledge.
///
/// Data and overridable: these are ordinary rules, and they go first, so
/// a scene about the click can open the Guide folder by saying so.
///
/// `active_language` (`flow.vocals.language.active`) adds one hide rule
/// per sung language that is not the active one: `Selector::language`
/// needs no negation for this because the vocabulary is closed and
/// small — "not the active language" is just every *other* language,
/// enumerated. `None` (no switch has happened yet) adds none, which is
/// "hide nothing" rather than a guess.
// r[impl flow.scenes.guide-folder]
// r[impl flow.scenes.keyflow-folder]
// r[impl flow.vocals.language.active]
#[must_use]
pub fn prelude(mode: Option<&str>, active_language: Option<Language>) -> Vec<Rule> {
    let collapsed = Effect::at(Size::Compact).folded(Fold::Collapsed);
    let mut rules = vec![
        Rule::new(
            Selector {
                kind: Some("guide".to_owned()),
                ..Selector::default()
            },
            collapsed,
        ),
        Rule::new(
            Selector {
                kind: Some("keyflow".to_owned()),
                ..Selector::default()
            },
            collapsed,
        ),
    ];
    if matches!(mode, Some("write" | "produce")) {
        rules.push(Rule::new(
            Selector {
                kind: Some("keyflow".to_owned()),
                ..Selector::default()
            },
            Effect::default().folded(Fold::Open),
        ));
    }
    if let Some(active) = active_language {
        for language in Language::SUNG.into_iter().filter(|&l| l != active) {
            rules.push(Rule::new(
                Selector {
                    language: Some(language),
                    ..Selector::default()
                },
                Effect::hidden(),
            ));
        }
    }
    rules
}

/// The bus tree, out of the way: a scene about the instruments is not
/// about the mix.
///
/// One rule, on the tree's root kind, rather than one per bus: the fold
/// reaches everything under it, and a session with a bus the template
/// did not make still loses it with the rest.
fn hide_the_bus_tree() -> Rule {
    Rule::new(
        Selector {
            kind: Some("mix-bus".to_owned()),
            ..Selector::default()
        },
        Effect::hidden(),
    )
}

/// The monitor side out of the arrangement — the headphone mixes and the
/// talkback are heard in the room and never by the record, so they have
/// nothing to arrange — and kept in the mixer, where their faders are.
fn monitors_out_of_the_arrangement() -> Vec<Rule> {
    [
        Selector {
            kind: Some("headphones".to_owned()),
            ..Selector::default()
        },
        under(&["TALKBACK"]),
    ]
    .into_iter()
    .map(|selector| Rule {
        selector,
        effect: Effect::default(),
        arrange: Some(Effect::hidden()),
        mixer: None,
    })
    .collect()
}

/// The BGVs and the choir folded to one row each: opened, each part is a
/// row (`one_row_per_bgv_part`).
fn vocal_folders_folded() -> Vec<Rule> {
    [&["Vocals", "BGVs"][..], &["Choir"][..]]
        .into_iter()
        .map(|path| Rule {
            selector: Selector {
                role: Role::Bus,
                ..under(path)
            },
            effect: Effect::default(),
            arrange: Some(Effect::default().folded(Fold::Collapsed)),
            mixer: None,
        })
        .collect()
}

/// The language VCAs (VOX EN, ES, PT) out of the arrangement: control
/// tracks with nothing on them to arrange, kept in the mixer.
fn vcas_out_of_the_arrangement() -> Rule {
    Rule {
        selector: Selector {
            kind: Some("vca".to_owned()),
            ..Selector::default()
        },
        effect: Effect::default(),
        arrange: Some(Effect::hidden()),
        mixer: None,
    }
}

/// A synth family that holds one synth is one row in the arrangement:
/// the family and its synth are the same part.
const SYNTH_FAMILIES: [&[&str]; 4] = [
    &["Synths", "SY Arps"],
    &["Synths", "SY Pads"],
    &["Synths", "SY Leads"],
    &["Synths", "SY Chords"],
];

fn one_row_per_synth_family() -> Vec<Rule> {
    SYNTH_FAMILIES
        .iter()
        .map(|family| folded_in_the_arrangement(family))
        .collect()
}

/// Sub, Verb and Fundamental keep their strips and give up their rows.
///
/// They are parallel colour, not the kit. Nothing is recorded on one and
/// nothing is edited on one — a Verb bank has no takes to trim and a Sub
/// has no transient to nudge — so what they need is a fader, and a fader
/// is the mixer. In the panel they are rows between a drum and the next
/// drum, and on a kit that is five pieces deep they are the reason the
/// whole kit does not fit on screen.
///
/// This is the first scene rule to use the per-surface override at all,
/// and it is what it was for: `arrange` says hidden, `mixer` says
/// nothing, so the mixer keeps whatever an earlier rule already decided.
///
/// They come back when there is something to draw on them, which is
/// automation — but automation is a property of every phase from
/// `Rescue` onward rather than a phase of its own (see
/// `session::mix_phases`), and nothing in the scene engine reads a phase
/// yet. So this is a default, not a mode: the scene that wants them
/// visible is a scene, and it does not exist yet.
fn parallel_colour_out_of_the_panel() -> Vec<Rule> {
    ["sub", "verb", "fundamental"]
        .into_iter()
        .map(|kind| Rule {
            selector: Selector {
                kind: Some(kind.to_owned()),
                ..Selector::default()
            },
            // Nothing, deliberately: an empty effect leaves what an
            // earlier rule decided alone, and the mixer's widths are
            // already right.
            effect: Effect::default(),
            arrange: Some(Effect::hidden()),
            mixer: None,
        })
        .collect()
}

/// Each drum is one row in the panel, with its take on it.
///
/// A kit is a folder tree because that is how the audio is routed — a
/// kick is a Sum with three mics under it, a tom is its mic and its
/// trigger. Routing is a mix concern, and the panel is not where mixing
/// happens. While you are editing, a kick is one thing: the mics were
/// recorded together and comped together, so what belongs on screen is
/// one row per drum, with the take on it and the mics a fold away.
///
/// Collapsed rather than hidden, and in the ARRANGEMENT only. A
/// collapsed folder keeps its row, which is the row the take is drawn
/// on; the mixer keeps every strip, because the mics are exactly what
/// you reach for there.
///
/// The Toms group stays open — its toms are five separate drums and
/// reading them as one row would be reading a fill as a hit. Cymbals and
/// Rooms do collapse: they are arrays of one sound, and the overheads
/// are not five decisions.
///
/// What fills the row is `daw_ui::studio::folded`, which is where the
/// rule for folding several children's items into one row's lives.
fn one_row_per_drum() -> Vec<Rule> {
    let shut = |selector| Rule {
        selector,
        effect: Effect::default(),
        arrange: Some(Effect::default().folded(Fold::Collapsed)),
        mixer: None,
    };
    let mut rules: Vec<Rule> = ["Kick", "Snare", "Toms", "Cymbals", "Rooms"]
        .into_iter()
        .map(|piece| {
            shut(Selector {
                role: Role::Bus,
                ..under(&[KIT[0], KIT[1], piece])
            })
        })
        .collect();
    // And each tom inside the group, which stays open around them.
    rules.push(shut(Selector {
        kind: Some("piece".to_owned()),
        role: Role::Bus,
        ..under(&[KIT[0], KIT[1], "Toms"])
    }));
    rules
}

/// The electric guitar's taxonomy path.
const ELECTRIC: [&str; 2] = ["Guitars", "Electric"];

/// The electric's layers that stand over an L and an R, as paths below
/// [`ELECTRIC`]: the tracks a guitar part is processed and edited on.
///
/// Lead has no layer level — it is its own layer, doubled. Solo is not
/// here at all: its layers are single tracks, with nothing to fold.
const GUITAR_LAYERS: [&[&str]; 3] = [&["Rhythm", "Main"], &["Rhythm", "Octave"], &["Lead"]];

/// The channels under those layers that are folders of sources, as
/// paths below [`ELECTRIC`]. Lead's L and R are tracks rather than
/// folders, and the engine already rails them as halves of a pair.
const GUITAR_CHANNELS: [&[&str]; 4] = [
    &["Rhythm", "Main", "L"],
    &["Rhythm", "Main", "R"],
    &["Rhythm", "Octave", "L"],
    &["Rhythm", "Octave", "R"],
];

/// A path below a group, as one selector path.
fn below<'a>(group: &[&'a str], rest: &[&'a str]) -> Vec<&'a str> {
    group.iter().chain(rest).copied().collect()
}

/// Each guitar layer is one row in the panel, with its take on it.
///
/// The electric reads in the arrangement as ARRANGEMENT concerns —
/// Rhythm over its Main and Octave, Lead, Solo over its Main and
/// Harmony — and everything under a layer is a GUITAR concern: the L
/// and R a part was doubled into and the seven sources each channel was
/// captured on. Those were recorded together and are comped together,
/// so while editing a layer is one thing, and its channels and sources
/// are a fold away.
///
/// The same reasoning as [`one_row_per_drum`], and the same shape:
/// collapsed rather than hidden, so the layer keeps the row its take is
/// drawn on, and in the ARRANGEMENT only — the mixer keeps every strip.
fn one_row_per_guitar_layer() -> Vec<Rule> {
    GUITAR_LAYERS
        .iter()
        .map(|layer| folded_in_the_arrangement(&below(&ELECTRIC, layer)))
        .collect()
}

/// A folder collapsed in the ARRANGEMENT only: its row stays, with its
/// children's items folded onto it, and the mixer keeps every strip.
fn folded_in_the_arrangement(path: &[&str]) -> Rule {
    Rule {
        selector: Selector {
            role: Role::Bus,
            ..under(path)
        },
        effect: Effect::default(),
        arrange: Some(Effect::default().folded(Fold::Collapsed)),
        mixer: None,
    }
}

/// The vocal returns, out of the arrangement — hidden, not folded.
///
/// `Vox FX` is the vocal's Delay, Verb, Wide and Pitch banks beside the
/// leads that feed them. Like a drum's Verb, nothing is recorded or
/// edited on a return, so its rows are what stands between the leads
/// and the BGVs; unlike a drum's, the banks are folders, so folding
/// would still leave a row. Out of the panel until there is something
/// to draw on them — automation — and kept in the mixer, where a
/// return's fader is the whole point.
///
/// Matched by the template's kind (`fx`) under the Vocals group, so a
/// renamed folder still goes.
fn vox_fx_out_of_the_arrangement() -> Rule {
    Rule {
        selector: Selector {
            kind: Some("fx".to_owned()),
            role: Role::Bus,
            ..under(&["Vocals"])
        },
        effect: Effect::default(),
        arrange: Some(Effect::hidden()),
        mixer: None,
    }
}

/// The background vocal parts, as paths below the Vocals group: each a
/// part a singer is asked for, over the language sources (and doubles)
/// it was sung as. The choir's sections are parts too.
const BGV_PARTS: [&[&str]; 9] = [
    &["BGVs", "Octave Up"],
    &["BGVs", "Octave Down"],
    &["BGVs", "Higher Harmony"],
    &["BGVs", "Lower Harmony"],
    &["BGVs", "Hey"],
    &["Choir", "Soprano"],
    &["Choir", "Alto"],
    &["Choir", "Tenor"],
    &["Choir", "Bass"],
];

/// Each BGV part is one row in the panel, with its take on it.
///
/// A part is what the arrangement is about — the octave up in the
/// chorus, the "Hey!" on the four — and the English, Spanish and
/// Portuguese takes of it, and the doubles, are one decision sung
/// several ways. The same reasoning as [`one_row_per_guitar_layer`],
/// and the same shape: collapsed in the arrangement, every strip in the
/// mixer. The language switch still mutes what it mutes underneath.
fn one_row_per_bgv_part() -> Vec<Rule> {
    BGV_PARTS
        .iter()
        .map(|part| {
            // The BGVs stand for `Vocals/BGVs`; the choir is a group of
            // its own, `Choir`.
            if part.first() == Some(&"Choir") {
                folded_in_the_arrangement(part)
            } else {
                folded_in_the_arrangement(&below(&["Vocals"], part))
            }
        })
        .collect()
}

/// A lead singer's two parts, as paths below the Vocals group: the main
/// line and its double, each a folder over the language sources it was
/// sung as. Every lead (`Ron`, `Belen`, `Aline`) stands for the template
/// group `Vocals/Lead`, so one path reaches each of them.
const LEAD_PARTS: [&[&str]; 2] = [&["Lead", "Main"], &["Lead", "DBL"]];

/// Each lead's Main and DBL are one row in the panel, the languages
/// folded under them — [`one_row_per_bgv_part`]'s reasoning for the
/// leads: the English, Spanish and Portuguese takes of a line are one
/// decision sung several ways. The singer's own folder stays open, so
/// Main and DBL read as two rows under the name.
fn one_row_per_lead_part() -> Vec<Rule> {
    // Each singer is one row: Ron, Belen, Aline folded over their line
    // and its double — and those folded over their languages, for when
    // a singer is opened.
    std::iter::once(folded_in_the_arrangement(&["Vocals", "Lead"]))
        .chain(
            LEAD_PARTS
                .iter()
                .map(|part| folded_in_the_arrangement(&below(&["Vocals"], part))),
        )
        .collect()
}

/// The kit's parallel chain, out of the MIXER — hidden, not folded.
///
/// `Process` is what the kit is sent to: the compressors from Dry to
/// Crunch and the room sim and verb bank. Mixing the pieces happens on
/// the pieces; those returns are set once and then are strips between
/// the percussion and the bass. Matched by the template's kind, so a
/// renamed folder still goes. The arrangement keeps it — it is
/// `parallel_colour_out_of_the_panel` that decides what of it is drawn
/// there.
fn process_out_of_view() -> Rule {
    // Out of both: its returns are set once, the pieces are mixed on
    // the pieces, and there is nothing in it to arrange.
    Rule::new(
        Selector {
            kind: Some("process".to_owned()),
            role: Role::Bus,
            ..Selector::default()
        },
        Effect::hidden(),
    )
}

/// Each bass is one row in the panel: the bass guitar over its DI and
/// its amp — two captures of one performance, a mixer concern — and the
/// synth bass over its one synth and the sub beside it.
fn one_row_per_bass() -> Vec<Rule> {
    [["Bass", "Guitar"], ["Bass", "Synth"]]
        .iter()
        .map(|piece| folded_in_the_arrangement(piece))
        .collect()
}

/// Folders of captures as rails in the mixer: `rails` are paths below
/// `group`, each rule reaching the folder and every folder under it.
///
/// A guitar channel and its amps carry no processing — the chain is on
/// the layer above — so what they need on a strip is a fader and a
/// mute. Mixer only: a row's height is a different question.
fn railed(group: &[&str], rails: &[&[&str]]) -> Vec<Rule> {
    rails
        .iter()
        .map(|rail| Rule {
            selector: Selector {
                role: Role::Bus,
                ..under(&below(group, rail))
            },
            effect: Effect::default(),
            arrange: None,
            mixer: Some(Effect::at(Size::Minimum)),
        })
        .collect()
}

/// The five pieces a drum mix is made on — matched by the template
/// groups they stand for, so a kit with a sixth piece needs a line here
/// and a renamed one needs nothing.
const PIECES: [&str; 5] = ["Kick", "Snare", "Toms", "Cymbals", "Rooms"];

/// The kit's taxonomy path.
const KIT: [&str; 2] = ["Drums", "Drum Kit"];

/// The kit's parallel chains: in Drums, beside the pieces — a folder of
/// the session's, not a template group, so addressed where it sits
/// (the pieces are addressed by their group, [`KIT`], wherever they are).
const PROCESS: [&str; 2] = ["Drums", "process"];

/// One rule per kit piece: the piece folder and everything under it.
fn per_piece(effect: Effect) -> Vec<Rule> {
    PIECES
        .iter()
        .map(|piece| {
            Rule::new(
                Selector {
                    role: Role::Bus,
                    ..under(&[KIT[0], KIT[1], piece])
                },
                effect,
            )
        })
        .collect()
}

// ── the instrument scene sets ────────────────────────────────────────
//
// One shape, four instruments. Each set is the same four questions —
// what do you look at while getting a sound, while the band plays,
// while choosing takes, while editing, while mixing — and the answers
// differ only in which folder they are asked about. Writing them as
// data rather than as functions is what makes that visible: the
// difference between the bass set and the keys set is a group path.

/// Every scene an instrument needs, built from one description.
struct Set {
    instrument: &'static str,
    /// The group path the instrument lives under.
    group: &'static [&'static str],
    /// What the overview collapses to one row each — the pieces, the
    /// parts, the folders a player reads.
    rows: &'static [&'static str],
    /// The folders under the group that hold only captures — a guitar
    /// layer's channels — as paths below the group. Railed in the
    /// mixing scene: nothing is processed on them, so a fader and a
    /// mute is all they need.
    rails: &'static [&'static [&'static str]],
    /// The bus the mixing scene shows.
    bus: &'static str,
}

impl Set {
    /// The selector path to one of this instrument's rows.
    ///
    /// The group's own path plus the row, rather than the group's first
    /// segment plus the row: `Electric` lives under `Guitars`, and a
    /// selector that started at `Electric` matched nothing at all —
    /// silently, since a selector that matches nothing is a scene that
    /// shows nothing rather than an error.
    fn path_to(&self, row: &'static str) -> Vec<&'static str> {
        let mut path = self.group.to_vec();
        path.push(row);
        path
    }

    /// The engineer's tracking view: every source of the instrument
    /// open, with a level to watch and a phase to set, and the rest of
    /// the session out of the way.
    fn tracking(&self) -> Scene {
        Scene {
            name: format!("{} Tracking", title(self.instrument)),
            slug: format!("{}-tracking", self.instrument),
            short: "Trk".to_owned(),
            instrument: self.instrument.to_owned(),
            modes: vec!["record".to_owned()],
            audience: Audience::Engineer,
            // Tracking sorts by performer: while a take is going in,
            // what needs doing is about the person playing it.
            group_by: GroupBy::Performer,
            spec: vec!["flow.scenes.render".to_owned()],
            default: Effect::at(Size::Compact),
            rules: vec![
                Rule::new(
                    Selector {
                        role: Role::Leaf,
                        ..under(self.group)
                    },
                    Effect::at(Size::Working),
                ),
                Rule::new(under(&PROCESS), Effect::at(Size::Minimum)),
                hide_the_bus_tree(),
            ],
        }
    }

    /// The player's view: the instrument collapsed to the rows they
    /// read, and nothing else. The folder-record preview draws the take
    /// going in on these closed rows, which is what lets an overview
    /// stay collapsed while tracking.
    fn overview(&self) -> Scene {
        let mut rules = vec![Rule::new(
            Selector {
                role: Role::Leaf,
                ..under(self.group)
            },
            Effect::hidden(),
        )];
        for row in self.rows {
            rules.push(Rule::new(
                Selector {
                    role: Role::Bus,
                    ..under(&self.path_to(row))
                },
                Effect::at(Size::Working).folded(Fold::Collapsed),
            ));
        }
        rules.push(Rule::new(under(&PROCESS), Effect::hidden()));
        rules.push(hide_the_bus_tree());
        Scene {
            name: format!("{} Tracking Overview", title(self.instrument)),
            slug: format!("{}-tracking-overview", self.instrument),
            short: "Over".to_owned(),
            instrument: self.instrument.to_owned(),
            modes: vec!["record".to_owned()],
            audience: Audience::Player,
            group_by: GroupBy::Arrangement,
            spec: vec![
                "flow.scenes.two-audiences".to_owned(),
                "flow.scenes.render".to_owned(),
            ],
            default: Effect::at(Size::Compact),
            rules,
        }
    }

    /// Choosing takes: the instrument's tracks open with their lanes,
    /// everything else collapsed. Comping is not a mode — it lives
    /// inside Record and Edit — so this one is recall-only.
    fn comping(&self) -> Scene {
        Scene {
            name: format!("{} Comping", title(self.instrument)),
            slug: format!("{}-comping", self.instrument),
            short: "Comp".to_owned(),
            instrument: self.instrument.to_owned(),
            modes: Vec::new(),
            audience: Audience::Engineer,
            group_by: GroupBy::Arrangement,
            spec: vec!["flow.scenes.render".to_owned()],
            default: Effect::at(Size::Minimum),
            rules: vec![
                Rule::new(under(self.group), Effect::at(Size::Working)),
                hide_the_bus_tree(),
            ],
        }
    }

    /// Editing: one row per part, the sources folded into it, so an
    /// edit to a row is an edit to everything under it — the same fold
    /// the docked stack uses.
    fn editing(&self) -> Scene {
        let mut rules = vec![Rule::new(
            Selector {
                role: Role::Leaf,
                ..under(self.group)
            },
            Effect::hidden(),
        )];
        for row in self.rows {
            rules.push(Rule::new(
                Selector {
                    role: Role::Bus,
                    ..under(&self.path_to(row))
                },
                Effect::at(Size::Working).folded(Fold::Collapsed),
            ));
        }
        rules.extend([
            Rule::new(under(&PROCESS), Effect::hidden()),
            hide_the_bus_tree(),
        ]);
        Scene {
            name: format!("{} Editing", title(self.instrument)),
            slug: format!("{}-editing", self.instrument),
            short: "Edit".to_owned(),
            instrument: self.instrument.to_owned(),
            modes: vec!["edit".to_owned()],
            audience: Audience::Engineer,
            group_by: GroupBy::Arrangement,
            spec: vec![
                "flow.scenes.follow-mode".to_owned(),
                "flow.scenes.render".to_owned(),
            ],
            default: Effect::at(Size::Compact),
            rules,
        }
    }

    /// Mixing: the instrument's folders and its bus, the sources as
    /// rails. Almost all of a mix happens at the part and its bus, so
    /// that is the level this shows.
    fn mixing(&self) -> Scene {
        Scene {
            name: format!("{} Mixing", title(self.instrument)),
            slug: format!("{}-mixing", self.instrument),
            short: "Mix".to_owned(),
            instrument: self.instrument.to_owned(),
            modes: vec!["mix".to_owned()],
            audience: Audience::Engineer,
            group_by: GroupBy::Arrangement,
            spec: vec![
                "flow.scenes.follow-mode".to_owned(),
                "flow.scenes.render".to_owned(),
            ],
            default: Effect::at(Size::Minimum),
            rules: vec![
                // The multi-mic level goes. Almost all of a mix happens
                // at the part and its bus, so that is the level this
                // shows — and it is what makes Guitar Balance a scene
                // of its own rather than the same view wider: Balance
                // exists precisely to open what this hides.
                Rule::new(
                    Selector {
                        role: Role::Leaf,
                        ..under(self.group)
                    },
                    Effect::hidden(),
                ),
                Rule::new(
                    Selector {
                        role: Role::Bus,
                        ..under(self.group)
                    },
                    Effect::at(Size::Working),
                ),
                Rule::new(
                    Selector {
                        name: Some(self.bus.to_owned()),
                        ..Selector::default()
                    },
                    Effect::at(Size::Working),
                ),
            ]
            .into_iter()
            .chain(railed(self.group, self.rails))
            .collect(),
        }
    }
}

/// Title-case an instrument slug for a scene's display name.
fn title(instrument: &str) -> String {
    let mut chars = instrument.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// The instruments whose sets are generated, and what each collapses to.
const SETS: [Set; 4] = [
    Set {
        instrument: "bass",
        group: &["Bass"],
        rows: &[],
        rails: &[],
        bus: "BASS BUS",
    },
    Set {
        instrument: "guitar",
        group: &ELECTRIC,
        rows: &["Rhythm", "Lead", "Solo"],
        rails: &GUITAR_CHANNELS,
        bus: "ELECTRIC BUS",
    },
    Set {
        instrument: "keys",
        group: &["Keys"],
        rows: &["Piano", "Rhodes", "Wurli", "Organ"],
        rails: &[],
        bus: "KEYS BUS",
    },
    Set {
        instrument: "percussion",
        group: &["Percussion"],
        rows: &["Shaker", "Tambourine", "Claps"],
        rails: &[],
        bus: "PERC BUS",
    },
];

/// Every generated instrument scene, in set order.
fn instrument_sets() -> Vec<Scene> {
    SETS.iter()
        .flat_map(|set| {
            [
                set.tracking(),
                set.overview(),
                set.comping(),
                set.editing(),
                set.mixing(),
            ]
        })
        .collect()
}

/// **Guitar Balance**: what Guitar Mixing hides.
///
/// Guitar Mixing collapses every layer and multi-mic folder, because
/// almost all of a guitar mix happens at the part and its bus. This is
/// the one scene that opens the multi-mic level — every channel's
/// sources as strips — so the initial balance and panning of a
/// configuration can be set. Recall-only: it is a thing you go and do
/// once, not the view you land in (`flow.guitars.mixing.balance-scene`).
// r[impl flow.guitars.mixing.balance-scene]
fn guitar_balance() -> Scene {
    Scene {
        name: "Guitar Balance".to_owned(),
        slug: "guitar-balance".to_owned(),
        short: "Bal".to_owned(),
        instrument: "guitar".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.guitars.mixing.balance-scene".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Minimum),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&["Electric"])
                },
                Effect::at(Size::Working),
            ),
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&["Acoustic"])
                },
                Effect::at(Size::Working),
            ),
            hide_the_bus_tree(),
        ],
    }
}

/// **Vocal Tracking**: the active language's leads and every part being
/// recorded, sorted by performer.
///
/// The vocal FX returns stay present, which looks like a mixing concern
/// and is not: a singer needs to hear the reverb they are singing into,
/// and a tracking view that hid the returns would leave the engineer
/// unable to set what the performer hears.
// r[impl flow.vocals.tracking]
fn vocal_tracking() -> Scene {
    Scene {
        name: "Vocal Tracking".to_owned(),
        slug: "vocal-tracking".to_owned(),
        short: "Trk".to_owned(),
        instrument: "vocal".to_owned(),
        modes: vec!["record".to_owned()],
        audience: Audience::Engineer,
        // Ron's tracks together, Belen's together — while a take is
        // going in, what needs doing is about the person singing.
        group_by: GroupBy::Performer,
        spec: vec![
            "flow.vocals.tracking".to_owned(),
            "flow.scenes.performer-order".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&["Vocals"])
                },
                Effect::at(Size::Working),
            ),
            // The returns are what the singer hears themselves in.
            Rule::new(under(&["fx"]), Effect::at(Size::Compact)),
            hide_the_bus_tree(),
        ],
    }
}

/// **Vocal Comping**: the active language's sources with their lanes,
/// parts as folders.
///
/// The Edit-mode default for vocals. A part comps on its folder the way
/// a kit does — one lane per take of the whole part — so a fifty-layer
/// "Hey!" is comped once rather than fifty times.
// r[impl flow.vocals.comping]
fn vocal_comping() -> Scene {
    Scene {
        name: "Vocal Comping".to_owned(),
        slug: "vocal-comping".to_owned(),
        short: "Comp".to_owned(),
        instrument: "vocal".to_owned(),
        modes: vec!["edit".to_owned()],
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.vocals.comping".to_owned(),
            "flow.scenes.follow-mode".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Minimum),
        rules: vec![
            Rule::new(under(&["Vocals"]), Effect::at(Size::Working)),
            hide_the_bus_tree(),
        ],
    }
}

fn build() -> Vec<Scene> {
    vec![
        drum_tracking(),
        drum_tracking_overview(),
        drum_editing(),
        drum_mixing(),
        drum_overview(),
        drum_advanced(),
        drum_fx(),
        buses(),
        guitar_fx(),
        lead_vocal(),
        lead_vocal_fx(),
        guitar_balance(),
        vocal_tracking(),
        vocal_comping(),
    ]
    .into_iter()
    .chain(instrument_sets())
    .chain([overview(), overview_top()])
    .collect()
}

/// Every bus `depth` deep folded to one row in the arrangement.
fn folded_at(depth: u32) -> Rule {
    Rule {
        selector: Selector {
            role: Role::Bus,
            depth: Some(depth),
            ..Selector::default()
        },
        effect: Effect::default(),
        arrange: Some(Effect::default().folded(Fold::Collapsed)),
        mixer: None,
    }
}

/// What no overview shows: the kit's parallel chain, the monitors, the
/// control VCAs and the vocal returns — out of the way, as Drum Mixing
/// has them.
fn out_of_every_overview() -> Vec<Rule> {
    let mut rules = vec![
        process_out_of_view(),
        vcas_out_of_the_arrangement(),
        vox_fx_out_of_the_arrangement(),
    ];
    rules.extend(monitors_out_of_the_arrangement());
    rules
}

/// **Overview** (`v v`): the session one level down — every top folder
/// open, each thing in it one row: Drums as Kick, Snare, Toms, Cymbals,
/// Rooms; Bass as its guitar and its synth; the electric as its layers;
/// each singer one row.
fn overview() -> Scene {
    let mut rules = vec![folded_at(1)];
    rules.extend(out_of_every_overview());
    Scene {
        name: "Overview".to_owned(),
        slug: "overview".to_owned(),
        short: "Overview".to_owned(),
        instrument: "all".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: Vec::new(),
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// **Overview TOP** (`v t`): each top folder one row — but the
/// Orchestra open over its sections (Strings, Brass, Winds, Percussion),
/// each of those one row.
fn overview_top() -> Scene {
    let orchestra = |depth| Selector {
        role: Role::Bus,
        depth: Some(depth),
        ..under(&["Orchestra"])
    };
    let mut rules = vec![
        folded_at(0),
        Rule {
            selector: orchestra(0),
            effect: Effect::default(),
            arrange: Some(Effect::default().folded(Fold::Open)),
            mixer: None,
        },
        Rule {
            selector: orchestra(1),
            effect: Effect::default(),
            arrange: Some(Effect::default().folded(Fold::Collapsed)),
            mixer: None,
        },
    ];
    rules.extend(out_of_every_overview());
    Scene {
        name: "Overview TOP".to_owned(),
        slug: "overview-top".to_owned(),
        short: "Top".to_owned(),
        instrument: "all".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: Vec::new(),
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// **Drum Tracking Overview**: the kit as the drummer reads it, one
/// strip per piece.
///
/// This is not the engineer's view with fewer rows. While a sound is
/// being got, the engineer needs every mic; once tracking is a
/// whole-band session, everyone else needs to know the kit is going in
/// and nothing more. So each piece is one collapsed folder carrying its
/// folder item — the summed waveform in the piece's colour — and a
/// stereo pair is one strip, because OH is one capture of one thing.
///
/// It is the Record-mode default for the **player** audience, which is
/// what puts it on the drummer's tablet without anyone choosing it.
// r[impl flow.drums.tracking.overview]
// r[impl flow.drums.tracking.arm]
// r[impl flow.scenes.two-audiences]
fn drum_tracking_overview() -> Scene {
    let mut rules = vec![
        // Everything inside a piece folds away: at this zoom a mic is a
        // line, and thirty lines under a kit is the picture this scene
        // exists to remove.
        Rule::new(
            Selector {
                role: Role::Leaf,
                ..under(&KIT)
            },
            Effect::hidden(),
        ),
    ];
    // Each piece: one row, collapsed, at a size worth reading — the arm
    // and the monitor lamp sit on it, which is the half of
    // `flow.drums.tracking.arm` this scene owns.
    for piece in PIECES {
        rules.push(Rule::new(
            Selector {
                role: Role::Bus,
                ..under(&[KIT[0], KIT[1], piece])
            },
            Effect::at(Size::Working).folded(Fold::Collapsed),
        ));
    }
    rules.push(Rule::new(under(&PROCESS), Effect::hidden()));
    rules.push(hide_the_bus_tree());
    Scene {
        name: "Drum Tracking Overview".to_owned(),
        slug: "drum-tracking-overview".to_owned(),
        short: "Kit".to_owned(),
        instrument: "drums".to_owned(),
        modes: vec!["record".to_owned()],
        audience: Audience::Player,
        // The drummer reads the kit, not the room's personnel.
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.tracking.overview".to_owned(),
            "flow.drums.tracking.arm".to_owned(),
            "flow.scenes.two-audiences".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// **Drum Editing**: one row per source piece; opened, the tracks that
/// hold what was recorded.
///
/// The arrangement's counterpart of the docked stack's lanes: a piece
/// shut (as the window starts it) is every mic of it at once, so an edit
/// to the piece is an edit to all of them. Opened, a piece shows where its recording is — its
/// mics and its trigger, `In`, `Out`, `Trig` — lifted out of the Sum that
/// adds them up, and nothing derived from them: no sub, fundamental,
/// sample or reverb return, which have no hits of their own. The Process
/// folder and the bus tree are hidden outright — editing is about the
/// takes, and a send has no hits in it.
///
/// This is what Edit mode shows for the kit.
// r[impl flow.drums.editing.scene]
// r[impl flow.scenes.follow-mode]
fn drum_editing() -> Scene {
    // The pieces at working height and left to the window to fold: it
    // starts each piece shut, and a click opens it — a fold the scene made
    // would take the rows away for good.
    let mut rules = Vec::new();
    for piece in PIECES {
        rules.push(Rule::new(
            Selector {
                role: Role::Bus,
                ..under(&[KIT[0], KIT[1], piece])
            },
            Effect::at(Size::Working),
        ));
    }
    // The Sum's row goes, its mics stay; what is made from the mics goes.
    rules.push(Rule::new(
        Selector {
            kind: Some("sum".to_owned()),
            ..under(&KIT)
        },
        Effect {
            size: None,
            fold: Some(Fold::Lifted),
        },
    ));
    for derived in ["sub", "fundamental", "sample", "verb"] {
        rules.push(Rule::new(
            Selector {
                kind: Some(derived.to_owned()),
                ..under(&KIT)
            },
            Effect::hidden(),
        ));
    }
    rules.extend([
        Rule::new(under(&PROCESS), Effect::hidden()),
        hide_the_bus_tree(),
    ]);
    Scene {
        name: "Drum Editing".to_owned(),
        slug: "drum-editing".to_owned(),
        short: "Edit".to_owned(),
        instrument: "drums".to_owned(),
        modes: vec!["edit".to_owned()],
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.editing.scene".to_owned(),
            "flow.scenes.follow-mode".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// Tracking: the core microphones you are getting a sound on, and
/// nothing else that needs reading.
// r[impl flow.drums.tracking.full]
// r[impl flow.scenes.follow-mode]
fn drum_tracking() -> Scene {
    Scene {
        name: "Drum Tracking".to_owned(),
        slug: "drum-tracking".to_owned(),
        short: "Trk".to_owned(),
        instrument: "drums".to_owned(),
        modes: vec!["record".to_owned()],
        audience: Audience::Engineer,
        // Tracking sorts by performer (`flow.scenes.performer-order`).
        // The data says so here; emitting the header rows is #51, and
        // that ticket adds the emitting and nothing else.
        group_by: GroupBy::Performer,
        spec: vec![
            "flow.drums.tracking.full".to_owned(),
            "flow.scenes.follow-mode".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules: vec![
            // Everything hanging off the kit is present and unread.
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&KIT)
                },
                Effect::at(Size::Minimum),
            ),
            // Except the close mics of the two pieces a sound is got on
            // first — the ones with a level to watch and a phase to set.
            Rule::new(
                Selector {
                    kind: Some("source".to_owned()),
                    ..under(&[KIT[0], KIT[1], "Kick"])
                },
                Effect::at(Size::Working),
            ),
            Rule::new(
                Selector {
                    kind: Some("source".to_owned()),
                    ..under(&[KIT[0], KIT[1], "Snare"])
                },
                Effect::at(Size::Working),
            ),
            // What the kit is sent to is present and no more: a tracking
            // pass does not touch the parallel chain.
            Rule::new(under(&PROCESS), Effect::at(Size::Minimum)),
            hide_the_bus_tree(),
        ],
    }
}

/// Mixing: the pieces are the instrument; every mic is a rail.
// r[impl flow.drums.mixing.scenes]
fn drum_mixing() -> Scene {
    let mut rules = vec![Rule::new(
        Selector {
            role: Role::Leaf,
            ..Selector::default()
        },
        Effect::at(Size::Minimum),
    )];
    // The pieces opened to working size in the mixer only: in the
    // arrangement a piece is a row like any other instrument's, not a
    // taller one.
    rules.extend(
        per_piece(Effect::at(Size::Working))
            .into_iter()
            .map(|mut rule| {
                rule.arrange = Some(Effect {
                    size: None,
                    fold: None,
                });
                rule
            }),
    );
    // A piece's own folders are not the piece: the Sum that holds its
    // mics and the Verb bank beside it are structure, and structure is
    // read at the width of a bus.
    for kind in ["sum", "verb"] {
        rules.push(Rule::new(
            Selector {
                kind: Some(kind.to_owned()),
                role: Role::Bus,
                ..Selector::default()
            },
            Effect::at(Size::Compact),
        ));
    }
    rules.push(Rule::new(
        Selector {
            kind: Some("piece".to_owned()),
            role: Role::Bus,
            ..under(&[KIT[0], KIT[1], "Toms"])
        },
        Effect::at(Size::Compact),
    ));
    rules.push(hide_the_bus_tree());
    rules.extend(parallel_colour_out_of_the_panel());
    rules.extend(one_row_per_drum());
    // The session's first view, so it is also where the electric opens
    // as its arrangement with its channels little strips.
    rules.extend(one_row_per_guitar_layer());
    rules.extend(railed(&ELECTRIC, &GUITAR_CHANNELS));
    // And the rest of the band as its parts: a bass per row, a lead's
    // line and its double per row, a BGV part per row, the vocal returns
    // out of the panel.
    rules.extend(one_row_per_bass());
    rules.extend(one_row_per_lead_part());
    rules.extend(one_row_per_bgv_part());
    rules.push(vox_fx_out_of_the_arrangement());
    // The kit's parallel chain out of the mixer: the pieces are mixed on
    // the pieces.
    rules.push(process_out_of_view());
    rules.extend(monitors_out_of_the_arrangement());
    rules.extend(vocal_folders_folded());
    rules.push(vcas_out_of_the_arrangement());
    rules.extend(one_row_per_synth_family());
    Scene {
        name: "Drum Mixing".to_owned(),
        slug: "drum-mixing".to_owned(),
        short: "Mix".to_owned(),
        instrument: "drums".to_owned(),
        modes: vec!["mix".to_owned()],
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.mixing.scenes".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// Overview: the kit as its pieces — each collapsed to one strip at
/// working width, the parallel chain gone. What the kit sounds like,
/// five faders. The player's view of a kit.
// r[impl flow.drums.mixing.scenes]
// r[impl flow.scenes.two-audiences]
fn drum_overview() -> Scene {
    let mut rules = per_piece(Effect::at(Size::Working).folded(Fold::Collapsed));
    rules.push(Rule::new(
        Selector {
            kind: Some("process".to_owned()),
            ..Selector::default()
        },
        Effect::hidden(),
    ));
    rules.push(hide_the_bus_tree());
    Scene {
        name: "Drum Overview".to_owned(),
        slug: "drum-overview".to_owned(),
        short: "Over".to_owned(),
        instrument: "drums".to_owned(),
        modes: vec!["mix".to_owned()],
        audience: Audience::Player,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.mixing.scenes".to_owned(),
            "flow.scenes.two-audiences".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// Advanced: the tracks under the pieces that are not the close mics —
/// each Sub, Fund and Trig, and every verb the kit carries — open, the
/// mics and the parallel chain present.
// r[impl flow.drums.mixing.scenes]
fn drum_advanced() -> Scene {
    let mut rules = vec![Rule::new(
        Selector {
            role: Role::Leaf,
            ..under(&KIT)
        },
        Effect::at(Size::Minimum),
    )];
    for kind in ["trigger", "fundamental", "sub"] {
        rules.push(Rule::new(
            Selector {
                kind: Some(kind.to_owned()),
                ..Selector::default()
            },
            Effect::at(Size::Working),
        ));
    }
    rules.push(Rule::new(
        Selector {
            kind: Some("verb".to_owned()),
            role: Role::Leaf,
            ..Selector::default()
        },
        Effect::at(Size::Working),
    ));
    rules.extend([
        // The instrument returns are not the kit's: they came along for
        // the kind, and this scene is about what hangs off the pieces.
        Rule::new(under(&["fx"]), Effect::at(Size::Compact)),
        Rule::new(under(&PROCESS), Effect::at(Size::Minimum)),
        hide_the_bus_tree(),
    ]);
    Scene {
        name: "Drum Advanced".to_owned(),
        slug: "drum-advanced".to_owned(),
        short: "Adv".to_owned(),
        instrument: "drums".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.mixing.scenes".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules,
    }
}

/// The kit's effects: what it is sent to. The room sim in focus, the
/// verb banks at working width, the parallel compressors as tight rails
/// — once a compressor is dialled in it is a volume-balance game, and a
/// rail is a fader — and the kit itself present as rails.
// r[impl flow.drums.mixing.scenes]
fn drum_fx() -> Scene {
    Scene {
        name: "Drum FX".to_owned(),
        slug: "drum-fx".to_owned(),
        short: "FX".to_owned(),
        instrument: "drums".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.drums.mixing.scenes".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Bus,
                    ..Selector::default()
                },
                Effect::at(Size::Minimum),
            ),
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&KIT)
                },
                Effect::at(Size::Minimum),
            ),
            // The snare's three rooms are the one part of the kit this
            // scene is about: picking the room is a mute away.
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&[KIT[0], KIT[1], "Snare", "verb"])
                },
                Effect::at(Size::Working),
            ),
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&PROCESS)
                },
                Effect::at(Size::Minimum),
            ),
            Rule::new(
                Selector {
                    kind: Some("process".to_owned()),
                    ..Selector::default()
                },
                Effect::at(Size::Compact),
            ),
            Rule::new(
                Selector {
                    kind: Some("fx".to_owned()),
                    role: Role::Bus,
                    ..under(&PROCESS)
                },
                Effect::at(Size::Compact),
            ),
            Rule::new(
                Selector {
                    kind: Some("verb".to_owned()),
                    role: Role::Bus,
                    ..under(&PROCESS)
                },
                Effect::at(Size::Compact),
            ),
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&["process", "fx"])
                },
                Effect::at(Size::Working),
            ),
            // The room simulator is the subject: it is the one control
            // that decides what room the kit is in.
            Rule::new(
                Selector {
                    kind: Some("part".to_owned()),
                    role: Role::Leaf,
                    ..under(&["process", "fx"])
                },
                Effect::at(Size::Focus),
            ),
            hide_the_bus_tree(),
        ],
    }
}

/// The mix: the bus tree and nothing else — every bus at working width,
/// the folders that hold them compact, the instruments gone.
fn buses() -> Scene {
    Scene {
        name: "Buses".to_owned(),
        slug: "buses".to_owned(),
        short: "Bus".to_owned(),
        instrument: "bus".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec!["flow.scenes.render".to_owned()],
        default: Effect::at(Size::Working),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Bus,
                    ..Selector::default()
                },
                Effect::at(Size::Compact),
            ),
            Rule::new(
                Selector {
                    kind: Some("group".to_owned()),
                    ..Selector::default()
                },
                Effect::hidden(),
            ),
            Rule::new(
                Selector {
                    kind: Some("process".to_owned()),
                    ..Selector::default()
                },
                Effect::hidden(),
            ),
            Rule::new(under(&["fx"]), Effect::hidden()),
        ],
    }
}

/// The instrument bus: the electrics, acoustics, keys and synths at
/// working width with the instrument returns open beside them, the first
/// plate in focus, and the drums, their parallel chain and the vocals
/// out of the way.
fn guitar_fx() -> Scene {
    let mut rules = vec![Rule::new(
        Selector {
            role: Role::Bus,
            ..Selector::default()
        },
        Effect::at(Size::Compact),
    )];
    for group in [
        &["Guitars", "Electric"][..],
        &["Guitars", "Acoustic"],
        &["Keys"],
        &["Synths"],
        &["fx"],
    ] {
        rules.push(Rule::new(
            Selector {
                role: Role::Leaf,
                ..under(group)
            },
            Effect::at(Size::Working),
        ));
    }
    rules.extend([
        // A return a user picked, so a name: which plate is in front of
        // you is a choice of unit rather than a structure.
        Rule::new(
            Selector {
                name: Some("Fat Plate".to_owned()),
                ..under(&["fx"])
            },
            Effect::at(Size::Focus),
        ),
        // The drums, all of them: the folder the kit is in as well as the
        // kit, and its Process.
        Rule::new(under(&KIT[..1]), Effect::hidden()),
        Rule::new(
            Selector {
                kind: Some("process".to_owned()),
                ..Selector::default()
            },
            Effect::hidden(),
        ),
        Rule::new(under(&["Vocals"]), Effect::hidden()),
        hide_the_bus_tree(),
    ]);
    Scene {
        name: "Guitar FX".to_owned(),
        slug: "guitar-fx".to_owned(),
        short: "GFX".to_owned(),
        instrument: "guitar".to_owned(),
        // Recall-only. Guitar Mixing is what Mix mode opens
        // (`flow.guitars.mixing`); the FX view is a place you go on
        // purpose, not the one you land in.
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec!["flow.scenes.render".to_owned()],
        default: Effect::at(Size::Minimum),
        rules,
    }
}

/// The vocal's taxonomy path.
const VOX: [&str; 2] = ["Vocals", "Lead"];

/// The lead vocal open, every return present as a short rail.
// r[impl flow.vocals.mixing.main]
fn lead_vocal() -> Scene {
    Scene {
        name: "Lead Vocal".to_owned(),
        slug: "lead-vocal".to_owned(),
        short: "Vox".to_owned(),
        instrument: "vocal".to_owned(),
        modes: vec!["mix".to_owned()],
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.vocals.mixing.main".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Compact),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&VOX)
                },
                Effect::at(Size::Working),
            ),
            Rule::new(
                Selector {
                    role: Role::Leaf,
                    ..under(&[VOX[0], VOX[1], "fx"])
                },
                Effect::at(Size::Minimum),
            ),
            // The subject of the scene: its whole chain, top to bottom.
            Rule::new(
                Selector {
                    name: Some("Lead Vox".to_owned()),
                    ..under(&VOX)
                },
                Effect::at(Size::Focus),
            ),
        ],
    }
}

/// Editing the returns: one delay and one verb in focus, every other
/// return at the working width with its chain drawn, every folder a
/// rail. The returns are all live — a slap is a slap whether or not it
/// is the one being edited — so the scene shows them all working and
/// opens the two under the hands.
// r[impl flow.vocals.mixing.fx]
fn lead_vocal_fx() -> Scene {
    Scene {
        name: "Lead Vocal FX Edit".to_owned(),
        slug: "lead-vocal-fx".to_owned(),
        short: "VFX".to_owned(),
        instrument: "vocal".to_owned(),
        // Recall-only, like Guitar FX. Vocal Comping is what Edit
        // opens (#38); dialling a delay in is a thing you go and do.
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: vec![
            "flow.vocals.mixing.fx".to_owned(),
            "flow.scenes.render".to_owned(),
        ],
        default: Effect::at(Size::Working),
        rules: vec![
            Rule::new(
                Selector {
                    role: Role::Bus,
                    ..Selector::default()
                },
                Effect::at(Size::Minimum),
            ),
            Rule::new(
                Selector {
                    name: Some("Lead Vox".to_owned()),
                    ..under(&VOX)
                },
                Effect::at(Size::Focus),
            ),
            Rule::new(
                Selector {
                    name: Some("Short".to_owned()),
                    ..under(&[VOX[0], VOX[1], "fx", "Delay"])
                },
                Effect::at(Size::Focus),
            ),
            Rule::new(
                Selector {
                    name: Some("Long".to_owned()),
                    ..under(&[VOX[0], VOX[1], "fx", "Verb"])
                },
                Effect::at(Size::Focus),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slug distinct and every slug reachable. The count is
    /// asserted so that adding a scene is a deliberate edit here rather
    /// than something that slips in.
    #[test]
    fn every_scene_is_reachable_by_its_slug() {
        assert_eq!(scenes().len(), 36);
        for s in scenes() {
            assert_eq!(
                scene(&s.slug).map(|f| f.slug.as_str()),
                Some(s.slug.as_str())
            );
        }
        assert!(scene("nonesuch").is_none());
    }

    /// The table, spelled out: which scene answers to which
    /// (mode, instrument, audience). Written out rather than derived,
    /// because this IS the follow-mode contract and a change to it
    /// should be a diff someone reads.
    // r[verify flow.scenes.follow-mode]
    // r[verify flow.scenes.two-audiences]
    #[test]
    fn the_table_claims_the_triples_it_is_supposed_to() {
        let claimed: Vec<(&str, &str, Vec<&str>, Audience)> = scenes()
            .iter()
            .map(|s| {
                (
                    s.slug.as_str(),
                    s.instrument.as_str(),
                    s.modes.iter().map(String::as_str).collect(),
                    s.audience,
                )
            })
            .collect();

        // How many scenes at the front of the table are hand-written
        // rather than generated from a `Set`.
        const HAND_WRITTEN: usize = 14;

        // The hand-written scenes, spelled out: each one is a shape
        // somebody decided, and a change to any of them should be a
        // visible edit here.
        assert_eq!(
            &claimed[..HAND_WRITTEN],
            &[
                ("drum-tracking", "drums", vec!["record"], Audience::Engineer),
                (
                    "drum-tracking-overview",
                    "drums",
                    vec!["record"],
                    Audience::Player,
                ),
                ("drum-editing", "drums", vec!["edit"], Audience::Engineer),
                ("drum-mixing", "drums", vec!["mix"], Audience::Engineer),
                ("drum-overview", "drums", vec!["mix"], Audience::Player),
                ("drum-advanced", "drums", vec![], Audience::Engineer),
                ("drum-fx", "drums", vec![], Audience::Engineer),
                ("buses", "bus", vec![], Audience::Engineer),
                // Recall-only: Guitar Mixing is what Mix mode opens.
                ("guitar-fx", "guitar", vec![], Audience::Engineer),
                ("lead-vocal", "vocal", vec!["mix"], Audience::Engineer),
                ("lead-vocal-fx", "vocal", vec![], Audience::Engineer),
                // The one guitar scene that opens the multi-mic level:
                // recall-only, because setting a configuration's
                // balance is a thing you go and do.
                ("guitar-balance", "guitar", vec![], Audience::Engineer),
                (
                    "vocal-tracking",
                    "vocal",
                    vec!["record"],
                    Audience::Engineer,
                ),
                ("vocal-comping", "vocal", vec!["edit"], Audience::Engineer),
            ][..]
        );

        // The generated sets are checked by SHAPE rather than
        // transcribed. Spelling out twenty lines that a loop produced
        // tests the transcription; this tests the generator, which is
        // the thing that could actually be wrong.
        for set in &SETS {
            // Only the generated ones: the hand-written scenes share
            // an instrument with them (Guitar FX and Guitar Balance are
            // both "guitar") and are checked above.
            let mine: Vec<_> = claimed[HAND_WRITTEN..]
                .iter()
                .filter(|(_, instrument, ..)| *instrument == set.instrument)
                .collect();
            assert_eq!(
                mine.len(),
                5,
                "{} should have five scenes, got {mine:?}",
                set.instrument
            );
            let shape: Vec<(&str, Vec<&str>, Audience)> = mine
                .iter()
                .map(|(slug, _, modes, audience)| {
                    (
                        slug.rsplit_once('-').map_or(*slug, |(_, tail)| tail),
                        modes.clone(),
                        *audience,
                    )
                })
                .collect();
            assert_eq!(
                shape,
                vec![
                    ("tracking", vec!["record"], Audience::Engineer),
                    ("overview", vec!["record"], Audience::Player),
                    // Comping is not a mode: its view lives inside
                    // Record and Edit, because a part is comped while
                    // it is still being tracked as often as afterwards.
                    ("comping", vec![], Audience::Engineer),
                    ("editing", vec!["edit"], Audience::Engineer),
                    ("mixing", vec!["mix"], Audience::Engineer),
                ],
                "{}",
                set.instrument
            );
        }
    }

    /// A scene with no mode is recall-only: the number keys reach it,
    /// no mode opens it.
    // r[verify flow.scenes.follow-mode]
    #[test]
    fn a_scene_with_no_mode_is_recall_only() {
        // Every instrument's comping scene is recall-only too:
        // comping is NOT a mode — its view lives inside Record and
        // Edit, because a kit is comped while it is still being
        // tracked as often as afterwards (`flow.scenes.follow-mode`).
        let recall_only: Vec<&str> = scenes()
            .iter()
            .filter(|s| s.modes.is_empty())
            .map(|s| s.slug.as_str())
            .collect();
        assert_eq!(
            recall_only,
            [
                "drum-advanced",
                "drum-fx",
                "buses",
                "guitar-fx",
                "lead-vocal-fx",
                "guitar-balance",
                "bass-comping",
                "guitar-comping",
                "keys-comping",
                "percussion-comping",
                "overview",
                "overview-top"
            ]
        );
    }

    /// A session with the two folders the prelude is about: the Guide
    /// at the top, the Keyflow folder under it, and a kit beside them.
    fn with_the_guide() -> Vec<crate::scenes::Fact> {
        use crate::golden_session::Kind;
        use crate::scenes::{Fact, Segment};
        let guide = Segment::named("Guide").of(Kind::Guide);
        let keyflow = Segment::named("Keyflow").of(Kind::Keyflow);
        vec![
            Fact::folder("guide", "Guide", 0, 0)
                .of(Kind::Guide)
                .at(vec![guide.clone()]),
            Fact::leaf("click", "Click", 1, 1).at(vec![guide.clone()]),
            Fact::leaf("shaker", "Shaker", 2, 1).at(vec![guide]),
            Fact::folder("keyflow", "Keyflow", 3, 0)
                .of(Kind::Keyflow)
                .at(vec![keyflow.clone()]),
            Fact::leaf("chords", "CHORDS", 4, 1).at(vec![keyflow.clone()]),
            Fact::leaf("lines", "LINES", 5, 1).at(vec![keyflow]),
        ]
    }

    /// A kick with the parallel colour hung off it: a Sub, a Verb bank
    /// with two returns inside it, and a Fundamental.
    fn with_the_colour() -> Vec<crate::scenes::Fact> {
        use crate::golden_session::Kind;
        use crate::scenes::{Fact, Segment};
        let kit = Segment::named("Drum Kit").of(Kind::Group);
        let kick = Segment::named("Kick").of(Kind::Piece);
        let under = vec![kit.clone(), kick.clone()];
        let verb = Segment::named("Verb").of(Kind::Verb);
        let inside = vec![kit, kick, verb];
        vec![
            Fact::folder("kick", "Kick", 0, 0)
                .of(Kind::Piece)
                .at(under.clone()),
            Fact::leaf("in", "In", 1, 1)
                .of(Kind::Source)
                .at(under.clone()),
            Fact::leaf("sub", "Sub", 2, 1)
                .of(Kind::Sub)
                .at(under.clone()),
            Fact::leaf("fund", "Fund", 3, 1)
                .of(Kind::Fundamental)
                .at(under.clone()),
            Fact::folder("verb", "Verb", 4, 1).of(Kind::Verb).at(under),
            Fact::leaf("short", "Short", 5, 2)
                .of(Kind::Return)
                .at(inside.clone()),
            Fact::leaf("long", "Long", 6, 2).of(Kind::Return).at(inside),
        ]
    }

    /// The parallel colour keeps its strips and gives up its rows.
    ///
    /// Nothing is recorded on a Sub and nothing is edited on a Verb, so
    /// in the panel they are rows between one drum and the next — and on
    /// a kit five pieces deep they are why the kit does not fit. In the
    /// mixer they are faders, which is the whole point of them.
    ///
    /// A Verb is a FOLDER, so hiding it has to take its returns with it;
    /// a rule that left two orphaned returns behind would be worse than
    /// no rule at all.
    #[test]
    fn drum_mixing_shows_the_parallel_colour_only_in_the_mixer() {
        let facts = with_the_colour();
        let scene = scene("drum-mixing").expect("the drum mixing scene");
        let rows = |surface| -> Vec<String> {
            crate::scenes::resolve(scene, &facts, surface, Some("mix"), None, None)
                .iter()
                .filter_map(crate::scenes::Row::guid)
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(
            rows(crate::scenes::Surface::Arrange),
            ["kick", "in"],
            "the panel is still showing colour it cannot edit"
        );
        assert_eq!(
            rows(crate::scenes::Surface::Mixer),
            ["kick", "in", "sub", "fund", "verb", "short", "long"],
            "the mixer lost a fader it needs"
        );
    }

    /// The electric's rows under the session's first view, by path, in
    /// the golden session.
    fn electric_rows(surface: crate::scenes::Surface) -> Vec<(String, f64)> {
        use crate::golden_session::{maximal, rpp::flatten};
        let flats = flatten(&maximal());
        let facts = crate::scenes::from_flat(&flats);
        let scene = scene("drum-mixing").expect("the drum mixing scene");
        crate::scenes::resolve(scene, &facts, surface, None, None, None)
            .iter()
            .filter_map(|row| {
                let flat = flats.iter().find(|f| Some(f.guid.as_str()) == row.guid())?;
                flat.path.starts_with("Electric").then(|| {
                    (
                        flat.path.clone(),
                        crate::scenes::TABLES.width(row.size, 1440.0),
                    )
                })
            })
            .collect()
    }

    /// The arrangement shows the electric as its arrangement — Rhythm
    /// over Main and Octave, Lead, Solo over Main and Harmony — with
    /// each layer's channels and sources folded into it.
    #[test]
    fn the_arrangement_folds_the_electric_to_its_layers() {
        let paths: Vec<String> = electric_rows(crate::scenes::Surface::Arrange)
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(
            paths,
            [
                "Electric",
                "Electric/Rhythm",
                "Electric/Rhythm/Main",
                "Electric/Rhythm/Octave",
                "Electric/Lead",
                "Electric/Solo",
                "Electric/Solo/Main",
                "Electric/Solo/Harmony",
            ]
        );
    }

    /// The mixer keeps every strip — the fold is the arrangement's — and
    /// a channel and its amps are rails beside the sources.
    #[test]
    fn the_mixer_keeps_the_guitar_sources_as_little_strips() {
        let rows = electric_rows(crate::scenes::Surface::Mixer);
        assert_eq!(rows.len(), 1 + 1 + 2 + 4 * 9 + 3 + 3);
        let rail = crate::scenes::TABLES.width(Size::Minimum, 1440.0);
        for (path, width) in &rows {
            let under_a_layer = [
                "Electric/Rhythm/Main/",
                "Electric/Rhythm/Octave/",
                "Electric/Lead/",
            ]
            .iter()
            .any(|layer| path.starts_with(layer));
            if under_a_layer {
                assert!(
                    (width - rail).abs() < f64::EPSILON,
                    "{path} is {width} wide"
                );
            }
        }
        // The negative control: the folders the part is arranged and
        // processed on are not rails.
        for folder in [
            "Electric",
            "Electric/Rhythm",
            "Electric/Rhythm/Main",
            "Electric/Rhythm/Octave",
            "Electric/Lead",
            "Electric/Solo",
        ] {
            let width = rows
                .iter()
                .find(|(path, _)| path == folder)
                .map_or(0.0, |(_, w)| *w);
            assert!(width > rail, "{folder} is {width} wide");
        }
    }

    /// The paths of the golden session's rows under `prefix` in the
    /// session's first view.
    fn first_view_paths(surface: crate::scenes::Surface, prefix: &str) -> Vec<String> {
        scene_paths("drum-mixing", surface, prefix)
    }

    /// The golden session's rows under `prefix` as `slug` lays them out
    /// on `surface`, by path.
    fn scene_paths(slug: &str, surface: crate::scenes::Surface, prefix: &str) -> Vec<String> {
        use crate::golden_session::{maximal, rpp::flatten};
        let flats = flatten(&maximal());
        let facts = crate::scenes::from_flat(&flats);
        let Some(scene) = scene(slug) else {
            return Vec::new();
        };
        crate::scenes::resolve(scene, &facts, surface, None, None, None)
            .iter()
            .filter_map(|row| {
                let flat = flats.iter().find(|f| Some(f.guid.as_str()) == row.guid())?;
                flat.path.starts_with(prefix).then(|| flat.path.clone())
            })
            .collect()
    }

    /// Overview: one level down — the top folders open, each thing in
    /// them one row, the kit's parallel chain and the monitors gone.
    #[test]
    fn the_overview_is_the_session_one_level_down() {
        use crate::scenes::Surface;
        // The kit is Drums: its pieces one row each, Process not drawn.
        let kit = scene_paths("overview", Surface::Arrange, "Drums");
        assert_eq!(
            kit,
            [
                "Drums",
                "Drums/Kick",
                "Drums/Snare",
                "Drums/Toms",
                "Drums/Cymbals",
                "Drums/Rooms",
            ]
        );
        let electric = scene_paths("overview", Surface::Arrange, "Electric");
        assert!(
            electric.iter().all(|p| p.matches('/').count() <= 1),
            "{electric:?}"
        );
        assert!(
            electric.len() > 1,
            "the electric open over its layers: {electric:?}"
        );
        assert!(scene_paths("overview", Surface::Arrange, "HEADPHONE MIXES").is_empty());
        // Negative control: the mixer keeps every strip.
        assert!(scene_paths("overview", Surface::Mixer, "Drums").len() > kit.len());
    }

    /// Overview TOP: each top folder one row; the Orchestra open over
    /// its sections, each one row.
    #[test]
    fn overview_top_folds_every_top_folder_but_the_orchestra() {
        use crate::scenes::Surface;
        assert_eq!(
            scene_paths("overview-top", Surface::Arrange, "Drums"),
            ["Drums"]
        );
        assert_eq!(
            scene_paths("overview-top", Surface::Arrange, "Vocals"),
            ["Vocals"]
        );
        let orchestra = scene_paths("overview-top", Surface::Arrange, "Orchestra");
        assert!(orchestra.len() > 1, "{orchestra:?}");
        assert!(
            orchestra.iter().all(|p| p.matches('/').count() <= 1),
            "each section one row: {orchestra:?}"
        );
    }

    /// The vocal returns are gone from the arrangement — not a folded
    /// row, no row at all — and every one of them is in the mixer.
    #[test]
    fn the_arrangement_hides_the_vocal_returns_and_the_mixer_keeps_them() {
        use crate::scenes::Surface;
        let arranged = first_view_paths(Surface::Arrange, "Vocals/");
        assert!(
            arranged.iter().all(|p| !p.starts_with("Vocals/Vox FX")),
            "{arranged:?}"
        );
        // The leads beside them are still there.
        assert!(arranged.iter().any(|p| p == "Vocals/Ron"));
        let mixed = first_view_paths(Surface::Mixer, "Vocals/Vox FX");
        // The folder, its Delay (4), Verb (5) and Pitch (2) banks and Wide.
        assert_eq!(mixed.len(), 1 + 1 + 4 + 1 + 5 + 1 + 1 + 2, "{mixed:?}");
    }

    /// The BGVs and the choir are one row each in the arrangement; the
    /// mixer keeps every source.
    #[test]
    fn the_arrangement_folds_each_bgv_part_to_one_row() {
        use crate::scenes::Surface;
        // The folders folded to one row each by default; opened, each
        // part is one row (the part folds hold beneath).
        assert_eq!(first_view_paths(Surface::Arrange, "BGVs"), ["BGVs"]);
        assert_eq!(first_view_paths(Surface::Arrange, "Choir"), ["Choir"]);
        let mixed = first_view_paths(Surface::Mixer, "BGVs");
        assert_eq!(mixed.len(), 1 + 4 * (1 + 3) + 1 + 8, "{mixed:?}");
        assert!(mixed.iter().any(|p| p == "BGVs/Octave Up/ES"));
    }

    /// Each lead is one row, its Main and DBL (and their language
    /// sources) folded into it; the mixer keeps every source.
    #[test]
    fn the_arrangement_folds_each_lead_part_to_one_row() {
        use crate::scenes::Surface;
        for (lead, languages) in [("Ron", 3), ("Belen", 2), ("Aline", 1)] {
            let at = format!("Vocals/{lead}");
            assert_eq!(
                first_view_paths(Surface::Arrange, &at),
                [at.clone()],
                "{lead}"
            );
            assert_eq!(
                first_view_paths(Surface::Mixer, &at).len(),
                1 + 2 * (1 + languages),
                "{lead}"
            );
        }
    }

    /// The kit's parallel chain is out of both surfaces, every row of
    /// it.
    #[test]
    fn the_process_folder_is_out_of_view() {
        use crate::scenes::Surface;
        assert_eq!(first_view_paths(Surface::Mixer, "Process"), [] as [&str; 0]);
        assert_eq!(
            first_view_paths(Surface::Arrange, "Process"),
            [] as [&str; 0]
        );
    }

    /// The bass guitar and the synth bass are a row each in the
    /// arrangement; their DI, amp, sub and synth are strips in the mixer.
    #[test]
    fn the_arrangement_folds_each_bass_to_one_row() {
        use crate::scenes::Surface;
        assert_eq!(
            first_view_paths(Surface::Arrange, "Bass"),
            ["Bass", "Bass/Guitar", "Bass/Synth"]
        );
        assert_eq!(
            first_view_paths(Surface::Mixer, "Bass"),
            [
                "Bass",
                "Bass/Guitar",
                "Bass/Guitar/DI",
                "Bass/Guitar/Amp",
                "Bass/Synth",
                "Bass/Synth/Sub",
                "Bass/Synth/Synth",
            ]
        );
    }

    /// Every scene shows the Guide folder collapsed to one row, and no
    /// flow in the table opens it — opening it is a flow of its own.
    // r[verify flow.scenes.guide-folder]
    #[test]
    fn every_scene_collapses_the_guide_folder() {
        let facts = with_the_guide();
        for scene in scenes() {
            let rows = crate::scenes::resolve(
                scene,
                &facts,
                crate::scenes::Surface::Mixer,
                Some("mix"),
                None,
                None,
            );
            let guide: Vec<&str> = rows
                .iter()
                .filter_map(crate::scenes::Row::guid)
                .filter(|g| matches!(*g, "guide" | "click" | "shaker"))
                .collect();
            assert_eq!(
                guide,
                ["guide"],
                "{} shows the click and the shaker",
                scene.slug
            );
        }
    }

    /// Every scene shows the Keyflow folder collapsed too — except in
    /// Write and Produce, whose subject is the song's knowledge.
    // r[verify flow.scenes.keyflow-folder]
    #[test]
    fn write_and_produce_are_the_only_modes_that_open_keyflow() {
        let facts = with_the_guide();
        let items = |mode: &str| {
            crate::scenes::resolve(
                scenes().first().expect("a scene"),
                &facts,
                crate::scenes::Surface::Mixer,
                Some(mode),
                None,
                None,
            )
            .iter()
            .filter_map(crate::scenes::Row::guid)
            .filter(|g| matches!(*g, "keyflow" | "chords" | "lines"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
        };
        assert_eq!(items("mix"), ["keyflow"]);
        assert_eq!(items("record"), ["keyflow"]);
        assert_eq!(items("write"), ["keyflow", "chords", "lines"]);
        assert_eq!(items("produce"), ["keyflow", "chords", "lines"]);
    }

    /// The prelude is the same two rules in every mode, plus the one
    /// that opens the Keyflow folder where the song's knowledge is the
    /// subject.
    // r[verify flow.scenes.keyflow-folder]
    #[test]
    fn write_and_produce_open_the_keyflow_folder() {
        assert_eq!(prelude(None, None).len(), 2);
        assert_eq!(prelude(Some("mix"), None).len(), 2);
        for mode in ["write", "produce"] {
            let rules = prelude(Some(mode), None);
            assert_eq!(rules.len(), 3, "{mode}");
            assert_eq!(
                rules.last().map(|r| r.effect.fold),
                Some(Some(Fold::Open)),
                "{mode} opens it"
            );
        }
    }
}
