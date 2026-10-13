//! Undo through the doc (`Bridge::undo`): a peer's own steps undone and
//! redone by name, the engine made to match — and a collaborator's edit
//! made in between left standing.

#![cfg(feature = "engine")]

use std::sync::Arc;

use daw_proto::ProjectInfo;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_standalone::sync::Standalone;
use session_sync::engine::{MediaRoot, MemoryBlobs};
use session_sync::{Bridge, SessionDoc};

#[test]
fn own_steps_undo_by_name_and_a_peers_edit_stays() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(inner())
}

async fn inner() -> eyre::Result<()> {
    let standalone = Standalone::new();
    for guid in ["mine", "theirs"] {
        standalone.seed_project(ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
    }
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    let direct: Option<Arc<dyn session_sync::engine::Direct>> = Some(Arc::new(standalone.clone()));
    let media = MediaRoot(std::path::PathBuf::from("/"));
    let blobs = Arc::new(MemoryBlobs::default());

    let mine = bundle.daw.project("mine").await?;
    let kick = mine.tracks().add_with_guid("{KICK}", "Kick", None).await?;
    let mut me = Bridge::host(
        mine.clone(),
        media.clone(),
        SessionDoc::new(),
        String::new(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    assert_eq!(me.next_undo(), None, "opening is not a step");

    let item = mine
        .items()
        .add_with_guid(
            "{KICK}",
            "{HIT}",
            PositionInSeconds::from_seconds(1.0),
            Duration::from_seconds(0.5),
        )
        .await?;
    assert!(me.local_step("Add hit").await?);
    item.set_position(PositionInSeconds::from_seconds(3.0))
        .await?;
    assert!(me.local_step("Move hit").await?);
    assert_eq!(me.next_undo().as_deref(), Some("Move hit"));

    // A collaborator, joined, turns the kick down.
    let theirs = bundle.daw.project("theirs").await?;
    let mut them = Bridge::join(
        theirs.clone(),
        media.clone(),
        me.doc().fork(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    theirs
        .tracks()
        .by_guid("{KICK}")
        .await?
        .ok_or_else(|| eyre::eyre!("their kick"))?
        .set_volume(0.5)
        .await?;
    assert!(them.local_step("Kick down").await?);
    me.doc()
        .import(&them.doc().updates_since(&me.doc().version())?, "remote")?;
    me.remote_changed().await?;

    let at = || async {
        mine.items()
            .all()
            .await
            .map(|items| items.first().map(|i| i.position.as_seconds()))
    };
    let volume = || async { kick.info().await.map(|t| t.volume) };
    assert_eq!(volume().await?, 0.5, "their edit arrived");

    assert_eq!(me.undo().await?.as_deref(), Some("Move hit"));
    assert_eq!(at().await?, Some(1.0), "the move undone");
    assert_eq!(volume().await?, 0.5, "their edit stays");
    assert_eq!(me.next_redo().as_deref(), Some("Move hit"));

    assert_eq!(me.redo().await?.as_deref(), Some("Move hit"));
    assert_eq!(at().await?, Some(3.0), "the move redone");

    assert_eq!(me.undo().await?.as_deref(), Some("Move hit"));
    assert_eq!(me.undo().await?.as_deref(), Some("Add hit"));
    assert_eq!(at().await?, None, "the hit gone");
    assert_eq!(volume().await?, 0.5, "their edit still stays");
    assert_eq!(me.undo().await?, None, "nothing of mine before that");
    Ok(())
}
