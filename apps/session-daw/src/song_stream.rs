//! A song streamed in from elsewhere, when it is not on this machine —
//! from the engine this window drives, or from the Task library.
//!
//! One way, whatever the source ([`SongSource`]) and wherever it runs: the
//! small files — the prepared `.session`, the chart, the lyrics, the
//! waveform caches, each proxy's page index — are mirrored ([`Keep`]: a
//! cache on disk natively, memory in a browser), so the song opens the one
//! way songs open (`open_core::open_song_in`, through the mirror's
//! [`crate::folder::Folder`]); the proxies are not: each is attached as a
//! take that plays what has arrived (`attach_remote_ogg` over a
//! `SparseBytes`), and one fetcher per song brings their bytes in the order
//! they will be heard (`media_fetch::drive`) by range-reads on the source.
//! The originals stay where they are.
//!
//! - [`PeerSource`] — the engine's song folder (`daw::rpc::SongFiles`).
//! - [`TaskSource`] — the song's session File Root in the library
//!   (`session_library::TaskSong`); its library chart replaces the folder's.
//! - [`ShareSource`] — a Task share link to the song's session (the public
//!   demo: no account): its JSON listing, its documents whole, its proxies
//!   as the link's audio rendition with HTTP ranges.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use daw::standalone::audio_engine::materialize::PendingMedia;
use daw::standalone::audio_engine::media_fetch::{FetchConfig, Fetching, RangeFetch, StreamedTake};
use daw::standalone::sync::Standalone;

use crate::folder::{Folder, Memory};
use fts_sample::ogg_index::OggIndex;
use fts_sample::sparse::SparseBytes;

/// Where a song's files come from.
#[cfg(not(target_arch = "wasm32"))]
pub trait SongSource: Send + Sync + 'static {
    /// A name for its cache folder.
    fn label(&self) -> String;
    /// Every file of the song folder with its size, the project first.
    fn list(&self) -> Pending<eyre::Result<Listing>>;
    /// The bytes `range` of `path`.
    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>>;
    /// The song's chart, when the source keeps it apart from the folder
    /// (the library's) — it replaces the folder's `.kf`.
    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        Box::pin(async { Ok(None) })
    }
    /// Many whole documents in one request, when the source has one: each
    /// path with its bytes, or `None` for a path to fetch on its own.
    /// `None` altogether when it has no such request (or it failed) — the
    /// caller reads them one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let _ = paths;
        Box::pin(async { None })
    }
    /// The whole of `path` written to the file `into`, in one request,
    /// adding each piece's length to `got` as it lands — when the source
    /// can stream a whole file. `None` when it cannot: a download then
    /// reads the file by ranges.
    fn fetch_whole(
        &self,
        path: String,
        into: PathBuf,
        got: Arc<std::sync::atomic::AtomicU64>,
    ) -> Pending<Option<eyre::Result<()>>> {
        let _ = (path, into, got);
        Box::pin(async { None })
    }
}

/// Where a song's files come from (see the native definition: here a
/// request is a JS promise, and not `Send`).
#[cfg(target_arch = "wasm32")]
pub trait SongSource: 'static {
    /// A name for its cache folder.
    fn label(&self) -> String;
    /// Every file of the song folder with its size, the project first.
    fn list(&self) -> Pending<eyre::Result<Listing>>;
    /// The bytes `range` of `path`.
    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>>;
    /// The song's chart, when the source keeps it apart from the folder
    /// (the library's) — it replaces the folder's `.kf`.
    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        Box::pin(async { Ok(None) })
    }
    /// Many whole documents in one request, when the source has one: each
    /// path with its bytes, or `None` for a path to fetch on its own.
    /// `None` altogether when it has no such request (or it failed) — the
    /// caller reads them one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let _ = paths;
        Box::pin(async { None })
    }
}

/// A song folder's files, and the version they are at when the source
/// says (a share link's commit): the same version is the same bytes at
/// every path, so what was fetched at it is kept and not fetched again.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    /// Each file and its size, the project first.
    pub entries: Vec<(String, u64)>,
    pub version: Option<String>,
}

impl From<Vec<(String, u64)>> for Listing {
    fn from(entries: Vec<(String, u64)>) -> Self {
        Self {
            entries,
            version: None,
        }
    }
}

/// A source's answer, later.
#[cfg(not(target_arch = "wasm32"))]
pub type Pending<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;
/// A source's answer, later.
#[cfg(target_arch = "wasm32")]
pub type Pending<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T>>>;

/// The engine this window drives: its project's song folder.
#[cfg(feature = "native")]
pub struct PeerSource {
    remote: String,
    files: daw::rpc::SongFiles,
}

#[cfg(feature = "native")]
impl PeerSource {
    /// The song folder of `remote` (a project on the facade's engine).
    ///
    /// # Errors
    ///
    /// No facade, or no such project.
    pub async fn new(remote: &str) -> eyre::Result<Self> {
        let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("no facade"))?;
        Ok(Self {
            remote: remote.to_owned(),
            files: daw.project(remote).await?.song_files(),
        })
    }
}

#[cfg(feature = "native")]
impl SongSource for PeerSource {
    fn label(&self) -> String {
        format!("peer-{}", self.remote)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let files = self.files.clone();
        Box::pin(async move {
            Ok(files
                .list()
                .await?
                .into_iter()
                .map(|f| (f.path, f.size))
                .collect::<Vec<_>>()
                .into())
        })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let files = self.files.clone();
        Box::pin(async move { files.read(&path, range).await.map_err(|e| e.to_string()) })
    }
}

/// A song in the Task library: its session File Root, and its chart.
#[cfg(feature = "native")]
pub struct TaskSource {
    library: session_library::Library,
    song: session_library::TaskSong,
}

#[cfg(feature = "native")]
impl TaskSource {
    /// The library song `slug` (`washed`), from the library the
    /// environment names (`FTS_TASK_SERVER`, `FTS_TASK_ORG`, the token).
    ///
    /// # Errors
    ///
    /// The library cannot be reached, or has no session for the song.
    pub async fn new(slug: &str) -> eyre::Result<Self> {
        Self::of(session_library::Library::from_env(), slug).await
    }

    /// The song `slug` in `library` — one a window signed in to.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub async fn of(library: session_library::Library, slug: &str) -> eyre::Result<Self> {
        let song = library.song(slug).await?;
        Ok(Self { library, song })
    }
}

