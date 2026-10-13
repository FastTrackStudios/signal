//! The fixture song shape: what the album puts under each template node.
//!
//! The template tree ([`golden_template`](crate::golden::golden_template))
//! knows the groups, their sub-groups, their bus attachments and their
//! capture vocabularies (`Kick`'s SUM is `In, Out, Trig`). It does not
//! know that *this* song's kick also has a Sub and a Verb beside its Sum,
//! or that its rooms are two stereo pairs: that is the song's shape, and
//! it lives here. A [`Node`] that names a `template` path stands for that
//! template group and is checked against it
//! ([`Shape::resolve`]); a node without one is the song's own folder — a
//! Sum, a Verb bank, the kit's Process — and carries its kind directly.
//!
//! The shapes below are the two the Python writers produced until this
//! module replaced them: [`maximal`] (`make-template-rpp.py`) and
//! [`vocal_fx`] (`make-vocal-fx-rpp.py`). Their layout rules — which
//! tracks are auxiliaries, which are pieces, how wide a strip opens —
//! are ported with their reasoning, because the scene renders committed
//! beside them were taken against exactly this layout.

use dynamic_template_proto::{IdealFullSessionTemplate, TemplateNode};

use super::kind::Kind;

/// REAPER track-grouping roles a node can take (spec: the instrument
/// folder is the VCA lead of its bus, with mute and solo along for the
/// ride).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// VCA, mute and solo lead of group `n` (1-based).
    VcaLead(u32),
    /// VCA, mute and solo follower of group `n` (1-based).
    VcaFollow(u32),
}

/// A plugin written into a track's chain by name, at default state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fx {
    /// The plugin's display name, e.g. `ReaEQ (Cockos)`.
    pub name: &'static str,
    /// The plugin file REAPER resolves it by, e.g. `reaeq.so`.
    pub file: &'static str,
}

/// Where a track's audio goes.
///
/// One value rather than a send and two flags, because only four of the
/// eight combinations mean anything and the other four are bugs waiting:
/// a dead end that also sends, a kept parent with nothing to keep it
/// beside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Routing {
    /// Into the folder above — the usual case.
    #[default]
    Parent,
    /// An explicit send to the named track, parent send off: the track
    /// reaches the mix through its bus, not through the folder.
    Send(&'static str),
    /// An explicit send with the parent send left on, so the folder
    /// above still sums (and meters) what it holds while the audio that
    /// reaches the mix goes by the send.
    SendKeepingParent(&'static str),
    /// Parent send off and no send: a dead end whose fader and meter are
    /// still real, for a folder that is a group lead rather than a sum.
    DeadEnd,
}

impl Routing {
    /// The track this one sends to, if any.
    #[must_use]
    pub const fn send(self) -> Option<&'static str> {
        match self {
            Self::Send(to) | Self::SendKeepingParent(to) => Some(to),
            Self::Parent | Self::DeadEnd => None,
        }
    }

    /// Whether the audio also reaches the folder above.
    #[must_use]
    pub const fn parent_send(self) -> bool {
        matches!(self, Self::Parent | Self::SendKeepingParent(_))
    }
}

/// One node of the fixture tree: a folder when it has children, a track
/// when it does not.
#[derive(Debug, Clone)]
pub struct Node {
    /// The track name.
    pub name: String,
    /// RGB colour, `0xRRGGBB`.
    pub colour: u32,
    /// The taxonomy kind written to ext-state.
    pub kind: Kind,
    /// The template group this node stands for, if any.
    pub template: Option<&'static [&'static str]>,
    /// Children, in order.
    pub children: Vec<Self>,
    /// Where the audio goes.
    pub routing: Routing,
    /// Aux sends, beside the routing: the `/`-joined paths of the
    /// returns this track feeds, post-fader.
    ///
    /// By PATH rather than by name, unlike [`Routing::Send`]: a bus name
    /// is unique in the session, a return's is not — the vocal Delay and
    /// Verb both have a `Short` and a `Long`, and the kick's and the
    /// toms' Verbs both have a `Room`.
    pub aux: Vec<String>,
    /// REAPER track grouping.
    pub group: Option<GroupRole>,
    /// A stereo pair: one two-channel track on a stereo input.
    pub stereo: bool,
    /// The FX chain, by name.
    pub fx: Vec<Fx>,
    /// No items at all — a lane that is real and waiting.
    pub no_items: bool,
    /// In the mixer and not the track panel: mixing, not arranging.
    pub mixer_only: bool,
    /// Whether this track's items hold MIDI.
    ///
    /// Not derivable from `Kind`: a trigger is always MIDI, but the
    /// Guide and Keyflow tracks are `Source` like a close mic and hold
    /// notes rather than audio. The checklist has asserted they do
    /// since it was written, and nothing made it true.
    pub midi: bool,
    /// The mixer strip's width, when the node's place in the part says
    /// otherwise than the layout's rule ([`super::rpp::strip_width`]).
    ///
    /// The layout reads a strip's width off the tree's shape — a piece
    /// at the tone width, a folder at the head width — and a guitar's
    /// depth defeats that: every source under a channel is a leaf, so
    /// every one of the twenty-eight opens as a piece, while the layer
    /// that actually carries the chain is a folder. So the guitar says
    /// it here, as data, rather than as a name rule in the writer.
    pub strip: Option<u32>,
}

impl Node {
    /// A node with no children and no routing options.
    #[must_use]
    pub fn new(name: impl Into<String>, colour: u32, kind: Kind) -> Self {
        Self {
            name: name.into(),
            colour,
            kind,
            template: None,
            children: Vec::new(),
            routing: Routing::Parent,
            aux: Vec::new(),
            group: None,
            stereo: false,
            fx: Vec::new(),
            no_items: false,
            mixer_only: false,
            midi: false,
            strip: None,
        }
    }

