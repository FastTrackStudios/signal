//! Opening a song: THE one way, on every target.
//!
//! Which file opens (the prepared `.session`, else the `.RPP` prepared once
//! and saved), whether it is prepared, its project text, and loading it
//! into an engine. Everything here reads through a [`Folder`] — this
//! machine's disk ([`crate::folder::Disk`], what the plain functions use)
//! or a browser's memory ([`crate::folder::Memory`], a song from a share
//! link) — so a song opens the same way wherever its bytes are. The native
//! bootstrap (the runtime, the facade, the audio device) is
//! [`crate::open`]'s, which re-exports all of this.

use std::path::{Path, PathBuf};

use daw::standalone::Standalone;
use daw::standalone::project_loader::load_rpp_text;

use crate::folder::Folder;

/// A project that has been parsed and had its backend stood up.
pub struct Opened {
    pub daw: Standalone,
    pub name: String,
    pub project_guid: String,
    pub track_count: usize,
}

/// What opening a song file means: which file is read, whether it is
/// prepared on the way, and where the prepared song is saved.
///
/// Preparing (organize, build from the chart, generate the guide) is done
/// ONCE: the result is saved as `Song.session` beside `Song.RPP`, and from
/// then on the saved session is what opens — the `.RPP` is only the
/// multitrack it started from. A `.session` opened directly is never
/// prepared again. `FTS_SESSION_REPREPARE=1` prepares the `.RPP` afresh and
/// saves over the old session; nothing else overwrites one.
#[derive(Debug, PartialEq, Eq)]
pub struct SongPlan {
    pub open: std::path::PathBuf,
    pub prepare: bool,
    pub save_to: Option<std::path::PathBuf>,
}

/// How `path` opens (see [`SongPlan`]).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn song_plan(path: &Path, prepare: &crate::prepare::Prepare) -> SongPlan {
    song_plan_in(crate::folder::disk(), path, prepare)
}

/// [`song_plan`] for a song in `folder`. A folder that cannot be written
/// (a browser's copy) prepares in memory and saves nothing.
#[must_use]
pub fn song_plan_in(
    folder: &dyn Folder,
    path: &Path,
    prepare: &crate::prepare::Prepare,
) -> SongPlan {
    let reprepare = std::env::var("FTS_SESSION_REPREPARE").is_ok_and(|v| v == "1");
    song_plan_with(folder, path, prepare, reprepare)
}

pub(crate) fn song_plan_with(
    folder: &dyn Folder,
    path: &Path,
    prepare: &crate::prepare::Prepare,
    reprepare: bool,
) -> SongPlan {
    // A project file, or an alternative in one: opened as it is.
    #[cfg(feature = "native")]
    let in_a_file = crate::project_file::file_and_branch(path).is_some();
    #[cfg(not(feature = "native"))]
    let in_a_file = false;
    if is_session(path) || in_a_file {
        return SongPlan {
            open: path.to_path_buf(),
            prepare: false,
            save_to: None,
        };
    }
    // The song's project file, once it has been saved (a file: there is
    // no folder format any more).
    let saved = path.with_extension("session");
    if !folder.is_dir(&saved) && saved.is_file() && !reprepare {
        return SongPlan {
            open: saved,
            prepare: false,
            save_to: None,
        };
    }
    let prepare = !prepare.is_empty();
    SongPlan {
        open: path.to_path_buf(),
        prepare,
        save_to: (prepare && folder.writable()).then_some(saved),
    }
}

/// After a song's file is open: prepare it if its plan says so, and save
/// the prepared song as its `.session` — so the next open is the prepared
/// one. A preparation that fails saves nothing (a half-prepared session
/// would open as prepared next time).
pub fn prepare_and_save(
    folder: &dyn Folder,
    opened: &Opened,
    plan: &SongPlan,
    prepare: &crate::prepare::Prepare,
) {
    let mut save_to = plan.save_to.clone();
    if plan.prepare
        && let Err(e) = prepare.run_in(folder, opened)
    {
        tracing::error!(error = %e, "preparing the session failed; opening it as it was");
        save_to = None;
    }
    // Only a writable folder plans a save (see `song_plan_in`): never a
    // browser's.
    #[cfg(not(feature = "native"))]
    let _ = save_to;
    #[cfg(feature = "native")]
    if let Some(dir) = &save_to {
        match crate::session_file::save_session(&opened.daw, &opened.project_guid, dir) {
            Ok(at) => tracing::info!(
                session.saved = %at.display(),
                session.from = %plan.open.display(),
                "prepared once; saved as a session"
            ),
            Err(e) => tracing::warn!(
                session.save_error = %e,
                "the prepared session could not be saved; it will be prepared again next time"
            ),
        }
    }
}

