//! How long one edit takes into the project doc on a real song: the song
//! opened from its project file into daw-standalone, one item moved, and
//! the bridge's read of the engine and write of the doc timed apart.
//!
//! `cargo run --release -p session-sync --features engine --example doc_edit -- <Song.session>`

use std::sync::Arc;
use std::time::Instant;

use daw_proto::ProjectInfo;
use daw_standalone::sync::Standalone;
use session_sync::engine::{MediaRoot, MemoryBlobs};
use session_sync::{Bridge, SessionDoc};

fn main() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(run())
}

async fn run() -> eyre::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("a project file"))?;
    let store = dawfile_store::Store::open(std::path::Path::new(&path))?;
    let loaded = store.load(dawfile_store::MAIN)?;
    let standalone = Standalone::new();
    standalone.seed_project(ProjectInfo {
        guid: "song".into(),
        name: "song".into(),
        path: String::new(),
    });
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    let media = MediaRoot(
        std::path::Path::new(&path)
            .parent()
            .unwrap_or(std::path::Path::new("/"))
            .to_path_buf(),
    );
    let local: Option<Arc<dyn session_sync::engine::Direct>> = Some(Arc::new(standalone.clone()));
    let t = Instant::now();
    let mut bridge = Bridge::join(
        bundle.daw.project("song").await?,
        media.clone(),
        SessionDoc::from_loro(loaded),
        Arc::new(MemoryBlobs::default()),
        local.clone(),
    )
    .await?;
    println!(
        "opened                {:>7} ms   ({} items)",
        t.elapsed().as_millis(),
        bridge.shown().items.len()
    );

    let t = Instant::now();
    let unchanged = bridge.local_changed().await?;
    println!(
        "nothing changed       {:>7} ms   (recorded: {unchanged})",
        t.elapsed().as_millis()
    );

    let t = Instant::now();
    let model = session_sync::engine::read(
        &bundle.daw.project("song").await?,
        &media,
        &MemoryBlobs::default(),
        &session_sync::engine::EngineIds::default(),
        local.as_deref(),
    )
    .await?;
    println!("engine read           {:>7} ms", t.elapsed().as_millis());
    drop(model);
    let t = Instant::now();
    let read_back = bridge.doc().read();
    println!(
        "doc read to model     {:>7} ms   ({} items)",
        t.elapsed().as_millis(),
        read_back.items.len()
    );
    let t = Instant::now();
    let steps = session_sync::diff(bridge.shown(), &read_back);
    println!(
        "diff with shown       {:>7} ms   ({} steps)",
        t.elapsed().as_millis(),
        steps.len()
    );
    for step in &steps {
        let text = format!("{step:?}");
        println!("  {}", &text[..text.len().min(400)]);
    }
    let t = Instant::now();
    let ctx = daw_proto::ProjectContext::Project("song".into());
    let items = session_sync::direct::read_items(
        local.as_deref().ok_or_else(|| eyre::eyre!("direct"))?,
        &ctx,
        &media,
        &session_sync::engine::EngineIds::default(),
    );
    println!(
        "items read            {:>7} ms   ({})",
        t.elapsed().as_millis(),
        items.len()
    );

    let item = bundle
        .daw
        .project("song")
        .await?
        .items()
        .all()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("no items"))?;
    let handle = bundle
        .daw
        .project("song")
        .await?
        .items()
        .by_guid(&item.guid)
        .await?
        .ok_or_else(|| eyre::eyre!("the item"))?;
    handle
        .set_position(daw_proto::primitives::PositionInSeconds::from_seconds(
            item.position.as_seconds() + 0.5,
        ))
        .await?;
    let before = bridge.doc().version();
    let t = Instant::now();
    let recorded = bridge.local_changed().await?;
    println!(
        "one item moved, read whole       {:>7} ms   (recorded: {recorded}, update {} bytes)",
        t.elapsed().as_millis(),
        bridge.doc().updates_since(&before)?.len()
    );
    handle
        .set_position(daw_proto::primitives::PositionInSeconds::from_seconds(
            item.position.as_seconds() + 1.0,
        ))
        .await?;
    let before = bridge.doc().version();
    let t = Instant::now();
    bridge.local_step("Move item").await?;
    println!(
        "moved again, as a step  {:>7} ms   (update {} bytes)",
        t.elapsed().as_millis(),
        bridge.doc().updates_since(&before)?.len()
    );
    let t = Instant::now();
    let undone = bridge.undo().await?;
    println!(
        "undo                  {:>7} ms   ({undone:?})",
        t.elapsed().as_millis()
    );
    Ok(())
}
