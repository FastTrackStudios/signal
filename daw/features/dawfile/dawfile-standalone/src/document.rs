//! The document model — what a `.session` file *is*.
//!
//! The payload types are `daw_proto`'s, deliberately: the standalone format
//! is the on-disk form of what the `daw` facade already speaks, not a third
//! invented model and not REAPER's shape (#155 decision 2). What this module
//! adds on top of `daw_proto` is **containment and identity** — the tree of
//! tracks → items → takes, each node carrying an [`EntityId`], which is what
//! `daw_proto`'s flat service-reply types have no reason to carry.
//!
//! ## The modelled set
//!
//! Tracks, items, takes, source refs, colours, tempo map, time signatures,
//! track and item envelopes, stretch markers and fades — as settled in #155
//! decision 11. FX chains and MIDI source data are carried **opaquely**, as
//! content-addressed blobs; they round-trip exactly but the editor does not
//! reach into them from here. (MIDI event modelling is #162.)
//!
//! Nothing outside the modelled set is dropped: an imported document keeps
//! its whole source as a verbatim object (see [`Provenance`]) and reports
//! every construct it did not model — see [`crate::rpp::ImportReport`].

use crate::id::{EntityId, ObjectId};
use daw_proto::automation::{Envelope, EnvelopePoint};
use daw_proto::item::Item;
use daw_proto::item::Take;
use daw_proto::marker::Marker;
use daw_proto::stretch_marker::StretchMarker;
use daw_proto::tempo_map::TempoPoint;
use daw_proto::track::{LaneComping, Track};
use facet::Facet;

/// The `.session` format version. Bumped when the schema changes in a way a
/// previous reader cannot absorb.
pub const FORMAT_VERSION: u32 = 1;

/// One save, and the objects it referenced.
///
/// Retained so `compact` can tell a blob that is still reachable from an
/// older save from one nothing points at any more.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SaveEntry {
    /// Monotonic counter, oldest = 1. Not a timestamp: clocks move
    /// backwards across machines and this has to order reliably when a
    /// project is synced.
    pub seq: u64,
    /// The objects this save's manifest referenced.
    pub objects: Vec<ObjectId>,
}

/// One project (or, rooted at something smaller, one template — #175).
#[derive(Clone, Debug, Facet)]
pub struct DawDocument {
    /// Schema version of this file. See [`FORMAT_VERSION`].
    pub format_version: u32,
    /// The project's own stable id.
    pub id: EntityId,
    /// Display name. The file is `<name>.session`, but the name here is
    /// authoritative — renaming the file does not rename the project.
    pub name: String,

    /// Sample rate the project was authored at, when the source declared
    /// one. Optional on purpose: #159 established that a real session can
    /// mix rates, so nothing may assume one rate per project.
    pub sample_rate: Option<u32>,

    /// One entry per save, oldest first.
    ///
    /// This is what makes full version history cost only what actually
    /// changed: an entry is a list of hashes, and the bytes behind an
    /// unchanged hash are already in `objects/`. Forty Kontakt instances
    /// nobody touched cost nothing on save N+1.
    ///
    /// Kept in the manifest rather than in `objects/` because it is
    /// small, textual and mutable — the other side of the invariant that
    /// puts everything large, immutable and binary in the object store.
    #[facet(default)]
    pub saves: Vec<SaveEntry>,

    /// Tempo and time-signature map, ordered by position.
    ///
    /// Points are values, not entities — see the module docs on
    /// [`crate::id`] for why they carry no id.
    pub tempo_map: Vec<TempoPoint>,

    /// Project markers and regions, each with a stable id.
    pub markers: Vec<MarkerNode>,

    /// Tracks in arrange order. Order is data (it is what the arrange view
    /// shows) but it is never an address: every reference is by
    /// [`EntityId`].
    pub tracks: Vec<TrackNode>,

    /// Where this document came from, if it was imported rather than
    /// authored. Carries the verbatim source so a round trip can be exact.
    pub provenance: Option<Provenance>,

    /// Editor state. Lives in the project file, not a sidecar (#155
    /// decision 4). The per-user/per-project split is #157; this is the
    /// hook it will hang from.
    pub editor: EditorState,
    /// The persisted loro oplog, when this project has history (#173).
    ///
    /// `None` on a project that has never been saved, and on one whose
    /// manifest was hand-edited since the log was built — in both cases
    /// history starts fresh from the text, which is the trade the
    /// oplog module documents.
    pub oplog: Option<crate::oplog::OplogRef>,
}

