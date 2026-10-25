//! A rig's library as one Loro doc in a `.session` file: what the rig
//! reads and writes through it, what moves in from the styx directory it
//! replaces, and what another device's edits look like to the rig.

use std::path::Path;

use facet::Facet;
use loro::{ExportMode, VersionVector};
use signal_rig_host::book::{self, BookRead, RigBook};
use signal_rig_host::store::StyxDir;

#[derive(Clone, Debug, Default, PartialEq, Facet)]
struct Entry {
    id: String,
    name: String,
    bpm: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
struct Setlists {
    name: String,
    gain: f64,
    songs: Vec<Entry>,
}

fn sets() -> Setlists {
    Setlists {
        name: "CAA 2026".into(),
        gain: 0.5,
        songs: vec![
            Entry {
                id: "a".into(),
                name: "12 to 12".into(),
                bpm: 124,
            },
            Entry {
                id: "b".into(),
                name: "1901".into(),
                bpm: 144,
            },
        ],
    }
}

/// What `from` has that `to` has not.
fn delta(from: &RigBook, to: &RigBook) -> Vec<u8> {
    let since: VersionVector = to.doc().oplog_vv();
    from.doc()
        .export(ExportMode::updates(&since))
        .expect("an export")
}

fn book_in(dir: &Path, name: &str, syncs: bool) -> std::sync::Arc<RigBook> {
    RigBook::open(&dir.join(format!("{name}.session")), &dir.join(name), syncs).expect("a book")
}

#[test]
fn what_the_rig_writes_it_reads_back_and_keeps() {
    let tmp = tempfile::tempdir().expect("a dir");
    let book = book_in(tmp.path(), "rig", false);
    book.write("setlists.styx", &sets()).expect("written");
    match book.read::<Setlists>("setlists.styx") {
        BookRead::Ok(v) => assert_eq!(v, sets()),
        other => panic!("read back: {other:?}"),
    }
    assert_eq!(book.names(""), vec!["setlists.styx".to_owned()]);
    drop(book);
    // Opened again: it is in the file.
    let again = book_in(tmp.path(), "rig", false);
    assert!(matches!(again.read::<Setlists>("setlists.styx"), BookRead::Ok(v) if v == sets()));
}

#[test]
fn a_file_moves_in_once_and_the_file_is_left_alone() {
    let tmp = tempfile::tempdir().expect("a dir");
    let dir = tmp.path().join("rig");
    std::fs::create_dir_all(dir.join("profiles")).expect("dirs");
    let text = facet_styx::to_string(&sets()).expect("styx");
    std::fs::write(dir.join("profiles/worship.styx"), &text).expect("the old file");

    let book = book_in(tmp.path(), "rig", true);
    assert!(!book.contains("profiles/worship.styx"));
    assert!(
        matches!(book.read::<Setlists>("profiles/worship.styx"), BookRead::Ok(v) if v == sets())
    );
    assert!(book.contains("profiles/worship.styx"), "moved in");
    assert_eq!(
        book.names("profiles/"),
        vec!["profiles/worship.styx".to_owned()]
    );
    // The rig writes the book from now on; the file is the backup.
    let mut changed = sets();
    changed.gain = 0.9;
    book.write("profiles/worship.styx", &changed)
        .expect("written");
    assert_eq!(
        std::fs::read_to_string(dir.join("profiles/worship.styx")).expect("file"),
        text
    );
}

#[test]
fn two_devices_change_different_fields_of_one_entry_and_both_land() {
    let tmp = tempfile::tempdir().expect("a dir");
    let ipad = book_in(tmp.path(), "ipad", false);
    let mac = book_in(tmp.path(), "mac", false);
    ipad.write("setlists.styx", &sets()).expect("seed");
    mac.doc().import(&delta(&ipad, &mac)).expect("mac joins");
    let _ = mac.take_changes();

    // Apart: the iPad renames the set, the Mac moves the gain and a tempo.
    let mut on_ipad = sets();
    on_ipad.name = "CAA Awards".into();
    ipad.write("setlists.styx", &on_ipad).expect("ipad");
    let mut on_mac = sets();
    on_mac.gain = 0.8;
    on_mac.songs[1].bpm = 146;
    mac.write("setlists.styx", &on_mac).expect("mac");

    let (to_mac, to_ipad) = (delta(&ipad, &mac), delta(&mac, &ipad));
    mac.doc().import(&to_mac).expect("mac takes the ipad's");
    ipad.doc().import(&to_ipad).expect("ipad takes the mac's");

    let want = Setlists {
        name: "CAA Awards".into(),
        gain: 0.8,
        songs: vec![
            on_mac.songs[0].clone(),
            Entry {
                bpm: 146,
                ..on_mac.songs[1].clone()
            },
        ],
    };
    for (who, book) in [("ipad", &ipad), ("mac", &mac)] {
        match book.read::<Setlists>("setlists.styx") {
            BookRead::Ok(v) => assert_eq!(v, want, "{who}"),
            other => panic!("{who}: {other:?}"),
        }
    }
    // What arrived is reported by name, for the rig's hot reload; the
    // device's own edits are not.
    assert_eq!(mac.take_changes(), vec!["setlists.styx".to_owned()]);
    assert!(mac.take_changes().is_empty(), "once");
}

#[test]
fn styx_dir_reads_and_writes_go_to_the_book_standing_for_it() {
    let tmp = tempfile::tempdir().expect("a dir");
    let book = book_in(tmp.path(), "drums", false);
    book::register(book.clone());
    let store = StyxDir::new(tmp.path().join("drums"));
    store.write("setlists.styx", &sets());
    assert!(
        !tmp.path().join("drums/setlists.styx").exists(),
        "no file written"
    );
    assert_eq!(store.read::<Setlists>("setlists.styx"), Some(sets()));
    let (found, name) = book::for_path(&tmp.path().join("drums/profiles/a.styx")).expect("routed");
    assert_eq!(found.file(), book.file());
    assert_eq!(name, "profiles/a.styx");
}

#[test]
fn a_book_an_account_syncs_does_not_seed_until_it_may() {
    let tmp = tempfile::tempdir().expect("a dir");
    let seed = facet_styx::to_string(&sets()).expect("styx");

    let synced = book_in(tmp.path(), "keys", true);
    book::register(synced.clone());
    let store = StyxDir::new(tmp.path().join("keys"));
    assert_eq!(
        store.read_or_seed("setlists.styx", &seed, Setlists::default),
        sets(),
        "plays the default"
    );
    assert!(
        !synced.contains("setlists.styx"),
        "not written: the account's may arrive"
    );
    synced.allow_seeding();
    let _ = store.read_or_seed("setlists.styx", &seed, Setlists::default);
    assert!(synced.contains("setlists.styx"), "written once it may");

    let local = book_in(tmp.path(), "ekit", false);
    book::register(local.clone());
    let _ = StyxDir::new(tmp.path().join("ekit")).read_or_seed(
        "setlists.styx",
        &seed,
        Setlists::default,
    );
    assert!(
        local.contains("setlists.styx"),
        "nothing syncs it: seeded at once"
    );
}

#[test]
fn where_this_device_is_stays_a_file() {
    let tmp = tempfile::tempdir().expect("a dir");
    let book = book_in(tmp.path(), "bass", false);
    book::register(book.clone());
    let store = StyxDir::new(tmp.path().join("bass"));
    store.write("last-state.styx", &sets());
    assert!(tmp.path().join("bass/last-state.styx").exists(), "a file");
    assert!(!book.contains("last-state.styx"), "never in the book");
    assert!(book::for_path(&tmp.path().join("bass/last-state.styx")).is_none());
    assert!(book::is_device_local("frozen/a.nam") && book::is_device_local(".seeded-songs"));
    assert!(!book::is_device_local("profiles/worship.styx"));
}

#[test]
fn a_profile_deleted_from_the_book_stays_deleted_and_files_list_beside_it() {
    let tmp = tempfile::tempdir().expect("a dir");
    let dir = tmp.path().join("rig");
    std::fs::create_dir_all(dir.join("profiles")).expect("dirs");
    for name in ["blues", "metal"] {
        let text = facet_styx::to_string(&Setlists {
            name: name.into(),
            ..sets()
        })
        .expect("styx");
        std::fs::write(dir.join(format!("profiles/{name}.styx")), text).expect("old file");
    }
    let book = book_in(tmp.path(), "rig", false);
    book.write("profiles/worship.styx", &sets())
        .expect("one only in the book");
    assert_eq!(
        book.names_with_files("profiles/"),
        vec![
            "profiles/blues.styx",
            "profiles/metal.styx",
            "profiles/worship.styx"
        ]
    );
    assert!(matches!(
        book.read::<Setlists>("profiles/blues.styx"),
        BookRead::Ok(_)
    ));
    book.remove("profiles/blues.styx");
    assert!(
        matches!(
            book.read::<Setlists>("profiles/blues.styx"),
            BookRead::Missing
        ),
        "not back from its file"
    );
    assert_eq!(
        book.names_with_files("profiles/"),
        vec!["profiles/metal.styx", "profiles/worship.styx"]
    );
    drop(book);
    // The list of what moved in is this device's, and survives a restart.
    let again = book_in(tmp.path(), "rig", false);
    assert!(matches!(
        again.read::<Setlists>("profiles/blues.styx"),
        BookRead::Missing
    ));
    let (found, prefix) =
        book::for_tree(&dir.join("profiles")).map_or((None, String::new()), |(b, p)| (Some(b), p));
    assert!(found.is_none(), "not registered in this test");
    assert!(prefix.is_empty());
}

#[test]
fn a_library_inside_another_has_its_own_book() {
    let tmp = tempfile::tempdir().expect("a dir");
    let guitar = book_in(tmp.path(), "nest", false);
    book::register(guitar.clone());
    let bass = RigBook::open(
        &tmp.path().join("nest/bass.session"),
        &tmp.path().join("nest/bass"),
        false,
    )
    .expect("bass");
    book::register(bass.clone());
    let (found, name) = book::for_path(&tmp.path().join("nest/bass/songs.styx")).expect("routed");
    assert_eq!(found.file(), bass.file(), "the bass's own book");
    assert_eq!(name, "songs.styx");
    let (found, prefix) = book::for_tree(&tmp.path().join("nest/profiles")).expect("routed");
    assert_eq!(found.file(), guitar.file());
    assert_eq!(prefix, "profiles/");
}

#[test]
fn assets_are_files_the_book_knows_by_content() {
    let tmp = tempfile::tempdir().expect("a dir");
    let dir = tmp.path().join("amp");
    std::fs::create_dir_all(dir.join("models")).expect("dirs");
    std::fs::write(dir.join("models/Fender Clean.nam"), b"a capture's bytes").expect("an asset");
    let book = book_in(tmp.path(), "amp", false);
    book::register(book.clone());
    let store = StyxDir::new(&dir);

    // Resolving a preset's capture (as a load does) records what it is.
    let mut nam = "models/Fender Clean.nam".to_owned();
    store.resolve(&mut nam);
    assert_eq!(nam, dir.join("models/Fender Clean.nam").to_string_lossy());
    let known = book.asset("models/Fender Clean.nam").expect("indexed");
    assert_eq!(known.size, 17);
    assert_eq!(
        known.hash,
        blake3::hash(b"a capture's bytes").to_hex().to_string()
    );
    assert_eq!(
        book.asset_named(&known.hash).as_deref(),
        Some("models/Fender Clean.nam")
    );
    // The bytes are not in the doc: the index is all it holds of them.
    assert!(book.names("models/").is_empty());
    // A capture this device does not have keeps what the index says.
    let mut missing = "models/Not Here.nam".to_owned();
    store.resolve(&mut missing);
    assert!(book.asset("models/Not Here.nam").is_none());
}

#[test]
fn two_devices_index_their_assets_apart_and_both_are_kept() {
    let tmp = tempfile::tempdir().expect("a dir");
    let mut books = Vec::new();
    for (name, file) in [("one", "models/a.nam"), ("two", "models/b.nam")] {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(dir.join("models")).expect("dirs");
        std::fs::write(dir.join(file), name.as_bytes()).expect("asset");
        let b = book_in(tmp.path(), name, false);
        b.note_asset(file);
        books.push(b);
    }
    let (one, two) = (&books[0], &books[1]);
    let (to_two, to_one) = (delta(one, two), delta(two, one));
    two.doc().import(&to_two).expect("two takes one's");
    one.doc().import(&to_one).expect("one takes two's");
    for b in [one, two] {
        assert!(b.asset("models/a.nam").is_some() && b.asset("models/b.nam").is_some());
    }
}
