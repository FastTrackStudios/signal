//! The engine side: read a [`SessionModel`] back from a daw project, and
//! apply [`Change`]s to one.
//!
//! Talks to whichever backend `daw_control` is connected to —
//! daw-standalone in-process for the Session app, REAPER through the
//! bridge. Nothing here knows about Loro.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use daw_control::Project;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{FadeShape, TrackRef};

use crate::diff::{Change, ItemField, TrackField};
use crate::model::{
    CompAreaState, CompingState, EnvelopePointState, EnvelopeState, FxState, GainPoint, HitState,
    ItemState, ItemTiming, LaneState, MarkerState, NoteState, ProjectSettings, RegionState,
    SendState, SessionModel, Sources, StretchPoint, TakeMarkerState, TakeState, TempoPoint,
    TrackState,
};

/// Where an FX's state is kept: put its bytes, get its name back (a
/// SHA-256), and the bytes by name. The project's store on disk; a map in
/// memory for a session nobody saves.
pub trait Blobs: Send + Sync {
    fn put(&self, bytes: &[u8]) -> String;
    fn get(&self, name: &str) -> Option<Vec<u8>>;
}

/// [`Blobs`] in memory.
#[derive(Default)]
pub struct MemoryBlobs(std::sync::Mutex<HashMap<String, Vec<u8>>>);

impl Blobs for MemoryBlobs {
    fn put(&self, bytes: &[u8]) -> String {
        let name = blob_name(bytes);
        if let Ok(mut held) = self.0.lock() {
            held.entry(name.clone()).or_insert_with(|| bytes.to_vec());
        }
        name
    }
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(name).cloned()
    }
}

