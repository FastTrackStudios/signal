//! Fetching streamed media in the order it will be heard.
//!
//! A take streamed from elsewhere (Task, a peer) plays what has arrived
//! ([`super::streamed::RemoteOgg`] over a [`SparseBytes`]). What arrives
//! next is decided here, for every streamed take at once, by one measure:
//! **how soon it will be heard**. For each take the bytes under
//! `[playhead, playhead + horizon]` — the item's span mapped through its
//! source offset and rate to the file's frames, then through its page
//! index to bytes — that have not arrived, split into bounded requests so
//! a change of plan (a seek) takes effect within one request; the take's
//! header first of all, and any range a reader actually hit
//! ([`SparseBytes::wanted`]) before any plan. The horizon starts at a few
//! seconds and widens as what is near is in, so every track's next
//! seconds come before any track's later minutes, and a 30-minute file is
//! fetched from where it is being played, not from its start.
//!
//! [`plan`] is the pure decision; [`drive`] runs it against a
//! [`RangeFetch`] (the transport: HTTP ranges from Task, a range-read over
//! iroh from a peer), a few requests in flight, re-planning as bytes land
//! and the playhead moves.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fts_sample::ogg_index::OggIndex;
use fts_sample::sparse::SparseBytes;

/// A take whose media streams in.
#[derive(Clone)]
pub struct StreamedTake {
    /// What the fetcher asks for (the file, as its source names it).
    pub path: String,
    pub bytes: Arc<SparseBytes>,
    pub index: OggIndex,
    /// The item's span in the timeline, seconds.
    pub start: f64,
    pub end: f64,
    /// Where in the file the item starts (seconds) and the rate it plays
    /// the file at.
    pub source_offset: f64,
    pub playrate: f64,
}

impl std::fmt::Debug for StreamedTake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamedTake")
            .field("path", &self.path)
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}

impl StreamedTake {
    /// The bytes this take can ever play: its header, and the pages under
    /// its span — not the rest of a file an item plays only part of.
    #[must_use]
    pub fn needed(&self) -> [Range<u64>; 2] {
        let span = self.index.bytes_for(
            self.frame_at(self.start).saturating_sub(PREROLL),
            self.frame_at(self.end),
        );
        [self.index.header(), span]
    }

    /// Whether everything this take can play has arrived.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.needed().iter().all(|r| self.bytes.has(r))
    }

    /// Whether what this take plays over the `ahead` seconds from the
    /// timeline moment `t` has arrived — true where the take plays nothing
    /// then. What a holder swapping one stream for another waits on, so
    /// the swap is not a gap.
    #[must_use]
    pub fn has_at(&self, t: f64, ahead: f64) -> bool {
        let (from, to) = (t.max(self.start), (t + ahead).min(self.end));
        from >= to
            || (self.bytes.has(&self.index.header())
                && self.bytes.has(&self.index.bytes_for(
                    self.frame_at(from).saturating_sub(PREROLL),
                    self.frame_at(to),
                )))
    }

    /// The file frame the timeline moment `t` plays.
    fn frame_at(&self, t: f64) -> u64 {
        let seconds = ((t - self.start) * self.playrate + self.source_offset).max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frame = (seconds * f64::from(self.index.sample_rate)) as u64;
        frame.min(self.index.frames)
    }
}

/// One range to fetch, and how soon what it holds will be heard (seconds;
/// below zero: before anything planned — the header, a reader's miss).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub take: usize,
    pub range: Range<u64>,
    pub heard_in: f64,
}

/// How soon a take's header is needed: before anything.
const HEADER: f64 = -2.0;
/// How soon a range a reader hit is needed: before any plan.
const WANTED: f64 = -1.0;
/// Frames before a stretch a decoder primes from.
const PREROLL: u64 = 4096;
/// How soon what is behind the playhead is heard: after everything in
/// front of it (next time round — a loop, a jump back, a second pass).
const BEHIND: f64 = 1.0e6;

