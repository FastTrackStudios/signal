//! From a [`Shape`] to project text, through `dawfile-reaper`'s builders.
//!
//! Two things the writer does not model yet are spliced into the text
//! after serialisation, at the seam where `dawfile-reaper`'s writer lags
//! its parser: the project-level `<EXTSTATE>` block (the mixer strip
//! widths, which REAPER has no field for) and each grouped track's
//! `GROUP_FLAGS` line (parsed by `dawfile-reaper`, and by daw-standalone's
//! `decode_grouping`, but not yet written — the writer is #30's work).

use std::collections::BTreeMap;

use dawfile_reaper::builder::{MarkerBuilder, ReaperProjectBuilder, TrackBuilder};
use dawfile_reaper::types::item::FadeCurveType;
use dawfile_reaper::types::project::RulerLane;
use dawfile_reaper::types::track::{MasterSendSettings, MonitorMode, RecordMode, RecordSettings};
use dawfile_reaper::RppSerialize;

use super::guid::{guid, hash};
use super::kind::{Kind, GROUP_KEY, KIND_KEY};
use super::shape::{
    Fx, GroupRole, Layout, Node, Routing, Shape, FOLDER_WIDTH, MIN_HEIGHT, MIN_WIDTH, TONE_WIDTH,
};

/// One track of the flattened fixture: the node plus what its place in
/// the tree decides about it.
#[derive(Debug, Clone)]
pub struct Flat {
    /// The node path, `/`-joined, e.g. `Drums/Kick/Sum/In`.
    pub path: String,
    /// Which RECORDING this track's items belong to.
    ///
    /// The mics of one drum are one performance: the same punches, the
    /// same comp, the same edits. So they share an item layout, and the
    /// key they share is the piece they are mics of — `In`, `Out` and
    /// `Trig` all say `Drums/Kick/Sum`.
    ///
    /// Parallel colour does not: a Sub, a Verb bank and a Fundamental
    /// are fed from the take rather than being it, and giving them the
    /// take's own boundaries would say something about them that is not
    /// true.
    pub take: String,
    /// Track name.
    pub name: String,
    /// RGB colour.
    pub colour: u32,
    /// Taxonomy kind.
    pub kind: Kind,
    /// The template group this track stands for.
    pub template: Option<&'static [&'static str]>,
    /// Nesting level, 0 at the top.
    pub depth: u32,
    /// A folder.
    pub is_folder: bool,
    /// The track a kit piece is mixed on — see [`is_piece`].
    pub piece: bool,
    /// The track's GUID.
    pub guid: String,
    /// Where the audio goes.
    pub routing: Routing,
    /// The paths of the returns this track aux-sends to.
    pub aux: Vec<String>,
    /// No items at all — a lane that is real and waiting.
    pub no_items: bool,
    /// In the mixer only ([`super::shape::Node::mixer_only`]).
    pub mixer_only: bool,
    /// The items hold notes rather than audio.
    pub midi: bool,
    /// Track grouping.
    pub group: Option<GroupRole>,
    /// A stereo pair.
    pub stereo: bool,
    /// The FX chain by name.
    pub fx: Vec<Fx>,
    /// The strip width the node asks for itself — see [`Node::strip`].
    pub strip: Option<u32>,
}

impl Flat {
    /// A bus: no items, unarmed.
    #[must_use]
    pub fn is_bus(&self) -> bool {
        self.kind == Kind::Bus
    }

    /// A track you only need present — see [`is_auxiliary`].
    #[must_use]
    pub fn auxiliary(&self) -> bool {
        is_auxiliary(&self.name)
    }

    /// Whether the track carries audio items: a leaf that is not a bus.
    #[must_use]
    pub fn has_items(&self) -> bool {
        !self.is_folder && !self.is_bus() && !self.no_items
    }

    /// Whether the track's items are MIDI — a trigger (here the kit's
    /// notes; a session's may as well be a recorded piezo) and the samples
    /// it drives, a sampler's notes.
    #[must_use]
    pub fn is_midi(&self) -> bool {
        self.has_items() && (matches!(self.kind, Kind::Trigger | Kind::Sample) || self.midi)
    }

    /// The media file this track's items play, relative to the project.
    #[must_use]
    pub fn media_file(&self) -> Option<String> {
        (self.has_items() && !self.is_midi()).then(|| {
            // A performed track shares its performance's file.
            let name = super::perform::performance(&self.path)
                .map_or_else(|| slug(&self.path), |(key, _)| key);
            format!("media/{name}.wav")
        })
    }

    /// Whether the track's items read a performance the song's length,
    /// each from where it starts (`golden_session::perform`).
    #[must_use]
    pub fn performed(&self) -> bool {
        self.has_items() && !self.is_midi() && super::perform::performance(&self.path).is_some()
    }
}