#[cfg(feature = "native")]
impl SongSource for TaskSource {
    fn label(&self) -> String {
        format!("task-{}", self.song.slug)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        // The session's own charts give way to the library's.
        let entries: Vec<(String, u64)> = self
            .song
            .entries
            .iter()
            .filter(|(rel, _)| rel.contains('/') || !rel.to_lowercase().ends_with(".kf"))
            .cloned()
            .collect();
        Box::pin(async move { Ok(entries.into()) })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let song = self.song.clone();
        Box::pin(async move { song.read(&path, range).await.map_err(|e| e.to_string()) })
    }

    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        let library = self.library.clone();
        let slug = self.song.slug.clone();
        Box::pin(async move { library.song_chart(&slug).await })
    }

    /// The song's small files in one request (the server's
    /// `files/{root}/docs`) — its session's hundreds of objects and its
    /// waveforms, which read one by one took 19 s a song. `None` when the
    /// server has no such request, or it failed: they are read one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let url = self.song.docs_url();
        let token = self.song.token().map(str::to_owned);
        Box::pin(async move {
            let mut request = reqwest::Client::new()
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "text/plain")
                .body(paths.join("\n"));
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.ok()?;
            if !response.status().is_success() {
                tracing::warn!(
                    http.status = response.status().as_u16(),
                    "library: the documents batch was refused; reading them one by one"
                );
                return None;
            }
            let body = response.bytes().await.ok()?;
            Some(
                batch_records(&body)?
                    .into_iter()
                    .map(|(path, status, bytes)| (path, (status == 0).then_some(bytes)))
                    .collect(),
            )
        })
    }

    fn fetch_whole(
        &self,
        path: String,
        into: PathBuf,
        got: Arc<std::sync::atomic::AtomicU64>,
    ) -> Pending<Option<eyre::Result<()>>> {
        let song = self.song.clone();
        Box::pin(async move {
            Some(
                async {
                    use std::io::Write as _;
                    let mut file = std::io::BufWriter::new(std::fs::File::create(&into)?);
                    let mut failed = None;
                    song.read_to(&path, |piece| {
                        if failed.is_some() {
                            return;
                        }
                        match file.write_all(piece) {
                            Ok(()) => {
                                got.fetch_add(
                                    piece.len() as u64,
                                    std::sync::atomic::Ordering::Relaxed,
                                );
                            }
                            Err(e) => failed = Some(e),
                        }
                    })
                    .await?;
                    if let Some(e) = failed {
                        return Err(e.into());
                    }
                    file.flush()?;
                    Ok(())
                }
                .await,
            )
        })
    }
}

/// A Task share link to a song's session
/// (`https://task…/org/<org>/share/<token>`, `?pw=` when it has one).
pub struct ShareSource {
    link: reqwest::Url,
    password: Option<String>,
    http: reqwest::Client,
}

impl ShareSource {
    /// The session `link` shares.
    ///
    /// # Errors
    ///
    /// `link` is not a URL.
    pub fn new(link: &str) -> eyre::Result<Self> {
        let mut link = reqwest::Url::parse(link.trim().trim_end_matches('/'))?;
        let password = link
            .query_pairs()
            .find(|(k, _)| k == "pw")
            .map(|(_, v)| v.into_owned());
        link.set_query(None);
        Ok(Self {
            link,
            password,
            http: reqwest::Client::new(),
        })
    }

    /// The link's route `route` (`list`, `doc`, `rendition/audio`) for the
    /// song-folder path `path`, each segment escaped.
    fn url(&self, route: &str, path: &str) -> reqwest::Url {
        let mut url = self.link.clone();
        if let Ok(mut segments) = url.path_segments_mut() {
            segments
                .extend(route.split('/'))
                .extend(path.split('/').filter(|s| !s.is_empty()));
        }
        if let Some(pw) = &self.password {
            url.query_pairs_mut().append_pair("pw", pw);
        }
        url
    }

    /// The link's token, for the cache folder's name — never the password.
    fn token(&self) -> String {
        self.link
            .path_segments()
            .and_then(Iterator::last)
            .unwrap_or_default()
            .to_owned()
    }
}

/// A `docs` batch's records — `u8 status | u32le path length | path |
/// u32le length | bytes`, one per path (Task's
/// `share_documents_batch_handler`) — or `None` if the body is not one.
fn batch_records(mut body: &[u8]) -> Option<Vec<(String, u8, Vec<u8>)>> {
    fn take<'b>(body: &mut &'b [u8], n: usize) -> Option<&'b [u8]> {
        let (head, rest) = body.split_at_checked(n)?;
        *body = rest;
        Some(head)
    }
    fn len(body: &mut &[u8]) -> Option<usize> {
        let bytes: [u8; 4] = take(body, 4)?.try_into().ok()?;
        usize::try_from(u32::from_le_bytes(bytes)).ok()
    }
    let mut out = Vec::new();
    while !body.is_empty() {
        let status = take(&mut body, 1)?[0];
        let path_len = len(&mut body)?;
        let path = String::from_utf8(take(&mut body, path_len)?.to_vec()).ok()?;
        let bytes_len = len(&mut body)?;
        let bytes = take(&mut body, bytes_len)?.to_vec();
        out.push((path, status, bytes));
    }
    Some(out)
}

/// The media path whose audio rendition is the proxy at `proxy`
/// (`Media/Proxies/Bass.ogg` → `Media/Bass.ogg`: a link streams the
/// committed proxy beside a take by the take's stem), when it is one.
fn rendition_of(proxy: &str) -> Option<String> {
    let (dir, name) = proxy.rsplit_once('/')?;
    let parent = dir.strip_suffix("Proxies")?.trim_end_matches('/');
    let name = name.strip_suffix(".ogg")?;
    Some(if parent.is_empty() {
        format!("{name}.ogg")
    } else {
        format!("{parent}/{name}.ogg")
    })
}

impl SongSource for ShareSource {
    fn label(&self) -> String {
        format!("share-{}", self.token())
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let request = self.http.get(self.url("list", ""));
        Box::pin(async move {
            let listed: serde_json::Value =
                request.send().await?.error_for_status()?.json().await?;
            let mut entries: Vec<(String, u64)> = listed["entries"]
                .as_array()
                .ok_or_else(|| eyre::eyre!("the link lists no entries"))?
                .iter()
                .filter_map(|e| {
                    Some((
                        e["path"].as_str()?.to_owned(),
                        e["size"].as_u64().unwrap_or(0),
                    ))
                })
                .collect();
            // The project first: the top-level `.RPP`, as every source.
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            if let Some(at) = entries
                .iter()
                .position(|(p, _)| !p.contains('/') && p.to_lowercase().ends_with(".rpp"))
            {
                let project = entries.remove(at);
                entries.insert(0, project);
            }
            Ok(Listing {
                entries,
                version: listed["commit"].as_str().map(str::to_owned),
            })
        })
    }

    /// The link's `docs` batch: one POST for a whole song folder's small
    /// files (a Task that has no such route answers 404/405, and the
    /// caller reads them one by one).
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let request = self
            .http
            .post(self.url("docs", ""))
            // Plain text: a browser sends it without asking first.
            .header(reqwest::header::CONTENT_TYPE, "text/plain")
            .body(paths.join("\n"));
        Box::pin(async move {
            let response = request.send().await.ok()?.error_for_status().ok()?;
            let body = response.bytes().await.ok()?;
            let records = batch_records(&body)?;
            Some(
                records
                    .into_iter()
                    .map(|(path, status, bytes)| (path, (status == 0).then_some(bytes)))
                    .collect(),
            )
        })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let ranged = rendition_of(&path).map(|media| self.url("rendition/audio", &media));
        let request = match &ranged {
            Some(url) => self.http.get(url.clone()).header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range.start, range.end.saturating_sub(1)),
            ),
            // A document comes whole (it is small); the range is cut here.
            None => self.http.get(self.url("doc", &path)),
        };
        let partial = ranged.is_some();
        Box::pin(async move {
            let what = |e: &dyn std::fmt::Display| format!("{path} {range:?}: {e}");
            let response = request
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| what(&e))?;
            let served_range = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            let bytes = response.bytes().await.map_err(|e| what(&e))?;
            if partial && served_range {
                return Ok(bytes.to_vec());
            }
            let from = usize::try_from(range.start).unwrap_or(usize::MAX);
            let to = usize::try_from(range.end)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            bytes
                .get(from..to)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| what(&"shorter than asked"))
        })
    }
}

