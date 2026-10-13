//! Organize mode's toolbar — the song's structure, placed from above the
//! arrangement.
//!
//! The same buttons, in the same order, as REAPER's "Organize 1" toolbar
//! (`nix/reaper-config/reaper-menu.ini`): the structural markers and the
//! section regions, then the time signatures. Each runs the session's own
//! action, so REAPER and this window insert the same thing:
//!
//! - a **section** fills the arrangement's time selection, or — with none —
//!   its default length from the edit cursor
//!   (`session::keyflow::actions`, `infer_insert_bounds`);
//! - a **marker** goes at the edit cursor;
//! - a **time signature** starts at the measure holding the edit cursor;
//!   with Shift held it lasts that one measure and the signature before it
//!   comes back after (`session::keyflow::time_signature`).
//!
//! The actions read the ENGINE's cursor and selection, and the arrangement
//! keeps its own, so each press hands the arrangement's over first
//! ([`crate::cursor::current`]). Then the guide is regenerated — its cues
//! follow the sections — and the arrangement reads the session back.
//!
//! In Remote and Cue (`crate::audio_mode`) the same press goes through
//! the facade to the system this window drives: the cursor and selection
//! by its transport, the marker or section by running the session's own
//! REAPER action (`FTS_SESSION_INSERT_*`, the ids the FTS extension
//! registers — [`command_id`]), the time signature on its tempo map, and
//! the guide by `FTS_SESSION_GUIDE_GENERATE_GUIDE_TRACKS`.

use dioxus::prelude::*;
use session::section_kinds::{KeyflowAction, MarkerKind, SectionKind};

use crate::shell::{DIM, RULE, TEXT};

/// One button of the toolbar — what [`insert`] puts on the timeline.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Button {
    Marker(MarkerKind),
    Section(SectionKind),
    TimeSig(i32, i32),
}

/// The structure buttons, as the REAPER toolbar has them.
const STRUCTURE: [(Button, &str); 12] = [
    (Button::Marker(MarkerKind::CountIn), "Count-In"),
    (Button::Marker(MarkerKind::Start), "=START"),
    (Button::Marker(MarkerKind::SongStart), "SONGSTART"),
    (Button::Section(SectionKind::Intro), "INTRO"),
    (Button::Section(SectionKind::Verse), "VERSE"),
    (Button::Section(SectionKind::PreChorus), "PRE-CH"),
    (Button::Section(SectionKind::Chorus), "CHORUS"),
    (Button::Section(SectionKind::Bridge), "BRIDGE"),
    (Button::Section(SectionKind::Outro), "OUTRO"),
    (Button::Section(SectionKind::End), "ENDING"),
    (Button::Marker(MarkerKind::SongEnd), "SONGEND"),
    (Button::Marker(MarkerKind::End), "=END"),
];

impl Button {
    fn color(self) -> String {
        match self {
            Self::Marker(kind) => kind.css_color(),
            Self::Section(kind) => kind.css_color(),
            Self::TimeSig(..) => DIM.to_owned(),
        }
    }

    fn title(self) -> String {
        match self {
            Self::Marker(kind) => format!("Insert a {kind:?} marker at the edit cursor"),
            Self::Section(kind) => format!(
                "Insert a {kind:?} region over the time selection — or, with none, its default length from the edit cursor"
            ),
            Self::TimeSig(num, den) => format!(
                "Insert {num}/{den} at the measure under the edit cursor — Shift: for that one measure only"
            ),
        }
    }
}

/// The toolbar: the structure, a gap, the time signatures.
#[component]
pub fn OrganizeToolbar() -> Element {
    let signatures = session::keyflow::time_signature::TIME_SIGNATURES;
    rsx! {
        div {
            // One row, always: what does not fit scrolls sideways rather
            // than wrapping the toolbar taller over the arrangement.
            style: "flex:none; display:flex; flex-wrap:nowrap; align-items:center; gap:4px; \
                    height:36px; padding:0 8px; overflow-x:auto; overflow-y:hidden; \
                    background:#141518; border-bottom:1px solid {RULE};",
            for (button, label) in STRUCTURE {
                ToolButton { button, label: label.to_owned() }
            }
            div { style: "width:14px; flex:none;" }
            for &(num, den) in signatures {
                ToolButton { button: Button::TimeSig(num, den), label: format!("{num}/{den}") }
            }
        }
    }
}

