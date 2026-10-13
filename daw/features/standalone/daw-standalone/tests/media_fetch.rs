//! What streams in first: every track's next seconds before any track's
//! later minutes, from wherever in a long file the playhead is — and a
//! seek re-plans.

#![cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use daw_standalone::audio_engine::media_fetch::{
    FetchConfig, RangeFetch, StreamedTake, drive, plan,
};
use fts_sample::ogg_index::OggIndex;
use fts_sample::sparse::SparseBytes;

const RATE: u32 = 44_100;

/// A proxy with some noise in it, so its pages grow with time as a real
/// stem's do.
fn proxy(seconds: usize, seed: u32) -> Arc<[u8]> {
    let frames = RATE as usize * seconds;
    let mut pcm = Vec::with_capacity(frames * 2);
    let mut x = seed.wrapping_mul(2_654_435_761);
    for i in 0..frames {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
        let t = i as f32 / RATE as f32;
        let s = (t * 220.0 * std::f32::consts::TAU).sin() * 0.3 + noise * 0.2;
        pcm.push(s);
        pcm.push(-s);
    }
    fts_sample::cache::encode_ogg_vorbis(&pcm, 2, RATE, 0.4)
        .unwrap()
        .into()
}

fn take(bytes: &Arc<[u8]>, path: &str, start: f64, end: f64, source_offset: f64) -> StreamedTake {
    StreamedTake {
        path: path.to_owned(),
        bytes: SparseBytes::in_memory(bytes.len() as u64),
        index: OggIndex::build(bytes, u64::from(RATE)).unwrap(),
        start,
        end,
        source_offset,
        playrate: 1.0,
    }
}

fn frames(seconds: f64) -> u64 {
    (seconds * f64::from(RATE)) as u64
}

#[test]
fn the_plan_is_headers_then_everyones_next_seconds() {
    let a = proxy(60, 1);
    let b = proxy(60, 2);
    let takes = vec![take(&a, "a", 0.0, 60.0, 0.0), take(&b, "b", 0.0, 60.0, 0.0)];
    let requests = plan(&takes, 10.0, 3.0, 64 * 1024);
    // Headers first, for both.
    assert!(requests[0].heard_in < 0.0 && requests[1].heard_in < 0.0);
    // Then the next three seconds of both, interleaved — nothing later.
    let audio: Vec<_> = requests.iter().filter(|r| r.heard_in >= 0.0).collect();
    assert!(audio.iter().any(|r| r.take == 0) && audio.iter().any(|r| r.take == 1));
    assert!(
        audio.windows(2).all(|w| w[0].heard_in <= w[1].heard_in),
        "soonest first"
    );
    for r in &audio {
        let t = &takes[r.take];
        let within = t.index.bytes_for(frames(9.0), frames(14.0));
        assert!(
            r.range.start >= within.start && r.range.end <= within.end,
            "{r:?} outside {within:?}"
        );
    }
}

#[test]
fn a_long_file_is_fetched_from_where_it_plays() {
    // An item that plays a 3-minute file from 100 s in, placed at 0 s.
    let long = proxy(180, 3);
    let takes = vec![take(&long, "long", 0.0, 60.0, 100.0)];
    let requests = plan(&takes, 20.0, 3.0, 1 << 30);
    let audio: Vec<_> = requests.iter().filter(|r| r.heard_in >= 0.0).collect();
    let near = takes[0].index.bytes_for(frames(119.0), frames(124.0));
    assert!(!audio.is_empty());
    for r in audio {
        assert!(
            r.range.start >= near.start && r.range.end <= near.end,
            "{r:?}: 120 s of the file, not its start"
        );
    }
}

/// A link of `bytes_per_second`, recording when each request lands.
struct SlowLink {
    files: Vec<(String, Arc<[u8]>)>,
    bytes_per_second: f64,
    landed: Arc<Mutex<Vec<(String, Range<u64>)>>>,
}

impl RangeFetch for SlowLink {
    fn fetch(
        &self,
        path: &str,
        range: Range<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send>> {
        let data = self
            .files
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| Arc::clone(b));
        let wait = Duration::from_secs_f64(
            (range.end - range.start) as f64 / self.bytes_per_second + 0.01,
        );
        let landed = Arc::clone(&self.landed);
        let path = path.to_owned();
        Box::pin(async move {
            tokio::time::sleep(wait).await;
            let data = data.ok_or("no such file")?;
            landed.lock().unwrap().push((path, range.clone()));
            Ok(data[range.start as usize..range.end as usize].to_vec())
        })
    }
}