/// A track, its envelopes and its items.
#[derive(Clone, Debug, Facet)]
pub struct TrackNode {
    /// Stable id. Kept equal to `track.guid`; see
    /// [`DawDocument::check_invariants`].
    pub id: EntityId,
    /// The facade's view of the track.
    pub track: Track,
    /// Parent folder track, by id. `None` at the top level.
    ///
    /// Note this is an id, never "the track two rows up".
    pub parent: Option<EntityId>,
    /// Track envelopes (volume, pan, mute, FX parameters, sends).
    pub envelopes: Vec<EnvelopeNode>,
    /// Media items on this track.
    pub items: Vec<ItemNode>,
    /// The track's FX chain, carried opaquely (#155 decision 11).
    pub fx_chain: Option<ObjectId>,
    /// The track's input/record FX chain, carried opaquely.
    pub input_fx_chain: Option<ObjectId>,
    /// Fixed-lane comping: the record / comping lanes and the comp areas
    /// (REAPER's `LANEREC`, `ITEMLANES`, `LINKEDLANE`). Beside the
    /// `Track` rather than on it because the facade keeps it behind a
    /// getter of its own — see [`daw_proto::track::LaneComping`].
    #[facet(default)]
    pub comping: LaneComping,
    /// Signal arriving at this track from other tracks.
    ///
    /// Receives, not sends, because that is where the fact lives: REAPER
    /// writes one `AUXRECV` on the **destination**, and a bus fed by
    /// twelve stems is one list on the bus rather than twelve scattered
    /// lines. See [`ReceiveNode`] for why the source is an id here and an
    /// index there.
    #[facet(default)]
    pub receives: Vec<ReceiveNode>,
}

/// One track feeding another — REAPER's `AUXRECV`, addressed by id.
///
/// `AUXRECV` names its source by **track index**, which is the single
/// worst thing in the `.rpp` format for a tool that reorders tracks:
/// deleting track 3 silently re-points every send above it. The document
/// therefore stores the source as an [`EntityId`] and the exporter
/// resolves it back to an index against the track order it is actually
/// writing.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct ReceiveNode {
    /// The track the signal comes from.
    pub source: EntityId,
    /// Send gain, `1.0` = 0 dB.
    pub volume: f64,
    /// Send pan, `-1.0` hard left to `1.0` hard right.
    pub pan: f64,
    /// Whether the send is muted.
    pub muted: bool,
    /// Whether the send sums to mono.
    pub mono: bool,
    /// Whether the send's polarity is flipped.
    pub phase_inverted: bool,
    /// Where in the source's chain the signal is tapped: `0` post-fader,
    /// `1` pre-FX, `3` pre-fader. REAPER's numbering, kept as REAPER's
    /// numbering rather than renamed into a third vocabulary.
    pub send_mode: i32,
    /// Source channel selector, REAPER's packed `AUXRECV` field.
    pub source_channels: i32,
    /// Destination channel selector, REAPER's packed `AUXRECV` field.
    pub dest_channels: i32,
    /// Pan law, `-1` meaning "the project's".
    pub pan_law: f64,
    /// Packed MIDI channel routing, `-1` meaning "no MIDI".
    pub midi_channels: i32,
    /// Automation mode, `-1` meaning "the track's".
    pub automation_mode: i32,
}

impl ReceiveNode {
    /// A plain post-fader stereo send at unity, which is what a routing
    /// pass that only says "feed the bus" means.
    pub fn new(source: EntityId) -> Self {
        Self {
            source,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            mono: false,
            phase_inverted: false,
            send_mode: 0,
            source_channels: 0,
            dest_channels: 0,
            pan_law: -1.0,
            midi_channels: -1,
            automation_mode: -1,
        }
    }
}

/// A media item and its takes.
#[derive(Clone, Debug, Facet)]
pub struct ItemNode {
    /// Stable id. Kept equal to `item.guid`.
    pub id: EntityId,
    /// The facade's view of the item.
    pub item: Item,
    /// Takes, in order. Exactly one is active — by
    /// [`Take::is_active`](daw_proto::item::Take::is_active), not by
    /// position.
    pub takes: Vec<TakeNode>,
    /// Item envelopes — the item's own volume, pan and mute automation.
    ///
    /// Scenario 1 of #149 composites four generated envelopes into the
    /// item's one volume envelope; all five live here.
    pub envelopes: Vec<EnvelopeNode>,
}

