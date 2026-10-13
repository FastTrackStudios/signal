//! Where the files are found. The album file by walking up from the
//! project's directory to the first directory holding one (as the
//! keybind profile is found); the studio profile in the app config
//! directory, the active one a per-machine setting defaulting to the
//! hostname.
//!
//! r[verify flow.patch-list.project-level]
//! r[verify flow.patch-list.studio-profiles]

use std::fs;
use std::path::Path;

use patch_list::Resolved;
use patch_list::discover::{ALBUM_FILE, Studios, find_album, write_album};
use patch_list::{Entry, PatchList};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const ALBUM: &str = include_str!("../fixtures/album/patch-list.styx");
const ROOM: &str = include_str!("../fixtures/studios/golden-room.styx");

fn write(path: &Path, text: &str) -> Result {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

#[test]
fn the_album_file_is_found_by_walking_up_from_the_project() -> Result {
    let tmp = tempfile::tempdir()?;
    let album = tmp.path().join("album");
    write(&album.join(ALBUM_FILE), ALBUM)?;
    let song = album.join("songs").join("song-one");
    write(&song.join("song-one.rpp"), "<REAPER_PROJECT>")?;

    assert_eq!(find_album(&song), Some(album.join(ALBUM_FILE)));
    // From the album directory itself, too.
    assert_eq!(find_album(&album), Some(album.join(ALBUM_FILE)));
    Ok(())
}

#[test]
fn the_nearest_album_file_wins() -> Result {
    let tmp = tempfile::tempdir()?;
    let outer = tmp.path().join("label");
    write(&outer.join(ALBUM_FILE), "performers {}")?;
    let inner = outer.join("album");
    write(&inner.join(ALBUM_FILE), ALBUM)?;
    let song = inner.join("song");
    fs::create_dir_all(&song)?;

    assert_eq!(find_album(&song), Some(inner.join(ALBUM_FILE)));
    Ok(())
}

#[test]
fn a_project_with_no_album_file_above_it_has_none() -> Result {
    let tmp = tempfile::tempdir()?;
    let song = tmp.path().join("loose").join("song");
    fs::create_dir_all(&song)?;
    // The walk stops at the tree's root; nothing in a fresh temp tree
    // has one. (A file in the temp root's ancestors would be a machine
    // with a patch list over /tmp, which is not a case worth guarding.)
    assert_eq!(find_album(&song), None);
    Ok(())
}

#[test]
fn the_active_profile_is_the_machine_setting() -> Result {
    let tmp = tempfile::tempdir()?;
    let studios = Studios::at(tmp.path());
    write(&studios.profile_path("golden-room"), ROOM)?;
    write(&tmp.path().join("studio.styx"), "active golden-room")?;

    assert_eq!(studios.active_name(), "golden-room");
    let (name, room) = studios.active().ok_or("the active profile did not load")?;
    assert_eq!(name, "golden-room");
    assert_eq!(room.resolve_input("DI 3"), Resolved::Audio { channel: 2 });
    Ok(())
}

#[test]
fn the_active_profile_defaults_to_the_hostname() -> Result {
    let tmp = tempfile::tempdir()?;
    let studios = Studios::at(tmp.path());
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    assert_eq!(studios.active_name(), host);

    // With no profile for this host, the active profile is absent —
    // not an error: a machine that has not been given a room yet.
    assert!(studios.active().is_none());

    write(&studios.profile_path(&host), ROOM)?;
    let (name, _) = studios.active().ok_or("this host's profile did not load")?;
    assert_eq!(name, host);
    Ok(())
}

#[test]
fn a_profile_that_does_not_parse_is_an_error_not_an_absence() -> Result {
    let tmp = tempfile::tempdir()?;
    let studios = Studios::at(tmp.path());
    write(&studios.profile_path("broken"), "inputs (not a map)")?;

    let Err(err) = studios.load("broken") else {
        return Err("a broken profile loaded".into());
    };
    assert!(!err.is_missing(), "{err}");
    assert!(err.to_string().contains("broken"), "{err}");

    // And a missing one is a distinct, named absence.
    let Err(missing) = studios.load("nowhere") else {
        return Err("a profile that is not there loaded".into());
    };
    assert!(missing.is_missing(), "{missing}");
    Ok(())
}

#[test]
fn the_profile_directory_is_studios_under_the_config_root() {
    let studios = Studios::at(Path::new("/cfg/fts"));
    assert_eq!(
        studios.profile_path("golden-room"),
        Path::new("/cfg/fts/studios/golden-room.styx")
    );
}

#[test]
fn writing_the_album_is_read_back_by_from_styx() -> Result {
    // Editing destinations (spec #48): edits write the album file by
    // default. `write_album` is that write; the round trip through
    // `PatchList::from_styx` is what proves it wrote a document this
    // crate can open again, not just bytes.
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join(ALBUM_FILE);
    let mut list = PatchList::from_styx(ALBUM)?;
    list.performers
        .get_mut("cody")
        .and_then(|rigs| rigs.get_mut("guitar"))
        .ok_or("cody's guitar rig")?
        .insert("di".to_owned(), Entry::Role("DI 9".into()));

    write_album(&path, &list)?;

    let read_back = PatchList::from_styx(&fs::read_to_string(&path)?)?;
    assert_eq!(read_back, list);
    Ok(())
}