    /// The template group path this node stands for.
    #[must_use]
    pub const fn template(mut self, path: &'static [&'static str]) -> Self {
        self.template = Some(path);
        self
    }

    /// The node's children.
    #[must_use]
    pub fn children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }

    /// An explicit send, parent send off.
    #[must_use]
    pub const fn send(mut self, to: &'static str) -> Self {
        self.routing = Routing::Send(to);
        self
    }

    /// Aux sends to the returns at these paths, beside the routing.
    #[must_use]
    pub fn aux(mut self, returns: Vec<String>) -> Self {
        self.aux = returns;
        self
    }

    /// Keep the parent send on beside the explicit send.
    #[must_use]
    pub const fn keep_parent(mut self) -> Self {
        if let Routing::Send(to) = self.routing {
            self.routing = Routing::SendKeepingParent(to);
        }
        self
    }

    /// A dead end: parent send off, no send.
    #[must_use]
    pub const fn dead_end(mut self) -> Self {
        self.routing = Routing::DeadEnd;
        self
    }

    /// No items, its routing as it is: a return, fed by sends, that has
    /// nothing recorded on it.
    #[must_use]
    pub const fn without_items(mut self) -> Self {
        self.no_items = true;
        self
    }

    /// In the mixer only, out of the track panel.
    #[must_use]
    pub const fn mixer_only(mut self) -> Self {
        self.mixer_only = true;
        self
    }

    /// This track's items are notes, not audio.
    #[must_use]
    pub const fn midi(mut self) -> Self {
        self.midi = true;
        self
    }

    /// This track has no items at all.
    ///
    /// For a lane that is real and waiting: LINES and HITS exist so a
    /// person has somewhere to put a line or a hit, and a generator
    /// filling them would be composing.
    #[must_use]
    pub const fn empty(mut self) -> Self {
        self.routing = Routing::DeadEnd;
        self.no_items = true;
        self
    }

    /// REAPER track grouping.
    #[must_use]
    pub const fn group(mut self, role: GroupRole) -> Self {
        self.group = Some(role);
        self
    }

    /// Whether this node is a bus: no items, unarmed.
    #[must_use]
    pub fn is_bus(&self) -> bool {
        self.kind == Kind::Bus
    }

    /// A stereo pair on one track.
    #[must_use]
    pub const fn stereo(mut self) -> Self {
        self.stereo = true;
        self
    }

    /// The FX chain, by name.
    #[must_use]
    pub fn fx(mut self, chain: &[Fx]) -> Self {
        self.fx = chain.to_vec();
        self
    }

    /// A little strip: a track with nothing on it to read — a source
    /// under a guitar's channel, the channel itself — needs its fader
    /// and its mute and no more.
    #[must_use]
    pub const fn narrow(mut self) -> Self {
        self.strip = Some(MIN_WIDTH);
        self
    }

    /// A strip wide enough for the Tone rack: the track the part's
    /// chain sits on, folder or not.
    #[must_use]
    pub const fn toned(mut self) -> Self {
        self.strip = Some(TONE_WIDTH);
        self
    }

    /// Whether this node is a folder.
    #[must_use]
    pub const fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }
}

/// A whole fixture: the tree and the song around it.
#[derive(Debug, Clone)]
pub struct Shape {
    /// The fixture's name; also the RPP file stem.
    pub name: &'static str,
    /// Tempo, 4/4.
    pub bpm: f64,
    /// Song length in bars.
    pub bars: u32,
    /// The song's sections, on ruler lane 2; empty for a fixture with
    /// no arrangement.
    pub sections: Vec<Section>,
    /// The track that opens selected.
    pub selected: &'static str,
    /// How the mixer lays the fixture out — see [`Layout`].
    pub layout: Layout,
    /// The tree.
    pub roots: Vec<Node>,
}

/// One section of the song, as a region on the ruler's SECTIONS lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    /// What the section is called — `Verse 1`, `Chorus 2`.
    pub name: &'static str,
    /// The bar it starts on.
    pub start: u32,
    /// The bar it ends on.
    pub end: u32,
    /// RGB colour, `0xRRGGBB`.
    pub colour: u32,
}

impl Section {
    /// A section from bar `start` to bar `end`.
    #[must_use]
    pub const fn new(name: &'static str, start: u32, end: u32, colour: u32) -> Self {
        Self {
            name,
            start,
            end,
            colour,
        }
    }
}

/// The layout rules a fixture was rendered under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The maximal session's: auxiliaries at the minimum height and
    /// width, pieces at the tone width, folders at the head width, mics
    /// at the minimum; close mics armed on a rotating input.
    Maximal,
    /// The vocal template's: folders at the head width, the lead at the
    /// tone width, everything else at the minimum; only the lead armed.
    VocalFx,
}

/// The narrowest a strip may be set to — `Layout::strip_min` in the
/// window. Ours, so the number is ours too.
pub const MIN_WIDTH: u32 = 30;
/// A strip wide enough to hold the Tone rack.
///
/// Must equal `tone::WORKING` in the renderer, so selecting a piece does
/// not resize it. 133 is the budget: twelve pieces at 133 beside the
/// mics, the auxiliaries and the folders fit a 2560-wide display with
/// slack.
pub const TONE_WIDTH: u32 = 133;
/// A folder is a bus: its level and its mute, no button column.
pub const FOLDER_WIDTH: u32 = 56;
/// "As small as this host allows", not a number: one pixel is below
/// every floor there is, so each host clamps it to its own.
pub const MIN_HEIGHT: i32 = 1;