/// A take: one source, played one way.
#[derive(Clone, Debug, Facet)]
pub struct TakeNode {
    /// Stable id. Kept equal to `take.guid`.
    pub id: EntityId,
    /// The facade's view of the take.
    pub take: Take,
    /// Where the take's media comes from.
    pub source: SourceRef,
    /// Stretch markers, ordered by take position. Values, not entities.
    pub stretch_markers: Vec<StretchMarker>,
    /// Per-take envelopes (take volume, pan, pitch, mute).
    pub envelopes: Vec<EnvelopeNode>,
}

/// An envelope and its points.
#[derive(Clone, Debug, Facet)]
pub struct EnvelopeNode {
    /// Stable id. Envelopes imported from a format that gives them no
    /// identifier get one derived from their owner and kind, so re-import
    /// is idempotent — see [`EntityId::derived`].
    pub id: EntityId,
    /// The facade's view of the envelope.
    pub envelope: Envelope,
    /// Points, ordered by time. Values, not entities.
    pub points: Vec<EnvelopePoint>,
}

/// A marker or region.
#[derive(Clone, Debug, Facet)]
pub struct MarkerNode {
    /// Stable id.
    pub id: EntityId,
    /// The facade's view of the marker.
    pub marker: Marker,
    /// Region end, when this is a region rather than a point marker.
    pub region_end_seconds: Option<f64>,
}

/// Where a take's media lives.
///
/// Self-containment is an invariant, not a default (#155 decision 6): a
/// copied project directory opens with nothing else attached. `File` paths
/// are therefore relative to the project directory whenever they can be.
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Facet)]
pub enum SourceRef {
    /// No source — an empty item.
    Empty,
    /// A media file. `path` is relative to the project directory when the
    /// media is inside it, absolute when it is not (which is what
    /// `self_contained` reports on).
    File {
        /// Path as written in the file.
        path: String,
        /// `WAVE`, `MP3`, `FLAC`, … as the source format named it.
        kind: String,
    },
    /// Source data carried as an immutable object — a MIDI take's events,
    /// or any source block the format does not model yet.
    ///
    /// Opaque here does not mean lost: the bytes round-trip exactly, and
    /// modelling MIDI events properly is #162.
    Object {
        /// Content hash into `objects/`.
        object: ObjectId,
        /// `MIDI`, `SECTION`, … so a reader knows what it is holding.
        kind: String,
    },
}

/// What this document was imported from.
///
/// The `source` object is the **verbatim** original — the exact bytes. An
/// unmodified document therefore exports back to those exact bytes; a
/// modified one exports by patching the original, so constructs the schema
/// never modelled still survive the trip.
#[derive(Clone, Debug, Facet)]
pub struct Provenance {
    /// The format the document was imported from.
    pub format: SourceFormat,
    /// Content hash of the verbatim source in `objects/`.
    pub source: ObjectId,
    /// Original filename, for messages and for export defaults.
    pub original_name: Option<String>,
    /// Whether the document has diverged from `source` since it was
    /// imported.
    ///
    /// This is what decides between the two export paths, and it has to
    /// live **in the file**: an in-memory "modified" flag is cleared by
    /// saving, so a project edited, saved and reopened would export the
    /// bytes it was imported from and silently throw the session's work
    /// away. Persisted here, the fact survives the reopen.
    #[facet(default)]
    pub edited: bool,
}

/// Formats a document can be imported from.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub enum SourceFormat {
    /// REAPER `.rpp` / `.RPP`.
    Rpp,
    /// DAWproject `.dawproject` — interop, not native (#155 decision 3).
    /// Import/export is #174; the variant exists so #174 does not have to
    /// migrate the schema.
    DawProject,
}

/// Editor state persisted with the project.
///
/// Deliberately thin. #157 owns what goes in here and how per-user state is
/// separated from per-project state; this crate only guarantees it has a
/// home inside the one file.
#[derive(Clone, Debug, Default, Facet)]
pub struct EditorState {
    /// Which track the expression editor last had open, by id.
    pub focused_track: Option<EntityId>,
    /// Which item the expression editor last had open, by id.
    pub focused_item: Option<EntityId>,
}

