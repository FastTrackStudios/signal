//! A set streamed in from Task, played by this window's own engine: a live
//! set joined by its link (the public demo, or one someone shared), or a
//! setlist in an org's library, as a member.
//!
//! Each song comes in the one way a song streams in ([`crate::song_stream`]):
//! its small files mirrored to a cache on disk, so it opens the one way a
//! song opens ([`Setlist::open_streamed`]) with its media deferred; then
//! each take is attached to a stream of its proxy, and one fetcher per song
//! brings their bytes in, what the playhead will reach first first
//! ([`stream`]). The first song opens the set; the rest open behind it,
//! one by one ([`take_arrivals`]). A live set is joined as well
//! ([`crate::collab::join_task`]): its people, its leader, its edits.
//!
//! Blocking, like [`Setlist::open`]: a window runs it on a thread of its
//! own, and hears each step through `progress` — the loading screen's.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use daw::standalone::audio_engine::materialize::pending_media;
use daw::standalone::audio_engine::media_fetch::StreamedTake;

use crate::collab::TaskSet;
use crate::loading::Progress;
use crate::setlist::{Arrival, Opening, Setlist};
use crate::song_stream::{FetchGuard, Keep, ShareSource, SongSource, StreamedSong, TaskSource};

/// A setlist in an org's library, as [`setlists`] lists them.
pub use session_library::Setlist as LibrarySetlist;
/// A song in an org's library, and what a list is for — for the window's
/// list editor.
pub use session_library::{Library, ListKind, Song as LibrarySong};

/// A set Task keeps, to stream in.
pub enum Remote {
    /// A live share link (`https://…/org/<org>/share/<token>`): joined as a
    /// guest called `name`, each song from its files link.
    Live { link: String, name: String },
    /// A setlist in the org's library, joined as the member signed in to
    /// it, called `name`.
    Library {
        library: session_library::Library,
        setlist: session_library::Setlist,
        name: String,
    },
}

/// The library's setlists, for picking one.
///
/// # Errors
///
/// The library could not be reached, or refused.
pub fn setlists(library: &session_library::Library) -> eyre::Result<Vec<session_library::Setlist>> {
    crate::open::engine_runtime()?.block_on(library.setlists())
}

/// `work` against the library, to its end: what a window calls on a
/// thread of its own to change its lists (see `session_library::Library`'s
/// `create_list`, `set_songs` and the rest).
///
/// # Errors
///
/// The engine's runtime did not start, or `work` failed.
pub fn with_library<T, F>(
    library: &session_library::Library,
    work: impl FnOnce(session_library::Library) -> F,
) -> eyre::Result<T>
where
    F: std::future::Future<Output = eyre::Result<T>>,
{
    crate::open::engine_runtime()?.block_on(work(library.clone()))
}

