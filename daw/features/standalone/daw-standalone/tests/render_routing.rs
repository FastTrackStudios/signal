//! Render a small project through `ProjectRenderer`, exercising:
//! - Item playback (with fade-in/out)
//! - Track volume / pan / mute / solo
//! - Track-to-track sends
//! - parent_send_enabled
//!
//! No cpal involved — pure number-crunching, so this test works the
//! same on native and WASM.

#![cfg(feature = "decode")]

use daw_proto::midi::Midi;
use daw_proto::primitives::Duration as ProtoDuration;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, Items, ProjectInfo, RouteRef, RouteType, Routing, TrackRef, Tracks};
use daw_standalone::audio_engine::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::sync::Standalone;

const SAMPLE_RATE: u32 = 48_000;

fn seeded() -> (Standalone, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    (daw, guid)
}

/// Synth a 1s constant-amplitude mono buffer at value `v`.
fn const_audio(v: f32) -> DecodedAudio {
    let frames = SAMPLE_RATE as usize;
    DecodedAudio::new(vec![v; frames], 1, SAMPLE_RATE)
}

fn create_item_with_audio(
    daw: &Standalone,
    project_guid: &str,
    track_guid: &str,
    start_seconds: f64,
    length_seconds: f64,
    audio: DecodedAudio,
) -> (String, String) {
    let ctx = ProjectContext::Project(project_guid.to_string());
    // Make a MIDI item then convert to audio so we can wire a take
    // GUID we control.
    let loc = Midi::create_midi_item(
        daw,
        ctx.clone(),
        TrackRef::Guid(track_guid.to_string()),
        start_seconds,
        start_seconds + length_seconds,
    )
    .expect("midi item created");
    let item_guid = match &loc.item {
        ItemRef::Guid(g) => g.clone(),
        _ => panic!(),
    };
    // Flip the active take to audio + attach our synthetic source.
    let active =
        daw_proto::Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item_guid.clone()))
            .unwrap();
    daw.write_project(project_guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                    t.source_file_path = None; // already decoded
                }
            }
        }
    });
    attach_audio_source(daw, project_guid, &active.guid, audio);
    (item_guid, active.guid)
}

fn rms_l(buf: &daw_standalone::audio_engine::render::StereoBuffer) -> f32 {
    let mut s = 0.0;
    for i in 0..buf.frames {
        let x = buf.samples[i * 2] as f64;
        s += x * x;
    }
    ((s / buf.frames.max(1) as f64).sqrt()) as f32
}

fn rms_r(buf: &daw_standalone::audio_engine::render::StereoBuffer) -> f32 {
    let mut s = 0.0;
    for i in 0..buf.frames {
        let x = buf.samples[i * 2 + 1] as f64;
        s += x * x;
    }
    ((s / buf.frames.max(1) as f64).sqrt()) as f32
}

/// A centred track's gain on each side: REAPER's default stereo balance
/// with a 0 dB pan law — unity (`render::balance`).
const CENTRE: f32 = 1.0;

#[test]
fn renders_single_item_to_master() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx, "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(0.5));

    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    // 0.5 s of audio.
    let block = r.render_block(0, (SAMPLE_RATE / 2) as usize);
    // Constant 0.5 → after center pan + unity volume the L/R each
    // see 0.5 (centred, a 0 dB pan law: unity).
    let target = 0.5 * CENTRE;
    assert!(
        (rms_l(&block) - target).abs() < 0.05,
        "L rms={}, target={target}",
        rms_l(&block)
    );
    assert!(
        (rms_r(&block) - target).abs() < 0.05,
        "R rms={}, target={target}",
        rms_r(&block)
    );
}

#[test]
fn track_pan_routes_to_correct_side() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(0.5));
    Tracks::set_pan(&daw, ctx, TrackRef::Guid(t), 1.0).unwrap(); // hard right

    let block =
        ProjectRenderer::new(&daw, &guid, SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 2);
    assert!(rms_l(&block) < 0.01, "L should be silent on hard-right pan");
    assert!(
        rms_r(&block) > 0.4,
        "R should be loud, got {}",
        rms_r(&block)
    );
}

#[test]
fn muted_track_contributes_silence() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(0.5));
    Tracks::set_muted(&daw, ctx, TrackRef::Guid(t), true).unwrap();

    let block =
        ProjectRenderer::new(&daw, &guid, SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&block) < 0.001);
    assert!(rms_r(&block) < 0.001);
}

