//! `BayFileResolver` — the single point of indirection for turning
//! source-file paths into bytes.
//!
//! Native apps install [`FsFileResolver`]; WASM apps install a
//! resolver backed by JS (postMessage / fetch / IndexedDB).
//!
//! ```ignore
//! // native
//! daw.media_bay().set_file_resolver(Box::new(FsFileResolver));
//!
//! // WASM (sketch — implementation lives in the wasm-bindgen layer)
//! struct JsResolver { /* … Map<String, Vec<u8>> … */ }
//! impl BayFileResolver for JsResolver { ... }
//! daw.media_bay().set_file_resolver(Box::new(JsResolver::new()));
//! ```

/// Resolves a logical source path to file bytes.
pub trait BayFileResolver: Send + Sync {
    /// Return the bytes for the given source path, or an error
    /// string. The bay normalizes paths up front; implementations
    /// can assume the path is whatever was stored on
    /// `Take.source_file_path`.
    fn resolve(&self, path: &str) -> Result<Vec<u8>, String>;

    /// Resolve to an on-disk path when the source is a local file —
    /// lets the engine stream PCM straight from disk (mmap) instead of
    /// loading bytes. `None` (the default, and on WASM) falls back to
    /// the byte path + full decode.
    fn resolve_path(&self, _path: &str) -> Option<std::path::PathBuf> {
        None
    }
}

/// Native filesystem resolver — reads from disk. Pulls in
/// `std::fs`, so don't use this on WASM.
#[derive(Clone, Copy, Debug, Default)]
pub struct FsFileResolver;

impl BayFileResolver for FsFileResolver {
    fn resolve(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| format!("read {path}: {e}"))
    }

    fn resolve_path(&self, path: &str) -> Option<std::path::PathBuf> {
        let pb = std::path::PathBuf::from(path);
        pb.exists().then_some(pb)
    }
}

/// Filesystem resolver that interprets relative paths against a
/// project directory (matching REAPER's behavior — sources stored
/// relative to the `.rpp` file). Absolute paths pass through.
#[derive(Clone, Debug)]
pub struct ProjectRelativeResolver {
    project_dir: std::path::PathBuf,
}

impl ProjectRelativeResolver {
    pub fn new(project_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            project_dir: project_dir.into(),
        }
    }
}

impl ProjectRelativeResolver {
    /// Where `path` is on this machine: the file itself, or — when only
    /// its proxy was fetched (a session pulled from a library streams
    /// from its proxies) — `Proxies/<stem>.ogg` beside where it would be.
    /// The same rule the proxy writer and the server's renditions use.
    fn locate(&self, path: &str) -> Option<std::path::PathBuf> {
        let pb = std::path::PathBuf::from(path);
        let abs = if pb.is_absolute() {
            pb
        } else {
            self.project_dir.join(pb)
        };
        if abs.exists() {
            return Some(abs);
        }
        let proxy = abs
            .parent()?
            .join("Proxies")
            .join(abs.file_stem()?)
            .with_extension("ogg");
        proxy.exists().then_some(proxy)
    }
}

impl BayFileResolver for ProjectRelativeResolver {
    fn resolve(&self, path: &str) -> Result<Vec<u8>, String> {
        let abs = self
            .locate(path)
            .ok_or_else(|| format!("read {path}: not found (nor its proxy)"))?;
        std::fs::read(&abs).map_err(|e| format!("read {}: {e}", abs.display()))
    }

    fn resolve_path(&self, path: &str) -> Option<std::path::PathBuf> {
        self.locate(path)
    }
}

#[cfg(test)]
mod proxy_fallback {
    use super::{BayFileResolver, ProjectRelativeResolver};

    #[test]
    fn a_missing_source_is_found_by_its_proxy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Media/Proxies")).unwrap();
        std::fs::write(dir.path().join("Media/Proxies/Bass.ogg"), b"ogg").unwrap();
        std::fs::write(dir.path().join("Media/Keys.wav"), b"wav").unwrap();
        let r = ProjectRelativeResolver::new(dir.path());
        assert_eq!(r.resolve("Media/Bass.wav").unwrap(), b"ogg");
        assert_eq!(
            r.resolve("Media/Keys.wav").unwrap(),
            b"wav",
            "the original wins when it is there"
        );
        assert!(r.resolve("Media/Gone.wav").is_err());
        assert_eq!(
            r.resolve_path("Media/Bass.wav"),
            Some(dir.path().join("Media/Proxies/Bass.ogg"))
        );
    }
}

/// In-memory resolver — useful for tests and the WASM "ship bytes
/// up-front via postMessage" pattern. Cheap to clone (the inner map
/// is `Arc`-shared so multiple resolvers can read the same registry).
#[derive(Clone, Default)]
pub struct InMemoryResolver {
    files: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl InMemoryResolver {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register bytes for a path. Subsequent `resolve()` calls
    /// return these bytes.
    pub fn insert(&self, path: impl Into<String>, bytes: Vec<u8>) {
        self.files
            .lock()
            .expect("resolver poisoned")
            .insert(path.into(), bytes);
    }
    pub fn remove(&self, path: &str) {
        self.files.lock().expect("resolver poisoned").remove(path);
    }
}

impl BayFileResolver for InMemoryResolver {
    fn resolve(&self, path: &str) -> Result<Vec<u8>, String> {
        self.files
            .lock()
            .expect("resolver poisoned")
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path} not in InMemoryResolver"))
    }
}

/// Fetches sources from a base URL by HTTP — Nextcloud share link,
/// S3 bucket, generic static host, etc. Source paths are appended
/// (URL-escaped) onto the base. Native-only; the WASM equivalent is
/// to fetch on the JS side and push bytes into [`InMemoryResolver`].
///
/// Behind `feature = "http-resolver"` (pulls `ureq`).
///
/// ```ignore
/// daw.media_bay()
///     .set_file_resolver(Box::new(HttpBaseUrlResolver::new(
///         "https://nextcloud.example/s/SHARETOKEN/download?path=",
///     )));
/// load_rpp_via_bay(&daw, name, path, rpp_text)?;
/// ```
#[cfg(feature = "http-resolver")]
#[derive(Clone, Debug)]
pub struct HttpBaseUrlResolver {
    base_url: String,
    /// Optional override timeout in milliseconds (default = 30s).
    timeout_ms: u64,
}

#[cfg(feature = "http-resolver")]
impl HttpBaseUrlResolver {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_ms: 30_000,
        }
    }
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

#[cfg(feature = "http-resolver")]
impl BayFileResolver for HttpBaseUrlResolver {
    fn resolve(&self, path: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}{}", self.base_url, url_escape_path(path));
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .build();
        let resp = agent
            .get(&url)
            .call()
            .map_err(|e| format!("fetch {url}: {e}"))?;
        let mut bytes: Vec<u8> = Vec::new();
        resp.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read {url}: {e}"))?;
        Ok(bytes)
    }
}

/// Minimal path escaping for URL composition — escapes spaces and a
/// few reserved chars without pulling in a full URL crate. Good
/// enough for typical file-name characters.
#[cfg(feature = "http-resolver")]
fn url_escape_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut out, "%{:02X}", b);
            }
        }
    }
    out
}

#[cfg(all(test, feature = "http-resolver"))]
mod http_resolver_tests {
    use super::*;

    #[test]
    fn url_escapes_spaces_and_special() {
        assert_eq!(url_escape_path("hi there.wav"), "hi%20there.wav");
        assert_eq!(url_escape_path("sub/dir/file.wav"), "sub/dir/file.wav");
        assert_eq!(url_escape_path("naïve.wav"), "na%C3%AFve.wav");
    }
}
