//! The rest of a project, through daw-standalone's own state.
//!
//! The facade shows a project the way any backend can: a track's chains,
//! its sends, an item's active take. daw-standalone holds more — a
//! track's channel count and named ext-state sections, an FX's parameters
//! and preset, every take of an item with its MIDI beyond notes and its
//! stretch mode, every kind of envelope with its automation items, the
//! project's ext-state, ruler lanes, master bus and media bay. When it is
//! the engine in this process ([`Direct::standalone`]) the doc reads those
//! here, straight from its [`ProjectState`] under one lock, and writes
//! them back the same way.
//!
//! Writing runs after the facade's own calls — which add FX with their
//! plugins, items with their takes, sends with their receives — and sets
//! exactly what the doc holds over whatever those calls left. Nothing
//! here publishes the engine's events: the renderer follows the
//! project's revision, and the bridge's caller redraws from the changes.

use std::collections::{BTreeMap, HashMap, VecDeque};

use daw_proto::automation::{
    AutomationItem, EnvelopePoint, EnvelopeType, SendEnvelopeKind, TakeEnvelopeKind,
};
use daw_proto::item::{ItemRef, TakeRef};
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{DawError, DawResult, ProjectContext, Take, TrackRoute};
use daw_standalone::sync::{EnvelopeData, EnvelopeKey, FxChainKey, ProjectState, Standalone};

use crate::diff::{Change, ItemField, TrackField};
use crate::doc::MASTER;
use crate::engine::{
    Direct, EngineIds, MediaRoot, beat_attach_of, comping_state, grouping_of, lane_display_of,
    midi_mapping_of, mode_name, mode_of, monitor_of, record_input_of, send_mode_of, shape_number,
    shape_of, source_type_of,
};
use crate::model::{
    AutomationItemState, CcState, ChannelPressureState, CompingState, EnvelopePointState,
    EnvelopeState, FxState, GainPoint, ItemState, MidiEvents, NoteExpressionState, PitchBendState,
    PolyPressureState, ProgramChangeState, ProjectSettings, RulerLaneState, SendState,
    SessionModel, StretchPoint, SysexState, TakeMarkerState, TakeState, TrackState,
};

// ── reading ─────────────────────────────────────────────────────────────

/// Fill `model` with what only the engine's own state holds.
///
/// `model` is read through the facade; this adds the project's settings and master bus, every take of
/// every item, and (when `tracks` were read afresh rather than kept) each
/// track's channels, comping, ext sections, envelopes and FX parameters.
pub fn read_into(
    sa: &Standalone,
    project: &str,
    model: &mut SessionModel,
    media: &MediaRoot,
    ids: &EngineIds,
    tracks: bool,
) {
    let _ = sa.read_project(project, |p| {
        let envelopes = EnvelopeIndex::of(p);
        if tracks {
            for track in &mut model.tracks {
                read_track(p, &envelopes, track, ids);
            }
        }
        read_settings(p, &envelopes, &mut model.project, ids);
        for (id, item) in &mut model.items {
            read_item(p, &envelopes, &ids.items.engine(id), item, media);
        }
        model.take_markers = read_take_markers(p, &model.items, media, ids);
    });
}

/// The project's envelopes, by owner: each track's (and the master's),
/// and each take's — so a song of thirty thousand slices, each with its
/// take's curve, is not searched once a slice.
struct EnvelopeIndex<'a> {
    tracks: HashMap<&'a str, Vec<(&'a EnvelopeKey, &'a EnvelopeData)>>,
    takes: HashMap<(&'a str, &'a str), Vec<(TakeEnvelopeKind, &'a EnvelopeData)>>,
}

impl<'a> EnvelopeIndex<'a> {
    fn of(p: &'a ProjectState) -> Self {
        let mut tracks: HashMap<&str, Vec<(&EnvelopeKey, &EnvelopeData)>> = HashMap::new();
        let mut takes: HashMap<(&str, &str), Vec<(TakeEnvelopeKind, &EnvelopeData)>> =
            HashMap::new();
        for ((owner, key), data) in &p.envelopes {
            if let EnvelopeKey::Take {
                item_guid,
                take_guid,
                kind,
            } = key
            {
                takes
                    .entry((item_guid.as_str(), take_guid.as_str()))
                    .or_default()
                    .push((*kind, data));
            } else {
                tracks.entry(owner.as_str()).or_default().push((key, data));
            }
        }
        Self { tracks, takes }
    }

    /// The envelopes on `owner` (a track's guid, or [`MASTER`]), by the
    /// doc's names for them.
    fn of_track(&self, owner: &str, ids: &EngineIds) -> BTreeMap<String, EnvelopeState> {
        self.tracks
            .get(owner)
            .into_iter()
            .flatten()
            .filter_map(|(key, data)| Some((envelope_name(key, ids)?, envelope_state(data))))
            .collect()
    }
}

fn read_track(
    p: &ProjectState,
    envelopes: &EnvelopeIndex<'_>,
    t: &mut TrackState,
    ids: &EngineIds,
) {
    let ext = p.track_ext.get(&t.guid);
    t.channels = ext.map_or(2, |e| e.num_channels);
    t.comping = ext.map_or_else(CompingState::default, |e| comping_state(&e.comping));
    t.ext_sections = BTreeMap::new();
    t.ext = BTreeMap::new();
    for ((track, section, key), value) in &p.track_ext_state {
        if *track != t.guid {
            continue;
        }
        // A key of no section is the track's own (`P_EXT`): what the
        // window keeps on a track — its markers and regions, a leveled
        // vocal's lanes — and what an undo must take back with the rest.
        if section.is_empty() {
            t.ext.insert(key.clone(), value.clone());
        } else {
            t.ext_sections
                .entry(section.clone())
                .or_default()
                .insert(key.clone(), value.clone());
        }
    }
    t.envelopes = envelopes.of_track(&t.guid, ids);
    fx_extras(p, &FxChainKey::Track(t.guid.clone()), &mut t.fx, ids);
    fx_extras(p, &FxChainKey::Input(t.guid.clone()), &mut t.input_fx, ids);
}