/// Open `remote`'s songs into this window's engine, the first current, and
/// join the set. A song that cannot be brought in is left out, as a song
/// that will not open is.
///
/// # Errors
///
/// The set could not be joined, or none of its songs opened.
pub fn open(remote: Remote, progress: &dyn Fn(Progress)) -> eyre::Result<Setlist> {
    use crate::task_set::until_ok;
    let runtime = crate::open::engine_runtime()?;
    // What opens a song spawns tasks (`architect::platform::spawn`): on
    // the runtime, from this thread as from the one the rest open on.
    let _entered = runtime.enter();
    let (sources, set, name) = match remote {
        Remote::Live { link, name } => {
            let set = TaskSet::parse(&format!("share:{link}"));
            progress(Progress::Joining { retry: None });
            let joined = runtime.block_on(until_ok(
                Some(5),
                || crate::task_set::join(&set.url),
                |why| progress(Progress::Joining { retry: Some(why) }),
            ))?;
            tracing::info!(live.setlist = %joined.title, live.songs = joined.songs.len(), "stream-set: joined a live set");
            let sources: Vec<(String, Source)> = joined
                .songs
                .iter()
                .filter_map(|song| Some((song.title.clone(), Source::Share(song.files.clone()?))))
                .collect();
            let set = TaskSet {
                setlist: joined.setlist,
                ..set
            };
            (sources, set, name)
        }
        Remote::Library {
            library,
            setlist,
            name,
        } => {
            // Each song is looked up in the library when its turn comes —
            // not all of them before the first can open.
            let sources: Vec<(String, Source)> = setlist
                .songs
                .iter()
                .map(|song| {
                    (
                        song.title.clone(),
                        Source::Library(library.clone(), song.slug.clone()),
                    )
                })
                .collect();
            let set = TaskSet {
                url: library.org_url(),
                token: library.token.clone(),
                setlist: setlist.id.clone(),
            };
            (sources, set, name)
        }
    };
    let mut sources = sources.into_iter();
    let cache = cache_dir();
    // The first song that comes in opens the set; the rest open behind it.
    let (setlist, rest) = loop {
        let Some((title, source)) = sources.next() else {
            eyre::bail!("none of the set's songs could be brought in");
        };
        progress(Progress::Fetching {
            title: title.clone(),
            retry: None,
        });
        let song = match runtime.block_on(bring_in(&source, &cache)) {
            Ok(song) => song,
            Err(e) => {
                tracing::warn!(song.title = %title, error = %e, "stream-set: a song could not be brought in; the set goes on without it");
                continue;
            }
        };
        progress(Progress::Opening {
            title: title.clone(),
        });
        let mut setlist = Setlist::open_streamed(vec![Opening {
            path: song.project.clone(),
            title: Some(title),
            streamed: Some(song),
        }])?;
        let rest: Vec<(String, Source)> = sources.collect();
        setlist.pending = rest.iter().map(|(title, _)| title.clone()).collect();
        break (setlist, rest);
    };
    crate::collab::join_task_in_background(set, name);
    let (arrive, arrivals) = tokio::sync::mpsc::unbounded_channel();
    if let Ok(mut slot) = ARRIVALS.lock() {
        *slot = Some(arrivals);
    }
    std::thread::spawn(move || {
        let _entered = runtime.enter();
        for (title, source) in rest {
            let opened = runtime
                .block_on(bring_in(&source, &cache))
                .and_then(|song| {
                    Setlist::open_behind(Opening {
                        path: song.project.clone(),
                        title: Some(title.clone()),
                        streamed: Some(song),
                    })
                });
            let arrival = match opened {
                Ok(song) => Arrival::Song(song),
                Err(e) => {
                    tracing::warn!(song.title = %title, error = %e, "stream-set: a song did not open; the set goes on without it");
                    Arrival::Failed(title)
                }
            };
            if arrive.send(arrival).is_err() {
                break;
            }
        }
    });
    Ok(setlist)
}

/// Where a song of the set comes from.
enum Source {
    /// A live set's song: its files link.
    Share(String),
    /// A library setlist's song: its slug in the library.
    Library(session_library::Library, String),
}

impl Source {
    /// The song, reached: a share link parsed, or the library asked for it.
    async fn reach(&self) -> eyre::Result<Arc<dyn SongSource>> {
        Ok(match self {
            Self::Share(link) => Arc::new(ShareSource::new(link)?),
            Self::Library(library, slug) => Arc::new(TaskSource::of(library.clone(), slug).await?),
        })
    }
}

/// A song reached and its small files mirrored into `cache` — tried again
/// a few times, since Task may be busy.
async fn bring_in(source: &Source, cache: &std::path::Path) -> eyre::Result<StreamedSong> {
    let reached = source.reach().await?;
    crate::task_set::until_ok(
        Some(3),
        || crate::song_stream::mirror(Arc::clone(&reached), Keep::Disk(cache.to_path_buf())),
        |_| {},
    )
    .await
}

/// The set's songs after the first, as each opens — once, for the window
/// that shows them.
static ARRIVALS: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>> = Mutex::new(None);

/// The songs arriving behind the first (see [`open`]), for the window to
/// add to its set; `None` once taken, or when nothing is arriving.
pub fn take_arrivals() -> Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>> {
    ARRIVALS.lock().ok()?.take()
}

/// Where streamed songs' small files are kept: the same bytes at the same
/// version are not fetched twice.
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Session")
        .join("streamed")
}

/// A song's streams, kept playing for as long as the process runs.
struct Streaming {
    _song: StreamedSong,
    /// The song as opened here.
    project: String,
    /// The takes being fetched — the fetch reads this list as it goes, so
    /// what is added to it is fetched from then on.
    takes: Arc<Mutex<Vec<StreamedTake>>>,
    /// Heard by its reference ([`crate::reference_play`]): the stems,
    /// attached and not fetched, with the streams they fill.
    held: Vec<(
        StreamedTake,
        daw::standalone::audio_engine::streamed::Streamed,
    )>,
    _fetching: FetchGuard,
    /// The reference's own fetch, while it is heard by it.
    _reference: Option<FetchGuard>,
}

static STREAMS: Mutex<Vec<Streaming>> = Mutex::new(Vec::new());