/// A file-safe name for a track path: `Drums/Kick/Sum/In` →
/// `drum-kit-kick-sum-in`.
///
/// The signs the pitch returns are named by are spelled out, so `Oct+`
/// and `Oct-` do not share a file.
#[must_use]
pub fn slug(path: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in path.replace('+', " plus ").replace('-', " minus ").chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Is this a track you look at, or one you only need present?
///
/// A trigger and its samples, a sub, a fundamental and a reverb return
/// are all things
/// you want IN the session and almost never want to READ. Collapsed to
/// the minimum they stay reachable and stop spending the space the mics
/// and the sums actually need. By name rather than by kind because this
/// is the *layout* the renders were taken against: an Inst FX return
/// named `Short Room` is a reverb by kind but opens as a piece.
#[must_use]
pub fn is_auxiliary(name: &str) -> bool {
    matches!(name, "Sub" | "Verb" | "Fund") || name.ends_with("Trig") || name.ends_with("Sample")
}

/// Is this the track the KIT PIECE is mixed on?
///
/// A piece is one sound source, where the tone processing goes. It is a
/// **Sum** (several mics of one drum, summed), a piece folder of
/// captures only (the bass guitar is its DI and its amp, with no Sum
/// between) or a folder with exactly one real child (`Tom 1` is its mic
/// and its trigger). Anything else
/// with children is a group, and a group is a bus. A leaf is a piece
/// unless its parent already is one, which is what makes `In` and `Out`
/// mics of the kick rather than two kicks.
#[must_use]
pub fn is_piece(node: &Node, parent_is_piece: bool) -> bool {
    if is_auxiliary(&node.name) {
        return false;
    }
    if node.children.is_empty() {
        return !parent_is_piece;
    }
    if node.name == "Sum" {
        return true;
    }
    // Captures only — mics and DIs, no parts: a vocal's Main and Dbl
    // are two performances, not one heard twice.
    if node.kind == Kind::Piece
        && node
            .children
            .iter()
            .all(|c| c.children.is_empty() && c.kind == Kind::Source)
    {
        return true;
    }
    let not_aux = node
        .children
        .iter()
        .filter(|c| !is_auxiliary(&c.name))
        .count();
    let real = node
        .children
        .iter()
        .filter(|c| !is_auxiliary(&c.name) && c.children.is_empty())
        .count();
    real == 1 && real == not_aux
}

/// Flatten a shape depth-first, deciding each track's role from its
/// place in the tree.
#[must_use]
pub fn flatten(shape: &Shape) -> Vec<Flat> {
    let mut out = Vec::new();
    for root in &shape.roots {
        flatten_node(root, &mut Vec::new(), false, None, &mut out);
    }
    // What is in a mixer-only folder is mixer-only too: the panel shows
    // none of Process, not its folder alone.
    let folders: Vec<String> = out
        .iter()
        .filter(|t| t.mixer_only && t.is_folder)
        .map(|t| format!("{}/", t.path))
        .collect();
    for track in &mut out {
        if folders.iter().any(|f| track.path.starts_with(f)) {
            track.mixer_only = true;
        }
    }
    out
}

/// Whether a track's items are its own rather than the take's.
///
/// Parallel colour: fed from the performance rather than being it. A
/// Verb bank's returns come and go with the reverb, not with the punch.
fn has_its_own_comp(name: &str) -> bool {
    matches!(name, "Sub" | "Verb" | "Fund")
}

/// `enclosing` is the nearest enclosing piece's path — the recording
/// everything inside it is a microphone on.
fn flatten_node(
    node: &Node,
    path: &mut Vec<String>,
    parent_is_piece: bool,
    enclosing: Option<&str>,
    out: &mut Vec<Flat>,
) {
    path.push(node.name.clone());
    let joined = path.join("/");
    let piece = is_piece(node, parent_is_piece);
    let take = match enclosing {
        Some(piece_path) if parent_is_piece && !has_its_own_comp(&node.name) => {
            piece_path.to_owned()
        }
        _ => joined.clone(),
    };
    let inside = if piece {
        joined.clone()
    } else {
        enclosing.unwrap_or(&joined).to_owned()
    };
    out.push(Flat {
        guid: guid(&format!("track:{joined}")),
        path: joined,
        take,
        name: node.name.clone(),
        colour: node.colour,
        kind: node.kind,
        template: node.template,
        depth: u32::try_from(path.len().saturating_sub(1)).unwrap_or(u32::MAX),
        is_folder: node.is_folder(),
        piece,
        routing: node.routing,
        aux: node.aux.clone(),
        no_items: node.no_items,
        mixer_only: node.mixer_only,
        midi: node.midi,
        group: node.group,
        stereo: node.stereo,
        fx: node.fx.clone(),
        strip: node.strip,
    });
    for child in &node.children {
        flatten_node(child, path, piece, Some(&inside), out);
    }
    path.pop();
}

/// How wide a track's mixer strip opens under a layout: the node's own
/// width when it states one ([`Node::strip`]), else the layout's rule.
#[must_use]
pub fn strip_width(layout: Layout, track: &Flat) -> u32 {
    if let Some(width) = track.strip {
        return width;
    }
    match layout {
        Layout::Maximal => {
            if track.auxiliary() {
                MIN_WIDTH
            } else if track.piece {
                TONE_WIDTH
            } else if track.is_folder {
                FOLDER_WIDTH
            } else {
                MIN_WIDTH
            }
        }
        Layout::VocalFx => {
            if track.is_folder {
                FOLDER_WIDTH
            } else if track.name == "Lead Vox" {
                TONE_WIDTH
            } else {
                MIN_WIDTH
            }
        }
    }
}

/// REAPER's colour word, `0x01BBGGRR`, from `0xRRGGBB`.
#[must_use]
pub const fn native_colour(rgb: u32) -> u32 {
    let r = rgb >> 16 & 0xFF;
    let g = rgb >> 8 & 0xFF;
    let b = rgb & 0xFF;
    0x0100_0000 | b << 16 | g << 8 | r
}

/// REAPER's `GROUP_FLAGS` fields, in the order REAPER writes them.
///
/// Seven lead/follow pairs first — volume, pan, mute, solo, rec-arm,
/// polarity, automation mode — then the reverse and no-lead flags, width
/// only at 19/20, VCA at 21/22. Numbered from one, the way REAPER's own
/// saved lines read and the way `07_template_group_flags` names them
/// when it reads this back (session #41); that test keeps its own copy
/// of the numbers deliberately, as an oracle taken from a project REAPER
/// itself saved rather than from this table.
const GROUP_FIELDS: usize = 25;
const MUTE_LEAD: usize = 5;
const MUTE_FOLLOW: usize = 6;
const SOLO_LEAD: usize = 7;
const SOLO_FOLLOW: usize = 8;
const VCA_LEAD: usize = 21;
const VCA_FOLLOW: usize = 22;

/// The `GROUP_FLAGS` line for a grouping role: the folder is the VCA,
/// mute and solo lead of its bus, the bus the follower.
#[must_use]
pub fn group_flags(role: GroupRole) -> String {
    let mut fields = [0_u32; GROUP_FIELDS];
    let (bit, slots): (u32, [usize; 3]) = match role {
        GroupRole::VcaLead(n) => (n, [VCA_LEAD, MUTE_LEAD, SOLO_LEAD]),
        GroupRole::VcaFollow(n) => (n, [VCA_FOLLOW, MUTE_FOLLOW, SOLO_FOLLOW]),
    };
    let mask = 1_u32 << bit.saturating_sub(1).min(31);
    for slot in slots {
        if let Some(field) = slot.checked_sub(1).and_then(|i| fields.get_mut(i)) {
            *field |= mask;
        }
    }
    fields
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A tiny deterministic generator for item placement, seeded from a
/// path hash: xorshift64, which is all "different on every track, the
/// same on every run" needs.
struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    const fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next().checked_rem(n.max(1)).unwrap_or(0)
    }

    fn pick<T: Copy>(&mut self, options: &[T]) -> Option<T> {
        let i = usize::try_from(self.below(u64::try_from(options.len()).unwrap_or(1))).ok()?;
        options.get(i).copied()
    }
}

