//! The song's timeline on the Action List: its tempo, the time selection,
//! and the sections and markers the Organize toolbar inserts.
//!
//! - `TEMPO_SET` sets the tempo at a bar or a time. While the song has no
//!   audio its markers, sections and the edit cursor keep their bars — the
//!   structure is musical before anything is recorded against the clock —
//!   and once it has, the caller says which (`follow`). The guide is
//!   generated again, so the click follows (below the guide's threshold
//!   its eighths come in by themselves).
//! - `TIME_SELECTION_SET` sets the time selection, in the engine and in
//!   the arrangement (`crate::cursor::ask`), or clears it.
//! - `SECTION_INSERT` and `MARKER_INSERT` are the toolbar's buttons
//!   (`crate::organize::insert`, one path for both): a section fills the
//!   time selection, or its default length from the edit cursor, and the
//!   guide is generated again with its cue.
//!
//! Bars are the song's (`crate::fx_actions::moment`): bar 1 is the one
//! SONGSTART falls in, the count-in before it bars −1 and −2.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::transport::service::Transport as _;
use daw::service::{ItemRef, ProjectContext, TakeRef, Takes as _, TempoMap as _};
use facet::Facet;
use session::section_kinds::{MarkerKind, SectionKind};

use crate::cursor::{Edit, Span};
use crate::organize::Button;

static TEMPO: ActionMeta = crate::actions::meta(
    "TEMPO_SET",
    "tempo_set",
    "Set the tempo",
    "Set the tempo to `bpm` at `at` — a bar (`33`, `33:2.5`) or a time (`12.5s`); the song's start when left out. While the song has no audio its markers, sections and the edit cursor keep their bars; once it has, say `follow: true` for that or `follow: false` to leave them at their times. The click and guide are generated again (below 75 bpm the click plays eighths).",
    "Tempo",
    "",
);

static SELECTION: ActionMeta = crate::actions::meta(
    "TIME_SELECTION_SET",
    "time_selection_set",
    "Set the time selection",
    "Select `from` to `to` — each a bar (`3`, `3:2.5`), a time (`12.5s`) or a section (`VS 1`, `VS 1 end`) — with the edit cursor at its start, as a drag across the ruler does. Both left out (or null) clears the selection.",
    "View",
    "",
);

static SECTION: ActionMeta = crate::actions::meta(
    "SECTION_INSERT",
    "section_insert",
    "Insert a section",
    "Insert a `kind` of section (intro, verse, pre-chorus, chorus, bridge, outro, instrumental, solo, hits, interlude, breakdown, vamp, refrain, tag, turnaround, ending) as the Organize toolbar's button does: over `from`..`to` when given (bars, times or sections as TIME_SELECTION_SET), else over the time selection, else its default length (intro and outro 4 bars, verse and chorus 8) from the edit cursor, up to the next section. Sections it overlaps are cut back. The edit cursor moves to its end, so the next goes after it; SONGEND moves out to it when it runs past; the guide is generated again with its cue. Returns the section.",
    "Structure",
    "",
);

static MARKER: ActionMeta = crate::actions::meta(
    "MARKER_INSERT",
    "marker_insert",
    "Insert a marker",
    "Insert a song marker — `kind`: count-in, start (=START), songstart, songend or end (=END) — at `at` (a bar, a time or a section, as TIME_SELECTION_SET) or the edit cursor, as the Organize toolbar's button does. A COUNT-IN moves the edit cursor two bars on, where SONGSTART goes. The guide is generated again. Says where it went.",
    "Structure",
    "",
);

/// A tempo.
#[derive(Facet)]
struct TempoArg {
    bpm: f64,
    at: Option<String>,
    follow: Option<bool>,
}

/// A span, or none.
#[derive(Facet)]
struct SelectionArg {
    from: Option<String>,
    to: Option<String>,
}

/// A section to insert.
#[derive(Facet)]
struct SectionArg {
    kind: String,
    from: Option<String>,
    to: Option<String>,
}

/// A marker to insert.
#[derive(Facet)]
struct MarkerArg {
    kind: String,
    at: Option<String>,
}

/// The slowest and fastest tempo taken.
const BPM: std::ops::RangeInclusive<f64> = 10.0..=400.0;