#[component]
fn ToolButton(button: Button, label: String) -> Element {
    let color = button.color();
    rsx! {
        button {
            title: button.title(),
            style: "flex:none; height:24px; padding:0 9px; border-radius:5px; border:1px solid {RULE}; \
                    border-left:3px solid {color}; background:#1c1e22; color:{TEXT}; \
                    font-size:11px; font-weight:600; cursor:pointer; white-space:nowrap;",
            onclick: move |event| press(button, event.modifiers().shift()),
            "{label}"
        }
    }
}

/// Run a button's action on the engine, as the arrangement has the cursor
/// and selection, then bring the guide and the arrangement up to date.
fn press(button: Button, shift: bool) {
    if let Some(daw) = crate::open::local_engine() {
        insert(daw, button, shift, crate::cursor::current());
    } else if let Err(e) = press_remote(button, shift) {
        tracing::warn!(error = %e, ?button, "organize: the remote refused the insert");
    }
    crate::studio::request_resync();
}

/// Insert `button`'s marker, section or time signature, then bring the
/// guide up to date.
///
/// On the engine's current song at `edit` (the edit cursor and time selection, handed to
/// the engine first; the engine's own when `None`), then bring the guide
/// up to date — the toolbar's press, and `SECTION_INSERT` /
/// `MARKER_INSERT` on the Action List, one path.
///
/// A section past the song's end moves SONGEND out to it, so the guide
/// runs over it. The engine's cursor and selection afterwards (a section
/// moves the cursor to its end and uses up the selection) go back to the
/// arrangement.
pub fn insert(
    daw: &daw::standalone::Standalone,
    button: Button,
    shift: bool,
    edit: Option<crate::cursor::Edit>,
) {
    use daw::service::ProjectContext;
    use daw::service::transport::service::Transport as _;
    let project = ProjectContext::Current;
    if let Some(edit) = edit {
        let handed = match edit.selection {
            Some(span) => daw.set_time_selection(project.clone(), span.start, span.end),
            None => daw.clear_time_selection(project.clone()),
        };
        if let Err(e) = handed.and_then(|()| daw.set_position(project.clone(), edit.at)) {
            tracing::warn!(error = %e, "organize: the edit cursor could not be handed to the engine");
        }
    }
    match button {
        Button::Marker(kind) => {
            session::keyflow::actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
        }
        Button::Section(kind) => {
            session::keyflow::actions::dispatch(daw, KeyflowAction::InsertSection(kind));
            reach_song_end(daw, &project);
        }
        Button::TimeSig(num, den) => {
            if let Err(e) =
                session::keyflow::time_signature::insert_time_signature(daw, num, den, shift)
            {
                tracing::warn!(error = %e, num, den, "organize: the time signature was refused");
            }
        }
    }
    crate::cursor::ask(engine_edit(daw, &project));
    // The cues follow the sections; the click follows the meter.
    if let Err(e) = session::guide::Guide::new(daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(session::guide::GuideScope::All)
    {
        tracing::debug!(error = %e, "organize: no guide to regenerate yet");
    }
}

/// The engine's edit cursor and time selection on `project`, as the
/// arrangement keeps them.
#[must_use]
pub fn engine_edit(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
) -> crate::cursor::Edit {
    use daw::service::transport::service::Transport as _;
    let at = daw
        .get_state(project.clone())
        .edit_position
        .time
        .map_or_else(
            || daw.get_position(project.clone()),
            |position| position.as_seconds(),
        );
    let selection = daw
        .get_time_selection(project.clone())
        .map(|span| crate::cursor::Span::between(span.start_seconds, span.end_seconds))
        .filter(|span| span.is_meaningful());
    crate::cursor::Edit { at, selection }
}

/// The song's end moved out to its last section's, when a section now
/// runs past it: SONGEND, the `=END` after it and the SONG region. The
/// guide is laid over the song to SONGEND, so a section inserted after it
/// would otherwise have no click.
fn reach_song_end(daw: &daw::standalone::Standalone, project: &daw::service::ProjectContext) {
    use daw::service::{Markers, Regions};
    let song_lane = session::ruler_lanes::CoreLane::Song.lane_index();
    let last = crate::actions::sections(daw, project)
        .iter()
        .map(|s| s.end)
        .fold(0.0_f64, f64::max);
    let short = |at: f64| at < last - 1e-6;
    for marker in Markers::all(daw, project.clone()) {
        let (Some(id), Some(at)) = (marker.id, marker.position.seconds()) else {
            continue;
        };
        let name = marker.name.trim();
        if (name.eq_ignore_ascii_case("SONGEND") || name == "=END")
            && short(at)
            && let Err(e) = Markers::set_position(daw, project.clone(), id, last)
        {
            tracing::warn!(error = %e, marker = name, "organize: the song's end did not move");
        }
    }
    for region in Regions::all(daw, project.clone()) {
        let (Some(id), Some(lane)) = (region.id, region.lane) else {
            continue;
        };
        let (start, end) = (
            region.time_range.start_seconds(),
            region.time_range.end_seconds(),
        );
        if lane == song_lane
            && short(end)
            && let Err(e) = Regions::set_bounds(daw, project.clone(), id, start, last)
        {
            tracing::warn!(error = %e, "organize: the SONG region did not grow");
        }
    }
}

/// The session's guide action as REAPER knows it
/// (`session_proto::guide::GuideActions::generate_guide_tracks`).
const GENERATE_GUIDE: &str = "FTS_SESSION_GUIDE_GENERATE_GUIDE_TRACKS";

/// The REAPER command id of a marker or section button — the session's own
/// action (`session_proto::keyflow_actions::KeyflowActions`), which runs
/// the same `dispatch` the local press does. `None` for a time signature
/// (there is no action for one; the remote press writes it itself) and
/// for the sections with no action.
#[must_use]
fn command_id(button: Button) -> Option<&'static str> {
    Some(match button {
        Button::Marker(kind) => match kind {
            MarkerKind::CountIn => "FTS_SESSION_INSERT_COUNT_IN_MARKER",
            MarkerKind::Start => "FTS_SESSION_INSERT_START_MARKER",
            MarkerKind::End => "FTS_SESSION_INSERT_END_MARKER",
            MarkerKind::SongStart => "FTS_SESSION_INSERT_SONGSTART_MARKER",
            MarkerKind::SongEnd => "FTS_SESSION_INSERT_SONGEND_MARKER",
        },
        Button::Section(kind) => match kind {
            SectionKind::Intro => "FTS_SESSION_INSERT_INTRO_REGION",
            SectionKind::Verse => "FTS_SESSION_INSERT_VERSE_REGION",
            SectionKind::PreChorus => "FTS_SESSION_INSERT_PRE_CHORUS_REGION",
            SectionKind::Chorus => "FTS_SESSION_INSERT_CHORUS_REGION",
            SectionKind::Bridge => "FTS_SESSION_INSERT_BRIDGE_REGION",
            SectionKind::Outro => "FTS_SESSION_INSERT_OUTRO_REGION",
            SectionKind::Instrumental => "FTS_SESSION_INSERT_INSTRUMENTAL_REGION",
            SectionKind::Solo => "FTS_SESSION_INSERT_SOLO_REGION",
            SectionKind::Hits => "FTS_SESSION_INSERT_HITS_REGION",
            SectionKind::Interlude => "FTS_SESSION_INSERT_INTERLUDE_REGION",
            SectionKind::Breakdown => "FTS_SESSION_INSERT_BREAKDOWN_REGION",
            SectionKind::Vamp => "FTS_SESSION_INSERT_VAMP_REGION",
            SectionKind::CountIn => "FTS_SESSION_INSERT_COUNT_IN_REGION",
            SectionKind::End => "FTS_SESSION_INSERT_END_REGION",
            SectionKind::Refrain | SectionKind::Tag | SectionKind::Turnaround => return None,
        },
        Button::TimeSig(..) => return None,
    })
}