/// An FX's parameters, preset and named config, onto the doc's FX.
fn fx_extras(p: &ProjectState, chain: &FxChainKey, fx: &mut [FxState], ids: &EngineIds) {
    let entries = p.fx_chains.get(chain);
    for f in fx {
        let engine = ids.fx.engine(&f.guid);
        if let Some(e) = entries.into_iter().flatten().find(|e| e.fx.guid == engine) {
            f.params = e.params.iter().map(|(id, v)| (*id, *v)).collect();
            f.param_count = e.fx.parameter_count;
            f.preset_name.clone_from(&e.fx.preset_name);
        }
        f.preset = p.fx_preset_index.get(&engine).copied();
        f.config = p
            .fx_named_config
            .iter()
            .filter(|((guid, _), _)| *guid == engine)
            .map(|((_, key), value)| (key.clone(), value.clone()))
            .collect();
    }
}

/// The project's settings, the master bus and the monitoring chain.
fn read_settings(
    p: &ProjectState,
    envelopes: &EnvelopeIndex<'_>,
    s: &mut ProjectSettings,
    ids: &EngineIds,
) {
    s.master.volume = p.master_volume;
    s.master.pan = p.master_pan;
    s.master.muted = p.master_muted;
    s.master.envelopes = envelopes.of_track(MASTER, ids);
    s.master.hw_outputs = p
        .hw_outputs
        .get(MASTER)
        .into_iter()
        .flatten()
        .map(|r| crate::engine::route_state(r, r.hw_output_index.unwrap_or(0).to_string()))
        .collect();
    fx_extras(
        p,
        &FxChainKey::Track(MASTER.to_owned()),
        &mut s.master.fx,
        ids,
    );
    fx_extras(p, &FxChainKey::Monitoring, &mut s.monitoring_fx, ids);
    s.ext = BTreeMap::new();
    for ((section, key), value) in &p.project_ext_state {
        s.ext
            .entry(section.clone())
            .or_default()
            .insert(key.clone(), value.clone());
    }
    s.ruler_lanes = p
        .ruler_lanes
        .iter()
        .map(|(index, lane)| {
            (
                *index,
                RulerLaneState {
                    name: lane.name.clone(),
                    flags: lane.flags,
                },
            )
        })
        .collect();
    s.automation_override = p
        .global_automation_override
        .map(|m| mode_name(m).to_owned());
    s.bay = p
        .bay_state
        .as_ref()
        .map(daw_standalone::media_bay::BayState::serialize);
    s.looping = p.transport.looping;
    s.metronome = p.transport.metronome;
    s.playrate = p.transport.playrate;
    record_mode_name(p.transport.record_mode).clone_into(&mut s.record_mode);
}

/// An item's takes: the active one's fields the facade does not show, and
/// every other one whole.
fn read_item(
    p: &ProjectState,
    envelopes: &EnvelopeIndex<'_>,
    guid: &str,
    item: &mut ItemState,
    media: &MediaRoot,
) {
    let Some(list) = p.takes.get(guid) else {
        return;
    };
    let active = usize::try_from(list.active_idx).unwrap_or(usize::MAX);
    item.active_take = list.active_idx;
    item.other_takes = list
        .takes
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != active)
        .map(|(_, t)| take_state(p, envelopes, guid, t, media, item.length))
        .collect();
    if let Some(t) = list.takes.get(active) {
        take_extras(p, envelopes, guid, &t.guid, &mut item.take);
    }
}

/// A take whole, as the doc keeps it.
fn take_state(
    p: &ProjectState,
    envelopes: &EnvelopeIndex<'_>,
    item: &str,
    t: &Take,
    media: &MediaRoot,
    length: f64,
) -> TakeState {
    let mut take = TakeState {
        name: t.name.clone(),
        source: t.source_file_path.as_deref().map(|s| media.to_doc(s)),
        start_offset: t.start_offset.as_seconds(),
        playrate: t.play_rate,
        midi: t.is_midi,
        stretch: p
            .stretch_markers
            .get(&t.guid)
            .into_iter()
            .flatten()
            .map(|m| StretchPoint {
                at: m.position,
                source: m.source_position,
                slope: m.slope,
            })
            .collect(),
        notes: p
            .midi_notes
            .get(&t.guid)
            .into_iter()
            .flatten()
            .map(crate::engine::note_state)
            .collect(),
        color: t.color,
        gain: t.volume,
        pitch: t.pitch,
        preserve_pitch: t.preserve_pitch,
        channel_mode: t.channel_mode,
        source_type: crate::engine::source_type_name(t.source_type).to_owned(),
        source_length: t.source_length.map(|l| l.as_seconds()),
        source_rate: t.source_sample_rate,
        source_channels: t.source_channels,
        note_count: t.midi_note_count,
        ..TakeState::default()
    };
    let curve: Vec<GainPoint> = envelopes
        .takes
        .get(&(item, t.guid.as_str()))
        .into_iter()
        .flatten()
        .filter(|(kind, _)| *kind == TakeEnvelopeKind::Volume)
        .flat_map(|(_, data)| data.points.iter())
        .map(|g| GainPoint {
            at: g.time.as_seconds(),
            value: g.value,
        })
        .collect();
    take.volume = crate::engine::over_item(&curve, length);
    take_extras(p, envelopes, item, &t.guid, &mut take);
    take
}

/// A take's stretch mode, envelopes other than volume, and MIDI events
/// other than notes.
fn take_extras(
    p: &ProjectState,
    envelopes: &EnvelopeIndex<'_>,
    item: &str,
    take_guid: &str,
    take: &mut TakeState,
) {
    take.stretch_mode =
        stretch_mode_number(p.stretch_modes.get(take_guid).copied().unwrap_or_default());
    take.envelopes = envelopes
        .takes
        .get(&(item, take_guid))
        .into_iter()
        .flatten()
        .filter(|(kind, _)| *kind != TakeEnvelopeKind::Volume)
        .map(|(kind, data)| (take_envelope_name(*kind).to_owned(), envelope_state(data)))
        .collect();
    take.midi_events = midi_events(p, take_guid);
}

