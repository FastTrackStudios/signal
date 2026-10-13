use super::*;

fn album() -> tempfile::TempDir {
    let album = tempfile::tempdir().unwrap();
    for song in Song::BOTH {
        std::fs::create_dir_all(album.path().join(song.name()).join("Media")).unwrap();
    }
    album
}

fn write_project(album: &Path, song: Song, text: &str) {
    std::fs::write(
        album
            .join(song.name())
            .join(format!("{}.organized.RPP", song.name())),
        text,
    )
    .unwrap();
}

#[test]
fn legacy_cross_song_media_is_copied_once_and_edits_cannot_reach_originals() {
    let album = album();
    let original = album.path().join("set in stone/Media/drums.wav");
    std::fs::write(&original, b"original recording").unwrap();
    write_project(
        album.path(),
        Song::SetInStone,
        "<REAPER_PROJECT\r\n  FILE \"Media/drums.wav\" 1\r\n>\r\n",
    );
    write_project(
        album.path(),
        Song::Unbreakable,
        "<REAPER_PROJECT\n FILE \"/Users/old/Crescendum/set in stone/Media/drums.wav\"\n RENDER_FILE \"/original/mix.wav\"\n RECORD_PATH \"/original\" \"\"\n>\n",
    );
    let session = PracticeSession::prepare(album.path(), &Song::BOTH).unwrap();
    assert_eq!(session.media.len(), 1);
    assert_eq!(session.projects.len(), 2);
    assert!(session.media[0].copy.starts_with(&session.directory));
    let project = std::fs::read_to_string(&session.projects[0].copy).unwrap();
    assert!(project.contains("FILE \"Media/0000-drums.wav\" 1\r\n"));
    let other = std::fs::read_to_string(&session.projects[1].copy).unwrap();
    assert!(!other.contains("/Users/") && !other.contains("/original"));
    assert!(other.contains("RENDER_FILE \"Output/practice\""));
    std::fs::write(&session.media[0].copy, b"changed practice audio").unwrap();
    std::fs::write(&session.projects[0].copy, b"changed project").unwrap();
    assert_eq!(std::fs::read(original).unwrap(), b"original recording");
    assert!(
        std::fs::read_to_string(&session.projects[0].source)
            .unwrap()
            .contains("Media/drums.wav")
    );
    std::fs::remove_dir_all(session.directory).unwrap();
}

#[test]
fn missing_media_fails_instead_of_leaving_references_to_the_original() {
    let album = album();
    write_project(
        album.path(),
        Song::SetInStone,
        "<REAPER_PROJECT\n FILE \"missing.wav\"\n>\n",
    );
    let result = PracticeSession::prepare(album.path(), &[Song::SetInStone]);
    assert!(result.err().unwrap().to_string().contains("Missing media"));
}

#[test]
fn each_run_gets_a_fresh_retained_directory() {
    let album = album();
    std::fs::write(album.path().join("set in stone/Media/drums.wav"), b"audio").unwrap();
    write_project(
        album.path(),
        Song::SetInStone,
        "<REAPER_PROJECT\n FILE Media/drums.wav\n>\n",
    );
    let first = PracticeSession::prepare(album.path(), &[Song::SetInStone]).unwrap();
    let second = PracticeSession::prepare(album.path(), &[Song::SetInStone]).unwrap();
    assert_ne!(first.directory, second.directory);
    let retained = first.directory.clone();
    drop(first);
    assert!(retained.join("set in stone.practice.RPP").is_file());
    std::fs::remove_dir_all(retained).unwrap();
    std::fs::remove_dir_all(second.directory).unwrap();
}
