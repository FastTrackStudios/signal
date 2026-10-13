//! Media read-ahead worker — REAPER's "media buffering" thread.
//!
//! Streaming playback reads PCM through `mmap`; a page the kernel
//! hasn't cached yet means a **page fault inside the audio callback** —
//! measured ~14 ms on this machine's NVMe for a 90-track song played
//! cold, five times a 128-frame block's deadline. This worker walks the
//! items around the playhead several times a second and reads the next
//! few seconds of source audio into memory itself (`PcmFile::prefetch`:
//! a `WillNeed` hint, then a byte of every page), so the callback finds
//! the pages there.
//!
//! Native-only by construction (threads + mmap); WASM gets its audio
//! via `AudioSource::Memory` where there is nothing to prefetch.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::sync::Standalone;
use crate::transport_engine::TransportShared;

/// How far past the playhead to warm pages. REAPER's default media
/// buffer is 1.2 s; we read a little further because we re-scan only
/// every [`SCAN_INTERVAL`].
const LOOKAHEAD_SECONDS: f64 = 3.0;

/// Re-scan cadence. Each pass reads ~`LOOKAHEAD_SECONDS` of audio in
/// (see `PcmFile::prefetch`), so consecutive passes overlap heavily — a
/// missed pass costs nothing. Short, so a locate to audio not yet in
/// memory is read in before playback reaches it; a pass over audio that
/// is already in costs ~1 ms (a 90-track song).
const SCAN_INTERVAL: Duration = Duration::from_millis(150);

/// Guard handle: the worker thread runs until this is dropped.
pub struct PrefetchWorker {
    stop: Arc<AtomicBool>,
}

impl PrefetchWorker {
    /// Spawn the read-ahead thread for `project_guid`. The thread
    /// holds the project lock only long enough to snapshot
    /// `(source, range)` pairs; the `madvise` calls run unlocked.
    pub fn spawn(daw: Standalone, project_guid: String, shared: Arc<TransportShared>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let _ = std::thread::Builder::new()
            .name("fts-media-prefetch".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    let sr = shared.sample_rate().max(1);
                    let playhead = shared.playhead_samples().0.max(0) as f64 / sr as f64;
                    // Prefetch around the playhead even when stopped:
                    // a seek-then-play starts from warm pages.
                    let window_start = playhead;
                    let window_end = playhead + LOOKAHEAD_SECONDS;

                    // Snapshot (source, start_frame, frames) under the
                    // lock; advise after releasing it.
                    let wanted = daw
                        .read_project(&project_guid, |p| {
                            let mut out: Vec<(Arc<super::source::AudioSource>, usize, usize)> =
                                Vec::new();
                            for (item_guid, ie) in p.items.iter() {
                                let item = &ie.item;
                                let item_start = item.position.as_seconds();
                                let item_end = item_start + item.length.as_seconds();
                                if item_end <= window_start || item_start >= window_end {
                                    continue;
                                }
                                let Some(tl) = p.takes.get(item_guid) else {
                                    continue;
                                };
                                let Some(take) = tl.takes.get(tl.active_idx as usize) else {
                                    continue;
                                };
                                let Some(audio) = p.audio_sources.get(&take.guid) else {
                                    continue;
                                };
                                let rate = if take.play_rate.abs() < 1e-9 {
                                    1.0
                                } else {
                                    take.play_rate
                                };
                                let offs = take.start_offset.as_seconds();
                                // Item-relative window → source seconds.
                                let t0 = (window_start.max(item_start) - item_start) * rate + offs;
                                let t1 = (window_end.min(item_end) - item_start) * rate + offs;
                                if t1 <= 0.0 {
                                    continue;
                                }
                                let src_rate = audio.sample_rate().max(1) as f64;
                                let f0 = (t0.max(0.0) * src_rate) as usize;
                                let f1 = (t1 * src_rate) as usize;
                                if f1 > f0 {
                                    out.push((audio.clone(), f0, f1 - f0));
                                }
                            }
                            out
                        })
                        .unwrap_or_default();
                    for (audio, start, frames) in wanted {
                        audio.prefetch(start, frames);
                    }
                    std::thread::sleep(SCAN_INTERVAL);
                }
            });
        Self { stop }
    }
}

impl Drop for PrefetchWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