impl DawDocument {
    /// An empty project.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            saves: Vec::new(),
            format_version: FORMAT_VERSION,
            id: EntityId::new(),
            name: name.into(),
            sample_rate: None,
            tempo_map: Vec::new(),
            markers: Vec::new(),
            tracks: Vec::new(),
            provenance: None,
            editor: EditorState::default(),
            oplog: None,
        }
    }

    /// Recompute every derived index field from list position.
    ///
    /// Index fields are a cache for the facade's callers and are never used
    /// to find anything (see [`crate::id`]). Calling this after a structural
    /// edit, and on every load, keeps the cache honest.
    pub fn reindex(&mut self) {
        // Send counts are a cache of the receive lists, which live on the
        // destination. Derived here so no caller has to keep two numbers in
        // step by hand.
        let mut sends: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for track_node in &self.tracks {
            for receive in &track_node.receives {
                *sends
                    .entry(receive.source.as_str().to_string())
                    .or_insert(0) += 1;
            }
        }

        for (track_pos, track_node) in self.tracks.iter_mut().enumerate() {
            track_node.track.receive_count = track_node.receives.len() as u32;
            track_node.track.send_count = sends.get(track_node.id.as_str()).copied().unwrap_or(0);
            track_node.track.index = track_pos as u32;
            track_node.track.guid = track_node.id.as_str().to_string();
            track_node.track.parent_guid = track_node
                .parent
                .as_ref()
                .map(|parent| parent.as_str().to_string());

            for envelope in &mut track_node.envelopes {
                envelope.sync();
                envelope.envelope.track_guid = track_node.id.as_str().to_string();
            }

            for (item_pos, item_node) in track_node.items.iter_mut().enumerate() {
                item_node.item.index = item_pos as u32;
                item_node.item.guid = item_node.id.as_str().to_string();
                item_node.item.track_guid = track_node.id.as_str().to_string();
                item_node.item.take_count = item_node.takes.len() as u32;

                for envelope in &mut item_node.envelopes {
                    envelope.sync();
                }

                let mut active = 0;
                for (take_pos, take_node) in item_node.takes.iter_mut().enumerate() {
                    take_node.take.index = take_pos as u32;
                    take_node.take.guid = take_node.id.as_str().to_string();
                    take_node.take.item_guid = item_node.id.as_str().to_string();
                    if take_node.take.is_active {
                        active = take_pos as u32;
                    }
                    for envelope in &mut take_node.envelopes {
                        envelope.sync();
                    }
                }
                item_node.item.active_take_index = active;
            }
        }
    }

    /// Re-derive REAPER's folder encoding from the `parent` links.
    ///
    /// `parent` is the document's truth; `is_folder` and `folder_depth`
    /// are the `.rpp` spelling of it — `ISBUS <opens a folder> <levels
    /// closed after this track>`. Anything that changes the hierarchy has
    /// to refresh the spelling, or the exported project comes back flat.
    ///
    /// Deliberately **not** called from [`reindex`](Self::reindex): a
    /// document loaded from a file must be left with the encoding its
    /// source wrote, so an untouched export stays byte-identical even on a
    /// project REAPER itself spelled oddly. The structural edits in
    /// [`crate::DocumentEdit`] call it, because they are exactly the
    /// operations that invalidate it.
    ///
    /// Assumes the arrange order is a depth-first walk of the hierarchy,
    /// which is the only order `.rpp` can express in the first place.
    pub fn rebuild_folder_encoding(&mut self) {
        let parents: std::collections::HashMap<String, Option<String>> = self
            .tracks
            .iter()
            .map(|node| {
                (
                    node.id.as_str().to_string(),
                    node.parent.as_ref().map(|p| p.as_str().to_string()),
                )
            })
            .collect();

        // Ancestor count, guarded against a cycle a hand-edited file could
        // carry: `set_parent` refuses to make one, but nothing stops a text
        // editor, and an unguarded walk would hang rather than complain.
        let depth_of = |id: &str| -> i32 {
            let mut depth = 0;
            let mut cursor = parents.get(id).cloned().flatten();
            while let Some(parent) = cursor {
                depth += 1;
                if depth > self.tracks.len() as i32 {
                    break;
                }
                cursor = parents.get(&parent).cloned().flatten();
            }
            depth
        };

        let depths: Vec<i32> = self
            .tracks
            .iter()
            .map(|node| depth_of(node.id.as_str()))
            .collect();

        for position in 0..self.tracks.len() {
            // The depth change REAPER writes is simply "how much deeper is
            // the next row" — positive opens a folder, negative closes as
            // many levels as it is deep. Past the end everything closes.
            let next = depths.get(position + 1).copied().unwrap_or(0);
            let change = next - depths[position];
            self.tracks[position].track.folder_depth = change;
            self.tracks[position].track.is_folder = change > 0;
        }
    }

    /// Check the format's structural invariants, returning every violation
    /// rather than the first.
    ///
    /// Enforced here so a bug that reintroduces positional referencing
    /// fails a test rather than surfacing as a mangled project three
    /// tickets later.
    pub fn check_invariants(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut track_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for track_node in &self.tracks {
            track_ids.insert(track_node.id.as_str());
        }

        for track_node in &self.tracks {
            if track_node.id.as_str().is_empty() {
                problems.push("a track has an empty id".to_string());
            }
            if !seen.insert(track_node.id.as_str()) {
                problems.push(format!("duplicate track id {}", track_node.id));
            }
            if track_node.track.guid != track_node.id.as_str() {
                problems.push(format!(
                    "track {} has guid {} — id and guid must agree",
                    track_node.id, track_node.track.guid
                ));
            }
            if let Some(parent) = &track_node.parent
                && !track_ids.contains(parent.as_str())
            {
                problems.push(format!(
                    "track {} names parent {parent}, which is not in the document",
                    track_node.id
                ));
            }
            for receive in &track_node.receives {
                // A receive from a track that is not here cannot be given a
                // `AUXRECV` index on export, and silently dropping the send
                // is how a bus goes quiet without anyone noticing.
                if !track_ids.contains(receive.source.as_str()) {
                    problems.push(format!(
                        "track {} receives from {}, which is not in the document",
                        track_node.id, receive.source
                    ));
                }
            }

            for item_node in &track_node.items {
                if !seen.insert(item_node.id.as_str()) {
                    problems.push(format!("duplicate item id {}", item_node.id));
                }
                if item_node.item.track_guid != track_node.id.as_str() {
                    problems.push(format!(
                        "item {} claims track {}, but sits under {}",
                        item_node.id, item_node.item.track_guid, track_node.id
                    ));
                }
                for take_node in &item_node.takes {
                    if !seen.insert(take_node.id.as_str()) {
                        problems.push(format!("duplicate take id {}", take_node.id));
                    }
                }
            }
        }
        problems
    }

    /// Whether the project is self-contained — every media reference is
    /// relative, so a copy of the directory opens anywhere (#155 decision 6).
    ///
    /// Returns the offending absolute paths rather than a bare bool, because
    /// the useful thing to show a user is *which* file is outside.
    pub fn external_media(&self) -> Vec<&str> {
        let mut external = Vec::new();
        for track_node in &self.tracks {
            for item_node in &track_node.items {
                for take_node in &item_node.takes {
                    if let SourceRef::File { path, .. } = &take_node.source
                        && (path.starts_with('/') || path.contains(":\\"))
                    {
                        external.push(path.as_str());
                    }
                }
            }
        }
        external
    }

    /// Every object this document references. What #172's `compact` will
    /// mark as reachable, and what a save must be sure it wrote.
    pub fn referenced_objects(&self) -> Vec<ObjectId> {
        let mut objects = Vec::new();
        if let Some(provenance) = &self.provenance {
            objects.push(provenance.source.clone());
        }
        // The oplog is referenced by the manifest, so it is reachable —
        // without this a compaction silently deletes the project's
        // history and the next load starts fresh from the text.
        if let Some(oplog) = &self.oplog {
            objects.push(oplog.object.clone());
        }
        for track_node in &self.tracks {
            objects.extend(track_node.fx_chain.clone());
            objects.extend(track_node.input_fx_chain.clone());
            for item_node in &track_node.items {
                for take_node in &item_node.takes {
                    if let SourceRef::Object { object, .. } = &take_node.source {
                        objects.push(object.clone());
                    }
                }
            }
        }
        objects.sort();
        objects.dedup();
        objects
    }
}

impl EnvelopeNode {
    /// Refresh the envelope's derived fields from its points.
    pub fn sync(&mut self) {
        self.envelope.point_count = self.points.len() as u32;
        for (position, point) in self.points.iter_mut().enumerate() {
            point.index = position as u32;
        }
    }
}

impl DawDocument {
    /// The track whose stable id is `guid`.
    ///
    /// Named for the caller's vocabulary: `.rpp` and the `daw` facade both
    /// say "guid", and this is the lookup the exporter uses to pair a chunk
    /// with its node.
    pub fn track_by_guid(&self, guid: &str) -> Option<&TrackNode> {
        self.tracks.iter().find(|node| node.id.as_str() == guid)
    }
}
