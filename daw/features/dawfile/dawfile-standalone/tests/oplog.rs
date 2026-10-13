//! The persisted oplog (#173).
//!
//! The two tests the ticket names — divergent offline copies merging,
//! and a hand-edited `.daw` loading with its oplog discarded — plus the
//! properties they depend on.

use dawfile_standalone::document::DawDocument;
use dawfile_standalone::objects::ObjectStore;
use dawfile_standalone::oplog::{self, OplogRef, hash_text};
use dawfile_standalone::{DawProject, DocumentEdit};
use loro::LoroDoc;

/// A doc with one map entry, standing in for a project edit.
fn doc_with(key: &str, value: &str) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.get_map("tracks").insert(key, value).unwrap();
    doc.commit();
    doc
}

fn read(doc: &LoroDoc, key: &str) -> Option<String> {
    doc.get_map("tracks")
        .get(key)
        .and_then(|v| v.into_value().ok())
        .and_then(|v| v.into_string().ok())
        .map(|s| s.to_string())
}

#[test]
fn history_survives_a_round_trip_through_bytes() {
    let doc = doc_with("lead-vox", "Vocals");
    let bytes = oplog::export(&doc).unwrap();
    let back = oplog::import(&bytes).unwrap();
    assert_eq!(read(&back, "lead-vox").as_deref(), Some("Vocals"));
}

#[test]
fn two_divergent_offline_copies_merge() {
    // The whole reason the log is persisted. Both sides start from a
    // shared history, then edit with no contact.
    let base = doc_with("lead-vox", "Vocals");
    let shared = oplog::export(&base).unwrap();

    let alice = oplog::import(&shared).unwrap();
    alice.get_map("tracks").insert("kit", "Drums").unwrap();
    alice.commit();

    let bob = oplog::import(&shared).unwrap();
    bob.get_map("tracks").insert("bass", "Bass").unwrap();
    bob.commit();

    // Each saved their own `.daw`; reconciling replays causality rather
    // than diffing text.
    oplog::merge(&alice, &oplog::export(&bob).unwrap()).unwrap();

    assert_eq!(read(&alice, "lead-vox").as_deref(), Some("Vocals"));
    assert_eq!(
        read(&alice, "kit").as_deref(),
        Some("Drums"),
        "alice's edit"
    );
    assert_eq!(read(&alice, "bass").as_deref(), Some("Bass"), "bob's edit");
}

#[test]
fn merging_is_order_independent() {
    let base = doc_with("lead-vox", "Vocals");
    let shared = oplog::export(&base).unwrap();

    let a = oplog::import(&shared).unwrap();
    a.get_map("tracks").insert("kit", "Drums").unwrap();
    a.commit();
    let b = oplog::import(&shared).unwrap();
    b.get_map("tracks").insert("bass", "Bass").unwrap();
    b.commit();

    let into_a = oplog::import(&shared).unwrap();
    oplog::merge(&into_a, &oplog::export(&a).unwrap()).unwrap();
    oplog::merge(&into_a, &oplog::export(&b).unwrap()).unwrap();

    let into_b = oplog::import(&shared).unwrap();
    oplog::merge(&into_b, &oplog::export(&b).unwrap()).unwrap();
    oplog::merge(&into_b, &oplog::export(&a).unwrap()).unwrap();

    for key in ["lead-vox", "kit", "bass"] {
        assert_eq!(read(&into_a, key), read(&into_b, key), "diverged on {key}");
    }
}

#[test]
fn a_hand_edited_daw_loads_with_its_oplog_discarded() {
    // The rule hand-editability depends on: replaying a log built from
    // text somebody has since overwritten would resurrect the state
    // they just replaced.
    let text = "name Belief\nversion 1\n";
    let doc = doc_with("lead-vox", "Vocals");
    let bytes = oplog::export(&doc).unwrap();
    let stored = OplogRef {
        object: dawfile_standalone::id::ObjectId::of(&bytes),
        text_hash: hash_text(text),
    };

    // Unchanged: the log still belongs to this text.
    assert!(
        oplog::load_if_current(Some(&stored), Some(&bytes), text).is_some(),
        "an untouched project keeps its history"
    );

    // Hand-edited: the log is stale and is dropped.
    let edited = "name Belief Reprise\nversion 1\n";
    assert!(
        oplog::load_if_current(Some(&stored), Some(&bytes), edited).is_none(),
        "the hand edit must win over the log"
    );
}