#[test]
fn solo_isolates_track() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let a = Tracks::add(&daw, ctx.clone(), "A", None).unwrap();
    let b = Tracks::add(&daw, ctx.clone(), "B", None).unwrap();
    create_item_with_audio(&daw, &guid, &a, 0.0, 1.0, const_audio(0.5));
    create_item_with_audio(&daw, &guid, &b, 0.0, 1.0, const_audio(0.5));

    Tracks::set_soloed(&daw, ctx, TrackRef::Guid(a), true).unwrap();
    let block =
        ProjectRenderer::new(&daw, &guid, SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    // Only A contributes; B's content is suppressed.
    let l = rms_l(&block);
    let target = 0.5 * CENTRE;
    assert!((l - target).abs() < 0.05, "solo L rms={l}, target={target}");
}

#[test]
fn fade_in_attenuates_block_start() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let (item, _take) = create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(1.0));
    // 200ms linear fade-in.
    Items::set_fade_in(
        &daw,
        ctx,
        ItemRef::Guid(item),
        ProtoDuration::from_seconds(0.2),
        daw_proto::item::FadeShape::Linear,
    )
    .unwrap();

    // Render 100ms at the very start — fade environment is 0 → 0.5.
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let early = r.render_block(0, SAMPLE_RATE as usize / 10); // 100ms
    // Render 100ms past the fade-in — full level.
    let later = r.render_block((SAMPLE_RATE as u64) * 3 / 10, SAMPLE_RATE as usize / 10); // start at 300ms

    let target = CENTRE; // gain 1.0 then center pan
    assert!(
        rms_l(&early) < rms_l(&later),
        "fade-in early ({}) should be quieter than past-fade ({})",
        rms_l(&early),
        rms_l(&later)
    );
    assert!(
        rms_l(&later) > target * 0.8,
        "past-fade L should approach unity"
    );
}

#[test]
fn send_routes_audio_into_destination_bus() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let src = Tracks::add(&daw, ctx.clone(), "Src", None).unwrap();
    let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    create_item_with_audio(&daw, &guid, &src, 0.0, 1.0, const_audio(1.0));

    // Disable Src's parent send so it ONLY reaches master via Bus.
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
        .unwrap();

    // Without the send: master is silent (Src has no parent send, Bus
    // has no items).
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let pre = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&pre) < 0.001 && rms_r(&pre) < 0.001,
        "expected silent master without send"
    );

    // Add send Src → Bus.
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(bus.clone()),
    )
    .unwrap();

    let post = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&post) > 0.1,
        "send should bring audio to master via Bus"
    );

    // Mute the send → master goes silent again.
    Routing::set_muted(
        &daw,
        ctx,
        daw_proto::RouteLocation {
            track: TrackRef::Guid(src),
            route_type: RouteType::Send,
            route: RouteRef::Index(0),
        },
        true,
    )
    .unwrap();
    let muted = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&muted) < 0.001, "muted send should be silent");
}

/// A send onto the destination's channels 3/4 is its sidechain: it keys
/// the destination's plugins and is not heard. Moved back onto 1/2, the
/// same send is heard — the negative control.
#[test]
fn send_onto_channels_3_4_is_not_heard() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let src = Tracks::add(&daw, ctx.clone(), "Trig", None).unwrap();
    let gated = Tracks::add(&daw, ctx.clone(), "Kick", None).unwrap();
    create_item_with_audio(&daw, &guid, &src, 0.0, 1.0, const_audio(1.0));
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
        .unwrap();
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(gated),
    )
    .unwrap();
    let send = daw_proto::RouteLocation {
        track: TrackRef::Guid(src),
        route_type: RouteType::Send,
        route: RouteRef::Index(0),
    };
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);

    Routing::set_dest_channels(&daw, ctx.clone(), send.clone(), 2, 2).unwrap();
    let keyed = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&keyed) < 0.001 && rms_r(&keyed) < 0.001,
        "a sidechain is not heard"
    );

    Routing::set_dest_channels(&daw, ctx, send, 0, 2).unwrap();
    let heard = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&heard) > 0.1, "the same send onto 1/2 is heard");
}

/// A soloed one-note track (the kick's Sub) hears what feeds it and
/// only that: the mic sends into it, and is not heard dry beside it —
/// with the Sub's fader down, nothing is heard at all.
#[test]
fn a_soloed_return_hears_its_feed_and_not_the_feed_dry() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let mic = Tracks::add(&daw, ctx.clone(), "In", None).unwrap();
    let sub = Tracks::add(&daw, ctx.clone(), "Sub", None).unwrap();
    create_item_with_audio(&daw, &guid, &mic, 0.0, 1.0, const_audio(1.0));
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(mic),
        TrackRef::Guid(sub.clone()),
    )
    .unwrap();
    Tracks::set_soloed(&daw, ctx.clone(), TrackRef::Guid(sub.clone()), true).unwrap();
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let soloed = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&soloed) > 0.1, "the mic, through the Sub");
    Tracks::set_volume(&daw, ctx, TrackRef::Guid(sub), 0.0).unwrap();
    let down = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&down) < 0.001,
        "the mic not heard dry: {}",
        rms_l(&down)
    );
}