fn midi_events(p: &ProjectState, take: &str) -> MidiEvents {
    MidiEvents {
        ccs: p
            .midi_ccs
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| CcState {
                at: e.position_ppq,
                channel: e.channel,
                controller: e.controller,
                value: e.value,
            })
            .collect(),
        pitch_bends: p
            .midi_pitch_bends
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| PitchBendState {
                at: e.position_ppq,
                channel: e.channel,
                value: e.value,
            })
            .collect(),
        program_changes: p
            .midi_program_changes
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| ProgramChangeState {
                at: e.position_ppq,
                channel: e.channel,
                program: e.program,
            })
            .collect(),
        sysex: p
            .midi_sysex
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| SysexState {
                at: e.position_ppq,
                data: e.data.clone(),
            })
            .collect(),
        channel_pressures: p
            .midi_channel_pressures
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| ChannelPressureState {
                at: e.position_ppq,
                channel: e.channel,
                pressure: e.pressure,
            })
            .collect(),
        poly_pressures: p
            .midi_poly_pressures
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| PolyPressureState {
                at: e.position_ppq,
                channel: e.channel,
                note: e.note,
                pressure: e.pressure,
            })
            .collect(),
        note_expressions: p
            .midi_note_expressions
            .get(take)
            .into_iter()
            .flatten()
            .map(|e| NoteExpressionState {
                at: e.position_ppq,
                channel: e.channel,
                note: e.note,
                dimension: dimension_number(e.dimension),
                value: e.value,
            })
            .collect(),
    }
}

/// Where a take is in the doc: its item's id and its place there.
type TakePlace = (String, usize);

/// Every take marker the engine holds, keyed as the doc keeps them: a
/// recording's by its file (whether or not an active take plays it), a
/// take with no file's by `take:<item>:<place>`.
fn read_take_markers(
    p: &ProjectState,
    items: &BTreeMap<String, ItemState>,
    media: &MediaRoot,
    ids: &EngineIds,
) -> BTreeMap<String, TakeMarkerState> {
    let fileless = fileless_takes(p, items, ids);
    let mut out = BTreeMap::new();
    for (key, markers) in &p.take_markers {
        let source = match fileless.get(key.as_str()) {
            Some((item, place)) => format!("take:{item}:{place}"),
            None => media.to_doc(key),
        };
        for m in markers {
            let state = crate::engine::take_marker_state(&source, m.clone());
            out.insert(state.key(), state);
        }
    }
    out
}

/// The takes with no file, by guid: where each one is in the doc.
fn fileless_takes<'a>(
    p: &'a ProjectState,
    items: &BTreeMap<String, ItemState>,
    ids: &EngineIds,
) -> HashMap<&'a str, TakePlace> {
    let mut out = HashMap::new();
    for id in items.keys() {
        let engine = ids.items.engine(id);
        let takes = p
            .takes
            .get(&engine)
            .into_iter()
            .flat_map(|l| l.takes.iter());
        for (place, t) in takes.enumerate() {
            if t.source_file_path.is_none() {
                out.insert(t.guid.as_str(), (id.clone(), place));
            }
        }
    }
    out
}

// ── writing ─────────────────────────────────────────────────────────────

/// A take whose source file changed: told to the engine after the write,
/// which lets go of the audio decoded from the old one.
struct NewSource {
    item: String,
    take: String,
    path: String,
    kind: daw_proto::SourceType,
}

/// Set in the engine's own state what `changes` lead to and the facade
/// could not set — `target` is the model they lead to.
///
/// # Errors
/// When the project is not the engine's, or the engine refuses a take's
/// new source.
pub fn apply(
    sa: &Standalone,
    d: &dyn Direct,
    project: &str,
    changes: &[Change],
    target: &SessionModel,
    ids: &EngineIds,
    media: &MediaRoot,
) -> DawResult<()> {
    let mut sources: Vec<NewSource> = Vec::new();
    sa.write_project(project, |p| {
        let mut routes = false;
        for change in changes {
            match change {
                Change::TrackAdded { track, .. } => routes |= write_track(p, track, None, ids),
                Change::TrackChanged { track, fields } => {
                    routes |= write_track(p, track, Some(fields.as_slice()), ids);
                }
                Change::ItemAdded { guid, item } => {
                    write_item(p, &ids.items.engine(guid), item, None, media, &mut sources);
                }
                Change::ItemChanged { guid, item, fields } => {
                    let at = ids.items.engine(guid);
                    write_item(p, &at, item, Some(fields.as_slice()), media, &mut sources);
                }
                Change::ProjectChanged(settings) => write_settings(p, settings, ids),
                Change::TakeMarkersSet { source, markers } => {
                    write_take_markers(p, target, source, markers, media, ids);
                }
                _ => {}
            }
        }
        if routes {
            mirror_receives(p);
        }
    })
    .ok_or_else(|| DawError::not_found("Project", project))?;
    let ctx = ProjectContext::Project(project.to_owned());
    for s in sources {
        d.set_source_file(
            ctx.clone(),
            ItemRef::Guid(s.item.clone()),
            TakeRef::Guid(s.take.clone()),
            s.path,
        )?;
        // That makes it an audio source; a MIDI or empty one is put back.
        if s.kind != daw_proto::SourceType::Audio {
            sa.write_project(project, |p| {
                if let Some(t) = p
                    .takes
                    .get_mut(&s.item)
                    .and_then(|l| l.takes.iter_mut().find(|t| t.guid == s.take))
                {
                    t.source_type = s.kind;
                }
            });
        }
    }
    Ok(())
}