/// Where a mirrored song's small files are kept.
#[derive(Clone, Debug)]
pub enum Keep {
    /// A cache folder on this machine (under it, one folder per source),
    /// its proxies' bytes in `.part` files beside them.
    #[cfg(not(target_arch = "wasm32"))]
    Disk(PathBuf),
    /// Memory — a browser's; the proxies' bytes too.
    Memory(Memory),
}

impl Keep {
    /// The folder the mirrored files are read back from.
    fn folder(&self) -> Arc<dyn Folder> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => Arc::new(crate::folder::Disk),
            Self::Memory(memory) => Arc::new(memory.clone()),
        }
    }

    /// Whether a song-folder file (lower-case path) is mirrored here. A
    /// browser skips the waveform caches: its peak store reads them by
    /// disk path, which it has none of, so held in memory they would only
    /// be weight.
    fn wants(&self, lower: &str) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => true,
            Self::Memory(_) => !lower.starts_with("media/peaks/"),
        }
    }

    /// How the fetcher holds what it brings: all of it on disk; in memory,
    /// a window around the playhead (the rest is in the browser's cache).
    fn fetch_config(&self) -> FetchConfig {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => FetchConfig::default(),
            Self::Memory(_) => FetchConfig {
                resident: Some(daw::standalone::audio_engine::media_fetch::Resident::BROWSER),
                // A seek needs every stem's block at the new place at once,
                // and over the internet each one is a round trip: more of
                // them in flight is what gets the song sounding again.
                concurrency: 8,
                ..FetchConfig::default()
            },
        }
    }

    /// Where a source labelled `label` is mirrored.
    fn base(&self, label: &str) -> PathBuf {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(root) => root.join(safe_name(label)),
            Self::Memory(_) => Path::new("/").join(safe_name(label)),
        }
    }

    /// Whether `path` is already here at `size` bytes.
    fn has(&self, path: &Path, size: u64) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => std::fs::metadata(path).is_ok_and(|m| m.len() == size),
            Self::Memory(memory) => memory.len_of(path) == Some(size),
        }
    }

    fn write(&self, path: &Path, bytes: Vec<u8>) -> eyre::Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, bytes)?;
            }
            Self::Memory(memory) => memory.insert(path, bytes),
        }
        Ok(())
    }

    /// Room for the `size` bytes of the proxy at `path`, as they arrive.
    fn sparse(&self, path: &Path, size: u64) -> Option<Arc<SparseBytes>> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => {
                let store = PathBuf::from(format!("{}.part", path.display()));
                std::fs::create_dir_all(store.parent()?).ok()?;
                SparseBytes::on_disk(size, &store).ok()
            }
            Self::Memory(_) => {
                let _ = path;
                Some(SparseBytes::in_memory(size))
            }
        }
    }
}

/// A song mirrored from a source: where it opens, and its proxies to
/// stream.
pub struct StreamedSong {
    /// The project to open (in the mirror).
    pub project: PathBuf,
    /// The mirror it opens from.
    pub folder: Arc<dyn Folder>,
    keep: Keep,
    base: PathBuf,
    source: Arc<dyn SongSource>,
    /// Proxies by their file stem, lower-case: (path in the song, size).
    proxies: HashMap<String, (String, u64)>,
    /// Waveform caches by their original's file name, lower-case
    /// (`bass.wav`): (path in the song, size). Ours before REAPER's.
    peaks: HashMap<String, (String, u64)>,
    /// The streams attached so far, by their original's file name.
    attached: Mutex<Vec<(String, daw::standalone::audio_engine::streamed::Streamed)>>,
}

fn safe_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Whether a song-folder path is audio the fetcher streams (a proxy) or
/// leaves alone (an original): everything else is small, and mirrored.
fn is_media(lower: &str) -> bool {
    lower.starts_with("media/")
        && !lower.starts_with("media/peaks/")
        && !lower.ends_with(".ogg.idx")
}

/// What a download brings onto the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offline {
    /// The song's reference (its mix but the guide, one stereo proxy of a
    /// few megabytes, and its preview) and the tracks it leaves out — the
    /// guide — which play live over it: what a phone plays by default. A
    /// song with no reference comes down whole.
    Reference,
    /// Every proxy: each track of the song, to mix.
    Multitracks,
}

/// The file beside a song's proxies that says it came down by its
/// reference only ([`Offline::Reference`]): opened from disk, it is heard
/// by its reference ([`crate::reference_play::hear_local`]), its stems not
/// being there.
pub const REFERENCE_ONLY: &str = ".reference-only";

/// Bring a song onto this device, to open with no connection: its small
/// files mirrored into `root` (as [`mirror`] does, the source's chart
/// written in), and its proxies fetched whole beside them — every one, or
/// only its reference and guide (`offline`). The originals are never
/// fetched: a song opened from disk plays `Media/Proxies/<name>.ogg` where
/// its original is missing (the engine's `ProjectRelativeResolver`). A
/// file already here at its size is kept. `progress` hears the proxies'
/// bytes: fetched so far, and in all. Returns the song's folder.
///
/// # Errors
///
/// The source lists nothing, or a file could not be fetched or written.
#[cfg(not(target_arch = "wasm32"))]
pub async fn download(
    source: Arc<dyn SongSource>,
    root: &Path,
    offline: Offline,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> eyre::Result<PathBuf> {
    use futures_util::StreamExt as _;
    /// Proxies fetched at once: each is its own request, so a few side by
    /// side keep the connection full while one waits on the server.
    const AT_ONCE: usize = 4;
    /// Tries at one proxy before the song is given up on: a phone's
    /// connection drops a request now and then.
    const TRIES: u32 = 4;
    let keep = Keep::Disk(root.to_path_buf());
    let song = mirror(Arc::clone(&source), keep.clone()).await?;
    // By its reference: the reference, its preview, and the guide's own
    // proxies — or, a song with no reference, all of them.
    let only: Option<std::collections::HashSet<String>> = match offline {
        Offline::Reference if song.has_reference() => Some(reference_stems(&song.base)?),
        _ => None,
    };
    let marker = song.base.join("Media").join("Proxies").join(REFERENCE_ONLY);
    match &only {
        Some(_) => {
            if let Some(dir) = marker.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(
                &marker,
                "The stems of this song are not on this device: it plays its reference.\n",
            )?;
        }
        None => {
            let _ = std::fs::remove_file(&marker);
        }
    }
    let mut proxies: Vec<(String, u64)> = song
        .proxies
        .iter()
        .filter(|(stem, _)| {
            only.as_ref()
                .is_none_or(|only| only.contains(stem.as_str()))
        })
        .map(|(_, proxy)| proxy.clone())
        .collect();
    proxies.sort();
    let total: u64 = proxies.iter().map(|(_, size)| size).sum();
    // What has arrived: the proxies already here, and each one being
    // fetched by its own count (a failed try zeroes its own, not the rest).
    let mut here = 0;
    let mut wanted = Vec::new();
    for (path, size) in proxies {
        let local = song.base.join(&path);
        if keep.has(&local, size) {
            here += size;
        } else {
            wanted.push((
                path,
                size,
                local,
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
            ));
        }
    }
    let counts: Vec<Arc<std::sync::atomic::AtomicU64>> =
        wanted.iter().map(|(.., got)| Arc::clone(got)).collect();
    let done = || {
        (here
            + counts
                .iter()
                .map(|c| c.load(Ordering::Relaxed))
                .sum::<u64>())
        .min(total)
    };
    progress(done(), total);
    let fetching = futures_util::stream::iter(wanted)
        .map(|(path, size, local, got)| {
            let source = Arc::clone(&source);
            async move { fetch_proxy(&*source, &path, size, &local, &got, TRIES).await }
        })
        .buffer_unordered(AT_ONCE)
        .collect::<Vec<eyre::Result<()>>>();
    tokio::pin!(fetching);
    let outcomes = loop {
        tokio::select! {
            outcomes = &mut fetching => break outcomes,
            () = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                progress(done(), total);
            }
        }
    };
    progress(done(), total);
    for outcome in outcomes {
        outcome?;
    }
    Ok(song.base)
}

