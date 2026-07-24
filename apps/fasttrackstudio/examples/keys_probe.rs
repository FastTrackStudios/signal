//! Probe the engine's KeysRig service over the wire — the same establish the
//! browser remote does, with the error printed instead of swallowed.
//!
//! ```bash
//! cargo run -p fasttrackstudio --example keys_probe \
//!     --no-default-features --features signal -- ws://127.0.0.1:4040/vox
//! ```

use signal_keys_proto::keys::{KeysRigClient, KeysRigStreamClient};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
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
    println!("KeysRig established ✓");

    let link2 = vox_websocket::WsLink::connect(&url).await.map_err(|e| eyre::eyre!("ws2: {e:?}"))?;
    let _stream: KeysRigStreamClient = vox_core::initiator_on(link2)
        .establish()
        .await
        .map_err(|e| eyre::eyre!("KeysRigStream handshake: {e:?}"))?;
    println!("KeysRigStream established ✓");

    let status = rig.status().await.map_err(|e| eyre::eyre!("status: {e:?}"))?;
    println!(
        "running={} patch={:?} midi={:?} err={:?}",
        status.running, status.loaded_preset, status.midi_port, status.last_error
    );
    let mixer = rig.mixer().await.map_err(|e| eyre::eyre!("mixer: {e:?}"))?;
    println!("profile {} — {} engines", mixer.profile, mixer.engines.len());
    for e in &mixer.engines {
        let lanes: Vec<String> = e
            .layers
            .iter()
            .map(|l| format!("{}{}", l.name, if l.live { "*" } else { "" }))
            .collect();
        println!("  {:<6} {}", e.name, lanes.join(" · "));
    }
    let t0 = std::time::Instant::now();
    let presets = rig.presets().await.map_err(|e| eyre::eyre!("presets: {e:?}"))?;
    println!("presets: {} in {:?}", presets.len(), t0.elapsed());
    // The Pad lane's modules — American Obesity is OB-8 + a Juno sub, so
    // module A and B should both be live.
    for slot in 0..4u32 {
        let d = rig
            .layer_detail("Pad".into(), slot)
            .await
            .map_err(|e| eyre::eyre!("layer_detail: {e:?}"))?;
        let m = d.modules.iter().find(|m| m.index == slot);
        println!(
            "  Pad module {} → {:<28} macros={} tree={}",
            m.map(|m| m.slot.clone()).unwrap_or_else(|| slot.to_string()),
            if d.patch.is_empty() { "—".into() } else { d.patch.clone() },
            d.macros.len(),
            d.tree.label,
        );
    }
    Ok(())
}