/// A blob's name: its SHA-256, lowercase hex — the store's and the
/// `.session` objects' naming.
#[must_use]
pub fn blob_name(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for b in sha2::Sha256::digest(bytes) {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Which engine FX and item each doc FX and item is.
///
/// An FX cannot be added with a chosen guid (REAPER's cannot, nor this
/// engine's), nor a MIDI item, so a project built from the doc has those
/// under new guids. The doc keeps the ids it first saw; this maps them to
/// whatever the engine calls the same thing now, both ways, for the life
/// of a bridge.
#[derive(Debug, Default, Clone)]
pub struct EngineIds {
    pub fx: IdMap,
    pub items: IdMap,
}

/// One kind's doc ids and engine guids, both ways.
#[derive(Debug, Default, Clone)]
pub struct IdMap {
    doc_to_engine: HashMap<String, String>,
    engine_to_doc: HashMap<String, String>,
}

impl IdMap {
    /// The doc's id for the engine's `guid` (its own, if never mapped).
    #[must_use]
    pub fn doc(&self, guid: &str) -> String {
        self.engine_to_doc
            .get(guid)
            .cloned()
            .unwrap_or_else(|| guid.to_owned())
    }

    /// The engine's guid for the doc's `id` (the id, if never mapped).
    #[must_use]
    pub fn engine(&self, id: &str) -> String {
        self.doc_to_engine
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_owned())
    }

    /// The doc's `id` is the engine's `guid`.
    pub fn bind(&mut self, id: &str, guid: &str) {
        self.doc_to_engine.insert(id.to_owned(), guid.to_owned());
        self.engine_to_doc.insert(guid.to_owned(), id.to_owned());
    }
}

/// Where the session's media lives on this machine.
///
/// The doc stores a take's source relative to the session folder, because
/// each peer keeps the session somewhere else; this turns one into the
/// other.
#[derive(Debug, Clone)]
pub struct MediaRoot(pub PathBuf);

impl MediaRoot {
    /// A path as the doc stores it: relative when it is under the root.
    #[must_use]
    pub fn to_doc(&self, path: &str) -> String {
        Path::new(path).strip_prefix(&self.0).map_or_else(
            |_| path.to_string(),
            |rel| rel.to_string_lossy().into_owned(),
        )
    }

    /// A doc path as this machine opens it.
    #[must_use]
    pub fn to_local(&self, path: &str) -> String {
        let p = Path::new(path);
        if p.is_absolute() {
            path.to_string()
        } else {
            self.0.join(p).to_string_lossy().into_owned()
        }
    }
}

/// Everything the engine holds that the doc shares. `chart` is left
/// empty: the chart is not engine state.
///
/// What the facade cannot show (a track's channel count, an FX's
/// parameters, every take, MIDI beyond notes, the project's own
/// settings) is read from daw-standalone's own state when the engine is
/// one in this process ([`Direct::standalone`], [`crate::whole`]).
///
/// # Errors
/// When a daw call fails.
pub async fn read(
    project: &Project,
    media: &MediaRoot,
    blobs: &dyn Blobs,
    ids: &EngineIds,
    direct: Option<&dyn Direct>,
) -> daw_control::Result<SessionModel> {
    read_keeping(project, media, blobs, ids, direct, None).await
}

/// As [`read`], with `tracks` kept as they are when given.
///
/// A step that touched no track (an item moved) leaves them as they were,
/// and reading a song's tracks, FX state and all, is most of a read.
///
/// # Errors
/// When a daw call fails.
pub async fn read_keeping(
    project: &Project,
    media: &MediaRoot,
    blobs: &dyn Blobs,
    ids: &EngineIds,
    direct: Option<&dyn Direct>,
    tracks: Option<Vec<TrackState>>,
) -> daw_control::Result<SessionModel> {
    let fresh = tracks.is_none();
    let tracks = match tracks {
        Some(tracks) => tracks,
        None => read_tracks(project, blobs, ids).await?,
    };
    let items = read_items(project, media, ids, direct).await?;
    let standalone = direct.and_then(Direct::standalone);
    // daw-standalone's own state holds every recording's markers, which
    // [`crate::whole`] reads at once.
    let take_markers = if standalone.is_some() {
        BTreeMap::new()
    } else {
        read_take_markers(project, &items, ids).await?
    };

    let markers = project
        .markers()
        .all()
        .await?
        .into_iter()
        .filter_map(|m| {
            Some((
                m.guid?,
                MarkerState {
                    at: m.position.time.map_or(0.0, |t| t.as_seconds()),
                    name: m.name,
                    color: m.color,
                    lane: m.lane,
                },
            ))
        })
        .collect();
    let regions = project
        .regions()
        .all()
        .await?
        .into_iter()
        .filter_map(|r| {
            Some((
                r.guid?,
                RegionState {
                    start: r.time_range.start_seconds(),
                    end: r.time_range.end_seconds(),
                    name: r.name,
                    color: r.color,
                    lane: r.lane,
                },
            ))
        })
        .collect();
    let mut tempo: Vec<TempoPoint> = project
        .tempo_map()
        .points()
        .await?
        .into_iter()
        .map(|p| {
            let (beats_per_bar, beat_unit) = p
                .time_signature
                .map_or((4, 4), |s| (s.numerator, s.denominator));
            TempoPoint {
                at: p.position.time.map_or(0.0, |t| t.as_seconds()),
                bpm: p.bpm,
                beats_per_bar,
                beat_unit,
            }
        })
        .collect();
    // No points: the project's own tempo and metre, as the one point a
    // song with a steady tempo has.
    if tempo.is_empty() {
        let map = project.tempo_map();
        let (beats, unit) = map.default_time_signature().await?;
        tempo.push(TempoPoint {
            at: 0.0,
            bpm: map.default_tempo().await?,
            beats_per_bar: u32::try_from(beats).unwrap_or(4),
            beat_unit: u32::try_from(unit).unwrap_or(4),
        });
    }

    let mut model = SessionModel {
        tracks,
        items,
        markers,
        regions,
        tempo,
        chart: String::new(),
        take_markers,
        project: ProjectSettings::default(),
    };
    // The master's and the monitoring chain's FX, as a track's are read.
    model.project.monitoring_fx = read_chain(&project.monitoring_fx(), blobs, ids).await?;
    model.project.master.fx =
        read_chain(&project.tracks().master().await?.fx_chain(), blobs, ids).await?;
    if let Some(sa) = standalone {
        crate::whole::read_into(sa, project.guid(), &mut model, media, ids, fresh);
    }
    Ok(model)
}

/// Every recording's markers, keyed as the doc keeps them: read once a
/// recording, through any item cut from it.
async fn read_take_markers(
    project: &Project,
    items: &BTreeMap<String, ItemState>,
    ids: &EngineIds,
) -> daw_control::Result<BTreeMap<String, TakeMarkerState>> {
    let mut sources: BTreeMap<&str, &str> = BTreeMap::new();
    for (guid, item) in items {
        if let Some(source) = item.take.source.as_deref() {
            sources.entry(source).or_insert(guid.as_str());
        }
    }
    let mut out = BTreeMap::new();
    for (source, guid) in sources {
        let Some(handle) = project.items().by_guid(&ids.items.engine(guid)).await? else {
            continue;
        };
        for m in handle.active_take().markers().await.unwrap_or_default() {
            let state = take_marker_state(source, m);
            out.insert(state.key(), state);
        }
    }
    Ok(out)
}

/// A take marker as the doc keeps it, on the recording `source` names.
pub(crate) fn take_marker_state(source: &str, m: daw_proto::TakeMarker) -> TakeMarkerState {
    TakeMarkerState {
        source: source.to_owned(),
        // A backend that keeps no ids (REAPER) is keyed by place.
        id: if m.id.is_empty() {
            format!("@{}", m.index)
        } else {
            m.id
        },
        at: m.source_position_seconds,
        name: m.name,
        color: m.color,
        hit: m.hit.map(|h| HitState {
            piece: h.piece,
            origin: match h.origin {
                daw_proto::MarkOrigin::Auto => "auto",
                daw_proto::MarkOrigin::Manual => "manual",
                daw_proto::MarkOrigin::Approved => "approved",
            }
            .to_owned(),
            run: h.run,
            peak_db: h.peak_db,
            confidence: h.confidence,
            heard: Sources {
                trigger: h.from_trigger,
                mic: h.from_mic,
            },
            ghost: h.ghost,
            bleed: h.bleed,
            flam_with: h.flam_with,
            note: h.note,
        }),
    }
}

/// A doc take marker as the engine takes it.
pub(crate) fn proto_take_marker(m: &TakeMarkerState) -> daw_proto::TakeMarker {
    daw_proto::TakeMarker {
        index: 0,
        id: m.id.clone(),
        name: m.name.clone(),
        source_position_seconds: m.at,
        color: m.color,
        hit: m.hit.as_ref().map(|h| daw_proto::HitMark {
            piece: h.piece.clone(),
            origin: match h.origin.as_str() {
                "manual" => daw_proto::MarkOrigin::Manual,
                "approved" => daw_proto::MarkOrigin::Approved,
                _ => daw_proto::MarkOrigin::Auto,
            },
            run: h.run.clone(),
            peak_db: h.peak_db,
            confidence: h.confidence,
            from_trigger: h.heard.trigger,
            from_mic: h.heard.mic,
            ghost: h.ghost,
            bleed: h.bleed,
            flam_with: h.flam_with.clone(),
            note: h.note.clone(),
        }),
    }
}

/// Set a recording's markers through an item cut from it. Nothing when
/// no item plays it any more.
async fn set_take_markers(
    project: &Project,
    target: &SessionModel,
    ids: &EngineIds,
    (source, markers): (&str, &[TakeMarkerState]),
) -> daw_control::Result<()> {
    let Some(guid) = target
        .items
        .iter()
        .find(|(_, i)| i.take.source.as_deref() == Some(source))
        .map(|(g, _)| g)
    else {
        return Ok(());
    };
    if let Some(handle) = project.items().by_guid(&ids.items.engine(guid)).await? {
        handle
            .active_take()
            .set_markers(markers.iter().map(proto_take_marker).collect())
            .await?;
    }
    Ok(())
}

/// Every track as the doc keeps it, its FX, sends and envelopes with it.
async fn read_tracks(
    project: &Project,
    blobs: &dyn Blobs,
    ids: &EngineIds,
) -> daw_control::Result<Vec<TrackState>> {
    let all = project.tracks().all().await?;
    let parents: HashMap<String, Option<String>> = all
        .iter()
        .map(|t| (t.guid.clone(), t.parent_guid.clone()))
        .collect();
    let mut tracks = Vec::with_capacity(all.len());
    for t in all {
        let mut state = TrackState {
            parent: parents.get(&t.guid).cloned().flatten(),
            name: t.name,
            color: t.color,
            volume: t.volume,
            pan: t.pan,
            muted: t.muted,
            soloed: t.soloed,
            phase_inverted: t.phase_inverted,
            parent_send: t.parent_send,
            visible_in_tcp: t.visible_in_tcp,
            visible_in_mixer: t.visible_in_mixer,
            lanes: LaneState {
                count: t.lane_count,
                names: t.lane_names,
                play_mask: t.lane_play_mask,
            },
            grouping: grouping_map(&t.grouping),
            armed: t.armed,
            automation_mode: mode_name(t.automation_mode).to_owned(),
            monitor: monitor_name(t.input_monitor).to_owned(),
            record_input: record_input_name(t.record_input),
            // The facade has no reader for it; daw-standalone's own state
            // does ([`crate::whole`]).
            channels: 2,
            height: t.height,
            width: t.width,
            lane_display: lane_display_name(t.lane_display).to_owned(),
            guid: t.guid,
            ..TrackState::default()
        };
        if let Some(handle) = project.tracks().by_guid(&state.guid).await? {
            state.ext = handle
                .ext_state()
                .await?
                .into_iter()
                .map(|e| (e.key, e.value))
                .collect();
            // Only a track with lanes has comping to read: on REAPER the
            // read is a state chunk's.
            if t.lane_count > 0 {
                state.comping = handle
                    .comping()
                    .await
                    .ok()
                    .as_ref()
                    .map_or_else(CompingState::default, comping_state);
            }
            read_track_parts(&handle, &mut state, blobs, ids).await?;
        }
        tracks.push(state);
    }
    Ok(tracks)
}

/// Every item, keyed by its doc id, with its active take — through the
/// engine in this process when there is one ([`Direct`]), else over the
/// facade, a few calls an item.
async fn read_items(
    project: &Project,
    media: &MediaRoot,
    ids: &EngineIds,
    direct: Option<&dyn Direct>,
) -> daw_control::Result<BTreeMap<String, ItemState>> {
    if let Some(direct) = direct {
        return Ok(crate::direct::read_items(
            direct,
            &context_of(project),
            media,
            ids,
        ));
    }
    let mut items = BTreeMap::new();
    for item in project.items().all().await? {
        let Some(handle) = project.items().by_guid(&item.guid).await? else {
            continue;
        };
        let active = handle.active_take();
        let take = match active.info().await {
            Ok(take) => {
                let stretch = active.stretch_markers().await.unwrap_or_default();
                let notes = if take.is_midi {
                    active.midi().notes().await.unwrap_or_default()
                } else {
                    Vec::new()
                };
                let volume = match active.envelopes().await {
                    Ok(envelopes) => envelopes.volume().points().await.unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                Some((take, stretch, notes, volume))
            }
            // An item with no take (a label-only KEY item, say).
            Err(_) => None,
        };
        items.insert(ids.items.doc(&item.guid), item_state(item, take, media));
    }
    Ok(items)
}

/// A take as the engine reports it: itself, its stretch markers, its
/// notes (a MIDI take's), its volume envelope.
pub type TakeRead = (
    daw_proto::item::Take,
    Vec<daw_proto::StretchMarker>,
    Vec<daw_proto::MidiNote>,
    Vec<daw_proto::automation::EnvelopePoint>,
);

/// How far past an item a take's volume curve is kept, seconds.
const CURVE_PAD: f64 = 1.0;

/// `points` (by time) cut to those over an item `length` long and
/// [`CURVE_PAD`] either side, with the nearest beyond each end kept so
/// the curve is what it was at the edges: a drum slice keeps its own
/// stretch of a whole take's leveling, not all of it.
pub(crate) fn over_item(points: &[GainPoint], length: f64) -> Vec<GainPoint> {
    let (from, to) = (-CURVE_PAD, length + CURVE_PAD);
    let first = points
        .iter()
        .position(|p| p.at >= from)
        .unwrap_or(points.len());
    let last = points
        .iter()
        .rposition(|p| p.at <= to)
        .map_or(0, |i| i.saturating_add(1));
    let (keep_from, keep_to) = (
        first.saturating_sub(1),
        last.saturating_add(1).min(points.len()),
    );
    points
        .get(keep_from..keep_to.max(keep_from))
        .unwrap_or_default()
        .to_vec()
}

/// An item, with its active take, as the doc keeps it.
#[must_use]
pub fn item_state(item: daw_proto::Item, take: Option<TakeRead>, media: &MediaRoot) -> ItemState {
    let take = take.map_or_else(
        || TakeState {
            playrate: 1.0,
            ..TakeState::default()
        },
        |(take, stretch, notes, volume)| TakeState {
            stretch: stretch
                .into_iter()
                .map(|m| StretchPoint {
                    at: m.position,
                    source: m.source_position,
                    slope: m.slope,
                })
                .collect(),
            volume: over_item(
                &volume
                    .into_iter()
                    .map(|p| GainPoint {
                        at: p.time.as_seconds(),
                        value: p.value,
                    })
                    .collect::<Vec<_>>(),
                item.length.as_seconds(),
            ),
            notes: notes.iter().map(note_state).collect(),
            midi: take.is_midi,
            color: take.color,
            gain: take.volume,
            pitch: take.pitch,
            preserve_pitch: take.preserve_pitch,
            channel_mode: take.channel_mode,
            source_type: source_type_name(take.source_type).to_owned(),
            source_length: take.source_length.map(|l| l.as_seconds()),
            source_rate: take.source_sample_rate,
            source_channels: take.source_channels,
            note_count: take.midi_note_count,
            name: take.name,
            source: take.source_file_path.map(|p| media.to_doc(&p)),
            start_offset: take.start_offset.as_seconds(),
            playrate: take.play_rate,
            ..TakeState::default()
        },
    );
    ItemState {
        track: item.track_guid,
        position: item.position.as_seconds(),
        length: item.length.as_seconds(),
        snap_offset: item.snap_offset.as_seconds(),
        muted: item.muted,
        locked: item.locked,
        volume: item.volume,
        fade_in: item.fade_in_length.as_seconds(),
        fade_out: item.fade_out_length.as_seconds(),
        fade_in_shape: fade_name(item.fade_in_shape).to_string(),
        fade_out_shape: fade_name(item.fade_out_shape).to_string(),
        label: item.label.unwrap_or_default(),
        color: item.color,
        take,
        fixed_lane: item.fixed_lane,
        loop_source: item.loop_source,
        timing: ItemTiming {
            beat_attach: beat_attach_name(item.beat_attach_mode).to_owned(),
            auto_stretch: item.auto_stretch,
            group: item.group_id,
            start_offset: item.start_offset.as_seconds(),
        },
        // The others are daw-standalone's to tell ([`crate::whole`]).
        other_takes: Vec::new(),
        active_take: 0,
    }
}

/// A MIDI note as the doc keeps it.
pub(crate) const fn note_state(n: &daw_proto::MidiNote) -> NoteState {
    NoteState {
        start: n.start_ppq,
        length: n.length_ppq,
        pitch: n.pitch,
        velocity: n.velocity,
        channel: n.channel,
        muted: n.muted,
    }
}

/// The project a facade handle is on, as the engine's services name it.
fn context_of(project: &Project) -> daw_proto::ProjectContext {
    daw_proto::ProjectContext::Project(project.guid().to_owned())
}

/// The engine's item services, called in this process.
///
/// What the doc's bridge reads and rebuilds a song's items through when
/// the engine is its own (daw-standalone in the Session app), with none of
/// the facade's per-call encoding — a kit of thirty thousand slices is
/// that many items.
pub trait Direct:
    daw_proto::Items
    + daw_proto::Takes
    + daw_proto::Midi
    + daw_proto::StretchMarkers
    + daw_proto::automation::Automation
    + Send
    + Sync
{
    /// The engine itself, when it is daw-standalone: what the rest of a
    /// project — everything the facade does not show — is read from and
    /// written to ([`crate::whole`]).
    fn standalone(&self) -> Option<&daw_standalone::sync::Standalone>;
}

impl<T> Direct for T
where
    T: daw_proto::Items
        + daw_proto::Takes
        + daw_proto::Midi
        + daw_proto::StretchMarkers
        + daw_proto::automation::Automation
        + Send
        + Sync
        + 'static,
{
    fn standalone(&self) -> Option<&daw_standalone::sync::Standalone> {
        let any: &dyn std::any::Any = self;
        any.downcast_ref()
    }
}

/// Apply `changes` (from [`crate::diff`]) to the engine. `target` is the
/// model the changes lead to — structural steps are applied against its
/// whole track order.
///
/// Chart steps are skipped: the chart is not engine state.
///
/// # Errors
/// On the first daw call that fails; the caller re-reads the engine and
/// reconciles from wherever it got to.
pub async fn apply(
    project: &Project,
    media: &MediaRoot,
    changes: &[Change],
    target: &SessionModel,
    blobs: &dyn Blobs,
    ids: &mut EngineIds,
    direct: Option<&dyn Direct>,
) -> daw_control::Result<()> {
    let ctx = context_of(project);
    let mut structure = false;
    // A track's FX, sends and envelopes wait until every track is there
    // and in place: a send names its destination, an FX's automation its
    // FX.
    let mut parts: Vec<(&TrackState, Vec<TrackField>)> = Vec::new();
    for change in changes {
        match change {
            Change::TrackAdded { track, .. } => {
                structure = true;
                let index = target.tracks.iter().position(|t| t.guid == track.guid);
                let at = index.and_then(|i| u32::try_from(i).ok());
                project
                    .tracks()
                    .add_with_guid(&track.guid, &track.name, at)
                    .await?;
                set_track_fields(project, track, ALL_TRACK_FIELDS).await?;
                parts.push((track, ALL_TRACK_FIELDS.to_vec()));
            }
            Change::TrackMoved { .. } => structure = true,
            Change::TrackChanged { track, fields } => {
                set_track_fields(project, track, fields).await?;
                parts.push((track, fields.clone()));
            }
            Change::TrackRemoved { guid } => {
                project
                    .tracks()
                    .remove(TrackRef::Guid(guid.clone()))
                    .await?;
            }
            Change::ItemAdded { guid, item } => match direct {
                Some(d) => {
                    crate::direct::add_item(d, &ctx, media, guid, item, ids)
                        .map_err(|e| engine_error(&e))?;
                    let at = ids.items.engine(guid);
                    crate::direct::set_item_fields(
                        d,
                        &ctx,
                        media,
                        &at,
                        item,
                        &new_item_fields(item),
                    )
                    .map_err(|e| engine_error(&e))?;
                }
                None => add_item(project, media, guid, item, ids).await?,
            },
            Change::ItemChanged { guid, item, fields } => {
                let at = ids.items.engine(guid);
                match direct {
                    Some(d) => crate::direct::set_item_fields(d, &ctx, media, &at, item, fields)
                        .map_err(|e| engine_error(&e))?,
                    None => set_item_fields(project, media, &at, item, fields).await?,
                }
            }
            Change::ItemRemoved { guid } => {
                let at = ids.items.engine(guid);
                match direct {
                    Some(d) => {
                        crate::direct::delete_item(d, &ctx, &at).map_err(|e| engine_error(&e))?;
                    }
                    None => {
                        if let Some(handle) = project.items().by_guid(&at).await? {
                            handle.delete().await?;
                        }
                    }
                }
            }
            Change::MarkerSet { .. }
            | Change::MarkerRemoved { .. }
            | Change::RegionSet { .. }
            | Change::RegionRemoved { .. } => apply_mark(project, change).await?,
            Change::TempoChanged(points) => apply_tempo(project, points).await?,
            Change::ChartChanged(_) => {}
            Change::TakeMarkersSet { source, markers } => {
                set_take_markers(project, target, ids, (source, markers)).await?;
            }
            Change::ProjectChanged(settings) => {
                apply_project(project, settings, blobs, ids).await?;
            }
        }
    }
    if structure {
        apply_structure(project, &target.tracks).await?;
    }
    apply_parts(project, &parts, blobs, ids).await?;
    // What the facade cannot set, set in daw-standalone's own state —
    // last, over what the calls above made, so the engine holds exactly
    // the doc's.
    if let Some(d) = direct
        && let Some(sa) = d.standalone()
    {
        crate::whole::apply(sa, d, project.guid(), changes, target, ids, media)
            .map_err(|e| engine_error(&e))?;
    }
    Ok(())
}

/// A track's FX chains, sends, hardware outputs and envelopes, once
/// every track is in place.
async fn apply_parts(
    project: &Project,
    parts: &[(&TrackState, Vec<TrackField>)],
    blobs: &dyn Blobs,
    ids: &mut EngineIds,
) -> daw_control::Result<()> {
    for (track, fields) in parts {
        if fields.contains(&TrackField::Fx) {
            apply_fx(project, track, false, blobs, ids).await?;
        }
        if fields.contains(&TrackField::InputFx) {
            apply_fx(project, track, true, blobs, ids).await?;
        }
    }
    for (track, fields) in parts {
        if fields.contains(&TrackField::Sends) {
            apply_sends(project, track).await?;
        }
        if fields.contains(&TrackField::HwOutputs) {
            apply_hw_outputs(project, track).await?;
        }
        if fields.contains(&TrackField::Envelopes) {
            apply_envelopes(project, track, ids).await?;
        }
    }
    Ok(())
}

/// The project's settings through the facade: the master's and the
/// monitoring chain's FX, and the transport's switches. The rest (the
/// master's level, ext-state, ruler lanes) has no facade setter and is
/// daw-standalone's ([`crate::whole`]).
async fn apply_project(
    project: &Project,
    settings: &ProjectSettings,
    blobs: &dyn Blobs,
    ids: &mut EngineIds,
) -> daw_control::Result<()> {
    apply_chain(
        &project.monitoring_fx(),
        &settings.monitoring_fx,
        blobs,
        ids,
    )
    .await?;
    let master = project.tracks().master().await?;
    apply_chain(&master.fx_chain(), &settings.master.fx, blobs, ids).await?;
    let transport = project.transport();
    transport.set_loop(settings.looping).await?;
    transport.set_metronome(settings.metronome).await?;
    transport.set_playrate(settings.playrate).await?;
    Ok(())
}

/// Make the engine's hardware outputs from `track` the doc's: every one
/// removed and the doc's added in order.
async fn apply_hw_outputs(project: &Project, track: &TrackState) -> daw_control::Result<()> {
    let Some(handle) = project.tracks().by_guid(&track.guid).await? else {
        return Ok(());
    };
    let outputs = handle.hardware_outputs();
    for _ in outputs.all().await? {
        if let Some(first) = outputs.by_index(0).await? {
            first.remove().await?;
        }
    }
    for out in &track.hw_outputs {
        let route = outputs.add(out.dest.parse().unwrap_or(0)).await?;
        set_route(&route, out, false).await?;
    }
    Ok(())
}

/// A new route's settings: level, pan, mute, channels — and, for a send
/// (`is_send`), its mode: a hardware output has none.
async fn set_route(
    route: &daw_control::RouteHandle,
    send: &SendState,
    is_send: bool,
) -> daw_control::Result<()> {
    if is_send {
        route.set_send_mode(send_mode_of(&send.mode)).await?;
    }
    route.set_volume(send.volume).await?;
    route.set_pan(send.pan).await?;
    if send.muted {
        route.mute().await?;
    }
    route.set_mono(send.mono).await?;
    route.set_phase_inverted(send.phase_inverted).await?;
    if send.from != 0 || send.from_count != 0 {
        route
            .set_source_channels(send.from, send.from_count.max(1))
            .await?;
    }
    if send.to != 0 || send.to_count != 0 {
        route
            .set_dest_channels(send.to, send.to_count.max(1))
            .await?;
    }
    Ok(())
}

/// A new item, its take made the way it plays: a MIDI item (under a
/// guid of the engine's own, bound in `ids`) or an audio take on its
/// source; then every field set.
async fn add_item(
    project: &Project,
    media: &MediaRoot,
    guid: &str,
    item: &ItemState,
    ids: &mut EngineIds,
) -> daw_control::Result<()> {
    if item.take.midi {
        // A MIDI item is made with its MIDI take, under a
        // guid of the engine's own.
        let Some(track) = project.tracks().by_guid(&item.track).await? else {
            return Ok(());
        };
        if let Some(handle) = track
            .items()
            .create_midi_item_with_notes(item.position, item.position + item.length, Vec::new())
            .await?
        {
            let made = handle.info().await?.guid;
            ids.items.bind(guid, &made);
        }
    } else {
        let handle = project
            .items()
            .add_with_guid(
                &item.track,
                guid,
                PositionInSeconds::from_seconds(item.position),
                Duration::from_seconds(item.length),
            )
            .await?;
        // A new item has no take; the one it plays is added
        // here (take guids are each engine's own — the doc
        // keeps the active take's fields on the item).
        let take = handle.takes().add().await?;
        if let Some(source) = &item.take.source {
            take.set_source_file(&media.to_local(source)).await?;
        }
    }
    let at = ids.items.engine(guid);
    set_item_fields(project, media, &at, item, &new_item_fields(item)).await?;
    Ok(())
}

/// The fields a new item needs set: what it does not already have.
fn new_item_fields(item: &ItemState) -> Vec<ItemField> {
    // Only what a new item does not already have: a field call each is
    // most of what rebuilding a song costs.
    let fresh = ItemState {
        track: item.track.clone(),
        position: item.position,
        length: item.length,
        volume: 1.0,
        // What a new item and its take start as: linear fades, no name,
        // its source already set (an audio take's, above).
        fade_in_shape: "linear".to_owned(),
        fade_out_shape: "linear".to_owned(),
        take: TakeState {
            source: item.take.source.clone(),
            playrate: 1.0,
            midi: item.take.midi,
            ..TakeState::default()
        },
        ..ItemState::default()
    };
    let mut fields = crate::diff::item_fields(&fresh, item);
    if item.take.midi {
        // Made from its start and end: its length set exactly.
        fields.push(ItemField::Length);
    }
    fields
}

/// Put every track at its place in project order, and set the folder
/// depths that make the tree: a track's depth is how far the NEXT track's
/// nesting level differs from its own (REAPER's convention — 1 opens a
/// folder, -n closes n of them).
async fn apply_structure(project: &Project, tracks: &[TrackState]) -> daw_control::Result<()> {
    for (index, track) in tracks.iter().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            break;
        };
        project
            .tracks()
            .move_to(TrackRef::Guid(track.guid.clone()), index)
            .await?;
    }
    for (track, depth) in tracks.iter().zip(folder_depths(tracks)) {
        if let Some(handle) = project.tracks().by_guid(&track.guid).await? {
            handle.set_folder_depth(depth).await?;
        }
    }
    Ok(())
}

