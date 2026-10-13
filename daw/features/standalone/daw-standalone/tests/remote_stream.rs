//! A proxy streamed from elsewhere, as the engine plays it: silent where
//! its bytes have not arrived, the straight decode where they have — and a
//! jump into a stretch not yet fetched goes silent again until it lands.

#![cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::sync::Arc;

use daw_standalone::audio_engine::streamed::{CHUNK, RemoteOgg, StreamFeeder, Streamed};
use fts_sample::ogg_index::OggIndex;
use fts_sample::ogg_stream::OggStream;
use fts_sample::sparse::SparseBytes;

const RATE: u32 = 44_100;

/// A stereo proxy of steady tones, `seconds` long.
fn proxy(seconds: usize) -> Arc<[u8]> {
    let frames = RATE as usize * seconds;
    let mut pcm = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        pcm.push((t * 220.0 * std::f32::consts::TAU).sin() * 0.5);
        pcm.push((t * 330.0 * std::f32::consts::TAU).sin() * 0.5);
    }
    fts_sample::cache::encode_ogg_vorbis(&pcm, 2, RATE, 0.4)
        .unwrap()
        .into()
}

fn arrive(sparse: &SparseBytes, bytes: &[u8], range: std::ops::Range<u64>) {
    sparse
        .insert(
            range.start,
            &bytes[range.start as usize..range.end as usize],
        )
        .unwrap();
}

fn pump(feeder: &mut StreamFeeder<RemoteOgg>) {
    for _ in 0..200 {
        if !feeder.pump(50_000) {
            break;
        }
    }
}

#[test]
fn a_streamed_proxy_plays_what_has_arrived_and_waits_for_the_rest() {
    let bytes = proxy(30);
    let index = OggIndex::build(&bytes, u64::from(RATE)).unwrap();
    let (_, all) = {
        let mut whole = OggStream::open(Arc::clone(&bytes)).unwrap();
        let mut out = Vec::new();
        while whole.decode(&mut out).unwrap().is_some() {}
        (0, out)
    };

    // Only the header and the index are here: set up, silent, not failed.
    let sparse = SparseBytes::in_memory(bytes.len() as u64);
    arrive(&sparse, &bytes, index.header());
    let source = Streamed::new(index.channels, index.sample_rate, index.frames);
    let mut feeder = StreamFeeder::new(
        source.clone(),
        RemoteOgg::new(Arc::clone(&sparse), index.clone()),
    );
    let playhead = u64::from(RATE) * 10;
    source.want(playhead);
    pump(&mut feeder);
    let chunk = (playhead as usize) / CHUNK;
    assert!(
        !source.resident(chunk),
        "nothing to play before the bytes arrive"
    );
    assert!(
        sparse.wanted().is_some(),
        "the fetcher is told what is wanted"
    );

    // The next few seconds' bytes arrive: they play, as decoded straight.
    arrive(
        &sparse,
        &bytes,
        index.bytes_for(
            playhead.saturating_sub(8192),
            playhead + u64::from(RATE) * 4,
        ),
    );
    pump(&mut feeder);
    assert!(source.resident(chunk), "what arrived plays");
    let frame = playhead as usize + 1_000;
    let got = source.sample(frame, 0);
    let want = all[frame * 2];
    assert!((got - want).abs() < 0.02, "frame {frame}: {got} vs {want}");

    // A jump to a stretch not yet here: silent again, until it lands.
    let jump = u64::from(RATE) * 25;
    source.want(jump);
    pump(&mut feeder);
    let far = (jump as usize) / CHUNK;
    assert!(!source.resident(far), "not yet");
    arrive(
        &sparse,
        &bytes,
        index.bytes_for(jump.saturating_sub(8192), jump + u64::from(RATE) * 2),
    );
    pump(&mut feeder);
    assert!(source.resident(far), "arrived, and plays");
    let frame = jump as usize + 500;
    assert!((source.sample(frame, 1) - all[frame * 2 + 1]).abs() < 0.02);
}
