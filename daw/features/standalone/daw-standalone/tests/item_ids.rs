//! A new item's GUID is never one the project already has: a reopened
//! session keeps the engine's GUIDs it was saved with, and the counter
//! that mints them starts again at zero.

use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, Items, ProjectInfo, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

#[test]
fn a_duplicate_never_takes_a_saved_items_place() {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid);
    let track = Tracks::add(&daw, ctx.clone(), "Kick", None).expect("a track");
    // As a reopened session has it: an item named as the engine names its
    // first.
    let saved = "standalone-item-0000000000000000";
    daw.add_item_with_guid(
        ctx.clone(),
        TrackRef::Guid(track.clone()),
        saved,
        daw_proto::ItemSpan::new(
            daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            daw_proto::primitives::Duration::from_seconds(1.0),
        ),
    )
    .expect("the saved item");
    let other = daw
        .add_item_with_guid(
            ctx.clone(),
            TrackRef::Guid(track.clone()),
            "{B0000000-0000-0000-0000-000000000001}",
            daw_proto::ItemSpan::new(
                daw_proto::primitives::PositionInSeconds::from_seconds(2.0),
                daw_proto::primitives::Duration::from_seconds(1.0),
            ),
        )
        .expect("another");
    let copy = daw
        .duplicate_item(ctx.clone(), ItemRef::Guid(other))
        .expect("a copy");
    assert_ne!(copy, saved, "the copy took the saved item's GUID");
    let items = daw.get_items(ctx, TrackRef::Guid(track));
    assert_eq!(items.len(), 3, "{items:?}");
    assert!(
        items.iter().any(|i| i.guid == saved),
        "the saved item is still there"
    );
}
