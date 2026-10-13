//! The session document: where each part of a [`SessionModel`] lives in
//! the Loro doc, and the two directions between them.
//!
//! Schema (version [`SCHEMA`]):
//!
//! ```text
//! meta   : Map   { schema: i64, tempo: String }
//! tracks : Tree  one node per track, folders as parents, sibling order
//!                by fractional index; each node's meta map holds `guid`
//!                and the track's fields
//! items  : Map   guid → Map { track, position, …, "take.source", … }
//! markers: Map   guid → Map { at, name, color }
//! regions: Map   guid → Map { start, end, name, color }
//! chart  : Text  the keyflow chart
//! take_markers: Map "<source>#<id>" → Map { source, id, at, name, color,
//!                hit, hit.piece, hit.origin, hit.run, hit.peak_db,
//!                hit.confidence, hit.from_trigger, hit.from_mic,
//!                hit.ghost, hit.bleed, hit.flam_with, hit.note }
//! fx     : Map   fx id → Map { track, index, plugin, name, enabled,
//!                offline, state }  (state: a blob's name in the store)
//! sends  : Map   "<track>#<n>" → Map { track, index, dest, mode, volume,
//!                pan, muted, from, to }
//! envelopes: Map "<track>|<what>" → Map { track, what, mode, visible,
//!                armed, points, own_lane, lane_height, items }
//!                (points: [at, value, shape, tension]…)
//! hw_outputs: Map "<track>#<n>" → Map { as a send's, dest the output }
//! project: Map  { master.volume, master.pan, master.muted, ext,
//!                ruler_lanes, automation_override, bay, looping,
//!                metronome, playrate, record_mode }
//! ```
//!
//! Schema 2 adds `fx`, `sends` and `envelopes`, a track's fixed lanes and
//! groups (`lanes.*`, `group.*` on its node), and an item's fixed lane,
//! loop, stretch markers and MIDI notes (`take.stretch`, `take.notes`,
//! flat number lists). An FX, a send, an envelope is each its own map, so
//! one person bypassing a plugin while another automates a fader both
//! land.
//!
//! Schema 3 adds `take_markers`: a recording's markers, drum hits among
//! them, each its own map — one person approving a hit while another
//! moves the next both land.
//!
//! Schema 4 holds the rest of the project, so that every edit is a step
//! the doc records and an undo can put back:
//!
//! - on a track's node: `armed`, `automation_mode`, `monitor`,
//!   `record_input`, `channels`, `height`, `width`, `lane_display`, its
//!   comping (`comp.record`, `comp.lane`, `comp.last`, `comp.areas`:
//!   `[start, end, source lane, comp lane, fade in, fade out]…`) and its
//!   ext-state under named sections (`ext_sections`);
//! - an FX's `chain` (`""` a track's own, `input`, `monitoring`), its
//!   `params` (`[id, value]…`), `param_count`, `preset`, `preset_name`
//!   and named `config`; the master bus's FX, sends and envelopes are
//!   kept as a track's, under the track id `master`, and the monitoring
//!   chain's FX under no track;
//! - a send's `mono`, `phase_inverted`, `automation_mode`, channel
//!   counts, MIDI channels and far end's `label`; hardware outputs, in
//!   `hw_outputs`;
//! - an envelope's lane (`own_lane`, `lane_height`) and its automation
//!   items (`items`, each with its own points); a track's envelopes are
//!   also its sends' (`send:<n>:<volume|pan|mute>`), play rate and pitch
//!   (`playrate`, `pitch`), and named ones (`named:<name>`);
//! - an item's `beat_attach`, `auto_stretch`, `group`, `start_offset`,
//!   every take (`takes`: the others, in order, each a map; the active
//!   one's place `active_take`), and the active take's fields beyond the
//!   flat `take.*` ones in `take.more`: colour, gain, pitch, channel mode,
//!   source details, stretch mode, its envelopes other than volume, which
//!   notes are muted, and its MIDI events other than notes (`midi`:
//!   `cc`, `pb`, `pc`, `chp`, `pp`, `nx` flat number lists and `sysex`).
//!
//! `take.more`, `takes`, an envelope's `items`, the project's `ext` and
//! `ruler_lanes` are each one value: two people changing one of them at
//! once is last-writer-wins for the whole of it. A take with no file
//! keeps its markers in `take_markers` under `take:<item>:<place>`.
//!
//! A doc written before schema 4 reads with each of these at the
//! engine's default.
//!
//! Every field is its own map entry, so two people changing different
//! things about the same item — one moves it, one turns it down — both
//! land. The same field changed twice concurrently is last-writer-wins,
//! which for a fader or a position is what anyone would expect.
//!
//! The tempo map is one value on purpose: a tempo map is only meaningful
//! whole, and merging two people's halves of one would be a tempo map
//! neither of them wrote.
//!
//! Writing is a *reconcile*: [`SessionDoc::write`] compares the model
//! with what the doc already holds and emits operations only for what
//! differs. Writing the same model twice records nothing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use loro::{
    ExportMode, LoroDoc, LoroEncodeError, LoroError, LoroMap, LoroResult, LoroText, LoroTree,
    LoroValue, TreeID, TreeParentId, UpdateOptions, ValueOrContainer, VersionVector,
};

use crate::model::{
    AutomationItemState, CcState, ChannelPressureState, CompAreaState, CompingState,
    EnvelopePointState, EnvelopeState, FxState, GainPoint, HitState, ItemState, LaneState,
    MarkerState, MidiEvents, NoteExpressionState, NoteState, PitchBendState, PolyPressureState,
    ProgramChangeState, ProjectSettings, RegionState, RulerLaneState, SendState, SessionModel,
    Sources, StretchPoint, SysexState, TakeMarkerState, TakeState, TempoPoint, TrackState,
};

/// The schema version written into `meta.schema`.
pub const SCHEMA: i64 = 4;

const META: &str = "meta";
const TRACKS: &str = "tracks";
const ITEMS: &str = "items";
const MARKERS: &str = "markers";
const REGIONS: &str = "regions";
const CHART: &str = "chart";
const FX: &str = "fx";
const SENDS: &str = "sends";
const ENVELOPES: &str = "envelopes";
const TAKE_MARKERS: &str = "take_markers";
const HW_OUTPUTS: &str = "hw_outputs";
const PROJECT: &str = "project";

/// The track id the master bus's FX, sends and envelopes are kept under —
/// the engine's own name for it.
pub const MASTER: &str = "master";

/// Origin tag for commits made from the local engine.
pub const ORIGIN_LOCAL: &str = "local";

/// One song's live session as a Loro document.
///
/// Cloning shares the document (Loro docs are handles), which is how the
/// sync driver and the bridge work on one replica.
#[derive(Debug, Clone)]
pub struct SessionDoc {
    doc: LoroDoc,
}

impl Default for SessionDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionDoc {
    /// An empty session.
    #[must_use]
    pub fn new() -> Self {
        Self::from_loro(LoroDoc::new())
    }

    /// Wrap an existing doc — one restored from a `.session` file or
    /// handed over by a sync driver.
    #[must_use]
    pub fn from_loro(doc: LoroDoc) -> Self {
        doc.get_tree(TRACKS).enable_fractional_index(0);
        Self { doc }
    }

    /// A replica with its own peer id, for a second participant.
    #[must_use]
    pub fn fork(&self) -> Self {
        Self::from_loro(self.doc.fork())
    }

    /// The underlying Loro doc (for sync drivers, persistence, history).
    #[must_use]
    pub const fn loro(&self) -> &LoroDoc {
        &self.doc
    }

    /// The whole history, for saving.
    ///
    /// # Errors
    /// When Loro cannot encode the document.
    pub fn snapshot(&self) -> Result<Vec<u8>, LoroEncodeError> {
        self.doc.export(ExportMode::Snapshot)
    }

    /// What this replica has that a peer at `since` does not.
    ///
    /// # Errors
    /// When Loro cannot encode the updates.
    pub fn updates_since(&self, since: &VersionVector) -> Result<Vec<u8>, LoroEncodeError> {
        self.doc.export(ExportMode::updates(since))
    }

    /// This replica's version, to ask a peer for what it is missing.
    #[must_use]
    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    /// Merge a snapshot or updates from elsewhere.
    ///
    /// # Errors
    /// When the bytes are not a Loro export this doc can import.
    pub fn import(&self, bytes: &[u8], origin: &str) -> Result<(), LoroError> {
        self.doc.import_with(bytes, origin).map(|_| ())
    }

    /// Reconcile the doc with `model`, committing under `origin`.
    ///
    /// # Errors
    /// When a Loro operation fails (a corrupt tree, say).
    pub fn write(&self, model: &SessionModel, origin: &str) -> LoroResult<()> {
        let meta = self.doc.get_map(META);
        put(&meta, "schema", LoroValue::I64(SCHEMA))?;
        put(&meta, "tempo", LoroValue::from(encode_tempo(&model.tempo)))?;
        self.write_tracks(&model.tracks)?;
        write_entities(&self.doc.get_map(ITEMS), &model.items, item_fields)?;
        write_entities(&self.doc.get_map(MARKERS), &model.markers, marker_fields)?;
        write_entities(&self.doc.get_map(REGIONS), &model.regions, region_fields)?;
        self.write_parts(&model.tracks, &model.project)?;
        sync_fields(&self.doc.get_map(PROJECT), &project_fields(&model.project))?;
        write_text(&self.doc.get_text(CHART), &model.chart)?;
        write_entities(
            &self.doc.get_map(TAKE_MARKERS),
            &model.take_markers,
            take_marker_fields,
        )?;
        self.doc.set_next_commit_origin(origin);
        self.doc.commit();
        Ok(())
    }

