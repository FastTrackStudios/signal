//! How long a real song takes through the project doc: an RPP loaded into
//! daw-standalone, read into the doc by the bridge, the doc's size, and
//! the song rebuilt from the doc in an empty project.
//!
//! `cargo run --release -p session-sync --features engine --example doc_scale -- <Song.RPP>`

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
        .ok_or_else(|| eyre::eyre!("an RPP"))?;
    // A `.session` folder through its RPP export; an RPP as it is.
    let text = if path.ends_with(".session") {
        daw_standalone::session_file::session_rpp_text(std::path::Path::new(&path))
            .map_err(|e| eyre::eyre!(e))?
    } else {
        std::fs::read_to_string(&path)?
    };
    let standalone = Standalone::new();
    let t = Instant::now();
    let loaded = daw_standalone::project_loader::load_rpp_text(&standalone, "song", &path, &text)
        .map_err(|e| eyre::eyre!(e))?;
    println!("rpp loaded            {:>7} ms", t.elapsed().as_millis());
    standalone.seed_project(ProjectInfo {
        guid: "rebuilt".into(),
        name: "rebuilt".into(),
        path: String::new(),
    });
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    let song = bundle.daw.project(loaded.project_guid.clone()).await?;
    let media = MediaRoot(
        std::path::Path::new(&path)
            .parent()
            .unwrap_or(std::path::Path::new("/"))
            .to_path_buf(),
    );
    let blobs = Arc::new(MemoryBlobs::default());

    let t = Instant::now();
    let doc = SessionDoc::new();
    // Through the engine in this process, as the Session app does.
    let local: Option<Arc<dyn session_sync::engine::Direct>> = Some(Arc::new(standalone.clone()));
    let host = Bridge::host(
        song,
        media.clone(),
        doc.clone(),
        String::new(),
        blobs.clone(),
        local.clone(),
    )
    .await?;
    let model = host.shown();
    println!(
        "read into the doc     {:>7} ms   ({} tracks, {} items)",
        t.elapsed().as_millis(),
        model.tracks.len(),
        model.items.len()
    );
    let snapshot = doc.snapshot()?;
    let packed = zstd::encode_all(snapshot.as_slice(), 9)?;
    println!(
        "doc snapshot          {:>7.1} MB raw, {:.2} MB zstd",
        snapshot.len() as f64 / 1e6,
        packed.len() as f64 / 1e6
    );

    let t = Instant::now();
    let replica = SessionDoc::new();
    replica.import(&snapshot, "remote")?;
    println!("doc imported          {:>7} ms", t.elapsed().as_millis());
    let t2 = Instant::now();
    let read_back = replica.read();
    println!("doc read to model     {:>7} ms", t2.elapsed().as_millis());
    let t2 = Instant::now();
    let steps = session_sync::diff(&session_sync::SessionModel::default(), &read_back);
    println!(
        "diff from empty       {:>7} ms   ({} steps)",
        t2.elapsed().as_millis(),
        steps.len()
    );
    let rebuilt = bundle.daw.project("rebuilt").await?;
    let joined = Bridge::join(rebuilt, media, replica, blobs, local).await?;
    println!("rebuilt from the doc  {:>7} ms", t.elapsed().as_millis());
    if let Some((guid, item)) = model
        .items
        .iter()
        .find(|(g, i)| joined.shown().items.get(*g) != Some(*i))
    {
        println!(
            "first differing item {guid}:\n  built   {item:?}\n  rebuilt {:?}",
            joined.shown().items.get(guid)
        );
    }
    println!(
        "same tracks: {}  same items: {}",
        joined.shown().tracks == model.tracks,
        joined.shown().items == model.items
    );
    Ok(())
}