/// Everything missing under `[playhead, playhead + horizon]` across
/// `takes`, the soonest heard first, in requests of at most `max_request`
/// bytes. An unbounded horizon plans what is behind the playhead too, after
/// all of what is in front: a take is only done when all it plays is here.
#[must_use]
pub fn plan(takes: &[StreamedTake], playhead: f64, horizon: f64, max_request: u64) -> Vec<Request> {
    let max_request = max_request.max(1);
    let mut out = Vec::new();
    let mut push = |take: usize, range: Range<u64>, heard_from: f64, heard_to: f64| {
        let len = range.end.saturating_sub(range.start).max(1);
        let mut at = range.start;
        while at < range.end {
            let end = at.saturating_add(max_request).min(range.end);
            #[allow(clippy::cast_precision_loss)]
            let through = (at - range.start) as f64 / len as f64;
            out.push(Request {
                take,
                range: at..end,
                heard_in: (heard_to - heard_from).mul_add(through, heard_from),
            });
            at = end;
        }
    };
    for (i, take) in takes.iter().enumerate() {
        for missing in take.bytes.missing(&take.index.header()) {
            push(i, missing, HEADER, HEADER);
        }
        if let Some(wanted) = take.bytes.wanted() {
            for missing in take.bytes.missing(&wanted) {
                push(i, missing, WANTED, WANTED);
            }
        }
        let from = playhead.max(take.start);
        let to = (playhead + horizon).min(take.end);
        if horizon.is_infinite() && take.start < from {
            let behind = take.index.bytes_for(
                take.frame_at(take.start).saturating_sub(PREROLL),
                take.frame_at(from),
            );
            let span = behind.end.saturating_sub(behind.start).max(1);
            for missing in take.bytes.missing(&behind) {
                #[allow(clippy::cast_precision_loss)]
                let at = |b: u64| {
                    (from - take.start).mul_add(
                        (b.saturating_sub(behind.start)) as f64 / span as f64,
                        BEHIND,
                    )
                };
                let (a, b) = (at(missing.start), at(missing.end));
                push(i, missing, a, b);
            }
        }
        if from >= to {
            continue;
        }
        let bytes = take.index.bytes_for(
            take.frame_at(from).saturating_sub(PREROLL),
            take.frame_at(to),
        );
        let span = bytes.end.saturating_sub(bytes.start).max(1);
        for missing in take.bytes.missing(&bytes) {
            // How soon each part is heard: where it sits in the stretch.
            #[allow(clippy::cast_precision_loss)]
            let at = |b: u64| {
                (to - from).mul_add(
                    (b.saturating_sub(bytes.start)) as f64 / span as f64,
                    from - playhead,
                )
            };
            let (a, b) = (at(missing.start), at(missing.end));
            push(i, missing, a, b);
        }
    }
    out.sort_by(|a, b| a.heard_in.total_cmp(&b.heard_in).then(a.take.cmp(&b.take)));
    out
}

/// A fetch in flight: `Send` natively (it may run on any worker); in a
/// browser a request is a JS promise, which is not, and there is only the
/// one thread.
#[cfg(not(target_arch = "wasm32"))]
pub type Fetching =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send>>;
/// A fetch in flight (see the native definition).
#[cfg(target_arch = "wasm32")]
pub type Fetching = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>>>>;

/// Where a streamed take's bytes come from.
#[cfg(not(target_arch = "wasm32"))]
pub trait RangeFetch: Send + Sync + 'static {
    /// The bytes `range` of the file `path` names.
    fn fetch(&self, path: &str, range: Range<u64>) -> Fetching;
}

/// Where a streamed take's bytes come from (see the native definition).
#[cfg(target_arch = "wasm32")]
pub trait RangeFetch: 'static {
    /// The bytes `range` of the file `path` names.
    fn fetch(&self, path: &str, range: Range<u64>) -> Fetching;
}

/// How [`drive`] fetches.
#[derive(Clone, Debug)]
pub struct FetchConfig {
    /// Requests in flight at once.
    pub concurrency: usize,
    /// The most bytes one request asks for.
    pub max_request: u64,
    /// The horizons tried, nearest first: the plan is the nearest that
    /// still has something missing.
    pub horizons: Vec<f64>,
    /// How often to re-plan when nothing has landed (a playhead moving).
    pub tick: Duration,
    /// Hold only a window of each take in memory (a browser); `None`
    /// fetches everything and keeps it (natively, where it is on disk).
    pub resident: Option<Resident>,
}

/// The stretch of each take held in memory, around the playhead. The rest
/// is let go ([`SparseBytes::keep_only`]) and fetched again when it comes
/// near — from wherever the fetcher gets it (a local cache first, ideally).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resident {
    /// Seconds kept behind the playhead.
    pub behind: f64,
    /// Seconds fetched and kept in front of it: the furthest horizon.
    pub ahead: f64,
}

impl Resident {
    /// A browser's: 30 s in front, 5 s behind — a few MB for a whole band.
    pub const BROWSER: Self = Self {
        behind: 5.0,
        ahead: 30.0,
    };
}

