//! Pin the running keys rig to ONE port (the S88 Main) instead of omni, then
//! watch whether events start arriving.
//!
//! Omni makes every rig open all 23 inputs, so one app run subscribes ~40
//! JACK clients to the same hardware ports (REAPER is on them too). If a
//! single explicit subscription receives what omni does not, the fan-out is
//! the problem, not the keyboard.
use signal_keys_proto::keys::KeysRigClient;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "ws://127.0.0.1:4040/vox".into());
    let link = vox_websocket::WsLink::connect(&url).await.map_err(|e| eyre::eyre!("{e:?}"))?;
    let rig: KeysRigClient = vox_core::initiator_on(link).establish().await.map_err(|e| eyre::eyre!("{e:?}"))?;

    let ports = rig.midi_ports().await.unwrap_or_default();
    let Some(main) = ports.iter().find(|p| p.contains("S88") && p.contains("Main")).cloned() else {
        eyre::bail!("no S88 Main port in: {ports:#?}");
    };
    println!("pinning keys rig to: {main}");
    rig.set_midi_port(main).await.map_err(|e| eyre::eyre!("set_midi_port: {e:?}"))?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let base = rig.midi_recent().await.unwrap_or_default().len();
    println!("baseline events: {base}\n>>> PLAY THE S88 NOW — watching 15s <<<");
    let mut seen = base;
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let r = rig.midi_recent().await.unwrap_or_default();
        if r.len() != seen {
            for e in r.iter().skip(seen.min(r.len())) {
                println!("  rig saw: {e:?}");
            }
            seen = r.len();
        }
    }
    println!("\n=> new events while pinned: {}", seen.saturating_sub(base));
    Ok(())
}
