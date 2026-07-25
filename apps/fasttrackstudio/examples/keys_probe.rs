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
    // Optional: load a module preset by name onto Pad module A, the way the
    // layer browser does — `keys_probe <url> "American Obesity"`.
    if let Some(want) = std::env::args().nth(2) {
        let hit = presets
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(&want))
            .ok_or_else(|| eyre::eyre!("no preset named {want:?}"))?;
        println!("loading #{hit} {:?} ({}) → Pad module A", presets[hit].name, presets[hit].kind);
        rig.set_layer_patch("Pad".into(), 0, hit as u32)
            .await
            .map_err(|e| eyre::eyre!("set_layer_patch: {e:?}"))?;
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    }

    // The Pad lane's modules — American Obesity is OB-8 + a Juno sub, so
    // module A and B should both be live.
    for slot in 0..4u32 {
        let d = rig
            .layer_detail("Pad".into(), slot)
            .await
            .map_err(|e| eyre::eyre!("layer_detail: {e:?}"))?;
        let m = d.modules.iter().find(|m| m.index == slot);
        let show = |id: &str| {
            d.macros.iter().find(|x| x.id == id).map(|x| x.value).unwrap_or(f32::NAN)
        };
        println!(
            "  Pad module {} → {:<30} {:+.1} dB  cutoff={:.0} reso={:.2} uni={:.0} A/D/S/R={:.0}/{:.0}/{:.2}/{:.0}",
            m.map(|m| m.slot.clone()).unwrap_or_else(|| slot.to_string()),
            if d.patch.is_empty() { "—".into() } else { d.patch.clone() },
            m.map(|m| m.gain_db).unwrap_or(0.0),
            show("filter.cutoff"),
            show("filter.reso"),
            show("source.unison"),
            show("env1.attack"),
            show("env1.decay"),
            show("env1.sustain"),
            show("env1.release"),
        );
        println!(
            "        source={:<28} lfo1={:.2}Hz d={:.2} sh={:.0}  fenv_amt={:.2}",
            if d.source.is_empty() { "—".into() } else { d.source.clone() },
            show("lfo1.rate"),
            show("lfo1.depth"),
            show("lfo1.shape"),
            show("filter.env_amt"),
        );
    }
    Ok(())
}