/// The proxies a song mirrored into `folder` is heard by, by its
/// reference: the reference and its preview, and the proxies of the tracks
/// the reference leaves out (the guide) — lower-case file stems, as a
/// streamed song keys its proxies. Found by opening the song, its media
/// left unloaded, into an engine of its own.
///
/// # Errors
///
/// The song could not be found in `folder` or opened.
#[cfg(feature = "native")]
fn reference_stems(folder: &Path) -> eyre::Result<std::collections::HashSet<String>> {
    let song = crate::setlist::song_in(folder)
        .ok_or_else(|| eyre::eyre!("no song in {}", folder.display()))?;
    let daw = daw::standalone::Standalone::new();
    let prepare = crate::prepare::Prepare::for_song(&song);
    let (opened, _) =
        crate::open::open_song_into_with(&daw, &song, &prepare, crate::open::Media::Deferred)?;
    let project = opened.project_guid;
    let live = crate::reference::left_out(&daw, &project);
    let mut stems: std::collections::HashSet<String> =
        [crate::reference::STEM, crate::reference::PREVIEW_STEM]
            .into_iter()
            .map(str::to_owned)
            .collect();
    stems.extend(
        daw::standalone::audio_engine::materialize::pending_media(&daw, &project)
            .into_iter()
            .filter(|m| live.contains(&m.track_guid))
            .filter_map(|m| {
                Path::new(&m.path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase())
            }),
    );
    Ok(stems)
}

/// Without the engine to open a song in, a song downloaded by its
/// reference comes down whole.
#[cfg(not(feature = "native"))]
fn reference_stems(_folder: &Path) -> eyre::Result<std::collections::HashSet<String>> {
    eyre::bail!("no engine here to find a song's guide in")
}

/// Fetch proxy `path` (`size` bytes) of `source` to `local`: streamed
/// whole where the source can, else read by ranges — written aside and
/// moved in whole, so a download cut short never leaves a proxy that looks
/// complete. `got` is this proxy's own count, each piece added as it lands;
/// a failed try sets it back to nothing and starts again, `tries` times at
/// most.
#[cfg(not(target_arch = "wasm32"))]
async fn fetch_proxy(
    source: &dyn SongSource,
    path: &str,
    size: u64,
    local: &Path,
    got: &Arc<std::sync::atomic::AtomicU64>,
    tries: u32,
) -> eyre::Result<()> {
    use std::io::Write as _;
    /// A range read, where the source cannot stream a file: well inside
    /// the library's per-read timeout on a slow connection.
    const RANGE: u64 = 1024 * 1024;
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = PathBuf::from(format!("{}.download", local.display()));
    let mut tried = 0;
    loop {
        tried += 1;
        got.store(0, Ordering::Relaxed);
        let outcome = match source
            .fetch_whole(path.to_owned(), part.clone(), Arc::clone(got))
            .await
        {
            Some(outcome) => outcome,
            None => {
                async {
                    let mut file = std::fs::File::create(&part)?;
                    let mut at = 0;
                    while at < size {
                        let end = (at + RANGE).min(size);
                        let bytes = source
                            .read(path.to_owned(), at..end)
                            .await
                            .map_err(|e| eyre::eyre!("{path}: {e}"))?;
                        if bytes.is_empty() {
                            eyre::bail!("{path}: the source sent nothing at {at} of {size}");
                        }
                        file.write_all(&bytes)?;
                        at += bytes.len() as u64;
                        got.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                    file.flush()?;
                    Ok(())
                }
                .await
            }
        };
        match outcome {
            Ok(()) => {
                std::fs::rename(&part, local)?;
                return Ok(());
            }
            Err(e) => {
                got.store(0, Ordering::Relaxed);
                let _ = std::fs::remove_file(&part);
                if tried >= tries {
                    return Err(e);
                }
                tracing::warn!(download.path = %path, download.tried = tried, error = %e, "download: a proxy did not come down; trying again");
                architect::platform::sleep(std::time::Duration::from_millis(
                    500 * u64::from(tried),
                ))
                .await;
            }
        }
    }
}

/// Mirror a song from `source` into `keep`: every small file fetched whole
/// (skipped when already there at its size), the media left to stream; the
/// source's own chart, if it keeps one, written in.
///
/// # Errors
///
/// The source lists nothing, or a file could not be fetched or kept.
pub async fn mirror(source: Arc<dyn SongSource>, keep: Keep) -> eyre::Result<StreamedSong> {
    use futures_util::{StreamExt as _, TryStreamExt as _};
    /// Files fetched at once: each is a round trip, and most are a few
    /// hundred bytes (a session's objects — one per MIDI item), so what
    /// they cost is latency, not bandwidth. At 8 a prepared song's 138
    /// objects took 17 round trips (four seconds in a browser); HTTP/2 and
    /// /3 carry this many on one connection.
    const AT_ONCE: usize = 32;
    let Listing {
        entries: list,
        version,
    } = source.list().await?;
    let first = list
        .first()
        .ok_or_else(|| eyre::eyre!("the song folder is empty"))?;
    let label = source.label();
    let base = keep.base(&label);
    let project = base.join(&first.0);
    let mut proxies = HashMap::new();
    let mut peaks: HashMap<String, (String, u64)> = HashMap::new();
    let mut wanted = Vec::new();
    for (path, size) in &list {
        let lower = path.to_lowercase();
        if let Some(cache) = lower.strip_prefix("media/peaks/") {
            // `bass.wav.sessionpeaks` or `bass.wav.reapeaks`: ours wins.
            if let Some((original, ext)) = cache.rsplit_once('.')
                && (ext == "sessionpeaks" || (ext == "reapeaks" && !peaks.contains_key(original)))
            {
                peaks.insert(original.to_owned(), (path.clone(), *size));
            }
        }
        if lower.starts_with("media/proxies/") && lower.ends_with(".ogg") {
            let stem = Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            proxies.insert(stem, (path.clone(), *size));
            continue;
        }
        if is_media(&lower) || *size == 0 || !keep.wants(&lower) {
            continue;
        }
        let local = base.join(path);
        if keep.has(&local, *size) {
            continue;
        }
        wanted.push((path.clone(), *size, local));
    }
    let fetched = wanted.len();
    let docs = Docs::new(&source, &label, version.as_deref()).await;
    // What is kept already, then the rest in one batch where the source
    // has one, then whatever the batch did not bring, one by one.
    let looked: Vec<((String, u64, PathBuf), Option<Vec<u8>>)> = futures_util::stream::iter(wanted)
        .map(|want| {
            let docs = &docs;
            async move {
                let kept = docs.cached(&want.0).await;
                (want, kept)
            }
        })
        .buffer_unordered(AT_ONCE)
        .collect()
        .await;
    let mut arrived: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut missing = Vec::new();
    for (want, kept) in looked {
        match kept {
            Some(bytes) => arrived.push((want.2, bytes)),
            None => missing.push(want),
        }
    }
    let mut batched = 0;
    if missing.len() > 1 {
        let paths = missing.iter().map(|(path, ..)| path.clone()).collect();
        if let Some(got) = source.read_many(paths).await {
            let mut got: HashMap<String, Vec<u8>> = got
                .into_iter()
                .filter_map(|(path, bytes)| Some((path, bytes?)))
                .collect();
            let mut rest = Vec::new();
            for (path, size, local) in missing {
                match got.remove(&path) {
                    Some(bytes) => {
                        docs.store(&path, &bytes).await;
                        batched += 1;
                        arrived.push((local, bytes));
                    }
                    None => rest.push((path, size, local)),
                }
            }
            missing = rest;
        }
    }
    let one_by_one: Vec<(PathBuf, Vec<u8>)> = futures_util::stream::iter(missing)
        .map(|(path, size, local)| {
            let docs = &docs;
            async move { Ok::<_, eyre::Report>((local, docs.read(path, size).await?)) }
        })
        .buffer_unordered(AT_ONCE)
        .try_collect()
        .await?;
    arrived.extend(one_by_one);
    for (local, bytes) in arrived {
        keep.write(&local, bytes)?;
    }
    if let Some(chart) = source.chart().await? {
        write_chart(&keep, &project, &label, &chart)?;
    }
    tracing::info!(
        stream.source = %label,
        stream.files = list.len(),
        stream.fetched = fetched,
        stream.kept = docs.kept(),
        stream.batched = batched,
        stream.proxies = proxies.len(),
        "song-stream: mirrored"
    );
    Ok(StreamedSong {
        project,
        folder: keep.folder(),
        keep,
        base,
        source,
        proxies,
        peaks,
        attached: Mutex::default(),
    })
}

/// A song's documents (its small files), read whole: in a browser, through
/// its Cache Storage when the source says what version they are at — so a
/// page opening the song again (a playground set starting over, a reload)
/// fetches none of them until the song's files change.
///
/// A session's objects are cached by their own name whatever the version:
/// each is named by the hash of its bytes, so a name read once is those
/// bytes for good — in every song that shares it, at every commit.
struct Docs<'a> {
    source: &'a Arc<dyn SongSource>,
    #[cfg(target_arch = "wasm32")]
    cache: Option<web_sys::Cache>,
    /// Where this version's documents are kept in `cache`: `None` when the
    /// source does not say which version it lists.
    #[cfg(target_arch = "wasm32")]
    versioned: Option<String>,
    kept: std::sync::atomic::AtomicUsize,
}

