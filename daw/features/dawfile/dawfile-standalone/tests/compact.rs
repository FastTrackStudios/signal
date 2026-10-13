//! Save history and `compact` (#172).
//!
//! The invariant under all of this: everything large is immutable and
//! hash-named, everything mutable is small and textual. Compaction is
//! the only thing allowed to delete from the object store, and only on
//! the basis of reachability.

use dawfile_standalone::DawProject;
use dawfile_standalone::document::DawDocument;
use dawfile_standalone::objects::ObjectStore;

fn project(name: &str) -> DawProject {
    DawProject::new(DawDocument::new(name), ObjectStore::new())
}

/// A track carrying an opaque FX chunk — the reference that keeps a
/// blob reachable.
fn track_with_fx(fx: dawfile_standalone::id::ObjectId) -> dawfile_standalone::document::TrackNode {
    use dawfile_standalone::document::TrackNode;
    use dawfile_standalone::id::EntityId;
    let id = EntityId::new();
    let mut track = daw_proto::track::Track::default();
    track.guid = id.to_string();
    TrackNode {
        id,
        track,
        parent: None,
        envelopes: Vec::new(),
        items: Vec::new(),
        fx_chain: Some(fx),
        input_fx_chain: None,
        comping: daw_proto::track::LaneComping::default(),
        receives: Vec::new(),
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "fts-compact-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn saving_records_an_entry() {
    let dir = tmp("entry");
    let mut p = project("Belief");
    p.save(&dir).unwrap();
    assert_eq!(p.document().saves.len(), 1);
    assert_eq!(p.document().saves[0].seq, 1);

    p.save(&dir).unwrap();
    assert_eq!(p.document().saves.len(), 2);
    assert_eq!(p.document().saves[1].seq, 2, "monotonic, not a timestamp");
}

#[test]
fn the_history_survives_a_reload() {
    let dir = tmp("reload");
    let mut p = project("Belief");
    p.save(&dir).unwrap();
    p.save(&dir).unwrap();

    let back = DawProject::load(&dir).unwrap();
    assert_eq!(back.document().saves.len(), 2, "it is in the manifest");
}

#[test]
fn an_unchanged_object_is_not_rewritten_on_the_next_save() {
    // Where the autosave win comes from: forty Kontakt instances nobody
    // touched cost nothing on save N+1.
    let dir = tmp("unchanged");
    let mut p = project("Belief");
    let id = p.put_object(vec![7u8; 4096]);
    p.save(&dir).unwrap();

    let path = dir.join("objects").join(id.to_string());
    let first = std::fs::metadata(&path).unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    p.save(&dir).unwrap();
    let second = std::fs::metadata(&path).unwrap().modified().unwrap();

    assert_eq!(first, second, "an immutable blob was rewritten");
}

#[test]
fn identical_bytes_are_stored_once() {
    // Content-addressing *is* the dedup, which is why compact has no
    // dedup left to do.
    let mut p = project("Belief");
    let a = p.put_object(b"the same bytes".to_vec());
    let b = p.put_object(b"the same bytes".to_vec());
    assert_eq!(a, b);
    assert_eq!(p.objects().len(), 1);
}

#[test]
fn compact_deletes_what_nothing_points_at() {
    let mut p = project("Belief");
    p.put_object(b"orphan".to_vec());
    assert_eq!(p.objects().len(), 1);

    // No save has ever referenced it and the document does not either.
    let removed = p.compact(None);
    assert_eq!(removed, 1);
    assert_eq!(p.objects().len(), 0);
}

#[test]
fn compact_keeps_what_an_older_save_still_reaches() {
    let _dir = tmp("older");
    let mut p = project("Belief");
    let id = p.put_object(b"referenced by save 1".to_vec());
    p.edit(|d| {
        d.saves.push(dawfile_standalone::document::SaveEntry {
            seq: 99,
            objects: vec![id.clone()],
        })
    });

    let removed = p.compact(None);
    assert_eq!(removed, 0, "history is what makes it reachable");
    assert!(p.objects().contains(&id));
}

#[test]
fn trimming_the_history_is_what_makes_objects_collectable() {
    let mut p = project("Belief");
    let old = p.put_object(b"only save 1 wants this".to_vec());
    let new = p.put_object(b"save 2 wants this".to_vec());
    p.edit(|d| {
        d.saves.push(dawfile_standalone::document::SaveEntry {
            seq: 1,
            objects: vec![old.clone()],
        });
        d.saves.push(dawfile_standalone::document::SaveEntry {
            seq: 2,
            objects: vec![new.clone()],
        });
    });

    // Keeping both keeps both blobs.
    assert_eq!(p.clone().compact(Some(2)), 0);

    // Keeping only the last drops what only the older save reached.
    let removed = p.compact(Some(1));
    assert_eq!(removed, 1);
    assert!(!p.objects().contains(&old));
    assert!(
        p.objects().contains(&new),
        "the retained save still holds it"
    );
    assert_eq!(p.document().saves.len(), 1);
}

#[test]
fn compact_never_touches_what_the_document_itself_references() {
    let mut p = project("Belief");
    let id = p.put_object(b"live fx chunk".to_vec());
    p.edit(|d| d.tracks.push(track_with_fx(id.clone())));
    assert_eq!(p.compact(None), 0);
    assert!(p.objects().contains(&id));
}

#[test]
fn compact_on_disk_removes_the_files_too() {
    let dir = tmp("ondisk");
    let mut p = project("Belief");
    let orphan = p.put_object(b"orphan".to_vec());
    p.save(&dir).unwrap();
    assert!(dir.join("objects").join(orphan.to_string()).exists());

    // The save entry referenced nothing, so the blob is unreachable.
    let removed = p.compact_on_disk(&dir, None).unwrap();
    assert_eq!(removed, 1);
    assert!(!dir.join("objects").join(orphan.to_string()).exists());

    // And the project still opens.
    DawProject::load(&dir).expect("a compacted project still opens");
}

#[test]
fn a_compacted_project_still_opens() {
    let dir = tmp("opens");
    let mut p = project("Belief");
    let live = p.put_object(b"live".to_vec());
    p.edit(|d| d.tracks.push(track_with_fx(live.clone())));
    p.put_object(b"dead".to_vec());
    p.save(&dir).unwrap();
    p.compact_on_disk(&dir, Some(1)).unwrap();

    let back = DawProject::load(&dir).unwrap();
    assert!(back.objects().contains(&live));
}