/// The press, through the facade, on the system this window drives.
fn press_remote(button: Button, shift: bool) -> eyre::Result<()> {
    let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
    let edit = crate::cursor::current();
    crate::open::block_on_engine(async move {
        let project = daw.current_project().await?;
        let transport = project.transport();
        if let Some(edit) = edit {
            match edit.selection {
                Some(span) => transport.set_time_selection(span.start, span.end).await?,
                None => transport.clear_time_selection().await?,
            }
            transport.set_position(edit.at).await?;
        }
        match button {
            Button::TimeSig(num, den) => {
                insert_time_signature_remote(&project, num, den, shift).await?
            }
            _ => {
                let id = command_id(button)
                    .ok_or_else(|| eyre::eyre!("{button:?} has no session action"))?;
                if !project.run_command(id).await? {
                    eyre::bail!(
                        "the remote does not know {id} — is the FTS session extension loaded?"
                    );
                }
            }
        }
        // The cues follow the sections; the click follows the meter.
        if !project.run_command(GENERATE_GUIDE).await.unwrap_or(false) {
            tracing::debug!("organize: the remote regenerated no guide");
        }
        Ok(())
    })
    .ok_or_else(|| eyre::eyre!("the engine runtime is not up"))?
}

/// [`session::keyflow::time_signature::insert_time_signature`], through
/// the facade: `num/den` from the measure holding the edit cursor — for
/// that one measure with `single_measure`, the signature before it coming
/// back on the next downbeat unless the project already changes there.
async fn insert_time_signature_remote(
    project: &daw_control::Project,
    num: i32,
    den: i32,
    single_measure: bool,
) -> eyre::Result<()> {
    let tempo = project.tempo_map();
    let transport = project.transport();
    let position = match transport.get_state().await?.edit_position.time {
        Some(at) => at.as_seconds(),
        None => transport.get_position().await?,
    };
    let (measure, ..) = tempo.time_to_musical(position).await?;
    let before = tempo.time_signature_at(position).await?;
    let start = tempo.musical_to_time(measure, 0, 0.0).await?;
    upsert_signature_remote(&tempo, start, num, den).await?;
    if single_measure && before != (num, den) {
        let next = tempo
            .musical_to_time(measure.saturating_add(1), 0, 0.0)
            .await?;
        let already = tempo.points().await?.iter().any(|p| {
            p.time_signature.is_some()
                && p.position
                    .seconds()
                    .is_some_and(|s| (s - next).abs() < 1e-6)
        });
        if !already {
            upsert_signature_remote(&tempo, next, before.0, before.1).await?;
        }
    }
    Ok(())
}

