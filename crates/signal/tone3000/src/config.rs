//! Where the integration gets its identity and its directories.
//!
//! The publishable key is not a secret — TONE3000 calls it "safe for
//! client-side use", it is the `client_id` in an OAuth 2.0 + PKCE flow, and
//! every plugin that ships one hands it to whoever looks. So it may be baked
//! into a release build. It is still not hard-coded here: the key belongs to
//! an account and a set of registered redirect URIs, and a fork or a dev build
//! needs its own without patching source.
//!
//! Resolution, first match wins:
//!
//! 1. the environment (`SIGNAL_T3K_PUBLISHABLE_KEY`, `SIGNAL_T3K_REDIRECT_URI`)
//! 2. the value compiled in at build time via the same names
//! 3. nothing — the service then answers every call with a configuration
//!    error rather than pretending to be signed out, which is a different
//!    thing and would send the user round the sign-in loop forever.

use std::path::PathBuf;

/// The default redirect the engine listens on.
///
/// Loopback rather than a custom scheme because a scheme registration is a
/// per-platform install step and this works from a plain `cargo run`. It is
/// deliberately a *fixed* port: RFC 8252 §7.3 allows a native app to vary the
/// port, but TONE3000 matches registered redirect URIs, so the port has to be
/// one we can register. 4040 is the engine's own port — the callback lands on
/// the HTTP server that is already there.
///
/// Spelled `localhost` rather than `127.0.0.1` because TONE3000's own demo
/// documents localhost origins as allowed during development without being
/// registered, and that is the string their check is described in terms of.
/// The two resolve to the same socket for us either way — the engine binds
/// `0.0.0.0` — so this costs nothing and removes a registration step.
pub const DEFAULT_REDIRECT_URI: &str = "http://localhost:4040/tone3000/callback";

/// The path the redirect URI above resolves to, mounted by the engine host.
pub const CALLBACK_PATH: &str = "/tone3000/callback";

/// Identity, endpoints and directories for one engine's integration.
#[derive(Debug, Clone)]
pub struct Config {
    /// OAuth `client_id`. Empty means unconfigured.
    pub publishable_key: String,
    /// Must be registered with TONE3000 or the authorize call is rejected.
    pub redirect_uri: String,
    /// Override for tests (a wiremock base URL). `None` = the real API.
    pub base_url: Option<String>,
    /// Root of the local NAM library; downloads land under `tone3000/` in it.
    pub library_root: PathBuf,
    /// The catalog scanner's index, updated as models arrive.
    pub catalog_path: PathBuf,
    /// Persisted session.
    pub token_path: PathBuf,
    /// Fetched artwork, keyed by URL digest.
    pub image_cache: PathBuf,
}

impl Config {
    /// Read the environment, falling back to compiled-in values and the
    /// standard directories under `dir`.
    #[must_use]
    pub fn from_env(config_dir: &std::path::Path, library_root: PathBuf) -> Self {
        let publishable_key = std::env::var("SIGNAL_T3K_PUBLISHABLE_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| option_env!("SIGNAL_T3K_PUBLISHABLE_KEY").map(ToString::to_string))
            .unwrap_or_default();
        let redirect_uri = std::env::var("SIGNAL_T3K_REDIRECT_URI")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| option_env!("SIGNAL_T3K_REDIRECT_URI").map(ToString::to_string))
            .unwrap_or_else(|| DEFAULT_REDIRECT_URI.to_string());
        let base_url = std::env::var("SIGNAL_T3K_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());

        let t3k = config_dir.join("tone3000");
        Self {
            publishable_key,
            redirect_uri,
            base_url,
            catalog_path: library_root.join("catalog.json"),
            library_root,
            token_path: t3k.join("session.json"),
            image_cache: t3k.join("images"),
        }
    }

    /// Whether a publishable key is present. Without one the flow cannot even
    /// be started, and saying so is more useful than a failed round trip.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.publishable_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_hang_off_the_config_dir_and_library() {
        let cfg = Config::from_env(
            std::path::Path::new("/cfg"),
            std::path::PathBuf::from("/lib/nam"),
        );
        assert_eq!(cfg.token_path, PathBuf::from("/cfg/tone3000/session.json"));
        assert_eq!(cfg.image_cache, PathBuf::from("/cfg/tone3000/images"));
        assert_eq!(cfg.catalog_path, PathBuf::from("/lib/nam/catalog.json"));
    }

    /// An engine with no key must be able to say so, rather than reporting the
    /// user as signed out — the remedies are entirely different.
    #[test]
    fn a_config_without_a_key_is_not_configured() {
        let cfg = Config {
            publishable_key: String::new(),
            redirect_uri: DEFAULT_REDIRECT_URI.into(),
            base_url: None,
            library_root: PathBuf::from("/lib"),
            catalog_path: PathBuf::from("/lib/catalog.json"),
            token_path: PathBuf::from("/t"),
            image_cache: PathBuf::from("/i"),
        };
        assert!(!cfg.is_configured());
        assert!(Config {
            publishable_key: "t3k_pub_x".into(),
            ..cfg
        }
        .is_configured());
    }
}
