//! Audio source materialization — turn `source_file_path` strings on
//! takes into decoded PCM stashed in `ProjectState.audio_sources`.
//!
//! The function takes a caller-supplied resolver closure so the same
//! code works on native (read from disk) and WASM (file bytes
//! provided via browser upload / fetch). Decoding is via fts-sample
//! (`feature = "decode"` or `feature = "audio"`).
//!
//! Typical use:
//!
//! ```ignore
//! use daw_standalone::audio_engine::materialize::{materialize_audio, MaterializeReport};
//!
//! let report: MaterializeReport = materialize_audio(&daw, &project_guid, |path| {
//!     std::fs::read(path).map_err(|e| e.to_string())
//! });
//! eprintln!("loaded {} sources, skipped {}", report.loaded, report.failed.len());
//! ```

use std::sync::Arc;

use super::decoder::{DecodedAudio, decode_audio_with_extension};
use super::source::AudioSource;
use crate::sync::Standalone;

/// Per-take materialization outcome.
#[derive(Debug, Default)]
pub struct MaterializeReport {
    /// Number of takes whose audio was decoded + stashed.
    pub loaded: usize,
    /// Number of takes that had no source path (MIDI / empty).
    pub skipped_no_source: usize,
    /// `(take_guid, reason)` for takes whose resolver call or decode
    /// failed. Caller decides how to surface — log warnings, fail
    /// the load, retry, etc.
    pub failed: Vec<(String, String)>,
}

/// Walk every take in `project_guid` and call `resolve(path)` for
/// each one whose source has a file path. Successful decodes land in
/// `ProjectState.audio_sources[take_guid]`.
pub fn materialize_audio<F>(daw: &Standalone, project_guid: &str, resolve: F) -> MaterializeReport
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    materialize_audio_streaming(daw, project_guid, resolve, |_| None)
}

/// One take's media, not necessarily loaded yet: which take, its file,
/// and where its item sits in the timeline — so a loader can fetch and
/// open media in the order it will be heard (nearest the playhead first).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingMedia {
    pub take_guid: String,
    pub item_guid: String,
    pub track_guid: String,
    /// The take's source file, as the project names it.
    pub path: String,
    /// The item's span in the timeline, seconds.
    pub start: f64,
    pub end: f64,
    /// Seconds into the source where the item starts, and the rate it
    /// plays the source at — what maps a moment of the timeline to a
    /// moment of the file (`source = offset + (t - start) × rate`).
    pub source_offset: f64,
    pub playrate: f64,
}

/// Every take in `project_guid` that plays a file — what materializing the
/// project loads, take by take.
#[must_use]
pub fn pending_media(daw: &Standalone, project_guid: &str) -> Vec<PendingMedia> {
    daw.with_project(project_guid, |p| {
        let mut out = Vec::new();
        for (item_guid, take_list) in &p.takes {
            let span = p.items.get(item_guid).map(|entry| {
                let start = entry.item.position.as_seconds();
                (
                    entry.item.track_guid.clone(),
                    start,
                    start + entry.item.length.as_seconds(),
                )
            });
            for take in &take_list.takes {
                if let Some(path) = &take.source_file_path
                    && !path.is_empty()
                {
                    let (track_guid, start, end) = span.clone().unwrap_or_default();
                    out.push(PendingMedia {
                        take_guid: take.guid.clone(),
                        item_guid: item_guid.clone(),
                        track_guid,
                        path: path.clone(),
                        start,
                        end,
                        source_offset: take.start_offset.as_seconds(),
                        playrate: if take.play_rate > 0.0 {
                            take.play_rate
                        } else {
                            1.0
                        },
                    });
                }
            }
        }
        out
    })
    .unwrap_or_default()
}

/// Whether a take's media is loaded (it has a source to play).
#[must_use]
pub fn is_loaded(daw: &Standalone, project_guid: &str, take_guid: &str) -> bool {
    daw.with_project(project_guid, |p| p.audio_sources.contains_key(take_guid))
        .unwrap_or(false)
}

