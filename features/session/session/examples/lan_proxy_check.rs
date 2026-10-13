//! One-off manual check: connect to `session-desktop --engine`'s `/vox`
//! over a real WebSocket and drive the setlist through it — the full
//! chain (LAN client -> axum -> `reaper_lan_proxy` -> real REAPER
//! extension -> back) that `daw::test` alone can't exercise, since the
//! desktop binary is a separate process it doesn't spawn.
//!
//! Run with `session-desktop --engine` (Recording Mode, connected to a
//! real REAPER) already up on the default port:
//!   cargo run --example lan_proxy_check -p session

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let url = std::env::var("LAN_URL").unwrap_or_else(|_| "ws://127.0.0.1:4040/vox".to_string());
    println!("connecting to {url} …");
    let link = vox_websocket::WsLink::connect(&url)
        .await
        .map_err(|e| eyre::eyre!("connecting to {url}: {e}"))?;
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("vox handshake with {url}: {e:?}"))?;
    let client = connection
        .open_lane::<session::SetlistServiceClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceClient lane: {e:?}"))?;
    println!("connected. building setlist from open projects …");

    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;

    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist: {e:?}"))?;
    println!("setlist has {} song(s):", setlist.songs.len());
    for song in &setlist.songs {
        println!(
            "  - {} ({} sections, guid {})",
            song.name,
            song.sections.len(),
            song.project_guid
        );
    }

    Ok(())
}
