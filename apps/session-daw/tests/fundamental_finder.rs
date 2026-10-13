//! The fundamental finder on a drum with a known note: a decaying 120 Hz
//! ring on every hit, a louder 400 Hz click on each attack, and a steady
//! 60 Hz kick bleeding through between them. The trigger says when the
//! drum is hit; the finder reads the ring.

#![cfg(feature = "native")]

use daw::standalone::Standalone;
use daw::standalone::audio_engine::DecodedAudio;
use daw::standalone::audio_engine::materialize::attach_audio_source;
use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, ProjectInfo, TrackRef, Tracks};

const RATE: u32 = 48_000;
const SECONDS: u32 = 8;
/// A hit every half second.
const EVERY: f64 = 0.5;

/// `samples` as a whole-song item on `track`.
fn item(daw: &Standalone, project: &str, track: &str, samples: Vec<f32>) {
    let ctx = ProjectContext::Project(project.to_owned());
    let at = Midi::create_midi_item(
        daw,
        ctx.clone(),
        TrackRef::Guid(track.to_owned()),
        0.0,
        f64::from(SECONDS),
    )
    .expect("an item");
    let ItemRef::Guid(guid) = at.item else {
        panic!("an item guid")
    };
    let take = daw_proto::Takes::get_active_take(daw, ctx, ItemRef::Guid(guid)).expect("a take");
    daw.write_project(project, |p| {
        for t in p.takes.values_mut().flat_map(|l| l.takes.iter_mut()) {
            if t.guid == take.guid {
                t.is_midi = false;
                t.source_type = daw_proto::item::SourceType::Audio;
                t.source_file_path = None;
            }
        }
    });
    attach_audio_source(
        daw,
        project,
        &take.guid,
        DecodedAudio::new(samples, 1, RATE),
    );
}

#[test]
fn the_ring_is_read_not_the_click_or_the_bleed() {
    let daw = Standalone::new();
    let project = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let trig = Tracks::add(&daw, ctx.clone(), "Trig", None).expect("trig");
    let mic = Tracks::add(&daw, ctx, "T1", None).expect("mic");
    let frames = usize::try_from(RATE * SECONDS).unwrap_or(0);
    let rate = f64::from(RATE);
    let mut trigger = vec![0.0_f32; frames];
    let mut close = vec![0.0_f32; frames];
    for (i, (t, m)) in trigger.iter_mut().zip(close.iter_mut()).enumerate() {
        let time = f64::from(u32::try_from(i).unwrap_or(0)) / rate;
        let since = time % EVERY;
        if since < 0.002 {
            *t = 0.9;
        }
        let ring = (std::f64::consts::TAU * 120.0 * since).sin() * (-since * 6.0).exp() * 0.5;
        let click = (std::f64::consts::TAU * 400.0 * since).sin() * (-since * 300.0).exp() * 0.9;
        let bleed = (std::f64::consts::TAU * 60.0 * time).sin() * 0.05;
        *m = num_traits::ToPrimitive::to_f32(&(ring + click + bleed)).unwrap_or(0.0);
    }
    item(&daw, &project, &trig, trigger);
    item(&daw, &project, &mic, close);

    let hz = session_daw::fundamental::find(
        &daw,
        &ProjectContext::Project(project),
        &trig,
        &[mic],
        session_daw::fundamental::TOM,
    )
    .expect("a fundamental");
    assert!((hz - 120.0).abs() < 2.0, "{hz}");
}