// Drums: one red family, ordered the way the kit is stacked.
const DRUMS: u32 = 0x00B2_3A3F;
const KICK: u32 = 0x008C_2F35;
const SNARE: u32 = 0x00C9_4540;
const TOMS: u32 = 0x00B8_613F;
const CYMBALS: u32 = 0x00C7_6B7A;
const ROOMS: u32 = 0x0093_425C;
const PROCESS: u32 = 0x007A_2E3A;
/// A cool slate for the bus tree: plumbing, not another instrument.
const BUS: u32 = 0x003E_4C5E;
const BASS: u32 = 0x006B_8E3F;
// Session order and hue order: electrics blue, acoustics seafoam, keys
// green. Synths are purple, the catalogue's violet family, and so is a
// synth wherever it sits — the synth bass under Bass included.
const ELECTRIC: u32 = 0x003A_6FB5;
const ACOUSTIC: u32 = 0x003F_A9A0;
const KEYS: u32 = 0x004E_9A55;
/// Muted against the catalogue's violet-500, which the vocal Verb
/// returns already wear.
const SYNTHS: u32 = 0x007E_57C2;
const VOX: u32 = 0x00B0_4A6A;
/// The background vocals and the choir: the voice's rose, each a shade
/// off it — the BGVs lighter, the choir deeper — so the three vocal
/// folders sit together and still read apart.
const BGV: u32 = 0x00C2_5F80;
const CHOIR: u32 = 0x0092_3D5A;
/// The monitor side of the session — the count, the song's knowledge and
/// the cue mixes. Deliberately dim: present at the top of every scene,
/// read only when a flow opens one of them.
const GUIDE: u32 = 0x006A_6A78;
const KEYFLOW: u32 = 0x0058_6A84;
const MONITOR: u32 = 0x004A_4A55;
// The vocal template's returns take their unit's colour: delay blue,
// reverb purple — the inks the visualisers are drawn in.
const VOX_FX: u32 = 0x0064_748B;
const DELAY: u32 = 0x003B_82F6;
const VERB: u32 = 0x008B_5CF6;
const WIDE: u32 = 0x0022_C55E;
const PITCH: u32 = 0x00EC_4899;
// The orchestra by section, each in the colour its instruments are
// made of: strings wood brown, brass gold, winds a light air blue,
// percussion a struck red-orange. Muted to sit beside the kit's reds
// and the electrics' blue rather than shout over them.
const STRINGS: u32 = 0x008B_5A2B;
/// The orchestra folder: brown like its strings, a shade warmer and
/// lighter so the folder and its first section read apart.
const ORCHESTRA: u32 = 0x00A0_6B3C;
const BRASS: u32 = 0x00C9_9A1E;
const WINDS: u32 = 0x007E_B8DA;
const ORCH_PERC: u32 = 0x00D9_5A2B;

/// REAPER's own EQ and compressor, by name — the chain every bus
/// carries (EQ → Comp).
pub const BUS_CHAIN: &[Fx] = &[
    Fx {
        name: "ReaEQ (Cockos)",
        file: "reaeq.so",
    },
    Fx {
        name: "ReaComp (Cockos)",
        file: "reacomp.so",
    },
];

/// A one-note track's chain: Gate → band-pass at the fundamental → Sat.
pub const ONE_NOTE_CHAIN: &[Fx] = &[
    Fx {
        name: "ReaGate (Cockos)",
        file: "reagate.so",
    },
    Fx {
        name: "ReaEQ (Cockos)",
        file: "reaeq.so",
    },
    Fx {
        name: "Saturation",
        file: "saturation",
    },
];

fn mic(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Source)
}

fn trig(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Trigger)
}

/// The drum's samples, beside its `Trig`: the notes a sampler plays.
fn sample(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Sample)
}

fn fund(colour: u32) -> Node {
    Node::new("Fund", colour, Kind::Fundamental).fx(ONE_NOTE_CHAIN)
}

fn verb(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Verb)
}

fn part(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Part)
}

/// A stereo pair: ONE stereo track, not a folder over an L and an R.
fn pair(name: &str, colour: u32) -> Node {
    part(name, colour).stereo()
}

/// A kit piece: its close mics under a Sum, with the sends beside it.
///
/// The Sum is a folder of the mics; anything the piece sends to — a
/// Sub, a Verb — sits NEXT to the Sum rather than inside it, because it
/// is fed by the sum and is not one of the things being summed.
fn summed(name: &str, colour: u32, sum: Vec<Node>, beside: Vec<Node>) -> Node {
    let mut children = vec![Node::new("Sum", colour, Kind::Sum).children(sum)];
    children.extend(beside);
    Node::new(name, colour, Kind::Piece).children(children)
}

/// A tom: its mic, its trigger, its samples and its fundamental, and
/// nothing else. No
/// Sum and no Verb of its own — the piece IS the pair, and the toms
/// share one Verb bank beside them at the Toms level.
fn tom(number: u32) -> Node {
    Node::new(format!("Tom {number}"), TOMS, Kind::Piece).children(vec![
        mic(&format!("T{number}"), TOMS),
        trig(&format!("T{number} Trig"), TOMS),
        sample(&format!("T{number} Sample"), TOMS),
        fund(TOMS),
    ])
}

fn returns(folder: &str, colour: u32, kind: Kind, names: &[&str]) -> Node {
    Node::new(folder, colour, Kind::Fx)
        .children(names.iter().map(|n| Node::new(*n, colour, kind)).collect())
}

fn bus(name: &str, colour: u32) -> Node {
    Node::new(name, colour, Kind::Bus).fx(BUS_CHAIN)
}

/// The drums: every piece a folder, each piece's close mics under a Sum
/// with its sends beside rather than inside it — and, after the pieces,
/// the kit's [`process`] chains, in the mixer only.
///
/// One kit is no folder of its own: the pieces are Drums' (a level is a
/// folder only when it has more than one member). They still stand for
/// the template's `Drums/Drum Kit/…` groups, which is what a scene
/// selects them by.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
#[must_use]
pub fn drums() -> Node {
    Node::new("Drums", DRUMS, Kind::Group)
        .template(&["Drums"])
        .send("DRUM BUS")
        .children(vec![
            summed(
                "Kick",
                KICK,
                vec![
                    mic("In", KICK),
                    mic("Out", KICK),
                    trig("Trig", KICK),
                    sample("Sample", KICK),
                ],
                vec![
                    Node::new("Sub", KICK, Kind::Sub).fx(ONE_NOTE_CHAIN),
                    // A folder of one, like the snare's and the toms':
                    // every piece's Verb is a bank of its rooms, and the
                    // name is what the return's reverb opens on — a
                    // kick is put in a room.
                    verb("Verb", KICK).children(vec![verb("Room", KICK)]),
                ],
            )
            .template(&["Drums", "Drum Kit", "Kick"]),
            // The snare's verb is a folder of three: a plate, a hall and
            // a nonlin — the three rooms a snare is put in, so the one
            // the song wants is a mute away. Each opens on its own
            // algorithm, which its name picks.
            summed(
                "Snare",
                SNARE,
                vec![
                    mic("Top", SNARE),
                    mic("Bottom", SNARE),
                    trig("Trig", SNARE),
                    sample("Sample", SNARE),
                    fund(SNARE),
                ],
                vec![verb("Verb", SNARE).children(vec![
                    verb("Plate", SNARE),
                    verb("Hall", SNARE),
                    verb("Nonlin", SNARE),
                ])],
            )
            .template(&["Drums", "Drum Kit", "Snare"]),
            Node::new("Toms", TOMS, Kind::Group)
                .template(&["Drums", "Drum Kit", "Toms"])
                .children(vec![
                    tom(1),
                    tom(2),
                    tom(3),
                    tom(4),
                    // Shared by the four: a room for the fills and a
                    // nonlin for the big ones.
                    verb("Verb", TOMS).children(vec![verb("Room", TOMS), verb("Nonlin", TOMS)]),
                ]),
            Node::new("Cymbals", CYMBALS, Kind::Group)
                .template(&["Drums", "Drum Kit", "Cymbals"])
                .children(vec![
                    part("OH", CYMBALS),
                    part("Hi-Hat", CYMBALS),
                    part("Ride", CYMBALS),
                ]),
            Node::new("Rooms", ROOMS, Kind::Group)
                .template(&["Drums", "Drum Kit", "Rooms"])
                .children(vec![pair("Rooms", ROOMS), pair("Rooms Far", ROOMS)]),
            process(),
        ])
}