/// REAPER folder depths for tracks in project order, from their parents.
#[must_use]
pub fn folder_depths(tracks: &[TrackState]) -> Vec<i32> {
    let mut levels: HashMap<&str, i32> = HashMap::new();
    let level: Vec<i32> = tracks
        .iter()
        .map(|t| {
            let l = t
                .parent
                .as_deref()
                .and_then(|p| levels.get(p))
                .map_or(0, |p| p.saturating_add(1));
            levels.insert(t.guid.as_str(), l);
            l
        })
        .collect();
    level
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let next = level.get(i.saturating_add(1)).copied().unwrap_or(0);
            next.saturating_sub(*l)
        })
        .collect()
}

const ALL_TRACK_FIELDS: &[TrackField] = &[
    TrackField::Name,
    TrackField::Color,
    TrackField::Volume,
    TrackField::Pan,
    TrackField::Muted,
    TrackField::Soloed,
    TrackField::PhaseInverted,
    TrackField::ParentSend,
    TrackField::VisibleInTcp,
    TrackField::Lanes,
    TrackField::Grouping,
    TrackField::Ext,
    TrackField::Fx,
    TrackField::Sends,
    TrackField::Envelopes,
    TrackField::Armed,
    TrackField::AutomationMode,
    TrackField::Monitor,
    TrackField::RecordInput,
    TrackField::Channels,
    TrackField::Height,
    TrackField::Width,
    TrackField::LaneDisplay,
    TrackField::Comping,
    TrackField::ExtSections,
    TrackField::InputFx,
    TrackField::HwOutputs,
];