/// A track's state the facade could not set; `None` is every field (a
/// new track). Returns whether its sends changed.
fn write_track(
    p: &mut ProjectState,
    t: &TrackState,
    fields: Option<&[TrackField]>,
    ids: &EngineIds,
) -> bool {
    let has = |f: TrackField| fields.is_none_or(|fs| fs.contains(&f));
    let settings = [
        TrackField::Armed,
        TrackField::AutomationMode,
        TrackField::Monitor,
        TrackField::RecordInput,
        TrackField::Channels,
        TrackField::Height,
        TrackField::Width,
        TrackField::LaneDisplay,
        TrackField::Grouping,
        TrackField::ParentSend,
    ];
    if settings.into_iter().any(has) {
        write_track_settings(p, t);
    }
    if has(TrackField::Comping) {
        p.track_ext.entry(t.guid.clone()).or_default().comping = daw_proto::LaneComping {
            record_lane: t.comping.record_lane,
            comp_lane: t.comping.comp_lane,
            last_comp_lane: t.comping.last_comp_lane,
            areas: crate::engine::comp_areas(&t.comping),
        };
    }
    if has(TrackField::Ext) {
        p.track_ext_state
            .retain(|(track, section, _), _| *track != t.guid || !section.is_empty());
        for (key, value) in &t.ext {
            p.track_ext_state
                .insert((t.guid.clone(), String::new(), key.clone()), value.clone());
        }
    }
    if has(TrackField::ExtSections) {
        p.track_ext_state
            .retain(|(track, section, _), _| *track != t.guid || section.is_empty());
        for (section, keys) in &t.ext_sections {
            for (key, value) in keys {
                p.track_ext_state.insert(
                    (t.guid.clone(), section.clone(), key.clone()),
                    value.clone(),
                );
            }
        }
    }
    if has(TrackField::Envelopes) {
        replace_envelopes(p, &t.guid, &t.envelopes, ids);
    }
    if has(TrackField::Fx) {
        write_fx_extras(p, &FxChainKey::Track(t.guid.clone()), &t.fx, ids);
    }
    if has(TrackField::InputFx) {
        write_fx_extras(p, &FxChainKey::Input(t.guid.clone()), &t.input_fx, ids);
    }
    if has(TrackField::HwOutputs) {
        set_routes(
            &mut p.hw_outputs,
            &t.guid,
            hw_routes(&t.guid, &t.hw_outputs),
        );
    }
    if has(TrackField::Sends) {
        set_routes(&mut p.sends, &t.guid, send_routes(&t.guid, &t.sends));
        return true;
    }
    false
}

/// Armed, modes, inputs, channels, height, width, lane display, the
/// whole group matrix and the master send — on the track and on the
/// engine's side record of it, which must agree.
fn write_track_settings(p: &mut ProjectState, t: &TrackState) {
    let input = record_input_of(&t.record_input);
    if let Some(track) = p.tracks.iter_mut().find(|x| x.guid == t.guid) {
        track.armed = t.armed;
        track.automation_mode = mode_of(&t.automation_mode);
        track.input_monitor = monitor_of(&t.monitor);
        track.record_input = input;
        track.height = t.height;
        track.width = t.width;
        track.lane_display = lane_display_of(&t.lane_display);
        track.grouping = grouping_of(&t.grouping);
        track.parent_send = t.parent_send;
    }
    let ext = p.track_ext.entry(t.guid.clone()).or_default();
    ext.num_channels = t.channels.clamp(1, 128);
    ext.record_input = input;
    ext.tcp_height_pixels = t.height.unwrap_or(0);
    ext.parent_send_enabled = t.parent_send;
}

/// `owner`'s envelopes (a track's or the master's) made the doc's; a
/// take's are left alone.
fn replace_envelopes(
    p: &mut ProjectState,
    owner: &str,
    envelopes: &BTreeMap<String, EnvelopeState>,
    ids: &EngineIds,
) {
    p.envelopes
        .retain(|(o, key), _| o != owner || matches!(key, EnvelopeKey::Take { .. }));
    for (name, e) in envelopes {
        if let Some(key) = envelope_key(name, ids) {
            p.envelopes
                .insert((owner.to_owned(), key), envelope_data(e));
        }
    }
}

/// An FX's parameters, preset and named config set from the doc's.
fn write_fx_extras(p: &mut ProjectState, chain: &FxChainKey, fx: &[FxState], ids: &EngineIds) {
    for f in fx {
        let engine = ids.fx.engine(&f.guid);
        if let Some(e) = p
            .fx_chains
            .get_mut(chain)
            .and_then(|c| c.iter_mut().find(|e| e.fx.guid == engine))
        {
            e.params = f.params.iter().map(|(id, v)| (*id, *v)).collect();
            e.fx.parameter_count = f.param_count;
            e.fx.preset_name.clone_from(&f.preset_name);
        }
        p.fx_preset_index.remove(&engine);
        if let Some(index) = f.preset {
            p.fx_preset_index.insert(engine.clone(), index);
        }
        p.fx_named_config.retain(|(guid, _), _| *guid != engine);
        for (key, value) in &f.config {
            p.fx_named_config
                .insert((engine.clone(), key.clone()), value.clone());
        }
    }
}

/// A track's route list set; an empty one only where it had one.
fn set_routes(map: &mut HashMap<String, Vec<TrackRoute>>, track: &str, routes: Vec<TrackRoute>) {
    if routes.is_empty() {
        if let Some(had) = map.get_mut(track) {
            had.clear();
        }
    } else {
        map.insert(track.to_owned(), routes);
    }
}

fn send_routes(track: &str, sends: &[SendState]) -> Vec<TrackRoute> {
    sends
        .iter()
        .enumerate()
        .map(|(i, s)| TrackRoute {
            route_type: daw_proto::RouteType::Send,
            dest_track_guid: Some(s.dest.clone()),
            dest_track_name: s.label.clone(),
            hw_output_index: None,
            hw_output_name: None,
            ..route(i, track, s)
        })
        .collect()
}