/// One item of a track's deterministic pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Start bar.
    pub bar: u32,
    /// Length in bars.
    pub length: u32,
    /// `(shape, seconds)` fade in, if any.
    pub fade_in: Option<(i32, f64)>,
    /// `(shape, seconds)` fade out, if any.
    pub fade_out: Option<(i32, f64)>,
}

/// Where a track's items land: a comped session's worth of takes across
/// the song, the way the reference has them, derived from the path so a
/// re-run lands them in the same places.
#[must_use]
pub fn items_for(layout: Layout, path: &str, bars: u32) -> Vec<Placed> {
    // Seeded by the recording pass, not the track: every mic of one pass
    // was cut where the others were (a kit's, a guitar's), so a folded row
    // over them lines up as a real session's does.
    let pass = super::perform::pass(path);
    let mut rng = Rng::new(hash(&format!("items:{pass}")));
    let mut out = Vec::new();
    let (first, lengths, gaps): (u64, &[u32], &[u32]) = match layout {
        Layout::Maximal => (8, &[4, 8, 8, 16], &[0, 0, 4]),
        Layout::VocalFx => (4, &[4, 8, 8], &[0, 4]),
    };
    let mut bar = u32::try_from(rng.below(first)).unwrap_or(0);
    while bar < bars {
        let length = rng.pick(lengths).unwrap_or(4);
        let fades = match layout {
            Layout::Maximal => {
                let shape_in = rng.pick(&[0, 0, 1, 2, 5]).unwrap_or(0);
                let time_in = rng.pick(&[0.02, 0.05, 0.25, 0.5]).unwrap_or(0.02);
                let shape_out = rng.pick(&[0, 0, 1, 2, 5]).unwrap_or(0);
                let time_out = rng.pick(&[0.1, 0.5, 1.0, 2.0]).unwrap_or(0.1);
                (Some((shape_in, time_in)), Some((shape_out, time_out)))
            }
            Layout::VocalFx => (None, None),
        };
        out.push(Placed {
            bar,
            length,
            fade_in: fades.0,
            fade_out: fades.1,
        });
        let gap = rng.pick(gaps).unwrap_or(0);
        bar = bar.saturating_add(length).saturating_add(gap);
    }
    out
}

