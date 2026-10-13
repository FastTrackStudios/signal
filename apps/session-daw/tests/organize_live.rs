//! The organize pass run live, against the engine this window drives, is
//! the organize pass the `--apply-buses` CLI runs over a file.
//!
//! One pipeline (`dynamic_template::apply::organize`) and two targets: the
//! `.RPP` chunk tree, and `DawTarget` over `daw-standalone` through the
//! `daw::service` traits. If the two ever disagree about a session — which
//! buses it needs, where each track ends up, how deep it sits — the window
//! and the batch tool would be organizing different sessions. This holds
//! them to the same answer on the golden session.

use std::path::Path;

use daw::service::Tracks;
use daw_proto::ProjectContext;
use dynamic_template::apply::chunk::RChunkTarget;
use dynamic_template::apply::{DawTarget, TemplateTarget, organize};

const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../features/dynamic-template/fixtures/golden/template.rpp"
);

#[test]
fn a_live_organize_matches_the_file_organize() {
    // The file side.
    let text = std::fs::read_to_string(GOLDEN).expect("golden session");
    let mut chunk = dawfile_reaper::read_rpp_chunk(&text).expect("parse");
    let file = {
        let mut target = RChunkTarget::new(&mut chunk);
        match organize(&mut target) {
            Ok(organized) => organized,
            Err(never) => match never {},
        }
    };
    let file_tracks: Vec<(String, i32)> = RChunkTarget::new(&mut chunk)
        .folder_depths()
        .into_iter()
        .map(|(_, name, depth)| (name, depth))
        .collect();

    // The live side: the same file, opened the way the window opens it.
    let opened = session_daw::open::open_silent(Path::new(GOLDEN)).expect("open");
    let project = ProjectContext::Project(opened.project_guid.clone());
    let mut target = DawTarget::on(opened.daw.clone(), project.clone());
    let live = organize(&mut target).expect("live organize");
    let live_tracks: Vec<(String, i32)> = Tracks::all(&opened.daw, project)
        .into_iter()
        .map(|t| (t.name, t.folder_depth))
        .collect();

    let live_buses: Vec<&str> = live.buses.iter().map(|b| b.name.as_str()).collect();
    let file_buses: Vec<&str> = file.buses.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(live_buses, file_buses, "the same bus tree");
    assert_eq!(
        live.justified, file.justified,
        "the same tracks justify each bus"
    );
    assert_eq!(live.unclassified, file.unclassified);
    assert_eq!(live.painted, file.painted);
    assert_eq!(live.routing.unrouted, file.routing.unrouted);
    if live_tracks.len() != file_tracks.len() {
        let mut f = file_tracks.iter().map(|t| t.0.as_str()).collect::<Vec<_>>();
        for (name, _) in &live_tracks {
            if let Some(i) = f.iter().position(|n| n == name) {
                f.remove(i);
            } else {
                eprintln!("only live: {name}");
            }
        }
        for name in f {
            eprintln!("only file: {name}");
        }
        eprintln!(
            "live gathered: {:?}",
            live.gathered
                .as_ref()
                .map(|g| (g.moved.len(), g.skipped.len()))
        );
        eprintln!(
            "file gathered: {:?}",
            file.gathered
                .as_ref()
                .map(|g| (g.moved.len(), g.skipped.len()))
        );
        eprintln!(
            "unsorted live {} file {}",
            live.unsorted.len(),
            file.unsorted.len()
        );
    }
    assert_eq!(
        live_tracks.len(),
        file_tracks.len(),
        "the same number of tracks afterwards"
    );
    for (i, (l, f)) in live_tracks.iter().zip(&file_tracks).enumerate() {
        assert_eq!(l, f, "track {i}: live {l:?} vs file {f:?}");
    }

    // And idempotent live, as it is on disk: a second pass adds nothing.
    let mut again = DawTarget::on(
        opened.daw.clone(),
        ProjectContext::Project(opened.project_guid),
    );
    let second = organize(&mut again).expect("second organize");
    assert!(second.applied.created.is_empty(), "no bus created twice");
}