async fn set_track_fields(
    project: &Project,
    track: &TrackState,
    fields: &[TrackField],
) -> daw_control::Result<()> {
    let Some(h) = project.tracks().by_guid(&track.guid).await? else {
        return Ok(());
    };
    for field in fields {
        match field {
            TrackField::Name => h.rename(&track.name).await?,
            TrackField::Color => h.set_color(track.color.unwrap_or(0)).await?,
            TrackField::Volume => h.set_volume(track.volume).await?,
            TrackField::Pan => h.set_pan(track.pan).await?,
            TrackField::Muted if track.muted => h.mute().await?,
            TrackField::Muted => h.unmute().await?,
            TrackField::Soloed if track.soloed => h.solo().await?,
            TrackField::Soloed => h.unsolo().await?,
            TrackField::PhaseInverted => h.set_phase_inverted(track.phase_inverted).await?,
            TrackField::ParentSend => h.set_parent_send(track.parent_send).await?,
            TrackField::VisibleInTcp | TrackField::VisibleInMixer => {
                h.set_visibility(track.visible_in_tcp, track.visible_in_mixer)
                    .await?;
            }
            TrackField::Lanes => {
                h.set_lane_count(track.lanes.count).await?;
                for (lane, name) in track.lanes.names.iter().enumerate() {
                    if let Ok(lane) = u32::try_from(lane) {
                        h.set_lane_name(lane, name).await?;
                    }
                }
                h.set_lane_play_mask(track.lanes.play_mask).await?;
            }
            TrackField::Grouping => apply_grouping(&h, track).await?,
            TrackField::Ext => {
                let now: BTreeMap<String, String> = h
                    .ext_state()
                    .await?
                    .into_iter()
                    .map(|e| (e.key, e.value))
                    .collect();
                for key in now.keys().filter(|k| !track.ext.contains_key(*k)) {
                    h.set_ext_state(key, "").await?;
                }
                for (key, value) in &track.ext {
                    if now.get(key) != Some(value) {
                        h.set_ext_state(key, value).await?;
                    }
                }
            }
            TrackField::Armed if track.armed => h.arm().await?,
            TrackField::Armed => h.disarm().await?,
            TrackField::AutomationMode => {
                h.set_automation_mode(mode_of(&track.automation_mode))
                    .await?;
            }
            TrackField::Monitor => h.set_input_monitor(monitor_of(&track.monitor)).await?,
            TrackField::RecordInput => {
                h.set_record_input(record_input_of(&track.record_input))
                    .await?;
            }
            TrackField::Height => h.set_tcp_height(track.height.unwrap_or(0)).await?,
            TrackField::Comping => {
                h.set_comp_areas(comp_areas(&track.comping)).await?;
                // The lane before it first, so it becomes the last one.
                if track.comping.last_comp_lane.is_some() {
                    h.set_active_comp(track.comping.last_comp_lane).await?;
                }
                h.set_active_comp(track.comping.comp_lane).await?;
            }
            // After the structure: see `apply`. Channels, width, lane
            // display and named sections have no facade setter
            // ([`crate::whole`]).
            TrackField::Fx
            | TrackField::Sends
            | TrackField::Envelopes
            | TrackField::InputFx
            | TrackField::HwOutputs
            | TrackField::Channels
            | TrackField::Width
            | TrackField::LaneDisplay
            | TrackField::ExtSections => {}
        }
    }
    Ok(())
}