/// A soloed trigger — parent send off, only a key — is listened to:
/// straight to master while soloed, and silent again after.
#[test]
fn a_soloed_track_with_no_route_is_listened_to() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let trig = Tracks::add(&daw, ctx.clone(), "Trig", None).unwrap();
    create_item_with_audio(&daw, &guid, &trig, 0.0, 1.0, const_audio(1.0));
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(trig.clone()), false)
        .unwrap();
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    Tracks::set_soloed(&daw, ctx.clone(), TrackRef::Guid(trig.clone()), true).unwrap();
    let soloed = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&soloed) > 0.1, "the trigger, heard while soloed");
    Tracks::set_soloed(&daw, ctx, TrackRef::Guid(trig), false).unwrap();
    let after = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&after) < 0.001, "and unheard after");
}

#[test]
fn parent_send_disabled_excludes_track_from_master() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(1.0));
    Routing::set_parent_send_enabled(&daw, ctx, TrackRef::Guid(t), false).unwrap();

    let block =
        ProjectRenderer::new(&daw, &guid, SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&block) < 0.001);
}

#[test]
fn volume_envelope_attenuates_master() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Volume envelope ramping 1.0 → 0.0 over the second.
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(t.clone()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(1.0), 0.0),
    );

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    // First 100ms — envelope ≈ 1.0, full volume.
    let early = r.render_block(0, SAMPLE_RATE as usize / 10);
    // Last 100ms — envelope ≈ 0.0, near silence.
    let late = r.render_block(SAMPLE_RATE as u64 * 9 / 10, SAMPLE_RATE as usize / 10);

    assert!(
        rms_l(&early) > rms_l(&late) + 0.05,
        "early ({}) should be louder than late ({})",
        rms_l(&early),
        rms_l(&late)
    );
}

#[test]
fn pan_envelope_routes_to_right_side() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Pan envelope held at 1.0 (hard right).
    let loc = EnvelopeLocation::new(TrackRef::Guid(t), EnvelopeRef::Type(EnvelopeType::Pan));
    Automation::add_point(
        &daw,
        ctx,
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    let block = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&block) < 0.05,
        "hard-right pan env should silence L, got {}",
        rms_l(&block)
    );
    assert!(
        rms_r(&block) > 0.5,
        "R should be loud, got {}",
        rms_r(&block)
    );
}

#[test]
fn mute_envelope_gates_output() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Mute envelope held above 0.5 = muted.
    let loc = EnvelopeLocation::new(TrackRef::Guid(t), EnvelopeRef::Type(EnvelopeType::Mute));
    Automation::add_point(
        &daw,
        ctx,
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );

    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&block) < 0.001);
    assert!(rms_r(&block) < 0.001);
}

#[test]
fn send_volume_envelope_rides_send_level() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, SendEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let src = Tracks::add(&daw, ctx.clone(), "Src", None).unwrap();
    let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    create_item_with_audio(&daw, "p", &src, 0.0, 1.0, const_audio(1.0));
    // Route ONLY through the bus.
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
        .unwrap();
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(bus),
    )
    .unwrap();

    // Send vol envelope: full at t=0, fades to silence at t=1.
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(src),
        EnvelopeRef::Send {
            send_index: 0,
            kind: SendEnvelopeKind::Volume,
        },
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    Automation::add_point(
        &daw,
        ctx,
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(1.0), 0.0),
    );

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    let early = r.render_block(0, SAMPLE_RATE as usize / 10);
    let late = r.render_block(SAMPLE_RATE as u64 * 9 / 10, SAMPLE_RATE as usize / 10);
    assert!(
        rms_l(&early) > rms_l(&late) + 0.05,
        "send vol env: early={}, late={}",
        rms_l(&early),
        rms_l(&late)
    );
}

#[test]
fn send_mute_envelope_silences_send() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, SendEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let src = Tracks::add(&daw, ctx.clone(), "Src", None).unwrap();
    let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    create_item_with_audio(&daw, "p", &src, 0.0, 1.0, const_audio(1.0));
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
        .unwrap();
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(bus),
    )
    .unwrap();

    Automation::add_point(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(src),
            EnvelopeRef::Send {
                send_index: 0,
                kind: SendEnvelopeKind::Mute,
            },
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&block) < 0.001, "send-muted master should be silent");
}