fn hw_routes(track: &str, outputs: &[SendState]) -> Vec<TrackRoute> {
    outputs
        .iter()
        .enumerate()
        .map(|(i, s)| TrackRoute {
            route_type: daw_proto::RouteType::HardwareOutput,
            dest_track_guid: None,
            dest_track_name: None,
            hw_output_index: s.dest.parse().ok(),
            hw_output_name: s.label.clone(),
            ..route(i, track, s)
        })
        .collect()
}

/// A route's level, pan, switches, mode and channels from the doc's.
fn route(index: usize, track: &str, s: &SendState) -> TrackRoute {
    TrackRoute {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        route_type: daw_proto::RouteType::Send,
        source_track_guid: track.to_owned(),
        dest_track_guid: None,
        dest_track_name: None,
        hw_output_index: None,
        hw_output_name: None,
        volume: s.volume,
        pan: s.pan,
        muted: s.muted,
        mono: s.mono,
        phase_inverted: s.phase_inverted,
        send_mode: send_mode_of(&s.mode),
        automation_mode: mode_of(&s.automation_mode),
        source_channels: daw_proto::ChannelMapping {
            start_channel: s.from,
            num_channels: s.from_count,
        },
        dest_channels: daw_proto::ChannelMapping {
            start_channel: s.to,
            num_channels: s.to_count,
        },
        midi_channel_mapping: s.midi.as_deref().map(midi_mapping_of),
    }
}

/// Every receive rebuilt from the sends, as the engine mirrors them: on
/// the receiving track, its far end the sending one — in track order.
fn mirror_receives(p: &mut ProjectState) {
    let mut receives: HashMap<String, Vec<TrackRoute>> = HashMap::new();
    for track in &p.tracks {
        for send in p.sends.get(&track.guid).into_iter().flatten() {
            let Some(dest) = send.dest_track_guid.clone() else {
                continue;
            };
            let list = receives.entry(dest.clone()).or_default();
            list.push(TrackRoute {
                index: u32::try_from(list.len()).unwrap_or(u32::MAX),
                route_type: daw_proto::RouteType::Receive,
                source_track_guid: dest,
                dest_track_guid: Some(send.source_track_guid.clone()),
                dest_track_name: Some(track.name.clone()),
                ..send.clone()
            });
        }
    }
    for list in p.receives.values_mut() {
        list.clear();
    }
    p.receives.extend(receives);
}

/// An item's state the facade could not set; `None` is every field (a
/// new item).
fn write_item(
    p: &mut ProjectState,
    guid: &str,
    item: &ItemState,
    fields: Option<&[ItemField]>,
    media: &MediaRoot,
    sources: &mut Vec<NewSource>,
) {
    let has = |f: ItemField| fields.is_none_or(|fs| fs.contains(&f));
    // The facade has no setter for it.
    if has(ItemField::SnapOffset)
        && let Some(e) = p.items.get_mut(guid)
    {
        e.item.snap_offset = Duration::from_seconds(item.snap_offset);
    }
    if has(ItemField::Timing)
        && let Some(e) = p.items.get_mut(guid)
    {
        e.item.beat_attach_mode = beat_attach_of(&item.timing.beat_attach);
        e.item.auto_stretch = item.timing.auto_stretch;
        e.item.group_id = item.timing.group;
        e.item.start_offset = Duration::from_seconds(item.timing.start_offset);
    }
    let takes = [
        ItemField::Take,
        ItemField::Stretch,
        ItemField::TakeVolume,
        ItemField::Notes,
        ItemField::TakeSettings,
        ItemField::TakeEnvelopes,
        ItemField::Midi,
        ItemField::Takes,
    ];
    if takes.into_iter().any(has) {
        write_takes(p, guid, item, media, sources);
    }
}

/// An item's takes made the doc's: the engine's own takes kept where
/// they still are (the active one as the active one, the others in
/// order), new ones minted, those left over dropped with what they held.
fn write_takes(
    p: &mut ProjectState,
    guid: &str,
    item: &ItemState,
    media: &MediaRoot,
    sources: &mut Vec<NewSource>,
) {
    let (engine, engine_active) = p.takes.get(guid).map_or((Vec::new(), 0), |l| {
        (
            l.takes.clone(),
            usize::try_from(l.active_idx).unwrap_or(usize::MAX),
        )
    });
    // An item with no take at all (a label-only one) reads as an empty
    // active take with no kind: it stays without one.
    if engine.is_empty() && item.other_takes.is_empty() && item.take.source_type.is_empty() {
        return;
    }
    let active = usize::try_from(item.active_take)
        .unwrap_or(usize::MAX)
        .min(item.other_takes.len());
    let mut spare: VecDeque<Take> = engine
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != engine_active)
        .map(|(_, t)| t.clone())
        .collect();
    let mut takes = Vec::new();
    for (place, doc) in item.takes().into_iter().enumerate() {
        let base = if place == active {
            engine.get(engine_active).cloned()
        } else {
            spare.pop_front()
        };
        let mut t = base.unwrap_or_else(|| Take {
            guid: p.mint_take_guid(),
            ..Take::default()
        });
        let had = t.source_file_path.clone();
        set_take(&mut t, (guid, place, place == active), doc, media);
        if t.source_file_path != had
            && let Some(path) = t.source_file_path.clone()
        {
            sources.push(NewSource {
                item: guid.to_owned(),
                take: t.guid.clone(),
                path,
                kind: t.source_type,
            });
        }
        write_take_data(p, guid, &t.guid, doc);
        takes.push(t);
    }
    for gone in spare {
        drop_take_data(p, guid, &gone.guid);
    }
    let count = u32::try_from(takes.len()).unwrap_or(u32::MAX);
    let list = p.takes.entry(guid.to_owned()).or_default();
    list.takes = takes;
    list.active_idx = u32::try_from(active).unwrap_or(0);
    let active_idx = list.active_idx;
    if let Some(e) = p.items.get_mut(guid) {
        e.item.take_count = count;
        e.item.active_take_index = active_idx;
    }
}