/// The fixture's fader and pan, spread across the whole travel.
///
/// About -20 dB to +1.6 dB for the maximal session, so caps sit below
/// the levels passing them and the meter column has something to show —
/// every strip parked within 5 dB of unity hid that entirely. The vocal
/// template does keep its strips near unity, panned centre.
#[must_use]
pub fn volpan(layout: Layout, index: u32) -> (f64, f64) {
    let step = |modulus: u32| f64::from(index.checked_rem(modulus).unwrap_or(0));
    match layout {
        Layout::Maximal => (
            11.0_f64.mul_add(step(11), 10.0) / 100.0,
            40.0_f64.mul_add(step(5), -80.0) / 100.0,
        ),
        Layout::VocalFx => (75.0_f64.mul_add(step(7), 550.0) / 1000.0, 0.0),
    }
}

/// REAPER's record input: a mono hardware input counted from zero, or
/// `1024 +` the first channel of a stereo pair.
#[must_use]
pub fn record_input(layout: Layout, track: &Flat, index: u32) -> i32 {
    match layout {
        Layout::Maximal => {
            let source = if track.is_folder || track.auxiliary() {
                0
            } else {
                index % 16
            };
            let input = if track.stereo {
                1024_u32.saturating_add(source & !1)
            } else {
                source
            };
            i32::try_from(input).unwrap_or(0)
        }
        Layout::VocalFx => 0,
    }
}

/// Whether a track opens armed: the close mics, the way a kit is before
/// a take; never a folder, an auxiliary or a bus. The vocal template arms
/// only its lead.
#[must_use]
pub fn armed(layout: Layout, track: &Flat) -> bool {
    match layout {
        Layout::Maximal => !(track.is_folder || track.auxiliary() || track.is_bus()),
        Layout::VocalFx => track.name == "Lead Vox",
    }
}

/// The built fixture: the text and the tracks it was built from.
#[derive(Debug, Clone)]
pub struct Built {
    /// The project text.
    pub rpp: String,
    /// The tracks, in project order.
    pub tracks: Vec<Flat>,
}

/// What a track's place in the project decides about it, gathered so the
/// track builder takes one argument rather than seven.
struct TrackContext<'a> {
    /// The track's index in the project.
    index: u32,
    /// How many folder levels close on this track.
    closes: u32,
    /// Whether the session opens with this track selected.
    selected: bool,
    /// The indices of the tracks that send here, bus sends first and
    /// aux sends after.
    receives: &'a [i32],
    /// Seconds per bar at the fixture's tempo.
    secs_per_bar: f64,
    /// The song's sections: what the song's parts are written over.
    sections: &'a [super::shape::Section],
    /// The song's length in bars.
    bars: u32,
    /// The layout the fixture is rendered under.
    layout: Layout,
}

/// The ruler's lanes and the song's regions, the way an FTS session
/// keeps them: lane 1 the SONG, lane 2 the SECTIONS, lane 3 the MARKS.
fn with_markers(
    mut project: ReaperProjectBuilder,
    shape: &Shape,
    secs_per_bar: f64,
) -> ReaperProjectBuilder {
    if shape.sections.is_empty() {
        return project;
    }
    let mut id = 1_i32;
    let mut next = |id: &mut i32| {
        let this = *id;
        *id = id.saturating_add(1);
        this
    };
    project = project.add_marker(
        MarkerBuilder::region(
            next(&mut id),
            0.0,
            f64::from(shape.bars) * secs_per_bar,
            "SONG",
        )
        .locked()
        .lane(1)
        .guid(guid("region:SONG"))
        .build(),
    );
    for section in &shape.sections {
        project = project.add_marker(
            MarkerBuilder::region(
                next(&mut id),
                f64::from(section.start) * secs_per_bar,
                f64::from(section.end) * secs_per_bar,
                section.name,
            )
            .locked()
            .lane(2)
            .color(i32::try_from(native_colour(section.colour)).unwrap_or(0))
            .guid(guid(&format!("region:{}", section.name)))
            .build(),
        );
    }
    for (name, bar) in [("SONGSTART", 0), ("SOLO", 40), ("SONGEND", shape.bars)] {
        project = project.add_marker(
            MarkerBuilder::marker(next(&mut id), f64::from(bar) * secs_per_bar, name)
                .locked()
                .lane(3)
                .guid(guid(&format!("marker:{name}")))
                .build(),
        );
    }
    project
}