/// A content-addressed object's name, if `path` is one: `…/objects/sha256-…`.
#[cfg(any(target_arch = "wasm32", test))]
fn object_name(path: &str) -> Option<&str> {
    let (_, name) = path.rsplit_once("/objects/")?;
    (name.starts_with("sha256-") && !name.contains('/')).then_some(name)
}

impl<'a> Docs<'a> {
    #[allow(unused_variables)]
    async fn new(source: &'a Arc<dyn SongSource>, label: &str, version: Option<&str>) -> Docs<'a> {
        #[cfg(target_arch = "wasm32")]
        let cache = CachedFetch::cache().await.ok();
        #[cfg(target_arch = "wasm32")]
        let versioned = version.map(|version| {
            format!(
                "https://fts-cache.invalid/{}/docs/{}",
                safe_name(label),
                safe_name(version)
            )
        });
        Docs {
            source,
            #[cfg(target_arch = "wasm32")]
            cache,
            #[cfg(target_arch = "wasm32")]
            versioned,
            kept: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// How many came from the cache.
    fn kept(&self) -> usize {
        self.kept.load(Ordering::Relaxed)
    }

    /// Where `path` is kept in the cache, if it is kept at all.
    #[cfg(target_arch = "wasm32")]
    fn key(&self, path: &str) -> Option<String> {
        match (object_name(path), &self.versioned) {
            (Some(name), _) => Some(format!("https://fts-cache.invalid/objects/{name}")),
            (None, Some(base)) => Some(format!("{base}/{}", url_path(path))),
            (None, None) => None,
        }
    }

    /// `path` from the cache, when it is there.
    #[allow(clippy::unused_async, unused_variables)]
    async fn cached(&self, path: &str) -> Option<Vec<u8>> {
        #[cfg(target_arch = "wasm32")]
        if let (Some(cache), Some(key)) = (&self.cache, self.key(path)) {
            let bytes = cache_get(cache, &key).await;
            if bytes.is_some() {
                self.kept.fetch_add(1, Ordering::Relaxed);
            }
            return bytes;
        }
        None
    }

    /// Keep `bytes` as `path`. A cache that is full or refused only costs
    /// a fetch next time.
    #[allow(clippy::unused_async, unused_variables)]
    async fn store(&self, path: &str, bytes: &[u8]) {
        #[cfg(target_arch = "wasm32")]
        if let (Some(cache), Some(key)) = (&self.cache, self.key(path)) {
            let mut owned = bytes.to_vec();
            if let Ok(response) = web_sys::Response::new_with_opt_u8_array(Some(&mut owned)) {
                let _ =
                    wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&key, &response)).await;
            }
        }
    }

    /// `path` whole: kept, else fetched and kept.
    async fn read(&self, path: String, size: u64) -> eyre::Result<Vec<u8>> {
        if let Some(bytes) = self.cached(&path).await {
            return Ok(bytes);
        }
        let bytes = self
            .source
            .read(path.clone(), 0..size)
            .await
            .map_err(|e| eyre::eyre!(e))?;
        self.store(&path, &bytes).await;
        Ok(bytes)
    }
}

/// A path's segments, each escaped for a cache key.
#[cfg(target_arch = "wasm32")]
fn url_path(path: &str) -> String {
    path.split('/')
        .map(|s| String::from(js_sys::encode_uri_component(s)))
        .collect::<Vec<_>>()
        .join("/")
}

/// What `cache` holds under `key`, if anything.
#[cfg(target_arch = "wasm32")]
async fn cache_get(cache: &web_sys::Cache, key: &str) -> Option<Vec<u8>> {
    use wasm_bindgen::JsCast as _;
    let hit = wasm_bindgen_futures::JsFuture::from(cache.match_with_str(key))
        .await
        .ok()?;
    let response = hit.dyn_into::<web_sys::Response>().ok()?;
    let body = wasm_bindgen_futures::JsFuture::from(response.array_buffer().ok()?)
        .await
        .ok()?;
    Some(js_sys::Uint8Array::new(&body).to_vec())
}

/// The source's chart, as THE chart beside `project` (the folder's own
/// gave way to it in the source's listing).
fn write_chart(keep: &Keep, project: &Path, label: &str, chart: &str) -> eyre::Result<()> {
    match keep {
        #[cfg(feature = "native")]
        Keep::Disk(_) => {
            if let Some(folder) = project.parent() {
                session_library::write_chart(folder, label, chart)?;
            }
            Ok(())
        }
        #[allow(unreachable_patterns)]
        _ => {
            let stem = project
                .file_stem()
                .map_or_else(|| label.to_owned(), |s| s.to_string_lossy().into_owned());
            keep.write(
                &project.with_file_name(format!("{stem}.kf")),
                chart.as_bytes().to_vec(),
            )
        }
    }
}

/// A take attached as a stream of its proxy: what to fetch for it, and the
/// audio it plays from (its waveform too, once handed in).
pub struct Attached {
    pub take: StreamedTake,
    pub source: daw::standalone::audio_engine::streamed::Streamed,
}

impl StreamedSong {
    /// Attach `media` (a take of the song opened into `daw` as `local`) as
    /// a stream of its proxy — silent until its bytes arrive. `None` when
    /// the peer has no indexed proxy for it.
    pub fn attach(&self, daw: &Standalone, local: &str, media: &PendingMedia) -> Option<Attached> {
        let stem = Path::new(&media.path)
            .file_stem()?
            .to_string_lossy()
            .to_lowercase();
        let (path, size) = self.proxies.get(&stem)?.clone();
        let at = self.base.join(&path);
        let index =
            OggIndex::from_text(&self.folder.read_to_string(&OggIndex::path_for(&at)).ok()?)?;
        let bytes = self.keep.sparse(&at, size)?;
        let feeder = daw::standalone::audio_engine::materialize::attach_remote_ogg(
            daw,
            local,
            &media.take_guid,
            Arc::clone(&bytes),
            index.clone(),
        );
        let source = feeder.source().clone();
        if let (Ok(mut attached), Some(name)) =
            (self.attached.lock(), Path::new(&media.path).file_name())
        {
            attached.push((
                name.to_string_lossy().to_lowercase(),
                feeder.source().clone(),
            ));
        }
        // Natively the butler thread decodes it; in a browser, the page's
        // audio loop.
        #[cfg(not(target_arch = "wasm32"))]
        daw::standalone::audio_engine::streamed::butler_adopt(feeder);
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        crate::web_audio::adopt(local, feeder.boxed());
        #[cfg(all(not(feature = "web"), target_arch = "wasm32"))]
        drop(feeder);
        Some(Attached {
            take: StreamedTake {
                path,
                bytes,
                index,
                start: media.start,
                end: media.end,
                source_offset: media.source_offset,
                playrate: media.playrate,
            },
            source,
        })
    }