#[test]
fn discarding_a_stale_log_is_not_an_error() {
    // A hand edit is a normal thing to do. Losing merge history is
    // recoverable; refusing to open the project is not.
    let stored = OplogRef {
        object: dawfile_standalone::id::ObjectId::of(b"x"),
        text_hash: hash_text("original"),
    };
    let got = oplog::load_if_current(Some(&stored), Some(b"garbage"), "edited");
    assert!(got.is_none(), "no panic, no Err — just fresh history");
}

#[test]
fn no_stored_log_means_fresh_history() {
    assert!(oplog::load_if_current(None, None, "anything").is_none());
}

#[test]
fn the_hash_tracks_content_not_timestamps() {
    // A project synced between machines arrives with whatever timestamp
    // the transport felt like.
    assert_eq!(hash_text("same"), hash_text("same"));
    assert_ne!(hash_text("same"), hash_text("same "));
}

#[test]
fn a_corrupt_oplog_costs_history_and_nothing_else() {
    let stored = OplogRef {
        object: dawfile_standalone::id::ObjectId::of(b"x"),
        text_hash: hash_text("text"),
    };
    // Right text, unreadable bytes.
    assert!(oplog::load_if_current(Some(&stored), Some(b"not loro"), "text").is_none());
}

/// A fresh directory per test, matching the idiom the sibling suites
/// use — the tree does not carry `tempfile`.
fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "fts-oplog-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

// ── the oplog wired into the project lifecycle ───────────────────────
//
// The module above was fully implemented and tested and *nothing in the
// project lifecycle called it*: `DawProject` held no doc, `save` never
// exported one, `load` never restored one. So the trade #173 documents
// — hand edits win, at the cost of that project's merge history — was
// not a trade anybody was making, because there was no history to lose.

#[test]
fn history_survives_save_and_load() {
    let dir = scratch("hist");
    let mut project = DawProject::new(DawDocument::new("Hist"), ObjectStore::new());
    project.edit(|d| {
        d.add_track("Lead Vox");
    });
    project.save(&dir).expect("save");

    assert!(
        project.document().oplog.is_some(),
        "saving must record where the history went"
    );

    let reopened = DawProject::load(&dir).expect("load");
    assert!(
        reopened.history().is_some(),
        "a project saved and reopened untouched must keep its history"
    );
}

#[test]
fn a_hand_edited_manifest_discards_history_on_load() {
    // The whole point of storing the text hash. Replaying a log built
    // from text a human has since changed would resurrect what they
    // overwrote.
    let dir = scratch("hand");
    let mut project = DawProject::new(DawDocument::new("Hand"), ObjectStore::new());
    project.edit(|d| {
        d.add_track("Lead Vox");
    });
    project.save(&dir).expect("save");

    let manifest = dawfile_standalone::project::manifest_path(&dir, "Hand");
    let text = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(&manifest, text.replace("Lead Vox", "Lead Vocal")).expect("hand edit");

    let reopened = DawProject::load(&dir).expect("load");
    assert!(
        reopened.history().is_none(),
        "a hand-edited manifest must start fresh history, not replay over the edit"
    );
    // And the hand edit is what survives, which is the property the
    // trade exists to protect.
    assert_eq!(reopened.document().tracks[0].track.name, "Lead Vocal");
}

#[test]
fn a_project_that_was_never_saved_has_no_history() {
    let project = DawProject::new(DawDocument::new("Fresh"), ObjectStore::new());
    assert!(project.history().is_none());
    assert!(project.document().oplog.is_none());
}

#[test]
fn compaction_keeps_the_history_it_finds() {
    // The oplog is a blob like any other, so it has to be reachable or
    // the garbage collector eats the project's history.
    let dir = scratch("compact");
    let mut project = DawProject::new(DawDocument::new("Keep"), ObjectStore::new());
    project.edit(|d| {
        d.add_track("Lead Vox");
    });
    // Several saves, so compaction actually *trims* something and the
    // manifest text changes. The first version of this test compacted a
    // single-save project, nothing moved, and it passed while the bug
    // it was guarding was live.
    for i in 0..4 {
        project.edit(|d| {
            d.add_track(&format!("Extra {i}"));
        });
        project.save(&dir).expect("save");
    }
    let before = std::fs::read_to_string(dawfile_standalone::project::manifest_path(&dir, "Keep"))
        .expect("read");

    project.compact_on_disk(&dir, Some(1)).expect("compact");
    let after = std::fs::read_to_string(dawfile_standalone::project::manifest_path(&dir, "Keep"))
        .expect("read");
    assert_ne!(before, after, "compaction must have rewritten the manifest");

    let reopened = DawProject::load(&dir).expect("load");
    assert!(
        reopened.history().is_some(),
        "compaction rewrote the manifest and left the hash stale, so \
         history was silently discarded on the next load"
    );
}