    /// Move the doc from `before` — what it holds now — to `model`, as one
    /// commit under `origin` named `label`: only what differs is written,
    /// so a step that moved one item of thousands compares one.
    ///
    /// `before` must be what the doc reads as (the bridge's last model);
    /// anything else and the doc keeps what `before` claimed unchanged.
    ///
    /// # Errors
    /// When a Loro operation fails.
    pub fn write_step(
        &self,
        before: &SessionModel,
        model: &SessionModel,
        origin: &str,
        label: &str,
    ) -> LoroResult<()> {
        let meta = self.doc.get_map(META);
        put(&meta, "schema", LoroValue::I64(SCHEMA))?;
        if before.tempo != model.tempo {
            put(&meta, "tempo", LoroValue::from(encode_tempo(&model.tempo)))?;
        }
        if before.tracks != model.tracks {
            self.write_tracks(&model.tracks)?;
        }
        if before.tracks != model.tracks || before.project != model.project {
            self.write_parts(&model.tracks, &model.project)?;
        }
        if before.project != model.project {
            sync_fields(&self.doc.get_map(PROJECT), &project_fields(&model.project))?;
        }
        write_changed(
            &self.doc.get_map(ITEMS),
            &before.items,
            &model.items,
            item_fields,
        )?;
        write_changed(
            &self.doc.get_map(MARKERS),
            &before.markers,
            &model.markers,
            marker_fields,
        )?;
        write_changed(
            &self.doc.get_map(REGIONS),
            &before.regions,
            &model.regions,
            region_fields,
        )?;
        if before.chart != model.chart {
            write_text(&self.doc.get_text(CHART), &model.chart)?;
        }
        write_changed(
            &self.doc.get_map(TAKE_MARKERS),
            &before.take_markers,
            &model.take_markers,
            take_marker_fields,
        )?;
        self.doc.set_next_commit_origin(origin);
        if !label.is_empty() {
            self.doc.set_next_commit_message(label);
        }
        self.doc.commit();
        Ok(())
    }

    /// Whether anything has been written or received yet — a joiner
    /// waits for this before letting its bridge at the doc.
    #[must_use]
    pub fn has_session(&self) -> bool {
        matches!(
            self.doc.get_map(META).get("schema"),
            Some(ValueOrContainer::Value(LoroValue::I64(_)))
        )
    }

    /// A stable position in the chart text (Unicode offset) that stays put
    /// while others type, encoded for presence.
    #[must_use]
    pub fn chart_cursor(&self, pos: usize) -> Option<Vec<u8>> {
        let text = self.doc.get_text(CHART);
        text.get_cursor(pos.min(text.len_unicode()), loro::cursor::Side::Left)
            .map(|c| c.encode())
    }

    /// Where an encoded chart cursor (from [`Self::chart_cursor`]) is now.
    #[must_use]
    pub fn resolve_chart_cursor(&self, bytes: &[u8]) -> Option<usize> {
        let cursor = loro::cursor::Cursor::decode(bytes).ok()?;
        self.doc.get_cursor_pos(&cursor).ok().map(|p| p.current.pos)
    }

    /// The model the doc currently holds.
    #[must_use]
    pub fn read(&self) -> SessionModel {
        let meta = map_value(&self.doc.get_map(META).get_deep_value());
        let tempo = meta
            .get("tempo")
            .and_then(as_string)
            .map(|s| decode_tempo(&s))
            .unwrap_or_default();
        let (tracks, project) = self.read_tracks_and_project();
        SessionModel {
            tracks,
            items: read_entities(&self.doc.get_map(ITEMS), read_item),
            markers: read_entities(&self.doc.get_map(MARKERS), read_marker),
            regions: read_entities(&self.doc.get_map(REGIONS), read_region),
            tempo,
            chart: self.doc.get_text(CHART).to_string(),
            take_markers: read_entities(&self.doc.get_map(TAKE_MARKERS), read_take_marker),
            project,
        }
    }

    /// The tracks with their parts, and the project's settings with the
    /// master's and the monitoring chain's.
    fn read_tracks_and_project(&self) -> (Vec<TrackState>, ProjectSettings) {
        let mut tracks = self.read_tracks();
        let mut project = read_project(&map_value(&self.doc.get_map(PROJECT).get_deep_value()));
        let parts = Parts {
            fx: read_entities(&self.doc.get_map(FX), read_fx),
            sends: read_entities(&self.doc.get_map(SENDS), read_send),
            hw: read_entities(&self.doc.get_map(HW_OUTPUTS), read_send),
            envelopes: read_entities(&self.doc.get_map(ENVELOPES), read_envelope),
        };
        attach_parts(&mut tracks, &mut project, &parts);
        (tracks, project)
    }

    /// Every FX, send, hardware output and envelope — the tracks', the
    /// master's, the monitoring chain's — each in its own map.
    fn write_parts(&self, tracks: &[TrackState], project: &ProjectSettings) -> LoroResult<()> {
        let parts = track_parts(tracks, project);
        write_entities(&self.doc.get_map(FX), &parts.fx, fx_fields)?;
        write_entities(&self.doc.get_map(SENDS), &parts.sends, send_fields)?;
        write_entities(&self.doc.get_map(HW_OUTPUTS), &parts.hw, send_fields)?;
        write_entities(
            &self.doc.get_map(ENVELOPES),
            &parts.envelopes,
            envelope_fields,
        )
    }

    /// Note what each change to the doc touches, from now on, into the
    /// returned [`Touched`] — until the subscription is dropped. A change
    /// committed as [`ORIGIN_LOCAL`] is the engine's own, written from
    /// what it shows, and is not noted: only what the engine has yet to
    /// match is (another peer's edit, an undo).
    #[must_use]
    pub fn watch(&self) -> Watch {
        let touched = Arc::new(Mutex::new(Touched::everything()));
        let into = Arc::clone(&touched);
        let subscription = self.doc.subscribe_root(Arc::new(move |event| {
            if event.origin == ORIGIN_LOCAL {
                return;
            }
            let Ok(mut touched) = into.lock() else {
                return;
            };
            for diff in &event.events {
                touched.note(diff);
            }
        }));
        Watch {
            touched,
            subscription,
        }
    }

    /// The doc as a model, read only where `touched` says it changed: the
    /// rest taken from `base`, what the doc read as before those changes.
    #[must_use]
    pub fn read_touched(&self, base: &SessionModel, touched: &Touched) -> SessionModel {
        if touched.everything {
            return self.read();
        }
        let mut model = base.clone();
        if touched.parts.tracks || touched.parts.project {
            // The master's parts are kept beside the tracks', so either
            // reads both.
            let (tracks, project) = self.read_tracks_and_project();
            model.tracks = tracks;
            model.project = project;
        }
        reread(
            &self.doc.get_map(ITEMS),
            &touched.items,
            &mut model.items,
            read_item,
        );
        reread(
            &self.doc.get_map(MARKERS),
            &touched.markers,
            &mut model.markers,
            read_marker,
        );
        reread(
            &self.doc.get_map(REGIONS),
            &touched.regions,
            &mut model.regions,
            read_region,
        );
        reread(
            &self.doc.get_map(TAKE_MARKERS),
            &touched.take_markers,
            &mut model.take_markers,
            read_take_marker,
        );
        if touched.parts.meta {
            model.tempo = map_value(&self.doc.get_map(META).get_deep_value())
                .get("tempo")
                .and_then(as_string)
                .map(|s| decode_tempo(&s))
                .unwrap_or_default();
        }
        if touched.parts.chart {
            model.chart = self.doc.get_text(CHART).to_string();
        }
        model
    }

    // ── tracks ──────────────────────────────────────────────────────

    /// Each live track node by guid, plus any second node for a guid
    /// already seen.
    ///
    /// A duplicate happens when two replicas each create a node for the
    /// same track before they have synced — two people opening their own
    /// copy of one `.session` and joining. The canonical node is the
    /// smallest `TreeID`, the same answer on every replica, so each one
    /// deletes the same extras and they converge.
    fn track_nodes(tree: &LoroTree) -> (HashMap<String, TreeID>, Vec<TreeID>) {
        let mut all: Vec<(String, TreeID)> = tree
            .get_nodes(false)
            .into_iter()
            .filter_map(|node| {
                let meta = tree.get_meta(node.id).ok()?;
                let guid = map_value(&meta.get_deep_value())
                    .get("guid")
                    .and_then(as_string)?;
                Some((guid, node.id))
            })
            .collect();
        all.sort_by_key(|(_, id)| (id.peer, id.counter));
        let mut nodes = HashMap::new();
        let mut extras = Vec::new();
        for (guid, id) in all {
            if let std::collections::hash_map::Entry::Vacant(slot) = nodes.entry(guid) {
                slot.insert(id);
            } else {
                extras.push(id);
            }
        }
        (nodes, extras)
    }

