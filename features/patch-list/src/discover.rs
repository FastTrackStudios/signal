//! Where the files are.
//!
//! The album file is `patch-list.styx` in the album's directory — beside
//! the sessions, above them, wherever the album keeps its one setup —
//! found by walking up from the project's directory to the first
//! directory holding one, the way the keybind profile is found.
//!
//! Studio profiles are the machine's, in the app config directory
//! (`~/.config/fts` — the directory the guide's samples and the TTS
//! cache already share): `studios/<name>.styx`, one per room. Which one
//! is active is a per-machine setting in `studio.styx` beside them,
//! and defaults to the hostname, so a box named after its room needs
//! no setting at all.

use std::path::{Path, PathBuf};

use facet::Facet;

use crate::list::PatchList;
use crate::profile::StudioProfile;

/// The album file's name.
pub const ALBUM_FILE: &str = "patch-list.styx";

/// The app config directory's name under the platform config root.
const APP_DIR: &str = "fts";
/// Where the profiles live under the config root.
const STUDIOS_DIR: &str = "studios";
/// The per-machine setting naming the active profile.
const SETTING_FILE: &str = "studio.styx";

/// The album file for a project: the first `patch-list.styx` at or
/// above `dir`.
///
/// `None` when no directory up to the root holds one — a project that
/// is not part of an album, which is not an error.
// r[impl flow.patch-list.project-level]
#[must_use]
pub fn find_album(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .map(|ancestor| ancestor.join(ALBUM_FILE))
        .find(|candidate| candidate.is_file())
}

/// Write the album file.
///
/// Edits in the Patch List view write here by default (spec #48,
/// "Editing destinations"); the per-session override
/// (`apply::set_override`) is the only other place an edit can go,
/// behind the view's "for this session" toggle.
///
/// # Errors
///
/// [`WriteError::Serialize`] when the list cannot be written back as
/// styx; [`WriteError::Write`] when the file cannot be written.
pub fn write_album(path: &Path, list: &PatchList) -> Result<(), WriteError> {
    let text = list.to_styx().map_err(WriteError::Serialize)?;
    std::fs::write(path, text).map_err(|source| WriteError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Why the album file could not be written.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The list itself would not serialize.
    #[error("the album list did not serialize: {0}")]
    Serialize(crate::styx::WriteError),
    /// The file could not be written.
    #[error("writing {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The machine's studio profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Studios {
    root: PathBuf,
}

/// The per-machine setting file.
#[derive(Facet, Debug, Default)]
struct Setting {
    #[facet(default)]
    active: Option<String>,
}

/// Why a profile could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// There is no profile of that name.
    #[error("no studio profile at {}", path.display())]
    Missing { path: PathBuf },
    /// The file could not be read.
    #[error("reading {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file is not a profile.
    #[error("{} is not a studio profile: {message}", path.display())]
    Parse { path: PathBuf, message: String },
}

impl Error {
    /// Whether this is the named absence rather than a broken file.
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing { .. })
    }
}

impl Studios {
    /// The profiles under a config root.
    #[must_use]
    pub fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// The profiles in the app config directory, if the platform has
    /// one.
    #[must_use]
    pub fn in_config_dir() -> Option<Self> {
        dirs::config_dir().map(|config| Self::at(&config.join(APP_DIR)))
    }

    /// Where a profile of this name lives.
    #[must_use]
    pub fn profile_path(&self, name: &str) -> PathBuf {
        self.root.join(STUDIOS_DIR).join(format!("{name}.styx"))
    }

    /// The active profile's name: the machine setting, else the
    /// hostname.
    // r[impl flow.patch-list.studio-profiles]
    #[must_use]
    pub fn active_name(&self) -> String {
        std::fs::read_to_string(self.root.join(SETTING_FILE))
            .ok()
            .and_then(|text| facet_styx::from_str::<Setting>(&text).ok())
            .and_then(|setting| setting.active)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| gethostname::gethostname().to_string_lossy().into_owned())
    }

    /// The active profile, with its name.
    ///
    /// `None` when the machine has no profile of the active name — a
    /// box that has not been given a room yet. A profile that exists
    /// and does not parse is not silently `None`: it is logged as a
    /// denial and then treated as absent, so a typo in the room's file
    /// shows every role unresolved rather than failing the window.
    #[must_use]
    pub fn active(&self) -> Option<(String, StudioProfile)> {
        let name = self.active_name();
        match self.load(&name) {
            Ok(profile) => Some((name, profile)),
            Err(err) if err.is_missing() => None,
            Err(err) => {
                tracing::warn!(
                    patch.studio = %name,
                    error = %err,
                    "the active studio profile does not load"
                );
                None
            }
        }
    }

    /// Load a profile by name.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] when there is no such file; [`Error::Read`]
    /// and [`Error::Parse`] when there is one and it is not usable.
    pub fn load(&self, name: &str) -> Result<StudioProfile, Error> {
        let path = self.profile_path(name);
        if !path.is_file() {
            return Err(Error::Missing { path });
        }
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        StudioProfile::from_styx(&text).map_err(|err| Error::Parse {
            path,
            message: err.to_string(),
        })
    }
}