/// [`materialize_audio`] with a streaming fast path: when `resolve_path`
/// returns an on-disk file, uncompressed PCM is **memory-mapped instead of
/// decoded** — REAPER's model. Opening parses the header only, so a
/// multi-gigabyte session "materializes" in milliseconds with flat RAM;
/// the OS page cache streams samples during playback. Compressed formats
/// (and non-file sources) fall back to the byte resolver + full decode.
///
/// Every take through [`materialize_take`]: the one way a take's media is
/// loaded, all at once here, one at a time by a progressive loader.
pub fn materialize_audio_streaming<F, P>(
    daw: &Standalone,
    project_guid: &str,
    mut resolve: F,
    mut resolve_path: P,
) -> MaterializeReport
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
    P: FnMut(&str) -> Option<std::path::PathBuf>,
{
    let mut report = MaterializeReport::default();
    // The list is taken outside the project lock, so the resolver never
    // runs while holding it (it may do filesystem / network I/O).
    for pending in pending_media(daw, project_guid) {
        match materialize_take(
            daw,
            project_guid,
            &pending.take_guid,
            &pending.path,
            &mut resolve,
            &mut resolve_path,
        ) {
            Ok(()) => report.loaded += 1,
            Err(e) => report.failed.push((pending.take_guid, e)),
        }
    }

    // Count takes that legitimately had no source.
    let _ = daw.with_project(project_guid, |p| {
        for take_list in p.takes.values() {
            for take in &take_list.takes {
                let has_path = take
                    .source_file_path
                    .as_deref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
                if !has_path {
                    report.skipped_no_source += 1;
                }
            }
        }
    });

    report
}

/// Load one take's media: `path` (as the project names it) resolved to a
/// file or to bytes, then streamed, memory-mapped or decoded as its format
/// asks. The take plays from the next block.
///
/// # Errors
///
/// The resolver could not find the file, or it did not open / decode.
pub fn materialize_take<F, P>(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    path: &str,
    resolve: &mut F,
    resolve_path: &mut P,
) -> Result<(), String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
    P: FnMut(&str) -> Option<std::path::PathBuf>,
{
    #[cfg(target_arch = "wasm32")]
    let _ = &mut *resolve_path;
    // A proxy on disk streams from the file itself: nothing held in
    // memory but the few seconds decoded around the playhead.
    #[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
    if let Some(disk_path) = resolve_path(path)
        && disk_path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ogg"))
    {
        let stream = fts_sample::ogg_stream::OggStream::open_file(&disk_path)
            .map_err(|e| format!("ogg stream for {path}: {e}"))?;
        stream_ogg(daw, project_guid, take_guid, stream);
        return Ok(());
    }
    // Streaming fast path: mmap uncompressed PCM straight from disk.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(disk_path) = resolve_path(path) {
        // Open errors mean "not a plain RIFF/PCM file" — fall through
        // to decode.
        if let Ok(pcm) = super::source::PcmFile::open(&disk_path) {
            let _ = daw.with_project_mut(project_guid, |p| {
                p.audio_sources
                    .insert(take_guid.to_owned(), Arc::new(AudioSource::PcmFile(pcm)));
            });
            return Ok(());
        }
    }
    let bytes = resolve(path)?;
    // What the bytes ARE, before what the path says: a resolver may
    // hand back a stand-in (the proxy `Proxies/Bass.ogg` for a
    // `Bass.wav` that was never fetched), and decoding Ogg as WAV
    // fails every time.
    let ext = sniff_extension(&bytes).map_or_else(
        || path.rsplit('.').next().unwrap_or("").to_ascii_lowercase(),
        str::to_owned,
    );
    // An Ogg stand-in (a proxy) streams around the playhead rather than
    // decoding whole: resident, a setlist's proxies were 17 GB.
    #[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
    if ext == "ogg" {
        let bytes: Arc<[u8]> = bytes.into();
        let stream = fts_sample::ogg_stream::OggStream::open(bytes)
            .map_err(|e| format!("ogg stream for {path}: {e}"))?;
        stream_ogg(daw, project_guid, take_guid, stream);
        return Ok(());
    }
    // Resident decode: the whole file lands in RAM, charged against the
    // process-wide preload budget inside `DecodedAudio::charged` (over
    // budget still loads — playback must not silently fail). FUTURE
    // SEAM: compressed timeline streaming (a butler thread over
    // fts-sample's stream layer) replaces this eager decode.
    let decoded = decode_audio_with_extension(&bytes, &ext)
        .ok_or_else(|| format!("decode failed for {path}"))?;
    attach_audio_source(daw, project_guid, take_guid, decoded);
    Ok(())
}

/// [`materialize_take`] through the project Media Bay's resolver — what a
/// progressive loader calls per take.
///
/// # Errors
///
/// As [`materialize_take`].
pub fn materialize_take_via_bay(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    path: &str,
) -> Result<(), String> {
    let bay = daw.media_bay();
    materialize_take(
        daw,
        project_guid,
        take_guid,
        path,
        &mut |p: &str| bay.resolve_file(p),
        &mut |p: &str| bay.resolve_file_path(p),
    )
}