async fn set_item_fields(
    project: &Project,
    media: &MediaRoot,
    guid: &str,
    item: &ItemState,
    fields: &[ItemField],
) -> daw_control::Result<()> {
    let Some(h) = project.items().by_guid(guid).await? else {
        return Ok(());
    };
    for field in fields {
        match field {
            ItemField::Track => h.move_to_track(TrackRef::Guid(item.track.clone())).await?,
            ItemField::Position => {
                h.set_position(PositionInSeconds::from_seconds(item.position))
                    .await?;
            }
            ItemField::Length => h.set_length(Duration::from_seconds(item.length)).await?,
            // No setter on the handle for these; daw-standalone's own
            // state takes them ([`crate::whole`]).
            ItemField::SnapOffset
            | ItemField::Timing
            | ItemField::TakeSettings
            | ItemField::TakeEnvelopes
            | ItemField::Midi
            | ItemField::Takes => {}
            ItemField::Muted if item.muted => h.mute().await?,
            ItemField::Muted => h.unmute().await?,
            ItemField::Locked if item.locked => h.lock().await?,
            ItemField::Locked => h.unlock().await?,
            ItemField::Volume => h.set_volume(item.volume).await?,
            ItemField::FadeIn | ItemField::FadeInShape => {
                h.set_fade_in(
                    Duration::from_seconds(item.fade_in),
                    fade_shape(&item.fade_in_shape),
                )
                .await?;
            }
            ItemField::FadeOut | ItemField::FadeOutShape => {
                h.set_fade_out(
                    Duration::from_seconds(item.fade_out),
                    fade_shape(&item.fade_out_shape),
                )
                .await?;
            }
            ItemField::Label => h.set_label(&item.label).await?,
            ItemField::Color => h.set_color(item.color).await?,
            ItemField::Take => {
                let take = h.active_take();
                take.set_name(&item.take.name).await?;
                if let Some(source) = &item.take.source {
                    take.set_source_file(&media.to_local(source)).await?;
                }
                take.set_start_offset(Duration::from_seconds(item.take.start_offset))
                    .await?;
                take.set_play_rate(item.take.playrate).await?;
            }
            ItemField::FixedLane => {
                if let Some(lane) = item.fixed_lane {
                    h.set_fixed_lane(lane).await?;
                }
            }
            ItemField::Loop => h.set_loop_source(item.loop_source).await?,
            ItemField::Stretch => {
                h.active_take()
                    .set_stretch_markers(
                        item.take
                            .stretch
                            .iter()
                            .map(|m| daw_proto::StretchMarker {
                                position: m.at,
                                source_position: m.source,
                                slope: m.slope,
                            })
                            .collect(),
                    )
                    .await?;
            }
            ItemField::TakeVolume => {
                let volume = h.active_take().envelopes().await?.volume();
                volume.clear().await?;
                for g in &item.take.volume {
                    volume
                        .add_point_linear(PositionInSeconds::from_seconds(g.at), g.value)
                        .await?;
                }
            }
            ItemField::Notes => {
                let notes = item
                    .take
                    .notes
                    .iter()
                    .map(|n| daw_proto::MidiNoteCreate {
                        channel: n.channel,
                        pitch: n.pitch,
                        velocity: n.velocity,
                        start_ppq: n.start,
                        length_ppq: n.length,
                    })
                    .collect();
                // Cleared, then the notes in the take's own ticks — the
                // units `notes()` reads them in.
                let midi = h.active_take().midi();
                midi.write_take(
                    daw_proto::midi::MidiTakeContent::default(),
                    daw_proto::midi::WriteMode::Replace,
                )
                .await?;
                midi.add_notes_ppq(notes).await?;
            }
        }
    }
    Ok(())
}

async fn marker_id(project: &Project, guid: &str) -> daw_control::Result<Option<u32>> {
    Ok(project
        .markers()
        .all()
        .await?
        .into_iter()
        .find(|m| m.guid.as_deref() == Some(guid))
        .and_then(|m| m.id))
}

async fn region_id(project: &Project, guid: &str) -> daw_control::Result<Option<u32>> {
    Ok(project
        .regions()
        .all()
        .await?
        .into_iter()
        .find(|r| r.guid.as_deref() == Some(guid))
        .and_then(|r| r.id))
}

/// Make the engine's tempo map the doc's: its points replaced, the first
/// one's tempo and metre the project's own too. A doc with no points
/// leaves the engine's alone.
async fn apply_tempo(project: &Project, points: &[TempoPoint]) -> daw_control::Result<()> {
    // No points is no tempo known (a doc written before it kept one), not
    // a song without one: the engine's is left as it is.
    if points.is_empty() {
        return Ok(());
    }
    let map = project.tempo_map();
    let count = u32::try_from(map.count().await?).unwrap_or(u32::MAX);
    for index in (0..count).rev() {
        map.remove_point(index).await?;
    }
    let metre = |p: &TempoPoint| {
        (
            i32::try_from(p.beats_per_bar).unwrap_or(4),
            i32::try_from(p.beat_unit).unwrap_or(4),
        )
    };
    if let Some(first) = points.first() {
        let (beats, unit) = metre(first);
        map.set_default_tempo(first.bpm).await?;
        map.set_default_time_signature(beats, unit).await?;
    }
    for point in points {
        let index = map.add_point(point.at, point.bpm).await?;
        let (beats, unit) = metre(point);
        map.set_time_signature_at(index, beats, unit).await?;
    }
    Ok(())
}

