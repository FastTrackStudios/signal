//! Build a song's structure into a project from its keyflow chart.
//!
//! One call for what a chart can say about a session before anything is
//! recorded against it: the project tempo and meter, the SONG-lane region,
//! the COUNT-IN / SONGSTART / SONGEND / END markers, one coloured region
//! per section on the SECTIONS lane, and the Keyflow folder (KEY / CHORD /
//! LINES / HITS) the generator writes into.
//!
//! Generic over the `daw::service` traits, so it runs on REAPER and on the
//! in-process `daw-standalone` engine the Session window drives. The
//! pieces are the existing ones — [`chart_to_layout`] for the timeline,
//! [`stamp_song_with_default_tempo_native`] for tempo, markers and regions,
//! and the scaffold's folder builder — composed without the scaffold's
//! text-input dialog, which standalone has no UI for.
//!
//! The chart is laid out from zero: a leading count-in section starts at
//! 0 s and the song follows. Multitracks bounced from a click that starts
//! at zero line up with that without being moved.

use daw::service::transport::service::Transport as TransportService;
use daw::service::{
    ItemRef, Items, Markers, Midi, MidiNoteCreate, PositionConversion, PositionInSeconds,
    ProjectContext, Projects, Regions, TempoMap, TrackRef, Tracks,
};

use crate::setlist::chart_import::{ChartLayout, chart_to_layout};
use crate::setlist::service::demo::{
    chart_layout_to_demo_song, stamp_song_with_default_tempo_native,
};

/// What [`build_from_chart`] put into the project.
#[derive(Debug, Clone)]
pub struct ChartBuilt {
    pub title: String,
    pub tempo_bpm: f64,
    pub time_sig: (u32, u32),
    pub sections: usize,
    /// Where the song starts after the count-in, and where it ends.
    pub song_start_seconds: f64,
    pub song_end_seconds: f64,
}

/// The backend surface [`build_from_chart`] needs.
pub trait ChartDaw:
    Projects
    + TransportService
    + Markers
    + Regions
    + TempoMap
    + Tracks
    + Items
    + Midi
    + PositionConversion
{
}
impl<T> ChartDaw for T where
    T: Projects
        + TransportService
        + Markers
        + Regions
        + TempoMap
        + Tracks
        + Items
        + Midi
        + PositionConversion
{
}

/// Build the chart's structure into `project`.
///
/// Refuses a project that already has a `Keyflow` track: a second run
/// would stamp every marker and region twice. [`rebuild_from_chart`] is the
/// way to lay a changed chart over a built one.
///
/// The edit cursor is put back where it was — the marker inserts move it.
///
/// # Errors
///
/// The chart does not parse, the project already has its structure, or a
/// backend call fails.
pub fn build_from_chart<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
    chart_text: &str,
) -> eyre::Result<ChartBuilt> {
    let layout: ChartLayout = chart_to_layout(chart_text)?;
    if keyflow_folder(daw, project).is_some() {
        eyre::bail!("this project already has a Keyflow folder; its chart structure is built");
    }
    stamp_chart(daw, project, chart_text, &layout, "Build song from chart")
}

/// What [`rebuild_from_chart`] did: the new structure, and the span the
/// old one covered — so whatever was generated against the old song (the
/// click and guide) can be cleared past the new one's end.
#[derive(Debug, Clone)]
pub struct ChartRebuilt {
    pub built: ChartBuilt,
    /// From the old count-in to the old `=END`, when there was a song.
    pub replaced: Option<(f64, f64)>,
}

/// Lay the chart over the project again, replacing what a previous build
/// put there — the live half of editing a chart: every change to the text
/// re-runs this.
///
/// The chart OWNS, and so replaces: the SONG-lane region, the SECTIONS-lane
/// regions, the COUNT-IN / SONGSTART / SONGEND / =END markers, the project
/// tempo and meter, and the items on the Keyflow folder's KEY and CHORD
/// tracks. Everything else is left as it is — markers someone placed by
/// hand, the LINES and HITS tracks, every audio track. The Keyflow folder is
/// built if it is missing, so this also does a first build.
///
/// The chart is parsed before anything is touched: text that does not parse
/// (half-typed, mid-edit) changes nothing, and the session keeps the last
/// chart that did.
///
/// # Errors
///
/// The chart does not parse, or a backend call fails.
pub fn rebuild_from_chart<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
    chart_text: &str,
) -> eyre::Result<ChartRebuilt> {
    let layout: ChartLayout = chart_to_layout(chart_text)?;
    keyflow::text::chart::parse_chart(chart_text).map_err(|e| eyre::eyre!("chart: {e}"))?;
    let replaced = clear_chart_structure(daw, project)?;
    let built = stamp_chart(daw, project, chart_text, &layout, "Rebuild song from chart")?;
    Ok(ChartRebuilt { built, replaced })
}