/// How close two times are to be one tempo point, seconds.
const SAME_POINT: f64 = 0.001;

/// The section kinds by what they are called, squashed (letters and
/// digits only, lower case).
const SECTIONS: &[(&str, SectionKind)] = &[
    ("intro", SectionKind::Intro),
    ("in", SectionKind::Intro),
    ("verse", SectionKind::Verse),
    ("vs", SectionKind::Verse),
    ("prechorus", SectionKind::PreChorus),
    ("pre", SectionKind::PreChorus),
    ("chorus", SectionKind::Chorus),
    ("ch", SectionKind::Chorus),
    ("bridge", SectionKind::Bridge),
    ("br", SectionKind::Bridge),
    ("outro", SectionKind::Outro),
    ("instrumental", SectionKind::Instrumental),
    ("solo", SectionKind::Solo),
    ("hits", SectionKind::Hits),
    ("interlude", SectionKind::Interlude),
    ("breakdown", SectionKind::Breakdown),
    ("vamp", SectionKind::Vamp),
    ("refrain", SectionKind::Refrain),
    ("tag", SectionKind::Tag),
    ("turnaround", SectionKind::Turnaround),
    ("ending", SectionKind::End),
    ("end", SectionKind::End),
];

/// The song markers by what they are called, squashed, and as written.
const MARKERS: &[(&str, MarkerKind, &str)] = &[
    ("countin", MarkerKind::CountIn, "COUNT-IN"),
    ("start", MarkerKind::Start, "=START"),
    ("songstart", MarkerKind::SongStart, "SONGSTART"),
    ("songend", MarkerKind::SongEnd, "SONGEND"),
    ("end", MarkerKind::End, "=END"),
];

