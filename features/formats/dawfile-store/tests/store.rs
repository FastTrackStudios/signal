//! The project file: blobs once each, each branch's history appended and
//! folded, alternatives forked — and all of it read back as written.

use dawfile_store::{MAIN, Store, hash_of};

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("a dir");
    let store = Store::open(&dir.path().join("Song.session")).expect("a store");
    (dir, store)
}

/// One step of edits on `doc`, appended to `branch`: an item at `at`.
fn step(store: &Store, branch: &str, doc: &loro::LoroDoc, guid: &str, at: f64) {
    let from = doc.oplog_vv();
    let item = doc
        .get_map("items")
        .insert_container(guid, loro::LoroMap::new())
        .expect("an item");
    item.insert("position", at).expect("a position");
    doc.commit();
    let update = doc
        .export(loro::ExportMode::updates(&from))
        .expect("an update");
    store
        .append_update(branch, &format!("Add {guid}"), &update)
        .expect("appended");
}

/// A blob put twice — by two branches, two joined projects — is one row,
/// and comes back as it went in.
#[test]
fn a_blob_is_kept_once() {
    let (_dir, store) = store();
    let state = b"kit=heavy thwacky".repeat(1_000);
    let a = store.put_blob("fx", &state).expect("put");
    let b = store.put_blob("fx", &state).expect("put again");
    assert_eq!(a, b);
    assert_eq!(a, hash_of(&state));
    let (count, size, packed) = store.blob_totals().expect("totals");
    assert_eq!(count, 1, "stored once");
    assert_eq!(usize::try_from(size).ok(), Some(state.len()));
    assert!(packed < size / 10, "compressed: {packed} of {size}");
    assert_eq!(store.blob(&a).expect("read"), state);
    assert!(store.blob(&hash_of(b"nothing")).is_err(), "not here");
}

/// The song's undo steps load back as the doc they made, compacted or
/// not, from the file reopened.
#[test]
fn the_song_loads_back_compacted_or_not() {
    let (dir, store) = store();
    let doc = loro::LoroDoc::new();
    for (i, guid) in ["k1", "k2", "k3"].iter().enumerate() {
        step(
            &store,
            MAIN,
            &doc,
            guid,
            f64::from(u8::try_from(i).unwrap_or(0)),
        );
    }
    assert_eq!(
        store.load(MAIN).expect("loaded").get_deep_value(),
        doc.get_deep_value()
    );
    let names = |store: &Store| -> Vec<String> {
        store
            .steps(MAIN)
            .expect("the steps")
            .into_iter()
            .map(|s| s.label)
            .collect()
    };
    assert_eq!(names(&store), ["Add k1", "Add k2", "Add k3"]);
    store.compact(MAIN, &doc).expect("compacted");
    assert!(names(&store).is_empty(), "folded into the snapshot");
    step(&store, MAIN, &doc, "k4", 9.0);
    assert_eq!(names(&store), ["Add k4"]);
    drop(store);
    let reopened = Store::open(&dir.path().join("Song.session")).expect("reopened");
    assert_eq!(
        reopened.load(MAIN).expect("loaded").get_deep_value(),
        doc.get_deep_value(),
        "a snapshot and an update after it"
    );
}

/// An alternative forked from the song keeps what the song had then, its
/// own edits after, and none of the song's later ones — and the song
/// none of its.
#[test]
fn an_alternative_is_the_song_forked_and_edited_on_its_own() {
    let (_dir, store) = store();
    let song = loro::LoroDoc::new();
    step(&store, MAIN, &song, "k1", 1.0);
    store.fork("Organized", MAIN, &song).expect("forked");
    step(&store, MAIN, &song, "k2", 2.0);

    let organized = store.load("Organized").expect("the alternative");
    assert_eq!(organized.get_map("items").len(), 1, "the song as it was");
    step(&store, "Organized", &organized, "o1", 5.0);

    let organized = store.load("Organized").expect("again");
    let items = organized.get_map("items");
    assert!(
        items.get("k1").is_some() && items.get("o1").is_some(),
        "its own edit"
    );
    assert!(items.get("k2").is_none(), "not the song's later one");
    let song_back = store.load(MAIN).expect("the song");
    assert!(
        song_back.get_map("items").get("o1").is_none(),
        "the song has none of its"
    );
    let names: Vec<String> = store
        .branches()
        .expect("branches")
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, vec![MAIN.to_owned(), "Organized".to_owned()]);
}

/// A file from another schema is refused, not misread.
#[test]
fn another_schema_is_refused() {
    let dir = tempfile::tempdir().expect("a dir");
    let path = dir.path().join("Song.session");
    Store::open(&path)
        .expect("a store")
        .set_meta("schema", "99")
        .expect("set");
    assert!(Store::open(&path).is_err());
}

/// After a checkpoint the project is one file: nothing left in a log
/// beside it.
#[test]
fn a_checkpointed_project_is_one_file() {
    let (dir, store) = store();
    let doc = loro::LoroDoc::new();
    step(&store, MAIN, &doc, "k1", 1.0);
    store
        .put_blob("fx", &b"state".repeat(10_000))
        .expect("a blob");
    store.checkpoint().expect("checkpointed");
    let wal = dir.path().join("Song.session-wal");
    let left = std::fs::metadata(&wal).map_or(0, |m| m.len());
    assert_eq!(left, 0, "the log is empty");
    assert!(
        std::fs::metadata(dir.path().join("Song.session")).map_or(0, |m| m.len()) > 4096,
        "the file holds it"
    );
}