#[tokio::test(start_paused = true)]
async fn on_a_slow_link_the_near_seconds_land_first_and_a_seek_replans() {
    let files: Vec<(String, Arc<[u8]>)> = (0..3)
        .map(|i| (format!("t{i}"), proxy(60, 10 + i)))
        .collect();
    let takes: Vec<StreamedTake> = files
        .iter()
        .map(|(p, b)| take(b, p, 0.0, 60.0, 0.0))
        .collect();
    let shared = Arc::new(Mutex::new(takes));
    let playhead = Arc::new(Mutex::new(10.0f64));
    let landed = Arc::new(Mutex::new(Vec::new()));
    let link = Arc::new(SlowLink {
        files: files.clone(),
        bytes_per_second: 1_000_000.0,
        landed: Arc::clone(&landed),
    });
    let stop = Arc::new(AtomicBool::new(false));
    let config = FetchConfig {
        max_request: 32 * 1024,
        ..FetchConfig::default()
    };
    let at = Arc::clone(&playhead);
    let driver = tokio::spawn(drive(
        Arc::clone(&shared),
        link,
        Arc::new(move || *at.lock().unwrap()),
        config,
        Arc::clone(&stop),
    ));

    // Every track's 10..13 s is in before any track's 40 s+ has been asked.
    let near_done = |t: &StreamedTake| t.bytes.has(&t.index.bytes_for(frames(10.0), frames(13.0)));
    let far = |t: &StreamedTake| t.index.bytes_for(frames(40.0), frames(60.0));
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let takes = shared.lock().unwrap().clone();
        if takes.iter().all(near_done) {
            for t in &takes {
                assert!(
                    t.bytes.missing(&far(t)).len() > 0,
                    "{}: the far end was not fetched first",
                    t.path
                );
            }
            break;
        }
    }

    // A seek to 45 s: its seconds come next, before the middle of the set.
    *playhead.lock().unwrap() = 45.0;
    let seek_done = |t: &StreamedTake| t.bytes.has(&t.index.bytes_for(frames(45.0), frames(48.0)));
    let middle = |t: &StreamedTake| t.index.bytes_for(frames(25.0), frames(35.0));
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let takes = shared.lock().unwrap().clone();
        if takes.iter().all(seek_done) {
            for t in &takes {
                assert!(
                    !t.bytes.has(&middle(t)),
                    "{}: the middle came before the seek",
                    t.path
                );
            }
            break;
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = driver.await;
    assert!(!landed.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn what_is_behind_the_playhead_comes_last_and_the_fetch_finishes() {
    // One take playing all of its file, one playing only 20..40 s of its
    // own (from 0 s in the timeline); the playhead parked at 30 s.
    let whole = proxy(60, 21);
    let part = proxy(60, 22);
    let files = vec![
        ("whole".to_owned(), Arc::clone(&whole)),
        ("part".to_owned(), Arc::clone(&part)),
    ];
    let takes = vec![
        take(&whole, "whole", 0.0, 60.0, 0.0),
        take(&part, "part", 0.0, 20.0, 20.0),
    ];
    let shared = Arc::new(Mutex::new(takes));
    let landed = Arc::new(Mutex::new(Vec::new()));
    let link = Arc::new(SlowLink {
        files,
        bytes_per_second: 1_000_000.0,
        landed: Arc::clone(&landed),
    });
    let config = FetchConfig {
        max_request: 32 * 1024,
        ..FetchConfig::default()
    };
    // Returns by itself: nothing stops it but being done.
    tokio::time::timeout(
        Duration::from_secs(600),
        drive(
            Arc::clone(&shared),
            link,
            Arc::new(|| 30.0),
            config,
            Arc::new(AtomicBool::new(false)),
        ),
    )
    .await
    .expect("the fetch finishes with the playhead parked mid-song");

    let takes = shared.lock().unwrap().clone();
    assert!(takes.iter().all(StreamedTake::complete));
    let whole_take = &takes[0];
    assert!(
        whole_take
            .bytes
            .has(&whole_take.index.bytes_for(0, frames(5.0))),
        "the start, behind the playhead, came too"
    );
    // The part take's file beyond what it plays was never asked for.
    let part_take = &takes[1];
    assert!(
        !part_take
            .bytes
            .missing(&part_take.index.bytes_for(frames(50.0), frames(60.0)))
            .is_empty()
    );
    // And what was behind came after what was in front — but for the
    // requests already in flight when the last of the front was asked.
    let order = landed.lock().unwrap().clone();
    let first_behind = order
        .iter()
        .position(|(p, r)| {
            p == "whole"
                && r.start >= whole_take.index.audio_start
                && r.start < whole_take.index.bytes_for(frames(20.0), frames(21.0)).start
        })
        .unwrap();
    let last_ahead = order
        .iter()
        .rposition(|(p, r)| {
            p == "whole" && r.start > whole_take.index.bytes_for(frames(31.0), frames(32.0)).end
        })
        .unwrap();
    let in_flight = FetchConfig::default().concurrency;
    assert!(
        first_behind + in_flight > last_ahead,
        "behind at {first_behind}, ahead until {last_ahead}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_resident_window_holds_only_what_is_near_the_playhead() {
    use daw_standalone::audio_engine::media_fetch::Resident;
    let files: Vec<(String, Arc<[u8]>)> = (0..2)
        .map(|i| (format!("w{i}"), proxy(120, 30 + i)))
        .collect();
    let takes: Vec<StreamedTake> = files
        .iter()
        .map(|(p, b)| take(b, p, 0.0, 120.0, 0.0))
        .collect();
    let shared = Arc::new(Mutex::new(takes));
    let playhead = Arc::new(Mutex::new(10.0f64));
    let link = Arc::new(SlowLink {
        files: files.clone(),
        bytes_per_second: 4_000_000.0,
        landed: Arc::new(Mutex::new(Vec::new())),
    });
    let stop = Arc::new(AtomicBool::new(false));
    let window = Resident {
        behind: 5.0,
        ahead: 20.0,
    };
    let config = FetchConfig {
        max_request: 32 * 1024,
        resident: Some(window),
        ..FetchConfig::default()
    };
    let at = Arc::clone(&playhead);
    let driver = tokio::spawn(drive(
        Arc::clone(&shared),
        link,
        Arc::new(move || *at.lock().unwrap()),
        config,
        Arc::clone(&stop),
    ));

    let settle = || async {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    settle().await;
    let held =
        |t: &StreamedTake, a: f64, b: f64| t.bytes.has(&t.index.bytes_for(frames(a), frames(b)));
    for t in shared.lock().unwrap().iter() {
        assert!(held(t, 10.0, 29.0), "{}: the window is here", t.path);
        assert!(
            !held(t, 60.0, 61.0),
            "{}: nothing far ahead is fetched",
            t.path
        );
        assert!(
            t.bytes.resident() < t.bytes.len() / 3,
            "{}: {} of {} held",
            t.path,
            t.bytes.resident(),
            t.bytes.len()
        );
    }

    // The playhead moves on: what it left is let go, what it nears arrives.
    *playhead.lock().unwrap() = 80.0;
    settle().await;
    for t in shared.lock().unwrap().iter() {
        assert!(held(t, 80.0, 99.0), "{}: the new window is here", t.path);
        assert!(!held(t, 10.0, 20.0), "{}: the old one is let go", t.path);
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = driver.await;
}

#[test]
fn a_take_has_a_moment_once_its_header_and_the_pages_under_it_arrive() {
    let bytes = proxy(60, 4);
    let t = take(&bytes, "t", 10.0, 70.0, 0.0);
    // Where it plays nothing, there is nothing to wait for.
    assert!(t.has_at(0.0, 2.0) && t.has_at(80.0, 2.0));
    assert!(!t.has_at(30.0, 2.0), "nothing has arrived");
    let under = t.index.bytes_for(frames(20.0) - 4096, frames(22.0));
    let put = |r: Range<u64>| {
        t.bytes
            .insert(r.start, &bytes[r.start as usize..r.end as usize])
            .unwrap();
    };
    put(under);
    assert!(!t.has_at(30.0, 2.0), "the pages, but not the header");
    put(t.index.header());
    assert!(t.has_at(30.0, 2.0));
    assert!(
        !t.has_at(40.0, 2.0),
        "a moment whose pages have not arrived"
    );
}
