//! Ask the RUNNING engine's keys rig what it is seeing: is it started, what
//! MIDI port is selected, and has any MIDI actually reached it?
//!
//! Splits "the rig never receives the events" from "it receives them and
//! stays silent" — the two halves look identical from the keyboard.
//!
//! ```bash
//! cargo run -p signal-desktop --example keys_midi_probe \
//!     --no-default-features --features signal -- ws://127.0.0.1:4040/vox
//! ```

use signal_keys_proto::keys::KeysRigClient;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".into());
    let link = vox_websocket::WsLink::connect(&url)
        .await
        .map_err(|e| eyre::eyre!("ws connect {url}: {e:?}"))?;
    let rig: KeysRigClient = vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| eyre::eyre!("KeysRig handshake: {e:?}"))?;

    let status = rig
        .status()
        .await
        .map_err(|e| eyre::eyre!("status: {e:?}"))?;
    println!("status: {status:#?}");

    let ports = rig.midi_ports().await.unwrap_or_default();
    println!("\nmidi_ports(): {} port(s)", ports.len());
    for p in ports.iter().filter(|p| p.to_lowercase().contains("s88")) {
        println!("  S88 -> {p}");
    }

    println!("\nWatching midi_recent() for 12s — PLAY NOW.");
    let mut seen = 0usize;
    for _ in 0..12 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let recent = rig.midi_recent().await.unwrap_or_default();
        if recent.len() != seen {
            for e in recent.iter().skip(seen) {
                println!("  rig saw: {e:?}");
            }
            seen = recent.len();
        }
    }
    println!(
        "\n=> the rig saw {seen} event(s). 0 means MIDI never reaches it; \
         >0 means it arrives and the silence is downstream (voices/routing)."
    );
    Ok(())
}