#[test]
fn send_pan_envelope_steers_send() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, SendEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let src = Tracks::add(&daw, ctx.clone(), "Src", None).unwrap();
    let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    create_item_with_audio(&daw, "p", &src, 0.0, 1.0, const_audio(1.0));
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
        .unwrap();
    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(bus),
    )
    .unwrap();

    Automation::add_point(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(src),
            EnvelopeRef::Send {
                send_index: 0,
                kind: SendEnvelopeKind::Pan,
            },
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0), // hard right
    );
    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&block) < 0.05,
        "hard-right send pan: L should be quiet"
    );
    assert!(rms_r(&block) > 0.3, "hard-right send pan: R audible");
}

#[test]
fn volume_prefx_envelope_stacks_with_main_volume_envelope() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Main env at 0.5 + PreFX env at 0.5 → combined gain 0.25.
    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 0.5),
    );
    Automation::add_point(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(t),
            EnvelopeRef::Type(EnvelopeType::VolumePrefx),
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 0.5),
    );
    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    // After center pan: gain 0.25, centred at unity.
    let target = 0.25 * CENTRE;
    assert!(
        (rms_l(&block) - target).abs() < 0.05,
        "stacked vol envs: L rms={}, target={target}",
        rms_l(&block)
    );
}

#[test]
fn take_volume_envelope_attenuates_item() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let (item_guid, take_guid) = create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Take vol envelope: 1.0 at item start, 0.0 at end.
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(t),
        EnvelopeRef::Take {
            item_guid,
            take_guid,
            kind: TakeEnvelopeKind::Volume,
        },
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    Automation::add_point(
        &daw,
        ctx,
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(1.0), 0.0),
    );

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    let early = r.render_block(0, SAMPLE_RATE as usize / 10);
    let late = r.render_block(SAMPLE_RATE as u64 * 9 / 10, SAMPLE_RATE as usize / 10);
    assert!(
        rms_l(&early) > rms_l(&late) + 0.05,
        "take vol env: early={}, late={}",
        rms_l(&early),
        rms_l(&late)
    );
}

#[test]
fn take_mute_envelope_silences_item() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let (item_guid, take_guid) = create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    Automation::add_point(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(t),
            EnvelopeRef::Take {
                item_guid,
                take_guid,
                kind: TakeEnvelopeKind::Mute,
            },
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&block) < 0.001);
    assert!(rms_r(&block) < 0.001);
}

#[test]
fn take_pan_envelope_pans_item_within_track() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let (item_guid, take_guid) = create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Hard right take pan.
    Automation::add_point(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(t),
            EnvelopeRef::Take {
                item_guid,
                take_guid,
                kind: TakeEnvelopeKind::Pan,
            },
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&block) < 0.05,
        "L should be near-silent, got {}",
        rms_l(&block)
    );
    assert!(
        rms_r(&block) > 0.3,
        "R should be audible, got {}",
        rms_r(&block)
    );
}

#[test]
fn take_pitch_envelope_shifts_play_rate() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    // Item is 1s long; source is also 1s of constant audio. With
    // +12 semitones, source advances 2x → after 0.5s of wall time
    // we've consumed all 1s of the source.
    let (item_guid, take_guid) = create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Take {
                item_guid: item_guid.clone(),
                take_guid: take_guid.clone(),
                kind: TakeEnvelopeKind::Pitch,
            },
        ),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 12.0),
    );
    // Sanity: the envelope shows up via the proto getter.
    let points = Automation::points(
        &daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(t),
            EnvelopeRef::Take {
                item_guid,
                take_guid,
                kind: TakeEnvelopeKind::Pitch,
            },
        ),
    );
    assert_eq!(
        points.len(),
        1,
        "pitch env should have 1 point, got {points:?}"
    );
    assert!((points[0].value - 12.0).abs() < 1e-6);

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    // First half-second: source is in range, audible.
    let first = r.render_block(0, SAMPLE_RATE as usize / 8);
    assert!(
        rms_l(&first) > 0.1,
        "early block with +12st pitch should still be audible"
    );
    // After 0.6s (past the source-exhausted point at 0.5s) source
    // index is past the end and frames are skipped — much quieter.
    let late = r.render_block(SAMPLE_RATE as u64 * 6 / 10, SAMPLE_RATE as usize / 8);
    assert!(
        rms_l(&late) < rms_l(&first) * 0.5 + 0.01,
        "post-source-exhaust block ({}) should be much quieter than early ({})",
        rms_l(&late),
        rms_l(&first)
    );
}

#[test]
fn automation_mode_off_bypasses_envelope() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::{AutomationMode, PositionInSeconds};

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    let loc = EnvelopeLocation::new(
        TrackRef::Guid(t.clone()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );
    // Envelope ramps to silence — but with mode=Off it should be
    // ignored, and full volume should reach the master.
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 0.0),
    );
    Automation::set_automation_mode(&daw, ctx.clone(), loc, AutomationMode::Off);

    let block =
        ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 10);
    // Without Off this would be silent; with Off the static volume
    // (1.0) passes through.
    let target = CENTRE; // const 1.0 audio × center pan
    assert!(
        (rms_l(&block) - target).abs() < 0.05,
        "mode=Off should bypass envelope: L rms={}, target≈{target}",
        rms_l(&block)
    );
}