/// Open a song into `daw` — THE way a song is opened, whoever runs the
/// engine (this window in Engine mode, `session-desktop --engine`): the
/// prepared `.session` when there is one, otherwise the file itself,
/// prepared and saved as its `.session` once. `daw` should be
/// [`equip`]ped. The song does not become current.
///
/// # Errors
///
/// The file could not be read or parsed, or its media did not materialize.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_song_into(
    daw: &Standalone,
    path: &Path,
    prepare: &crate::prepare::Prepare,
) -> eyre::Result<(Opened, SongPlan)> {
    open_song_into_with(daw, path, prepare, Media::All)
}

/// When a song's media is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// All of it, as the song opens.
    All,
    /// None yet: a progressive loader brings it in, what will be heard
    /// first first (`daw::standalone::audio_engine::materialize::materialize_take`).
    /// Until then a take is silent.
    Deferred,
}

/// [`open_song_into`], choosing when the media loads.
///
/// # Errors
///
/// As [`open_song_into`].
#[cfg(not(target_arch = "wasm32"))]
pub fn open_song_into_with(
    daw: &Standalone,
    path: &Path,
    prepare: &crate::prepare::Prepare,
    media: Media,
) -> eyre::Result<(Opened, SongPlan)> {
    open_song_in(crate::folder::disk(), daw, path, prepare, media)
}

/// [`open_song_into_with`] for a song in `folder` — a browser opens a
/// shared song by this, from memory.
///
/// # Errors
///
/// As [`open_song_into`].
pub fn open_song_in(
    folder: &dyn Folder,
    daw: &Standalone,
    path: &Path,
    prepare: &crate::prepare::Prepare,
    media: Media,
) -> eyre::Result<(Opened, SongPlan)> {
    let plan = song_plan_in(folder, path, prepare);
    let opened = load_in(folder, daw, &plan.open, media)?;
    prepare_and_save(folder, &opened, &plan, prepare);
    Ok((opened, plan))
}

/// Load exactly `path` (a `.RPP` or a `.session`) into `daw`: parse it,
/// resolve and anchor its media, materialize its audio. No preparing, no
/// choosing — see [`open_song_into`] for opening a *song*.
///
/// # Errors
///
/// The file could not be read or parsed, or its media did not materialize.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_into(daw: &Standalone, path: &Path) -> eyre::Result<Opened> {
    load_into_with(daw, path, Media::All)
}

/// [`load_into`], choosing when the media loads.
///
/// # Errors
///
/// The file could not be read or parsed, or (with [`Media::All`]) its
/// media did not materialize.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_into_with(daw: &Standalone, path: &Path, media: Media) -> eyre::Result<Opened> {
    // Absolute from here on: the project's path is what a later save
    // writes its media relative to, and a path relative to wherever the
    // app was started from means nothing once it is saved.
    let path = &std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    load_in(crate::folder::disk(), daw, path, media)
}

/// [`load_into_with`] for a file in `folder`.
///
/// # Errors
///
/// As [`load_into_with`].
pub fn load_in(
    folder: &dyn Folder,
    daw: &Standalone,
    path: &Path,
    media: Media,
) -> eyre::Result<Opened> {
    #[cfg(feature = "native")]
    let opened = match crate::project_file::file_and_branch(path) {
        Some((file, branch)) => load_project_file(daw, path, &file, &branch, media)?,
        None => load_rpp_in(folder, daw, path, media)?,
    };
    #[cfg(not(feature = "native"))]
    let opened = load_rpp_in(folder, daw, path, media)?;
    colour_sections(&opened);
    Ok(opened)
}

/// The song's sections in keyflow's colours ([`crate::section_colors`]),
/// as one step — none when they are coloured already.
fn colour_sections(opened: &Opened) {
    let project = daw::service::ProjectContext::Project(opened.project_guid.clone());
    if crate::section_colors::pending(&opened.daw, &project) == 0 {
        return;
    }
    daw::service::Projects::begin_undo_block(&opened.daw, project.clone(), "Colour the sections");
    let coloured = crate::section_colors::apply(&opened.daw, &project);
    daw::service::Projects::end_undo_block(&opened.daw, project, "Colour the sections", None);
    match coloured {
        Ok(count) => tracing::info!(sections.coloured = count, "sections coloured"),
        Err(e) => tracing::warn!(error = %e, "the sections could not be coloured"),
    }
}