    fn write_tracks(&self, tracks: &[TrackState]) -> LoroResult<()> {
        let tree = self.doc.get_tree(TRACKS);
        let (mut nodes, extras) = Self::track_nodes(&tree);
        for id in extras {
            if tree.contains(id) && !tree.is_node_deleted(&id)? {
                tree.delete(id)?;
            }
        }
        // How many of each folder's children have been placed so far:
        // the index the next one belongs at.
        let mut placed: HashMap<Option<String>, usize> = HashMap::new();

        for track in tracks {
            // A folder the model does not hold leaves the track at the
            // top level rather than losing it.
            let parent = track
                .parent
                .as_ref()
                .and_then(|guid| nodes.get(guid))
                .map_or(TreeParentId::Root, |id| TreeParentId::Node(*id));
            let slot = placed.entry(track.parent.clone()).or_insert(0);
            let index = *slot;
            *slot = slot.saturating_add(1);

            let id = if let Some(id) = nodes.get(&track.guid).copied() {
                let here = tree.parent(id) == Some(parent)
                    && tree
                        .children(parent)
                        .and_then(|c| c.iter().position(|n| *n == id))
                        == Some(index);
                if !here {
                    tree.mov_to(id, parent, index)?;
                }
                id
            } else {
                let id = tree.create_at(parent, index)?;
                nodes.insert(track.guid.clone(), id);
                id
            };
            sync_fields(&tree.get_meta(id)?, &track_fields(track))?;
        }

        let keep: HashSet<&str> = tracks.iter().map(|t| t.guid.as_str()).collect();
        for (guid, id) in &nodes {
            if !keep.contains(guid.as_str()) && tree.contains(*id) && !tree.is_node_deleted(id)? {
                tree.delete(*id)?;
            }
        }
        Ok(())
    }

    fn read_tracks(&self) -> Vec<TrackState> {
        fn walk(
            tree: &LoroTree,
            canonical: &HashMap<String, TreeID>,
            parent: TreeParentId,
            folder: Option<&str>,
            out: &mut Vec<TrackState>,
        ) {
            for id in tree.children(parent).unwrap_or_default() {
                let Ok(meta) = tree.get_meta(id) else {
                    continue;
                };
                let fields = map_value(&meta.get_deep_value());
                let Some(track) = read_track(&fields, folder) else {
                    continue;
                };
                if canonical.get(&track.guid) != Some(&id) {
                    continue; // a duplicate node; see `track_nodes`
                }
                let guid = track.guid.clone();
                out.push(track);
                walk(tree, canonical, TreeParentId::Node(id), Some(&guid), out);
            }
        }
        let tree = self.doc.get_tree(TRACKS);
        let (canonical, _) = Self::track_nodes(&tree);
        let mut out = Vec::new();
        walk(&tree, &canonical, TreeParentId::Root, None, &mut out);
        out
    }
}

// ── generic entity maps ─────────────────────────────────────────────────

type Fields = Vec<(&'static str, LoroValue)>;

fn write_entities<T>(
    root: &LoroMap,
    entities: &BTreeMap<String, T>,
    fields: fn(&T) -> Fields,
) -> LoroResult<()> {
    for (guid, entity) in entities {
        // Mergeable: two peers creating the same entity concurrently get
        // ONE map, not two containers racing for the key.
        let map = root.ensure_mergeable_map(guid)?;
        sync_fields(&map, &fields(entity))?;
    }
    let stale: Vec<String> = root
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !entities.contains_key(k))
        .collect();
    for key in stale {
        root.delete(&key)?;
    }
    Ok(())
}

/// The entities of `now` that differ from `before` written, and those
/// `before` had that `now` does not deleted.
fn write_changed<T: PartialEq>(
    root: &LoroMap,
    before: &BTreeMap<String, T>,
    now: &BTreeMap<String, T>,
    fields: fn(&T) -> Fields,
) -> LoroResult<()> {
    for (guid, entity) in now {
        if before.get(guid) != Some(entity) {
            let map = root.ensure_mergeable_map(guid)?;
            sync_fields(&map, &fields(entity))?;
        }
    }
    for guid in before.keys().filter(|k| !now.contains_key(*k)) {
        root.delete(guid)?;
    }
    Ok(())
}

/// What changes to the doc touched since it was last taken.
///
/// Which items, markers and regions by id; which whole parts (the tracks
/// with their FX, sends and envelopes; the tempo; the chart). `everything`
/// until the first take, and whenever a change could not be placed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    pub everything: bool,
    pub parts: Whole,
    pub items: std::collections::BTreeSet<String>,
    pub markers: std::collections::BTreeSet<String>,
    pub regions: std::collections::BTreeSet<String>,
    pub take_markers: std::collections::BTreeSet<String>,
}

/// What [`SessionDoc::watch`] notes into, while it is kept.
pub struct Watch {
    /// What has been touched since last taken.
    pub touched: Arc<Mutex<Touched>>,
    /// The doc's notes to it: dropped, they stop.
    pub subscription: loro::Subscription,
}

/// The parts of the doc read back whole when touched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Whole {
    /// The track tree, and the FX, sends, hardware outputs and envelopes
    /// on it.
    pub tracks: bool,
    /// The tempo map.
    pub meta: bool,
    pub chart: bool,
    /// The project's settings and the master bus.
    pub project: bool,
}

impl Touched {
    /// Everything: read the whole doc.
    #[must_use]
    pub fn everything() -> Self {
        Self {
            everything: true,
            ..Self::default()
        }
    }

    /// Whether nothing at all was touched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn note(&mut self, diff: &loro::event::ContainerDiff<'_>) {
        // An entity's own map is a mergeable child of its root map, its id
        // naming both (`🤝:$items>guid`). Otherwise the path runs from the
        // root (its first step names the root itself) down to the target:
        // the step after the root is the key, and a root map's own change
        // names the keys it changed.
        let (root, key) = if let Some((root, key)) = mergeable(diff.target) {
            (Some(root), Some(key))
        } else {
            let root = diff
                .path
                .first()
                .map_or_else(|| root_name(diff.target), |(id, _)| root_name(id));
            let key = if let Some((_, loro::Index::Key(key))) = diff.path.get(1) {
                Some(key.to_string())
            } else {
                None
            };
            (root, key)
        };
        let keys: Vec<String> = match (key, &diff.diff) {
            (Some(key), _) => vec![key],
            (None, loro::event::Diff::Map(delta)) => {
                delta.updated.keys().map(ToString::to_string).collect()
            }
            (None, _) => Vec::new(),
        };
        match root.as_deref() {
            Some(TRACKS | FX | SENDS | ENVELOPES | HW_OUTPUTS) => self.parts.tracks = true,
            Some(PROJECT) => self.parts.project = true,
            Some(META) => self.parts.meta = true,
            Some(CHART) => self.parts.chart = true,
            Some(ITEMS) if !keys.is_empty() => self.items.extend(keys),
            Some(MARKERS) if !keys.is_empty() => self.markers.extend(keys),
            Some(REGIONS) if !keys.is_empty() => self.regions.extend(keys),
            Some(TAKE_MARKERS) if !keys.is_empty() => self.take_markers.extend(keys),
            _ => self.everything = true,
        }
    }
}

/// A root container's name.
fn root_name(id: &loro::ContainerID) -> Option<String> {
    match id {
        loro::ContainerID::Root { name, .. } => match mergeable(id) {
            Some((root, _)) => Some(root),
            None => Some(name.to_string()),
        },
        loro::ContainerID::Normal { .. } => None,
    }
}

/// A mergeable child's root map and key, from its id's name.
fn mergeable(id: &loro::ContainerID) -> Option<(String, String)> {
    let loro::ContainerID::Root { name, .. } = id else {
        return None;
    };
    let (root, key) = name.strip_prefix("🤝:$")?.split_once('>')?;
    Some((root.to_owned(), key.to_owned()))
}

/// The entities of `keys` read again from `root` into `into`: those gone
/// from the doc removed.
fn reread<T>(
    root: &LoroMap,
    keys: &std::collections::BTreeSet<String>,
    into: &mut BTreeMap<String, T>,
    read: fn(&HashMap<String, LoroValue>) -> T,
) {
    for key in keys {
        match root.get(key) {
            Some(ValueOrContainer::Container(loro::Container::Map(map))) => {
                into.insert(key.clone(), read(&map_value(&map.get_deep_value())));
            }
            _ => {
                into.remove(key);
            }
        }
    }
}

