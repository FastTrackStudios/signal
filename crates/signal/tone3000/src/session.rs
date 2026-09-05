//! The OAuth handshake and the download path.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::store::{DownloadOutcome, TokenStore, Tokens};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("not signed in to TONE3000")]
    NotSignedIn,
    /// No publishable key. Distinct from being signed out: the user cannot
    /// fix this by signing in, so a UI must not send them round that loop.
    #[error("this build has no TONE3000 publishable key configured")]
    NotConfigured,
    /// The callback did not match the request that started it. Treated as a
    /// hard failure rather than a retry: a mismatched `state` is what a CSRF
    /// attempt looks like, and it is indistinguishable from one.
    #[error("authorization state did not match the request that started it")]
    StateMismatch,
    #[error("the authorization callback carried no code")]
    MissingCode,
    #[error("TONE3000 rejected the request: {0}")]
    Api(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// An authorization in flight.
///
/// The verifier and state are held by the engine between handing the URL to a
/// GUI and receiving the callback back from it — the GUI is only a courier, so
/// it is never given either value to keep.
#[derive(Debug, Clone)]
pub struct AuthStart {
    /// Open this in the system browser. Not renderable in-process: it is a web
    /// application, and our plugin surface is a renderer, not a browser.
    pub authorize_url: String,
    /// PKCE verifier — proves the callback belongs to this request.
    verifier: String,
    /// CSRF nonce echoed back by the callback.
    state: String,
}

impl AuthStart {
    #[must_use] 
    pub const fn new(authorize_url: String, verifier: String, state: String) -> Self {
        Self { authorize_url, verifier, state }
    }

    #[must_use] 
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Check the `state` echoed by a callback against the one we issued.
    ///
    /// Compared in full rather than short-circuiting on the first differing
    /// byte, so the check does not leak the value through its timing.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::StateMismatch` if the echoed state does not match the stored one.
    pub fn verify_state(&self, echoed: &str) -> Result<(), SessionError> {
        let (a, b) = (self.state.as_bytes(), echoed.as_bytes());
        let equal = a.len() == b.len()
            && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0;
        if equal {
            Ok(())
        } else {
            Err(SessionError::StateMismatch)
        }
    }
}

/// The engine's TONE3000 session: where tokens live, and where models land.
#[derive(Debug, Clone)]
pub struct Session {
    tokens: TokenStore,
    /// Root of the local NAM library — the same tree `signal_nam`'s scanner
    /// walks, so a downloaded file is indexed by the existing pipeline rather
    /// than a parallel one.
    library_root: PathBuf,
}

impl Session {
    pub fn new(tokens: TokenStore, library_root: impl Into<PathBuf>) -> Self {
        Self { tokens, library_root: library_root.into() }
    }

    #[must_use] 
    pub const fn token_store(&self) -> &TokenStore {
        &self.tokens
    }

    #[must_use] 
    pub fn library_root(&self) -> &Path {
        &self.library_root
    }

    /// Whether a session is stored. Not whether it is still valid — that is
    /// only knowable by spending a request, and the caller's next step (start
    /// the flow, or try and refresh) is the same either way.
    #[must_use] 
    pub fn is_signed_in(&self) -> bool {
        matches!(self.tokens.load(), Ok(Some(_)))
    }

    /// Sign out by clearing stored tokens.
    ///
    /// # Errors
    ///
    /// Returns an error if the token store fails to clear (e.g., I/O error).
    pub fn sign_out(&self) -> Result<(), SessionError> {
        self.tokens.clear().map_err(SessionError::from)
    }

    /// Retrieve the stored tokens if the session is signed in.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::NotSignedIn` if no tokens are stored, or a token store error if loading fails.
    pub fn stored_tokens(&self) -> Result<Tokens, SessionError> {
        self.tokens.load()?.ok_or(SessionError::NotSignedIn)
    }

    /// Where a tone's model file belongs inside the library.
    ///
    /// Downloads go under a `tone3000/` subtree so that what was fetched stays
    /// visibly distinct from what the user put there — it is the difference
    /// between a file we may re-fetch and one we must never touch. The name is
    /// sanitised because it comes from a public catalog: a creator-supplied
    /// name containing a separator would otherwise choose its own directory.
    #[must_use] 
    pub fn destination(&self, tone_id: &str, filename: &str) -> PathBuf {
        self.library_root
            .join("tone3000")
            .join(sanitize(tone_id))
            .join(sanitize(filename))
    }

    /// Record bytes already fetched into the library.
    ///
    /// Content-addressed: if a file with these exact bytes is already at the
    /// destination, nothing is rewritten and `written` is false. Re-picking a
    /// tone you already have is a normal thing to do, and it should cost
    /// nothing and disturb no file the catalog has already indexed.
    ///
    /// # Errors
    ///
    /// Returns an error if creating directories or writing the file fails.
    pub fn place_model(
        &self,
        tone_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> Result<DownloadOutcome, SessionError> {
        let path = self.destination(tone_id, filename);
        let hash = hex(&Sha256::digest(bytes));

        if let Ok(existing) = std::fs::read(&path) {
            if hex(&Sha256::digest(&existing)) == hash {
                return Ok(DownloadOutcome { path, hash, written: false });
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write-then-rename: a partial `.nam` left by an interrupted download
        // would be indexed by the next scan as a real, broken model.
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)?;
        Ok(DownloadOutcome { path, hash, written: true })
    }
}

/// Reduce a catalog-supplied string to one safe path segment.
fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '-' | '_' | '.' | ' ' => c,
            _ => '_',
        })
        .collect();
    // `.` and `..` are legal filenames by the rule above but are not names.
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(dir: &Path) -> Session {
        Session::new(TokenStore::new(dir.join("t3k.json")), dir.join("nam"))
    }

    #[test]
    fn a_fresh_session_is_not_signed_in() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!session(dir.path()).is_signed_in());
    }

    #[test]
    fn state_must_match_the_request_that_started_it() {
        let start = AuthStart::new("https://example/authorize".into(), "v".into(), "s123".into());
        assert!(start.verify_state("s123").is_ok());
        assert!(matches!(
            start.verify_state("other"),
            Err(SessionError::StateMismatch)
        ));
        assert!(matches!(start.verify_state(""), Err(SessionError::StateMismatch)));
    }

    #[test]
    fn a_model_lands_under_the_tone3000_subtree() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let out = s.place_model("1234", "amp.nam", b"weights").unwrap();
        assert!(out.written);
        assert!(out.path.ends_with("nam/tone3000/1234/amp.nam"), "{:?}", out.path);
        assert_eq!(std::fs::read(&out.path).unwrap(), b"weights");
    }

    #[test]
    fn re_fetching_identical_bytes_rewrites_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let first = s.place_model("1234", "amp.nam", b"weights").unwrap();
        let again = s.place_model("1234", "amp.nam", b"weights").unwrap();
        assert!(first.written);
        assert!(!again.written, "identical bytes must not be rewritten");
        assert_eq!(first.hash, again.hash);
    }

    #[test]
    fn changed_bytes_at_the_same_name_do_replace_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        s.place_model("1234", "amp.nam", b"old").unwrap();
        let out = s.place_model("1234", "amp.nam", b"new").unwrap();
        assert!(out.written);
        assert_eq!(std::fs::read(&out.path).unwrap(), b"new");
    }

    /// Names come from a public catalog, so they are untrusted input.
    #[test]
    fn catalog_names_cannot_escape_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let out = s.place_model("../../etc", "../../passwd", b"x").unwrap();
        assert!(
            out.path.starts_with(s.library_root()),
            "escaped the library: {:?}",
            out.path
        );
        // The property is that no *component* can walk upward. A literal ".."
        // inside a longer segment is inert once the separators are gone.
        assert!(
            !out.path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "a parent-dir component survived: {:?}",
            out.path
        );
    }

    #[test]
    fn an_empty_or_dotted_name_still_yields_a_filename() {
        assert_eq!(sanitize(""), "unnamed");
        assert_eq!(sanitize("   "), "unnamed");
        assert_eq!(sanitize(".."), "unnamed");
        assert_eq!(sanitize("."), "unnamed");
    }

    #[test]
    fn the_hash_is_the_catalog_key_for_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let out = s.place_model("1", "a.nam", b"abc").unwrap();
        // SHA-256("abc"), the value signal-nam's scanner would compute.
        assert_eq!(
            out.hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