    /// The song's reference ([`crate::reference`]) as streams of their own
    /// — attached to no take: what a page in reference mode plays instead
    /// of the stems. Its preview too, when it has one: small enough to be
    /// heard almost at once, played until the reference proper has arrived
    /// where the song is. `None` when the song has no indexed reference.
    pub fn reference(&self) -> Option<Reference> {
        Some(Reference {
            full: self.loose_stream(crate::reference::STEM)?,
            preview: self.loose_stream(crate::reference::PREVIEW_STEM),
        })
    }

    /// The proxy of stem `stem` as a stream attached to no take: its fetch
    /// (a [`StreamedTake`] spanning it from 0 s) and its feeder, silent
    /// until the bytes arrive.
    fn loose_stream(&self, stem: &str) -> Option<LooseStream> {
        use daw::standalone::audio_engine::streamed::{RemoteOgg, StreamFeeder, Streamed};
        let (path, size) = self.proxies.get(stem)?.clone();
        let at = self.base.join(&path);
        let index =
            OggIndex::from_text(&self.folder.read_to_string(&OggIndex::path_for(&at)).ok()?)?;
        let bytes = self.keep.sparse(&at, size)?;
        let seconds = index.frames as f64 / f64::from(index.sample_rate.max(1));
        let feeder = StreamFeeder::new(
            Streamed::new(index.channels, index.sample_rate, index.frames),
            RemoteOgg::new(Arc::clone(&bytes), index.clone()),
        );
        Some((
            StreamedTake {
                path,
                bytes,
                index,
                start: 0.0,
                end: seconds,
                source_offset: 0.0,
                playrate: 1.0,
            },
            feeder,
        ))
    }

    /// The proxy paths a song is heard by in `heard`.
    fn paths_of(&self, heard: Heard) -> std::collections::HashSet<&str> {
        let reference = [crate::reference::STEM, crate::reference::PREVIEW_STEM];
        self.proxies
            .iter()
            .filter(|(stem, _)| match heard {
                Heard::Preview => stem.as_str() == crate::reference::PREVIEW_STEM,
                Heard::Reference => stem.as_str() == crate::reference::STEM,
                Heard::Stems => !reference.contains(&stem.as_str()),
            })
            .map(|(_, (path, _))| path.as_str())
            .collect()
    }

    /// Whether the song has a reference to be heard by.
    #[must_use]
    pub fn has_reference(&self) -> bool {
        self.proxies.contains_key(crate::reference::STEM)
    }

    /// The bytes of the song's stems — every proxy but the reference: what
    /// loading the multitracks brings in.
    #[must_use]
    pub fn stems_bytes(&self) -> u64 {
        let stems = self.paths_of(Heard::Stems);
        self.proxies
            .values()
            .filter(|(path, _)| stems.contains(path.as_str()))
            .map(|(_, size)| size)
            .sum()
    }

    /// Fetch each attached take's waveform — its original's peaks cache,
    /// reduced for a view that never draws blocks finer than `block`
    /// samples ([`daw::standalone::reapeaks::ReaPeaks::coarsened`]) — and
    /// hand it to the take's stream, one at a time. Returns how many took
    /// one. For a holder with no disk to find the caches on (a browser); a
    /// mirror on disk has them where the peak store looks.
    pub async fn load_peaks(&self, block: u32) -> usize {
        use daw::standalone::reapeaks::ReaPeaks;
        let attached = self.attached.lock().map(|a| a.clone()).unwrap_or_default();
        let mut loaded = 0usize;
        for (original, streamed) in attached {
            let Some((path, size)) = self.peaks.get(&original).cloned() else {
                continue;
            };
            match self.source.read(path.clone(), 0..size).await {
                Ok(bytes) => match ReaPeaks::parse(&bytes) {
                    Ok(peaks) => {
                        streamed.set_peaks(Arc::new(peaks.coarsened(block)));
                        loaded = loaded.saturating_add(1);
                    }
                    Err(e) => {
                        tracing::warn!(media = %path, error = %e, "song-stream: a waveform cache did not parse")
                    }
                },
                Err(e) => {
                    tracing::warn!(media = %path, error = %e, "song-stream: a waveform cache did not arrive")
                }
            }
        }
        tracing::info!(stream.source = %self.source.label(), stream.waveforms = loaded, "song-stream: waveforms in");
        loaded
    }