/// A marker or region set or removed; anything else is left alone.
async fn apply_mark(project: &Project, change: &Change) -> daw_control::Result<()> {
    match change {
        Change::MarkerSet { guid, marker } => set_marker(project, guid, marker).await?,
        Change::MarkerRemoved { guid } => {
            if let Some(id) = marker_id(project, guid).await? {
                project.markers().remove(id).await?;
            }
        }
        Change::RegionSet { guid, region } => set_region(project, guid, region).await?,
        Change::RegionRemoved { guid } => {
            if let Some(id) = region_id(project, guid).await? {
                project.regions().remove(id).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn set_marker(project: &Project, guid: &str, m: &MarkerState) -> daw_control::Result<()> {
    let markers = project.markers();
    let id = match marker_id(project, guid).await? {
        Some(id) => {
            markers.move_to(id, m.at).await?;
            markers.rename(id, &m.name).await?;
            id
        }
        None => markers.add_with_guid(guid, m.at, &m.name).await?,
    };
    if let Some(c) = m.color {
        markers.set_color(id, c).await?;
    }
    markers.set_lane(id, m.lane).await?;
    Ok(())
}

async fn set_region(project: &Project, guid: &str, r: &RegionState) -> daw_control::Result<()> {
    let regions = project.regions();
    let id = match region_id(project, guid).await? {
        Some(id) => {
            regions.set_bounds(id, r.start, r.end).await?;
            regions.rename(id, &r.name).await?;
            id
        }
        None => regions.add_with_guid(guid, r.start, r.end, &r.name).await?,
    };
    if let Some(c) = r.color {
        regions.set_color(id, c).await?;
    }
    regions.set_lane(id, r.lane).await?;
    Ok(())
}

const fn fade_name(shape: FadeShape) -> &'static str {
    match shape {
        FadeShape::Linear => "linear",
        FadeShape::FastStart => "fast_start",
        FadeShape::FastEnd => "fast_end",
        FadeShape::FastStartSteep => "fast_start_steep",
        FadeShape::FastEndSteep => "fast_end_steep",
        FadeShape::SlowStartEnd => "slow_start_end",
        FadeShape::SlowStartEndSteep => "slow_start_end_steep",
    }
}

pub(crate) fn fade_shape(name: &str) -> FadeShape {
    match name {
        "fast_start" => FadeShape::FastStart,
        "fast_end" => FadeShape::FastEnd,
        "fast_start_steep" => FadeShape::FastStartSteep,
        "fast_end_steep" => FadeShape::FastEndSteep,
        "slow_start_end" => FadeShape::SlowStartEnd,
        "slow_start_end_steep" => FadeShape::SlowStartEndSteep,
        _ => FadeShape::Linear,
    }
}

// ── a track's parts ─────────────────────────────────────────────────────

/// A track's group masks by family, the non-zero ones.
pub(crate) fn grouping_map(g: &daw_proto::track::TrackGrouping) -> BTreeMap<String, u128> {
    [
        ("volume_lead", g.volume_lead),
        ("volume_follow", g.volume_follow),
        ("pan_lead", g.pan_lead),
        ("pan_follow", g.pan_follow),
        ("mute_lead", g.mute_lead),
        ("mute_follow", g.mute_follow),
        ("solo_lead", g.solo_lead),
        ("solo_follow", g.solo_follow),
        ("recarm_lead", g.recarm_lead),
        ("recarm_follow", g.recarm_follow),
        ("polarity_lead", g.polarity_lead),
        ("polarity_follow", g.polarity_follow),
        ("automode_lead", g.automode_lead),
        ("automode_follow", g.automode_follow),
        ("width_lead", g.width_lead),
        ("width_follow", g.width_follow),
        ("vca_lead", g.vca_lead),
        ("vca_follow", g.vca_follow),
        ("media_edit_lead", g.media_edit_lead),
        ("media_edit_follow", g.media_edit_follow),
        ("volume_reverse", g.volume_reverse),
        ("pan_reverse", g.pan_reverse),
        ("width_reverse", g.width_reverse),
        ("no_lead_when_follow", g.no_lead_when_follow),
        ("vca_prefx_follow", g.vca_prefx_follow),
    ]
    .into_iter()
    .filter(|(_, mask)| *mask != 0)
    .map(|(family, mask)| (family.to_owned(), mask))
    .collect()
}

/// A track's group masks from the doc's map: [`grouping_map`] undone.
pub(crate) fn grouping_of(map: &BTreeMap<String, u128>) -> daw_proto::track::TrackGrouping {
    let m = |family: &str| map.get(family).copied().unwrap_or(0);
    daw_proto::track::TrackGrouping {
        volume_lead: m("volume_lead"),
        volume_follow: m("volume_follow"),
        pan_lead: m("pan_lead"),
        pan_follow: m("pan_follow"),
        mute_lead: m("mute_lead"),
        mute_follow: m("mute_follow"),
        solo_lead: m("solo_lead"),
        solo_follow: m("solo_follow"),
        recarm_lead: m("recarm_lead"),
        recarm_follow: m("recarm_follow"),
        polarity_lead: m("polarity_lead"),
        polarity_follow: m("polarity_follow"),
        automode_lead: m("automode_lead"),
        automode_follow: m("automode_follow"),
        volume_reverse: m("volume_reverse"),
        pan_reverse: m("pan_reverse"),
        no_lead_when_follow: m("no_lead_when_follow"),
        width_reverse: m("width_reverse"),
        width_lead: m("width_lead"),
        width_follow: m("width_follow"),
        vca_lead: m("vca_lead"),
        vca_follow: m("vca_follow"),
        vca_prefx_follow: m("vca_prefx_follow"),
        media_edit_lead: m("media_edit_lead"),
        media_edit_follow: m("media_edit_follow"),
    }
}

// ── names the doc keeps the engine's enums by ──────────────────────────

pub(crate) const fn monitor_name(m: daw_proto::InputMonitoringMode) -> &'static str {
    use daw_proto::InputMonitoringMode as M;
    match m {
        M::Off => "off",
        M::Normal => "normal",
        M::NotWhenPlaying => "not_when_playing",
    }
}

pub(crate) fn monitor_of(name: &str) -> daw_proto::InputMonitoringMode {
    use daw_proto::InputMonitoringMode as M;
    match name {
        "normal" => M::Normal,
        "not_when_playing" => M::NotWhenPlaying,
        _ => M::Off,
    }
}

pub(crate) const fn lane_display_name(d: daw_proto::LaneDisplay) -> &'static str {
    use daw_proto::LaneDisplay as D;
    match d {
        D::Small => "small",
        D::Big => "big",
        D::One => "one",
    }
}

pub(crate) fn lane_display_of(name: &str) -> daw_proto::LaneDisplay {
    use daw_proto::LaneDisplay as D;
    match name {
        "big" => D::Big,
        "one" => D::One,
        _ => D::Small,
    }
}

/// `none`, `audio:<channel>`, `midi:<device>:<channel>` (`*` for all),
/// `raw:<n>`.
pub(crate) fn record_input_name(input: daw_proto::RecordInput) -> String {
    use daw_proto::RecordInput as R;
    let any = |v: Option<u8>| v.map_or_else(|| "*".to_owned(), |v| v.to_string());
    match input {
        R::None => "none".to_owned(),
        R::Audio { channel } => format!("audio:{channel}"),
        R::Midi { device_id, channel } => format!("midi:{}:{}", any(device_id), any(channel)),
        R::Raw(n) => format!("raw:{n}"),
    }
}

pub(crate) fn record_input_of(name: &str) -> daw_proto::RecordInput {
    use daw_proto::RecordInput as R;
    let any = |v: &str| v.parse::<u8>().ok();
    let mut parts = name.split(':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("audio"), Some(channel), None) => R::Audio {
            channel: channel.parse().unwrap_or(0),
        },
        (Some("midi"), Some(device), Some(channel)) => R::Midi {
            device_id: any(device),
            channel: any(channel),
        },
        (Some("raw"), Some(n), None) => n.parse().map_or(R::None, R::Raw),
        _ => R::None,
    }
}

pub(crate) const fn beat_attach_name(mode: daw_proto::BeatAttachMode) -> &'static str {
    use daw_proto::BeatAttachMode as B;
    match mode {
        B::Time => "time",
        B::Beats => "beats",
        B::BeatsPositionOnly => "beats_position",
    }
}

pub(crate) fn beat_attach_of(name: &str) -> daw_proto::BeatAttachMode {
    use daw_proto::BeatAttachMode as B;
    match name {
        "beats" => B::Beats,
        "beats_position" => B::BeatsPositionOnly,
        _ => B::Time,
    }
}

pub(crate) const fn source_type_name(kind: daw_proto::SourceType) -> &'static str {
    use daw_proto::SourceType as S;
    match kind {
        S::Audio => "audio",
        S::Midi => "midi",
        S::Empty => "empty",
        S::Video => "video",
        S::Unknown => "unknown",
    }
}

pub(crate) fn source_type_of(name: &str) -> daw_proto::SourceType {
    use daw_proto::SourceType as S;
    match name {
        "midi" => S::Midi,
        "empty" => S::Empty,
        "video" => S::Video,
        "unknown" => S::Unknown,
        _ => S::Audio,
    }
}

/// `<from>><to>`: a channel number, or `*` (every channel; as it was).
pub(crate) fn midi_mapping_name(m: &daw_proto::MidiChannelMapping) -> String {
    let from = match m.source {
        daw_proto::MidiSourceChannel::All => "*".to_owned(),
        daw_proto::MidiSourceChannel::Channel(c) => c.to_string(),
    };
    let to = match m.destination {
        daw_proto::MidiDestinationChannel::Original => "*".to_owned(),
        daw_proto::MidiDestinationChannel::Channel(c) => c.to_string(),
    };
    format!("{from}>{to}")
}

pub(crate) fn midi_mapping_of(name: &str) -> daw_proto::MidiChannelMapping {
    let (from, to) = name.split_once('>').unwrap_or(("*", "*"));
    daw_proto::MidiChannelMapping {
        source: from.parse().map_or(
            daw_proto::MidiSourceChannel::All,
            daw_proto::MidiSourceChannel::Channel,
        ),
        destination: to.parse().map_or(
            daw_proto::MidiDestinationChannel::Original,
            daw_proto::MidiDestinationChannel::Channel,
        ),
    }
}