/// What the kit is sent to: parallel compressors from dry to crushed,
/// and an FX folder with a fake room and a bank of reverbs to pick the
/// room the band is in.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn process() -> Node {
    Node::new("Process", PROCESS, Kind::Process)
        .mixer_only()
        .children(vec![
            Node::new("Compress", PROCESS, Kind::Compress).children(
                ["Dry", "Tight", "Punch", "Smash", "Crunch"]
                    .iter()
                    .map(|n| part(n, PROCESS).without_items())
                    .collect(),
            ),
            Node::new("FX", PROCESS, Kind::Fx).children(vec![
                part("Room Sim", PROCESS).without_items(),
                verb("Verb", PROCESS).children(
                    [
                        "Wood Room",
                        "Music Club",
                        "Stadium",
                        "RMX 16",
                        "Nonlin",
                        "Brick Wall",
                    ]
                    .iter()
                    .map(|n| verb(n, PROCESS).without_items())
                    .collect(),
                ),
            ]),
        ])
}

/// Bass, maximally: a bass guitar (its DI and its amp) and a synth
/// bass (its sub and the synth), to the bass bus.
///
/// The bass guitar has no Sum: the folder IS the piece, summing the two
/// captures of one performance itself. A Sum is the kit's — a piece
/// with sends beside its mics needs one to keep them apart, and a bass
/// has nothing beside them.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn bass() -> Node {
    Node::new("Bass", BASS, Kind::Group)
        .template(&["Bass"])
        .send("BASS BUS")
        .children(vec![
            Node::new("Guitar", BASS, Kind::Piece)
                .template(&["Bass", "Guitar"])
                .children(vec![mic("DI", BASS), mic("Amp", BASS)]),
            // A synth first: purple like every synth, on the bass bus.
            Node::new("Synth", SYNTHS, Kind::Piece)
                .template(&["Bass", "Synth"])
                .children(vec![
                    Node::new("Sub", SYNTHS, Kind::Sub).fx(ONE_NOTE_CHAIN),
                    part("Synth", SYNTHS),
                ]),
        ])
}

/// The electrics: each part a track, each to one of the three electric
/// buses by a send, the folder a dead end that meters and leads its bus.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn electric() -> Node {
    Node::new("Electric", ELECTRIC, Kind::Group)
        .template(&["Guitars", "Electric"])
        .dead_end()
        .group(GroupRole::VcaLead(1))
        .children(vec![rhythm(), lead_part(), solo()])
}

/// The seven sources one channel of a rhythm guitar is captured on.
///
/// A DI, a pedalboard, and two amps each with a 57 and a 121 — before
/// the part is even doubled. This is the level `Guitar Mixing` hides
/// and `Guitar Balance` exists to open.
///
/// The mics are `SM57` and `Royer` rather than `57` and `R121`, and
/// that is not a preference: a bare `57` also matches the mic named
/// inside a capture's own file name (`18.EG2 (57).wav`), and `R121`
/// reads as the **R channel**. The names have to survive the classifier
/// they will be read back through.
///
/// Everything here is a guitar concern rather than an arrangement one:
/// the chain is on the layer above, so the channel, its amps and its
/// sources carry nothing and open as little strips.
fn seven_sources(channel: &str) -> Node {
    let source = |name: &str| Node::new(name, ELECTRIC, Kind::Source).narrow();
    let amp = |name: &str| {
        Node::new(name, ELECTRIC, Kind::Group)
            .narrow()
            .children(vec![source("SM57"), source("Royer")])
    };
    Node::new(channel, ELECTRIC, Kind::Group)
        .narrow()
        .children(vec![
            source("DI"),
            source("Pedalboard"),
            amp("Amp 1"),
            amp("Amp 2"),
        ])
}

/// **Rhythm**: the maximal guitar shape.
///
/// An arrangement over two layers (Main and Octave doing the same part
/// an octave apart), each over L and R channels, each channel over its
/// seven sources. Four channels, twenty-eight source tracks, one part —
/// which is the depth every guitar scene and gesture has to work at
/// without knowing it is deep.
///
/// The arrangement and its layers are where the part is processed, so
/// they open at the tone width; everything under a layer is a little
/// strip.
fn rhythm() -> Node {
    Node::new("Rhythm", ELECTRIC, Kind::Group)
        .send("GTR RHYTHM")
        .toned()
        .children(vec![
            Node::new("Main", ELECTRIC, Kind::Group)
                .toned()
                .children(vec![seven_sources("L"), seven_sources("R")]),
            Node::new("Octave", ELECTRIC, Kind::Group)
                .toned()
                .children(vec![seven_sources("L"), seven_sources("R")]),
        ])
}

/// **Lead**: a double-tracked DI-only part.
///
/// L and R carry items themselves, with **no folder under them** — a
/// level is a folder only when it has more than one member, and a
/// channel captured one way has nothing to hold. The same scenes and
/// gestures must work here as on Rhythm, which is the point of having
/// both in the fixture.
///
/// Lead is its own layer: the chain is on it, and its two channels are
/// little strips.
fn lead_part() -> Node {
    Node::new("Lead", ELECTRIC, Kind::Group)
        .send("GTR LEAD")
        .toned()
        .children(vec![
            Node::new("L", ELECTRIC, Kind::Source).narrow(),
            Node::new("R", ELECTRIC, Kind::Source).narrow(),
        ])
}

