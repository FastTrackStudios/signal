//! Saving a song: `Song.session`, its project file (`crate::project_file`)
//! — beside the `Song.RPP` it was prepared from, its media paths resolving
//! against that same folder.

/// Save `project_guid` to the project file `path` names (`Song.session`,
/// or an alternative `Song.session/Organized`): what changed since its
/// last save, as one step of its history. Says where.
///
/// # Errors
/// The file would not take the save, or the engine would not be read.
pub fn save_session(
    daw: &daw::standalone::Standalone,
    project_guid: &str,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    #[cfg(feature = "native")]
    {
        let (file, branch) = crate::project_file::save_target(path);
        crate::project_file::save(daw, project_guid, &file, &branch, "Save")
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = (daw, project_guid, path);
        Err("a page saves nothing".to_owned())
    }
}

/// [`save_session`] behind the caller, on a thread of its own.
///
/// For a save that follows an edit (a chart rebuilt, the song's colour),
/// so whoever made the edit — the window, often — does not wait on the
/// file.
pub fn save_session_behind(
    daw: &daw::standalone::Standalone,
    project_guid: &str,
    path: &std::path::Path,
) {
    let (daw, guid, path) = (daw.clone(), project_guid.to_owned(), path.to_path_buf());
    let saving = std::thread::Builder::new()
        .name("save".into())
        .spawn(move || {
            if let Err(e) = save_session(&daw, &guid, &path) {
                tracing::warn!(session.save_error = %e, "the song could not be saved");
            }
        });
    if let Err(e) = saving {
        tracing::warn!(session.save_error = %e, "no thread to save the song on");
    }
}