/// A take's own fields from the doc's; `at` is its item, its place and
/// whether it is the active one.
fn set_take(t: &mut Take, at: (&str, usize, bool), doc: &TakeState, media: &MediaRoot) {
    let (item, place, active) = at;
    item.clone_into(&mut t.item_guid);
    t.index = u32::try_from(place).unwrap_or(u32::MAX);
    t.is_active = active;
    t.name.clone_from(&doc.name);
    t.color = doc.color;
    t.volume = doc.gain;
    t.play_rate = doc.playrate;
    t.pitch = doc.pitch;
    t.preserve_pitch = doc.preserve_pitch;
    t.channel_mode = doc.channel_mode;
    t.start_offset = Duration::from_seconds(doc.start_offset);
    t.source_type = source_type_of(&doc.source_type);
    t.source_file_path = doc.source.as_deref().map(|s| media.to_local(s));
    t.source_length = doc.source_length.map(Duration::from_seconds);
    t.source_sample_rate = doc.source_rate;
    t.source_channels = doc.source_channels;
    t.is_midi = doc.midi;
    t.midi_note_count = doc.note_count;
}

/// What a take holds beside itself: stretch markers and mode, notes and
/// the rest of its MIDI, its envelopes.
fn write_take_data(p: &mut ProjectState, item: &str, take: &str, doc: &TakeState) {
    let key = take.to_owned();
    put(
        &mut p.stretch_markers,
        &key,
        doc.stretch
            .iter()
            .map(|m| daw_proto::StretchMarker {
                position: m.at,
                source_position: m.source,
                slope: m.slope,
            })
            .collect(),
    );
    if doc.stretch_mode == 0 {
        p.stretch_modes.remove(&key);
    } else {
        p.stretch_modes
            .insert(key.clone(), stretch_mode_of(doc.stretch_mode));
    }
    let notes: Vec<daw_proto::MidiNote> = doc
        .notes
        .iter()
        .enumerate()
        .map(|(i, n)| daw_proto::MidiNote {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            channel: n.channel,
            pitch: n.pitch,
            velocity: n.velocity,
            start_ppq: n.start,
            length_ppq: n.length,
            selected: false,
            muted: n.muted,
        })
        .collect();
    if doc.midi || !notes.is_empty() {
        p.midi_notes.insert(key.clone(), notes);
    } else {
        p.midi_notes.remove(&key);
    }
    write_midi_events(p, &key, &doc.midi_events);
    write_take_envelopes(p, item, take, doc);
}

/// `value` under `key`, or nothing there when it is empty.
fn put<T>(map: &mut HashMap<String, Vec<T>>, key: &str, value: Vec<T>) {
    if value.is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_owned(), value);
    }
}

fn write_midi_events(p: &mut ProjectState, take: &str, m: &MidiEvents) {
    let index = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let ccs = m
        .ccs
        .iter()
        .enumerate()
        .map(|(i, e)| daw_proto::midi::MidiCC {
            index: index(i),
            channel: e.channel,
            controller: e.controller,
            value: e.value,
            position_ppq: e.at,
            selected: false,
        });
    put(&mut p.midi_ccs, take, ccs.collect());
    let bends = m
        .pitch_bends
        .iter()
        .enumerate()
        .map(|(i, e)| daw_proto::midi::MidiPitchBend {
            index: index(i),
            channel: e.channel,
            value: e.value,
            position_ppq: e.at,
            selected: false,
        });
    put(&mut p.midi_pitch_bends, take, bends.collect());
    let programs =
        m.program_changes
            .iter()
            .enumerate()
            .map(|(i, e)| daw_proto::midi::MidiProgramChange {
                index: index(i),
                channel: e.channel,
                program: e.program,
                position_ppq: e.at,
            });
    put(&mut p.midi_program_changes, take, programs.collect());
    let sysex = m
        .sysex
        .iter()
        .enumerate()
        .map(|(i, e)| daw_proto::midi::MidiSysEx {
            index: index(i),
            position_ppq: e.at,
            data: e.data.clone(),
        });
    put(&mut p.midi_sysex, take, sysex.collect());
    let channel =
        m.channel_pressures
            .iter()
            .enumerate()
            .map(|(i, e)| daw_proto::midi::MidiChannelPressure {
                index: index(i),
                channel: e.channel,
                pressure: e.pressure,
                position_ppq: e.at,
                selected: false,
            });
    put(&mut p.midi_channel_pressures, take, channel.collect());
    let poly =
        m.poly_pressures
            .iter()
            .enumerate()
            .map(|(i, e)| daw_proto::midi::MidiPolyPressure {
                index: index(i),
                channel: e.channel,
                note: e.note,
                pressure: e.pressure,
                position_ppq: e.at,
                selected: false,
            });
    put(&mut p.midi_poly_pressures, take, poly.collect());
    let expressions =
        m.note_expressions
            .iter()
            .enumerate()
            .map(|(i, e)| daw_proto::midi::MidiNoteExpression {
                index: index(i),
                channel: e.channel,
                note: e.note,
                dimension: dimension_of(e.dimension),
                value: e.value,
                position_ppq: e.at,
                selected: false,
            });
    put(&mut p.midi_note_expressions, take, expressions.collect());
}