/// Each group family, by the name its masks are kept under.
const FAMILIES: [(&str, daw_proto::track::GroupFamily); 10] = [
    ("volume", daw_proto::track::GroupFamily::Volume),
    ("vca", daw_proto::track::GroupFamily::Vca),
    ("pan", daw_proto::track::GroupFamily::Pan),
    ("width", daw_proto::track::GroupFamily::Width),
    ("mute", daw_proto::track::GroupFamily::Mute),
    ("solo", daw_proto::track::GroupFamily::Solo),
    ("recarm", daw_proto::track::GroupFamily::RecArm),
    ("polarity", daw_proto::track::GroupFamily::Polarity),
    ("automode", daw_proto::track::GroupFamily::AutoMode),
    ("media_edit", daw_proto::track::GroupFamily::MediaEdit),
];

/// An envelope's key on its track, or `None` for one the doc does not
/// keep (a take's, play rate, a named one).
fn envelope_key(e: &daw_proto::automation::Envelope, ids: &EngineIds) -> Option<String> {
    use daw_proto::automation::EnvelopeType as T;
    Some(match e.envelope_type {
        T::Volume => "volume".to_owned(),
        T::VolumePrefx => "volume_prefx".to_owned(),
        T::Pan => "pan".to_owned(),
        T::PanPrefx => "pan_prefx".to_owned(),
        T::Width => "width".to_owned(),
        T::WidthPrefx => "width_prefx".to_owned(),
        T::Mute => "mute".to_owned(),
        T::PlayRate => "playrate".to_owned(),
        T::Pitch => "pitch".to_owned(),
        T::FxParam => format!(
            "fx:{}:{}",
            ids.fx.doc(e.fx_guid.as_deref()?),
            e.param_index?
        ),
        _ => return None,
    })
}

/// The envelope a key names, on the engine (FX ids its own): a track's
/// own, an FX parameter's (`fx:<fx>:<param>`), a send's
/// (`send:<n>:<volume|pan|mute>`) or a named one (`named:<name>`).
pub(crate) fn envelope_ref(
    key: &str,
    ids: &EngineIds,
) -> Option<daw_proto::automation::EnvelopeRef> {
    use daw_proto::automation::{EnvelopeRef as R, EnvelopeType as T, SendEnvelopeKind as K};
    if let Some(name) = key.strip_prefix("named:") {
        return Some(R::ByName(name.to_owned()));
    }
    if let Some(send) = key.strip_prefix("send:") {
        let (index, kind) = send.split_once(':')?;
        return Some(R::Send {
            send_index: index.parse().ok()?,
            kind: match kind {
                "pan" => K::Pan,
                "mute" => K::Mute,
                _ => K::Volume,
            },
        });
    }
    Some(match key {
        "volume" => R::Type(T::Volume),
        "volume_prefx" => R::Type(T::VolumePrefx),
        "pan" => R::Type(T::Pan),
        "pan_prefx" => R::Type(T::PanPrefx),
        "width" => R::Type(T::Width),
        "width_prefx" => R::Type(T::WidthPrefx),
        "mute" => R::Type(T::Mute),
        "playrate" => R::Type(T::PlayRate),
        "pitch" => R::Type(T::Pitch),
        other => {
            let (fx, param) = other.strip_prefix("fx:")?.rsplit_once(':')?;
            R::FxParam {
                fx_guid: ids.fx.engine(fx),
                param_index: param.parse().ok()?,
            }
        }
    })
}

pub(crate) const fn mode_name(mode: daw_proto::primitives::AutomationMode) -> &'static str {
    use daw_proto::primitives::AutomationMode as M;
    match mode {
        M::TrimRead => "trim",
        M::Read => "read",
        M::Touch => "touch",
        M::Write => "write",
        M::Latch => "latch",
        M::LatchPreview => "latch_preview",
        M::Off => "off",
    }
}

pub(crate) fn mode_of(name: &str) -> daw_proto::primitives::AutomationMode {
    use daw_proto::primitives::AutomationMode as M;
    match name {
        "read" => M::Read,
        "touch" => M::Touch,
        "write" => M::Write,
        "latch" => M::Latch,
        "latch_preview" => M::LatchPreview,
        "off" => M::Off,
        _ => M::TrimRead,
    }
}

pub(crate) const fn shape_number(shape: daw_proto::automation::EnvelopeShape) -> u8 {
    use daw_proto::automation::EnvelopeShape as S;
    match shape {
        S::Linear => 0,
        S::Square => 1,
        S::SlowStartEnd => 2,
        S::FastStart => 3,
        S::FastEnd => 4,
        S::Bezier => 5,
    }
}

pub(crate) const fn shape_of(n: u8) -> daw_proto::automation::EnvelopeShape {
    use daw_proto::automation::EnvelopeShape as S;
    match n {
        1 => S::Square,
        2 => S::SlowStartEnd,
        3 => S::FastStart,
        4 => S::FastEnd,
        5 => S::Bezier,
        _ => S::Linear,
    }
}

pub(crate) const fn send_mode_name(mode: daw_proto::routing::SendMode) -> &'static str {
    use daw_proto::routing::SendMode as M;
    match mode {
        M::PreFx => "pre_fx",
        M::PostFx => "post_fx",
        M::PostFader => "post_fader",
    }
}

pub(crate) fn send_mode_of(name: &str) -> daw_proto::routing::SendMode {
    use daw_proto::routing::SendMode as M;
    match name {
        "pre_fx" => M::PreFx,
        "post_fx" => M::PostFx,
        _ => M::PostFader,
    }
}

/// A track's FX (their state into `blobs`), sends and envelopes.
async fn read_track_parts(
    handle: &daw_control::TrackHandle,
    state: &mut TrackState,
    blobs: &dyn Blobs,
    ids: &EngineIds,
) -> daw_control::Result<()> {
    state.fx = read_chain(&handle.fx_chain(), blobs, ids).await?;
    state.input_fx = read_chain(&handle.input_fx_chain(), blobs, ids).await?;
    for route in handle.sends().all().await? {
        if let Some(dest) = route.dest_track_guid.clone() {
            state.sends.push(route_state(&route, dest));
        }
    }
    for route in handle.hardware_outputs().all().await? {
        let dest = route.hw_output_index.unwrap_or(0).to_string();
        state.hw_outputs.push(route_state(&route, dest));
    }
    let envelopes = handle.envelopes();
    for e in envelopes.all().await? {
        let Some(key) = envelope_key(&e, ids) else {
            continue;
        };
        let Some(reference) = envelope_ref(&key, ids) else {
            continue;
        };
        let points = envelopes.of(reference).points().await?;
        if points.is_empty() {
            continue;
        }
        state.envelopes.insert(
            key,
            EnvelopeState {
                mode: mode_name(e.automation_mode).to_owned(),
                visible: e.visible,
                armed: e.armed,
                points: points
                    .iter()
                    .map(|p| EnvelopePointState {
                        at: p.time.as_seconds(),
                        value: p.value,
                        shape: shape_number(p.shape),
                        tension: p.tension,
                    })
                    .collect(),
                own_lane: e.in_own_lane,
                lane_height: e.lane_height,
                // Its automation items are daw-standalone's to tell
                // ([`crate::whole`]).
                items: Vec::new(),
            },
        );
    }
    Ok(())
}

/// A chain's FX as the doc keeps them, their state into `blobs`. Their
/// parameters, preset and named config are daw-standalone's to tell
/// ([`crate::whole`]).
async fn read_chain(
    chain: &daw_control::FxChain,
    blobs: &dyn Blobs,
    ids: &EngineIds,
) -> daw_control::Result<Vec<FxState>> {
    let mut out = Vec::new();
    for fx in chain.all().await? {
        let saved = match chain.by_guid(&fx.guid).await? {
            Some(h) => h.state_chunk().await.ok().flatten(),
            None => None,
        };
        out.push(FxState {
            guid: ids.fx.doc(&fx.guid),
            plugin: if fx.plugin_name.is_empty() {
                fx.name.clone()
            } else {
                fx.plugin_name.clone()
            },
            name: fx.name,
            enabled: fx.enabled,
            offline: fx.offline,
            state: saved.map(|bytes| blobs.put(&bytes)),
            param_count: fx.parameter_count,
            preset_name: fx.preset_name,
            ..FxState::default()
        });
    }
    Ok(out)
}

/// A send or hardware output as the doc keeps it, `dest` its far end.
pub(crate) fn route_state(route: &daw_proto::TrackRoute, dest: String) -> SendState {
    SendState {
        dest,
        mode: send_mode_name(route.send_mode).to_owned(),
        volume: route.volume,
        pan: route.pan,
        muted: route.muted,
        from: route.source_channels.start_channel,
        to: route.dest_channels.start_channel,
        mono: route.mono,
        phase_inverted: route.phase_inverted,
        automation_mode: mode_name(route.automation_mode).to_owned(),
        from_count: route.source_channels.num_channels,
        to_count: route.dest_channels.num_channels,
        midi: route.midi_channel_mapping.as_ref().map(midi_mapping_name),
        label: route
            .dest_track_name
            .clone()
            .or_else(|| route.hw_output_name.clone()),
    }
}