/// **Solo**: the smallest shape — one DI track, and a harmony beside it.
///
/// An arrangement with a Main layer and a Harmony layer of one track
/// each. No channel level at all, because neither layer is doubled.
/// The two layers carry their own chains; Solo itself is only the
/// folder over them.
fn solo() -> Node {
    Node::new("Solo", ELECTRIC, Kind::Group)
        .send("GTR SOLO")
        .children(vec![
            Node::new("Main", ELECTRIC, Kind::Source).toned(),
            Node::new("Harmony", ELECTRIC, Kind::Source).toned(),
        ])
}

/// The acoustics: the same shape as the electrics, on their own bus.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn acoustic() -> Node {
    Node::new("Acoustic", ACOUSTIC, Kind::Group)
        .template(&["Guitars", "Acoustic"])
        .dead_end()
        .group(GroupRole::VcaLead(2))
        .children(vec![
            part("Steel", ACOUSTIC).send("ACOUSTIC BUS"),
            part("Nylon", ACOUSTIC).send("ACOUSTIC BUS"),
            // High-strung, doubling the steel an octave up: a layer, not
            // a second guitar.
            part("Nashville", ACOUSTIC).send("ACOUSTIC BUS"),
        ])
}

/// Keys: a stereo piano, a Rhodes and an organ, to the keys bus.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn keys() -> Node {
    Node::new("Keys", KEYS, Kind::Group)
        .template(&["Keys"])
        .send("KEYS BUS")
        .children(vec![
            // Piano is a stereo TRACK, not a folder over an L and an
            // R: two mics on one instrument are one capture of one
            // thing (`flow.scenes.reaper-model`).
            // MIDI, every one: the song's chords as parts
            // (`golden_session::song`), so the editor has notes to show.
            pair("Piano", KEYS).template(&["Keys", "Piano"]).midi(),
            part("Rhodes", KEYS)
                .template(&["Keys", "Electric", "Rhodes"])
                .midi(),
            part("Wurli", KEYS)
                .template(&["Keys", "Electric", "Wurlitzer"])
                .midi(),
            part("Organ", KEYS).template(&["Keys", "Organ"]).midi(),
        ])
}

/// Synths: a pad, a lead and an arp, to the same bus as the keys.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
fn synths() -> Node {
    Node::new("Synths", SYNTHS, Kind::Group)
        .template(&["Synths"])
        .send("KEYS BUS")
        .children(vec![
            // A family is the MIXING level, a synth the tracking and
            // editing one — so the families are folders and the synths
            // that belong to no family sit beside them at the top,
            // where they are still reachable without inventing a
            // family to hold them (`flow.synths.families`).
            part("Sub Bass Synth", SYNTHS).midi(),
            part("Texture", SYNTHS).midi(),
            Node::new("SY Arps", SYNTHS, Kind::Group).children(vec![part("Arp", SYNTHS).midi()]),
            Node::new("SY Pads", SYNTHS, Kind::Group).children(vec![part("Pad", SYNTHS).midi()]),
            Node::new("SY Leads", SYNTHS, Kind::Group)
                .children(vec![part("Lead Synth", SYNTHS).midi()]),
            Node::new("SY Chords", SYNTHS, Kind::Group)
                .children(vec![part("Chord", SYNTHS).midi()]),
        ])
}

/// Percussion: its own folder beside the drums, its own bus.
///
/// Beside rather than inside, because percussion is tracked one
/// instrument at a time and comps on the track like a bass — the kit's
/// folder comping would be the wrong gesture for a shaker
/// (`flow.percussion.folder`).
fn percussion() -> Node {
    Node::new("Percussion", TOMS, Kind::Group)
        .template(&["Percussion"])
        .send("PERC BUS")
        .children(vec![
            part("Shaker", TOMS),
            part("Tambourine", TOMS),
            part("Claps", TOMS),
        ])
}

/// The orchestra's golden shape: four sections, one bus each.
///
/// Only the SHAPE is in scope here. Divisi, seating, spot mics and
/// scoring to picture are an effort of their own; what the golden
/// session owes is a tree every orchestra scene can be rendered
/// against (`flow.orchestra.golden`).
fn orchestra() -> Node {
    Node::new("Orchestra", ORCHESTRA, Kind::Group)
        .template(&["Orchestra"])
        .children(vec![
            Node::new("Winds", WINDS, Kind::Group)
                .send("WINDS BUS")
                .children(vec![
                    part("Flute", WINDS),
                    part("Oboe", WINDS),
                    part("Clarinet", WINDS),
                    part("Bassoon", WINDS),
                ]),
            Node::new("Brass", BRASS, Kind::Group)
                .send("BRASS BUS")
                .children(vec![
                    part("Trumpets", BRASS),
                    part("Horns", BRASS),
                    part("Trombones", BRASS),
                    part("Tuba", BRASS),
                ]),
            Node::new("Strings", STRINGS, Kind::Group)
                .send("STRINGS BUS")
                .children(vec![
                    part("Violin 1", STRINGS),
                    part("Violin 2", STRINGS),
                    part("Viola", STRINGS),
                    part("Cello", STRINGS),
                    part("Bass", STRINGS),
                ]),
            // Present and empty: the section exists in the shape so a
            // scene can render it, and a session fills it or does not.
            Node::new("Orch Percussion", ORCH_PERC, Kind::Group)
                .send("ORCH PERC BUS")
                .children(vec![part("Timpani", ORCH_PERC)]),
        ])
}

/// The lead vocal and the layers beside it, to the lead vocal bus.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
/// The three languages every vocal source is sung in, plus the
/// language-free `All`.
///
/// `All` is not a fourth language: it is a wordless part — a "Hey!", a
/// hummed pad — that belongs to every version, and it is what the
/// switch must never mute (`flow.vocals.language`).
const LANGUAGES: [&str; 3] = ["EN", "ES", "PT"];

