//! What changed between two models, as the steps an engine takes to get
//! from one to the other.
//!
//! The doc side never needs this — [`crate::SessionDoc::write`] reconciles
//! field by field on its own. This is the other direction: a remote edit
//! arrives, the doc now reads differently from what the engine last
//! showed, and the engine has to be told exactly what moved.
//!
//! Steps come out in an order an engine can apply blindly: tracks are
//! created before anything is put on them, moved before they are edited,
//! and removed only after every item on them has gone.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::model::{
    ItemState, MarkerState, ProjectSettings, RegionState, SessionModel, TakeMarkerState,
    TempoPoint, TrackState,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A new track, at `index` among its folder's children.
    TrackAdded {
        track: TrackState,
        index: usize,
    },
    /// A track now sits in a different folder, or at a different place
    /// among its siblings.
    TrackMoved {
        guid: String,
        parent: Option<String>,
        index: usize,
    },
    TrackChanged {
        track: TrackState,
        fields: Vec<TrackField>,
    },
    TrackRemoved {
        guid: String,
    },
    ItemAdded {
        guid: String,
        item: ItemState,
    },
    ItemChanged {
        guid: String,
        item: ItemState,
        fields: Vec<ItemField>,
    },
    ItemRemoved {
        guid: String,
    },
    MarkerSet {
        guid: String,
        marker: MarkerState,
    },
    MarkerRemoved {
        guid: String,
    },
    RegionSet {
        guid: String,
        region: RegionState,
    },
    RegionRemoved {
        guid: String,
    },
    TempoChanged(Vec<TempoPoint>),
    ChartChanged(String),
    /// A recording's markers are now these, in time order — every one of
    /// them, since an engine replaces a recording's markers whole.
    TakeMarkersSet {
        source: String,
        markers: Vec<TakeMarkerState>,
    },
    /// The project's settings, the master bus or the monitoring chain
    /// changed: all of them as they are now.
    ProjectChanged(ProjectSettings),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackField {
    Name,
    Color,
    Volume,
    Pan,
    Muted,
    Soloed,
    PhaseInverted,
    ParentSend,
    VisibleInTcp,
    VisibleInMixer,
    /// Its fixed lanes: how many, their names, which play.
    Lanes,
    /// Its part in the track groups.
    Grouping,
    /// Its ext-state.
    Ext,
    /// Its FX chain: an FX added, removed, moved, bypassed or set.
    Fx,
    Sends,
    Envelopes,
    Armed,
    /// Its automation mode — which, set, every envelope on it takes.
    AutomationMode,
    /// Input monitoring.
    Monitor,
    RecordInput,
    /// How many channels it carries.
    Channels,
    /// Its panel's height.
    Height,
    /// Its mixer strip's width.
    Width,
    /// How its fixed lanes are drawn.
    LaneDisplay,
    /// Its comping: record and comp lanes, comp areas.
    Comping,
    /// Its ext-state under named sections.
    ExtSections,
    /// Its input FX chain.
    InputFx,
    /// Its hardware outputs.
    HwOutputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemField {
    Track,
    Position,
    Length,
    SnapOffset,
    Muted,
    Locked,
    Volume,
    FadeIn,
    FadeOut,
    FadeInShape,
    FadeOutShape,
    Label,
    Color,
    /// The active take's name, source, offset and rate.
    Take,
    FixedLane,
    Loop,
    /// The take's stretch markers.
    Stretch,
    /// The take's volume envelope (after [`Self::Take`]: a take's offset
    /// moves its envelope with its audio).
    TakeVolume,
    /// A MIDI take's notes.
    Notes,
    /// How it follows tempo, its auto-stretch, group and own offset.
    Timing,
    /// The active take's colour, gain, pitch, channel mode, source
    /// details and stretch mode.
    TakeSettings,
    /// The active take's envelopes other than volume.
    TakeEnvelopes,
    /// The active take's MIDI events other than notes.
    Midi,
    /// Its other takes, and which one is active.
    Takes,
}

/// The steps from `old` to `new`.
#[must_use]
pub fn diff(old: &SessionModel, new: &SessionModel) -> Vec<Change> {
    let mut out = Vec::new();
    let old_tracks: HashMap<&str, &TrackState> =
        old.tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let old_slots = sibling_slots(&old.tracks);
    let new_slots = sibling_slots(&new.tracks);

    for track in &new.tracks {
        let index = new_slots
            .get(track.guid.as_str())
            .copied()
            .unwrap_or_default();
        match old_tracks.get(track.guid.as_str()) {
            None => out.push(Change::TrackAdded {
                track: track.clone(),
                index,
            }),
            Some(before) => {
                if before.parent != track.parent
                    || old_slots.get(track.guid.as_str()) != Some(&index)
                {
                    out.push(Change::TrackMoved {
                        guid: track.guid.clone(),
                        parent: track.parent.clone(),
                        index,
                    });
                }
                let fields = track_fields(before, track);
                if !fields.is_empty() {
                    out.push(Change::TrackChanged {
                        track: track.clone(),
                        fields,
                    });
                }
            }
        }
    }

    for (guid, item) in &new.items {
        match old.items.get(guid) {
            None => out.push(Change::ItemAdded {
                guid: guid.clone(),
                item: item.clone(),
            }),
            Some(before) => {
                let fields = item_fields(before, item);
                if !fields.is_empty() {
                    out.push(Change::ItemChanged {
                        guid: guid.clone(),
                        item: item.clone(),
                        fields,
                    });
                }
            }
        }
    }
    for guid in old.items.keys().filter(|g| !new.items.contains_key(*g)) {
        out.push(Change::ItemRemoved { guid: guid.clone() });
    }

    for (guid, marker) in &new.markers {
        if old.markers.get(guid) != Some(marker) {
            out.push(Change::MarkerSet {
                guid: guid.clone(),
                marker: marker.clone(),
            });
        }
    }
    for guid in old.markers.keys().filter(|g| !new.markers.contains_key(*g)) {
        out.push(Change::MarkerRemoved { guid: guid.clone() });
    }
    for (guid, region) in &new.regions {
        if old.regions.get(guid) != Some(region) {
            out.push(Change::RegionSet {
                guid: guid.clone(),
                region: region.clone(),
            });
        }
    }
    for guid in old.regions.keys().filter(|g| !new.regions.contains_key(*g)) {
        out.push(Change::RegionRemoved { guid: guid.clone() });
    }

    if old.tempo != new.tempo {
        out.push(Change::TempoChanged(new.tempo.clone()));
    }
    if old.chart != new.chart {
        out.push(Change::ChartChanged(new.chart.clone()));
    }
    // After the items: a recording's markers are set through an item cut
    // from it, which has to be there.
    out.extend(take_markers(old, new));
    out.extend((old.project != new.project).then(|| Change::ProjectChanged(new.project.clone())));

    let kept: HashSet<&str> = new.tracks.iter().map(|t| t.guid.as_str()).collect();
    // Deepest first, so a folder goes after what it held.
    for track in old
        .tracks
        .iter()
        .rev()
        .filter(|t| !kept.contains(t.guid.as_str()))
    {
        out.push(Change::TrackRemoved {
            guid: track.guid.clone(),
        });
    }
    out
}

/// For each recording whose markers changed, all of its markers now.
fn take_markers(old: &SessionModel, new: &SessionModel) -> Vec<Change> {
    let changed: BTreeSet<&str> = new
        .take_markers
        .iter()
        .filter(|(key, m)| old.take_markers.get(*key) != Some(*m))
        .map(|(_, m)| m.source.as_str())
        .chain(
            old.take_markers
                .iter()
                .filter(|(key, _)| !new.take_markers.contains_key(*key))
                .map(|(_, m)| m.source.as_str()),
        )
        .collect();
    changed
        .into_iter()
        .map(|source| {
            let mut markers: Vec<TakeMarkerState> = new
                .take_markers
                .values()
                .filter(|m| m.source == source)
                .cloned()
                .collect();
            markers.sort_by(|a, b| a.at.total_cmp(&b.at));
            Change::TakeMarkersSet {
                source: source.to_owned(),
                markers,
            }
        })
        .collect()
}

/// Each track's index among its folder's children.
fn sibling_slots(tracks: &[TrackState]) -> HashMap<&str, usize> {
    let mut counts: HashMap<Option<&str>, usize> = HashMap::new();
    tracks
        .iter()
        .map(|t| {
            let slot = counts.entry(t.parent.as_deref()).or_insert(0);
            let index = *slot;
            *slot = slot.saturating_add(1);
            (t.guid.as_str(), index)
        })
        .collect()
}

#[allow(clippy::float_cmp)] // exact: a value the engine reported, read back
fn track_fields(a: &TrackState, b: &TrackState) -> Vec<TrackField> {
    let checks = [
        (a.name != b.name, TrackField::Name),
        (a.color != b.color, TrackField::Color),
        (a.volume != b.volume, TrackField::Volume),
        (a.pan != b.pan, TrackField::Pan),
        (a.muted != b.muted, TrackField::Muted),
        (a.soloed != b.soloed, TrackField::Soloed),
        (
            a.phase_inverted != b.phase_inverted,
            TrackField::PhaseInverted,
        ),
        (a.parent_send != b.parent_send, TrackField::ParentSend),
        (
            a.visible_in_tcp != b.visible_in_tcp,
            TrackField::VisibleInTcp,
        ),
        (
            a.visible_in_mixer != b.visible_in_mixer,
            TrackField::VisibleInMixer,
        ),
        (a.lanes != b.lanes, TrackField::Lanes),
        (a.grouping != b.grouping, TrackField::Grouping),
        (a.ext != b.ext, TrackField::Ext),
        (a.fx != b.fx, TrackField::Fx),
        (a.sends != b.sends, TrackField::Sends),
        (a.envelopes != b.envelopes, TrackField::Envelopes),
        (a.armed != b.armed, TrackField::Armed),
        (
            a.automation_mode != b.automation_mode,
            TrackField::AutomationMode,
        ),
        (a.monitor != b.monitor, TrackField::Monitor),
        (a.record_input != b.record_input, TrackField::RecordInput),
        (a.channels != b.channels, TrackField::Channels),
        (a.height != b.height, TrackField::Height),
        (a.width != b.width, TrackField::Width),
        (a.lane_display != b.lane_display, TrackField::LaneDisplay),
        (a.comping != b.comping, TrackField::Comping),
        (a.ext_sections != b.ext_sections, TrackField::ExtSections),
        (a.input_fx != b.input_fx, TrackField::InputFx),
        (a.hw_outputs != b.hw_outputs, TrackField::HwOutputs),
    ];
    checks
        .into_iter()
        .filter(|(changed, _)| *changed)
        .map(|(_, f)| f)
        .collect()
}

#[allow(clippy::float_cmp)] // exact: a value the engine reported, read back
/// What differs between two states of one item.
#[must_use]
pub fn item_fields(a: &ItemState, b: &ItemState) -> Vec<ItemField> {
    let checks = [
        (a.track != b.track, ItemField::Track),
        (a.position != b.position, ItemField::Position),
        (a.length != b.length, ItemField::Length),
        (a.snap_offset != b.snap_offset, ItemField::SnapOffset),
        (a.muted != b.muted, ItemField::Muted),
        (a.locked != b.locked, ItemField::Locked),
        (a.volume != b.volume, ItemField::Volume),
        (a.fade_in != b.fade_in, ItemField::FadeIn),
        (a.fade_out != b.fade_out, ItemField::FadeOut),
        (a.fade_in_shape != b.fade_in_shape, ItemField::FadeInShape),
        (
            a.fade_out_shape != b.fade_out_shape,
            ItemField::FadeOutShape,
        ),
        (a.label != b.label, ItemField::Label),
        (a.color != b.color, ItemField::Color),
        (
            a.take.name != b.take.name
                || a.take.source != b.take.source
                || a.take.start_offset != b.take.start_offset
                || a.take.playrate != b.take.playrate
                || a.take.midi != b.take.midi,
            ItemField::Take,
        ),
        (a.fixed_lane != b.fixed_lane, ItemField::FixedLane),
        (a.loop_source != b.loop_source, ItemField::Loop),
        (a.take.stretch != b.take.stretch, ItemField::Stretch),
        (a.take.volume != b.take.volume, ItemField::TakeVolume),
        (a.take.notes != b.take.notes, ItemField::Notes),
        (a.timing != b.timing, ItemField::Timing),
        (
            a.take.color != b.take.color
                || a.take.gain != b.take.gain
                || a.take.pitch != b.take.pitch
                || a.take.preserve_pitch != b.take.preserve_pitch
                || a.take.channel_mode != b.take.channel_mode
                || a.take.source_type != b.take.source_type
                || a.take.source_length != b.take.source_length
                || a.take.source_rate != b.take.source_rate
                || a.take.source_channels != b.take.source_channels
                || a.take.note_count != b.take.note_count
                || a.take.stretch_mode != b.take.stretch_mode,
            ItemField::TakeSettings,
        ),
        (
            a.take.envelopes != b.take.envelopes,
            ItemField::TakeEnvelopes,
        ),
        (a.take.midi_events != b.take.midi_events, ItemField::Midi),
        (
            a.other_takes != b.other_takes || a.active_take != b.active_take,
            ItemField::Takes,
        ),
    ];
    checks
        .into_iter()
        .filter(|(changed, _)| *changed)
        .map(|(_, f)| f)
        .collect()
}