/// Attach every take of the song just opened as `local` to a stream of its
/// proxy in `song`, and start bringing their bytes in from the song's
/// playhead on.
///
/// A song with a reference is heard by it until the multitracks are asked
/// for ([`crate::reference_play`]): only the reference's bytes are fetched,
/// and the tracks it leaves out (the guide) — they play live over it. The
/// other stems are attached (their waveforms draw) and held.
pub fn stream(song: StreamedSong, local: &str) {
    let Some(daw) = crate::open::local_engine() else {
        tracing::warn!("stream-set: no engine to stream into");
        return;
    };
    let playhead: Arc<dyn Fn() -> f64 + Send + Sync> = match daw.sync_backend(local) {
        Some(backend) => Arc::new(move || {
            daw_transport_sync::TransportBackend::snapshot(&backend)
                .map_or(0.0, |s| s.playhead_seconds)
        }),
        None => Arc::new(|| 0.0),
    };
    let reference = if crate::reference_play::multitracks() {
        None
    } else {
        song.reference()
    };
    let by_reference = reference.is_some();
    let reference_fetch = reference.map(|crate::song_stream::Reference { full, preview }| {
        let (preview_take, preview) = preview.unzip();
        crate::reference_play::hear(
            local,
            full.1.source().clone(),
            preview.as_ref().map(|p| p.source().clone()),
        );
        daw::standalone::audio_engine::streamed::butler_adopt(full.1);
        if let Some(preview) = preview {
            daw::standalone::audio_engine::streamed::butler_adopt(preview);
        }
        // The preview first: small, so it is heard almost at once.
        let takes = preview_take.into_iter().chain([full.0]).collect();
        let stop = Arc::new(AtomicBool::new(false));
        song.fetch(
            Arc::new(Mutex::new(takes)),
            Arc::clone(&playhead),
            Arc::clone(&stop),
        );
        FetchGuard(stop)
    });
    // Heard by its reference, the tracks it leaves out play live.
    let live = if by_reference {
        crate::reference::left_out(daw, local)
    } else {
        std::collections::HashSet::new()
    };
    let media = pending_media(daw, local);
    let mut takes = Vec::new();
    let mut held = Vec::new();
    for m in &media {
        let Some(attached) = song.attach(daw, local, m) else {
            tracing::warn!(stream.media = %m.path, "stream-set: no indexed proxy for this take");
            continue;
        };
        if by_reference && !live.contains(&m.track_guid) {
            held.push((attached.take, attached.source));
        } else {
            takes.push(attached.take);
        }
    }
    tracing::info!(
        stream.song = local,
        stream.media = media.len(),
        stream.fetched = takes.len(),
        stream.held = held.len(),
        stream.by_reference = by_reference,
        "stream-set: takes attached to their streams"
    );
    let takes = Arc::new(Mutex::new(takes));
    let stop = Arc::new(AtomicBool::new(false));
    song.fetch(Arc::clone(&takes), playhead, Arc::clone(&stop));
    if let Ok(mut streams) = STREAMS.lock() {
        streams.push(Streaming {
            _song: song,
            project: local.to_owned(),
            takes,
            held,
            _fetching: FetchGuard(stop),
            _reference: reference_fetch,
        });
    }
}

/// How long a song keeps its reference once its stems are asked for, at
/// most: a stem that will not come must not keep it on its reference.
const HANDOVER: std::time::Duration = std::time::Duration::from_secs(20);

/// Hear every streamed song by its stems: the held stems are fetched, and
/// each song's reference goes once its stems have all arrived at the
/// playhead (or after [`HANDOVER`]) — so the sound never drops out between
/// the two.
pub fn hear_stems() {
    let waiting: Vec<(
        String,
        Vec<(
            StreamedTake,
            daw::standalone::audio_engine::streamed::Streamed,
        )>,
    )> = STREAMS
        .lock()
        .map(|mut streams| {
            streams
                .iter_mut()
                .filter(|s| !s.held.is_empty())
                .map(|s| {
                    let held = std::mem::take(&mut s.held);
                    if let Ok(mut takes) = s.takes.lock() {
                        takes.extend(held.iter().map(|(take, _)| take.clone()));
                    }
                    (s.project.clone(), held)
                })
                .collect()
        })
        .unwrap_or_default();
    if waiting.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("session-stems-handover".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut waiting = waiting;
            while !waiting.is_empty() {
                let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
                let late = started.elapsed() >= HANDOVER;
                waiting.retain(|(project, stems)| {
                    let here = stems.iter().all(|(take, source)| arrived_at(take, source, at));
                    if here || late {
                        tracing::info!(stream.song = %project, stream.late = late, "stream-set: the stems take over from the reference");
                        crate::reference_play::stop(project);
                        false
                    } else {
                        true
                    }
                });
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "stream-set: no thread to hand over to the stems");
    }
}

