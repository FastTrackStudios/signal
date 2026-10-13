//! Explicit opt-in: copies the real album into /tmp, then edits only the copies.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use daw::service::{ItemRef, Items, ProjectContext, TakeRef, Takes};
use expression_editor_audio::quantize::SplitConfig;
use expression_editor_core::{Viewport, kit::LaneRole};
use expression_editor_standalone::practice::{PracticeSession, Song, album_directory};
use expression_editor_standalone::{Runner, Source, Target};
use sha2::{Digest, Sha256};

fn digest(path: &Path) -> eyre::Result<[u8; 32]> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

#[derive(Debug, PartialEq)]
struct ItemState {
    track: String,
    position: f64,
    length: f64,
    offset: f64,
    source: Option<PathBuf>,
}

fn items(runner: &Runner) -> Vec<ItemState> {
    let daw = runner.daw.as_ref().unwrap();
    let project = daw::service::Projects::info(daw, ProjectContext::Current).unwrap();
    let directory = Path::new(&project.path).parent().unwrap();
    let mut items: Vec<_> = runner
        .loaded
        .editor()
        .tracks
        .tracks()
        .iter()
        .flat_map(|track| {
            daw.get_items(
                ProjectContext::Current,
                daw::service::TrackRef::Guid(track.guid.clone()),
            )
        })
        .map(|item| {
            let take = daw.get_take(
                ProjectContext::Current,
                ItemRef::Guid(item.guid),
                TakeRef::Active,
            );
            ItemState {
                track: item.track_guid,
                position: item.position.as_seconds(),
                length: item.length.as_seconds(),
                offset: take
                    .as_ref()
                    .map_or(0.0, |take| take.start_offset.as_seconds()),
                source: take.and_then(|take| take.source_file_path).map(|path| {
                    directory
                        .join(path)
                        .canonicalize()
                        .expect("staged source resolves")
                }),
            }
        })
        .collect();
    items.sort_by(|a, b| {
        a.track
            .cmp(&b.track)
            .then(a.position.total_cmp(&b.position))
            .then(a.length.total_cmp(&b.length))
            .then(a.offset.total_cmp(&b.offset))
            .then(a.source.cmp(&b.source))
    });
    items
}

fn assert_same_audio(actual: &[ItemState], expected: &[ItemState]) {
    assert_eq!(actual.len(), expected.len(), "saved item count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual.track, expected.track, "item {index} track");
        assert_eq!(actual.source, expected.source, "item {index} active source");
        assert!(
            (actual.position - expected.position).abs() < 1e-8,
            "item {index} position"
        );
        assert!(
            (actual.length - expected.length).abs() < 1e-8,
            "item {index} length"
        );
        assert!(
            (actual.offset - expected.offset).abs() < 1e-8,
            "item {index} source offset: {actual:?} versus {expected:?}"
        );
    }
}

#[test]
#[ignore = "copies several GB of real Crescendum recordings; run just ee-practice-test"]
fn crescendum_practice_copies_load_edit_undo_and_save() -> eyre::Result<()> {
    let session = PracticeSession::prepare(&album_directory(), &Song::BOTH)?;
    eprintln!(
        "Practice workspace retained at {}",
        session.directory.display()
    );
    let originals: BTreeMap<PathBuf, [u8; 32]> = session
        .media
        .iter()
        .map(|media| &media.source)
        .chain(session.projects.iter().map(|project| &project.source))
        .map(|path| Ok((path.clone(), digest(path)?)))
        .collect::<eyre::Result<_>>()?;

    for project in &session.projects {
        eprintln!("Opening {}", project.copy.display());
        let runner = Runner::open(
            &Source::Rpp(project.copy.clone()),
            &Target {
                drums: Some(None),
                ..Default::default()
            },
            Viewport::new(1400.0, 700.0),
            None,
        )?;
        let editor = runner.loaded.editor();
        for role in [LaneRole::Kick, LaneRole::Snare, LaneRole::Toms] {
            assert!(
                !editor.tracks.role_members(role).is_empty(),
                "missing {role:?}"
            );
        }
        assert!(
            editor.doc.peaks.iter().any(|peak| *peak > 0.0),
            "must load audible drum audio"
        );
        let host = runner.host.as_ref().unwrap();
        let before = items(&runner);
        let hit = host
            .hits(&Default::default())
            .into_iter()
            .find(|hit| hit.at > 1.0 && hit.at < host.take_secs - 1.0)
            .expect("song has editable drum hits");
        let done = host.split(
            hit.at,
            SplitConfig {
                leading_pad_secs: 0.005,
                crossfade_secs: 0.005,
            },
        )?;
        assert!(
            done.items > 0 && done.pieces > 0,
            "must perform a real edit"
        );
        assert_ne!(items(&runner), before);
        assert!(host.undo());
        assert_eq!(items(&runner), before, "one undo restores the whole kit");
        assert!(host.redo());
        let saved = host.save().map_err(eyre::Report::msg)?;
        assert!(saved.starts_with(&session.directory));
        assert_ne!(saved, project.source);
        assert!(saved.is_file());
        eprintln!(
            "Edited {} mic tracks; saved {}",
            done.items,
            saved.display()
        );
        // Reopen the saved project: it must remain independently editable.
        let reopened = Runner::open(
            &Source::Rpp(saved),
            &Target {
                drums: Some(None),
                ..Default::default()
            },
            Viewport::new(1400.0, 700.0),
            None,
        )?;
        assert_same_audio(&items(&reopened), &items(&runner));
    }
    for (path, before) in &originals {
        assert_eq!(
            digest(path)?,
            *before,
            "original changed: {}",
            path.display()
        );
    }
    for media in &session.media {
        assert_eq!(
            originals[&media.source],
            digest(&media.copy)?,
            "practice edits must not rewrite audio samples"
        );
    }
    eprintln!(
        "Both songs passed; original projects and audio are byte-identical. Copies: {}",
        session.directory.display()
    );
    Ok(())
}
