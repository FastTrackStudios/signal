//! The save-as experiment against the real kit session.
//!
//! Skips when the project is not mounted, or mounted but not writable by
//! whatever account is running the test (the self-hosted CI runner's
//! service account is not `cody`, so a personal drive at 0755 is a skip,
//! not a failure — this test writes beside the original file). The
//! written file lands in a temp dir copy of nothing — it goes beside the
//! original, so this test deletes what it writes.

#![cfg(feature = "rpp-save")]

use daw_proto::{Projects, Tracks};
use daw_standalone::save::save_project_as;
use daw_standalone::sync::Standalone;

const RPP: &str = "/run/media/AudioHaven/Project/02 LORD OF THE FIGHT/02 LORD OF THE FIGHT.RPP";

/// Deletes the written file when dropped, even on panic.
struct Written(std::path::PathBuf);

impl Drop for Written {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// r[verify drums.save.patched]
// r[verify drums.save.reopens]
#[test]
fn the_real_session_saves_byte_identical_and_reopens() {
    let dir = std::path::Path::new(RPP)
        .parent()
        .expect("RPP has a parent dir");
    if !dir.exists() {
        eprintln!("skipping: {RPP} not mounted");
        return;
    }
    let probe = dir.join(".save_as_real-write-probe");
    if let Err(e) = std::fs::write(&probe, b"") {
        eprintln!("skipping: {dir:?} not writable here: {e}");
        return;
    }
    let _ = std::fs::remove_file(&probe);
    let daw = Standalone::new();
    let info = Projects::open(&daw, RPP).expect("open");

    let (path, changes) =
        daw_standalone::save::save_project_as_reported(&daw, &info.guid).expect("saved");
    let written = Written(path.clone());
    if !changes.is_empty() {
        eprintln!("unexpected patch churn ({} changes):", changes.len());
        for line in changes.iter().take(40) {
            eprintln!("  {line}");
        }
    }
    assert_ne!(path.to_string_lossy(), RPP);

    // Unedited: byte-identical to the 3.9 MB original.
    let original = std::fs::read(RPP).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(saved.len(), original.len(), "same size");
    assert_eq!(saved, original, "byte-identical");

    // And it reopens: same track list, same media resolution (the file
    // sits in the same directory, so relative Media/ paths still work).
    let daw2 = Standalone::new();
    let info2 = Projects::open(&daw2, &path.to_string_lossy()).expect("reopen");
    let ctx = daw_proto::ProjectContext::Project(info.guid.clone());
    let ctx2 = daw_proto::ProjectContext::Project(info2.guid.clone());
    let names = |d: &Standalone, c: &daw_proto::ProjectContext| -> Vec<String> {
        Tracks::all(d, c.clone())
            .iter()
            .map(|t| t.name.clone())
            .collect()
    };
    assert_eq!(names(&daw2, &ctx2), names(&daw, &ctx));
    drop(written);

    // Now the real experiment: a drum-editing shaped edit — split the
    // first Kick/In item, slide the piece, warp-mark a take — saved and
    // reopened.
    use daw_proto::{
        Duration, ItemRef, Items, PositionInSeconds, StretchMarker, StretchMarkers, StretchTakeRef,
        TakeRef, Takes, TrackRef,
    };
    let kick_in = Tracks::all(&daw, ctx.clone())
        .into_iter()
        .find(|t| t.name == "In")
        .expect("Kick/In");
    let first = Items::get_items(&daw, ctx.clone(), TrackRef::Guid(kick_in.guid.clone()))
        .into_iter()
        .min_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()))
        .expect("an item");
    let original_item = ItemRef::Guid(first.guid.clone());
    let piece_guid = Items::duplicate_item(&daw, ctx.clone(), original_item.clone()).unwrap();
    let piece = ItemRef::Guid(piece_guid.clone());
    Items::set_length(
        &daw,
        ctx.clone(),
        original_item.clone(),
        Duration::from_seconds(10.0),
    )
    .unwrap();
    Items::set_position(
        &daw,
        ctx.clone(),
        piece.clone(),
        PositionInSeconds::from_seconds(10.03),
    )
    .unwrap();
    Items::set_length(
        &daw,
        ctx.clone(),
        piece.clone(),
        Duration::from_seconds(5.0),
    )
    .unwrap();
    Takes::set_start_offset(
        &daw,
        ctx.clone(),
        piece.clone(),
        TakeRef::Active,
        Duration::from_seconds(10.0),
    )
    .unwrap();
    StretchMarkers::set_stretch_markers(
        &daw,
        StretchTakeRef {
            project: ctx.clone(),
            item: original_item,
            take: TakeRef::Active,
        },
        vec![StretchMarker::new(0.0, 0.0), StretchMarker::new(1.0, 1.2)],
    )
    .unwrap();

    let (path2, changes) =
        daw_standalone::save::save_project_as_reported(&daw, &info.guid).expect("saved edited");
    let written2 = Written(path2.clone());
    assert!(!changes.is_empty(), "the edit shows in the report");
    // The patch touched only what the edit touched: everything it
    // rewrote names our two items.
    for line in &changes {
        assert!(
            line.contains(&first.guid) || line.contains(&piece_guid),
            "phantom change: {line}"
        );
    }

    let daw3 = Standalone::new();
    let info3 = Projects::open(&daw3, &path2.to_string_lossy()).expect("reopen edited");
    let ctx3 = daw_proto::ProjectContext::Project(info3.guid.clone());
    let kick_in3 = Tracks::all(&daw3, ctx3.clone())
        .into_iter()
        .find(|t| t.name == "In")
        .unwrap();
    let mut items3: Vec<_> =
        Items::get_items(&daw3, ctx3.clone(), TrackRef::Guid(kick_in3.guid.clone()));
    items3.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
    let reopened_piece = items3
        .iter()
        .find(|i| (i.position.as_seconds() - 10.03).abs() < 1e-9)
        .expect("the slid piece reopened");
    let off = Takes::get_take(
        &daw3,
        ctx3.clone(),
        ItemRef::Guid(reopened_piece.guid.clone()),
        TakeRef::Active,
    )
    .map(|t| t.start_offset.as_seconds())
    .unwrap();
    assert!((off - 10.0).abs() < 1e-9, "offset survived, got {off}");
    let markers = StretchMarkers::get_stretch_markers(
        &daw3,
        ctx3,
        ItemRef::Guid(first.guid.clone()),
        TakeRef::Active,
    );
    assert_eq!(markers.len(), 2, "SM lines survived the real session");
    assert!((markers[1].source_position - 1.2).abs() < 1e-9);
    drop(written2);
}
