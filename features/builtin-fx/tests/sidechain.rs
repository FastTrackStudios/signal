//! FTS Gate keyed from another track: a drum's trigger sent onto the
//! close mic's channels 3/4 opens the gate on the mic — through the
//! engine, as a mix wires it.

use std::sync::Arc;

use daw_proto::fx::{Effects, FxChainContext, FxTarget, SetParameterRequest};
use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, ProjectInfo, RouteRef, RouteType, Routing, TrackRef, Tracks};
use daw_standalone::audio_engine::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::sync::Standalone;

const RATE: u32 = 48_000;

/// A second of `level` on `track`; `None` if the item would not take.
fn steady(daw: &Standalone, project: &str, track: &str, level: f32) -> Option<()> {
    let ctx = ProjectContext::Project(project.to_owned());
    let at = Midi::create_midi_item(daw, ctx.clone(), TrackRef::Guid(track.to_owned()), 0.0, 1.0)?;
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
    let samples = vec![level; usize::try_from(RATE).unwrap_or(0)];
    attach_audio_source(
        daw,
        project,
        &take.guid,
        DecodedAudio::new(samples, 1, RATE),
    );
    Some(())
}

fn rms(block: &daw_standalone::audio_engine::render::StereoBuffer) -> f32 {
    let n = block.samples.len().max(1);
    let sum: f32 = block.samples.iter().map(|s| s * s).sum();
    (sum / f32::from(u16::try_from(n).unwrap_or(u16::MAX))).sqrt()
}

/// A quiet kick under a -10 dB threshold: shut keyed on itself, open
/// keyed on its loud trigger, shut again when the trigger's send is
/// muted.
#[test]
fn the_trigger_opens_the_gate_on_the_mic() {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(builtin_fx::BuiltinFx));
    let project = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let trig = Tracks::add(&daw, ctx.clone(), "Trig", None).expect("trig");
    let kick = Tracks::add(&daw, ctx.clone(), "Kick", None).expect("kick");
    steady(&daw, &project, &trig, 0.9).expect("the trigger");
    steady(&daw, &project, &kick, 0.05).expect("the mic");
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(trig.clone()), false)
        .expect("trig unheard");
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(trig.clone()),
        TrackRef::Guid(kick.clone()),
    )
    .expect("a send");
    let send = daw_proto::RouteLocation {
        track: TrackRef::Guid(trig),
        route_type: RouteType::Send,
        route: RouteRef::Index(0),
    };
    Routing::set_dest_channels(&daw, ctx.clone(), send.clone(), 2, 2).expect("onto 3/4");

    let chain = FxChainContext::Track(kick);
    let gate = Effects::add(&daw, ctx.clone(), chain.clone(), "FTS Gate").expect("a gate");
    let set = |param: &str, text: &str| {
        let (index, value) =
            builtin_fx::setting("FTS Gate", param, text).unwrap_or_else(|| panic!("{param}"));
        Effects::set_parameter(
            &daw,
            ctx.clone(),
            SetParameterRequest {
                target: FxTarget::new(chain.clone(), daw_proto::fx::FxRef::Guid(gate.clone())),
                index,
                value,
            },
        )
        .expect("set");
    };
    set("Threshold", "-10.0 dB");
    let renderer = ProjectRenderer::new(&daw, &project, RATE);
    let quarter = usize::try_from(RATE / 4).unwrap_or(0);

    let on_itself = rms(&renderer.render_block(0, quarter));
    set("Key", "Sidechain");
    let on_trigger = rms(&renderer.render_block(0, quarter));
    Routing::set_muted(&daw, ctx, send, true).expect("send muted");
    // The first block after is the gate's hold and release, from open.
    let _ = renderer.render_block(0, quarter);
    let no_trigger = rms(&renderer.render_block(0, quarter));

    assert!(
        on_itself < 0.005,
        "keyed on itself, a quiet kick stays shut: {on_itself}"
    );
    assert!(
        on_trigger > 0.04,
        "keyed on its trigger, it opens: {on_trigger}"
    );
    assert!(no_trigger < 0.005, "no trigger, shut: {no_trigger}");
}