/// One performer's layer: a mix track with a source per language under
/// it.
///
/// The LAYER carries the chain and the sends; the language tracks under
/// it are sources only. That is what makes the English and the Spanish
/// vocal come out the same, and it is most of the CPU — one chain per
/// performer instead of one per language.
fn layer(name: &str, languages: &[&str]) -> Node {
    layer_in(name, languages, VOX)
}

/// A part over its language sources, in `colour`.
fn layer_in(name: &str, languages: &[&str], colour: u32) -> Node {
    Node::new(name, colour, Kind::Group).children(
        languages
            .iter()
            .map(|code| Node::new(*code, colour, Kind::Source))
            .collect(),
    )
}

/// A lead: a performer's mix track, a Main and a DBL under it, each
/// with a source per language they actually sing.
///
/// A performer who does not sing a language simply has no source track
/// in it — Aline sings only the Portuguese version. Absence is the
/// representation; there is no empty placeholder to mistake for one.
///
/// The mix track sends to every vocal return (`flow.vocals.leads`: the
/// performer's mix track carries the chain and the sends), so whichever
/// language is up, it is the same vocal into the same delays and verbs.
fn lead(name: &str, languages: &[&str], returns: &[String]) -> Node {
    Node::new(name, VOX, Kind::Group)
        .template(&["Vocals", "Lead"])
        .aux(returns.to_vec())
        .children(vec![layer("Main", languages), layer("DBL", languages)])
}

/// The vocal returns: delays slap to throw, verbs room to throw, a
/// widener and the octaves — the vocal template's `Vox FX`, one
/// definition for both fixtures.
#[must_use]
pub fn vox_fx() -> Node {
    Node::new("Vox FX", VOX_FX, Kind::Fx).children(vec![
        returns(
            "Delay",
            DELAY,
            Kind::Return,
            &["Slap", "Short", "Long", "Throw"],
        ),
        returns(
            "Verb",
            VERB,
            Kind::Verb,
            &["Room", "Short", "Long", "Moment", "Throw"],
        ),
        Node::new("Wide", WIDE, Kind::Return),
        returns("Pitch", PITCH, Kind::Return, &["Oct+", "Oct-"]),
    ])
}

/// The paths of every return under `node` — its leaves — with `node`
/// itself at `at`.
fn return_paths(node: &Node, at: &str) -> Vec<String> {
    if node.children.is_empty() {
        return vec![at.to_owned()];
    }
    node.children
        .iter()
        .flat_map(|child| return_paths(child, &format!("{at}/{}", child.name)))
        .collect()
}

fn vocals() -> Node {
    let fx = vox_fx();
    let returns = return_paths(&fx, &format!("Vocals/{}", fx.name));
    Node::new("Vocals", VOX, Kind::Group)
        .template(&["Vocals"])
        .send("LEAD VOX BUS")
        .children(vec![
            lead("Ron", &LANGUAGES, &returns),
            lead("Belen", &["EN", "ES"], &returns),
            lead("Aline", &["PT"], &returns),
            // The vocal template's returns, folded in beside the leads
            // that feed them — the way a kit piece's Verb sits beside
            // its Sum. Into the folder, so to LEAD VOX BUS with the
            // dry vocal.
            fx,
            // The VCAs: control tracks, no audio, deliberately routed
            // nowhere. One per language, over every source of that
            // language across every performer, part and choir section.
            Node::new("VOX EN", VOX, Kind::Vca).dead_end(),
            Node::new("VOX ES", VOX, Kind::Vca).dead_end(),
            Node::new("VOX PT", VOX, Kind::Vca).dead_end(),
        ])
}

/// The background vocals: a folder of their own beside Vocals in the
/// arrangement, arranged by PART — and a part may carry as many layers
/// as it needs; nothing in the scenes, the comp or the edit assumes a
/// count, which is why one of them is deliberately many
/// (`flow.vocals.bgvs`). Still sung into the vocal bus (BGV BUS), and
/// still `Vocals/BGVs` to the taxonomy.
fn bgvs() -> Node {
    Node::new("BGVs", BGV, Kind::Group)
        .template(&["Vocals", "BGVs"])
        .send("BGV BUS")
        .children(vec![
            layer_in("Octave Up", &LANGUAGES, BGV),
            layer_in("Octave Down", &LANGUAGES, BGV),
            layer_in("Higher Harmony", &LANGUAGES, BGV),
            layer_in("Lower Harmony", &LANGUAGES, BGV),
            // The fifty-layer "Hey!", in `All`: language-free,
            // so it is in every version and the switch leaves
            // it alone. Eight here rather than fifty, which is
            // enough to prove nothing counts them.
            Node::new("Hey", BGV, Kind::Group).children(
                (1..=8)
                    .map(|n| Node::new(&format!("All {n}"), BGV, Kind::Source))
                    .collect(),
            ),
        ])
}

/// A four-section choir per language, on the language VCAs like every
/// other voice: its own folder beside Vocals in the arrangement, into
/// BGV BUS on the mix side.
fn choir() -> Node {
    Node::new("Choir", CHOIR, Kind::Group)
        .template(&["Choir"])
        .send("BGV BUS")
        .children(
            ["Soprano", "Alto", "Tenor", "Bass"]
                .into_iter()
                .map(|section| layer_in(section, &LANGUAGES, CHOIR))
                .collect(),
        )
}

/// The bus tree.
///
/// The dynamic template's canonical one without GUITAR BUS, with the
/// three electric buses under ELECTRIC BUS — in the bus list rather
/// than in the folder.
///
/// The bus tree — the dynamic template's canonical one without GUITAR
/// BUS, with the three electric buses under ELECTRIC BUS, in the bus
/// list rather than in the folder.
/// The **Guide** folder: the count and the pulse the band tracks to.
///
/// It sits at the top of every session and every scene shows it
/// collapsed until a flow opens it, which is why it is its own `Kind`
/// rather than a folder named "Guide" — a scene says "the guide, out of
/// the way" in one rule that survives someone renaming it.
///
/// Its tracks reach the headphone mixes and **never the mix bus**. A
/// click printed into the record is the mistake this routing exists to
/// make impossible, so each one is a dead end here and gets there by a
/// cue send instead.
fn guide() -> Node {
    Node::new("Guide", GUIDE, Kind::Guide).children(vec![
        // Click, Count and Guide are what the guide engine STAMPS, and
        // it stamps notes — `session::guide` writes MIDI items through
        // `create_midi_item` and `add_notes`. They were audio here,
        // which meant the fixture disagreed with the only code that
        // produces them.
        Node::new("Click", GUIDE, Kind::Source).dead_end().midi(),
        Node::new("Count", GUIDE, Kind::Source).dead_end().midi(),
        Node::new("Guide", GUIDE, Kind::Source).dead_end().midi(),
        // The shaker is a played part, not a stamped one.
        Node::new("Shaker", GUIDE, Kind::Source).dead_end(),
    ])
}

