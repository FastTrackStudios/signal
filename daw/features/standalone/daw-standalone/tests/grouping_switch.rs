//! Grouping's one switch (`daw_standalone::grouping`): on, arming a group
//! member arms the group; off, only the track touched — its own binary,
//! since the switch is the whole engine's.

use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

#[test]
fn grouping_off_arms_only_the_track_touched() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid);
    let kick = Tracks::add(&daw, ctx.clone(), "Kick In", None).expect("a track");
    let snare = Tracks::add(&daw, ctx.clone(), "Snare Top", None).expect("a track");
    for t in [&kick, &snare] {
        Tracks::set_group_membership(&daw, ctx.clone(), TrackRef::Guid(t.clone()), 1, true)
            .expect("a member");
    }
    let armed = |guid: &str| {
        Tracks::all(&daw, ctx.clone())
            .into_iter()
            .find(|t| t.guid == guid)
            .is_some_and(|t| t.armed)
    };

    daw_standalone::grouping::set_enabled(false);
    Tracks::set_armed(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), true).expect("armed");
    assert!(armed(&kick));
    assert!(!armed(&snare), "grouping off: the snare is left alone");
    Tracks::set_armed(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), false).expect("disarmed");

    daw_standalone::grouping::set_enabled(true);
    Tracks::set_armed(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), true).expect("armed");
    assert!(armed(&snare), "grouping on: the group arms together");
}
