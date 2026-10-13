//! A split on the engine keeps the audio on both sides, and an item the
//! window named before the engine made it is found by that name: what a
//! drum quantize — a cut before every hit, then a move of every slice —
//! stands on.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;

use session_daw::engine::{Edit, apply_now};

#[test]
fn a_split_keeps_its_audio_and_its_new_half_answers_to_the_windows_name() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    session_daw::open::open_silent(&path).expect("the golden session opens");
    let runtime = session_daw::open::runtime().expect("a runtime");
    let read = || {
        runtime.block_on(async {
            let daw = daw::rpc::Daw::try_get().expect("the facade");
            let project = daw.current_project().await.expect("a project");
            project.items().all().await.expect("the items")
        })
    };
    let source = |guid: &str| {
        runtime.block_on(async {
            let daw = daw::rpc::Daw::try_get().expect("the facade");
            let project = daw.current_project().await.expect("a project");
            let item = project.items().by_guid(guid).await.ok().flatten()?;
            let take = item.takes().active().await.ok()?;
            take.info().await.ok()?.source_file_path
        })
    };

    // A performed take: its audio is the whole song, read from where the
    // item starts, so the slip offset is not zero.
    let before = read();
    let item = before
        .iter()
        .find(|i| i.start_offset.as_seconds() > 1.0 && i.length.as_seconds() > 4.0)
        .expect("a performed item")
        .clone();
    let (start, length) = (item.position.as_seconds(), item.length.as_seconds());
    let offset = item.start_offset.as_seconds();
    let file = source(&item.guid).expect("the item plays a file");

    assert!(apply_now(&Edit::SplitItem(
        item.guid.clone(),
        start + 1.0,
        "window-named-half".to_owned()
    )));
    let after = read();
    assert_eq!(after.len(), before.len() + 1, "one item became two");
    let left = after
        .iter()
        .find(|i| i.guid == item.guid)
        .expect("the left half");
    assert!(
        (left.length.as_seconds() - 1.0).abs() < 1e-6,
        "the left half ends at the cut"
    );
    let right = after
        .iter()
        .find(|i| {
            i.track_guid == item.track_guid
                && (i.position.as_seconds() - (start + 1.0)).abs() < 1e-6
        })
        .expect("the right half, at the cut")
        .clone();
    assert!((right.length.as_seconds() - (length - 1.0)).abs() < 1e-6);
    assert!(
        (right.start_offset.as_seconds() - (offset + 1.0)).abs() < 1e-6,
        "the right half plays on from the cut, not from the item's start"
    );
    assert_eq!(source(&right.guid), Some(file), "and plays the same file");

    // The window's name for the new half finds it.
    assert!(apply_now(&Edit::MoveItem(
        "window-named-half".to_owned(),
        start + 1.5
    )));
    let moved = read();
    let right = moved
        .iter()
        .find(|i| i.guid == right.guid)
        .expect("the right half");
    assert!(
        (right.position.as_seconds() - (start + 1.5)).abs() < 1e-6,
        "moved by the window's name"
    );
}
