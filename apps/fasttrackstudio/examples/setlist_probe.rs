//! Probe a running `fasttrackstudio --engine`'s SetlistService over the
//! network — the same wire path the browser session remote uses (one typed
//! vox client per service over its own WebSocket link).
//!
//! ```bash
//! cargo run -p fasttrackstudio --example setlist_probe -- ws://127.0.0.1:4040/vox
//! ```
//!
//! Prints the served setlist, the active song/section cursor from the
//! `active_indices` `#[subscribe]` stream, and one `events` stream frame —
//! proving RPC + both subscription streams reach the engine's session
//! router end-to-end.

use session_proto::services::setlist_service::SetlistServiceStreamClient;
use session_proto::{ActiveIndices, SetlistEvent, SetlistServiceClient};

async fn establish<C: vox_core::FromVoxLane>(url: &str) -> C {
    let link = vox_websocket::WsLink::connect(url).await.expect("ws connect");
    vox_core::initiator_on(link)
        .establish::<C>()
        .await
        .expect("vox handshake")
}

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string());

    // 16 MiB worker stacks: vox 0.10's debug-build channel encode recurses
    // deeply on Setlist payloads (see session_engine.rs).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async move {
        // ── RPC: fetch the setlist ────────────────────────────────────────
        let client: SetlistServiceClient = establish(&url).await;
        let setlist = client.setlist().await.expect("setlist() rpc");
        println!("setlist '{}': {} songs", setlist.name, setlist.songs.len());
        for (i, song) in setlist.songs.iter().enumerate() {
            println!("  {i:02} {} ({} sections)", song.name, song.sections.len());
        }

        // ── `#[subscribe]` streams: active_indices + events ──────────────
        let stream: SetlistServiceStreamClient = establish(&url).await;
        let (atx, mut arx) = vox::channel::<ActiveIndices>();
        let ai_stream = stream.clone();
        tokio::spawn(async move {
            let _ = ai_stream.active_indices(atx).await;
        });
        let ai = tokio::time::timeout(std::time::Duration::from_secs(5), arx.recv())
            .await
            .expect("active_indices frame within 5s")
            .expect("active_indices stream")
            .expect("active_indices closed");
        let ai = ai.get();
        println!(
            "active cursor: song {:?} / section {:?}",
            ai.song_index, ai.section_index
        );

        // The events hub has no replay (remotes snapshot the setlist via
        // RPC), so an idle transport legitimately produces no frame here —
        // subscribing without error is the assertion; a frame is a bonus.
        let (etx, mut erx) = vox::channel::<SetlistEvent>();
        tokio::spawn(async move {
            let _ = stream.events(etx).await;
        });
        match tokio::time::timeout(std::time::Duration::from_secs(3), erx.recv()).await {
            Ok(Ok(Some(ev))) => {
                println!("events stream: first frame = {:?}", variant_name(ev.get()))
            }
            Ok(_) => panic!("events stream closed"),
            Err(_) => println!("events stream: subscribed (no traffic while idle — ok)"),
        }

        println!("OK: SetlistService reachable over {url}");
    });
}

fn variant_name(ev: &SetlistEvent) -> &'static str {
    match ev {
        SetlistEvent::SetlistChanged(_) => "SetlistChanged",
        SetlistEvent::SongHydrated { .. } => "SongHydrated",
        SetlistEvent::SongEntered { .. } => "SongEntered",
        _ => "(other)",
    }
}