/// Attach decoded audio for a specific take, bypassing the resolver
/// flow. Useful for tests + injecting synthesized audio.
pub fn attach_audio_source(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    decoded: DecodedAudio,
) {
    let _ = daw.with_project_mut(project_guid, |p| {
        p.audio_sources.insert(
            take_guid.to_string(),
            Arc::new(AudioSource::Memory(decoded)),
        );
    });
}

/// Attach any source for a take — a [`Streamed`](super::streamed::Streamed)
/// one, decoded around the playhead from a proxy, is what a browser
/// attaches.
pub fn attach_source(daw: &Standalone, project_guid: &str, take_guid: &str, source: AudioSource) {
    let _ = daw.with_project_mut(project_guid, |p| {
        p.audio_sources
            .insert(take_guid.to_string(), Arc::new(source));
    });
}

/// Drop the decoded source for a take (e.g. unload to save memory
/// while keeping project structure).
pub fn detach_audio_source(daw: &Standalone, project_guid: &str, take_guid: &str) {
    let _ = daw.with_project_mut(project_guid, |p| {
        p.audio_sources.remove(take_guid);
    });
}

/// Convenience: materialize every take's audio via whatever
/// `BayFileResolver` is currently installed on the project Media Bay.
/// Same shape as [`materialize_audio`] but the resolver isn't passed
/// explicitly — the bay handles WASM vs native indirection.
///
/// Returns `Err` if no bay resolver is installed.
/// Attach an Ogg stream to a take as a [`super::streamed::Streamed`]
/// source, fed around the playhead by the native butler.
#[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
fn stream_ogg(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    stream: fts_sample::ogg_stream::OggStream,
) {
    use super::streamed::{StreamFeeder, Streamed};
    let streamed = Streamed::new(stream.channels(), stream.sample_rate(), stream.frames());
    let _ = daw.with_project_mut(project_guid, |p| {
        p.audio_sources.insert(
            take_guid.to_owned(),
            Arc::new(AudioSource::Streamed(streamed.clone())),
        );
    });
    super::streamed::butler_adopt(StreamFeeder::new(streamed, stream));
}

/// Attach a proxy streamed from elsewhere (Task, a peer, a share link) to
/// a take: its bytes as they arrive and its page index. The take plays
/// what has arrived and is silent where it has not; the reader records
/// what it wanted (`SparseBytes::wanted`) for the fetcher.
///
/// Returns the take's feeder, for whoever pumps feeders here: the butler
/// thread natively ([`stream_remote_ogg`]), the page's audio loop in a
/// browser.
#[cfg(feature = "stream-ogg")]
pub fn attach_remote_ogg(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    bytes: Arc<fts_sample::sparse::SparseBytes>,
    index: fts_sample::ogg_index::OggIndex,
) -> super::streamed::StreamFeeder<super::streamed::RemoteOgg> {
    use super::streamed::{RemoteOgg, StreamFeeder, Streamed};
    let streamed = Streamed::new(index.channels, index.sample_rate, index.frames);
    let _ = daw.with_project_mut(project_guid, |p| {
        p.audio_sources.insert(
            take_guid.to_owned(),
            Arc::new(AudioSource::Streamed(streamed.clone())),
        );
    });
    StreamFeeder::new(streamed, RemoteOgg::new(bytes, index))
}

/// [`attach_remote_ogg`], fed by the butler thread.
#[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
pub fn stream_remote_ogg(
    daw: &Standalone,
    project_guid: &str,
    take_guid: &str,
    bytes: Arc<fts_sample::sparse::SparseBytes>,
    index: fts_sample::ogg_index::OggIndex,
) {
    super::streamed::butler_adopt(attach_remote_ogg(
        daw,
        project_guid,
        take_guid,
        bytes,
        index,
    ));
}

/// The container a file's first bytes announce, when they announce one.
fn sniff_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"OggS") {
        Some("ogg")
    } else if bytes.starts_with(b"fLaC") {
        Some("flac")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Some("wav")
    } else {
        None
    }
}

pub fn materialize_via_bay(
    daw: &Standalone,
    project_guid: &str,
) -> Result<MaterializeReport, String> {
    let bay = daw.media_bay();
    Ok(materialize_audio_streaming(
        daw,
        project_guid,
        |path| bay.resolve_file(path),
        |path| bay.resolve_file_path(path),
    ))
}
