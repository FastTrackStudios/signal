//! New Project: a song from nothing (`SESSION_NEW`).
//!
//! An untitled song in the window's own engine, with what every song
//! starts from already placed: COUNT-IN on bar 1, SONGSTART after the
//! two-bar count-in keyflow's insert gives one, SONGEND [`SONG_BARS`] bars
//! on, and the guide (click, count, cues) generated over them. The window
//! is asked into Organize mode, where the structure and the instruments
//! go in.
//!
//! The song has no file until it is first saved: `SESSION_SAVE` then needs
//! a `to`, and the stem of `to` becomes its name ([`named`]).
//!
//! The start screen's New Project makes it and opens it as a set of one
//! ([`setlist`]). The action, run from inside another song, makes it in
//! the engine at once — so a model can go straight on — and asks the
//! window back out to the start screen, which opens what was made
//! ([`take_made`]).

use std::sync::Mutex;

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::transport::service::Transport as _;
use daw::service::{ProjectContext, TempoMap as _};
use facet::Facet;
use session::section_kinds::{KeyflowAction, MarkerKind};

/// What a new song is called until it is saved.
pub const UNTITLED: &str = "Untitled";

/// The count-in's bars before SONGSTART — keyflow's, as its COUNT-IN
/// insert moves the cursor on by.
const COUNT_IN_BARS: i32 = 2;

/// The bars from SONGSTART to SONGEND in a new song: something for the
/// click to run over. A section inserted past them moves SONGEND out
/// (`crate::organize::insert`).
const SONG_BARS: i32 = 8;

static NEW: ActionMeta = crate::actions::meta(
    "SESSION_NEW",
    "session_new",
    "New project",
    "Start a new song from nothing, untitled: COUNT-IN on bar 1, SONGSTART after the two-bar count-in, SONGEND eight bars on, the guide (Click, Count, Guide) generated over them, the window in Organize mode. The song open now is left as it is — SESSION_SAVE it first if it should be. The new one has no file until its first SESSION_SAVE, which needs `to` and takes its name from it. `template` is not there yet.",
    "Session",
    "",
);

/// A new song.
#[derive(Facet)]
struct NewArg {
    /// A template to start from — none yet.
    template: Option<String>,
}

pub fn register(list: &ActionList) {
    list.register(&NEW, Effect::Mutating, |a: NewArg| {
        if let Some(template) = a.template.filter(|t| !t.trim().is_empty()) {
            return Err(format!(
                "no templates yet ({template}): leave `template` out for an empty song"
            ));
        }
        let guid = create(None)?;
        if let Ok(mut made) = MADE.lock() {
            *made = Some(guid);
        }
        // Out of the song on screen; the start screen opens the new one.
        crate::projects::go_back();
        Ok(format!(
            "a new song, {UNTITLED}, in Organize: COUNT-IN, SONGSTART and SONGEND placed and the guide over them. It is named when it is first saved (SESSION_SAVE with `to`)."
        ))
    });
}

/// The song `SESSION_NEW` made, until the start screen opens it.
static MADE: Mutex<Option<String>> = Mutex::new(None);

/// The song `SESSION_NEW` made (its project GUID), for the start screen
/// to open — once.
#[must_use]
pub fn take_made() -> Option<String> {
    MADE.lock().ok().and_then(|mut made| made.take())
}

/// Make a new, untitled song in the window's engine and make it current.
///
/// Its markers placed, its guide generated, Organize mode asked for.
/// `audio` as [`crate::open::open_untitled`]. Returns its project GUID.
///
/// # Errors
///
/// The window has no engine of its own, or the song's start could not be
/// placed.
pub fn create(audio: Option<bool>) -> Result<String, String> {
    let opened = crate::open::open_untitled(UNTITLED, audio).map_err(|e| e.to_string())?;
    // Placing the start moves the transport, which spawns on the engine's
    // runtime: run inside it, whichever thread the action came from.
    let runtime = crate::open::engine_runtime().map_err(|e| e.to_string())?;
    let _inside = runtime.enter();
    let project = ProjectContext::Project(opened.project_guid.clone());
    stamp(&opened.daw, &project)?;
    session::guide::Guide::new(opened.daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(session::guide::GuideScope::All)
        .map_err(|e| format!("the guide: {e}"))?;
    crate::actions::request_mode(session::modes::Mode::Organize);
    crate::studio::request_resync();
    crate::actions::list().changed("project", UNTITLED);
    tracing::info!(
        song.project = %opened.project_guid,
        song.new = true,
        "a new song was made"
    );
    Ok(opened.project_guid)
}

/// COUNT-IN, SONGSTART and SONGEND, each placed by keyflow's own insert
/// (the toolbar's), the edit cursor left on SONGSTART: where the first
/// section goes.
fn stamp(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Result<(), String> {
    let bar = |measure: i32| daw.musical_to_time(project.clone(), measure, 1, 0.0);
    let start = bar(COUNT_IN_BARS.saturating_add(1));
    let end = bar(COUNT_IN_BARS.saturating_add(1).saturating_add(SONG_BARS));
    for (kind, at) in [
        (MarkerKind::CountIn, 0.0),
        (MarkerKind::SongStart, start),
        (MarkerKind::SongEnd, end),
    ] {
        daw.set_position(project.clone(), at)
            .map_err(|e| format!("the cursor would not move to {at:.3} s: {e}"))?;
        // The insert acts on the current song, which this one now is.
        session::keyflow::actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
    }
    daw.set_position(project.clone(), start)
        .map_err(|e| format!("the cursor would not move to SONGSTART: {e}"))?;
    crate::cursor::ask(crate::cursor::Edit {
        at: start,
        selection: None,
    });
    Ok(())
}

/// Whether the song `guid` has never been saved: no file behind it.
#[must_use]
pub fn untitled(daw: &daw::standalone::Standalone, guid: &str) -> bool {
    daw.read_project(guid, |p| p.info.path.trim().is_empty())
        .unwrap_or(false)
}

/// The song `guid`, saved for the first time as `to` (into the file
/// `at`): named for `to`, and `at` its own file from now on.
pub fn named(
    daw: &daw::standalone::Standalone,
    guid: &str,
    to: &std::path::Path,
    at: &std::path::Path,
) {
    let name = to
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| UNTITLED.to_owned());
    let path = at.to_string_lossy().into_owned();
    daw.write_project(guid, |p| {
        p.info.name = name;
        p.info.path = path;
    });
}

/// A new song made and opened as a set of one — the start screen's New
/// Project. `audio` as [`create`].
///
/// # Errors
///
/// The song could not be made, or read back.
pub fn setlist(audio: Option<bool>) -> eyre::Result<crate::setlist::Setlist> {
    let guid = create(audio).map_err(|e| eyre::eyre!(e))?;
    setlist_of(guid)
}

/// The song `guid` — current in the engine, as [`create`] leaves it — as
/// a set of one.
///
/// # Errors
///
/// It could not be read back.
pub fn setlist_of(guid: String) -> eyre::Result<crate::setlist::Setlist> {
    let session = crate::studio::StudioSession::read_current(None, None)?;
    Ok(crate::setlist::Setlist::of(vec![crate::setlist::Song::of(
        UNTITLED.to_owned(),
        guid,
        session,
    )]))
}
