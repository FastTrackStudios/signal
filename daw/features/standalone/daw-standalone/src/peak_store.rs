//! Persistent source-level waveform peaks — the `.sessionpeaks` cache.
//!
//! Take-level peaks depend on placement, play rate and stretch markers,
//! which is why [`crate::peak`]'s in-memory cache is revision-keyed. The
//! *expensive* part, though, is scanning the source PCM — and that is a
//! property of the media file alone. So the disk artifact lives at the
//! SOURCE level, in REAPER's own format, so REAPER and FTS share caches
//! where projects overlap.
//!
//! Where it sits is [`dawfile_reaper::sessionpeaks`]'s business: reads
//! try `Media/Peaks/<name>.sessionpeaks`, then REAPER's
//! `Media/peaks/<name>.reapeaks`, then `<name>.reapeaks` beside the
//! media; writes go to the first. A session `session peaks` has already
//! built, or REAPER has already scanned, costs this nothing.
//!
//! Flow: the first peaks request for a source loads a valid cache, or
//! scans the PCM once, writes the cache, and keeps the parsed mipmap in
//! a process-global map (keyed by media path, revalidated by stamp).
//! Cold starts after that fold coarse-zoom peaks from the cache instead
//! of rescanning gigabytes of PCM. Fine zooms (below the finest mipmap
//! ratio — `sr/300`, 160 samples/peak at 48 kHz) still read PCM: the
//! mipmap can't resolve them.
//!
//! Validation matches REAPER's model: the sidecar stores the source
//! file's size and mtime packed into one u64
//! ([`dawfile_reaper::reapeaks::stamp`]), and we additionally require
//! the channel count, sample rate and length (within one fine peak
//! window) to match the opened source. Anything stale is recomputed and
//! rewritten. Writing the same stamp REAPER writes is what keeps the
//! sidecars mutually usable — a bare timestamp there is a file REAPER
//! rebuilds the moment it opens the project.
//!
//! In-memory sources ([`AudioSource::Memory`], compressed decodes) get no
//! cache — their `min_max_block` is a RAM walk, cheap enough to redo.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use dawfile_reaper::reapeaks::ReaPeaks;
use dawfile_reaper::sessionpeaks;

use crate::audio_engine::AudioSource;

/// A cache is valid for `source` iff its recorded stamp matches the
/// media file and its shape matches the opened audio: same channels,
/// same rate, and the finest level covers the source length (peak count
/// is `ceil(frames / spp)`, so the covered length may exceed the frame
/// count by at most one window).
fn is_valid(pk: &ReaPeaks, source: &AudioSource, media_stamp: u64) -> bool {
    let Some(fine) = pk.levels.first() else {
        return false;
    };
    let frames = source.frame_count() as u64;
    let spp = fine.samples_per_peak.max(1) as u64;
    pk.source_stamp == media_stamp
        && pk.channels == source.channels().max(1) as usize
        && pk.samplerate == source.sample_rate()
        && fine.count as u64 == frames.div_ceil(spp)
}

/// How far a stand-in's length may differ from its original's, in
/// frames. An Ogg Vorbis proxy reports up to a block more than the WAV it
/// was made from — the encoder pads the end of the stream (measured: up
/// to ~1000 frames on real sessions) — and a different recording is off
/// by seconds, not by this.
const STAND_IN_SLACK: u64 = 8_192;

/// The shape half of [`is_valid`], for a stand-in: channels, rate, and
/// length to within [`STAND_IN_SLACK`].
fn fits(pk: &ReaPeaks, source: &AudioSource) -> bool {
    let Some(fine) = pk.levels.first() else {
        return false;
    };
    let frames = source.frame_count() as u64;
    let spp = fine.samples_per_peak.max(1) as u64;
    pk.channels == source.channels().max(1) as usize
        && pk.samplerate == source.sample_rate()
        && (fine.count as u64).abs_diff(frames.div_ceil(spp)) <= STAND_IN_SLACK.div_ceil(spp) + 1
}

/// The peaks of an original that is not on disk, for the stand-in
/// playing in its place (the proxy `Proxies/Bass.ogg` for `Bass.wav`).
///
/// The cache was built from the original — `Peaks/Bass.wav.sessionpeaks`,
/// fetched with the proxies — and describes the same audio. There is no
/// file to stamp it against, so it is trusted on shape: the stand-in's
/// channels, rate and length must match. Never built or written here:
/// scanning a streamed source would decode it whole.
pub(crate) fn for_stand_in(original: &Path, source: &AudioSource) -> Option<Arc<ReaPeaks>> {
    let (_, pk) = sessionpeaks::read_any(original).filter(|(_, pk)| fits(pk, source))?;
    Some(Arc::new(pk))
}

type Store = HashMap<PathBuf, (u64, Arc<ReaPeaks>)>;

/// Process-global parsed-sidecar map — `Standalone` is a cloneable
/// handle (same reason the peaks cache in [`crate::peak`] is global),
/// and one media file may back takes in several projects.
fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The parsed peaks for an on-disk media file: in-process map, else a
/// valid sidecar, else one PCM scan + sidecar write. `None` when the
/// media file doesn't exist (nothing to stamp the cache against).
pub(crate) fn get_or_build(media: &Path, source: &AudioSource) -> Option<Arc<ReaPeaks>> {
    let stamp = sessionpeaks::media_stamp(media)?;
    if let Ok(map) = store().lock()
        && let Some((cached, pk)) = map.get(media)
        && *cached == stamp
    {
        return Some(pk.clone());
    }

    let found = sessionpeaks::read_any(media).filter(|(_, pk)| is_valid(pk, source, stamp));
    let pk = match found {
        Some((_, pk)) => pk,
        None => {
            // Absent or stale: one scan of the source PCM, carrying the
            // media's stamp. The write is best-effort — a read-only
            // media directory just means the next cold start rescans.
            let mut pk = ReaPeaks::compute(
                source.channels().max(1) as usize,
                source.sample_rate(),
                source.frame_count(),
                |frame, ch| source.channel_interp(frame, frame, 0.0, ch),
            );
            pk.source_stamp = stamp;
            if let Err(err) = sessionpeaks::write(media, &pk) {
                tracing::warn!(
                    peaks.media = %media.display(),
                    peaks.write_error = %err,
                    "sessionpeaks write failed; peaks stay in-memory only"
                );
            }
            pk
        }
    };
    let pk = Arc::new(pk);
    if let Ok(mut map) = store().lock() {
        map.insert(media.to_path_buf(), (stamp, pk.clone()));
    }
    Some(pk)
}

/// Min/max over source frames `[lo, hi)` for one channel, folded from
/// the mipmap (REAPER's pick: the coarsest level resolving the span).
/// Peaks cover fixed absolute windows, so the result may be up to one
/// window wider than the exact range on each side — conservative
/// (never narrower than the true min/max). Returns `(min, max)`.
pub(crate) fn min_max_block(pk: &ReaPeaks, lo: usize, hi: usize, channel: usize) -> (f32, f32) {
    let level = pk.level_for((hi.saturating_sub(lo)) as f64);
    let per = (level.samples_per_peak as usize).max(1);
    let a = lo / per;
    let b = hi.div_ceil(per).min(level.count).max(a);
    let (mut max, mut min) = (f32::MIN, f32::MAX);
    for p in a..b {
        let (pmax, pmin) = level.pair(pk.channels, channel, p);
        max = max.max(pmax);
        min = min.min(pmin);
    }
    if max < min { (0.0, 0.0) } else { (min, max) }
}