/// A take's envelopes made the doc's: each kind but volume whole; the
/// volume curve's points (the doc keeps its gains, cut to the item), its
/// settings left as the engine has them.
fn write_take_envelopes(p: &mut ProjectState, item: &str, take: &str, doc: &TakeState) {
    let key = |kind: TakeEnvelopeKind| {
        (
            String::new(),
            EnvelopeKey::Take {
                item_guid: item.to_owned(),
                take_guid: take.to_owned(),
                kind,
            },
        )
    };
    for kind in [
        TakeEnvelopeKind::Pan,
        TakeEnvelopeKind::Mute,
        TakeEnvelopeKind::Pitch,
    ] {
        p.envelopes.remove(&key(kind));
        if let Some(e) = doc.envelopes.get(take_envelope_name(kind)) {
            p.envelopes.insert(key(kind), envelope_data(e));
        }
    }
    let points: Vec<EnvelopePoint> = doc
        .volume
        .iter()
        .enumerate()
        .map(|(i, g)| EnvelopePoint {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            time: PositionInSeconds::from_seconds(g.at),
            value: g.value,
            shape: daw_proto::automation::EnvelopeShape::Linear,
            tension: 0.0,
            selected: false,
        })
        .collect();
    let volume = key(TakeEnvelopeKind::Volume);
    if points.is_empty() {
        if let Some(e) = p.envelopes.get_mut(&volume) {
            e.points.clear();
        }
    } else {
        p.envelopes.entry(volume).or_default().points = points;
    }
}

/// A take dropped: what it held goes with it. (Its decoded audio, if
/// any, stays in the engine's cache — as the engine's own `delete_take`
/// leaves it; that cache is not visible from here.)
fn drop_take_data(p: &mut ProjectState, item: &str, take: &str) {
    p.stretch_markers.remove(take);
    p.stretch_modes.remove(take);
    p.midi_notes.remove(take);
    p.midi_ccs.remove(take);
    p.midi_pitch_bends.remove(take);
    p.midi_program_changes.remove(take);
    p.midi_sysex.remove(take);
    p.midi_channel_pressures.remove(take);
    p.midi_poly_pressures.remove(take);
    p.midi_note_expressions.remove(take);
    p.envelopes.retain(|(_, key), _| {
        !matches!(key, EnvelopeKey::Take { item_guid, take_guid, .. }
            if item_guid == item && take_guid == take)
    });
}

/// The project's settings, the master's level, envelopes, FX parameters
/// and outputs, the monitoring chain's FX parameters.
fn write_settings(p: &mut ProjectState, s: &ProjectSettings, ids: &EngineIds) {
    p.master_volume = s.master.volume;
    p.master_pan = s.master.pan;
    p.master_muted = s.master.muted;
    replace_envelopes(p, MASTER, &s.master.envelopes, ids);
    write_fx_extras(p, &FxChainKey::Track(MASTER.to_owned()), &s.master.fx, ids);
    write_fx_extras(p, &FxChainKey::Monitoring, &s.monitoring_fx, ids);
    set_routes(
        &mut p.hw_outputs,
        MASTER,
        hw_routes(MASTER, &s.master.hw_outputs),
    );
    p.project_ext_state = s
        .ext
        .iter()
        .flat_map(|(section, keys)| {
            keys.iter()
                .map(move |(key, value)| ((section.clone(), key.clone()), value.clone()))
        })
        .collect();
    p.ruler_lanes = s
        .ruler_lanes
        .iter()
        .map(|(index, lane)| {
            (
                *index,
                daw_standalone::sync::RulerLane {
                    name: lane.name.clone(),
                    flags: lane.flags,
                },
            )
        })
        .collect();
    p.global_automation_override = s.automation_override.as_deref().map(mode_of);
    p.bay_state = s
        .bay
        .as_deref()
        .and_then(|b| daw_standalone::media_bay::BayState::deserialize(b).ok());
    p.transport.looping = s.looping;
    p.transport.metronome = s.metronome;
    p.transport.playrate = s.playrate;
    p.transport.record_mode = record_mode_of(&s.record_mode);
}

/// A recording's markers set, whichever takes play it: by its file, or a
/// fileless take's by its place (`take:<item>:<place>`).
fn write_take_markers(
    p: &mut ProjectState,
    target: &SessionModel,
    source: &str,
    markers: &[TakeMarkerState],
    media: &MediaRoot,
    ids: &EngineIds,
) {
    let fileless = source
        .strip_prefix("take:")
        .map(|place| fileless_take(p, target, place, ids));
    let key = match fileless {
        None => media.to_local(source),
        Some(Some(guid)) => guid,
        // A take no item holds any more.
        Some(None) => return,
    };
    let list: Vec<daw_proto::TakeMarker> = markers
        .iter()
        .enumerate()
        .map(|(i, m)| daw_proto::TakeMarker {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            ..crate::engine::proto_take_marker(m)
        })
        .collect();
    put(&mut p.take_markers, &key, list);
}

/// The guid of the take at `<item>:<place>`.
fn fileless_take(
    p: &ProjectState,
    target: &SessionModel,
    place: &str,
    ids: &EngineIds,
) -> Option<String> {
    let (item, at) = place.rsplit_once(':')?;
    if !target.items.contains_key(item) {
        return None;
    }
    let at: usize = at.parse().ok()?;
    p.takes
        .get(&ids.items.engine(item))?
        .takes
        .get(at)
        .map(|t| t.guid.clone())
}

// ── envelopes ───────────────────────────────────────────────────────────

/// What the doc calls an envelope on a track: its type (`volume`, …), an
/// FX parameter's (`fx:<fx>:<param>`), a send's
/// (`send:<n>:<volume|pan|mute>`) or a named one (`named:<name>`). A
/// take's is its take's ([`TakeState::envelopes`]).
fn envelope_name(key: &EnvelopeKey, ids: &EngineIds) -> Option<String> {
    Some(match key {
        EnvelopeKey::Track(kind) => track_envelope_name(*kind).to_owned(),
        EnvelopeKey::FxParam {
            fx_guid,
            param_index,
        } => format!("fx:{}:{param_index}", ids.fx.doc(fx_guid)),
        EnvelopeKey::Send { send_index, kind } => {
            format!("send:{send_index}:{}", send_envelope_name(*kind))
        }
        EnvelopeKey::Named(name) => format!("named:{name}"),
        EnvelopeKey::Take { .. } => return None,
    })
}