/// One track's items: the takes a comped session has across the song,
/// audio on every leaf and MIDI on the triggers.
fn with_items(mut builder: TrackBuilder, track: &Flat, context: &TrackContext) -> TrackBuilder {
    let name = track.name.clone();
    let media = track.media_file();
    let midi = track.is_midi();
    let performed = track.performed();
    // A part of the song (the Keyflow folder, the keys, the synths):
    // its MIDI is the song's, and the Keyflow tracks are laid out by it —
    // one key, an item a chord, an item a section.
    let part = midi
        .then(|| super::song::Part::of(&track.name))
        .flatten()
        .filter(|_| !context.sections.is_empty());
    if let Some(part) = part {
        let laid_out = part
            .items(context.sections, context.bars)
            .unwrap_or_else(|| {
                items_for(context.layout, &track.take, context.bars)
                    .into_iter()
                    .map(|p| {
                        let n = p.bar.saturating_div(4).saturating_add(1);
                        (
                            p.bar,
                            p.length.min(context.bars.saturating_sub(p.bar)),
                            format!("{name} {n}"),
                        )
                    })
                    .filter(|(_, len, _)| *len > 0)
                    .collect()
            });
        for (n, (bar, bars, item_name)) in laid_out.into_iter().enumerate() {
            let item_path = format!("item:{}/{n}", track.path);
            builder = builder.item(
                f64::from(bar) * context.secs_per_bar,
                f64::from(bars) * context.secs_per_bar,
                |item| {
                    item.name(item_name)
                        .guid(guid(&item_path))
                        .midi(|m| part.write(m, context.sections, bar, bars))
                },
            );
        }
        return builder;
    }
    // Seeded by the TAKE, not by the track: the mics of one drum were
    // recorded together and comped together, so their items land in the
    // same places. Seeding per track gave a snare top and a snare bottom
    // different punches, which is not a session anyone has ever had.
    for (n, placed) in items_for(context.layout, &track.take, context.bars)
        .into_iter()
        .enumerate()
    {
        let item_name = format!("{name} {}", placed.bar.saturating_div(4).saturating_add(1));
        let item_path = format!("item:{}/{n}", track.path);
        let length_beats = placed.length.saturating_mul(4);
        let media = media.clone();
        builder = builder.item(
            f64::from(placed.bar) * context.secs_per_bar,
            f64::from(placed.length) * context.secs_per_bar,
            |mut item| {
                item = item.name(item_name).guid(guid(&item_path));
                if let Some((shape, time)) = placed.fade_in {
                    item = item.fade_in(time, FadeCurveType::from(shape));
                }
                if let Some((shape, time)) = placed.fade_out {
                    item = item.fade_out(time, FadeCurveType::from(shape));
                }
                if midi {
                    // Each of these reads as the thing it is. A fixture
                    // whose MIDI tracks all held the same spike would
                    // pass every count and show nothing — and the point
                    // of a golden session is to be LOOKED at.
                    let pattern = MidiPattern::of(&track.name, track.kind);
                    item = item.midi(|m| pattern.write(m, length_beats));
                } else if let Some(file) = media {
                    item = item.source_wave(file);
                    // A performance is the whole song: the item plays its
                    // own stretch of it.
                    if performed {
                        item = item.slip_offset(f64::from(placed.bar) * context.secs_per_bar);
                    }
                }
                item
            },
        );
    }
    builder
}

/// What a MIDI track's items hold.
///
/// The golden session is a fixture to look at as much as to count, so
/// its MIDI has to read as what it is: a click is not a chord and a
/// chord is not a trigger. None of this is musical content — it is the
/// SHAPE the real generators produce, at the density they produce it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MidiPattern {
    /// One spike a beat: a trigger's hit list.
    Trigger,
    /// One a beat, the downbeat higher — a click.
    Click,
    /// Four at the top of the bar and nothing after: a count-in.
    Count,
    /// Sparse, one a bar: a spoken cue.
    Cue,
    /// One long note a bar, low: the key, held.
    Key,
    /// A triad a bar, held: the chords.
    Chord,
}

impl MidiPattern {
    fn of(name: &str, kind: Kind) -> Self {
        match name {
            "Click" => Self::Click,
            "Count" => Self::Count,
            "Guide" => Self::Cue,
            "KEY" => Self::Key,
            "CHORD" => Self::Chord,
            _ if matches!(kind, Kind::Trigger | Kind::Sample) => Self::Trigger,
            _ => Self::Trigger,
        }
    }