#[test]
fn envelope_eval_is_per_sample_not_block() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));

    // Ramp 1.0 → 0.0 across 10ms. With block-midpoint eval over a
    // 100ms block, both halves would see a single midpoint value
    // (~0.5) and rms would be uniform. With per-sample eval, the
    // first-half rms (≈0.75) is clearly louder than the second-half
    // rms (≈0.25).
    let loc = EnvelopeLocation::new(TrackRef::Guid(t), EnvelopeRef::Type(EnvelopeType::Volume));
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 1.0),
    );
    Automation::add_point(
        &daw,
        ctx,
        loc,
        AddPointParams::linear(PositionInSeconds::from_seconds(0.01), 0.0),
    );

    let r = ProjectRenderer::new(&daw, "p", SAMPLE_RATE);
    // Render the first 100ms in two halves and compare.
    let half = SAMPLE_RATE as usize / 20; // 50ms
    let a = r.render_block(0, half);
    let b = r.render_block(half as u64, half);
    // First half hears the full ramp (loud) then silence. Second
    // half is fully past the ramp (silent).
    assert!(
        rms_l(&a) > rms_l(&b) + 0.05,
        "per-sample eval: first half ({}) should be louder than second ({})",
        rms_l(&a),
        rms_l(&b)
    );
    assert!(rms_l(&b) < 0.02, "post-ramp half should be near-silent");
}

#[test]
fn item_position_shifts_into_block() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx, "T", None).unwrap();
    // Item starts at 0.5s, lasts 0.5s.
    create_item_with_audio(&daw, &guid, &t, 0.5, 0.5, const_audio(1.0));

    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    // Render first 0.25s — silent.
    let early = r.render_block(0, SAMPLE_RATE as usize / 4);
    assert!(rms_l(&early) < 0.001, "early block should be silent");
    // Render 0.5-0.75s — audible.
    let mid = r.render_block(SAMPLE_RATE as u64 / 2, SAMPLE_RATE as usize / 4);
    assert!(
        rms_l(&mid) > 0.3,
        "mid block should be audible: {}",
        rms_l(&mid)
    );
}

#[test]
fn render_writes_post_fader_track_meters() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(0.5));

    // Install a meter bank like AudioEngine::attached_to does.
    daw.set_meters(daw_standalone::metering::Meters::new(1));

    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let _ = r.render_block(0, 512);
    let meters = daw.meters();
    let cell = meters.cell(0).expect("cell 0");
    // Constant 0.5, center pan, unity volume → 0.5 per side.
    let target = 0.5 * CENTRE;
    assert!(
        (cell.peak(0) - target).abs() < 0.05,
        "L peak={}, target={target}",
        cell.peak(0)
    );
    assert!((cell.peak(1) - target).abs() < 0.05);

    // Halve the fader: meter is post-fader, peak follows.
    Tracks::set_volume(&daw, ctx, TrackRef::Guid(t.clone()), 0.5).unwrap();
    let _ = r.render_block(0, 512);
    assert!(
        (meters.cell(0).unwrap().peak(0) - target * 0.5).abs() < 0.05,
        "post-fader peak={}",
        meters.cell(0).unwrap().peak(0)
    );
}

#[test]
fn loop_source_tiles_past_source_end() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx, "T", None).unwrap();
    // 2s item over a 1s source (const_audio is 1s) — the second half
    // only sounds when LOOP is honored.
    let (item_guid, _take) = create_item_with_audio(&daw, &guid, &t, 0.0, 2.0, const_audio(0.5));
    daw.write_project(&guid, |p| {
        p.items.get_mut(&item_guid).unwrap().item.loop_source = true;
    });

    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    // Render half a second starting at 1.5s — past the source length.
    let block = r.render_block(
        (SAMPLE_RATE + SAMPLE_RATE / 2) as u64,
        (SAMPLE_RATE / 4) as usize,
    );
    let target = 0.5 * CENTRE;
    assert!(
        (rms_l(&block) - target).abs() < 0.05,
        "looped tail should sound: rms={}",
        rms_l(&block)
    );

    // Sanity: with loop off the same window is silent.
    daw.write_project(&guid, |p| {
        p.items.get_mut(&item_guid).unwrap().item.loop_source = false;
    });
    let block = r.render_block(
        (SAMPLE_RATE + SAMPLE_RATE / 2) as u64,
        (SAMPLE_RATE / 4) as usize,
    );
    assert!(rms_l(&block) < 1e-6, "unlooped tail must be silent");
}

