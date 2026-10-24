//! Token persistence and the shape of a completed download.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An OAuth session, as persisted between runs.
///
/// Stored rather than held in memory so that signing in once covers every
/// later session and every plugin instance — the alternative is each of them
/// running its own authorization dance against the same account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds. Compared against the clock before each use; the client
    /// refreshes ahead of expiry rather than waiting for a 401.
    pub expires_at: i64,
    /// The account these tokens belong to, as the catalog names it.
    ///
    /// Cached with the session so "signed in as who?" is answerable from
    /// disk. The alternative — a `/user` call every time a surface asks —
    /// spends a request from a 100-per-minute budget on a question whose
    /// answer changes only at sign-in, and would make the status call fail
    /// whenever the network is down while the session is perfectly valid.
    #[serde(default)]
    pub username: String,
}

impl Tokens {
    /// Whether the access token should be refreshed before the next call.
    ///
    /// The minute of slack covers a request that is issued just before expiry
    /// and arrives just after — the failure that would otherwise show up as an
    /// occasional, unreproducible 401.
    #[must_use]
    pub const fn needs_refresh(&self, now_unix: i64) -> bool {
        self.expires_at - now_unix <= 60
    }
}

/// Where the session is kept on disk.
///
/// One file for the machine, not one per plugin instance: the library the
/// tokens feed is shared, so the session should be too.
#[derive(Debug, Clone)]
pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the stored session. A missing file is `None`, not an error — not
    /// being signed in is an ordinary state, and the caller's next move is the
    /// same either way.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the file cannot be read (other than missing).
    pub fn load(&self) -> std::io::Result<Option<Tokens>> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(serde_json::from_str(&s).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Persist the session, replacing any previous one.
    ///
    /// Written through a temporary file and renamed, so an interrupted write
    /// cannot leave a half-written token file that reads as corrupt (and, on
    /// unix, created 0600 — it is a credential).
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if directory creation, file write, or rename fails.
    pub fn save(&self, tokens: &Tokens) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        let json = serde_json::to_string(tokens).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)
    }

    /// Forget the session — signing out. Absent is already the desired state.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if file deletion fails (other than missing file).
    pub fn clear(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// What a completed download produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// Where the file landed, absolute.
    pub path: PathBuf,
    /// SHA-256 of the bytes written — the key the NAM catalog indexes by, so
    /// the caller can address the new file without rescanning the tree.
    pub hash: String,
    /// False when an identical file (same hash) was already in the library and
    /// nothing was written.
    pub written: bool,
}