impl StreamedTake {
    /// The bytes to hold for the window `window` around `playhead`: the
    /// header, and the pages under the part of the take inside it.
    fn window(&self, playhead: f64, window: Resident) -> Vec<Range<u64>> {
        let from = (playhead - window.behind).max(self.start);
        let to = (playhead + window.ahead).min(self.end);
        let mut keep = vec![self.index.header()];
        if from < to {
            keep.push(self.index.bytes_for(
                self.frame_at(from).saturating_sub(PREROLL),
                self.frame_at(to),
            ));
        }
        keep
    }
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            max_request: 256 * 1024,
            horizons: vec![3.0, 10.0, 30.0, 120.0, f64::INFINITY],
            tick: Duration::from_millis(50),
            resident: None,
        }
    }
}

/// Fetch `takes`' media in the order it will be heard until everything
/// has arrived or `stop` is set. `playhead` is read each re-plan. Failed
/// requests are logged and tried again on a later plan. With a
/// [`Resident`] window it holds only that window of each take, and follows
/// the playhead until `stop`.
pub async fn drive(
    takes: Arc<Mutex<Vec<StreamedTake>>>,
    fetch: Arc<dyn RangeFetch>,
    playhead: Arc<dyn Fn() -> f64 + Send + Sync>,
    config: FetchConfig,
    stop: Arc<AtomicBool>,
) {
    use futures::StreamExt as _;
    #[cfg(not(target_arch = "wasm32"))]
    type InFlight = std::pin::Pin<
        Box<dyn std::future::Future<Output = (Request, Result<Vec<u8>, String>)> + Send>,
    >;
    #[cfg(target_arch = "wasm32")]
    type InFlight =
        std::pin::Pin<Box<dyn std::future::Future<Output = (Request, Result<Vec<u8>, String>)>>>;
    let mut in_flight: futures::stream::FuturesUnordered<InFlight> =
        futures::stream::FuturesUnordered::new();
    let mut flying: Vec<Request> = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let snapshot: Vec<StreamedTake> = takes.lock().map(|t| t.clone()).unwrap_or_default();
        let now = playhead();
        if let Some(window) = config.resident {
            for take in &snapshot {
                take.bytes.keep_only(&take.window(now, window));
            }
        }
        let reach = config.resident.map_or(f64::INFINITY, |w| w.ahead);
        // The nearest horizon with something still missing — never past
        // the window, when there is one.
        let wanted = config
            .horizons
            .iter()
            .map(|&h| h.min(reach))
            .map(|h| plan(&snapshot, now, h, config.max_request))
            .find(|p| p.iter().any(|r| !overlaps(&flying, r)))
            .unwrap_or_default();
        for request in wanted {
            if flying.len() >= config.concurrency.max(1) {
                break;
            }
            if overlaps(&flying, &request) {
                continue;
            }
            let Some(take) = snapshot.get(request.take) else {
                continue;
            };
            let future = fetch.fetch(&take.path, request.range.clone());
            let r = request.clone();
            in_flight.push(Box::pin(async move { (r, future.await) }));
            flying.push(request);
        }
        if config.resident.is_none()
            && flying.is_empty()
            && snapshot.iter().all(StreamedTake::complete)
        {
            return;
        }
        // The next landing, or a tick to re-plan for a moving playhead.
        let landed = if in_flight.is_empty() {
            architect::platform::sleep(config.tick).await;
            None
        } else {
            let tick = architect::platform::sleep(config.tick);
            futures::pin_mut!(tick);
            match futures::future::select(in_flight.next(), tick).await {
                futures::future::Either::Left((done, _)) => done,
                futures::future::Either::Right(((), _)) => None,
            }
        };
        if let Some((request, result)) = landed {
            flying.retain(|f| f != &request);
            match result {
                Ok(data) => {
                    if let Some(take) = snapshot.get(request.take)
                        && let Err(e) = take.bytes.insert(request.range.start, &data)
                    {
                        tracing::warn!(media = %take.path, error = %e, "streamed media could not be stored");
                    }
                }
                Err(e) => {
                    let path = snapshot.get(request.take).map_or("?", |t| t.path.as_str());
                    tracing::warn!(media = %path, range = ?request.range, error = %e, "a streamed media range failed; it is asked for again");
                }
            }
        }
    }
}

fn overlaps(flying: &[Request], r: &Request) -> bool {
    flying
        .iter()
        .any(|f| f.take == r.take && f.range.start < r.range.end && r.range.start < f.range.end)
}