/// The **Keyflow** folder: the song's knowledge as MIDI.
///
/// KEY, CHORD, LINES and HITS are read by the guide, by the expression
/// editor's key and chord tools, and by the click's count. They are
/// knowledge, not audio — so they are dead ends too, and every scene
/// keeps them collapsed until Write or Produce opens them.
///
/// KEY and CHORD are what a chart knows and the generator writes; LINES
/// and HITS are what a person puts there. The four names are the same
/// four in the scaffold and in the generator, which they were not
/// before — the scaffold built KEY/CHORD/MELODY/SCALE while this said
/// CHORDS/LINES/HITS, so a project scaffolded from a chart failed the
/// checklist that was meant to check it.
fn keyflow() -> Node {
    Node::new("Keyflow", KEYFLOW, Kind::Keyflow).children(vec![
        Node::new("KEY", KEYFLOW, Kind::Source).dead_end().midi(),
        Node::new("CHORD", KEYFLOW, Kind::Source).dead_end().midi(),
        // The song's melody and its accents (`golden_session::song`):
        // the golden session is a song, so its Keyflow folder says
        // everything a chart would.
        Node::new("LINES", KEYFLOW, Kind::Source).dead_end().midi(),
        Node::new("Lyrics", KEYFLOW, Kind::Source).empty(),
        Node::new("HITS", KEYFLOW, Kind::Source).dead_end().midi(),
    ])
}

/// The monitor buses, beside `MIX BUS` rather than under it.
///
/// Beside is the whole point: everything here is heard by somebody in
/// the room and none of it is heard by the record. Putting the cue
/// mixes under the mix bus is how a foldback ends up printed.
fn monitor_buses() -> Node {
    // A bus is a bus: no items, unarmed, EQ then Comp. A cue mix gets
    // compressed like any other, and the checklist's rule does not have
    // an exception for the monitor side.
    Node::new("HEADPHONE MIXES", MONITOR, Kind::Headphones)
        .fx(BUS_CHAIN)
        .dead_end()
        .children(vec![
            bus("HP Engineer", MONITOR).dead_end(),
            bus("HP Producer", MONITOR).dead_end(),
            bus("HP Broadcast", MONITOR).dead_end(),
        ])
}

/// The talkback bus: the room's comms, recorded alongside the music so
/// a take can be reviewed with the conversation intact — and summed
/// nowhere near the mix.
fn talkback() -> Node {
    Node::new("TALKBACK", MONITOR, Kind::Bus)
        .fx(BUS_CHAIN)
        .dead_end()
        .children(vec![
            Node::new("TB Engineer", MONITOR, Kind::Source).dead_end()
        ])
}

fn mix_bus() -> Node {
    // The root of the tree is its own kind, so a scene can say "the mix
    // bus tree, out of the way" in one rule instead of naming every bus
    // under it. `flow.scenes.reaper-model`.
    Node::new("MIX BUS", BUS, Kind::MixBus)
        .fx(BUS_CHAIN)
        .children(vec![
            bus("INST BUS", BUS).children(vec![
                bus("DRUM BUS", DRUMS),
                bus("BASS BUS", BASS),
                bus("ELECTRIC BUS", ELECTRIC)
                    .send("Electric")
                    .keep_parent()
                    .group(GroupRole::VcaFollow(1))
                    .children(vec![
                        bus("GTR RHYTHM", ELECTRIC),
                        bus("GTR LEAD", ELECTRIC),
                        bus("GTR SOLO", ELECTRIC),
                    ]),
                bus("ACOUSTIC BUS", ACOUSTIC)
                    .send("Acoustic")
                    .keep_parent()
                    .group(GroupRole::VcaFollow(2)),
                bus("KEYS BUS", KEYS),
            ]),
            bus("VOX BUS", BUS).children(vec![bus("LEAD VOX BUS", VOX), bus("BGV BUS", VOX)]),
        ])
}

/// The maximal session: every base the dynamic template has to cover.
#[must_use]
pub fn maximal() -> Shape {
    Shape {
        name: "template",
        bpm: 120.0,
        bars: 64,
        sections: vec![
            Section::new("Intro", 0, 4, 0x004A_6FA5),
            Section::new("Verse 1", 4, 12, 0x003F_A9A0),
            Section::new("Chorus 1", 12, 20, 0x00C9_4540),
            Section::new("Verse 2", 20, 28, 0x003F_A9A0),
            Section::new("Chorus 2", 28, 36, 0x00C9_4540),
            Section::new("Bridge", 36, 44, 0x008C_AA3A),
            Section::new("Chorus 3", 44, 56, 0x00C9_4540),
            Section::new("Outro", 56, 64, 0x004A_6FA5),
        ],
        selected: "Kick",
        layout: Layout::Maximal,
        roots: vec![
            // The Guide and Keyflow folders sit at the TOP of the
            // session, above every instrument, because the count and
            // the song's knowledge are what everything else is played
            // against (`flow.scenes.guide-folder`).
            guide(),
            keyflow(),
            drums(),
            percussion(),
            bass(),
            electric(),
            acoustic(),
            keys(),
            synths(),
            vocals(),
            bgvs(),
            choir(),
            orchestra(),
            mix_bus(),
            // Beside MIX BUS, never under it.
            monitor_buses(),
            talkback(),
        ],
    }
}