    /// Start fetching `takes`' bytes in the order they will be heard from
    /// `playhead`; stops when `stop` is set.
    pub fn fetch(
        &self,
        takes: Arc<Mutex<Vec<StreamedTake>>>,
        playhead: Arc<dyn Fn() -> f64 + Send + Sync>,
        stop: Arc<AtomicBool>,
    ) {
        let fetch: Arc<dyn RangeFetch> = Arc::new(SourceFetch(Arc::clone(&self.source)));
        // In a browser, through its cache: what the window let go comes
        // back from there, and a reload streams nothing twice. (Filling the
        // cache ahead of playing is [`Self::warm`], the page's to run.)
        #[cfg(target_arch = "wasm32")]
        let fetch: Arc<dyn RangeFetch> = Arc::new(CachedFetch::new(
            fetch,
            &self.source.label(),
            self.versions(),
        ));
        let driving = daw::standalone::audio_engine::media_fetch::drive(
            takes,
            fetch,
            playhead,
            self.keep.fetch_config(),
            stop,
        );
        #[cfg(feature = "native")]
        if let Some(runtime) = crate::open::runtime() {
            runtime.spawn(driving);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(driving);
    }
}

impl StreamedSong {
    /// Bring what the song is `heard` by into the browser's cache before it
    /// is played: every block — so a seek anywhere in the song is local — or
    /// only the first `blocks` of each (the song's opening, for the song
    /// after the one on screen), until done or `stop`.
    #[cfg(target_arch = "wasm32")]
    pub async fn warm(&self, heard: Heard, blocks: Option<u64>, stop: Arc<AtomicBool>) {
        let fetch: Arc<dyn RangeFetch> = Arc::new(SourceFetch(Arc::clone(&self.source)));
        let wanted = self.paths_of(heard);
        let mut files = self.versions();
        files.retain(|path, _| wanted.contains(path.as_str()));
        CachedFetch::new(fetch, &self.source.label(), files)
            .warm(blocks, stop)
            .await;
    }

    /// Each proxy's size and version (its page index's hash: a proxy made
    /// again is indexed again), by its path in the song.
    #[cfg(target_arch = "wasm32")]
    fn versions(&self) -> HashMap<String, (u64, u64)> {
        self.proxies
            .values()
            .map(|(path, size)| {
                let index = self
                    .folder
                    .read(&OggIndex::path_for(&self.base.join(path)))
                    .unwrap_or_default();
                (path.clone(), (*size, fnv(&index)))
            })
            .collect()
    }
}

/// What a song is heard by: its reference's preview, its reference, or
/// its stems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Preview,
    Reference,
    Stems,
}

/// A stream attached to no take: its fetch and its feeder.
pub type LooseStream = (
    StreamedTake,
    daw::standalone::audio_engine::streamed::StreamFeeder<
        daw::standalone::audio_engine::streamed::RemoteOgg,
    >,
);

/// A song's reference, as streams: the reference proper, and its preview.
pub struct Reference {
    pub full: LooseStream,
    pub preview: Option<LooseStream>,
}

/// FNV-1a: a stable, dependency-free hash for cache keys.
#[cfg(target_arch = "wasm32")]
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A song's proxies through the browser's Cache Storage — off the wasm
/// heap, and kept across reloads: each proxy in aligned blocks, a block
/// answered from the cache when it is there and fetched (then kept) when
/// not. What the resident window lets go comes back from here, and
/// [`Self::warm`] fills the cache with the whole song in the background,
/// so a seek anywhere is local once it has.
#[cfg(target_arch = "wasm32")]
pub struct CachedFetch {
    inner: Arc<dyn RangeFetch>,
    label: String,
    /// Size and version by path.
    files: HashMap<String, (u64, u64)>,
}

#[cfg(target_arch = "wasm32")]
impl CachedFetch {
    /// Bytes per cached block.
    const BLOCK: u64 = 256 * 1024;
    /// The Cache Storage cache the blocks live in.
    const CACHE: &'static str = "fts-session-media";

    fn new(inner: Arc<dyn RangeFetch>, label: &str, files: HashMap<String, (u64, u64)>) -> Self {
        Self {
            inner,
            label: safe_name(label),
            files,
        }
    }

    /// The cache key of block `block` of `path` (a URL: Cache Storage keys
    /// are requests; this origin is never fetched).
    fn key(&self, path: &str, version: u64, block: u64) -> String {
        let path = url_path(path);
        format!(
            "https://fts-cache.invalid/{}/{version:016x}/{path}/{block}",
            self.label
        )
    }

    async fn cache() -> Result<web_sys::Cache, String> {
        use wasm_bindgen::JsCast as _;
        let caches = web_sys::window()
            .ok_or("no window")?
            .caches()
            .map_err(|e| format!("no cache storage: {e:?}"))?;
        let cache = wasm_bindgen_futures::JsFuture::from(caches.open(Self::CACHE))
            .await
            .map_err(|e| format!("{e:?}"))?;
        cache.dyn_into().map_err(|_| "not a cache".to_owned())
    }