/// [`load_in`] for an `.RPP` (or the text a folder holds for one).
fn load_rpp_in(
    folder: &dyn Folder,
    daw: &Standalone,
    path: &Path,
    media: Media,
) -> eyre::Result<Opened> {
    let ProjectText { text, media_dir } = project_text_in(folder, path)?;
    let daw = daw.clone();
    daw.media_bay().set_file_resolver(Box::new(
        daw::standalone::media_bay::ProjectRelativeResolver::new(media_dir.clone()),
    ));
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let began = std::time::Instant::now();
    let summary = load_rpp_text(&daw, &name, &path.to_string_lossy(), &text)
        .map_err(|e| eyre::eyre!("{name} did not parse: {e}"))?;
    let parsed = began.elapsed();
    daw::standalone::project_loader::anchor_media(&daw, &summary.project_guid, &media_dir);
    let anchored = began.elapsed();

    if media == Media::All {
        let audio = daw::standalone::audio_engine::materialize::materialize_via_bay(
            &daw,
            &summary.project_guid,
        )
        .map_err(|e| eyre::eyre!("{name}'s media did not materialize: {e}"))?;
        if !audio.failed.is_empty() {
            tracing::warn!(
                failed = audio.failed.len(),
                loaded = audio.loaded,
                "some sources did not materialize"
            );
        }
    }

    let materialized = began.elapsed();
    let track_count = daw::service::Tracks::all(
        &daw,
        daw::service::ProjectContext::Project(summary.project_guid.clone()),
    )
    .len();
    // One line per load: where it went.
    tracing::info!(
        load.name = %name,
        load.tracks = track_count,
        load.parse_ms = parsed.as_millis(),
        load.anchor_ms = (anchored - parsed).as_millis(),
        load.media_ms = (materialized - anchored).as_millis(),
        load.count_ms = (began.elapsed() - materialized).as_millis(),
        "project loaded"
    );
    Ok(Opened {
        daw,
        name,
        project_guid: summary.project_guid.clone(),
        track_count,
    })
}

/// [`load_in`] for a project file: `branch` of `file` rebuilt in the
/// engine from its doc, its media anchored and (with [`Media::All`])
/// materialized as an `.RPP`'s are.
///
/// # Errors
///
/// The file would not open, or its media did not materialize.
#[cfg(feature = "native")]
fn load_project_file(
    daw: &Standalone,
    path: &Path,
    file: &Path,
    branch: &str,
    media: Media,
) -> eyre::Result<Opened> {
    let media_dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    daw.media_bay().set_file_resolver(Box::new(
        daw::standalone::media_bay::ProjectRelativeResolver::new(media_dir.clone()),
    ));
    let name = if branch == dawfile_store::MAIN {
        file.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    } else {
        branch.to_owned()
    };
    let began = std::time::Instant::now();
    let project_guid = crate::project_file::load(daw, &name, file, branch)
        .map_err(|e| eyre::eyre!("{} did not open: {e}", path.display()))?;
    let built = began.elapsed();
    daw::standalone::project_loader::anchor_media(daw, &project_guid, &media_dir);
    if media == Media::All {
        let audio =
            daw::standalone::audio_engine::materialize::materialize_via_bay(daw, &project_guid)
                .map_err(|e| eyre::eyre!("{name}'s media did not materialize: {e}"))?;
        if !audio.failed.is_empty() {
            tracing::warn!(
                failed = audio.failed.len(),
                loaded = audio.loaded,
                "some sources did not materialize"
            );
        }
    }
    let track_count = daw::service::Tracks::all(
        daw,
        daw::service::ProjectContext::Project(project_guid.clone()),
    )
    .len();
    tracing::info!(
        load.name = %name,
        load.tracks = track_count,
        load.build_ms = built.as_millis(),
        load.media_ms = began.elapsed().saturating_sub(built).as_millis(),
        "project file opened"
    );
    Ok(Opened {
        daw: daw.clone(),
        name,
        project_guid,
        track_count,
    })
}

/// A project's text as the loader reads it, and the folder its media
/// paths are relative to.
pub struct ProjectText {
    pub text: String,
    pub media_dir: PathBuf,
}

/// Whether `path` is a saved session — a `Song.session` project directory,
/// the native format — rather than a REAPER project.
#[must_use]
pub fn is_session(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("session"))
}

/// Read a project for the loader: a `.RPP` as it is, a `.session` through
/// the format's own `.rpp` export (the round trip it is proven lossless
/// on). Media resolves against the folder either one sits in — a session
/// is saved beside the `.RPP` it was prepared from, so `Media/Bass.wav`
/// means the same file to both.
///
/// # Errors
///
/// The file could not be read, or the session could not be exported.
#[cfg(not(target_arch = "wasm32"))]
pub fn project_text(path: &Path) -> eyre::Result<ProjectText> {
    project_text_in(crate::folder::disk(), path)
}

/// [`project_text`] for a file in `folder`.
///
/// # Errors
///
/// As [`project_text`].
pub fn project_text_in(folder: &dyn Folder, path: &Path) -> eyre::Result<ProjectText> {
    if is_session(path) {
        return Err(eyre::eyre!(
            "{} is a project file: it opens through its doc, not as text",
            path.display()
        ));
    }
    let text = folder
        .read_to_string(path)
        .map_err(|e| eyre::eyre!("could not read {}: {e}", path.display()))?;
    let media_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(ProjectText { text, media_dir })
}