/// [`envelope_name`] undone, the FX ids the engine's.
fn envelope_key(name: &str, ids: &EngineIds) -> Option<EnvelopeKey> {
    match name {
        "custom" => Some(EnvelopeKey::Track(EnvelopeType::Custom)),
        "fx_type" => Some(EnvelopeKey::Track(EnvelopeType::FxParam)),
        _ => crate::engine::envelope_ref(name, ids).map(|r| EnvelopeKey::from_ref(&r)),
    }
}

const fn track_envelope_name(kind: EnvelopeType) -> &'static str {
    match kind {
        EnvelopeType::Volume => "volume",
        EnvelopeType::VolumePrefx => "volume_prefx",
        EnvelopeType::Pan => "pan",
        EnvelopeType::PanPrefx => "pan_prefx",
        EnvelopeType::Width => "width",
        EnvelopeType::WidthPrefx => "width_prefx",
        EnvelopeType::Mute => "mute",
        EnvelopeType::PlayRate => "playrate",
        EnvelopeType::Pitch => "pitch",
        EnvelopeType::FxParam => "fx_type",
        EnvelopeType::Custom => "custom",
    }
}

const fn send_envelope_name(kind: SendEnvelopeKind) -> &'static str {
    match kind {
        SendEnvelopeKind::Volume => "volume",
        SendEnvelopeKind::Pan => "pan",
        SendEnvelopeKind::Mute => "mute",
    }
}

const fn take_envelope_name(kind: TakeEnvelopeKind) -> &'static str {
    match kind {
        TakeEnvelopeKind::Volume => "volume",
        TakeEnvelopeKind::Pan => "pan",
        TakeEnvelopeKind::Mute => "mute",
        TakeEnvelopeKind::Pitch => "pitch",
    }
}

fn envelope_state(data: &EnvelopeData) -> EnvelopeState {
    EnvelopeState {
        mode: mode_name(data.automation_mode).to_owned(),
        visible: data.visible,
        armed: data.armed,
        points: data.points.iter().map(point_state).collect(),
        own_lane: data.in_own_lane,
        lane_height: data.lane_height,
        items: data
            .automation_items
            .iter()
            .map(|(item, points)| AutomationItemState {
                pool: item.pool_id,
                name: item.name.clone(),
                position: item.position.as_seconds(),
                length: item.length.as_seconds(),
                start_offset: item.start_offset.as_seconds(),
                playrate: item.play_rate,
                baseline: item.baseline,
                amplitude: item.amplitude,
                looped: item.loop_source,
                points: points.iter().map(point_state).collect(),
            })
            .collect(),
    }
}

fn point_state(p: &EnvelopePoint) -> EnvelopePointState {
    EnvelopePointState {
        at: p.time.as_seconds(),
        value: p.value,
        shape: shape_number(p.shape),
        tension: p.tension,
    }
}

fn envelope_data(e: &EnvelopeState) -> EnvelopeData {
    EnvelopeData {
        visible: e.visible,
        armed: e.armed,
        automation_mode: mode_of(&e.mode),
        in_own_lane: e.own_lane,
        lane_height: e.lane_height,
        points: proto_points(&e.points),
        automation_items: e
            .items
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    AutomationItem {
                        index: u32::try_from(i).unwrap_or(u32::MAX),
                        pool_id: a.pool,
                        name: a.name.clone(),
                        position: PositionInSeconds::from_seconds(a.position),
                        length: Duration::from_seconds(a.length),
                        start_offset: Duration::from_seconds(a.start_offset),
                        play_rate: a.playrate,
                        baseline: a.baseline,
                        amplitude: a.amplitude,
                        loop_source: a.looped,
                        selected: false,
                    },
                    proto_points(&a.points),
                )
            })
            .collect(),
    }
}

fn proto_points(points: &[EnvelopePointState]) -> Vec<EnvelopePoint> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| EnvelopePoint {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            time: PositionInSeconds::from_seconds(p.at),
            value: p.value,
            shape: shape_of(p.shape),
            tension: p.tension,
            selected: false,
        })
        .collect()
}

// ── numbers and names for the engine's enums ───────────────────────────

const fn stretch_mode_number(mode: daw_proto::StretchMode) -> u8 {
    use daw_proto::StretchMode as M;
    match mode {
        M::ProjectDefault => 0,
        M::Balanced => 1,
        M::Tonal => 2,
        M::Transient => 4,
        M::NoPreEcho => 5,
    }
}

const fn stretch_mode_of(n: u8) -> daw_proto::StretchMode {
    use daw_proto::StretchMode as M;
    match n {
        1 => M::Balanced,
        2 => M::Tonal,
        4 => M::Transient,
        5 => M::NoPreEcho,
        _ => M::ProjectDefault,
    }
}

const fn dimension_number(d: daw_proto::midi::NoteExpressionDim) -> u8 {
    use daw_proto::midi::NoteExpressionDim as D;
    match d {
        D::Volume => 0,
        D::Pan => 1,
        D::Tuning => 2,
        D::Vibrato => 3,
        D::Expression => 4,
        D::Brightness => 5,
        D::Pressure => 6,
    }
}

const fn dimension_of(n: u8) -> daw_proto::midi::NoteExpressionDim {
    use daw_proto::midi::NoteExpressionDim as D;
    match n {
        1 => D::Pan,
        2 => D::Tuning,
        3 => D::Vibrato,
        4 => D::Expression,
        5 => D::Brightness,
        6 => D::Pressure,
        _ => D::Volume,
    }
}

const fn record_mode_name(mode: daw_proto::RecordMode) -> &'static str {
    use daw_proto::RecordMode as R;
    match mode {
        R::Normal => "normal",
        R::TimeSelection => "time_selection",
        R::Item => "item",
    }
}

fn record_mode_of(name: &str) -> daw_proto::RecordMode {
    use daw_proto::RecordMode as R;
    match name {
        "time_selection" => R::TimeSelection,
        "item" => R::Item,
        _ => R::Normal,
    }
}
