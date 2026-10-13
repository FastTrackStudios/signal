//! End-to-end realtime-automation write tests.

use std::time::Duration;

use daw_proto::automation::{EnvelopeLocation, EnvelopeRef, EnvelopeType};
use daw_proto::primitives::AutomationMode;
use daw_proto::project::ProjectContext;
use daw_proto::transport::service::Transport;
use daw_proto::{Automation, ProjectInfo, TrackRef, Tracks};
use daw_standalone::TouchableParam;
use daw_standalone::sync::Standalone;

fn seeded() -> (Standalone, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    (daw, guid)
}

fn set_mode(daw: &Standalone, ctx: ProjectContext, track_guid: &str, mode: AutomationMode) {
    Automation::set_automation_mode(
        daw,
        ctx,
        EnvelopeLocation::new(
            TrackRef::Guid(track_guid.to_string()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        mode,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_param_in_write_mode_records_during_playback() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();

    // Touch the volume so the envelope exists with mode=Write.
    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        daw_proto::automation::AddPointParams::linear(
            daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            1.0,
        ),
    );
    set_mode(&daw, ctx.clone(), &t, AutomationMode::Write);

    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let param = TouchableParam::TrackVolume {
        track_guid: t.clone(),
    };
    daw.write_param(ctx.clone(), param.clone(), 0.5).unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    daw.write_param(ctx.clone(), param, 0.25).unwrap();

    // Two new points should have been added (we started with 1).
    let pts = Automation::points(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
    );
    assert!(pts.len() >= 3, "expected ≥3 points, got {pts:?}");

    // Static fader also updated.
    let track = Tracks::get(&daw, ctx, TrackRef::Guid(t)).unwrap();
    assert!((track.volume - 0.25).abs() < 1e-9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_param_in_touch_mode_only_records_while_touched() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        daw_proto::automation::AddPointParams::linear(
            daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            1.0,
        ),
    );
    set_mode(&daw, ctx.clone(), &t, AutomationMode::Touch);

    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;

    let param = TouchableParam::TrackVolume {
        track_guid: t.clone(),
    };

    // Not touched yet → no recording.
    daw.write_param(ctx.clone(), param.clone(), 0.5).unwrap();
    let pts = Automation::points(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
    );
    assert_eq!(pts.len(), 1, "touch mode without touch should not record");

    // Touch → record.
    daw.touch_param(param.clone());
    daw.write_param(ctx.clone(), param.clone(), 0.4).unwrap();
    let pts = Automation::points(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
    );
    assert!(pts.len() >= 2, "touched → point should record");

    // Release → stops recording.
    daw.release_param(param.clone());
    let count_before = pts.len();
    daw.write_param(ctx.clone(), param, 0.3).unwrap();
    let pts = Automation::points(
        &daw,
        ctx,
        EnvelopeLocation::new(TrackRef::Guid(t), EnvelopeRef::Type(EnvelopeType::Volume)),
    );
    assert_eq!(pts.len(), count_before, "released → no further recording");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_param_in_read_mode_updates_static_but_no_points() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        daw_proto::automation::AddPointParams::linear(
            daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            1.0,
        ),
    );
    set_mode(&daw, ctx.clone(), &t, AutomationMode::Read);

    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;

    daw.write_param(
        ctx.clone(),
        TouchableParam::TrackVolume {
            track_guid: t.clone(),
        },
        0.5,
    )
    .unwrap();

    let pts = Automation::points(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
    );
    assert_eq!(pts.len(), 1, "read mode never records new points");
    let track = Tracks::get(&daw, ctx, TrackRef::Guid(t)).unwrap();
    assert!((track.volume - 0.5).abs() < 1e-9, "static should update");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_param_when_stopped_does_not_record() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    Automation::add_point(
        &daw,
        ctx.clone(),
        EnvelopeLocation::new(
            TrackRef::Guid(t.clone()),
            EnvelopeRef::Type(EnvelopeType::Volume),
        ),
        daw_proto::automation::AddPointParams::linear(
            daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            1.0,
        ),
    );
    set_mode(&daw, ctx.clone(), &t, AutomationMode::Write);

    // Transport stopped → no recording even in Write mode.
    daw.write_param(
        ctx.clone(),
        TouchableParam::TrackVolume {
            track_guid: t.clone(),
        },
        0.5,
    )
    .unwrap();
    let pts = Automation::points(
        &daw,
        ctx,
        EnvelopeLocation::new(TrackRef::Guid(t), EnvelopeRef::Type(EnvelopeType::Volume)),
    );
    assert_eq!(pts.len(), 1);
}

/// A touch pass over old automation replaces it: between the first and
/// the last point the pass lays, none of the old points is left.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_touch_pass_replaces_what_it_rides_over() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let t = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let at = EnvelopeLocation::new(
        TrackRef::Guid(t.clone()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );
    // Old automation: a point every 10 ms for two seconds, all at 0.9.
    for k in 0..200 {
        Automation::add_point(
            &daw,
            ctx.clone(),
            at.clone(),
            daw_proto::automation::AddPointParams::linear(
                daw_proto::primitives::PositionInSeconds::from_seconds(f64::from(k) * 0.01),
                0.9,
            ),
        );
    }
    set_mode(&daw, ctx.clone(), &t, AutomationMode::Touch);
    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let param = TouchableParam::TrackVolume {
        track_guid: t.clone(),
    };
    daw.touch_param(param.clone());
    for value in [0.5, 0.4, 0.3] {
        daw.write_param(ctx.clone(), param.clone(), value).unwrap();
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    daw.release_param(param);
    let pts = Automation::points(&daw, ctx, at);
    let written: Vec<f64> = pts
        .iter()
        .filter(|p| p.value < 0.6)
        .map(|p| p.time.as_seconds())
        .collect();
    let (Some(first), Some(last)) = (written.first(), written.last()) else {
        panic!("the pass wrote: {pts:?}");
    };
    let left = pts
        .iter()
        .filter(|p| p.value > 0.8 && p.time.as_seconds() > *first && p.time.as_seconds() < *last)
        .count();
    assert_eq!(left, 0, "old points under the pass: {written:?}");
    assert!(
        pts.iter()
            .any(|p| p.value > 0.8 && p.time.as_seconds() > *last),
        "after it, the old automation"
    );
}