/// The markers a chart build places, by name.
fn structural_marker(name: &str) -> bool {
    use crate::section_kinds::MarkerKind as K;
    [K::CountIn, K::SongStart, K::SongEnd, K::End]
        .iter()
        .any(|kind| name.trim().eq_ignore_ascii_case(kind.name()))
}

/// Remove what a chart build owns (see [`rebuild_from_chart`]). Returns the
/// span the removed song covered.
fn clear_chart_structure<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
) -> eyre::Result<Option<(f64, f64)>> {
    use session_proto::ruler_lanes::CoreLane;
    let owned_lanes = [CoreLane::Song.lane_index(), CoreLane::Sections.lane_index()];
    let mut span: Option<(f64, f64)> = None;
    let mut cover = |start: f64, end: f64| {
        span = Some(span.map_or((start, end), |(a, b)| (a.min(start), b.max(end))));
    };

    for marker in Markers::all(daw, project.clone()) {
        if structural_marker(&marker.name)
            && let Some(id) = marker.id
        {
            let at = marker.position.seconds().unwrap_or(0.0);
            cover(at, at);
            Markers::remove(daw, project.clone(), id)?;
        }
    }
    for region in Regions::all(daw, project.clone()) {
        if region.lane.is_some_and(|lane| owned_lanes.contains(&lane))
            && let Some(id) = region.id
        {
            cover(
                region.time_range.start_seconds(),
                region.time_range.end_seconds(),
            );
            Regions::remove(daw, project.clone(), id)?;
        }
    }
    // The tempo map is the chart's too: its meter changes, and whatever
    // tempo points a multitrack arrived with.
    let points = TempoMap::get_tempo_points(daw, project.clone()).len();
    for index in (0..points).rev() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        TempoMap::remove_tempo_point(daw, project.clone(), index)?;
    }
    for name in ["KEY", "CHORD"] {
        let Some(track) = keyflow_child(daw, project, name) else {
            continue;
        };
        for item in daw.get_items(project.clone(), TrackRef::Guid(track)) {
            daw.delete_item(project.clone(), ItemRef::Guid(item.guid.clone()))?;
        }
    }
    Ok(span)
}

/// Stamp tempo, markers, regions and the Keyflow folder's content, as one
/// undo step, putting the edit cursor back afterwards.
fn stamp_chart<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
    chart_text: &str,
    layout: &ChartLayout,
    undo: &str,
) -> eyre::Result<ChartBuilt> {
    let title = layout
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "Song".to_string());
    // `DemoSong` names its region with a `&'static str` (it was written for
    // fixtures). One short string per chart built is a leak worth not
    // widening that type for.
    let name: &'static str = Box::leak(title.clone().into_boxed_str());
    let song = chart_layout_to_demo_song(name, layout);

    let cursor = TransportService::get_position(daw, project.clone());
    daw.begin_undo_block(project.clone(), undo);
    // SONG / SECTIONS / MARKS, named and flagged, before anything lands
    // on them — what the insert actions do in REAPER.
    super::actions::ensure_core_lanes(daw);
    let stamped = stamp_song_with_default_tempo_native(daw, project, &song)
        .map_err(|e| eyre::eyre!("{e}"))
        .and_then(|_| stamp_meter_changes(daw, project, layout));
    let folder = stamped
        .and_then(|_| match keyflow_folder(daw, project) {
            Some(_) => Ok(()),
            None => super::scaffold::build_keyflow_folder(daw, project),
        })
        .and_then(|()| stamp_keyflow_tracks(daw, project, chart_text, layout));
    daw.end_undo_block(project.clone(), undo, None);
    let _ = TransportService::set_position(daw, project.clone(), cursor);
    folder?;

    Ok(ChartBuilt {
        title,
        tempo_bpm: layout.tempo_bpm,
        time_sig: (layout.time_sig_num, layout.time_sig_den),
        sections: layout.sections.len(),
        song_start_seconds: layout.song_start_seconds,
        song_end_seconds: layout.song_end_seconds,
    })
}