/// A lead vocal and its returns, laid out the way a vocal template is:
/// one lead and a pyramid of depth behind it.
#[must_use]
pub fn vocal_fx() -> Shape {
    let lead = Node::new("Vox Lead", VOX, Kind::Group)
        .template(&["Vocals", "Lead"])
        .children(vec![
            Node::new("Chase Vox", VOX, Kind::Piece)
                .children(vec![part("Lead Vox", VOX), part("Vox Dbl", VOX)]),
            vox_fx(),
        ]);
    Shape {
        name: "vocal-fx",
        bpm: 92.0,
        bars: 48,
        sections: Vec::new(),
        selected: "Lead Vox",
        layout: Layout::VocalFx,
        roots: vec![lead],
    }
}

/// Where a fixture and the template disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The fixture track's path.
    pub track: String,
    /// What the fixture claims about the template.
    pub claim: String,
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.track, self.claim)
    }
}

/// The template node at a group path, walking the tree top-level first.
#[must_use]
pub fn template_node<'a>(
    template: &'a IdealFullSessionTemplate,
    path: &[&str],
) -> Option<&'a TemplateNode> {
    let (first, rest) = path.split_first()?;
    let mut node = template.root.iter().find(|n| n.name == *first)?;
    for name in rest {
        node = node.children.iter().find(|c| c.name == *name)?;
    }
    Some(node)
}

impl Shape {
    /// Check every template claim the fixture makes against the template
    /// tree: each `template` path must name a real node. Returns every
    /// disagreement rather than the first, so a renamed group shows up
    /// once per track that stood for it.
    #[must_use]
    pub fn resolve(&self, template: &IdealFullSessionTemplate) -> Vec<Drift> {
        let mut drift = Vec::new();
        for root in &self.roots {
            resolve_node(root, template, &mut Vec::new(), &mut drift);
        }
        drift
    }

    /// Every node with the path of folders above it, depth-first.
    #[must_use]
    pub fn walk(&self) -> Vec<(Vec<String>, &Node)> {
        let mut out = Vec::new();
        for root in &self.roots {
            walk_node(root, &mut Vec::new(), &mut out);
        }
        out
    }
}

fn resolve_node(
    node: &Node,
    template: &IdealFullSessionTemplate,
    path: &mut Vec<String>,
    drift: &mut Vec<Drift>,
) {
    path.push(node.name.clone());
    if let Some(claim) = node.template {
        if template_node(template, claim).is_none() {
            drift.push(Drift {
                track: path.join("/"),
                claim: format!(
                    "stands for template group {}, which does not exist",
                    claim.join("/")
                ),
            });
        }
    }
    for child in &node.children {
        resolve_node(child, template, path, drift);
    }
    path.pop();
}

fn walk_node<'a>(node: &'a Node, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Node)>) {
    out.push((path.clone(), node));
    path.push(node.name.clone());
    for child in &node.children {
        walk_node(child, path, out);
    }
    path.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden::golden_template;

    #[test]
    fn both_fixtures_resolve_against_the_template() {
        let template = golden_template();
        assert_eq!(maximal().resolve(&template), Vec::new());
        assert_eq!(vocal_fx().resolve(&template), Vec::new());
    }

    /// Each orchestra section and everything under it wears the
    /// section's colour: strings brown, brass gold, winds light blue,
    /// percussion red-orange.
    #[test]
    fn every_orchestra_section_wears_its_colour_all_the_way_down() {
        let shape = maximal();
        let walked = shape.walk();
        for (section, colour) in [
            ("Strings", STRINGS),
            ("Brass", BRASS),
            ("Winds", WINDS),
            ("Orch Percussion", ORCH_PERC),
        ] {
            let under: Vec<&Node> = walked
                .iter()
                .filter(|(path, node)| {
                    path.first().is_some_and(|p| p == "Orchestra")
                        && (path.get(1).is_some_and(|p| p == section)
                            || (path.len() == 1 && node.name == section))
                })
                .map(|(_, node)| *node)
                .collect();
            assert!(under.len() > 1, "{section} has a subtree");
            assert!(
                under.iter().all(|node| node.colour == colour),
                "{section}: {:?}",
                under
                    .iter()
                    .map(|n| (&n.name, n.colour))
                    .collect::<Vec<_>>()
            );
        }
    }

    /// Every synth is purple: the Synths folder and all under it, and
    /// the synth bass and all under it.
    #[test]
    fn every_synth_is_purple() {
        let shape = maximal();
        let synths: Vec<&Node> = shape
            .walk()
            .into_iter()
            .filter(|(path, node)| {
                let path: Vec<&str> = path.iter().map(String::as_str).collect();
                matches!(path.as_slice(), ["Synths", ..] | ["Bass", "Synth", ..])
                    || (path.is_empty() && node.name == "Synths")
                    || (path == ["Bass"] && node.name == "Synth")
            })
            .map(|(_, node)| node)
            .collect();
        assert_eq!(synths.len(), 1 + 6 + 4 + 3, "the synths and the synth bass");
        assert!(synths.iter().all(|node| node.colour == SYNTHS));
        assert_ne!(SYNTHS, VERB, "not the vocal Verb returns' violet");
    }

    /// The bass guitar's DI and amp sit directly under it: no Sum.
    #[test]
    fn the_bass_guitar_has_no_sum() {
        let shape = maximal();
        let guitar = shape
            .walk()
            .into_iter()
            .find(|(path, node)| path == &["Bass"] && node.name == "Guitar")
            .map(|(_, node)| {
                node.children
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
            });
        assert_eq!(guitar, Some(vec!["DI", "Amp"]));
    }

    #[test]
    fn a_claim_on_a_group_that_does_not_exist_is_drift() {
        let template = golden_template();
        let shape = Shape {
            roots: vec![Node::new("Theremin", 0, Kind::Group).template(&["Theremins"])],
            ..maximal()
        };
        let drift = shape.resolve(&template);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift.first().map(|d| d.track.as_str()), Some("Theremin"));
    }

    #[test]
    fn the_kick_stands_for_the_template_group_whose_sum_it_uses() {
        let template = golden_template();
        let kick = template_node(&template, &["Drums", "Drum Kit", "Kick"]).map(|k| {
            k.children
                .iter()
                .find(|c| c.name == "SUM")
                .map(|s| s.vocabulary.clone())
        });
        assert_eq!(
            kick,
            Some(Some(vec![
                "In".to_string(),
                "Out".to_string(),
                "Trig".to_string()
            ]))
        );
    }
}