    /// Write `beats` worth of this pattern.
    fn write(
        self,
        m: dawfile_reaper::builder::MidiSourceBuilder,
        beats: u32,
    ) -> dawfile_reaper::builder::MidiSourceBuilder {
        const QN: u32 = 960;
        match self {
            Self::Trigger => {
                let mut m = m.ticks_per_qn(QN);
                for _ in 0..beats {
                    m = m.note(0, 0, 36, 100, 240).advance(QN);
                }
                m
            }
            Self::Click => {
                let mut m = m.ticks_per_qn(QN);
                for beat in 0..beats {
                    // The downbeat is a different note, which is how a
                    // click says where the bar is.
                    let (pitch, velocity) = if beat % 4 == 0 { (76, 110) } else { (77, 80) };
                    m = m.note(0, 0, pitch, velocity, 120).advance(QN);
                }
                m
            }
            Self::Count => {
                let mut m = m.ticks_per_qn(QN);
                // Four in, then silence: a count-in counts once.
                for beat in 0..beats.min(4) {
                    m = m.note(0, 0, 60 + beat as u8, 100, 240).advance(QN);
                }
                m
            }
            Self::Cue => {
                // ONE cue, where the item starts. A guide speaks at a
                // section — "chorus in two" — and a voice that said
                // something every bar would be a voice you turn off.
                // Each item here is a section's worth, so its start IS
                // the transition.
                m.ticks_per_qn(QN).note(0, 0, 72, 90, QN)
            }
            Self::Key => {
                // ONE note, held for the whole item. A key is a state,
                // not an event: it is written where it CHANGES and it
                // holds until it changes again. Restating it every bar
                // would say something happened when nothing did.
                m.ticks_per_qn(QN)
                    .note(0, 0, 36, 70, (QN * beats.max(1)).max(QN))
            }
            Self::Chord => {
                let mut m = m.ticks_per_qn(QN);
                let mut beat = 0;
                while beat < beats {
                    // A triad sounded together and held UNTIL THE NEXT
                    // CHORD — which is what a chord on a chart means.
                    // A chord that stopped early would leave a hole
                    // where the harmony is still sounding, and the
                    // analyser reading this track back would see one.
                    let bar = QN * 4;
                    // Held exactly to where the next one begins, with
                    // no tick of daylight. That used to be unsafe: the
                    // builder sorted note-ons first at a shared tick,
                    // so a reader pairing by pitch handed the SECOND
                    // chord the first one's note-off and called it zero
                    // long. Fixed in the builder (daw#28) — offs sort
                    // before ons — so the generator can write what it
                    // means again.
                    let step = bar.min(QN * (beats - beat));
                    for pitch in [60u8, 64, 67] {
                        m = m.note(0, 0, pitch, 85, step);
                    }
                    m = m.advance(step);
                    beat += 4;
                }
                m
            }
        }
    }
}

/// One track of the project: the node's own settings, its routing, its
/// kind in ext-state and its items.
fn build_track(track: &Flat, context: &TrackContext) -> dawfile_reaper::types::Track {
    let (volume, pan) = volpan(context.layout, context.index);
    let mut builder = TrackBuilder::new(&track.name)
        .guid(&track.guid)
        .color(native_colour(track.colour))
        .volume(volume)
        .pan(pan);
    // REAPER stores the depth DELTA: a folder always opens one; a leaf
    // closes however many end on it.
    if track.is_folder {
        builder = builder.folder_start();
    } else if context.closes > 0 {
        builder = builder.folder_end(i32::try_from(context.closes).unwrap_or(0));
    }
    if context.selected {
        builder = builder.selected();
    }
    if track.mixer_only {
        builder = builder.mixer_only();
    }
    if context.layout == Layout::Maximal && track.auxiliary() {
        builder = builder.height(MIN_HEIGHT);
    }
    for src in context.receives {
        builder = builder.receive(*src);
    }
    for fx in &track.fx {
        builder = builder.vst(fx.name, fx.file);
    }
    if track.has_items() {
        builder = with_items(builder, track, context);
    }
    let mut built = builder.build();
    built.master_send = Some(MasterSendSettings {
        enabled: track.routing.parent_send(),
        unknown_field_2: 0,
    });
    built.record = Some(RecordSettings {
        armed: armed(context.layout, track),
        input: record_input(context.layout, track, context.index),
        monitor: MonitorMode::On,
        record_mode: RecordMode::Input,
        monitor_track_media: false,
        preserve_pdc_delayed: false,
        record_path: 0,
    });
    built
        .extension_data
        .push((KIND_KEY.to_string(), track.kind.to_string()));
    if let Some(group) = track.template {
        built
            .extension_data
            .push((GROUP_KEY.to_string(), group.join("/")));
    }
    for (n, item) in built.items.iter_mut().enumerate() {
        item.item_guid = Some(guid(&format!("iguid:{}/{n}", track.path)));
    }
    built
}