/// A track's comping as the doc keeps it.
pub(crate) fn comping_state(c: &daw_proto::LaneComping) -> CompingState {
    CompingState {
        record_lane: c.record_lane,
        comp_lane: c.comp_lane,
        last_comp_lane: c.last_comp_lane,
        areas: c
            .areas
            .iter()
            .map(|a| CompAreaState {
                start: a.start.as_seconds(),
                end: a.end.as_seconds(),
                source_lane: a.source_lane,
                comp_lane: a.comp_lane,
                fade_in: a.fade_in.as_seconds(),
                fade_out: a.fade_out.as_seconds(),
            })
            .collect(),
    }
}

/// The doc's comp areas as the engine takes them.
pub(crate) fn comp_areas(c: &CompingState) -> Vec<daw_proto::CompArea> {
    c.areas
        .iter()
        .map(|a| daw_proto::CompArea {
            start: PositionInSeconds::from_seconds(a.start),
            end: PositionInSeconds::from_seconds(a.end),
            source_lane: a.source_lane,
            comp_lane: a.comp_lane,
            fade_in: Duration::from_seconds(a.fade_in),
            fade_out: Duration::from_seconds(a.fade_out),
        })
        .collect()
}

/// Make the engine's chain on `track` the doc's (see [`apply_chain`]).
async fn apply_fx(
    project: &Project,
    track: &TrackState,
    input: bool,
    blobs: &dyn Blobs,
    ids: &mut EngineIds,
) -> daw_control::Result<()> {
    let Some(handle) = project.tracks().by_guid(&track.guid).await? else {
        return Ok(());
    };
    if input {
        apply_chain(&handle.input_fx_chain(), &track.input_fx, blobs, ids).await
    } else {
        apply_chain(&handle.fx_chain(), &track.fx, blobs, ids).await
    }
}

/// Make the engine's `chain` the doc's `fx`: what is missing added (with
/// its state), what is gone removed, each one's switches set, all in the
/// doc's order. New FX are bound in `ids`.
async fn apply_chain(
    chain: &daw_control::FxChain,
    fx: &[FxState],
    blobs: &dyn Blobs,
    ids: &mut EngineIds,
) -> daw_control::Result<()> {
    let wanted: std::collections::HashSet<String> =
        fx.iter().map(|f| ids.fx.engine(&f.guid)).collect();
    for fx in chain.all().await? {
        if !wanted.contains(&fx.guid)
            && let Some(h) = chain.by_guid(&fx.guid).await?
        {
            h.remove().await?;
        }
    }
    for (index, fx) in fx.iter().enumerate() {
        let at = u32::try_from(index).unwrap_or(u32::MAX);
        let guid = ids.fx.engine(&fx.guid);
        let h = if let Some(h) = chain.by_guid(&guid).await? {
            h.move_to(at).await?;
            // Its state only if it changed: a plugin reloads on a load.
            if let Some(name) = &fx.state
                && h.state_chunk()
                    .await
                    .ok()
                    .flatten()
                    .map(|b| blob_name(&b))
                    .as_ref()
                    != Some(name)
                && let Some(bytes) = blobs.get(name)
            {
                h.set_state_chunk(bytes).await?;
            }
            h
        } else {
            // By its name first: a built-in's is what its engine adds it by
            // (`fts.guide:click`), where its plugin's is the family's
            // (`FTS Guide`); a hosted plugin's by its plugin, failing that.
            let h = match chain.add_at(&fx.name, at).await {
                Ok(h) => h,
                Err(_) => chain.add_at(&fx.plugin, at).await?,
            };
            let info = h.info().await?;
            ids.fx.bind(&fx.guid, &info.guid);
            if let Some(bytes) = fx.state.as_deref().and_then(|name| blobs.get(name)) {
                h.set_state_chunk(bytes).await?;
            }
            h
        };
        if fx.enabled {
            h.enable().await?;
        } else {
            h.disable().await?;
        }
        h.set_offline(fx.offline).await?;
    }
    Ok(())
}

/// Make the engine's sends from `track` the doc's: every one removed and
/// the doc's added in order (sends have no identity of their own).
async fn apply_sends(project: &Project, track: &TrackState) -> daw_control::Result<()> {
    let Some(handle) = project.tracks().by_guid(&track.guid).await? else {
        return Ok(());
    };
    let sends = handle.sends();
    for _ in sends.all().await? {
        if let Some(first) = sends.by_index(0).await? {
            first.remove().await?;
        }
    }
    for send in &track.sends {
        let route = sends.add_to(&send.dest).await?;
        set_route(&route, send, true).await?;
    }
    Ok(())
}

/// Make the engine's envelopes on `track` the doc's: each one's points
/// replaced, its mode and switches set; one the doc no longer has
/// cleared.
async fn apply_envelopes(
    project: &Project,
    track: &TrackState,
    ids: &EngineIds,
) -> daw_control::Result<()> {
    let Some(handle) = project.tracks().by_guid(&track.guid).await? else {
        return Ok(());
    };
    let envelopes = handle.envelopes();
    for e in envelopes.all().await? {
        if let Some(key) = envelope_key(&e, ids)
            && !track.envelopes.contains_key(&key)
            && let Some(reference) = envelope_ref(&key, ids)
        {
            envelopes.of(reference).clear().await?;
        }
    }
    for (key, envelope) in &track.envelopes {
        let Some(reference) = envelope_ref(key, ids) else {
            continue;
        };
        let h = envelopes.of(reference);
        h.clear().await?;
        for p in &envelope.points {
            h.add_point(
                PositionInSeconds::from_seconds(p.at),
                p.value,
                shape_of(p.shape),
            )
            .await?;
        }
        h.set_automation_mode(mode_of(&envelope.mode)).await?;
        h.set_visible(envelope.visible).await?;
        h.set_armed(envelope.armed).await?;
        if envelope.own_lane {
            h.show_in_lane(envelope.lane_height).await?;
        } else {
            h.overlay().await?;
        }
    }
    Ok(())
}

/// Make the engine's group masks on `track` the doc's: each slot the
/// track is in joined (every family, both ways), then each family set to
/// its role — lead, follow, both, or out.
async fn apply_grouping(
    handle: &daw_control::TrackHandle,
    track: &TrackState,
) -> daw_control::Result<()> {
    let now = grouping_map(&handle.group_flags().await?);
    let mask = |map: &BTreeMap<String, u128>, family: &str, role: &str| {
        map.get(&format!("{family}_{role}")).copied().unwrap_or(0)
    };
    let slots: u128 = track
        .grouping
        .values()
        .chain(now.values())
        .fold(0, |all, m| all | m);
    for slot in 1..=128_u32 {
        let Some(bit) = 1_u128.checked_shl(slot.saturating_sub(1)) else {
            continue;
        };
        if slots & bit == 0 {
            continue;
        }
        let member = track.grouping.values().any(|m| m & bit != 0);
        handle.set_group_membership(slot, member).await?;
        if !member {
            continue;
        }
        for (family, which) in FAMILIES {
            let lead = mask(&track.grouping, family, "lead") & bit != 0;
            let follow = mask(&track.grouping, family, "follow") & bit != 0;
            let role = match (lead, follow) {
                (true, true) => continue,
                (true, false) => daw_proto::track::GroupRole::Lead,
                (false, true) => daw_proto::track::GroupRole::Follow,
                (false, false) => daw_proto::track::GroupRole::None,
            };
            handle.set_group_flags(slot, which, role).await?;
        }
    }
    Ok(())
}

/// An engine refusal, as the facade reports one.
fn engine_error(e: &daw_proto::DawError) -> daw_control::Error {
    daw_control::Error::Other(e.to_string())
}

#[cfg(test)]
mod curve_tests {
    use super::{GainPoint, over_item};

    /// A slice keeps its own stretch of a take's curve, and the points
    /// just beyond it, so the curve at its edges is what it was.
    #[test]
    fn a_slice_keeps_its_stretch_of_the_curve() {
        let curve: Vec<GainPoint> = (-50..50)
            .map(|s| GainPoint {
                at: f64::from(s),
                value: 1.0,
            })
            .collect();
        let kept = over_item(&curve, 2.0);
        let at: Vec<f64> = kept.iter().map(|g| g.at).collect();
        assert_eq!(at, [-2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        assert!(over_item(&[], 2.0).is_empty());
    }
}