#[test]
fn master_volume_pan_mute_apply() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx, "T", None).unwrap();
    create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, const_audio(0.5));
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let unity = 0.5 * CENTRE;

    daw.write_project(&guid, |p| p.master_volume = 0.5);
    let block = r.render_block(0, 512);
    assert!(
        (rms_l(&block) - unity * 0.5).abs() < 0.05,
        "master gain halves output: rms={}",
        rms_l(&block)
    );

    daw.write_project(&guid, |p| p.master_muted = true);
    let block = r.render_block(0, 512);
    assert!(rms_l(&block) < 1e-6, "master mute silences output");
}

/// CHANMODE maps source channels onto the item's stereo signal:
/// reverse stereo swaps, mono-left/right pick a channel.
#[test]
fn channel_modes_remap_source_channels() {
    let stereo = |l: f32, r: f32| -> DecodedAudio {
        let frames = SAMPLE_RATE as usize;
        let mut samples = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            samples.push(l);
            samples.push(r);
        }
        DecodedAudio::new(samples, 2, SAMPLE_RATE)
    };
    // L = 0.8, R = 0.2.
    let run = |mode: u32| -> (f32, f32) {
        let (daw, guid) = seeded();
        let ctx = ProjectContext::Current;
        let t = Tracks::add(&daw, ctx, "T", None).unwrap();
        let (item_guid, _) = create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, stereo(0.8, 0.2));
        daw.write_project(&guid, |p| {
            let tl = p.takes.get_mut(&item_guid).unwrap();
            let idx = tl.active_idx as usize;
            tl.takes[idx].channel_mode = mode;
        });
        let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
        let block = r.render_block(0, 512);
        (rms_l(&block), rms_r(&block))
    };
    let g = CENTRE; // centred: unity

    let (l, r) = run(0); // normal
    assert!(
        (l - 0.8 * g).abs() < 0.02 && (r - 0.2 * g).abs() < 0.02,
        "normal {l} {r}"
    );
    let (l, r) = run(1); // reverse stereo
    assert!(
        (l - 0.2 * g).abs() < 0.02 && (r - 0.8 * g).abs() < 0.02,
        "reversed {l} {r}"
    );
    let (l, r) = run(2); // mono downmix
    assert!(
        (l - 0.5 * g).abs() < 0.02 && (r - 0.5 * g).abs() < 0.02,
        "downmix {l} {r}"
    );
    let (l, r) = run(3); // mono left
    assert!(
        (l - 0.8 * g).abs() < 0.02 && (r - 0.8 * g).abs() < 0.02,
        "mono-L {l} {r}"
    );
    let (l, r) = run(4); // mono right
    assert!(
        (l - 0.2 * g).abs() < 0.02 && (r - 0.2 * g).abs() < 0.02,
        "mono-R {l} {r}"
    );
}

/// Send taps: a post-fader send scales with the source fader; a
/// pre-fader (PostFx) send ignores it.
#[test]
fn send_modes_tap_pre_or_post_fader() {
    use daw_proto::routing::SendMode;
    let run = |mode: SendMode| -> f32 {
        let (daw, guid) = seeded();
        let ctx = ProjectContext::Current;
        let src = Tracks::add(&daw, ctx.clone(), "Src", None).unwrap();
        let bus = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
        create_item_with_audio(&daw, &guid, &src, 0.0, 1.0, const_audio(0.5));
        // Source fader at half; its parent send disabled so ONLY the
        // bus contributes to master.
        Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(src.clone()), 0.5).unwrap();
        Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(src.clone()), false)
            .unwrap();
        daw.write_project(&guid, |p| {
            let mut route = daw_proto::TrackRoute::default();
            route.route_type = RouteType::Send;
            route.source_track_guid = src.clone();
            route.dest_track_guid = Some(bus.clone());
            route.volume = 1.0;
            route.send_mode = mode;
            p.sends.entry(src.clone()).or_default().push(route);
        });
        let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
        rms_l(&r.render_block(0, 512))
    };
    // Constant-power centre pan costs √½ per stage. Post-fader signal
    // passes the source pan stage + send pan + bus pan (3 stages) and
    // the 0.5 fader; pre-fader taps skip the source's gain stage
    // entirely (send pan + bus pan only).
    let g = CENTRE;
    let post_expect = 0.5 * 0.5 * g * g * g;
    let pre_expect = 0.5 * g * g;
    let post = run(SendMode::PostFader);
    let pre = run(SendMode::PostFx);
    let prefx = run(SendMode::PreFx);
    assert!(
        (post - post_expect).abs() < 0.03,
        "post-fader send scales with fader: {post} vs {post_expect}"
    );
    assert!(
        (pre - pre_expect).abs() < 0.03,
        "pre-fader send ignores fader: {pre} vs {pre_expect}"
    );
    assert!(
        (prefx - pre_expect).abs() < 0.03,
        "pre-FX send ignores fader: {prefx} vs {pre_expect}"
    );
}