/// Build a shape into project text.
#[must_use]
pub fn build(shape: &Shape) -> Built {
    let tracks = flatten(shape);
    let secs_per_bar = 4.0 * 60.0 / shape.bpm;
    let layout = shape.layout;

    // Who receives from whom: a send is written on the RECEIVING track,
    // as the index of the track it comes from. A bus send names its bus;
    // an aux send names its return's path, because return names repeat.
    let mut receives: BTreeMap<&str, Vec<i32>> = BTreeMap::new();
    let mut aux_receives: BTreeMap<&str, Vec<i32>> = BTreeMap::new();
    for (i, track) in tracks.iter().enumerate() {
        let from = i32::try_from(i).unwrap_or(i32::MAX);
        if let Some(to) = track.routing.send() {
            receives.entry(to).or_default().push(from);
        }
        for to in &track.aux {
            aux_receives.entry(to.as_str()).or_default().push(from);
        }
    }
    let selected = tracks.iter().position(|t| t.name == shape.selected);

    let mut project = ReaperProjectBuilder::new()
        .version_string("7.0")
        .tempo(shape.bpm);

    project = with_markers(project, shape, secs_per_bar);

    for (i, track) in tracks.iter().enumerate() {
        let next_depth = tracks.get(i.saturating_add(1)).map_or(0, |t| t.depth);
        let sources: Vec<i32> = [
            receives.get(track.name.as_str()),
            aux_receives.get(track.path.as_str()),
        ]
        .into_iter()
        .flatten()
        .flatten()
        .copied()
        .collect();
        let context = TrackContext {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            closes: track.depth.saturating_sub(next_depth),
            selected: selected == Some(i),
            receives: &sources,
            secs_per_bar,
            sections: &shape.sections,
            bars: shape.bars,
            layout,
        };
        project = project.add_track(build_track(track, &context));
    }

    let mut project = project.build();
    if !shape.sections.is_empty() {
        project.ruler_lanes = [(1, "SONG"), (2, "SECTIONS"), (3, "MARKS")]
            .into_iter()
            .map(|(index, name)| RulerLane {
                index,
                flags: 8,
                name: name.to_string(),
                color: 0,
                extra: -1,
            })
            .collect();
    }
    let text = project.to_rpp_string();
    let rpp = splice(&text, layout, &tracks);
    Built { rpp, tracks }
}

