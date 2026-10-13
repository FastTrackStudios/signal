//! A vocal recorded on a dual-capsule mic — front capsule on the left,
//! rear on the right — played through the engine with FTS Mic first on
//! its track: what comes out is the modelled mic, the same on both sides.
//! Bypassed, the two capsules come through as they are.

use std::sync::Arc;

use daw_proto::fx::{AddFxAtRequest, Effects, FxChainContext, FxRef, FxTarget};
use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, ProjectInfo, TrackRef, Tracks};
use daw_standalone::audio_engine::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::audio_engine::render::{ProjectRenderer, StereoBuffer};
use daw_standalone::sync::Standalone;

const RATE: u32 = 48_000;

/// Two seconds of a capsule pair on `track`: a front and a rear that are
/// not the same signal.
fn capsule_pair(daw: &Standalone, project: &str, track: &str) -> Option<()> {
    let ctx = ProjectContext::Project(project.to_owned());
    let at = Midi::create_midi_item(daw, ctx.clone(), TrackRef::Guid(track.to_owned()), 0.0, 2.0)?;
    let ItemRef::Guid(item) = at.item else {
        return None;
    };
    let take = daw_proto::Takes::get_active_take(daw, ctx, ItemRef::Guid(item))?;
    daw.write_project(project, |p| {
        for t in p.takes.values_mut().flat_map(|l| l.takes.iter_mut()) {
            if t.guid == take.guid {
                t.is_midi = false;
                t.source_type = daw_proto::item::SourceType::Audio;
                t.source_file_path = None;
            }
        }
    });
    let frames = 2 * RATE;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|n| {
            let t = f64::from(n) / f64::from(RATE);
            let front = (t * 220.0 * std::f64::consts::TAU).sin() * 0.4;
            let rear = (t * 1_310.0 * std::f64::consts::TAU).sin() * 0.15;
            [front, rear].map(|x| num_traits::ToPrimitive::to_f32(&x).unwrap_or(0.0))
        })
        .collect();
    attach_audio_source(
        daw,
        project,
        &take.guid,
        DecodedAudio::new(samples, 2, RATE),
    );
    Some(())
}

/// The most the two sides of `block` differ, and how loud its left is.
fn sides(block: &StereoBuffer) -> (f32, f32) {
    let frames = || {
        block.samples.chunks_exact(2).filter_map(|f| match f {
            [l, r] => Some((*l, *r)),
            _ => None,
        })
    };
    let apart = frames().map(|(l, r)| (l - r).abs()).fold(0.0_f32, f32::max);
    let left = frames().map(|(l, _)| l.abs()).fold(0.0_f32, f32::max);
    (apart, left)
}

#[test]
fn a_capsule_pair_comes_out_one_mic() {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(builtin_fx::BuiltinFx));
    let project = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let vocal = Tracks::add(&daw, ctx.clone(), "Lead Vocal", None).expect("a vocal");
    capsule_pair(&daw, &project, &vocal).expect("the recording");
    let chain = FxChainContext::Track(vocal);
    // Its Tone chain first, bypassed as a prepared song has it, then the
    // mic put ahead of it.
    let eq = Effects::add(&daw, ctx.clone(), chain.clone(), "FTS EQ").expect("an EQ");
    Effects::set_enabled(
        &daw,
        ctx.clone(),
        FxTarget::new(chain.clone(), FxRef::Guid(eq)),
        false,
    )
    .expect("bypassed");
    let mic = Effects::add_at(
        &daw,
        ctx.clone(),
        AddFxAtRequest {
            context: chain.clone(),
            name: "FTS Mic".into(),
            index: 0,
        },
    )
    .expect("FTS Mic");
    let names: Vec<String> = Effects::list(&daw, ctx.clone(), chain.clone())
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(
        names.first().map(String::as_str),
        Some("FTS Mic"),
        "{names:?}"
    );

    let renderer = ProjectRenderer::new(&daw, &project, RATE);
    let block = RATE / 20;
    // The song plays on, block after block; the models load off the
    // audio thread, so the mic is silent until they are in.
    let mut at = 0_u64;
    let mut play = || {
        let out = renderer.render_block(at, usize::try_from(block).unwrap_or(0));
        at = (at + u64::from(block)) % u64::from(RATE);
        out
    };
    let mut heard = (f32::MAX, 0.0);
    for _ in 0..100 {
        heard = sides(&play());
        if heard.1 > 0.1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(heard.1 > 0.1, "the mic plays: {heard:?}");
    assert!(heard.0 < 1e-5, "one mic, the same both sides: {heard:?}");

    Effects::set_enabled(&daw, ctx, FxTarget::new(chain, FxRef::Guid(mic)), false)
        .expect("bypassed");
    let (apart, _) = sides(&play());
    assert!(
        apart > 0.3,
        "bypassed, the two capsules as recorded: {apart}"
    );
}