/// VCA grouping at playback (user guide §5.16): the lead's fader
/// dB-adds (linear-multiplies) onto followers, the lead's pan offsets
/// follower pan, and the lead's mute — button or ENVELOPE — gates them
/// without touching the follower's own mute button.
#[test]
fn vca_lead_scales_and_mutes_followers() {
    use daw_proto::automation::{EnvelopePoint, EnvelopeShape, EnvelopeType};
    use daw_standalone::sync::{EnvelopeData, EnvelopeKey};

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let lead = Tracks::add(&daw, ctx.clone(), "VCA", None).unwrap();
    let follower = Tracks::add(&daw, ctx.clone(), "F", None).unwrap();
    create_item_with_audio(&daw, &guid, &follower, 0.0, 1.0, const_audio(0.5));
    daw.write_project(&guid, |p| {
        p.tracks[0].grouping.vca_lead = 1;
        p.tracks[1].grouping.vca_follow = 1;
    });

    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let unity = 0.5 * CENTRE;
    let base = rms_l(&r.render_block(0, 512));
    assert!((base - unity).abs() < 0.02, "lead at unity: {base}");

    // Lead fader at half → follower output halves (faders don't move:
    // the follower's stored volume stays 1.0).
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(lead.clone()), 0.5).unwrap();
    let halved = rms_l(&r.render_block(0, 512));
    assert!(
        (halved - unity * 0.5).abs() < 0.02,
        "VCA lead scales follower: {halved} vs {}",
        unity * 0.5
    );
    let follower_vol = daw.read_project(&guid, |p| p.tracks[1].volume).unwrap();
    assert!((follower_vol - 1.0).abs() < 1e-9, "follower fader unmoved");

    // The lead's mute BUTTON gates followers (the facade behaviour the
    // language switch relies on: one VCA per language, mute the lead).
    // The follower's own button stays up.
    Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(lead.clone()), true).unwrap();
    let gated_by_button = rms_l(&r.render_block(0, 512));
    assert!(
        gated_by_button < 1e-6,
        "lead mute button mutes follower: {gated_by_button}"
    );
    let follower_muted = daw.read_project(&guid, |p| p.tracks[1].muted).unwrap();
    assert!(!follower_muted, "follower mute button unmoved");
    Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(lead.clone()), false).unwrap();
    let back = rms_l(&r.render_block(0, 512));
    assert!(
        (back - unity * 0.5).abs() < 0.02,
        "unmuting the lead restores the follower: {back}"
    );

    // A mute ENVELOPE on the lead gates the follower.
    daw.write_project(&guid, |p| {
        let mut data = EnvelopeData::new();
        data.points = vec![EnvelopePoint {
            index: 0,
            time: daw_proto::PositionInSeconds::from_seconds(0.0),
            value: 1.0, // ours: >0.5 = muted
            shape: EnvelopeShape::Square,
            tension: 0.0,
            selected: false,
        }];
        p.envelopes
            .insert((lead.clone(), EnvelopeKey::Track(EnvelopeType::Mute)), data);
    });
    let gated = rms_l(&r.render_block(0, 512));
    assert!(gated < 1e-6, "lead mute envelope gates follower: {gated}");
}

/// VCA pan: the lead's pan offsets follower pan at playback.
#[test]
fn vca_lead_pan_offsets_follower() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let lead = Tracks::add(&daw, ctx.clone(), "VCA", None).unwrap();
    let follower = Tracks::add(&daw, ctx.clone(), "F", None).unwrap();
    create_item_with_audio(&daw, &guid, &follower, 0.0, 1.0, const_audio(0.5));
    daw.write_project(&guid, |p| {
        p.tracks[0].grouping.vca_lead = 1;
        p.tracks[1].grouping.vca_follow = 1;
    });

    // Pan the LEAD hard left → the follower's signal goes left.
    Tracks::set_pan(&daw, ctx, TrackRef::Guid(lead), -1.0).unwrap();
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let block = r.render_block(0, 512);
    assert!(
        rms_l(&block) > 0.3,
        "left carries signal: {}",
        rms_l(&block)
    );
    assert!(rms_r(&block) < 1e-6, "right silent: {}", rms_r(&block));
}

