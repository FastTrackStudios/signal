//! The strip's rack reaches the track's processor: an `Edit::SetFxParam`
//! goes through the engine as the window sends it and lands on the
//! built-in FTS Comp, which then reads the setting in its own words.

#![cfg(feature = "native")]

use std::path::{Path, PathBuf};

use session_daw::engine::Edit;

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

#[test]
fn a_rack_edit_sets_the_tracks_processor() {
    let opened = session_daw::open::open_silent(&template()).expect("the template opens");
    let read = |what: &str| {
        session_daw::open::block_on_engine(async move {
            let daw = daw::rpc::Daw::try_get().expect("the facade");
            let project = daw.current_project().await.expect("a project");
            let track = project
                .tracks()
                .all()
                .await
                .expect("tracks")
                .into_iter()
                .find(|t| !t.is_folder)
                .expect("a track");
            let fx = match what {
                "add" => Some(
                    project
                        .tracks()
                        .by_guid(&track.guid)
                        .await
                        .expect("found")
                        .expect("there")
                        .fx_chain()
                        .add("FTS Comp")
                        .await
                        .expect("added"),
                ),
                _ => project
                    .tracks()
                    .by_guid(&track.guid)
                    .await
                    .expect("found")
                    .expect("there")
                    .fx_chain()
                    .by_name("FTS Comp")
                    .await
                    .expect("looked up"),
            };
            let index = builtin_fx::setting("FTS Comp", "Threshold", "-30.0 dB")
                .expect("a threshold")
                .0;
            let text = match fx {
                Some(fx) => fx.param(index).formatted().await.expect("its text"),
                None => String::new(),
            };
            (track.guid, text)
        })
        .expect("the engine")
    };
    let _ = opened;
    let (track, before) = read("add");
    assert_ne!(before, "-30.0 dB", "a different threshold to start from");
    let (index, value) =
        builtin_fx::setting("FTS Comp", "Threshold", "-30.0 dB").expect("a threshold");
    assert!(session_daw::engine::apply_now(&Edit::SetFxParam(
        track,
        "FTS Comp".into(),
        index,
        value
    )));
    let (_, after) = read("read");
    assert_eq!(after, "-30.0 dB");
}