/// `text` with only its letters and digits, lower case: `Pre-Chorus` and
/// `pre chorus` are one name.
fn squashed(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn section_kind(text: &str) -> Result<SectionKind, String> {
    let want = squashed(text);
    SECTIONS
        .iter()
        .find(|(name, _)| *name == want)
        .map(|(_, kind)| *kind)
        .ok_or_else(|| {
            format!(
                "no section kind {text}: one of {:?}",
                SECTIONS.iter().map(|(name, _)| *name).collect::<Vec<_>>()
            )
        })
}

fn marker_kind(text: &str) -> Result<(MarkerKind, &'static str), String> {
    let want = squashed(text);
    MARKERS
        .iter()
        .find(|(name, ..)| *name == want)
        .map(|(_, kind, written)| (*kind, *written))
        .ok_or_else(|| {
            format!(
                "no marker kind {text}: one of {:?}",
                MARKERS
                    .iter()
                    .map(|(.., written)| *written)
                    .collect::<Vec<_>>()
            )
        })
}

/// Where `text` is, seconds — a bar, a time or a section, as `FX_AUTOMATE`
/// reads them.
fn moment(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<f64, String> {
    crate::fx_actions::moment(daw, project, text)
}

/// `from`..`to`, both given or neither.
fn span(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Option<Span>, String> {
    match (from, to) {
        (None, None) => Ok(None),
        (Some(from), Some(to)) => {
            let span = Span::between(moment(daw, project, from)?, moment(daw, project, to)?);
            if span.is_meaningful() {
                Ok(Some(span))
            } else {
                Err(format!("{from}..{to} is no span"))
            }
        }
        _ => Err("a span is `from` and `to` together".to_owned()),
    }
}

/// Where the edit cursor and selection are: the arrangement's, as last
/// drawn or asked for; the engine's when there is no arrangement.
fn edit_now(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Edit {
    crate::cursor::current().unwrap_or_else(|| crate::organize::engine_edit(daw, project))
}

/// The click, count and cues generated again over the song as it is now.
fn regenerate_guide(daw: &daw::standalone::Standalone) {
    if let Err(e) = session::guide::Guide::new(daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(session::guide::GuideScope::All)
    {
        tracing::debug!(error = %e, "timeline: no guide to regenerate yet");
    }
}

static WHAT_IS_SELECTED: ActionMeta = crate::actions::meta(
    "SESSION_SELECTION",
    "selection",
    "What is selected",
    "What the person has selected in the window, to know exactly what they mean: the time selection (seconds, and bars as the ruler writes them, with the sections it overlaps), the edit cursor, the selected tracks (paths) and items (track, where, how long; the first 100), the kit's hits inside the time selection by drum, and the track regions it overlaps. Read-only.",
    "Session",
    "",
);

/// A stretch of time, said both ways.
#[derive(Facet)]
struct StretchOut {
    from: f64,
    to: f64,
    seconds: f64,
    /// Where it starts and ends, as the ruler writes them: `78.3 – 79.1`.
    bars: String,
    sections: Vec<String>,
}

/// A selected item.
#[derive(Facet)]
struct SelectedItem {
    guid: String,
    track: String,
    at: f64,
    length: f64,
}

/// One drum's hits in the time selection.
#[derive(Facet)]
struct HitsIn {
    piece: String,
    hits: u32,
}

/// A track region the time selection overlaps.
#[derive(Facet)]
struct RegionIn {
    track: String,
    label: String,
    bars: String,
}

/// What is selected.
#[derive(Facet)]
struct SelectionOut {
    time_selection: Option<StretchOut>,
    cursor: f64,
    cursor_bar: String,
    tracks: Vec<String>,
    items: Vec<SelectedItem>,
    items_selected: u32,
    hits: Vec<HitsIn>,
    regions: Vec<RegionIn>,
}

/// The most selected items said one by one.
const MOST_ITEMS: usize = 100;

fn selection(daw: &daw::standalone::Standalone, project: &ProjectContext) -> SelectionOut {
    use daw::service::{Items as _, Tracks};
    let edit = edit_now(daw, project);
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let path = |guid: &str| {
        by_guid
            .get(guid)
            .map(|t| crate::gain_stage::path_of(t, &by_guid).join("/"))
            .unwrap_or_default()
    };
    let span = edit.selection.map(|sel| (sel.start, sel.end));
    let time_selection = span.map(|(from, to)| StretchOut {
        from,
        to,
        seconds: to - from,
        bars: format!(
            "{} – {}",
            crate::track_regions::bar_of(daw, project, from),
            crate::track_regions::bar_of(daw, project, to)
        ),
        sections: crate::actions::sections(daw, project)
            .into_iter()
            .filter(|s| s.start < to && s.end > from)
            .map(|s| s.name)
            .collect(),
    });
    let selected: Vec<daw_proto::Item> = daw
        .get_all_items(project.clone())
        .into_iter()
        .filter(|i| i.selected)
        .collect();
    let items = selected
        .iter()
        .take(MOST_ITEMS)
        .map(|i| SelectedItem {
            guid: i.guid.clone(),
            track: path(&i.track_guid),
            at: i.position.as_seconds(),
            length: i.length.as_seconds(),
        })
        .collect();
    let mut hits: Vec<HitsIn> = Vec::new();
    let mut regions = Vec::new();
    if let Some((from, to)) = span {
        for hit in crate::hits::playback(daw, project)
            .iter()
            .filter(|h| h.time >= from && h.time < to)
        {
            match hits.iter_mut().find(|h| h.piece == hit.piece) {
                Some(h) => h.hits = h.hits.saturating_add(1),
                None => hits.push(HitsIn {
                    piece: hit.piece.clone(),
                    hits: 1,
                }),
            }
        }
        for (track, marks) in crate::track_regions::all(daw, project) {
            for r in marks
                .regions
                .iter()
                .filter(|r| r.start < to && r.end > from)
            {
                regions.push(RegionIn {
                    track: path(&track),
                    label: r.label(),
                    bars: format!(
                        "{} – {}",
                        crate::track_regions::bar_of(daw, project, r.start),
                        crate::track_regions::bar_of(daw, project, r.end)
                    ),
                });
            }
        }
    }
    SelectionOut {
        time_selection,
        cursor: edit.at,
        cursor_bar: crate::track_regions::bar_of(daw, project, edit.at),
        tracks: tracks
            .iter()
            .filter(|t| t.selected)
            .map(|t| path(&t.guid))
            .collect(),
        items,
        items_selected: u32::try_from(selected.len()).unwrap_or(u32::MAX),
        hits,
        regions,
    }
}

pub fn register(list: &ActionList) {
    list.register(&WHAT_IS_SELECTED, Effect::ReadOnly, |(): ()| {
        let (daw, project) = crate::actions::song()?;
        Ok(selection(daw, &project))
    });
    list.register(&TEMPO, Effect::Mutating, |a: TempoArg| {
        if !BPM.contains(&a.bpm) {
            return Err(format!(
                "{} bpm is not a tempo: {} to {}",
                a.bpm,
                BPM.start(),
                BPM.end()
            ));
        }
        let (daw, project) = crate::actions::song()?;
        let at = a
            .at
            .as_deref()
            .map(|text| moment(daw, &project, text))
            .transpose()?
            .unwrap_or(0.0);
        let follow = match (a.follow, has_audio(daw, &project)) {
            (Some(follow), _) => follow,
            (None, false) => true,
            (None, true) => {
                return Err(
                    "the song has audio, recorded against the clock: say `follow: true` to keep the markers and sections on their bars (they move in time), or `follow: false` to leave them where they are"
                        .to_owned(),
                );
            }
        };
        // Not an undo step: the engine's undo keeps tracks and items, not
        // the tempo map or the ruler.
        set_tempo(daw, &project, a.bpm, at, follow)?;
        regenerate_guide(daw);
        crate::studio::request_resync();
        crate::actions::list().changed("tempo", &format!("{} bpm", a.bpm));
        Ok(format!(
            "{} bpm from {at:.3} s{}; the guide follows",
            a.bpm,
            if follow {
                ", the markers and sections on their bars"
            } else {
                ""
            }
        ))
    });
    list.register(&SELECTION, Effect::Mutating, |a: SelectionArg| {
        let (daw, project) = crate::actions::song()?;
        let Some(chosen) = span(daw, &project, a.from.as_deref(), a.to.as_deref())? else {
            daw.clear_time_selection(project.clone())
                .map_err(|e| e.to_string())?;
            let at = edit_now(daw, &project).at;
            crate::cursor::ask(Edit {
                at,
                selection: None,
            });
            return Ok("no time selection".to_owned());
        };
        daw.set_time_selection(project.clone(), chosen.start, chosen.end)
            .and_then(|()| daw.set_position(project.clone(), chosen.start))
            .map_err(|e| e.to_string())?;
        crate::cursor::ask(Edit {
            at: chosen.start,
            selection: Some(chosen),
        });
        Ok(format!(
            "{:.3} s to {:.3} s selected, the edit cursor at its start",
            chosen.start, chosen.end
        ))
    });
    list.register(&SECTION, Effect::Mutating, |a: SectionArg| {
        let kind = section_kind(&a.kind)?;
        let (daw, project) = crate::actions::song()?;
        let edit = span(daw, &project, a.from.as_deref(), a.to.as_deref())?.map_or_else(
            || edit_now(daw, &project),
            |over| Edit {
                at: over.start,
                selection: Some(over),
            },
        );
        crate::organize::insert(daw, Button::Section(kind), false, Some(edit));
        crate::studio::request_resync();
        crate::actions::list().changed("markers", &a.kind);
        // The section ends where the insert left the cursor.
        let end = crate::organize::engine_edit(daw, &project).at;
        crate::actions::sections(daw, &project)
            .into_iter()
            .find(|s| (s.end - end).abs() < SAME_POINT)
            .ok_or_else(|| format!("no {} was inserted at {:.3} s", a.kind, edit.at))
    });
    list.register(&MARKER, Effect::Mutating, |a: MarkerArg| {
        let (kind, written) = marker_kind(&a.kind)?;
        let (daw, project) = crate::actions::song()?;
        let mut edit = edit_now(daw, &project);
        if let Some(at) = a.at.as_deref() {
            edit.at = moment(daw, &project, at)?;
        }
        crate::organize::insert(daw, Button::Marker(kind), false, Some(edit));
        crate::studio::request_resync();
        crate::actions::list().changed("markers", written);
        Ok(format!("{written} at {:.3} s", edit.at))
    });
}

/// Whether anything in the song is audio — a take recorded or imported
/// against the clock, which a tempo change cannot move without moving
/// what it plays. The guide's MIDI is not.
fn has_audio(daw: &daw::standalone::Standalone, project: &ProjectContext) -> bool {
    let audio = |item: &daw_proto::Item| {
        let source = daw.get_source_type(
            project.clone(),
            ItemRef::Guid(item.guid.clone()),
            TakeRef::Active,
        );
        item.take_count > 0
            && matches!(
                source,
                daw_proto::SourceType::Audio | daw_proto::SourceType::Video
            )
    };
    daw::service::Items::get_all_items(daw, project.clone())
        .iter()
        .any(audio)
}

/// A moment as the tempo map counts it: measure, beat, fraction of a beat.
type Musical = (i32, i32, f64);

/// `bpm` from `at`: the tempo point there set, the song's own tempo when
/// `at` is its start and there is none, else a point added. With
/// `follow`, the markers, sections and edit cursor go back onto the bars
/// they were on.
fn set_tempo(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    bpm: f64,
    at: f64,
    follow: bool,
) -> Result<(), String> {
    let musical = |seconds: f64| daw.time_to_musical(project.clone(), seconds);
    let marks: Vec<(u32, Musical)> = daw::service::Markers::all(daw, project.clone())
        .into_iter()
        .filter_map(|m| Some((m.id?, musical(m.position.seconds()?))))
        .collect();
    let spans: Vec<(u32, Musical, Musical)> = daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .filter_map(|r| {
            Some((
                r.id?,
                musical(r.time_range.start_seconds()),
                musical(r.time_range.end_seconds()),
            ))
        })
        .collect();
    let edit = edit_now(daw, project);
    let cursor = (
        musical(edit.at),
        edit.selection.map(|s| (musical(s.start), musical(s.end))),
    );

    let points = daw.get_tempo_points(project.clone());
    let here = points.iter().position(|p| {
        p.position
            .seconds()
            .is_some_and(|s| (s - at).abs() < SAME_POINT)
    });
    match here {
        Some(index) => {
            let index = u32::try_from(index).map_err(|e| e.to_string())?;
            daw.set_tempo_at_point(project.clone(), index, bpm)
        }
        None if at < SAME_POINT => daw.set_default_tempo(project.clone(), bpm),
        None => daw.add_tempo_point(project.clone(), at, bpm).map(|_| ()),
    }
    .map_err(|e| format!("the tempo was refused: {e}"))?;
    if !follow {
        return Ok(());
    }

    let time = |(measure, beat, fraction): Musical| {
        daw.musical_to_time(project.clone(), measure, beat, fraction)
    };
    for (id, bar) in marks {
        daw::service::Markers::set_position(daw, project.clone(), id, time(bar))
            .map_err(|e| format!("a marker did not keep its bar: {e}"))?;
    }
    for (id, start, end) in spans {
        daw::service::Regions::set_bounds(daw, project.clone(), id, time(start), time(end))
            .map_err(|e| format!("a section did not keep its bars: {e}"))?;
    }
    let (cursor_at, selection) = cursor;
    let edit = Edit {
        at: time(cursor_at),
        selection: selection.map(|(start, end)| Span::between(time(start), time(end))),
    };
    daw.set_position(project.clone(), edit.at)
        .map_err(|e| e.to_string())?;
    if let Some(span) = edit.selection {
        daw.set_time_selection(project.clone(), span.start, span.end)
            .map_err(|e| e.to_string())?;
    }
    crate::cursor::ask(edit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind is named however it is written.
    #[test]
    fn kinds_are_named_loosely() {
        assert_eq!(section_kind("Pre-Chorus"), Ok(SectionKind::PreChorus));
        assert_eq!(section_kind(" chorus "), Ok(SectionKind::Chorus));
        assert_eq!(section_kind("IN"), Ok(SectionKind::Intro));
        assert!(section_kind("coda please").is_err());
        assert_eq!(marker_kind("Count-In").map(|(_, w)| w), Ok("COUNT-IN"));
        assert_eq!(marker_kind("=END").map(|(_, w)| w), Ok("=END"));
        assert_eq!(marker_kind("SONGSTART").map(|(_, w)| w), Ok("SONGSTART"));
    }
}