/// A signature on the tempo point at `seconds`, making one (at the tempo
/// there) when there is none.
async fn upsert_signature_remote(
    tempo: &daw_control::TempoMap,
    seconds: f64,
    num: i32,
    den: i32,
) -> eyre::Result<()> {
    let points = tempo.points().await?;
    let existing = points.iter().position(|p| {
        p.position
            .seconds()
            .is_some_and(|s| (s - seconds).abs() < 1e-6)
    });
    let index = match existing {
        Some(index) => u32::try_from(index)?,
        None => {
            let bpm = tempo.tempo_at(seconds).await?;
            tempo.add_point(seconds, bpm).await?
        }
    };
    tempo.set_time_signature_at(index, num, den).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every button a remote press runs by id names an action the session
    /// actually registers with REAPER.
    #[test]
    fn every_structure_button_is_a_registered_session_action() {
        let registered: Vec<&str> = session::keyflow_actions::KeyflowActionsActions::all()
            .iter()
            .map(|m| m.id)
            .collect();
        for (button, label) in STRUCTURE {
            let id = command_id(button).unwrap_or_else(|| panic!("{label} has no command id"));
            assert!(
                registered.contains(&id),
                "{label}: {id} is not a registered session action"
            );
        }
        assert_eq!(
            command_id(Button::TimeSig(4, 4)),
            None,
            "a signature is written, not run"
        );
    }
}