    /// Block `block` of `path`: from the cache, or fetched and kept.
    async fn block(
        &self,
        cache: &web_sys::Cache,
        path: &str,
        block: u64,
    ) -> Result<Vec<u8>, String> {
        let (size, version) = self
            .files
            .get(path)
            .copied()
            .ok_or_else(|| format!("{path}: not a proxy of this song"))?;
        let key = self.key(path, version, block);
        if let Some(bytes) = cache_get(cache, &key).await {
            return Ok(bytes);
        }
        let start = block * Self::BLOCK;
        let mut bytes = self
            .inner
            .fetch(path, start..(start + Self::BLOCK).min(size))
            .await?;
        if let Ok(response) = web_sys::Response::new_with_opt_u8_array(Some(&mut bytes)) {
            // A cache that is full or refused only costs a fetch next time.
            let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&key, &response)).await;
        }
        Ok(bytes)
    }

    /// Whether block `block` of `path` is in the cache already.
    async fn cached(&self, cache: &web_sys::Cache, path: &str, block: u64) -> bool {
        let Some((_, version)) = self.files.get(path).copied() else {
            return false;
        };
        wasm_bindgen_futures::JsFuture::from(cache.match_with_str(&self.key(path, version, block)))
            .await
            .is_ok_and(|hit| !hit.is_undefined())
    }

    /// Fill the cache with every proxy's blocks — all of them, or the
    /// first `limit` of each — until done or `stop`: several requests at
    /// once (each is a round trip), block by block across every file so
    /// the whole band's next stretch comes before any one track's end.
    /// A block already there is only looked up, not read.
    async fn warm(&self, limit: Option<u64>, stop: Arc<AtomicBool>) {
        use futures_util::StreamExt as _;
        /// Blocks in flight at once.
        const AT_ONCE: usize = 8;
        let Ok(cache) = Self::cache().await else {
            return;
        };
        let mut files: Vec<(&String, &(u64, u64))> = self.files.iter().collect();
        files.sort();
        let blocks = files
            .iter()
            .map(|(_, (size, _))| size.div_ceil(Self::BLOCK))
            .max()
            .unwrap_or(0);
        let blocks = limit.map_or(blocks, |limit| blocks.min(limit));
        let wanted = (0..blocks).flat_map(|block| {
            files
                .iter()
                .filter(move |(_, (size, _))| block * Self::BLOCK < *size)
                .map(move |(path, _)| (path.as_str(), block))
        });
        let cache = &cache;
        let stop = &stop;
        futures_util::stream::iter(wanted)
            .map(|(path, block)| async move {
                if stop.load(Ordering::Relaxed) || self.cached(cache, path, block).await {
                    return;
                }
                if let Err(e) = self.block(cache, path, block).await {
                    tracing::debug!(media = %path, block, error = %e, "song-stream: warming the cache skipped a block");
                }
            })
            .buffer_unordered(AT_ONCE)
            .for_each(|()| async {})
            .await;
        if !stop.load(Ordering::Relaxed) {
            tracing::info!(stream.source = %self.label, stream.files = files.len(), stream.blocks = ?limit, "song-stream: in the browser's cache");
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl RangeFetch for CachedFetch {
    fn fetch(&self, path: &str, range: std::ops::Range<u64>) -> Fetching {
        let this = Self {
            inner: Arc::clone(&self.inner),
            label: self.label.clone(),
            files: self.files.clone(),
        };
        let path = path.to_owned();
        Box::pin(async move {
            if range.is_empty() {
                return Ok(Vec::new());
            }
            let cache = Self::cache().await?;
            let mut out = Vec::with_capacity(usize::try_from(range.end - range.start).unwrap_or(0));
            for block in range.start / Self::BLOCK..=(range.end - 1) / Self::BLOCK {
                let bytes = this.block(&cache, &path, block).await?;
                let at = block * Self::BLOCK;
                let from = usize::try_from(range.start.saturating_sub(at)).unwrap_or(0);
                let to = usize::try_from((range.end - at).min(bytes.len() as u64)).unwrap_or(0);
                out.extend_from_slice(
                    bytes
                        .get(from..to)
                        .ok_or_else(|| format!("{path}: block {block} is short"))?,
                );
            }
            Ok(out)
        })
    }
}

/// How many of `takes` have all they play.
#[must_use]
pub fn arrived(takes: &Mutex<Vec<StreamedTake>>) -> usize {
    takes
        .lock()
        .map(|t| t.iter().filter(|t| t.complete()).count())
        .unwrap_or(0)
}

/// Bytes from a song's source.
struct SourceFetch(Arc<dyn SongSource>);

impl RangeFetch for SourceFetch {
    fn fetch(&self, path: &str, range: std::ops::Range<u64>) -> Fetching {
        self.0.read(path.to_owned(), range)
    }
}

/// Stop a song's fetcher when dropped.
pub struct FetchGuard(pub Arc<AtomicBool>);

impl Drop for FetchGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_docs_batch_reads_back_record_by_record() {
        let mut body = Vec::new();
        for (status, path, bytes) in [(0_u8, "Song.RPP", &b"<REAPER"[..]), (2, "take.txt", b"")] {
            body.push(status);
            body.extend_from_slice(&u32::try_from(path.len()).unwrap().to_le_bytes());
            body.extend_from_slice(path.as_bytes());
            body.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
            body.extend_from_slice(bytes);
        }
        assert_eq!(
            batch_records(&body),
            Some(vec![
                ("Song.RPP".to_owned(), 0, b"<REAPER".to_vec()),
                ("take.txt".to_owned(), 2, Vec::new()),
            ])
        );
        body.pop();
        assert_eq!(batch_records(&body), None, "a cut body is not a batch");
    }

    #[test]
    fn only_a_sessions_hashed_objects_are_cached_by_name() {
        assert_eq!(
            object_name("Song.session/objects/sha256-0123abcd"),
            Some("sha256-0123abcd")
        );
        assert_eq!(object_name("Song.session/Song.session"), None);
        assert_eq!(object_name("Song.session/objects/notes.txt"), None);
        assert_eq!(object_name("Media/Proxies/Bass.ogg.idx"), None);
    }

    #[test]
    fn a_proxy_is_the_rendition_of_its_take_stem() {
        assert_eq!(
            rendition_of("Media/Proxies/Keys 1.ogg").as_deref(),
            Some("Media/Keys 1.ogg")
        );
        assert_eq!(
            rendition_of("Proxies/Bass.ogg").as_deref(),
            Some("Bass.ogg")
        );
        assert_eq!(rendition_of("Media/Proxies/Bass.ogg.idx"), None);
        assert_eq!(rendition_of("Media/Bass.wav"), None);
    }

    #[test]
    fn a_share_link_escapes_paths_and_keeps_its_password_out_of_the_label() {
        let source = ShareSource::new("http://task.test/org/demo/share/tok123?pw=secret").unwrap();
        assert_eq!(source.label(), "share-tok123");
        assert_eq!(
            source.url("doc", "Media/Proxies/Keys 1.ogg.idx").as_str(),
            "http://task.test/org/demo/share/tok123/doc/Media/Proxies/Keys%201.ogg.idx?pw=secret"
        );
        assert_eq!(
            source.url("list", "").as_str(),
            "http://task.test/org/demo/share/tok123/list?pw=secret"
        );
    }

    /// Probe: a whole song from a live share link through the fetcher
    /// (`FTS_PROBE_LINK=<link> cargo test -p session-daw --lib probe -- --ignored --nocapture`).
    #[test]
    #[ignore = "needs a live Task share link"]
    fn probe_share_link_throughput() {
        let link = std::env::var("FTS_PROBE_LINK").unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let source: Arc<dyn SongSource> = Arc::new(ShareSource::new(&link).unwrap());
            let cache = std::env::temp_dir().join(format!("fts-probe-{}", std::process::id()));
            let t = std::time::Instant::now();
            let song = mirror(Arc::clone(&source), Keep::Disk(cache.clone()))
                .await
                .unwrap();
            eprintln!("mirror {:?}", t.elapsed());
            let mut takes = Vec::new();
            for (path, size) in song.proxies.values() {
                let index = OggIndex::from_text(
                    &std::fs::read_to_string(OggIndex::path_for(&song.base.join(path))).unwrap(),
                )
                .unwrap();
                let secs = index.frames as f64 / f64::from(index.sample_rate);
                takes.push(StreamedTake {
                    path: path.clone(),
                    bytes: SparseBytes::in_memory(*size),
                    index,
                    start: 0.0,
                    end: secs,
                    source_offset: 0.0,
                    playrate: 1.0,
                });
            }
            let total: u64 = takes.iter().map(|t| t.bytes.len()).sum();
            let takes = Arc::new(Mutex::new(takes));
            let t = std::time::Instant::now();
            daw::standalone::audio_engine::media_fetch::drive(
                takes,
                Arc::new(SourceFetch(source)),
                {
                    // A playhead moving from 30 s, as a playing remote's.
                    let began = std::time::Instant::now();
                    Arc::new(move || 30.0 + began.elapsed().as_secs_f64())
                },
                FetchConfig::default(),
                Arc::new(AtomicBool::new(false)),
            )
            .await;
            eprintln!("fetched {total} bytes in {:?}", t.elapsed());
            let _ = std::fs::remove_dir_all(cache);
        });
    }
}
