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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_token_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(dir.path().join("absent.json"));
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn tokens_round_trip_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(dir.path().join("t3k.json"));
        let tokens = Tokens {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 1_800_000_000,
        };
        store.save(&tokens).unwrap();
        assert_eq!(store.load().unwrap(), Some(tokens));
    }

    /// The token file is a credential; on unix it must not be world-readable.
    #[cfg(unix)]
    #[test]
    fn the_token_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(dir.path().join("t3k.json"));
        store
            .save(&Tokens {
                access_token: "a".into(),
                refresh_token: "r".into(),
                expires_at: 0,
            })
            .unwrap();
        let mode = std::fs::metadata(store.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "group/other bits must be clear (mode {mode:o})");
    }

    #[test]
    fn clearing_an_absent_session_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(dir.path().join("gone.json"));
        store.clear().unwrap();
    }

    #[test]
    fn a_token_near_expiry_is_refreshed_before_it_is_used() {
        let t = Tokens {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 1_000,
        };
        assert!(t.needs_refresh(1_000), "expired");
        assert!(t.needs_refresh(950), "inside the slack window");
        assert!(!t.needs_refresh(800), "comfortably valid");
    }
}