fn read_entities<T>(
    root: &LoroMap,
    read: fn(&HashMap<String, LoroValue>) -> T,
) -> BTreeMap<String, T> {
    match root.get_deep_value() {
        LoroValue::Map(entities) => entities
            .iter()
            .filter_map(|(guid, value)| match value {
                LoroValue::Map(_) => Some((guid.clone(), read(&map_value(value)))),
                _ => None,
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Set each field that differs from what the map holds.
fn sync_fields(map: &LoroMap, fields: &[(&'static str, LoroValue)]) -> LoroResult<()> {
    for (key, value) in fields {
        put(map, key, value.clone())?;
    }
    Ok(())
}

fn put(map: &LoroMap, key: &str, value: LoroValue) -> LoroResult<()> {
    if let Some(ValueOrContainer::Value(current)) = map.get(key)
        && current == value
    {
        return Ok(());
    }
    map.insert(key, value)
}

fn write_text(text: &LoroText, value: &str) -> LoroResult<()> {
    if text.to_string() == value {
        return Ok(());
    }
    text.update(value, UpdateOptions::default())
        .map_err(|e| LoroError::Unknown(format!("chart diff: {e:?}").into_boxed_str()))
}

fn map_value(value: &LoroValue) -> HashMap<String, LoroValue> {
    match value {
        LoroValue::Map(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

// ── field codecs ────────────────────────────────────────────────────────

/// An optional `u32` — a colour, a lane — as the doc keeps it: a number,
/// or null for none.
fn color(c: Option<u32>) -> LoroValue {
    c.map_or(LoroValue::Null, |c| LoroValue::I64(i64::from(c)))
}

fn opt_str(s: Option<&str>) -> LoroValue {
    s.map_or(LoroValue::Null, LoroValue::from)
}

fn as_f64(v: &LoroValue) -> Option<f64> {
    match v {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => i32::try_from(*i).ok().map(f64::from),
        _ => None,
    }
}

const fn as_bool(v: &LoroValue) -> Option<bool> {
    match v {
        LoroValue::Bool(b) => Some(*b),
        _ => None,
    }
}

fn as_string(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn as_color(v: &LoroValue) -> Option<u32> {
    match v {
        LoroValue::I64(i) => u32::try_from(*i).ok(),
        _ => None,
    }
}

struct Get<'a>(&'a HashMap<String, LoroValue>);

impl Get<'_> {
    fn f64(&self, key: &str, default: f64) -> f64 {
        self.0.get(key).and_then(as_f64).unwrap_or(default)
    }
    fn bool(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(as_bool).unwrap_or(default)
    }
    fn string(&self, key: &str) -> String {
        self.0.get(key).and_then(as_string).unwrap_or_default()
    }
    /// A string, `default` when the doc predates it.
    fn string_or(&self, key: &str, default: &str) -> String {
        self.0
            .get(key)
            .and_then(as_string)
            .unwrap_or_else(|| default.to_owned())
    }
    fn opt_string(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(as_string)
    }
    fn color(&self, key: &str) -> Option<u32> {
        self.0.get(key).and_then(as_color)
    }
    fn i64(&self, key: &str, default: i64) -> i64 {
        match self.0.get(key) {
            Some(LoroValue::I64(i)) => *i,
            _ => default,
        }
    }
}

fn track_fields(t: &TrackState) -> Fields {
    vec![
        ("guid", LoroValue::from(t.guid.as_str())),
        ("name", LoroValue::from(t.name.as_str())),
        ("color", color(t.color)),
        ("volume", LoroValue::Double(t.volume)),
        ("pan", LoroValue::Double(t.pan)),
        ("muted", LoroValue::Bool(t.muted)),
        ("soloed", LoroValue::Bool(t.soloed)),
        ("phase_inverted", LoroValue::Bool(t.phase_inverted)),
        ("parent_send", LoroValue::Bool(t.parent_send)),
        ("visible_in_tcp", LoroValue::Bool(t.visible_in_tcp)),
        ("visible_in_mixer", LoroValue::Bool(t.visible_in_mixer)),
        ("lanes.count", LoroValue::I64(i64::from(t.lanes.count))),
        (
            "lanes.names",
            LoroValue::from(
                t.lanes
                    .names
                    .iter()
                    .map(|n| LoroValue::from(n.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
        // The bits as they are: lane 63 playing is a negative number.
        (
            "lanes.play",
            LoroValue::I64(i64::from_ne_bytes(t.lanes.play_mask.to_ne_bytes())),
        ),
        ("grouping", grouping_value(&t.grouping)),
        ("ext", string_map(&t.ext)),
        ("armed", LoroValue::Bool(t.armed)),
        (
            "automation_mode",
            LoroValue::from(t.automation_mode.as_str()),
        ),
        ("monitor", LoroValue::from(t.monitor.as_str())),
        ("record_input", LoroValue::from(t.record_input.as_str())),
        ("channels", LoroValue::I64(i64::from(t.channels))),
        ("height", color(t.height)),
        ("width", color(t.width)),
        ("lane_display", LoroValue::from(t.lane_display.as_str())),
        ("comp.record", color(t.comping.record_lane)),
        ("comp.lane", color(t.comping.comp_lane)),
        ("comp.last", color(t.comping.last_comp_lane)),
        (
            "comp.areas",
            number_list(t.comping.areas.iter().flat_map(|a| {
                [
                    a.start,
                    a.end,
                    f64::from(a.source_lane),
                    f64::from(a.comp_lane),
                    a.fade_in,
                    a.fade_out,
                ]
            })),
        ),
        ("ext_sections", sections_value(&t.ext_sections)),
    ]
}

/// A string map as a map value.
fn string_map(map: &BTreeMap<String, String>) -> LoroValue {
    LoroValue::from(
        map.iter()
            .map(|(k, v)| (k.clone(), LoroValue::from(v.as_str())))
            .collect::<HashMap<String, LoroValue>>(),
    )
}

fn read_string_map(v: Option<&LoroValue>) -> BTreeMap<String, String> {
    match v {
        Some(LoroValue::Map(map)) => map
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), as_string(v)?)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Ext-state by section, as nested map values.
fn sections_value(sections: &BTreeMap<String, BTreeMap<String, String>>) -> LoroValue {
    LoroValue::from(
        sections
            .iter()
            .map(|(section, keys)| (section.clone(), string_map(keys)))
            .collect::<HashMap<String, LoroValue>>(),
    )
}

fn read_sections(v: Option<&LoroValue>) -> BTreeMap<String, BTreeMap<String, String>> {
    match v {
        Some(LoroValue::Map(map)) => map
            .iter()
            .map(|(section, keys)| (section.clone(), read_string_map(Some(keys))))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// An optional `u32` read back (see [`color`]).
fn opt_u32(v: Option<&LoroValue>) -> Option<u32> {
    v.and_then(as_color)
}

/// A track's group masks, as `family → hex`.
fn grouping_value(grouping: &BTreeMap<String, u128>) -> LoroValue {
    let map: HashMap<String, LoroValue> = grouping
        .iter()
        .filter(|(_, mask)| **mask != 0)
        .map(|(family, mask)| (family.clone(), LoroValue::from(format!("{mask:x}"))))
        .collect();
    LoroValue::from(map)
}

fn read_grouping(v: Option<&LoroValue>) -> BTreeMap<String, u128> {
    match v {
        Some(LoroValue::Map(map)) => map
            .iter()
            .filter_map(|(family, hex)| {
                let mask = u128::from_str_radix(&as_string(hex)?, 16).ok()?;
                Some((family.clone(), mask))
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn strings(v: Option<&LoroValue>) -> Vec<String> {
    match v {
        Some(LoroValue::List(list)) => list.iter().filter_map(as_string).collect(),
        _ => Vec::new(),
    }
}

/// A flat list of numbers.
fn numbers(v: Option<&LoroValue>) -> Vec<f64> {
    match v {
        Some(LoroValue::List(list)) => list.iter().filter_map(as_f64).collect(),
        _ => Vec::new(),
    }
}

fn number_list(values: impl IntoIterator<Item = f64>) -> LoroValue {
    LoroValue::from(
        values
            .into_iter()
            .map(LoroValue::Double)
            .collect::<Vec<_>>(),
    )
}

fn read_track(fields: &HashMap<String, LoroValue>, folder: Option<&str>) -> Option<TrackState> {
    let g = Get(fields);
    let guid = g.opt_string("guid")?;
    Some(TrackState {
        guid,
        parent: folder.map(str::to_string),
        name: g.string("name"),
        color: g.color("color"),
        volume: g.f64("volume", 1.0),
        pan: g.f64("pan", 0.0),
        muted: g.bool("muted", false),
        soloed: g.bool("soloed", false),
        phase_inverted: g.bool("phase_inverted", false),
        parent_send: g.bool("parent_send", true),
        visible_in_tcp: g.bool("visible_in_tcp", true),
        visible_in_mixer: g.bool("visible_in_mixer", true),
        lanes: LaneState {
            count: u32::try_from(g.i64("lanes.count", 0)).unwrap_or(0),
            names: strings(fields.get("lanes.names")),
            play_mask: u64::from_ne_bytes(g.i64("lanes.play", 0).to_ne_bytes()),
        },
        grouping: read_grouping(fields.get("grouping")),
        ext: read_string_map(fields.get("ext")),
        fx: Vec::new(),
        sends: Vec::new(),
        envelopes: BTreeMap::new(),
        armed: g.bool("armed", false),
        automation_mode: g.string_or("automation_mode", "trim"),
        monitor: g.string_or("monitor", "off"),
        record_input: g.string_or("record_input", "none"),
        channels: u32::try_from(g.i64("channels", 2)).unwrap_or(2),
        height: opt_u32(fields.get("height")),
        width: opt_u32(fields.get("width")),
        lane_display: g.string_or("lane_display", "small"),
        comping: CompingState {
            record_lane: opt_u32(fields.get("comp.record")),
            comp_lane: opt_u32(fields.get("comp.lane")),
            last_comp_lane: opt_u32(fields.get("comp.last")),
            areas: numbers(fields.get("comp.areas"))
                .chunks_exact(6)
                .filter_map(|a| match *a {
                    [start, end, source, comp, fade_in, fade_out] => Some(CompAreaState {
                        start,
                        end,
                        source_lane: count(source),
                        comp_lane: count(comp),
                        fade_in,
                        fade_out,
                    }),
                    _ => None,
                })
                .collect(),
        },
        ext_sections: read_sections(fields.get("ext_sections")),
        input_fx: Vec::new(),
        hw_outputs: Vec::new(),
    })
}

/// A count or index stored as a number.
fn count(v: f64) -> u32 {
    num_traits::ToPrimitive::to_u32(&v.round().max(0.0)).unwrap_or(0)
}

fn item_fields(i: &ItemState) -> Fields {
    vec![
        ("track", LoroValue::from(i.track.as_str())),
        ("position", LoroValue::Double(i.position)),
        ("length", LoroValue::Double(i.length)),
        ("snap_offset", LoroValue::Double(i.snap_offset)),
        ("muted", LoroValue::Bool(i.muted)),
        ("locked", LoroValue::Bool(i.locked)),
        ("volume", LoroValue::Double(i.volume)),
        ("fade_in", LoroValue::Double(i.fade_in)),
        ("fade_out", LoroValue::Double(i.fade_out)),
        ("fade_in_shape", LoroValue::from(i.fade_in_shape.as_str())),
        ("fade_out_shape", LoroValue::from(i.fade_out_shape.as_str())),
        ("label", LoroValue::from(i.label.as_str())),
        ("color", color(i.color)),
        ("take.name", LoroValue::from(i.take.name.as_str())),
        ("take.source", opt_str(i.take.source.as_deref())),
        ("take.start_offset", LoroValue::Double(i.take.start_offset)),
        ("take.playrate", LoroValue::Double(i.take.playrate)),
        ("take.midi", LoroValue::Bool(i.take.midi)),
        (
            "fixed_lane",
            i.fixed_lane
                .map_or(LoroValue::Null, |l| LoroValue::I64(i64::from(l))),
        ),
        ("loop", LoroValue::Bool(i.loop_source)),
        (
            "take.stretch",
            number_list(
                i.take
                    .stretch
                    .iter()
                    .flat_map(|m| [m.at, m.source, m.slope]),
            ),
        ),
        (
            "take.volume",
            number_list(i.take.volume.iter().flat_map(|g| [g.at, g.value])),
        ),
        ("take.notes", notes_value(&i.take.notes)),
        ("take.more", take_more(&i.take)),
        (
            "beat_attach",
            LoroValue::from(i.timing.beat_attach.as_str()),
        ),
        ("auto_stretch", LoroValue::Bool(i.timing.auto_stretch)),
        ("group", color(i.timing.group)),
        ("start_offset", LoroValue::Double(i.timing.start_offset)),
        ("active_take", LoroValue::I64(i64::from(i.active_take))),
        (
            "takes",
            LoroValue::from(i.other_takes.iter().map(take_value).collect::<Vec<_>>()),
        ),
    ]
}

fn notes_value(notes: &[NoteState]) -> LoroValue {
    number_list(notes.iter().flat_map(|n| {
        [
            n.start,
            n.length,
            f64::from(n.pitch),
            f64::from(n.velocity),
            f64::from(n.channel),
        ]
    }))
}

fn read_notes(v: Option<&LoroValue>) -> Vec<NoteState> {
    numbers(v)
        .chunks_exact(5)
        .filter_map(|n| match *n {
            [start, length, pitch, velocity, channel] => Some(NoteState {
                start,
                length,
                pitch: byte(pitch),
                velocity: byte(velocity),
                channel: byte(channel),
                muted: false,
            }),
            _ => None,
        })
        .collect()
}

fn stretch_value(stretch: &[StretchPoint]) -> LoroValue {
    number_list(stretch.iter().flat_map(|m| [m.at, m.source, m.slope]))
}

fn read_stretch(v: Option<&LoroValue>) -> Vec<StretchPoint> {
    numbers(v)
        .chunks_exact(3)
        .filter_map(|m| match *m {
            [at, source, slope] => Some(StretchPoint { at, source, slope }),
            _ => None,
        })
        .collect()
}

fn gains_value(volume: &[GainPoint]) -> LoroValue {
    number_list(volume.iter().flat_map(|g| [g.at, g.value]))
}

fn read_gains(v: Option<&LoroValue>) -> Vec<GainPoint> {
    numbers(v)
        .chunks_exact(2)
        .filter_map(|g| match *g {
            [at, value] => Some(GainPoint { at, value }),
            _ => None,
        })
        .collect()
}

/// A take's fields beyond the flat `take.*` ones, as one map value.
fn take_more(t: &TakeState) -> LoroValue {
    let muted: Vec<f64> = t
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.muted)
        .filter_map(|(i, _)| u32::try_from(i).ok().map(f64::from))
        .collect();
    let mut map: HashMap<String, LoroValue> = HashMap::new();
    map.insert("color".into(), color(t.color));
    map.insert("gain".into(), LoroValue::Double(t.gain));
    map.insert("pitch".into(), LoroValue::Double(t.pitch));
    map.insert("preserve_pitch".into(), LoroValue::Bool(t.preserve_pitch));
    map.insert(
        "channel_mode".into(),
        LoroValue::I64(i64::from(t.channel_mode)),
    );
    map.insert(
        "source_type".into(),
        LoroValue::from(t.source_type.as_str()),
    );
    map.insert(
        "source_length".into(),
        t.source_length.map_or(LoroValue::Null, LoroValue::Double),
    );
    map.insert("source_rate".into(), color(t.source_rate));
    map.insert("source_channels".into(), color(t.source_channels));
    map.insert("note_count".into(), color(t.note_count));
    map.insert(
        "stretch_mode".into(),
        LoroValue::I64(i64::from(t.stretch_mode)),
    );
    map.insert("notes_muted".into(), number_list(muted));
    map.insert(
        "envelopes".into(),
        LoroValue::from(
            t.envelopes
                .iter()
                .map(|(kind, e)| (kind.clone(), envelope_value(e)))
                .collect::<HashMap<String, LoroValue>>(),
        ),
    );
    map.insert("midi".into(), midi_value(&t.midi_events));
    LoroValue::from(map)
}

/// [`take_more`]'s fields read back onto `take`.
fn read_take_more(v: Option<&LoroValue>, take: &mut TakeState) {
    // A doc from before schema 4: the take as the engine would make it.
    let Some(v) = v else {
        return;
    };
    let fields = map_value(v);
    let g = Get(&fields);
    take.color = g.color("color");
    take.gain = g.f64("gain", 1.0);
    take.pitch = g.f64("pitch", 0.0);
    take.preserve_pitch = g.bool("preserve_pitch", false);
    take.channel_mode = u32::try_from(g.i64("channel_mode", 0)).unwrap_or(0);
    take.source_type = g.string("source_type");
    take.source_length = fields.get("source_length").and_then(as_f64);
    take.source_rate = g.color("source_rate");
    take.source_channels = g.color("source_channels");
    take.note_count = g.color("note_count");
    take.stretch_mode = u8::try_from(g.i64("stretch_mode", 0)).unwrap_or(0);
    for i in numbers(fields.get("notes_muted")) {
        if let Some(note) = usize::try_from(count(i))
            .ok()
            .and_then(|i| take.notes.get_mut(i))
        {
            note.muted = true;
        }
    }
    take.envelopes = match fields.get("envelopes") {
        Some(LoroValue::Map(map)) => map
            .iter()
            .map(|(kind, e)| (kind.clone(), read_envelope_state(&map_value(e))))
            .collect(),
        _ => BTreeMap::new(),
    };
    take.midi_events = read_midi(fields.get("midi"));
}

/// A whole take, as one map value: an item's other takes.
fn take_value(t: &TakeState) -> LoroValue {
    let mut map = map_value(&take_more(t));
    map.insert("name".into(), LoroValue::from(t.name.as_str()));
    map.insert("source".into(), opt_str(t.source.as_deref()));
    map.insert("start_offset".into(), LoroValue::Double(t.start_offset));
    map.insert("playrate".into(), LoroValue::Double(t.playrate));
    map.insert("midi_take".into(), LoroValue::Bool(t.midi));
    map.insert("stretch".into(), stretch_value(&t.stretch));
    map.insert("volume".into(), gains_value(&t.volume));
    map.insert("notes".into(), notes_value(&t.notes));
    LoroValue::from(map)
}

fn read_take_value(v: &LoroValue) -> TakeState {
    let fields = map_value(v);
    let g = Get(&fields);
    let mut take = TakeState {
        name: g.string("name"),
        source: g.opt_string("source"),
        start_offset: g.f64("start_offset", 0.0),
        playrate: g.f64("playrate", 1.0),
        midi: g.bool("midi_take", false),
        stretch: read_stretch(fields.get("stretch")),
        volume: read_gains(fields.get("volume")),
        notes: read_notes(fields.get("notes")),
        ..TakeState::default()
    };
    read_take_more(Some(v), &mut take);
    take
}

/// A take's MIDI events other than notes: a flat number list per kind,
/// and each sysex message as `[at, bytes]`.
fn midi_value(m: &MidiEvents) -> LoroValue {
    let mut map: HashMap<String, LoroValue> = HashMap::new();
    map.insert(
        "cc".into(),
        number_list(m.ccs.iter().flat_map(|e| {
            [
                e.at,
                f64::from(e.channel),
                f64::from(e.controller),
                f64::from(e.value),
            ]
        })),
    );
    map.insert(
        "pb".into(),
        number_list(
            m.pitch_bends
                .iter()
                .flat_map(|e| [e.at, f64::from(e.channel), f64::from(e.value)]),
        ),
    );
    map.insert(
        "pc".into(),
        number_list(
            m.program_changes
                .iter()
                .flat_map(|e| [e.at, f64::from(e.channel), f64::from(e.program)]),
        ),
    );
    map.insert(
        "chp".into(),
        number_list(
            m.channel_pressures
                .iter()
                .flat_map(|e| [e.at, f64::from(e.channel), f64::from(e.pressure)]),
        ),
    );
    map.insert(
        "pp".into(),
        number_list(m.poly_pressures.iter().flat_map(|e| {
            [
                e.at,
                f64::from(e.channel),
                f64::from(e.note),
                f64::from(e.pressure),
            ]
        })),
    );
    map.insert(
        "nx".into(),
        number_list(m.note_expressions.iter().flat_map(|e| {
            [
                e.at,
                f64::from(e.channel),
                f64::from(e.note),
                f64::from(e.dimension),
                e.value,
            ]
        })),
    );
    map.insert(
        "sysex".into(),
        LoroValue::from(
            m.sysex
                .iter()
                .map(|e| {
                    LoroValue::from(vec![
                        LoroValue::Double(e.at),
                        LoroValue::from(e.data.clone()),
                    ])
                })
                .collect::<Vec<_>>(),
        ),
    );
    LoroValue::from(map)
}

fn read_midi(v: Option<&LoroValue>) -> MidiEvents {
    let fields = v.map(map_value).unwrap_or_default();
    let list = |key: &str, stride: usize| -> Vec<Vec<f64>> {
        numbers(fields.get(key))
            .chunks_exact(stride)
            .map(<[f64]>::to_vec)
            .collect()
    };
    MidiEvents {
        ccs: list("cc", 4)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, controller, value] => Some(CcState {
                    at,
                    channel: byte(channel),
                    controller: byte(controller),
                    value: byte(value),
                }),
                _ => None,
            })
            .collect(),
        pitch_bends: list("pb", 3)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, value] => Some(PitchBendState {
                    at,
                    channel: byte(channel),
                    value: num_traits::ToPrimitive::to_i16(&value.round()).unwrap_or(0),
                }),
                _ => None,
            })
            .collect(),
        program_changes: list("pc", 3)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, program] => Some(ProgramChangeState {
                    at,
                    channel: byte(channel),
                    program: byte(program),
                }),
                _ => None,
            })
            .collect(),
        channel_pressures: list("chp", 3)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, pressure] => Some(ChannelPressureState {
                    at,
                    channel: byte(channel),
                    pressure: byte(pressure),
                }),
                _ => None,
            })
            .collect(),
        poly_pressures: list("pp", 4)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, note, pressure] => Some(PolyPressureState {
                    at,
                    channel: byte(channel),
                    note: byte(note),
                    pressure: byte(pressure),
                }),
                _ => None,
            })
            .collect(),
        note_expressions: list("nx", 5)
            .iter()
            .filter_map(|e| match **e {
                [at, channel, note, dimension, value] => Some(NoteExpressionState {
                    at,
                    channel: byte(channel),
                    note: byte(note),
                    dimension: byte(dimension),
                    value,
                }),
                _ => None,
            })
            .collect(),
        sysex: match fields.get("sysex") {
            Some(LoroValue::List(list)) => list
                .iter()
                .filter_map(|e| match e {
                    LoroValue::List(pair) => match (pair.first(), pair.get(1)) {
                        (Some(at), Some(LoroValue::Binary(data))) => Some(SysexState {
                            at: as_f64(at)?,
                            data: data.to_vec(),
                        }),
                        _ => None,
                    },
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        },
    }
}

fn read_item(fields: &HashMap<String, LoroValue>) -> ItemState {
    let g = Get(fields);
    let mut item = ItemState {
        track: g.string("track"),
        position: g.f64("position", 0.0),
        length: g.f64("length", 0.0),
        snap_offset: g.f64("snap_offset", 0.0),
        muted: g.bool("muted", false),
        locked: g.bool("locked", false),
        volume: g.f64("volume", 1.0),
        fade_in: g.f64("fade_in", 0.0),
        fade_out: g.f64("fade_out", 0.0),
        fade_in_shape: g.string("fade_in_shape"),
        fade_out_shape: g.string("fade_out_shape"),
        label: g.string("label"),
        color: g.color("color"),
        take: TakeState {
            name: g.string("take.name"),
            source: g.opt_string("take.source"),
            start_offset: g.f64("take.start_offset", 0.0),
            playrate: g.f64("take.playrate", 1.0),
            midi: g.bool("take.midi", false),
            stretch: read_stretch(fields.get("take.stretch")),
            volume: read_gains(fields.get("take.volume")),
            notes: read_notes(fields.get("take.notes")),
            ..TakeState::default()
        },
        fixed_lane: match fields.get("fixed_lane") {
            Some(LoroValue::I64(l)) => u32::try_from(*l).ok(),
            _ => None,
        },
        loop_source: g.bool("loop", false),
        timing: crate::model::ItemTiming {
            beat_attach: g.string_or("beat_attach", "time"),
            auto_stretch: g.bool("auto_stretch", false),
            group: g.color("group"),
            start_offset: g.f64("start_offset", 0.0),
        },
        other_takes: match fields.get("takes") {
            Some(LoroValue::List(list)) => list.iter().map(read_take_value).collect(),
            _ => Vec::new(),
        },
        active_take: u32::try_from(g.i64("active_take", 0)).unwrap_or(0),
    };
    read_take_more(fields.get("take.more"), &mut item.take);
    item
}

/// A MIDI byte stored as a number.
fn byte(v: f64) -> u8 {
    num_traits::ToPrimitive::to_u8(&v.round().clamp(0.0, 127.0)).unwrap_or(0)
}

// ── a track's parts: its FX, sends, hardware outputs, envelopes ─────────

/// The chain an FX is on: a track's own, its input chain, the monitoring
/// chain.
const CHAIN_INPUT: &str = "input";
const CHAIN_MONITORING: &str = "monitoring";

/// One FX as the doc keeps it, with the track it is on, its chain and
/// its place.
#[derive(Debug, Clone, PartialEq)]
struct FxEntry {
    track: String,
    chain: String,
    index: usize,
    fx: FxState,
}

/// A send or a hardware output, with its track and place.
#[derive(Debug, Clone, PartialEq)]
struct SendEntry {
    track: String,
    index: usize,
    send: SendState,
}

#[derive(Debug, Clone, PartialEq)]
struct EnvelopeEntry {
    track: String,
    what: String,
    envelope: EnvelopeState,
}

/// Every FX, send, hardware output and envelope, each keyed as the doc
/// keeps it.
#[derive(Default)]
struct Parts {
    fx: BTreeMap<String, FxEntry>,
    sends: BTreeMap<String, SendEntry>,
    hw: BTreeMap<String, SendEntry>,
    envelopes: BTreeMap<String, EnvelopeEntry>,
}

impl Parts {
    fn add_chain(&mut self, track: &str, chain: &str, fx: &[FxState]) {
        for (index, f) in fx.iter().enumerate() {
            self.fx.insert(
                f.guid.clone(),
                FxEntry {
                    track: track.to_owned(),
                    chain: chain.to_owned(),
                    index,
                    fx: f.clone(),
                },
            );
        }
    }

    fn add_routes(into: &mut BTreeMap<String, SendEntry>, track: &str, routes: &[SendState]) {
        for (index, send) in routes.iter().enumerate() {
            into.insert(
                format!("{track}#{index}"),
                SendEntry {
                    track: track.to_owned(),
                    index,
                    send: send.clone(),
                },
            );
        }
    }

    fn add_envelopes(&mut self, track: &str, envelopes: &BTreeMap<String, EnvelopeState>) {
        for (what, envelope) in envelopes {
            self.envelopes.insert(
                format!("{track}|{what}"),
                EnvelopeEntry {
                    track: track.to_owned(),
                    what: what.clone(),
                    envelope: envelope.clone(),
                },
            );
        }
    }
}

/// Every track's parts, the master's and the monitoring chain's.
fn track_parts(tracks: &[TrackState], project: &ProjectSettings) -> Parts {
    let mut parts = Parts::default();
    for t in tracks {
        parts.add_chain(&t.guid, "", &t.fx);
        parts.add_chain(&t.guid, CHAIN_INPUT, &t.input_fx);
        Parts::add_routes(&mut parts.sends, &t.guid, &t.sends);
        Parts::add_routes(&mut parts.hw, &t.guid, &t.hw_outputs);
        parts.add_envelopes(&t.guid, &t.envelopes);
    }
    let master = &project.master;
    parts.add_chain(MASTER, "", &master.fx);
    Parts::add_routes(&mut parts.hw, MASTER, &master.hw_outputs);
    parts.add_envelopes(MASTER, &master.envelopes);
    parts.add_chain("", CHAIN_MONITORING, &project.monitoring_fx);
    parts
}

/// Where a part goes: a track (by its index), or the master.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Owner {
    Track(usize),
    Master,
}

/// Each part back onto its track (or the master, or the monitoring
/// chain), in order. A part of a track the doc no longer holds is left
/// out.
fn attach_parts(tracks: &mut [TrackState], project: &mut ProjectSettings, parts: &Parts) {
    let by_track: HashMap<String, usize> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.guid.clone(), i))
        .collect();
    let owner = |track: &str| -> Option<Owner> {
        if track == MASTER {
            Some(Owner::Master)
        } else {
            by_track.get(track).map(|&i| Owner::Track(i))
        }
    };
    let mut chains: HashMap<(Option<Owner>, String), Vec<FxEntry>> = HashMap::new();
    for (id, entry) in &parts.fx {
        let place = if entry.chain == CHAIN_MONITORING {
            None
        } else {
            match owner(&entry.track) {
                Some(o) => Some(o),
                None => continue,
            }
        };
        // The FX's id is the key it is kept under.
        let mut entry = entry.clone();
        entry.fx.guid.clone_from(id);
        chains
            .entry((place, entry.chain.clone()))
            .or_default()
            .push(entry);
    }
    for ((place, chain), mut entries) in chains {
        // By place, then id: two FX given one place at once (two people
        // adding at the same time) keep one order on every replica.
        entries.sort_by(|a, b| a.index.cmp(&b.index).then(a.fx.guid.cmp(&b.fx.guid)));
        let fx: Vec<FxState> = entries.into_iter().map(|e| e.fx).collect();
        match (place, chain.as_str()) {
            (None, _) => project.monitoring_fx = fx,
            (Some(Owner::Master), _) => project.master.fx = fx,
            (Some(Owner::Track(i)), CHAIN_INPUT) => {
                if let Some(t) = tracks.get_mut(i) {
                    t.input_fx = fx;
                }
            }
            (Some(Owner::Track(i)), _) => {
                if let Some(t) = tracks.get_mut(i) {
                    t.fx = fx;
                }
            }
        }
    }
    for (routes, hw) in [(&parts.sends, false), (&parts.hw, true)] {
        let mut by_owner: HashMap<Owner, Vec<&SendEntry>> = HashMap::new();
        for entry in routes.values() {
            if let Some(o) = owner(&entry.track) {
                by_owner.entry(o).or_default().push(entry);
            }
        }
        for (o, mut entries) in by_owner {
            entries.sort_by_key(|e| e.index);
            let list: Vec<SendState> = entries.into_iter().map(|e| e.send.clone()).collect();
            match (o, hw) {
                (Owner::Master, true) => project.master.hw_outputs = list,
                // The master sends nowhere but its outputs.
                (Owner::Master, false) => {}
                (Owner::Track(i), true) => {
                    if let Some(t) = tracks.get_mut(i) {
                        t.hw_outputs = list;
                    }
                }
                (Owner::Track(i), false) => {
                    if let Some(t) = tracks.get_mut(i) {
                        t.sends = list;
                    }
                }
            }
        }
    }
    for entry in parts.envelopes.values() {
        match owner(&entry.track) {
            Some(Owner::Master) => {
                project
                    .master
                    .envelopes
                    .insert(entry.what.clone(), entry.envelope.clone());
            }
            Some(Owner::Track(i)) => {
                if let Some(t) = tracks.get_mut(i) {
                    t.envelopes
                        .insert(entry.what.clone(), entry.envelope.clone());
                }
            }
            None => {}
        }
    }
}

fn fx_fields(e: &FxEntry) -> Fields {
    vec![
        ("track", LoroValue::from(e.track.as_str())),
        ("chain", LoroValue::from(e.chain.as_str())),
        ("index", LoroValue::I64(i64::try_from(e.index).unwrap_or(0))),
        ("plugin", LoroValue::from(e.fx.plugin.as_str())),
        ("name", LoroValue::from(e.fx.name.as_str())),
        ("enabled", LoroValue::Bool(e.fx.enabled)),
        ("offline", LoroValue::Bool(e.fx.offline)),
        ("state", opt_str(e.fx.state.as_deref())),
        (
            "params",
            number_list(
                e.fx.params
                    .iter()
                    .flat_map(|(id, value)| [f64::from(*id), *value]),
            ),
        ),
        ("param_count", LoroValue::I64(i64::from(e.fx.param_count))),
        ("preset", color(e.fx.preset)),
        ("preset_name", opt_str(e.fx.preset_name.as_deref())),
        ("config", string_map(&e.fx.config)),
    ]
}

fn read_fx(fields: &HashMap<String, LoroValue>) -> FxEntry {
    let g = Get(fields);
    FxEntry {
        track: g.string("track"),
        chain: g.string("chain"),
        index: usize::try_from(g.i64("index", 0)).unwrap_or(0),
        fx: FxState {
            guid: String::new(),
            plugin: g.string("plugin"),
            name: g.string("name"),
            enabled: g.bool("enabled", true),
            offline: g.bool("offline", false),
            state: g.opt_string("state"),
            params: numbers(fields.get("params"))
                .chunks_exact(2)
                .filter_map(|p| match *p {
                    [id, value] => Some((count(id), value)),
                    _ => None,
                })
                .collect(),
            param_count: u32::try_from(g.i64("param_count", 0)).unwrap_or(0),
            preset: g.color("preset"),
            preset_name: g.opt_string("preset_name"),
            config: read_string_map(fields.get("config")),
        },
    }
}

fn send_fields(e: &SendEntry) -> Fields {
    vec![
        ("track", LoroValue::from(e.track.as_str())),
        ("index", LoroValue::I64(i64::try_from(e.index).unwrap_or(0))),
        ("dest", LoroValue::from(e.send.dest.as_str())),
        ("mode", LoroValue::from(e.send.mode.as_str())),
        ("volume", LoroValue::Double(e.send.volume)),
        ("pan", LoroValue::Double(e.send.pan)),
        ("muted", LoroValue::Bool(e.send.muted)),
        ("from", LoroValue::I64(i64::from(e.send.from))),
        ("to", LoroValue::I64(i64::from(e.send.to))),
        ("mono", LoroValue::Bool(e.send.mono)),
        ("phase_inverted", LoroValue::Bool(e.send.phase_inverted)),
        (
            "automation_mode",
            LoroValue::from(e.send.automation_mode.as_str()),
        ),
        ("from_count", LoroValue::I64(i64::from(e.send.from_count))),
        ("to_count", LoroValue::I64(i64::from(e.send.to_count))),
        ("midi", opt_str(e.send.midi.as_deref())),
        ("label", opt_str(e.send.label.as_deref())),
    ]
}

fn read_send(fields: &HashMap<String, LoroValue>) -> SendEntry {
    let g = Get(fields);
    SendEntry {
        track: g.string("track"),
        index: usize::try_from(g.i64("index", 0)).unwrap_or(0),
        send: SendState {
            dest: g.string("dest"),
            mode: g.string("mode"),
            volume: g.f64("volume", 1.0),
            pan: g.f64("pan", 0.0),
            muted: g.bool("muted", false),
            from: u32::try_from(g.i64("from", 0)).unwrap_or(0),
            to: u32::try_from(g.i64("to", 0)).unwrap_or(0),
            mono: g.bool("mono", false),
            phase_inverted: g.bool("phase_inverted", false),
            automation_mode: g.string_or("automation_mode", "trim"),
            from_count: u32::try_from(g.i64("from_count", 2)).unwrap_or(2),
            to_count: u32::try_from(g.i64("to_count", 2)).unwrap_or(2),
            midi: g.opt_string("midi"),
            label: g.opt_string("label"),
        },
    }
}

fn points_value(points: &[EnvelopePointState]) -> LoroValue {
    number_list(
        points
            .iter()
            .flat_map(|p| [p.at, p.value, f64::from(p.shape), p.tension]),
    )
}

fn read_points(v: Option<&LoroValue>) -> Vec<EnvelopePointState> {
    numbers(v)
        .chunks_exact(4)
        .filter_map(|p| match *p {
            [at, value, shape, tension] => Some(EnvelopePointState {
                at,
                value,
                shape: num_traits::ToPrimitive::to_u8(&shape).unwrap_or(0),
                tension,
            }),
            _ => None,
        })
        .collect()
}

/// An envelope's own fields, whoever owns it.
fn envelope_parts(e: &EnvelopeState) -> Fields {
    vec![
        ("mode", LoroValue::from(e.mode.as_str())),
        ("visible", LoroValue::Bool(e.visible)),
        ("armed", LoroValue::Bool(e.armed)),
        ("points", points_value(&e.points)),
        ("own_lane", LoroValue::Bool(e.own_lane)),
        ("lane_height", LoroValue::I64(i64::from(e.lane_height))),
        (
            "items",
            LoroValue::from(
                e.items
                    .iter()
                    .map(automation_item_value)
                    .collect::<Vec<_>>(),
            ),
        ),
    ]
}

/// An envelope as one map value (a take's).
fn envelope_value(e: &EnvelopeState) -> LoroValue {
    LoroValue::from(
        envelope_parts(e)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<HashMap<String, LoroValue>>(),
    )
}

fn read_envelope_state(fields: &HashMap<String, LoroValue>) -> EnvelopeState {
    let g = Get(fields);
    EnvelopeState {
        mode: g.string("mode"),
        visible: g.bool("visible", false),
        armed: g.bool("armed", false),
        points: read_points(fields.get("points")),
        own_lane: g.bool("own_lane", false),
        lane_height: u32::try_from(g.i64("lane_height", 0)).unwrap_or(0),
        items: match fields.get("items") {
            Some(LoroValue::List(list)) => list
                .iter()
                .map(|v| read_automation_item(&map_value(v)))
                .collect(),
            _ => Vec::new(),
        },
    }
}

fn automation_item_value(a: &AutomationItemState) -> LoroValue {
    let mut map: HashMap<String, LoroValue> = HashMap::new();
    map.insert("pool".into(), LoroValue::I64(i64::from(a.pool)));
    map.insert("name".into(), LoroValue::from(a.name.as_str()));
    map.insert("position".into(), LoroValue::Double(a.position));
    map.insert("length".into(), LoroValue::Double(a.length));
    map.insert("start_offset".into(), LoroValue::Double(a.start_offset));
    map.insert("playrate".into(), LoroValue::Double(a.playrate));
    map.insert("baseline".into(), LoroValue::Double(a.baseline));
    map.insert("amplitude".into(), LoroValue::Double(a.amplitude));
    map.insert("looped".into(), LoroValue::Bool(a.looped));
    map.insert("points".into(), points_value(&a.points));
    LoroValue::from(map)
}

fn read_automation_item(fields: &HashMap<String, LoroValue>) -> AutomationItemState {
    let g = Get(fields);
    AutomationItemState {
        pool: i32::try_from(g.i64("pool", 0)).unwrap_or(0),
        name: g.string("name"),
        position: g.f64("position", 0.0),
        length: g.f64("length", 0.0),
        start_offset: g.f64("start_offset", 0.0),
        playrate: g.f64("playrate", 1.0),
        baseline: g.f64("baseline", 0.0),
        amplitude: g.f64("amplitude", 1.0),
        looped: g.bool("looped", false),
        points: read_points(fields.get("points")),
    }
}

fn envelope_fields(e: &EnvelopeEntry) -> Fields {
    let mut fields = vec![
        ("track", LoroValue::from(e.track.as_str())),
        ("what", LoroValue::from(e.what.as_str())),
    ];
    fields.extend(envelope_parts(&e.envelope));
    fields
}

fn read_envelope(fields: &HashMap<String, LoroValue>) -> EnvelopeEntry {
    let g = Get(fields);
    EnvelopeEntry {
        track: g.string("track"),
        what: g.string("what"),
        envelope: read_envelope_state(fields),
    }
}

// ── the project's own settings ──────────────────────────────────────────

fn project_fields(p: &ProjectSettings) -> Fields {
    vec![
        ("master.volume", LoroValue::Double(p.master.volume)),
        ("master.pan", LoroValue::Double(p.master.pan)),
        ("master.muted", LoroValue::Bool(p.master.muted)),
        ("ext", sections_value(&p.ext)),
        (
            "ruler_lanes",
            LoroValue::from(
                p.ruler_lanes
                    .iter()
                    .map(|(index, lane)| {
                        let mut map: HashMap<String, LoroValue> = HashMap::new();
                        map.insert("name".into(), LoroValue::from(lane.name.as_str()));
                        map.insert("flags".into(), LoroValue::I64(i64::from(lane.flags)));
                        (index.to_string(), LoroValue::from(map))
                    })
                    .collect::<HashMap<String, LoroValue>>(),
            ),
        ),
        (
            "automation_override",
            opt_str(p.automation_override.as_deref()),
        ),
        (
            "bay",
            p.bay
                .as_ref()
                .map_or(LoroValue::Null, |b| LoroValue::from(b.clone())),
        ),
        ("looping", LoroValue::Bool(p.looping)),
        ("metronome", LoroValue::Bool(p.metronome)),
        ("playrate", LoroValue::Double(p.playrate)),
        ("record_mode", LoroValue::from(p.record_mode.as_str())),
    ]
}

/// The project's settings; the master's and the monitoring chain's parts
/// are attached after ([`attach_parts`]).
fn read_project(fields: &HashMap<String, LoroValue>) -> ProjectSettings {
    let g = Get(fields);
    let defaults = ProjectSettings::default();
    ProjectSettings {
        master: crate::model::MasterState {
            volume: g.f64("master.volume", 1.0),
            pan: g.f64("master.pan", 0.0),
            muted: g.bool("master.muted", false),
            ..crate::model::MasterState::default()
        },
        ext: read_sections(fields.get("ext")),
        ruler_lanes: match fields.get("ruler_lanes") {
            Some(LoroValue::Map(map)) => map
                .iter()
                .filter_map(|(index, lane)| {
                    let lane = map_value(lane);
                    let g = Get(&lane);
                    Some((
                        index.parse().ok()?,
                        RulerLaneState {
                            name: g.string("name"),
                            flags: u32::try_from(g.i64("flags", 0)).unwrap_or(0),
                        },
                    ))
                })
                .collect(),
            _ => BTreeMap::new(),
        },
        automation_override: g.opt_string("automation_override"),
        monitoring_fx: Vec::new(),
        bay: match fields.get("bay") {
            Some(LoroValue::Binary(bytes)) => Some(bytes.to_vec()),
            _ => None,
        },
        looping: g.bool("looping", defaults.looping),
        metronome: g.bool("metronome", defaults.metronome),
        playrate: g.f64("playrate", defaults.playrate),
        record_mode: g.string_or("record_mode", &defaults.record_mode),
    }
}

fn marker_fields(m: &MarkerState) -> Fields {
    vec![
        ("at", LoroValue::Double(m.at)),
        ("name", LoroValue::from(m.name.as_str())),
        ("color", color(m.color)),
        ("lane", color(m.lane)),
    ]
}

fn read_marker(fields: &HashMap<String, LoroValue>) -> MarkerState {
    let g = Get(fields);
    MarkerState {
        at: g.f64("at", 0.0),
        name: g.string("name"),
        color: g.color("color"),
        lane: g.color("lane"),
    }
}

fn take_marker_fields(m: &TakeMarkerState) -> Fields {
    let mut fields = vec![
        ("source", LoroValue::from(m.source.as_str())),
        ("id", LoroValue::from(m.id.as_str())),
        ("at", LoroValue::Double(m.at)),
        ("name", LoroValue::from(m.name.as_str())),
        ("color", color(m.color)),
        ("hit", LoroValue::Bool(m.hit.is_some())),
    ];
    // A plain marker writes its hit fields as their defaults, so a hit
    // turned back into a plain marker leaves nothing of itself behind.
    let h = m.hit.clone().unwrap_or_default();
    fields.extend([
        ("hit.piece", LoroValue::from(h.piece.as_str())),
        ("hit.origin", LoroValue::from(h.origin.as_str())),
        ("hit.run", LoroValue::from(h.run.as_str())),
        ("hit.peak_db", LoroValue::Double(h.peak_db)),
        ("hit.confidence", LoroValue::Double(h.confidence)),
        ("hit.from_trigger", LoroValue::Bool(h.heard.trigger)),
        ("hit.from_mic", LoroValue::Bool(h.heard.mic)),
        ("hit.ghost", LoroValue::Bool(h.ghost)),
        ("hit.bleed", LoroValue::Bool(h.bleed)),
        ("hit.flam_with", opt_str(h.flam_with.as_deref())),
        ("hit.note", LoroValue::from(h.note.as_str())),
    ]);
    fields
}

fn read_take_marker(fields: &HashMap<String, LoroValue>) -> TakeMarkerState {
    let g = Get(fields);
    TakeMarkerState {
        source: g.string("source"),
        id: g.string("id"),
        at: g.f64("at", 0.0),
        name: g.string("name"),
        color: g.color("color"),
        hit: g.bool("hit", false).then(|| HitState {
            piece: g.string("hit.piece"),
            origin: g.string("hit.origin"),
            run: g.string("hit.run"),
            peak_db: g.f64("hit.peak_db", 0.0),
            confidence: g.f64("hit.confidence", 0.0),
            heard: Sources {
                trigger: g.bool("hit.from_trigger", false),
                mic: g.bool("hit.from_mic", false),
            },
            ghost: g.bool("hit.ghost", false),
            bleed: g.bool("hit.bleed", false),
            flam_with: g.opt_string("hit.flam_with"),
            note: g.string("hit.note"),
        }),
    }
}

fn region_fields(r: &RegionState) -> Fields {
    vec![
        ("start", LoroValue::Double(r.start)),
        ("end", LoroValue::Double(r.end)),
        ("name", LoroValue::from(r.name.as_str())),
        ("color", color(r.color)),
        ("lane", color(r.lane)),
    ]
}

fn read_region(fields: &HashMap<String, LoroValue>) -> RegionState {
    let g = Get(fields);
    RegionState {
        start: g.f64("start", 0.0),
        end: g.f64("end", 0.0),
        name: g.string("name"),
        color: g.color("color"),
        lane: g.color("lane"),
    }
}

/// `at:bpm:beats/unit` per point, space-separated. `f64`'s `Display` is
/// the shortest string that reads back to the same number, so a tempo
/// map survives the round trip exactly.
fn encode_tempo(points: &[TempoPoint]) -> String {
    points
        .iter()
        .map(|p| format!("{}:{}:{}/{}", p.at, p.bpm, p.beats_per_bar, p.beat_unit))
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_tempo(s: &str) -> Vec<TempoPoint> {
    s.split_whitespace()
        .filter_map(|point| {
            let mut parts = point.split(':');
            let at = parts.next()?.parse().ok()?;
            let bpm = parts.next()?.parse().ok()?;
            let (beats, unit) = parts.next()?.split_once('/')?;
            Some(TempoPoint {
                at,
                bpm,
                beats_per_bar: beats.parse().ok()?,
                beat_unit: unit.parse().ok()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod touched_tests {
    use super::*;
    use crate::model::ItemState;

    fn with_item(at: f64) -> SessionModel {
        let mut model = SessionModel::default();
        model.items.insert(
            "hit".into(),
            ItemState {
                position: at,
                length: 1.0,
                ..ItemState::default()
            },
        );
        model
    }

    /// The engine's own writes are not noted; what it has yet to match —
    /// an undo of one of them — is, down to the item.
    #[test]
    fn an_undo_is_touched_and_the_engines_own_write_is_not() {
        let doc = SessionDoc::new();
        doc.write(&with_item(1.0), ORIGIN_LOCAL).expect("written");
        let watch = doc.watch();
        let mut undo = loro::UndoManager::new(doc.loro());
        let take = || std::mem::take(&mut *watch.touched.lock().expect("touched"));
        take();
        doc.write_step(&with_item(1.0), &with_item(3.0), ORIGIN_LOCAL, "Move")
            .expect("a step");
        assert!(take().is_empty(), "the engine's own step");
        assert!(undo.undo().expect("undone"));
        let undone = take();
        assert!(undone.items.contains("hit"), "{undone:?}");
        assert!((doc.read().items["hit"].position - 1.0).abs() < 1e-9);
    }
}