/// Add what the writer does not model: the strip widths block and the
/// `GROUP_FLAGS` lines.
fn splice(text: &str, layout: Layout, tracks: &[Flat]) -> String {
    let widths: Vec<String> = tracks
        .iter()
        .map(|t| format!("{}={}", t.guid, strip_width(layout, t)))
        .collect();
    let flags: BTreeMap<&str, String> = tracks
        .iter()
        .filter_map(|t| t.group.map(|g| (t.guid.as_str(), group_flags(g))))
        .collect();

    let mut out = String::with_capacity(text.len().saturating_add(4096));
    let mut widths_written = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !widths_written && trimmed.starts_with("<TRACK") {
            out.push_str("  <EXTSTATE\n    <FTSMCP\n      WIDTHS ");
            out.push_str(&widths.join(" "));
            out.push_str("\n    >\n  >\n");
            widths_written = true;
        }
        out.push_str(line);
        out.push('\n');
        if let Some(id) = trimmed.strip_prefix("TRACKID ") {
            if let Some(flags) = flags.get(id.trim()) {
                out.push_str("    GROUP_FLAGS ");
                out.push_str(flags);
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden_session::shape::{maximal, vocal_fx};

    #[test]
    fn group_flags_land_on_reapers_fields() {
        // Fields 5, 7 and 21 for a lead of group 1; 6, 8 and 22 for a
        // follower of group 2, with bit 1 set.
        assert_eq!(
            group_flags(GroupRole::VcaLead(1)),
            "0 0 0 0 1 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0"
        );
        assert_eq!(
            group_flags(GroupRole::VcaFollow(2)),
            "0 0 0 0 0 2 0 2 0 0 0 0 0 0 0 0 0 0 0 0 0 2 0 0 0"
        );
    }

    #[test]
    fn the_python_layout_rules_survive_the_port() {
        // The Sum is the piece; its mics are not; the toms are pieces
        // with no Sum; the FX folder with one real child counts as one.
        let tracks = flatten(&maximal());
        let by_path = |p: &str| tracks.iter().find(|t| t.path == p);
        assert!(by_path("Drums/Kick/Sum").is_some_and(|t| t.piece));
        assert!(by_path("Drums/Kick/Sum/In").is_some_and(|t| !t.piece));
        assert!(by_path("Drums/Toms/Tom 1").is_some_and(|t| t.piece));
        assert!(by_path("Drums/Toms/Tom 1/T1").is_some_and(|t| !t.piece));
        assert!(by_path("Drums/Process/FX").is_some_and(|t| t.piece));
        assert!(by_path("Drums/Process/FX/Room Sim").is_some_and(|t| !t.piece));
        assert!(by_path("Drums/Snare/Verb").is_some_and(|t| t.auxiliary() && t.is_folder));
        // The maximal session, all of it: the instruments, the monitor
        // side (Guide, Keyflow, the headphone buses, talkback), the
        // three vocal languages across every lead, BGV part and choir
        // section with their VCAs, the synth families, percussion and
        // the orchestra's four sections.
        //
        // A number rather than a shape, on purpose: this test is the
        // tripwire for a shape appearing or vanishing by accident, and
        // the per-rule `flow.*.golden` tests are what say the shapes
        // are RIGHT.
        // 277 since the Keyflow folder became KEY, CHORD, LINES and
        // HITS — four where it had three. The names disagreed across
        // three places before that: the scaffold built KEY/CHORD/
        // MELODY/SCALE while this built CHORDS/LINES/HITS, so a project
        // scaffolded from a chart could not pass the checklist meant to
        // check it — and the Guide folder gained Count, which the guide
        // engine has always stamped and this had no track for.
        // 278 with Lyrics beside LINES: the song's words, one item a
        // line, where LINES is the chart's melodies.
        // 294 with the vocal template's returns folded in: Vox FX, its
        // Delay (4), Verb (5) and Pitch (2) folders and Wide — sixteen.
        // 296 with every drum Verb a bank of its rooms — the kick's
        // Room under its Verb (+1), the toms' Room and Nonlin (+2) —
        // and the bass guitar's Sum gone (−1).
        // 272 with the Inst FX folder gone — the folder, its six banks
        // and their seventeen returns (−24).
        // 278 with a Sample beside each drum's Trig — the kick's, the
        // snare's and the four toms' (+6): Trig is what says the drum was
        // hit, Sample what plays from it.
        assert_eq!(tracks.len(), 278);
    }

    fn clear_aux(nodes: &mut [Node]) {
        for node in nodes {
            node.aux.clear();
            clear_aux(&mut node.children);
        }
    }

    #[test]
    fn every_aux_send_reaches_a_return_and_is_written_on_it() {
        for shape in [maximal(), vocal_fx()] {
            let built = build(&shape);
            for track in &built.tracks {
                for to in &track.aux {
                    assert!(
                        built.tracks.iter().any(|t| t.path == *to && !t.is_folder),
                        "{} sends to {to}, which is not a track",
                        track.path
                    );
                }
            }
        }
        // Three leads into twelve returns: one AUXRECV per lead on each,
        // over what the same session writes with no aux sends at all.
        let auxrecvs = |shape: &Shape| {
            build(shape)
                .rpp
                .lines()
                .filter(|l| l.trim_start().starts_with("AUXRECV "))
                .count()
        };
        let mut dry = maximal();
        clear_aux(&mut dry.roots);
        assert_eq!(auxrecvs(&maximal()), auxrecvs(&dry) + 3 * 12);
    }

    #[test]
    fn a_rerun_is_byte_identical_and_the_two_fixtures_differ() {
        let a = build(&maximal()).rpp;
        let b = build(&maximal()).rpp;
        assert_eq!(a, b);
        assert_ne!(a, build(&vocal_fx()).rpp);
    }

    #[test]
    fn items_are_deterministic_per_pass_and_cover_the_song() {
        let a = items_for(Layout::Maximal, "Drums/Kick/Sum/In", 64);
        assert_eq!(a, items_for(Layout::Maximal, "Drums/Kick/Sum/In", 64));
        // One pass, one layout: every mic of the kit was cut where the
        // others were, so its folded row lines up.
        assert_eq!(a, items_for(Layout::Maximal, "Drums/Snare/Sum/Top", 64));
        // Another pass is cut its own way.
        assert_ne!(a, items_for(Layout::Maximal, "Bass/DI", 64));
        assert!(a.iter().all(|p| p.bar < 64));
        assert!(a
            .last()
            .is_some_and(|p| p.bar.saturating_add(p.length) >= 60));
    }

    #[test]
    fn slugs_are_file_safe() {
        assert_eq!(slug("Drums/Kick/Sum/In"), "drums-kick-sum-in");
        assert_eq!(
            slug("Vox Lead/Vox FX/Pitch/Oct+"),
            "vox-lead-vox-fx-pitch-oct-plus"
        );
        assert_ne!(
            slug("Vox Lead/Vox FX/Pitch/Oct+"),
            slug("Vox Lead/Vox FX/Pitch/Oct-")
        );
        assert_eq!(slug("Drums/Cymbals/Hi-Hat"), "drums-cymbals-hi-minus-hat");
    }
}
