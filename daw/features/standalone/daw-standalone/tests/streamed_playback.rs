//! A take streamed from an Ogg Vorbis proxy plays through the real
//! renderer: the item's audio is the proxy's, read from the resident
//! window the feeder keeps filled around where the renderer reads — and a
//! jump across the song is silent for one block, then right.

#![cfg(all(feature = "stream-ogg", feature = "audio"))]

use std::sync::Arc;

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, ProjectInfo, TrackRef, Tracks};
use daw_standalone::audio_engine::materialize::attach_source;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::audio_engine::source::AudioSource;
use daw_standalone::audio_engine::streamed::{StreamFeeder, Streamed};
use daw_standalone::sync::Standalone;
use fts_sample::ogg_stream::OggStream;

const RATE: u32 = 44_100;
const SECONDS: usize = 30;
const BLOCK: usize = 512;

/// Thirty seconds of a stereo tone whose pitch climbs a semitone a second
/// — so audio from the wrong place in the song does not match.
fn proxy() -> Arc<[u8]> {
    let mut pcm = Vec::with_capacity(RATE as usize * SECONDS * 2);
    let mut phase = 0.0f64;
    for i in 0..RATE as usize * SECONDS {
        let hz = 220.0 * 2f64.powf((i / RATE as usize) as f64 / 12.0);
        phase += hz / f64::from(RATE);
        let v = (phase * std::f64::consts::TAU).sin() as f32 * 0.5;
        pcm.push(v);
        pcm.push(v);
    }
    fts_sample::cache::encode_ogg_vorbis(&pcm, 2, RATE, 0.4)
        .expect("encode")
        .into()
}

/// A project with one track and one audio item over the whole song, its
/// take streamed from `bytes`. Returns the feeder that keeps it filled.
fn project(bytes: Arc<[u8]>) -> (Standalone, String, StreamFeeder<OggStream>) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "streamed".into(),
        name: "streamed".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let track = Tracks::add(&daw, ctx.clone(), "Stem", None).expect("track");
    let loc = Midi::create_midi_item(
        &daw,
        ctx.clone(),
        TrackRef::Guid(track),
        0.0,
        SECONDS as f64,
    )
    .expect("item");
    let ItemRef::Guid(item) = &loc.item else {
        panic!()
    };
    let take =
        daw_proto::Takes::get_active_take(&daw, ctx, ItemRef::Guid(item.clone())).expect("take");
    daw.write_project(&guid, |p| {
        for list in p.takes.values_mut() {
            for t in &mut list.takes {
                if t.guid == take.guid {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                    t.source_file_path = Some("Media/Stem.wav".into());
                }
            }
        }
    });
    let stream = OggStream::open(bytes).expect("open");
    let streamed = Streamed::new(stream.channels(), stream.sample_rate(), stream.frames());
    attach_source(
        &daw,
        &guid,
        &take.guid,
        AudioSource::Streamed(streamed.clone()),
    );
    (daw, guid, StreamFeeder::new(streamed, stream))
}

/// The proxy decoded straight through: what every block should hear.
fn reference(bytes: Arc<[u8]>) -> Vec<f32> {
    let mut stream = OggStream::open(bytes).expect("open");
    let mut out = Vec::new();
    while stream.decode(&mut out).expect("decode").is_some() {}
    out
}

/// Render `blocks` blocks from `start`, pumping the feeder before each
/// the way a playback loop does. Returns the left channel.
fn play(
    renderer: &ProjectRenderer,
    feeder: &mut StreamFeeder<OggStream>,
    start: u64,
    blocks: usize,
) -> Vec<f32> {
    let mut left = Vec::with_capacity(blocks * BLOCK);
    for b in 0..blocks {
        while feeder.pump(8192) {}
        let out = renderer.render_block(start + (b * BLOCK) as u64, BLOCK);
        left.extend(out.samples.iter().step_by(2));
    }
    left
}

/// How far `got` is from `want` once scaled by their ratio (the pan law's
/// gain), as a fraction of `want`'s level.
fn mismatch(got: &[f32], want: &[f32]) -> f32 {
    let dot: f32 = got.iter().zip(want).map(|(a, b)| a * b).sum();
    let norm: f32 = want.iter().map(|b| b * b).sum();
    let gain = dot / norm.max(f32::EPSILON);
    let err: f32 = got
        .iter()
        .zip(want)
        .map(|(a, b)| (a - b * gain).powi(2))
        .sum();
    assert!(gain > 0.3, "the take is audible: gain {gain}");
    (err / norm.max(f32::EPSILON)).sqrt()
}

#[test]
fn a_streamed_take_plays_the_proxy_through_the_renderer() {
    let bytes = proxy();
    let want = reference(Arc::clone(&bytes));
    let (daw, guid, mut feeder) = project(bytes);
    let renderer = ProjectRenderer::new(&daw, &guid, RATE);

    // Two seconds from the top.
    let got = play(&renderer, &mut feeder, 0, 2 * RATE as usize / BLOCK);
    let n = got.len();
    let straight: Vec<f32> = want.iter().step_by(2).take(n).copied().collect();
    let off = mismatch(&got, &straight);
    assert!(off < 0.01, "the first two seconds are the proxy's: {off}");

    // A jump to 25 s, past the lookahead: the first block reads what is not decoded yet as
    // silence, then the feeder catches up and it is the proxy's 25 s.
    let at = 25 * RATE as u64;
    let first = renderer.render_block(at, BLOCK);
    assert!(
        first.samples.iter().all(|s| *s == 0.0),
        "not resident yet: silence, not the wrong audio"
    );
    let got = play(
        &renderer,
        &mut feeder,
        at + BLOCK as u64,
        RATE as usize / BLOCK,
    );
    let from = (at as usize + BLOCK) * 2;
    let straight: Vec<f32> = want[from..]
        .iter()
        .step_by(2)
        .take(got.len())
        .copied()
        .collect();
    let off = mismatch(&got, &straight);
    assert!(off < 0.01, "after the jump it is the proxy's 25 s: {off}");
}