/// Whether `take`'s audio has arrived where the song is at `at` seconds —
/// true for a take that does not play there.
fn arrived_at(
    take: &StreamedTake,
    source: &daw::standalone::audio_engine::streamed::Streamed,
    at: f64,
) -> bool {
    if at < take.start || at >= take.end {
        return true;
    }
    let seconds = (at - take.start) * take.playrate + take.source_offset;
    let frame = (seconds.max(0.0) * f64::from(source.sample_rate())) as usize;
    source.resident(frame / daw::standalone::audio_engine::streamed::CHUNK)
}

/// Where a download has got to: which song of how many, and that song's
/// proxies — bytes fetched, and in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloading {
    pub title: String,
    pub song: String,
    pub index: usize,
    pub count: usize,
    pub done: u64,
    pub total: u64,
}

/// The folder downloaded songs are kept in, under a device's documents:
/// one folder per song, shared by every set that has it.
pub const DOWNLOADED_SONGS: &str = "Songs";

/// The file a list downloaded as `title` is kept as, in `into`.
#[must_use]
pub fn download_file(into: &std::path::Path, title: &str) -> PathBuf {
    let name: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = if name.trim().is_empty() {
        "Download".to_owned()
    } else {
        name
    };
    into.join(format!("{name}.setlist"))
}