/// Record-arm grouping gangs the gesture: arming the lead arms every
/// follower (the session's "BAND RECORD VCA" workflow).
#[test]
fn recarm_group_arms_followers() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;
    let lead = Tracks::add(&daw, ctx.clone(), "BAND VCA", None).unwrap();
    let a = Tracks::add(&daw, ctx.clone(), "In", None).unwrap();
    let b = Tracks::add(&daw, ctx.clone(), "Out", None).unwrap();
    let c = Tracks::add(&daw, ctx.clone(), "Unrelated", None).unwrap();
    daw.write_project(&guid, |p| {
        p.tracks[0].grouping.recarm_lead = 0b110; // leads groups 2+3
        p.tracks[1].grouping.recarm_follow = 0b010; // group 2
        p.tracks[2].grouping.recarm_follow = 0b100; // group 3
        p.tracks[3].grouping.recarm_follow = 0b1000; // group 4 — untouched
    });

    Tracks::set_armed(&daw, ctx.clone(), TrackRef::Guid(lead), true).unwrap();
    let tracks = Tracks::all(&daw, ctx);
    assert!(tracks[0].armed);
    assert!(tracks[1].armed, "group-2 follower armed");
    assert!(tracks[2].armed, "group-3 follower armed");
    assert!(!tracks[3].armed, "non-member untouched");
    let _ = (a, b, c);
}

/// A source whose value IS its time: sample i = i / SAMPLE_RATE (1 s).
fn ramp_audio() -> DecodedAudio {
    let frames = SAMPLE_RATE as usize;
    DecodedAudio::new(
        (0..frames).map(|i| i as f32 / SAMPLE_RATE as f32).collect(),
        1,
        SAMPLE_RATE,
    )
}

// r[verify drums.open.playback-warped]
#[test]
fn stretch_markers_warp_playback() {
    use daw_proto::{StretchMarker, StretchMarkers, StretchTakeRef, TakeRef};
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid.clone());
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let (item_guid, _) = create_item_with_audio(&daw, &guid, &t, 0.0, 1.0, ramp_audio());
    // No fades, so the rendered value is the source value × pan law.
    let none = ProtoDuration::from_seconds(0.0);
    let shape = daw_proto::item::FadeShape::Linear;
    Items::set_fade_in(
        &daw,
        ctx.clone(),
        ItemRef::Guid(item_guid.clone()),
        none,
        shape,
    )
    .unwrap();
    Items::set_fade_out(
        &daw,
        ctx.clone(),
        ItemRef::Guid(item_guid.clone()),
        none,
        shape,
    )
    .unwrap();

    // Play 0.5 s of take time over 0.25 s of source (rate ½... i.e. slowed):
    // take 0.5 → source 0.25, then unstretched on.
    StretchMarkers::set_stretch_markers(
        &daw,
        StretchTakeRef {
            project: ctx.clone(),
            item: ItemRef::Guid(item_guid.clone()),
            take: TakeRef::Active,
        },
        vec![StretchMarker::new(0.0, 0.0), StretchMarker::new(0.5, 0.25)],
    )
    .expect("markers");

    let pan = CENTRE;
    let r = ProjectRenderer::new(&daw, &guid, SAMPLE_RATE);
    let block = r.render_block(0, SAMPLE_RATE as usize);
    let at = |secs: f64| block.samples[(secs * SAMPLE_RATE as f64) as usize * 2] / pan;
    // Inside the stretched span: source time = take time / 2.
    assert!((at(0.2) - 0.1).abs() < 1e-3, "at 0.2 s got {}", at(0.2));
    assert!((at(0.4) - 0.2).abs() < 1e-3, "at 0.4 s got {}", at(0.4));
    // After the last marker: rate 1 again, continuing from source 0.25.
    assert!((at(0.75) - 0.5).abs() < 1e-3, "at 0.75 s got {}", at(0.75));
}

/// Read (and the writing modes) play the volume envelope as the volume:
/// the fader is not applied on top. Trim/Read keeps it as a trim.
#[test]
fn a_read_volume_envelope_is_the_volume_and_trim_read_trims_it() {
    use daw_proto::Automation;
    use daw_proto::automation::{AddPointParams, EnvelopeLocation, EnvelopeRef, EnvelopeType};
    use daw_proto::primitives::{AutomationMode, PositionInSeconds};

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Project(guid);
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    create_item_with_audio(&daw, "p", &t, 0.0, 1.0, const_audio(1.0));
    // The fader at a quarter, the envelope at a half.
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(t.clone()), 0.25).unwrap();
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(t.clone()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 0.5),
    );
    let level = || {
        let block =
            ProjectRenderer::new(&daw, "p", SAMPLE_RATE).render_block(0, SAMPLE_RATE as usize / 10);
        rms_l(&block) + rms_r(&block)
    };
    let trimmed = level();
    Automation::set_automation_mode(&daw, ctx, loc, AutomationMode::Read);
    let read = level();
    assert!(
        (read / trimmed - 4.0).abs() < 0.05,
        "Read plays the envelope's half, Trim/Read a half of the fader's quarter: {read} vs {trimmed}"
    );
}