/// The chart's meter changes, as time-signature points on the tempo map —
/// at the chart's one tempo, so the tempo is unchanged and only the meter
/// moves: a bar of 2/4, and 4/4 again after it. The grid, the ruler and the
/// click all read the meter from here.
fn stamp_meter_changes<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
    layout: &ChartLayout,
) -> eyre::Result<()> {
    for &(seconds, num, den) in &layout.meter_changes {
        let index = daw.add_tempo_point(project.clone(), seconds, layout.tempo_bpm)?;
        daw.set_time_signature_at_point(
            project.clone(),
            index,
            i32::try_from(num).unwrap_or(4),
            i32::try_from(den).unwrap_or(4),
        )?;
    }
    Ok(())
}

/// The `Keyflow` folder track's GUID, when the project has one.
fn keyflow_folder<D: ChartDaw>(daw: &D, project: &ProjectContext) -> Option<String> {
    Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| t.name.trim().eq_ignore_ascii_case("Keyflow"))
        .map(|t| t.guid)
}

/// A track named `name` inside the Keyflow folder (KEY, CHORD, …).
fn keyflow_child<D: ChartDaw>(daw: &D, project: &ProjectContext, name: &str) -> Option<String> {
    let folder = keyflow_folder(daw, project)?;
    Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| {
            t.parent_guid.as_deref() == Some(folder.as_str())
                && t.name.trim().eq_ignore_ascii_case(name)
        })
        .map(|t| t.guid)
}

/// The Keyflow folder's content from the chart: the KEY track gets one
/// key item at the start (a label the rest of session reads back —
/// `crate::key`), and CHORD one MIDI item per chord, named with the
/// chord as written and holding its voicing. LINES and HITS stay empty;
/// the chart says nothing about them yet.
fn stamp_keyflow_tracks<D: ChartDaw>(
    daw: &D,
    project: &ProjectContext,
    chart_text: &str,
    layout: &ChartLayout,
) -> eyre::Result<()> {
    let chart =
        keyflow::text::chart::parse_chart(chart_text).map_err(|e| eyre::eyre!("chart: {e}"))?;
    if let Some(key) = &chart.initial_key {
        crate::key::set_key_at(daw, project.clone(), 0.0, key)?;
    }

    let Some(chord_track) = Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| t.name.trim().eq_ignore_ascii_case("CHORD") && t.folder_depth <= 0)
    else {
        return Ok(());
    };
    // The chart is laid out from zero at one tempo (`chart_to_layout`): a
    // beat is a quarter at `tempo_bpm`, and each measure starts where the
    // layout put it — its meter, not the header's, decides how long it is.
    let beat = 60.0 / layout.tempo_bpm.max(1.0);
    let qn = |seconds: f64| {
        daw.time_to_quarter_notes(project.clone(), PositionInSeconds::from_seconds(seconds))
            .quarter_notes
            .as_quarter_notes()
    };
    for chord in super::generate::voicings(&chart, CHORD_OCTAVE) {
        #[expect(clippy::cast_precision_loss, reason = "a bar count")]
        // Every chord's bar is one the layout placed; past them would be a
        // chord after the song, which lands at its end.
        let bar_start = layout
            .measure_starts
            .get(chord.measure)
            .copied()
            .unwrap_or(layout.song_end_seconds);
        let start = chord.beat.mul_add(beat, bar_start);
        let end = start + chord.beats * beat;
        let Some(location) = daw.create_midi_item(
            project.clone(),
            TrackRef::Guid(chord_track.guid.clone()),
            start,
            end,
        ) else {
            eyre::bail!("could not create a chord item at {start:.3} s");
        };
        let item: ItemRef = location.item.clone();
        let notes = chord
            .pitches
            .iter()
            .map(|&pitch| MidiNoteCreate {
                channel: 0,
                pitch,
                velocity: CHORD_VELOCITY,
                // A project quarter-note position, as the guide writes
                // them (see `Guide::note_create`); length in 960-PPQ ticks.
                start_ppq: qn(start),
                length_ppq: (chord.beats * 960.0).max(1.0),
            })
            .collect();
        daw.add_notes(location, notes);
        Items::set_label(daw, project.clone(), item, &chord.symbol)?;
    }
    Ok(())
}

/// Where the chord track's voicings sit: a reference to read, out of the
/// way of anything played.
const CHORD_OCTAVE: i32 = 4;
const CHORD_VELOCITY: u8 = 85;