/// Download `songs` from `library` onto this device, to open with no
/// connection: each song's small files and its proxies (not the originals)
/// under `into/Songs/`, and `title` as a `.setlist` in `into` listing them
/// in order — what the start screen lists, and opens as any setlist on the
/// device. Songs downloaded already are not fetched again; a song that
/// cannot be brought is left out, as a set leaves out a song that will not
/// open. Blocking: run it on a thread of its own.
///
/// # Errors
///
/// None of the songs could be downloaded, or the setlist not written.
pub fn download(
    library: &session_library::Library,
    title: &str,
    songs: &[session_library::Song],
    into: &std::path::Path,
    offline: crate::song_stream::Offline,
    progress: &(dyn Fn(Downloading) + Sync),
) -> eyre::Result<Downloaded> {
    let runtime = crate::open::engine_runtime()?;
    let _entered = runtime.enter();
    let root = into.join(DOWNLOADED_SONGS);
    std::fs::create_dir_all(&root)?;
    let count = songs.len();
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    for (index, song) in songs.iter().enumerate() {
        let step = |done, total| {
            progress(Downloading {
                title: title.to_owned(),
                song: song.title.clone(),
                index,
                count,
                done,
                total,
            });
        };
        step(0, 0);
        let fetched = runtime.block_on(async {
            let source: Arc<dyn SongSource> =
                Arc::new(TaskSource::of(library.clone(), &song.slug).await?);
            crate::song_stream::download(source, &root, offline, &step).await
        });
        match fetched {
            Ok(folder) => {
                if let Ok(relative) = folder.strip_prefix(into) {
                    lines.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
            Err(e) => {
                tracing::warn!(song.title = %song.title, error = %e, "download: a song did not come down; the set goes on without it");
                failed.push((song.title.clone(), format!("{e:#}")));
            }
        }
    }
    if lines.is_empty() {
        // Why, in the words of the first song's failure: "none could be"
        // alone said nothing a person (or a bug report) could act on.
        let why = failed
            .first()
            .map_or_else(String::new, |(title, e)| format!(" — {title}: {e}"));
        eyre::bail!("none of its songs could be downloaded{why}");
    }
    let file = download_file(into, title);
    let mut text = format!("# {title}\n");
    for line in &lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(&file, text)?;
    tracing::info!(
        download.songs = lines.len(),
        download.failed = failed.len(),
        "download: a set is on this device"
    );
    Ok(Downloaded { file, failed })
}

/// A set downloaded: its `.setlist`, and the songs that did not come down
/// (title, why) — the set plays without them, and the person is told.
#[derive(Debug)]
pub struct Downloaded {
    pub file: PathBuf,
    pub failed: Vec<(String, String)>,
}

/// Take a downloaded set off this device: its `.setlist`, and every song
/// folder of it no other downloaded set still lists.
///
/// # Errors
///
/// A file could not be removed.
pub fn remove_download(into: &std::path::Path, file: &std::path::Path) -> eyre::Result<()> {
    let listed = |path: &std::path::Path| -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    };
    let mine = listed(file);
    let mut kept = std::collections::HashSet::new();
    for entry in std::fs::read_dir(into)?.flatten() {
        let other = entry.path();
        if other != file
            && other
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("setlist"))
        {
            kept.extend(listed(&other));
        }
    }
    std::fs::remove_file(file)?;
    for line in mine {
        // Only what the downloads keep: never a folder outside it.
        if line.starts_with(&format!("{DOWNLOADED_SONGS}/"))
            && !line.contains("..")
            && !kept.contains(&line)
        {
            let folder = into.join(&line);
            if folder.is_dir() {
                std::fs::remove_dir_all(&folder)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod download_tests {
    use super::{download_file, remove_download};

    #[test]
    fn a_downloaded_set_is_removed_but_not_the_songs_another_still_lists() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let into = dir.path();
        for song in ["a", "b", "c"] {
            std::fs::create_dir_all(into.join("Songs").join(song)).expect("a song folder");
        }
        std::fs::write(into.join("One.setlist"), "# One\nSongs/a\nSongs/b\n").expect("one");
        std::fs::write(into.join("Two.setlist"), "# Two\nSongs/b\nSongs/c\n").expect("two");
        remove_download(into, &into.join("One.setlist")).expect("removed");
        assert!(!into.join("One.setlist").exists());
        assert!(!into.join("Songs/a").exists(), "only One had a");
        assert!(into.join("Songs/b").exists(), "Two still lists b");
        assert!(into.join("Songs/c").exists());
        assert_eq!(
            download_file(into, "JHM Sunday / 9am"),
            into.join("JHM Sunday - 9am.setlist")
        );
    }
}

#[cfg(test)]
mod download_probe {
    /// The smallest setlist in the signed-in account's orgs, downloaded
    /// into a temporary folder as the start screen does it — through the
    /// library, not a share. Needs this machine to be signed in:
    /// `cargo test -p session-daw --lib download_probe -- --ignored`.
    #[test]
    #[ignore = "needs a signed-in account and the network"]
    fn a_library_setlist_downloads_and_lists_its_songs() {
        let account = crate::task_account::signed_in().expect("signed in on this machine");
        let orgs = crate::task_account::orgs(&account).expect("the orgs");
        let (library, list) = orgs
            .iter()
            .filter_map(|org| {
                let library = account.library(org);
                let lists = super::setlists(&library).ok()?;
                let list = lists
                    .into_iter()
                    .filter(|l| !l.songs.is_empty())
                    .min_by_key(|l| l.songs.len())?;
                Some((library, list))
            })
            .next()
            .unwrap_or_else(|| panic!("no setlist with songs in any of {orgs:?}"));
        let dir = tempfile::tempdir().expect("a temp dir");
        let offline = if std::env::var("FTS_PROBE_MULTITRACKS").is_ok() {
            crate::song_stream::Offline::Multitracks
        } else {
            crate::song_stream::Offline::Reference
        };
        let downloaded = super::download(
            &library,
            &list.title,
            &list.songs,
            dir.path(),
            offline,
            &|_| {},
        )
        .unwrap_or_else(|e| panic!("{} did not download: {e:#}", list.title));
        assert!(
            downloaded.failed.is_empty(),
            "{} came down without {} of its songs:\n{}",
            list.title,
            downloaded.failed.len(),
            downloaded
                .failed
                .iter()
                .map(|(song, why)| format!("  {song}: {why}\n"))
                .collect::<String>()
        );
        let file = downloaded.file;
        let text = std::fs::read_to_string(&file).expect("the .setlist");
        let songs: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert!(!songs.is_empty(), "{} lists no songs:\n{text}", list.title);
        for song in songs {
            let folder = dir.path().join(song);
            assert!(folder.is_dir(), "{song} is not a folder");
            assert!(
                crate::setlist::song_in(&folder).is_some(),
                "{song} has no song to open in it"
            );
            let proxies = folder.join("Media").join("Proxies");
            if offline == crate::song_stream::Offline::Reference
                && proxies.join("Reference.ogg").exists()
            {
                // By its reference: that, not its stems — and it opens from
                // disk as a song opened offline plays it.
                assert!(
                    proxies.join(crate::song_stream::REFERENCE_ONLY).exists(),
                    "{song} is not marked"
                );
                let project = format!("probe-{song}");
                crate::reference_play::hear_local(&project, &folder);
                assert!(
                    crate::reference_play::any(),
                    "{song}'s reference did not open from disk"
                );
                crate::reference_play::stop(&project);
            }
        }
        // Every proxy that came down, and how much.
        let bytes: u64 = walk(dir.path())
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "ogg"))
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        if offline == crate::song_stream::Offline::Reference {
            assert!(
                bytes < 40_000_000 * list.songs.len() as u64,
                "{} by its reference came to {bytes} bytes",
                list.title
            );
        }
    }

    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
}
